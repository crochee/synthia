//! OpenAI delta response types for SSE streaming deserialization.

#[derive(Debug, serde::Deserialize)]
pub struct OpenAIDeltaResponse {
    pub choices: Vec<OpenAIDeltaChoice>,
}

#[derive(Debug, serde::Deserialize)]
pub struct OpenAIDeltaChoice {
    pub delta: Option<OpenAIDelta>,
    #[serde(default)]
    pub finish_reason: Option<String>,
    /// Usage attached to a final empty choice (OpenAI `stream_options.include_usage`).
    #[serde(default)]
    pub usage: Option<OpenAIDeltaUsage>,
}

#[derive(Debug, serde::Deserialize)]
pub struct OpenAIDeltaUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

#[derive(Debug, serde::Deserialize)]
pub struct OpenAIDelta {
    pub content: Option<String>,
    #[serde(default, rename = "reasoning_content")]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<OpenAIDeltaToolUse>>,
}

#[derive(Debug, serde::Deserialize)]
pub struct OpenAIDeltaToolUse {
    pub id: Option<String>,
    pub function: OpenAIDeltaToolUseFunction,
    #[serde(default)]
    pub index: Option<u32>,
}

#[derive(Debug, serde::Deserialize)]
pub struct OpenAIDeltaToolUseFunction {
    pub name: Option<String>,
    pub arguments: Option<String>,
}
