//! Agent role system

use std::collections::BTreeMap;

use crate::config::{AgentConfig, AgentName};

pub const DEFAULT_ROLE_NAME: &str = "default";

pub mod built_in {
    use std::path::Path;

    use super::*;
    use crate::GuardianConfig;

    pub fn configs(workspace_dir: &Path) -> BTreeMap<String, AgentConfig> {
        BTreeMap::from([
            (
                DEFAULT_ROLE_NAME.to_string(),
                AgentConfig {
                    name: AgentName::Solo,
                    models: Default::default(),
                    description: "Default agent".to_string(),
                    allowed_tools: Default::default(),
                    denied_tools: Default::default(),
                    hidden: Default::default(),
                    workspace_dir: workspace_dir.to_path_buf(),
                    is_subagent: false,
                    guardian: GuardianConfig::default(),
                    prompt: None,
                }
            ),
            (
                "explorer".to_string(),
                AgentConfig {
                    name: AgentName::Custom("explorer".to_string()),
                    models: Default::default(),
                    description: "Code explorer agent for fast and authoritative codebase analysis".to_string(),
                    allowed_tools: vec!["readFile".to_string(), "glob".to_string(), "grep".to_string(), "listDirectory".to_string(), "directoryTree".to_string()],
                    denied_tools: vec!["exec".to_string(), "writeFile".to_string(), "editFile".to_string(), "deleteFile".to_string()],
                    hidden: Default::default(),
                    workspace_dir: workspace_dir.to_path_buf(),
                    is_subagent: false,
                    guardian: GuardianConfig::default(),
                    prompt: None,
                }
            ),
            (
                "worker".to_string(),
                AgentConfig {
                    name: AgentName::Custom("worker".to_string()),
                    models: Default::default(),
                    description: r#"Use for execution and production work.
Typical tasks:
- Implement part of a feature
- Fix tests or bugs
- Split large refactors into independent chunks
Rules:
- Explicitly assign **ownership** of the task (files / responsibility). When the subtask involves code changes, you should clearly specify which files or modules the worker is responsible for. This helps avoid merge conflicts and ensures accountability.
- Always tell workers they are **not alone in the codebase**, and they should not revert the edits made by others, and they should adjust their implementation to accommodate the changes made by others."#.to_string(),
                    allowed_tools: Default::default(),
                    denied_tools: Default::default(),
                    hidden: Default::default(),
                    workspace_dir: workspace_dir.to_path_buf(),
                    is_subagent: false,
                    guardian: GuardianConfig::default(),
                    prompt: None,
                }
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_default_role_name() {
        assert_eq!(DEFAULT_ROLE_NAME, "default");
    }

    #[test]
    fn test_built_in_configs_returns_map() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);

        assert!(!configs.is_empty());
        assert!(configs.contains_key(DEFAULT_ROLE_NAME));
    }

    #[test]
    fn test_built_in_configs_contains_default_role() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);

        let default_config = configs.get(DEFAULT_ROLE_NAME).unwrap();
        assert_eq!(
            default_config.name,
            AgentName::Custom(DEFAULT_ROLE_NAME.to_string())
        );
        assert_eq!(default_config.description, "Default agent");
        assert!(!default_config.is_subagent);
        assert_eq!(default_config.workspace_dir, workspace);
    }

    #[test]
    fn test_built_in_configs_contains_explorer_role() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);

        let explorer = configs.get("explorer").unwrap();
        assert_eq!(explorer.name, AgentName::Custom("explorer".to_string()));
        assert!(explorer.description.contains("Code explorer"));
        // Explorer should have read-only tools allowed
        assert!(explorer.allowed_tools.contains(&"readFile".to_string()));
        assert!(explorer.allowed_tools.contains(&"glob".to_string()));
        assert!(explorer.allowed_tools.contains(&"grep".to_string()));
        // Explorer should deny write tools
        assert!(explorer.denied_tools.contains(&"exec".to_string()));
        assert!(explorer.denied_tools.contains(&"writeFile".to_string()));
        assert!(explorer.denied_tools.contains(&"editFile".to_string()));
        assert!(explorer.denied_tools.contains(&"deleteFile".to_string()));
    }

    #[test]
    fn test_built_in_configs_contains_worker_role() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);

        let worker = configs.get("worker").unwrap();
        assert_eq!(worker.name, AgentName::Custom("worker".to_string()));
        assert!(worker.description.contains("execution"));
        // Worker should have no restrictions by default
        assert!(worker.allowed_tools.is_empty());
        assert!(worker.denied_tools.is_empty());
    }

    #[test]
    fn test_built_in_configs_has_three_roles() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);
        assert_eq!(configs.len(), 3);
    }

    #[test]
    fn test_built_in_configs_all_have_guardian() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);

        for (_, config) in configs {
            assert!(config.guardian.enabled);
        }
    }

    #[test]
    fn test_built_in_configs_all_have_workspace() {
        let workspace = PathBuf::from("/custom/workspace");
        let configs = built_in::configs(&workspace);

        for (_, config) in configs {
            assert_eq!(config.workspace_dir, workspace);
        }
    }

    #[test]
    fn test_explorer_denies_all_write_tools() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);

        let explorer = configs.get("explorer").unwrap();
        let write_tools = ["exec", "writeFile", "editFile", "deleteFile"];
        for tool in write_tools {
            assert!(
                explorer.denied_tools.contains(&tool.to_string()),
                "Explorer should deny {tool}"
            );
        }
    }

    #[test]
    fn test_explorer_allows_read_tools() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);

        let explorer = configs.get("explorer").unwrap();
        let read_tools =
            ["readFile", "glob", "grep", "listDirectory", "directoryTree"];
        for tool in read_tools {
            assert!(
                explorer.allowed_tools.contains(&tool.to_string()),
                "Explorer should allow {tool}"
            );
        }
    }

    #[test]
    fn test_worker_role_has_no_tool_restrictions() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);

        let worker = configs.get("worker").unwrap();
        assert!(worker.allowed_tools.is_empty());
        assert!(worker.denied_tools.is_empty());
    }

    #[test]
    fn test_default_role_is_not_subagent() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);

        let default_role = configs.get(DEFAULT_ROLE_NAME).unwrap();
        assert!(!default_role.is_subagent);
    }

    #[test]
    fn test_explorer_role_is_not_subagent() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);

        let explorer = configs.get("explorer").unwrap();
        assert!(!explorer.is_subagent);
    }

    #[test]
    fn test_worker_role_is_not_subagent() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);

        let worker = configs.get("worker").unwrap();
        assert!(!worker.is_subagent);
    }

    #[test]
    fn test_default_role_has_no_allowed_tools() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);

        let default_role = configs.get(DEFAULT_ROLE_NAME).unwrap();
        assert!(default_role.allowed_tools.is_empty());
    }

    #[test]
    fn test_default_role_has_no_denied_tools() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);

        let default_role = configs.get(DEFAULT_ROLE_NAME).unwrap();
        assert!(default_role.denied_tools.is_empty());
    }

    #[test]
    fn test_explorer_has_five_allowed_tools() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);

        let explorer = configs.get("explorer").unwrap();
        assert_eq!(explorer.allowed_tools.len(), 5);
    }

    #[test]
    fn test_explorer_has_four_denied_tools() {
        let workspace = PathBuf::from("/tmp");
        let configs = built_in::configs(&workspace);

        let explorer = configs.get("explorer").unwrap();
        assert_eq!(explorer.denied_tools.len(), 4);
    }
}
