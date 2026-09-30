## ADDED Requirements

### Requirement: Extension system SHALL provide 4-scope × 64-extension-point matrix

The synthia extension system SHALL provide a typed registry of extension points organized into 10 scopes (Agent Loop, LLM, Tool, Context, Permission, Provider, Plugin Lifecycle, Event Bus, Session Tree, Output/UI) with a total of 64 typed extension points.

#### Scenario: Scope enumeration
- **WHEN** the extension system initializes
- **THEN** it SHALL expose 10 scopes as enum variants
- **AND** each scope SHALL group related extension points with typed inputs and outputs

#### Scenario: Typed contracts (no serde_json::Value for inputs)
- **WHEN** an extension point is defined
- **THEN** its input and output types SHALL be concrete Rust structs (not `serde_json::Value`)
- **AND** the types SHALL be `#[derive(Serialize, Deserialize, JsonSchema)]` for schema generation
- **AND** the orchestrator SHALL validate extension payloads against the schema at registration time

### Requirement: Agent Loop scope SHALL expose 12 extension points

The Agent Loop scope SHALL expose: `agent_start`, `agent_end`, `turn_start`, `turn_end`, `iteration_start`, `iteration_end`, `error { severity, source, recoverable }`, `compact_start { reason: Manual|Threshold|Overflow }`, `compact_end`, `branch_navigate { from_id, to_id }`, `session_start`, `session_end`.

#### Scenario: compact_start typed input
- **WHEN** the orchestrator fires `compact_start`
- **THEN** the input SHALL be `CompactStartInput { reason: CompactionReason, current_tokens: u64, threshold: u64 }`
- **AND** the extension SHALL NOT receive the actual messages (P3 lazy load)

#### Scenario: error severity typed
- **WHEN** the orchestrator fires `error`
- **THEN** `severity` SHALL be one of `ErrorSeverity::{Warning, Recoverable, Fatal}`
- **AND** the extension MAY return `ErrorAction::{Continue, Abort, Retry { backoff_ms }}`

### Requirement: LLM scope SHALL expose 8 extension points

The LLM scope SHALL expose: `system_prompt.transform`, `messages.transform`, `chat.params { temperature, top_p, top_k, max_tokens }`, `chat.headers.inject`, `tool_choice.override`, `model.select`, `cache.breakpoint.set`, `response.transform`.

#### Scenario: chat.params allows modification
- **WHEN** `chat.params` is fired before an LLM call
- **THEN** the extension SHALL receive `ChatParams { temperature, top_p, top_k, max_tokens }` by mutable reference
- **AND** any field the extension modifies SHALL be reflected in the actual LLM request
- **AND** P1 prefix consistency SHALL be preserved (modifications after the prefix hash is computed invalidate the cache — extensions modifying chat.params MUST be deterministic)

#### Scenario: cache.breakpoint placement
- **WHEN** `cache.breakpoint.set` is fired
- **THEN** the extension SHALL return a list of `CacheBreakpoint { scope: CacheScope, ttl: CacheTtl }` values
- **AND** the orchestrator SHALL honor the breakpoints in the actual LLM request

### Requirement: Tool scope SHALL expose 9 extension points

The Tool scope SHALL expose: `tool.execute.before`, `tool.execute.after`, `tool.definition.transform`, `tool.registry.register`, `tool.registry.unregister`, `tool.execution_mode.override`, `tool.parallelism.barrier`, `tool.output.format`, `tool.output.metadata.inject`.

#### Scenario: tool.execute.before can modify args
- **WHEN** `tool.execute.before` is fired before a tool call
- **THEN** the extension SHALL receive `ToolExecuteBeforeInput { name, args, ctx }` by mutable reference
- **AND** the extension SHALL return `ToolAction::{Proceed, Skip { reason }, Modify { new_args }, PendingConfirm { blocking } }`
- **AND** `Modify` SHALL replace the args before passing to the tool

#### Scenario: tool.execute.after can format output
- **WHEN** `tool.execute.after` is fired after a tool call
- **THEN** the extension SHALL receive `ToolExecuteAfterInput { name, output }` by mutable reference
- **AND** the extension MAY modify `output.content`, `output.metadata`, or `output.truncated_by`
- **AND** the modified output SHALL be the one stored in the session and sent to LLM

#### Scenario: tool.definition.transform affects LLM-visible description
- **WHEN** `tool.definition.transform` is fired during tool registration
- **THEN** the extension SHALL receive `ToolDefinition { name, description, parameters }` by mutable reference
- **AND** modifications SHALL affect the LLM's view of the tool
- **AND** P1 prefix consistency: tool definitions are part of the system prompt hash, so transformations MUST be deterministic across calls

### Requirement: Context / Compaction scope SHALL expose 7 extension points

The Context scope SHALL expose: `context.compact.trigger`, `context.compact.summarize`, `context.compact.replace`, `context.prefix.participate`, `context.observability.emit`, `context.token_budget.adjust`, `context.message_filter`.

#### Scenario: context.compact.summarize allows custom summary
- **WHEN** the orchestrator decides to compact
- **THEN** `context.compact.summarize` SHALL be fired with `SummarizeInput { head: String, previous_summary: Option<String>, max_tokens: u32 }`
- **AND** the extension SHALL return `Option<String>` (None = use default truncation)
- **AND** if the extension returns a summary, the orchestrator SHALL use it instead of calling the LLM for summarization

#### Scenario: context.prefix.participate for P1 hash extension
- **WHEN** the `PrefixTracker::compute_hash_bytes` is called
- **THEN** all registered extensions on `context.prefix.participate` SHALL be invoked
- **AND** each extension SHALL return a `Vec<u8>` to include in the hash
- **AND** this enables extensions (e.g., skill snapshots) to participate in the P1 cache key

### Requirement: Permission scope SHALL expose 5 extension points

The Permission scope SHALL expose: `permission.ask`, `permission.notify`, `doom_loop.detected`, `blacklist.match`, `permission.persist`.

#### Scenario: permission.ask is the gate (fail-closed)
- **WHEN** a tool call requires permission
- **THEN** `permission.ask` SHALL be fired with `PermissionRequest { tool, args, ctx }`
- **AND** the extension SHALL return `PermissionDecision::{Allow, Deny { reason }, Ask { message } }`
- **AND** per project hard constraint: the default SHALL be `Ask` (fail-closed), not `Allow` (fail-open)

#### Scenario: doom_loop.detected triggers extension notification
- **WHEN** the DoomLoopDetector detects 3 identical `(tool_name, args_hash)` calls
- **THEN** `doom_loop.detected` SHALL be fired with `DoomLoopInput { signature, count }`
- **AND** the extension MAY return `DoomLoopAction::{RequirePermission, Abort, Ignore}`

### Requirement: Provider scope SHALL expose 4 extension points

The Provider scope SHALL expose: `provider.register (lazy)`, `provider.unregister`, `provider.auth (oauth|apikey)`, `provider.fallback`.

#### Scenario: Lazy provider registration
- **WHEN** `provider.register` is fired
- **THEN** the extension SHALL provide a `LazyProvider { name, load_fn: Box<dyn Fn() -> Future<...>> }`
- **AND** `load_fn` SHALL only be called when the provider is first used
- **AND** the result SHALL be cached for subsequent calls (P3 lazy load)

### Requirement: Plugin Lifecycle scope SHALL expose 6 extension points

The Plugin Lifecycle scope SHALL expose: `extension.load (pending)`, `extension.bind (flush)`, `extension.invalidate (mark_stale)`, `extension.unload (cleanup)`, `extension.hot_swap (reload)`, `extension.dual_form (agent|extension)`.

#### Scenario: Loading state cannot call action methods
- **WHEN** an extension is in `ExtensionContext::Loading` state
- **THEN** the extension SHALL only have access to `register_*` methods
- **AND** calling `send_message`, `append_entry`, or `ui_dialog` SHALL panic with `NotInitializedError` (fail-fast)

#### Scenario: bind_core flushes pending registrations
- **WHEN** `bind_core()` is called on the ExtensionRuntime
- **THEN** all queued `register_*` calls during `Loading` SHALL be processed in order
- **AND** after flush, `register_*` methods SHALL become direct (not queued) for O(1) subsequent calls

#### Scenario: Stale state on session replacement
- **WHEN** a new session is created and the old ExtensionContext is replaced
- **THEN** the old context SHALL transition to `ExtensionContext::Stale { reason: "session_replaced" }`
- **AND** any subsequent call on the stale context SHALL return `Err(StaleContextError)`

### Requirement: Event Bus scope SHALL expose 4 extension points

The Event Bus scope SHALL expose: `event.subscribe`, `event.publish`, `event.aggregate`, `event.replay`.

#### Scenario: Typed event bus
- **WHEN** an extension subscribes to a topic
- **THEN** the topic SHALL be a typed enum variant (not a string)
- **AND** the payload SHALL be a typed struct (not `serde_json::Value`)
- **AND** the bus SHALL use sequence numbers to maintain ordering

#### Scenario: Event replay from sequence
- **WHEN** an extension calls `event.replay(from_sequence: u64)`
- **THEN** the bus SHALL re-emit all events from the given sequence to the extension
- **AND** this enables late-joining extensions to reconstruct session state

### Requirement: Session Tree scope SHALL expose 5 extension points

The Session Tree scope SHALL expose: `session.entry.append`, `session.entry.tree_walk`, `session.branch.create`, `session.version.migrate`, `session.compaction.preserve`.

#### Scenario: session.branch.create forks session
- **WHEN** an extension calls `session.branch.create(parent_id: SessionId)`
- **THEN** a new branch SHALL be created with the given parent
- **AND** subsequent entries SHALL be appended to the new branch
- **AND** the original branch SHALL remain immutable

#### Scenario: session.compaction.preserve for extension-generated summaries
- **WHEN** a compaction is triggered
- **THEN** the orchestrator SHALL preserve `from_hook=true` CompactionEntry details (per pi-mono `session-manager.ts:48-61` semantics)
- **AND** subsequent re-compactions SHALL preserve core-generated details, discarding extension-generated details

### Requirement: Output/UI scope SHALL expose 4 extension points

The Output/UI scope SHALL expose: `output.format`, `output.metadata.inject`, `ui.dialog.select|confirm|input|notify`, `ui.render.component`.

#### Scenario: ui.dialog.notify
- **WHEN** an extension calls `ui.dialog.notify(message, NotificationLevel)`
- **THEN** a notification SHALL appear in the host (TUI / RPC / Server)
- **AND** the notification SHALL NOT block the agent loop
- **AND** the orchestrator SHALL be able to map `NotificationLevel::{Info, Warning, Error}` to host-specific UI

---

### Requirement: Every extension point SHALL be observable via OTel and event log

Every fired extension point SHALL produce an OTel span and a P9 event for observability.

#### Scenario: OTel span attributes
- **WHEN** an extension point is fired
- **THEN** an OTel span SHALL be created with attributes:
  - `extension.point: <point_name>`
  - `extension.scope: <scope_name>`
  - `extension.id: <extension_id>`
  - `extension.duration_us: <measured>`
  - `extension.result: "ok" | "error" | "skipped"`

#### Scenario: P9 event log
- **WHEN** an extension point is fired
- **THEN** a JSONL event SHALL be appended to the session log with the same fields
- **AND** the event SHALL be queryable via `event.replay(from_sequence)`
