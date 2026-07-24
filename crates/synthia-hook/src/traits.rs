use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use synthia_core::Error;
use synthia_provider::types::{Message, ToolUse};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

impl ToolCall {
    pub fn from_tool_use(tu: &ToolUse) -> Self {
        Self {
            id: tu.id.clone(),
            name: tu.name.clone(),
            input: tu.input.clone(),
        }
    }

    pub fn from_value(v: &serde_json::Value) -> Option<Self> {
        let name = v.get("name")?.as_str()?.to_string();
        let id = v
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let input = v.get("input").cloned().unwrap_or(serde_json::Value::Null);
        Some(Self { id, name, input })
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "name": self.name,
            "input": self.input,
        })
    }
}

pub fn message_from_value(v: &serde_json::Value) -> Option<Message> {
    serde_json::from_value(v.clone()).ok()
}

pub fn message_to_value(m: &Message) -> serde_json::Value {
    serde_json::to_value(m).unwrap_or(serde_json::Value::Null)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContext {
    pub session_id: String,
    pub turn_id: String,
    pub iteration: usize,
    pub context_tokens: usize,
    pub messages: Vec<Message>,
    pub pending_tool_calls: Vec<ToolCall>,
    pub last_response: Option<serde_json::Value>,
    pub last_tool_call: Option<ToolCall>,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl AgentContext {
    pub fn new(session_id: String, turn_id: String) -> Self {
        Self {
            session_id,
            turn_id,
            iteration: 0,
            context_tokens: 0,
            messages: Vec::new(),
            pending_tool_calls: Vec::new(),
            last_response: None,
            last_tool_call: None,
            metadata: std::collections::HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FailPolicy {
    #[default]
    FailOpen,
    FailClosed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolAction {
    Proceed,
    Skip,
    Modify(serde_json::Value),
    PendingConfirm {
        tool_call: serde_json::Value,
        timeout_secs: u64,
        blocking: bool,
    },
}

#[async_trait]
pub trait AgentHook: Send + Sync + std::fmt::Debug {
    fn fail_policy(&self) -> FailPolicy {
        FailPolicy::default()
    }

    async fn on_error(
        &self,
        _ctx: &AgentContext,
        _error: &synthia_core::Error,
    ) {
    }

    async fn on_before_llm(
        &self,
        _ctx: &mut AgentContext,
    ) -> Result<(), Error> {
        Ok(())
    }

    async fn on_after_llm(
        &self,
        _ctx: &AgentContext,
        _response: &serde_json::Value,
    ) -> Result<(), Error> {
        Ok(())
    }

    async fn on_before_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
    ) -> Result<ToolAction, Error> {
        Ok(ToolAction::Proceed)
    }

    async fn on_after_tool(
        &self,
        _ctx: &AgentContext,
        _call: &serde_json::Value,
        _result: &serde_json::Value,
    ) -> Result<(), Error> {
        Ok(())
    }

    async fn on_iteration_end(
        &self,
        _ctx: &AgentContext,
        _iteration: usize,
    ) -> Result<(), Error> {
        Ok(())
    }

    async fn on_complete(&self, _ctx: &AgentContext) -> Result<(), Error> {
        Ok(())
    }
}
