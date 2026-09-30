## ADDED Requirements

### Requirement: Markdown File Agent Loading
The system SHALL support loading Agent definitions from Markdown files in `.agents/agents/<id>.md` format. Each file MUST contain YAML frontmatter with fields: `model`, `permission_rules`, `permission_default`, `tools`, `denied_tools`, `extends`, `mode`, `hidden`, `color`, `steps`, `options`. The markdown body SHALL be used as the `system_prompt`.

### Requirement: YAML Frontmatter Parsing
The system SHALL parse YAML frontmatter from Markdown files using `serde_yaml`. The frontmatter parser MUST support `Option<PermissionAction>` for `permission_default` field, where YAML value "inherit" SHALL deserialize to None. The parser MUST reject YAML with `!!` tags.

### Requirement: Agent ID Validation
The system SHALL validate Agent IDs against pattern `[a-z0-9][a-z0-9_-]{0,63}`. IDs MUST start with an alphanumeric character, MUST NOT start with `-` or `_`, and MUST NOT exceed 63 characters.

### Requirement: extends Inheritance
When an Agent definition contains an `extends` field referencing a parent Agent ID, the system SHALL merge parent and child permission_rules using rule-level merge with child priority. Child rules with the same pattern SHALL override parent rules. Unmodified parent rules SHALL be preserved. The extends chain depth MUST NOT exceed 4 levels.

### Requirement: Hot Reload with Debounce
The system SHALL watch `.agents/` directory for changes using `notify` watcher. The system SHALL apply a500ms debounce to coalesce multiple rapid changes. On file modification, the system SHALL compute SHA-256 content hash and skip reload if hash is unchanged.

### Requirement: Change Event Notification
On Agent definition changes (add/remove/modify), the system SHALL emit `AgentChangeEvent` to subscribed listeners. Failed validation of a modified file SHALL retain the old definition and log a warning, without affecting other files.

---

## MODIFIED Requirements

None — this is a new capability.

---

## REMOVED Requirements

None — this is a new capability.

---

## RENAMED Requirements

None.