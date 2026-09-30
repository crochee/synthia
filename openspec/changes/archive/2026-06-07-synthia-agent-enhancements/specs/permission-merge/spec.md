## ADDED Requirements

### Requirement: PermissionRule Data Structure
The system SHALL define `PermissionRule { pattern: String, action: PermissionAction, forced: bool }`. The `pattern` field SHALL use multi-segment colon glob syntax (`bash:rm*`, `fs:write:/etc/**`). The `action` field SHALL be one of `Allow`, `Deny`, `Ask`. The `forced` field, when true, SHALL bypass specificity ordering and Short-circuit the evaluation.

### Requirement: Three-Layer Merge
The system SHALL implement three-layer permission merge with priority order `User > Agent > Default`. Within the same layer, rules with higher specificity SHALL take precedence. When specificity is equal, the priority order SHALL be `Deny > Ask > Allow`. Each layer MAY contain multiple rules.

### Requirement: Multi-Segment Colon Glob Pattern Matching
The system SHALL match permission patterns using multi-segment colon syntax where segments are separated by `:`. Wildcard `*` SHALL match any sequence of characters within a segment. Wildcard `**` SHALL match any sequence of characters across segments. Pattern matching SHALL be case-sensitive.

### Requirement: Ask Flow with Guardian Integration
When a rule evaluates to `Ask`, the system SHALL translate this to `ApprovalRequest` and delegate to `GuardianDecision::NeedUserConfirm` with `ToolAction::PendingConfirm`. The system SHALL wait for user confirmation via the existing Hook system. On timeout, the system SHALL treat as implicit denial.

### Requirement: MergedPolicy Evaluation
The system SHALL implement `MergedPolicy::evaluate(tool_id: &str, pattern: &str) -> PermissionResult`. The evaluation SHALL first check `forced: true` rules (Short-circuit on match). Then apply specificity ordering. Finally apply layer priority. The result SHALL be `Allowed`, `Denied`, or `Ask`.

### Requirement: Backward Compatibility
The system SHALL maintain backward compatibility with existing `PermissionPolicy` API. Existing TOML configuration files SHALL continue to work. The system SHALL provide `From<PermissionPolicy> for RuleSet` adapter.

---

## MODIFIED Requirements

None — this is a new capability.

---

## REMOVED Requirements

None — this is a new capability.

---

## RENAMED Requirements

None.