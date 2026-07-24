use chrono::{DateTime, Utc};
use synthia_core::Error;
use synthia_hook::ToolCall;
use ulid::Ulid;

#[derive(Debug, Clone)]
pub struct ReasoningStep {
    pub id: Ulid,
    pub iteration: usize,
    pub thought: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub observation: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct ReasoningChain {
    steps: Vec<ReasoningStep>,
    _current_thought: Option<String>,
}

impl ReasoningChain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, step: ReasoningStep) {
        self.steps.push(step);
    }

    pub fn steps(&self) -> &[ReasoningStep] {
        &self.steps
    }

    pub fn backtrack(&mut self, to_iteration: usize) -> Result<(), Error> {
        let idx = self
            .steps
            .iter()
            .position(|s| s.iteration == to_iteration)
            .ok_or_else(|| {
                Error::Internal(format!(
                    "Iteration {} not found in reasoning chain",
                    to_iteration
                ))
            })?;
        self.steps.truncate(idx + 1);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use ulid::Ulid;

    use super::*;

    #[test]
    fn test_reasoning_chain_push() {
        let mut chain = ReasoningChain::new();
        let step = ReasoningStep {
            id: Ulid::new(),
            iteration: 1,
            thought: "First thought".to_string(),
            tool_calls: None,
            observation: None,
            timestamp: Utc::now(),
        };
        chain.push(step.clone());
        assert_eq!(chain.steps().len(), 1);
    }

    #[test]
    fn test_reasoning_chain_backtrack() {
        let mut chain = ReasoningChain::new();
        for i in 1..=5 {
            chain.push(ReasoningStep {
                id: Ulid::new(),
                iteration: i,
                thought: format!("Thought {}", i),
                tool_calls: None,
                observation: None,
                timestamp: Utc::now(),
            });
        }
        chain.backtrack(3).unwrap();
        assert_eq!(chain.steps().len(), 3);
        assert_eq!(chain.steps().last().unwrap().iteration, 3);
    }
}
