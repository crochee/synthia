use std::{io::Read, path::Path};

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum MediaType {
    Image,
    Audio,
    Video,
    Pdf,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct MediaAttachment {
    pub media_type: MediaType,
    pub mime_type: String,
    pub data: Vec<u8>,
    pub data_url: String,
    pub source_path: Option<String>,
    pub source_url: Option<String>,
}

impl MediaAttachment {
    pub fn from_path(path: &Path) -> Result<Self> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        let (media_type, mime_type) = Self::detect_type(&extension)
            .with_context(|| format!("Unsupported file type: {}", extension))?;

        let mut file = std::fs::File::open(path).with_context(|| {
            format!("Failed to open file: {}", path.display())
        })?;

        let mut data = Vec::new();
        file.read_to_end(&mut data).with_context(|| {
            format!("Failed to read file: {}", path.display())
        })?;

        let data_url = Self::create_data_url(&mime_type, &data);

        Ok(Self {
            media_type,
            mime_type,
            data,
            data_url,
            source_path: Some(path.to_string_lossy().to_string()),
            source_url: None,
        })
    }

    pub fn from_url(url: &str) -> Result<Self> {
        let response = reqwest::blocking::get(url)
            .with_context(|| format!("Failed to fetch URL: {}", url))?;

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        let data = response
            .bytes()
            .with_context(|| format!("Failed to read response from: {}", url))?
            .to_vec();

        let (media_type, mime_type) = Self::detect_from_mime(&content_type)
            .with_context(|| {
                format!("Unsupported media type: {}", content_type)
            })?;

        let data_url = Self::create_data_url(&mime_type, &data);

        Ok(Self {
            media_type,
            mime_type,
            data,
            data_url,
            source_path: None,
            source_url: Some(url.to_string()),
        })
    }

    fn detect_type(extension: &str) -> Result<(MediaType, String)> {
        match extension {
            "png" => Ok((MediaType::Image, "image/png".to_string())),
            "jpg" | "jpeg" => Ok((MediaType::Image, "image/jpeg".to_string())),
            "gif" => Ok((MediaType::Image, "image/gif".to_string())),
            "webp" => Ok((MediaType::Image, "image/webp".to_string())),
            "svg" => Ok((MediaType::Image, "image/svg+xml".to_string())),
            "bmp" => Ok((MediaType::Image, "image/bmp".to_string())),
            "ico" => Ok((MediaType::Image, "image/x-icon".to_string())),
            "mp3" => Ok((MediaType::Audio, "audio/mpeg".to_string())),
            "wav" => Ok((MediaType::Audio, "audio/wav".to_string())),
            "ogg" => Ok((MediaType::Audio, "audio/ogg".to_string())),
            "flac" => Ok((MediaType::Audio, "audio/flac".to_string())),
            "m4a" => Ok((MediaType::Audio, "audio/mp4".to_string())),
            "aac" => Ok((MediaType::Audio, "audio/aac".to_string())),
            "mp4" => Ok((MediaType::Video, "video/mp4".to_string())),
            "webm" => Ok((MediaType::Video, "video/webm".to_string())),
            "mov" => Ok((MediaType::Video, "video/quicktime".to_string())),
            "avi" => Ok((MediaType::Video, "video/x-msvideo".to_string())),
            "mkv" => Ok((MediaType::Video, "video/x-matroska".to_string())),
            "pdf" => Ok((MediaType::Pdf, "application/pdf".to_string())),
            _ => {
                Ok((MediaType::Unknown, "application/octet-stream".to_string()))
            }
        }
    }

    fn detect_from_mime(mime: &str) -> Result<(MediaType, String)> {
        if mime.starts_with("image/") {
            Ok((MediaType::Image, mime.to_string()))
        } else if mime.starts_with("audio/") {
            Ok((MediaType::Audio, mime.to_string()))
        } else if mime.starts_with("video/") {
            Ok((MediaType::Video, mime.to_string()))
        } else if mime == "application/pdf" {
            Ok((MediaType::Pdf, mime.to_string()))
        } else {
            Err(anyhow::anyhow!("Unsupported MIME type: {}", mime))
        }
    }

    fn create_data_url(mime_type: &str, data: &[u8]) -> String {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let b64_data = STANDARD.encode(data);
        format!("data:{};base64,{}", mime_type, b64_data)
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }
}

pub struct MediaProcessor;

impl MediaProcessor {
    pub fn process_input(
        input: &str,
    ) -> Result<(String, Vec<MediaAttachment>)> {
        let mut text = String::new();
        let mut attachments = Vec::new();

        let tokens = Self::tokenize(input);

        for token in tokens {
            match token {
                Token::Text(t) => {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(&t);
                }
                Token::ImagePath(path) => {
                    if let Ok(attachment) =
                        MediaAttachment::from_path(Path::new(&path))
                    {
                        attachments.push(attachment);
                    } else {
                        tracing::warn!("Failed to load image: {}", path);
                    }
                }
                Token::ImageUrl(url) => {
                    if let Ok(attachment) = MediaAttachment::from_url(&url) {
                        attachments.push(attachment);
                    } else {
                        tracing::warn!(
                            "Failed to load image from URL: {}",
                            url
                        );
                    }
                }
                Token::AudioPath(path) => {
                    if let Ok(attachment) =
                        MediaAttachment::from_path(Path::new(&path))
                    {
                        attachments.push(attachment);
                    } else {
                        tracing::warn!("Failed to load audio: {}", path);
                    }
                }
            }
        }

        Ok((text, attachments))
    }

    fn tokenize(input: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut current_pos = 0;
        let input_chars: Vec<char> = input.chars().collect();

        while current_pos < input_chars.len() {
            let ch = input_chars[current_pos];

            if ch == '@' {
                let (token, new_pos) =
                    Self::extract_media_ref(&input_chars, current_pos, '@');
                tokens.push(token);
                current_pos = new_pos;
            } else if ch == '!' {
                let (token, new_pos) =
                    Self::extract_media_ref(&input_chars, current_pos, '!');
                tokens.push(token);
                current_pos = new_pos;
            } else {
                let (txt, new_pos) =
                    Self::extract_text(&input_chars, current_pos);
                if !txt.is_empty() {
                    tokens.push(Token::Text(txt));
                }
                current_pos = new_pos;
            }
        }

        tokens
    }

    fn extract_media_ref(
        chars: &[char],
        start: usize,
        prefix: char,
    ) -> (Token, usize) {
        let mut end = start + 1;
        while end < chars.len() && !chars[end].is_whitespace() {
            end += 1;
        }

        let media_path: String = chars[start + 1..end].iter().collect();
        let media_path = media_path.trim().to_string();

        if prefix == '@' {
            if media_path.starts_with("http://")
                || media_path.starts_with("https://")
            {
                (Token::ImageUrl(media_path), end)
            } else {
                (Token::ImagePath(media_path), end)
            }
        } else {
            (Token::AudioPath(media_path), end)
        }
    }

    fn extract_text(chars: &[char], start: usize) -> (String, usize) {
        let mut end = start;
        while end < chars.len() {
            let ch = chars[end];
            if ch == '@' || ch == '!' {
                break;
            }
            end += 1;
        }

        let text: String = chars[start..end].iter().collect();
        (text.trim().to_string(), end)
    }
}

#[derive(Debug, Clone)]
enum Token {
    Text(String),
    ImagePath(String),
    ImageUrl(String),
    AudioPath(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_image_types() {
        let (media_type, mime) = MediaAttachment::detect_type("png").unwrap();
        assert_eq!(media_type, MediaType::Image);
        assert_eq!(mime, "image/png");

        let (media_type, mime) = MediaAttachment::detect_type("jpg").unwrap();
        assert_eq!(media_type, MediaType::Image);
        assert_eq!(mime, "image/jpeg");

        let (media_type, mime) = MediaAttachment::detect_type("gif").unwrap();
        assert_eq!(media_type, MediaType::Image);
        assert_eq!(mime, "image/gif");
    }

    #[test]
    fn test_detect_audio_types() {
        let (media_type, mime) = MediaAttachment::detect_type("mp3").unwrap();
        assert_eq!(media_type, MediaType::Audio);
        assert_eq!(mime, "audio/mpeg");

        let (media_type, mime) = MediaAttachment::detect_type("wav").unwrap();
        assert_eq!(media_type, MediaType::Audio);
        assert_eq!(mime, "audio/wav");
    }

    #[test]
    fn test_detect_video_types() {
        let (media_type, mime) = MediaAttachment::detect_type("mp4").unwrap();
        assert_eq!(media_type, MediaType::Video);
        assert_eq!(mime, "video/mp4");

        let (media_type, mime) = MediaAttachment::detect_type("webm").unwrap();
        assert_eq!(media_type, MediaType::Video);
        assert_eq!(mime, "video/webm");
    }

    #[test]
    fn test_tokenize_text_only() {
        let tokens = MediaProcessor::tokenize("Hello world");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(&tokens[0], Token::Text(s) if s == "Hello world"));
    }

    #[test]
    fn test_tokenize_with_image() {
        let tokens = MediaProcessor::tokenize("@image.png describe this");
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[0], Token::ImagePath(p) if p == "image.png"));
        assert!(matches!(&tokens[1], Token::Text(t) if t == "describe this"));
    }

    #[test]
    fn test_tokenize_with_url() {
        let tokens =
            MediaProcessor::tokenize("@https://example.com/image.jpg analyze");
        assert_eq!(tokens.len(), 2);
        assert!(
            matches!(&tokens[0], Token::ImageUrl(u) if u == "https://example.com/image.jpg")
        );
        assert!(matches!(&tokens[1], Token::Text(t) if t == "analyze"));
    }

    #[test]
    fn test_process_input() {
        let (text, attachments) =
            MediaProcessor::process_input("Hello").unwrap();
        assert_eq!(text, "Hello");
        assert!(attachments.is_empty());
    }
}
