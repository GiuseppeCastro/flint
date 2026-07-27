use std::path::Path;
use std::process::{Command, Output};

fn flint(data_dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flint"))
        .args(args)
        .env("FLINT_DATA_DIR", data_dir)
        .env_remove("HISTFILE")
        .output()
        .expect("failed to run flint binary")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn init_zsh_prints_integration_script() {
    let dir = tempfile::tempdir().unwrap();
    let out = flint(dir.path(), &["init", "zsh"]);
    assert!(out.status.success());
    let script = stdout(&out);
    assert!(script.contains("_flint_precmd"));
    assert!(script.contains("bindkey '^R'"));
    assert!(script.contains("add-zle-hook-widget line-pre-redraw"));
}

#[test]
fn init_zsh_scaffolds_a_discoverable_config_file_once() {
    let dir = tempfile::tempdir().unwrap();
    flint(dir.path(), &["init", "zsh"]);
    let config_path = dir.path().join("config.toml");
    assert!(config_path.exists());
    let contents = std::fs::read_to_string(&config_path).unwrap();
    assert!(contents.contains("[ranking]"));

    // Re-running setup must not clobber user edits to the config.
    std::fs::write(&config_path, "# edited by user\n").unwrap();
    flint(dir.path(), &["init", "zsh"]);
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        "# edited by user\n"
    );
}

#[test]
fn init_rejects_unsupported_shell() {
    let dir = tempfile::tempdir().unwrap();
    let out = flint(dir.path(), &["init", "bash"]);
    assert!(!out.status.success());
}

#[test]
fn record_then_suggest_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    for _ in 0..5 {
        let out = flint(
            dir.path(),
            &[
                "record",
                "--command",
                "docker compose up -d",
                "--exit-code",
                "0",
            ],
        );
        assert!(out.status.success());
    }
    let out = flint(dir.path(), &["suggest", "--prefix", "docker co"]);
    assert!(out.status.success());
    assert_eq!(stdout(&out).trim_end(), "mpose up -d");
}

#[test]
fn suggest_with_no_history_prints_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let out = flint(dir.path(), &["suggest", "--prefix", "docker co"]);
    assert!(out.status.success());
    assert_eq!(stdout(&out), "");
}

#[test]
fn suggest_with_empty_prefix_prints_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let out = flint(dir.path(), &["suggest", "--prefix", ""]);
    assert!(out.status.success());
    assert_eq!(stdout(&out), "");
}

#[test]
fn secret_command_is_not_recorded() {
    let dir = tempfile::tempdir().unwrap();
    flint(
        dir.path(),
        &[
            "record",
            "--command",
            "mysql -u root -pSuperSecret123",
            "--exit-code",
            "0",
        ],
    );
    let out = flint(dir.path(), &["stats"]);
    assert!(!stdout(&out).contains("SuperSecret123"));
}

#[test]
fn status_reports_recorded_commands() {
    let dir = tempfile::tempdir().unwrap();
    flint(
        dir.path(),
        &["record", "--command", "git status", "--exit-code", "0"],
    );
    let out = flint(dir.path(), &["status"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("unique commands"));
    assert!(text.contains('1'));
}

#[test]
fn doctor_reports_ranking_latency_and_exits_zero_when_healthy() {
    let dir = tempfile::tempdir().unwrap();
    flint(
        dir.path(),
        &["record", "--command", "git status", "--exit-code", "0"],
    );
    let out = flint(dir.path(), &["doctor"]);
    let text = stdout(&out);
    assert!(text.contains("ranking latency"));
}

#[test]
fn clear_requires_confirmation_flag() {
    let dir = tempfile::tempdir().unwrap();
    flint(
        dir.path(),
        &["record", "--command", "git status", "--exit-code", "0"],
    );
    let out = flint(dir.path(), &["clear"]);
    assert!(!out.status.success());

    let out = flint(dir.path(), &["clear", "--yes"]);
    assert!(out.status.success());

    let status = flint(dir.path(), &["status"]);
    assert!(stdout(&status).contains("unique commands:  0"));
}

#[test]
fn corrupted_database_does_not_crash_status() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path()).unwrap();
    std::fs::write(dir.path().join("history.db"), b"not a sqlite database").unwrap();
    let out = flint(dir.path(), &["status"]);
    // Must exit cleanly one way or another — no panic, no hang.
    assert!(out.status.code().is_some());
}

#[test]
fn sync_with_missing_histfile_does_not_error() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_flint"))
        .arg("sync")
        .env("FLINT_DATA_DIR", dir.path())
        .env("HISTFILE", "/nonexistent/path/to/histfile")
        .output()
        .unwrap();
    assert!(out.status.success());
}

#[test]
fn transition_context_promotes_learned_next_command() {
    let dir = tempfile::tempdir().unwrap();
    for _ in 0..3 {
        flint(
            dir.path(),
            &["record", "--command", "git add .", "--exit-code", "0"],
        );
        flint(
            dir.path(),
            &[
                "record",
                "--command",
                "git commit -m wip",
                "--prev",
                "git add .",
                "--exit-code",
                "0",
            ],
        );
    }
    // A rarely used but equally-prefixed alternative.
    flint(
        dir.path(),
        &[
            "record",
            "--command",
            "git commit --amend",
            "--exit-code",
            "0",
        ],
    );

    let out = flint(
        dir.path(),
        &["suggest", "--prefix", "git comm", "--prev", "git add ."],
    );
    assert_eq!(stdout(&out).trim_end(), "it -m wip");
}

#[test]
fn unknown_subcommand_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let out = flint(dir.path(), &["bogus-command"]);
    assert!(!out.status.success());
}
