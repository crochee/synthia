## ADDED Requirements

### Requirement: v3 tool architecture absorbed by synthia-session-v2 + ProviderRegistry v2

The tool-first architecture originally scoped in this change (`synthia-tool-core` crate, `AgentTool`/`ExtensionTool` dual shape, `ToolRouter`, `ToolSearch`, 9-abstractions toolification, 7 Tool-scope extension points) SHALL be considered **absorbed** by the actual v3 rollout shipped in commits `3e5940c..6288a5b`, without requiring a standalone `synthia-tool-core` crate.

This requirement is **VERIFIED** as of 2026-07-14: the planning skeleton (`plan.md`, `tasks.md`, `proposal.md`, `design.md`) was authored 2026-07-12 but never committed, and the substantive work landed in `synthia-session-v2` (Change 3 of v3 architecture) plus `ProviderRegistry v2` instead. See the Archive Note at the top of [tasks.md](../tasks.md) for the commit-by-commit mapping.

#### Scenario: planning skeleton never committed
- **WHEN** inspecting `git log -- openspec/changes/synthia-tool-refactor/` and the parent history
- **THEN** no commit SHALL introduce `crates/synthia-tool-core/` or modify this change folder — confirmed; only the planning markdown files exist on disk and have never been committed

#### Scenario: v3 commits absorb the in-flight work (VERIFIED)
- **WHEN** running `git log --oneline 3e5940c..6288a5b`
- **THEN** the command SHALL return 9 commits covering `synthia-session-v2` (R2-R6) + `ProviderRegistry v2` (R7) + `9-abstractions toolification verification` (R8) — confirmed; marked VERIFIED

#### Scenario: 9-abstractions integration test passes (VERIFIED)
- **WHEN** running `cargo test -p synthia-agent --test 9_abstractions`
- **THEN** the 5 new tests introduced by commit `7393a7a` SHALL pass: `spec_names_list_has_nine_entries`, `spec_names_are_all_distinct`, `query_skill_usage_tool_impl_exists`, `compact_context_tool_impl_exists`, `empty_registry_reports_empty` — confirmed; marked VERIFIED

---

### Requirement: ProviderRegistry v2 with source_id hot-swap is the canonical registry

The canonical runtime tool/provider registry in v3 SHALL be `ProviderRegistry v2` as shipped in commit `6f48d76`, with `source_id`-aware `register` / `unregister` / `replace_source` semantics. The originally planned `ToolRegistry { tools, providers, cache_version }` and `AtomicU64` cache-version CAS pattern SHALL be considered deferred.

This requirement is **VERIFIED** as of 2026-07-14.

#### Scenario: source_id isolation
- **WHEN** two providers register under the same `name` but different `source_id`
- **THEN** `ProviderRegistry::get(name)` SHALL return both, distinguishable by `source_id` — covered by the source_id isolation test in `crates/synthia-provider/`

#### Scenario: atomic hot-swap (VERIFIED)
- **WHEN** calling `ProviderRegistry::replace_source(source_id, new_set)` while readers iterate the registry
- **THEN** the swap SHALL be atomic with respect to readers (no torn reads) — covered by the atomic hot-swap test in `crates/synthia-provider/`; marked VERIFIED

#### Scenario: re-register under same source REJECTS
- **WHEN** a caller attempts `register(name, provider, source_id)` for a `(name, source_id)` pair that is already registered
- **THEN** the call SHALL return an error rather than silently replacing — covered by commit `6f48d76`'s test suite; marked VERIFIED

---

### Requirement: 9-abstractions toolification verified on the build path

Per the `9-abstractions-toolification/spec.md` spec, all 9 non-Tool abstractions SHALL be reachable through the standard `ToolRegistry` registration path. The build-path proof is shipped by commit `7393a7a`. The originally planned `ExtensionTool`-only wrapping SHALL be considered deferred — the current path uses the existing `Tool` trait rather than the proposed `AgentTool`/`ExtensionTool` split.

This requirement is **VERIFIED** as of 2026-07-14.

#### Scenario: 9-abstractions integration test passes (VERIFIED)
- **WHEN** running `cargo test -p synthia-agent --test 9_abstractions -- --nocapture`
- **THEN** all 5 tests SHALL pass, proving each of the 9 abstraction names is on the build path — confirmed; marked VERIFIED

#### Scenario: full workspace test suite green
- **WHEN** running `cargo test --workspace`
- **THEN** 729 `synthia-agent` + 198 `synthia-session` + 148 `synthia-server` + 124 `synthia-cli` tests SHALL pass without modification — confirmed via commits `7393a7a` and `6288a5b`

---

### Requirement: 7 Tool-scope extension points remain DECLARED, not VERIFIED

The 7 Tool-scope extension points originally scoped in this change (`tool.registry.register`, `tool.registry.unregister`, `tool.definition.transform`, `tool.execution_mode.override`, `tool.parallelism.barrier`, `tool.output.format`, `tool.output.metadata.inject`) SHALL remain in the `extension-point-matrix` spec as **DECLARED**, not VERIFIED. Wiring is deferred to a future Change (originally scoped as Change 2 / Change 3 follow-up).

This requirement is **OPEN** as of 2026-07-14. Partial coverage exists via `extension-points-phase-2/` but the 7 Tool-scope events from `proposal.md` §C1.5 were not wired as a unit.

#### Scenario: extension-point-matrix status for Tool-scope points
- **WHEN** reading `openspec/specs/extension-point-matrix/spec.md`
- **THEN** the 7 Tool-scope points SHALL be marked DECLARED (not VERIFIED) — pending confirmation by spec owner

#### Scenario: 64-point partial matrix integration test deferred
- **WHEN** checking for `crates/synthia-agent/tests/extension_matrix_r1_to_r7.rs`
- **THEN** the file SHALL NOT exist on disk — confirmed; deferred