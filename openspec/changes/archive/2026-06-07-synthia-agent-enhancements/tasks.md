## 1. P0: File-Based Agent Definition (Phase 1)

- [x] 1.1 Create `agent_file` module structure in `crates/synthia-agent/src/agent_file/`
- [x] 1.2 Implement `FileAgentFrontmatter` YAML parsing with `serde_yaml`
- [x] 1.3 Implement `ID_PATTERN` validation `[a-z0-9][a-z0-9_-]{0,63}`
- [x] 1.4 Implement `ParsedAgentFile` with frontmatter/body split
- [x] 1.5 Implement `merge_permission_rules()` with child priority rule-level merge
- [x] 1.6 Implement `extends` resolution with chain depth ≤ 4 and cycle detection
- [x] 1.7 Implement `AgentFileLoader` with directory scanning
- [x] 1.8 Add `notify` watcher with500ms debounce for hot reload
- [x] 1.9 Implement content_hash (SHA-256) caching to skip unchanged files
- [x] 1.10 Implement `AgentChangeEvent` emission on add/remove/modify
- [x] 1.11 Add validation failure handling (retain old definition + warn)
- [x] 1.12 Extend `AgentDefinition` with new fields (permission_rules, permission_default, tools, denied_tools, extends, mode)
- [x] 1.13 Write unit tests: frontmatter parsing, extends merge, ID validation (≥24 tests)
- [x] 1.14 Write integration tests: hot reload, concurrent file changes (≥6 tests)

## 2. P0: Multi-Layer Permission Merge (Phase 2)

- [x] 2.1 Add `PermissionRule` struct with `pattern`, `action`, `forced` fields in `synthia-permission`
- [x] 2.2 Add `RuleLayer` enum `{ Default=0, Agent=1, User=2 }` with layer priority
- [x] 2.3 Implement `MergedPolicy::new()` with three-layer merge algorithm
- [x] 2.4 Implement multi-segment colon glob pattern matcher
- [x] 2.5 Add `forced: true` Short-circuit logic in evaluation
- [x] 2.6 Implement specificity ordering within same layer
- [x] 2.7 Add `AskNotifier` trait with `on_ask_triggered` / `on_ask_resolved`
- [x] 2.8 Implement `NoopAskNotifier` for CLI single-agent mode
- [x] 2.9 Implement `AgentControlAskNotifier` bridging to mailbox Suspended
- [x] 2.10 Add `From<PermissionPolicy> for RuleSet` backward compatibility adapter
- [x] 2.11 Write unit tests: rule merge, pattern matching, forced Short-circuit
- [x] 2.12 Write integration tests: Ask flow with Guardian, timeout handling

## 3. P0: Permission-Config Bridge

- [x] 3.1 Add `permission_rules: Vec<PermissionRule>` and `permission_default: Option<PermissionAction>` to `AgentDefinition`
- [x] 3.2 Implement `MergedPolicy` builder from `AgentDefinition` fields
- [x] 3.3 Add `allowed_tools` (ToolRegistry pre-filter) and `denied_tools` (MergedPolicy forced Deny) integration
- [x] 3.4 Write unit tests: permission build from file-based Agent

## 4. P1: Multi-Agent Control Plane (Phase 3, Part 1)

- [x] 4.1 Implement `AgentPath` with validation `[a-z0-9][a-z0-9_-]{0,63}`, `/root` prefix
- [x] 4.2 Implement `AgentRegistry` with HashMap<AgentPath, AgentMetadata> + atomic thread count
- [x] 4.3 Implement nickname pool with auto-reset when exhausted
- [x] 4.4 Implement `SpawnReservation` RAII two-phase commit
- [x] 4.5 Implement `AgentControl` as cloneable handle (Arc<Registry> + Weak<...>)
- [x] 4.6 Implement `AgentControl::spawn_agent()` with metadata
- [x] 4.7 Implement `AgentControl::send_message()` routing via path
- [x] 4.8 Implement `AgentControl::list_agents()` with prefix filtering
- [x] 4.9 Implement `AgentControl::shutdown_agent_tree()` with BFS walk
- [x] 4.10 Implement `Mailbox` with MPSC + watch sequence counter
- [x] 4.11 Implement `MailboxDeliveryPhase` state machine (CurrentTurn/NextTurn/Suspended)
- [x] 4.12 Implement `CompletionWatcher` detached tokio::spawn monitoring
- [x] 4.13 Write unit tests: AgentPath validation, registry operations, mailbox phase transitions

## 5. P1: StreamBuilder Integration (Phase 3, Part 2)

- [x] 5.1 Add optional `agent_control: Arc<AgentControl>` to `AgentRunConfig`
- [x] 5.2 Add optional `fork_policy: ForkPolicy` to `AgentRunConfig`
- [x] 5.3 Implement `StepSpawn` in `stream_builder/steps/spawn.rs`
- [x] 5.4 Add 4 new `AgentEvent` variants: SubagentSpawnBegin/End, SubagentMessage, SubagentComplete
- [x] 5.5 Implement `AgentTool` routing to `AgentControl::spawn_agent()`
- [x] 5.6 Implement Ask-Suspended coordination (mailbox phase transition)
- [x] 5.7 Write unit tests: StepSpawn, event emission
- [x] 5.8 Write e2e test: multi-agent协作 with mailbox communication

## 6. P1: ForkPolicy Implementation

- [x] 6.1 Implement `ForkPolicy` with 6 variants: InheritAll, LastNTurns, SinceStep, ByTag, Empty, SystemOnly
- [x] 6.2 Implement `ForkPermissionPolicy` with 4 variants: InheritAll, InheritAsUser, InheritAsAgent, Empty
- [x] 6.3 Implement `keep_forked_rollout_item()` filtering logic
- [x] 6.4 Implement default combination `SystemOnly + InheritAsUser`
- [x] 6.5 Implement definition_drift telemetry on sub-agent completion
- [x] 6.6 Write unit tests: fork policy combinations, filtering logic

## 7. Testing & Integration

- [x] 7.1 Run `cargo test -p synthia-agent --lib` after each phase
- [x] 7.2 Write e2e tests for file-based Agent loading +热重载
- [x] 7.3 Write e2e tests for permission merge三层合并
- [x] 7.4 Write e2e tests for multi-agent spawn + mailbox communication
- [x] 7.5 Verify backward compatibility: existing TOML configs still work
- [x] 7.6 Verify `cargo clippy` passes with no warnings