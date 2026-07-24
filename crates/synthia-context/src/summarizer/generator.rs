//! Summary generation: LLM-based summary with retry mechanism.

use std::sync::Arc;

use synthia_model_router::ModelRouter;
use synthia_provider::{CachePolicy, CompletionRequest, Message, Role};

/// Maximum number of retries for summary generation
const DEFAULT_MAX_RETRIES: u32 = 2;

/// Extract text content from a message
fn extract_text_from_message(msg: &Message) -> String {
    msg.content.extract_text().unwrap_or_default()
}

/// Generate summary with optional quality checking and retry mechanism
pub async fn generate_summary(
    model_router: &Arc<dyn ModelRouter>,
    messages: &[Message],
    quality_check_enabled: bool,
    max_tokens: usize,
) -> anyhow::Result<String> {
    generate_summary_with_retries(
        model_router,
        messages,
        quality_check_enabled,
        max_tokens,
        DEFAULT_MAX_RETRIES,
    )
    .await
}

/// Generate summary with configurable retry mechanism
async fn generate_summary_with_retries(
    model_router: &Arc<dyn ModelRouter>,
    messages: &[Message],
    quality_check_enabled: bool,
    max_tokens: usize,
    max_retries: u32,
) -> anyhow::Result<String> {
    let mut last_quality = None;

    for attempt in 0..=max_retries {
        let summary =
            try_generate_summary(model_router, messages, max_tokens).await?;

        // Quality check
        if quality_check_enabled {
            let quality = super::quality::check_summary_quality(&summary, messages);
            last_quality = Some(quality);

            if quality.overall_score >= 0.8 {
                tracing::debug!(
                    "Summary quality check passed (score: {:.2}) on attempt {}",
                    quality.overall_score,
                    attempt + 1
                );
                return Ok(summary);
            }

            // Log detailed warning about quality issues
            if attempt < max_retries {
                tracing::warn!(
                    "Summary quality check failed on attempt {} (score: {:.2}). \
                     Missing: [sections: {}, file_paths: {}, user_requests: {}, key_decisions: {}]. Retrying...",
                    attempt + 1,
                    quality.overall_score,
                    !quality.has_required_sections,
                    !quality.has_file_paths,
                    !quality.has_user_requests,
                    !quality.has_key_decisions
                );
            } else {
                tracing::warn!(
                    "Summary quality check failed after {} attempts (score: {:.2}). \
                     Missing: [sections: {}, file_paths: {}, user_requests: {}, key_decisions: {}]. Using best effort.",
                    max_retries + 1,
                    quality.overall_score,
                    !quality.has_required_sections,
                    !quality.has_file_paths,
                    !quality.has_user_requests,
                    !quality.has_key_decisions
                );
            }
        } else {
            return Ok(summary);
        }
    }

    // If we exhausted retries, try one more time and return whatever we get
    let final_summary =
        try_generate_summary(model_router, messages, max_tokens).await?;

    if let Some(quality) = last_quality {
        tracing::info!(
            "Returning summary with quality score: {:.2}",
            quality.overall_score
        );
    }

    Ok(final_summary)
}

/// Try to generate a summary
async fn try_generate_summary(
    model_router: &Arc<dyn ModelRouter>,
    messages: &[Message],
    max_tokens: usize,
) -> anyhow::Result<String> {
    let messages_text = messages
        .iter()
        .map(|m| {
            let role = match m.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::System => "System",
                Role::Tool => "Tool",
            };
            format!("{role}: {}", extract_text_from_message(m))
        })
        .collect::<Vec<_>>()
        .join("\n");

    let result = model_router.route(messages).await?;
    let provider = result.provider;
    let model_cfg = result.config;

    // Render compaction prompt inline
    let _system_prompt = format!(
        "You are summarizing a conversation. Below is the conversation history.\n\n{messages_text}"
    );
    let user_message = Message::user("Summarize the conversation above.");

    let params = CompletionRequest {
        model: model_cfg.model_info().name.clone(),
        messages: Arc::new(vec![user_message]),
        tools: Arc::new(vec![]),
        tool_choice: synthia_provider::ToolChoice::None,
        temperature: Some(0.3),
        max_tokens: Some(max_tokens),
        stop_sequences: vec![],
        extra_body: None,
        cache_policy: Some(CachePolicy::default()),
    };

    let response = provider.complete(params).await?;
    let text = response.content.extract_text().unwrap_or_default();
    Ok(text)
}

/// Create a summary message with prefix
pub fn create_summary_message(summary: &str) -> Message {
    const SUMMARY_PREFIX: &str = "## Summary of Previous Conversation";
    Message::assistant(format!("{SUMMARY_PREFIX}\n\n{summary}"))
}