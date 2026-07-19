# hook-modify-tool-input Specification

## Purpose
TBD - created by archiving change fix-agent-critical-bugs-and-production-gaps. Update Purpose after archive.
## Requirements
### Requirement: Hook SHALL receive modified tool call input when Modify action is returned

When a `before_tool` hook returns `ToolAction::Modify(new_input)`, the agent SHALL execute the tool with the modified input parameters rather than the original input. The modified tool call SHALL preserve the original tool name unless the hook also specifies a new name.

#### Scenario: Modify input is actually used
- **WHEN** a hook returns `ToolAction::Modify` with modified input
- **THEN** the tool SHALL execute with the modified input, not the original

### Requirement: Hook SHALL support Modify action for both name and input fields

The `ToolAction::Modify` variant SHALL support modification of:
- The tool's input parameters (via `input` field)
- The tool's name (via `name` field), enabling tool substitution

#### Scenario: Modify input parameters
- **WHEN** a hook returns `ToolAction::Modify` with a JSON value containing only an `input` field different from the original
- **THEN** the tool SHALL execute with the modified input and the original tool name

#### Scenario: Modify both name and input
- **WHEN** a hook returns `ToolAction::Modify` with a JSON value containing both `name` and `input` fields
- **THEN** the tool SHALL execute with the specified new name and modified input

#### Scenario: Modify with Skip fallback
- **WHEN** a hook returns `ToolAction::Modify` but the resulting call would be invalid
- **THEN** the agent SHALL log a warning and fall back to the original tool call behavior

---

