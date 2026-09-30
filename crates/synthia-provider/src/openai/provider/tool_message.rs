//! The tool-message transform method on
//! [`super::core::OpenAICompatibleProvider`]:
//!
//! - [`OpenAICompatibleProvider::transform_tool_message`] —
//!   builds the `OpenAIMessage` for a `role == "tool"`
//!   message. It walks the content looking for
//!   `ContentPart::ToolResult(tr)`, extracts the text body
//!   (joined with `\n`) and the first media part
//!   (image/audio from `tr.content`); `tool_call_id` is
//!   `msg.tool_call_id` or, as a fallback, the first
//!   `tr.tool_use_id` found in the content.

use super::{
    super::types::{OpenAIContentPart, OpenAIImageUrl, OpenAIMessage},
    core::OpenAICompatibleProvider,
};

impl OpenAICompatibleProvider {
    pub(in crate::openai) fn transform_tool_message(
        &self,
        msg: &crate::types::Message,
    ) -> OpenAIMessage {
        fn extract_text_from_content(
            content: &[crate::types::ContentPart],
        ) -> String {
            let texts: Vec<String> = content
                .iter()
                .filter_map(|p| {
                    if let crate::types::ContentPart::Text(tc) = p {
                        Some(tc.text.clone())
                    } else {
                        None
                    }
                })
                .collect();
            texts.join("\n")
        }

        fn extract_media_parts(
            content: &[&crate::types::ContentPart],
        ) -> Vec<OpenAIContentPart> {
            content
                .iter()
                .filter_map(|p| match *p {
                    crate::types::ContentPart::Image(ic) => {
                        Some(OpenAIContentPart::ImageUrl {
                            image_url: OpenAIImageUrl {
                                url: ic.to_url(),
                                detail: ic.detail.as_ref().map(|d| match d {
                                    crate::types::ImageDetail::Low => {
                                        "low".to_string()
                                    }
                                    crate::types::ImageDetail::High => {
                                        "high".to_string()
                                    }
                                    crate::types::ImageDetail::Auto => {
                                        "auto".to_string()
                                    }
                                }),
                            },
                        })
                    }
                    crate::types::ContentPart::Audio(ac) => {
                        // Same narrowing as `transform_part`:
                        // `input_audio` accepts only `wav`/`mp3` inline
                        // bytes, so anything else (a `flac` upload, a
                        // remote URL) has no slot here and the part is
                        // dropped from the tool result rather than
                        // producing an invalid body.
                        super::input_audio(ac).map(|input_audio| {
                            OpenAIContentPart::InputAudio { input_audio }
                        })
                    }
                    crate::types::ContentPart::ToolResult(tr) => {
                        let inner: Vec<&crate::types::ContentPart> =
                            tr.content.iter().collect();
                        let media = extract_media_parts(&inner);
                        if !media.is_empty() {
                            media.into_iter().next()
                        } else {
                            None
                        }
                    }
                    _ => None,
                })
                .collect()
        }

        let content_parts: Vec<&crate::types::ContentPart> = match &msg.content
        {
            crate::types::Content::Single(p) => vec![p],
            crate::types::Content::Multi(ps) => ps.iter().collect(),
        };

        // A tool message's content is EITHER the canonical
        // `ToolResult` wrapper (what the session sink's projection
        // writes) OR the tool's own result parts, unwrapped — which is
        // exactly what the harness commits for a live run:
        // `Message::tool(Content::parts(tr.content.clone()), call_id)`.
        //
        // Reading only the wrapper left `content` empty for every
        // harness-built message, so the wire body was
        // `{"role":"tool","content":""}` and the model was told the tool
        // had produced nothing. It then reported exactly that back to
        // the user, while the real output sat in the session log
        // (`ContentPart::ToolResult` reaches that log by a different
        // path) — which is why the transcript looked correct.
        let mut texts: Vec<String> = Vec::new();
        let mut tool_use_id: Option<String> = None;
        for part in &content_parts {
            match part {
                crate::types::ContentPart::ToolResult(tr) => {
                    if tool_use_id.is_none() {
                        tool_use_id = Some(tr.tool_use_id.clone());
                    }
                    texts.push(extract_text_from_content(&tr.content));
                }
                crate::types::ContentPart::Text(tc) => {
                    texts.push(tc.text.clone())
                }
                // Everything else (media, tool_use, reasoning,
                // resource) carries no text body; media is projected
                // by `extract_media_parts` below.
                _ => {}
            }
        }
        let content_str = texts
            .into_iter()
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        let mut parts: Vec<OpenAIContentPart> = if content_str.is_empty() {
            Vec::new()
        } else {
            vec![OpenAIContentPart::Text { text: content_str }]
        };
        parts.extend(extract_media_parts(&content_parts));

        OpenAIMessage {
            role: "tool".to_string(),
            content: if parts.is_empty() {
                Some(vec![OpenAIContentPart::Text {
                    text: String::new(),
                }])
            } else {
                Some(parts)
            },
            tool_calls: None,
            tool_call_id: msg.tool_call_id.clone().or(tool_use_id),
            name: msg.name.clone(),
            reasoning_content: None,
            reasoning: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{
        Content,
        ContentPart,
        Message,
        ModelConfig,
        Role,
        ToolResult,
        ToolUse,
        openai::{
            provider::core::OpenAICompatibleProvider,
            types::{OpenAIContentPart, OpenAIMessage},
        },
        types::{AudioContent, AudioFormat, TextContent},
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

    /// Empty content with no tool_call_id MUST produce
    /// `role = "tool"` and an empty-text `parts` placeholder
    /// (NOT `None`).
    #[test]
    fn transform_empty_message_yields_role_tool_with_placeholder() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::text(""),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        assert_eq!(result.role, "tool");
        // Empty content_str + no media → vec with single empty Text part.
        match &result.content {
            Some(parts) => {
                assert_eq!(parts.len(), 1);
                match &parts[0] {
                    OpenAIContentPart::Text { text } => {
                        assert_eq!(text, "");
                    }
                    _ => panic!("expected Text, got {:?}", parts[0]),
                }
            }
            None => panic!("expected Some(parts)"),
        }
        assert!(result.tool_call_id.is_none());
        assert!(result.tool_calls.is_none());
        assert!(result.name.is_none());
    }

    /// `msg.tool_call_id` MUST win over `tr.tool_use_id` when both
    /// are present (explicit field overrides content-derived value).
    #[test]
    fn transform_msg_tool_call_id_overrides_content() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::Single(ContentPart::ToolResult(ToolResult::new(
                "use-id-1", "ok",
            ))),
            tool_call_id: Some("explicit-id".to_string()),
            name: None,
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        assert_eq!(result.tool_call_id, Some("explicit-id".to_string()));
    }

    /// When `msg.tool_call_id` is None, the first
    /// `tr.tool_use_id` from a `Content::Multi` MUST be used.
    #[test]
    fn transform_falls_back_to_first_tool_use_id() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::Multi(vec![
                ContentPart::ToolResult(ToolResult::new("first-id", "r1")),
                ContentPart::ToolResult(ToolResult::new("second-id", "r2")),
            ]),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        assert_eq!(result.tool_call_id, Some("first-id".to_string()));
    }

    /// `Content::Single(ToolResult)` MUST produce a Text part
    /// with the joined text body.
    #[test]
    fn transform_single_tool_result_joins_text_with_newline() {
        let p = provider();
        let mut tr = ToolResult::new("id", "");
        tr.content = vec![
            ContentPart::Text(TextContent {
                text: "line1".to_string(),
                cache_control: None,
            }),
            ContentPart::Text(TextContent {
                text: "line2".to_string(),
                cache_control: None,
            }),
        ];
        let msg = Message {
            role: Role::Tool,
            content: Content::Single(ContentPart::ToolResult(tr)),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        let parts = result.content.expect("content");
        // First part is the joined text.
        match &parts[0] {
            OpenAIContentPart::Text { text } => {
                assert_eq!(text, "line1\nline2");
            }
            _ => panic!("expected Text, got {:?}", parts[0]),
        }
    }

    /// `Content::Multi` with multiple `ToolResult`s MUST join
    /// their text bodies with `\n`.
    #[test]
    fn transform_multi_tool_results_join_with_newline() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::Multi(vec![
                ContentPart::ToolResult(ToolResult::new("a", "alpha")),
                ContentPart::ToolResult(ToolResult::new("b", "beta")),
            ]),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        let parts = result.content.expect("content");
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            OpenAIContentPart::Text { text } => {
                assert_eq!(text, "alpha\nbeta");
            }
            _ => panic!("expected Text"),
        }
    }

    /// `msg.name` MUST be forwarded verbatim to the wire message.
    #[test]
    fn transform_forwards_name() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::text(""),
            tool_call_id: None,
            name: Some("bash".to_string()),
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        assert_eq!(result.name, Some("bash".to_string()));
    }

    /// `OpenAIMessage` from `transform_tool_message` MUST have
    /// `tool_calls = None` (only assistant messages can issue
    /// tool_calls; tool messages are results).
    #[test]
    fn transform_tool_calls_is_none() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::Single(ContentPart::ToolResult(ToolResult::new(
                "id", "x",
            ))),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        assert!(result.tool_calls.is_none());
    }

    /// A bare `Text` part (the harness's shape) produces one content
    /// part carrying that text, and no `tool_use_id` is derived from it —
    /// the id can only come from `msg.tool_call_id` or a `ToolResult`
    /// wrapper.
    #[test]
    fn transform_text_part_yields_content_without_a_derived_tool_id() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::Single(ContentPart::Text(TextContent {
                text: "hello".to_string(),
                cache_control: None,
            })),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let result = p.transform_tool_message(&msg);
        let parts = result.content.expect("content");
        // No tool_use_id extracted from Text → fallback to msg.tool_call_id (None).
        assert_eq!(parts.len(), 1);
        assert_eq!(result.tool_call_id, None);
    }

    /// A `flac` audio part inside a tool result MUST be dropped, not
    /// forwarded as `input_audio` — on either route to the label.
    ///
    /// Same narrowing as `transform_part`: `input_audio.format` accepts
    /// only `wav` / `mp3`, while the shared `AudioContent::format_label`
    /// also reports `"flac"` (explicit format, or an `audio/flac` /
    /// `audio/x-flac` media type). The text half of the result is
    /// unaffected.
    #[test]
    fn transform_tool_result_flac_audio_is_dropped() {
        let p = provider();
        for (format, mime_type) in [
            (Some(AudioFormat::Flac), "audio/x-flac"),
            (None, "audio/flac"),
        ] {
            let mut tr = ToolResult::new("id", "ok");
            tr.content.push(ContentPart::Audio(AudioContent {
                data: "ZkxhYw==".to_string(),
                mime_type: mime_type.to_string(),
                format,
            }));
            let msg = Message {
                role: Role::Tool,
                content: Content::Single(ContentPart::ToolResult(tr)),
                tool_call_id: None,
                name: None,
                tool_result_cleared_at: None,
            };
            let result = p.transform_tool_message(&msg);
            let parts = result.content.as_ref().expect("content");
            assert!(
                !parts
                    .iter()
                    .any(|p| matches!(p, OpenAIContentPart::InputAudio { .. })),
                "flac/{mime_type} has no input_audio slot; got {parts:?}"
            );
            match &parts[0] {
                OpenAIContentPart::Text { text } => assert_eq!(text, "ok"),
                other => {
                    panic!("expected the text half first; got {other:?}")
                }
            }
            let json = serde_json::to_string(&result).expect("serialize");
            assert!(!json.contains("input_audio"), "got: {json}");
        }
    }

    /// `wav` / `mp3` audio in a tool result MUST reach the wire as
    /// `input_audio` carrying the provider's container label. One row
    /// per label route: explicit `format`, and the `mime_type` fallback
    /// a bare upload arrives with.
    #[test]
    fn transform_tool_result_wav_and_mp3_emit_input_audio() {
        let p = provider();
        for (format, mime_type, expected) in [
            (Some(AudioFormat::Wav), "audio/x-wav", "wav"),
            (None, "audio/mpeg", "mp3"),
        ] {
            let mut tr = ToolResult::new("id", "ok");
            tr.content.push(ContentPart::Audio(AudioContent {
                data: "QUJD".to_string(),
                mime_type: mime_type.to_string(),
                format,
            }));
            let msg = Message {
                role: Role::Tool,
                content: Content::Single(ContentPart::ToolResult(tr)),
                tool_call_id: None,
                name: None,
                tool_result_cleared_at: None,
            };
            let result = p.transform_tool_message(&msg);
            let parts = result.content.expect("content");
            let audio = parts
                .iter()
                .find_map(|p| match p {
                    OpenAIContentPart::InputAudio { input_audio } => {
                        Some(input_audio)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| {
                    panic!(
                        "expected input_audio for {mime_type}; got {parts:?}"
                    )
                });
            assert_eq!(audio.data, "QUJD");
            assert_eq!(audio.format, expected);
        }
    }

    /// The output MUST be serializable to JSON (the OpenAI
    /// wire format is JSON; if it can't be serialized, the
    /// provider is broken).
    #[test]
    fn transform_output_is_json_serializable() {
        let p = provider();
        let msg = Message {
            role: Role::Tool,
            content: Content::Single(ContentPart::ToolResult(ToolResult::new(
                "id", "ok",
            ))),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        };
        let result: OpenAIMessage = p.transform_tool_message(&msg);
        let json = serde_json::to_string(&result).expect("serialize");
        // Pinned wire-format markers.
        assert!(json.contains("\"role\":\"tool\""), "got: {json}");
        assert!(json.contains("\"content\""), "got: {json}");
        assert!(json.contains("\"tool_call_id\":\"id\""), "got: {json}");
    }
    /// The harness commits a tool result as
    /// `Message::tool(Content::parts(tr.content.clone()), call_id)` — the
    /// tool's RAW parts, not a `ToolResult` wrapper.
    ///
    /// Before this was handled, the wire body was
    /// `{"role":"tool","content":""}`: the model was told every tool had
    /// produced nothing and said so to the user, while the real output sat
    /// in the session log (which projects `ToolResult` by a different
    /// path). The log looking right is exactly why this could hide.
    #[test]
    fn tool_message_with_unwrapped_result_parts_carries_output() {
        let p = provider();
        // Exactly what commit.rs builds for a single-part shell result.
        let msg = Message::tool(
            Content::parts(vec![ContentPart::Text(TextContent {
                text: "stdout:\nE2E-TOOLBLOCK\n\n\n[exit code: 0]".to_string(),
                cache_control: None,
            })]),
            "call_1",
        );
        let out = p.transform_tool_message(&msg);
        let json = serde_json::to_value(&out).unwrap();
        assert!(
            json["content"].to_string().contains("E2E-TOOLBLOCK"),
            "tool output missing from the wire body: {json}"
        );
    }
    /// PROBE: history containing an assistant message with TWO parallel
    /// tool calls plus their two results must produce an OpenAI body where
    /// both tool_call_ids are answered.
    #[test]
    fn parallel_tool_calls_are_all_answered() {
        let p = provider();
        let assistant = Message::new(
            Role::Assistant,
            Content::parts(vec![
                ContentPart::ToolUse(ToolUse {
                    id: "call_a".to_string(),
                    name: "shell".to_string(),
                    input: json!({"command": "echo ONE"}),
                }),
                ContentPart::ToolUse(ToolUse {
                    id: "call_b".to_string(),
                    name: "shell".to_string(),
                    input: json!({"command": "echo TWO"}),
                }),
            ]),
        );
        let r1 = Message::tool(
            Content::parts(vec![ContentPart::Text(TextContent {
                text: "ONE".to_string(),
                cache_control: None,
            })]),
            "call_a",
        );
        let r2 = Message::tool(
            Content::parts(vec![ContentPart::Text(TextContent {
                text: "TWO".to_string(),
                cache_control: None,
            })]),
            "call_b",
        );
        let msgs: Vec<serde_json::Value> = [assistant, r1, r2]
            .iter()
            .filter_map(|m| p.transform_message(m))
            .map(|m| serde_json::to_value(&m).unwrap())
            .collect();
        let calls = msgs[0]["tool_calls"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let answered: Vec<String> = msgs
            .iter()
            .filter(|m| m["role"] == "tool")
            .filter_map(|m| m["tool_call_id"].as_str().map(str::to_string))
            .collect();
        assert_eq!(calls.len(), 2, "two calls on the assistant message");
        for c in &calls {
            let id = c["id"].as_str().unwrap();
            assert!(
                answered.iter().any(|a| a == id),
                "call {id} is unanswered; tool ids = {answered:?}"
            );
        }
    }
}
