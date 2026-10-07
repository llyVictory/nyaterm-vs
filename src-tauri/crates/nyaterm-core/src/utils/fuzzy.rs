//! Fuzzy search using nucleo-matcher.

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use serde::{Deserialize, Serialize};

/// Single fuzzy match with score and highlighted character indices.
#[derive(Debug, Clone, Serialize)]
pub struct FuzzyResult {
    pub command: String,
    pub score: u32,
    pub indices: Vec<u32>,
    /// Provider tag: "history", "quickCommand", etc.
    pub source: String,
    /// Text shown in the suggestion panel (may differ from `command`).
    pub display: String,
}

/// Candidate passed from the frontend for stateless fuzzy scoring.
#[derive(Debug, Clone, Deserialize)]
pub struct FuzzySearchCandidate {
    pub id: String,
    pub value: String,
    pub display: String,
}

/// Generic frontend candidate match with stable id and value.
#[derive(Debug, Clone, Serialize)]
pub struct FuzzyCandidateResult {
    pub id: String,
    pub value: String,
    pub display: String,
    pub score: u32,
    pub indices: Vec<u32>,
}

/// Generic fuzzy search over `(display_text, value)` pairs.
///
/// Matches against `display_text` (shown in the suggestion panel).
/// `value` is stored as `command` in the result (used when filling/executing).
/// `source` tags every result so the frontend can distinguish providers.
pub fn fuzzy_search_items(
    items: &[(&str, &str)],
    pattern_str: &str,
    source: &str,
    limit: usize,
    min_command_length: Option<usize>,
    max_command_length: Option<usize>,
) -> Vec<FuzzyResult> {
    let pattern_str = pattern_str.trim();
    if pattern_str.is_empty() {
        return Vec::new();
    }

    let pattern = Pattern::new(
        pattern_str,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );

    if pattern.atoms.is_empty() {
        return Vec::new();
    }

    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let mut buf = Vec::new();

    let mut scored: Vec<(usize, u32)> = Vec::new();
    for (idx, (display, value)) in items.iter().enumerate() {
        let value_length = value.chars().count();

        if min_command_length.is_some_and(|min| value_length < min) {
            continue;
        }

        if max_command_length.is_some_and(|max| value_length > max) {
            continue;
        }

        let haystack = Utf32Str::new(display, &mut buf);
        if let Some(score) = pattern.score(haystack, &mut matcher) {
            scored.push((idx, score));
        }
    }

    scored.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
    scored.truncate(limit);

    let mut results = Vec::with_capacity(scored.len());
    for (idx, score) in scored {
        let (display, value) = &items[idx];
        let haystack = Utf32Str::new(display, &mut buf);
        let mut indices = Vec::new();
        pattern.indices(haystack, &mut matcher, &mut indices);
        indices.sort_unstable();
        indices.dedup();

        results.push(FuzzyResult {
            command: (*value).to_string(),
            score,
            indices,
            source: source.to_string(),
            display: (*display).to_string(),
        });
    }

    results
}

/// Generic fuzzy search over frontend-provided candidates.
///
/// This keeps business state on the frontend while sharing the Rust fuzzy scorer.
pub fn fuzzy_search_candidates(
    items: &[FuzzySearchCandidate],
    pattern_str: &str,
    limit: usize,
) -> Vec<FuzzyCandidateResult> {
    let pattern_str = pattern_str.trim();
    if pattern_str.is_empty() {
        return Vec::new();
    }

    let pattern = Pattern::new(
        pattern_str,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );

    if pattern.atoms.is_empty() {
        return Vec::new();
    }

    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let mut buf = Vec::new();

    let mut scored: Vec<(usize, u32)> = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        let haystack = Utf32Str::new(&item.display, &mut buf);
        if let Some(score) = pattern.score(haystack, &mut matcher) {
            scored.push((idx, score));
        }
    }

    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.truncate(limit);

    let mut results = Vec::with_capacity(scored.len());
    for (idx, score) in scored {
        let item = &items[idx];
        let haystack = Utf32Str::new(&item.display, &mut buf);
        let mut indices = Vec::new();
        pattern.indices(haystack, &mut matcher, &mut indices);
        indices.sort_unstable();
        indices.dedup();

        results.push(FuzzyCandidateResult {
            id: item.id.clone(),
            value: item.value.clone(),
            display: item.display.clone(),
            score,
            indices,
        });
    }

    results
}

#[cfg(test)]
mod tests {
    use super::{FuzzySearchCandidate, fuzzy_search_candidates};

    fn candidate(id: &str, display: &str) -> FuzzySearchCandidate {
        FuzzySearchCandidate {
            id: id.to_string(),
            value: id.to_string(),
            display: display.to_string(),
        }
    }

    #[test]
    fn fuzzy_candidates_return_empty_for_blank_pattern() {
        let items = [candidate("session:1", "production ssh")];

        let results = fuzzy_search_candidates(&items, "  ", 10);

        assert!(results.is_empty());
    }

    #[test]
    fn fuzzy_candidates_respect_limit_and_return_stable_ids() {
        let items = [
            candidate("session:1", "production ssh root@10.0.0.1"),
            candidate("connection:1", "production database postgres"),
            candidate("session:2", "local shell"),
        ];

        let results = fuzzy_search_candidates(&items, "prod", 1);

        assert_eq!(results.len(), 1);
        assert!(matches!(
            results[0].id.as_str(),
            "session:1" | "connection:1"
        ));
        assert_eq!(results[0].value, results[0].id);
    }

    #[test]
    fn fuzzy_candidates_sort_by_score_then_original_order() {
        let items = [
            candidate("first", "docker image"),
            candidate("second", "docker image"),
            candidate("third", "documentation index"),
        ];

        let results = fuzzy_search_candidates(&items, "docker image", 10);

        assert!(results.len() >= 2);
        assert_eq!(results[0].id, "first");
        assert_eq!(results[1].id, "second");
        assert!(results[0].score >= results[1].score);
    }
}
