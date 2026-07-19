# Plan — Synthia 仓库级架构重设 change #1 (架构基础设施)

> **Per-change execution plan** (per OpenSpec `superpowers-bridge` schema)
> **Order**: omo-writing-plans skill flow applied (brainstorm → proposal → design → specs/* → tasks → plan → [apply] → verify → retrospective)

---

## Trigger

User explicit decision: "我想一次性完成整个仓库的重构达到我的目的" → 选定 C 路线：仓库级架构重设 (1+ 年)，拆为 4 个 OpenSpec change。本文件是 change #1（架构基础设施, ~3 月）的执行计划。

## Goal

在 master HEAD `2f0a9ad`（v3 已 merged）基础上，落地 8 个新 capability + 5 个 modified capability，**不触碰 main_loop / agent core / tool business logic**（由 change #2-#4 承载）。

## Success criteria (binary, verifiable)

1. **8 个新 capability spec.md 通过 OpenSpec CLI schema 校验**
   - `openspec validate 2026-07-18-synthia-top5-borrow-integration --schema superpowers-bridge` 退出 0
2. **25 个独立 PR**，每个 < 500 LOC、可独立 review、revert safe
3. **Quality gates**
   - `cargo +nightly fmt --all` 退出 0
   - `cargo clippy --workspace --all-targets --all-features --tests -- -D warnings` 退出 0
   - `cargo test -p <crate>` **分批**（per Rust project rules: 按模块执行，禁止一次性全跑）
4. **不破坏 Synthia 7 项独有设计**：PrefixTracker 三段 hash、CachePolicyApplier `Arc::ptr_eq`、JSONL 事件流 + TURN_* 三态机、CompactionAnalyticsAttempt trigger、DefinitionDrift、gRPC message-proxy、LoopDetector 三件套 全部保留
5. **公共 API 兼容**：v3 ToolProvider 接口、CLI / Server / Web 二进制入口、`synthia.toml` schema、Synthia 与外部 LLM / sandbox provider 接口全部不变
6. **docs 同步**：`unified-registry-architecture-design.md` + `design-review.md` checked items 标注完成

---

## Execution order (sequential by capability, parallel within capability)

### Phase A — foundation (Week 1-4)

```
Sequential chain (must NOT parallelize — deps chain):
  Task 1.1 PR-1.1 scaffold synthia-event-v2
      ↓
  Task 1.2 PR-1.2 EventEnvelope + EventVersion + EventMeta (uses synthia-prefix-tracker)
      ↓ (parallel batch)
      ├── Task 1.3 PR-1.3 InMemory sink (default)
      ├── Task 1.4 PR-1.4 Sqlite sink (feature-gated)
      └── Task 1.5 PR-1.5 Projector + CommitGuard + gRPC bridge

Sequential chain (must NOT parallelize — deps on 1.x):
  Task 3.1 PR-3.1 OutputBound::Service trait
      ↓
  Task 3.2 PR-3.2 Capability typed contract
      ↓ (parallel batch)
      ├── Task 3.3 PR-3.3 reverse-dependency tracking
      ├── Task 3.4 PR-3.4 peer-source registration
      └── Task 4.1 PR-3.5 scaffold synthia-goal-service
            ↓
            Task 4.2 PR-3.6 CodeGoalService via Arc<Semaphore>
                  ↓
                  Task 4.3 PR-3.7 Keep/Set OCC retry
```

### Phase B — extension + hook (Week 5-10)

```
Parallel batch (independent of A):
  Task 2.1 PR-2.1 scaffold synthia-extension-v2
      ↓
      Task 2.2 PR-2.2 19 typed events
            ↓ (parallel)
            ├── Task 2.3 PR-2.3 typed capability-scoped sandbox (depends on hook-system-unification)
            └── Task 5.1 PR-4.1 HookOutcome 3-state + 10 events
                  ↓
                  Task 5.2 PR-4.2 unified Hook trait + deprecation
                        ↓
                        Task 5.3 PR-4.3 LoopDetector integration
                              ↓
                              Task 2.4 PR-2.4 ExtensionRegistry + double-registration (depends on 3.x)
```

### Phase C — materialization + sanitizer (Week 11-14)

```
Sequential chain:
  Task 6.1 PR-5.1 ToolId + ProviderId + ToolVisibility
      ↓
      Task 6.2 PR-5.2 Materialization struct (in scoped_registry)
            ↓ (parallel batch)
            ├── Task 6.3 PR-5.3 ToolProvenance enum
            ├── Task 6.4 PR-5.4 Scope.fork + whollyDisabled + tool_id projection
            └── Task 7.1 PR-6.1 OutputBound 60 行 (uses ToolId)
                  ↓
                  Task 7.2 PR-6.2 CleanupTask 7d retention + ToolContext::take_output
```

### Phase D — custom event + docs (Week 15-16)

```
Sequential chain (depends on 1.x + 2.x):
  Task 8.1 PR-7.1 AgentEvent::Custom variant
      ↓
      Task 8.2 PR-7.2 EventRenderer registry + builtin JSON
            ↓
            Task 8.3 PR-7.3 Custom projection to AgentMessage
                  ↓ (parallel with 8.3)
                  ├── Task 9.1 docs/unified-registry-architecture-design.md checked
                  ├── Task 9.2 docs/design-review.md remediation table
                  └── Task 9.3 retrospective.md
```

### Phase E — quality gates (Week 16)

```
Parallel batch:
  Task 10.1 cargo fmt + clippy
  Task 10.2 cargo test split (per-module, NEVER --workspace all-at-once)
  Task 10.3 OpenSpec CLI schema validation
  Task 10.4 public-api diff (cargo public-api)
```

---

## Constraints (from `.trae/rules/rust.md` + `CLAUDE.md`)

> Verbatim from `.trae/rules/rust.md`:
> - "不要主动向我提问，自己探索最佳路径实施"
> - "测试不能一次性执行所有测试用例，必须分批次执行，每个批次执行按模块进行"
> - "修复所有 clippy 警告，确保代码通过检查"

> Verbatim from `CLAUDE.md`:
> - "Minimum code that solves the problem. Nothing speculative."
> - "Touch only what you must. Clean up only your own mess."

**Applied to this change**:
- 不一次跑 `cargo test --workspace` — 必须按 crate 分批（Task 10.2）
- 不修复与本 change 范围无关的预存 lint 错误（Task 10.1 仅修本 change 引入的）
- 不主动向用户提问（change #1 决策点已在 design.md D1-D10 锁死）

---

## Risk register + mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| **R1**: 破坏 `ScopedToolRegistry` 618 行现有测试 | Blocker | PR-5.2 显式声明 "现有 LIFO + RAII 不变"；Task 6.2 acceptance = "existing scoped_registry tests pass untouched" |
| **R2**: service 反向依赖循环未切断 | High (H9) | PR-3.1 `OutputBound::Service` 抽象切入口；broker 推到 change #3 |
| **R3**: Plugin 沙箱 sandbox 不充分 | High (B4) | typed capability-scoped sandbox (PR-2.3)；wasm 沙箱推到 change #3 |
| **R4**: MainLoop 跨 phase 引用 (e.g. goal → agent) | Medium | `Weak<runtime>` + `Semaphore` 模式天然无环 (Task 4.2/4.3) |
| **R5**: deprecation window 内用户无法升级 | Medium | 6 个月 + 兼容性 adapter pattern (Task 5.2) |
| **R6**: 新增 `rusqlite` 影响编译时间 | Low | feature-gated (`event-v2,sqlite`)；默认 impl 是 in-memory |
| **R7**: 25 个 PR 跨 3 个月，merge 冲突 | Medium | PR 严格按 capability 边界；agent core 文件不跨 capability 改 |

---

## Stop / signal / continue checkpoints

| Checkpoint | After | Signal | Next |
|------------|-------|--------|------|
| **CP-1** | Phase A end | Task 1.5 + Task 4.3 all green | start Phase B |
| **CP-2** | Phase B end | Task 5.3 + Task 2.4 all green | start Phase C |
| **CP-3** | Phase C end | Task 7.2 + Scope.fork tests green | start Phase D |
| **CP-4** | Phase D end | Task 9.3 + 8.3 green | start Phase E |
| **CP-5** | Phase E end | all 4 quality gates green | mark change #1 archived |

每个 CP 触发 retro checkpoint（不阻塞 30 分钟）：扫描 failure cases / 大改动 / 决策 deviation，写入 `retrospective.md`（5–10 行短记录），不正式开 CR 流程。

---

## Out of scope (frozen for change #1)

- main_loop / turn-state / message projection / convertToLlm → **change #2**
- Tool business logic / tree-sitter AST / sandbox 二阶段 / orchestrator → **change #3**
- Server CLI / MCP / OAuth / 背压 / 分布式 → **change #4**
- 评测 benchmark / 多语言 SDK / Web UI → **路线图外**
- synthia → WASM → **路线图外**

---

## Reference docs (read-only inputs)

| Doc | Role |
|-----|------|
| `docs/superpowers/specs/2026-07-18-synthia-unified-registry-architecture-design.md` | 4 层架构基线 |
| `docs/superpowers/specs/2026-07-18-synthia-design-review.md` | 121 findings (35 critical + 40 high) |
| `openspec/changes/_inbox/v3-tool-centric-multi-expert-analysis.md` | Top-5 ROI 排序 |
| `openspec/changes/_inbox/synthia-critical-review.md` | 11 AgentRunConfig 字段被丢弃 + 20 G/N gap |
| `openspec/changes/_inbox/{codex-vs-opencode-design,opencode-control-plane-patterns,synthia-current-architecture,codex-deep-analysis,opencode-deep-analysis}.md` | 5 份 baseline |
| `openspec/schemas/superpowers-bridge/schema.yaml` | schema 强约束 |

---

## Acceptance gates (binary)

- [ ] 25 PRs each merged with `cargo +nightly fmt --all` + clippy clean + targeted tests green
- [ ] 8 spec.md files pass `openspec validate ... --schema superpowers-bridge`
- [ ] no breaking change to v3 ToolProvider / CLI / Server / Web / synthia.toml / external LLM / sandbox provider interfaces
- [ ] Synthia 7 保留项逐项验证通过 (grep + targeted test)
- [ ] docs unified-registry-architecture + design-review remediation table checked
- [ ] retrospective.md exists with ≥ 4 sections (what worked / what didn't / surprises / lessons for #2-#4)
- [ ] verify.md completed with all quality gates green

---

## Hand-off to apply phase

When this plan is approved, the apply phase (`openspec apply`) will:
1. Pre-flight: cargo baseline (Task 0.1)
2. Execute PR chain A → D
3. Run quality gates E at end
4. Auto-generate `verify.md` from gate results
5. Synthia team manually writes `retrospective.md`

User explicitly excluded commit / push until change #1 verify is approved.