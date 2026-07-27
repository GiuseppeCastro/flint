use crate::paths;

pub fn run() -> i32 {
    let Ok(app) = crate::app::App::open() else {
        println!(
            "flint: could not open database at {}",
            paths::db_path().display()
        );
        return 1;
    };

    let unique: i64 = app
        .conn
        .query_row("SELECT count(*) FROM command_stats", [], |r| r.get(0))
        .unwrap_or(0);
    let total: i64 = app
        .conn
        .query_row("SELECT count(*) FROM commands", [], |r| r.get(0))
        .unwrap_or(0);
    let transitions: i64 = app
        .conn
        .query_row("SELECT count(*) FROM transitions", [], |r| r.get(0))
        .unwrap_or(0);
    let last_sync: Option<String> = app
        .conn
        .query_row(
            "SELECT value FROM ingest_state WHERE key = 'zsh_history_ingested'",
            [],
            |r| r.get(0),
        )
        .ok();

    println!("flint {}", env!("CARGO_PKG_VERSION"));
    println!("database:        {}", paths::db_path().display());
    println!("unique commands:  {unique}");
    println!(
        "recorded events:  {total} (raw log, capped at {})",
        app.config.max_history_records
    );
    println!("known transitions: {transitions}");
    println!(
        "zsh history synced up to entry: {}",
        last_sync.unwrap_or_else(|| "never".to_string())
    );
    println!(
        "zsh integration in ~/.zshrc: {}",
        if zshrc_has_init_line() {
            "found"
        } else {
            "not found (run `flint doctor`)"
        }
    );
    0
}

pub fn zshrc_has_init_line() -> bool {
    let path = paths::home_dir().join(".zshrc");
    std::fs::read_to_string(path)
        .map(|contents| contents.contains("flint init zsh"))
        .unwrap_or(false)
}
