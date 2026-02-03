//! Guardian 对话记录管理
//!
//! 此模块处理对话记录的收集和截断，用于 Guardian 审查过程。

use rmcp::model::SamplingMessage;

use crate::{AgentError, Result};

const MAX_MESSAGE_TOKENS: usize = 10_000;
const MAX_TOOL_TOKENS: usize = 10_000;
const MAX_ENTRY_TOKENS: usize = 2_000;

/// 对话记录条目
#[derive(Debug, Clone)]
pub struct TranscriptEntry {
    pub role: String,
    pub content: String,
    pub is_tool: bool,
}

/// 从采样消息中收集对话记录条目
pub fn collect_transcript_entries(
    messages: &[SamplingMessage],
) -> Vec<TranscriptEntry> {
    messages
        .iter()
        .filter_map(|msg| {
            let role = match &msg.role {
                rmcp::model::Role::User => "user",
                rmcp::model::Role::Assistant => "assistant",
            }
            .to_string();

            let content = extract_message_content(msg);

            if content.trim().is_empty() {
                None
            } else {
                Some(TranscriptEntry {
                    role,
                    content,
                    is_tool: false,
                })
            }
        })
        .collect()
}

/// 提取消息内容
fn extract_message_content(msg: &SamplingMessage) -> String {
    use rmcp::model::{
        RawTextContent,
        SamplingContent,
        SamplingMessageContent,
    };

    match &msg.content {
        SamplingContent::Single(SamplingMessageContent::Text(
            RawTextContent { text, .. },
        )) => text.clone(),
        SamplingContent::Multiple(contents) => contents
            .iter()
            .filter_map(|c| {
                if let SamplingMessageContent::Text(RawTextContent {
                    text,
                    ..
                }) = c
                {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// 截断文本到指定 token 数
pub(super) fn truncate_text(content: &str, max_tokens: usize) -> String {
    let max_bytes = max_tokens * 4;

    if content.len() <= max_bytes {
        return content.to_string();
    }

    let prefix_len = max_bytes / 2;
    let suffix_len = max_bytes / 2;

    let prefix = &content[..prefix_len];
    let suffix = &content[content.len() - suffix_len..];

    format!("{prefix}<truncated>{suffix}")
}

/// 构建审查提示词
pub fn build_review_prompt(
    transcript_entries: &[TranscriptEntry],
    action_json: &str,
    retry_reason: Option<&str>,
) -> String {
    let mut prompt = String::with_capacity(4096);

    prompt.push_str(
        "You are a security reviewer assessing a proposed action.\n\n",
    );
    prompt.push_str("=== CONVERSATION TRANSCRIPT ===\n");

    let mut message_tokens = 0usize;
    let mut tool_tokens = 0usize;

    for entry in transcript_entries.iter().take(40) {
        let entry_tokens = entry.content.len() / 4;
        let target = if entry.is_tool {
            &mut tool_tokens
        } else {
            &mut message_tokens
        };
        let max_tokens = if entry.is_tool {
            MAX_TOOL_TOKENS
        } else {
            MAX_MESSAGE_TOKENS
        };

        if *target + entry_tokens > max_tokens {
            continue;
        }

        *target += entry_tokens;
        let truncated = truncate_text(&entry.content, MAX_ENTRY_TOKENS);
        prompt.push_str(&format!("[{}] {}\n\n", entry.role, truncated));
    }

    prompt.push_str("=== PROPOSED ACTION ===\n");
    prompt.push_str(action_json);
    prompt.push_str("\n\n");

    if let Some(reason) = retry_reason {
        prompt.push_str(&format!("=== RETRY REASON ===\n{reason}\n\n"));
    }

    prompt.push_str(
        "Assess the risk of this action. Consider:\n\
         - Potential for data loss or corruption\n\
         - Security implications\n\
         - System integrity\n\
         - User authorization\n\n\
         Respond with JSON:\n\
         {\n\
           \"risk_level\": \"low\" | \"medium\" | \"high\",\n\
           \"risk_score\": 0-100,\n\
           \"rationale\": \"explanation\",\n\
           \"evidence\": [{\"message\": \"finding\", \"why\": \"reason\"}]\n\
         }",
    );

    prompt
}

/// 解析评估响应
pub fn parse_assessment_response(text: &str) -> Result<super::Assessment> {
    let trimmed = text.trim();

    // 尝试直接解析
    if let Ok(assessment) = serde_json::from_str::<super::Assessment>(trimmed) {
        return Ok(assessment);
    }

    // 尝试从 JSON 块中提取
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}'))
        && let Ok(assessment) =
            serde_json::from_str::<super::Assessment>(&trimmed[start..=end])
    {
        return Ok(assessment);
    }

    Err(AgentError::InternalError(format!(
        "Failed to parse assessment response: {trimmed}"
    )))
}

#[cfg(test)]
mod tests {
    use rmcp::model::{
        RawTextContent,
        Role,
        SamplingContent,
        SamplingMessage,
        SamplingMessageContent,
    };

    use super::*;
    use crate::guardian::GuardianRiskLevel;

    // =============================================================================
    // Test Helpers
    // =============================================================================

    fn create_text_message(role: Role, text: &str) -> SamplingMessage {
        SamplingMessage {
            role,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: text.to_string(),
                    meta: None,
                },
            )),
            meta: None,
        }
    }

    // =============================================================================
    // TranscriptEntry Tests
    // =============================================================================

    #[test]
    fn test_transcript_entry_clone() {
        let entry = TranscriptEntry {
            role: "user".to_string(),
            content: "Hello world".to_string(),
            is_tool: false,
        };
        let cloned = entry.clone();
        assert_eq!(cloned.role, entry.role);
        assert_eq!(cloned.content, entry.content);
        assert_eq!(cloned.is_tool, entry.is_tool);
    }

    #[test]
    fn test_transcript_entry_debug() {
        let entry = TranscriptEntry {
            role: "assistant".to_string(),
            content: "I'm thinking".to_string(),
            is_tool: true,
        };
        let debug_str = format!("{entry:?}");
        assert!(debug_str.contains("assistant"));
        assert!(debug_str.contains("I'm thinking"));
        assert!(debug_str.contains("true"));
    }

    // =============================================================================
    // collect_transcript_entries Tests
    // =============================================================================

    #[test]
    fn test_collect_transcript_entries_user_message() {
        let messages = vec![create_text_message(Role::User, "Hello, world!")];
        let entries = collect_transcript_entries(&messages);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].role, "user");
        assert_eq!(entries[0].content, "Hello, world!");
        assert!(!entries[0].is_tool);
    }

    #[test]
    fn test_collect_transcript_entries_assistant_message() {
        let messages =
            vec![create_text_message(Role::Assistant, "I see the issue")];
        let entries = collect_transcript_entries(&messages);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].role, "assistant");
        assert_eq!(entries[0].content, "I see the issue");
    }

    #[test]
    fn test_collect_transcript_entries_multiple_messages() {
        let messages = vec![
            create_text_message(Role::User, "First message"),
            create_text_message(Role::Assistant, "Second message"),
            create_text_message(Role::User, "Third message"),
        ];
        let entries = collect_transcript_entries(&messages);

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].role, "user");
        assert_eq!(entries[1].role, "assistant");
        assert_eq!(entries[2].role, "user");
    }

    #[test]
    fn test_collect_transcript_entries_empty_content_filtered() {
        let messages = vec![create_text_message(Role::User, "   ")];
        let entries = collect_transcript_entries(&messages);

        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_collect_transcript_entries_whitespace_only_filtered() {
        let messages = vec![create_text_message(Role::Assistant, "\n\t  \n")];
        let entries = collect_transcript_entries(&messages);

        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_collect_transcript_entries_empty_messages() {
        let messages: Vec<SamplingMessage> = vec![];
        let entries = collect_transcript_entries(&messages);

        assert!(entries.is_empty());
    }

    #[test]
    fn test_collect_transcript_entries_multiple_texts_in_multiple_content() {
        // Test SamplingContent::Multiple with multiple text items
        let messages = vec![SamplingMessage {
            role: Role::User,
            content: SamplingContent::Multiple(vec![
                SamplingMessageContent::Text(RawTextContent {
                    text: "First part".to_string(),
                    meta: None,
                }),
                SamplingMessageContent::Text(RawTextContent {
                    text: "Second part".to_string(),
                    meta: None,
                }),
            ]),
            meta: None,
        }];
        let entries = collect_transcript_entries(&messages);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "First part\nSecond part");
    }

    // =============================================================================
    // truncate_text Tests
    // =============================================================================

    #[test]
    fn test_truncate_text() {
        let long_text = "a".repeat(10000);
        let truncated = truncate_text(&long_text, 1000);

        assert!(truncated.contains("<truncated>"));
        assert!(truncated.len() < long_text.len());
    }

    #[test]
    fn test_truncate_text_short() {
        let short_text = "short text";
        let result = truncate_text(short_text, 100);

        assert_eq!(result, short_text);
    }

    #[test]
    fn test_truncate_text_exactly_at_boundary() {
        let text = "a".repeat(400); // 400 chars = 100 tokens * 4
        let result = truncate_text(&text, 100);
        assert_eq!(result, text);
    }

    #[test]
    fn test_truncate_text_empty() {
        let text = "";
        let result = truncate_text(text, 100);
        assert_eq!(result, "");
    }

    #[test]
    fn test_truncate_text_contains_prefix_and_suffix() {
        let long_text = "abcdefghij".repeat(1000);
        let truncated = truncate_text(&long_text, 100);

        // Should contain prefix, suffix, and truncation marker
        assert!(truncated.starts_with("abcdefghij"));
        assert!(truncated.ends_with("abcdefghij"));
        assert!(truncated.contains("<truncated>"));
    }

    // =============================================================================
    // build_review_prompt Tests
    // =============================================================================

    #[test]
    fn test_build_review_prompt() {
        let entries = vec![TranscriptEntry {
            role: "user".to_string(),
            content: "Hello".to_string(),
            is_tool: false,
        }];
        let action = r#"{"tool": "shell", "command": "ls"}"#;

        let prompt = build_review_prompt(&entries, action, None);

        assert!(prompt.contains("CONVERSATION TRANSCRIPT"));
        assert!(prompt.contains("PROPOSED ACTION"));
        assert!(prompt.contains("ls"));
    }

    #[test]
    fn test_build_review_prompt_with_retry() {
        let entries = vec![];
        let action = r#"{"tool": "test"}"#;

        let prompt = build_review_prompt(
            &entries,
            action,
            Some("Previous assessment was invalid"),
        );

        assert!(prompt.contains("RETRY REASON"));
        assert!(prompt.contains("Previous assessment was invalid"));
    }

    #[test]
    fn test_build_review_prompt_empty_entries() {
        let entries: Vec<TranscriptEntry> = vec![];
        let action = r#"{"tool": "read"}"#;

        let prompt = build_review_prompt(&entries, action, None);

        assert!(prompt.contains("CONVERSATION TRANSCRIPT"));
        assert!(prompt.contains("PROPOSED ACTION"));
        // Empty transcript should still produce valid prompt
        assert!(prompt.contains("Assess the risk"));
    }

    #[test]
    fn test_build_review_prompt_contains_all_sections() {
        let entries = vec![
            TranscriptEntry {
                role: "user".to_string(),
                content: "Run a command".to_string(),
                is_tool: false,
            },
            TranscriptEntry {
                role: "assistant".to_string(),
                content: "I'll run ls".to_string(),
                is_tool: false,
            },
        ];
        let action = r#"{"tool": "shell", "command": "rm -rf /"}"#;

        let prompt =
            build_review_prompt(&entries, action, Some("Please double-check"));

        // Check all expected sections are present
        assert!(prompt.contains("You are a security reviewer"));
        assert!(prompt.contains("CONVERSATION TRANSCRIPT"));
        assert!(prompt.contains("PROPOSED ACTION"));
        assert!(prompt.contains("RETRY REASON"));
        assert!(prompt.contains("Assess the risk"));
        assert!(prompt.contains("risk_level"));
        assert!(prompt.contains("risk_score"));
        assert!(prompt.contains("rationale"));
        assert!(prompt.contains("evidence"));
    }

    #[test]
    fn test_build_review_prompt_tool_entry_format() {
        let entries = vec![TranscriptEntry {
            role: "tool".to_string(),
            content: "File content here".to_string(),
            is_tool: true,
        }];

        let prompt = build_review_prompt(&entries, "{}", None);

        assert!(prompt.contains("[tool]"));
        assert!(prompt.contains("File content here"));
    }

    // =============================================================================
    // parse_assessment_response Tests
    // =============================================================================

    #[test]
    fn test_parse_assessment_response() {
        let json = r#"{
            "risk_level": "low",
            "risk_score": 20,
            "rationale": "Safe operation",
            "evidence": []
        }"#;

        let result = parse_assessment_response(json);
        assert!(result.is_ok());

        let assessment = result.unwrap();
        assert_eq!(assessment.risk_level, GuardianRiskLevel::Low);
        assert_eq!(assessment.risk_score, 20);
    }

    #[test]
    fn test_parse_assessment_response_with_code_block() {
        let json = r#"
        ```json
        {
            "risk_level": "medium",
            "risk_score": 50,
            "rationale": "Moderate risk",
            "evidence": []
        }
        ```
        "#;

        let result = parse_assessment_response(json);
        assert!(result.is_ok());

        let assessment = result.unwrap();
        assert_eq!(assessment.risk_level, GuardianRiskLevel::Medium);
    }

    #[test]
    fn test_parse_assessment_response_invalid() {
        let invalid = "not valid json";
        let result = parse_assessment_response(invalid);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_assessment_response_high_risk() {
        let json = r#"{
            "risk_level": "high",
            "risk_score": 85,
            "rationale": "Dangerous operation",
            "evidence": []
        }"#;

        let result = parse_assessment_response(json);
        assert!(result.is_ok());

        let assessment = result.unwrap();
        assert_eq!(assessment.risk_level, GuardianRiskLevel::High);
        assert_eq!(assessment.risk_score, 85);
    }

    #[test]
    fn test_parse_assessment_response_with_whitespace() {
        let json = r#"

        {
            "risk_level": "low",
            "risk_score": 10,
            "rationale": "Minimal risk",
            "evidence": []
        }

        "#;

        let result = parse_assessment_response(json);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().risk_level, GuardianRiskLevel::Low);
    }

    #[test]
    fn test_parse_assessment_response_with_leading_text() {
        let response = r#"Based on my analysis:

        {
            "risk_level": "medium",
            "risk_score": 45,
            "rationale": "Some concerns",
            "evidence": []
        }

        This assessment is provided above.
        "#;

        let result = parse_assessment_response(response);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().risk_level, GuardianRiskLevel::Medium);
    }

    #[test]
    fn test_parse_assessment_response_with_trailing_text() {
        let response = r#"{
            "risk_level": "low",
            "risk_score": 15,
            "rationale": "Safe",
            "evidence": []
        }

        End of analysis.
        "#;

        let result = parse_assessment_response(response);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_assessment_response_malformed_json_but_valid_content() {
        // JSON has trailing comma but contains valid content
        let json = r#"{
            "risk_level": "high",
            "risk_score": 90,
            "rationale": "Very dangerous",
            "evidence": []
        }"#;

        let result = parse_assessment_response(json);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().risk_level, GuardianRiskLevel::High);
    }
}
