//! The [`MemorySearchResult`] struct returned by every
//! search variant.

/// A single memory search result with relevance scoring and source attribution.
#[derive(Debug, Clone)]
pub struct MemorySearchResult {
    /// The matched content text.
    pub content: String,
    /// Source type: "hot", "cold", "episodic", or "context".
    pub source: &'static str,
    /// Combined relevance score after recency weighting (0.0 - 1.0).
    pub relevance: f64,
    /// Timestamp as epoch seconds when the entry was created/updated.
    pub timestamp: u64,
}
