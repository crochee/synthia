//! Agent roles module
//!
//! Defines agent roles for multi-agent coordination.

use serde::{Deserialize, Serialize};

/// Agent role in a multi-agent hierarchy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AgentRole {
    #[default]
    /// Coordinator - can spawn child agents, orchestrates workflow
    Coordinator,
    /// Worker - executes tasks with restricted permissions
    Worker,
    /// Specialist - focused on specific domain (e.g., explorer, planner)
    Specialist,
}

/// Configuration for an agent role
#[derive(Debug, Clone)]
pub struct AgentRoleConfig {
    pub role: AgentRole,
    pub allowed_tools: Vec<String>,
    pub denied_tools: Vec<String>,
    pub max_depth: usize,
    pub max_spawns: usize,
    pub model: Option<String>,
}

impl Default for AgentRoleConfig {
    fn default() -> Self {
        Self {
            role: AgentRole::default(),
            allowed_tools: Vec::new(),
            denied_tools: Vec::new(),
            max_depth: 3,
            max_spawns: 5,
            model: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_role_default() {
        assert_eq!(AgentRole::default(), AgentRole::Coordinator);
    }

    #[test]
    fn test_agent_role_variants() {
        let coordinator = AgentRole::Coordinator;
        let worker = AgentRole::Worker;
        let specialist = AgentRole::Specialist;

        match coordinator {
            AgentRole::Coordinator => {}
            _ => panic!("Expected Coordinator"),
        }
        match worker {
            AgentRole::Worker => {}
            _ => panic!("Expected Worker"),
        }
        match specialist {
            AgentRole::Specialist => {}
            _ => panic!("Expected Specialist"),
        }
    }

    #[test]
    fn test_agent_role_serde() {
        // Test serialization
        let role = AgentRole::Specialist;
        let json = serde_json::to_string(&role).unwrap();
        assert!(json.contains("Specialist"));

        // Test deserialization
        let parsed: AgentRole = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, AgentRole::Specialist);
    }

    #[test]
    fn test_agent_role_config_default() {
        let config = AgentRoleConfig::default();
        assert_eq!(config.role, AgentRole::Coordinator);
        assert!(config.allowed_tools.is_empty());
        assert!(config.denied_tools.is_empty());
        assert_eq!(config.max_depth, 3);
        assert_eq!(config.max_spawns, 5);
        assert!(config.model.is_none());
    }

    #[test]
    fn test_agent_role_config_with_role() {
        let config = AgentRoleConfig {
            role: AgentRole::Worker,
            allowed_tools: vec!["readFile".to_string()],
            denied_tools: vec!["exec".to_string()],
            max_depth: 5,
            max_spawns: 10,
            model: Some("gpt-4".to_string()),
        };

        assert_eq!(config.role, AgentRole::Worker);
        assert_eq!(config.allowed_tools, vec!["readFile".to_string()]);
        assert_eq!(config.denied_tools, vec!["exec".to_string()]);
        assert_eq!(config.max_depth, 5);
        assert_eq!(config.max_spawns, 10);
        assert_eq!(config.model, Some("gpt-4".to_string()));
    }

    #[test]
    fn test_agent_role_config_debug() {
        let config = AgentRoleConfig::default();
        let debug_str = format!("{config:?}");
        assert!(debug_str.contains("AgentRoleConfig"));
    }

    #[test]
    fn test_agent_role_equality() {
        assert_eq!(AgentRole::Coordinator, AgentRole::Coordinator);
        assert_eq!(AgentRole::Worker, AgentRole::Worker);
        assert_eq!(AgentRole::Specialist, AgentRole::Specialist);
        assert_ne!(AgentRole::Coordinator, AgentRole::Worker);
    }

    #[test]
    fn test_agent_role_clone() {
        let role = AgentRole::Specialist;
        let cloned = role.clone();
        assert_eq!(role, cloned);
    }

    #[test]
    fn test_agent_role_config_clone() {
        let config = AgentRoleConfig {
            role: AgentRole::Worker,
            allowed_tools: vec!["tool1".to_string()],
            denied_tools: vec!["tool2".to_string()],
            max_depth: 7,
            max_spawns: 15,
            model: Some("claude-3".to_string()),
        };
        let cloned = config.clone();
        assert_eq!(config.role, cloned.role);
        assert_eq!(config.allowed_tools, cloned.allowed_tools);
        assert_eq!(config.denied_tools, cloned.denied_tools);
        assert_eq!(config.max_depth, cloned.max_depth);
        assert_eq!(config.max_spawns, cloned.max_spawns);
        assert_eq!(config.model, cloned.model);
    }

    #[test]
    fn test_agent_role_serde_coordinator() {
        let json = serde_json::to_string(&AgentRole::Coordinator).unwrap();
        let parsed: AgentRole = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, AgentRole::Coordinator);
    }

    #[test]
    fn test_agent_role_serde_worker() {
        let json = serde_json::to_string(&AgentRole::Worker).unwrap();
        let parsed: AgentRole = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, AgentRole::Worker);
    }
}
