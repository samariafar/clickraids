use include_dir::{include_dir, Dir};
use std::sync::LazyLock;

static GAMES_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/src/games");

pub static SLUGS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut slugs: Vec<&'static str> = GAMES_DIR
        .dirs()
        .filter(|d| d.get_file(d.path().join("meta.json")).is_some())
        .filter_map(|d| d.path().file_name().and_then(|n| n.to_str()))
        .collect();
    slugs.sort();
    slugs
});
