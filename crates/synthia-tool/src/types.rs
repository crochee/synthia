use std::path::PathBuf;

use serde::{Deserialize, Serialize};
pub use synthia_core::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DispatchMode {
    Fork,
    Teammate,
    Worktree,
}

/// Execution context passed to every tool invocation.
///
/// Carries session identity, workspace root, dispatch mode,
/// visible conversation messages, and output truncation config.
#[derive(Debug, Clone)]
pub struct Context {
    pub session_id: String,
    pub workspace_root: PathBuf,
    pub caller_agent: String,
    pub dispatch_mode: DispatchMode,
    /// Conversation messages visible to the current turn. Populated by
    /// the agent runtime so that context-aware tools (e.g.
    /// `self_reflect`) can review the session history without requiring
    /// the LLM to pass it as arguments.
    pub messages: Vec<synthia_provider::types::Message>,
    /// Output truncation configuration. Default: 50 KiB / 2000 lines.
    pub output_bound: crate::truncate::OutputBound,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            workspace_root: PathBuf::new(),
            caller_agent: String::new(),
            dispatch_mode: DispatchMode::Fork,
            messages: Vec::new(),
            output_bound: crate::truncate::OutputBound::default(),
        }
    }
}

impl Context {
    pub fn new(session_id: String, workspace_root: PathBuf) -> Self {
        Self {
            session_id,
            workspace_root,
            caller_agent: "default".to_string(),
            dispatch_mode: DispatchMode::Fork,
            messages: Vec::new(),
            output_bound: crate::truncate::OutputBound::default(),
        }
    }

    /// Attach the conversation messages that should be visible to the
    /// tool execution.
    pub fn with_messages(
        mut self,
        messages: Vec<synthia_provider::types::Message>,
    ) -> Self {
        self.messages = messages;
        self
    }
}

/// Reason a [`ToolOutput`] was truncated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TruncatedBy {
    /// Truncated to a maximum number of output lines.
    Lines { shown: usize, total: usize },
    /// Truncated to a maximum number of output bytes.
    Bytes { shown: usize, total: usize },
    /// Output spilled to a managed file.
    SpilledTo { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolOutput {
    pub content: Vec<synthia_provider::types::ContentPart>,
    pub is_error: Option<bool>,
    /// Structured metadata accompanying the tool result (e.g. counts,
    /// timing, truncation reason). Defaults to empty for backward
    /// compatibility.
    #[serde(default)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
    /// Optional truncation reason — populated when the orchestrator or
    /// the tool itself trimmed the output before returning it to the
    /// LLM.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated_by: Option<TruncatedBy>,
}

impl ToolOutput {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: vec![synthia_provider::types::ContentPart::Text(
                synthia_provider::types::TextContent {
                    text: content.into(),
                    cache_control: None,
                },
            )],
            is_error: None,
            metadata: serde_json::Map::new(),
            truncated_by: None,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: vec![synthia_provider::types::ContentPart::Text(
                synthia_provider::types::TextContent {
                    text: content.into(),
                    cache_control: None,
                },
            )],
            is_error: Some(true),
            metadata: serde_json::Map::new(),
            truncated_by: None,
        }
    }

    pub fn empty() -> Self {
        Self {
            content: vec![],
            is_error: None,
            metadata: serde_json::Map::new(),
            truncated_by: None,
        }
    }

    pub fn is_text(&self) -> bool {
        self.is_error.is_none()
    }

    /// Build a [`ToolOutput`] from a raw [`serde_json::Value`], using the
    /// JSON string form as the textual content.
    pub fn from_raw(raw: serde_json::Value) -> Self {
        Self::text(raw.to_string())
    }

    /// Attach a truncation reason. Builder-style; returns the modified
    /// output for chaining.
    pub fn with_truncated_by(mut self, truncated_by: TruncatedBy) -> Self {
        self.truncated_by = Some(truncated_by);
        self
    }

    /// Insert a metadata entry. Builder-style; returns the modified
    /// output for chaining.
    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

impl From<String> for ToolOutput {
    fn from(content: String) -> Self {
        Self::text(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_new_defaults() {
        let ctx = Context::new("s1".into(), PathBuf::from("/tmp"));
        assert_eq!(ctx.session_id, "s1");
        assert_eq!(ctx.caller_agent, "default");
        assert!(ctx.messages.is_empty());
        assert_eq!(ctx.output_bound.per_call_max_bytes, 50 * 1024);
        assert_eq!(ctx.output_bound.per_call_max_lines, 2000);
    }

    // -- DispatchMode --------------------------------------------------

    /// `DispatchMode` MUST serialize each variant in PascalCase form
    /// (the default serde behavior — no `rename_all` attribute).
    #[test]
    fn dispatch_mode_serializes_as_pascal_case() {
        assert_eq!(
            serde_json::to_string(&DispatchMode::Fork).unwrap(),
            "\"Fork\""
        );
        assert_eq!(
            serde_json::to_string(&DispatchMode::Teammate).unwrap(),
            "\"Teammate\""
        );
        assert_eq!(
            serde_json::to_string(&DispatchMode::Worktree).unwrap(),
            "\"Worktree\""
        );
    }

    /// `DispatchMode` MUST round-trip each variant through JSON.
    #[test]
    fn dispatch_mode_round_trips_through_json() {
        for mode in [
            DispatchMode::Fork,
            DispatchMode::Teammate,
            DispatchMode::Worktree,
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            let parsed: DispatchMode = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, mode);
        }
    }

    /// All three `DispatchMode` variants MUST be pairwise distinct.
    #[test]
    fn dispatch_mode_variants_are_pairwise_distinct() {
        assert_ne!(DispatchMode::Fork, DispatchMode::Teammate);
        assert_ne!(DispatchMode::Fork, DispatchMode::Worktree);
        assert_ne!(DispatchMode::Teammate, DispatchMode::Worktree);
    }

    // -- Context::default ----------------------------------------------

    /// `Context::default()` MUST yield a session-less, message-less
    /// context with the default OutputBound (50 KiB / 2000 lines).
    #[test]
    fn context_default_is_empty_with_default_bound() {
        let ctx = Context::default();
        assert_eq!(ctx.session_id, "");
        assert_eq!(ctx.workspace_root, PathBuf::new());
        assert_eq!(ctx.caller_agent, "");
        assert_eq!(ctx.dispatch_mode, DispatchMode::Fork);
        assert!(ctx.messages.is_empty());
        assert_eq!(ctx.output_bound.per_call_max_bytes, 50 * 1024);
        assert_eq!(ctx.output_bound.per_call_max_lines, 2000);
    }

    // -- Context::new --------------------------------------------------

    /// `Context::new` MUST initialize `caller_agent` to `"default"`.
    #[test]
    fn context_new_defaults_caller_agent_to_default_string() {
        let ctx = Context::new("s".into(), PathBuf::from("/p"));
        assert_eq!(ctx.caller_agent, "default");
    }

    /// `Context::new` MUST populate `session_id` and `workspace_root`
    /// from its arguments verbatim.
    #[test]
    fn context_new_propagates_session_id_and_workspace_root() {
        let ctx =
            Context::new("session-xyz".into(), PathBuf::from("/home/u/proj"));
        assert_eq!(ctx.session_id, "session-xyz");
        assert_eq!(ctx.workspace_root, PathBuf::from("/home/u/proj"));
    }

    /// `Context::new` MUST default `dispatch_mode` to `Fork`.
    #[test]
    fn context_new_defaults_dispatch_mode_to_fork() {
        let ctx = Context::new("s".into(), PathBuf::from("/p"));
        assert_eq!(ctx.dispatch_mode, DispatchMode::Fork);
    }

    // -- Context::with_messages ----------------------------------------

    /// `Context::with_messages` MUST consume the receiver, attach the
    /// supplied messages, and return the modified context.
    #[test]
    fn context_with_messages_consumes_and_attaches() {
        let ctx = Context::new("s".into(), PathBuf::from("/p"));
        let ctx = ctx.with_messages(vec![]);
        assert!(ctx.messages.is_empty());
    }

    // -- TruncatedBy ---------------------------------------------------

    /// `TruncatedBy` MUST serialize with the `kind` tag and snake_case
    /// variant name.
    #[test]
    fn truncated_by_serializes_with_kind_tag_snake_case() {
        let t = TruncatedBy::Lines {
            shown: 10,
            total: 100,
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"kind\":\"lines\""), "got: {json}");
        assert!(json.contains("\"shown\":10"), "got: {json}");
        assert!(json.contains("\"total\":100"), "got: {json}");
    }

    #[test]
    fn truncated_by_bytes_variant_uses_snake_case_kind() {
        let t = TruncatedBy::Bytes {
            shown: 256,
            total: 1024,
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"kind\":\"bytes\""), "got: {json}");
    }

    #[test]
    fn truncated_by_spilled_to_uses_snake_case_kind() {
        let t = TruncatedBy::SpilledTo {
            path: "/tmp/spill.txt".to_string(),
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"kind\":\"spilled_to\""), "got: {json}");
    }

    /// `TruncatedBy` MUST round-trip each variant through JSON.
    #[test]
    fn truncated_by_round_trips_each_variant() {
        let variants = [
            TruncatedBy::Lines {
                shown: 10,
                total: 100,
            },
            TruncatedBy::Bytes {
                shown: 256,
                total: 1024,
            },
            TruncatedBy::SpilledTo {
                path: "/tmp/spill".to_string(),
            },
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: TruncatedBy = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, v);
        }
    }

    // -- ToolOutput constructors ---------------------------------------

    /// `ToolOutput::text` MUST build a `is_error = None` output with
    /// empty metadata and no truncation reason.
    #[test]
    fn tool_output_text_builds_non_error_with_empty_metadata() {
        let o = ToolOutput::text("hello");
        assert!(o.is_error.is_none());
        assert_eq!(o.content.len(), 1);
        assert!(o.metadata.is_empty());
        assert!(o.truncated_by.is_none());
    }

    /// `ToolOutput::text` MUST accept both `&str` and `String`.
    #[test]
    fn tool_output_text_accepts_str_and_string() {
        let a = ToolOutput::text("abc");
        let b = ToolOutput::text(String::from("abc"));
        assert_eq!(a, b);
    }

    /// `ToolOutput::error` MUST set `is_error = Some(true)`.
    #[test]
    fn tool_output_error_sets_is_error_true() {
        let o = ToolOutput::error("bad input");
        assert_eq!(o.is_error, Some(true));
        assert_eq!(o.content.len(), 1);
        assert!(o.metadata.is_empty());
    }

    /// `ToolOutput::empty` MUST build an empty content vector with
    /// `is_error = None`.
    #[test]
    fn tool_output_empty_has_no_content_and_no_error() {
        let o = ToolOutput::empty();
        assert!(o.content.is_empty());
        assert!(o.is_error.is_none());
        assert!(o.metadata.is_empty());
        assert!(o.truncated_by.is_none());
    }

    // -- ToolOutput::is_text -------------------------------------------

    /// `ToolOutput::is_text` MUST return `true` iff `is_error` is `None`.
    #[test]
    fn tool_output_is_text_true_when_error_is_none() {
        assert!(ToolOutput::text("x").is_text());
        assert!(ToolOutput::empty().is_text());
    }

    /// `ToolOutput::is_text` MUST return `false` when error is set.
    #[test]
    fn tool_output_is_text_false_when_error_is_true() {
        assert!(!ToolOutput::error("x").is_text());
    }

    // -- ToolOutput::from_raw ------------------------------------------

    /// `ToolOutput::from_raw` MUST convert any `serde_json::Value` to
    /// its `to_string()` form wrapped in `text`.
    #[test]
    fn tool_output_from_raw_wraps_value_as_text() {
        let raw = serde_json::json!({"k": [1, 2]});
        let o = ToolOutput::from_raw(raw);
        assert!(o.is_error.is_none());
        assert_eq!(o.content.len(), 1);
    }

    // -- ToolOutput::with_truncated_by ---------------------------------

    /// `ToolOutput::with_truncated_by` MUST consume the receiver,
    /// attach the supplied `TruncatedBy`, and return the modified output.
    #[test]
    fn tool_output_with_truncated_by_attaches_reason() {
        let o = ToolOutput::text("x").with_truncated_by(TruncatedBy::Lines {
            shown: 10,
            total: 100,
        });
        assert!(matches!(
            o.truncated_by,
            Some(TruncatedBy::Lines {
                shown: 10,
                total: 100
            })
        ));
    }

    // -- ToolOutput::with_metadata -------------------------------------

    /// `ToolOutput::with_metadata` MUST insert the supplied key/value
    /// pair into the `metadata` map.
    #[test]
    fn tool_output_with_metadata_inserts_entry() {
        let o = ToolOutput::text("x")
            .with_metadata("elapsed_ms", serde_json::json!(42));
        assert_eq!(o.metadata.get("elapsed_ms"), Some(&serde_json::json!(42)));
    }

    /// Multiple `with_metadata` calls MUST accumulate entries.
    #[test]
    fn tool_output_with_metadata_accumulates_entries() {
        let o = ToolOutput::text("x")
            .with_metadata("a", serde_json::json!(1))
            .with_metadata("b", serde_json::json!("two"));
        assert_eq!(o.metadata.len(), 2);
    }

    // -- ToolOutput JSON round-trip ------------------------------------

    /// `ToolOutput` MUST round-trip every field through JSON.
    #[test]
    fn tool_output_round_trips_through_json() {
        let o = ToolOutput::text("hi")
            .with_metadata("k", serde_json::json!("v"))
            .with_truncated_by(TruncatedBy::Bytes {
                shown: 100,
                total: 1000,
            });
        let json = serde_json::to_string(&o).unwrap();
        let parsed: ToolOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, o);
    }

    /// Old `ToolOutput` payloads without `metadata` and `truncated_by`
    /// MUST still deserialize (forward-compat for fields added later).
    #[test]
    fn tool_output_old_payload_without_metadata_deserializes() {
        let json = r#"{"content":[],"is_error":null}"#;
        let parsed: ToolOutput =
            serde_json::from_str(json).expect("old payload");
        assert!(parsed.content.is_empty());
        assert!(parsed.is_error.is_none());
        assert!(parsed.metadata.is_empty());
        assert!(parsed.truncated_by.is_none());
    }

    /// `truncated_by` MUST be omitted from JSON output when `None`.
    #[test]
    fn tool_output_omits_truncated_by_when_none() {
        let o = ToolOutput::text("x");
        let json = serde_json::to_string(&o).unwrap();
        assert!(
            !json.contains("truncated_by"),
            "expected no truncated_by key in {json}"
        );
    }

    // -- From<String> --------------------------------------------------

    /// `From<String>` MUST build a non-error `ToolOutput`.
    #[test]
    fn from_string_builds_text_output() {
        let o: ToolOutput = String::from("hi").into();
        assert!(o.is_error.is_none());
        assert_eq!(o.content.len(), 1);
    }
}
