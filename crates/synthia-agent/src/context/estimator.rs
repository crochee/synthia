//! Token estimation module with enhanced accuracy

use rmcp::model::{SamplingContent, SamplingMessage, SamplingMessageContent};

/// Bytes per token approximation (4 bytes ≈ 1 token)
pub(crate) const BYTES_PER_TOKEN: f64 = 4.0;

/// Safety margin multiplier (20%)
pub(crate) const SAFETY_MARGIN: f64 = 1.2;

/// Base token cost for tool use
pub(crate) const TOOL_USE_BASE_COST: usize = 15;

/// Approximate model-visible byte cost for one image input.
const RESIZED_IMAGE_BYTES_ESTIMATE: i64 = 7373;

/// Estimate total tokens for a list of messages
pub fn estimate_tokens(messages: &[SamplingMessage]) -> usize {
    messages.iter().map(estimate_message_tokens).sum()
}

/// Estimate tokens for a single message
pub(crate) fn estimate_message_tokens(msg: &SamplingMessage) -> usize {
    let base = estimate_message_model_visible_bytes(msg);
    ((base as f64 / BYTES_PER_TOKEN) * SAFETY_MARGIN) as usize
}

/// Estimate model-visible bytes for a message
fn estimate_message_model_visible_bytes(msg: &SamplingMessage) -> i64 {
    match &msg.content {
        SamplingContent::Single(c) => estimate_content_model_visible_bytes(c),
        SamplingContent::Multiple(cs) => {
            cs.iter().map(estimate_content_model_visible_bytes).sum()
        }
    }
}

/// Estimate model-visible bytes for content with type-specific handling
fn estimate_content_model_visible_bytes(
    content: &SamplingMessageContent,
) -> i64 {
    match content {
        SamplingMessageContent::Text(t) => {
            // For text, use raw length but apply encoding factor
            let text_bytes = t.text.len() as i64;
            // Account for JSON serialization overhead
            text_bytes.saturating_mul(110).saturating_div(100)
        }
        SamplingMessageContent::ToolResult(r) => {
            // Enhanced estimation for tool results with content type detection
            r.content
                .iter()
                .map(|c| {
                    c.as_text()
                        .map(|t| {
                            let text = &t.text;
                            // Check for base64-like data (especially images)
                            if let Some((payload_bytes, replacement_bytes)) =
                                image_data_url_estimate_adjustment(text)
                            {
                                // Replace raw base64 payload bytes with a per-image estimate
                                let raw = text.len() as i64;
                                raw.saturating_sub(payload_bytes)
                                    .saturating_add(replacement_bytes)
                            } else if is_base64_like(text) {
                                base64_estimated_bytes(text) as i64
                            } else if is_hex_like(text) {
                                hex_estimated_bytes(text) as i64
                            } else {
                                text.len() as i64
                            }
                        })
                        .unwrap_or(0)
                })
                .sum()
        }
        SamplingMessageContent::ToolUse(t) => estimate_tool_use_bytes(t),
        SamplingMessageContent::Image(_) => RESIZED_IMAGE_BYTES_ESTIMATE,
        SamplingMessageContent::Audio(_) => 100_i64 * 4,
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
    // Check if it's an image type
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

/// Estimate bytes for tool use
fn estimate_tool_use_bytes(tool_use: &rmcp::model::ToolUseContent) -> i64 {
    let input_len = serde_json::to_string(&tool_use.input)
        .unwrap_or_default()
        .len() as i64;
    ((TOOL_USE_BASE_COST * 4) as i64) // Base cost in bytes
        .saturating_add(input_len)
        .saturating_mul(110)
        .saturating_div(100) // Add 10% overhead for JSON serialization
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

/// Estimate bytes for base64 data (base64 is ~4/3 of original)
fn base64_estimated_bytes(text: &str) -> usize {
    // Base64 encoding increases size by ~33%, but tokenization is more efficient
    // Use a conservative estimate
    text.len() * 3 / 4
}

/// Estimate bytes for hex data (hex is 2x original)
fn hex_estimated_bytes(text: &str) -> usize {
    // Hex encoding doubles size
    text.len() / 2
}

#[cfg(test)]
mod tests {
    use rmcp::model::RawTextContent;

    use super::*;

    #[test]
    fn test_estimate_message_tokens_text() {
        let message = SamplingMessage {
            role: rmcp::model::Role::User,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: "Hello world".to_string(),
                    meta: None,
                },
            )),
            meta: None,
        };
        let tokens = estimate_message_tokens(&message);
        // 11 chars + overhead / 4 * 1.2 ≈ 4
        assert!(tokens > 0 && tokens < 10);
    }

    #[test]
    fn test_is_base64_like() {
        // Valid base64
        let base64 = "SGVsbG8gV29ybGQgVGhpcyBpcyBhIHRlc3Qgc3RyaW5nIHdpdGggZW5vdWdoIGxlbmd0aCB0byBiZSBjb25zaWRlcmVkIGJhc2U2NCBsaWtl";
        assert!(is_base64_like(base64));

        // Not base64 (too short)
        let short = "abc123";
        assert!(!is_base64_like(short));

        // Not base64 (contains invalid chars)
        let invalid = "Hello world! This is a test string with enough length to be considered but has invalid chars like @#$";
        assert!(!is_base64_like(invalid));
    }

    #[test]
    fn test_is_hex_like() {
        // Valid hex
        let hex = "48656c6c6f20576f726c6420546869732069732061207465737420737472696e67207769746820656e6f756768206c656e677468";
        assert!(is_hex_like(hex));

        // Not hex (too short)
        let short = "abc123";
        assert!(!is_hex_like(short));
    }

    #[test]
    fn test_base64_estimated_bytes() {
        let base64 = "SGVsbG8gV29ybGQgVGhpcyBpcyBhIHRlc3Qgc3RyaW5nIHdpdGggZW5vdWdoIGxlbmd0aCB0byBiZSBjb25zaWRlcmVkIGJhc2U2NCBsaWtl";
        let bytes = base64_estimated_bytes(base64);
        // Should be roughly len * 3/4
        assert!(bytes < base64.len());
        assert!(bytes > base64.len() / 2);
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
}
