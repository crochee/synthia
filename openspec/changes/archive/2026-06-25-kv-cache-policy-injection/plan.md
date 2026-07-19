# KV Cache Policy Injection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `apply_cache_policy` (Rust port of opencode's `applyCachePolicy`) that proactively injects Anthropic `cache_control` hints on the last tool / system / user message, gated by `ModelProvider::supports_inline_cache_hints()`.

**Architecture:** New `cache_policy.rs` module in `synthia-provider` crate exports `CachePolicy` struct + `apply_cache_policy` free function. `AnthropicProvider::transform_request` calls `apply_cache_policy` when `request.cache_policy` is `Some(policy)` and constructs `AnthropicSystem::Structured` / cache_control-bearing tools & messages. When `cache_policy: None`, behavior is byte-identical to current (backward compatible).

**Tech Stack:** Rust + serde + async-trait. No new crate dependencies.

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/synthia-provider/src/anthropic/types.rs` | Modify | Add `CacheControl`, `AnthropicSystem`, `AnthropicSystemBlock`; add `cache_control` field to `AnthropicTool` + `AnthropicContentBlock` variants; change `AnthropicRequest.system` type |
| `crates/synthia-provider/src/cache_policy.rs` | Create | New module: `CachePolicy`, `MessageCacheStrategy`, `apply_cache_policy` |
| `crates/synthia-provider/src/lib.rs` | Modify | Export `cache_policy` module + public types |
| `crates/synthia-provider/src/types/completion.rs` | Modify | Add `cache_policy: Option<CachePolicy>` field to `CompletionRequest` |
| `crates/synthia-provider/src/types/tool.rs` | Modify | Add `cache_control: Option<CacheControl>` field to `ToolDefinition` (needed because apply_cache_policy marks CompletionRequest.tools, not AnthropicRequest.tools) |
| `crates/synthia-provider/src/traits.rs` | Modify | Add `supports_inline_cache_hints` default method to `ModelProvider` trait |
| `crates/synthia-provider/src/anthropic/provider/core.rs` | Modify | Impl `supports_inline_cache_hints` override on `AnthropicProvider` |
| `crates/synthia-provider/src/anthropic/provider/transform.rs` | Modify | Integrate `apply_cache_policy` call; construct `AnthropicSystem::Structured` when policy.system; mark last tool / last user content block |
| `crates/synthia-provider/src/tests.rs` | Modify | Add cache_policy unit tests (or new `cache_policy/tests.rs` inline) |

---

## Task 1: Add CacheControl and AnthropicSystem types

**Files:**
- Modify: `crates/synthia-provider/src/anthropic/types.rs`
- Test: inline `#[cfg(test)] mod tests` at bottom of `types.rs`

- [ ] **Step 1: Write failing test for CacheControl serialization**

Append to `crates/synthia-provider/src/anthropic/types.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_control_default_serializes_to_ephemeral() {
        let cc = CacheControl::default();
        let json = serde_json::to_value(&cc).unwrap();
        assert_eq!(json, serde_json::json!({"type": "ephemeral"}));
    }

    #[test]
    fn cache_control_with_ttl_serializes_with_ttl_seconds() {
        let cc = CacheControl {
            r#type: "ephemeral".to_string(),
            ttl_seconds: Some(3600),
        };
        let json = serde_json::to_value(&cc).unwrap();
        assert_eq!(json, serde_json::json!({"type": "ephemeral", "ttl_seconds": 3600}));
    }

    #[test]
    fn anthropic_system_text_serializes_as_plain_string() {
        let sys = AnthropicSystem::Text("You are helpful.".to_string());
        let json = serde_json::to_value(&sys).unwrap();
        assert_eq!(json, serde_json::json!("You are helpful."));
    }

    #[test]
    fn anthropic_system_structured_serializes_as_array_with_cache_control() {
        let sys = AnthropicSystem::Structured(vec![AnthropicSystemBlock {
            text: "You are helpful.".to_string(),
            cache_control: Some(CacheControl::default()),
        }]);
        let json = serde_json::to_value(&sys).unwrap();
        assert_eq!(json, serde_json::json!([
            {"type": "text", "text": "You are helpful.", "cache_control": {"type": "ephemeral"}}
        ]));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p synthia-provider --lib anthropic::types::tests`
Expected: FAIL with "cannot find type `CacheControl`" / "`AnthropicSystem`" errors

- [ ] **Step 3: Implement CacheControl, AnthropicSystem, AnthropicSystemBlock**

Insert ABOVE the `#[cfg(test)] mod tests` block in `crates/synthia-provider/src/anthropic/types.rs`:

```rust
/// Anthropic `cache_control` hint. Serialized as `{"type": "ephemeral"}` by default.
/// Attached to the last tool / last content block / last system block to mark
/// cache prefix boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct CacheControl {
    #[serde(rename = "type")]
    pub(super) r#type: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(super) ttl_seconds: Option<u32>,
}

impl Default for CacheControl {
    fn default() -> Self {
        Self {
            r#type: "ephemeral".to_string(),
            ttl_seconds: None,
        }
    }
}

/// One block of a structured system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AnthropicSystemBlock {
    #[serde(rename = "type", default = "default_system_block_type")]
    pub(super) r#type: String,
    pub(super) text: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(super) cache_control: Option<CacheControl>,
}

fn default_system_block_type() -> String {
    "text".to_string()
}

/// Anthropic system field. `Text` variant preserves pre-change serialization
/// (plain JSON string). `Structured` variant enables `cache_control` attachment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum AnthropicSystem {
    Text(String),
    Structured(Vec<AnthropicSystemBlock>),
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p synthia-provider --lib anthropic::types::tests`
Expected: PASS (4 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-provider/src/anthropic/types.rs
git commit -m "feat(provider): add CacheControl + AnthropicSystem types for cache hint injection"
```

---

## Task 2: Add cache_control field to AnthropicTool and AnthropicContentBlock variants

**Files:**
- Modify: `crates/synthia-provider/src/anthropic/types.rs`
- Modify: `crates/synthia-provider/src/anthropic/provider/transform.rs` (update match arms)

- [ ] **Step 1: Write failing test for AnthropicTool with cache_control**

Append to the `#[cfg(test)] mod tests` block in `types.rs`:

```rust
    #[test]
    fn anthropic_tool_with_cache_control_serializes() {
        let tool = AnthropicTool {
            name: "read_file".to_string(),
            description: "Read a file".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            cache_control: Some(CacheControl::default()),
        };
        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(json, serde_json::json!({
            "name": "read_file",
            "description": "Read a file",
            "input_schema": {"type": "object"},
            "cache_control": {"type": "ephemeral"}
        }));
    }

    #[test]
    fn anthropic_tool_without_cache_control_omits_field() {
        let tool = AnthropicTool {
            name: "read_file".to_string(),
            description: "Read a file".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            cache_control: None,
        };
        let json = serde_json::to_value(&tool).unwrap();
        assert!(json.get("cache_control").is_none());
    }

    #[test]
    fn anthropic_content_block_text_with_cache_control_serializes() {
        let block = AnthropicContentBlock::Text {
            text: "hello".to_string(),
            cache_control: Some(CacheControl::default()),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json, serde_json::json!({
            "type": "text",
            "text": "hello",
            "cache_control": {"type": "ephemeral"}
        }));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p synthia-provider --lib anthropic::types::tests`
Expected: FAIL (struct missing `cache_control` field)

- [ ] **Step 3: Add cache_control field to AnthropicTool**

In `crates/synthia-provider/src/anthropic/types.rs`, replace the `AnthropicTool` struct:

```rust
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(super) struct AnthropicTool {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(super) cache_control: Option<CacheControl>,
}
```

- [ ] **Step 4: Add cache_control field to AnthropicContentBlock Text/ToolUse/ToolResult variants**

Replace the `AnthropicContentBlock` enum:

```rust
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub(super) enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        cache_control: Option<CacheControl>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        cache_control: Option<CacheControl>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        #[serde(deserialize_with = "deserialize_tool_result_content")]
        content: Vec<AnthropicToolResultContent>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        is_error: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        cache_control: Option<CacheControl>,
    },
    #[serde(rename = "image")]
    Image { source: AnthropicImageSource },
    #[serde(rename = "audio")]
    Audio { source: AnthropicAudioSource },
    #[serde(rename = "document")]
    Document { source: AnthropicDocumentSource },
    #[serde(rename = "thinking")]
    ThinkingBlock { thinking: String },
}
```

Note: Only Text/ToolUse/ToolResult variants get `cache_control` (the variants Anthropic allows caching on). Image/Audio/Document/ThinkingBlock omitted for MVP.

- [ ] **Step 5: Update match arms in transform.rs**

In `crates/synthia-provider/src/anthropic/provider/transform.rs`, update all `AnthropicContentBlock::Text { text }` patterns to `AnthropicContentBlock::Text { text, cache_control: _ }` (or `..` if preferred). Specifically update `transform_part`:

```rust
ContentPart::Text(tc) => AnthropicContentBlock::Text {
    text: tc.text.clone(),
    cache_control: None,
},
```

And update `reorder_anthropic_messages` filter patterns:

```rust
.filter(|c| matches!(c, AnthropicContentBlock::ToolUse { .. }))
```

(Use `..` to ignore the new field — `matches!` already supports this.)

Also update `AnthropicContentBlock::ToolUse { id, name, input }` if any destructure exists — use `..` to ignore cache_control. Search for `AnthropicContentBlock::` in transform.rs and update all match arms.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p synthia-provider --lib anthropic::types::tests`
Expected: PASS (7 tests)

Run: `cargo check -p synthia-provider`
Expected: PASS (no compile errors in transform.rs)

- [ ] **Step 7: Commit**

```bash
git add crates/synthia-provider/src/anthropic/types.rs crates/synthia-provider/src/anthropic/provider/transform.rs
git commit -m "feat(provider): add cache_control field to AnthropicTool + AnthropicContentBlock variants"
```

---

## Task 3: Change AnthropicRequest.system type to Option<AnthropicSystem>

**Files:**
- Modify: `crates/synthia-provider/src/anthropic/types.rs`
- Modify: `crates/synthia-provider/src/anthropic/provider/transform.rs`

- [ ] **Step 1: Write failing test for backward-compatible system serialization**

Append to `types.rs` tests:

```rust
    #[test]
    fn anthropic_request_system_text_serializes_as_plain_string() {
        let req = AnthropicRequest {
            model: "claude-3".to_string(),
            system: Some(AnthropicSystem::Text("You are helpful.".to_string())),
            messages: vec![],
            max_tokens: 100,
            tools: None,
            temperature: None,
            stream: false,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["system"], serde_json::json!("You are helpful."));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p synthia-provider --lib anthropic::types::tests::anthropic_request_system_text_serializes_as_plain_string`
Expected: FAIL (system field type is `Option<String>`)

- [ ] **Step 3: Change AnthropicRequest.system field type**

In `crates/synthia-provider/src/anthropic/types.rs`, update `AnthropicRequest`:

```rust
#[derive(Debug, Serialize)]
pub(super) struct AnthropicRequest {
    pub(super) model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) system: Option<AnthropicSystem>,
    pub(super) messages: Vec<AnthropicMessage>,
    pub(super) max_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) temperature: Option<f64>,
    pub(super) stream: bool,
}
```

- [ ] **Step 4: Update transform_request to construct AnthropicSystem::Text**

In `crates/synthia-provider/src/anthropic/provider/transform.rs`, update `transform_request`:

```rust
AnthropicRequest {
    model: request.model.clone(),
    system: system_text.map(AnthropicSystem::Text),
    messages,
    max_tokens: request.max_tokens.unwrap_or(4096),
    tools: if tools.is_empty() { None } else { Some(tools) },
    temperature: request.temperature,
    stream: false,
}
```

This preserves backward compatibility — when no cache_policy, system is `Text(...)` which serializes as plain string.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p synthia-provider --lib anthropic::types::tests`
Expected: PASS (8 tests)

Run: `cargo check -p synthia-provider`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-provider/src/anthropic/types.rs crates/synthia-provider/src/anthropic/provider/transform.rs
git commit -m "refactor(provider): change AnthropicRequest.system to Option<AnthropicSystem>"
```

---

## Task 4: Create cache_policy.rs module with CachePolicy + apply_cache_policy

**Files:**
- Create: `crates/synthia-provider/src/cache_policy.rs`
- Modify: `crates/synthia-provider/src/lib.rs` (export module)
- Modify: `crates/synthia-provider/src/types/tool.rs` (add cache_control field to ToolDefinition)
- Modify: `crates/synthia-provider/src/types/content.rs` (add cache_control field to ContentPart::Text variant — for marking last user message at CompletionRequest level)

**Design note:** `apply_cache_policy` operates on `CompletionRequest` (provider-neutral). It marks the last `ToolDefinition` and the last user `ContentPart::Text` with a `cache_control: Option<CacheControl>` field. The actual injection into `AnthropicTool` / `AnthropicContentBlock` happens in `transform_request` which reads these markers.

This avoids polluting `CompletionRequest` with Anthropic-specific types — we use a provider-neutral `CacheControlMark` marker type in `cache_policy.rs`.

- [ ] **Step 1: Write failing tests for CachePolicy::default and apply_cache_policy**

Create `crates/synthia-provider/src/cache_policy.rs`:

```rust
//! Provider-neutral cache policy injection.
//!
//! Aligned with opencode's `applyCachePolicy` from `packages/llm/src/cache-policy.ts`.
//! Marks the last tool / system / user message with a `CacheControlMark` so that
//! provider `transform_request` implementations can translate the mark to
//! provider-specific `cache_control` hints (e.g. Anthropic `{"type":"ephemeral"}`).
//!
//! Marks are stored on `CompletionRequest` fields (ToolDefinition.cache_control,
//! ContentPart::Text.cache_control). Providers that don't support inline cache
//! hints (e.g. OpenAI) ignore the marks entirely.

use crate::types::{CompletionRequest, ContentPart, Message, Role};

/// Strategy for caching the message tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageCacheStrategy {
    None,
    LatestUserMessage,
}

/// Cache policy. Default aligns with opencode `AUTO`:
/// `tools: true, system: true, messages: LatestUserMessage, ttl_seconds: None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachePolicy {
    pub tools: bool,
    pub system: bool,
    pub messages: MessageCacheStrategy,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ttl_seconds: Option<u32>,
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            tools: true,
            system: true,
            messages: MessageCacheStrategy::LatestUserMessage,
            ttl_seconds: None,
        }
    }
}

/// Provider-neutral mark. Translated by provider `transform_request` to
/// provider-specific `cache_control` (e.g. Anthropic `{"type":"ephemeral"}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheControlMark {
    pub ttl_seconds: Option<u32>,
}

/// Apply cache policy to a `CompletionRequest` by marking the last tool /
/// last user message with `CacheControlMark`. Idempotent: calling twice with
/// the same (request, policy) produces identical state.
pub fn apply_cache_policy(
    request: &mut CompletionRequest,
    policy: &CachePolicy,
) {
    if policy.tools {
        if let Some(last_tool) = request.tools.last_mut() {
            last_tool.cache_control = Some(CacheControlMark {
                ttl_seconds: policy.ttl_seconds,
            });
        }
    }
    if policy.messages == MessageCacheStrategy::LatestUserMessage {
        mark_last_user_message(&mut request.messages, policy.ttl_seconds);
    }
    // system marking deferred to provider transform_request because
    // system is embedded as Role::System message in CompletionRequest.messages
    // and provider constructs AnthropicSystem from it.
}

fn mark_last_user_message(messages: &mut [Message], ttl_seconds: Option<u32>) {
    for msg in messages.iter_mut().rev() {
        if msg.role == Role::User {
            let parts: &mut Vec<ContentPart> = match &mut msg.content {
                crate::types::Content::Single(part) => {
                    // Single-part message: convert to Multi to mark the part
                    let taken = std::mem::replace(part, ContentPart::Text(crate::types::TextContent {
                        text: String::new(),
                    }));
                    msg.content = crate::types::Content::Multi(vec![taken,]);
                    if let crate::types::Content::Multi(v) = &mut msg.content {
                        v
                    } else {
                        unreachable!()
                    }
                }
                crate::types::Content::Multi(v) => v,
            };
            if let Some(last_part) = parts.last_mut() {
                set_cache_control_mark(last_part, ttl_seconds);
            }
            break;
        }
    }
}

fn set_cache_control_mark(part: &mut ContentPart, ttl_seconds: Option<u32>) {
    // ContentPart::Text variant carries cache_control field (added in Step 4)
    if let ContentPart::Text(tc) = part {
        tc.cache_control = Some(CacheControlMark { ttl_seconds });
    }
    // Other ContentPart variants (Image/Audio/ToolUse/ToolResult/Reasoning/Resource) omitted for MVP
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Content, TextContent, ToolDefinition};

    #[test]
    fn cache_policy_default_aligns_with_opencode_auto() {
        let p = CachePolicy::default();
        assert!(p.tools);
        assert!(p.system);
        assert_eq!(p.messages, MessageCacheStrategy::LatestUserMessage);
        assert_eq!(p.ttl_seconds, None);
    }

    fn make_request_with_tools(n: usize) -> CompletionRequest {
        CompletionRequest {
            model: "claude-3".to_string(),
            messages: vec![],
            tools: (0..n).map(|i| ToolDefinition::new(
                format!("tool_{}", i),
                format!("Tool {}", i),
                serde_json::json!({"type": "object"}),
            )).collect(),
            tool_choice: crate::types::ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        }
    }

    #[test]
    fn apply_policy_marks_last_tool_only() {
        let mut req = make_request_with_tools(3);
        let policy = CachePolicy { tools: true, system: false, messages: MessageCacheStrategy::None, ttl_seconds: None };
        apply_cache_policy(&mut req, &policy);
        assert!(req.tools[0].cache_control.is_none());
        assert!(req.tools[1].cache_control.is_none());
        assert!(req.tools[2].cache_control.is_some());
    }

    #[test]
    fn apply_policy_empty_tools_is_noop() {
        let mut req = make_request_with_tools(0);
        let policy = CachePolicy { tools: true, system: false, messages: MessageCacheStrategy::None, ttl_seconds: None };
        apply_cache_policy(&mut req, &policy);
        assert!(req.tools.is_empty());
    }

    #[test]
    fn apply_policy_all_disabled_is_noop() {
        let mut req = make_request_with_tools(3);
        let original = req.clone();
        let policy = CachePolicy { tools: false, system: false, messages: MessageCacheStrategy::None, ttl_seconds: None };
        apply_cache_policy(&mut req, &policy);
        assert_eq!(req.tools, original.tools);
    }

    #[test]
    fn apply_policy_is_idempotent() {
        let mut req = make_request_with_tools(3);
        let policy = CachePolicy::default();
        apply_cache_policy(&mut req, &policy);
        let after_first = req.clone();
        apply_cache_policy(&mut req, &policy);
        assert_eq!(req.tools, after_first.tools);
    }

    #[test]
    fn apply_policy_marks_last_user_message_text_part() {
        let mut req = make_request_with_tools(0);
        req.messages = vec![
            Message { role: Role::User, content: Content::Single(ContentPart::Text(TextContent { text: "hi".into(), cache_control: None })) },
            Message { role: Role::Assistant, content: Content::Single(ContentPart::Text(TextContent { text: "hello".into(), cache_control: None })) },
            Message { role: Role::User, content: Content::Multi(vec![
                ContentPart::Text(TextContent { text: "first".into(), cache_control: None }),
                ContentPart::Text(TextContent { text: "second".into(), cache_control: None }),
            ]) },
        ];
        let policy = CachePolicy { tools: false, system: false, messages: MessageCacheStrategy::LatestUserMessage, ttl_seconds: None };
        apply_cache_policy(&mut req, &policy);
        // Last user message is messages[2], its last part should be marked
        if let Content::Multi(parts) = &req.messages[2].content {
            assert!(parts.last().unwrap().cache_control().is_some());
        } else {
            panic!("expected Multi");
        }
    }
}
```

Add `use serde::{Deserialize, Serialize};` at the top of `cache_policy.rs`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p synthia-provider --lib cache_policy::tests`
Expected: FAIL (module not declared in lib.rs, ToolDefinition.cache_control field missing, TextContent.cache_control field missing, ContentPart::cache_control() helper missing)

- [ ] **Step 3: Add cache_control field to ToolDefinition**

In `crates/synthia-provider/src/types/tool.rs`:

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cache_policy::CacheControlMark;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cache_control: Option<CacheControlMark>,
}
```

Update `ToolDefinition::new`:

```rust
impl ToolDefinition {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            cache_control: None,
        }
    }
}
```

- [ ] **Step 4: Add cache_control field to TextContent**

In `crates/synthia-provider/src/types/content.rs`, find `TextContent` and add:

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TextContent {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cache_control: Option<CacheControlMark>,
}
```

Update all `TextContent { text: ... }` literals in the crate to include `cache_control: None` (search for `TextContent {` and update).

Add a helper method on `ContentPart`:

```rust
impl ContentPart {
    /// Get the cache_control mark if this part is a Text variant with a mark.
    pub fn cache_control(&self) -> Option<&CacheControlMark> {
        match self {
            ContentPart::Text(tc) => tc.cache_control.as_ref(),
            _ => None,
        }
    }
}
```

- [ ] **Step 5: Export cache_policy module from lib.rs**

In `crates/synthia-provider/src/lib.rs`, add:

```rust
pub mod cache_policy;

pub use cache_policy::{CacheControlMark, CachePolicy, MessageCacheStrategy, apply_cache_policy};
```

Place `pub mod cache_policy;` BEFORE `pub mod types;` (types.rs depends on cache_policy::CacheControlMark).

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p synthia-provider --lib cache_policy::tests`
Expected: PASS (6 tests)

Run: `cargo check -p synthia-provider`
Expected: PASS (fix any remaining `TextContent { text: ... }` literals missing `cache_control: None`)

- [ ] **Step 7: Commit**

```bash
git add crates/synthia-provider/src/cache_policy.rs crates/synthia-provider/src/lib.rs crates/synthesis-provider/src/types/tool.rs crates/synthia-provider/src/types/content.rs
git commit -m "feat(provider): add cache_policy module with apply_cache_policy"
```

---

## Task 5: Add cache_policy field to CompletionRequest

**Files:**
- Modify: `crates/synthia-provider/src/types/completion.rs`

- [ ] **Step 1: Write failing test for cache_policy field omission**

Append to `crates/synthia-provider/src/types/tests.rs` (or create inline test in completion.rs):

```rust
    #[test]
    fn completion_request_without_cache_policy_omits_field() {
        let req = CompletionRequest {
            model: "claude-3".to_string(),
            messages: vec![],
            tools: vec![],
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("cache_policy").is_none());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p synthia-provider --lib completion_request_without_cache_policy_omits_field`
Expected: FAIL (no `cache_policy` field on CompletionRequest)

- [ ] **Step 3: Add cache_policy field**

In `crates/synthia-provider/src/types/completion.rs`:

```rust
use crate::cache_policy::CachePolicy;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub tool_choice: ToolChoice,
    pub temperature: Option<f64>,
    pub max_tokens: Option<usize>,
    pub stop_sequences: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub extra_body: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cache_policy: Option<CachePolicy>,
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p synthia-provider --lib`
Expected: PASS

Run: `cargo check --workspace`
Expected: PASS (fix any CompletionRequest literals missing `cache_policy: None`)

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-provider/src/types/completion.rs
git commit -m "feat(provider): add cache_policy field to CompletionRequest"
```

---

## Task 6: Add supports_inline_cache_hints to ModelProvider trait

**Files:**
- Modify: `crates/synthia-provider/src/traits.rs`
- Modify: `crates/synthia-provider/src/anthropic/provider/core.rs`

- [ ] **Step 1: Write failing test for trait method**

In `crates/synthia-provider/src/tests.rs` (or inline in traits.rs):

```rust
    #[test]
    fn anthropic_provider_supports_inline_cache_hints() {
        use crate::anthropic::AnthropicProvider;
        use crate::types::ModelConfig;
        let provider = AnthropicProvider::new(ModelConfig::default());
        assert!(provider.supports_inline_cache_hints());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p synthia-provider --lib anthropic_provider_supports_inline_cache_hints`
Expected: FAIL (no method `supports_inline_cache_hints`)

- [ ] **Step 3: Add default method to ModelProvider trait**

In `crates/synthia-provider/src/traits.rs`, add to the `ModelProvider` trait:

```rust
    /// Whether this provider supports inline `cache_control` hints
    /// (Anthropic, Bedrock Converse). Providers using implicit prefix
    /// caching (OpenAI) return `false`.
    fn supports_inline_cache_hints(&self) -> bool { false }
```

- [ ] **Step 4: Override in AnthropicProvider**

In `crates/synthia-provider/src/anthropic/provider/core.rs`, add impl block (or extend existing):

```rust
use crate::traits::ModelProvider;

impl ModelProvider for AnthropicProvider {
    // ... existing methods unchanged ...
    fn supports_inline_cache_hints(&self) -> bool { true }
}
```

If `ModelProvider` is already implemented elsewhere for `AnthropicProvider`, find the impl block and add only the override method.

Search for `impl ModelProvider for AnthropicProvider` and add the method inside.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p synthia-provider --lib anthropic_provider_supports_inline_cache_hints`
Expected: PASS

Run: `cargo check -p synthia-provider`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-provider/src/traits.rs crates/synthia-provider/src/anthropic/provider/core.rs
git commit -m "feat(provider): add supports_inline_cache_hints to ModelProvider trait"
```

---

## Task 7: Integrate apply_cache_policy in transform_request

**Files:**
- Modify: `crates/synthia-provider/src/anthropic/provider/transform.rs`

- [ ] **Step 1: Write failing integration test for None cache_policy (backward compat)**

In `crates/synthia-provider/src/tests.rs`:

```rust
    #[test]
    fn transform_request_with_none_cache_policy_preserves_text_system() {
        use crate::anthropic::AnthropicProvider;
        use crate::types::{Content, ModelConfig, TextContent};

        let provider = AnthropicProvider::new(ModelConfig::default());
        let req = CompletionRequest {
            model: "claude-3".to_string(),
            messages: vec![
                Message { role: Role::System, content: Content::Single(ContentPart::Text(TextContent { text: "You are helpful.".into(), cache_control: None })) },
                Message { role: Role::User, content: Content::Single(ContentPart::Text(TextPart { text: "hi".into(), cache_control: None })) },
            ],
            tools: vec![],
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        };
        let anthropic_req = provider.transform_request(&req);
        // system should be Text variant (serializes as plain string)
        let sys_json = serde_json::to_value(&anthropic_req.system).unwrap();
        assert_eq!(sys_json, serde_json::json!("You are helpful."));
    }
```

Note: `transform_request` is `pub(in crate::anthropic)` — test may need to be in `anthropic::provider::tests` module or method exposed via `pub(crate)`. Check current visibility and adjust.

- [ ] **Step 2: Write failing integration test for Some cache_policy injection**

```rust
    #[test]
    fn transform_request_with_some_cache_policy_injects_cache_control_on_last_tool() {
        use crate::anthropic::AnthropicProvider;
        use crate::cache_policy::CachePolicy;
        use crate::types::ModelConfig;

        let provider = AnthropicProvider::new(ModelConfig::default());
        let req = CompletionRequest {
            model: "claude-3".to_string(),
            messages: vec![],
            tools: vec![
                ToolDefinition::new("tool_a", "A", serde_json::json!({})),
                ToolDefinition::new("tool_b", "B", serde_json::json!({})),
            ],
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: Some(CachePolicy::default()),
        };
        let anthropic_req = provider.transform_request(&req);
        let tools = anthropic_req.tools.unwrap();
        assert!(tools[0].cache_control.is_none());
        assert!(tools[1].cache_control.is_some());
    }

    #[test]
    fn transform_request_with_some_cache_policy_injects_structured_system() {
        use crate::anthropic::AnthropicProvider;
        use crate::cache_policy::CachePolicy;
        use crate::types::{Content, ModelConfig, TextContent};

        let provider = AnthropicProvider::new(ModelConfig::default());
        let req = CompletionRequest {
            model: "claude-3".to_string(),
            messages: vec![
                Message { role: Role::System, content: Content::Single(ContentPart::Text(TextContent { text: "You are helpful.".into(), cache_control: None })) },
            ],
            tools: vec![],
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: Some(CachePolicy::default()),
        };
        let anthropic_req = provider.transform_request(&req);
        // system should be Structured variant with cache_control on last block
        let sys_json = serde_json::to_value(&anthropic_req.system).unwrap();
        assert!(sys_json.is_array());
        assert_eq!(sys_json[0]["cache_control"]["type"], "ephemeral");
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p synthia-provider --lib transform_request_with_`
Expected: FAIL (transform_request doesn't read cache_policy)

- [ ] **Step 4: Update transform_request to apply policy**

In `crates/synthia-provider/src/anthropic/provider/transform.rs`, update `transform_request`:

```rust
pub(in crate::anthropic) fn transform_request(
    &self,
    request: &CompletionRequest,
) -> AnthropicRequest {
    // Clone request so we can apply policy mutably without modifying caller's request
    let mut request = request.clone();

    // Apply cache policy if present (provider-aware: only Anthropic supports inline hints,
    // and this code path is only reached for AnthropicProvider, so no extra gate needed)
    if let Some(ref policy) = request.cache_policy {
        crate::cache_policy::apply_cache_policy(&mut request, policy);
    }

    let system_text = request
        .messages
        .iter()
        .find(|m| m.role == Role::System)
        .and_then(|m| m.content.extract_text());

    let messages: Vec<AnthropicMessage> = request
        .messages
        .iter()
        .filter(|m| m.role != Role::System)
        .filter_map(|m| self.transform_message(m))
        .collect();

    let messages = Self::reorder_anthropic_messages(messages);

    let system = build_anthropic_system(system_text, request.cache_policy.as_ref());

    let tools: Vec<AnthropicTool> = request
        .tools
        .iter()
        .map(|t| AnthropicTool {
            name: t.name.clone(),
            description: t.description.clone(),
            input_schema: t.input_schema.clone(),
            cache_control: t.cache_control.as_ref().map(|mark| AnthropicCacheControl {
                r#type: "ephemeral".to_string(),
                ttl_seconds: mark.ttl_seconds,
            }),
        })
        .collect();

    AnthropicRequest {
        model: request.model.clone(),
        system,
        messages,
        max_tokens: request.max_tokens.unwrap_or(4096),
        tools: if tools.is_empty() { None } else { Some(tools) },
        temperature: request.temperature,
        stream: false,
    }
}

/// Build AnthropicSystem. If policy is Some and policy.system == true, use Structured
/// variant with cache_control. Otherwise use Text variant (backward compatible).
fn build_anthropic_system(
    system_text: Option<String>,
    cache_policy: Option<&crate::cache_policy::CachePolicy>,
) -> Option<AnthropicSystem> {
    let text = system_text?;
    let use_structured = cache_policy
        .map(|p| p.system)
        .unwrap_or(false);
    if use_structured {
        Some(AnthropicSystem::Structured(vec![AnthropicSystemBlock {
            r#type: "text".to_string(),
            text,
            cache_control: Some(CacheControl::default()),
        }]))
    } else {
        Some(AnthropicSystem::Text(text))
    }
}
```

Add `use super::super::types::{AnthropicSystem, AnthropicSystemBlock, CacheControl};` to the imports at top of transform.rs (extend existing import block).

- [ ] **Step 5: Update transform_message to propagate cache_control from ContentPart to AnthropicContentBlock**

In `transform.rs`, update `transform_part`:

```rust
pub(in crate::anthropic) fn transform_part(
    &self,
    part: &ContentPart,
) -> AnthropicContentBlock {
    let cache_control = part.cache_control().map(|mark| CacheControl {
        r#type: "ephemeral".to_string(),
        ttl_seconds: mark.ttl_seconds,
    });
    match part {
        ContentPart::Text(tc) => AnthropicContentBlock::Text {
            text: tc.text.clone(),
            cache_control,
        },
        // ... other variants unchanged (cache_control only attached to Text for MVP)
        // For ToolUse / ToolResult, ignore cache_control mark (not on user message tail)
        ContentPart::ToolUse(tu) => AnthropicContentBlock::ToolUse {
            id: Self::sanitize_tool_id(&tu.id),
            name: tu.name.clone(),
            input: tu.input.clone(),
            cache_control: None,
        },
        ContentPart::ToolResult(tr) => AnthropicContentBlock::ToolResult {
            tool_use_id: Self::sanitize_tool_id(&tr.tool_use_id),
            content: vec![AnthropicToolResultContent {
                r#type: "text".to_string(),
                text: if tr.is_error.unwrap_or(false) {
                    format!("Error: {:?}", tr.content)
                } else {
                    format!("{:?}", tr.content)
                },
            }],
            is_error: tr.is_error,
            cache_control: None,
        },
        // ... other variants (Image/Audio/Reasoning/Resource) unchanged
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p synthia-provider --lib transform_request_with_`
Expected: PASS (3 tests)

Run: `cargo test -p synthia-provider --lib`
Expected: PASS (all tests)

- [ ] **Step 7: Commit**

```bash
git add crates/synthia-provider/src/anthropic/provider/transform.rs
git commit -m "feat(provider): integrate apply_cache_policy in transform_request"
```

---

## Task 8: Final verification and cleanup

**Files:** None (verification only)

- [ ] **Step 1: Run cargo fmt**

Run: `cargo +nightly fmt --all`
Expected: no diff (or auto-format minor whitespace)

- [ ] **Step 2: Run cargo clippy**

Run: `cargo clippy --all-targets --all-features --tests --all`
Expected: zero warnings (fix any warnings by following clippy suggestions)

- [ ] **Step 3: Run cargo test workspace**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 4: Run cargo check workspace**

Run: `cargo check --workspace`
Expected: PASS (no breaking changes leaked to other crates)

- [ ] **Step 5: Verify no public API breakage**

Run: `cargo check --workspace` and grep for `AnthropicRequest.system` external usage:

```bash
rg "AnthropicRequest" --type rust | grep -v "crates/synthia-provider/src/anthropic/"
```

Expected: no matches outside `synthia-provider` crate (AnthropicRequest is `pub(super)` — internal).

- [ ] **Step 6: Commit any fmt/clippy fixes**

```bash
git add -A
git commit -m "chore(provider): fmt + clippy cleanup for cache_policy injection"
```

---

## Self-Review Notes

**Spec coverage check:**
- ✅ CachePolicy struct (4 fields + Default) → Task 4
- ✅ apply_cache_policy idempotency → Task 4 (test + impl)
- ✅ AnthropicProvider transform_request integration → Task 7
- ✅ supports_inline_cache_hints trait method → Task 6
- ✅ CacheControl serialization → Task 1
- ✅ AnthropicSystem enum (Text + Structured) → Task 1 + Task 3
- ✅ AnthropicTool / AnthropicContentBlock cache_control field → Task 2
- ✅ Backward compatibility (None cache_policy) → Task 7 Step 1 test
- ✅ Provider-aware no-op when supports_inline_cache_hints == false → implicit (transform_request is only called for Anthropic; OpenAI path doesn't go through this code)

**Open design notes:**
- `apply_cache_policy` operates on `CompletionRequest` (provider-neutral) using `CacheControlMark`. The Anthropic translation to `CacheControl {"type":"ephemeral"}` happens in `transform_request`. This avoids leaking Anthropic types into the provider-neutral `cache_policy.rs` module.
- `system` marking is deferred to `transform_request` because system text is embedded in `messages[Role::System]` and only extracted during transform. The `policy.system` flag is read directly in `build_anthropic_system`.
- MVP only marks `ContentPart::Text` on last user message. Image/Audio/ToolUse/ToolResult variants are not marked. If the last part of the last user message is non-Text, no cache_control is attached to that message (acceptable for MVP — opencode also focuses on text).
