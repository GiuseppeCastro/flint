use crate::args::Args;
use crate::context::detect_git_context;
use crate::rank::{self, Candidate, RankContext};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::{cursor, queue, terminal};
use std::io::{self, IsTerminal, Write};
use std::path::Path;

const MAX_ROWS: usize = 10;

/// Interactive Ctrl+R replacement. All UI is drawn to stderr so that the
/// zsh widget can capture only the final chosen command from stdout via
/// `$(...)`. Any setup failure (not a real terminal, raw mode unavailable)
/// falls back to printing nothing so the caller keeps its original buffer.
pub fn run(args: &Args) -> i32 {
    let query = args.get_owned("query").unwrap_or_default();
    let cwd = args.get_owned("cwd");
    let git_root = cwd
        .as_deref()
        .map(|c| detect_git_context(Path::new(c)))
        .unwrap_or_default();
    let ctx = RankContext {
        cwd,
        git_root: git_root.root,
        prev_key: None,
    };

    let Ok(app) = crate::app::App::open() else {
        return 1;
    };

    match interact(&app, query, &ctx) {
        Ok(Some(selected)) => {
            println!("{selected}");
            0
        }
        Ok(None) => 0,
        Err(_) => 0,
    }
}

fn interact(
    app: &crate::app::App,
    mut query: String,
    ctx: &RankContext,
) -> io::Result<Option<String>> {
    if !io::stdin().is_terminal() {
        return Ok(None);
    }
    terminal::enable_raw_mode()?;
    let result = run_loop(app, &mut query, ctx);
    terminal::disable_raw_mode()?;
    result
}

fn run_loop(
    app: &crate::app::App,
    query: &mut String,
    ctx: &RankContext,
) -> io::Result<Option<String>> {
    let mut out = io::stderr();
    let mut selected: usize = 0;
    let mut drawn_lines: u16 = 0;
    let mut results = search_candidates(app, query, ctx);

    loop {
        drawn_lines = redraw(&mut out, query, &results, selected, drawn_lines)?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Esc => {
                    clear_lines(&mut out, drawn_lines)?;
                    return Ok(None);
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    clear_lines(&mut out, drawn_lines)?;
                    return Ok(None);
                }
                KeyCode::Enter => {
                    clear_lines(&mut out, drawn_lines)?;
                    return Ok(results.get(selected).map(|c| c.command.clone()));
                }
                KeyCode::Up => selected = selected.saturating_sub(1),
                KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    selected = selected.saturating_sub(1)
                }
                KeyCode::Down => {
                    if selected + 1 < results.len() {
                        selected += 1;
                    }
                }
                KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if selected + 1 < results.len() {
                        selected += 1;
                    }
                }
                KeyCode::Backspace => {
                    query.pop();
                    selected = 0;
                    results = search_candidates(app, query, ctx);
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    query.push(c);
                    selected = 0;
                    results = search_candidates(app, query, ctx);
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn search_candidates(app: &crate::app::App, query: &str, ctx: &RankContext) -> Vec<Candidate> {
    rank::search(&app.conn, query, ctx, &app.config.weights, MAX_ROWS).unwrap_or_default()
}

// Both `clear_lines` and `redraw` assume the cursor sits on the *last* row
// of the previously drawn block (where the last write left it, since we
// never emit a trailing newline) — so returning to the first row takes
// `count - 1` moves up, not `count`.
fn clear_lines(out: &mut impl Write, count: u16) -> io::Result<()> {
    if count == 0 {
        return Ok(());
    }
    if count > 1 {
        queue!(out, cursor::MoveUp(count - 1))?;
    }
    queue!(
        out,
        cursor::MoveToColumn(0),
        terminal::Clear(terminal::ClearType::CurrentLine)
    )?;
    for _ in 1..count {
        queue!(
            out,
            cursor::MoveToNextLine(1),
            terminal::Clear(terminal::ClearType::CurrentLine)
        )?;
    }
    if count > 1 {
        queue!(out, cursor::MoveUp(count - 1))?;
    }
    queue!(out, cursor::MoveToColumn(0))?;
    out.flush()
}

fn redraw(
    out: &mut impl Write,
    query: &str,
    results: &[Candidate],
    selected: usize,
    prev_lines: u16,
) -> io::Result<u16> {
    if prev_lines > 1 {
        queue!(out, cursor::MoveUp(prev_lines - 1))?;
    }
    let cols = terminal::size()
        .map(|(c, _)| c as usize)
        .unwrap_or(80)
        .max(10);

    queue!(
        out,
        cursor::MoveToColumn(0),
        terminal::Clear(terminal::ClearType::CurrentLine)
    )?;
    write!(
        out,
        "flint search> {}",
        truncate(query, cols.saturating_sub(14))
    )?;
    let mut lines = 1u16;

    if results.is_empty() {
        queue!(
            out,
            cursor::MoveToNextLine(1),
            terminal::Clear(terminal::ClearType::CurrentLine)
        )?;
        write!(out, "  (no matches)")?;
        lines += 1;
    } else {
        for (i, candidate) in results.iter().enumerate() {
            queue!(
                out,
                cursor::MoveToNextLine(1),
                terminal::Clear(terminal::ClearType::CurrentLine)
            )?;
            let marker = if i == selected { "> " } else { "  " };
            write!(
                out,
                "{marker}{}",
                truncate(&candidate.command, cols.saturating_sub(2))
            )?;
            lines += 1;
        }
    }
    out.flush()?;
    Ok(lines)
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let take = max.saturating_sub(1);
        format!("{}…", s.chars().take(take).collect::<String>())
    }
}
