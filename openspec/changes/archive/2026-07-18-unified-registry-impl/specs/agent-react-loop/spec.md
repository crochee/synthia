## MODIFIED Requirements

### Requirement: AgentRunConfig Service-Based Injection
`AgentRunConfig` SHALL replace its 11+ individual service fields with `services: Arc<ServiceRegistry>` and `loop_services: OnceLock<LoopServices>`. The old fields (`subagent_session_factory`, `sandbox_manager`, `extension_manager`, `approval_service`, `guardian_coordinator`, `model_router`, `fork_policy`, `compaction_provider`, `steering_channel`, `context_assembler`, `tool_orchestrator`) SHALL be marked `#[deprecated]` and remain available for 1 release cycle. The new `tools` field SHALL use `Materialization` instead of raw tool list.

#### Scenario: New config uses ServiceRegistry
- **WHEN** `AgentRunConfig` is constructed with the `unified-registry` feature
- **THEN** `services` and `loop_services` SHALL be the primary fields; deprecated fields SHALL emit warnings

#### Scenario: Loop resolves services via LoopServices
- **WHEN** `main_loop.rs` needs a service (e.g., SessionService)
- **THEN** it SHALL access it via `services.session` from the cached `LoopServices`

---

### Requirement: Main Loop Service Resolution
`main_loop.rs` SHALL resolve all 11 previously-discarded services through `LoopServices` cached fields. Each resolution SHALL use the typed `services.<field>` accessor. The loop SHALL validate all required services at `run_stream` entry via `LoopServices::bootstrap`.

#### Scenario: Discarded field restored as service
- **WHEN** the loop needs the steering channel (previously `_steering_channel`)
- **THEN** it SHALL use `services.steering.drain()` via the cached `LoopServices`

#### Scenario: Missing required service at run entry
- **WHEN** `run_stream` is called but a required service is missing from the registry
- **THEN** the loop SHALL return `AgentError::RequiredServiceMissing` before any LLM call
