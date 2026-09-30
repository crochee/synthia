//! The Anthropic Messages wire contract, owned by `synthia-server`.
//!
//! The provider crate's Anthropic types cannot be reused for this
//! surface: `synthia::provider::anthropic::types` is `pub(super)` (its
//! items are deliberately crate-private) and the stream types that
//! crate *does* export (`AnthropicStreamEvent` and friends) derive
//! `Deserialize` only — they are the outbound client view, so they can
//! neither be serialised into a response nor carry this endpoint's
//! error envelope. The DTOs below therefore live here, next to the
//! handler that uses them.
//!
//! Unknown request fields are accepted and ignored on purpose. The
//! protocol keeps growing optional fields (`metadata`, `top_p`,
//! `tool_choice`, `thinking`, `stop_sequences`, …) that a compatible
//! client may send and this endpoint has no opinion about; rejecting
//! them would break clients for no benefit. Fields this endpoint does
//! *not* act on are deliberately absent from the request DTO rather
//! than parsed and dropped — see the module docs on
//! [`super`] for the list.

use std::fmt;

use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    de::{Error, SeqAccess, Visitor, value::SeqAccessDeserializer},
};
use serde_json::Value;

/// `POST /v1/messages` request body.
#[derive(Debug, Deserialize)]
pub(super) struct MessagesRequest {
    /// Echoed back on the response. The agent run itself is resolved
    /// through the server's own agent/provider configuration; this
    /// endpoint does not switch models.
    pub(super) model: String,
    /// Required by the protocol. Validated (non-zero) and otherwise
    /// not plumbed — the run's token cap belongs to the deployment's
    /// model configuration, exactly as it does for `/api/v1/chat/*`.
    pub(super) max_tokens: u32,
    pub(super) messages: Vec<WireMessage>,
    #[serde(default)]
    pub(super) system: Option<SystemPrompt>,
    #[serde(default)]
    pub(super) tools: Option<Vec<WireTool>>,
    #[serde(default)]
    pub(super) stream: Option<bool>,
    /// The protocol's free-form `metadata` object (`user_id` is the
    /// field Anthropic documents). Everything in it is ignored except
    /// the one namespaced key below; see [`Metadata`].
    #[serde(default)]
    pub(super) metadata: Option<Metadata>,
}

/// The `metadata` object, of which this endpoint reads two keys.
///
/// `user_id` belongs to the protocol, so Synthia's own web client names
/// what it is asking for under namespaced keys instead:
/// `metadata.synthia_session_id` (the conversation to continue) and
/// `metadata.synthia_agent_name` (the agent to answer it). An external
/// Anthropic client never sends either, which is what keeps the
/// extension invisible to the protocol's own contract — and the session
/// key is also the gate for the tool blocks a Synthia client needs and
/// an external one must never be handed (see [`super::super`]).
#[derive(Debug, Deserialize)]
pub(super) struct Metadata {
    #[serde(default)]
    pub(super) synthia_session_id: Option<String>,
    #[serde(default)]
    pub(super) synthia_agent_name: Option<String>,
}

/// One inbound message. Anthropic has exactly two message roles;
/// `system` is a top-level field, not a role.
#[derive(Debug, Deserialize)]
pub(super) struct WireMessage {
    pub(super) role: WireRole,
    pub(super) content: WireContent,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum WireRole {
    User,
    Assistant,
}

impl WireRole {
    /// Canonical role for this wire role.
    pub(super) fn canonical(&self) -> synthia::provider::Role {
        match self {
            Self::User => synthia::provider::Role::User,
            Self::Assistant => synthia::provider::Role::Assistant,
        }
    }
}

/// `string | [block]` — the protocol's two content spellings.
///
/// The two spellings are told apart by the JSON token rather than by
/// `#[serde(untagged)]`, and the reason is observable: serde collapses
/// *every* failure inside an untagged variant into "data did not match
/// any variant of untagged enum WireContent", discarding the variant's
/// own sentence. Under it, an unknown block `type` and a block missing
/// a required field both answer with that sentence — the client is
/// told nothing about what to fix. Deserializing the list on its own
/// lets `WireBlock`'s error through, so a malformed block names its
/// variant, its missing field, and where the body went wrong.
#[derive(Debug)]
pub(super) enum WireContent {
    Text(String),
    Blocks(Vec<WireBlock>),
}

impl<'de> Deserialize<'de> for WireContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        /// Accepts either spelling; every other token is refused with
        /// this visitor's `expecting` sentence.
        struct Spellings;

        impl<'de> Visitor<'de> for Spellings {
            type Value = WireContent;

            fn expecting(
                &self,
                formatter: &mut fmt::Formatter<'_>,
            ) -> fmt::Result {
                formatter.write_str("a string or an array of content blocks")
            }

            fn visit_str<E>(self, text: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(WireContent::Text(text.to_string()))
            }

            fn visit_string<E>(self, text: String) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(WireContent::Text(text))
            }

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                // Straight through to the real deserializer rather than
                // via an intermediate `Value`: the list is never
                // buffered, and a bad block reports its position in the
                // body.
                Vec::<WireBlock>::deserialize(SeqAccessDeserializer::new(seq))
                    .map(WireContent::Blocks)
            }
        }

        deserializer.deserialize_any(Spellings)
    }
}

/// `string | [ {type: "text", text} ]` — the `system` field's two
/// spellings.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum SystemPrompt {
    Text(String),
    Blocks(Vec<SystemBlock>),
}

#[derive(Debug, Deserialize)]
pub(super) struct SystemBlock {
    pub(super) text: String,
}

/// One inbound content block.
///
/// `document` is the protocol's *file* block — a PDF, or a plain text
/// or Markdown file the client attached — spelled exactly like an
/// `image`: a `source` that is inline base64 bytes or a URL.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum WireBlock {
    Text {
        text: String,
    },
    Image {
        source: ImageSource,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        /// `string | [text|image block]` — the same two spellings a
        /// message's `content` has, so the same type reads it and the
        /// same clear rejection (rather than an untagged one) reaches
        /// a client whose result carries something else.
        #[serde(default)]
        content: Option<WireContent>,
        #[serde(default)]
        is_error: Option<bool>,
    },
    Thinking {
        thinking: String,
        #[serde(default)]
        signature: Option<String>,
    },
    Document {
        source: DocumentSource,
        /// The protocol's optional document title — the field a client
        /// sends the file name in. Carried through so a reloaded
        /// transcript can name the file instead of falling back to a
        /// bare media type.
        #[serde(default)]
        title: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ImageSource {
    Base64 { media_type: String, data: String },
    Url { url: String },
}

/// `document.source` — the two spellings the protocol defines for a
/// file: inline base64 bytes with the file's media type, or a URL.
///
/// Structurally the same pair as [`ImageSource`], and deliberately a
/// separate type: a `source` is named for the block that carries it,
/// and the two are free to diverge (only an image's URL source derives
/// a media type from its path). A `type` neither variant names is
/// refused with a sentence quoting the value, and each spelling's
/// required fields are enforced, because [`WireContent`] deserializes
/// block lists directly rather than through an untagged wrapper that
/// would discard these messages.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum DocumentSource {
    Base64 { media_type: String, data: String },
    Url { url: String },
}

/// One entry of the request's `tools` array.
#[derive(Debug, Deserialize)]
pub(super) struct WireTool {
    pub(super) name: String,
    #[serde(default)]
    pub(super) description: Option<String>,
    pub(super) input_schema: Value,
}

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

/// `"message"` — the `type` discriminant of a full response.
pub(super) const MESSAGE_KIND: &str = "message";
/// `"assistant"` — the only role this endpoint produces.
pub(super) const ASSISTANT_ROLE: &str = "assistant";
/// `"error"` — the `type` discriminant of the error envelope.
pub(super) const ERROR_KIND: &str = "error";

/// The six streaming event names, in the order the protocol sends
/// them. Each is both the SSE `event:` name and the payload's `type`.
pub(super) const MESSAGE_START: &str = "message_start";
pub(super) const CONTENT_BLOCK_START: &str = "content_block_start";
pub(super) const CONTENT_BLOCK_DELTA: &str = "content_block_delta";
pub(super) const CONTENT_BLOCK_STOP: &str = "content_block_stop";
pub(super) const MESSAGE_DELTA: &str = "message_delta";
pub(super) const MESSAGE_STOP: &str = "message_stop";
/// The mid-stream failure event.
pub(super) const ERROR: &str = "error";

/// Anthropic error types this endpoint emits.
///
/// `invalid_request_error` covers every body/parameter rejection;
/// `api_error` covers a run that failed after the request was accepted.
/// The `authentication_error` a rejected credential produces is owned —
/// and named — by the auth middleware that emits it.
pub(super) mod error_type {
    pub(crate) const INVALID_REQUEST: &str = "invalid_request_error";
    pub(crate) const API: &str = "api_error";
}

/// `usage` block of a response, and of a `message_start` /
/// `message_delta` streaming frame.
///
/// The token counts are the run's totals across every sampling pass
/// the agent made: the message this endpoint returns covers one whole
/// *run*, which for a tool-using turn is several provider calls.
#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct Usage {
    pub(super) input_tokens: u64,
    pub(super) output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) cache_creation_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) cache_read_input_tokens: Option<u64>,
}

/// A full message: the non-streaming body, the `message` object of a
/// `message_start` frame, and the payload of the JSON response.
#[derive(Clone, Debug, Serialize)]
pub(super) struct MessageEnvelope {
    pub(super) id: String,
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) role: &'static str,
    pub(super) model: String,
    pub(super) content: Vec<ResponseBlock>,
    pub(super) stop_reason: Option<String>,
    pub(super) stop_sequence: Option<String>,
    pub(super) usage: Usage,
}

/// One block of the assistant's response.
///
/// `Text`, `Thinking` and `ToolUse` are the protocol's own assistant
/// blocks. `ToolResult` is the endpoint's one documented extension: the
/// protocol carries a tool result inside a *user* message (there is no
/// assistant `tool_result` block), but this server executes the tools
/// itself, so the only place a Synthia client can observe one in the
/// same turn is inline in the assistant's content. It is emitted **only**
/// for a request that opted in through `metadata.synthia_session_id`
/// (see [`super::super`]); an external client never receives it.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ResponseBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
        /// Always present, `""` when the runtime had no signature to
        /// propagate. Anthropic's own SDKs model the field as a
        /// non-optional string, so omitting it breaks their parsers;
        /// see [`super::blocks`] for why a streamed thinking block
        /// usually has none.
        signature: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        /// The executed tool's name, when the runtime carried one. The
        /// protocol's own `tool_result` has no such field — it is added
        /// here because a client that pairs call→result by id alone
        /// cannot label a result whose call it never saw (an interceptor
        /// or a replayed transcript can produce one).
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        content: Vec<ResultBlock>,
        /// Never omitted: a client must be able to tell a failed tool
        /// from one that succeeded without inferring it from the text.
        is_error: bool,
    },
}

/// One block of a `tool_result`'s `content` array.
///
/// The protocol spells a result's content `string | [text|image block]`;
/// the runtime's tool results are already canonical parts, so the two
/// Anthropic block spellings are modelled and anything else is flattened
/// to text (see [`super::blocks`]).
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ResultBlock {
    Text { text: String },
    Image { source: ResultImageSource },
}

/// `image.source` inside a result block — the two spellings the
/// protocol defines, and the pair [`super::convert`] reads back in.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ResultImageSource {
    Base64 { media_type: String, data: String },
    Url { url: String },
}

impl ResponseBlock {
    /// Seed an empty `text` block — the shape `content_block_start`
    /// carries before any delta.
    pub(super) fn empty_text() -> Self {
        Self::Text {
            text: String::new(),
        }
    }

    /// Seed an empty `thinking` block.
    pub(super) fn empty_thinking() -> Self {
        Self::Thinking {
            thinking: String::new(),
            signature: String::new(),
        }
    }
}

/// One `content_block_delta` payload.
///
/// The variants are named for their *shape* and renamed to the
/// protocol's delta kinds on the wire, which is what a client reads.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub(super) enum Delta {
    #[serde(rename = "text_delta")]
    Text { text: String },
    #[serde(rename = "input_json_delta")]
    InputJson { partial_json: String },
    #[serde(rename = "thinking_delta")]
    Thinking { thinking: String },
    #[serde(rename = "signature_delta")]
    Signature { signature: String },
}

/// `message_delta.delta` — the turn's terminal metadata.
#[derive(Debug, Serialize)]
pub(super) struct MessageDelta {
    pub(super) stop_reason: String,
    pub(super) stop_sequence: Option<String>,
}

/// `content_block_start` body.
#[derive(Debug, Serialize)]
pub(super) struct ContentBlockStart {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) index: usize,
    pub(super) content_block: ResponseBlock,
}

/// `content_block_delta` body.
#[derive(Debug, Serialize)]
pub(super) struct ContentBlockDelta {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) index: usize,
    pub(super) delta: Delta,
}

/// `content_block_stop` body.
#[derive(Debug, Serialize)]
pub(super) struct ContentBlockStop {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) index: usize,
}

/// `message_start` body.
#[derive(Debug, Serialize)]
pub(super) struct MessageStart {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) message: MessageEnvelope,
}

/// `message_delta` body.
#[derive(Debug, Serialize)]
pub(super) struct MessageDeltaBody {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) delta: MessageDelta,
    pub(super) usage: Usage,
}

/// `message_stop` body. The protocol sends `{"type":"message_stop"}`.
#[derive(Debug, Serialize)]
pub(super) struct MessageStop {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
}

/// `{"type":"error","error":{"type":…,"message":…}}` — the envelope
/// every failure of this endpoint carries, including the 401 the auth
/// middleware produces for `/v1/messages`.
#[derive(Debug, Serialize)]
pub(super) struct ErrorEnvelope {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) error: ErrorDetail,
}

#[derive(Debug, Serialize)]
pub(super) struct ErrorDetail {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) message: String,
}

impl ErrorEnvelope {
    pub(super) fn new(
        error_kind: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind: ERROR_KIND,
            error: ErrorDetail {
                kind: error_kind,
                message: message.into(),
            },
        }
    }

    /// Serialise the envelope as `{"type":"error", …}`.
    pub(super) fn body(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"type":"error","error":{"type":"api_error","message":"serialisation failed"}}"#
                .to_string()
        })
    }
}
