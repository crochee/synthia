//! AgentExecutor / AgentStreamExecutor trait — 统一 Run/Resume 接口。
//!
//! 两条路径，显式 trait 边界：
//! - run(): 无状态单发 — 创建新 Session，执行，返回
//! - resume(): 有状态续跑 — 基于已有 Session 继续执行

use async_trait::async_trait;

use crate::{
    agent_session::AgentSession,
    error::AgentError,
    types::AgentOutput,
};

/// Agent 执行配置（精简版，不含重叠字段）。
///
/// 只包含运行时参数。tool_registry / hook_registry / session_store 从 AgentHandle 获取。
#[derive(Debug, Clone, Default)]
pub struct RunConfig {
    /// 拥有用户标识。
    pub user_id: String,
    /// Session 标识（可选，不提供则自动生成）。
    pub session_id: Option<String>,
    /// 最大迭代次数（覆盖 AgentConfig 中的值）。
    pub max_iterations: Option<usize>,
}

impl RunConfig {
    /// 创建带 user_id 的 RunConfig。
    pub fn for_user(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            ..Default::default()
        }
    }
}

/// Agent 执行接口 — 无状态单发 + 有状态续跑。
#[async_trait]
pub trait AgentExecutor: Send + Sync {
    /// 无状态单发 — 创建新 Session，执行，返回。
    async fn run(
        &self,
        prompt: &str,
        config: RunConfig,
    ) -> Result<AgentOutput, AgentError>;

    /// 有状态续跑 — 基于已有 Session 继续执行。
    async fn resume(
        &self,
        session: &mut AgentSession,
        prompt: &str,
    ) -> Result<AgentOutput, AgentError>;
}

/// Agent 流式执行接口 — 与 chat 同构。
#[async_trait]
pub trait AgentStreamExecutor: AgentExecutor {
    /// 无状态流式执行。
    async fn run_stream(
        &self,
        prompt: &str,
        config: RunConfig,
    ) -> Result<AgentOutput, AgentError>;

    /// 有状态流式续跑。
    async fn resume_stream(
        &self,
        session: &mut AgentSession,
        prompt: &str,
    ) -> Result<AgentOutput, AgentError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_config_default() {
        let config = RunConfig::default();
        assert!(config.user_id.is_empty());
        assert!(config.session_id.is_none());
        assert!(config.max_iterations.is_none());
    }

    #[test]
    fn run_config_for_user() {
        let config = RunConfig::for_user("alice");
        assert_eq!(config.user_id, "alice");
    }
}
