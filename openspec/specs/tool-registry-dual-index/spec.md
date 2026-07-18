# tool-registry-dual-index Specification

## Purpose
TBD - created by archiving change agent-toolification-v3. Update Purpose after archive.
## Requirements
### Requirement: Dual-Index ToolRegistry

The `ToolRegistry` type MUST maintain two indices: a `HashMap<String, Arc<dyn ToolDefinition + ToolExecution + ToolLifecycle>>` for O(1) name lookup, and a `Vec<ToolMetadata>` snapshot for stable order iteration in LLM-visible views. Insertions and removals MUST keep both indices consistent via a single internal API.

#### Scenario: Insert Synchronizes Both Indices

- **WHEN** `ToolRegistry::insert(name, tool)` is called
- **THEN** the HashMap entry SHALL be added AND a corresponding `ToolMetadata` entry SHALL be appended to the Vec in insertion order

#### Scenario: Remove Cleans Both Indices

- **WHEN** `ToolRegistry::remove(name)` is called with a registered name
- **THEN** the HashMap entry SHALL be removed AND the corresponding Vec entry SHALL be removed, preserving order of remaining entries

#### Scenario: LLM View Uses Vec Order

- **WHEN** the agent renders the LLM-visible tool list
- **THEN** it MUST iterate over `Vec<ToolMetadata>` in order, not over the HashMap

#### Scenario: Lookup Performance

- **WHEN** `ToolRegistry::get(name)` is called with a registered name
- **THEN** the lookup MUST complete in O(1) average time

### Requirement: ToolMetadata Type

The `ToolMetadata` type MUST be a value type containing at minimum: `name: String`, `description: String`, `category: ToolCategory`, `parameters_schema: serde_json::Value`, and `version: semver::Version`. `ToolMetadata` MUST be `Clone + Send + Sync + 'static`.

#### Scenario: Clone for Snapshot

- **WHEN** `ToolRegistry::snapshot()` is called
- **THEN** it SHALL return `Vec<ToolMetadata>` with each entry cloned, decoupled from the underlying Arc

