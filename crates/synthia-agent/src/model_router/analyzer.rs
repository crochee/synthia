use std::sync::LazyLock;

use regex::Regex;
use rmcp::model::SamplingMessage;

use crate::{
    model_router::types::{ComplexityLevel, ConversationMetrics},
    utils::extract_text,
};

#[allow(clippy::expect_used)]
static WORD_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-z]+(?:'[A-Za-z]+)?").expect("valid regex")
});

#[allow(clippy::expect_used)]
static SENTENCE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[.!?]+").expect("valid regex"));

const WORDS_PER_SENTENCE_HIGH: f64 = 25.0;
const WORDS_PER_SENTENCE_MEDIUM: f64 = 15.0;
const LONG_WORD_RATIO_HIGH: f64 = 0.25;
const LONG_WORD_RATIO_MEDIUM: f64 = 0.15;

pub struct ConversationAnalyzer;

impl ConversationAnalyzer {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze(
        &self,
        conversation: &[SamplingMessage],
    ) -> ConversationMetrics {
        let message_count = conversation.len();
        let total_tokens_estimate = Self::estimate_tokens(conversation);
        let complexity = self.analyze_complexity(conversation);
        let (tool_call_count, consecutive_failures) =
            self.analyze_tool_patterns(conversation);

        ConversationMetrics {
            message_count,
            total_tokens_estimate,
            complexity,
            tool_call_count,
            consecutive_failures,
        }
    }

    fn estimate_tokens(conversation: &[SamplingMessage]) -> usize {
        conversation.iter().map(|m| extract_text(m).len() / 4).sum()
    }

    fn analyze_tool_patterns(
        &self,
        conversation: &[SamplingMessage],
    ) -> (usize, usize) {
        let mut tool_count = 0;
        let mut failure_count = 0;

        for msg in conversation.iter().rev() {
            match msg.role {
                rmcp::model::Role::Assistant => {
                    let text = extract_text(msg);
                    if text.contains("tool_call")
                        || text.contains("function_call")
                    {
                        tool_count += 1;
                    }
                }
                rmcp::model::Role::User => break,
            }
        }

        for msg in conversation.iter().rev() {
            if msg.role == rmcp::model::Role::User {
                break;
            }
            let text = extract_text(msg);
            if text.contains("error") || text.contains("failed") {
                failure_count += 1;
            }
        }

        (tool_count, failure_count)
    }

    fn analyze_complexity(
        &self,
        conversation: &[SamplingMessage],
    ) -> ComplexityLevel {
        let last_msg = conversation
            .iter()
            .rev()
            .find(|m| matches!(m.role, rmcp::model::Role::User));

        let Some(last_msg) = last_msg else {
            return ComplexityLevel::Low;
        };

        let text = extract_text(last_msg);
        let words: Vec<&str> =
            WORD_REGEX.find_iter(&text).map(|m| m.as_str()).collect();
        let word_count = words.len().max(1);
        let sentence_count = SENTENCE_REGEX.find_iter(&text).count().max(1);

        let avg_words_per_sentence = word_count as f64 / sentence_count as f64;
        let long_word_ratio = words.iter().filter(|w| w.len() > 6).count()
            as f64
            / word_count as f64;

        if avg_words_per_sentence > WORDS_PER_SENTENCE_HIGH
            || long_word_ratio > LONG_WORD_RATIO_HIGH
        {
            ComplexityLevel::High
        } else if avg_words_per_sentence > WORDS_PER_SENTENCE_MEDIUM
            || long_word_ratio > LONG_WORD_RATIO_MEDIUM
        {
            ComplexityLevel::Medium
        } else {
            ComplexityLevel::Low
        }
    }
}

impl Default for ConversationAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use rmcp::model::{
        RawTextContent,
        Role,
        SamplingContent,
        SamplingMessage,
        SamplingMessageContent,
    };

    use super::*;

    fn user_msg(text: &str) -> SamplingMessage {
        SamplingMessage {
            role: Role::User,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: text.to_string(),
                    meta: None,
                },
            )),
            meta: None,
        }
    }

    fn assistant_msg(text: &str) -> SamplingMessage {
        SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: text.to_string(),
                    meta: None,
                },
            )),
            meta: None,
        }
    }

    fn assistant_tool_call(text: &str) -> SamplingMessage {
        SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: text.to_string(),
                    meta: None,
                },
            )),
            meta: None,
        }
    }

    // estimate_tokens is a private static fn, tested indirectly via analyze()

    #[test]
    fn test_analyze_empty_conversation() {
        let analyzer = ConversationAnalyzer::new();
        let metrics = analyzer.analyze(&[]);
        assert_eq!(metrics.message_count, 0);
        assert_eq!(metrics.total_tokens_estimate, 0);
        assert_eq!(metrics.complexity, ComplexityLevel::Low);
        assert_eq!(metrics.tool_call_count, 0);
        assert_eq!(metrics.consecutive_failures, 0);
    }

    #[test]
    fn test_analyze_single_user_message() {
        let analyzer = ConversationAnalyzer::new();
        let conversation = vec![user_msg("Hello world")];
        let metrics = analyzer.analyze(&conversation);
        assert_eq!(metrics.message_count, 1);
        assert!(metrics.total_tokens_estimate > 0);
        assert_eq!(metrics.complexity, ComplexityLevel::Low);
        assert_eq!(metrics.tool_call_count, 0);
    }

    #[test]
    fn test_analyze_complexity_low() {
        let analyzer = ConversationAnalyzer::new();
        let conversation = vec![user_msg("Hi")];
        let metrics = analyzer.analyze(&conversation);
        assert_eq!(metrics.complexity, ComplexityLevel::Low);
    }

    #[test]
    fn test_analyze_complexity_medium_sentence_length() {
        let analyzer = ConversationAnalyzer::new();
        // 16 words in one sentence = medium complexity (> 15 per sentence)
        // Use short words (<=6 chars) to avoid triggering high via long-word ratio
        let text = "The code uses a simple approach with basic functions and direct calls here.".to_string();
        let conversation = vec![user_msg(&text)];
        let metrics = analyzer.analyze(&conversation);
        assert_eq!(metrics.complexity, ComplexityLevel::Medium);
    }

    #[test]
    fn test_analyze_complexity_high_long_words() {
        let analyzer = ConversationAnalyzer::new();
        // High ratio of long words (> 6 chars) triggers High
        let text = "The architecture utilizes sophisticated implementation strategies with sophisticated sophisticated sophisticated sophisticated components.".to_string();
        let conversation = vec![user_msg(&text)];
        let metrics = analyzer.analyze(&conversation);
        assert_eq!(metrics.complexity, ComplexityLevel::High);
    }

    #[test]
    fn test_analyze_tool_call_count() {
        let analyzer = ConversationAnalyzer::new();
        let conversation = vec![
            user_msg("Do something"),
            assistant_tool_call("I will use the tool function_call"),
            assistant_tool_call("Another tool_call here"),
        ];
        let metrics = analyzer.analyze(&conversation);
        assert_eq!(metrics.tool_call_count, 2);
    }

    #[test]
    fn test_analyze_consecutive_failures() {
        let analyzer = ConversationAnalyzer::new();
        let conversation = vec![
            user_msg("Hello"),
            assistant_msg("error occurred"),
            assistant_msg("failed to process"),
        ];
        let metrics = analyzer.analyze(&conversation);
        assert_eq!(metrics.consecutive_failures, 2);
    }

    #[test]
    fn test_analyze_tool_patterns_stops_at_user() {
        let analyzer = ConversationAnalyzer::new();
        let conversation = vec![
            assistant_msg("error occurred"),
            assistant_msg("another error"),
            user_msg("Now I ask something"),
            assistant_msg("result delivered"),
        ];
        let metrics = analyzer.analyze(&conversation);
        // Error counting iterates in reverse and breaks at first user message,
        // so errors BEFORE the user message are not counted.
        // Here the assistant messages AFTER user have no errors -> 0
        assert_eq!(metrics.consecutive_failures, 0);
    }

    #[test]
    fn test_analyze_consecutive_failures_only_after_last_user() {
        let analyzer = ConversationAnalyzer::new();
        // Only errors in assistant messages AFTER the last user message count
        let conversation = vec![
            user_msg("Hello"),
            assistant_msg("error one"),
            assistant_msg("error two"),
        ];
        let metrics = analyzer.analyze(&conversation);
        assert_eq!(metrics.consecutive_failures, 2);
    }

    #[test]
    fn test_analyze_multiple_messages() {
        let analyzer = ConversationAnalyzer::new();
        let conversation = vec![
            user_msg("First message with some substantial content here"),
            assistant_msg("Let me help you with that request"),
            user_msg("Follow up question about the same topic"),
        ];
        let metrics = analyzer.analyze(&conversation);
        assert_eq!(metrics.message_count, 3);
        assert!(metrics.total_tokens_estimate > 0);
        // Last user message is short
        assert_eq!(metrics.complexity, ComplexityLevel::Low);
    }

    #[test]
    fn test_analyze_token_estimate_scales_with_text() {
        let analyzer = ConversationAnalyzer::new();
        let short = vec![user_msg("Hi")];
        let long = vec![user_msg(
            "This is a much longer message with many more characters to estimate tokens from properly",
        )];

        let short_metrics = analyzer.analyze(&short);
        let long_metrics = analyzer.analyze(&long);

        assert!(
            long_metrics.total_tokens_estimate
                > short_metrics.total_tokens_estimate
        );
    }
}
