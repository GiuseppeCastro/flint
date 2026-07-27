use rusqlite::Connection;
use std::collections::HashMap;

const MAX_KEY_TOKENS: usize = 3;

/// Reduces a command to a short, deterministic key used to bucket "what
/// tends to follow this". We keep leading plain-word tokens (the
/// command + subcommand chain, e.g. `docker compose build`) and stop at the
/// first flag or argument-shaped token, so distinct subcommands
/// (`docker compose build` vs `docker compose logs`) stay distinct while
/// their trailing arguments don't fragment the bucket.
pub fn normalize_key(command: &str) -> String {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    let mut key_tokens: Vec<&str> = Vec::with_capacity(MAX_KEY_TOKENS);
    for tok in &tokens {
        if key_tokens.len() >= MAX_KEY_TOKENS {
            break;
        }
        if tok.starts_with('-') || looks_like_value(tok) {
            break;
        }
        key_tokens.push(tok);
    }
    if key_tokens.is_empty() {
        return tokens.first().copied().unwrap_or("").to_string();
    }
    key_tokens.join(" ")
}

fn looks_like_value(tok: &str) -> bool {
    if tok == "." || tok == ".." {
        return true;
    }
    if tok.starts_with("./")
        || tok.starts_with('/')
        || tok.starts_with('~')
        || tok.starts_with("../")
    {
        return true;
    }
    if tok.contains('=') {
        return true;
    }
    if tok.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return true;
    }
    false
}

pub fn record_transition(
    conn: &Connection,
    prev_key: &str,
    next_command: &str,
    ts: i64,
) -> rusqlite::Result<()> {
    if prev_key.is_empty() {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO transitions (prev_key, next_command, count, last_seen)
         VALUES (?1, ?2, 1, ?3)
         ON CONFLICT(prev_key, next_command)
         DO UPDATE SET count = count + 1, last_seen = excluded.last_seen",
        rusqlite::params![prev_key, next_command, ts],
    )?;
    Ok(())
}

/// Returns `next_command -> probability` for everything ever seen following
/// `prev_key`, where probability is that command's share of all recorded
/// transitions out of `prev_key`.
pub fn transitions_for(
    conn: &Connection,
    prev_key: &str,
) -> rusqlite::Result<HashMap<String, f64>> {
    if prev_key.is_empty() {
        return Ok(HashMap::new());
    }
    let mut stmt =
        conn.prepare("SELECT next_command, count FROM transitions WHERE prev_key = ?1")?;
    let rows: Vec<(String, i64)> = stmt
        .query_map([prev_key], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let total: i64 = rows.iter().map(|(_, c)| c).sum();
    if total == 0 {
        return Ok(HashMap::new());
    }
    Ok(rows
        .into_iter()
        .map(|(cmd, count)| (cmd, count as f64 / total as f64))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn normalizes_git_subcommands() {
        assert_eq!(normalize_key("git add ."), "git add");
        assert_eq!(normalize_key("git commit -m 'fix'"), "git commit");
        assert_eq!(normalize_key("git push"), "git push");
    }

    #[test]
    fn normalizes_docker_compose_subcommands_distinctly() {
        assert_eq!(
            normalize_key("docker compose build"),
            "docker compose build"
        );
        assert_eq!(normalize_key("docker compose up -d"), "docker compose up");
        assert_eq!(
            normalize_key("docker compose logs -f api"),
            "docker compose logs"
        );
        assert_eq!(
            normalize_key("docker compose exec postgres psql"),
            "docker compose exec"
        );
    }

    #[test]
    fn falls_back_to_base_command_for_flag_only_invocations() {
        assert_eq!(normalize_key("ls -la"), "ls");
        assert_eq!(normalize_key("-weird"), "-weird");
        assert_eq!(normalize_key(""), "");
    }

    #[test]
    fn learns_and_ranks_transitions() {
        let conn = db::open_in_memory().unwrap();
        let key = normalize_key("git add .");
        record_transition(&conn, &key, "git commit -m 'wip'", 1).unwrap();
        record_transition(&conn, &key, "git commit -m 'wip'", 2).unwrap();
        record_transition(&conn, &key, "git status", 3).unwrap();

        let scores = transitions_for(&conn, &key).unwrap();
        assert!(scores["git commit -m 'wip'"] > scores["git status"]);
        let total: f64 = scores.values().sum();
        assert!((total - 1.0).abs() < 1e-9);
    }

    #[test]
    fn unknown_prev_key_has_no_transitions() {
        let conn = db::open_in_memory().unwrap();
        assert!(transitions_for(&conn, "never seen").unwrap().is_empty());
    }
}
