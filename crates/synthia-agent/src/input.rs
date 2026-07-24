use synthia_provider::{ContentPart, ImageContent, Message, Role, TextContent};

/// Base64 encoding helper (no external dependency).
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).map(|&b| b as u32).unwrap_or(0);
        let b2 = chunk.get(2).map(|&b| b as u32).unwrap_or(0);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        result.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(ALPHABET[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(ALPHABET[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Input to the agent run loop. Can contain text, images, or a mix of content parts.
#[derive(Clone)]
pub struct AgentInput {
    pub content: Vec<ContentPart>,
}

impl AgentInput {
    /// Create an input from plain text.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ContentPart::Text(TextContent {
                text: text.into(),
                cache_control: None,
            })],
        }
    }

    /// Create an input from multiple content parts.
    pub fn multi(parts: Vec<ContentPart>) -> Self {
        Self { content: parts }
    }

    /// Create an input from a file (text files only).
    pub fn from_file(path: &str) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        Ok(Self {
            content: vec![ContentPart::Text(TextContent {
                text: content,
                cache_control: None,
            })],
        })
    }

    /// Create an input from a single image file. Supported formats: PNG, JPG, JPEG, GIF, WEBP.
    pub fn from_image(path: &str) -> Result<Self, std::io::Error> {
        let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
        let mime = match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Unsupported image format: {}", ext),
                ));
            }
        };
        let bytes = std::fs::read(path)?;
        let data = base64_encode(&bytes);
        Ok(Self {
            content: vec![ContentPart::Image(ImageContent {
                data,
                mime_type: mime.to_string(),
                detail: None,
            })],
        })
    }

    /// Create an input from text and base64-encoded image data.
    pub fn with_images(text: impl Into<String>, images: Vec<String>) -> Self {
        let mut parts: Vec<ContentPart> =
            vec![ContentPart::Text(TextContent {
                text: text.into(),
                cache_control: None,
            })];
        for img_data in images {
            parts.push(ContentPart::Image(ImageContent {
                data: img_data,
                mime_type: "image/png".to_string(),
                detail: None,
            }));
        }
        Self { content: parts }
    }

    /// Convert this input into a [`Message`] with [`Role::User`].
    pub fn to_message(&self) -> Message {
        let content = if self.content.len() == 1 {
            synthia_provider::Content::Single(self.content[0].clone())
        } else {
            synthia_provider::Content::Multi(self.content.clone())
        };
        Message {
            role: Role::User,
            content,
            tool_call_id: None,
            name: None,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_input() {
        let input = AgentInput::text("hello");
        assert_eq!(input.content.len(), 1);
        match &input.content[0] {
            ContentPart::Text(tc) => assert_eq!(tc.text, "hello"),
            _ => panic!("expected text content"),
        }
    }

    #[test]
    fn test_multi_input() {
        let parts = vec![
            ContentPart::Text(TextContent {
                text: "hello".to_string(),
                cache_control: None,
            }),
            ContentPart::Image(ImageContent {
                data: "img".to_string(),
                mime_type: "image/png".to_string(),
                detail: None,
            }),
        ];
        let input = AgentInput::multi(parts);
        assert_eq!(input.content.len(), 2);
    }

    #[test]
    fn test_with_images() {
        let input =
            AgentInput::with_images("describe", vec!["base64data".to_string()]);
        assert_eq!(input.content.len(), 2);
        assert!(matches!(input.content[0], ContentPart::Text(_)));
        assert!(matches!(input.content[1], ContentPart::Image(_)));
    }

    #[test]
    fn test_to_message_single() {
        let input = AgentInput::text("hello");
        let msg = input.to_message();
        assert_eq!(msg.role, Role::User);
        assert!(matches!(msg.content, synthia_provider::Content::Single(_)));
    }

    #[test]
    fn test_to_message_multi() {
        let input =
            AgentInput::with_images("describe", vec!["img".to_string()]);
        let msg = input.to_message();
        assert!(matches!(msg.content, synthia_provider::Content::Multi(_)));
    }

    #[test]
    fn test_from_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "hello world").unwrap();

        let input = AgentInput::from_file(file_path.to_str().unwrap()).unwrap();
        assert_eq!(input.content.len(), 1);
        match &input.content[0] {
            ContentPart::Text(tc) => assert_eq!(tc.text, "hello world"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn test_from_image_unsupported() {
        let result = AgentInput::from_image("file.xyz");
        assert!(result.is_err());
    }
}
