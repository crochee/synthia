# adopt-explore-agent-recommendations Implementation Plan

> **For agentic workers:** Use `superpowers:subagent-driven-development`
> to implement this plan task-by-task.
>
> Each ## section below maps 1:1 to a task group in `tasks.md`. Within each group, micro-steps are sized for 2-5 minute TDD cycles. Commit points are listed explicitly.

**Goal:** Close 6 ranked recommendations identified by explore-agent scan — complete Phase 2 of `add-dynamic-tool-provider-system`, archive `production-grade-agent-architecture`, archive `add-dynamic-tool-provider-system` after R1, fill `## Purpose` on 5 high-impact specs, verify & archive `architecture-audit`, and deliver a research note on 4 P2 capability gaps.

**Architecture:** This is a bundled superpowers-bridge change. Code changes are localized to `synthia-agent`, `synthia-mcp`, `synthia-tool-bash`, `synthia-cli`, `synthia-server` (Phase 2 of `add-dynamic-tool-provider-system`). Spec/paperwork changes cover 14 spec folders under `openspec/specs/` and 2 archival promotions.

**Tech Stack:** Rust 2021, `cargo +nightly fmt`, `cargo clippy --workspace -- -D warnings`, OpenSpec CLI v1.3.x.

---

## Task 1: BashToolsProvider (commits: §1.1–1.5 in tasks.md)

- [ ] **Step 1.1** Read `crates/synthia-tool-bash/src/lib.rs` and `crates/synthia-agent/src/tools/providers/file_tools_provider.rs`. Confirm the `ToolProvider` trait surface: `list_tools()`, `before_tool_execute`, `after_tool_execute`, `on_event`.
- [ ] **Step 1.2** TDD red: write `crates/synthia-agent/src/tools/providers/bash_tools_provider.rs` test asserting `BashToolsProvider::new().list_tools()` returns the expected bash-tool names (`bash`, `shell_exec`, etc. — confirm via existing tools list).
- [ ] **Step 1.3** TDD green: implement `BashToolsProvider` wrapping the existing `Bash`/`Shell` tools; ensure each `Tool::call_with_sandbox` carries `&CancellationToken` from the provider.
- [ ] **Step 1.4** TDD red+green: add test that `before_tool_execute` is called with the bash tool name and a stub `input`.
- [ ] **Step 1.5** Add `BashToolsProvider` to `ExtensionManager::default_providers()` factory in `crates/synthia-agent/src/tools/extension_manager.rs`.
- [ ] **Step 1.6** Run: `cargo test -p synthia-agent --lib tools::providers::bash_tools_provider`; expect pass.
- [ ] **Step 1.7** Commit: `feat(agent): implement BashToolsProvider (Phase 2 of dynamic-tool-provider)`.

---

## Task 2: MCPToolsProvider (commits: §2.1–2.5)

- [ ] **Step 2.1** Read `crates/synthia-mcp/src/lib.rs` and identify `Client::list_tools()` API; confirm it returns `Result<Vec<ToolDefinition>>`.
- [ ] **Step 2.2** TDD red: write `crates/synthia-agent/src/tools/providers/mcp_tools_provider.rs` test using `test-support::MockMcpServer` returning 2 sample tools.
- [ ] **Step 2.3** TDD green: implement `MCPToolsProvider::new(client)` that calls `client.list_tools()` lazily on first `list_tools()` invocation, caches the result, and wraps each `ToolDefinition` as `Arc<dyn Tool>` via `McpToolAdapter`.
- [ ] **Step 2.4** Add cancellation-token propagation through `McpToolAdapter::call_with_sandbox`.
- [ ] **Step 2.5** Add to `default_providers()` factory.
- [ ] **Step 2.6** Integration test: spin up test MCP server, register provider, invoke a discovered tool, assert response.
- [ ] **Step 2.7** Commit: `feat(agent): implement MCPToolsProvider`.

---

## Task 3: SearchToolsProvider (commits: §3.1–3.4)

- [ ] **Step 3.1** Read `crates/synthia-tool-file/` and any other crate exposing file/grep/glob tools; produce a list of tool name → struct references.
- [ ] **Step 3.2** TDD red: write `crates/synthia-agent/src/tools/providers/search_tools_provider.rs` test asserting `list_tools()` returns every search-adjacent tool name.
- [ ] **Step 3.3** TDD green: implement `SearchToolsProvider` wrapping the identified tools; add to factory.
- [ ] **Step 3.4** Run `cargo test -p synthia-agent --lib tools::providers::search_tools_provider`; commit.

---

## Task 4: Deprecate `register_defaults` and migrate call sites (§4.1–4.7)

- [ ] **Step 4.1** Run `grep -rn 'register_defaults\|build_default_tool_registry' crates/ --include='*.rs'`; record all callers in a comment block.
- [ ] **Step 4.2** Annotate `crates/synthia-tool/src/registry.rs`:
  ```rust
  #[deprecated(
      since = "0.X.Y",
      note = "Use ToolProvider + ExtensionManager; see synthia-agent::tools::ExtensionManager::default_providers"
  )]
  pub fn register_defaults(&mut self) { ... }
  ```
- [ ] **Step 4.3** In `crates/synthia-cli/src/repl_core/repl/agent_message.rs:62`, replace any `register_defaults()` call with `let ext_mgr = ExtensionManager::default_providers();` and pass `&ext_mgr` to `AgentRunConfig`.
- [ ] **Step 4.4** Same edit in `crates/synthia-server/src/state/app_state.rs:108`.
- [ ] **Step 4.5** Same edit in `crates/synthia-agent/src/subagent/config.rs:102`.
- [ ] **Step 4.6** Run `cargo clippy --workspace -- -D warnings`; fix any deprecation warnings on internal code (add `#[allow(deprecated)]` only where a single-call dependency cannot yet be migrated, with a TODO).
- [ ] **Step 4.7** Run `cargo test --workspace`; commit: `refactor: migrate CLI/server/subagent to ExtensionManager; deprecate register_defaults`.

---

## Task 5: Promote `production-grade-agent-architecture` specs to canonical (§5.1–5.7)

- [ ] **Step 5.1** For each of `tool-cancellation-propagation`, `async-permission-deferred`, `scoped-tool-registry`, `doom-loop-proactive-detection`, `smart-compaction-agent`:
  - `mkdir -p openspec/specs/<name>`
  - `cp openspec/changes/production-grade-agent-architecture/specs/<name>/spec.md openspec/specs/<name>/spec.md`
- [ ] **Step 5.2** In each canonical spec, insert above `## Requirements`:
  ```
  ## Purpose
  <1-2 sentence summary — draft each per the spec's intent; see R3 §6.2–6.5 style>
  ```
- [ ] **Step 5.3** Edit `openspec/changes/production-grade-agent-architecture/tasks.md`: replace every `- [ ]` with `- [x]` (sed in-place).
- [ ] **Step 5.4** Run `openspec validate --all`; expect zero new errors.
- [ ] **Step 5.5** Run `openspec archive production-grade-agent-architecture`; confirm archive folder created.
- [ ] **Step 5.6** Commit: `docs(openspec): close out production-grade-agent-architecture change`.

---

## Task 6: Fill `## Purpose` on 5 high-impact specs (§6.1–6.7)

- [ ] **Step 6.1** For each of `architecture-audit`, `agent-bus`, `context-compaction`, `agent-react-loop`, `convergent-prompt-assembly`, read existing `spec.md` and craft a 1-2 sentence `## Purpose`.
- [ ] **Step 6.2** Insert `## Purpose` section above `## Requirements` in each.
- [ ] **Step 6.3** Run `openspec validate --all`; confirm TBD warnings on those 5 are gone.
- [ ] **Step 6.4** Commit: `docs(openspec): fill Purpose on 5 high-impact specs`.

---

## Task 7: Verify & archive `architecture-audit` (§7.1–7.5)

- [ ] **Step 7.1** `grep -rn synthia-multiagent crates/`; assert zero matches; record in `verify.md`.
- [ ] **Step 7.2** `cargo build -p synthia-permission`; assert no `PermissionPolicy` legacy type; record in `verify.md`.
- [ ] **Step 7.3** Update `openspec/changes/adopt-explore-agent-recommendations/specs/architecture-audit/spec.md` delta scenarios (already created by the propose step) and propagate verification status back to canonical `openspec/specs/architecture-audit/spec.md`.
- [ ] **Step 7.4** Open follow-up issue / design-note TODO for TaskScheduler vs TaskDispatcher responsibility boundary (OPEN).
- [ ] **Step 7.5** Commit: `docs(openspec): mark 2 architecture-audit requirements VERIFIED; note 1 OPEN`.

---

## Task 8: Research note on 4 P2 capability gaps (§8.1–8.5)

<!-- research-only, not on critical path -->

- [ ] **Step 8.1** `mkdir -p openspec/changes/adopt-explore-agent-recommendations/research/`.
- [ ] **Step 8.2** Write `p2-gap-feasibility.md` skeleton:
  ```markdown
  # P2 Capability Gap Feasibility Note

  ## Gap 1: Effect-rs framework adoption
  ## Gap 2: Full event sourcing with aggregate sequences
  ## Gap 3: WebSocket transport resilience
  ## Gap 4: Fiber-based automatic cancellation
  ```
- [ ] **Step 8.3** For each gap: cite `file:line` references in synthia repo for current implementation; reference explore-agent prior research on OpenCode / Codex / pi-mono comparators.
- [ ] **Step 8.4** Each section ends with `**Conclusion:** Open — needs planning-round decision, not adopting here.`
- [ ] **Step 8.5** Commit: `docs(research): feasibility note on 4 P2 capability gaps`.

---

## Task 9: Promote `add-dynamic-tool-provider-system` specs (§9.1–9.8)

- [ ] **Step 9.1** Verify Phase 2 (§1-4) is on master via `git log --oneline master -10`.
- [ ] **Step 9.2** `cp openspec/changes/add-dynamic-tool-provider-system/specs/dynamic-tool-provider/spec.md openspec/specs/dynamic-tool-provider/spec.md` and analogous for `tool-adapter`, `tool-runtime`, `provider-hooks`.
- [ ] **Step 9.3** Insert `## Purpose` (1-2 sentences) above `## Requirements` in each canonical spec.
- [ ] **Step 9.4** Run `openspec validate --all`.
- [ ] **Step 9.5** `sed -i 's/- \[ \]/- [x]/g' openspec/changes/add-dynamic-tool-provider-system/tasks.md`.
- [ ] **Step 9.6** `openspec archive add-dynamic-tool-provider-system`.
- [ ] **Step 9.7** Commit: `docs(openspec): close out add-dynamic-tool-provider-system change`.

---

## Task 10: Final verification (§10.1–10.7)

- [ ] **Step 10.1** `cargo build --workspace` — must be clean.
- [ ] **Step 10.2** `cargo test --workspace` — all pass.
- [ ] **Step 10.3** `cargo clippy --workspace -- -D warnings` — clean.
- [ ] **Step 10.4** `cargo +nightly fmt --all -- --check` — no diff.
- [ ] **Step 10.5** `openspec validate --all` — zero errors.
- [ ] **Step 10.6** `openspec list --changes` — only `adopt-explore-agent-recommendations` (plus any unrelated in-flight).
- [ ] **Step 10.7** Write `verify.md` summarizing R1-R6 outcomes and any leftover follow-ons (TaskScheduler/TaskDispatcher design note; P2 gap decisions).

---

## TDD discipline reminders

- Red → Green → Refactor every micro-step that has a test.
- Micro-steps ≤ 5 minutes; commit at each green step.
- Do not implement multiple providers in one commit (isolation aids review and revert).
- Do not promote a spec until its change's tasks are merged on master.
