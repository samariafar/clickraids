mod games;

use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use tokio::{
    net::TcpListener,
    sync::{broadcast, mpsc},
    task,
};
use tower_http::services::ServeDir;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const CELL_COUNT: usize = 1_000_000;
const WORD_COUNT: usize = CELL_COUNT / 64;
const SNAPSHOT_BYTES: usize = WORD_COUNT * 8;

// Wire format: u32 big-endian per flip; MSB = state, lower 31 bits = index.
const STATE_BIT: u32 = 1 << 31;
const INDEX_MASK: u32 = !STATE_BIT;

#[derive(Debug, Clone, Copy)]
struct CheckboxUpdate {
    index: usize,
    state: bool,
}

impl CheckboxUpdate {
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 4 {
            return None;
        }
        let packed = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        Some(Self {
            index: (packed & INDEX_MASK) as usize,
            state: packed & STATE_BIT != 0,
        })
    }

    fn to_bytes(self) -> [u8; 4] {
        let packed = (self.state as u32) << 31 | (self.index as u32 & INDEX_MASK);
        packed.to_be_bytes()
    }
}

type Bitmap = Arc<Vec<AtomicU64>>;

struct Game {
    state: Bitmap,
    player_count: AtomicUsize,
    broadcast_tx: broadcast::Sender<CheckboxUpdate>,
    backup_tx: mpsc::UnboundedSender<usize>,
}

#[derive(Clone)]
struct AppState {
    games: Arc<HashMap<&'static str, Arc<Game>>>,
}

fn new_state() -> Bitmap {
    let mut words = Vec::with_capacity(WORD_COUNT);
    words.resize_with(WORD_COUNT, || AtomicU64::new(0));
    Arc::new(words)
}

fn set_bit(state: &Bitmap, index: usize, value: bool) {
    let word = index >> 6;
    let mask = 1u64 << (index & 63);
    if value {
        state[word].fetch_or(mask, Ordering::Relaxed);
    } else {
        state[word].fetch_and(!mask, Ordering::Relaxed);
    }
}

fn snapshot_bytes(state: &Bitmap) -> Vec<u8> {
    let mut buf = Vec::with_capacity(SNAPSHOT_BYTES);
    for word in state.iter() {
        buf.extend_from_slice(&word.load(Ordering::Relaxed).to_le_bytes());
    }
    buf
}

fn load_from_file_or_init(file: &mut File, state: &Bitmap, slug: &str) {
    let file_len = file.metadata().unwrap().len();

    if file_len == 0 {
        let buffer = [0u8; SNAPSHOT_BYTES];

        file.write_all(&buffer).unwrap();

        log::info!("[{}] Initialized new state file with default state.", slug);
    } else {
        let mut buffer = [0u8; SNAPSHOT_BYTES];

        file.seek(SeekFrom::Start(0)).unwrap();
        file.read_exact(&mut buffer).unwrap();

        for (i, chunk) in buffer.chunks_exact(8).enumerate() {
            let word = u64::from_le_bytes(chunk.try_into().unwrap());
            state[i].store(word, Ordering::Relaxed);
        }

        log::info!("[{}] Loaded state from existing file.", slug);
    }
}

// Single dedicated writer task per game; reads the affected byte's current value from the
// in-memory atomic (the source of truth) and persists it. The file is a recovery backup,
// never on the serving path.
fn spawn_backup_writer(
    slug: &'static str,
    state: Bitmap,
    mut file: File,
    mut rx: mpsc::UnboundedReceiver<usize>,
) {
    task::spawn_blocking(move || {
        while let Some(index) = rx.blocking_recv() {
            let word_idx = index >> 6;
            let byte_in_word = (index >> 3) & 7;
            let word = state[word_idx].load(Ordering::Relaxed);
            let byte = (word >> (byte_in_word * 8)) as u8;
            let byte_pos = (word_idx * 8 + byte_in_word) as u64;

            if let Err(e) = file
                .seek(SeekFrom::Start(byte_pos))
                .and_then(|_| file.write_all(&[byte]))
            {
                log::error!("[{}] Backup write failed at byte {}: {}", slug, byte_pos, e);
            }
        }
    });
}

async fn spa_fallback() -> impl IntoResponse {
    match tokio::fs::read("public/index.html").await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], bytes).into_response(),
        Err(e) => {
            log::error!("Failed to read public/index.html: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "App bundle missing").into_response()
        }
    }
}

async fn ws_handler(
    upgrade: WebSocketUpgrade,
    Path(slug): Path<String>,
    State(app): State<AppState>,
) -> impl IntoResponse {
    let Some((&slug, game)) = app.games.get_key_value(slug.as_str()) else {
        return (StatusCode::NOT_FOUND, "Unknown game").into_response();
    };
    let game = game.clone();
    upgrade
        .on_upgrade(move |socket| handle_socket(socket, slug, game))
        .into_response()
}

async fn stats_ws_handler(
    upgrade: WebSocketUpgrade,
    State(app): State<AppState>,
) -> impl IntoResponse {
    upgrade.on_upgrade(move |socket| handle_stats_socket(socket, app))
}

async fn handle_stats_socket(mut socket: WebSocket, app: AppState) {
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tick.tick().await;

        let mut buf = Vec::with_capacity(games::SLUGS.len() * 4);
        for &slug in games::SLUGS.iter() {
            let count = app.games[slug].player_count.load(Ordering::Relaxed) as u32;
            buf.extend_from_slice(&count.to_be_bytes());
        }

        if socket.send(Message::Binary(buf)).await.is_err() {
            break;
        }
    }
}

async fn handle_socket(socket: WebSocket, slug: &'static str, game: Arc<Game>) {
    let mut last_update_time = Instant::now();
    let update_interval = Duration::from_secs_f32(1.0 / 3.0);

    let count = game.player_count.fetch_add(1, Ordering::Relaxed) + 1;
    log::info!("[{}] New player joined. Game players: {}", slug, count);

    let (mut write, mut read) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let mut receiver = game.broadcast_tx.subscribe();

    {
        let bytes = snapshot_bytes(&game.state);
        let compressed = zstd::bulk::compress(&bytes, 0).unwrap();
        let message = Message::Binary(compressed);

        if tx.send(message).is_err() {
            game.player_count.fetch_sub(1, Ordering::Relaxed);
            return;
        }
    }

    let write_task = task::spawn(async move {
        while let Some(message) = rx.recv().await {
            if write.send(message).await.is_err() {
                break;
            }
        }
    });

    let read_game = game.clone();
    let read_task = task::spawn(async move {
        while let Some(Ok(message)) = read.next().await {
            if let Message::Binary(bytes) = message {
                if last_update_time.elapsed() >= update_interval {
                    last_update_time = Instant::now();

                    if let Some(update) = CheckboxUpdate::from_bytes(&bytes) {
                        set_bit(&read_game.state, update.index, update.state);
                        let _ = read_game.backup_tx.send(update.index);

                        log::info!(
                            "[{}] Checkbox at index {} updated to state: {}",
                            slug,
                            update.index,
                            update.state
                        );

                        let _ = read_game.broadcast_tx.send(update);
                    }
                }
            }
        }
    });

    let broadcast_task = task::spawn(async move {
        while let Ok(update) = receiver.recv().await {
            let message = Message::Binary(update.to_bytes().to_vec());

            if tx.send(message).is_err() {
                break;
            }
        }
    });

    tokio::select! {
        _ = write_task => {},
        _ = read_task => {},
        _ = broadcast_task => {},
    }

    let count = game.player_count.fetch_sub(1, Ordering::Relaxed) - 1;
    log::info!("[{}] Player left. Game players: {}", slug, count);
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    let port: u16 = std::env::var("BACKEND_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8000);
    let addr = format!("[::]:{}", port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            log::error!("Cannot bind to {}: {}. Set BACKEND_PORT=NNNN to use a different port.", addr, e);
            std::process::exit(1);
        }
    };

    log::info!("Listening on: {}", addr);

    let state_dir: PathBuf = std::env::var("STATE_DIR")
        .unwrap_or_else(|_| "./data".to_string())
        .into();
    if let Err(e) = std::fs::create_dir_all(&state_dir) {
        log::error!("Cannot create state directory '{}': {}", state_dir.display(), e);
        std::process::exit(1);
    }

    let mut games_map: HashMap<&'static str, Arc<Game>> = HashMap::with_capacity(games::SLUGS.len());
    for &slug in games::SLUGS.iter() {
        let state = new_state();
        let path = state_dir.join(format!("{}-state.bin", slug));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .unwrap_or_else(|e| {
                log::error!("Cannot open state file '{}': {}", path.display(), e);
                std::process::exit(1);
            });
        load_from_file_or_init(&mut file, &state, slug);

        let (backup_tx, backup_rx) = mpsc::unbounded_channel::<usize>();
        spawn_backup_writer(slug, state.clone(), file, backup_rx);

        let (broadcast_tx, _) = broadcast::channel(100);

        games_map.insert(
            slug,
            Arc::new(Game {
                state,
                player_count: AtomicUsize::new(0),
                broadcast_tx,
                backup_tx,
            }),
        );
    }

    let app_state = AppState {
        games: Arc::new(games_map),
    };

    let app = Router::new()
        .route("/ws/stats", get(stats_ws_handler))
        .route("/ws/:slug", get(ws_handler))
        .fallback_service(
            ServeDir::new("public")
                .append_index_html_on_directories(true)
                .fallback(get(spa_fallback)),
        )
        .with_state(app_state);

    axum::serve(listener, app).await.unwrap();
}
