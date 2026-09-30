//! The 2 content methods on
//! [`super::core::OpenAICompatibleProvider`]:
//!
//! - [`OpenAICompatibleProvider::transform_content`] —
//!   dispatches on `Content::Single` vs `Content::Multi` to
//!   map each `ContentPart` via `transform_part`.
//! - [`OpenAICompatibleProvider::transform_part`] — maps a
//!   single [`crate::types::ContentPart`] to an
//!   [`super::types::OpenAIContentPart`]. Text/Image map 1:1;
//!   `Audio` becomes `input_audio` only for the containers this API
//!   family accepts (`wav`/`mp3`) and otherwise degrades to a text
//!   note; `ToolUse` keeps `id`/`name`/`input`;
//!   `ToolResult` becomes a flat text representation via
//!   `format!("{:?}", tr.content)` (the OpenAI API does not
//!   preserve the structured result); `Reasoning` is dropped (no
//!   request-side slot exists); `Resource` is projected by
//!   [`ResourceLink::prompt_projection`] — a bounded note carrying the
//!   name, the media type, and any small inline `text/*` payload, never
//!   the base64.
//!
//! [`ResourceLink::prompt_projection`]: crate::types::ResourceLink::prompt_projection

use super::{
    super::types::{OpenAIContentPart, OpenAIImageUrl},
    core::OpenAICompatibleProvider,
};

impl OpenAICompatibleProvider {
    pub(in crate::openai) fn transform_content(
        &self,
        content: &crate::types::Content,
    ) -> Vec<OpenAIContentPart> {
        match content {
            crate::types::Content::Single(part) => {
                self.transform_part(part).into_iter().collect()
            }
            crate::types::Content::Multi(parts) => parts
                .iter()
                .filter_map(|p| self.transform_part(p))
                .collect(),
        }
    }

    /// Project one canonical part onto the OpenAI-compatible wire
    /// vocabulary, or `None` when this API family has no request-side
    /// slot for it.
    ///
    /// Only [`ContentPart::Reasoning`] is such a part. Reasoning is an
    /// *output* concept here: `delta.reasoning_content` is read off the
    /// response (see `openai_streaming`), but no `/chat/completions`
    /// request body defines a `reasoning` content block — real OpenAI
    /// rejects an unknown `type`, and the gateways that do expose
    /// reasoning expose it on the way out only. Anthropic is the
    /// opposite: it *requires* the signed `thinking` block to be echoed
    /// back, which is why the same part survives
    /// `anthropic::transform_part` as a `ThinkingBlock`.
    ///
    /// The history keeps the reasoning part either way; dropping it
    /// happens here, at the last moment before the wire, where the
    /// adapter owns the wire shape.
    ///
    /// [`ContentPart::Reasoning`]: crate::types::ContentPart::Reasoning
    pub(in crate::openai) fn transform_part(
        &self,
        part: &crate::types::ContentPart,
    ) -> Option<OpenAIContentPart> {
        let mapped = match part {
            crate::types::ContentPart::Text(tc) => OpenAIContentPart::Text {
                text: tc.text.clone(),
            },
            crate::types::ContentPart::Image(ic) => {
                OpenAIContentPart::ImageUrl {
                    image_url: OpenAIImageUrl {
                        url: ic.to_url(),
                        detail: ic.detail.as_ref().map(|d| match d {
                            crate::types::ImageDetail::Low => "low".to_string(),
                            crate::types::ImageDetail::High => {
                                "high".to_string()
                            }
                            crate::types::ImageDetail::Auto => {
                                "auto".to_string()
                            }
                        }),
                    },
                }
            }
            crate::types::ContentPart::Audio(ac) => {
                // `input_audio` requires inline bytes in a container
                // this API family knows (`wav` / `mp3` only), and it has
                // no URL slot. Neither a wider container such as `flac`
                // nor a remote URL can be expressed, so each degrades to
                // a text note instead of a body the provider rejects.
                let Some(input_audio) = super::input_audio(ac) else {
                    return Some(OpenAIContentPart::Text {
                        text: format!("[Audio: {}]", ac.data),
                    });
                };
                OpenAIContentPart::InputAudio { input_audio }
            }
            crate::types::ContentPart::ToolUse(tu) => {
                OpenAIContentPart::ToolUse {
                    id: tu.id.clone(),
                    name: tu.wire_name(),
                    input: tu.wire_input(),
                }
            }
            crate::types::ContentPart::ToolResult(tr) => {
                OpenAIContentPart::ToolResult {
                    id: tr.tool_use_id.clone(),
                    content: format!("{:?}", tr.content),
                    is_error: tr.is_error.unwrap_or(false),
                }
            }
            crate::types::ContentPart::Reasoning(_) => return None,
            crate::types::ContentPart::Resource(link) => {
                OpenAIContentPart::Text {
                    text: link.prompt_projection(),
                }
            }
        };
        Some(mapped)
    }
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use serde_json::json;

    use crate::{
        Content,
        ContentPart,
        Message,
        ModelConfig,
        ReasoningContent,
        ResourceLink,
        Role,
        TextContent,
        ToolResult,
        ToolUse,
        openai::{
            provider::core::OpenAICompatibleProvider,
            types::OpenAIContentPart,
        },
        types::{AudioContent, AudioFormat},
    };

    fn provider() -> OpenAICompatibleProvider {
        OpenAICompatibleProvider::new(
            "https://api.openai.com/v1".to_string(),
            ModelConfig {
                name: "gpt-4o".to_string(),
                provider: "openai".to_string(),
                context_window: 128_000,
                max_output_tokens: 4096,
                supports_tools: true,
                supports_streaming: true,
                supports_reasoning: false,
            },
        )
    }

    // -- transform_content (Single / Multi) -----------------------

    /// `transform_content(Single(part))` MUST produce a 1-element
    /// Vec with the mapped part.
    #[test]
    fn transform_content_single_produces_one_part() {
        let p = provider();
        let c = Content::Single(ContentPart::Text(TextContent {
            text: "hello".to_string(),
            cache_control: None,
        }));
        let result = p.transform_content(&c);
        assert_eq!(result.len(), 1);
        match &result[0] {
            OpenAIContentPart::Text { text } => assert_eq!(text, "hello"),
            _ => panic!("expected Text, got {:?}", result[0]),
        }
    }

    /// `transform_content(Multi(parts))` MUST preserve order.
    #[test]
    fn transform_content_multi_preserves_order() {
        let p = provider();
        let c = Content::Multi(vec![
            ContentPart::Text(TextContent {
                text: "a".to_string(),
                cache_control: None,
            }),
            ContentPart::Text(TextContent {
                text: "b".to_string(),
                cache_control: None,
            }),
            ContentPart::Text(TextContent {
                text: "c".to_string(),
                cache_control: None,
            }),
        ]);
        let result = p.transform_content(&c);
        assert_eq!(result.len(), 3);
    }

    /// `transform_content(Multi(parts))` MUST map heterogeneous
    /// parts to the right OpenAIContentPart variant.
    #[test]
    fn transform_content_multi_mixed_variants() {
        let p = provider();
        let c = Content::Multi(vec![
            ContentPart::Text(TextContent {
                text: "x".to_string(),
                cache_control: None,
            }),
            ContentPart::ToolUse(ToolUse {
                id: "t1".to_string(),
                name: "bash".to_string(),
                input: json!({"cmd": "ls"}),
            }),
        ]);
        let result = p.transform_content(&c);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], OpenAIContentPart::Text { .. }));
        match &result[1] {
            OpenAIContentPart::ToolUse { id, name, .. } => {
                assert_eq!(id, "t1");
                assert_eq!(name, "bash");
            }
            _ => panic!("expected ToolUse, got {:?}", result[1]),
        }
    }

    // -- transform_part (per-variant) -----------------------------

    /// Text → Text with verbatim copy.
    #[test]
    fn transform_part_text_copies_verbatim() {
        let p = provider();
        let part = ContentPart::Text(TextContent {
            text: "verbatim".to_string(),
            cache_control: None,
        });
        let result = p.transform_part(&part).expect("part is representable");
        match result {
            OpenAIContentPart::Text { text } => assert_eq!(text, "verbatim"),
            _ => panic!("expected Text"),
        }
    }

    /// ToolUse → ToolUse with id/name/input preserved.
    #[test]
    fn transform_part_tool_use_preserves_fields() {
        let p = provider();
        let part = ContentPart::ToolUse(ToolUse {
            id: "call-1".to_string(),
            name: "search".to_string(),
            input: json!({"q": "rust"}),
        });
        let result = p.transform_part(&part).expect("part is representable");
        match result {
            OpenAIContentPart::ToolUse { id, name, input } => {
                assert_eq!(id, "call-1");
                assert_eq!(name, "search");
                assert_eq!(input, json!({"q": "rust"}));
            }
            _ => panic!("expected ToolUse"),
        }
    }

    /// ToolResult → ToolResult with Debug-formatted content and
    /// `is_error` defaulted to false when None.
    #[test]
    fn transform_part_tool_result_debug_formats_content() {
        let p = provider();
        let part = ContentPart::ToolResult(ToolResult::new("id", "ok"));
        let result = p.transform_part(&part).expect("part is representable");
        match result {
            OpenAIContentPart::ToolResult {
                id,
                content,
                is_error,
            } => {
                assert_eq!(id, "id");
                // Debug formatting of Vec<ContentPart> produces
                // something like `[Text { text: "ok", ... }]`.
                assert!(content.contains("ok"), "got: {content}");
                assert!(!is_error);
            }
            _ => panic!("expected ToolResult"),
        }
    }

    /// ToolResult with is_error=true MUST preserve the flag.
    #[test]
    fn transform_part_tool_result_propagates_is_error() {
        let p = provider();
        let part = ContentPart::ToolResult(ToolResult::error("id", "failed"));
        let result = p.transform_part(&part).expect("part is representable");
        match result {
            OpenAIContentPart::ToolResult { is_error, .. } => {
                assert!(is_error);
            }
            _ => panic!("expected ToolResult"),
        }
    }

    /// A `flac` audio part MUST degrade to a text note rather than an
    /// `input_audio` block — on either route to the label.
    ///
    /// The shared `AudioContent::format_label` reports `"flac"` both for
    /// an explicit `AudioFormat::Flac` and for an `audio/flac` /
    /// `audio/x-flac` media type — Anthropic accepts it — but
    /// `input_audio.format` accepts only `wav` / `mp3`, so forwarding
    /// the label produces a request the provider answers with a 400.
    /// This site degrades instead, exactly as it already did for a
    /// remote URL.
    #[test]
    fn transform_part_flac_audio_degrades_to_text_note() {
        let p = provider();
        for (format, mime_type) in [
            (Some(AudioFormat::Flac), "audio/x-flac"),
            (None, "audio/flac"),
        ] {
            let part = ContentPart::Audio(AudioContent {
                data: "ZkxhYw==".to_string(),
                mime_type: mime_type.to_string(),
                format,
            });
            match p.transform_part(&part).expect("part is representable") {
                OpenAIContentPart::Text { text } => {
                    assert_eq!(text, "[Audio: ZkxhYw==]");
                }
                other => {
                    panic!(
                        "flac/{mime_type} has no input_audio slot; got {other:?}"
                    )
                }
            }
        }
    }

    /// `wav` and `mp3` parts MUST still reach the wire as `input_audio`
    /// carrying the provider's container label — not the `audio/...`
    /// media type. One row per label route: explicit `format`, and the
    /// `mime_type` fallback a bare upload arrives with.
    #[test]
    fn transform_part_wav_and_mp3_emit_input_audio() {
        let p = provider();
        for (format, mime_type, expected) in [
            (Some(AudioFormat::Wav), "audio/x-wav", "wav"),
            (None, "audio/mpeg", "mp3"),
        ] {
            let part = ContentPart::Audio(AudioContent {
                data: "QUJD".to_string(),
                mime_type: mime_type.to_string(),
                format,
            });
            match p.transform_part(&part).expect("part is representable") {
                OpenAIContentPart::InputAudio { input_audio } => {
                    assert_eq!(input_audio.data, "QUJD");
                    assert_eq!(input_audio.format, expected);
                }
                other => {
                    panic!(
                        "expected input_audio for {mime_type}; got {other:?}"
                    )
                }
            }
        }
    }

    /// A `text/*` resource arrives as its own text, not as base64.
    ///
    /// An uploaded document rides the canonical wire as a `data:` URI
    /// holding the whole file base64-encoded. Projecting that URI
    /// verbatim (which this adapter used to do) put the payload in the
    /// prompt — context spent, nothing learned.
    #[test]
    fn transform_part_resource_inlines_small_text_payload() {
        let p = provider();
        let payload = BASE64.encode(b"# notes\n\nhello world\n");
        let part = ContentPart::Resource(ResourceLink {
            uri: format!("data:text/plain;base64,{payload}"),
            name: "notes.md".to_string(),
            title: None,
            description: None,
            mime_type: Some("text/plain".to_string()),
        });
        match p.transform_part(&part).expect("part is representable") {
            OpenAIContentPart::Text { text } => {
                assert!(text.contains("hello world"), "got: {text}");
                assert!(text.contains("notes.md"), "got: {text}");
                assert!(
                    !text.contains(&payload),
                    "base64 must never reach the prompt: {text}"
                );
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    /// A large binary resource is a short note, not a prompt full of
    /// base64: the emitted text stays bounded however big the payload
    /// is, and the filename survives.
    ///
    /// A 1 MB PDF encodes to ~1.4 M characters — enough to blow the
    /// context window on its own. The bound, not the wording, is the
    /// invariant here.
    #[test]
    fn transform_part_resource_bounds_a_large_binary_payload() {
        let p = provider();
        let payload = BASE64.encode(vec![0u8; 1024 * 1024]);
        let part = ContentPart::Resource(ResourceLink {
            uri: format!("data:application/pdf;base64,{payload}"),
            name: "report.pdf".to_string(),
            title: None,
            description: None,
            mime_type: Some("application/pdf".to_string()),
        });
        match p.transform_part(&part).expect("part is representable") {
            OpenAIContentPart::Text { text } => {
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

    /// A `text/*` payload past the inline limit is left out of the
    /// prompt, and the note says so — the model can tell the user what
    /// it was handed instead of silently seeing nothing.
    #[test]
    fn transform_part_resource_notes_an_over_limit_text_payload() {
        let p = provider();
        let payload = BASE64.encode(vec![b'x'; 64 * 1024]);
        let part = ContentPart::Resource(ResourceLink {
            uri: format!("data:text/plain;base64,{payload}"),
            name: "notes.txt".to_string(),
            title: None,
            description: None,
            mime_type: Some("text/plain".to_string()),
        });
        match p.transform_part(&part).expect("part is representable") {
            OpenAIContentPart::Text { text } => {
                assert!(
                    text.len() < 300,
                    "a 64 KiB payload must not be inlined; got {} chars",
                    text.len()
                );
                assert!(text.contains("notes.txt"), "got: {text}");
                assert!(text.contains("not inlined"), "got: {text}");
                assert!(
                    !text.contains(&payload[..64]),
                    "no base64 may survive: {text}"
                );
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    /// A resource that is not an inline `data:` URI — a `file:` or
    /// `https:` reference — still reaches the model by name and type.
    #[test]
    fn transform_part_resource_names_a_referenced_uri() {
        let p = provider();
        let part = ContentPart::Resource(ResourceLink {
            uri: "file:///tmp/x.txt".to_string(),
            name: "x.txt".to_string(),
            title: None,
            description: None,
            mime_type: Some("text/plain".to_string()),
        });
        match p.transform_part(&part).expect("part is representable") {
            OpenAIContentPart::Text { text } => {
                assert!(text.contains("x.txt"), "got: {text}");
                assert!(text.contains("text/plain"), "got: {text}");
                assert!(text.contains("not inlined"), "got: {text}");
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    /// Reasoning has no request-side slot in this API family, so it is
    /// dropped rather than sent as a `reasoning` content block.
    ///
    /// Real OpenAI rejects an unknown `type`, and the gateways that do
    /// expose reasoning expose it on the response only. Anthropic is the
    /// opposite — it *requires* the block back — which is why the same
    /// part survives `anthropic::transform_part` as a `ThinkingBlock`.
    /// Both halves matter: this one keeps the request valid, and the
    /// harness commits the part to history so the Anthropic half has
    /// something to echo.
    #[test]
    fn transform_part_reasoning_is_dropped() {
        let p = provider();
        let part = ContentPart::Reasoning(ReasoningContent {
            text: "thinking step".to_string(),
            signature: None,
        });
        assert!(
            p.transform_part(&part).is_none(),
            "reasoning must not reach an OpenAI-compatible request body"
        );
    }

    /// A message whose only content is reasoning reduces to nothing and
    /// is dropped, rather than sent as a `content: []` body.
    #[test]
    fn transform_message_drops_reasoning_only_turn() {
        let p = provider();
        let msg = Message {
            role: Role::Assistant,
            content: Content::Multi(vec![ContentPart::Reasoning(
                ReasoningContent {
                    text: "only thinking".to_string(),
                    signature: Some("sig".to_string()),
                },
            )]),
            tool_call_id: None,
            name: None,
            ..Default::default()
        };
        assert!(
            p.transform_message(&msg).is_none(),
            "a reasoning-only turn has nothing to send and must be dropped"
        );
    }

    /// …but a turn that also carries text keeps the text and loses only
    /// the reasoning.
    #[test]
    fn transform_message_keeps_text_next_to_reasoning() {
        let p = provider();
        let msg = Message {
            role: Role::Assistant,
            content: Content::Multi(vec![
                ContentPart::Reasoning(ReasoningContent {
                    text: "thinking".to_string(),
                    signature: None,
                }),
                ContentPart::Text(TextContent {
                    text: "the answer".to_string(),
                    cache_control: None,
                }),
            ]),
            tool_call_id: None,
            name: None,
            ..Default::default()
        };
        let out = p
            .transform_message(&msg)
            .expect("a turn with text is representable");
        let parts = out.content.expect("content present");
        assert_eq!(parts.len(), 1, "only the text survives; got {parts:?}");
        match &parts[0] {
            OpenAIContentPart::Text { text } => {
                assert_eq!(text, "the answer");
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }
}
