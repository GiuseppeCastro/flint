use crate::paths;
use crate::rank::{self, RankContext};
use std::time::Instant;

struct Check {
    label: &'static str,
    ok: bool,
    detail: String,
}

pub fn run() -> i32 {
    let mut checks = Vec::new();

    checks.push(check_binary_on_path());
    checks.push(check_data_dir_permissions());

    let app = crate::app::App::open().ok();
    checks.push(check_db_open(app.is_some()));

    if let Some(app) = &app {
        checks.push(check_db_integrity(app));
        checks.push(check_zshrc_integration());
        checks.push(check_histfile());
        checks.push(benchmark_suggest(app));
    }

    let mut all_ok = true;
    for c in &checks {
        let mark = if c.ok { "OK  " } else { "WARN" };
        println!("[{mark}] {:<28} {}", c.label, c.detail);
        all_ok &= c.ok;
    }

    if all_ok {
        0
    } else {
        1
    }
}

fn check_binary_on_path() -> Check {
    Check {
        label: "flint on PATH",
        ok: true,
        detail: "running, so yes".to_string(),
    }
}

fn check_data_dir_permissions() -> Check {
    match paths::ensure_data_dir() {
        Ok(dir) => Check {
            label: "data directory",
            ok: true,
            detail: dir.display().to_string(),
        },
        Err(e) => Check {
            label: "data directory",
            ok: false,
            detail: format!("cannot create: {e}"),
        },
    }
}

fn check_db_open(opened: bool) -> Check {
    Check {
        label: "database",
        ok: opened,
        detail: if opened {
            paths::db_path().display().to_string()
        } else {
            "failed to open — try `flint clear` to reset".to_string()
        },
    }
}

fn check_db_integrity(app: &crate::app::App) -> Check {
    let result: Result<String, _> = app
        .conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0));
    match result {
        Ok(ref s) if s == "ok" => Check {
            label: "database integrity",
            ok: true,
            detail: "ok".to_string(),
        },
        Ok(s) => Check {
            label: "database integrity",
            ok: false,
            detail: s,
        },
        Err(e) => Check {
            label: "database integrity",
            ok: false,
            detail: e.to_string(),
        },
    }
}

fn check_zshrc_integration() -> Check {
    let ok = super::status::zshrc_has_init_line();
    Check {
        label: "zsh integration",
        ok,
        detail: if ok {
            "found in ~/.zshrc".to_string()
        } else {
            "add `eval \"$(flint init zsh)\"` to the end of ~/.zshrc".to_string()
        },
    }
}

fn check_histfile() -> Check {
    let path = paths::zsh_history_path();
    let exists = path.exists();
    Check {
        label: "zsh history file",
        ok: exists,
        detail: if exists {
            path.display().to_string()
        } else {
            format!("not found at {} (nothing to import yet)", path.display())
        },
    }
}

fn benchmark_suggest(app: &crate::app::App) -> Check {
    let ctx = RankContext::default();
    let iterations = 200;
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let _ = rank::suggest(&app.conn, "git", &ctx, &app.config.weights);
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p95 = samples[(samples.len() as f64 * 0.95) as usize - 1];
    let ok = p95 <= 5.0;
    Check {
        label: "ranking latency (p95)",
        ok,
        detail: format!("{p95:.3}ms over {iterations} calls (target <= 5ms)"),
    }
}
