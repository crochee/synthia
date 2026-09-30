//! [`ToolOutputDefinition`] — tool-owned canonical rendering contract.
//!
//! Adopted from dsh `packages/core/tools/src/index.ts:144-168`
//! `ToolOutputDefinition`. Each tool declares one definition that
//! covers both the LLM-facing projection (what the model sees as
//! the tool call / tool result) and the UI-facing projection (what
//! the chat UI renders for the human). One source of truth per
//! tool, so the LLM and the UI can never disagree about what
//! happened.
//!
//! ## Why a definition separate from the tool itself?
//!
//! The tool's `Tool::call` returns a [`ToolOutput`] (machine
//! content parts). The chat UI wants a *rendering* of that output
//! — a styled string, a code block, a file-tree, a diff. The LLM
//! wants a *textual projection* for its next reasoning pass. Both
//! are functions of `(tool_name, call_args, raw_output)`; both must
//! be replay-safe (deterministic from inputs, no I/O, no clock).
//!
//! [`ToolOutputDefinition`] makes the contract explicit:
//!
//! | Surface    | Method                                  | Returns       |
//! |------------|-----------------------------------------|---------------|
//! | LLM-bound  | [`ToolOutputDefinition::present_call`]   | one short text line for the next LLM call |
//! | UI-bound   | [`ToolOutputDefinition::present_result`] | one styled markdown / html block |
//!
//! `Tool::output_definition()` is opt-in. Tools that don't override
//! get a default pass-through that round-trips the raw text
//! verbatim. Lib consumers wire [`ToolOutputDefinition`] into a
//! `OutputDefinitionRenderer` (synthia-steering) or a chat-UI
//! formatter (synthia-web) without changing the tool itself.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use synthia_provider::types::ContentPart;

use crate::{ToolOutput, types::Context};

/// Stable per-tool rendering contract. Cheap to clone; embed in
/// any catalog or snapshot.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolOutputDefinition {
    /// Tool name (matches `Tool::name()`).
    pub name: String,
    /// Render kind hint. Consumers (chat UI, TUI, replay log)
    /// use this to pick the right renderer. Defaults to
    /// [`RenderKind::Text`].
    pub kind: RenderKind,
    /// Optional title used by UI surfaces. Falls back to `name`
    /// when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Tool-authored hints (e.g. "this tool streams progress").
    /// Read by UI surfaces; never interpreted by the LLM.
    #[serde(default)]
    pub presentation: BTreeMap<String, Value>,
    /// R29 (dsh `ToolDefinition.presentationMeta` parity):
    /// tool-authored presentation metadata. Lets the same tool
    /// serve the LLM (via `present_result`) and a UI card (via
    /// this field) without leaking UI vocabulary into the
    /// model-visible text. Default: `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation_meta: Option<Value>,
}

impl ToolOutputDefinition {
    /// Build a definition for a tool that just round-trips its
    /// output verbatim. The default for tools that do not override
    /// `output_definition()`.
    pub fn passthrough(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: RenderKind::Text,
            title: None,
            presentation: BTreeMap::new(),
            presentation_meta: None,
        }
    }

    /// Builder: set the render kind.
    #[must_use]
    pub fn with_kind(mut self, kind: RenderKind) -> Self {
        self.kind = kind;
        self
    }

    /// Builder: set the display title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Builder: insert a presentation hint.
    #[must_use]
    pub fn with_presentation(
        mut self,
        key: impl Into<String>,
        value: Value,
    ) -> Self {
        self.presentation.insert(key.into(), value);
        self
    }

    /// R29 (dsh `presentationMeta` parity): builder: attach a
    /// presentation-metadata value (UI card data). The default
    /// `None` keeps the model-visible and UI-visible projections
    /// aligned; tools that need a richer UI card attach their
    /// own value here without changing `present_result`.
    #[must_use]
    pub fn with_presentation_meta(mut self, value: Value) -> Self {
        self.presentation_meta = Some(value);
        self
    }

    /// Render the tool call (the "what the model asked for"
    /// projection). Default impl emits `name(args)` as a short
    /// one-liner. Override for tools whose calls deserve
    /// symbol-aware rendering (e.g. `bash` shows the command,
    /// `read` shows the path + offset).
    pub fn present_call(
        &self,
        arguments: &Value,
        _context: &Context,
    ) -> String {
        format!("{}: {}", self.name, summarise_args(arguments))
    }

    /// Render the tool result (the "what the tool returned"
    /// projection). Default impl joins the text parts of the
    /// output. Override for tools whose results deserve structured
    /// rendering (e.g. `shell` includes an exit-code footer,
    /// `web_fetch` includes a status line).
    pub fn present_result(
        &self,
        arguments: &Value,
        output: &ToolOutput,
        _context: &Context,
    ) -> String {
        let body = output
            .content
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text(tc) => Some(tc.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");
        let prefix = summarise_args(arguments);
        if prefix.is_empty() {
            body
        } else {
            format!("{} → {}", prefix, body)
        }
    }

    /// R29 (dsh `finalizeContent` parity): the last-mile
    /// transform applied to a tool's [`ToolOutput`] after every
    /// outcome — including pipeline failures. The default impl
    /// returns the output's `content` unchanged so the
    /// `isError` flag travels through. Tools that need a
    /// different shape on the failure path (e.g. an error
    /// shortener that drops the body when the model should just
    /// re-try) override this.
    ///
    /// R29 also passes this the `is_error` flag, so an override
    /// that wants to swap content based on outcome has a
    /// complete view. The default preserves existing behaviour
    /// for every tool written against the pre-R17 contract.
    pub fn finalize_content(
        &self,
        output: &ToolOutput,
    ) -> Vec<synthia_provider::types::ContentPart> {
        output.content.clone()
    }
}

// R17: the `HasOutputDefinition` extension trait from R10 was
// removed — its `impl<T: Tool + ?Sized> HasOutputDefinition for T`
// blanket impl made a concrete override a coherence error, so no
// tool could ever opt in. The method now lives on `Tool` itself as
// a defaulted method (see `Tool::output_definition`).

/// Render-kind hint for UI consumers. Serialises in
/// `snake_case` (matching the rest of the API surface:
/// `ContentPart`, `OperationState`), so `"shell"` / `"web_fetch"`
/// / `"todo"` ride the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RenderKind {
    /// Plain text.
    Text,
    /// Shell command output (exit-code / signal markers).
    Shell,
    /// Read tool (path + offset + limit line numbers).
    Read,
    /// Write tool (path + bytes written).
    Write,
    /// Web fetch (status + body).
    WebFetch,
    /// TODO list (JSON array of items).
    Todo,
    /// Generic JSON object.
    Json,
}

/// One-line summary of a tool call's arguments. Used by the default
/// `present_call`. Keeps the projection deterministic and short.
fn summarise_args(arguments: &Value) -> String {
    match arguments {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(_) | Value::Object(_) => {
            if let Some(obj) = arguments.as_object() {
                let first_string =
                    obj.values().find_map(|v| v.as_str().map(str::to_string));
                if let Some(s) = first_string {
                    return s;
                }
                return format!(
                    "{{{} keys}}",
                    obj.keys().cloned().collect::<Vec<_>>().join(", ")
                );
            }
            "[…]".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use synthia_provider::types::TextContent;

    use super::*;
    #[test]
    fn passthrough_definition_is_text() {
        let d = ToolOutputDefinition::passthrough("read");
        assert_eq!(d.name, "read");
        assert_eq!(d.kind, RenderKind::Text);
        assert!(d.title.is_none());
        assert!(d.presentation.is_empty());
    }

    #[test]
    fn builder_chain_sets_fields() {
        let d = ToolOutputDefinition::passthrough("bash")
            .with_kind(RenderKind::Shell)
            .with_title("Shell")
            .with_presentation("streaming", serde_json::json!(true));
        assert_eq!(d.kind, RenderKind::Shell);
        assert_eq!(d.title.as_deref(), Some("Shell"));
        assert_eq!(
            d.presentation.get("streaming"),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn present_call_default_renders_name_args() {
        let d = ToolOutputDefinition::passthrough("bash");
        let ctx = Context::default();
        let line = d.present_call(&serde_json::json!({"cmd": "ls -la"}), &ctx);
        assert_eq!(line, "bash: ls -la");
    }

    #[test]
    fn present_result_default_joins_text_parts() {
        let d = ToolOutputDefinition::passthrough("read");
        let output = ToolOutput {
            content: vec![ContentPart::Text(TextContent {
                text: "hello\nworld".into(),
                cache_control: None,
            })],
            is_error: None,
            metadata: serde_json::Map::new(),
            truncated_by: None,
        };
        let ctx = Context::default();
        let rendered = d.present_result(
            &serde_json::json!({"path": "/tmp/x"}),
            &output,
            &ctx,
        );
        assert_eq!(rendered, "/tmp/x → hello\nworld");
    }

    #[test]
    fn present_result_skips_non_text_parts() {
        let d = ToolOutputDefinition::passthrough("read");
        let output = ToolOutput {
            content: vec![
                ContentPart::Text(TextContent {
                    text: "A".into(),
                    cache_control: None,
                }),
                ContentPart::Text(TextContent {
                    text: "B".into(),
                    cache_control: None,
                }),
            ],
            is_error: None,
            metadata: serde_json::Map::new(),
            truncated_by: None,
        };
        let ctx = Context::default();
        let rendered =
            d.present_result(&serde_json::Value::Null, &output, &ctx);
        assert_eq!(rendered, "AB");
    }

    #[test]
    fn summarise_args_extracts_first_string_field() {
        let s =
            summarise_args(&serde_json::json!({"cmd": "ls", "cwd": "/tmp"}));
        assert_eq!(s, "ls");
    }

    #[test]
    fn summarise_args_handles_primitive_types() {
        assert_eq!(summarise_args(&serde_json::json!(null)), "");
        assert_eq!(summarise_args(&serde_json::json!(true)), "true");
        assert_eq!(summarise_args(&serde_json::json!(42)), "42");
        assert_eq!(summarise_args(&serde_json::json!("plain")), "plain");
    }

    #[test]
    fn summarise_args_object_without_string_falls_back_to_keys() {
        let s = summarise_args(&serde_json::json!({"a": 1, "b": 2}));
        assert!(s.contains("a") && s.contains("b"));
        assert!(s.contains("keys"));
    }

    // R29 dsh §8 parity tests: presentation_meta + finalize_content.
    #[test]
    fn presentation_meta_defaults_to_none() {
        let d = ToolOutputDefinition::passthrough("read");
        assert!(d.presentation_meta.is_none());
    }

    #[test]
    fn presentation_meta_builder_attaches_value() {
        let d = ToolOutputDefinition::passthrough("read")
            .with_presentation_meta(serde_json::json!({
                "kind": "code",
                "language": "rust",
            }));
        let meta = d
            .presentation_meta
            .as_ref()
            .expect("presentation_meta attached");
        assert_eq!(meta["kind"], "code");
        assert_eq!(meta["language"], "rust");
    }

    #[test]
    fn presentation_meta_skipped_when_none() {
        let d = ToolOutputDefinition::passthrough("read");
        let v = serde_json::to_value(&d).expect("serializable");
        assert!(
            v.get("presentation_meta").is_none(),
            "presentation_meta absent in serialised output when None"
        );
    }

    #[test]
    fn finalize_content_default_returns_content_unchanged() {
        let d = ToolOutputDefinition::passthrough("read");
        let output = ToolOutput {
            content: vec![ContentPart::Text(TextContent {
                text: "hello".into(),
                cache_control: None,
            })],
            is_error: Some(true),
            metadata: serde_json::Map::new(),
            truncated_by: None,
        };
        let finalized = d.finalize_content(&output);
        assert_eq!(finalized.len(), 1);
        match &finalized[0] {
            ContentPart::Text(tc) => assert_eq!(tc.text, "hello"),
            _ => panic!("expected text part"),
        }
    }
}
