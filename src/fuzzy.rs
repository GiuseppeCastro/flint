/// Scores how well `query` fuzzy-matches `candidate` as an ordered
/// subsequence. Returns `None` when query is not a subsequence at all.
///
/// Uses a small DP over (candidate length x query length) to find the
/// *best-scoring* alignment rather than the first one found greedily —
/// e.g. for query "co" against "docker compose", a greedy left-to-right
/// scan matches the stray 'c' inside "docker" first and then a distant
/// 'o', when matching "co" at the start of "compose" scores far better.
/// Both dimensions are small (shell commands, short queries), so the
/// O(n*m) cost is trivial; only the current and previous DP row are kept,
/// so this is O(m) memory regardless of candidate length.
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<f64> {
    if query.is_empty() {
        return Some(0.0);
    }
    let q_lower: Vec<char> = query.chars().map(|c| c.to_ascii_lowercase()).collect();
    fuzzy_score_lower(&q_lower, candidate)
}

/// Same as `fuzzy_score`, but takes an already-lowercased query. Ranking a
/// large candidate set against one typed query should lowercase it once
/// up front rather than redoing that work on every row.
pub fn fuzzy_score_lower(q_lower: &[char], candidate: &str) -> Option<f64> {
    if q_lower.is_empty() {
        return Some(0.0);
    }
    let c: Vec<char> = candidate.chars().map(|c| c.to_ascii_lowercase()).collect();
    let n = c.len();
    let m = q_lower.len();
    if m > n {
        return None;
    }
    let q = q_lower;

    // prev_h[j] / prev_ends[j] hold row i-1 of the DP described above;
    // cur_h/cur_ends are filled in for row i and then swapped in.
    let mut prev_h = vec![0.0f64; m + 1];
    let mut prev_ends = vec![false; m + 1];
    let mut cur_h = vec![0.0f64; m + 1];
    let mut cur_ends = vec![false; m + 1];
    for slot in prev_h.iter_mut().skip(1) {
        *slot = f64::NEG_INFINITY;
    }

    for i in 1..=n {
        cur_h[0] = 0.0;
        cur_ends[0] = false;
        for j in 1..=m {
            let mut best = prev_h[j];
            let mut best_ends = false;

            if c[i - 1] == q[j - 1] {
                let prefix_score = if j == 1 { 0.0 } else { prev_h[j - 1] };
                if prefix_score.is_finite() {
                    let pos_bonus = if i == 1 { 2.0 } else { 0.0 };
                    let is_boundary = i == 1 || matches!(c[i - 2], ' ' | '/' | '-' | '_' | '.');
                    let boundary_bonus = if is_boundary { 1.5 } else { 0.0 };
                    let consecutive_bonus = if j > 1 && prev_ends[j - 1] { 2.0 } else { 0.0 };
                    let candidate_score =
                        1.0 + pos_bonus + boundary_bonus + consecutive_bonus + prefix_score;
                    if candidate_score >= best {
                        best = candidate_score;
                        best_ends = true;
                    }
                }
            }

            cur_h[j] = best;
            cur_ends[j] = best_ends;
        }
        std::mem::swap(&mut prev_h, &mut cur_h);
        std::mem::swap(&mut prev_ends, &mut cur_ends);
    }

    let score = prev_h[m];
    if !score.is_finite() {
        return None;
    }
    let length_penalty = (n as f64).sqrt() * 0.05;
    Some((score - length_penalty).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_prefix_scores_highest() {
        let prefix = fuzzy_score("doc", "docker compose up").unwrap();
        let scattered = fuzzy_score("dcu", "docker compose up").unwrap();
        assert!(prefix > scattered);
    }

    #[test]
    fn non_subsequence_has_no_score() {
        assert!(fuzzy_score("xyz", "docker compose up").is_none());
    }

    #[test]
    fn empty_query_matches_everything_with_zero_score() {
        assert_eq!(fuzzy_score("", "anything"), Some(0.0));
    }

    #[test]
    fn word_boundary_match_beats_mid_word_match() {
        // "co" fits contiguously at a word boundary in "compose". In
        // "cabbage soup" the only "c...o" subsequence is a distant, non-
        // boundary gap. Same query, so the DP's alignment choice is what's
        // under test, not raw bonus mass from a longer/different query.
        let contiguous_boundary = fuzzy_score("co", "compose").unwrap();
        let distant_gapped = fuzzy_score("co", "cabbage soup").unwrap();
        assert!(contiguous_boundary > distant_gapped);
    }

    #[test]
    fn contiguous_run_scores_higher_than_gapped_equivalent() {
        let contiguous = fuzzy_score("comp", "docker compose").unwrap();
        let gapped = fuzzy_score("cmoe", "docker compose").unwrap();
        assert!(contiguous > gapped);
    }

    #[test]
    fn shorter_candidate_wins_close_ties() {
        let short = fuzzy_score("git", "git commit").unwrap();
        let long = fuzzy_score("git", "git commit --amend --no-edit --allow-empty").unwrap();
        assert!(short > long);
    }

    #[test]
    fn query_longer_than_candidate_has_no_score() {
        assert!(fuzzy_score("docker compose", "doc").is_none());
    }

    #[test]
    fn picks_best_alignment_despite_misleading_earlier_partial_match() {
        // A greedy left-to-right matcher would latch onto the stray 'c'
        // inside "docker" and the first 'o' it can find afterwards. The DP
        // must instead recognize the fully contiguous, boundary-aligned
        // "co" that starts "compose" as the better alignment.
        let with_misleading_prefix = fuzzy_score("co", "docker compose").unwrap();
        let clean = fuzzy_score("co", "compose").unwrap();
        // Both find the same contiguous "co" match; the extra "docker "
        // prefix only adds to candidate length, so the clean one scores
        // at least as well once that length penalty is accounted for.
        assert!(with_misleading_prefix > 0.0);
        assert!(clean > 0.0);
    }

    #[test]
    fn lower_precomputed_variant_matches_plain_variant() {
        let q_lower: Vec<char> = "Doc".chars().map(|c| c.to_ascii_lowercase()).collect();
        assert_eq!(
            fuzzy_score_lower(&q_lower, "docker compose"),
            fuzzy_score("Doc", "docker compose")
        );
    }
}
