//! Inbound Anthropic request → canonical provider types.
//!
//! [`to_canonical`] is the whole of the request-side projection: it
//! validates the body and returns the exact values the agent runtime
//! consumes ([`synthia::provider::Message`] /
//! [`synthia::provider::ContentPart`] / [`ToolDefinition`]), split into
//! the transcript to replay and the turn to answer.
//!
//! Two structural decisions are worth stating, because neither is
//! forced by the wire format:
//!
//! 1. **`tool_result` blocks become their own [`Role::Tool`] messages.**
//!    Anthropic carries a tool result as a block inside a `user`
//!    message; the canonical model has a dedicated `Role::Tool`
//!    message, and that is what the runtime's own runs write into the
//!    session log (`synthia::session::fold_log_surface` maps a
//!    `tool_result` row onto `Role::Tool`). Projecting onto the
//!    runtime's own shape keeps a replayed transcript identical to a
//!    recorded one, and it is the only spelling every provider adapter
//!    renders correctly.
//!
//! 2. **The client's `system` stays separate.** It cannot become a
//!    `Role::System` message: the harness assembles the agent's own
//!    system prompt ahead of `input.history`
//!    (`synthia::harness::agent::re_act::loop_::steps::prepare`) and the
//!    Anthropic adapter promotes only the *first* `Role::System`
//!    message to the request's top-level `system` field while
//!    filtering the rest out
//!    (`synthia::provider::anthropic::transform`) — a seeded
//!    `Role::System` message would therefore be silently dropped for
//!    exactly the provider this endpoint mirrors. It is returned
//!    separately and delivered as a leading user turn instead (see
//!    [`super::seed_history`]).
//!
//! `tools` are converted faithfully but are **not** injected into the
//! run: the run's tool surface comes from the server's own registry
//! and configuration, and the session-controller seam has no
//! per-dispatch tool-injection point. See the module docs on [`super`].

use anyhow::{Result, bail};
use synthia::provider::{
    Content,
    ContentPart,
    ImageContent,
    Message,
    ReasoningContent,
    ResourceLink,
    TextContent,
    ToolDefinition,
    ToolResult,
    ToolUse,
};

use super::wire::{
    DocumentSource,
    ImageSource,
    MessagesRequest,
    SystemPrompt,
    WireBlock,
    WireContent,
    WireMessage,
};

/// One inbound request, projected onto the canonical types the agent
/// runtime consumes.
#[derive(Debug)]
pub(super) struct CanonicalRequest {
    /// The client's `system`, flattened to plain text.
    pub(super) system: Option<String>,
    /// Every message except the final user turn, in order.
    pub(super) history: Vec<Message>,
    /// The final user turn's parts — what the run is asked to answer.
    pub(super) turn: Vec<ContentPart>,
    /// The request's tool definitions, canonical.
    pub(super) tools: Vec<ToolDefinition>,
}

/// Validate `request` and project it onto canonical types.
///
/// # Errors
///
/// Returns a human-readable reason for every rejected body; the
/// handler maps it onto a `400 invalid_request_error`.
pub(super) fn to_canonical(
    request: &MessagesRequest,
) -> Result<CanonicalRequest> {
    if request.max_tokens == 0 {
        bail!("max_tokens must be at least 1");
    }
    let system = request.system.as_ref().and_then(flatten_system);
    let mut messages = canonical_messages(&request.messages)?;
    let tools = request
        .tools
        .iter()
        .flatten()
        .map(|tool| {
            ToolDefinition::new(
                tool.name.clone(),
                tool.description.clone().unwrap_or_default(),
                tool.input_schema.clone(),
            )
        })
        .collect();
    let turn = split_final_turn(&mut messages)?;
    Ok(CanonicalRequest {
        system,
        history: messages,
        turn,
        tools,
    })
}

/// Project every wire message onto canonical messages, in order.
///
/// A message whose content contains `tool_result` blocks expands to
/// the message's other parts followed by one `Role::Tool` message per
/// result — see the module docs.
fn canonical_messages(wire: &[WireMessage]) -> Result<Vec<Message>> {
    let mut messages = Vec::with_capacity(wire.len());
    for (index, message) in wire.iter().enumerate() {
        let role = message.role.canonical();
        let parts = match &message.content {
            WireContent::Text(text) => vec![text_part(text)],
            WireContent::Blocks(blocks) => {
                canonical_blocks(blocks, &format!("messages[{index}]"))?
            }
        };
        if parts.is_empty() {
            bail!("messages[{index}] must have non-empty content");
        }
        push_parts(&mut messages, role, parts);
    }
    Ok(messages)
}

/// Split `parts` into at most one role-bearing message plus one
/// [`Role::Tool`] message per tool result, preserving order.
fn push_parts(
    messages: &mut Vec<Message>,
    role: synthia::provider::Role,
    parts: Vec<ContentPart>,
) {
    let mut plain = Vec::new();
    for part in parts {
        let result = match part {
            ContentPart::ToolResult(result) => result,
            other => {
                plain.push(other);
                continue;
            }
        };
        if !plain.is_empty() {
            messages.push(message_of(role, std::mem::take(&mut plain)));
        }
        let tool_use_id = result.tool_use_id.clone();
        messages.push(Message::tool(
            Content::Single(ContentPart::ToolResult(result)),
            tool_use_id,
        ));
    }
    if !plain.is_empty() {
        messages.push(message_of(role, plain));
    }
}

fn message_of(
    role: synthia::provider::Role,
    parts: Vec<ContentPart>,
) -> Message {
    Message {
        role,
        content: Content::parts(parts),
        ..Message::default()
    }
}

/// Project one block list onto canonical parts.
fn canonical_blocks(
    blocks: &[WireBlock],
    where_: &str,
) -> Result<Vec<ContentPart>> {
    let mut parts = Vec::with_capacity(blocks.len());
    for (index, block) in blocks.iter().enumerate() {
        let part = match block {
            WireBlock::Text { text } => text_part(text),
            WireBlock::Image { source } => image_part(source),
            WireBlock::ToolUse { id, name, input } => {
                ContentPart::ToolUse(ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                })
            }
            WireBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => ContentPart::ToolResult(ToolResult {
                tool_use_id: tool_use_id.clone(),
                tool_name: None,
                content: tool_result_parts(content, where_, index)?,
                structured_content: None,
                is_error: *is_error,
                metadata: serde_json::Map::new(),
                truncated_by: None,
            }),
            WireBlock::Thinking {
                thinking,
                signature,
            } => ContentPart::Reasoning(ReasoningContent {
                text: thinking.clone(),
                signature: signature.clone(),
            }),
            WireBlock::Document { source, title } => {
                document_part(source, title.as_deref())
            }
        };
        parts.push(part);
    }
    Ok(parts)
}

/// Project a `tool_result`'s inner content.
///
/// The protocol allows `string | [text|image]`; anything else is
/// rejected rather than silently flattened, so a client that sends a
/// document-backed result learns why instead of losing the bytes.
fn tool_result_parts(
    content: &Option<WireContent>,
    where_: &str,
    index: usize,
) -> Result<Vec<ContentPart>> {
    let Some(content) = content else {
        return Ok(Vec::new());
    };
    let blocks = match content {
        WireContent::Text(text) => return Ok(vec![text_part(text)]),
        WireContent::Blocks(blocks) => blocks,
    };
    let mut parts = Vec::with_capacity(blocks.len());
    for (inner, block) in blocks.iter().enumerate() {
        let part = match block {
            WireBlock::Text { text } => text_part(text),
            WireBlock::Image { source } => image_part(source),
            _ => bail!(
                "{where_}.content[{index}].content[{inner}]: tool results support only `text` and `image` blocks"
            ),
        };
        parts.push(part);
    }
    Ok(parts)
}

/// Pop the final user turn off `messages`, returning its parts.
fn split_final_turn(messages: &mut Vec<Message>) -> Result<Vec<ContentPart>> {
    let Some(last) = messages.pop() else {
        bail!("messages must contain at least one message");
    };
    if last.role != synthia::provider::Role::User {
        bail!(
            "the final message must have role `user` (assistant prefills are not supported)"
        );
    }
    let parts: Vec<ContentPart> = last.content.into_iter().collect();
    if parts.is_empty() {
        bail!("the final message must have non-empty content");
    }
    Ok(parts)
}

/// Flatten a `system` field to plain text, dropping empty blocks.
fn flatten_system(prompt: &SystemPrompt) -> Option<String> {
    let joined = match prompt {
        SystemPrompt::Text(text) => text.trim().to_string(),
        SystemPrompt::Blocks(blocks) => blocks
            .iter()
            .map(|block| block.text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
    };
    (!joined.is_empty()).then_some(joined)
}

fn text_part(text: &str) -> ContentPart {
    ContentPart::Text(TextContent {
        text: text.to_string(),
        cache_control: None,
    })
}

/// Project an Anthropic image source onto [`ImageContent`].
///
/// The wire already says which spelling this is, so no `data:`-URL
/// classification is needed here: a `base64` source's payload is
/// stored verbatim (the canonical type accepts bare base64 as well as
/// a `data:` URL) and a `url` source stores the URL in `data`, which
/// is how the adapters recognise a remote image.
fn image_part(source: &ImageSource) -> ContentPart {
    let image = match source {
        ImageSource::Base64 { media_type, data } => ImageContent {
            data: data.clone(),
            mime_type: media_type.clone(),
            detail: None,
        },
        ImageSource::Url { url } => ImageContent {
            data: url.clone(),
            mime_type: url_mime_type(url).to_string(),
            detail: None,
        },
    };
    ContentPart::Image(image)
}

/// Project an Anthropic `document` source onto
/// [`ContentPart::Resource`].
///
/// The canonical model has no document part, so a file rides as the
/// same generic [`ResourceLink`] the old `/api/v1/chat/*` wire's
/// `kind: "file"` attachment produced (see `routes::chat::file_part`) —
/// which is what makes a transcript replayed through either endpoint
/// project identically:
///
/// - an inline source becomes the historical
///   `data:<media_type>;base64,<payload>` URI, with the media type
///   repeated in `mime_type`. The URI has to carry the bytes because
///   the [`ResourceLink`] is what the run persists and replays; the
///   field the adapters actually render is the URI (the OpenAI adapter
///   as `[Resource: <uri> - <name>]`, the Anthropic one as a
///   `[ResourceLink]` placeholder, that provider having no resource
///   block);
/// - a URL source keeps the URL as its `uri` and carries no media type
///   at all: the protocol's `url` spelling names no type, and guessing
///   one from the path extension would invent a fact the client did
///   not send.
///
/// `name` carries the block's optional `title`, which is the field a
/// client sends the file name in. It is what lets a reloaded transcript
/// show which file a turn carried instead of falling back to a bare
/// media type; a block without a title leaves it empty rather than
/// inventing one from the URI.
///
/// A malformed source never reaches here: [`DocumentSource`] is a
/// tagged enum whose own deserialization refuses an unknown `type` and
/// a missing field, and that sentence reaches the client because
/// [`WireContent`] deserializes block lists directly (see `wire`).
fn document_part(source: &DocumentSource, title: Option<&str>) -> ContentPart {
    let name = title.unwrap_or_default().to_string();
    let link = match source {
        DocumentSource::Base64 { media_type, data } => ResourceLink {
            uri: format!("data:{media_type};base64,{data}"),
            mime_type: Some(media_type.clone()),
            name: name.clone(),
            title: title.map(str::to_string),
            description: None,
        },
        DocumentSource::Url { url } => ResourceLink {
            uri: url.clone(),
            mime_type: None,
            name: name.clone(),
            title: title.map(str::to_string),
            description: None,
        },
    };
    ContentPart::Resource(link)
}

/// Best-effort media type for a `{type:"url"}` image source.
///
/// The canonical part requires a `mime_type` but the URL source has
/// none, so it is derived from the URL's path extension. Both provider
/// adapters ignore the field for a remote source (they pass the URL
/// through unchanged), and `image/*` is the honest answer for "an
/// image whose type the URL does not name".
fn url_mime_type(url: &str) -> &'static str {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let extension = path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "image/*",
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context as _;
    use synthia::provider::Role;

    use super::*;

    fn parse(body: serde_json::Value) -> Result<CanonicalRequest> {
        let request: MessagesRequest =
            serde_json::from_value(body).context("test body must parse")?;
        to_canonical(&request)
    }

    fn body(messages: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "model": "claude-test",
            "max_tokens": 64,
            "messages": messages,
        })
    }

    /// A `content: "<string>"` user turn is the request's whole payload:
    /// no history, one text part, no system, no tools.
    #[test]
    fn string_content_becomes_the_turn() {
        let canonical = parse(body(serde_json::json!([
            { "role": "user", "content": "hi" }
        ])))
        .unwrap();
        assert!(canonical.system.is_none());
        assert!(canonical.history.is_empty());
        assert!(canonical.tools.is_empty());
        assert_eq!(canonical.turn.len(), 1);
        assert_eq!(canonical.turn[0].text(), Some("hi"));
    }

    /// Multi-turn input: every message but the last lands in `history`
    /// with its own role, and the last becomes the turn.
    #[test]
    fn earlier_turns_become_history_with_their_roles() {
        let canonical = parse(body(serde_json::json!([
            { "role": "user", "content": "one" },
            { "role": "assistant", "content": "two" },
            { "role": "user", "content": "three" }
        ])))
        .unwrap();
        let roles: Vec<Role> =
            canonical.history.iter().map(|m| m.role).collect();
        assert_eq!(roles, vec![Role::User, Role::Assistant]);
        assert_eq!(
            canonical.history[1].content.extract_text().as_deref(),
            Some("two")
        );
        assert_eq!(canonical.turn[0].text(), Some("three"));
    }

    /// The `system` field's two spellings flatten to the same text, and
    /// an all-empty block list flattens to `None`.
    #[test]
    fn system_flattens_from_string_and_blocks() {
        let string_form = parse(serde_json::json!({
            "model": "m",
            "max_tokens": 8,
            "system": "be brief",
            "messages": [{ "role": "user", "content": "hi" }],
        }))
        .unwrap();
        assert_eq!(string_form.system.as_deref(), Some("be brief"));

        let block_form = parse(serde_json::json!({
            "model": "m",
            "max_tokens": 8,
            "system": [{ "type": "text", "text": "be brief" }],
            "messages": [{ "role": "user", "content": "hi" }],
        }))
        .unwrap();
        assert_eq!(block_form.system.as_deref(), Some("be brief"));

        let empty = parse(serde_json::json!({
            "model": "m",
            "max_tokens": 8,
            "system": [{ "type": "text", "text": "  " }],
            "messages": [{ "role": "user", "content": "hi" }],
        }))
        .unwrap();
        assert!(empty.system.is_none());
    }

    /// An inline image keeps its bytes and media type; a URL image
    /// stores the URL in `data` with a media type derived from its
    /// extension.
    #[test]
    fn image_sources_round_trip_into_image_parts() {
        let canonical = parse(body(serde_json::json!([{
            "role": "user",
            "content": [
                { "type": "text", "text": "look" },
                { "type": "image", "source": {
                    "type": "base64", "media_type": "image/png", "data": "AAAB"
                }},
                { "type": "image", "source": {
                    "type": "url", "url": "https://example.test/a.jpg"
                }},
            ],
        }])))
        .unwrap();
        let images: Vec<&ImageContent> = canonical
            .turn
            .iter()
            .filter_map(|part| match part {
                ContentPart::Image(image) => Some(image),
                _ => None,
            })
            .collect();
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].data, "AAAB");
        assert_eq!(images[0].mime_type, "image/png");
        assert_eq!(images[1].data, "https://example.test/a.jpg");
        assert_eq!(images[1].mime_type, "image/jpeg");
    }

    /// A URL with no recognised extension still yields an image part
    /// with the neutral `image/*` type.
    #[test]
    fn url_image_without_extension_uses_neutral_media_type() {
        assert_eq!(super::url_mime_type("https://x.test/i"), "image/*");
        assert_eq!(
            super::url_mime_type("https://x.test/i.png?w=2"),
            "image/png"
        );
    }

    /// A `document` block rides as the `ResourceLink` the old
    /// `/api/v1/chat/*` wire's file attachment produced: inline bytes
    /// become the historical `data:<media_type>;base64,<payload>` URI
    /// with the media type beside them, and a URL source keeps the URL.
    #[test]
    fn document_sources_become_resource_parts() {
        let canonical = parse(body(serde_json::json!([{
            "role": "user",
            "content": [
                { "type": "document", "source": {
                    "type": "base64",
                    "media_type": "application/pdf",
                    "data": "JVBERi0=",
                }},
                { "type": "document", "source": {
                    "type": "url",
                    "url": "https://example.test/spec.md",
                }},
            ],
        }])))
        .unwrap();
        let links: Vec<&ResourceLink> = canonical
            .turn
            .iter()
            .filter_map(|part| match part {
                ContentPart::Resource(link) => Some(link),
                _ => None,
            })
            .collect();
        assert_eq!(links.len(), 2, "both documents must convert: {links:?}");
        assert_eq!(links[0].uri, "data:application/pdf;base64,JVBERi0=");
        assert_eq!(links[0].mime_type.as_deref(), Some("application/pdf"));
        assert_eq!(links[1].uri, "https://example.test/spec.md");
        assert_eq!(
            links[1].mime_type, None,
            "a `url` source names no media type, so none may be invented"
        );
    }

    /// A `document` the endpoint cannot use is refused with a sentence
    /// naming what is wrong — the unknown `source.type`, the missing
    /// `source`, or the field a spelling requires — never converted to
    /// an empty part and never silently dropped.
    ///
    /// The sentences come from serde and reach the handler intact
    /// because [`WireContent`] deserializes block lists directly; under
    /// an untagged wrapper every one of them would collapse into the
    /// single "data did not match any variant" sentence and the client
    /// would be told nothing about what to fix.
    #[test]
    fn unusable_documents_are_named() {
        for (label, block, named) in [
            (
                "unknown source type",
                serde_json::json!({
                    "type": "document",
                    "source": { "type": "file", "file_id": "f1" },
                }),
                "unknown variant `file`",
            ),
            (
                "no source at all",
                serde_json::json!({ "type": "document" }),
                "missing field `source`",
            ),
            (
                "base64 source without its payload",
                serde_json::json!({
                    "type": "document",
                    "source": {
                        "type": "base64",
                        "media_type": "application/pdf",
                    },
                }),
                "missing field `data`",
            ),
            (
                "url source without a url",
                serde_json::json!({
                    "type": "document",
                    "source": { "type": "url" },
                }),
                "missing field `url`",
            ),
        ] {
            let error = parse(body(serde_json::json!([{
                "role": "user",
                "content": [block],
            }])))
            .err()
            .unwrap_or_else(|| panic!("{label} must be rejected"));
            let reason = format!("{error:#}");
            assert!(
                reason.contains(named),
                "{label} must name the problem ({named:?}): {reason}"
            );
            assert!(
                !reason.contains("untagged"),
                "{label} must not answer with serde's collapsed sentence: {reason}"
            );
        }
    }

    /// A `document` inside a `tool_result` is refused at its own
    /// position: a result may carry `text` and `image` blocks only, and
    /// the client must learn which block and which nesting was the
    /// problem rather than seeing the bytes vanish.
    #[test]
    fn a_document_inside_a_tool_result_is_named_by_position() {
        let error = parse(body(serde_json::json!([
            { "role": "user", "content": "run it" },
            { "role": "assistant", "content": [
                { "type": "tool_use", "id": "toolu_1", "name": "bash",
                  "input": { "cmd": "ls" } }
            ]},
            { "role": "user", "content": [
                { "type": "tool_result", "tool_use_id": "toolu_1",
                  "content": [
                    { "type": "text", "text": "output" },
                    { "type": "document", "source": {
                        "type": "base64",
                        "media_type": "application/pdf",
                        "data": "JVBERi0=",
                    }},
                  ]},
            ]},
            { "role": "user", "content": "go on" }
        ])))
        .expect_err("a document-backed tool result must be rejected");
        let reason = format!("{error:#}");
        assert!(
            reason.contains("messages[2].content[0].content[1]"),
            "the inner block's position must be named: {reason}"
        );
        assert!(
            reason.contains("tool results support only `text` and `image`"),
            "the reason must say what a result may carry: {reason}"
        );
    }

    /// `tool_result` blocks become their own `Role::Tool` messages
    /// (canonical shape) while the text around them stays a user
    /// message, so a replayed transcript matches a recorded one.
    #[test]
    fn tool_results_split_into_tool_role_messages() {
        let canonical = parse(body(serde_json::json!([
            { "role": "user", "content": "run it" },
            { "role": "assistant", "content": [
                { "type": "tool_use", "id": "toolu_1", "name": "bash",
                  "input": { "cmd": "ls" } }
            ]},
            { "role": "user", "content": [
                { "type": "text", "text": "and the output was:" },
                { "type": "tool_result", "tool_use_id": "toolu_1",
                  "content": "a.txt", "is_error": false }
            ]},
            { "role": "user", "content": "thanks" }
        ])))
        .unwrap();
        let roles: Vec<Role> =
            canonical.history.iter().map(|m| m.role).collect();
        assert_eq!(
            roles,
            vec![Role::User, Role::Assistant, Role::User, Role::Tool]
        );
        match &canonical.history[1].content {
            Content::Single(ContentPart::ToolUse(tool_use)) => {
                assert_eq!(tool_use.id, "toolu_1");
                assert_eq!(tool_use.name, "bash");
                assert_eq!(tool_use.input["cmd"], "ls");
            }
            other => {
                panic!("assistant turn must carry the tool_use: {other:?}")
            }
        }
        let result = &canonical.history[3];
        assert_eq!(result.tool_call_id.as_deref(), Some("toolu_1"));
        match &result.content {
            Content::Single(ContentPart::ToolResult(result)) => {
                assert_eq!(result.is_error, Some(false));
                assert_eq!(
                    result.content[0].text(),
                    Some("a.txt"),
                    "the result's text must survive"
                );
            }
            other => panic!("expected a tool result: {other:?}"),
        }
        assert_eq!(canonical.turn[0].text(), Some("thanks"));
    }

    /// A thinking block keeps its text and signature.
    #[test]
    fn thinking_blocks_keep_their_signature() {
        let canonical = parse(body(serde_json::json!([
            { "role": "assistant", "content": [
                { "type": "thinking", "thinking": "hmm", "signature": "sig" }
            ]},
            { "role": "user", "content": "go on" }
        ])))
        .unwrap();
        match &canonical.history[0].content {
            Content::Single(ContentPart::Reasoning(reasoning)) => {
                assert_eq!(reasoning.text, "hmm");
                assert_eq!(reasoning.signature.as_deref(), Some("sig"));
            }
            other => panic!("expected a reasoning part: {other:?}"),
        }
    }

    /// `tools` project onto canonical definitions, with a missing
    /// description defaulting to empty.
    #[test]
    fn tools_become_definitions() {
        let canonical = parse(serde_json::json!({
            "model": "m",
            "max_tokens": 8,
            "messages": [{ "role": "user", "content": "hi" }],
            "tools": [{
                "name": "get_weather",
                "description": "Look it up",
                "input_schema": { "type": "object", "properties": {} },
            }, {
                "name": "no_description",
                "input_schema": { "type": "object" },
            }],
        }))
        .unwrap();
        assert_eq!(canonical.tools.len(), 2);
        assert_eq!(canonical.tools[0].name, "get_weather");
        assert_eq!(canonical.tools[0].description, "Look it up");
        assert_eq!(
            canonical.tools[0].input_schema["type"],
            serde_json::json!("object")
        );
        assert_eq!(canonical.tools[1].description, "");
    }

    /// Unknown extra fields are ignored — the protocol grows optional
    /// fields and a client that sends one must not be rejected.
    #[test]
    fn unknown_request_fields_are_ignored() {
        let canonical = parse(serde_json::json!({
            "model": "m",
            "max_tokens": 8,
            "temperature": 0.7,
            "stop_sequences": ["\n\n"],
            "metadata": { "user_id": "u1" },
            "messages": [{
                "role": "user",
                "content": [{ "type": "text", "text": "hi",
                              "cache_control": { "type": "ephemeral" } }],
            }],
        }))
        .unwrap();
        assert_eq!(canonical.turn[0].text(), Some("hi"));
    }

    /// Unsupported and malformed shapes are rejected with a sentence
    /// naming the offending field.
    #[test]
    fn unusable_requests_are_rejected() {
        for (label, payload) in [
            (
                "no messages",
                serde_json::json!({
                    "model": "m", "max_tokens": 8, "messages": [],
                }),
            ),
            (
                "final role is assistant",
                body(serde_json::json!([
                    { "role": "assistant", "content": "hi" }
                ])),
            ),
            (
                "empty content list",
                body(serde_json::json!([
                    { "role": "user", "content": [] }
                ])),
            ),
            (
                "zero max_tokens",
                serde_json::json!({
                    "model": "m", "max_tokens": 0,
                    "messages": [{ "role": "user", "content": "hi" }],
                }),
            ),
            (
                "unknown role",
                body(serde_json::json!([
                    { "role": "system", "content": "hi" }
                ])),
            ),
            (
                "unknown image source",
                body(serde_json::json!([{
                    "role": "user",
                    "content": [{ "type": "image",
                                  "source": { "type": "file" } }],
                }])),
            ),
        ] {
            let error = parse(payload)
                .err()
                .unwrap_or_else(|| panic!("{label} must be rejected"));
            assert!(
                !error.to_string().is_empty(),
                "{label} must carry a reason"
            );
        }
    }
    /// A document's `title` is the field a client sends the file name in,
    /// so it must survive as the resource's `name`. Without this a
    /// reloaded transcript can only show a bare media type, and the user
    /// cannot tell which file the turn carried.
    #[test]
    fn a_documented_title_becomes_the_resource_name() {
        let canonical = parse(body(serde_json::json!([
            { "role": "user", "content": [
                { "type": "document", "title": "quarterly-report.pdf",
                  "source": { "type": "base64",
                              "media_type": "application/pdf",
                              "data": "JVBERi0=" } }
            ]}
        ])))
        .expect("a titled document is valid");
        let links: Vec<&ResourceLink> = canonical
            .turn
            .iter()
            .filter_map(|part| match part {
                ContentPart::Resource(link) => Some(link),
                _ => None,
            })
            .collect();
        assert_eq!(links.len(), 1, "got {links:?}");
        assert_eq!(links[0].name, "quarterly-report.pdf");
        assert_eq!(links[0].mime_type.as_deref(), Some("application/pdf"));
    }

    /// A document with no title leaves the name empty rather than
    /// inventing one — the renderer owns the fallback label, because only
    /// it knows what reads well for a given client.
    #[test]
    fn an_untitled_document_leaves_the_name_empty() {
        let canonical = parse(body(serde_json::json!([
            { "role": "user", "content": [
                { "type": "document",
                  "source": { "type": "url", "url": "https://x/a.pdf" } }
            ]}
        ])))
        .expect("an untitled document is valid");
        let links: Vec<&ResourceLink> = canonical
            .turn
            .iter()
            .filter_map(|part| match part {
                ContentPart::Resource(link) => Some(link),
                _ => None,
            })
            .collect();
        assert_eq!(links.len(), 1, "got {links:?}");
        assert!(links[0].name.is_empty());
        assert_eq!(links[0].uri, "https://x/a.pdf");
    }
}
