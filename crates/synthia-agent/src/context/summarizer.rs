//! Summary generation with quality checking

use std::sync::{Arc, LazyLock};

use rmcp::model::{CreateMessageRequestParams, Role, SamplingMessage};
use synthia_provider::collect_stream;
use tokio_util::sync::CancellationToken;

use crate::{
    context::types::SummaryQuality,
    model_router::ModelRouter,
    prompt::render_compaction_prompt,
    utils::{extract_text_content, extract_text_from_result},
};

static FILE_PATH_PATTERNS: LazyLock<Vec<(&'static str, &'static str)>> =
    LazyLock::new(|| {
        vec![
            ("src/", ".rs"),
            ("tests/", ".rs"),
            ("examples/", ".rs"),
            ("benches/", ".rs"),
            ("crates/", "/"),
            ("/home/", "/"),
            ("/usr/", "/"),
            ("/var/", "/"),
            ("/tmp/", "/"),
            (".", ".rs"),
            (".", ".toml"),
            (".", ".json"),
            (".", ".yaml"),
            (".", ".yml"),
            (".", ".md"),
        ]
    });

static REQUEST_KEYWORDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "user request",
        "user asked",
        "user wants",
        "user need",
        "requested",
        "require",
        "implement",
        "add ",
        "modify",
        "update",
        "delete",
        "remove",
        "create",
        "fix",
        "change",
        "please",
        "want to",
        "need to",
        "would like",
    ]
});

static DECISION_KEYWORDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "decided",
        "decision",
        "conclusion",
        "agreed",
        "chosen",
        "selected",
        "opted",
        "determined",
        "resolved",
        "will",
        "plan to",
        "going to",
        "approach",
        "strategy",
        "solution",
        "recommend",
        "suggest",
    ]
});

/// Check if summary contains file paths
fn has_file_paths(summary: &str) -> bool {
    FILE_PATH_PATTERNS.iter().any(|(prefix, suffix)| {
        summary.contains(prefix) && summary.contains(suffix)
    })
}

/// Check if summary contains user requests
fn has_user_requests(summary: &str) -> bool {
    let summary_lower = summary.to_lowercase();
    REQUEST_KEYWORDS
        .iter()
        .any(|keyword| summary_lower.contains(*keyword))
}

/// Check if summary contains key decisions
fn has_key_decisions(summary: &str) -> bool {
    let summary_lower = summary.to_lowercase();
    DECISION_KEYWORDS
        .iter()
        .any(|keyword| summary_lower.contains(*keyword))
}

/// Maximum number of retries for summary generation
const DEFAULT_MAX_RETRIES: u32 = 2;

/// Generate summary with optional quality checking and retry mechanism
pub(crate) async fn generate_summary(
    model_router: &Arc<dyn ModelRouter>,
    messages: &[SamplingMessage],
    quality_check_enabled: bool,
    max_tokens: usize,
) -> crate::Result<String> {
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
    messages: &[SamplingMessage],
    quality_check_enabled: bool,
    max_tokens: usize,
    max_retries: u32,
) -> crate::Result<String> {
    let mut last_quality = None;

    for attempt in 0..=max_retries {
        let summary =
            try_generate_summary(model_router, messages, max_tokens).await?;

        // Quality check
        if quality_check_enabled {
            let quality = check_summary_quality(&summary, messages);
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
    messages: &[SamplingMessage],
    max_tokens: usize,
) -> crate::Result<String> {
    let messages_text = messages
        .iter()
        .map(|m| {
            let role = if m.role == Role::User {
                "User"
            } else {
                "Assistant"
            };
            format!("{role}: {}", extract_text_content(m))
        })
        .collect::<Vec<_>>()
        .join("\n");

    let result = model_router.route(messages).await?;
    let provider = result.provider;
    let model_cfg = result.config;

    let system_prompt = render_compaction_prompt(&messages_text)?;
    let user_message =
        SamplingMessage::user_text("Summarize the conversation above.");

    let params = CreateMessageRequestParams {
        messages: vec![user_message],
        system_prompt: Some(system_prompt),
        max_tokens: max_tokens as u32,
        temperature: Some(0.3),
        model_preferences: Some(rmcp::model::ModelPreferences {
            hints: Some(vec![rmcp::model::ModelHint {
                name: Some(model_cfg.model_info().name.clone()),
            }]),
            cost_priority: None,
            speed_priority: Some(1.0),
            intelligence_priority: None,
        }),
        tools: None,
        stop_sequences: None,
        metadata: None,
        include_context: None,
        meta: None,
        task: None,
        tool_choice: None,
    };

    let stream = provider.stream(params, CancellationToken::new()).await?;
    let result = collect_stream(stream).await?;
    Ok(extract_text_from_result(&result))
}

/// Check summary quality
pub fn check_summary_quality(
    summary: &str,
    original: &[SamplingMessage],
) -> SummaryQuality {
    let required_sections = ["## Summary", "## User Intent", "## Current Work"];

    let has_required_sections = required_sections
        .iter()
        .all(|section| summary.contains(section));

    // Check identifier integrity (basic check for common patterns)
    let identifier_integrity = check_identifier_integrity(summary);

    // Check user request reflection
    let user_request_reflected =
        summary.contains("User Intent") || summary.contains("user");

    // Check for file paths in summary
    let summary_has_file_paths = has_file_paths(summary);

    // Check for user requests preservation
    let summary_has_user_requests = has_user_requests(summary);

    // Check for key decisions
    let summary_has_key_decisions = has_key_decisions(summary);

    // If original messages contain file paths but summary doesn't, that's a quality issue
    let original_has_files = original.iter().any(|msg| {
        let text = extract_text_content(msg);
        has_file_paths(&text)
    });

    // If original has file paths but summary doesn't, mark as missing file paths
    let effective_has_file_paths = if original_has_files {
        summary_has_file_paths
    } else {
        // If original doesn't have file paths, consider this check passed
        true
    };

    SummaryQuality::new(
        has_required_sections,
        identifier_integrity,
        user_request_reflected,
        effective_has_file_paths,
        summary_has_user_requests,
        summary_has_key_decisions,
    )
}

/// Basic identifier integrity check
fn check_identifier_integrity(summary: &str) -> bool {
    // Check for common identifier patterns
    let has_file_refs = summary.contains(".") && summary.contains("/");
    let has_code_blocks = summary.contains("```");
    let has_structure = summary.contains("##");

    // At least one structural element should be present
    has_structure || has_file_refs || has_code_blocks
}

/// Create a summary message with prefix
pub fn create_summary_message(summary: &str) -> SamplingMessage {
    const SUMMARY_PREFIX: &str = "## Summary of Previous Conversation";
    SamplingMessage::assistant_text(format!("{SUMMARY_PREFIX}\n\n{summary}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_file_paths() {
        // Should detect Rust source files
        assert!(has_file_paths("src/main.rs"));
        assert!(has_file_paths("crates/synthia-agent/src/context/mod.rs"));
        assert!(has_file_paths("tests/integration_test.rs"));
        assert!(has_file_paths("examples/demo.rs"));
        assert!(has_file_paths("benches/benchmark.rs"));

        // Should detect config files
        assert!(has_file_paths("config.toml"));
        assert!(has_file_paths("package.json"));
        assert!(has_file_paths("settings.yaml"));
        assert!(has_file_paths("data.yml"));
        assert!(has_file_paths("README.md"));

        // Should detect home directory paths
        assert!(has_file_paths("/home/user/project/src/lib.rs"));
        assert!(has_file_paths("/usr/local/bin/executable"));

        // Should not match when only prefix or suffix exists
        assert!(!has_file_paths("src/"));
        assert!(!has_file_paths("just some text"));
        assert!(!has_file_paths("no file path here"));
    }

    #[test]
    fn test_has_user_requests() {
        // Should detect user request keywords
        assert!(has_user_requests("user request"));
        assert!(has_user_requests("User Request"));
        assert!(has_user_requests("user asked for help"));
        assert!(has_user_requests("user wants to implement"));
        assert!(has_user_requests("user needs a new feature"));
        assert!(has_user_requests("requested by user"));
        assert!(has_user_requests("this is required"));
        assert!(has_user_requests("implement a new system"));
        assert!(has_user_requests("add a test"));
        assert!(has_user_requests("modify the config"));
        assert!(has_user_requests("update the documentation"));
        assert!(has_user_requests("delete the file"));
        assert!(has_user_requests("remove the old code"));
        assert!(has_user_requests("create a new module"));
        assert!(has_user_requests("fix the bug"));
        assert!(has_user_requests("change the behavior"));
        assert!(has_user_requests("please help"));
        assert!(has_user_requests("I want to start"));
        assert!(has_user_requests("I need to finish"));
        assert!(has_user_requests("I would like to continue"));

        // Should not match plain text without keywords
        assert!(!has_user_requests("The sky is blue."));
        assert!(!has_user_requests("This is a normal conversation."));
        assert!(!has_user_requests("The file contains data."));
    }

    #[test]
    fn test_has_key_decisions() {
        // Should detect decision keywords
        assert!(has_key_decisions("we decided to go with A"));
        assert!(has_key_decisions("The decision was made"));
        assert!(has_key_decisions("in conclusion"));
        assert!(has_key_decisions("we agreed on the approach"));
        assert!(has_key_decisions("we chosen option B"));
        assert!(has_key_decisions("selected the fast path"));
        assert!(has_key_decisions("opted for simplicity"));
        assert!(has_key_decisions("determined to be correct"));
        assert!(has_key_decisions("resolved to proceed"));
        assert!(has_key_decisions("we will do it"));
        assert!(has_key_decisions("plan to implement"));
        assert!(has_key_decisions("going to refactor"));
        assert!(has_key_decisions("the approach is solid"));
        assert!(has_key_decisions("our strategy is clear"));
        assert!(has_key_decisions("the solution works well"));
        assert!(has_key_decisions("recommend using cache"));
        assert!(has_key_decisions("suggest trying again"));

        // Should not match plain text without keywords
        assert!(!has_key_decisions("The weather is nice."));
        assert!(!has_key_decisions("This file has content."));
        assert!(!has_key_decisions("Maybe we should consider."));
    }

    #[test]
    fn test_check_summary_quality() {
        let good_summary = r#"
## Summary
This is a good summary with file path src/main.rs mentioned.

## User Intent
The user requested to implement a new feature.

## Current Work
We decided to use approach A for the solution.
"#;

        let quality = check_summary_quality(good_summary, &[]);
        assert!(quality.has_required_sections);
        assert!(quality.identifier_integrity);
        assert!(quality.user_request_reflected);
        assert!(quality.has_file_paths);
        assert!(quality.has_user_requests);
        assert!(quality.has_key_decisions);
        assert!(quality.overall_score >= 0.8);

        let bad_summary = "This is just a plain text without structure.";
        let quality = check_summary_quality(bad_summary, &[]);
        assert!(!quality.has_required_sections);
        assert!(quality.overall_score < 0.8);
    }

    #[test]
    fn test_create_summary_message() {
        let summary = "Test summary content";
        let msg = create_summary_message(summary);

        if let rmcp::model::SamplingContent::Single(
            rmcp::model::SamplingMessageContent::Text(text),
        ) = &msg.content
        {
            assert!(text.text.contains("## Summary of Previous Conversation"));
            assert!(text.text.contains(summary));
        } else {
            panic!("Expected text content");
        }
    }
}
