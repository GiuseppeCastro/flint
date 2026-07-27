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
             \n\
             # Test-only instrumentation (not part of flint itself): dumps\n\
             # region_highlight's length and the current BUFFER so tests can\n\
             # assert on ZLE state that isn't otherwise observable from a PTY.\n\
             _test_dump_state() {{ print -r -- \"STATE:${{#region_highlight}}:$BUFFER\" >> {zdotdir}/test_state.log }}\n\
             zle -N _test_dump_state\n\
             bindkey '^T' _test_dump_state\n\
             \n\
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
    // Plain "xterm" doesn't support color index 8+, so zsh silently
    // downgrades `fg=8` region_highlight entries to "none" — masking the
    // exact state these tests assert on. Real terminals overwhelmingly
    // report 256-color support, so match that instead of the more limited
    // default.
    cmd.env("TERM", "xterm-256color");

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

/// Sends Ctrl-T (bound in the test .zshrc to dump `${#region_highlight}`
/// and `$BUFFER`) and returns the freshly appended `(highlight_count,
/// buffer)` line from the log.
fn dump_state(
    session: &mut rexpect::session::PtySession,
    log_path: &std::path::Path,
) -> (usize, String) {
    session.send_control('t').unwrap();
    std::thread::sleep(Duration::from_millis(150));
    let contents = std::fs::read_to_string(log_path).unwrap_or_default();
    let last = contents.lines().last().expect("no STATE line logged yet");
    let rest = last.strip_prefix("STATE:").expect("malformed STATE line");
    let (count, buffer) = rest.split_once(':').unwrap();
    (count.parse().unwrap(), buffer.to_string())
}

#[test]
fn region_highlight_never_accumulates_stale_entries() {
    // Regression: region_highlight was append-only in the zsh integration,
    // so every recomputed ghost suggestion left the *previous* one's dim
    // span behind, eventually painting real typed text gray.
    let dir = tempfile::tempdir().unwrap();
    let mut session = spawn_test_shell(dir.path());
    let log_path = dir.path().join("test_state.log");

    for _ in 0..3 {
        session.send_line("git status").unwrap();
        session.exp_string("TESTPROMPT$ ").unwrap();
    }
    std::thread::sleep(Duration::from_millis(300));

    for ch in "git status".chars() {
        session.send(&ch.to_string()).unwrap();
        session.flush().unwrap();
        std::thread::sleep(Duration::from_millis(80));
        let (count, _) = dump_state(&mut session, &log_path);
        assert!(
            count <= 1,
            "region_highlight grew to {count} entries mid-typing"
        );
    }

    session.send_control('u').unwrap();
    session.send_line("exit").unwrap();
}

#[test]
fn end_key_does_not_swallow_suggestion_when_cursor_not_at_end() {
    // Regression: End/Ctrl-E accepted the ghost suggestion unconditionally,
    // even with the cursor in the middle of the buffer, silently appending
    // suggested text the user never asked to accept.
    let dir = tempfile::tempdir().unwrap();
    let mut session = spawn_test_shell(dir.path());
    let log_path = dir.path().join("test_state.log");

    for _ in 0..3 {
        session.send_line("git status --verbose").unwrap();
        session.exp_string("TESTPROMPT$ ").unwrap();
    }
    std::thread::sleep(Duration::from_millis(300));

    session.send("git status").unwrap();
    session.flush().unwrap();
    session
        .exp_string("--verbose")
        .expect("expected a ghost suggestion after 'git status'");

    for _ in 0..3 {
        session.send("\x1b[D").unwrap(); // Left arrow: move cursor off the end
    }
    session.flush().unwrap();
    std::thread::sleep(Duration::from_millis(100));

    session.send("\x05").unwrap(); // Ctrl-E / End
    session.flush().unwrap();
    std::thread::sleep(Duration::from_millis(100));

    let (_, buffer) = dump_state(&mut session, &log_path);
    assert_eq!(
        buffer, "git status",
        "End must move to the real end of line, not silently accept the ghost suggestion"
    );

    session.send_control('u').unwrap();
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
