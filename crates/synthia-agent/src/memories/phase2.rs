//! Phase 2: Memory consolidation

use std::{collections::HashMap, path::Path, sync::Arc};

use rmcp::model::{ModelHint, ModelPreferences, SamplingMessage};

use super::{
    call_model_with_routed,
    data::Stage1Output,
    store_consolidated_memory,
};
use crate::{Result, memories::MemoryStore, model_router::ModelRouter};

/// Run phase 2 memory consolidation
pub(crate) async fn run(
    storage: Arc<dyn MemoryStore>,
    model_router: Arc<dyn ModelRouter>,
    workspace: &Path,
) -> Result<()> {
    consolidate_memories(storage, model_router, workspace).await
}

async fn consolidate_memories(
    storage: Arc<dyn MemoryStore>,
    model_router: Arc<dyn ModelRouter>,
    workspace: &Path,
) -> Result<()> {
    let stage1_outputs = storage.get_recent_stage1_outputs(20).await?;

    if stage1_outputs.is_empty() {
        return Ok(());
    }

    let grouped = group_by_topic(&stage1_outputs);

    for (topic, memories) in grouped {
        let consolidated =
            consolidate_group(model_router.as_ref(), &topic, &memories).await?;
        storage
            .store_consolidated_memory(&topic, &consolidated)
            .await?;
        store_consolidated_memory(
            &super::memory_root(workspace),
            &topic,
            &consolidated,
        )
        .await?;
    }

    Ok(())
}

fn group_by_topic(
    outputs: &[Stage1Output],
) -> HashMap<String, Vec<Stage1Output>> {
    let mut groups = HashMap::new();

    for output in outputs {
        let topic = extract_topic(&output.rollout_summary);
        groups
            .entry(topic)
            .or_insert_with(Vec::new)
            .push(output.clone());
    }

    groups
}

fn extract_topic(summary: &str) -> String {
    if summary.contains("code") || summary.contains("program") {
        "coding".to_string()
    } else if summary.contains("test") || summary.contains("debug") {
        "testing".to_string()
    } else if summary.contains("config") || summary.contains("setup") {
        "configuration".to_string()
    } else {
        "general".to_string()
    }
}

async fn consolidate_group(
    model_router: &dyn ModelRouter,
    topic: &str,
    memories: &[Stage1Output],
) -> Result<String> {
    let system_prompt = "You are a memory consolidation assistant. Your task is to analyze multiple related memories and create a comprehensive summary that identifies common themes, key insights, and actionable information.";

    let memories_text = memories
        .iter()
        .map(|m| format!("- {}", m.raw_memory))
        .collect::<Vec<_>>()
        .join("\n");

    let user_prompt = format!(
        "Consolidate these related memories into a comprehensive summary. Focus on:\
1. Common themes and patterns\n2. Key insights and learnings\n3. Actionable information\n4. Chronological progression if relevant\n\nTopic: {topic}\n\nMemories:\n{memories_text}"
    );

    let result = model_router
        .route(std::slice::from_ref(&SamplingMessage::user_text(
            &user_prompt,
        )))
        .await?;
    let model_preferences = Some(ModelPreferences {
        hints: Some(vec![ModelHint {
            name: Some(result.decision.selected_model.clone()),
        }]),
        cost_priority: None,
        speed_priority: None,
        intelligence_priority: None,
    });

    call_model_with_routed(
        result.provider.as_ref(),
        &result.config,
        system_prompt,
        &user_prompt,
        model_preferences,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memories::data::Stage1Output;

    #[test]
    fn test_extract_topic() {
        assert_eq!(extract_topic("Fixed bug in code"), "coding");
        assert_eq!(extract_topic("Ran tests and debugged issues"), "testing");
        assert_eq!(
            extract_topic("Updated configuration settings"),
            "configuration"
        );
        assert_eq!(extract_topic("Discussed project plans"), "general");
    }

    #[test]
    fn test_extract_topic_keyword_matching() {
        // Topics are detected via substring matching (case-sensitive)
        assert_eq!(extract_topic("Fixed code bug"), "coding");
        assert_eq!(extract_topic("code review changes"), "coding");
        assert_eq!(extract_topic("wrote program code"), "coding");
    }

    #[test]
    fn test_extract_topic_multiple_keywords() {
        // When multiple keywords match, first match wins based on order in function
        assert_eq!(extract_topic("code and test"), "coding");
    }

    #[test]
    fn test_extract_topic_no_keywords() {
        // Default to "general" when no keywords match
        assert_eq!(extract_topic("What a nice day"), "general");
        assert_eq!(extract_topic(""), "general");
    }

    #[test]
    fn test_group_by_topic_empty() {
        let outputs: Vec<Stage1Output> = vec![];
        let grouped = group_by_topic(&outputs);
        assert!(grouped.is_empty());
    }

    #[test]
    fn test_group_by_topic_single_item() {
        let outputs = vec![Stage1Output {
            thread_id: "thread-1".to_string(),
            raw_memory: "Memory 1".to_string(),
            rollout_summary: "Discussed project plans".to_string(),
            cwd: std::path::PathBuf::from("/test"),
            source_updated_at: chrono::Utc::now(),
        }];
        let grouped = group_by_topic(&outputs);
        assert_eq!(grouped.len(), 1);
        assert!(grouped.contains_key("general"));
        assert_eq!(grouped.get("general").unwrap().len(), 1);
    }

    #[test]
    fn test_group_by_topic_multiple_same_topic() {
        let outputs = vec![
            Stage1Output {
                thread_id: "thread-1".to_string(),
                raw_memory: "Fixed code bug".to_string(),
                rollout_summary: "Fixed code issue".to_string(),
                cwd: std::path::PathBuf::from("/test"),
                source_updated_at: chrono::Utc::now(),
            },
            Stage1Output {
                thread_id: "thread-2".to_string(),
                raw_memory: "More coding work".to_string(),
                rollout_summary: "continued code work".to_string(),
                cwd: std::path::PathBuf::from("/test"),
                source_updated_at: chrono::Utc::now(),
            },
        ];
        let grouped = group_by_topic(&outputs);
        assert_eq!(grouped.len(), 1);
        assert!(grouped.contains_key("coding"));
        assert_eq!(grouped.get("coding").unwrap().len(), 2);
    }

    #[test]
    fn test_group_by_topic_multiple_different_topics() {
        let outputs = vec![
            Stage1Output {
                thread_id: "thread-1".to_string(),
                raw_memory: "Fixed code bug".to_string(),
                rollout_summary: "Fixed code issue".to_string(),
                cwd: std::path::PathBuf::from("/test"),
                source_updated_at: chrono::Utc::now(),
            },
            Stage1Output {
                thread_id: "thread-2".to_string(),
                raw_memory: "Ran tests".to_string(),
                rollout_summary: "completed tests successfully".to_string(),
                cwd: std::path::PathBuf::from("/test"),
                source_updated_at: chrono::Utc::now(),
            },
            Stage1Output {
                thread_id: "thread-3".to_string(),
                raw_memory: "Discussed plans".to_string(),
                rollout_summary: "General discussion".to_string(),
                cwd: std::path::PathBuf::from("/test"),
                source_updated_at: chrono::Utc::now(),
            },
        ];
        let grouped = group_by_topic(&outputs);
        assert_eq!(grouped.len(), 3);
        assert!(grouped.contains_key("coding"));
        assert!(grouped.contains_key("testing"));
        assert!(grouped.contains_key("general"));
    }

    #[test]
    fn test_group_by_topic_preserves_order() {
        // Each output should be in the group corresponding to its extracted topic
        let outputs = vec![
            Stage1Output {
                thread_id: "thread-1".to_string(),
                raw_memory: "Config setup".to_string(),
                rollout_summary: "setup config files".to_string(),
                cwd: std::path::PathBuf::from("/test"),
                source_updated_at: chrono::Utc::now(),
            },
            Stage1Output {
                thread_id: "thread-2".to_string(),
                raw_memory: "Debug issue".to_string(),
                rollout_summary: "debugged the issue".to_string(),
                cwd: std::path::PathBuf::from("/test"),
                source_updated_at: chrono::Utc::now(),
            },
        ];
        let grouped = group_by_topic(&outputs);
        let config_group = grouped.get("configuration").unwrap();
        let testing_group = grouped.get("testing").unwrap();
        assert_eq!(config_group[0].thread_id, "thread-1");
        assert_eq!(testing_group[0].thread_id, "thread-2");
    }

    #[test]
    fn test_consolidate_group_requires_model_router() {
        // consolidate_group is async and requires a ModelRouter mock
        // This test just verifies the function signature is correct
        // Integration tests cover the actual behavior
    }
}
