use synthia_permission::{MergedPolicy, PermissionChecker, PermissionRule};
use synthia_provider::types::Message;

use crate::{
    config::{AgentRunConfig, AgentRunStateConfig},
    control::fork_policy::ForkPolicy,
};

/// Build an [`AgentRunStateConfig`] for a sub-agent by inheriting from the
/// parent runtime configuration.
///
/// - The parent's message history is filtered through `fork_policy` via
///   [`apply_fork_policy`].
/// - The derived permission rules (e.g. from
///   [`crate::subagent::permission::derive_subagent_permission`]) are wired
///   into a fresh [`PermissionChecker`] attached to the child's
///   [`ToolRegistry`].
pub fn build_subagent_config(
    parent_config: &AgentRunConfig,
    parent_messages: &[Message],
    fork_policy: &ForkPolicy,
    subagent_permission: Vec<PermissionRule>,
) -> AgentRunStateConfig {
    let filtered_messages = apply_fork_policy(fork_policy, parent_messages);

    let mut child_config = parent_config.clone();
    let workspace_root = &parent_config.config.workspace_root;
    let checker = PermissionChecker::new(MergedPolicy::new(
        &[],
        &subagent_permission,
        &[],
    ))
    .with_workspace_root(workspace_root);
    child_config.tool_registry =
        child_config.tool_registry.with_checker(checker);

    AgentRunStateConfig {
        run_config: child_config,
        initial_messages: filtered_messages,
        start_iteration: 0,
    }
}

/// Apply a [`ForkPolicy`] to filter the parent's message history,
/// returning the subset of messages that the sub-agent should inherit.
pub fn apply_fork_policy(
    policy: &ForkPolicy,
    messages: &[Message],
) -> Vec<Message> {
    match policy {
        ForkPolicy::InheritAll => messages.to_vec(),
        ForkPolicy::LastNTurns(n) => {
            if *n == 0 {
                return messages
                    .iter()
                    .filter(|m| m.role == synthia_provider::Role::System)
                    .cloned()
                    .collect();
            }
            let mut turns: Vec<Vec<Message>> = Vec::new();
            let mut current_turn: Vec<Message> = Vec::new();
            for msg in messages.iter().rev() {
                current_turn.push(msg.clone());
                if msg.role == synthia_provider::Role::User {
                    turns.push(current_turn);
                    current_turn = Vec::new();
                    if turns.len() >= *n {
                        break;
                    }
                }
            }
            turns.into_iter().rev().flatten().collect()
        }
        ForkPolicy::Empty => Vec::new(),
        ForkPolicy::SystemOnly => messages
            .iter()
            .filter(|m| m.role == synthia_provider::Role::System)
            .cloned()
            .collect(),
        ForkPolicy::SinceStep(_) => {
            unimplemented!("ForkPolicy::SinceStep is not yet supported")
        }
        ForkPolicy::ByTag(_) => {
            unimplemented!("ForkPolicy::ByTag is not yet supported")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use synthia_permission::PermissionAction;
    use synthia_provider::types::Message;

    use super::*;

    fn dummy_parent_config() -> AgentRunConfig {
        AgentRunConfig {
            provider: Arc::new(test_support::FakeProvider::new(vec![])),
            tool_registry:
                synthia_tool::registry::ToolRegistry::register_defaults(),
            hook_registry: Arc::new(synthia_hook::HookRegistry::new()),
            model_router: Arc::new(synthia_provider::router::ModelRouter::new()),
            user_id: "u".to_string(),
            session_id: "s".to_string(),
            input: crate::input::AgentInput::text("hi"),
            config: crate::config::AgentConfig::default(),
            context_assembler: None,
            session_store: synthia_session::Store::new(std::env::temp_dir()),
            steering_channel: None,
            session_input_queue: None,
            cancel_token: tokio_util::sync::CancellationToken::new(),
            memory_event_sender: None,
            agent_control: None,
            fork_policy: ForkPolicy::InheritAll,
            compaction_provider: None,
            subagent_session_factory: None,
            approval_service: None,
            sandbox_manager: None,
            tool_orchestrator: None,
            guardian_coordinator: None,
            extension_manager: None,
        }
    }

    #[test]
    fn inherit_all_keeps_all_messages() {
        let messages = vec![
            Message::system("sys".to_string()),
            Message::user("q1".to_string()),
            Message::assistant("a1".to_string()),
        ];
        let result = apply_fork_policy(&ForkPolicy::InheritAll, &messages);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn last_n_turns_keeps_only_last_n_user_turns() {
        let messages = vec![
            Message::system("sys".to_string()),
            Message::user("q1".to_string()),
            Message::assistant("a1".to_string()),
            Message::user("q2".to_string()),
            Message::assistant("a2".to_string()),
        ];
        let result = apply_fork_policy(&ForkPolicy::LastNTurns(1), &messages);
        assert!(
            result
                .iter()
                .any(|m| m.content
                    == synthia_provider::types::Content::text("q2"))
        );
        assert!(
            !result
                .iter()
                .any(|m| m.content
                    == synthia_provider::types::Content::text("q1"))
        );
        // System messages before the last user turn are not included.
        assert!(!result.iter().any(
            |m| m.content == synthia_provider::types::Content::text("sys")
        ));
    }

    #[test]
    fn last_n_turns_zero_keeps_system_only() {
        let messages = vec![
            Message::system("sys".to_string()),
            Message::user("q1".to_string()),
            Message::assistant("a1".to_string()),
            Message::user("q2".to_string()),
            Message::assistant("a2".to_string()),
        ];
        let result = apply_fork_policy(&ForkPolicy::LastNTurns(0), &messages);
        assert!(
            result
                .iter()
                .all(|m| m.role == synthia_provider::Role::System)
        );
        assert!(
            !result
                .iter()
                .any(|m| m.role == synthia_provider::Role::User)
        );
    }

    #[test]
    fn system_only_keeps_system_messages() {
        let messages = vec![
            Message::system("sys".to_string()),
            Message::user("q1".to_string()),
            Message::assistant("a1".to_string()),
        ];
        let result = apply_fork_policy(&ForkPolicy::SystemOnly, &messages);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].content,
            synthia_provider::types::Content::text("sys")
        );
    }

    #[test]
    fn empty_fork_policy_returns_no_messages() {
        let messages = vec![
            Message::system("sys".to_string()),
            Message::user("q1".to_string()),
        ];
        let result = apply_fork_policy(&ForkPolicy::Empty, &messages);
        assert!(result.is_empty());
    }

    #[test]
    fn config_applies_fork_policy_and_wires_permissions() {
        let parent = dummy_parent_config();
        let messages = vec![Message::user("hi".to_string())];
        let rules = vec![PermissionRule {
            pattern: "bash".to_string(),
            action: PermissionAction::Deny,
            forced: true,
        }];

        let state = build_subagent_config(
            &parent,
            &messages,
            &ForkPolicy::InheritAll,
            rules,
        );

        assert!(!state.initial_messages.is_empty());
        assert_eq!(state.run_config.user_id, "u");
    }
}
