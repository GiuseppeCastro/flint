use crate::args::Args;
use crate::context::detect_git_context;
use crate::store::{self, RecordInput};
use crate::time::now_unix;
use std::path::Path;

pub fn run(args: &Args) -> i32 {
    let Some(command) = args.get("command").filter(|c| !c.is_empty()) else {
        return 0;
    };
    let Ok(mut app) = crate::app::App::open() else {
        return 0;
    };

    let cwd = args.get_owned("cwd");
    let git = cwd
        .as_deref()
        .map(|c| detect_git_context(Path::new(c)))
        .unwrap_or_default();

    let input = RecordInput {
        command: command.to_string(),
        ts: now_unix(),
        cwd,
        git_root: git.root,
        git_branch: git.branch,
        exit_code: args.get_i64("exit-code").map(|c| c as i32),
        duration_ms: args.get_i64("duration-ms"),
        prev_command: args.get_owned("prev").filter(|p| !p.is_empty()),
        accepted_suggestion: args.get_bool("accepted"),
    };

    let scanner = app.scanner();
    let _ = store::record(&mut app.conn, &input, &scanner);
    let _ = store::prune_if_needed(&app.conn, app.config.max_history_records);
    0
}
