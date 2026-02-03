//! Tool registry for managing and accessing tools.

use std::{collections::BTreeSet, fmt::Debug, sync::Arc};

use dashmap::DashMap;
use moka::future::Cache;
use serde_json::Value;
use tokio::sync::{RwLock, Semaphore};

use super::Tool;
use crate::{AgentError, config::ToolConfig};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FilterKey {
    allowed: Vec<String>,
    denied: Vec<String>,
}

/// Tool registry for managing and accessing tools.
pub struct ToolRegistry {
    tools: DashMap<String, Arc<dyn Tool>>,
    config: RwLock<ToolConfig>,
    filtered_cache: Cache<FilterKey, Vec<Arc<dyn Tool>>>,
    read_pool: Arc<Semaphore>,
    write_pool: Arc<Semaphore>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tool_count", &self.tools.len())
            .field(
                "config",
                &self.config.try_read().map(|c| *c).unwrap_or_default(),
            )
            .finish()
    }
}

impl ToolRegistry {
    /// Create a new tool registry
    pub fn new() -> Self {
        Self::with_config(ToolConfig::default())
    }

    /// Create a new tool registry with custom configuration
    pub fn with_config(config: ToolConfig) -> Self {
        Self {
            tools: DashMap::new(),
            config: RwLock::new(config),
            filtered_cache: Cache::builder().max_capacity(32).build(),
            read_pool: Arc::new(Semaphore::new(config.read_pool_size)),
            write_pool: Arc::new(Semaphore::new(config.write_pool_size)),
        }
    }

    pub async fn register(&self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
        self.filtered_cache.invalidate_all();
    }

    pub async fn registers(&self, tools: impl Iterator<Item = Arc<dyn Tool>>) {
        for tool in tools {
            let name = tool.name().to_string();
            self.tools.insert(name, tool);
        }
        self.filtered_cache.invalidate_all();
    }

    /// Execute a tool with automatic pool selection.
    ///
    /// This method:
    /// 1. Gets the tool by name
    /// 2. Determines if it's read-only based on args
    /// 3. Selects appropriate pool (read or write)
    /// 4. Executes the closure with the tool
    pub async fn execute_with_tool<F, Fut, T>(
        &self,
        tool_name: &str,
        tool_args: &Value,
        f: F,
    ) -> Result<T, AgentError>
    where
        F: FnOnce(Arc<dyn Tool>) -> Fut,
        Fut: std::future::Future<Output = T> + Send,
        T: Send,
    {
        let tool = self
            .tools
            .get(tool_name)
            .map(|entry| Arc::clone(entry.value()))
            .ok_or_else(|| AgentError::tool(tool_name, "tool not found"))?;

        let is_read_only = tool.is_read_only(tool_args);

        if is_read_only {
            let permit = self.read_pool.acquire().await.map_err(|e| {
                AgentError::internal(format!("Read pool error: {e}"))
            })?;
            let result = f(tool).await;
            drop(permit);
            Ok(result)
        } else {
            let permit = self.write_pool.acquire().await.map_err(|e| {
                AgentError::internal(format!("Write pool error: {e}"))
            })?;
            let result = f(tool).await;
            drop(permit);
            Ok(result)
        }
    }

    /// Check if a tool exists
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get a tool by name (for inspection purposes)
    pub fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).map(|entry| Arc::clone(entry.value()))
    }

    pub async fn tool_count(&self) -> usize {
        self.tools.len()
    }

    pub fn tool_names(&self) -> Vec<String> {
        self.tools.iter().map(|r| r.key().clone()).collect()
    }

    pub async fn config(&self) -> ToolConfig {
        *self.config.read().await
    }

    pub async fn update_config(&self, config: ToolConfig) {
        let mut cfg = self.config.write().await;
        *cfg = config;
    }

    pub async fn filtered_tools(
        &self,
        allowed_tools: &[String],
        denied_tools: &[String],
    ) -> Vec<Arc<dyn Tool>> {
        let mut allowed = allowed_tools.to_vec();
        let mut denied = denied_tools.to_vec();
        allowed.sort();
        denied.sort();
        allowed.dedup();
        denied.dedup();
        let key = FilterKey { allowed, denied };

        if let Some(cached) = self.filtered_cache.get(&key).await {
            return cached;
        }

        let allowed_set: BTreeSet<&str> =
            allowed_tools.iter().map(String::as_str).collect();
        let denied_set: BTreeSet<&str> =
            denied_tools.iter().map(String::as_str).collect();

        let tools: Vec<Arc<dyn Tool>> = self
            .tools
            .iter()
            .filter(|entry| {
                let tool_name = entry.key();
                (allowed_set.is_empty()
                    || allowed_set.contains(tool_name.as_str()))
                    && (denied_set.is_empty()
                        || !denied_set.contains(tool_name.as_str()))
            })
            .map(|entry| Arc::clone(entry.value()))
            .collect();

        self.filtered_cache.insert(key, tools.clone()).await;

        tools
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rmcp::model::CallToolResult;
    use serde_json::Value;

    use super::*;

    struct TestTool;

    #[async_trait::async_trait]
    impl Tool for TestTool {
        fn name(&self) -> &str {
            "test_tool"
        }

        fn description(&self) -> &str {
            "Test tool"
        }

        fn parameters(&self) -> Value {
            Value::Object(serde_json::Map::new())
        }

        async fn call(&self, _args: Value) -> CallToolResult {
            CallToolResult::success(vec![])
        }
    }

    impl TestTool {
        fn new() -> Self {
            Self
        }
    }

    #[tokio::test]
    async fn test_register() {
        let registry = ToolRegistry::new();

        registry.register(Arc::new(TestTool::new())).await;
        assert_eq!(registry.tool_count().await, 1);
        assert!(registry.contains("test_tool"));
    }

    #[tokio::test]
    async fn test_tool_names() {
        let registry = ToolRegistry::new();

        registry.register(Arc::new(TestTool::new())).await;

        let names = registry.tool_names();
        assert_eq!(names.len(), 1);
        assert!(names.contains(&"test_tool".to_string()));
    }

    #[tokio::test]
    async fn test_debug_impl() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(TestTool::new())).await;

        let debug_output = format!("{registry:?}");
        assert!(debug_output.contains("ToolRegistry"));
    }

    #[tokio::test]
    async fn test_tool_registry_config() {
        let registry = ToolRegistry::new();
        let config = registry.config().await;
        assert_eq!(config.notification_interval_secs, 30);
    }

    #[tokio::test]
    async fn test_tool_registry_with_config() {
        let config = ToolConfig {
            notification_interval_secs: 60,
            max_notifications: 5,
            max_concurrent_tools: 10,
            default_tool_timeout_secs: 30,
            read_pool_size: 20,
            write_pool_size: 10,
        };
        let registry = ToolRegistry::with_config(config);
        let cfg = registry.config().await;
        assert_eq!(cfg.notification_interval_secs, 60);
        assert_eq!(cfg.max_notifications, 5);
        assert_eq!(cfg.max_concurrent_tools, 10);
        assert_eq!(cfg.read_pool_size, 20);
        assert_eq!(cfg.write_pool_size, 10);
    }

    #[tokio::test]
    async fn test_execute_with_tool() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(TestTool::new())).await;

        let result: Result<CallToolResult, _> = registry
            .execute_with_tool("test_tool", &Value::Null, |tool| async move {
                tool.call(Value::Null).await
            })
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_with_tool_not_found() {
        let registry = ToolRegistry::new();

        let result: Result<i32, _> = registry
            .execute_with_tool(
                "nonexistent",
                &Value::Null,
                |_tool| async move { 42 },
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_filtered_tools() {
        let registry = ToolRegistry::new();

        registry.register(Arc::new(TestTool::new())).await;

        struct TestTool2;
        #[async_trait::async_trait]
        impl Tool for TestTool2 {
            fn name(&self) -> &str {
                "test_tool2"
            }

            fn description(&self) -> &str {
                "Test tool 2"
            }

            fn parameters(&self) -> Value {
                Value::Object(serde_json::Map::new())
            }

            async fn call(&self, _args: Value) -> CallToolResult {
                CallToolResult::success(vec![])
            }
        }
        registry.register(Arc::new(TestTool2)).await;

        struct TestTool3;
        #[async_trait::async_trait]
        impl Tool for TestTool3 {
            fn name(&self) -> &str {
                "test_tool3"
            }

            fn description(&self) -> &str {
                "Test tool 3"
            }

            fn parameters(&self) -> Value {
                Value::Object(serde_json::Map::new())
            }

            async fn call(&self, _args: Value) -> CallToolResult {
                CallToolResult::success(vec![])
            }
        }
        registry.register(Arc::new(TestTool3)).await;

        let all_tools = registry.filtered_tools(&[], &[]).await;
        assert_eq!(all_tools.len(), 3);

        let allowed_tools = registry
            .filtered_tools(
                &["test_tool".to_string(), "test_tool2".to_string()],
                &[],
            )
            .await;
        assert_eq!(allowed_tools.len(), 2);
        let allowed_names: Vec<&str> =
            allowed_tools.iter().map(|t| t.name()).collect();
        assert!(allowed_names.contains(&"test_tool"));
        assert!(allowed_names.contains(&"test_tool2"));

        let denied_tools = registry
            .filtered_tools(&[], &["test_tool".to_string()])
            .await;
        assert_eq!(denied_tools.len(), 2);
        assert!(
            !denied_tools
                .iter()
                .map(|t| t.name())
                .any(|x| x == "test_tool")
        );
    }

    #[tokio::test]
    async fn test_registers_bulk() {
        let registry = ToolRegistry::new();

        struct ToolA;
        #[async_trait::async_trait]
        impl Tool for ToolA {
            fn name(&self) -> &str {
                "tool_a"
            }

            fn description(&self) -> &str {
                "Tool A"
            }

            fn parameters(&self) -> Value {
                Value::Object(serde_json::Map::new())
            }

            async fn call(&self, _args: Value) -> CallToolResult {
                CallToolResult::success(vec![])
            }
        }

        struct ToolB;
        #[async_trait::async_trait]
        impl Tool for ToolB {
            fn name(&self) -> &str {
                "tool_b"
            }

            fn description(&self) -> &str {
                "Tool B"
            }

            fn parameters(&self) -> Value {
                Value::Object(serde_json::Map::new())
            }

            async fn call(&self, _args: Value) -> CallToolResult {
                CallToolResult::success(vec![])
            }
        }

        let tools: Vec<Arc<dyn Tool>> = vec![
            Arc::new(ToolA) as Arc<dyn Tool>,
            Arc::new(ToolB) as Arc<dyn Tool>,
        ];
        registry.registers(tools.into_iter()).await;
        assert_eq!(registry.tool_count().await, 2);
        assert!(registry.contains("tool_a"));
        assert!(registry.contains("tool_b"));
    }

    #[tokio::test]
    async fn test_get_tool() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(TestTool::new())).await;

        let tool = registry.get_tool("test_tool");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name(), "test_tool");

        let none = registry.get_tool("nonexistent");
        assert!(none.is_none());
    }

    #[tokio::test]
    async fn test_update_config() {
        let registry = ToolRegistry::new();
        let initial = registry.config().await;
        assert_eq!(initial.read_pool_size, 10); // default from ToolConfig

        let new_config = ToolConfig {
            notification_interval_secs: 120,
            max_notifications: 100,
            max_concurrent_tools: 50,
            default_tool_timeout_secs: 60,
            read_pool_size: 100,
            write_pool_size: 50,
        };
        registry.update_config(new_config).await;

        let updated = registry.config().await;
        assert_eq!(updated.read_pool_size, 100);
        assert_eq!(updated.write_pool_size, 50);
        assert_eq!(updated.notification_interval_secs, 120);
    }

    #[tokio::test]
    async fn test_filtered_tools_denied_and_allowed_overlap() {
        let registry = ToolRegistry::new();

        struct AltTool;
        #[async_trait::async_trait]
        impl Tool for AltTool {
            fn name(&self) -> &str {
                "alt_tool"
            }

            fn description(&self) -> &str {
                "Alt tool"
            }

            fn parameters(&self) -> Value {
                Value::Object(serde_json::Map::new())
            }

            async fn call(&self, _args: Value) -> CallToolResult {
                CallToolResult::success(vec![])
            }
        }

        registry.register(Arc::new(TestTool::new())).await;
        registry.register(Arc::new(AltTool)).await;

        // Tool in both allowed and denied list - denied should take precedence
        let tools = registry
            .filtered_tools(
                &["test_tool".to_string(), "alt_tool".to_string()],
                &["test_tool".to_string()],
            )
            .await;
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "alt_tool");
    }

    #[tokio::test]
    async fn test_filtered_tools_empty_deny_allows_all() {
        let registry = ToolRegistry::new();

        struct AnotherTool;
        #[async_trait::async_trait]
        impl Tool for AnotherTool {
            fn name(&self) -> &str {
                "another_tool"
            }

            fn description(&self) -> &str {
                "Another tool"
            }

            fn parameters(&self) -> Value {
                Value::Object(serde_json::Map::new())
            }

            async fn call(&self, _args: Value) -> CallToolResult {
                CallToolResult::success(vec![])
            }
        }

        registry.register(Arc::new(TestTool::new())).await;
        registry.register(Arc::new(AnotherTool)).await;

        // Empty allowed list means all are allowed (when not denied)
        let tools = registry
            .filtered_tools(&[], &["another_tool".to_string()])
            .await;
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "test_tool");
    }

    #[test]
    fn test_filter_key_derives() {
        let key1 = FilterKey {
            allowed: vec!["a".to_string(), "b".to_string()],
            denied: vec!["c".to_string()],
        };
        let key2 = FilterKey {
            allowed: vec!["a".to_string(), "b".to_string()],
            denied: vec!["c".to_string()],
        };
        let key3 = FilterKey {
            allowed: vec!["a".to_string()],
            denied: vec!["c".to_string()],
        };

        // Test Eq, PartialEq, Hash, Debug
        assert_eq!(key1, key1);
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);

        let debug = format!("{key1:?}");
        assert!(debug.contains("FilterKey"));
    }

    #[test]
    fn test_filter_key_order_independence() {
        // FilterKey equality works regardless of field order
        let key1 = FilterKey {
            allowed: vec!["a".to_string(), "b".to_string()],
            denied: vec!["c".to_string()],
        };
        let key2 = FilterKey {
            allowed: vec!["a".to_string(), "b".to_string()],
            denied: vec!["c".to_string()],
        };
        let key3 = FilterKey {
            allowed: vec!["b".to_string(), "a".to_string()],
            denied: vec!["c".to_string()],
        };

        // Same content equals itself
        assert_eq!(key1, key1);
        assert_eq!(key2, key2);
        // Same content despite different construction order should be equal
        assert_eq!(key1, key2);
        // But different order inVec is preserved (not sorted internally)
        assert_ne!(key1.allowed, key3.allowed);
        assert_eq!(key1.allowed, vec!["a", "b"]);
        assert_eq!(key3.allowed, vec!["b", "a"]);

        // Hash consistency for same content
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(key1);
        set.insert(key2);
        assert_eq!(set.len(), 1); // key1 and key2 are equal and have same hash
    }
}
