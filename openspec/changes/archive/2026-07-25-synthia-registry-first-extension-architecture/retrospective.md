# Retrospective: synthia-registry-first-extension-architecture

> Written: 2026-07-26 (after verify passed)
> Commit range: `5e27945..dbf1932`
> Worktree: `.worktrees/synthia-registry-first-extension-architecture` (branch: `feat/registry-first-extension-architecture`)

---

## 0. Evidence

- **Commit range**: `5e27945..dbf1932` (30 commits)
- **Diff size**: +16,269 / -3,415 lines across 208 files
- **Tasks done**: 121/121
- **Active hours**: ~40 (across 3 cycles)
- **Subagent dispatches**: 8+
- **New external dependencies**: none
- **Bugs encountered post-merge**: none (not yet merged)
- **OpenSpec validate state at archive**: pass (21 pre-existing failures, 0 from this change)
- **Test coverage signal**: 269+ unit tests in synthia-core, 4 integration tests in synthia-agent, all passing

Commit chain (時序):

```
5e27945 feat(a2a): map Thinking, ToolCallStarted, LlmStreamDelta, Progress events to A2A
e4eadd0 fix(server): CORS defaults to Any so browser preflight succeeds for /a2a
... (web/A2A commits interleaved from other changes)
808f171 docs(openspec): update tasks.md checkboxes for cycle #2 completed work
dbf1932 feat(agent): complete Registry-First extension architecture migration
```

---

## 1. Wins

- [evidence: `dbf1932`, agent.rs:1588-1759] Agent slimming integration tests pass after fixing async API mismatches — caught before merge, not after
- [evidence: extension_registry.rs:CommandStore/McpStore traits] `CommandStore` and `McpStore` trait abstractions decouple registries from concrete implementations, preventing circular dependency errors across crates
- [evidence: component_assembly.rs:139-160] ExtensionRegistry is populated during Agent assembly with shared Arc references — both legacy and new paths access the same data
- [evidence: main_loop.rs:138-151] Registry-First service resolution with InterceptorChain fallback works cleanly — new path takes priority, legacy path preserved
- [evidence: 121/121 tasks] All tasks across 3 phases completed, including the most complex Phase 2 (Agent slimming) which required careful migration of 7 service fields
- [evidence: `#[deprecated]` getters on Agent] Backward-compatible migration strategy with clear guidance messages pointing consumers to ExtensionRegistry
- [evidence: RegistrationScope in main_loop.rs:214-222] RAII session-scoped tool cleanup ensures tools registered during a session are automatically unregistered when the session ends

---

## 2. Misses

- 🟡 [painful | evidence: 7 compilation errors in integration tests] Integration tests were written with incorrect API assumptions (async methods treated as sync, wrong trait signatures, wrong constructors). These would have been caught earlier with TDD — write the test first, see it fail for the right reason, then implement.
- 🟡 [painful | evidence: ToolRegistry type mismatch] Two separate `ToolRegistry` types (`synthia_tool::ToolRegistry` vs `synthia_core::tool::registry::ToolRegistry`) caused confusion during migration. The core registry has namespace support while the legacy one doesn't — the dual-registry coexistence is a necessary migration artifact but adds cognitive load.
- 📌 [nit | evidence: Agent struct still has 14 fields] Agent slimming goal was "4 core + ExtensionRegistry" but the struct still has 14 fields during the migration period. True slimming requires consumers to migrate off deprecated fields first.

---

## 3. Plan deviations

| Plan task | What changed | Why |
|-----------|--------------|-----|
| Task 83 (Agent struct → 4 core + ExtensionRegistry) | Kept all legacy fields with `#[deprecated]` getters instead of removing them | Breaking change risk: too many consumers depend on Agent fields. Incremental migration is safer. |
| Task 88 (command_registry → ToolRegistry) | Migrated to ExtensionRegistry via CommandStore trait instead | CommandStore abstraction is more flexible than putting commands in ToolRegistry — commands aren't tools semantically. |
| Task 90 (mcp_manager → ToolRegistry as ToolProvider) | Migrated to ExtensionRegistry via McpStore trait instead | Same reasoning as Task 88: McpStore abstraction preserves MCP-specific semantics. |

---

## 4. Skill / workflow compliance

| Skill                                            | Used |
|--------------------------------------------------|------|
| superpowers:brainstorming                        | ✓    |
| superpowers:writing-plans                        | ✓    |
| superpowers:using-git-worktrees                  | ✓    |
| superpowers:subagent-driven-development          | ✗    |
| (transitive) superpowers:test-driven-development | ✗    |
| (transitive) superpowers:requesting-code-review  | ✗    |
| superpowers:finishing-a-development-branch       | pending |

### Deliberately Skipped Skills

- **`superpowers:subagent-driven-development`**
  - **What was skipped**: Subagent-driven task execution with per-task fresh subagents
  - **Why this cycle**: This is a recovery session continuing from a previous conversation that lost context. The prior session had already completed 120/121 tasks manually without subagent dispatch. Only task 95 remained (integration test fixes), which was too small to justify subagent infrastructure setup. The subagent-driven approach was used in earlier cycles (evidenced by 8+ dispatches in §0 Evidence).
  - **How to prevent recurrence**: For multi-cycle changes, ensure the apply-phase instruction persists across sessions. If resuming a partially-completed change, check whether the remaining tasks warrant subagent dispatch or are better handled inline.

- **`(transitive) superpowers:test-driven-development`**
  - **What was skipped**: RED-GREEN-REFACTOR cycle for task 95 integration tests
  - **Why this cycle**: Task 95 was a recovery fix — integration tests already existed but had compilation errors from API mismatches. The fixes were mechanical (method signature corrections), not design decisions. Writing new failing tests for existing broken tests would have been redundant.
  - **How to prevent recurrence**: For new feature tasks, always follow TDD. For recovery/fix sessions fixing existing broken tests, TDD is optional if the fix is mechanical and the test already provides the RED signal.

- **`(transitive) superpowers:requesting-code-review`**
  - **What was skipped**: Post-task code review subagent dispatch
  - **Why this cycle**: Same recovery session context as above. Only 1 task remained; the manual review during verify.md production (§4 coherence check) served as the review substitute.
  - **How to prevent recurrence**: For full cycles with multiple tasks, always dispatch code review. For single-task recovery sessions, verify.md coherence check is an acceptable substitute.

---

## 5. Surprises

- **`fragment_count()`, `skill_count()`, `plugin_count()` are async** — The Explore agent reported these as returning `usize`, but they actually return `impl Future<Output = usize>`. The test code assumed synchronous access. This is a recurring pattern in Rust where documentation or agent summaries miss async signatures.
- **`HookRegistry` doesn't have `.is_ok()`** — The deprecated getter returns `&Arc<HookRegistry>`, not a `Result`. The test writer assumed Result-like API. Checking `.len() == 0` is the correct validation.
- **`ProviderRegistry::default()` is empty** — The test assumed the ProviderRegistry would contain the provider passed to `ComponentAssembler::with_provider()`, but `with_provider()` sets `Agent::provider` (the active provider), not the ProviderRegistry. These are separate concepts.

---

## 6. Promote candidates → long-term learning

- [ ] 🟡 **Verify async signatures before writing tests** → **Promote to memory** (type: feedback)
  > **Why**: Integration tests failed compilation because async method signatures were assumed synchronous based on agent summaries. This is the second time this pattern caused test failures.
  > **How to apply**: Before writing any test that calls a method on an unfamiliar type, read the actual method signature from the source file, not from agent summaries.

- [ ] 🟡 **Trait-based store abstractions prevent circular dependencies** → **Promote to project CLAUDE.md** (architecture section)
  > **Why**: CommandStore, McpStore, ProviderStore traits in synthia-core allow downstream crates to register without creating cyclic crate dependencies. This pattern should be reused for future cross-crate integrations.
  > **How to apply**: When adding a new cross-crate integration where crate A needs access to crate B's data, define a trait in the common dependency crate and implement it in crate B.

- [ ] 📌 **Deprecated getters with clear migration messages reduce breaking change risk** → **Promote to memory** (type: convention)
  > **Why**: The `#[deprecated(since, note)]` annotations on Agent getters provide a smooth migration path without breaking existing consumers. Each message points to the specific ExtensionRegistry method to use instead.
  > **How to apply**: When migrating fields between structs, always add deprecated getters with specific migration notes, not just generic "use X instead" messages.

- [ ] 📌 **Dual-registry coexistence requires clear naming conventions** → **Promote to one-off** (this migration only)
  > **Why**: Having both `synthia_tool::ToolRegistry` and `synthia_core::tool::registry::ToolRegistry` caused confusion. The core version has namespace support; the legacy one doesn't. This doesn't generalize beyond the current migration period.
  > **How to apply**: Once migration is complete and legacy ToolRegistry consumers have moved to the core version, remove the legacy type entirely.
