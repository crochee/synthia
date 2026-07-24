//! 4 private helper functions used by
//! [`super::search::MemoryRetriever::search_with_mode`].
//!
//! - [`compute_text_relevance`] — word-level matching with
//!   bonuses for exact matches (+1.0), term frequency
//!   (capped at +0.7 for repeated terms), partial substring
//!   matches (+0.4), and a coverage bonus (0.5 + 0.5 *
//!   `matched_terms / query_terms`). Returns a score
//!   clamped to [0, 1].
//! - [`apply_recency_weight_hours`] —
//!   `relevance * (1 - recency_weight) + recency_score *
//!   recency_weight` where `recency_score = 1 / (1 +
//!   hours_since)`. Used for the semantic layer (no
//!   importance score).
//! - [`apply_importance_and_recency_weight`] — wrapper
//!   that converts a `u64` epoch delta to `f64` hours and
//!   forwards to
//!   [`apply_importance_and_recency_weight_hours`].
//!   Used for the hot layer (no timestamp).
//! - [`apply_importance_and_recency_weight_hours`] —
//!   `relevance * available_weight + importance_score *
//!   importance_weight + recency_score * recency_weight`
//!   where `available_weight = 1 - importance_weight -
//!   recency_weight`. Used for cold and episodic layers.

/// Compute a simple text relevance score between a query and content.
///
/// Uses word-level matching with bonuses for:
/// - Exact word matches
/// - Term frequency
/// - Content coverage (fraction of query terms matched)
pub(super) fn compute_text_relevance(query: &str, content: &str) -> f64 {
    let query_terms: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .filter(|t| t.len() > 1)
        .map(|s| s.to_string())
        .collect();

    if query_terms.is_empty() {
        return 0.0;
    }

    let content_lower = content.to_lowercase();
    let content_words: std::collections::HashSet<&str> =
        content_lower.split_whitespace().collect();

    let mut score = 0.0;
    let mut matched_terms = 0;

    for term in &query_terms {
        // Check for exact word match
        if content_words.contains(term.as_str()) {
            score += 1.0;
            matched_terms += 1;
            // Bonus for term frequency
            let freq = content_lower.matches(term.as_str()).count();
            if freq > 1 {
                score += ((freq - 1) as f64 * 0.3).min(0.7);
            }
        } else if content_lower.contains(term.as_str()) {
            // Partial match bonus
            score += 0.4;
            matched_terms += 1;
        }
    }

    // Coverage bonus: fraction of query terms that matched
    let coverage = matched_terms as f64 / query_terms.len() as f64;
    score *= 0.5 + 0.5 * coverage;

    // Normalize to 0-1 range
    score.min(1.0)
}

/// Apply recency weighting using hours since the entry was last updated.
///
/// final_score = relevance * (1.0 - recency_weight) + recency_score * recency_weight
/// where recency_score = 1.0 / (1.0 + hours_since_update)
pub(super) fn apply_recency_weight_hours(
    relevance: f64,
    hours_since: f64,
    recency_weight: f64,
) -> f64 {
    let recency_score = 1.0 / (1.0 + hours_since);
    relevance * (1.0 - recency_weight) + recency_score * recency_weight
}

/// Apply both importance and recency weighting to the relevance score.
pub(super) fn apply_importance_and_recency_weight(
    relevance: f64,
    importance_score: f64,
    _hours_since: u64,
    _now: chrono::DateTime<chrono::Utc>,
    importance_weight: f64,
    recency_weight: f64,
) -> f64 {
    apply_importance_and_recency_weight_hours(
        relevance,
        importance_score,
        0.0,
        importance_weight,
        recency_weight,
    )
}

/// Apply both importance and recency weighting using hours since the entry was last updated.
pub(super) fn apply_importance_and_recency_weight_hours(
    relevance: f64,
    importance_score: f64,
    hours_since: f64,
    importance_weight: f64,
    recency_weight: f64,
) -> f64 {
    let recency_score = 1.0 / (1.0 + hours_since);
    let available_weight = 1.0 - importance_weight - recency_weight;

    (relevance * available_weight)
        + (importance_score * importance_weight)
        + (recency_score * recency_weight)
}
