use crate::config::RankWeights;
use crate::fuzzy::fuzzy_score_lower;
use crate::time::now_unix;
use rusqlite::Connection;
use std::collections::HashMap;

const RECENCY_HALF_LIFE_HOURS: f64 = 72.0;
const PREFIX_CANDIDATE_LIMIT: i64 = 1000;

#[derive(Debug, Clone)]
pub struct Candidate {
    pub command: String,
    pub score: f64,
}

#[derive(Debug, Clone, Default)]
pub struct RankContext {
    pub cwd: Option<String>,
    pub git_root: Option<String>,
    /// Normalized key of the previously executed command, see `transitions::normalize_key`.
    pub prev_key: Option<String>,
}

struct CommandRow {
    command: String,
    total_count: i64,
    success_count: i64,
    accepted_count: i64,
    last_seen: i64,
    last_cwd: Option<String>,
    repo_count: i64,
}

fn row_from_query(row: &rusqlite::Row) -> rusqlite::Result<CommandRow> {
    Ok(CommandRow {
        command: row.get(0)?,
        total_count: row.get(1)?,
        success_count: row.get(2)?,
        accepted_count: row.get(3)?,
        last_seen: row.get(4)?,
        last_cwd: row.get(5)?,
        repo_count: row.get(6)?,
    })
}

const SELECT_COLUMNS: &str = "cs.command, cs.total_count, cs.success_count, cs.accepted_count, \
     cs.last_seen, cs.last_cwd, COALESCE(crs.count, 0)";

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn load_prefix_candidates(
    conn: &Connection,
    prefix: &str,
    git_root: Option<&str>,
) -> rusqlite::Result<Vec<CommandRow>> {
    let pattern = format!("{}%", escape_like(prefix));
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLUMNS} FROM command_stats cs
         LEFT JOIN command_repo_stats crs ON crs.command = cs.command AND crs.git_root = ?2
         WHERE cs.command LIKE ?1 ESCAPE '\\'
         ORDER BY cs.total_count DESC LIMIT {PREFIX_CANDIDATE_LIMIT}"
    ))?;
    let rows = stmt
        .query_map(
            rusqlite::params![pattern, git_root.unwrap_or("")],
            row_from_query,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn load_all_candidates(
    conn: &Connection,
    git_root: Option<&str>,
) -> rusqlite::Result<Vec<CommandRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLUMNS} FROM command_stats cs
         LEFT JOIN command_repo_stats crs ON crs.command = cs.command AND crs.git_root = ?1"
    ))?;
    let rows = stmt
        .query_map([git_root.unwrap_or("")], row_from_query)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn frequency_component(total_count: i64) -> f64 {
    ((total_count + 1) as f64).ln()
}

fn recency_component(last_seen: i64, now: i64) -> f64 {
    let age_hours = ((now - last_seen).max(0) as f64) / 3600.0;
    0.5f64.powf(age_hours / RECENCY_HALF_LIFE_HOURS)
}

fn success_component(success_count: i64, total_count: i64) -> f64 {
    if total_count > 0 {
        success_count as f64 / total_count as f64
    } else {
        0.5
    }
}

fn acceptance_component(accepted_count: i64, total_count: i64) -> f64 {
    if total_count > 0 {
        accepted_count as f64 / total_count as f64
    } else {
        0.0
    }
}

fn score_row(
    row: &CommandRow,
    query: &str,
    q_lower: &[char],
    ctx: &RankContext,
    weights: &RankWeights,
    transitions: &HashMap<String, f64>,
    now: i64,
) -> Option<f64> {
    let fuzzy = fuzzy_score_lower(q_lower, &row.command)?;
    let prefix = if row.command.starts_with(query) {
        1.0
    } else {
        0.0
    };
    let same_cwd = match (&ctx.cwd, &row.last_cwd) {
        (Some(a), Some(b)) if a == b => 1.0,
        _ => 0.0,
    };
    let same_repo = ((row.repo_count + 1) as f64).ln();
    let transition = transitions.get(&row.command).copied().unwrap_or(0.0);

    Some(
        weights.prefix * prefix
            + weights.fuzzy * fuzzy
            + weights.frequency * frequency_component(row.total_count)
            + weights.recency * recency_component(row.last_seen, now)
            + weights.same_cwd * same_cwd
            + weights.same_repo * same_repo
            + weights.transition * transition
            + weights.success_rate * success_component(row.success_count, row.total_count)
            + weights.acceptance * acceptance_component(row.accepted_count, row.total_count),
    )
}

fn load_transitions(
    conn: &Connection,
    ctx: &RankContext,
) -> rusqlite::Result<HashMap<String, f64>> {
    match &ctx.prev_key {
        Some(key) if !key.is_empty() => crate::transitions::transitions_for(conn, key),
        _ => Ok(HashMap::new()),
    }
}

/// Single best inline (ghost-text) suggestion for `prefix`. Candidates are
/// restricted to commands that literally start with `prefix` via an indexed
/// SQL range scan before any scoring happens in Rust, which is what keeps
/// this fast regardless of total history size.
pub fn suggest(
    conn: &Connection,
    prefix: &str,
    ctx: &RankContext,
    weights: &RankWeights,
) -> rusqlite::Result<Option<String>> {
    if prefix.is_empty() {
        return Ok(None);
    }
    let rows = load_prefix_candidates(conn, prefix, ctx.git_root.as_deref())?;
    if rows.is_empty() {
        return Ok(None);
    }
    let transitions = load_transitions(conn, ctx)?;
    let now = now_unix();
    let q_lower: Vec<char> = prefix.chars().map(|c| c.to_ascii_lowercase()).collect();

    let mut best: Option<(f64, String)> = None;
    for row in rows {
        if let Some(score) = score_row(&row, prefix, &q_lower, ctx, weights, &transitions, now) {
            let is_better = match &best {
                Some((s, _)) => score > *s,
                None => true,
            };
            if is_better {
                best = Some((score, row.command));
            }
        }
    }
    Ok(best.map(|(_, cmd)| cmd))
}

/// Ranked fuzzy search across full history, powering the Ctrl+R experience.
/// Unlike `suggest`, `query` need not be a literal prefix of the result.
pub fn search(
    conn: &Connection,
    query: &str,
    ctx: &RankContext,
    weights: &RankWeights,
    limit: usize,
) -> rusqlite::Result<Vec<Candidate>> {
    let rows = load_all_candidates(conn, ctx.git_root.as_deref())?;
    let transitions = load_transitions(conn, ctx)?;
    let now = now_unix();
    let q_lower: Vec<char> = query.chars().map(|c| c.to_ascii_lowercase()).collect();

    let mut scored: Vec<Candidate> = rows
        .iter()
        .filter_map(|row| {
            score_row(row, query, &q_lower, ctx, weights, &transitions, now).map(|score| {
                Candidate {
                    command: row.command.clone(),
                    score,
                }
            })
        })
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(limit);
    Ok(scored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn seed(
        conn: &Connection,
        command: &str,
        total: i64,
        success: i64,
        last_seen: i64,
        cwd: Option<&str>,
    ) {
        conn.execute(
            "INSERT INTO command_stats (command, total_count, success_count, accepted_count, first_seen, last_seen, last_cwd)
             VALUES (?1, ?2, ?3, 0, ?4, ?4, ?5)",
            rusqlite::params![command, total, success, last_seen, cwd],
        )
        .unwrap();
    }

    #[test]
    fn prefix_ranks_above_unrelated_fuzzy_match() {
        let conn = db::open_in_memory().unwrap();
        let now = now_unix();
        seed(&conn, "docker compose up -d", 5, 5, now, None);
        seed(
            &conn,
            "docker compose logs -f api",
            1,
            1,
            now - 10_000,
            None,
        );

        let ctx = RankContext::default();
        let weights = RankWeights::default();
        let results = search(&conn, "docker co", &ctx, &weights, 10).unwrap();
        assert_eq!(results[0].command, "docker compose up -d");
    }

    #[test]
    fn same_repo_boosts_project_specific_command() {
        let conn = db::open_in_memory().unwrap();
        let now = now_unix();
        seed(&conn, "docker compose logs -f api", 10, 10, now, None);
        seed(
            &conn,
            "docker compose exec postgres psql",
            10,
            10,
            now,
            None,
        );
        conn.execute(
            "INSERT INTO command_repo_stats (command, git_root, count, last_seen) VALUES (?1, ?2, 20, ?3)",
            rusqlite::params!["docker compose exec postgres psql", "/repo/b", now],
        )
        .unwrap();

        let ctx = RankContext {
            git_root: Some("/repo/b".to_string()),
            ..Default::default()
        };
        let weights = RankWeights::default();
        let results = search(&conn, "docker co", &ctx, &weights, 10).unwrap();
        assert_eq!(results[0].command, "docker compose exec postgres psql");
    }

    #[test]
    fn recent_command_beats_old_equally_frequent_one() {
        let conn = db::open_in_memory().unwrap();
        let now = now_unix();
        seed(&conn, "git status", 5, 5, now, None);
        seed(&conn, "git stash list", 5, 5, now - 1_000_000, None);

        let ctx = RankContext::default();
        let weights = RankWeights::default();
        let results = search(&conn, "git st", &ctx, &weights, 10).unwrap();
        assert_eq!(results[0].command, "git status");
    }

    #[test]
    fn transition_probability_promotes_likely_next_command() {
        let conn = db::open_in_memory().unwrap();
        let now = now_unix();
        seed(&conn, "git push", 3, 3, now, None);
        seed(&conn, "git push --force-with-lease", 3, 3, now, None);
        crate::transitions::record_transition(&conn, "git commit", "git push", now).unwrap();
        for _ in 0..4 {
            crate::transitions::record_transition(&conn, "git commit", "git push", now).unwrap();
        }

        let ctx = RankContext {
            prev_key: Some("git commit".to_string()),
            ..Default::default()
        };
        let weights = RankWeights::default();
        let results = search(&conn, "git push", &ctx, &weights, 10).unwrap();
        assert_eq!(results[0].command, "git push");
    }

    #[test]
    fn success_rate_penalizes_frequently_failing_command() {
        let conn = db::open_in_memory().unwrap();
        let now = now_unix();
        seed(&conn, "flaky-deploy", 10, 1, now, None);
        seed(&conn, "flaky-deploy-v2", 10, 9, now, None);

        let ctx = RankContext::default();
        let weights = RankWeights::default();
        let results = search(&conn, "flaky", &ctx, &weights, 10).unwrap();
        assert_eq!(results[0].command, "flaky-deploy-v2");
    }

    #[test]
    fn empty_prefix_suggests_nothing() {
        let conn = db::open_in_memory().unwrap();
        let ctx = RankContext::default();
        let weights = RankWeights::default();
        assert_eq!(suggest(&conn, "", &ctx, &weights).unwrap(), None);
    }

    #[test]
    fn suggest_returns_single_best_prefix_match() {
        let conn = db::open_in_memory().unwrap();
        let now = now_unix();
        seed(&conn, "docker compose up -d", 8, 8, now, None);
        seed(&conn, "docker compose down", 2, 2, now - 100_000, None);

        let ctx = RankContext::default();
        let weights = RankWeights::default();
        let suggestion = suggest(&conn, "docker co", &ctx, &weights).unwrap();
        assert_eq!(suggestion, Some("docker compose up -d".to_string()));
    }
}
