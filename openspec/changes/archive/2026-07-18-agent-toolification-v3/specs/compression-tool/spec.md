## ADDED Requirements

### Requirement: CompressionTool Trait

The `synthia-agent` crate MUST export a `CompressionTool` trait with method `fn compress(&self, history: &[AgentMessage]) -> Result<CompressionResult>`. The trait MUST be object-safe. A `DefaultCompactionTool` implementation MUST be provided that preserves the existing `compress` behavior from `react_loop`.

#### Scenario: Default Implementation Preserves Current Behavior

- **WHEN** `DefaultCompactionTool::compress()` is called on a history of 100 messages
- **THEN** the output MUST be byte-equivalent to the legacy inline `compress()` function for the same input

#### Scenario: Custom CompressionTool Injection

- **WHEN** a user provides a custom `CompressionTool` implementation to the agent
- **THEN** the agent MUST use the custom implementation and MUST NOT call `DefaultCompactionTool::compress()`

### Requirement: Compression Injection Point

The `Agent` type MUST expose a constructor variant or builder method that accepts `Arc<dyn CompressionTool>`. If no compression tool is provided, the agent MUST default to `DefaultCompactionTool`.

#### Scenario: Default Injection

- **WHEN** `Agent::builder().build()` is called without specifying compression
- **THEN** the agent SHALL internally hold `Arc::new(DefaultCompactionTool::new())`

#### Scenario: Custom Injection

- **WHEN** `Agent::builder().compression(Arc::new(MyCompactionTool)).build()` is called
- **THEN** the agent SHALL hold the user's `Arc<dyn CompressionTool>` and call it instead of the default

### Requirement: Backward Compatibility

The existing inline `compress()` function or method in `react_loop` MUST remain callable when no `CompressionTool` is injected, so existing users see no behavioral change.

#### Scenario: Existing Tests Pass

- **WHEN** the existing test suite for `react_loop` is executed
- **THEN** all tests that rely on default compression MUST pass without modification