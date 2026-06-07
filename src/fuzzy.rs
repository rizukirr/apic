//! Lightweight, dependency-free fuzzy matching.
//!
//! Matching is case-insensitive and based on ordered-subsequence search: every
//! character of the query must appear in the candidate in order, though not
//! necessarily contiguously. Matches earn a score so callers can rank results.

use std::cmp::Reverse;

/// Scores how well `candidate` matches `query` and reports which candidate
/// characters matched, using ordered-subsequence matching.
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
/// `Some((score, indices))` if every query character was matched in order,
/// where `indices` are the char positions (not bytes) of the matched
/// characters in `candidate` (an empty query matches everything with a score
/// of `0` and no indices); `None` if the query is not a subsequence of
/// `candidate`.
pub(crate) fn fuzzy_match(query: &str, candidate: &str) -> Option<(i32, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }

    let query = query.to_lowercase();
    let candidate_lower = candidate.to_lowercase();

    let query_chars: Vec<char> = query.chars().collect();

    let mut query_index = 0;
    let mut score = 0;
    let mut matched_indices = Vec::with_capacity(query_chars.len());

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
            matched_indices.push(candidate_index);
            query_index += 1;
        }

        prev_char = Some(c);
    }

    if query_index == query_chars.len() {
        // Penalty: longer candidate is weaker
        score -= candidate.len() as i32 / 2;
        Some((score, matched_indices))
    } else {
        None
    }
}

/// Scores how well `candidate` matches `query`; see [`fuzzy_match`] for the
/// scoring rules. This is `fuzzy_match` minus the matched-index bookkeeping.
pub(crate) fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    fuzzy_match(query, candidate).map(|(score, _)| score)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(fuzzy_score("", "anything"), Some(0));
    }

    #[test]
    fn non_subsequence_does_not_match() {
        assert_eq!(fuzzy_score("xyz", "login"), None);
    }

    #[test]
    fn ordered_subsequence_matches() {
        assert!(fuzzy_score("lgn", "login").is_some());
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(fuzzy_score("LOGIN", "login").is_some());
    }

    #[test]
    fn path_boundary_scores_higher_than_mid_word() {
        // "login" right after a '/' boundary should beat an embedded match.
        let boundary = fuzzy_score("login", "auth/login.json").unwrap();
        let embedded = fuzzy_score("login", "prologinx").unwrap();
        assert!(
            boundary > embedded,
            "boundary={boundary} embedded={embedded}"
        );
    }

    #[test]
    fn find_ranks_best_match_first() {
        let items = vec![
            "src/main.rs".to_string(),
            "api/login.json".to_string(),
            "api/logout.json".to_string(),
        ];
        let hits = fuzzy_find("login", &items).unwrap();
        assert_eq!(hits[0].0, "api/login.json");
        // Scores are sorted descending.
        assert!(hits.windows(2).all(|w| w[0].1 >= w[1].1));
    }

    #[test]
    fn find_returns_none_when_nothing_matches() {
        let items = vec!["abc".to_string()];
        assert!(fuzzy_find("zzz", &items).is_none());
    }

    #[test]
    fn match_returns_indices_of_matched_chars() {
        // "lgn" in "login": l(0), g(2), n(4).
        let (_, indices) = fuzzy_match("lgn", "login").unwrap();
        assert_eq!(indices, vec![0, 2, 4]);
    }

    #[test]
    fn match_indices_are_case_insensitive() {
        let (_, indices) = fuzzy_match("LOG", "Login").unwrap();
        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn empty_query_matches_with_no_indices() {
        assert_eq!(fuzzy_match("", "anything"), Some((0, Vec::new())));
    }

    #[test]
    fn score_is_match_score() {
        assert_eq!(
            fuzzy_score("login", "auth/login.json"),
            fuzzy_match("login", "auth/login.json").map(|(s, _)| s)
        );
    }
}
