//! Workflow 流水线模式 — 多 agent 串行 pipe。

use std::{path::PathBuf, sync::Arc};

use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

use crate::{a2t::agent_as_tool, handle::AgentHandle};

/// 从 ToolOutput 提取文本内容。
fn extract_text(output: &ToolOutput) -> String {
    output
        .content
        .iter()
        .filter_map(|part| match part {
            synthia_provider::types::ContentPart::Text(t) => {
                Some(t.text.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// 构建简单 ToolInput（name + prompt）。
fn make_tool_input(name: &str, prompt: &str) -> ToolInput {
    ToolInput {
        name: name.to_string(),
        input: serde_json::json!({ "prompt": prompt }),
        context: synthia_tool::types::ToolExecutionContext::new(
            String::new(),
            PathBuf::new(),
        ),
    }
}

/// Workflow 流水线模式。
///
/// 多个 agent 按序执行，前一阶段输出作为下一阶段输入。
/// 典型用例：分析 → 设计 → 实现 → 测试
///
/// 这是 agent_as_tool() 的组合——不是新概念。
pub struct Workflow {
    /// 阶段列表。按序执行。
    pub stages: Vec<Arc<AgentHandle>>,
}

impl Workflow {
    /// 创建 Workflow。
    pub fn new(stages: Vec<Arc<AgentHandle>>) -> Self {
        Self { stages }
    }

    /// 执行 Workflow 流水线。
    pub async fn run(&self, input: &str) -> String {
        let mut current = input.to_string();

        for (i, stage) in self.stages.iter().enumerate() {
            let tool = agent_as_tool(stage.clone());
            let stage_input = make_tool_input(tool.name(), &current);
            let output = tool.call(stage_input).await;
            current = extract_text(&output);

            tracing::info!(
                stage = i + 1,
                total_stages = self.stages.len(),
                stage_id = %stage.id,
                "Workflow: stage completed"
            );
        }

        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_empty_stages() {
        let workflow = Workflow::new(vec![]);
        assert_eq!(workflow.stages.len(), 0);
    }
}
