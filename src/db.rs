use rusqlite::Connection;
use std::path::Path;

pub const SCHEMA_VERSION: i32 = 1;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    // Shell commands are case-sensitive; this also lets SQLite use the
    // index for `LIKE 'prefix%'` prefix scans instead of a full table scan.
    conn.pragma_update(None, "case_sensitive_like", "ON")?;
    conn.busy_timeout(std::time::Duration::from_millis(1000))?;
    migrate(&conn)?;
    secure_permissions(path);
    Ok(conn)
}

pub fn open_in_memory() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "case_sensitive_like", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

#[cfg(unix)]
fn secure_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        if perms.mode() & 0o777 != 0o600 {
            perms.set_mode(0o600);
            let _ = std::fs::set_permissions(path, perms);
        }
    }
}

#[cfg(not(unix))]
fn secure_permissions(_path: &Path) {}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let current: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current < 1 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS commands (
                id INTEGER PRIMARY KEY,
                command TEXT NOT NULL,
                ts INTEGER NOT NULL,
                cwd TEXT,
                git_root TEXT,
                git_branch TEXT,
                exit_code INTEGER,
                duration_ms INTEGER,
                prev_command TEXT,
                accepted_suggestion INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_commands_ts ON commands(ts);

            CREATE TABLE IF NOT EXISTS command_stats (
                command TEXT PRIMARY KEY,
                total_count INTEGER NOT NULL DEFAULT 0,
                success_count INTEGER NOT NULL DEFAULT 0,
                accepted_count INTEGER NOT NULL DEFAULT 0,
                first_seen INTEGER NOT NULL,
                last_seen INTEGER NOT NULL,
                last_cwd TEXT,
                last_git_root TEXT,
                last_git_branch TEXT
            );

            CREATE TABLE IF NOT EXISTS command_repo_stats (
                command TEXT NOT NULL,
                git_root TEXT NOT NULL,
                count INTEGER NOT NULL DEFAULT 0,
                last_seen INTEGER NOT NULL,
                PRIMARY KEY (command, git_root)
            );
            CREATE INDEX IF NOT EXISTS idx_repo_stats_root ON command_repo_stats(git_root);

            CREATE TABLE IF NOT EXISTS transitions (
                prev_key TEXT NOT NULL,
                next_command TEXT NOT NULL,
                count INTEGER NOT NULL DEFAULT 0,
                last_seen INTEGER NOT NULL,
                PRIMARY KEY (prev_key, next_command)
            );
            CREATE INDEX IF NOT EXISTS idx_transitions_prev ON transitions(prev_key);

            CREATE TABLE IF NOT EXISTS ingest_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            PRAGMA user_version = 1;
            ",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_is_idempotent() {
        let conn = open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn creates_expected_tables() {
        let conn = open_in_memory().unwrap();
        for table in [
            "commands",
            "command_stats",
            "command_repo_stats",
            "transitions",
            "ingest_state",
        ] {
            let count: i32 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "missing table {table}");
        }
    }
}
