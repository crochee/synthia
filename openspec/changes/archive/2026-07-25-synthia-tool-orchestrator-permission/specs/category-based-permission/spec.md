# Spec: category-based-permission

## ADDED Requirements

### Requirement: ToolCategory-based security_check

`PermissionChecker::security_check()` SHALL first check the tool's `ToolCategory`. If the category is available, category-specific rules SHALL be applied. If the category is `None`, the existing name-based fallback SHALL be used.

#### Scenario: Shell category tool

WHEN `security_check()` is called for a tool with `ToolCategory::Shell`
THEN the checker SHALL apply shell-specific security rules (path traversal, dangerous command detection)
AND the tool name SHALL NOT be matched against the hardcoded `"bash"|"shell"` pattern list

#### Scenario: Filesystem category tool

WHEN `security_check()` is called for a tool with `ToolCategory::Filesystem`
THEN the checker SHALL apply filesystem-specific security rules (workspace path containment)
AND the tool name SHALL NOT be matched against the hardcoded `"read_file"|"write_file"` pattern list

#### Scenario: No category fallback to name matching

WHEN `security_check()` is called for a tool with `ToolCategory::None` or no category
THEN the checker SHALL fall back to the existing tool-name string matching

### Requirement: PermissionRule category pattern syntax

`PermissionRule.pattern` SHALL support `category:<CategoryName>` prefix syntax (e.g., `category:Shell`, `category:Filesystem`).

#### Scenario: Category pattern matches Shell tools

WHEN a `PermissionRule` has `pattern: "category:Shell"` and `decision: Deny`
AND `evaluate()` is called for a tool with `ToolCategory::Shell`
THEN the rule SHALL match and the decision SHALL be `Deny`

#### Scenario: Category pattern does not match other tools

WHEN a `PermissionRule` has `pattern: "category:Shell"` and `evaluate()` is called for a tool with `ToolCategory::Filesystem`
THEN the rule SHALL NOT match

#### Scenario: Mixed category and name patterns

WHEN rules contain both `category:Shell` patterns and `"bash"` name patterns
THEN category patterns SHALL be evaluated first; name patterns SHALL be evaluated as fallback

### Requirement: ToolPermission sub-trait deprecation

`ToolPermission` sub-trait SHALL be marked with `#[deprecated(note = "Use PermissionChecker via synthia-permission instead. Will be removed after 6-month deprecation window.")]`.

#### Scenario: Deprecated ToolPermission still compiles

WHEN code implements `ToolPermission` for a tool
THEN the code SHALL compile with a deprecation warning
AND the `check()` method SHALL still function

#### Scenario: Migration to PermissionChecker

WHEN a tool previously implemented `ToolPermission::check()` returning `PermissionDecision::Allow`
THEN the equivalent `PermissionRule` SHALL be `{ pattern: "<tool_name>", decision: AutoApprove }`
