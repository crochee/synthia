//! `SessionEntry` — 14-variant tagged union, one per JSONL line.
//!
//! Append-only tree: every change writes one entry; state is replay-able.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use synthia_protocol::{MessageId, SessionId};

use crate::part::Part;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum SessionEntry {
    Header {
        id: SessionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_id: Option<SessionId>,
        created_at: DateTime<Utc>,
        cli_version: String,
        rust_version: String,
        model_provider: String,
        agent_name: String,
        agent_role: String,
        sandbox_policy: String,
        approval_policy: String,
        version: u32, // CURRENT_SESSION_VERSION
    },
    Message {
        id: MessageId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_message_id: Option<MessageId>,
        role: String,
        parts: Vec<Part>,
        time: DateTime<Utc>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model_id: Option<String>,
    },
    Compaction {
        id: MessageId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_message_id: Option<MessageId>,
        first_kept_message_id: MessageId,
        tokens_before: u64,
        #[serde(default)]
        from_hook: bool,
        summary: String,
        dropped_message_ids: Vec<MessageId>,
    },
    BranchSummary {
        id: MessageId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_message_id: Option<MessageId>,
        from_message_id: MessageId,
        summary: String,
        #[serde(default)]
        from_hook: bool,
    },
    ModelChange {
        id: MessageId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_message_id: Option<MessageId>,
        from_model: String,
        to_model: String,
        reason: String,
    },
    ThinkingLevelChange {
        id: MessageId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_message_id: Option<MessageId>,
        from: String,
        to: String,
    },
    Label {
        id: MessageId,
        target_id: MessageId,
        label: String,
        #[serde(default)]
        sticky: bool,
    },
    SessionInfo {
        id: MessageId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_session_id: Option<SessionId>,
        name: String,
        labels: Vec<String>,
    },
    CustomMessageEntry {
        id: MessageId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_message_id: Option<MessageId>,
        payload: serde_json::Value,
        #[serde(default)]
        display: bool,
        source: String,
    },
    CustomEntry {
        id: MessageId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_message_id: Option<MessageId>,
        payload: serde_json::Value,
        source: String,
    },
    Fork {
        id: MessageId,
        parent_session_id: SessionId,
        forked_at_message_id: MessageId,
    },
    Rollback {
        id: MessageId,
        target_message_id: MessageId,
        num_turns: u32,
    },
    ErrorEvent {
        id: MessageId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_message_id: Option<MessageId>,
        error_kind: String,
        recoverable: bool,
        payload: serde_json::Value,
    },
}

impl SessionEntry {
    /// Returns the `MessageId` for this entry, if any.
    ///
    /// `Header` entries have no meaningful `MessageId` and return `None`.
    /// All other variants return `Some(self_id)`.
    pub fn id(&self) -> Option<MessageId> {
        match self {
            SessionEntry::Header { .. } => None,
            SessionEntry::Message { id, .. } => Some(*id),
            SessionEntry::Compaction { id, .. } => Some(*id),
            SessionEntry::BranchSummary { id, .. } => Some(*id),
            SessionEntry::ModelChange { id, .. } => Some(*id),
            SessionEntry::ThinkingLevelChange { id, .. } => Some(*id),
            SessionEntry::Label { id, .. } => Some(*id),
            SessionEntry::SessionInfo { id, .. } => Some(*id),
            SessionEntry::CustomMessageEntry { id, .. } => Some(*id),
            SessionEntry::CustomEntry { id, .. } => Some(*id),
            SessionEntry::Fork { id, .. } => Some(*id),
            SessionEntry::Rollback { id, .. } => Some(*id),
            SessionEntry::ErrorEvent { id, .. } => Some(*id),
        }
    }

    /// Unwrap `id()` with a clear panic message. Use only when you know the
    /// entry is not a `Header`.
    ///
    /// Prefer `id()` (returning `Option<MessageId>`) for code paths that may
    /// encounter any variant.
    pub fn id_unwrap(&self) -> MessageId {
        self.id()
            .expect("Header has no MessageId; use id() to handle the None case")
    }

    pub fn parent_id(&self) -> Option<MessageId> {
        match self {
            SessionEntry::Header { .. } => None,
            SessionEntry::Message {
                parent_message_id, ..
            } => *parent_message_id,
            SessionEntry::Compaction {
                parent_message_id, ..
            } => *parent_message_id,
            SessionEntry::BranchSummary {
                parent_message_id, ..
            } => *parent_message_id,
            SessionEntry::ModelChange {
                parent_message_id, ..
            } => *parent_message_id,
            SessionEntry::ThinkingLevelChange {
                parent_message_id, ..
            } => *parent_message_id,
            SessionEntry::Label { .. } => None,
            SessionEntry::SessionInfo { .. } => None,
            SessionEntry::CustomMessageEntry {
                parent_message_id, ..
            } => *parent_message_id,
            SessionEntry::CustomEntry {
                parent_message_id, ..
            } => *parent_message_id,
            SessionEntry::Fork { .. } => None,
            SessionEntry::Rollback { .. } => None,
            SessionEntry::ErrorEvent {
                parent_message_id, ..
            } => *parent_message_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_entry_serde() {
        let entry = SessionEntry::Header {
            id: SessionId::new(),
            parent_id: None,
            created_at: Utc::now(),
            cli_version: "0.2.0".to_string(),
            rust_version: "1.85".to_string(),
            model_provider: "anthropic".to_string(),
            agent_name: "build".to_string(),
            agent_role: "coder".to_string(),
            sandbox_policy: "default".to_string(),
            approval_policy: "unless_trusted".to_string(),
            version: 2,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: SessionEntry = serde_json::from_str(&json).unwrap();
        match parsed {
            SessionEntry::Header { version, .. } => assert_eq!(version, 2),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn header_entry_id_is_none() {
        let entry = SessionEntry::Header {
            id: SessionId::new(),
            parent_id: None,
            created_at: Utc::now(),
            cli_version: "0.2.0".to_string(),
            rust_version: "1.85".to_string(),
            model_provider: "anthropic".to_string(),
            agent_name: "build".to_string(),
            agent_role: "coder".to_string(),
            sandbox_policy: "default".to_string(),
            approval_policy: "unless_trusted".to_string(),
            version: 2,
        };
        assert_eq!(entry.id(), None);
        assert_eq!(entry.parent_id(), None);
    }

    #[test]
    fn message_entry_id_is_some() {
        let mid = MessageId::new();
        let entry = SessionEntry::Message {
            id: mid,
            parent_message_id: None,
            role: "user".to_string(),
            parts: vec![],
            time: Utc::now(),
            agent_name: None,
            model_id: None,
        };
        assert_eq!(entry.id(), Some(mid));
    }
}
