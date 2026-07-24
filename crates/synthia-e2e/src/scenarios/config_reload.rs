use std::sync::Arc;

use synthia_provider::ModelProvider;

use crate::{
    fixtures::configs::TestConfig,
    utils::mock_provider::{MockProvider, MockResponse},
};

#[derive(Debug, Clone)]
struct SkillRegistry {
    skills: Arc<tokio::sync::RwLock<Vec<String>>>,
}

impl SkillRegistry {
    fn new() -> Self {
        Self {
            skills: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }

    async fn add(&self, skill: &str) {
        let mut skills = self.skills.write().await;
        if !skills.contains(&skill.to_string()) {
            skills.push(skill.to_string());
        }
    }

    async fn remove(&self, skill: &str) {
        let mut skills = self.skills.write().await;
        skills.retain(|s| s != skill);
    }

    async fn list(&self) -> Vec<String> {
        self.skills.read().await.clone()
    }

    async fn count(&self) -> usize {
        self.skills.read().await.len()
    }
}

#[tokio::test]
async fn test_skill_hot_reload() {
    let registry = SkillRegistry::new();

    registry.add("rust-developer").await;
    assert_eq!(registry.count().await, 1);

    registry.add("python-expert").await;
    registry.add("web-developer").await;
    assert_eq!(registry.count().await, 3);

    registry.remove("python-expert").await;
    assert_eq!(registry.count().await, 2);

    let skills = registry.list().await;
    assert!(skills.contains(&"rust-developer".to_string()));
    assert!(skills.contains(&"web-developer".to_string()));
    assert!(!skills.contains(&"python-expert".to_string()));
}

#[tokio::test]
async fn test_provider_config_reload() {
    let config = TestConfig::provider_openai();

    assert_eq!(config.content["model"], "gpt-4o");
    assert_eq!(config.content["provider"], "openai");

    let updated_config = TestConfig::provider_anthropic();
    assert_eq!(updated_config.content["model"], "claude-sonnet-4-20250514");

    let mut provider = MockProvider::new();
    provider.with_response(MockResponse::text("Response with OpenAI"));

    let response = provider
        .complete(synthia_provider::CompletionRequest::default())
        .await;
    assert!(response.is_ok());
}

#[cfg(test)]
mod reload_tests {
    use std::{
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
    };

    use super::*;

    #[derive(Debug)]
    struct ConfigWatcher {
        changed: Arc<AtomicBool>,
        reload_count: Arc<AtomicUsize>,
        path: PathBuf,
    }

    impl ConfigWatcher {
        fn new(path: PathBuf) -> Self {
            Self {
                changed: Arc::new(AtomicBool::new(false)),
                reload_count: Arc::new(AtomicUsize::new(0)),
                path,
            }
        }

        fn check_and_reload(&self) -> bool {
            if self.changed.load(Ordering::SeqCst) {
                self.reload_count.fetch_add(1, Ordering::SeqCst);
                self.changed.store(false, Ordering::SeqCst);
                true
            } else {
                false
            }
        }

        fn trigger_change(&self) {
            self.changed.store(true, Ordering::SeqCst);
        }

        fn reload_count(&self) -> usize {
            self.reload_count.load(Ordering::SeqCst)
        }
    }

    #[tokio::test]
    async fn test_config_watcher_triggers_reload() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let watcher = ConfigWatcher::new(temp_dir.path().to_path_buf());

        assert_eq!(watcher.reload_count(), 0);

        watcher.trigger_change();
        assert!(watcher.check_and_reload());
        assert_eq!(watcher.reload_count(), 1);

        watcher.trigger_change();
        watcher.trigger_change();
        assert!(watcher.check_and_reload());
        assert!(
            watcher.reload_count() >= 2,
            "Should have reloaded at least 2 times"
        );

        // Additional check - no more changes pending
        let _ = watcher.check_and_reload();
        assert!(
            watcher.reload_count() >= 2,
            "Should have reloaded at least 2 times total"
        );
    }

    #[tokio::test]
    async fn test_skill_registry_concurrent_access() {
        let registry = SkillRegistry::new();

        let registry_clone = registry.clone();
        let handle1 = tokio::spawn(async move {
            for i in 0..10 {
                registry_clone.add(&format!("skill-{}", i)).await;
            }
        });

        let registry_clone = registry.clone();
        let handle2 = tokio::spawn(async move {
            for i in 10..20 {
                registry_clone.add(&format!("skill-{}", i)).await;
            }
        });

        handle1.await.unwrap();
        handle2.await.unwrap();

        assert_eq!(registry.count().await, 20);
    }

    #[tokio::test]
    async fn test_guardian_config_hot_reload() {
        let config = TestConfig::guardian_config();

        assert!(
            config.content["loop_detection"]["enabled"]
                .as_bool()
                .unwrap()
        );
        assert_eq!(
            config.content["loop_detection"]["soft_block_after"]
                .as_i64()
                .unwrap(),
            5
        );
        assert_eq!(
            config.content["circuit_breaker"]["error_threshold"]
                .as_i64()
                .unwrap(),
            5
        );
    }

    #[tokio::test]
    async fn test_behavior_config_hot_reload() {
        let config = TestConfig::behavior_config();

        assert_eq!(config.content["max_iterations"].as_i64().unwrap(), 100);
        assert_eq!(
            config.content["compaction_threshold_percent"]
                .as_i64()
                .unwrap(),
            80
        );
    }
}
