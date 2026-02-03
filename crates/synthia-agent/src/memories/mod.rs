//! Memory subsystem for startup extraction and consolidation.
//!
//! The memory pipeline is split into two phases:
//! - Phase 1: extract raw memories from conversation history
//! - Phase 2: consolidate raw memories into meaningful summaries
//!
//! For detailed documentation, see [README.md](./README.md)

pub mod cron;
pub mod data;
pub(crate) mod phase1;
pub(crate) mod phase2;
pub mod session_memory;
pub mod store;

use std::path::{Path, PathBuf};

pub use data::{
    Memory,
    MemoryImportance,
    MemoryQuery,
    MemoryStats,
    MemoryType,
    Stage1Output,
};
use rmcp::model::{
    CreateMessageRequestParams,
    ModelPreferences,
    SamplingMessage,
};
pub use store::{MemoryFileStore, MemoryStore};
use tokio_util::sync::CancellationToken;

use crate::{
    Result,
    model_router::{ModelConfig, ModelRouter},
    utils::{message_to_string, sampling_content_to_string},
};

/// Get the memory root directory
pub fn memory_root(workspace: &Path) -> PathBuf {
    workspace.join("memories")
}

async fn ensure_layout(root: &Path) -> std::io::Result<()> {
    tokio::fs::create_dir_all(root.join("rollout_summaries")).await
}

pub(crate) async fn store_stage1_output(
    root: &Path,
    output: &Stage1Output,
) -> std::io::Result<()> {
    ensure_layout(root).await?;

    let raw_memories_path = root.join("raw_memories.md");
    let mut raw_memories = String::new();

    if raw_memories_path.exists() {
        raw_memories = tokio::fs::read_to_string(&raw_memories_path).await?;
    }

    let memory_entry = format!(
        "## Thread {}\n\n{}\n\n",
        output.thread_id, output.raw_memory
    );

    raw_memories.push_str(&memory_entry);
    tokio::fs::write(raw_memories_path, raw_memories).await?;

    let summary_path = root
        .join("rollout_summaries")
        .join(format!("{}.md", output.thread_id));

    let summary_content = format!(
        "# Rollout Summary\n\n{}\n\nRaw memory: {}\n",
        output.rollout_summary, output.raw_memory
    );

    tokio::fs::write(summary_path, summary_content).await
}

pub(crate) async fn store_consolidated_memory(
    root: &Path,
    topic: &str,
    content: &str,
) -> std::io::Result<()> {
    ensure_layout(root).await?;

    let consolidated_path = root.join(format!("consolidated_{topic}.md"));
    tokio::fs::write(consolidated_path, content).await
}

pub(crate) async fn call_model_intern(
    model_router: &dyn ModelRouter,
    system_prompt: &str,
    user_prompt: &str,
    model_preferences: Option<ModelPreferences>,
) -> Result<String> {
    let msg = SamplingMessage::user_text(user_prompt);
    let result = model_router.route(std::slice::from_ref(&msg)).await?;
    let provider = result.provider;
    let config = result.config;

    let params = CreateMessageRequestParams {
        meta: None,
        task: None,
        messages: vec![msg],
        model_preferences,
        system_prompt: Some(system_prompt.to_string()),
        include_context: None,
        temperature: config.model_info().temperature,
        max_tokens: config.model_info().max_tokens,
        stop_sequences: None,
        metadata: None,
        tools: None,
        tool_choice: None,
    };

    let stream = provider.stream(params, CancellationToken::new()).await?;
    let result = synthia_provider::collect_stream(stream).await?;

    Ok(sampling_content_to_string(&result.message.content))
}

pub(crate) async fn call_model_with_routed(
    provider: &dyn synthia_provider::ModelProvider,
    config: &ModelConfig,
    system_prompt: &str,
    user_prompt: &str,
    model_preferences: Option<ModelPreferences>,
) -> Result<String> {
    let msg = SamplingMessage::user_text(user_prompt);

    let params = CreateMessageRequestParams {
        meta: None,
        task: None,
        messages: vec![msg],
        model_preferences,
        system_prompt: Some(system_prompt.to_string()),
        include_context: None,
        temperature: config.model_info().temperature,
        max_tokens: config.model_info().max_tokens,
        stop_sequences: None,
        metadata: None,
        tools: None,
        tool_choice: None,
    };

    let stream = provider.stream(params, CancellationToken::new()).await?;
    let result = synthia_provider::collect_stream(stream).await?;

    Ok(sampling_content_to_string(&result.message.content))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_memory_root() {
        let workspace = Path::new("/test/workspace");
        let root = memory_root(workspace);
        assert_eq!(root, Path::new("/test/workspace/memories"));
    }

    #[tokio::test]
    async fn test_ensure_layout() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("memory_root");
        ensure_layout(&root).await.unwrap();

        assert!(root.join("rollout_summaries").exists());
    }

    #[tokio::test]
    async fn test_store_stage1_output_creates_new_file() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        let output = Stage1Output {
            thread_id: "thread-1".to_string(),
            raw_memory: "raw memory content".to_string(),
            rollout_summary: "summary".to_string(),
            cwd: temp.path().to_path_buf(),
            source_updated_at: chrono::Utc::now(),
        };

        store_stage1_output(root, &output).await.unwrap();

        assert!(root.join("raw_memories.md").exists());
        assert!(root.join("rollout_summaries/thread-1.md").exists());
    }

    #[tokio::test]
    async fn test_store_stage1_output_appends_to_existing() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        let output1 = Stage1Output {
            thread_id: "thread-1".to_string(),
            raw_memory: "raw memory 1".to_string(),
            rollout_summary: "summary 1".to_string(),
            cwd: temp.path().to_path_buf(),
            source_updated_at: chrono::Utc::now(),
        };

        store_stage1_output(root, &output1).await.unwrap();

        let output2 = Stage1Output {
            thread_id: "thread-2".to_string(),
            raw_memory: "raw memory 2".to_string(),
            rollout_summary: "summary 2".to_string(),
            cwd: temp.path().to_path_buf(),
            source_updated_at: chrono::Utc::now(),
        };

        store_stage1_output(root, &output2).await.unwrap();

        let content = tokio::fs::read_to_string(root.join("raw_memories.md"))
            .await
            .unwrap();
        assert!(content.contains("thread-1"));
        assert!(content.contains("thread-2"));
    }

    #[tokio::test]
    async fn test_store_consolidated_memory() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        store_consolidated_memory(root, "topic1", "consolidated content")
            .await
            .unwrap();

        assert!(root.join("consolidated_topic1.md").exists());
        let content =
            tokio::fs::read_to_string(root.join("consolidated_topic1.md"))
                .await
                .unwrap();
        assert_eq!(content, "consolidated content");
    }

    #[tokio::test]
    async fn test_store_consolidated_memory_updates_existing() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        store_consolidated_memory(root, "topic1", "first content")
            .await
            .unwrap();
        store_consolidated_memory(root, "topic1", "updated content")
            .await
            .unwrap();

        let content =
            tokio::fs::read_to_string(root.join("consolidated_topic1.md"))
                .await
                .unwrap();
        assert_eq!(content, "updated content");
    }
}
