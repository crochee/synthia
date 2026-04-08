//! Context management types

use chrono::{DateTime, Utc};
use rmcp::model::SamplingMessage;

pub(super) const HARD_MIN_CONTEXT_TOKENS: usize = 16_000;
pub(super) const WARN_CONTEXT_TOKENS: usize = 32_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextSafetyLevel {
    Safe,
    Warning,
    Critical,
}

impl ContextSafetyLevel {
    pub fn from_tokens(available_tokens: usize) -> Self {
        if available_tokens < HARD_MIN_CONTEXT_TOKENS {
            ContextSafetyLevel::Critical
        } else if available_tokens < WARN_CONTEXT_TOKENS {
            ContextSafetyLevel::Warning
        } else {
            ContextSafetyLevel::Safe
        }
    }
}

/// Compaction strategy used
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactionStrategy {
    /// No compaction needed
    None,
    /// Micro compact: replace old tool results with placeholders
    MicroCompact,
    /// Soft pruning of tool results
    SoftPruning,
    /// Hard clearing of non-critical tools
    HardClearing,
    /// LLM summarization
    Summarization,
}

impl std::fmt::Display for CompactionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompactionStrategy::None => write!(f, "none"),
            CompactionStrategy::MicroCompact => write!(f, "micro_compact"),
            CompactionStrategy::SoftPruning => write!(f, "soft_pruning"),
            CompactionStrategy::HardClearing => write!(f, "hard_clearing"),
            CompactionStrategy::Summarization => write!(f, "summarization"),
        }
    }
}

/// Reason for compaction triggering
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum CompactionReason {
    /// Compaction triggered by token threshold
    TokenThreshold,
    /// Compaction triggered by mid-turn pressure
    MidTurnPressure,
    /// Compaction triggered by emergency truncation
    Emergency,
    /// Compaction triggered by manual request
    Manual,
}

impl std::fmt::Display for CompactionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompactionReason::TokenThreshold => write!(f, "token_threshold"),
            CompactionReason::MidTurnPressure => write!(f, "mid_turn_pressure"),
            CompactionReason::Emergency => write!(f, "emergency"),
            CompactionReason::Manual => write!(f, "manual"),
        }
    }
}

/// Metadata for a compaction operation
#[derive(Clone, Debug)]
pub struct CompactionMetadata {
    /// Original message count
    pub original_count: usize,
    /// Compacted message count
    pub compacted_count: usize,
    /// Tokens saved by compaction
    pub tokens_saved: usize,
    /// Strategy used for compaction
    pub strategy: CompactionStrategy,
    /// Compaction timestamp
    pub compacted_at: DateTime<Utc>,
    /// Usage ratio before compaction
    pub usage_ratio_before: f64,
    /// Usage ratio after compaction
    pub usage_ratio_after: f64,
}

impl CompactionMetadata {
    /// Create new compaction metadata
    pub fn new(
        original_count: usize,
        compacted_count: usize,
        tokens_saved: usize,
        strategy: CompactionStrategy,
        usage_ratio_before: f64,
        usage_ratio_after: f64,
    ) -> Self {
        Self {
            original_count,
            compacted_count,
            tokens_saved,
            strategy,
            compacted_at: Utc::now(),
            usage_ratio_before,
            usage_ratio_after,
        }
    }
}

/// Result of a compaction operation
#[derive(Clone, Debug)]
pub struct CompactionResult {
    /// Reason for compaction
    pub reason: String,
    /// Compacted messages
    pub messages: Vec<SamplingMessage>,
    /// Compaction metadata
    pub metadata: CompactionMetadata,
}

impl CompactionResult {
    /// Create a new compaction result
    pub fn new(
        reason: String,
        messages: Vec<SamplingMessage>,
        metadata: CompactionMetadata,
    ) -> Self {
        Self {
            reason,
            messages,
            metadata,
        }
    }
}

/// Summary quality check result
#[derive(Clone, Copy, Debug)]
pub struct SummaryQuality {
    /// Has all required sections
    pub has_required_sections: bool,
    /// Identifier integrity maintained
    pub identifier_integrity: bool,
    /// User request reflected
    pub user_request_reflected: bool,
    /// Has file paths in summary
    pub has_file_paths: bool,
    /// Has user requests preserved
    pub has_user_requests: bool,
    /// Has key decisions recorded
    pub has_key_decisions: bool,
    /// Overall quality score (0.0 - 1.0)
    pub overall_score: f64,
}

impl SummaryQuality {
    /// Create a new summary quality result
    pub fn new(
        has_required_sections: bool,
        identifier_integrity: bool,
        user_request_reflected: bool,
        has_file_paths: bool,
        has_user_requests: bool,
        has_key_decisions: bool,
    ) -> Self {
        let overall_score = calculate_overall_score(
            has_required_sections,
            identifier_integrity,
            user_request_reflected,
            has_file_paths,
            has_user_requests,
            has_key_decisions,
        );

        Self {
            has_required_sections,
            identifier_integrity,
            user_request_reflected,
            has_file_paths,
            has_user_requests,
            has_key_decisions,
            overall_score,
        }
    }
}

/// Calculate overall quality score
/// Weights:
/// - Required sections: 0.25
/// - Identifier integrity: 0.15
/// - User request reflected: 0.15
/// - File paths: 0.15
/// - User requests: 0.15
/// - Key decisions: 0.15
fn calculate_overall_score(
    has_required_sections: bool,
    identifier_integrity: bool,
    user_request_reflected: bool,
    has_file_paths: bool,
    has_user_requests: bool,
    has_key_decisions: bool,
) -> f64 {
    let mut score = 0.0;

    if has_required_sections {
        score += 0.25;
    }
    if identifier_integrity {
        score += 0.15;
    }
    if user_request_reflected {
        score += 0.15;
    }
    if has_file_paths {
        score += 0.15;
    }
    if has_user_requests {
        score += 0.15;
    }
    if has_key_decisions {
        score += 0.15;
    }

    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compaction_strategy_display() {
        assert_eq!(format!("{}", CompactionStrategy::None), "none");
        assert_eq!(
            format!("{}", CompactionStrategy::MicroCompact),
            "micro_compact"
        );
        assert_eq!(
            format!("{}", CompactionStrategy::SoftPruning),
            "soft_pruning"
        );
        assert_eq!(
            format!("{}", CompactionStrategy::HardClearing),
            "hard_clearing"
        );
        assert_eq!(
            format!("{}", CompactionStrategy::Summarization),
            "summarization"
        );
    }

    #[test]
    fn test_compaction_metadata() {
        let metadata = CompactionMetadata::new(
            100,
            10,
            5000,
            CompactionStrategy::Summarization,
            0.9,
            0.3,
        );

        assert_eq!(metadata.original_count, 100);
        assert_eq!(metadata.compacted_count, 10);
        assert_eq!(metadata.tokens_saved, 5000);
        assert_eq!(metadata.strategy, CompactionStrategy::Summarization);
        assert_eq!(metadata.usage_ratio_before, 0.9);
        assert_eq!(metadata.usage_ratio_after, 0.3);
    }

    #[test]
    fn test_summary_quality() {
        let quality = SummaryQuality::new(true, true, true, true, true, true);
        assert!(quality.has_required_sections);
        assert!(quality.identifier_integrity);
        assert!(quality.user_request_reflected);
        assert!(quality.has_file_paths);
        assert!(quality.has_user_requests);
        assert!(quality.has_key_decisions);
        assert_eq!(quality.overall_score, 1.0);

        let quality =
            SummaryQuality::new(false, false, false, false, false, false);
        assert!(!quality.has_required_sections);
        assert!(!quality.identifier_integrity);
        assert!(!quality.user_request_reflected);
        assert!(!quality.has_file_paths);
        assert!(!quality.has_user_requests);
        assert!(!quality.has_key_decisions);
        assert_eq!(quality.overall_score, 0.0);

        let quality =
            SummaryQuality::new(true, false, true, false, true, false);
        assert!(quality.has_required_sections);
        assert!(!quality.identifier_integrity);
        assert!(quality.user_request_reflected);
        assert!(!quality.has_file_paths);
        assert!(quality.has_user_requests);
        assert!(!quality.has_key_decisions);
        assert_eq!(quality.overall_score, 0.55);
    }

    // =============================================================================
    // ContextSafetyLevel tests
    // =============================================================================

    #[test]
    fn test_context_safety_level_critical() {
        // Below HARD_MIN_CONTEXT_TOKENS (16_000)
        assert_eq!(
            ContextSafetyLevel::from_tokens(0),
            ContextSafetyLevel::Critical
        );
        assert_eq!(
            ContextSafetyLevel::from_tokens(1),
            ContextSafetyLevel::Critical
        );
        assert_eq!(
            ContextSafetyLevel::from_tokens(15_999),
            ContextSafetyLevel::Critical
        );
    }

    #[test]
    fn test_context_safety_level_warning() {
        // Between HARD_MIN_CONTEXT_TOKENS (16_000) and WARN_CONTEXT_TOKENS (32_000)
        assert_eq!(
            ContextSafetyLevel::from_tokens(16_000),
            ContextSafetyLevel::Warning
        );
        assert_eq!(
            ContextSafetyLevel::from_tokens(20_000),
            ContextSafetyLevel::Warning
        );
        assert_eq!(
            ContextSafetyLevel::from_tokens(31_999),
            ContextSafetyLevel::Warning
        );
    }

    #[test]
    fn test_context_safety_level_safe() {
        // Above WARN_CONTEXT_TOKENS (32_000)
        assert_eq!(
            ContextSafetyLevel::from_tokens(32_000),
            ContextSafetyLevel::Safe
        );
        assert_eq!(
            ContextSafetyLevel::from_tokens(50_000),
            ContextSafetyLevel::Safe
        );
        assert_eq!(
            ContextSafetyLevel::from_tokens(100_000),
            ContextSafetyLevel::Safe
        );
    }

    // =============================================================================
    // CompactionResult tests
    // =============================================================================

    #[test]
    fn test_compaction_result_new() {
        let messages = vec![];
        let metadata = CompactionMetadata::new(
            100,
            50,
            1000,
            CompactionStrategy::MicroCompact,
            0.8,
            0.4,
        );
        let result = CompactionResult::new(
            "Test compaction".to_string(),
            messages.clone(),
            metadata,
        );

        assert_eq!(result.reason, "Test compaction");
        assert_eq!(result.messages, messages);
        assert_eq!(result.metadata.original_count, 100);
    }

    // =============================================================================
    // SummaryQuality score calculation tests
    // =============================================================================

    #[test]
    fn test_summary_quality_partial_scores() {
        // Only required sections (0.25)
        let quality =
            SummaryQuality::new(true, false, false, false, false, false);
        assert_eq!(quality.overall_score, 0.25);

        // Only identifier_integrity (0.15)
        let quality =
            SummaryQuality::new(false, true, false, false, false, false);
        assert_eq!(quality.overall_score, 0.15);

        // Only user_request_reflected (0.15)
        let quality =
            SummaryQuality::new(false, false, true, false, false, false);
        assert_eq!(quality.overall_score, 0.15);

        // Only has_file_paths (0.15)
        let quality =
            SummaryQuality::new(false, false, false, true, false, false);
        assert_eq!(quality.overall_score, 0.15);

        // Only has_user_requests (0.15)
        let quality =
            SummaryQuality::new(false, false, false, false, true, false);
        assert_eq!(quality.overall_score, 0.15);

        // Only has_key_decisions (0.15)
        let quality =
            SummaryQuality::new(false, false, false, false, false, true);
        assert_eq!(quality.overall_score, 0.15);

        // Required sections + identifier_integrity = 0.40
        let quality =
            SummaryQuality::new(true, true, false, false, false, false);
        assert_eq!(quality.overall_score, 0.40);

        // All except required_sections = 0.75
        let quality = SummaryQuality::new(false, true, true, true, true, true);
        assert_eq!(quality.overall_score, 0.75);
    }
}
