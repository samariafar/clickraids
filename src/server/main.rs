use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
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

#[derive(Clone)]
struct AppState {
    state: Bitmap,
    broadcast_tx: broadcast::Sender<CheckboxUpdate>,
    backup_tx: mpsc::UnboundedSender<usize>,
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

fn load_from_file_or_init(file: &mut File, state: &Bitmap) {
    let file_len = file.metadata().unwrap().len();

    if file_len == 0 {
        let buffer = [0u8; SNAPSHOT_BYTES];

        file.write_all(&buffer).unwrap();

        log::info!("Initialized new state file with default state.");
    } else {
        let mut buffer = [0u8; SNAPSHOT_BYTES];

        file.seek(SeekFrom::Start(0)).unwrap();
        file.read_exact(&mut buffer).unwrap();

        for (i, chunk) in buffer.chunks_exact(8).enumerate() {
            let word = u64::from_le_bytes(chunk.try_into().unwrap());
            state[i].store(word, Ordering::Relaxed);
        }

        log::info!("Loaded state from existing file.");
    }
}

// Single dedicated writer task; reads the affected byte's current value from the in-memory atomic
// (the source of truth) and persists it. The file is a recovery backup, never on the serving path.
fn spawn_backup_writer(state: Bitmap, mut file: File, mut rx: mpsc::UnboundedReceiver<usize>) {
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
                log::error!("Backup write failed at byte {}: {}", byte_pos, e);
            }
        }
    });
}

async fn ws_handler(
    upgrade: WebSocketUpgrade,
    State(app): State<AppState>,
) -> impl IntoResponse {
    upgrade.on_upgrade(move |socket| handle_socket(socket, app))
}

async fn handle_socket(socket: WebSocket, app: AppState) {
    let mut last_update_time = Instant::now();
    let update_interval = Duration::from_secs_f32(1.0 / 3.0);

    let (mut write, mut read) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let mut receiver = app.broadcast_tx.subscribe();

    {
        let bytes = snapshot_bytes(&app.state);
        let compressed = zstd::bulk::compress(&bytes, 0).unwrap();
        let message = Message::Binary(compressed);

        if tx.send(message).is_err() {
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

    let state = app.state.clone();
    let backup_tx = app.backup_tx.clone();
    let broadcast_tx = app.broadcast_tx.clone();
    let read_task = task::spawn(async move {
        while let Some(Ok(message)) = read.next().await {
            if let Message::Binary(bytes) = message {
                if last_update_time.elapsed() >= update_interval {
                    last_update_time = Instant::now();

                    if let Some(update) = CheckboxUpdate::from_bytes(&bytes) {
                        set_bit(&state, update.index, update.state);
                        let _ = backup_tx.send(update.index);

                        log::info!("Checkbox at index {} updated to state: {}", update.index, update.state);

                        let _ = broadcast_tx.send(update);
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

    let state = new_state();
    let (broadcast_tx, _) = broadcast::channel(100);

    let state_path = std::env::var("STATE_FILE").unwrap_or_else(|_| "./state.bin".to_string());
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&state_path)
        .unwrap_or_else(|e| {
            log::error!("Cannot open state file '{}': {}", state_path, e);
            std::process::exit(1);
        });

    load_from_file_or_init(&mut file, &state);

    let (backup_tx, backup_rx) = mpsc::unbounded_channel::<usize>();
    spawn_backup_writer(state.clone(), file, backup_rx);

    let app_state = AppState {
        state,
        broadcast_tx,
        backup_tx,
    };

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new("public").append_index_html_on_directories(true))
        .with_state(app_state);

    axum::serve(listener, app).await.unwrap();
}
