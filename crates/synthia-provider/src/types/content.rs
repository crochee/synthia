//! The [`Content`] / [`ContentPart`] enums, the data structs they
//! reference (`TextContent` / `ImageContent` / `AudioContent`), the
//! `ImageDetail` / `AudioFormat` small enums, all `From` /
//! `IntoIterator` impls, and the `is_tool_use` / `text` /
//! `cache_control` accessors.

use serde::{Deserialize, Serialize};
use synthia_cache_mark::CacheControlMark;

use super::tool::{ResourceLink, ToolResult, ToolUse};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Content {
    Single(ContentPart),
    Multi(Vec<ContentPart>),
}

impl Content {
    pub fn text(text: impl Into<String>) -> Self {
        Content::Single(ContentPart::Text(TextContent {
            text: text.into(),
            cache_control: None,
        }))
    }

    pub fn parts(mut parts: Vec<ContentPart>) -> Self {
        if parts.len() == 1 {
            Content::Single(parts.remove(0))
        } else {
            Content::Multi(parts)
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &ContentPart> {
        self.into_iter()
    }

    pub fn has_tool_use(&self) -> bool {
        self.into_iter().any(ContentPart::is_tool_use)
    }

    pub fn has_text(&self) -> bool {
        self.into_iter().any(|p| matches!(p, ContentPart::Text(..)))
    }

    pub fn extract_text(&self) -> Option<String> {
        let texts: Vec<String> = self
            .into_iter()
            .filter_map(|p| {
                if let ContentPart::Text(tc) = p {
                    Some(tc.text.clone())
                } else {
                    None
                }
            })
            .collect();
        if texts.is_empty() {
            None
        } else {
            Some(texts.join(""))
        }
    }

    pub fn extract_tool_uses(&self) -> Vec<ToolUse> {
        self.into_iter()
            .filter_map(|p| {
                if let ContentPart::ToolUse(tu) = p {
                    Some(tu.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}

impl From<String> for Content {
    fn from(s: String) -> Self {
        Content::text(s)
    }
}

impl From<&str> for Content {
    fn from(s: &str) -> Self {
        Content::text(s.to_string())
    }
}

impl IntoIterator for Content {
    type IntoIter = std::vec::IntoIter<Self::Item>;
    type Item = ContentPart;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Content::Single(part) => vec![part].into_iter(),
            Content::Multi(parts) => parts.into_iter(),
        }
    }
}

impl<'a> IntoIterator for &'a Content {
    type IntoIter = std::slice::Iter<'a, ContentPart>;
    type Item = &'a ContentPart;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Content::Single(part) => std::slice::from_ref(part).iter(),
            Content::Multi(parts) => parts.iter(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TextContent {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cache_control: Option<CacheControlMark>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReasoningContent {
    pub text: String,
    /// Anthropic `signature_delta` value attached to the most recent
    /// reasoning block. Required to preserve cross-turn reasoning
    /// continuity when the upstream Provider is Anthropic.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub signature: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ImageContent {
    pub data: String,
    pub mime_type: String,
    pub detail: Option<ImageDetail>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AudioContent {
    pub data: String,
    pub mime_type: String,
    pub format: Option<AudioFormat>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text(TextContent),
    Image(ImageContent),
    Audio(AudioContent),
    ToolUse(ToolUse),
    ToolResult(ToolResult),
    Reasoning(ReasoningContent),
    Resource(ResourceLink),
}

impl ContentPart {
    pub fn is_tool_use(&self) -> bool {
        matches!(self, ContentPart::ToolUse(..))
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            ContentPart::Text(tc) => Some(&tc.text),
            _ => None,
        }
    }

    /// Get the [`CacheControlMark`] on this part when it is a `Text`
    /// variant with a mark set. Returns `None` for non-`Text` variants
    /// or when no mark is present.
    pub fn cache_control(&self) -> Option<&CacheControlMark> {
        match self {
            ContentPart::Text(tc) => tc.cache_control.as_ref(),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImageDetail {
    Low,
    High,
    Auto,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioFormat {
    Wav,
    Mp3,
    Flac,
}
