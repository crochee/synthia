//! Format handling for different LLM providers

pub(crate) mod anthropic;
pub(crate) mod openai_compatible;

pub use openai_compatible::collect_stream;
use rmcp::model::{CreateMessageRequestParams, Tool};

/// Extract model name from CreateMessageRequestParams with a default fallback
pub(crate) fn get_model_name(params: &CreateMessageRequestParams) -> String {
    params
        .model_preferences
        .as_ref()
        .and_then(|p| p.hints.as_ref())
        .and_then(|hints| hints.first())
        .and_then(|hint| hint.name.clone())
        .unwrap_or_default()
}

/// Extract tools from CreateMessageRequestParams
pub(crate) fn extract_tools(params: &CreateMessageRequestParams) -> Vec<Tool> {
    params.tools.clone().unwrap_or_default()
}
