use crate::history::parse_zsh_history;
use crate::secrets::SecretScanner;
use crate::transitions;
use rusqlite::Connection;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct RecordInput {
    pub command: String,
    pub ts: i64,
    pub cwd: Option<String>,
    pub git_root: Option<String>,
    pub git_branch: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
    pub prev_command: Option<String>,
    pub accepted_suggestion: bool,
}

/// Persists one executed command: raw log entry, rolling aggregates used by
/// the ranking engine, and (if there was a previous command in this
/// session) the transition between the two. Returns `false` without writing
/// anything if the command looks like it carries a secret.
pub fn record(
    conn: &mut Connection,
    input: &RecordInput,
    scanner: &SecretScanner,
) -> rusqlite::Result<bool> {
    if scanner.is_sensitive(&input.command) {
        return Ok(false);
    }

    let tx = conn.transaction()?;
    let success = matches!(input.exit_code, Some(0));

    tx.execute(
        "INSERT INTO commands (command, ts, cwd, git_root, git_branch, exit_code, duration_ms, prev_command, accepted_suggestion)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            input.command,
            input.ts,
            input.cwd,
            input.git_root,
            input.git_branch,
            input.exit_code,
            input.duration_ms,
            input.prev_command,
            input.accepted_suggestion as i64,
        ],
    )?;

    tx.execute(
        "INSERT INTO command_stats (command, total_count, success_count, accepted_count, first_seen, last_seen, last_cwd, last_git_root, last_git_branch)
         VALUES (?1, 1, ?2, ?3, ?4, ?4, ?5, ?6, ?7)
         ON CONFLICT(command) DO UPDATE SET
           total_count = total_count + 1,
           success_count = success_count + ?2,
           accepted_count = accepted_count + ?3,
           last_seen = excluded.last_seen,
           last_cwd = excluded.last_cwd,
           last_git_root = excluded.last_git_root,
           last_git_branch = excluded.last_git_branch",
        rusqlite::params![
            input.command,
            success as i64,
            input.accepted_suggestion as i64,
            input.ts,
            input.cwd,
            input.git_root,
            input.git_branch,
        ],
    )?;

    if let Some(root) = &input.git_root {
        tx.execute(
            "INSERT INTO command_repo_stats (command, git_root, count, last_seen)
             VALUES (?1, ?2, 1, ?3)
             ON CONFLICT(command, git_root) DO UPDATE SET count = count + 1, last_seen = excluded.last_seen",
            rusqlite::params![input.command, root, input.ts],
        )?;
    }

    if let Some(prev) = &input.prev_command {
        let key = transitions::normalize_key(prev);
        transitions::record_transition(&tx, &key, &input.command, input.ts)?;
    }

    tx.commit()?;
    Ok(true)
}

/// Deletes oldest raw log rows once they exceed `max_records` by a 10%
/// margin, so we don't pay a delete cost on every single insert. This only
/// touches the audit-log `commands` table — the aggregates the ranking
/// engine actually reads (`command_stats`, `command_repo_stats`,
/// `transitions`) are unaffected and stay small on their own.
pub fn prune_if_needed(conn: &Connection, max_records: u64) -> rusqlite::Result<()> {
    let count: i64 = conn.query_row("SELECT count(*) FROM commands", [], |r| r.get(0))?;
    let threshold = (max_records as f64 * 1.1) as i64;
    if count <= threshold {
        return Ok(());
    }
    conn.execute(
        "DELETE FROM commands WHERE id NOT IN (SELECT id FROM commands ORDER BY id DESC LIMIT ?1)",
        [max_records as i64],
    )?;
    Ok(())
}

/// Ingests any zsh history entries not yet seen. Progress is tracked as an
/// entry count in `ingest_state`, so re-running this after nothing new has
/// been appended is a cheap no-op past the initial file read.
pub fn ingest_zsh_history(
    conn: &mut Connection,
    histfile: &Path,
    scanner: &SecretScanner,
) -> std::io::Result<usize> {
    let Ok(bytes) = std::fs::read(histfile) else {
        return Ok(0);
    };
    let text = String::from_utf8_lossy(&bytes);
    let entries = parse_zsh_history(&text);

    let already_ingested: usize = conn
        .query_row(
            "SELECT value FROM ingest_state WHERE key = 'zsh_history_ingested'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if already_ingested > entries.len() {
        // History file was rotated/truncated; restart from scratch rather
        // than skipping entries that no longer exist.
        ingest_from(conn, &entries, 0, scanner);
    } else {
        ingest_from(conn, &entries, already_ingested, scanner);
    }

    let _ = conn.execute(
        "INSERT INTO ingest_state (key, value) VALUES ('zsh_history_ingested', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![entries.len().to_string()],
    );

    Ok(entries.len().saturating_sub(already_ingested))
}

fn ingest_from(
    conn: &mut Connection,
    entries: &[crate::history::HistoryEntry],
    from: usize,
    scanner: &SecretScanner,
) {
    let mut prev_command: Option<String> = None;
    for (i, entry) in entries.iter().enumerate() {
        // Backfill a monotonically increasing synthetic timestamp for
        // plain-format history lines that never recorded one, so recency
        // ordering among them still reflects file order.
        let ts = entry.timestamp.unwrap_or(i as i64);
        if i >= from {
            let input = RecordInput {
                command: entry.command.clone(),
                ts,
                prev_command: prev_command.clone(),
                exit_code: Some(0),
                ..Default::default()
            };
            let _ = record(conn, &input, scanner);
        }
        prev_command = Some(entry.command.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn records_and_aggregates_command_stats() {
        let mut conn = db::open_in_memory().unwrap();
        let scanner = SecretScanner::default();
        let input = RecordInput {
            command: "git status".to_string(),
            ts: 100,
            exit_code: Some(0),
            ..Default::default()
        };
        assert!(record(&mut conn, &input, &scanner).unwrap());
        record(&mut conn, &input, &scanner).unwrap();

        let total: i64 = conn
            .query_row(
                "SELECT total_count FROM command_stats WHERE command = 'git status'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total, 2);
    }

    #[test]
    fn secret_command_is_never_persisted() {
        let mut conn = db::open_in_memory().unwrap();
        let scanner = SecretScanner::default();
        let input = RecordInput {
            command: "mysql -u root -pSuperSecret123".to_string(),
            ts: 100,
            ..Default::default()
        };
        assert!(!record(&mut conn, &input, &scanner).unwrap());

        let count: i64 = conn
            .query_row("SELECT count(*) FROM commands", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn records_repo_scoped_stats_when_git_root_present() {
        let mut conn = db::open_in_memory().unwrap();
        let scanner = SecretScanner::default();
        let input = RecordInput {
            command: "docker compose up -d".to_string(),
            ts: 100,
            git_root: Some("/repo/a".to_string()),
            exit_code: Some(0),
            ..Default::default()
        };
        record(&mut conn, &input, &scanner).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT count FROM command_repo_stats WHERE command = ?1 AND git_root = ?2",
                rusqlite::params!["docker compose up -d", "/repo/a"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn prune_keeps_only_most_recent_records() {
        let mut conn = db::open_in_memory().unwrap();
        let scanner = SecretScanner::default();
        for i in 0..20 {
            let input = RecordInput {
                command: format!("cmd{i}"),
                ts: i,
                exit_code: Some(0),
                ..Default::default()
            };
            record(&mut conn, &input, &scanner).unwrap();
        }
        prune_if_needed(&conn, 10).unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM commands", [], |r| r.get(0))
            .unwrap();
        assert!(
            count <= 11,
            "expected pruning to bring count near the limit, got {count}"
        );

        let newest_kept: String = conn
            .query_row(
                "SELECT command FROM commands ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(newest_kept, "cmd19");
    }

    #[test]
    fn ingest_is_idempotent_across_repeated_runs() {
        let dir = tempfile::tempdir().unwrap();
        let histfile = dir.path().join("zsh_history");
        std::fs::write(
            &histfile,
            ": 1700000000:0;git status\n: 1700000001:0;git push\n",
        )
        .unwrap();

        let mut conn = db::open_in_memory().unwrap();
        let scanner = SecretScanner::default();
        let first = ingest_zsh_history(&mut conn, &histfile, &scanner).unwrap();
        let second = ingest_zsh_history(&mut conn, &histfile, &scanner).unwrap();
        assert_eq!(first, 2);
        assert_eq!(second, 0);

        let total: i64 = conn
            .query_row(
                "SELECT total_count FROM command_stats WHERE command = 'git status'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total, 1);
    }

    #[test]
    fn ingest_picks_up_newly_appended_lines() {
        let dir = tempfile::tempdir().unwrap();
        let histfile = dir.path().join("zsh_history");
        std::fs::write(&histfile, ": 1700000000:0;git status\n").unwrap();

        let mut conn = db::open_in_memory().unwrap();
        let scanner = SecretScanner::default();
        ingest_zsh_history(&mut conn, &histfile, &scanner).unwrap();

        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&histfile)
            .unwrap();
        use std::io::Write;
        writeln!(f, ": 1700000001:0;git push").unwrap();

        let second = ingest_zsh_history(&mut conn, &histfile, &scanner).unwrap();
        assert_eq!(second, 1);
    }

    #[test]
    fn ingest_records_transitions_between_history_lines() {
        let dir = tempfile::tempdir().unwrap();
        let histfile = dir.path().join("zsh_history");
        std::fs::write(&histfile, ": 1:0;git add .\n: 2:0;git commit -m x\n").unwrap();

        let mut conn = db::open_in_memory().unwrap();
        let scanner = SecretScanner::default();
        ingest_zsh_history(&mut conn, &histfile, &scanner).unwrap();

        let scores = transitions::transitions_for(&conn, "git add").unwrap();
        assert_eq!(scores.get("git commit -m x"), Some(&1.0));
    }

    #[test]
    fn missing_histfile_ingests_nothing_without_error() {
        let mut conn = db::open_in_memory().unwrap();
        let scanner = SecretScanner::default();
        let count =
            ingest_zsh_history(&mut conn, Path::new("/nonexistent/histfile"), &scanner).unwrap();
        assert_eq!(count, 0);
    }
}
