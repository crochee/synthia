use std::collections::HashMap;

use crate::{
    layer::RuleLayer,
    rule::{PermissionAction, PermissionRule},
};

/// Resolved permission policy assembled from three priority layers.
///
/// Layers are merged with strict priority: `User > Agent > Default`.
/// Within a single layer, rules later in the input slice override earlier
/// rules with the same `pattern`.
///
/// # ADR-2026-06-10
///
/// This is the unified permission policy after 6-expert adversarial review
/// (R1 Architect, R2 Security, R3 Performance, R4 Rust, R5 Concurrency, R6 Devil's Advocate).
///
/// Trait abstraction (D1-D4) was rejected as over-engineered.
/// Re-evaluation of trait abstraction is scheduled for 6 months from 2026-06-10.
#[derive(Debug, Clone, Default)]
pub struct MergedPolicy {
    rules: HashMap<String, (PermissionAction, bool, RuleLayer)>,
}

impl MergedPolicy {
    pub fn new(
        default_rules: &[PermissionRule],
        agent_rules: &[PermissionRule],
        user_rules: &[PermissionRule],
    ) -> Self {
        let mut rules: HashMap<String, (PermissionAction, bool, RuleLayer)> =
            HashMap::new();

        for rule in default_rules {
            rules.insert(
                rule.pattern.clone(),
                (rule.action, rule.forced, RuleLayer::Default),
            );
        }
        for rule in agent_rules {
            rules.insert(
                rule.pattern.clone(),
                (rule.action, rule.forced, RuleLayer::Agent),
            );
        }
        for rule in user_rules {
            rules.insert(
                rule.pattern.clone(),
                (rule.action, rule.forced, RuleLayer::User),
            );
        }

        Self { rules }
    }

    /// Resolve a request `pattern` to a `PermissionAction`.
    ///
    /// A rule's `forced` flag always yields `Deny`, regardless of the rule's
    /// declared action. Patterns with no matching rule default to `Ask`
    /// (fail-closed). Unknown tools require explicit user confirmation.
    pub fn evaluate(&self, pattern: &str) -> PermissionAction {
        self.rules
            .get(pattern)
            .map(|(action, forced, _)| {
                if *forced {
                    PermissionAction::Deny
                } else {
                    *action
                }
            })
            .unwrap_or(PermissionAction::Ask)
    }

    /// Number of distinct patterns currently held by the policy.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Returns true when no rules are stored.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Add a forced Deny rule for `pattern` at the `Agent` layer.
    ///
    /// The `forced` flag is honored by [`MergedPolicy::evaluate`]: a forced
    /// rule always resolves to `Deny` regardless of the declared action.
    /// This is used to integrate an agent's `denied_tools` list into the
    /// merged policy.
    pub fn add_forced_deny_rule(&mut self, pattern: &str) {
        self.rules.insert(
            pattern.to_string(),
            (PermissionAction::Deny, true, RuleLayer::Agent),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(
        pattern: &str,
        action: PermissionAction,
        forced: bool,
    ) -> PermissionRule {
        PermissionRule {
            pattern: pattern.to_string(),
            action,
            forced,
        }
    }

    #[test]
    fn empty_inputs_ask_by_default() {
        // ADR-2026-06-10: fail-closed default
        let policy = MergedPolicy::new(&[], &[], &[]);
        assert!(policy.is_empty());
        assert_eq!(policy.evaluate("anything"), PermissionAction::Ask);
    }

    #[test]
    fn single_default_rule_is_returned() {
        let defaults = vec![rule("bash", PermissionAction::Deny, false)];
        let policy = MergedPolicy::new(&defaults, &[], &[]);
        assert_eq!(policy.evaluate("bash"), PermissionAction::Deny);
        assert_eq!(policy.evaluate("read"), PermissionAction::Ask);
    }

    #[test]
    fn agent_rule_overrides_default_with_same_pattern() {
        let defaults = vec![rule("write", PermissionAction::Deny, false)];
        let agents = vec![rule("write", PermissionAction::Allow, false)];
        let policy = MergedPolicy::new(&defaults, &agents, &[]);
        assert_eq!(policy.evaluate("write"), PermissionAction::Allow);
    }

    #[test]
    fn user_rule_overrides_agent_and_default() {
        let defaults = vec![rule("net", PermissionAction::Allow, false)];
        let agents = vec![rule("net", PermissionAction::Ask, false)];
        let users = vec![rule("net", PermissionAction::Deny, false)];
        let policy = MergedPolicy::new(&defaults, &agents, &users);
        assert_eq!(policy.evaluate("net"), PermissionAction::Deny);
    }

    #[test]
    fn forced_rule_always_returns_deny() {
        let defaults = vec![rule("bash", PermissionAction::Allow, true)];
        let policy = MergedPolicy::new(&defaults, &[], &[]);
        assert_eq!(policy.evaluate("bash"), PermissionAction::Deny);
    }

    #[test]
    fn test_forced_short_circuit() {
        let policy = MergedPolicy::new(
            &[PermissionRule {
                pattern: "bash:*".into(),
                action: PermissionAction::Allow,
                forced: true,
            }],
            &[],
            &[],
        );
        assert_eq!(policy.evaluate("bash:*"), PermissionAction::Deny);
    }

    #[test]
    fn user_can_override_forced_default_with_non_forced_allow() {
        let defaults = vec![rule("bash", PermissionAction::Allow, true)];
        let users = vec![rule("bash", PermissionAction::Allow, false)];
        let policy = MergedPolicy::new(&defaults, &[], &users);
        assert_eq!(policy.evaluate("bash"), PermissionAction::Allow);
    }

    #[test]
    fn user_forced_deny_wins_over_agent_ask() {
        let agents = vec![rule("rm", PermissionAction::Ask, false)];
        let users = vec![rule("rm", PermissionAction::Deny, true)];
        let policy = MergedPolicy::new(&[], &agents, &users);
        assert_eq!(policy.evaluate("rm"), PermissionAction::Deny);
    }

    #[test]
    fn child_priority_within_same_layer_later_wins() {
        let agents = vec![
            rule("bash", PermissionAction::Allow, false),
            rule("bash", PermissionAction::Deny, false),
        ];
        let policy = MergedPolicy::new(&[], &agents, &[]);
        assert_eq!(policy.evaluate("bash"), PermissionAction::Deny);
        assert_eq!(policy.len(), 1);
    }

    #[test]
    fn distinct_patterns_coexist() {
        let defaults = vec![rule("read", PermissionAction::Allow, false)];
        let agents = vec![rule("write", PermissionAction::Ask, false)];
        let users = vec![rule("net", PermissionAction::Deny, false)];
        let policy = MergedPolicy::new(&defaults, &agents, &users);
        assert_eq!(policy.len(), 3);
        assert_eq!(policy.evaluate("read"), PermissionAction::Allow);
        assert_eq!(policy.evaluate("write"), PermissionAction::Ask);
        assert_eq!(policy.evaluate("net"), PermissionAction::Deny);
        // ADR-2026-06-10: missing patterns now Ask (fail-closed)
        assert_eq!(policy.evaluate("missing"), PermissionAction::Ask);
    }

    #[test]
    fn layer_does_not_leak_into_unrelated_patterns() {
        let defaults = vec![rule("a", PermissionAction::Deny, false)];
        let agents = vec![rule("b", PermissionAction::Allow, false)];
        let policy = MergedPolicy::new(&defaults, &agents, &[]);
        assert_eq!(policy.evaluate("a"), PermissionAction::Deny);
        assert_eq!(policy.evaluate("b"), PermissionAction::Allow);
    }

    // ADR-2026-06-10: explicit fail-closed test
    #[test]
    fn unknown_pattern_asks() {
        let policy = MergedPolicy::default();
        assert_eq!(policy.evaluate("nonexistent_tool"), PermissionAction::Ask);
    }
}
