//! Generator-Verifier 闭环模式 — 生成→验证→循环直到 PASS。

use std::{path::PathBuf, sync::Arc};

use synthia_tool::{
    traits::Tool,
    types::{ToolInput, ToolOutput},
};
use thiserror::Error;

use crate::{a2t::agent_as_tool, handle::AgentHandle};

/// Generator-Verifier 执行超过最大循环次数。
#[derive(Debug, Error)]
#[error("generator-verifier exceeded max rounds ({max_rounds})")]
pub struct MaxRoundsExceeded {
    pub max_rounds: usize,
}

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

/// Generator-Verifier 闭环模式。
///
/// 生成器生成输出，验证器验证输出，循环直到验证通过或超过最大次数。
/// 典型用例：生成代码 → 跑测试 → 失败则修 → 再跑 → PASS
///
/// 这是 agent_as_tool() 的组合——不是新概念。
pub struct GeneratorVerifier {
    /// 生成器 agent。
    pub generator: Arc<AgentHandle>,
    /// 验证器 agent。
    pub verifier: Arc<AgentHandle>,
    /// 最大循环次数。
    pub max_rounds: usize,
    /// 判定 PASS 的函数。输入验证器输出文本，返回 true 表示通过。
    pub pass_fn: Box<dyn Fn(&str) -> bool + Send + Sync>,
}

impl GeneratorVerifier {
    /// 创建 GeneratorVerifier。
    pub fn new(
        generator: Arc<AgentHandle>,
        verifier: Arc<AgentHandle>,
        max_rounds: usize,
        pass_fn: impl Fn(&str) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            generator,
            verifier,
            max_rounds,
            pass_fn: Box::new(pass_fn),
        }
    }

    /// 执行 Generator-Verifier 循环。
    ///
    /// 1. 生成器生成输出
    /// 2. 验证器验证输出
    /// 3. 如果通过，返回生成器输出
    /// 4. 否则，将验证器反馈注入下一轮生成器 prompt
    /// 5. 循环直到通过或超过 max_rounds
    pub async fn run(&self, task: &str) -> Result<String, MaxRoundsExceeded> {
        let gen_tool = agent_as_tool(self.generator.clone());
        let ver_tool = agent_as_tool(self.verifier.clone());

        let mut feedback = String::new();

        for round in 0..self.max_rounds {
            // 构建生成器 prompt（包含前轮反馈）
            let gen_prompt = if feedback.is_empty() {
                task.to_string()
            } else {
                format!("{task}\n\nPrevious attempt feedback:\n{feedback}")
            };

            // 生成器生成输出
            let gen_input = make_tool_input(gen_tool.name(), &gen_prompt);
            let gen_output = gen_tool.call(gen_input).await;
            let gen_text = extract_text(&gen_output);

            // 验证器验证输出
            let ver_input = make_tool_input(ver_tool.name(), &gen_text);
            let ver_output = ver_tool.call(ver_input).await;
            let ver_text = extract_text(&ver_output);

            // 判定是否通过
            if (self.pass_fn)(&ver_text) {
                tracing::info!(round = round + 1, "Generator-Verifier: PASS");
                return Ok(gen_text);
            }

            feedback = ver_text;
            tracing::info!(
                round = round + 1,
                max_rounds = self.max_rounds,
                "Generator-Verifier: FAIL, continuing"
            );
        }

        tracing::warn!(
            max_rounds = self.max_rounds,
            "Generator-Verifier: max rounds exceeded"
        );
        Err(MaxRoundsExceeded {
            max_rounds: self.max_rounds,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_rounds_exceeded_error() {
        let err = MaxRoundsExceeded { max_rounds: 5 };
        assert!(err.to_string().contains("5"));
    }

    #[test]
    fn extract_text_from_output() {
        let output = ToolOutput::text("hello world");
        assert_eq!(extract_text(&output), "hello world");
    }
}
