//! agent_as_tool() — 把 AgentHandle 包成 Tool 的纯函数。
//!
//! 这是整个 Multi-Agent 模式层的基石：
//! - Orchestrator = agents as tools, LLM picks whom
//! - GeneratorVerifier = gen + ver as tools, loop until PASS
//! - Workflow = pipe(agents as tools)
//! - Transfer = bidir agent_as_tool injection
//!
//! 没有 sub-agent 概念。Agent 就是 Tool，Tool 就是 Agent。

use std::sync::Arc;

use async_trait::async_trait;
use synthia_tool::{
    traits::{ExecutionMode, Tool},
    types::{ToolInput, ToolOutput},
};

use crate::handle::AgentHandle;

/// 把 AgentHandle 包成 Tool — 纯函数，无副作用。
///
/// `AgentAsTool::call()` 创建新 AgentSession，执行 handle.run()，返回 ToolOutput。
/// 每次调用创建独立 Session，不共享状态。
pub fn agent_as_tool(handle: Arc<AgentHandle>) -> AgentAsTool {
    AgentAsTool { handle }
}

/// Agent-as-Tool 包装器。
///
/// 将一个 AgentHandle 包装为 Tool trait 实现，
/// 使得 LLM 可以像调用普通工具一样调用另一个 agent。
pub struct AgentAsTool {
    handle: Arc<AgentHandle>,
}

impl AgentAsTool {
    /// 获取底层 AgentHandle 的引用。
    pub fn handle(&self) -> &AgentHandle {
        &self.handle
    }
}

#[async_trait]
impl Tool for AgentAsTool {
    fn name(&self) -> &str {
        &self.handle.id
    }

    fn description(&self) -> &str {
        &self.handle.description
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Task description to send to the agent"
                },
                "context": {
                    "type": "string",
                    "description": "Additional context for the agent (optional)"
                }
            },
            "required": ["prompt"]
        })
    }

    fn requires_permission(&self) -> bool {
        false
    }

    fn is_concurrency_safe(&self) -> bool {
        // Agent 调用可能修改外部状态，保守起见返回 false
        false
    }

    fn execution_mode(&self) -> ExecutionMode {
        // Agent 调用应串行执行（避免并发冲突）
        ExecutionMode::Sequential
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        // 提取 prompt 参数
        let prompt = input
            .input
            .as_object()
            .and_then(|obj| obj.get("prompt"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if prompt.is_empty() {
            return ToolOutput::error("prompt parameter is required");
        }

        // 提取可选 context 参数
        let context = input
            .input
            .as_object()
            .and_then(|obj| obj.get("context"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // 构建完整 prompt
        let full_prompt = if context.is_empty() {
            prompt.to_string()
        } else {
            format!("{prompt}\n\nContext:\n{context}")
        };

        // TODO: Phase 2 — 实际调用 handle.run(session, &full_prompt)
        // 当前 Phase 1 只返回 prompt 确认，Phase 2 接入 Agent::run_stream
        ToolOutput::text(format!(
            "[AgentAsTool] agent={id} prompt={prompt}",
            id = self.handle.id,
            prompt = full_prompt
        ))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn agent_as_tool_parameters_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Task description to send to the agent"
                },
                "context": {
                    "type": "string",
                    "description": "Additional context for the agent (optional)"
                }
            },
            "required": ["prompt"]
        });
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("prompt"))
        );
    }
}
