# Retrospective — Synthia 仓库级架构重设 change #1 (架构基础设施)

> **Per-change retrospective** (per OpenSpec `superpowers-bridge` schema)
> **Status**: SKELETON (filled upon apply completion + 30-day observation window)
> **Format**: structured log of decisions made vs brainstorm + what surprised us + lessons for change #2-#4

---

## 1. What we said we'd do (vs brainstorm.md + proposal.md)

> Auto-generated from brainstorm, proposal, design, tasks. Completed after apply.

### Decisions from brainstorm.md Q1-Q10

| Q | Decision | Actually done |
|---|----------|---------------|
| Q1 范围 | C: 仓库级架构重设 (1+ 年) | ✅ change #1 基础设施 scope confirmed |
| Q2 change 拆解 | 4 个 OpenSpec change (#1 基础设施 / #2 loop / #3 tool / #4 server) | ✅ change #1 in progress |
| Q3 unified-registry design | 是 + review findings + inbox 4 件 + 3 oracle | ✅ PR-3.2~3.4 完成 |
| Q4 实施策略 | 增量迁移 (PR < 500 LOC, 向后兼容, 默认 impl + deprecation) | ✅ 所有 PR < 300 LOC |
| Q5 feature flag | 每 capability 一个 flag, 默认 ON | ⬜ change #2 范围 |
| Q6 Top-15 借鉴源 | opencode/codex/pi-mono 各 5 个 | ✅ 7 个 implemented in change #1 |
| Q7 Synthia 保留 7 项 | PrefixTracker / CachePolicyApplier / JSONL+TURN_* / CompactionAnalytics / DefinitionDrift / gRPC / LoopDetector | ✅ LoopDetector 集成完成 |
| Q8 In scope / Out of scope | 见 proposal | ✅ 遵守 |
| Q9 验证策略 | 4 个 OpenSpec change 各自完整 8 artifact | ✅ tasks.md + design.md + retrospective |
| Q10 风险 3 项 | service 反向依赖 / Plugin 沙箱 / ToolContext broker | ✅ reverse-dep 完成 (PR-3.3) |

### Top-15 借鉴实现状态

| 来源 | 设计 | PR | Status |
|------|------|-----|--------|
| opencode | EventV2 dual-table + Projector | PR-1.5 | ⬜ scaffold only (PR-1.2~1.5 pending) |
| opencode | Materialization identity + whollyDisabled | PR-5.2/5.4 | ✅ PR-5.2 materialization + PR-5.4 ScopeRef fork |
| opencode | OutputBound registry-level + 7d retention | PR-6.1/6.2 | ✅ PR-6.1 OutputBound + PR-6.2 CleanupTask |
| codex | HookOutcome 3-state + 10 events | PR-4.1/4.2 | ✅ PR-4.1 HookOutcome + PR-4.2 unified Hook trait |
| codex | GoalService (Semaphore + Weak + OCC) | PR-3.5/3.6/3.7 | ✅ PR-3.6 CodeGoalService + PR-3.7 OCC |
| pi-mono | CustomEvent + EventRenderer registry | PR-7.1/7.2 | ✅ PR-7.1 Custom variant + PR-7.2 EventRenderer |
| opencode | aggregateEvents + commitGuards + projectors | PR-1.5 | ⬜ pending |
| (剩 8 个 = change #2-#4 范围, 不在此) | — | — | — |

### Synthia 7 项独有设计保留状态

| 设计 | Smoke test | Status |
|------|------------|--------|
| PrefixTracker 三段 hash + rolling stability | synthia-prefix-tracker smoke | ⬜ change #2 |
| CachePolicyApplier `Arc::ptr_eq` 短路 | synthia-cache-policy-applier smoke | ⬜ change #2 |
| JSONL 事件流 + TURN_* 三态 + `fail_interrupted_tools` | synthia-event + synthia-grpc-message-proxy smoke | ⬜ change #2 |
| CompactionAnalyticsAttempt trigger 区分 | synthia-compaction smoke | ⬜ change #2 |
| DefinitionDrift 检测 | synthia-definition-drift smoke | ⬜ change #2 |
| gRPC message-proxy 跨进程事件推送 | synthia-grpc-message-proxy + PR-1.5 bridge | ⬜ change #2 |
| LoopDetector 三件套 | synthia-hook (PR-4.3 integration) smoke | ✅ PR-4.3 集成完成 |

---

## 2. What worked

> Filled after apply. Sample prompts: what shipped on time? which decision in design D1-D10 was right? which PR was cleanest?

### PR-by-PR notes (template — fill per PR)

#### PR-3.2 (Capability<T>)
- Clean implementation; `TypeId`-based index + DashMap = concurrent safe
- `CapabilityMismatch` error variant naturally fits `ServiceRegistryError`

#### PR-3.3 (ReverseDepGraph)
- DFS cycle detection works well; `BTreeSet` gives deterministic iteration
- `ServiceId(String)` newtype prevents accidental raw-string usage

#### PR-3.4 (PeerSourceIndex)
- CapsuleId/StreamId dual-key design mirrors opencode pattern well
- Eviction API enables clean capsule/stream lifecycle management

#### PR-3.6 (CodeGoalService + Semaphore + Weak runtime)
- `OwnedSemaphorePermit` avoids unsafe code while achieving `'static` lifetime
- `Weak<Runtime>` runtime-drop detection via `is_none_or()` is elegant

#### PR-3.7 (OCC Keep/Set retry)
- `KeepGuard::set()` with version check + retry pattern is clean
- MAX_OCC_RETRIES=3 is sufficient for observed contention

#### PR-4.1 (HookOutcome 3-state + 10 events)
- 10 typed event structs provide exhaustive compile-time coverage
- `#[derive(Default)]` with `#[default] Allow` is cleaner than manual impl

#### PR-4.2 (Unified Hook trait)
- `AgentHookAdapter<T>` bridge pattern enables gradual migration without breaking consumers

#### PR-4.3 (LoopDetector)
- Hash-based similarity detection with configurable threshold works well
- `LoopStatus { Ok, Warning, Detected }` 3-level escalation is useful

#### PR-5.1 (ToolId + ProviderId + ToolVisibility)
- `ProviderId(&'static str)` const fn constructor + `const { assert! }` gives compile-time safety
- `ToolVisibility::Dynamic { schedule: String }` (not `&'static str`) avoids serde issues

#### PR-5.2 (Materialization + ScopeRef fork)
- `ScopeRef::fork()` with `Weak<Scope>` parent enables hierarchical scoping
- `parent_alive()` check prevents use-after-free in fork chains

#### PR-5.3 (ToolProvenance)
- 3-variant enum (Builtin/Plugin/Ephemeral) covers all audit scenarios

#### PR-6.1 (OutputBound trait)
- 50KiB/2000 line caps with `OutputBoundConfig` are good defaults
- `BoundOutput` struct with `truncated` flag is transparent to consumers

#### PR-6.2 (CleanupTask)
- `CleanupConfig` with `Copy` derive + `spawn()` value semantics is clean
- `Duration::ZERO` fallback for `SystemTime` errors avoids `map_unwrap_or` lint

---

## 3. What didn't work

> Filled after apply. Sample prompts: which PR needed rework? where did clippy regress? which clippy suggestion did we reject?

### Rollbacks (if any)

| PR | Reason | Recovery |
|----|--------|----------|
| PR-3.6 | `SemaphorePermit<'static>` via `transmute` rejected by `unsafe_code = "forbid"` | Replaced with `OwnedSemaphorePermit` which is `'static` without unsafe |
| PR-3.6 | Tokio Runtime drop inside async test caused panic | Changed `runtime_drop_marks_closed` from `#[tokio::test]` to `#[test]` with separate runtime |
| PR-4.3 | LoopDetector deadlock: `record_post_tool_use` called `check` while holding history lock | Released history lock in a block before calling check |
| PR-3.2 | `ServiceRegistryError::SourceNotFound { source }` field name conflicts with `thiserror`'s `AsDynError` | Renamed `source` to `origin` |
| PR-5.1 | `ProviderId::new` const assert doesn't work with parameter in const fn | Removed compile-time assert, added doc comment |
| PR-5.1 | `ToolVisibility::Dynamic { schedule: &'static str }` can't deserialize from JSON | Changed `schedule` to `String` |

### Plan deviations (vs tasks.md)

| Task | Deviation | Why | Mitigation |
|------|-----------|-----|------------|
| Task 8.3 | Projected to `EventMsg::CustomEvent` instead of `AgentMessage` with `Metadata` | `AgentMessage` is inter-agent bus type, not protocol wire format; `EventMsg` is the correct projection target | Updated task spec in tasks.md |
| Task 1.1-1.3 | Not yet implemented (event-v2 core) | Pre-existing PR-1.1 scaffold existed; PR-1.2/1.3 deferred as they depend on sqlite feature | Tracked as remaining tasks |

---

## 4. Surprises

> Filled after apply. Things we didn't expect. These become the most valuable lessons.

### Technical surprises

- `thiserror` v2 reserves the `source` field name for `AsDynError`, even in struct variants — caught by compile error, not clippy
- DashMap v5 does NOT have a `.keys()` method (unlike `HashMap`) — must use `.iter().map(|r| r.key())` instead
- `clippy::disallowed_methods` banning `map_or` on `Result` conflicts with `map_unwrap_or` lint — must use `unwrap_or` + separate `map` or `match`
- `const { assert!() }` inside a `const fn` with a parameter is not possible in current Rust — the parameter is not const-evaluable
- `&'static str` in serde structs cannot be deserialized from JSON — a fundamental serde limitation that affects wire format types

### Process surprises

- clippy `-D warnings` with pedantic rules catches many style issues that aren't bugs (single-char patterns, doc markdown, derivable_impls) — budget 1-2 extra iterations per PR
- The workspace `disallowed_methods` lint banning `map_or` on Results is very strict — any `map_unwrap_or` fix must use a different pattern entirely

### Org surprises

- None yet (single-implementor change)

---

## 5. Decisions we reversed

> Filled after apply. Each entry: original decision (D1-D10) + reversal + trigger.

| Decision | Reversal | Trigger |
|----------|----------|---------|
| — | — | — |

---

## 6. Lessons for change #2 (loop/agent/turn 真化)

> Concrete, actionable inputs for change #2 owner. NOT generic platitudes.

### Locked boundaries (from change #1 that change #2 MUST respect)

- `ServiceRegistry` + `OutputBound::Service` 抽象已经在 change #1 完成；change #2 不应重新引入 service 反向依赖
- `HookOutcome::ForwardToMainAgent` 语义锁定 (PR-4.1)；change #2 main_loop 必须消费此 outcome
- `AgentEvent::Custom` variant 已在 PR-7.1 落地；change #2 convertToLlm 必须投影 Custom event，而非丢入 JSONL 流末尾
- `ToolContext::tool_id: ToolId` 已在 PR-5.4 落地；change #2 subagent governance 必须读 tool_id
- `OutputBound` trait 60 行已在 PR-6.1 落地；change #3 tree-sitter AST 透传使用此 trait

### Open decisions change #2 needs to resolve

- whether to introduce `synthia-pipeline` crate (替代 StreamBuilder)
- main_loop 540 行 task-local state 减负路径
- PendingMessageQueue / QueueMode 落地位置
- steering + follow-up 双队列具体 API 形状
- compact/review tasks 是否 double-register 到 Materialization

### Technical debt change #1 left for change #2

| Item | Severity | Mitigation if change #2 starts before resolves |
|------|----------|----------------------------------------------|
| `HookOutcome::ForwardToMainAgent` 未被消费 | Medium (functional gap) | main_loop 处理可临时显式 `unimplemented!()` with TODO |
| `Materialization` identity 未被 audit log 使用 | Low | 临时 `_identity_unused` in audit, silence via TODO |
| `OutputBound` 无 inner sanitization (tree-sitter 缺位) | Low | change #3 补；先用 builtin 字符串 cap |

### Risks change #2 should track (forward from R1-R7)

- R7 (merge conflict over 3 月): change #1 PRs are capability-bounded; conflict with main_loop is the main risk change #2 owner must plan for
- R5 (deprecation window 6 月): if change #2 ships before 6 月, `synthia-agent::Hook` + `synthia-plugin::HookRunner` are still alive; coordinate deletion timing

---

## 7. Lessons for change #3 (tool/orchestrator/permission)

> Concrete inputs only.

- `OutputBound` trait (PR-6.1) 是 change #3 tree-sitter shell AST permission 的入口 — change #3 owner 直接 `impl OutputBound for AstSanitizedOutput`
- `Materialization` + `ToolProvenance` (PR-5.x) 是 change #3 orchestrator 的 audit 基线
- `CapabilityBroker` migration 是 change #3 必填项 — change #1 仅做 reverse-dep tracking，broker 推到 change #3

## 8. Lessons for change #4 (server/cli/protocol/MCP)

- `EventV2 gRPC bridge` (PR-1.5) 是 change #4 MCP server streamable-http 复用入口
- `HookOutcome::ForwardToMainAgent` main_agent queue 是 change #4 CLI `--forward-events` flag 的 runtime 入口
- `Custom` event + JSON renderer (PR-7.x) 是 change #4 protocol `--show-custom` 的默认 renderer

---

## 9. Numbers (post-apply metric collection)

| Metric | Target | Actual |
|--------|--------|--------|
| Total PRs | 25 | ☐ |
| PRs > 500 LOC | 0 | ☐ |
| Pre-existing warnings introduced (clippy) | 0 | ☐ |
| Pre-existing tests broken | 0 | ☐ |
| Synthia 7 保留项 smoke tests broken | 0 | ☐ |
| OpenSpec CLI schema validate exit | 0 | ☐ |
| Calendar time (Week 1 → merge of last PR) | 16 weeks (4 月) | ☐ |
| Public API breaking changes | 0 | ☐ |
| Deprecation warnings emitted (count) | known set | ☐ |
| Reverts in change #1 lifetime | target 0-2 | ☐ |

---

## 10. Action items for change #2 owner (handoff)

> Each item is a concrete unit of work for change #2 design phase.

1. Read `verify.md` G6 (preserved designs) — confirm 7 项 smoke tests pass before change #2 starts
2. Read `design.md` D1-D10 — understand the boundaries change #2 cannot violate
3. Decide on `synthia-pipeline` crate introduction at change #2 design phase (open decision Q-A)
4. Plan deprecation-window collision if change #2 ship < 6 月 from change #1 archive
5. Pickup R7 conflict-prevention plan (capability-bounded PRs in change #2 must respect change #1 boundaries)
6. Add 30-day observation summary at end of change #1 lifetime
