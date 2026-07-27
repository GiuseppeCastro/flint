/// A single parsed entry from a zsh history file. `timestamp` is `None` for
/// plain (non-extended-history) files where zsh never recorded one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub timestamp: Option<i64>,
    pub command: String,
}

struct Pending {
    ts: Option<i64>,
    buf: String,
}

/// Parses a zsh `$HISTFILE`. Supports both `EXTENDED_HISTORY`
/// (`: <epoch>:<elapsed>;<command>`) and plain one-command-per-line format,
/// and joins commands that span multiple physical lines (zsh writes a
/// trailing `\` to continue a command with embedded newlines onto the next
/// line).
pub fn parse_zsh_history(text: &str) -> Vec<HistoryEntry> {
    let mut entries = Vec::new();
    let mut pending: Option<Pending> = None;

    for line in text.lines() {
        if let Some(p) = pending.as_mut() {
            if let Some(stripped) = line.strip_suffix('\\') {
                p.buf.push('\n');
                p.buf.push_str(stripped);
                continue;
            }
            p.buf.push('\n');
            p.buf.push_str(line);
            let p = pending.take().unwrap();
            push_entry(&mut entries, p.ts, p.buf);
            continue;
        }

        if let Some(entry) = try_parse_extended(line, &mut pending) {
            entries.push(entry);
            continue;
        }
        if pending.is_some() {
            continue; // extended entry started a continuation, handled above next iteration
        }
        if !line.trim().is_empty() {
            entries.push(HistoryEntry {
                timestamp: None,
                command: line.to_string(),
            });
        }
    }
    if let Some(p) = pending.take() {
        push_entry(&mut entries, p.ts, p.buf);
    }
    entries
}

fn push_entry(entries: &mut Vec<HistoryEntry>, ts: Option<i64>, command: String) {
    if !command.trim().is_empty() {
        entries.push(HistoryEntry {
            timestamp: ts,
            command,
        });
    }
}

fn try_parse_extended(line: &str, pending: &mut Option<Pending>) -> Option<HistoryEntry> {
    let rest = line.strip_prefix(": ")?;
    let (meta, cmd) = rest.split_once(';')?;
    let (ts_str, _elapsed) = meta.split_once(':')?;
    let ts: i64 = ts_str.trim().parse().ok()?;

    if let Some(stripped) = cmd.strip_suffix('\\') {
        *pending = Some(Pending {
            ts: Some(ts),
            buf: stripped.to_string(),
        });
        None
    } else if cmd.trim().is_empty() {
        None
    } else {
        Some(HistoryEntry {
            timestamp: Some(ts),
            command: cmd.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extended_history_lines() {
        let text = ": 1700000000:0;git status\n: 1700000005:2;docker compose up -d\n";
        let entries = parse_zsh_history(text);
        assert_eq!(
            entries,
            vec![
                HistoryEntry {
                    timestamp: Some(1700000000),
                    command: "git status".to_string()
                },
                HistoryEntry {
                    timestamp: Some(1700000005),
                    command: "docker compose up -d".to_string()
                },
            ]
        );
    }

    #[test]
    fn parses_plain_history_lines_without_timestamp() {
        let text = "git status\nls -la\n";
        let entries = parse_zsh_history(text);
        assert_eq!(
            entries,
            vec![
                HistoryEntry {
                    timestamp: None,
                    command: "git status".to_string()
                },
                HistoryEntry {
                    timestamp: None,
                    command: "ls -la".to_string()
                },
            ]
        );
    }

    #[test]
    fn joins_multiline_commands() {
        let text = ": 1700000000:0;echo \"foo\\\nbar\"\n: 1700000010:0;git status\n";
        let entries = parse_zsh_history(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].command, "echo \"foo\nbar\"");
        assert_eq!(entries[1].command, "git status");
    }

    #[test]
    fn ignores_blank_lines() {
        let text = ": 1700000000:0;git status\n\n\nls\n";
        let entries = parse_zsh_history(text);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn handles_mixed_extended_and_plain_lines() {
        let text = ": 1700000000:0;git status\nls -la\n";
        let entries = parse_zsh_history(text);
        assert_eq!(entries[0].timestamp, Some(1700000000));
        assert_eq!(entries[1].timestamp, None);
    }

    #[test]
    fn empty_file_yields_no_entries() {
        assert!(parse_zsh_history("").is_empty());
    }
}
