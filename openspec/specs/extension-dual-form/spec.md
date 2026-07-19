# extension-dual-form Specification

## Purpose
TBD - created by archiving change tool-abstraction-and-extensibility. Update Purpose after archive.
## Requirements
### Requirement: Extension system SHALL support dual forms: Tool and ExtensionTool

The synthia extension system SHALL support two complementary tool forms: a lean `Tool` trait (core agent capability) and a richer `ExtensionTool` trait (with extension context). The two forms SHALL be interconvertible via decorators.

#### Scenario: Tool trait (core, no extension context)
- **WHEN** a tool is defined via `impl Tool`
- **THEN** it SHALL NOT have access to `ExtensionContext`
- **AND** the tool SHALL be self-contained (no plugin/extension dependencies)
- **AND** the tool SHALL be constructable in any context (loading, runtime, tests)

#### Scenario: ExtensionTool trait (with extension context)
- **WHEN** a tool is defined via `impl ExtensionTool`
- **THEN** its `execute` method SHALL receive `ExtensionContext` as the last argument
- **AND** the extension MAY use the context to send messages, append entries, register providers, or trigger UI dialogs
- **AND** the extension MAY also have access to `ui_render` for host-specific rendering hints

#### Scenario: ToolAdapter converts ExtensionTool to Tool
- **WHEN** `ToolAdapter::new(inner: Arc<dyn ExtensionTool>, ctx_factory: Arc<dyn Fn() -> ExtensionContext>)` is called
- **THEN** it SHALL return `Arc<dyn Tool>`
- **AND** when the resulting Tool is called, the `ctx_factory` SHALL be invoked to construct a fresh `ExtensionContext`
- **AND** the `ExtensionTool::execute(input, ctx)` SHALL be called with the constructed context

#### Scenario: Lazy context construction
- **WHEN** a ToolAdapter is used in a session
- **THEN** the `ExtensionContext` SHALL be constructed at the time of `call()`, NOT at registration time
- **AND** this ensures the context reflects the current session state, not stale state from registration

### Requirement: Conversion SHALL be lossless in both directions

`Tool` to `ExtensionTool` and `ExtensionTool` to `Tool` conversions SHALL preserve all callable behavior (modulo context access).

#### Scenario: Tool -> ExtensionTool synthesis
- **WHEN** `create_extension_tool_from_agent(tool: Arc<dyn Tool>) -> Arc<dyn ExtensionTool>` is called
- **THEN** the returned ExtensionTool SHALL delegate `execute` to `tool.call()`
- **AND** `ui_render` SHALL return `None` (no extension-specific rendering)
- **AND** the conversion SHALL be O(1) (no allocation beyond the wrapper)

#### Scenario: ExtensionTool -> Tool via ToolAdapter
- **WHEN** an `ExtensionTool` is registered in the `ToolRegistry` via `ToolAdapter`
- **THEN** the registry SHALL see it as a regular `Tool`
- **AND** all orchestrator features (permission check, doom loop detection, execution mode routing) SHALL apply to the wrapped tool

### Requirement: ToolAdapter SHALL propagate ToolContext

The `ToolAdapter` SHALL propagate `ToolContext` (containing `cancel_token`, `directory`, `worktree`) to the wrapped `ExtensionTool` via the `ExtensionContext`.

#### Scenario: ToolContext fields in ExtensionContext
- **WHEN** `ToolAdapter::call(input, ctx)` is invoked
- **THEN** the `ExtensionContext` constructed by `ctx_factory` SHALL contain:
  - `cancel_token: ctx.cancel_token.clone()`
  - `directory: ctx.directory.to_path_buf()`
  - `worktree: ctx.worktree.to_path_buf()`
- **AND** the wrapped `ExtensionTool::execute` SHALL be able to access these fields via the context

### Requirement: ExtensionContext SHALL have three states: Loading, Active, Stale

The `ExtensionContext` SHALL be an enum with three variants: `Loading`, `Active`, `Stale`. State transitions SHALL be controlled by the orchestrator, not the extension.

#### Scenario: Initial state is Loading
- **WHEN** an extension is loaded but `bind_core()` has not been called
- **THEN** the `ExtensionContext` SHALL be `Loading`
- **AND** only `register_*` methods SHALL be callable
- **AND** calling other methods SHALL panic with `NotInitializedError`

#### Scenario: bind_core transitions to Active
- **WHEN** `ExtensionRuntime::bind_core()` is called
- **THEN** all queued `register_*` calls during `Loading` SHALL be processed
- **AND** the `ExtensionContext` SHALL transition to `Active`
- **AND** all action methods SHALL become available

#### Scenario: Session replacement transitions to Stale
- **WHEN** a new session is created and replaces the old one
- **THEN** all `ExtensionContext` instances associated with the old session SHALL transition to `Stale { reason: "session_replaced" }`
- **AND** any subsequent call on a stale context SHALL return `Err(StaleContextError)`

#### Scenario: assert_active fails on non-Active state
- **WHEN** `ctx.assert_active()` is called on a `Loading` or `Stale` context
- **THEN** it SHALL return `Err(StaleContextError)` (or panic on Loading, per fail-fast design)

