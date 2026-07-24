//! Token estimation module with enhanced accuracy

use synthia_provider::{Content, ContentPart, Message};

/// Bytes per token approximation (4 bytes ≈ 1 token)
pub(crate) const BYTES_PER_TOKEN: f64 = 4.0;

/// Safety margin multiplier (20%)
pub(crate) const SAFETY_MARGIN: f64 = 1.2;

/// Base token cost for tool use
pub(crate) const TOOL_USE_BASE_COST: usize = 15;

/// Approximate model-visible byte cost for one image input.
const RESIZED_IMAGE_BYTES_ESTIMATE: i64 = 7373;

/// Estimate total tokens for a list of messages
pub fn estimate_tokens(messages: &[Message]) -> usize {
    messages.iter().map(estimate_message_tokens).sum()
}

/// Estimate tokens for a single message.
///
/// Handles `ContentPart::ToolResult` and `ContentPart::ToolUse` content
/// correctly (the simple text-only estimator in `crate::traits` does
/// not). Use this when accuracy matters more than the 4-bytes-per-token
/// rough heuristic — e.g. when deciding whether a tool-result message
/// should count against `PRUNE_PROTECT_TOKENS`.
pub fn estimate_message_tokens(msg: &Message) -> usize {
    let base = estimate_message_model_visible_bytes(msg);
    ((base as f64 / BYTES_PER_TOKEN) * SAFETY_MARGIN) as usize
}

/// Estimate model-visible bytes for a message
fn estimate_message_model_visible_bytes(msg: &Message) -> i64 {
    match &msg.content {
        Content::Single(c) => estimate_content_model_visible_bytes(c),
        Content::Multi(cs) => {
            cs.iter().map(estimate_content_model_visible_bytes).sum()
        }
    }
}

/// Estimate model-visible bytes for content with type-specific handling
fn estimate_content_model_visible_bytes(content: &ContentPart) -> i64 {
    match content {
        ContentPart::Text(t) => {
            let text_bytes = t.text.len() as i64;
            text_bytes.saturating_mul(110).saturating_div(100)
        }
        ContentPart::ToolResult(r) => r
            .content
            .iter()
            .map(|c| {
                c.text()
                    .map(|text| {
                        if let Some((payload_bytes, replacement_bytes)) =
                            image_data_url_estimate_adjustment(text)
                        {
                            let raw = text.len() as i64;
                            raw.saturating_sub(payload_bytes)
                                .saturating_add(replacement_bytes)
                        } else if is_base64_like(text) {
                            let raw = text.len() as i64;
                            raw.saturating_sub(raw / 3)
                        } else if is_hex_like(text) {
                            hex_estimated_bytes(text) as i64
                        } else {
                            text.len() as i64
                        }
                    })
                    .unwrap_or(0)
            })
            .sum::<i64>()
            .saturating_mul(110)
            .saturating_div(100),
        ContentPart::ToolUse(tu) => {
            let input_len = serde_json::to_string(&tu.input)
                .unwrap_or_default()
                .len() as i64;
            ((TOOL_USE_BASE_COST * 4) as i64)
                .saturating_add(input_len)
                .saturating_mul(110)
                .saturating_div(100)
        }
        ContentPart::Image(_) => RESIZED_IMAGE_BYTES_ESTIMATE,
        ContentPart::Audio(_) => 100_i64 * 4,
        ContentPart::Reasoning(r) => r.text.len() as i64,
        ContentPart::Resource(_) => 100,
    }
}

/// Returns the base64 payload byte length for inline image data URLs that are
/// eligible for token-estimation discounting.
fn parse_base64_image_data_url(url: &str) -> Option<&str> {
    if !url
        .get(.."data:".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
    {
        return None;
    }
    let comma_index = url.find(',')?;
    let metadata = &url[..comma_index];
    let payload = &url[comma_index + 1..];
    if metadata.contains("image/") {
        Some(payload)
    } else {
        None
    }
}

/// Returns (payload_bytes, replacement_bytes) for image data URLs.
fn image_data_url_estimate_adjustment(url: &str) -> Option<(i64, i64)> {
    let payload = parse_base64_image_data_url(url)?;
    let payload_bytes = payload.len() as i64;
    Some((payload_bytes, RESIZED_IMAGE_BYTES_ESTIMATE))
}

/// Check if text looks like base64 encoded data
fn is_base64_like(text: &str) -> bool {
    is_encoded_data(
        text,
        |c| matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '+' | '/' | '='),
    )
}

/// Check if text looks like hex encoded data
fn is_hex_like(text: &str) -> bool {
    is_encoded_data(text, |c| c.is_ascii_hexdigit())
}

fn is_encoded_data(text: &str, valid_char: impl Fn(char) -> bool) -> bool {
    if text.len() < 100 {
        return false;
    }

    let mut valid_count = 0;
    for c in text.chars() {
        if valid_char(c) {
            valid_count += 1;
        } else {
            return false;
        }
    }

    valid_count as f64 / text.len() as f64 > 0.95
}

/// Estimate bytes for hex data (hex is 2x original)
fn hex_estimated_bytes(text: &str) -> usize {
    text.len() / 2
}

#[cfg(test)]
mod tests {
    use synthia_provider::{
        Content,
        ContentPart,
        Message,
        TextContent,
        ToolResult,
        ToolUse,
    };

    use super::*;

    fn tool_result(id: &str, content: &str) -> ToolResult {
        ToolResult {
            tool_use_id: id.to_string(),
            content: vec![ContentPart::Text(TextContent {
                text: content.to_string(),
                cache_control: None,
            })],
            structured_content: None,
            is_error: None,
        }
    }

    #[test]
    fn test_estimate_message_tokens_text() {
        let message = Message {
            role: synthia_provider::Role::User,
            content: Content::Single(ContentPart::Text(TextContent {
                text: "Hello world".to_string(),
                cache_control: None,
            })),
            tool_call_id: None,
            name: None,
            ..Default::default()
        };
        let tokens = estimate_message_tokens(&message);
        assert!(tokens > 0 && tokens < 10);
    }

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(&[]), 0);
    }

    #[test]
    fn test_estimate_tokens_multiple_messages() {
        let messages =
            vec![Message::user("Hello"), Message::assistant("Hi there")];
        let tokens = estimate_tokens(&messages);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_tool_use_content() {
        let message = Message {
            role: synthia_provider::Role::Assistant,
            content: Content::Single(ContentPart::ToolUse(ToolUse {
                id: "tool-1".to_string(),
                name: "read_file".to_string(),
                input: serde_json::json!({"path": "/tmp/test.txt"}),
            })),
            tool_call_id: None,
            name: None,
            ..Default::default()
        };
        let tokens = estimate_message_tokens(&message);
        assert!(tokens >= TOOL_USE_BASE_COST);
    }

    #[test]
    fn test_estimate_tokens_tool_result_with_text() {
        let message = Message {
            role: synthia_provider::Role::User,
            content: Content::Single(ContentPart::ToolResult(tool_result(
                "tool-1",
                "Result content",
            ))),
            tool_call_id: None,
            name: None,
            ..Default::default()
        };
        let tokens = estimate_message_tokens(&message);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_multiple_content_items() {
        let message = Message {
            role: synthia_provider::Role::User,
            content: Content::Multi(vec![
                ContentPart::Text(TextContent {
                    text: "First".to_string(),
                    cache_control: None,
                }),
                ContentPart::Text(TextContent {
                    text: "Second".to_string(),
                    cache_control: None,
                }),
            ]),
            tool_call_id: None,
            name: None,
            ..Default::default()
        };
        let tokens = estimate_message_tokens(&message);
        assert!(tokens > 0);
    }

    #[test]
    fn test_is_base64_like() {
        let base64 = "SGVsbG8gV29ybGQgVGhpcyBpcyBhIHRlc3Qgc3RyaW5nIHdpdGggZW5vdWdoIGxlbmd0aCB0byBiZSBjb25zaWRlcmVkIGJhc2U2NCBsaWtl";
        assert!(is_base64_like(base64));

        let short = "abc123";
        assert!(!is_base64_like(short));
    }

    #[test]
    fn test_is_hex_like() {
        let hex = "48656c6c6f20576f726c6420546869732069732061207465737420737472696e67207769746820656e6f756768206c656e677468";
        assert!(is_hex_like(hex));

        let short = "abc123";
        assert!(!is_hex_like(short));
    }

    #[test]
    fn test_image_data_url_adjustment() {
        let image_url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
        let adjustment = image_data_url_estimate_adjustment(image_url);
        assert!(adjustment.is_some());
        let (payload_bytes, replacement_bytes) = adjustment.unwrap();
        assert!(payload_bytes > 0);
        assert_eq!(replacement_bytes, RESIZED_IMAGE_BYTES_ESTIMATE);
    }

    #[test]
    fn test_hex_estimated_bytes() {
        let hex = "48656c6c6f";
        let bytes = hex_estimated_bytes(hex);
        assert_eq!(bytes, 5);
    }

    #[test]
    fn test_parse_base64_image_data_url() {
        let url = "data:image/jpeg;base64,/9j/4AAQSkZJRg==";
        let payload = parse_base64_image_data_url(url);
        assert!(payload.is_some());
        assert_eq!(payload.unwrap(), "/9j/4AAQSkZJRg==");
    }

    #[test]
    fn test_parse_base64_image_data_url_wrong_type() {
        let url = "data:text/plain;base64,SGVsbG8=";
        assert!(parse_base64_image_data_url(url).is_none());
    }

    #[test]
    fn test_safety_margin_applied() {
        let message = Message::user("Test message");
        let tokens = estimate_message_tokens(&message);
        let raw_bytes =
            "Test message".len() as f64 / BYTES_PER_TOKEN * SAFETY_MARGIN;
        assert!((tokens as f64 - raw_bytes).abs() < 1.0);
    }
}
