# Change: Synthia 仓库级架构重设 change #1 — 架构基础设施

> **Why, What, Impact** (per OpenSpec `superpowers-bridge` schema)

---

## Why

Synthia master (`2f0a9ad`) 已完成 v3 agent-toolification，但仓库级实测架构与"生产级 AI agent"差距巨大，存在三组阻塞：

1. **架构文档已写但未落地**：`unified-registry-architecture-design.md` (1823 行) 定义了 4 层 + Materialization + ServiceRegistry + EventBus + Extension，但 master 仅有 4 处 stub file（`synthia-event::bus` 39 行 + `synthia-extension::lib` 1 行 + `synthia-service` 1211 行含 4 处 TODO）；`design-review.md` 121 findings (35 critical + 40 high) 全未修；4 份 `openspec/changes/_inbox/` baseline analysis 显示 11 个 AgentRunConfig 字段被丢弃。

2. **基线差距**（3 oracle 并行调研 opencode/codex/pi-mono 已确认）：Event 系统仅有 3 平行 channel stub；Tool 输出截断散落 MAX_CAPTURE_BYTES 无 registry 级；Materialization 缺 identity + whollyDisabled；Hook 双系统（AgentHook + HookRunner）并存且无 HookOutcome 3 态；`main_loop` 1077 行含 540 行 task-local state hack；`apply-patch` linear scanner 缺 4-tier seek + lenient + ENVIRONMENT_ID；`ScopedToolRegistry` 618 行 LIFO + RAII 但缺 identity field。

3. **路线图缺失**：用户决策"一次性完成整个仓库的重构"，但单 sprint 无能力承载 1+ 年重设。需要 4 个 OpenSpec change 顺序推进：change #1 基础设施 → change #2 loop/agent/turn 真化 → change #3 tool/orchestrator/permission → change #4 server/cli/protocol/MCP。

本次 change #1 是 4 change 路线图的第 1 步，目标填实 8 个 capability（event-v2 / extension-system / service-registry-completion / goal-service-runtime / hook-system-unification / tool-materialization-identity / tool-output-sanitizer / custom-event-renderer），全部为后续 change #2-#4 铺基线；本次 change 不触碰 main_loop / agent core / tool business logic（由 change #2-#3 承载）。

---

## What

### 8 个新增 capability（全部 change #1 内）

| Capability | Spec delta |
|------------|-----------|
| `event-v2-system` | ADDED — durable/ephemeral dual-table EventV2 + Projector + CommitGuard + EventStore + EventStream subscriptions |
| `extension-system` | ADDED — 19 typed hook events + Extension trait + ExtensionManifest + ExtensionRegistry + sandbox isolation |
| `service-registry-completion` | ADDED — `OutputBoundService::bound()` + typed Capability contract + peer-source (CapsuleId/StreamId) + reverse-dependency resolution (change #1 决策点) |
| `goal-service-runtime` | ADDED — CodeGoalService via `Arc<tokio::sync::Semaphore>` + Weak runtime + Keep/Set OCC + eviction |
| `hook-system-unification` | ADDED — HookOutcome 3-state (Allow/Deny/ForwardToMainAgent) + 10 events + 双系统合并为单一 `synthia-hook::HookRunner` |
| `tool-materialization-identity` | ADDED — `Materialization` + identity field + `whollyDisabled` + `ToolProvenance` + `Scope.fork` + `tool_id` 投影 |
| `tool-output-sanitizer` | ADDED — `OutputBound` + Contentlen histogram + 50KiB/2K 行 cap + 7d retention + CleanupTask + ToolContext::take_output + apply-patch v4a lenient + 4-tier seek |
| `custom-event-renderer` | ADDED — Custom event variant + EventRenderer registry + builtin JSON renderer + 投影到 AgentMessage |

### 5 个 modified capability

| Capability | 改动点 |
|------------|--------|
| `core-orchestration` | 现有 `ScopedToolRegistry` 增加 `identity` field；保留 LIFO + RAII 不动；新增 `Scope.fork`/`whollyDisabled` filter |
| `session-management` | `OpRun` 接口保留并接受 v3 ToolProvider；新增 `tool_id` 字段记录 materialization identity |
| `event-system` | 现有 `AgentEventEmitter` 标注 deprecated，3 月后删除；`EventV2` 替代为默认实现 |
| `extension-system` | 现有 hook 系统保留作 fallback；新 `Extension` trait 并行；3 月后合并 |
| `protocol` | JSONL 事件流保留；增加 Extension event 字段投影到协议 header |

### Top-15 设计借鉴（强化此 change 的 8 capability）

| 来源 | 设计 | 落地 |
|------|------|------|
| opencode | Event Sourcing + durable/ephemeral + versioned + aggregateEvents | event-v2-system |
| opencode | Materialization identity + whollyDisabled + ToolProvenance | tool-materialization-identity |
| codex | HookOutcome 3-state + 10 events | hook-system-unification |
| codex | GoalService (Semaphore + Weak runtime + Keep/Set OCC) | goal-service-runtime |
| pi-mono | CustomEvent + EventRenderer registry | custom-event-renderer |
| opencode | OutputBound registry-level + 7d retention + CleanupTask | tool-output-sanitizer |
| opencode | tree-sitter shell AST permission | change #3 (此处不开) |
| opencode | aggregateEvents + commitGuards + projectors | event-v2-system |
| codex | Tasks enum (Regular/Compact/Review) + AnySessionTask | change #2 (此处不开) |
| codex | TurnState 5 channels + MailboxDeliveryPhase | change #2 |
| codex | ToolSpec 4-layer + ToolPayload + ToolExposure | change #3 |
| codex | MCP streamable-http + ws + OAuth | change #4 |
| pi-mono | convertToLlm + transformContext | change #2 |
| pi-mono | PendingMessageQueue + QueueMode | change #2 |
| pi-mono | main_loop 减负 | change #2 |

### Synthia 保留 7 项独有设计（禁止覆盖）

- **PrefixTracker 三段 hash + rolling stability window** (P1) — event-v2-system 必需复用
- **CachePolicyApplier `Arc::ptr_eq` 短路** (零拷贝) — tool-output-sanitizer 必须保留路径
- **JSONL 事件流 + TURN_* 三态机 + `fail_interrupted_tools`** (P5) — hook-system-unification 保留兼容
- **CompactionAnalyticsAttempt trigger 区分** (P4+P9) — event-v2-system 保留事件
- **DefinitionDrift 检测** (subagent governance) — extension-system 借用兼容
- **gRPC message-proxy** 跨进程事件推送 — event-v2-system 必须支持
- **LoopDetector 三件套** — hook-system-unification 集成事件

---

## Impact

### 受影响的现有 capabilities

| Capability | 关联 PR | 现有 Owner |
|------------|---------|------------|
| `core-orchestration` | PR-5.1–5.4 | `crates/synthia-tool/src/scoped_registry.rs` (618 行) |
| `event-system` | PR-1.1–1.5 | `crates/synthia-event/src/{bus,lib}.rs` (40 行 stub) |
| `extension-system` | PR-2.1–2.4 | `crates/synthia-extension/src/lib.rs` + `crates/synthia-hook/src/lib.rs` (双系统) |
| `service-registry` | PR-3.1–3.4 | `crates/synthia-service/src/{traits,registry,goal}.rs` (1211 行) |
| `session-management` | PR-5.2 | `crates/synthia-session/` |
| `protocol` | PR-1.5 + PR-7.3 | `crates/synthia-protocol/` |

### 新增 crates

- **新增**（3 个独立 crate，质量门禁后启用）：
  - `synthia-extension-hook`（19 typed hook events，sandbox infrastructure）
  - `synthia-goal-service`（CodeGoalService + Keep/Set OCC）
  - `synthia-tool-materialization`（Materialization + identity + Provenance + Scope.fork）

### 影响环境与依赖

- **新增外部依赖**：`rusqlite` 0.32+（仅 EventV2 双表）；其他 crate 仅使用现有 `parking_lot` / `tokio` / `serde_json` / `async-trait` / `thiserror`
- **Feature flags 新增**（默认 ON）：
  - `event-v2`
  - `extension-v1`
  - `goal-service-v1`
  - `hook-unified`
  - `tool-materialization-v1`
  - `tool-output-sanitizer-v1`
  - `custom-event-v1`
- **Migration windows**：
  - `synthia-plugin::hook_runner` deprecated 3 月
  - `synthia-event::bus::AgentEventEmitter` deprecated 3 月
  - `synthia-agent::Hook` trait deprecated 6 月
- **二进制接口**：仅 in-process 改动；不影响 CLI / Server / Web 二进制接口
- **配置文件**：`synthia.toml` 无必需变化；新增 `[event_v2]` / `[extension]` / `[goal_service]` 段（全部可选）

### 风险与依赖循环

- **风险 1**：service → [core, loop] 反向依赖循环（design-review H9 blocking）— 本 change 在 PR-3.1 通过 `OutputBound::Service` trait 抽象 + Capability contract 解决
- **风险 2**：Plugin 沙箱（design-review B4 blocking）— 本 change 在 PR-2.3 实现 typed hook 签名 + capability-scoped execution，避免外进程沙箱依赖
- **风险 3**：ToolContext `Arc<ServiceRegistry>` → CapabilityBroker（change #3 解决）
- **依赖循环**：本 change 不引入跨 cycle 依赖；`synthia-event-v2` 仅依赖 `synthia-protocol` + `synthia-tool` traits；`synthia-extension-hook` 依赖 `synthia-event-v2` 单向

### 验证标准

- [ ] `cargo +nightly fmt --all` 无改动
- [ ] `cargo clippy --workspace --all-targets --all-features --tests -- -D warnings` 全绿
- [ ] `cargo test -p synthia-event-v2` + `cargo test -p synthia-extension-hook` + 4 个 crate 验收测试全绿
- [ ] `cargo bench -p synthia-tool-materialization` 无 regression
- [ ] docs/unified-registry-architecture-design.md + design-review.md 全部 checked items 标记
- [ ] 8 个 capability spec.md 通过 OpenSpec CLI schema 校验
- [ ] 25 个 PR 各 < 500 LOC、独立 review、revert safe

### Out of scope（推后到 change #2-#4）

- change #2: main_loop 减负 / convertToLlm / SteeringQueue / turn-state machine / message projection
- change #3: Tool business logic 全量 / tree-sitter AST / sandbox 二阶段 / orchestrator 编排 / 多策略 router / permission policy
- change #4: server CLI / MCP streamable-http / OAuth / 背压 -32001 / 分布式
- 评测 benchmark / 多语言 SDK / Web UI / synthia→WASM / 分布式全量

### Systems 边界

本 change **不修改**：
- CLI / Server / Web 二进制入口
- `synthia.toml` schema
- Synthia 与外部 LLM provider 接口
- Synthia 与外部 sandbox / 沙箱 provider 接口
- 跨进程协议 (gRPC message-proxy 仅适配 EventV2，行为不变)