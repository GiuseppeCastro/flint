use crate::{paths, store};

/// Incremental ingestion of the zsh history file. Invoked in the background
/// from shell startup (`flint sync &!`) — never on the interactive hot path.
pub fn run() -> i32 {
    let Ok(mut app) = crate::app::App::open() else {
        return 0;
    };
    let scanner = app.scanner();
    let histfile = paths::zsh_history_path();
    let _ = store::ingest_zsh_history(&mut app.conn, &histfile, &scanner);
    let _ = store::prune_if_needed(&app.conn, app.config.max_history_records);
    0
}
