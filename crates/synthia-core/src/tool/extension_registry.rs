//! ExtensionRegistry — 聚合所有子 Registry 的顶层结构。
//!
//! Phase 2.5: 提供统一的生命周期管理，聚合五种正交扩展维度：
//! - ToolRegistry：行动能力
//! - FragmentRegistry：上下文注入
//! - InterceptorChain：横切拦截（在 synthia-agent crate 中）
//! - SkillRegistry：技能组合（Phase 3）
//! - PluginRegistry：第三方插件（Phase 3）

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::tool::{
    fragment::FragmentRegistry,
    plugin_registry::PluginRegistry,
    registry::ToolRegistry,
    skill_registry::SkillRegistry,
};

/// 扩展注册中心运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionState {
    /// 正在初始化
    Initializing,
    /// 运行中
    Running,
    /// 正在关闭
    ShuttingDown,
    /// 已关闭
    Closed,
}

/// 扩展注册中心错误类型。
#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    /// 扩展注册中心正在关闭
    #[error("extension registry is shutting down")]
    ShuttingDown,
    /// 扩展注册中心已关闭
    #[error("extension registry is closed")]
    Closed,
    /// 健康检查失败
    #[error("health check failed: {0}")]
    HealthCheckFailed(String),
}

/// 健康检查结果。
#[derive(Debug)]
pub struct HealthCheckResult {
    /// 已注册工具数量
    pub tool_count: usize,
    /// 已注册片段数量
    pub fragment_count: usize,
    /// 已注册技能数量
    pub skill_count: usize,
    /// 已注册插件数量
    pub plugin_count: usize,
    /// 当前运行状态
    pub state: ExtensionState,
    /// 是否健康（Running 状态视为健康）
    pub healthy: bool,
}

/// 扩展注册中心 — 聚合所有子 Registry，提供统一的生命周期管理。
///
/// 五种正交扩展维度：
/// - ToolRegistry：行动能力
/// - FragmentRegistry：上下文注入
/// - InterceptorChain：横切拦截（在 synthia-agent crate 中）
/// - SkillRegistry：技能组合（Phase 3）
/// - PluginRegistry：第三方插件（Phase 3）
pub struct ExtensionRegistry {
    /// 工具注册表
    tool_registry: Arc<ToolRegistry>,
    /// 片段注册表
    fragment_registry: Arc<FragmentRegistry>,
    /// 技能注册表
    skill_registry: Arc<SkillRegistry>,
    /// 插件注册表
    plugin_registry: Arc<PluginRegistry>,
    /// 运行状态
    state: RwLock<ExtensionState>,
}

impl Clone for ExtensionRegistry {
    fn clone(&self) -> Self {
        let state = self
            .state
            .try_read()
            .map(|s| *s)
            .unwrap_or(ExtensionState::Running);
        Self {
            tool_registry: Arc::clone(&self.tool_registry),
            fragment_registry: Arc::clone(&self.fragment_registry),
            skill_registry: Arc::clone(&self.skill_registry),
            plugin_registry: Arc::clone(&self.plugin_registry),
            state: RwLock::new(state),
        }
    }
}

impl ExtensionRegistry {
    /// 创建新的 ExtensionRegistry，持有给定的子 Registry。
    ///
    /// 初始状态为 `Running`。SkillRegistry 和 PluginRegistry
    /// 默认创建空实例；调用方可通过访问器注册内容。
    pub fn new(
        tool_registry: Arc<ToolRegistry>,
        fragment_registry: Arc<FragmentRegistry>,
    ) -> Self {
        Self {
            tool_registry,
            fragment_registry,
            skill_registry: Arc::new(SkillRegistry::new()),
            plugin_registry: Arc::new(PluginRegistry::new()),
            state: RwLock::new(ExtensionState::Running),
        }
    }

    /// 创建包含所有五个子 Registry 的 ExtensionRegistry。
    pub fn with_all(
        tool_registry: Arc<ToolRegistry>,
        fragment_registry: Arc<FragmentRegistry>,
        skill_registry: Arc<SkillRegistry>,
        plugin_registry: Arc<PluginRegistry>,
    ) -> Self {
        Self {
            tool_registry,
            fragment_registry,
            skill_registry,
            plugin_registry,
            state: RwLock::new(ExtensionState::Running),
        }
    }

    /// 获取工具注册表引用。
    pub fn tool_registry(&self) -> &Arc<ToolRegistry> {
        &self.tool_registry
    }

    /// 获取片段注册表引用。
    pub fn fragment_registry(&self) -> &Arc<FragmentRegistry> {
        &self.fragment_registry
    }

    /// 获取技能注册表引用。
    pub fn skill_registry(&self) -> &Arc<SkillRegistry> {
        &self.skill_registry
    }

    /// 获取插件注册表引用。
    pub fn plugin_registry(&self) -> &Arc<PluginRegistry> {
        &self.plugin_registry
    }

    /// 获取当前状态。
    pub async fn state(&self) -> ExtensionState {
        *self.state.read().await
    }

    /// 优雅关闭：设置 ShuttingDown 状态。
    ///
    /// 关闭流程：Running/ShuttingDown → ShuttingDown → Closed。
    /// 如果已经是 Closed，返回 `ExtensionError::Closed`。
    pub async fn shutdown(&self) -> Result<(), ExtensionError> {
        let mut state = self.state.write().await;
        match *state {
            ExtensionState::Closed => Err(ExtensionError::Closed),
            ExtensionState::Initializing
            | ExtensionState::Running
            | ExtensionState::ShuttingDown => {
                *state = ExtensionState::ShuttingDown;
                // 立即推进到 Closed
                *state = ExtensionState::Closed;
                Ok(())
            }
        }
    }

    /// 健康检查：验证所有子 Registry 状态正常。
    ///
    /// 当状态为 `Running` 时视为健康。
    pub async fn health_check(&self) -> HealthCheckResult {
        let state = *self.state.read().await;
        let tool_count = self.tool_registry.tool_count();
        let fragment_count = self.fragment_registry.fragment_count().await;
        let skill_count = self.skill_registry.skill_count().await;
        let plugin_count = self.plugin_registry.plugin_count().await;
        let healthy = state == ExtensionState::Running;
        HealthCheckResult {
            tool_count,
            fragment_count,
            skill_count,
            plugin_count,
            state,
            healthy,
        }
    }

    /// 检查当前是否允许操作（状态为 Running）。
    pub async fn is_operational(&self) -> bool {
        *self.state.read().await == ExtensionState::Running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_registry() -> ExtensionRegistry {
        ExtensionRegistry::new(
            Arc::new(ToolRegistry::new()),
            Arc::new(FragmentRegistry::new()),
        )
    }

    #[tokio::test]
    async fn new_creates_with_running_state() {
        let reg = new_registry();
        assert_eq!(reg.state().await, ExtensionState::Running);
    }

    #[tokio::test]
    async fn tool_registry_accessor() {
        let reg = new_registry();
        let tool_reg = reg.tool_registry();
        assert_eq!(tool_reg.tool_count(), 0);
    }

    #[tokio::test]
    async fn fragment_registry_accessor() {
        let reg = new_registry();
        let frag_reg = reg.fragment_registry();
        assert_eq!(frag_reg.fragment_count().await, 0);
    }

    #[tokio::test]
    async fn shutdown_transitions_state() {
        let reg = new_registry();
        assert_eq!(reg.state().await, ExtensionState::Running);

        reg.shutdown().await.unwrap();
        assert_eq!(reg.state().await, ExtensionState::Closed);
    }

    #[tokio::test]
    async fn shutdown_idempotent() {
        let reg = new_registry();

        // 第一次关闭成功
        reg.shutdown().await.unwrap();
        assert_eq!(reg.state().await, ExtensionState::Closed);

        // 第二次关闭返回 Closed 错误
        let result = reg.shutdown().await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ExtensionError::Closed));

        // 状态仍为 Closed
        assert_eq!(reg.state().await, ExtensionState::Closed);
    }

    #[tokio::test]
    async fn health_check_when_running() {
        let reg = new_registry();
        let result = reg.health_check().await;

        assert_eq!(result.tool_count, 0);
        assert_eq!(result.fragment_count, 0);
        assert_eq!(result.skill_count, 0);
        assert_eq!(result.plugin_count, 0);
        assert_eq!(result.state, ExtensionState::Running);
        assert!(result.healthy);
    }

    #[tokio::test]
    async fn health_check_when_closed() {
        let reg = new_registry();
        reg.shutdown().await.unwrap();

        let result = reg.health_check().await;
        assert_eq!(result.state, ExtensionState::Closed);
        assert!(!result.healthy);
    }

    #[tokio::test]
    async fn operations_blocked_when_shutting_down() {
        let reg = new_registry();

        // 关闭后状态为 Closed，is_operational 返回 false
        reg.shutdown().await.unwrap();
        assert!(!reg.is_operational().await);
    }
}
