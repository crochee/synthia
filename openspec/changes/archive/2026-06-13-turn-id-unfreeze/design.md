## Context

### 背景

`turn-id-mvp` change 在 2026-06-13 启动后被冻结 3 个月（2026-06-13 → 2026-09-13），冻结期内不解冻、不实施。冻结的 3 个解冻条件：

1. **条件 #1**：出现"按 turn 维度查询"的真实 caller
2. **条件 #2**：TokenUsage 收敛、recovery path 等其他原语收敛
3. **条件 #3**：3 个月时间窗口

2026-06-13 当日，OpenAI codex 团队合并 2 个 PR，**直接满足条件 #1**：

| PR | 标题 | 改动文件 | 与 Turn 的关系 |
|----|------|----------|----------------|
| #28002 | `[codex] Send turn state through compact requests` | `codex-rs/core/src/session/turn.rs` | 跨 compact 请求传递 turn state |
| #27996 | `[codex] Send request-scoped turn state over WebSocket` | `codex-rs/core/src/session/turn.rs` | WebSocket 升级头无法表达 turn 生命周期；必须改成 request-scoped |

PR #27996 描述原文（直接引用）：

> "Turn state is scoped to one logical turn, but the WebSocket path currently exchanges it through upgrade headers, which are scoped to the physical connection. A connection may be reused across turns, so its handshake cannot represent the turn lifecycle reliably."

该描述准确命中 `turn-id-mvp` 提案中"按 turn 维度可观测 / 可关联"的真实需求：Turn 状态属于**逻辑 turn** 生命周期，**物理连接**（HTTP upgrade / WebSocket / SSE）可能被跨 turn 复用，因此 transport-level ID（upgrade header）不能稳定表达 turn 生命周期。

### codex 投资规模

codex 为 Turn 概念投入的工程量：

| 模块 | 行数 | 职责 |
|------|------|------|
| `codex-rs/core/src/session/turn.rs` | 2296 | Turn 核心状态机 + 状态转移 |
| `codex-rs/core/src/turn_timing.rs` | 391 | Turn 时间维度指标 |
| `codex-rs/core/src/turn_metadata.rs` | 349 | Turn 元数据（用户、模型、token） |
| `codex-rs/core/src/turn_diff_tracker.rs` | (未列出) | Turn 内 diff 追踪 |
| `codex-rs/core/src/state/turn.rs` | 241 | Turn 持久化状态 |
| `codex-rs/core/src/context/turn_aborted.rs` | (未列出) | Turn 中止事件 |

合计 3000+ 行。这是**工业级真实需求**的强证据——OpenAI 团队为该需求投入了多个模块、2296 行核心代码、3 个独立维度（timing / metadata / diff）。

### 本 change 的定位

本 change **不实施 TurnId MVP**。它是一个**元变更（meta-change）**，解决以下 3 个问题：

1. **记录触发事件**：codex PR #28002 / #27996 何时、由谁、为何触发了 `turn-id-mvp` 的解冻条件 #1
2. **重新评估冻结期**：3 个月硬延迟到 2026-09-13 是否仍合理？条件 #1 满足后是否应立即解冻？
3. **形式化解冻决策**：把"codex 触发的解冻事件"纳入 OpenSpec 元数据，避免未来误以为条件 #1 仍未满足

实施 TurnId MVP 仍归 `turn-id-mvp` change，本 change 只做"记录 + 评估 + 决策"三件事。

### 关键参考

- `openspec/changes/turn-id-mvp/proposal.md`（冻结的 MVP 提案）
- `openspec/changes/turn-id-mvp/design.md`（冻结的设计文档）
- `openspec/changes/turn-id-mvp/tasks.md`（冻结后不解冻的实施任务）
- `openspec/changes/turn-id-mvp/specs/turn-id-label/spec.md`（FROZEN 状态的 spec）
- `openspec/changes/turn-id-mvp/brainstorm.md`（4 派对抗性审查记录）

---

## Goals / Non-Goals

**Goals:**

- G1: 记录 codex PR #28002 / #27996 作为 `turn-id-mvp` 解冻条件 #1 已满足的证据
- G2: 重新评估 3 个月冻结期是否合理，**维持 3 个月不缩短**（保留"speculative architecture 应被推迟"项目原则）
- G3: 明确解冻后实施仍受 3 个前置条件门控（TokenUsage 收敛、turn_id 表示收敛、recovery path 显式化）
- G4: 标记 codex 设计为 reference（解冻后实施时可参考，不复制）
- G5: 本 change 0 代码变更

**Non-Goals:**

- N1: ❌ 本 change 不实施 TurnId MVP
- N2: ❌ 本 change 不修改 `turn-id-mvp` 目录（保持 FROZEN 状态）
- N3: ❌ 本 change 不创建 `crates/synthia-agent/src/turn.rs`
- N4: ❌ 本 change 不修改 `LoopContext` / `StreamBuilder` / `synthia-hook`
- N5: ❌ 本 change 不把 3 个月冻结期缩短为"立即解冻"或"1 个月后解冻"
- N6: ❌ 本 change 不引入完整 `Turn` struct / `TurnStatus` 枚举 / 新 `AgentEvent` 变体
- N7: ❌ 本 change 不复制 codex Turn 模型的任何代码（仅作 reference）
- N8: ❌ 本 change 不增加 turn_id 表示的种类（仍受"turn_id 表示必须先收敛到 1 个"前置条件门控）

---

## Decisions

### D1: 把本 change 定义为元变更（meta-change），不实施代码

- **选择**：本 change 仅做"记录 + 评估 + 决策"，不实施 TurnId
- **理由**：
  1. 实施仍归 `turn-id-mvp` change（其 tasks.md 已写好 ~20 行 MVP 任务）
  2. 3 个前置条件（TokenUsage 收敛 / turn_id 表示收敛 / recovery path 显式化）未完成时实施 MVP 仍会与 5 个 turn_id 表示产生第 6 个冲突
  3. 元变更让"codex 触发的解冻事件"有独立的 OpenSpec 记录，便于未来回溯
- **已考虑 alternative**：
  - **B. 直接修改 `turn-id-mvp` 目录** → 违反"Do NOT modify the existing `turn-id-mvp/` directory — it stays frozen"约束
  - **C. 合并到 `turn-id-mvp` 目录** → 污染 FROZEN 状态，违反 4 派审查达成的"冻结期不修改"共识

### D2: 维持 3 个月冻结期不缩短

- **选择**：解冻仍 2026-09-13，codex 触发的条件 #1 不缩短冻结期
- **理由**：
  1. **保留项目原则的克制**："speculative architecture 应被推迟"原则要求即使有外部证据，3 个月窗口本身也有"观察 codebase 状态变化"的价值
  2. **前置条件仍未完成**：TokenUsage 收敛 / turn_id 表示收敛 / recovery path 显式化任一未完成时实施 MVP 都会引入风险
  3. **codex 工业级证据已记录**：即使未来 3 个月内 codebase 状态变化大，codex PR #28002 / #27996 已永久证明"按 turn 维度查询"是真实工业级需求
  4. **避免"破窗效应"**：未来类似场景（外部 PR 触发条件）可参考本 change 的"维持冻结期"先例
- **已考虑 alternative**：
  - **B. 立即解冻** → 违反项目"speculative architecture 应被推迟"原则；失去 3 个月观察窗口
  - **C. 缩短到 1 个月** → 与"3 个月窗口本身有价值"的判断冲突；1 个月不足以观察 codebase 状态变化
  - **D. 缩短到 2 个月** → 折中但缺乏清晰理由；3 个月是项目工作流已有的周期

### D3: codex 设计仅作 reference，不复制

- **选择**：解冻后实施 TurnId MVP 时可参考 codex 设计，但 Synthia 仍走简化派 MVP（~20 行），不复制 codex 任何代码
- **理由**：
  1. **scope 差异**：codex Turn 是完整的 session 状态机（13 字段 + 状态机 + 4 事件 + 持久化），Synthia 只需 1 个 `TurnId(Uuid)` 类型作可观测性标签
  2. **依赖差异**：codex Turn 依赖 codex 内部 session persistence、compact 机制、recovery 路径；Synthia 的 MVP 不引入这些
  3. **成本差异**：codex 3000+ 行 vs Synthia ~20 行（150x 差距），复制 codex 是 YAGNI 反例
  4. **value 差异**：复制 codex 全量会引入"speculative architecture 应被推迟"项目原则拒绝的所有复杂度
- **已考虑 alternative**：
  - **B. 复制 codex 全量 3000+ 行** → 4 派审查 2026-06-13 一致拒绝
  - **C. 复制 codex 子集（如 metadata.rs）** → MVP 阶段不需要 metadata（LoopContext 已包含 session_id / iteration / token 等信息）

### D4: 仍受 3 个前置条件门控

- **选择**：解冻后实施 MVP 前必须先完成 3 个前置条件（任一未完成时不解冻）
- **理由**：
  1. **TokenUsage 收敛**（`unify-token-usage-types` change，已启动）：turn metadata 需要 TokenUsage 字段；若 TokenUsage 仍有 4 个不同定义，新代码会引入第 5 个
  2. **turn_id 表示收敛**（`turn-id-unify` change，未启动）：当前 5 个 turn_id 表示（`LoopContext.iteration: usize` / `AgentContext.turn_id: String` / `PrefixStabilityEvent.turn_id: u64` / `ApprovalRequest.turn_id: String` / 未来的 `TurnId(Uuid)`），新增 `TurnId` 会变成第 6 个
  3. **recovery path 显式化**（`recovery-path-explicit` change，未启动）：`builder.rs:355-363` 的 `continue` 修复前，turn_id 在 recovery 路径下行为未定义
- **已考虑 alternative**：
  - **B. 跳过前置条件直接实施 MVP** → 引入第 6 个 turn_id 表示，违反 R4 风险
  - **C. 把前置条件作为 MVP 的一部分** → 扩大 scope，违反"严格按简化派 MVP 实施"约束

### D5: 不修改 `turn-id-mvp` 目录

- **选择**：本 change 不修改 `openspec/changes/turn-id-mvp/` 下的任何文件
- **理由**：
  1. 任务约束明确要求"do NOT modify the existing `turn-id-mvp/` directory"
  2. 维持 FROZEN 状态的纯粹性，避免"边冻结边修改"的逻辑矛盾
  3. 本 change 的产出（proposal/design/tasks/spec）放在独立 `turn-id-unfreeze/` 目录，OpenSpec 元数据层是隔离的
- **已考虑 alternative**：
  - **B. 同步更新 `turn-id-mvp/brainstorm.md`** → 违反任务约束 + 4 派审查共识

### D6: 触发证据以 PR 链接 + commit hash 记录

- **选择**：本 change 的 proposal/design/tasks 中明确记录 codex PR #28002 / #27996 的 PR 号 + 改动文件路径
- **理由**：
  1. **可追溯性**：未来审阅者可点击 PR 链接验证 codex 工业级证据的真实性
  2. **可重现性**：codex commit hash 让"重置 trigger 状态"时可直接 grep codex git history
  3. **避免泛化**："codex 团队合并了 PR"比"听说 codex 也做了类似事"更有说服力
- **已考虑 alternative**：
  - **B. 仅文字描述无 PR 链接** → 证据强度弱，未来审阅者无法验证

---

## Risks / Trade-offs

### R1: codex 触发的解冻事件被误读为"立即实施"

- **Risk**: 审阅者看到 codex PR 后认为 TurnId MVP 应立即实施
- **Mitigation**:
  - proposal.md 明确写"3 个月冻结期不缩短"
  - tasks.md 的所有任务标注"仅记录 / 仅评估 / 0 代码变更"
  - spec 明确写 "implementation SHALL remain gated by 3 prerequisites"

### R2: 3 个月后 codebase 状态变化大，MVP 失去意义

- **Risk**: 2026-09-13 时 TokenUsage 收敛已完成，turn_id 表示已收敛到 1 个，recovery path 已显式化 —— 但 MVP 形态（仅 TurnId(Uuid)）已不足以表达 turn 维度
- **Mitigation**:
  - 解冻时重新走 4 派审查 + 苏格拉底式拆解
  - 如 MVP 不再足够，扩展为简化派 + codex metadata 子集
  - 永久归档到 `openspec/changes/archive/turn-id-mvp-expired/`

### R3: codex 后续 PR 引入 Turn 模型重大变更

- **Risk**: codex 在 2026-06-13 → 2026-09-13 之间又合并 Turn 相关 PR，引入新维度（如 turn-level compact 策略、turn-level cost attribution）
- **Mitigation**:
  - 监控 codex 后续 PR（每周 grep `codex-rs/core/src/session/turn.rs` history）
  - 若发现重大变更，触发本 change 的"二阶评估"（meta-meta-change）
  - 实施 MVP 时可参考新 codex 设计，但仍不复制

### R4: turn_id 表示从 5 个变 6 个

- **Risk**: `LoopContext.iteration: usize` / `AgentContext.turn_id: String` / `PrefixStabilityEvent.turn_id: u64` / `ApprovalRequest.turn_id: String` / 未来的 `TurnId(Uuid)`，新增 `TurnId` 变成第 6 个表示
- **Mitigation**:
  - 维持 D4（3 个前置条件门控）
  - `turn-id-unify` change 必须先完成
  - 解冻后实施时先收敛表示，再加 `TurnId`

### T1: 元变更 vs 实施变更的边界混淆

- **Trade-off**: 元变更形式（"记录 + 评估 + 决策"）比直接实施更轻量，但未来审阅者可能误以为"已经解冻了可以实施"
- **接受理由**: OpenSpec 状态机本身支持元变更；本 change 的 spec 明确写"implementation SHALL remain FROZEN"

### T2: codex 工业级证据 vs Synthia 简化派 MVP 的认知失调

- **Trade-off**: codex 投入 3000+ 行做完整 Turn 模型，Synthia 仅投入 ~20 行做 TurnId 标签；审阅者可能质疑"为什么 codex 那么多，Synthia 这么少"
- **接受理由**: scope 差异显著（codex 有 session persistence / compact 机制 / recovery 路径，Synthia 没有）；TurnId(Uuid) 作为可观测性标签已满足 Synthia 当前需求

---

## Migration Plan

本 change **不涉及部署变更**（0 代码变更）。

### 立即（2026-06-13 当日）

1. 创建 `openspec/changes/turn-id-unfreeze/` 目录及 4 个 artifact
2. 提交 commit：`docs(openspec): record turn-id-mvp unfreeze trigger (codex PR #28002 + #27996)`
3. 不实施任何代码

### 冻结期（2026-06-13 → 2026-09-13）

1. `turn-id-mvp` 仍保持 FROZEN 状态
2. 监控 codex 后续 PR（每周一次 `git -C codex fetch && git log --oneline --since=... -- codex-rs/core/src/session/turn.rs`）
3. 监控 3 个前置条件完成进度：
   - `unify-token-usage-types` change（已启动）
   - `turn-id-unify` change（未启动）
   - `recovery-path-explicit` change（未启动）
4. 等待 2026-09-13 硬解冻日 / 提前满足前置条件

### 解冻后实施（仍归 `turn-id-mvp` change）

1. 仍按 `turn-id-mvp/tasks.md` 的 2.1-2.6 节执行（创建 `TurnId(Uuid)` 类型、`LoopContext` 加字段、`StreamBuilder` 替换字符串、验证）
2. **可选子任务**（不强制）：阅读 codex 2296 行 `turn.rs` 后写 "synthia-vs-codex Turn design notes" markdown（仅作 reference notes，不复制任何代码）
3. 提交 commit：`feat(agent): introduce TurnId(Uuid) as MVP turn label (~20 lines)`
4. 推送，等待 CI 通过
5. `openspec archive turn-id-mvp`

### 6 个月硬截止（2026-12-13）

如 2026-09-13 时前置条件未完成：
1. `turn-id-mvp` 继续 FROZEN
2. 评估 codex 后续 PR 是否引入新维度
3. 2026-12-13 时归档到 `openspec/changes/archive/turn-id-mvp-expired/`
4. `turn-id-label` capability 标注 "deferred indefinitely"

### Rollback 策略

- 本 change 0 破坏性变更
- 如发现元变更记录有误，编辑 proposal.md / design.md 后重新提交 commit 即可
- 冻结期内不解冻，等下次评估

### 验收条件（本 change）

- [ ] `openspec/changes/turn-id-unfreeze/` 含 4 个 artifact（proposal.md / design.md / tasks.md / specs/turn-id-unfreeze/spec.md）
- [ ] `openspec validate turn-id-unfreeze --type change` 通过
- [ ] 所有 Requirement 第一句包含 SHALL 或 MUST
- [ ] 所有 Requirement 至少 1 个 Scenario
- [ ] proposal.md 明确引用 codex PR #28002 / #27996
- [ ] `turn-id-mvp/` 目录未被修改
- [ ] 0 代码变更（`git diff` 仅限 `openspec/changes/turn-id-unfreeze/`）

### 验收条件（解冻后实施，仍归 `turn-id-mvp`）

- [ ] `crates/synthia-agent/src/turn.rs` 文件 < 30 行
- [ ] `LoopContext` 字段数量 + 1
- [ ] `builder.rs:327` 字符串构造替换
- [ ] `cargo check --workspace` 0 错误
- [ ] `cargo test --workspace` 100% 通过
- [ ] `cargo clippy` 0 警告
- [ ] `grep "pub struct Turn\b" crates/` 返回 0 行
- [ ] `grep "TurnStatus"` 返回 0 行
- [ ] `grep "TurnStarted\|TurnCompleted\|TurnFailed\|TurnAborted"` 返回 0 行

---

## Open Questions

### Q1: 解冻后是否真的需要 "synthia-vs-codex Turn design notes"？

- 当前决策：可选，不强制
- 替代方案：必做（强制 read codex `turn.rs` 全量后再实施 MVP）
- 待决原因：codex 2296 行阅读成本 ~2-4 小时；MVP 实施成本 ~20 行；ROI 不确定

### Q2: 监控 codex 后续 PR 的频率如何设定？

- 当前决策：每周一次（如有重大 PR 则提前触发二阶评估）
- 替代方案：每日（高开销）；每月（响应慢）
- 待决原因：codex 团队迭代速度未知

### Q3: 如果 codex 在冻结期内引入 Turn 持久化（turns.jsonl 类似），Synthia 是否也该引入？

- 当前决策：不引入（与"3 个月冻结期不缩短"+"MVP 不扩展"约束一致）
- 替代方案：codex 引入即解冻（响应外部信号）
- 待决原因：MVP scope 应由 Synthia 内部需求驱动，不应被 codex 牵引

### Q4: 本 change 是否需要 `.openspec.yaml`（schema 文件）？

- 当前决策：暂不创建
- 替代方案：创建 `schema: superpowers-bridge`（与 `turn-id-mvp` 一致）
- 待决原因：本 change 是元变更，无 superpowers-bridge 需求
