# Brainstorm — Synthia 仓库级架构重设 change #1: 架构基础设施

> **Source**: openspec change `2026-07-18-synthia-top5-borrow-integration`（路线图 change #1）
> **Schema**: superpowers-bridge
> **Captured**: 2026-07-18
> **Format**: raw capture of brainstorming conversation (decision log)

---

## Background

Synthia master (`2f0a9ad` "Merge feat/agent-toolification-v3 into master") 已完成 v3 agent-toolification，但仓库仍有大量架构债务：

### 文档已写但未落地

1. `docs/superpowers/specs/2026-07-18-synthia-unified-registry-architecture-design.md` (1823 行) — 4 层架构 + Materialization + ServiceRegistry + EventBus + Extension，**多数未落地**
2. `docs/superpowers/specs/2026-07-18-synthia-design-review.md` (373 行) — 4 oracle 评审 121 个 findings (35 critical + 40 high + 38 medium + 8 low)，**全部未修**
3. `openspec/changes/_inbox/v3-tool-centric-multi-expert-analysis.md` (751 行) — Top-5 ROI 排序 + P0-P8 路线图
4. `openspec/changes/_inbox/synthia-critical-review.md` (233 行) — 11 个 AgentRunConfig 字段被丢弃 + 20 G/N gap
5. `openspec/changes/_inbox/codex-vs-opencode-design.md` + `opencode-control-plane-patterns.md` + `synthia-current-architecture.md` + `codex-deep-analysis.md` + `opencode-deep-analysis.md` — 5 份 base analysis

### 实际 stub/不完整模块 (git ls-tree + wc -l 验证)

| 模块 | 文件:行 | 状态 |
|------|---------|------|
| `synthia-event` | `bus.rs` 39 行 + `lib.rs` 1 行 | stub |
| `synthia-extension` | `lib.rs` 1 行 + `manifest.rs` 33 行 | stub |
| `synthia-service::registry` | 286 行 | 4 处 TODO |
| `synthia-service::goal` | 190 行 | stub-grade (单 Mutex<Option<Goal>>) |
| `synthia-hook` | 双系统 (AgentHook + HookRunner) | 未合并，无 HookOutcome 3 态 |
| `synthia-agent::main_loop` | 1077 行 | 缺 AgentMessage 投影、SteeringQueue 双队列 |
| `Tool` 输出截断 | 散落 MAX_CAPTURE_BYTES | 无 registry-level |
| `ScopedToolRegistry` | 618 行 | 缺 identity field + whollyDisabled filter |
| apply-patch v4a | 488 行 | linear scanner，缺 4-tier seek + lenient + ENVIRONMENT_ID |
| HookRunner | 200 行 | 仅外进程 plugin |

### 与 opencode/codex/pi-mono 的真实差距 (3 oracle 并行验证)

| 维度 | opencode | codex | pi-mono | synthia |
|------|----------|-------|---------|---------|
| Event 系统 | Event Sourcing + durable/ephemeral + versioned + aggregateEvents | partial JSON-RPC | 简单 events | 3 平行 channel, stub |
| Tool 输出截断 | 50KiB/2K 行 + 7d retention cleanup | TruncateConfig | 无 | 散落内联 |
| Materialization | identity + whollyDisabled + ToolProvenance | 无 | 无 | LIFO 有, identity 无 |
| Hook 系统 | Plugin typed 19 | 10 events + 3-state Fail | 30 extension overloads | 双系统并存 |
| convertToLlm/transformContext | Effect transform chain | item 硬编码 | 4 行 default | ❌ (main_loop:264 hack) |
| Steering 队列 | 单一 | 5 通道 | steering + follow-up 双队列 | 单优先级 cap=8 |
| Goal/目标驱动 | ❌ | ✅ GoalService + semaphore | ❌ | stub Mutex<Option> |
| RunCoordinator | coalesced wake/run | ❌ | ❌ | ❌ |
| AST shell permission | tree-sitter | ❌ | ❌ | 字符串黑名单 |

---

## Decision chain

### Q1: 范围 — 整个仓库怎么重设？
- A: 仅 Top-5 ROI ❌
- B: Top-5 + inbox 4 件 ❌
- C: **仓库级架构重设 (1+ 年)** ✅
- D: 仓库质量 pass ❌

**Chosen**: C。目标 = 仓库级架构落地，达到"超过 opencode/codex/pi-mono 的任意一个"。

### Q2: 单一 OpenSpec change 还是多个？
**Chosen**: **4 个 OpenSpec change 按依赖顺序**：
- change #1 (本 change): 架构基础设施 (registry/event/extension/service 真化) — ~3 月
- change #2: loop/agent/message/turn-state 真化 — ~3 月
- change #3: tool/orchestrator/permission/sandbox 真化 — ~3 月
- change #4: server/cli/protocol/mcp/server-背压 真化 — ~2 月

### Q3: 是否使用 unified-registry design 1823 行作为基线？
**Chosen**: 是 + 叠加 121 review findings + inbox 4 份 + 3 oracle 新发现。

### Q4: 实施策略 — 大爆炸 vs 增量迁移？
**Chosen**: **增量迁移**。每 PR < 500 LOC、向后兼容 (默认 impl + deprecation)、独立 review、revert 安全。

### Q5: feature flag 策略？
**Chosen**: **每 capability 一个独立 feature flag**，默认 ON，关闭作 escape hatch。

### Q6: 哪些 opencode/codex/pi-mono 设计是仓库级重设必含？
**Chosen (Top-15)**：

| 来源 | 设计 | 落地 change |
|------|------|-------------|
| opencode | EventV2 (durable/ephemeral + 双表 + Projector/CommitGuard) | #1 |
| opencode | Materialization identity + whollyDisabled + ToolProvenance + Scope.fork | #1 |
| opencode | OutputBound registry-level + 7d retention + CleanupTask | #3 |
| opencode | tree-sitter shell AST permission | #3 |
| opencode | aggregateEvents + commitGuards + projectors | #1 |
| codex | Tasks enum (Regular/Compact/Review) + AnySessionTask erasure | #2 |
| codex | TurnState 5 channels + MailboxDeliveryPhase + oneshot routing | #2 |
| codex | ToolSpec 4-layer + ToolPayload + ToolExposure | #3 |
| codex | HookOutcome 3-state + 10 events + HookRunner 合并 | #1 |
| codex | GoalService (Semaphore + Weak runtime + Keep/Set OCC) | #1 |
| codex | MCP streamable-http + ws + OAuth + JSON-RPC + 背压 -32001 | #4 |
| pi-mono | convertToLlm + transformContext + AgentMessage 投影 | #2 |
| pi-mono | PendingMessageQueue (steering + follow-up) + QueueMode | #2 |
| pi-mono | CustomEvent + EventRenderer registry | #1 |
| pi-mono | main_loop 减负 (删 task-local + Span guard + 540 行) | #2 |

### Q7: Synthia 哪些独有设计必须保留？
**Chosen (保留 7 项)**：
- PrefixTracker 三段 hash + rolling stability window (P1 锚点)
- CachePolicyApplier `Arc::ptr_eq` 短路 (零拷贝)
- JSONL 事件流 + TURN_* 三态机 + `fail_interrupted_tools` (中断正确性)
- CompactionAnalyticsAttempt trigger 区分 (P4 + P9 交叉点)
- DefinitionDrift 检测 (subagent governance)
- gRPC message-proxy 跨进程事件推送
- LoopDetector 三件套 (pi-mono 没有)

### Q8: 范围 — In scope / Out of scope？
**In scope (1+ 年路线图)**：上述 Top-15 + 7 项保留 + 121 findings + 34 crate stub 填实 + v3 接口不变 + 完整测试
**Out of scope**：多语言 SDK / Web UI / 分布式 / 评测 benchmark / synthia→WASM

### Q9: 验证策略？
**Chosen**: 4 个 OpenSpec change 各自完整 8 artifact。每 change 完成后跑 `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo +nightly fmt --all`。

### Q10: 风险最大的 3 件事？
1. **service → [core, loop] 反向依赖循环** (design-review H9 blocking)
2. **Plugin 沙箱** (design-review B4 blocking)
3. **ToolContext `Arc<ServiceRegistry>` → `CapabilityBroker`** (design-review B5 security blocking, change #3)

---

## Trade-offs accepted

- [Trade-off] 1+ 年路线图 vs 单 sprint — 接受理由：用户明确选 C；4 change 各 2-3 月仍可控
- [Trade-off] 4 个 OpenSpec change vs 单 change — 接受理由：apply phase 上下文限制
- [Trade-off] 每 capability 一个 feature flag — 接受理由：灰度迁移能力；禁止 flag abuse
- [Trade-off] 不实现 tree-sitter PoC 之外的 WASM 沙箱 — 接受理由：1+ 年路线不背额外技术风险
- [Trade-off] Synthia 独有 7 项优势保留 — 接受理由：避免妄自菲薄

## Open Questions

- 是否在 change #1 引入 synthesized-agent 概念 — 推迟到 change #2 设计阶段
- 是否所有 v3 ToolProvider 都需 double-register 到新 Materialization — change #1 决策点
- 4 change 之间的边界 (service/hook/extension ownership) — 在 change #1 的 design.md 锁定
- 是否引入 `synthia-pipeline` crate (替代 StreamBuilder) — change #2 决策点