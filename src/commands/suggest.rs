use crate::args::Args;
use crate::context::detect_git_context;
use crate::rank::{self, RankContext};
use crate::transitions::normalize_key;
use std::path::Path;

/// Prints only the remainder of `prefix` to complete it into the suggested
/// command (what zsh's ghost-text `POSTDISPLAY` needs), or nothing. Every
/// failure path prints nothing and returns success — this runs on every
/// keystroke and must never surface noise or a nonzero exit into the shell.
pub fn run(args: &Args) -> i32 {
    let Some(prefix) = args.get("prefix") else {
        return 0;
    };
    if prefix.is_empty() {
        return 0;
    }
    let Ok(app) = crate::app::App::open() else {
        return 0;
    };

    let cwd = args.get("cwd");
    let git_root = cwd
        .map(|c| detect_git_context(Path::new(c)))
        .unwrap_or_default();
    let prev_key = args
        .get("prev")
        .filter(|p| !p.is_empty())
        .map(normalize_key);

    let ctx = RankContext {
        cwd: cwd.map(str::to_string),
        git_root: git_root.root,
        prev_key,
    };

    let Ok(Some(best)) = rank::suggest(&app.conn, prefix, &ctx, &app.config.weights) else {
        return 0;
    };
    if let Some(remainder) = best.strip_prefix(prefix) {
        if !remainder.is_empty() {
            print!("{remainder}");
        }
    }
    0
}
