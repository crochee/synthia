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

use synthia_cache_mark::{CacheControlMark, CacheScope, CacheTtl};

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
use crate::types::{
    AudioFormat,
    CompletionRequest,
    Content,
    ContentPart,
    Role,
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
            model: request.model.clone(),
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
