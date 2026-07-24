use crate::memory_retriever::MemorySearchResult;

const MAX_ENTRY_CHARS: usize = 100;

/// Produces concise text summaries of memory search results.
pub struct MemorySummarizer;

impl MemorySummarizer {
    /// Summarize a list of search results into a compact text format.
    ///
    /// - Groups entries by source type ("hot", "cold", "episodic")
    /// - Shows most recent entries first within each group
    /// - Truncates long entries to ~100 characters
    /// - Returns empty string for empty input
    pub fn summarize(entries: &[MemorySearchResult]) -> String {
        if entries.is_empty() {
            return String::new();
        }

        // Group by source type
        let mut groups: std::collections::HashMap<
            &str,
            Vec<&MemorySearchResult>,
        > = std::collections::HashMap::new();
        for entry in entries {
            groups.entry(entry.source).or_default().push(entry);
        }

        let mut output = String::new();

        // Process groups in a consistent order: hot first, then cold, episodic, then others
        let source_order = ["hot", "cold", "episodic"];

        for &source in &source_order {
            if let Some(mut group) = groups.remove(source) {
                // Sort by timestamp descending (most recent first)
                group.sort_by(|a, b| {
                    b.timestamp.cmp(&a.timestamp).then(
                        b.relevance
                            .partial_cmp(&a.relevance)
                            .unwrap_or(std::cmp::Ordering::Equal),
                    )
                });

                if !output.is_empty() {
                    output.push_str("\n\n");
                }

                output.push_str(&format!(
                    "=== {} memory ===\n",
                    source.to_uppercase()
                ));

                for entry in &group {
                    let truncated =
                        truncate_content(&entry.content, MAX_ENTRY_CHARS);
                    output.push_str(&format!(
                        "- {} [score: {:.2}]\n",
                        truncated, entry.relevance
                    ));
                }
            }
        }

        // Handle any remaining sources not in the predefined order
        for (source, mut group) in groups {
            group.sort_by(|a, b| {
                b.timestamp.cmp(&a.timestamp).then(
                    b.relevance
                        .partial_cmp(&a.relevance)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            });

            if !output.is_empty() {
                output.push_str("\n\n");
            }

            output.push_str(&format!(
                "=== {} memory ===\n",
                source.to_uppercase()
            ));

            for entry in &group {
                let truncated =
                    truncate_content(&entry.content, MAX_ENTRY_CHARS);
                output.push_str(&format!(
                    "- {} [score: {:.2}]\n",
                    truncated, entry.relevance
                ));
            }
        }

        output
    }
}

/// Truncate content to approximately the specified character count,
/// adding "..." if the content was truncated.
fn truncate_content(content: &str, max_chars: usize) -> String {
    let trimmed = content.trim();
    if trimmed.len() <= max_chars {
        return trimmed.to_string();
    }

    // Find a good break point at a word boundary
    let truncate_point = trimmed
        .char_indices()
        .take(max_chars.saturating_sub(3))
        .last()
        .map(|(idx, _)| idx)
        .unwrap_or(max_chars.saturating_sub(3));

    // Try to end at a word boundary
    let final_point = trimmed[truncate_point..]
        .find(' ')
        .map(|space_idx| truncate_point + space_idx)
        .unwrap_or(truncate_point);

    format!("{}...", &trimmed[..final_point])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(
        content: &str,
        source: &'static str,
        relevance: f64,
        timestamp: u64,
    ) -> MemorySearchResult {
        MemorySearchResult {
            content: content.to_string(),
            source,
            relevance,
            timestamp,
        }
    }

    #[test]
    fn test_summarize_empty() {
        let result = MemorySummarizer::summarize(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_summarize_single_entry() {
        let entries = vec![make_entry("short content", "hot", 0.8, 1000)];
        let result = MemorySummarizer::summarize(&entries);
        assert!(result.contains("=== HOT memory ==="));
        assert!(result.contains("short content"));
        assert!(result.contains("0.80"));
    }

    #[test]
    fn test_summarize_groups_by_source() {
        let entries = vec![
            make_entry("hot content", "hot", 0.9, 1000),
            make_entry("cold content", "cold", 0.7, 1001),
            make_entry("another hot", "hot", 0.8, 1002),
        ];
        let result = MemorySummarizer::summarize(&entries);

        assert!(result.contains("=== HOT memory ==="));
        assert!(result.contains("=== COLD memory ==="));

        // Hot section should have 2 entries
        let hot_section =
            result.split("=== COLD memory ===").next().unwrap_or("");
        assert!(hot_section.contains("hot content"));
        assert!(hot_section.contains("another hot"));
    }

    #[test]
    fn test_summarize_sorts_by_timestamp() {
        let entries = vec![
            make_entry("old entry", "hot", 0.9, 500),
            make_entry("new entry", "hot", 0.8, 1000),
            make_entry("middle entry", "hot", 0.85, 750),
        ];
        let result = MemorySummarizer::summarize(&entries);

        // Find positions of entries in output
        let new_pos = result.find("new entry").unwrap_or(0);
        let middle_pos = result.find("middle entry").unwrap_or(0);
        let old_pos = result.find("old entry").unwrap_or(0);

        assert!(
            new_pos < middle_pos && middle_pos < old_pos,
            "Entries should be sorted newest first: new({}) < middle({}) < old({})",
            new_pos,
            middle_pos,
            old_pos
        );
    }

    #[test]
    fn test_summarize_truncates_long_entries() {
        let long_content = "This is a very long piece of content that should definitely be truncated because it exceeds the maximum character limit for display purposes in the summary";
        let entries = vec![make_entry(long_content, "hot", 0.7, 1000)];
        let result = MemorySummarizer::summarize(&entries);

        assert!(
            result.contains("..."),
            "Long entries should be truncated with ellipsis"
        );
        assert!(
            !result.contains(&long_content[101..]),
            "Truncated content should not include full long string"
        );
    }

    #[test]
    fn test_summarize_preserves_short_entries() {
        let short_content = "Short entry";
        let entries = vec![make_entry(short_content, "hot", 0.7, 1000)];
        let result = MemorySummarizer::summarize(&entries);

        assert!(result.contains("Short entry"));
        assert!(
            !result.contains("..."),
            "Short entries should not be truncated"
        );
    }

    #[test]
    fn test_summarize_multiple_sources_order() {
        let entries = vec![
            make_entry("episodic data", "episodic", 0.6, 1000),
            make_entry("hot data", "hot", 0.9, 1001),
            make_entry("cold data", "cold", 0.8, 1002),
        ];
        let result = MemorySummarizer::summarize(&entries);

        let hot_pos = result.find("=== HOT memory ===").unwrap_or(usize::MAX);
        let cold_pos = result.find("=== COLD memory ===").unwrap_or(usize::MAX);
        let episodic_pos =
            result.find("=== EPISODIC memory ===").unwrap_or(usize::MAX);

        assert!(
            hot_pos < cold_pos,
            "Hot should come before cold: hot({}) < cold({})",
            hot_pos,
            cold_pos
        );
        assert!(
            cold_pos < episodic_pos,
            "Cold should come before episodic: cold({}) < episodic({})",
            cold_pos,
            episodic_pos
        );
    }

    #[test]
    fn test_summarize_includes_scores() {
        let entries = vec![make_entry("test", "hot", 0.75, 1000)];
        let result = MemorySummarizer::summarize(&entries);

        assert!(
            result.contains("0.75"),
            "Summary should include relevance scores"
        );
    }

    #[test]
    fn test_truncate_content_short() {
        let result = truncate_content("hello world", 100);
        assert_eq!(result, "hello world");
        assert!(!result.ends_with("..."));
    }

    #[test]
    fn test_truncate_content_long() {
        let long = "a".repeat(200);
        let result = truncate_content(&long, 100);
        assert!(result.len() <= 103); // 100 chars + "..."
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_content_at_word_boundary() {
        let content =
            "word1 word2 word3 word4 word5 word6 word7 word8 word9 word10";
        let result = truncate_content(content, 30);
        assert!(result.ends_with("..."));
        // Should end at a word boundary
        assert!(!result.ends_with(" ..."));
    }
}
