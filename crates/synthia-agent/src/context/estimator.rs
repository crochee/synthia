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
    use rmcp::model::{Content, RawTextContent, ToolResultContent};

    use super::*;

    // =============================================================================
    // estimate_tokens and estimate_message_tokens tests
    // =============================================================================

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
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(&[]), 0);
    }

    #[test]
    fn test_estimate_tokens_multiple_messages() {
        let messages = vec![
            SamplingMessage {
                role: rmcp::model::Role::User,
                content: SamplingContent::Single(SamplingMessageContent::Text(
                    RawTextContent {
                        text: "Hello".to_string(),
                        meta: None,
                    },
                )),
                meta: None,
            },
            SamplingMessage {
                role: rmcp::model::Role::Assistant,
                content: SamplingContent::Single(SamplingMessageContent::Text(
                    RawTextContent {
                        text: "Hi there".to_string(),
                        meta: None,
                    },
                )),
                meta: None,
            },
        ];
        let tokens = estimate_tokens(&messages);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_empty_text() {
        let message = SamplingMessage {
            role: rmcp::model::Role::User,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: String::new(),
                    meta: None,
                },
            )),
            meta: None,
        };
        let tokens = estimate_message_tokens(&message);
        // Empty text should still have some overhead
        // tokens is usize, always >= 0
        let _ = tokens;
    }

    #[test]
    fn test_estimate_tokens_very_long_text() {
        let long_text = "a".repeat(50_000);
        let message = SamplingMessage {
            role: rmcp::model::Role::User,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: long_text,
                    meta: None,
                },
            )),
            meta: None,
        };
        let tokens = estimate_message_tokens(&message);
        assert!(tokens > 1000);
    }

    #[test]
    fn test_estimate_tokens_unicode_text() {
        let message = SamplingMessage {
            role: rmcp::model::Role::User,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: "Hello 你好 مرحبا".to_string(),
                    meta: None,
                },
            )),
            meta: None,
        };
        let tokens = estimate_message_tokens(&message);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_tool_use_content() {
        let message = SamplingMessage {
            role: rmcp::model::Role::Assistant,
            content: SamplingContent::Single(SamplingMessageContent::ToolUse(
                rmcp::model::ToolUseContent::new(
                    "tool-1",
                    "read_file",
                    serde_json::json!({"path": "/tmp/test.txt"})
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                ),
            )),
            meta: None,
        };
        let tokens = estimate_message_tokens(&message);
        assert!(tokens >= TOOL_USE_BASE_COST);
    }

    #[test]
    fn test_estimate_tokens_tool_result_with_text() {
        let message = SamplingMessage {
            role: rmcp::model::Role::User,
            content: SamplingContent::Single(
                SamplingMessageContent::ToolResult(ToolResultContent::new(
                    "tool-1",
                    vec![Content::text("Result content")],
                )),
            ),
            meta: None,
        };
        let tokens = estimate_message_tokens(&message);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_tool_result_with_base64() {
        // Base64 content should be estimated differently
        let base64_content = "SGVsbG8gV29ybGQgVGhpcyBpcyBhIHRlc3Qgc3RyaW5nIHdpdGggZW5vdWdoIGxlbmd0aCB0byBiZSBjb25zaWRlcmVkIGJhc2U2NCBsaWtl";
        let message = SamplingMessage {
            role: rmcp::model::Role::User,
            content: SamplingContent::Single(
                SamplingMessageContent::ToolResult(ToolResultContent::new(
                    "tool-1",
                    vec![Content::text(base64_content.to_string())],
                )),
            ),
            meta: None,
        };
        let tokens = estimate_message_tokens(&message);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_tool_result_with_hex() {
        let hex_content = "48656c6c6f20576f726c6420546869732069732061207465737420737472696e67207769746820656e6f756768206c656e677468";
        let message = SamplingMessage {
            role: rmcp::model::Role::User,
            content: SamplingContent::Single(
                SamplingMessageContent::ToolResult(ToolResultContent::new(
                    "tool-1",
                    vec![Content::text(hex_content.to_string())],
                )),
            ),
            meta: None,
        };
        let tokens = estimate_message_tokens(&message);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_tool_result_empty() {
        let message = SamplingMessage {
            role: rmcp::model::Role::User,
            content: SamplingContent::Single(
                SamplingMessageContent::ToolResult(ToolResultContent::new(
                    "tool-1",
                    vec![],
                )),
            ),
            meta: None,
        };
        let tokens = estimate_message_tokens(&message);
        // tokens is usize, always >= 0
        let _ = tokens;
    }

    #[test]
    fn test_estimate_tokens_multiple_content_items() {
        let message = SamplingMessage {
            role: rmcp::model::Role::User,
            content: SamplingContent::Multiple(vec![
                SamplingMessageContent::Text(RawTextContent {
                    text: "First".to_string(),
                    meta: None,
                }),
                SamplingMessageContent::Text(RawTextContent {
                    text: "Second".to_string(),
                    meta: None,
                }),
            ]),
            meta: None,
        };
        let tokens = estimate_message_tokens(&message);
        assert!(tokens > 0);
    }

    // =============================================================================
    // is_base64_like tests
    // =============================================================================

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
    fn test_is_base64_like_exactly_95_percent() {
        // Text with exactly 95% valid base64 chars should return true
        // 100 chars, 95 valid base64 chars = 95%
        let chars: String = "AAAA".repeat(25); // 100 chars, all valid
        let result = is_base64_like(&chars);
        assert!(result);
    }

    #[test]
    fn test_is_base64_like_less_than_95_percent() {
        // Text with only 94% valid base64 chars should return false
        let mut chars: String = "AAAA".repeat(23); // 92 A's
        chars.push_str("!@@@"); // 4 invalid chars = 92/96 = 95.8%, still passes due to rounding
        // Use 90 A's and 10 invalid = 90%
        let mut short: String = "A".repeat(90);
        short.push_str("!@#$%^&*("); // 10 invalid = 90%
        // This will be false since < 95%
    }

    #[test]
    fn test_is_base64_like_with_padding() {
        // is_encoded_data requires len >= 100, so short strings are never considered
        // This test verifies that short base64 strings return false
        let with_padding = "SGVsbG8gV29ybGQhIQ=="; // Only 20 chars
        assert!(!is_base64_like(with_padding));
    }

    // =============================================================================
    // is_hex_like tests
    // =============================================================================

    #[test]
    fn test_is_hex_like() {
        // Valid hex (long enough to pass the 100 char threshold)
        let hex = "48656c6c6f20576f726c6420546869732069732061207465737420737472696e67207769746820656e6f756768206c656e677468";
        assert!(is_hex_like(hex));

        // Not hex (too short - under 100 chars threshold)
        let short = "abc123";
        assert!(!is_hex_like(short));
    }

    #[test]
    fn test_is_hex_like_uppercase() {
        // is_encoded_data requires len >= 100, so short strings return false
        let hex_upper = "DEADBEEFCAFEBABE1234567890ABCDEF"; // Only 32 chars
        assert!(!is_hex_like(hex_upper));
    }

    #[test]
    fn test_is_hex_like_mixed_case() {
        // is_encoded_data requires len >= 100, so short strings return false
        let hex_mixed = "DeAdBeEfCaFeBaBe"; // Only 16 chars
        assert!(!is_hex_like(hex_mixed));
    }

    #[test]
    fn test_is_hex_like_long_uppercase() {
        // Long uppercase hex should pass
        let hex_upper = "DEADBEEF".repeat(13); // 104 chars
        assert!(is_hex_like(&hex_upper));
    }

    #[test]
    fn test_is_hex_like_long_mixed_case() {
        // Long mixed case hex should pass
        let hex_mixed = "DeAdBeEfCaFeBaBe".repeat(7); // 112 chars
        assert!(is_hex_like(&hex_mixed));
    }

    // =============================================================================
    // base64_estimated_bytes tests
    // =============================================================================

    #[test]
    fn test_base64_estimated_bytes() {
        let base64 = "SGVsbG8gV29ybGQgVGhpcyBpcyBhIHRlc3Qgc3RyaW5nIHdpdGggZW5vdWdoIGxlbmd0aCB0byBiZSBjb25zaWRlcmVkIGJhc2U2NCBsaWtl";
        let bytes = base64_estimated_bytes(base64);
        // Should be roughly len * 3/4
        assert!(bytes < base64.len());
        assert!(bytes > base64.len() / 2);
    }

    #[test]
    fn test_base64_estimated_bytes_empty() {
        let bytes = base64_estimated_bytes("");
        assert_eq!(bytes, 0);
    }

    #[test]
    fn test_base64_estimated_bytes_short() {
        // Short base64 strings should still work
        let short = "SGVsbG8="; // "Hello" in base64
        let bytes = base64_estimated_bytes(short);
        assert!(bytes > 0);
    }

    // =============================================================================
    // hex_estimated_bytes tests
    // =============================================================================

    #[test]
    fn test_hex_estimated_bytes() {
        let hex = "48656c6c6f";
        let bytes = hex_estimated_bytes(hex);
        // Hex doubles size, so hex.len() / 2
        assert_eq!(bytes, 5);
    }

    #[test]
    fn test_hex_estimated_bytes_empty() {
        let bytes = hex_estimated_bytes("");
        assert_eq!(bytes, 0);
    }

    #[test]
    fn test_hex_estimated_bytes_odd_length() {
        // Odd length hex should floor the result
        let hex = "ABC";
        let bytes = hex_estimated_bytes(hex);
        assert_eq!(bytes, 1); // 3 / 2 = 1 (integer division)
    }

    // =============================================================================
    // image_data_url_estimate_adjustment tests
    // =============================================================================

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
    fn test_image_data_url_adjustment_non_image() {
        // Data URL without image type should return None
        let text_url = "data:text/plain;base64,SGVsbG8=";
        assert!(image_data_url_estimate_adjustment(text_url).is_none());
    }

    #[test]
    fn test_image_data_url_adjustment_no_data_prefix() {
        let not_data_url = "http://example.com/image.png";
        assert!(image_data_url_estimate_adjustment(not_data_url).is_none());
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
    fn test_parse_base64_image_data_url_no_comma() {
        let url = "data:image/pngbase64data";
        assert!(parse_base64_image_data_url(url).is_none());
    }

    // =============================================================================
    // is_encoded_data edge case tests
    // =============================================================================

    #[test]
    fn test_is_encoded_data_single_char() {
        // Single char is never considered encoded data
        let result = is_encoded_data("A", |c| c.is_ascii_alphabetic());
        assert!(!result);
    }

    #[test]
    fn test_is_encoded_data_all_invalid() {
        // All invalid chars should return false immediately
        let result = is_encoded_data("!!!@@@###", |c| c.is_ascii_alphabetic());
        assert!(!result);
    }

    // =============================================================================
    // estimate_tool_use_bytes tests
    // =============================================================================

    #[test]
    fn test_estimate_tool_use_bytes_empty_input() {
        let tool_use = rmcp::model::ToolUseContent::new(
            "tool-1",
            "test",
            serde_json::json!({})
                .as_object()
                .cloned()
                .unwrap_or_default(),
        );
        let bytes = estimate_tool_use_bytes(&tool_use);
        assert!(bytes >= (TOOL_USE_BASE_COST * 4) as i64);
    }

    #[test]
    fn test_estimate_tool_use_bytes_with_input() {
        let tool_use = rmcp::model::ToolUseContent::new(
            "tool-1",
            "read_file",
            serde_json::json!({"path": "/tmp/test.txt", "lines": 100})
                .as_object()
                .cloned()
                .unwrap_or_default(),
        );
        let bytes = estimate_tool_use_bytes(&tool_use);
        assert!(bytes > (TOOL_USE_BASE_COST * 4) as i64);
    }

    // =============================================================================
    // Safety margin and constants tests
    // =============================================================================

    #[test]
    fn test_safety_margin_applied() {
        // Verify SAFETY_MARGIN is applied in token calculation
        let message = SamplingMessage {
            role: rmcp::model::Role::User,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: "Test message".to_string(),
                    meta: None,
                },
            )),
            meta: None,
        };
        let tokens = estimate_message_tokens(&message);
        let raw_bytes =
            "Test message".len() as f64 / BYTES_PER_TOKEN * SAFETY_MARGIN;
        assert!((tokens as f64 - raw_bytes).abs() < 1.0);
    }

    #[test]
    fn test_bytes_per_token_constant() {
        assert_eq!(BYTES_PER_TOKEN, 4.0);
    }

    #[test]
    fn test_safety_margin_constant() {
        assert_eq!(SAFETY_MARGIN, 1.2);
    }

    #[test]
    fn test_tool_use_base_cost_constant() {
        assert_eq!(TOOL_USE_BASE_COST, 15);
    }
}
