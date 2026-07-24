//! Parse the LLM's free-form response text into a typed
//! [`Assessment`](crate::review_types::Assessment).
//!
//! Tolerant of three real-world quirks:
//!
//! 1. **Markdown code fences**: the LLM may wrap the JSON in
//!    ```` ```json ... ``` ```` blocks.
//! 2. **Surrounding prose**: the LLM may prefix/suffix the JSON with
//!    analysis commentary (e.g. "Based on my analysis: …").
//! 3. **Whitespace**: the JSON may be surrounded by blank lines or
//!    indented for readability.
//!
//! Strategy: try direct parse first, then fall back to extracting the
//! outermost `{ … }` block and re-parsing.

/// 解析评估响应
pub fn parse_assessment_response(
    text: &str,
) -> anyhow::Result<super::super::review_types::Assessment> {
    let trimmed = text.trim();

    // 尝试直接解析
    if let Ok(assessment) =
        serde_json::from_str::<super::super::review_types::Assessment>(trimmed)
    {
        return Ok(assessment);
    }

    // 尝试从 JSON 块中提取
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}'))
        && let Ok(assessment) = serde_json::from_str::<
            super::super::review_types::Assessment,
        >(&trimmed[start..=end])
    {
        return Ok(assessment);
    }

    Err(anyhow::anyhow!(
        "Failed to parse assessment response: {trimmed}"
    ))
}
