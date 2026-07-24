//! Semantic retrieval: keyword matching with weighted scoring.
//!
//! Scoring algorithm:
//! - Exact word match (whole-token): +1.0
//! - Partial substring match: +0.5
//! - Frequency bonus: +0.2 per extra occurrence (after the first)
//! - Normalized to [0, 1] by dividing by query term count.

use crate::types::{ColdEntry, SearchResult};

/// Semantic retrieval placeholder: keyword matching with weighted scoring.
/// This simulates semantic relevance using:
/// - Exact keyword matches (weight: 1.0)
/// - Partial keyword matches (weight: 0.5)
/// - Term frequency weighting
pub fn semantic_search(
    entries: &[ColdEntry],
    query: &str,
    limit: usize,
) -> Vec<SearchResult> {
    let query_terms: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    if query_terms.is_empty() {
        return Vec::new();
    }

    let mut results: Vec<SearchResult> = entries
        .iter()
        .map(|entry| {
            let content_lower = entry.content.to_lowercase();
            let mut score = 0.0;

            for term in &query_terms {
                if content_lower.contains(term.as_str()) {
                    let is_exact_word = content_lower
                        .split_whitespace()
                        .any(|word| word == term.as_str());

                    if is_exact_word {
                        score += 1.0;
                    } else {
                        score += 0.5;
                    }

                    let term_count =
                        content_lower.matches(term.as_str()).count();
                    if term_count > 1 {
                        score += (term_count - 1) as f64 * 0.2;
                    }
                }
            }

            let normalized = if !query_terms.is_empty() {
                (score / query_terms.len() as f64).min(1.0)
            } else {
                0.0
            };

            SearchResult {
                entry: entry.clone(),
                score: normalized,
            }
        })
        .collect();

    // Sort by score descending
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Keep only results with score > 0
    results.retain(|r| r.score > 0.0);

    results.truncate(limit);
    results
}
