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
    super::types::{OpenAIContentPart, OpenAIMessage},
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
                            url: ic.data.clone(),
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
                        })
                    }
                    crate::types::ContentPart::Audio(ac) => {
                        Some(OpenAIContentPart::InputAudio {
                            url: ac.data.clone(),
                            mime: ac.mime_type.clone(),
                            format: ac.format.as_ref().map(|f| match f {
                                crate::types::AudioFormat::Wav => {
                                    "wav".to_string()
                                }
                                crate::types::AudioFormat::Mp3 => {
                                    "mp3".to_string()
                                }
                                crate::types::AudioFormat::Flac => {
                                    "flac".to_string()
                                }
                            }),
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

        let content_str;
        let tool_use_id = match &msg.content {
            crate::types::Content::Single(part) => {
                if let crate::types::ContentPart::ToolResult(tr) = part {
                    content_str = extract_text_from_content(&tr.content);
                    Some(tr.tool_use_id.clone())
                } else {
                    content_str = String::new();
                    None
                }
            }
            crate::types::Content::Multi(parts) => {
                let mut ids = Vec::new();
                let mut contents = Vec::new();
                for p in parts {
                    if let crate::types::ContentPart::ToolResult(tr) = p {
                        ids.push(tr.tool_use_id.clone());
                        contents.push(extract_text_from_content(&tr.content));
                    }
                }
                content_str = contents.join("\n");
                ids.into_iter().next()
            }
        };

        let mut parts: Vec<OpenAIContentPart> = if !content_str.is_empty() {
            vec![OpenAIContentPart::Text { text: content_str }]
        } else {
            Vec::new()
        };

        let content_parts: Vec<&crate::types::ContentPart> = match &msg.content
        {
            crate::types::Content::Single(p) => vec![p],
            crate::types::Content::Multi(ps) => ps.iter().collect(),
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
