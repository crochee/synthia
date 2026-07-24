//! Agent configuration
//!
//! Configuration for agent behavior and capabilities.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AgentName {
    /// 独立工作的 agent
    #[default]
    Solo,
    /// 团队领导者 - 可以创建团队、分配任务
    Lead,
    /// 团队成员或子智能体
    /// String 是 agent 的名称（如 "reviewer", "tester", "code-reviewer"）
    Custom(String),
}

impl AgentName {
    pub fn is_solo(&self) -> bool {
        matches!(self, AgentName::Solo)
    }

    pub fn is_lead(&self) -> bool {
        matches!(self, AgentName::Lead)
    }

    pub fn is_custom(&self) -> bool {
        matches!(self, AgentName::Custom(_))
    }

    pub fn as_str(&self) -> &str {
        match self {
            AgentName::Solo => "solo",
            AgentName::Lead => "lead",
            AgentName::Custom(name) => name,
        }
    }
}

impl fmt::Display for AgentName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for AgentName {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AgentName {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "solo" => AgentName::Solo,
            "lead" => AgentName::Lead,
            _ => AgentName::Custom(s),
        })
    }
}
