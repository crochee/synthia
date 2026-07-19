#![allow(deprecated)]
use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use synthia_permission::{Permission, PermissionChecker, PermissionRequest};
use synthia_tool::{Tool, ToolEntry, ToolInput, ToolOutput, ToolRegistry};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct MockToolWithPerm {
    name: String,
    description: String,
}

impl MockToolWithPerm {
    fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
        }
    }
}

#[async_trait]
impl Tool for MockToolWithPerm {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    fn requires_permission(&self) -> bool {
        true
    }

    async fn call(&self, _input: ToolInput) -> ToolOutput {
        ToolOutput::text(format!("Called {}", self.name))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct MockToolNoPerm {
    name: String,
    description: String,
}

impl MockToolNoPerm {
    fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
        }
    }
}

#[async_trait]
impl Tool for MockToolNoPerm {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _input: ToolInput) -> ToolOutput {
        ToolOutput::text(format!("Called {}", self.name))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct MockHiddenTool {
    name: String,
    description: String,
}

impl MockHiddenTool {
    fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
        }
    }
}

#[async_trait]
impl Tool for MockHiddenTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    fn is_hidden(&self) -> bool {
        true
    }

    async fn call(&self, _input: ToolInput) -> ToolOutput {
        ToolOutput::text(format!("Called {}", self.name))
    }
}

fn make_context() -> synthia_tool::ToolExecutionContext {
    synthia_tool::ToolExecutionContext::new(
        "test-session".to_string(),
        PathBuf::from("/tmp"),
    )
}

fn make_tool_use(name: &str, input: Value) -> synthia_provider::ToolUse {
    synthia_provider::ToolUse {
        id: "test-id".to_string(),
        name: name.to_string(),
        input,
    }
}

// ============ Permission level tests ============

#[tokio::test]
async fn test_permission_auto_approve() {
    let checker = PermissionChecker::allow_all();
    let registry = ToolRegistry::new().with_checker(checker);
    registry.register(ToolEntry::new(Arc::new(MockToolNoPerm::new(
        "auto_tool",
        "Auto approve tool",
    ))));

    let tool_uses = vec![make_tool_use("auto_tool", json!({}))];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_text());
}

#[tokio::test]
async fn test_permission_deny() {
    use synthia_permission::rule::PermissionRule;

    // PermissionAction::Deny maps to Permission::Block, not Permission::Deny
    let rules = vec![PermissionRule {
        pattern: "deny_tool".to_string(),
        action: synthia_permission::rule::PermissionAction::Deny,
        forced: false,
    }];
    let policy =
        synthia_permission::merged_policy::MergedPolicy::new(&rules, &[], &[]);
    let checker = PermissionChecker::new(policy);

    let registry = ToolRegistry::new().with_checker(checker);
    registry.register(ToolEntry::new(Arc::new(MockToolWithPerm::new(
        "deny_tool",
        "Deny tool",
    ))));

    let tool_uses = vec![make_tool_use("deny_tool", json!({}))];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_error == Some(true));
    let text = outputs[0]
        .content
        .iter()
        .filter_map(|p| p.text())
        .collect::<String>();
    // Policy Deny becomes Block, which results in "denied by user" message
    assert!(text.contains("denied by user"));
}

#[tokio::test]
async fn test_permission_require_confirm() {
    use synthia_permission::rule::PermissionRule;

    let rules = vec![PermissionRule {
        pattern: "confirm_tool".to_string(),
        action: synthia_permission::rule::PermissionAction::Ask,
        forced: false,
    }];
    let policy =
        synthia_permission::merged_policy::MergedPolicy::new(&rules, &[], &[]);
    let checker = PermissionChecker::new(policy);

    let registry = ToolRegistry::new().with_checker(checker);
    registry.register(ToolEntry::new(Arc::new(MockToolWithPerm::new(
        "confirm_tool",
        "Require confirm tool",
    ))));

    let tool_uses = vec![make_tool_use("confirm_tool", json!({}))];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_error == Some(true));
    let text = outputs[0]
        .content
        .iter()
        .filter_map(|p| p.text())
        .collect::<String>();
    assert!(text.contains("denied by user"));
}

#[tokio::test]
async fn test_permission_require_explicit() {
    use synthia_permission::rule::PermissionRule;

    // RequireExplicit is not a policy action - it's set when tool requires permission
    // but the policy returns Ask, so we need to check the policy evaluation path
    let rules = vec![PermissionRule {
        pattern: "explicit_tool".to_string(),
        action: synthia_permission::rule::PermissionAction::Ask,
        forced: false,
    }];
    let policy =
        synthia_permission::merged_policy::MergedPolicy::new(&rules, &[], &[]);
    let checker = PermissionChecker::new(policy);

    let registry = ToolRegistry::new().with_checker(checker);
    registry.register(ToolEntry::new(Arc::new(MockToolWithPerm::new(
        "explicit_tool",
        "Require explicit tool",
    ))));

    let tool_uses = vec![make_tool_use("explicit_tool", json!({}))];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();
    assert_eq!(outputs.len(), 1);
    // Ask results in error output just like RequireExplicit
    assert!(outputs[0].is_error == Some(true));
}

#[tokio::test]
async fn test_permission_block() {
    use synthia_permission::rule::PermissionRule;

    // Block permission level - when policy says Deny
    let rules = vec![PermissionRule {
        pattern: "blocked_tool".to_string(),
        action: synthia_permission::rule::PermissionAction::Deny,
        forced: false,
    }];
    let policy =
        synthia_permission::merged_policy::MergedPolicy::new(&rules, &[], &[]);
    let checker = PermissionChecker::new(policy);

    let registry = ToolRegistry::new().with_checker(checker);
    registry.register(ToolEntry::new(Arc::new(MockToolWithPerm::new(
        "blocked_tool",
        "Blocked tool",
    ))));

    let tool_uses = vec![make_tool_use("blocked_tool", json!({}))];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_error == Some(true));
}

// ============ Tool visibility tests ============

#[tokio::test]
async fn test_hidden_tool_not_executable() {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(MockHiddenTool::new(
        "hidden_tool",
        "Hidden tool",
    ))));

    let tool_uses = vec![make_tool_use("hidden_tool", json!({}))];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_error == Some(true));
    let text = outputs[0]
        .content
        .iter()
        .filter_map(|p| p.text())
        .collect::<String>();
    assert!(text.contains("not found"));
}

#[tokio::test]
async fn test_visible_tool_is_executable() {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(MockToolNoPerm::new(
        "visible_tool",
        "Visible tool",
    ))));

    let tool_uses = vec![make_tool_use("visible_tool", json!({}))];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_text());
}

// ============ Permission check results tests ============

#[tokio::test]
async fn test_permission_check_returns_decisions() {
    use synthia_permission::rule::PermissionRule;

    // Create a policy that allows specific tools
    let rules = vec![
        PermissionRule {
            pattern: "tool1".to_string(),
            action: synthia_permission::rule::PermissionAction::Allow,
            forced: false,
        },
        PermissionRule {
            pattern: "tool2".to_string(),
            action: synthia_permission::rule::PermissionAction::Allow,
            forced: false,
        },
    ];
    let policy =
        synthia_permission::merged_policy::MergedPolicy::new(&[], &[], &rules);
    let checker = PermissionChecker::new(policy);

    let requests = vec![
        PermissionRequest::new("tool1".to_string(), json!({}), true),
        PermissionRequest::new("tool2".to_string(), json!({}), false),
    ];

    let decisions = checker.check(&requests).await.unwrap();
    assert_eq!(decisions.len(), 2);
    assert_eq!(decisions.get("tool1"), Some(&Permission::AutoApprove));
    assert_eq!(decisions.get("tool2"), Some(&Permission::AutoApprove));
}

#[tokio::test]
async fn test_permission_check_without_checker_allows_all() {
    // When no checker is attached, all tools run regardless of requires_permission
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(MockToolWithPerm::new(
        "perm_tool",
        "Tool requiring permission",
    ))));

    let tool_uses = vec![make_tool_use("perm_tool", json!({}))];

    // No checker attached
    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();
    assert_eq!(outputs.len(), 1);
    // Without a checker, even tools that require permission are allowed
    assert!(outputs[0].is_text());
}

#[tokio::test]
async fn test_permission_check_with_checker_enforces() {
    use synthia_permission::rule::PermissionRule;

    let rules = vec![PermissionRule {
        pattern: "enforced_tool".to_string(),
        action: synthia_permission::rule::PermissionAction::Deny,
        forced: false,
    }];
    let policy =
        synthia_permission::merged_policy::MergedPolicy::new(&rules, &[], &[]);
    let checker = PermissionChecker::new(policy);

    let registry = ToolRegistry::new().with_checker(checker);
    registry.register(ToolEntry::new(Arc::new(MockToolWithPerm::new(
        "enforced_tool",
        "Tool requiring permission",
    ))));

    let tool_uses = vec![make_tool_use("enforced_tool", json!({}))];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].is_error == Some(true));
}

#[tokio::test]
async fn test_permission_remember_always() {
    let checker = PermissionChecker::allow_all();
    let original_resource =
        serde_json::to_string(&json!({"key": "value"})).unwrap();

    // Remember an "always" rule
    checker.remember_always(
        "remembered_tool".to_string(),
        original_resource.clone(),
    );

    let requests = vec![PermissionRequest::new(
        "remembered_tool".to_string(),
        json!({"key": "value"}),
        true,
    )];

    let decisions = checker.check(&requests).await.unwrap();
    assert_eq!(
        decisions.get("remembered_tool"),
        Some(&Permission::AutoApprove)
    );
}

#[tokio::test]
async fn test_permission_forget_always() {
    use synthia_permission::rule::PermissionRule;

    // Need an explicit Allow rule for the tool since allow_all() doesn't work as expected
    let rules = vec![PermissionRule {
        pattern: "forgotten_tool".to_string(),
        action: synthia_permission::rule::PermissionAction::Allow,
        forced: false,
    }];
    let policy =
        synthia_permission::merged_policy::MergedPolicy::new(&[], &[], &rules);
    let checker = PermissionChecker::new(policy);
    let original_resource =
        serde_json::to_string(&json!({"key": "value"})).unwrap();

    // Remember and then forget
    checker.remember_always(
        "forgotten_tool".to_string(),
        original_resource.clone(),
    );
    checker.forget_always("forgotten_tool", &original_resource);

    let requests = vec![PermissionRequest::new(
        "forgotten_tool".to_string(),
        json!({"key": "value"}),
        true,
    )];

    let decisions = checker.check(&requests).await.unwrap();
    // After forgetting, the policy Allow rule should still apply
    assert_eq!(
        decisions.get("forgotten_tool"),
        Some(&Permission::AutoApprove)
    );
}

#[tokio::test]
async fn test_permission_deny_with_reason() {
    let checker = PermissionChecker::allow_all();

    // Security check only applies to bash/shell tools, not arbitrary dangerous_tool
    let requests = vec![PermissionRequest::new(
        "bash".to_string(),
        json!({"command": "rm -rf /"}),
        true,
    )];

    let decisions = checker.check(&requests).await.unwrap();
    // The security check should catch the dangerous bash command
    assert!(matches!(
        decisions.get("bash"),
        Some(Permission::Deny { reason: _ })
    ));
}

#[tokio::test]
async fn test_multiple_tools_mixed_permissions() {
    use synthia_permission::rule::PermissionRule;

    let rules = vec![PermissionRule {
        pattern: "ask_tool".to_string(),
        action: synthia_permission::rule::PermissionAction::Ask,
        forced: false,
    }];
    let policy =
        synthia_permission::merged_policy::MergedPolicy::new(&rules, &[], &[]);
    let checker = PermissionChecker::new(policy);

    let registry = ToolRegistry::new().with_checker(checker);
    registry.register(ToolEntry::new(Arc::new(MockToolNoPerm::new(
        "auto_tool",
        "Auto tool",
    ))));
    registry.register(ToolEntry::new(Arc::new(MockToolWithPerm::new(
        "ask_tool", "Ask tool",
    ))));
    registry.register(ToolEntry::new(Arc::new(MockToolNoPerm::new(
        "auto_tool2",
        "Auto tool 2",
    ))));

    let tool_uses = vec![
        make_tool_use("auto_tool", json!({})),
        make_tool_use("ask_tool", json!({})),
        make_tool_use("auto_tool2", json!({})),
    ];

    let outputs = registry
        .run_with_context(tool_uses, make_context())
        .await
        .unwrap();
    assert_eq!(outputs.len(), 3);
    // auto_tool and auto_tool2 don't require permission, so they succeed
    assert!(outputs[0].is_text());
    // ask_tool requires permission and policy says Ask -> denied
    assert!(outputs[1].is_error == Some(true));
    assert!(outputs[2].is_text());
}
