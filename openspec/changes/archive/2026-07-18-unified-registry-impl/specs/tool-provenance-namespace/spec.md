## ADDED Requirements

### Requirement: ToolProvenance Enum
`ToolProvenance` SHALL be an enum with variants: `Core`, `Plugin { id: PluginId }`, `Mcp { server: String, host_owned: bool }`, `Context { source: ContextSource }`, `Dynamic`. It SHALL derive `Clone, Debug, PartialEq, Eq, Hash` but NOT `Copy` (owned String fields).

#### Scenario: Provenance comparison
- **WHEN** two tools from different providers have the same name
- **THEN** their `ToolProvenance` SHALL distinguish them for registry duplicate detection

---

### Requirement: Core Tool Name Immutability
The `ToolRegistry` SHALL refuse re-registration of any tool name that already carries `ToolProvenance::Core`. A second `Core` registration for the same name SHALL return `RegistrationError::CoreNameTaken`.

#### Scenario: Core tool shadowing prevented
- **WHEN** a plugin attempts to register a tool named "read" with `ToolProvenance::Core`
- **THEN** the registry SHALL reject the registration

#### Scenario: Plugin tool with core name allowed via namespace
- **WHEN** a plugin registers a tool named "read" with `ToolProvenance::Plugin { id: "my-plugin" }`
- **THEN** the LLM SHALL see it as `plugin:my-plugin:read`, avoiding shadowing

---

### Requirement: Plugin Tool Namespace Format
Plugin tools with `prompt_visible_provenance: true` SHALL be surfaced to the LLM as `plugin:<plugin_id>:<raw_name>`. Plugin tools with `prompt_visible_provenance: false` SHALL use the bare name (private plugins).

#### Scenario: Public plugin tool namespaced
- **WHEN** a plugin "data-tools" registers tool "query" with `prompt_visible_provenance: true`
- **THEN** the LLM SHALL see the tool as `plugin:data-tools:query`

#### Scenario: Private plugin tool bare name
- **WHEN** a plugin "internal" registers tool "audit" with `prompt_visible_provenance: false`
- **THEN** the LLM SHALL see the tool as `audit`
