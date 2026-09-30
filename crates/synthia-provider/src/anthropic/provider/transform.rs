//! The request- and message-transformation methods on
//! [`super::core::AnthropicProvider`]:
//!
//! - [`AnthropicProvider::transform_request`] — top-level
//!   dispatcher that takes a
//!   [`crate::types::CompletionRequest`] and produces an
//!   [`super::super::types::AnthropicRequest`].
//! - [`AnthropicProvider::transform_message`] — maps a
//!   [`crate::types::Message`] to an
//!   [`super::super::types::AnthropicMessage`].
//! - [`AnthropicProvider::transform_part`] — maps a
//!   [`crate::types::ContentPart`] to an
//!   [`super::super::types::AnthropicContentBlock`].
//! - [`AnthropicProvider::reorder_anthropic_messages`] —
//!   reorders assistant messages so that text blocks
//!   precede tool-use blocks.
//! - [`AnthropicProvider::sanitize_tool_id`] — replaces
//!   non-alphanumeric / non-underscore / non-hyphen chars
//!   with `_`.

use super::{
    super::types::{
        AnthropicAudioSource,
        AnthropicContentBlock,
        AnthropicImageSource,
        AnthropicMessage,
        AnthropicRequest,
        AnthropicSystem,
        AnthropicSystemBlock,
        AnthropicTool,
        AnthropicToolResultContent,
        CacheControl,
    },
    core::AnthropicProvider,
};
use crate::{
    cache_mark::{CacheControlMark, CacheScope, CacheTtl},
    types::{CompletionRequest, Content, ContentPart, Role},
};

/// Map the provider-neutral [`CacheTtl`] class to an Anthropic
/// `ttl_seconds` value. `Ephemeral` defers to Anthropic's default (no
/// explicit `ttl_seconds`); `Extended` requests 5 minutes and `Long`
/// requests 1 hour.
fn ttl_seconds_from_ttl(ttl: CacheTtl) -> Option<u32> {
    match ttl {
        CacheTtl::Ephemeral => None,
        CacheTtl::Extended => Some(300),
        CacheTtl::Long => Some(3600),
    }
}

/// Translate a provider-neutral [`CacheControlMark`] to the Anthropic
/// [`CacheControl`] wire type, including the `scope.0` value as a
/// `cache_namespace` field so two different users with otherwise identical
/// prompts produce distinct `cache_control` JSON (per the cross-session
/// cache leakage prevention requirement).
///
/// The namespace is only emitted when the scope differs from
/// [`CacheScope::default()`]; this keeps the anonymous-default path
/// byte-identical to the pre-change wire format (`{"type": "ephemeral"}`).
fn cache_control_from_mark(mark: &CacheControlMark) -> CacheControl {
    let cache_namespace = if mark.scope == CacheScope::default() {
        None
    } else {
        Some(mark.scope.0.clone())
    };
    CacheControl {
        r#type: "ephemeral".to_string(),
        ttl_seconds: ttl_seconds_from_ttl(mark.ttl),
        cache_namespace,
    }
}

/// Extract a representative [`CacheControlMark`] from `request` — the first
/// mark found scanning the last tool then the last user message in reverse.
/// Used to propagate the cache scope to the system block, which is marked
/// by the provider (not by `apply_cache_policy`) and otherwise has no mark
/// of its own.
fn representative_cache_mark(
    request: &CompletionRequest,
) -> Option<&CacheControlMark> {
    if let Some(mark) = request
        .tools
        .iter()
        .rev()
        .find_map(|t| t.cache_control.as_ref())
    {
        return Some(mark);
    }
    for msg in request.messages.iter().rev() {
        for part in &msg.content {
            if let Some(mark) = part.cache_control() {
                return Some(mark);
            }
        }
    }
    None
}

impl AnthropicProvider {
    pub(in crate::anthropic) fn transform_request(
        &self,
        request: &CompletionRequest,
    ) -> AnthropicRequest {
        // Clone so we can apply policy mutably without modifying the
        // caller's request. When `cache_policy` is `None` the clone is
        // byte-identical to the original, preserving backward-compatible
        // output (Text system variant, no cache_control fields anywhere).
        let mut request = request.clone();

        // Apply cache policy (marks last tool + last user message) when
        // present. System marking is deferred to `build_anthropic_system`
        // because the system text is embedded in a `Role::System` message
        // and is only extracted during this provider-specific transform.
        // The policy is cloned out first to avoid borrowing `request`
        // immutably while we need a mutable borrow to apply the marks.
        if let Some(policy) = request.cache_policy.clone() {
            // Resolve the effective cache policy through the optional
            // deployment-supplied hook (R62, pi
            // `cache_warming_decision` parity). Identity when no hook
            // is installed.
            let effective = crate::cache_policy::resolve_cache_policy(
                self.cache_strategy_hook.as_deref(),
                &request,
                &policy,
            );
            // `apply` short-circuits (returns `true`, skips
            // `apply_cache_policy`) when `tools` / `messages` `Arc`
            // references are identical to the previous call — the
            // cache_control marks from the prior call are still present.
            // Otherwise it performs full evaluation and stores the new
            // references for the next call.
            self.cache_policy_applier
                .lock()
                .apply(&mut request, &effective);
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

        // The system block is marked by the provider (not by
        // `apply_cache_policy`); propagate the scope from a representative
        // mark so the system cache entry is namespaced identically to the
        // tool / message cache entries.
        let representative_mark = representative_cache_mark(&request);
        let system = build_anthropic_system(
            system_text,
            request.cache_policy.as_ref(),
            representative_mark,
        );

        let tools: Vec<AnthropicTool> = request
            .tools
            .iter()
            .map(|t| {
                let cache_control =
                    t.cache_control.as_ref().map(cache_control_from_mark);
                AnthropicTool {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    input_schema: t.input_schema.clone(),
                    cache_control,
                }
            })
            .collect();

        AnthropicRequest {
            // Fall back to the provider-configured model name when
            // the caller leaves `request.model` empty.
            model: if request.model.is_empty() {
                self.model_config.name.clone()
            } else {
                request.model.clone()
            },
            system,
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            tools: if tools.is_empty() { None } else { Some(tools) },
            temperature: request.temperature,
            stream: false,
        }
    }

    fn reorder_anthropic_messages(
        messages: Vec<AnthropicMessage>,
    ) -> Vec<AnthropicMessage> {
        let mut result: Vec<AnthropicMessage> = Vec::new();

        for msg in messages {
            if msg.role != "assistant" {
                // Consecutive tool-result turns merge into ONE user
                // message — the protocol's own documented shape for
                // answering several calls at once. The harness commits one
                // `Message::tool` per result (`commit.rs`), so a turn that
                // asked for two tools in parallel arrives as two adjacent
                // user messages.
                //
                // This is NOT a rejection guard: the API documents that
                // "consecutive `user` or `assistant` turns in your request
                // will be combined into a single turn", so the unmerged form
                // is accepted as well. Emitting the merged shape avoids
                // depending on that combining, and matches the example the
                // tool-use docs give for parallel calls.
                //
                // Only tool-result-only turns merge: a genuine user turn
                // that follows another user turn keeps its own message, so
                // the transcript's turn boundaries are not rewritten.
                let incoming_is_results = !msg.content.is_empty()
                    && msg.content.iter().all(|block| {
                        matches!(
                            block,
                            AnthropicContentBlock::ToolResult { .. }
                        )
                    });
                if incoming_is_results
                    && let Some(last) = result.last_mut()
                    && last.role == "user"
                    && last.content.iter().all(|block| {
                        matches!(
                            block,
                            AnthropicContentBlock::ToolResult { .. }
                        )
                    })
                {
                    last.content.extend(msg.content);
                    continue;
                }
                result.push(msg);
                continue;
            }

            let tool_use_blocks: Vec<_> = msg
                .content
                .iter()
                .filter(|c| matches!(c, AnthropicContentBlock::ToolUse { .. }))
                .cloned()
                .collect();

            let other_blocks: Vec<_> = msg
                .content
                .iter()
                .filter(|c| !matches!(c, AnthropicContentBlock::ToolUse { .. }))
                .cloned()
                .collect();

            if !tool_use_blocks.is_empty() && !other_blocks.is_empty() {
                // Split into two messages: text first, then tool_use
                if !other_blocks.is_empty() {
                    result.push(AnthropicMessage {
                        role: msg.role.clone(),
                        content: other_blocks,
                    });
                }
                result.push(AnthropicMessage {
                    role: msg.role.clone(),
                    content: tool_use_blocks,
                });
            } else {
                result.push(msg);
            }
        }

        result
    }

    fn sanitize_tool_id(id: &str) -> String {
        id.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    pub(in crate::anthropic) fn transform_message(
        &self,
        msg: &crate::types::Message,
    ) -> Option<AnthropicMessage> {
        let role = match msg.role {
            Role::User => "user".to_string(),
            Role::Assistant => "assistant".to_string(),
            Role::Tool => "user".to_string(),
            Role::System => return None,
        };

        let mut content: Vec<AnthropicContentBlock> = match &msg.content {
            Content::Single(part) => {
                self.transform_part(part).into_iter().collect()
            }
            Content::Multi(parts) => parts
                .iter()
                .filter_map(|p| self.transform_part(p))
                .collect(),
        };

        // A tool message whose result arrived UNWRAPPED — the shape the
        // harness commits (`Message::tool(Content::parts(tr.content),
        // tr.call_id)`) — projects to plain `text`/`image` blocks with no
        // `tool_result` anywhere, and `tool_call_id` would be dropped.
        //
        // That is not merely lossy: Anthropic requires every `tool_use`
        // block to be answered by a `tool_result` immediately after it,
        // and rejects the request with "`tool_use` ids were found without
        // `tool_result` blocks" otherwise. So the id is re-attached and
        // the parts are re-projected through the tool-result vocabulary,
        // mirroring the OpenAI adapter's
        // `msg.tool_call_id.clone().or(tool_use_id)`.
        let already_a_result = content
            .iter()
            .any(|b| matches!(b, AnthropicContentBlock::ToolResult { .. }));
        if msg.role == Role::Tool
            && !already_a_result
            && let Some(call_id) = msg.tool_call_id.as_deref()
        {
            let parts: Vec<crate::types::ContentPart> =
                msg.content.iter().cloned().collect();
            let wrapped = crate::types::ToolResult {
                tool_use_id: call_id.to_string(),
                tool_name: msg.name.clone(),
                content: parts,
                structured_content: None,
                is_error: None,
                truncated_by: None,
                metadata: serde_json::Map::new(),
            };
            content = vec![AnthropicContentBlock::ToolResult {
                tool_use_id: Self::sanitize_tool_id(call_id),
                content: tool_result_blocks(&wrapped),
                is_error: None,
                cache_control: None,
            }];
        }

        // Nothing survived the projection — the message has no wire
        // representation, and an empty `content` array is not a legal
        // Anthropic message. Mirrors the OpenAI-compatible adapter.
        if content.is_empty() {
            return None;
        }

        Some(AnthropicMessage { role, content })
    }

    /// Project one canonical part onto the Anthropic wire vocabulary, or
    /// `None` when this API has no valid slot for it.
    ///
    /// Only an **unsigned** [`ContentPart::Reasoning`] is such a part.
    /// Anthropic requires a `thinking` block to carry its `signature` —
    /// that is exactly what the signature is for, and why the provider
    /// crate documents it as "Required to preserve reasoning continuity
    /// across turns". Sending `signature: None` produces a request
    /// Anthropic rejects, so the block is dropped instead.
    ///
    /// The unsigned case is the *reachable* one: `reasoning_content`
    /// gateways and `<think>`-marker-compatible upstreams emit reasoning
    /// without a signature, whereas a real Anthropic `signature_delta`
    /// only appears once extended thinking is enabled. Dropping here
    /// means an unsigned part can never become an invalid request,
    /// whichever upstream produced it — while a signed part, the one
    /// Anthropic actually needs back, is still carried.
    ///
    /// [`ContentPart::Reasoning`]: crate::types::ContentPart::Reasoning
    pub(in crate::anthropic) fn transform_part(
        &self,
        part: &ContentPart,
    ) -> Option<AnthropicContentBlock> {
        // Translate the provider-neutral `CacheControlMark` (only present on
        // `Text` parts per MVP) to the Anthropic `CacheControl` hint,
        // propagating the mark's `scope` as a `cache_namespace` field. Other
        // variants get `cache_control: None` — the mark is only ever set on
        // user-message Text parts by `apply_cache_policy`.
        let cache_control = part.cache_control().map(cache_control_from_mark);
        // An EMPTY text block is rejected by Anthropic, in every position.
        // It is reachable from a legitimately-persisted turn: the composer
        // lets the user send attachments with no text, which writes
        // `{"text":""}` to the log, and the next turn replays it as a
        // `Text("")` part. Dropping it here — rather than at each producer
        // — keeps every caller honest, and when the drop leaves a message
        // with no parts the existing `content.is_empty()` guard in
        // `transform_message` already turns it into "nothing to send".
        if let ContentPart::Text(tc) = part
            && tc.text.is_empty()
        {
            return None;
        }
        let block = match part {
            ContentPart::Text(tc) => AnthropicContentBlock::Text {
                text: tc.text.clone(),
                cache_control,
            },
            ContentPart::Reasoning(rc) => {
                let signature = rc.signature.clone()?;
                AnthropicContentBlock::ThinkingBlock {
                    thinking: rc.text.clone(),
                    signature: Some(signature),
                }
            }
            ContentPart::Image(ic) => AnthropicContentBlock::Image {
                source: image_source(ic),
            },
            ContentPart::Audio(ac) => {
                let remote = ac.is_remote_url();
                AnthropicContentBlock::Audio {
                    source: AnthropicAudioSource {
                        r#type: if remote {
                            "url".to_string()
                        } else {
                            "base64".to_string()
                        },
                        media_type: ac.mime_type.clone(),
                        data: if remote {
                            ac.data.clone()
                        } else {
                            ac.inline_base64().unwrap_or_default().to_string()
                        },
                        format: ac.format_label().map(str::to_string),
                    },
                }
            }
            ContentPart::ToolUse(tu) => AnthropicContentBlock::ToolUse {
                id: Self::sanitize_tool_id(&tu.id),
                // `name` is `1 <= len <= 200` and `input` is
                // `map[unknown]` on this wire; both are shaped by the
                // shared helpers so the same canonical part cannot be
                // valid here and rejected by the OpenAI adapter (or the
                // reverse).
                name: tu.wire_name(),
                input: tu.wire_input(),
                cache_control: None,
            },
            ContentPart::ToolResult(tr) => AnthropicContentBlock::ToolResult {
                tool_use_id: Self::sanitize_tool_id(&tr.tool_use_id),
                content: tool_result_blocks(tr),
                is_error: tr.is_error,
                cache_control: None,
            },
            ContentPart::Resource(link) => AnthropicContentBlock::Text {
                text: link.prompt_projection(),
                cache_control: None,
            },
        };
        Some(block)
    }
}

/// Map a provider [`ImageContent`](crate::types::ImageContent) onto
/// Anthropic's `image.source`.
///
/// Remote vs inline is decided by the URL scheme alone (see
/// [`ImageContent::is_remote_url`](crate::types::ImageContent::is_remote_url)).
/// The previous implementation also
/// treated any string shorter than 1000 characters as base64, which
/// silently mis-classifies a short remote URL and — the dangerous
/// direction — classifies a real (tens-of-kilobytes) base64 payload as
/// a URL, producing an `image.source.type = "url"` block the API
/// rejects.
fn image_source(ic: &crate::types::ImageContent) -> AnthropicImageSource {
    if ic.is_remote_url() {
        AnthropicImageSource::Url {
            url: ic.data.clone(),
        }
    } else {
        AnthropicImageSource::Base64 {
            media_type: ic.mime_type.clone(),
            data: ic.inline_base64().unwrap_or_default().to_string(),
        }
    }
}

/// Map a tool result's content parts onto Anthropic's
/// `tool_result.content` blocks.
///
/// `Text` and `Image` survive as themselves — Anthropic renders an
/// image block inside a tool result, which is how a screenshot,
/// chart, or OCR tool returns something the model can see. Anything
/// else (audio, a nested tool result, a resource link) is
/// Debug-formatted into a text block, and an **error** result keeps
/// the single `Error: {:?}` text block it has always produced.
///
/// An EMPTY text part becomes a placeholder rather than being shipped
/// as-is: the API's `text` field has `minLength: 1`, so a tool that
/// returned nothing would otherwise make the whole request invalid. The
/// placeholder matches the vocabulary the built-in shell tool already
/// prints for the same situation.
fn tool_result_blocks(
    tr: &crate::types::ToolResult,
) -> Vec<AnthropicToolResultContent> {
    if tr.is_error.unwrap_or(false) {
        return vec![AnthropicToolResultContent::Text {
            text: format!("Error: {:?}", tr.content),
        }];
    }
    // A result with no parts at all is the same hole from the other
    // direction: `content: []` carries nothing for the model to read.
    if tr.content.is_empty() {
        return vec![AnthropicToolResultContent::Text {
            text: EMPTY_TOOL_RESULT.to_string(),
        }];
    }
    tr.content
        .iter()
        .map(|part| match part {
            ContentPart::Text(tc) if tc.text.is_empty() => {
                AnthropicToolResultContent::Text {
                    text: EMPTY_TOOL_RESULT.to_string(),
                }
            }
            ContentPart::Text(tc) => AnthropicToolResultContent::Text {
                text: tc.text.clone(),
            },
            ContentPart::Image(ic) => AnthropicToolResultContent::Image {
                source: image_source(ic),
            },
            other => AnthropicToolResultContent::Text {
                text: format!("{other:?}"),
            },
        })
        .collect()
}

/// What the model is told when a tool returned no text. Matches the
/// wording the built-in shell tool prints for a command with no output.
const EMPTY_TOOL_RESULT: &str = "(no output)";

/// Build the Anthropic `system` field from the extracted system text and
/// the cache policy.
///
/// When `cache_policy` is `None` (or `policy.system == false`) the `Text`
/// variant is used, which serializes as a plain JSON string — preserving
/// byte-identical backward-compatible output. When `policy.system == true`
/// the `Structured` variant is used so a `cache_control` hint can be
/// attached to the (single) system block, marking the cache prefix
/// boundary.
///
/// `representative_mark` propagates the cache scope (carried on tool /
/// message marks by `apply_cache_policy`) to the system block so its
/// `cache_control` is namespaced identically. When `None` (no marks exist,
/// e.g. empty request) [`CacheControl::default()`] is used.
fn build_anthropic_system(
    system_text: Option<String>,
    cache_policy: Option<&crate::cache_policy::CachePolicy>,
    representative_mark: Option<&CacheControlMark>,
) -> Option<AnthropicSystem> {
    // An EMPTY system prompt is treated as no system prompt at all, because
    // the protocol's `system` text carries the same `minLength: 1` as every
    // other text block — `{"type":"text","text":""}` is rejected.
    //
    // Reachable rather than theoretical: `Content::extract_text` joins the
    // text parts, so a single empty `Text` part yields `Some("")` (the vec
    // is `[""]`, not empty), and this function would otherwise pass that
    // straight through. Omitting the field is also the honest encoding —
    // an empty prompt and no prompt mean the same thing to the model.
    let text = system_text.filter(|t| !t.is_empty())?;
    let use_structured = cache_policy.map(|p| p.system).unwrap_or(false);
    if use_structured {
        let cache_control = representative_mark
            .map(cache_control_from_mark)
            .unwrap_or_default();
        Some(AnthropicSystem::Structured(vec![AnthropicSystemBlock {
            r#type: "text".to_string(),
            text,
            cache_control: Some(cache_control),
        }]))
    } else {
        Some(AnthropicSystem::Text(text))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    use super::*;
    use crate::{
        ModelConfig,
        ToolUse,
        types::{
            AudioFormat,
            Message,
            TextContent,
            ToolChoice,
            ToolDefinition,
        },
    };

    /// `cache_control_from_mark` is the
    /// provider-neutral → Anthropic-cache-control
    /// bridge. Pinning its behavior here keeps the
    /// system-block cache marking deterministic
    /// regardless of how `apply_cache_policy` evolves.
    #[test]
    fn cache_control_from_mark_default_scope_has_no_cache_namespace() {
        let mark = CacheControlMark {
            ttl: CacheTtl::Ephemeral,
            scope: CacheScope::default(),
            pinned: false,
        };
        let cc = cache_control_from_mark(&mark);
        assert_eq!(cc.r#type, "ephemeral");
        assert!(cc.ttl_seconds.is_none());
        // Default scope MUST collapse to `None` so
        // the wire format stays byte-identical to
        // the pre-`cache_namespace` era.
        assert!(cc.cache_namespace.is_none());
    }

    #[test]
    fn cache_control_from_mark_non_default_scope_propagates() {
        let mark = CacheControlMark {
            ttl: CacheTtl::Extended,
            scope: CacheScope("tenant-42".to_string()),
            pinned: false,
        };
        let cc = cache_control_from_mark(&mark);
        assert_eq!(cc.r#type, "ephemeral");
        assert_eq!(cc.ttl_seconds, Some(300));
        assert_eq!(
            cc.cache_namespace.as_deref(),
            Some("tenant-42"),
            "non-default scope must propagate into cache_namespace"
        );
    }

    #[test]
    fn ttl_seconds_from_ttl_class_mapping() {
        assert_eq!(ttl_seconds_from_ttl(CacheTtl::Ephemeral), None);
        assert_eq!(ttl_seconds_from_ttl(CacheTtl::Extended), Some(300));
        assert_eq!(ttl_seconds_from_ttl(CacheTtl::Long), Some(3600));
    }

    /// `representative_cache_mark` is the fallback
    /// used to mark the system block. The system
    /// block has no mark of its own, so we borrow
    /// the last tool mark OR the last user message
    /// mark. Tool marks take priority over message
    /// marks, and the scan is in reverse so the
    /// most-recent mark wins. Without this contract
    /// the system block either gets a stale mark
    /// (cache poisoning) or no mark at all (cache
    /// miss on every turn).
    #[test]
    fn representative_cache_mark_priority_last_tool_over_messages() {
        let tool_mark = CacheControlMark {
            ttl: CacheTtl::Long,
            scope: CacheScope("t".to_string()),
            pinned: false,
        };
        let msg_mark = CacheControlMark {
            ttl: CacheTtl::Ephemeral,
            scope: CacheScope("m".to_string()),
            pinned: false,
        };
        let tools = vec![ToolDefinition {
            name: "t1".to_string(),
            description: String::new(),
            input_schema: serde_json::Value::Null,
            cache_control: Some(tool_mark.clone()),
            annotations: None,
        }];
        let messages = vec![Message {
            role: Role::User,
            content: Content::Single(ContentPart::Text(TextContent {
                text: "hi".to_string(),
                cache_control: Some(msg_mark),
            })),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        }];
        let req = CompletionRequest {
            model: "x".to_string(),
            messages: Arc::new(messages),
            tools: Arc::new(tools),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
            replay_state: None,
        };
        let got = representative_cache_mark(&req).expect("must find a mark");
        assert_eq!(
            got.ttl,
            CacheTtl::Long,
            "tool mark must win over message mark"
        );
    }

    #[test]
    fn representative_cache_mark_picks_last_tool_in_reverse_order() {
        let first = CacheControlMark {
            ttl: CacheTtl::Ephemeral,
            scope: CacheScope::default(),
            pinned: false,
        };
        let last = CacheControlMark {
            ttl: CacheTtl::Extended,
            scope: CacheScope::default(),
            pinned: false,
        };
        let tools = vec![
            ToolDefinition {
                name: "first".to_string(),
                description: String::new(),
                input_schema: serde_json::Value::Null,
                cache_control: Some(first),
                annotations: None,
            },
            ToolDefinition {
                name: "last".to_string(),
                description: String::new(),
                input_schema: serde_json::Value::Null,
                cache_control: Some(last.clone()),
                annotations: None,
            },
        ];
        let req = CompletionRequest {
            model: "x".to_string(),
            messages: Arc::new(vec![]),
            tools: Arc::new(tools),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
            replay_state: None,
        };
        let got = representative_cache_mark(&req).expect("must find a mark");
        assert_eq!(
            got.ttl,
            CacheTtl::Extended,
            "last tool mark must be returned (reverse scan)"
        );
    }

    #[test]
    fn representative_cache_mark_none_when_nothing_marked() {
        let req = CompletionRequest {
            model: "x".to_string(),
            messages: Arc::new(vec![]),
            tools: Arc::new(vec![ToolDefinition {
                name: "t".to_string(),
                description: String::new(),
                input_schema: serde_json::Value::Null,
                cache_control: None,
                annotations: None,
            }]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
            replay_state: None,
        };
        assert!(
            representative_cache_mark(&req).is_none(),
            "no marks anywhere must return None"
        );
    }

    /// `sanitize_tool_id` is the ONLY line of defense
    /// between caller-controlled tool IDs and the
    /// Anthropic wire format. Anthropic requires
    /// tool IDs to match `[A-Za-z0-9_-]+`. Any other
    /// character would produce a 400 from upstream.
    /// Pin the contract character-by-character.
    #[test]
    fn sanitize_tool_id_replaces_invalid_chars_with_underscore() {
        assert_eq!(
            AnthropicProvider::sanitize_tool_id("toolu_01"),
            "toolu_01",
            "alphanumeric + underscore must be preserved"
        );
        assert_eq!(
            AnthropicProvider::sanitize_tool_id("toolu-01"),
            "toolu-01",
            "dash must be preserved"
        );
        assert_eq!(
            AnthropicProvider::sanitize_tool_id("toolu/01"),
            "toolu_01",
            "slash must be replaced"
        );
        assert_eq!(
            AnthropicProvider::sanitize_tool_id("toolu 01"),
            "toolu_01",
            "space must be replaced"
        );
        assert_eq!(
            AnthropicProvider::sanitize_tool_id("toolu\n01"),
            "toolu_01",
            "newline must be replaced"
        );
        // The OpenAI default fallback `call_{index}`
        // round-trips through sanitize_tool_id.
        assert_eq!(AnthropicProvider::sanitize_tool_id("call_0"), "call_0");
    }

    /// `transform_message` filters `Role::System`
    /// entirely — system text goes through the
    /// top-level `system` field, not the `messages`
    /// array. Pin the filter so a refactor that
    /// accidentally maps `System → "system"` (the
    /// literal Anthropic role) wouldn't break the
    /// wire contract.
    #[test]
    fn transform_message_filters_system_role_to_none() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        let sys_msg = Message {
            role: Role::System,
            content: Content::Single(ContentPart::Text(TextContent {
                text: "You are a helpful assistant.".to_string(),
                cache_control: None,
            })),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let out = provider.transform_message(&sys_msg);
        assert!(
            out.is_none(),
            "Role::System MUST be filtered to None; got {out:?}"
        );
    }

    /// `Role::Tool` is mapped to `"user"` because
    /// Anthropic's API accepts tool results inside a
    /// user-role message (the `tool_result`
    /// content block). Pin the mapping so a
    /// refactor that maps `Tool → "tool"` doesn't
    /// break the wire format (Anthropic rejects
    /// unknown roles).
    #[test]
    fn transform_message_maps_tool_role_to_user() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        let tool_msg = Message {
            role: Role::Tool,
            content: Content::Single(ContentPart::ToolResult(
                crate::types::ToolResult {
                    tool_use_id: "toolu_1".to_string(),
                    tool_name: None,
                    content: vec![ContentPart::Text(TextContent {
                        text: "hi".to_string(),
                        cache_control: None,
                    })],
                    structured_content: None,
                    is_error: Some(false),
                    metadata: serde_json::Map::new(),
                    truncated_by: None,
                },
            )),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let out = provider
            .transform_message(&tool_msg)
            .expect("Tool role must not be filtered");
        assert_eq!(
            out.role, "user",
            "Role::Tool MUST map to \"user\" wire role (Anthropic rejects unknown roles); got {:?}",
            out.role
        );
    }

    /// `Content::Single` is normalized to a
    /// single-element `content` array (Anthropic
    /// always uses arrays). Pin this so a refactor
    /// that returns `content` as a scalar breaks
    /// loudly rather than silently.
    #[test]
    fn transform_message_single_content_normalizes_to_array() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        let msg = Message {
            role: Role::User,
            content: Content::Single(ContentPart::Text(TextContent {
                text: "hi".to_string(),
                cache_control: None,
            })),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let out = provider
            .transform_message(&msg)
            .expect("User role must not be filtered");
        assert_eq!(out.content.len(), 1, "Single must become 1-element array");
    }

    /// `Content::Multi(vec![])` — an empty multi
    /// MUST become an empty content array. This is
    /// rare but possible (e.g. a tool-only message
    /// where the tool_use was extracted to a
    /// separate variable).
    #[test]
    fn transform_message_empty_multi_is_dropped() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        let msg = Message {
            role: Role::User,
            content: Content::Multi(vec![]),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        // An empty `content` array is not a legal Anthropic message, so
        // a message with nothing representable is dropped rather than
        // sent malformed. (Was "empty Multi becomes empty array" before
        // the projection learned to drop unrepresentable parts; that
        // could not stay true once a part — unsigned reasoning — can
        // vanish, leaving the array empty.)
        assert!(
            provider.transform_message(&msg).is_none(),
            "a message with nothing to send must be dropped"
        );
    }

    /// Unsigned reasoning has no valid Anthropic representation and is
    /// dropped; signed reasoning is carried as a `ThinkingBlock`.
    ///
    /// The unsigned half is the reachable one — `reasoning_content`
    /// gateways and `<think>`-marker upstreams emit reasoning without a
    /// signature — and sending it would produce a request Anthropic
    /// rejects.
    #[test]
    fn transform_part_reasoning_requires_a_signature() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });

        let unsigned = ContentPart::Reasoning(crate::types::ReasoningContent {
            text: "no signature".to_string(),
            signature: None,
        });
        assert!(
            provider.transform_part(&unsigned).is_none(),
            "an unsigned thinking block must not reach the request"
        );

        let signed = ContentPart::Reasoning(crate::types::ReasoningContent {
            text: "signed".to_string(),
            signature: Some("sig_x".to_string()),
        });
        match provider.transform_part(&signed).expect("signed is valid") {
            AnthropicContentBlock::ThinkingBlock {
                thinking,
                signature,
            } => {
                assert_eq!(thinking, "signed");
                assert_eq!(signature.as_deref(), Some("sig_x"));
            }
            other => panic!("expected ThinkingBlock, got {other:?}"),
        }
    }

    /// `transform_part` AudioFormat mapping: an explicit format wins,
    /// and when it is absent the container is derived from the media
    /// type. An unrecognised media type yields no `format` field rather
    /// than a guessed one.
    #[test]
    fn transform_part_audio_format_mapping() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        for (fmt, mime, expected) in [
            (Some(AudioFormat::Wav), "audio/wav", Some("wav")),
            (Some(AudioFormat::Mp3), "audio/wav", Some("mp3")),
            (Some(AudioFormat::Flac), "audio/wav", Some("flac")),
            (None, "audio/wav", Some("wav")),
            (None, "audio/mpeg", Some("mp3")),
            (None, "application/octet-stream", None),
        ] {
            let part = ContentPart::Audio(crate::types::AudioContent {
                data: "abc".to_string(),
                mime_type: mime.to_string(),
                format: fmt.clone(),
            });
            let block = provider.transform_part(&part).expect("representable");
            match block {
                crate::anthropic::types::AnthropicContentBlock::Audio {
                    source,
                    ..
                } => {
                    assert_eq!(
                        source.format.as_deref(),
                        expected,
                        "format={fmt:?} mime={mime} must map to {expected:?}"
                    );
                }
                other => panic!("expected Audio block, got {other:?}"),
            }
        }
    }

    /// `transform_part` classifies an image as remote only when its
    /// data carries an explicit `http(s)://` scheme. A bare base64
    /// payload stays base64 no matter how long it is — a real PNG is
    /// tens of kilobytes, so any length threshold would send it as a
    /// URL and the API would reject it.
    #[test]
    fn transform_part_image_source_is_classified_by_scheme() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        let source_of = |ic: crate::types::ImageContent| match provider
            .transform_part(&ContentPart::Image(ic))
            .expect("representable")
        {
            crate::anthropic::types::AnthropicContentBlock::Image {
                source,
            } => source,
            other => panic!("expected Image block, got {other:?}"),
        };

        // A `data:` URL → base64 with the envelope stripped.
        match source_of(crate::types::ImageContent {
            data: "data:image/png;base64,iVBORw0KGgo=".to_string(),
            mime_type: "image/png".to_string(),
            detail: None,
        }) {
            crate::anthropic::types::AnthropicImageSource::Base64 {
                media_type,
                data,
            } => {
                assert_eq!(media_type, "image/png");
                assert_eq!(data, "iVBORw0KGgo=");
            }
            other => panic!("expected a base64 source, got {other:?}"),
        }

        // A long bare base64 payload → still base64. A real PNG is tens
        // of kilobytes, so any length threshold would misread it as a
        // URL.
        let long_b64 = "iVBORw0KGgoAAAANSUhEUg".repeat(100);
        match source_of(crate::types::ImageContent {
            data: long_b64.clone(),
            mime_type: "image/png".to_string(),
            detail: None,
        }) {
            crate::anthropic::types::AnthropicImageSource::Base64 {
                data,
                ..
            } => {
                assert_eq!(data, long_b64, "long base64 was misread as a URL");
            }
            other => panic!("expected a base64 source, got {other:?}"),
        }

        // An https URL → the single-field `url` variant. Anthropic
        // rejects `media_type` / `data` alongside `type: "url"`, so the
        // serialized block must carry the URL and nothing else.
        let remote = source_of(crate::types::ImageContent {
            data: "https://example.com/i.png".to_string(),
            mime_type: "image/png".to_string(),
            detail: None,
        });
        let json = serde_json::to_value(&remote).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "url", "url": "https://example.com/i.png"}),
            "a remote image source must be exactly {{type, url}}"
        );
    }

    /// `transform_part` ToolResult error formatting.
    /// `is_error=true` MUST prefix the content with
    /// `"Error: "`, otherwise the content is just
    /// `{:?}`. Pin the contract so the agent
    /// layer can rely on the prefix for
    /// downstream filtering.
    #[test]
    fn transform_part_tool_result_error_prefix() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        // Success path — no Error: prefix.
        let ok = ContentPart::ToolResult(crate::types::ToolResult {
            tool_use_id: "toolu_1".to_string(),
            tool_name: None,
            content: vec![ContentPart::Text(TextContent {
                text: "tool returned ok".to_string(),
                cache_control: None,
            })],
            structured_content: None,
            is_error: Some(false),
            metadata: serde_json::Map::new(),
            truncated_by: None,
        });
        match provider.transform_part(&ok).expect("representable") {
            crate::anthropic::types::AnthropicContentBlock::ToolResult {
                content,
                ..
            } => {
                assert_eq!(content.len(), 1);
                let AnthropicToolResultContent::Text { text } = &content[0]
                else {
                    panic!("expected a text block, got {:?}", content[0]);
                };
                assert!(
                    !text.starts_with("Error: "),
                    "is_error=false MUST NOT add Error: prefix; got {text:?}"
                );
            }
            other => panic!("expected ToolResult block, got {other:?}"),
        }
        // Error path — Error: prefix added.
        let err = ContentPart::ToolResult(crate::types::ToolResult {
            tool_use_id: "toolu_2".to_string(),
            tool_name: None,
            content: vec![ContentPart::Text(TextContent {
                text: "tool failed".to_string(),
                cache_control: None,
            })],
            structured_content: None,
            is_error: Some(true),
            metadata: serde_json::Map::new(),
            truncated_by: None,
        });
        match provider.transform_part(&err).expect("representable") {
            crate::anthropic::types::AnthropicContentBlock::ToolResult {
                content,
                ..
            } => {
                let AnthropicToolResultContent::Text { text } = &content[0]
                else {
                    panic!("expected a text block, got {:?}", content[0]);
                };
                assert!(
                    text.starts_with("Error: "),
                    "is_error=true MUST prefix Error: ; got {text:?}"
                );
            }
            other => panic!("expected ToolResult block, got {other:?}"),
        }
    }

    /// A tool result carrying an image MUST emit an Anthropic
    /// `image` block inside `tool_result.content` — not a
    /// Debug-formatted string. This is the hop that lets a
    /// screenshot / chart / OCR tool actually show the model
    /// something.
    #[test]
    fn transform_part_tool_result_preserves_image_blocks() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        });
        let b64 = "iVBORw0KGgoAAAANSUhEUg==";
        let tr = ContentPart::ToolResult(crate::types::ToolResult {
            tool_use_id: "toolu_img".to_string(),
            tool_name: None,
            content: vec![
                ContentPart::Text(TextContent {
                    text: "captured".to_string(),
                    cache_control: None,
                }),
                ContentPart::Image(crate::types::ImageContent {
                    data: b64.to_string(),
                    mime_type: "image/png".to_string(),
                    detail: None,
                }),
            ],
            structured_content: None,
            is_error: Some(false),
            metadata: serde_json::Map::new(),
            truncated_by: None,
        });

        match provider.transform_part(&tr).expect("representable") {
            crate::anthropic::types::AnthropicContentBlock::ToolResult {
                content,
                ..
            } => {
                assert_eq!(content.len(), 2, "text + image: {content:?}");
                match &content[0] {
                    AnthropicToolResultContent::Text { text } => {
                        assert_eq!(text, "captured");
                    }
                    other => panic!("expected text first, got {other:?}"),
                }
                match &content[1] {
                    AnthropicToolResultContent::Image { source } => {
                        match source {
                            crate::anthropic::types::AnthropicImageSource::Base64 {
                                media_type,
                                data,
                            } => {
                                assert_eq!(media_type, "image/png");
                                assert_eq!(data, b64);
                            }
                            other => panic!(
                                "bare base64 must stay a base64 source: {other:?}"
                            ),
                        }
                    }
                    other => panic!("expected image second, got {other:?}"),
                }
            }
            other => panic!("expected ToolResult block, got {other:?}"),
        }
    }

    /// An image block inside a tool result MUST round-trip through
    /// the wire shape (both directions of the transform).
    #[test]
    fn tool_result_image_block_round_trips_through_json() {
        let raw = serde_json::json!({
            "type": "tool_result",
            "tool_use_id": "toolu_1",
            "content": [
                {"type": "text", "text": "look"},
                {
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": "image/png",
                        "data": "QUJD"
                    }
                }
            ]
        });
        let block: crate::anthropic::types::AnthropicContentBlock =
            serde_json::from_value(raw).expect("parse tool_result with image");
        let crate::anthropic::types::AnthropicContentBlock::ToolResult {
            content,
            ..
        } = block
        else {
            panic!("expected ToolResult");
        };
        assert_eq!(content.len(), 2);
        assert!(matches!(
            &content[1],
            AnthropicToolResultContent::Image {
                source: crate::anthropic::types::AnthropicImageSource::Base64 {
                    data,
                    ..
                }
            } if data == "QUJD"
        ));
        // Re-serializing keeps the image block (not a flattened string).
        let back =
            serde_json::to_value(&content[1]).expect("serialize image block");
        assert_eq!(back["type"], "image");
        assert_eq!(back["source"]["data"], "QUJD");
    }

    /// The legacy string form of `tool_result.content` still parses
    /// into a single text block.
    #[test]
    fn tool_result_string_content_still_parses() {
        let raw = serde_json::json!({
            "type": "tool_result",
            "tool_use_id": "toolu_1",
            "content": "plain string body"
        });
        let block: crate::anthropic::types::AnthropicContentBlock =
            serde_json::from_value(raw).expect("parse string-form content");
        let crate::anthropic::types::AnthropicContentBlock::ToolResult {
            content,
            ..
        } = block
        else {
            panic!("expected ToolResult");
        };
        match &content[0] {
            AnthropicToolResultContent::Text { text } => {
                assert_eq!(text, "plain string body");
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    /// `transform_part` projects a `ResourceLink` into the same
    /// bounded note the OpenAI adapter emits: the name, the media
    /// type, and — for a small `text/*` payload — the decoded text.
    ///
    /// The placeholder this replaced was the constant
    /// `"[ResourceLink]"`, which lost the file: not even the filename
    /// reached the model, let alone an attached note's contents.
    #[test]
    fn transform_part_resource_inlines_small_text_payload() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        let payload = BASE64.encode(b"# spec\n\nhello world\n");
        let res = ContentPart::Resource(crate::types::ResourceLink {
            uri: format!("data:text/plain;base64,{payload}"),
            name: "spec.md".to_string(),
            title: Some("Spec".to_string()),
            description: None,
            mime_type: Some("text/plain".to_string()),
        });
        match provider.transform_part(&res).expect("representable") {
            crate::anthropic::types::AnthropicContentBlock::Text {
                text,
                cache_control,
            } => {
                assert!(text.contains("hello world"), "got: {text}");
                assert!(text.contains("spec.md"), "got: {text}");
                assert!(cache_control.is_none());
                assert!(
                    !text.contains(&payload),
                    "base64 must never reach the prompt: {text}"
                );
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    /// A large binary resource reaches Anthropic as a short note, and
    /// the emitted text stays bounded however big the payload is. A
    /// 1 MB PDF encodes to ~1.4 M characters — enough to blow the
    /// context window on its own.
    #[test]
    fn transform_part_resource_bounds_a_large_binary_payload() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        let payload = BASE64.encode(vec![0u8; 1024 * 1024]);
        let res = ContentPart::Resource(crate::types::ResourceLink {
            uri: format!("data:application/pdf;base64,{payload}"),
            name: "report.pdf".to_string(),
            title: None,
            description: None,
            mime_type: Some("application/pdf".to_string()),
        });
        match provider.transform_part(&res).expect("representable") {
            crate::anthropic::types::AnthropicContentBlock::Text {
                text,
                ..
            } => {
                assert!(
                    text.len() < 300,
                    "a 1 MB payload must not reach the prompt; got {} chars",
                    text.len()
                );
                assert!(text.contains("report.pdf"), "got: {text}");
                assert!(text.contains("application/pdf"), "got: {text}");
                assert!(
                    !text.contains(&payload[..64]),
                    "no base64 may survive: {text}"
                );
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    /// A resource that is not an inline `data:` URI still reaches the
    /// model by name and type — the filename is what may not vanish.
    #[test]
    fn transform_part_resource_names_a_referenced_uri() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        let res = ContentPart::Resource(crate::types::ResourceLink {
            uri: "file://docs/spec.md".to_string(),
            name: "spec.md".to_string(),
            title: Some("Spec".to_string()),
            description: None,
            mime_type: Some("text/markdown".to_string()),
        });
        match provider.transform_part(&res).expect("representable") {
            crate::anthropic::types::AnthropicContentBlock::Text {
                text,
                cache_control,
            } => {
                assert!(text.contains("spec.md"), "got: {text}");
                assert!(text.contains("text/markdown"), "got: {text}");
                assert!(cache_control.is_none());
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    /// `build_anthropic_system` has 6 critical
    /// branches that no test pins today. The
    /// `system_text: None` early-return, the
    /// `cache_policy = None` default, the
    /// `cache_policy.system = false` short-circuit,
    /// the `cache_policy.system = true` structured
    /// path with and without a representative
    /// mark, and the empty-string passthrough.
    /// A regression in any of these would either
    /// produce a `system: ""` field that Anthropic
    /// rejects or silently strip the cache
    /// marking.
    fn cp_with_system(b: bool) -> crate::cache_policy::CachePolicy {
        crate::cache_policy::CachePolicy {
            system: b,
            ..Default::default()
        }
    }

    #[test]
    fn build_anthropic_system_none_text_returns_none() {
        let out = build_anthropic_system(None, None, None);
        assert!(out.is_none(), "system_text=None MUST short-circuit to None");
    }

    #[test]
    fn build_anthropic_system_none_cache_policy_uses_text_variant() {
        let out = build_anthropic_system(Some("hi".to_string()), None, None);
        match out {
            Some(crate::anthropic::types::AnthropicSystem::Text(s)) => {
                assert_eq!(s, "hi");
            }
            other => {
                panic!("cache_policy=None MUST use Text variant; got {other:?}")
            }
        }
    }

    #[test]
    fn build_anthropic_system_system_false_uses_text_variant() {
        let cp = cp_with_system(false);
        let out =
            build_anthropic_system(Some("hi".to_string()), Some(&cp), None);
        match out {
            Some(crate::anthropic::types::AnthropicSystem::Text(s)) => {
                assert_eq!(s, "hi");
            }
            other => panic!(
                "policy.system=false MUST use Text variant even if mark exists; got {other:?}"
            ),
        }
    }

    #[test]
    fn build_anthropic_system_system_true_uses_structured_with_default_mark() {
        let cp = cp_with_system(true);
        let out =
            build_anthropic_system(Some("hi".to_string()), Some(&cp), None);
        match out {
            Some(crate::anthropic::types::AnthropicSystem::Structured(
                blocks,
            )) => {
                assert_eq!(blocks.len(), 1);
                assert_eq!(blocks[0].text, "hi");
                assert!(
                    blocks[0].cache_control.is_some(),
                    "structured path must attach cache_control"
                );
            }
            other => panic!(
                "policy.system=true without mark MUST use Structured variant; got {other:?}"
            ),
        }
    }

    #[test]
    fn build_anthropic_system_system_true_with_mark_propagates_it() {
        let cp = cp_with_system(true);
        let mark = CacheControlMark {
            ttl: CacheTtl::Extended,
            scope: CacheScope::default(),
            pinned: true,
        };
        let out = build_anthropic_system(
            Some("hi".to_string()),
            Some(&cp),
            Some(&mark),
        );
        match out {
            Some(crate::anthropic::types::AnthropicSystem::Structured(
                blocks,
            )) => {
                let cc = blocks[0]
                    .cache_control
                    .as_ref()
                    .expect("cache_control must be Some");
                // The wire `type` field is ALWAYS
                // "ephemeral" regardless of the mark's
                // ttl class — `ttl_seconds` is the
                // discriminator. Pin both invariants
                // so a refactor that switches to a
                // ttl-aware type string breaks loudly.
                assert_eq!(
                    cc.r#type, "ephemeral",
                    "wire type field MUST stay \"ephemeral\"; got {:?}",
                    cc.r#type
                );
                assert_eq!(
                    cc.ttl_seconds,
                    Some(300),
                    "CacheTtl::Extended MUST map to ttl_seconds=300; got {:?}",
                    cc.ttl_seconds
                );
            }
            other => panic!("expected Structured variant; got {other:?}"),
        }
    }

    /// An empty system prompt is OMITTED, not passed through.
    ///
    /// This replaces a test that pinned the opposite ("empty system text
    /// MUST pass through verbatim"). That contract produced a request the
    /// API rejects: the `system` parameter is `string or array of
    /// TextBlockParam`, and `TextBlockParam.text` is `minLength: 1` — so
    /// the cache-marked (Structured) branch, which always emits a text
    /// block, could not carry an empty string at all. Omitting the whole
    /// field is also the honest encoding, since an empty prompt and no
    /// prompt mean the same thing to the model.
    ///
    /// See <https://platform.claude.com/docs/en/api/messages> (`system`).
    #[test]
    fn build_anthropic_system_empty_string_is_omitted() {
        assert!(
            build_anthropic_system(Some(String::new()), None, None).is_none(),
            "an empty system prompt must be omitted, not sent as `system: \"\"`"
        );
        // The cache-marked branch shares the guard, and it is the one the
        // spec definitely rejects — its text block has no valid empty form.
        let policy = crate::cache_policy::CachePolicy {
            system: true,
            ..Default::default()
        };
        assert!(
            build_anthropic_system(Some(String::new()), Some(&policy), None)
                .is_none(),
            "the Structured branch must not emit an empty text block"
        );
        // A real prompt still rides either branch.
        assert!(matches!(
            build_anthropic_system(Some("be terse".to_string()), None, None),
            Some(crate::anthropic::types::AnthropicSystem::Text(s)) if s == "be terse"
        ));
    }

    /// The harness commits a tool result as raw parts with the call id on
    /// the MESSAGE, not as a `ToolResult` wrapper. The projection must
    /// still produce a `tool_result` block carrying that id.
    ///
    /// Asserting only that the text survived would miss the real defect:
    /// plain `text` blocks leave the preceding assistant `tool_use`
    /// unanswered, and Anthropic rejects such a request outright.
    #[test]
    fn tool_message_with_unwrapped_result_parts_becomes_a_tool_result_block() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        let msg = Message::tool(
            Content::parts(vec![ContentPart::Text(TextContent {
                text: "stdout:\nE2E-TOOLBLOCK\n\n\n[exit code: 0]".to_string(),
                cache_control: None,
            })]),
            "call_1",
        );
        let out = provider.transform_message(&msg).expect("representable");
        assert_eq!(out.role, "user", "a tool result rides in a user turn");
        assert_eq!(out.content.len(), 1, "got {:?}", out.content);
        match &out.content[0] {
            crate::anthropic::types::AnthropicContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => {
                assert_eq!(
                    tool_use_id, "call_1",
                    "the call id must be re-attached, or Anthropic \
                     rejects the tool_use it answers"
                );
                let text = content
                    .iter()
                    .filter_map(|b| match b {
                        crate::anthropic::types::AnthropicToolResultContent::Text {
                            text,
                        } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<String>();
                assert!(
                    text.contains("E2E-TOOLBLOCK"),
                    "tool output missing from the tool_result block: {text}"
                );
            }
            other => panic!("expected a ToolResult block, got {other:?}"),
        }
        // And nothing leaks as a bare text block.
        let json = serde_json::to_value(&out).unwrap();
        assert_eq!(json["content"][0]["type"], "tool_result");
    }
    /// Anthropic's own validation rule, encoded: every assistant
    /// `tool_use` block must be answered by a `tool_result` block in the
    /// turn that immediately follows — otherwise the API rejects the
    /// whole request with "`tool_use` ids were found without
    /// `tool_result` blocks immediately after".
    ///
    /// This drives a realistic run history (assistant calls a tool, the
    /// harness commits the result as raw parts) through
    /// `transform_request` and checks the resulting message sequence
    /// satisfies that rule. It is the contract a unit assertion on one
    /// block cannot see.
    #[test]
    fn parallel_tool_uses_are_answered_from_one_user_message() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });

        let request = CompletionRequest {
            model: "claude-3".to_string(),
            messages: vec![
                Message::user("run it"),
                // What the runtime records for an assistant tool call.
                Message::new(
                    Role::Assistant,
                    Content::parts(vec![
                        ContentPart::ToolUse(ToolUse {
                            id: "call_1".to_string(),
                            name: "shell".to_string(),
                            input: serde_json::json!({"command": "echo ONE"}),
                        }),
                        ContentPart::ToolUse(ToolUse {
                            id: "call_2".to_string(),
                            name: "shell".to_string(),
                            input: serde_json::json!({"command": "echo TWO"}),
                        }),
                    ]),
                ),
                // One `Message::tool` per result — what commit.rs builds,
                // so this is two consecutive tool messages.
                Message::tool(
                    Content::parts(vec![ContentPart::Text(TextContent {
                        text: "stdout:\nONE".to_string(),
                        cache_control: None,
                    })]),
                    "call_1",
                ),
                Message::tool(
                    Content::parts(vec![ContentPart::Text(TextContent {
                        text: "stdout:\nTWO".to_string(),
                        cache_control: None,
                    })]),
                    "call_2",
                ),
            ]
            .into(),
            ..CompletionRequest::default()
        };

        let wire = provider.transform_request(&request);
        let json = serde_json::to_value(&wire).unwrap();
        let messages = json["messages"].as_array().expect("messages");

        // Collect every tool_use id and every tool_result id, in order.
        let mut pending: Vec<String> = Vec::new();
        for (index, message) in messages.iter().enumerate() {
            for block in message["content"].as_array().into_iter().flatten() {
                match block["type"].as_str() {
                    Some("tool_use") => {
                        pending.push(block["id"].as_str().unwrap().to_string())
                    }
                    Some("tool_result") => {
                        let answered =
                            block["tool_use_id"].as_str().unwrap().to_string();
                        let pos = pending.iter().position(|p| *p == answered);
                        assert!(
                            pos.is_some(),
                            "message {index} answers `{answered}`, which no                              pending tool_use issued: {json}"
                        );
                        pending.remove(pos.unwrap());
                    }
                    _ => {}
                }
            }
        }
        assert!(
            pending.is_empty(),
            "Anthropic would reject this body — unanswered tool_use ids:              {pending:?} in {json}"
        );
        // A parallel pair shares ONE user message — the protocol's own
        // documented shape for answering several calls at once. Consecutive
        // same-role turns would also be *combined* server-side, so this is
        // not a rejection guard: it pins that we emit the documented shape
        // rather than depending on that combining.
        let result_messages = messages
            .iter()
            .filter(|m| {
                m["content"].as_array().is_some_and(|blocks| {
                    blocks.iter().any(|b| b["type"] == "tool_result")
                })
            })
            .count();
        assert_eq!(
            result_messages, 1,
            "both tool results belong in one user message: {json}"
        );
    }

    /// Anthropic rejects an empty text block, and one is reachable from a
    /// legitimately-persisted turn: the composer lets the user send
    /// attachments with no text, which writes `{"text":""}` to the log,
    /// and the next turn replays it. The wire must not carry it — and if
    /// dropping it empties the whole message, the message is dropped too
    /// rather than shipped as `content: []`.
    #[test]
    fn an_empty_text_part_is_not_shipped() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        let request = CompletionRequest {
            model: "claude-3".to_string(),
            messages: vec![
                Message::user(""),
                Message::new(
                    Role::Assistant,
                    Content::text("acknowledged the file"),
                ),
                Message::user("and now?"),
            ]
            .into(),
            ..CompletionRequest::default()
        };
        let wire = provider.transform_request(&request);
        let json = serde_json::to_value(&wire).unwrap();
        let empty_text_blocks = json["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|m| m["content"].as_array().into_iter().flatten())
            .filter(|b| b["type"] == "text" && b["text"] == "")
            .count();
        assert_eq!(
            empty_text_blocks, 0,
            "an empty text block is rejected by the API: {json}"
        );
    }

    /// The API's `text` field is `minLength: 1`, so an empty one invalidates
    /// the whole request. A tool that returned nothing — or returned no
    /// parts at all — must still reach the model as something readable
    /// rather than as an empty block.
    #[test]
    fn an_empty_tool_result_becomes_a_readable_placeholder() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        let wire = provider.transform_request(&CompletionRequest {
            model: "claude-3".to_string(),
            messages: vec![
                Message::new(
                    Role::Assistant,
                    Content::parts(vec![ContentPart::ToolUse(ToolUse {
                        id: "call_1".to_string(),
                        name: "shell".to_string(),
                        input: serde_json::json!({"command": "true"}),
                    })]),
                ),
                Message::tool(Content::text(""), "call_1"),
            ]
            .into(),
            ..CompletionRequest::default()
        });
        let json = serde_json::to_value(&wire).unwrap();
        let empty_texts = json["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|m| m["content"].as_array().into_iter().flatten())
            .filter(|b| b["type"] == "text" && b["text"] == "")
            .count();
        assert_eq!(empty_texts, 0, "an empty text block is invalid: {json}");
        assert!(
            json.to_string().contains("no output"),
            "the model must be told the tool produced nothing: {json}"
        );
    }

    /// The common multi-turn shape: turn 1 used a tool, turn 2 is a fresh
    /// prompt. Both must reach the wire, with the tool result still paired
    /// to its call.
    ///
    /// This deliberately does NOT assert role alternation — the API
    /// combines consecutive same-role turns, and this module itself splits
    /// an assistant message into two on purpose.
    #[test]
    fn a_fresh_user_turn_after_a_tool_exchange_still_reaches_the_wire() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        let wire = provider.transform_request(&CompletionRequest {
            model: "claude-3".to_string(),
            messages: vec![
                Message::user("run it"),
                Message::new(
                    Role::Assistant,
                    Content::parts(vec![ContentPart::ToolUse(ToolUse {
                        id: "call_1".to_string(),
                        name: "shell".to_string(),
                        input: serde_json::json!({"command": "echo hi"}),
                    })]),
                ),
                Message::tool(Content::text("stdout:\nhi"), "call_1"),
                Message::user("now the other thing"),
            ]
            .into(),
            ..CompletionRequest::default()
        });
        let json = serde_json::to_value(&wire).unwrap();
        let messages = json["messages"].as_array().unwrap();
        // The call is still answered by the message that follows it.
        let call_at = messages
            .iter()
            .position(|m| {
                m["content"].as_array().is_some_and(|blocks| {
                    blocks.iter().any(|b| b["type"] == "tool_use")
                })
            })
            .expect("the tool_use reached the wire");
        assert!(
            messages[call_at + 1]["content"]
                .as_array()
                .is_some_and(|blocks| blocks
                    .iter()
                    .any(|b| b["type"] == "tool_result")),
            "the tool result must follow its call: {json}"
        );
        // And the fresh question is the LAST turn, not lost.
        let last = messages.last().unwrap();
        assert!(
            last.to_string().contains("now the other thing"),
            "the new user turn must survive: {json}"
        );
    }
    /// An empty system prompt must be OMITTED, not sent as
    /// `{"type":"text","text":""}` — the protocol's text blocks carry
    /// `minLength: 1` in every position, `system` included.
    ///
    /// Reachable: a `System` message holding one empty `Text` part makes
    /// `extract_text` return `Some("")`, which the `Option` check alone
    /// would pass straight through. The Structured (cache-marked) branch
    /// shares the guard.
    #[test]
    fn an_empty_system_prompt_is_omitted() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        let request = CompletionRequest {
            model: "claude-3".to_string(),
            messages: vec![
                // A system turn whose text is empty.
                Message::new(Role::System, Content::text("")),
                Message::user("hello"),
            ]
            .into(),
            ..CompletionRequest::default()
        };
        let wire = provider.transform_request(&request);
        let json = serde_json::to_value(&wire).unwrap();
        assert!(
            json.get("system").is_none(),
            "no system field should be sent at all: {json}"
        );
        assert!(
            !json.to_string().contains(r#""text":"""#),
            "an empty text block is rejected in every position: {json}"
        );
        // A real system prompt still rides, so the guard is not a blanket drop.
        let with_system = provider.transform_request(&CompletionRequest {
            model: "claude-3".to_string(),
            messages: vec![
                Message::new(Role::System, Content::text("be terse")),
                Message::user("hello"),
            ]
            .into(),
            ..CompletionRequest::default()
        });
        let kept = serde_json::to_value(&with_system).unwrap();
        assert_eq!(kept["system"], "be terse");
    }
    /// Anthropic's `ToolUseBlockParam.input` is `map[unknown]` and `name` is
    /// `1 <= len <= 200`. A canonical tool call is not guaranteed to satisfy
    /// either — a malformed one can parse to null / a scalar / an array,
    /// which is exactly why the OpenAI adapter has coerced and tested this
    /// since before this adapter existed. The same canonical part must not
    /// be valid on one wire and a rejection on the other.
    #[test]
    fn a_tool_use_is_shaped_for_this_wire() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        let block_of = |tu: ToolUse| match provider
            .transform_part(&ContentPart::ToolUse(tu))
            .expect("a tool_use is representable")
        {
            crate::anthropic::types::AnthropicContentBlock::ToolUse {
                name,
                input,
                ..
            } => (name, input),
            other => panic!("expected ToolUse, got {other:?}"),
        };

        // A non-object input is coerced to an object rather than shipped.
        for bad in [
            serde_json::Value::Null,
            serde_json::Value::Bool(true),
            serde_json::Value::Number(7.into()),
            serde_json::Value::String("oops".to_string()),
            serde_json::json!(["a"]),
        ] {
            let (_, input) = block_of(ToolUse {
                id: "call_1".to_string(),
                name: "shell".to_string(),
                input: bad.clone(),
            });
            assert!(
                input.is_object(),
                "input {bad} must be coerced to an object, got {input}"
            );
        }
        // A real object is passed through untouched.
        let (_, input) = block_of(ToolUse {
            id: "call_1".to_string(),
            name: "shell".to_string(),
            input: serde_json::json!({"command": "echo hi"}),
        });
        assert_eq!(input, serde_json::json!({"command": "echo hi"}));

        // An empty name is labelled; an over-long one is bounded.
        let (name, _) = block_of(ToolUse {
            id: "call_1".to_string(),
            name: String::new(),
            input: serde_json::json!({}),
        });
        assert!(
            !name.is_empty(),
            "an empty name violates `name.minLength: 1`"
        );
        let (name, _) = block_of(ToolUse {
            id: "call_1".to_string(),
            name: "x".repeat(500),
            input: serde_json::json!({}),
        });
        assert_eq!(
            name.chars().count(),
            200,
            "an over-long name violates `name.maxLength: 200`"
        );
    }
}
