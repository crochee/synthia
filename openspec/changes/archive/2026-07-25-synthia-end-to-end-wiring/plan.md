# Implementation Plan: synthia-end-to-end-wiring

## Execution Order

Tasks are ordered by dependency — each step assumes prior steps are complete.

### Batch 1: Server-side wiring (tasks 1-9)

All tasks modify `synthia-server/src/state/` files. Execute sequentially since they share `AppState`.

1. **Task 1-8**: Modify `app_state.rs` — add FragmentRegistry, InterceptorChain, SkillRegistry, PluginRegistry, ExtensionRegistry, RolloutTracker construction + fields
2. **Task 9**: Write AppState construction unit test

**Verification**: `cargo check -p synthia-server`

### Batch 2: AgentFactory wiring (tasks 10-15)

3. **Task 10-14**: Modify `agent_factory.rs` — add extension_registry/rollout_tracker fields, update create() and from_state()
4. **Task 15**: Write AgentFactory verification test

**Verification**: `cargo check -p synthia-server`

### Batch 3: SessionController wiring (tasks 16-20)

5. **Task 16-19**: Modify `controller.rs` — add fields to RunDependencies, update build_run_config()
6. **Task 20**: Write Controller verification test

**Verification**: `cargo check -p synthia-server`

### Batch 4: Resume/Subagent wiring (tasks 21-23)

7. **Task 21-22**: Modify `resume.rs` and `subagent/config.rs` — propagate extension_registry/rollout_tracker
8. **Task 23**: Write verification test

**Verification**: `cargo check -p synthia-agent`

### Batch 5: Main loop FragmentRegistry (tasks 24-27)

9. **Task 24-26**: Modify `main_loop.rs` — add FragmentRegistry path for system prompt
10. **Task 27**: Write FragmentRegistry activation test

**Verification**: `cargo check -p synthia-agent && cargo test -p synthia-agent -- fragment`

### Batch 6: Main loop InterceptorChain (tasks 28-32)

11. **Task 28-31**: Modify `main_loop.rs` — add InterceptorChain dispatch around tool execution
12. **Task 32**: Write InterceptorChain dispatch test

**Verification**: `cargo check -p synthia-agent && cargo test -p synthia-agent -- interceptor`

### Batch 7: Main loop RolloutTracker (tasks 33-35)

13. **Task 33-34**: Modify `main_loop.rs` — add rollout_tracker.record_token_usage() and record_change() calls
14. **Task 35**: Write RolloutTracker call test

**Verification**: `cargo check -p synthia-agent && cargo test -p synthia-agent -- rollout`

### Batch 8: Crate consolidation (tasks 36-55)

Execute in order: session-v2 → event-v2 → message-proxy → evaluation docs.

15. **Task 36-40**: Merge session-v2 into session
16. **Task 41-45**: Merge event-v2 into core
17. **Task 46-49**: Merge message-proxy into server
18. **Task 50-55**: Evaluation docs for extension-v2 and service

**Verification after each merge**: `cargo check --workspace`

### Batch 9: Fix + verify (tasks 56-66)

19. **Task 56-57**: Fix l1_truncate test
20. **Task 58-61**: E2E integration tests
21. **Task 62-63**: Update registry-first tasks.md
22. **Task 64-66**: Final quality gate (fmt + clippy + test)

## Risk Mitigation

- **Each batch is independently verifiable** — if a batch breaks, revert only that batch
- **Phase 1 (batches 1-7) is the critical path** — do this first, verify end-to-end, then proceed to consolidation
- **Crate consolidation (batch 8) is risky** — do one crate at a time with full workspace check between each
- **Compat strategy**: always check `extension_registry.is_some()` before using new path; fall back to old path if None
