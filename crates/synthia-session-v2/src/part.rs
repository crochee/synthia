//! `Part` enum — 11 content variants within a `Message`.
//!
//! Mirrors opencode `Part` discriminated union.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use synthia_protocol::MessageId;

/// 11-variant content discriminated union.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Part {
    Text(TextPart),
    Reasoning(ReasoningPart),
    Tool(crate::tool_part::ToolPart),
    File(FilePart),
    StepStart(StepStartPart),
    StepFinish(StepFinishPart),
    Patch(PatchPart),
    Snapshot(SnapshotPart),
    Compaction(CompactionPart),
    Subtask(SubtaskPart),
    Agent(AgentPart),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextPart {
    pub text: String,
    #[serde(default)]
    pub synthetic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReasoningPart {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilePart {
    pub path: String,
    pub content_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StepStartPart {
    pub step_id: String,
    pub time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StepFinishPart {
    pub step_id: String,
    pub time: DateTime<Utc>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchPart {
    pub file_path: String,
    pub diff: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotPart {
    pub file_path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactionPart {
    pub summary: String,
    pub dropped_message_ids: Vec<MessageId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubtaskPart {
    pub agent_name: String,
    pub prompt: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPart {
    pub agent_name: String,
    pub session_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_part_serde() {
        let part = Part::Text(TextPart {
            text: "hello".to_string(),
            synthetic: false,
        });
        let json = serde_json::to_string(&part).unwrap();
        let parsed: Part = serde_json::from_str(&json).unwrap();
        match parsed {
            Part::Text(t) => assert_eq!(t.text, "hello"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn compaction_part_carries_dropped_ids() {
        let part = Part::Compaction(CompactionPart {
            summary: "summary".to_string(),
            dropped_message_ids: vec![MessageId::new(), MessageId::new()],
        });
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"compaction\""));
    }
}
