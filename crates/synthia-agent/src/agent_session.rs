//! AgentSession — 私有会话状态，每次运行独立。
//!
//! AgentSession 持有运行时状态（对话历史、token 预算、循环状态、压缩状态），
//! 反指所属 AgentHandle。每个 AgentSession 只属于一次 run/resume 调用。

use synthia_provider::types::Message;
use synthia_session::types::TokenBudget;
use tokio_util::sync::CancellationToken;

use crate::turn::TurnId;

/// 循环状态 — 从 AgentRunConfig / LoopContext 中提取。
#[derive(Debug, Clone)]
pub struct LoopState {
    /// 当前迭代次数。
    pub iteration: usize,
    /// 当前 turn 标识。
    pub turn_id: Option<TurnId>,
    /// 最大迭代次数。
    pub max_iterations: usize,
    /// 是否应该停止。
    pub should_stop: bool,
}

impl LoopState {
    /// 创建初始循环状态。
    pub fn new(max_iterations: usize) -> Self {
        Self {
            iteration: 0,
            turn_id: None,
            max_iterations,
            should_stop: false,
        }
    }

    /// 推进到下一迭代。
    pub fn advance(&mut self) {
        self.iteration += 1;
        if self.iteration >= self.max_iterations {
            self.should_stop = true;
        }
    }

    /// 设置新的 turn id。
    pub fn set_turn_id(&mut self, id: TurnId) {
        self.turn_id = Some(id);
    }
}

impl Default for LoopState {
    fn default() -> Self {
        Self::new(100)
    }
}

/// 压缩状态 — 追踪上下文压缩历史。
#[derive(Debug, Clone, Default)]
pub struct CompactionState {
    /// 上次压缩时的迭代号。
    pub last_compact_iteration: usize,
    /// 累计压缩次数。
    pub compact_count: usize,
}

/// 私有会话状态 — 每次运行独立。
///
/// AgentSession 持有运行时状态，反指所属 AgentHandle (agent_id)。
/// 一个 AgentHandle 可跨 N 个 AgentSession 复用。
#[derive(Debug, Clone)]
pub struct AgentSession {
    /// Session 唯一标识。
    pub id: String,
    /// 所属 AgentHandle 的 id。
    pub agent_id: String,
    /// 对话历史。
    pub history: Vec<Message>,
    /// Token 预算。
    pub token_budget: Option<TokenBudget>,
    /// 循环状态。
    pub loop_state: LoopState,
    /// 压缩状态。
    pub compaction_state: CompactionState,
    /// 取消令牌。
    pub cancel_token: CancellationToken,
}

impl AgentSession {
    /// 创建新的 AgentSession。
    pub fn new(agent_id: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.to_string(),
            history: Vec::new(),
            token_budget: None,
            loop_state: LoopState::default(),
            compaction_state: CompactionState::default(),
            cancel_token: CancellationToken::new(),
        }
    }

    /// 创建带指定 id 的 AgentSession。
    pub fn with_id(id: String, agent_id: &str) -> Self {
        let mut session = Self::new(agent_id);
        session.id = id;
        session
    }

    /// 设置 token 预算。
    pub fn with_token_budget(mut self, budget: TokenBudget) -> Self {
        self.token_budget = Some(budget);
        self
    }

    /// 设置最大迭代次数。
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.loop_state.max_iterations = max;
        self
    }

    /// 设置取消令牌。
    pub fn with_cancel_token(mut self, token: CancellationToken) -> Self {
        self.cancel_token = token;
        self
    }

    /// 推入消息到历史。
    pub fn push_message(&mut self, message: Message) {
        self.history.push(message);
    }

    /// 获取对话历史（只读）。
    pub fn get_history(&self) -> &[Message] {
        &self.history
    }

    /// 标记压缩发生。
    pub fn mark_compacted(&mut self) {
        self.compaction_state.last_compact_iteration =
            self.loop_state.iteration;
        self.compaction_state.compact_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_new() {
        let session = AgentSession::new("agent-1");
        assert_eq!(session.agent_id, "agent-1");
        assert!(session.history.is_empty());
        assert!(session.token_budget.is_none());
        assert_eq!(session.loop_state.iteration, 0);
        assert!(!session.loop_state.should_stop);
    }

    #[test]
    fn session_push_message() {
        let mut session = AgentSession::new("agent-1");
        session.push_message(Message::user("hello"));
        assert_eq!(session.history.len(), 1);
    }

    #[test]
    fn loop_state_advance() {
        let mut state = LoopState::new(3);
        state.advance(); // iteration 1
        assert_eq!(state.iteration, 1);
        assert!(!state.should_stop);
        state.advance(); // iteration 2
        assert_eq!(state.iteration, 2);
        assert!(!state.should_stop);
        state.advance(); // iteration 3 >= max_iterations
        assert_eq!(state.iteration, 3);
        assert!(state.should_stop);
    }

    #[test]
    fn compaction_state() {
        let mut session = AgentSession::new("agent-1");
        session.loop_state.iteration = 5;
        session.mark_compacted();
        assert_eq!(session.compaction_state.last_compact_iteration, 5);
        assert_eq!(session.compaction_state.compact_count, 1);
    }
}
