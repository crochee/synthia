//! Fork policies that govern what a spawned sub-agent inherits from its parent.

use serde::{Deserialize, Serialize};

/// Policy describing what conversation history / rollout items a forked
/// sub-agent receives from its parent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ForkPolicy {
    /// Inherit the parent's full conversation history.
    InheritAll,
    /// Inherit only the last N turns from the parent.
    LastNTurns(usize),
    /// Inherit history only since the given step index.
    SinceStep(usize),
    /// Inherit only items tagged with the given tag.
    ByTag(String),
    /// Start with an empty history.
    Empty,
    /// Inherit only system messages and tool definitions.
    #[default]
    SystemOnly,
}

/// Policy describing how a forked sub-agent inherits permission rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ForkPermissionPolicy {
    /// Inherit the parent's full permission policy.
    InheritAll,
    /// Inherit the parent's policy but downgrade all rules to the User layer.
    #[default]
    InheritAsUser,
    /// Inherit the parent's policy but downgrade all rules to the Agent layer.
    InheritAsAgent,
    /// Start with an empty permission policy.
    Empty,
}

/// Decide whether a given rollout item should be retained when applying a
/// [`ForkPolicy`] to a parent's history. The `total_steps` argument is the
/// total number of steps in the parent's history; indices are 0-based.
pub fn keep_forked_rollout_item(
    policy: &ForkPolicy,
    index: usize,
    total_steps: usize,
    is_system: bool,
    tags: &[String],
) -> bool {
    match policy {
        ForkPolicy::InheritAll => true,
        ForkPolicy::LastNTurns(n) => {
            let n = *n;
            if total_steps <= n {
                true
            } else {
                index >= total_steps - n
            }
        }
        ForkPolicy::SinceStep(step) => index >= *step,
        ForkPolicy::ByTag(tag) => tags.iter().any(|t| t == tag),
        ForkPolicy::Empty => false,
        ForkPolicy::SystemOnly => is_system,
    }
}

// ---------------------------------------------------------------------------
// Definition Drift Telemetry (task 6.5)
// ---------------------------------------------------------------------------

/// Result of comparing a sub-agent's effective definition against the parent's
/// fork policy expectations.
///
/// Emitted on sub-agent completion to detect when a sub-agent's behavior
/// diverged from what the parent expected (e.g. different permission rules,
/// tool access, or system prompt).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionDrift {
    pub sub_agent_id: String,
    /// Fields that differed between expected and actual.
    pub drifted_fields: Vec<String>,
    /// Severity: "minor" (cosmetic), "moderate" (tools/permissions changed),
    /// "severe" (system prompt or core constraints changed).
    pub severity: &'static str,
}

/// Compare the sub-agent's resolved definition against the parent's fork
/// policy to detect drift.
///
/// Returns `Some(DefinitionDrift)` when drift is detected, or `None` when
/// the sub-agent's definition is consistent with expectations.
pub fn detect_definition_drift(
    parent_system_prompt_hash: &str,
    parent_denied_tools: &[String],
    sub_agent_id: &str,
    sub_system_prompt_hash: &str,
    sub_denied_tools: &[String],
) -> Option<DefinitionDrift> {
    let mut drifted_fields = Vec::new();

    if parent_system_prompt_hash != sub_system_prompt_hash {
        drifted_fields.push("system_prompt".to_string());
    }

    let parent_set: std::collections::HashSet<_> =
        parent_denied_tools.iter().collect();
    let sub_set: std::collections::HashSet<_> =
        sub_denied_tools.iter().collect();
    if parent_set != sub_set {
        drifted_fields.push("denied_tools".to_string());
    }

    if drifted_fields.is_empty() {
        return None;
    }

    let severity = if drifted_fields.contains(&"system_prompt".to_string()) {
        "severe"
    } else {
        "moderate"
    };

    Some(DefinitionDrift {
        sub_agent_id: sub_agent_id.to_string(),
        drifted_fields,
        severity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inherit_all_keeps_everything() {
        assert!(keep_forked_rollout_item(
            &ForkPolicy::InheritAll,
            0,
            10,
            false,
            &[]
        ));
        assert!(keep_forked_rollout_item(
            &ForkPolicy::InheritAll,
            9,
            10,
            true,
            &[]
        ));
    }

    #[test]
    fn test_empty_keeps_nothing() {
        assert!(!keep_forked_rollout_item(
            &ForkPolicy::Empty,
            0,
            10,
            true,
            &[]
        ));
        assert!(!keep_forked_rollout_item(
            &ForkPolicy::Empty,
            5,
            10,
            false,
            &[]
        ));
    }

    #[test]
    fn test_system_only_keeps_only_system_items() {
        assert!(keep_forked_rollout_item(
            &ForkPolicy::SystemOnly,
            0,
            10,
            true,
            &[]
        ));
        assert!(!keep_forked_rollout_item(
            &ForkPolicy::SystemOnly,
            1,
            10,
            false,
            &[]
        ));
    }

    #[test]
    fn test_last_n_turns_with_short_history() {
        // history shorter than N: keep everything
        assert!(keep_forked_rollout_item(
            &ForkPolicy::LastNTurns(5),
            0,
            3,
            false,
            &[]
        ));
        assert!(keep_forked_rollout_item(
            &ForkPolicy::LastNTurns(5),
            2,
            3,
            false,
            &[]
        ));
    }

    #[test]
    fn test_last_n_turns_with_long_history() {
        // keep last 2 of 10: indices 8 and 9
        assert!(!keep_forked_rollout_item(
            &ForkPolicy::LastNTurns(2),
            0,
            10,
            false,
            &[]
        ));
        assert!(!keep_forked_rollout_item(
            &ForkPolicy::LastNTurns(2),
            7,
            10,
            false,
            &[]
        ));
        assert!(keep_forked_rollout_item(
            &ForkPolicy::LastNTurns(2),
            8,
            10,
            false,
            &[]
        ));
        assert!(keep_forked_rollout_item(
            &ForkPolicy::LastNTurns(2),
            9,
            10,
            false,
            &[]
        ));
    }

    #[test]
    fn test_since_step() {
        let policy = ForkPolicy::SinceStep(3);
        assert!(!keep_forked_rollout_item(&policy, 2, 10, false, &[]));
        assert!(keep_forked_rollout_item(&policy, 3, 10, false, &[]));
        assert!(keep_forked_rollout_item(&policy, 9, 10, false, &[]));
    }

    #[test]
    fn test_by_tag() {
        let tags_a = vec!["important".to_string()];
        let tags_b = vec!["other".to_string()];
        assert!(keep_forked_rollout_item(
            &ForkPolicy::ByTag("important".to_string()),
            0,
            10,
            false,
            &tags_a
        ));
        assert!(!keep_forked_rollout_item(
            &ForkPolicy::ByTag("important".to_string()),
            0,
            10,
            false,
            &tags_b
        ));
    }

    #[test]
    fn test_default_policies() {
        // Phase 5: default combination is SystemOnly + InheritAsUser.
        assert_eq!(ForkPolicy::default(), ForkPolicy::SystemOnly);
        assert_eq!(
            ForkPermissionPolicy::default(),
            ForkPermissionPolicy::InheritAsUser
        );
    }

    // definition_drift tests (task 6.5)
    #[test]
    fn drift_detect_no_drift_when_identical() {
        assert!(
            detect_definition_drift("hash-a", &[], "sub-1", "hash-a", &[])
                .is_none()
        );
    }

    #[test]
    fn drift_detect_system_prompt_change_is_severe() {
        let drift =
            detect_definition_drift("hash-a", &[], "sub-1", "hash-b", &[])
                .expect("drift expected");
        assert_eq!(drift.severity, "severe");
        assert_eq!(drift.drifted_fields, vec!["system_prompt"]);
        assert_eq!(drift.sub_agent_id, "sub-1");
    }

    #[test]
    fn drift_detect_denied_tools_change_is_moderate() {
        let drift = detect_definition_drift(
            "hash-a",
            &["bash".to_string()],
            "sub-1",
            "hash-a",
            &["rm".to_string()],
        )
        .expect("drift expected");
        assert_eq!(drift.severity, "moderate");
        assert_eq!(drift.drifted_fields, vec!["denied_tools"]);
    }

    #[test]
    fn drift_detect_both_fields_is_severe() {
        let drift = detect_definition_drift(
            "hash-a",
            &["bash".to_string()],
            "sub-1",
            "hash-b",
            &["rm".to_string()],
        )
        .expect("drift expected");
        assert_eq!(drift.severity, "severe");
        assert!(drift.drifted_fields.contains(&"system_prompt".to_string()));
        assert!(drift.drifted_fields.contains(&"denied_tools".to_string()));
    }
}
