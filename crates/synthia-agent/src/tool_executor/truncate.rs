use serde::{Deserialize, Serialize};

/// 截断结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruncatedResult {
    /// 是否被截断
    pub truncated: bool,
    /// 截断后的内容
    pub content: String,
    /// 原始内容长度（字节）
    pub original_length: usize,
    /// 截断后内容长度（字节）
    pub truncated_length: usize,
}

impl TruncatedResult {
    /// 创建完整的（未截断）结果
    pub fn full(content: String) -> Self {
        let len = content.len();
        Self {
            truncated: false,
            content,
            original_length: len,
            truncated_length: len,
        }
    }

    /// 创建截断的结果
    pub fn truncated(content: String, original_length: usize) -> Self {
        let truncated_len = content.len();
        Self {
            truncated: true,
            content,
            original_length,
            truncated_length: truncated_len,
        }
    }
}

/// 截断工具执行结果
///
/// 当结果超过阈值时，保留头部和尾部，中间用省略号替代
/// 这样 LLM 可以看到开始和结尾的关键信息
pub fn truncate_result(
    content: &str,
    threshold_bytes: usize,
    head_bytes: usize,
    tail_bytes: usize,
) -> TruncatedResult {
    let original_len = content.len();

    if original_len <= threshold_bytes {
        return TruncatedResult::full(content.to_string());
    }

    // 计算截断后的内容
    let head_end = head_bytes.min(original_len);
    let tail_start = original_len.saturating_sub(tail_bytes);

    let mut result = String::with_capacity(head_bytes + tail_bytes + 50);

    // 头部内容
    if head_end > 0 {
        // 确保不截断 UTF-8 字符边界
        let safe_head_end = find_safe_boundary(content, head_end);
        result.push_str(&content[..safe_head_end]);
    }

    // 省略标记
    result.push_str(&format!(
        "\n\n--- [Output truncated: {} bytes omitted, keeping head+tail] ---\n\n",
        original_len - head_end - (original_len - tail_start)
    ));

    // 尾部内容
    if tail_start < original_len {
        let safe_tail_start = find_safe_boundary(content, tail_start);
        result.push_str(&content[safe_tail_start..]);
    }

    TruncatedResult::truncated(result, original_len)
}

/// 查找安全的 UTF-8 字符边界
fn find_safe_boundary(s: &str, byte_index: usize) -> usize {
    if byte_index >= s.len() {
        return s.len();
    }

    // 向后查找直到找到有效的 char 边界
    let mut idx = byte_index;
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }

    idx.min(s.len())
}

#[cfg(test)]
mod tests {
    #![allow(deprecated)]
    use super::*;

    #[test]
    fn test_no_truncation_needed() {
        let content = "short content";
        let result = truncate_result(content, 100, 50, 50);

        assert!(!result.truncated);
        assert_eq!(result.content, content);
    }

    #[test]
    fn test_truncate_large_content() {
        let content = "a".repeat(1000);
        let result = truncate_result(&content, 100, 40, 40);

        assert!(result.truncated);
        assert!(result.original_length == 1000);
        assert!(result.content.len() < 200);
        assert!(result.content.contains("truncated"));
    }

    #[test]
    fn test_utf8_boundary_safety() {
        // 中文字符，每个 3 字节
        let content = "你好世界".repeat(100);
        let result = truncate_result(&content, 100, 40, 40);

        // 应该能正常解析为 UTF-8
        assert!(String::from_utf8(result.content.as_bytes().to_vec()).is_ok());
    }
}
