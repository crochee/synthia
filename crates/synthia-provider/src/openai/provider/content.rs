//! The 2 content methods on
//! [`super::core::OpenAICompatibleProvider`]:
//!
//! - [`OpenAICompatibleProvider::transform_content`] —
//!   dispatches on `Content::Single` vs `Content::Multi` to
//!   map each `ContentPart` via `transform_part`.
//! - [`OpenAICompatibleProvider::transform_part`] — maps a
//!   single [`crate::types::ContentPart`] to an
//!   [`super::types::OpenAIContentPart`]. Text/Image/Audio
//!   map 1:1; `ToolUse` keeps `id`/`name`/`input`;
//!   `ToolResult` becomes a flat text representation via
//!   `format!("{:?}", tr.content)` (the OpenAI API does not
//!   preserve the structured result); `Reasoning` becomes a
//!   text-style part; `Resource` is rendered as
//!   `"[Resource: uri - name]"`.

use super::{super::types::OpenAIContentPart, core::OpenAICompatibleProvider};

impl OpenAICompatibleProvider {
    pub(in crate::openai) fn transform_content(
        &self,
        content: &crate::types::Content,
    ) -> Vec<OpenAIContentPart> {
        match content {
            crate::types::Content::Single(part) => {
                vec![self.transform_part(part)]
            }
            crate::types::Content::Multi(parts) => {
                parts.iter().map(|p| self.transform_part(p)).collect()
            }
        }
    }

    pub(in crate::openai) fn transform_part(
        &self,
        part: &crate::types::ContentPart,
    ) -> OpenAIContentPart {
        match part {
            crate::types::ContentPart::Text(tc) => OpenAIContentPart::Text {
                text: tc.text.clone(),
            },
            crate::types::ContentPart::Image(ic) => {
                OpenAIContentPart::ImageUrl {
                    url: ic.data.clone(),
                    detail: ic.detail.as_ref().map(|d| match d {
                        crate::types::ImageDetail::Low => "low".to_string(),
                        crate::types::ImageDetail::High => "high".to_string(),
                        crate::types::ImageDetail::Auto => "auto".to_string(),
                    }),
                }
            }
            crate::types::ContentPart::Audio(ac) => {
                OpenAIContentPart::InputAudio {
                    url: ac.data.clone(),
                    mime: ac.mime_type.clone(),
                    format: ac.format.as_ref().map(|f| match f {
                        crate::types::AudioFormat::Wav => "wav".to_string(),
                        crate::types::AudioFormat::Mp3 => "mp3".to_string(),
                        crate::types::AudioFormat::Flac => "flac".to_string(),
                    }),
                }
            }
            crate::types::ContentPart::ToolUse(tu) => {
                OpenAIContentPart::ToolUse {
                    id: tu.id.clone(),
                    name: tu.name.clone(),
                    input: tu.input.clone(),
                }
            }
            crate::types::ContentPart::ToolResult(tr) => {
                OpenAIContentPart::ToolResult {
                    id: tr.tool_use_id.clone(),
                    content: format!("{:?}", tr.content),
                    is_error: tr.is_error.unwrap_or(false),
                }
            }
            crate::types::ContentPart::Reasoning(rc) => {
                OpenAIContentPart::Reasoning {
                    text: rc.text.clone(),
                }
            }
            crate::types::ContentPart::Resource(link) => {
                OpenAIContentPart::Text {
                    text: format!("[Resource: {} - {}]", link.uri, link.name),
                }
            }
        }
    }
}
