## MODIFIED Requirements

### Requirement: Tool Trait Minimal Interface
The `Tool` trait SHALL define exactly 3 methods: `name(&self) -> &str`, `execute(&self, input: ToolInput, ctx: &ToolContext) -> Result<ToolOutput, ToolError>`, and `descriptor(&self) -> &ToolDescriptor`. The trait SHALL be annotated with `#[async_trait]` and require `Send + Sync` bounds. The legacy 11-method `Tool` trait SHALL be marked `#[deprecated]` and remain available for 1 release cycle under `#[cfg(not(feature = "unified-registry"))]`.

#### Scenario: New Tool trait compiles
- **WHEN** a tool implements the new 3-method `Tool` trait
- **THEN** it SHALL compile without implementing any other trait

#### Scenario: Legacy trait deprecation warning
- **WHEN** code references the legacy 11-method `Tool` trait
- **THEN** the compiler SHALL emit a deprecation warning

#### Scenario: Feature flag gates trait selection
- **WHEN** `unified-registry` feature is enabled
- **THEN** the new `Tool` trait SHALL be the active trait; the legacy trait SHALL be behind `#[cfg(not(feature = "unified-registry"))]`
