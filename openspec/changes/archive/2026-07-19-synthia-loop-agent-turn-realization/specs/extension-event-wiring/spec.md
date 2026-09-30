# Spec: extension-event-wiring

## ADDED Requirements

### Requirement: All 19 Extension events triggered from main_loop

The main_loop SHALL trigger all 19 Extension events at the appropriate lifecycle points.

#### Scenario: Session lifecycle events

WHEN a session starts
THEN `on_session_start(SessionStartPayload)` SHALL be triggered via `UnifiedHookDispatcher`

WHEN a session ends
THEN `on_session_end(SessionEndPayload)` SHALL be triggered via `UnifiedHookDispatcher`

#### Scenario: LLM lifecycle events

WHEN an LLM request is about to begin
THEN `on_user_prompt_submit(UserPromptSubmitPayload)` SHALL be triggered

WHEN an LLM response is about to be sent
THEN `on_pre_response(PreResponsePayload)` SHALL be triggered

WHEN an LLM response has been received
THEN `on_post_response(PostResponsePayload)` SHALL be triggered

#### Scenario: Tool lifecycle events

WHEN a tool is about to be executed
THEN `on_pre_tool_use(PreToolUsePayload)` SHALL be triggered

WHEN a tool execution has completed
THEN `on_post_tool_use(PostToolUsePayload)` SHALL be triggered

#### Scenario: Compact lifecycle events

WHEN a context compaction is about to begin
THEN `on_pre_compact(PreCompactPayload)` SHALL be triggered

WHEN a context compaction has completed
THEN `on_post_compact(PostCompactPayload)` SHALL be triggered

#### Scenario: Steering lifecycle events

WHEN steering messages are drained from the steering channel
THEN `on_pre_steering(PreSteeringPayload)` SHALL be triggered before processing
AND `on_post_steering(PostSteeringPayload)` SHALL be triggered after processing

#### Scenario: Subagent lifecycle events

WHEN a sub-agent is about to be spawned
THEN `on_pre_subagent_spawn(PreSubagentSpawnPayload)` SHALL be triggered

WHEN a sub-agent has completed
THEN `on_post_subagent_spawn(PostSubagentSpawnPayload)` SHALL be triggered

#### Scenario: Message drop event

WHEN a message is about to be dropped (context compaction pruning)
THEN `on_pre_message_drop(PreMessageDropPayload)` SHALL be triggered

#### Scenario: Definition drift events

WHEN a file definition changes during a session
THEN `on_pre_definition_drift(PreDefinitionDriftPayload)` SHALL be triggered before re-indexing
AND `on_post_definition_drift(PostDefinitionDriftPayload)` SHALL be triggered after re-indexing

#### Scenario: MCP routing events

WHEN an MCP route is about to be resolved
THEN `on_pre_mcp_route(PreMCPRoutePayload)` SHALL be triggered

WHEN an MCP route has been resolved
THEN `on_post_mcp_route(PostMCPRoutePayload)` SHALL be triggered

#### Scenario: OAuth flow event

WHEN an OAuth flow is about to begin
THEN `on_pre_oauth_flow(PreOAuthFlowPayload)` SHALL be triggered

### Requirement: ExtensionRegistry double-registration fix

`ExtensionRegistry::register()` SHALL also register the extension with `ServiceRegistry::register_with_capability::<Extension>()`.

#### Scenario: Register extension with both registries

WHEN `ExtensionRegistry::register(manifest, extension)` is called
THEN the extension SHALL be recorded in the `ExtensionRegistry` internal DashMap
AND the extension SHALL be registered with `ServiceRegistry::register_with_capability::<Extension>(service_id, arc_extension)`
AND if the `ServiceRegistry` registration fails, the `ExtensionRegistry` registration SHALL also be rolled back

#### Scenario: Double registration rejected

WHEN `ExtensionRegistry::register()` is called with an extension ID that already exists
THEN the method SHALL return an error indicating duplicate registration
AND no registration with `ServiceRegistry` SHALL be attempted
