use rexpect::session::spawn_command;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Spawns a real, isolated zsh (via `ZDOTDIR`, `--no-globalrcs`) with flint's
/// integration sourced, so these exercise the actual shell hooks rather than
/// just the Rust side. Slower and more fragile than the CLI/unit tests, so
/// kept to a handful of golden-path checks.
fn spawn_test_shell(zdotdir: &std::path::Path) -> rexpect::session::PtySession {
    let flint_bin = PathBuf::from(env!("CARGO_BIN_EXE_flint"));
    let flint_dir = flint_bin.parent().unwrap();
    let path = format!(
        "{}:{}",
        flint_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    std::fs::write(
        zdotdir.join(".zshrc"),
        format!(
            "unsetopt PROMPT_SP\n\
             PS1='TESTPROMPT$ '\n\
             export HISTFILE={zdotdir}/.zsh_history\n\
             export HISTSIZE=1000\n\
             export SAVEHIST=1000\n\
             setopt EXTENDED_HISTORY\n\
             export FLINT_DATA_DIR={zdotdir}/.flint_data\n\
             docker() {{ :; }}\n\
             eval \"$(flint init zsh)\"\n\
             echo ZSHRC-DONE\n",
            zdotdir = zdotdir.display()
        ),
    )
    .unwrap();

    let mut cmd = Command::new("zsh");
    cmd.args(["--no-globalrcs", "-i"]);
    cmd.env("ZDOTDIR", zdotdir);
    cmd.env("HOME", zdotdir);
    cmd.env("PATH", path);
    cmd.env("TERM", "xterm");

    let mut session = spawn_command(cmd, Some(5_000)).expect("failed to spawn zsh under pty");
    session
        .exp_string("ZSHRC-DONE")
        .expect("zshrc did not finish loading");
    session
        .exp_string("TESTPROMPT$ ")
        .expect("first prompt never appeared");
    session
}

#[test]
fn inline_suggestion_appears_after_repeated_command() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = spawn_test_shell(dir.path());

    for _ in 0..3 {
        session.send_line("docker compose up -d").unwrap();
        session.exp_string("TESTPROMPT$ ").unwrap();
    }
    // Give the backgrounded `flint record &!` calls a moment to land.
    std::thread::sleep(Duration::from_millis(300));

    session.send("docker co").unwrap();
    session.flush().unwrap();
    session
        .exp_string("mpose up -d")
        .expect("expected ghost suggestion 'mpose up -d' to appear after 'docker co'");

    session.send_control('u').unwrap(); // clear the line before exiting
    session.send_line("exit").unwrap();
}

#[test]
fn accepting_suggestion_with_right_arrow_fills_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = spawn_test_shell(dir.path());

    for _ in 0..3 {
        session.send_line("git status").unwrap();
        session.exp_string("TESTPROMPT$ ").unwrap();
    }
    std::thread::sleep(Duration::from_millis(300));

    session.send("git st").unwrap();
    session.flush().unwrap();
    session
        .exp_string("atus")
        .expect("expected ghost suggestion for 'git st'");

    session.send("\x1b[C").unwrap(); // Right arrow: accept suggestion
    session.flush().unwrap();
    session.send_line("").unwrap();
    session.exp_string("TESTPROMPT$ ").unwrap();

    session.send_line("exit").unwrap();
}

#[test]
fn plain_shell_commands_still_execute_normally() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = spawn_test_shell(dir.path());

    session.send_line("echo hello-from-zsh").unwrap();
    session
        .exp_string("hello-from-zsh")
        .expect("normal command execution must be unaffected by the flint integration");

    session.send_line("exit").unwrap();
}
