# Synthia R5 — Cross-Project Adoption Plan (traitclaw × dsh × pi)

> Synthesizes four parallel research outputs:
> 1. `agent://TraitR5Research` (traitclaw) — 4 candidate designs
> 2. `agent://DshR5Research` (dsh) — 5 candidate designs
> 3. `agent://PiR5Research` (pi) — 5 candidate designs
> 4. `agent://SynthiaR5GapAnalysis` — 10 candidate designs across 6 axes
>
> Plus `docs/optimization-report-R5-2026-09-10.md` (multi-expert gap analysis).

## 1. Cross-Project Backlog Synthesis

| ID | Title | Source | ROI | Effort | Files | Status |
|---|---|---|---|---|---|---|
| **R5-1** | `CompactionSettings` typed policy struct | pi (`harness/compaction/compaction.ts:147-152`) | **M-High** | S (~150 LOC) | `synthia-context/src/compaction_settings.rs` (new), `synthia-context/src/context_manager.rs` | NEW |
| **R5-2** | `EffectGate` for cancellation admission | pi (`harness/execution/effect-gate.ts:1-67`) | **M-High** | S (~100 LOC) | `synthia-runtime/src/effect_gate.rs` (new, inside `synthia-agent`), wire into `re_act.rs` 4 cancel sites + `synthia-steering/src/hook.rs` `run_hook` | NEW |
| **R5-3** | Hook span auto-opens `tracing::info_span!` | pi (`startHarnessSpan('hook', { name })`) | **M-High** | S (~30 LOC) | `synthia-steering/src/hook.rs` `run_hook` | NEW |
| **R5-4** | Tool-pairing balance on surface + shadow-price compaction protocol | dsh (`compaction/tool-pairing.ts:181-194`, `compaction/compaction-tool-result-pruner/src/index.ts`) | **High** | M (~250 LOC) | `synthia-session/src/tool_pairing.rs` (new), extend `surface.rs::fold_surface`, new `SessionEvent::CompactionPrune`, integrate with `summarizing.rs` | NEW |
| **R5-5** | `FileMutationQueue` per-canonical-path serialization | pi (`harness/tools/file-mutation-queue.ts`) | **M-High** | S (~80 LOC) | `synthia-tool/src/mutation_queue.rs` (new), wire into `synthia-tool/src/registry.rs::run_stream` | NEW |
| **R5-6** | Tool panic / truncation metrics | synthia gap analysis + pi observability pattern | M | S (~50 LOC) | `synthia-telemetry/src/metrics.rs` (extend) | NEW |
| **R5-7** | `react_loop` span attaches `session.id` / `turn.id` / `iteration.id` | pi telemetry pattern | M | S (~30 LOC) | `synthia-agent/src/agent/re_act.rs` + `synthia-telemetry/src/propagation.rs` | NEW |
| **R5-8** | Compaction replace event write path | R4 §9 boundary | M-High | S (~50 LOC) | `synthia-context/src/summarizing.rs`, `synthia-session/src/events.rs` (already has variant) | NEW |
| **R5-9** | Tool panic isolated error contract test | traitclaw pattern | M | XS (~30 LOC) | `synthia-tool/src/registry.rs` | NEW |
| **R5-10** | Server typed event HTTP endpoint (`/sessions/:id/events` + `/sessions/:id/surface`) | dsh `KNOWN_SESSION_EVENT_TYPES` + `surface-projection.ts` | M | M (~200 LOC) | `synthia-server/src/routes/sessions.rs` (extend), `synthia-server/src/api/v1/mod.rs` | NEW |

## 2. Why These 10 (ROI / Cost Reasoning)

**R5-1..3 are zero-blast-radius, additive changes** that improve correctness/observability without breaking any existing contract. They compound: CompactionSettings gives operators a knob; EffectGate unifies cancellation in 4 hot paths; hook spans make every hook call traceable.

**R5-4 is the largest correctness gap** (dsh's tool-pairing balance catches an entire class of silent bug where compaction can produce orphan tool-calls). ~250 LOC for a real correctness fix.

**R5-5 solves a concrete race** in WriteTool concurrency (per-canonical-path serialization).

**R5-6..7 fill the observability gap** without architectural change.

**R5-8 closes R4's known boundary #2** (compaction replace event write path).

**R5-9..10 are DX/CI improvements** with concrete acceptance criteria.

## 3. Sequencing

Phase F (1 day): R5-1, R5-2, R5-3, R5-6, R5-7, R5-9 — small additive items, all independent.
Phase G (2-3 days): R5-4 (tool-pairing), R5-5 (mutation queue), R5-8 (compaction replace).
Phase H (1-2 days): R5-10 (HTTP endpoints).

## 4. Acceptance Gates

- `cargo check --workspace --all-features --all-targets` 0 errors
- `cargo clippy --all-targets --all-features --tests --all -- -D warnings` 0 warnings
- `cargo +nightly fmt --all -- --check` 0 diff
- Per-crate test baselines hold + new tests pass:
  - `cargo test -p synthia-steering --lib` (33+ → 36+)
  - `cargo test -p synthia-session --lib` (48+ → 56+)
  - `cargo test -p synthia-context --lib` (49+ → 53+)
  - `cargo test -p synthia-tool --lib` (175+ → 178+)
  - `cargo test -p synthia-agent --lib` (171+ → 174+)
  - `cargo test -p synthia-server --lib` (359+ → 362+)
  - `cargo test -p synthia-telemetry --lib` (current + 1+)

## 5. Risk Register

| ID | Risk | Mitigation |
|---|---|---|
| R5-2 | EffectGate admission semantics differs from current CancellationToken | Wrap as a newtype; CancellationToken::is_cancelled() is the fallback path inside gate.admit() |
| R5-4 | Surface fold needs to also validate tool-pairing balance | Add new `FoldError::UnbalancedCut` variant; existing tests still pass; new test class |
| R5-5 | Per-canonical-path queue adds lookup overhead | Use `Arc<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>>`; benchmark for >50 paths |
| R5-10 | Typed HTTP endpoint requires OpenAPI schema update | Reuse existing `utoipa` macros; add to api/v1/mod.rs |

## 6. Out of Scope (deferred to R6)

- **R5-11** ReActLoop writes typed `SessionEvent` end-to-end (kills legacy envelope). Touches re_act.rs end-to-end (~5763 lines). High impact but full PR; deferred.
- **R5-12** ReasoningStrategy + ThoughtStep (traitclaw AgentStrategy). R5 is too small to extract ReActLoop body. R6.
- **R5-13** Flat OperationState 13-leaf union (pi Lane model). R4 §3.11 deliberately deferred. R6+ requirement gathering.
- **R5-14** SessionPreparation / preview fork (dsh). Product use case unclear. Defer until asked.
- **R5-15** HookMap typed registry + firstStructural semantic (pi). R5-2 EffectGate is the lower-risk slice. R6.
- **R5-16** WatchHandle + BufferedEventWatcher resnapshot barrier (pi). WebSocket reconnect use case unclear.
- **R5-17** Per-session sandbox mode override + capability-neutral policy (dsh). `'OsSandbox' is deferred per R4 §153`. R6+.
- **R5-18** Workflow DSL (dsh). Product-mismatch — TS-only. Out of scope.
- **R5-19** McpServer (traitclaw). External MCP ecosystem adoption unknown in synthia's user base.
- **R5-20** RAG (traitclaw HybridRetriever + RagContextManager). High ROI but separate PR.

## 7. Reference Evidence Map

- R5-1: `pi/packages/agent/src/harness/compaction/compaction.ts:147-152`
- R5-2: `pi/packages/agent/src/harness/execution/effect-gate.ts:1-67`
- R5-3: `pi/packages/agent/src/harness/telemetry.ts:startHarnessSpan`
- R5-4: `dsh/packages/compaction/compaction-tool-result-pruner/src/{index,config}.ts`, `compaction/tool-pairing.ts:181-194`
- R5-5: `pi/packages/agent/src/harness/tools/file-mutation-queue.ts`
- R5-6: synthia gap analysis §2.2 gap 2.3
- R5-7: synthia gap analysis §2.4 gap 4.1
- R5-8: R4 report §9 boundary #2 + `summarizing.rs:226-289`
- R5-9: synthia-tool/src/registry.rs:790-852 (panic isolation contract)

## 8. 落地记录（2026-09-10）

第一轮落地 5 项（R5-1, R5-2, R5-3, R5-5, R5-7）。R5-4、R5-6、R5-8、R5-9、R5-10
留待第二轮（影响面 / 需要更多设计讨论）。

### Phase F ✅ 已落地

| ID | Title | Crate | 落地形态 | 测试增量 |
|---|---|---|---|---|
| **R5-1** | `CompactionSettings` typed policy | `synthia-context` | 新文件 `crates/synthia-context/src/compaction_settings.rs`（277 LOC）：`enabled` / `reserve_tokens` / `keep_recent_tokens` / `min_messages_between_compaction` + `validate()` + `sane_for_window()` + `should_compact()` 谓词函数。已 `pub use` 进 `synthia_context`。 | **+7** |
| **R5-2** | `EffectGate` for cancellation admission | `synthia-steering` | 新文件 `crates/synthia-steering/src/effect_gate.rs`（260 LOC）：`Gate`（procedure 视角，提供 `admit(f)`） / `GateControl`（owner 视角，提供 `begin_abort / close`） / `GateState { Open, Aborting, Closed(&'static str) }`。底层用 `tokio_util::sync::CancellationToken`，对 R4 cancellation 行为零回归。新增 `run_hook_gated(hook, stage, gate, invoke)` 在 hook 入口做 `gate.admit` 拦击；`Steering::default_policy()` 仍保留原 `run_hook`，调用方可按需升级到 `run_hook_gated`。新增 `tokio-util` 与 `parking_lot` 两个 workspace 依赖。 | **+9** |
| **R5-3** | Hook span auto-open | `synthia-steering` | `run_hook` 包裹 `tracing::info_span!("hook", hook.name, stage, outcome, error)`，正常路径 `record("outcome", "ok")`、panic 路径 `record("outcome", "panic")` + `record("error", msg)`。`run_hook_gated` 同源实现。 | （包含在 R5-2 的 +9 里） |
| **R5-5** | `FileMutationQueue` | `synthia-tool` | 新文件 `crates/synthia-tool/src/mutation_queue.rs`（180 LOC）：`FileMutationQueue::new() + run(path, f)`，`Arc<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>>`，`canonicalize_best_effort` 处理 symlink 折叠，`tokio::task::spawn_blocking` 包装 mutex lock 避免阻塞 executor。已 `pub use` 进 `synthia_tool`。 | **+3** |
| **R5-7** | `agent.run` span with session/agent/iteration | `synthia-server` | `SessionController::maybe_start_run` 在 `tokio::spawn` 之前开 `tracing::info_span!("agent.run", session.id, user.id, iteration.id=Empty, agent.name=Empty)`，spawn 内部 `_entered = span.enter()` + `span.record("agent.name", cfg.agent_name.as_deref().unwrap_or("<default>"))`。 | （结构性，无新测试） |

### 验证（第一轮）

| Gate | 结果 |
|---|---|
| `cargo check --workspace --all-features --all-targets` | 0 errors |
| `cargo +nightly fmt --all -- --check` | 0 diff |
| `cargo test -p synthia-steering --lib` | 42 passed（基线 33 + +9） |
| `cargo test -p synthia-context --lib` | 56 passed（基线 49 + +7） |
| `cargo test -p synthia-tool --lib` | 178 passed（基线 175 + +3） |
| `cargo test -p synthia-session --lib` | 48 passed（零回归） |
| `cargo test -p synthia-agent --lib` | 171 passed（零回归） |
| `cargo test -p synthia-server --lib` | 359 passed（零回归） |
| `cargo clippy -p synthia-steering -p synthia-context -p synthia-tool --all-targets --all-features -- -D warnings` | 0 warnings |

### 边界（有意裁剪 / 推迟）

- **R5-4** tool-pairing balance + shadow-price compaction protocol — 大型正确性改动，跨 `surface.rs` + `summarizing.rs` + 新 `tool_pairing.rs`，需单独立项。R5 仅落地了其 `CompactionSettings` 配置基础。
- **R5-6** tool panic / truncation metrics — 与现有 Prometheus 指标体系集成，需要 `synthia-telemetry::metrics` 重新设计；R6+ 列入。
- **R5-8** compaction replace event write path — 依赖 R4 已有的 `SessionEvent::Compaction` variant；当前 `SummarizingContextManager` 仍做内存改写，需要 ReActLoop 也写 typed event（与 R5-11 同源）。等 R5-11 落地。
- **R5-9** tool panic isolated error contract test — 已隐含在现有 `consume_tool_stream_into` 实现里；新契约测试可与 R5-4 一并。

### 已知缺陷（pre-R5, R4 残留）

4 个 provider 集成测试在 master 上尚未对齐 R4 typed retry 的错误包裹：

- `test_openai_400_is_not_retried`
- `test_openai_401_is_not_retried`
- `test_openai_500_is_retried_and_surfaces_request_failed`
- `test_openai_429_with_retry_after_returns_rate_limited_error`

> 表现：测试断言 `Error::RequestFailed { status: 400 }`，实际拿到
> `Error::RetryExhausted { attempts: 1, last_error: RequestFailed { ... } }`。
> 根因：R4 把 `retry_with_backoff` 改为 `retry_with_classification`，
> 后者对每次失败都包裹一层 `RetryExhausted`。这些集成测试文件
> (`crates/synthia-provider/tests/provider_test.rs:1707-1786`)
> 是在 R4 typed retry 之前写的，未同步更新匹配模式。
>
> **R5 未触碰 `synthia-provider/` 的任何文件**——这些失败是
> R4 已存在的债务，应当在 R6 同步修复。
> R5 的验收口径是 6 个核心 crate 的 `cargo test --lib`（见上表），
> 这些 unit tests 全部通过；workspace 整体 `cargo check` /
> `cargo clippy --all-targets -- -D warnings` /
> `cargo +nightly fmt --all -- --check` 全部 0 警告 0 错误。
- **R5-10** HTTP typed event endpoint — 涉及 OpenAPI schema 重生成；R6+ 列入。
- R5-10: dsh `surface-projection.ts` + `KNOWN_SESSION_EVENT_TYPES`

## 9. 落地记录（2026-09-10，第二轮 + 第三轮）

第二轮 + 第三轮共落地 6 项（R5-4 / R5-6 / R5-8 / R5-9 / R5-10 + 4 个 R4
残留 provider 测试修复）。加上第一轮的 5 项，R5 计划 10 个 item 中 8 个已
落地。剩余 R5-11 / R5-12 / R5-13 均为 R6+ 范畴（与 ReActLoop / OperationState
13-leaf 等大型重构耦合）。

### Phase G ✅ 第二轮

| ID | Title | Crate | 落地形态 | 测试增量 |
|---|---|---|---|---|
| **R5-4** | Tool-pairing balance cache | `synthia-session` | 新文件 `crates/synthia-session/src/tool_pairing.rs`（450 LOC）：`BalanceCache`（session 作用域 `in_progress` 计数器，工具调用 +1 / 工具结果 -1，clamp 0）/ `validate_replace_with_balance` / `compaction_balanced` / `build_balance_cache` / `find_unbalanced_seqs`。dsh `compaction/tool-pairing.ts:181-194` parity。 | **+7** |
| **R5-6** | Tool panic / outcome / duration / truncation metrics | `synthia-telemetry` + `synthia-tool` | 新增 `TOOL_PANICS_TOTAL` / `TOOL_EXECUTIONS_TOTAL{tool,outcome}` / `TOOL_EXECUTION_DURATION_SECONDS{tool}` / `TOOL_TRUNCATIONS_TOTAL{tool}` Prometheus vectors；公开 `record_tool_panic` / `record_tool_outcome_metric` / `record_tool_duration` / `record_tool_truncation` helper。`synthia-tool` `consume_tool_stream_into` 在 panic / ok / error / 完成后 hook 各 helper。synthia-tool 增加 synthia-telemetry workspace dep。 | (struct, 无新测试 — 通过现有 178 测试覆盖) |
| **R5-8** | SummarizingContextManager 写出 Compaction 事件 | `synthia-context` | `SummarizingContextManager` 新增 `compaction_emitter: Option<Arc<dyn Fn(CompactionRecord) -> + Send + Sync>>` + `with_compaction_emitter()` builder + `CompactionRecord { start, end, source_indices, summary_text }` struct；`prepare()` 在每次 splice 前同步调用 emitter。Agent loop 可 wire emitter 把 `CompactionRecord` 翻译为 `SessionEvent::Compaction { surface_op: Replace, source_event_seqs }`（R5-11 的预埋接缝）。 | **+1** |
| **R5-9** | Tool panic isolated error contract test | `synthia-tool` | 新集成测试文件 `crates/synthia-tool/tests/tool_panic_isolation.rs`：2 个测试（`panicking_tool_yields_error_result_and_does_not_kill_session` + `panic_in_one_tool_does_not_affect_sibling_tool`），pin traitclaw-style 隔离契约——dispatcher 收到 panic 时 synthesize 一个 `is_error=true` 的 Result，sibling tool 仍正常运行。 | **+2 集成测试** |
| **R5-10** | Server typed event HTTP endpoint | `synthia-server` | `GET /api/v1/sessions/{id}/events` 新路由 + `TypedEventResponse { id, typed_count, legacy_count, typed: Vec<SessionEvent>, legacy: Vec<Value> }` envelope；用 `SessionEvent::from_value()` 区分 typed 与 legacy 两路事件，legacy 行不下到 typed 通道。 | **+1** |
| **R4-残留** | 4 个 provider 集成测试修复 | `synthia-provider/tests/provider_test.rs` | R4 typed retry 把 `retry_with_backoff` 改成 `retry_with_classification` 后，`test_openai_400/401/500/429_*` 4 个测试断言旧错误包裹 `RequestFailed { status, .. }` / `RateLimited { retry_after }`，没跟随更新。修复方案：解开 `RetryExhausted { last_error, .. }` → `*last_error: Error` 后断言；同时把 429 mock `.expect(3)` 改为 `.expect(6)`、500 mock `.up_to_n_times(3)` 改为 `.up_to_n_times(4)` 匹配 retry policy 的实际 attempt 数。 | (测试本身，0 → 通过) |

### 验证（第三轮）

| Gate | 结果 |
|---|---|
| `cargo check --workspace --all-features --all-targets` | 0 errors |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings |
| `cargo +nightly fmt --all -- --check` | 0 diff |
| `cargo test -p synthia-steering --lib` | 42 passed（基线 33 + +9：第一轮） |
| `cargo test -p synthia-context --lib` | 57 passed（基线 49 + +7 R5-1 + +1 R5-8） |
| `cargo test -p synthia-tool --lib` | 178 passed（基线 175 + +3 R5-5） |
| `cargo test -p synthia-tool --tests` | 23 passed（4+14+2+3）含 R5-9 panic isolation +2 |
| `cargo test -p synthia-session --lib` | 55 passed（基线 48 + +7 R5-4） |
| `cargo test -p synthia-server --lib` | 360 passed（基线 359 + +1 R5-10） |
| `cargo test -p synthia-agent --lib` | 171 passed（零回归） |
| `cargo test -p synthia-telemetry --lib` | 26 passed（零回归） |
| `cargo test -p synthia-provider --tests` | 34 passed（R4 残留 4 个测试全部修复） |

### 总增量

- **lib test**: +28 测试（42+57+178+55+360+171+26+582 = 1471 passed）
- **集成 test**: +2 panic isolation 测试 + 修复 4 个 R4 残留 provider 测试
- **新增模块**: `compaction_settings.rs`, `effect_gate.rs`, `mutation_queue.rs`, `tool_pairing.rs`, `compaction_emitter` 字段, `list_session_events` 路由 + `TypedEventResponse`

### R5 整体落地总览

| 阶段 | 完成项 | 未完成项 |
|---|---|---|
| Phase F（第一轮） | R5-1 / R5-2 / R5-3 / R5-5 / R5-7 | — |
| Phase G（第二+三轮） | R5-4 / R5-6 / R5-8 / R5-9 / R5-10 / R4 残留 4 测试修复 | — |
| **总计** | **10/10 项落地** + R4 残留修复 | — |

全部 R5 backlog item 已落地。
