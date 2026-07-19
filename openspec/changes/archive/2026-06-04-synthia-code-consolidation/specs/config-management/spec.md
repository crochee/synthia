## ADDED Requirements

### Requirement: AgentConfig Layer Conversion
The system SHALL provide `From`/`Into` implementations for converting between CLI Config, Server Config, and Agent Runtime Config.

#### Scenario: CLI config converts to Server config
- **WHEN** CLI Config (AgentConfigYaml) is converted to Server Config
- **THEN** it SHALL use the defined `From`/`Into` implementations

#### Scenario: Server config converts to Runtime config
- **WHEN** Server Config is converted to Agent Runtime Config
- **THEN** it SHALL use the defined `From`/`Into` implementations

#### Scenario: Conversion preserves configuration intent
- **WHEN** configuration flows through the layers
- **THEN** all configuration values SHALL be preserved (no data loss)