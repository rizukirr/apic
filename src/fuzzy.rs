//! Lightweight, dependency-free fuzzy matching.
//!
//! Matching is case-insensitive and based on ordered-subsequence search: every
//! character of the query must appear in the candidate in order, though not
//! necessarily contiguously. Matches earn a score so callers can rank results.

use std::cmp::Reverse;

/// Scores how well `candidate` matches `query` using ordered-subsequence matching.
///
/// Both inputs are lowercased before comparison, so matching is case-insensitive.
/// The query characters must occur in `candidate` in order; gaps between them are
/// allowed. Each matched character contributes a base score, with bonuses that
/// reward higher-quality matches and a penalty that discourages overly long
/// candidates:
///
/// * `+10` for every matched character.
/// * `+15` when a match is adjacent to the previous match (a run).
/// * `+20` when the match is the first character of the candidate.
/// * `+20` when the match follows a word/path boundary (`/`, `-`, `_`, space).
/// * `-len/2` once fully matched, so shorter candidates rank higher.
///
/// # Returns
///
/// `Some(score)` if every query character was matched in order (an empty query
/// matches everything with a score of `0`); `None` if the query is not a
/// subsequence of `candidate`.
fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let query = query.to_lowercase();
    let candidate_lower = candidate.to_lowercase();

    let query_chars: Vec<char> = query.chars().collect();

    let mut query_index = 0;
    let mut score = 0;

    let mut last_match_index: Option<usize> = None;
    let mut prev_char: Option<char> = None;

    for (candidate_index, c) in candidate_lower.chars().enumerate() {
        if query_index >= query_chars.len() {
            break;
        }

        if c == query_chars[query_index] {
            score += 10;

            // Bonus: consecutive match
            if let Some(last_index) = last_match_index
                && candidate_index == last_index + 1
            {
                score += 15;
            }

            // Bonus: match near beginning
            if candidate_index == 0 {
                score += 20;
            }

            // Bonus: word/path boundary
            if let Some(prev) = prev_char
                && (prev == '/' || prev == '-' || prev == '_' || prev == ' ')
            {
                score += 20;
            }

            last_match_index = Some(candidate_index);
            query_index += 1;
        }

        prev_char = Some(c);
    }

    if query_index == query_chars.len() {
        // Penalty: longer candidate is weaker
        score -= candidate.len() as i32 / 2;
        Some(score)
    } else {
        None
    }
}

/// Finds and ranks the items that fuzzy-match `query`.
///
/// Each entry in `items` is scored with [`fuzzy_score`]; non-matching items are
/// dropped. The surviving `(item, score)` pairs are returned sorted by score in
/// descending order, so the best match is first.
///
/// # Returns
///
/// `Some(results)` with at least one match, or `None` if nothing matched.
///
/// # Examples
///
/// ```ignore
/// let items = vec!["src/main.rs".to_string(), "Cargo.toml".to_string()];
/// let hits = fuzzy_find("main", &items).unwrap();
/// assert_eq!(hits[0].0, "src/main.rs");
/// ```
pub fn fuzzy_find<'a>(query: &str, items: &'a [String]) -> Option<Vec<(&'a String, i32)>> {
    let mut results: Vec<(&String, i32)> = items
        .iter()
        .filter_map(|item| fuzzy_score(query, item).map(|score| (item, score)))
        .collect();

    if results.is_empty() {
        return None;
    }

    results.sort_by_key(|item| Reverse(item.1));

    Some(results)
}
