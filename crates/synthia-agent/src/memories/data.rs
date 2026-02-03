use std::str::FromStr;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum MemoryType {
    #[default]
    ConversationSummary,
    UserPreference,
    ToolUsagePattern,
    KeyInsight,
    TaskResult,
    ErrorRecord,
    UserFeedback,
}

impl MemoryType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ConversationSummary => "conversation_summary",
            Self::UserPreference => "user_preference",
            Self::ToolUsagePattern => "tool_usage_pattern",
            Self::KeyInsight => "key_insight",
            Self::TaskResult => "task_result",
            Self::ErrorRecord => "error_record",
            Self::UserFeedback => "user_feedback",
        }
    }

    pub fn parse(s: &str) -> Self {
        Self::from_str(s).unwrap_or_default()
    }
}

impl FromStr for MemoryType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "conversation_summary" => Ok(Self::ConversationSummary),
            "user_preference" => Ok(Self::UserPreference),
            "tool_usage_pattern" => Ok(Self::ToolUsagePattern),
            "key_insight" => Ok(Self::KeyInsight),
            "task_result" => Ok(Self::TaskResult),
            "error_record" => Ok(Self::ErrorRecord),
            "user_feedback" => Ok(Self::UserFeedback),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum MemoryImportance {
    Low,
    #[default]
    Medium,
    High,
    Critical,
}

impl MemoryImportance {
    pub fn score(&self) -> f32 {
        match self {
            Self::Low => 0.25,
            Self::Medium => 0.5,
            Self::High => 0.75,
            Self::Critical => 1.0,
        }
    }

    pub fn from_score(score: f32) -> Self {
        match score {
            s if s < 0.3 => Self::Low,
            s if s < 0.6 => Self::Medium,
            s if s < 0.85 => Self::High,
            _ => Self::Critical,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    pub fn parse(s: &str) -> Self {
        Self::from_str(s).unwrap_or_default()
    }
}

impl FromStr for MemoryImportance {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "critical" => Ok(Self::Critical),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Memory {
    pub id: String,
    pub session_id: String,
    pub content: String,
    pub memory_type: MemoryType,
    pub importance: MemoryImportance,
    pub tags: Vec<String>,
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub accessed_at: DateTime<Utc>,
    pub access_count: u32,
}

impl Memory {
    pub fn new(
        session_id: &str,
        content: String,
        memory_type: MemoryType,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            content,
            memory_type,
            importance: MemoryImportance::default(),
            tags: vec![],
            embedding: None,
            created_at: now,
            updated_at: now,
            accessed_at: now,
            access_count: 0,
        }
    }

    pub fn with_importance(mut self, importance: MemoryImportance) -> Self {
        self.importance = importance;
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    pub fn bump_access(&mut self) {
        self.accessed_at = Utc::now();
        self.access_count += 1;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemoryQuery {
    pub query: String,
    pub session_id: Option<String>,
    pub memory_types: Option<Vec<MemoryType>>,
    pub tags: Option<Vec<String>>,
    pub min_importance: Option<MemoryImportance>,
    pub limit: usize,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            session_id: None,
            memory_types: None,
            tags: None,
            min_importance: None,
            limit: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemoryStats {
    pub total_memories: usize,
    pub by_type: std::collections::HashMap<String, usize>,
    pub by_importance: std::collections::HashMap<String, usize>,
    pub avg_access_count: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage1Output {
    pub thread_id: String,
    pub raw_memory: String,
    pub rollout_summary: String,
    pub cwd: std::path::PathBuf,
    pub source_updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_type_snake_case() {
        assert_eq!(
            MemoryType::ConversationSummary.as_str(),
            "conversation_summary"
        );
        assert_eq!(MemoryType::UserPreference.as_str(), "user_preference");
        assert_eq!(MemoryType::ToolUsagePattern.as_str(), "tool_usage_pattern");
        assert_eq!(MemoryType::KeyInsight.as_str(), "key_insight");
        assert_eq!(MemoryType::TaskResult.as_str(), "task_result");
        assert_eq!(MemoryType::ErrorRecord.as_str(), "error_record");
        assert_eq!(MemoryType::UserFeedback.as_str(), "user_feedback");
    }

    #[test]
    fn test_memory_type_parse() {
        assert_eq!(
            MemoryType::parse("conversation_summary"),
            MemoryType::ConversationSummary
        );
        assert_eq!(
            MemoryType::parse("user_preference"),
            MemoryType::UserPreference
        );
        assert_eq!(MemoryType::parse("unknown_type"), MemoryType::default());
    }

    #[test]
    fn test_memory_type_from_str() {
        use std::str::FromStr;
        assert_eq!(
            MemoryType::from_str("conversation_summary").unwrap(),
            MemoryType::ConversationSummary
        );
        assert_eq!(
            MemoryType::from_str("key_insight").unwrap(),
            MemoryType::KeyInsight
        );
        assert!(MemoryType::from_str("invalid").is_err());
    }

    #[test]
    fn test_memory_importance_score() {
        assert_eq!(MemoryImportance::Low.score(), 0.25);
        assert_eq!(MemoryImportance::Medium.score(), 0.5);
        assert_eq!(MemoryImportance::High.score(), 0.75);
        assert_eq!(MemoryImportance::Critical.score(), 1.0);
    }

    #[test]
    fn test_memory_importance_from_score() {
        assert_eq!(MemoryImportance::from_score(0.1), MemoryImportance::Low);
        assert_eq!(MemoryImportance::from_score(0.3), MemoryImportance::Medium);
        assert_eq!(MemoryImportance::from_score(0.6), MemoryImportance::High);
        assert_eq!(
            MemoryImportance::from_score(0.9),
            MemoryImportance::Critical
        );
    }

    #[test]
    fn test_memory_importance_as_str() {
        assert_eq!(MemoryImportance::Low.as_str(), "low");
        assert_eq!(MemoryImportance::Medium.as_str(), "medium");
        assert_eq!(MemoryImportance::High.as_str(), "high");
        assert_eq!(MemoryImportance::Critical.as_str(), "critical");
    }

    #[test]
    fn test_memory_importance_parse() {
        assert_eq!(MemoryImportance::parse("low"), MemoryImportance::Low);
        assert_eq!(MemoryImportance::parse("high"), MemoryImportance::High);
        assert_eq!(
            MemoryImportance::parse("unknown"),
            MemoryImportance::default()
        );
    }

    #[test]
    fn test_memory_importance_from_str() {
        use std::str::FromStr;
        assert_eq!(
            MemoryImportance::from_str("low").unwrap(),
            MemoryImportance::Low
        );
        assert_eq!(
            MemoryImportance::from_str("critical").unwrap(),
            MemoryImportance::Critical
        );
        assert!(MemoryImportance::from_str("invalid").is_err());
    }

    #[test]
    fn test_memory_new() {
        let memory = Memory::new(
            "session-1",
            "Test content".to_string(),
            MemoryType::KeyInsight,
        );

        assert_eq!(memory.session_id, "session-1");
        assert_eq!(memory.content, "Test content");
        assert_eq!(memory.memory_type, MemoryType::KeyInsight);
        assert_eq!(memory.importance, MemoryImportance::default());
        assert!(memory.tags.is_empty());
        assert!(memory.embedding.is_none());
        assert_eq!(memory.access_count, 0);
        assert!(!memory.id.is_empty());
    }

    #[test]
    fn test_memory_with_importance() {
        let memory = Memory::new(
            "session-1",
            "Test content".to_string(),
            MemoryType::KeyInsight,
        )
        .with_importance(MemoryImportance::High);

        assert_eq!(memory.importance, MemoryImportance::High);
    }

    #[test]
    fn test_memory_with_tags() {
        let memory = Memory::new(
            "session-1",
            "Test content".to_string(),
            MemoryType::KeyInsight,
        )
        .with_tags(vec!["tag1".to_string(), "tag2".to_string()]);

        assert_eq!(memory.tags, vec!["tag1", "tag2"]);
    }

    #[test]
    fn test_memory_with_embedding() {
        let memory = Memory::new(
            "session-1",
            "Test content".to_string(),
            MemoryType::KeyInsight,
        )
        .with_embedding(vec![0.1, 0.2, 0.3]);

        assert_eq!(memory.embedding, Some(vec![0.1, 0.2, 0.3]));
    }

    #[test]
    fn test_memory_bump_access() {
        let mut memory = Memory::new(
            "session-1",
            "Test content".to_string(),
            MemoryType::KeyInsight,
        );

        let original_accessed_at = memory.accessed_at;
        memory.bump_access();

        assert_eq!(memory.access_count, 1);
        assert!(memory.accessed_at >= original_accessed_at);
    }

    #[test]
    fn test_memory_bump_access_multiple() {
        let mut memory =
            Memory::new("s1", "c".to_string(), MemoryType::KeyInsight);
        assert_eq!(memory.access_count, 0);

        memory.bump_access();
        assert_eq!(memory.access_count, 1);

        memory.bump_access();
        assert_eq!(memory.access_count, 2);
    }

    #[test]
    fn test_memory_id_unique() {
        let m1 = Memory::new("s1", "c1".to_string(), MemoryType::KeyInsight);
        let m2 = Memory::new("s1", "c1".to_string(), MemoryType::KeyInsight);
        assert_ne!(m1.id, m2.id);
    }

    #[test]
    fn test_memory_timestamps() {
        let before = chrono::Utc::now();
        let memory = Memory::new("s1", "c".to_string(), MemoryType::KeyInsight);
        let after = chrono::Utc::now();

        assert!(memory.created_at >= before && memory.created_at <= after);
        assert!(memory.updated_at >= before && memory.updated_at <= after);
        assert!(memory.accessed_at >= before && memory.accessed_at <= after);
    }

    #[test]
    fn test_memory_query_default() {
        let query = MemoryQuery::default();

        assert!(query.query.is_empty());
        assert!(query.session_id.is_none());
        assert!(query.memory_types.is_none());
        assert!(query.tags.is_none());
        assert!(query.min_importance.is_none());
        assert_eq!(query.limit, 5);
    }

    #[test]
    fn test_memory_query_with_all_fields() {
        let query = MemoryQuery {
            query: "test query".to_string(),
            session_id: Some("session-1".to_string()),
            memory_types: Some(vec![MemoryType::KeyInsight]),
            tags: Some(vec!["tag1".to_string()]),
            min_importance: Some(MemoryImportance::High),
            limit: 10,
        };

        assert_eq!(query.query, "test query");
        assert_eq!(query.session_id, Some("session-1".to_string()));
        assert_eq!(query.memory_types, Some(vec![MemoryType::KeyInsight]));
        assert_eq!(query.tags, Some(vec!["tag1".to_string()]));
        assert_eq!(query.min_importance, Some(MemoryImportance::High));
        assert_eq!(query.limit, 10);
    }

    #[test]
    fn test_stage1_output_serialization() {
        let output = Stage1Output {
            thread_id: "thread-1".to_string(),
            raw_memory: "raw memory content".to_string(),
            rollout_summary: "rollout summary".to_string(),
            cwd: std::path::PathBuf::from("/test/path"),
            source_updated_at: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&output).unwrap();
        let deserialized: Stage1Output = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.thread_id, output.thread_id);
        assert_eq!(deserialized.raw_memory, output.raw_memory);
        assert_eq!(deserialized.rollout_summary, output.rollout_summary);
    }

    #[test]
    fn test_memory_type_default() {
        let mt = MemoryType::default();
        assert_eq!(mt, MemoryType::ConversationSummary);
    }

    #[test]
    fn test_memory_importance_default() {
        let mi = MemoryImportance::default();
        assert_eq!(mi, MemoryImportance::Medium);
    }

    #[test]
    fn test_memory_importance_from_score_boundaries() {
        assert_eq!(MemoryImportance::from_score(0.0), MemoryImportance::Low);
        assert_eq!(MemoryImportance::from_score(0.29), MemoryImportance::Low);
        assert_eq!(MemoryImportance::from_score(0.3), MemoryImportance::Medium);
        assert_eq!(
            MemoryImportance::from_score(0.59),
            MemoryImportance::Medium
        );
        assert_eq!(MemoryImportance::from_score(0.6), MemoryImportance::High);
        assert_eq!(MemoryImportance::from_score(0.84), MemoryImportance::High);
        assert_eq!(
            MemoryImportance::from_score(0.85),
            MemoryImportance::Critical
        );
        assert_eq!(
            MemoryImportance::from_score(1.0),
            MemoryImportance::Critical
        );
    }

    #[test]
    fn test_memory_importance_from_score_exact_boundaries() {
        assert_eq!(MemoryImportance::from_score(0.3), MemoryImportance::Medium);
        assert_eq!(
            MemoryImportance::from_score(0.2999999),
            MemoryImportance::Low
        );
        assert_eq!(MemoryImportance::from_score(0.6), MemoryImportance::High);
        assert_eq!(
            MemoryImportance::from_score(0.5999999),
            MemoryImportance::Medium
        );
        assert_eq!(
            MemoryImportance::from_score(0.85),
            MemoryImportance::Critical
        );
        assert_eq!(
            MemoryImportance::from_score(0.8499999),
            MemoryImportance::High
        );
    }
}
