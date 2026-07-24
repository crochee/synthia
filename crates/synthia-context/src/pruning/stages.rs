//! Progressive-degradation primitives: stages, event types, and
//! the soft/hard trim + level-mapping helpers used by the
//! [`super::engine::prune`] orchestrator.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruningStage {
    None,
    SoftTrim,
    HardClear,
    Level1,
    Level2,
    Level3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    UserText,
    AssistantText,
    ToolUse,
    ToolResult,
    SystemPrompt,
    Other,
}

#[derive(Debug, Clone)]
pub struct PruningConfig {
    pub soft_trim_threshold: usize,
    pub hard_clear_threshold: usize,
    pub placeholder: String,
}

impl Default for PruningConfig {
    fn default() -> Self {
        Self {
            soft_trim_threshold: 2048,
            hard_clear_threshold: 8192,
            placeholder: "[content cleared to save context space]".to_string(),
        }
    }
}

pub fn soft_trim_content(content: &str, max_bytes: usize) -> String {
    if content.len() <= max_bytes {
        return content.to_string();
    }
    let keep = max_bytes.saturating_sub(64);
    let head_end = find_safe_utf8_boundary(content, keep / 2);
    let tail_start =
        find_safe_utf8_boundary(content, content.len() - (keep / 2));
    let head = &content[..head_end];
    let tail = &content[tail_start..];
    format!(
        "{}\n\n[... {} bytes truncated ...]\n\n{}",
        head,
        content.len() - head_end - (content.len() - tail_start),
        tail,
    )
}

pub fn hard_clear_content(_content: &str, placeholder: &str) -> String {
    placeholder.to_string()
}

pub fn get_compression_level(event_type: &EventType) -> u8 {
    match event_type {
        EventType::UserText | EventType::SystemPrompt => 0, // Preserve
        EventType::AssistantText => 1,                      // Mild compression
        EventType::ToolUse => 2,                            // Aggressive
        EventType::ToolResult => 3,                         // Most aggressive
        EventType::Other => 1,
    }
}

fn find_safe_utf8_boundary(s: &str, byte_index: usize) -> usize {
    if byte_index >= s.len() {
        return s.len();
    }
    let mut idx = byte_index;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_soft_trim_under_limit() {
        let content = "short content";
        let result = soft_trim_content(content, 100);
        assert_eq!(result, content);
    }

    #[test]
    fn test_soft_trim_over_limit() {
        let content = "a".repeat(1000);
        let result = soft_trim_content(&content, 100);
        assert!(result.len() < content.len());
        assert!(result.contains("truncated"));
    }

    #[test]
    fn test_soft_trim_respects_utf8_boundaries() {
        // Multi-byte chars must not be split.
        let content = "你好世界".repeat(200); // each char = 3 bytes
        let result = soft_trim_content(&content, 100);
        // Just check it doesn't panic and the result is shorter.
        assert!(result.len() < content.len());
    }

    #[test]
    fn test_hard_clear_content() {
        let result = hard_clear_content("anything", "[gone]");
        assert_eq!(result, "[gone]");
    }

    #[test]
    fn test_get_compression_level() {
        assert_eq!(get_compression_level(&EventType::UserText), 0);
        assert_eq!(get_compression_level(&EventType::SystemPrompt), 0);
        assert_eq!(get_compression_level(&EventType::AssistantText), 1);
        assert_eq!(get_compression_level(&EventType::ToolUse), 2);
        assert_eq!(get_compression_level(&EventType::ToolResult), 3);
        assert_eq!(get_compression_level(&EventType::Other), 1);
    }

    #[test]
    fn test_pruning_config_default() {
        let config = PruningConfig::default();
        assert!(config.soft_trim_threshold > 0);
        assert!(config.hard_clear_threshold > config.soft_trim_threshold);
        assert!(!config.placeholder.is_empty());
    }

    #[test]
    fn test_find_safe_utf8_boundary_at_char() {
        let s = "hello";
        assert_eq!(find_safe_utf8_boundary(s, 3), 3);
    }

    #[test]
    fn test_find_safe_utf8_boundary_at_mid_multibyte() {
        let s = "你a"; // '你' is 3 bytes
        // 1 is in the middle of '你' — should step back to 0
        assert_eq!(find_safe_utf8_boundary(s, 1), 0);
    }

    #[test]
    fn test_find_safe_utf8_boundary_past_end() {
        let s = "abc";
        assert_eq!(find_safe_utf8_boundary(s, 100), 3);
    }
}
