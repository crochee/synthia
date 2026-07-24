//! The [`TranscriptEntry`] data struct — the unit of transcript content
//! carried between the collect, prompt, and parse stages.

/// 对话记录条目
#[derive(Debug, Clone)]
pub struct TranscriptEntry {
    pub role: String,
    pub content: String,
    pub is_tool: bool,
}
