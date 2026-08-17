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
    types::{AudioFormat, CompletionRequest, Content, ContentPart, Role},
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
            // `apply` short-circuits (returns `true`, skips
            // `apply_cache_policy`) when `tools` / `messages` `Arc`
            // references are identical to the previous call — the
            // cache_control marks from the prior call are still present.
            // Otherwise it performs full evaluation and stores the new
            // references for the next call.
            self.cache_policy_applier
                .lock()
                .apply(&mut request, &policy);
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
        let mut result = Vec::new();

        for msg in messages {
            if msg.role != "assistant" {
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

        let content = match &msg.content {
            Content::Single(part) => {
                vec![self.transform_part(part)]
            }
            Content::Multi(parts) => {
                parts.iter().map(|p| self.transform_part(p)).collect()
            }
        };

        Some(AnthropicMessage { role, content })
    }

    pub(in crate::anthropic) fn transform_part(
        &self,
        part: &ContentPart,
    ) -> AnthropicContentBlock {
        // Translate the provider-neutral `CacheControlMark` (only present on
        // `Text` parts per MVP) to the Anthropic `CacheControl` hint,
        // propagating the mark's `scope` as a `cache_namespace` field. Other
        // variants get `cache_control: None` — the mark is only ever set on
        // user-message Text parts by `apply_cache_policy`.
        let cache_control = part.cache_control().map(cache_control_from_mark);
        match part {
            ContentPart::Text(tc) => AnthropicContentBlock::Text {
                text: tc.text.clone(),
                cache_control,
            },
            ContentPart::Reasoning(rc) => {
                AnthropicContentBlock::ThinkingBlock {
                    thinking: rc.text.clone(),
                    signature: rc.signature.clone(),
                }
            }
            ContentPart::Image(ic) => AnthropicContentBlock::Image {
                source: AnthropicImageSource {
                    r#type: if ic.data.starts_with("data:")
                        || ic.data.len() < 1000
                    {
                        "base64".to_string()
                    } else {
                        "url".to_string()
                    },
                    media_type: ic.mime_type.clone(),
                    data: if ic.data.starts_with("data:") {
                        ic.data
                            .split(',')
                            .nth(1)
                            .unwrap_or(&ic.data)
                            .to_string()
                    } else {
                        ic.data.clone()
                    },
                },
            },
            ContentPart::Audio(ac) => AnthropicContentBlock::Audio {
                source: AnthropicAudioSource {
                    r#type: if ac.data.starts_with("data:")
                        || ac.data.len() < 1000
                    {
                        "base64".to_string()
                    } else {
                        "url".to_string()
                    },
                    media_type: ac.mime_type.clone(),
                    data: if ac.data.starts_with("data:") {
                        ac.data
                            .split(',')
                            .nth(1)
                            .unwrap_or(&ac.data)
                            .to_string()
                    } else {
                        ac.data.clone()
                    },
                    format: ac.format.as_ref().map(|f| match f {
                        AudioFormat::Wav => "wav".to_string(),
                        AudioFormat::Mp3 => "mp3".to_string(),
                        AudioFormat::Flac => "flac".to_string(),
                    }),
                },
            },
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
            ContentPart::Resource(..) => AnthropicContentBlock::Text {
                text: "[ResourceLink]".to_string(),
                cache_control: None,
            },
        }
    }
}

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
    let text = system_text?;
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
mod transform_tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        ModelConfig,
        types::{Message, TextContent, ToolChoice, ToolDefinition},
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
            },
            ToolDefinition {
                name: "last".to_string(),
                description: String::new(),
                input_schema: serde_json::Value::Null,
                cache_control: Some(last.clone()),
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
            }]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
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
    fn transform_message_empty_multi_becomes_empty_array() {
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
        let out = provider
            .transform_message(&msg)
            .expect("User role must not be filtered");
        assert!(
            out.content.is_empty(),
            "empty Multi MUST become empty array; got {:?}",
            out.content
        );
    }

    /// `transform_part` AudioFormat mapping. Each
    /// enum variant MUST serialize to its
    /// corresponding lowercase string in the
    /// Anthropic `format` field.
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
        for (fmt, expected) in [
            (Some(AudioFormat::Wav), Some("wav".to_string())),
            (Some(AudioFormat::Mp3), Some("mp3".to_string())),
            (Some(AudioFormat::Flac), Some("flac".to_string())),
            (None, None),
        ] {
            let fmt_for_assert = fmt.clone();
            let part = ContentPart::Audio(crate::types::AudioContent {
                data: "abc".to_string(),
                mime_type: "audio/wav".to_string(),
                format: fmt,
            });
            let block = provider.transform_part(&part);
            match block {
                crate::anthropic::types::AnthropicContentBlock::Audio {
                    source,
                    ..
                } => {
                    assert_eq!(
                        source.format, expected,
                        "AudioFormat::{:?} must map to {:?}",
                        fmt_for_assert, expected
                    );
                }
                other => panic!("expected Audio block, got {other:?}"),
            }
        }
    }

    /// `transform_part` Image URL vs base64 detection
    /// heuristic — a `data:` prefix OR length < 1000
    /// is treated as base64. A long non-`data:`-prefixed
    /// string is treated as a URL. Pin the threshold.
    #[test]
    fn transform_part_image_url_vs_base64_detection() {
        let provider = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        // Case 1: explicit data: prefix → base64.
        let data_url = ContentPart::Image(crate::types::ImageContent {
            data: "data:image/png;base64,iVBORw0KGgo=".to_string(),
            mime_type: "image/png".to_string(),
            detail: None,
        });
        match provider.transform_part(&data_url) {
            crate::anthropic::types::AnthropicContentBlock::Image {
                source,
            } => {
                assert_eq!(source.r#type, "base64");
                assert_eq!(source.data, "iVBORw0KGgo=");
            }
            other => panic!("expected Image block, got {other:?}"),
        }
        // Case 2: short non-prefixed string → base64 (per length heuristic).
        let short = ContentPart::Image(crate::types::ImageContent {
            data: "abc123".to_string(),
            mime_type: "image/png".to_string(),
            detail: None,
        });
        match provider.transform_part(&short) {
            crate::anthropic::types::AnthropicContentBlock::Image {
                source,
            } => {
                assert_eq!(source.r#type, "base64");
                assert_eq!(source.data, "abc123");
            }
            other => panic!("expected Image block, got {other:?}"),
        }
        // Case 3: long non-prefixed string → url.
        // `.repeat(40)` pushes length past the 1000-char
        // base64 heuristic threshold.
        let long_url_data =
            "https://example.com/very/long/url/path/to/an/image/file.png"
                .repeat(40);
        let long_url = ContentPart::Image(crate::types::ImageContent {
            data: long_url_data.clone(),
            mime_type: "image/png".to_string(),
            detail: None,
        });
        match provider.transform_part(&long_url) {
            crate::anthropic::types::AnthropicContentBlock::Image {
                source,
            } => {
                assert_eq!(
                    source.r#type, "url",
                    "long non-prefixed string must be classified as url"
                );
                assert_eq!(source.data, long_url_data);
            }
            other => panic!("expected Image block, got {other:?}"),
        }
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
        match provider.transform_part(&ok) {
            crate::anthropic::types::AnthropicContentBlock::ToolResult {
                content,
                ..
            } => {
                assert_eq!(content.len(), 1);
                let text = &content[0].text;
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
        match provider.transform_part(&err) {
            crate::anthropic::types::AnthropicContentBlock::ToolResult {
                content,
                ..
            } => {
                let text = &content[0].text;
                assert!(
                    text.starts_with("Error: "),
                    "is_error=true MUST prefix Error: ; got {text:?}"
                );
            }
            other => panic!("expected ToolResult block, got {other:?}"),
        }
    }

    /// `transform_part` ResourceLink becomes a
    /// `"[ResourceLink]"` text placeholder. The
    /// Anthropic API has no native resource-link
    /// content type, so the placeholder lets the
    /// model know a resource was attached without
    /// losing the part entirely. Pin the exact
    /// placeholder string.
    #[test]
    fn transform_part_resource_link_becomes_placeholder_text() {
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
            name: "spec".to_string(),
            title: Some("Spec".to_string()),
            description: None,
            mime_type: None,
        });
        match provider.transform_part(&res) {
            crate::anthropic::types::AnthropicContentBlock::Text {
                text,
                cache_control,
            } => {
                assert_eq!(text, "[ResourceLink]");
                assert!(cache_control.is_none());
            }
            other => panic!("expected Text placeholder, got {other:?}"),
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

    #[test]
    fn build_anthropic_system_empty_string_passes_through() {
        // An empty string is a valid (if weird) system
        // text — the function does NOT filter it out.
        // Pin the contract so a future refactor that
        // adds an `is_empty()` early-return doesn't
        // silently strip callers' empty system text.
        let out = build_anthropic_system(Some(String::new()), None, None);
        match out {
            Some(crate::anthropic::types::AnthropicSystem::Text(s)) => {
                assert!(
                    s.is_empty(),
                    "empty system text MUST pass through verbatim"
                );
            }
            other => {
                panic!("empty string must produce Text(\"\"); got {other:?}")
            }
        }
    }
}
