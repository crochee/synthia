# turn-id-unify Brainstorm

> **状态**：研究阶段（4 派对抗性审查 + 共识形成）
> **创建**：2026-06-13
> **关联**：`turn-id-mvp` 3 个正交前置任务之一（与 `unify-token-usage-types`、`recovery-path-explicit` 并列）

---

## Q1（4 派共审题）：当前 4 个 turn_id 表示的真实数据流是什么？

### A1（事实层 + 数据流图）

4 个 turn_id 表示分 3 类，本质是 `LoopContext.iteration: usize` 的 3 个不同视图：

| # | 表示 | 类型 | 构造 | 使用方 | 位置 |
|---|------|------|------|--------|------|
| 1 | `LoopContext.iteration` | `usize` | 每次 LLM 调用 `+= 1` | 内部 agent 循环控制 | `loop_context.rs:11` |
| 2 | `AgentContext.turn_id` | `String` | `format!("turn-{}", iteration)` | hook 回调（`on_before_llm` 等） | `builder.rs:360` |
| 3 | `PrefixStabilityEvent.turn_id` | `u64` | `iteration as u64` | 前缀稳定性遥测事件 | `builder.rs:503` |
| 4 | `ApprovalRequest.NetworkAccess.turn_id` | `String` | `String` 字面量（测试中用 `"t"`） | Guardian 网络访问审批 | `approval_request.rs:33` |

**数据流图（5 个调用点，4 个类型）**：

```
LoopContext.iteration: usize
    ├── (builder.rs:360) format!("turn-{}", iteration) ──→ AgentContext.turn_id: String
    │                                                          │
    │                                                          ↓
    │                                                     on_before_llm() hook callbacks
    │                                                          │
    │                                                          ↓
    │                                              (审批触发时) ApprovalRequest::NetworkAccess.turn_id: String
    │
    ├── (builder.rs:376) iteration as u64 ──→ PrefixTracker.record_pre(turn_id: u64)
    │                                                          │
    │                                                          ↓
    └── (builder.rs:503) iteration as u64 ──→ PrefixStabilityEvent.turn_id: u64
```

**关键观察**：

- **#1 是 source of truth**，#2-#4 都是 `iteration` 的派生视图
- **#2 / #3 是同一 iteration 的不同表示**（`String` vs `u64`），构造点相邻（`builder.rs:360` vs `:503`）
- **#4 在 5 个 `ApprovalRequest` 变体中只有 `NetworkAccess` 有 `turn_id` 字段**，其他 4 个变体（`Shell` / `ExecCommand` / `ApplyPatch` / `McpToolCall`）都没有
- **#4 在测试中用 `"t"` 字面量**，没有任何代码从 `AgentContext.turn_id` 传递到 `ApprovalRequest.NetworkAccess.turn_id` —— **#4 与 #2-#3 没有实际数据流耦合**

**怀疑派裁决**：#4 实际上是个"未连接的孤儿字段"——既不在生产代码路径中流通，也没有跨变体一致性。

---

## Q2（架构派提问）：这 4 个表示是否构成"重复实现"？

### A2（架构派分析）

参考项目记忆硬约束：`"Duplicate implementations must be removed"`。

**但**：4 个表示**不是重复实现**（identical implementations），而是**不同视图**（different views of the same concept）。

| 维度 | #1 `iteration: usize` | #2 `String` | #3 `u64` | #4 `String` |
|------|----------------------|-------------|----------|-------------|
| 用途 | 内部循环计数器 | hook 序列化 | 遥测事件字段 | 审批请求字段 |
| 是否 alloc | 否 | 是（format!） | 否 | 是（构造时） |
| 序列化 | 否 | 是 | 否 | 是 |
| 跨进程 | 否 | 是（API） | 否 | 是（Guardian） |

**架构派裁决**：
- ❌ 不是"重复"（4 个表示没有同形代码）
- ✅ 是"概念同源"（都标识同一个"turn"概念）
- ⚠️ 是"字段不一致"（同样是 iteration 视图，#2 用 `String("turn-{}")`，#3 用 `u64`）

**架构派结论**："收敛"应该是**让 4 个表示共享同一个"turn_id 概念"**，而不是**让 4 个表示合并成 1 个类型**。前者是抽象层问题，后者是类型层问题。

---

## Q3（生产派提问）：4 个表示中哪些有"按 turn 维度查询"的真实 caller？

### A3（生产派 grep + 行为分析）

```bash
# 5 处 turn_id 引用（生产代码）
crates/synthia-agent/src/stream_builder/builder.rs:360   # format!("turn-{}", iteration) → AgentContext
crates/synthia-agent/src/stream_builder/builder.rs:376   # iteration as u64 → PrefixTracker
crates/synthia-agent/src/stream_builder/builder.rs:500   # iteration as u64 → record_post
crates/synthia-agent/src/stream_builder/builder.rs:503   # iteration as u64 → emit_stability_event
crates/synthia-agent/src/agent.rs:126                    # "shutdown-turn" 字面量（shutdown 阶段）
```

**生产派观察**：
- **#1 #2 #3 全部 4 处都源自 `builder.rs`**，调用点同文件
- **#4 在生产代码中无 caller**（测试用 `"t"` 字面量）
- **#2-#4 之间无跨边界数据流**（不传递、不关联、不 join）

**生产派裁决**：
- "按 turn 维度查询"在当前 codebase 中**不存在**真实 caller
- codex PR #27996/#28002 提供的工业级证据是**遥测 + 持久化**场景，不是"类型收敛"场景
- 如果 turn-id-mvp 真的解冻，`TurnId(Uuid)` 引入会自然替换 #2-#3（#2 是 `String` 格式化的目标，#3 是 `u64` 投射的目标）

---

## Q4（简化派提问）：如果今天不实施 turn-id-unify，3 个月后 codebase 会变成什么样？

### A4（简化派反事实推演）

**情景 1：什么都不做，3 个月后 codebase 状态**
- #1 #2 #3 仍然各自独立，没有真实 bug
- #4 仍然是孤儿字段（可能有人加新 variant 让 #4 派上用场，但与"turn_id 收敛"目标无关）
- 2026-09-13 评估 `turn-id-mvp` 解冻时，仍会发现"5 个 turn_id 表示"问题
- 仍需实施 turn-id-unify（但 3 个月后 codebase 可能有更多 turn_id 衍生表示）

**情景 2：实施 turn-id-unify（任何方案），3 个月后 codebase 状态**
- 多了一次小重构（无论方案大小）
- 如果方案引入 `TurnId(Uuid)`，则与 `turn-id-mvp` 的 `TurnId(Uuid)` 冲突（两个 `TurnId` 类型）→ 需协调
- 如果方案仅文档化（类型别名），则无实际收益，仅有命名一致性

**简化派裁决**：**最小可行方案**（minimal viable change）是优先项；任何引入"5.5 个表示"风险的方案都要拒绝。

---

## Q5（4 派综合）：推荐的实施方案是什么？

### 综合方案对比表

| 方案 | 变更范围 | 风险 | 收益 | 4 派评价 |
|------|---------|------|------|----------|
| **A. 仅文档化（不实施代码）** | 0 行 | 0 | 一致性注释 | 怀疑派 ✅ / 架构派 ⚠️ / 生产派 ✅ / 简化派 ✅ |
| **B. 集中格式化函数** | ~10 行 | 极低 | 命名一致性 | 怀疑派 ✅ / 架构派 ✅ / 生产派 ✅ / 简化派 ✅ |
| **C. 删除 #4 孤儿字段** | ~5 行 | 极低 | 消除孤儿代码 | 怀疑派 ✅ / 架构派 ✅ / 生产派 ✅ / 简化派 ✅ |
| **D. 引入 `TurnId(Uuid)` 提前** | ~80 行 | 中（与 turn-id-mvp 协调） | 真正类型统一 | 怀疑派 ❌ / 架构派 ⚠️ / 生产派 ❌ / 简化派 ❌ |
| **E. 类型别名 `type TurnId = u64`** | 1 行 | 0 | 仅命名（无实际统一） | 怀疑派 ⚠️ / 架构派 ❌ / 生产派 ⚠️ / 简化派 ⚠️ |

### 4 派共识（推荐路径）

**D1（共识）**：实施方案 = **B + C 组合**

理由：
1. **B（集中格式化函数）** 把 `format!("turn-{}", iteration)` 集中到 `synthia_agent::turn_id::format_turn_id(iter: usize) -> String`，单点定义替换 #2 的所有构造点
2. **C（删除 #4 孤儿字段）** 删除 `ApprovalRequest::NetworkAccess.turn_id: String`，因为：
   - 5 个 variant 中只有 1 个有该字段（不一致）
   - 测试用 `"t"` 字面量（无生产 caller）
   - Guardian 决策（`assess_risk`、`make_guardian_decision`）**不读取 `turn_id` 字段**（grep 0 处 match 模式）
3. **B + C 总变更 < 15 行**，零新类型，零新依赖，零与 `turn-id-mvp` 的协调
4. **#1 #3 保留 `usize` / `u64`**，因为：
   - `#1` 是 internal 计数器（不应受外部类型影响）
   - `#3` 是 `PrefixStabilityEvent` 内部字段，与 `PrefixTracker` 配套（不暴露 hook）
5. **#2 升级时机**：当 `turn-id-mvp` 解冻时，`AgentContext.turn_id: String` 升级为 `Option<TurnId>`（与 `loop_context.current_turn_id: Option<TurnId>` 同步）

### 风险评估

**R1**：B + C 实施后，3 个月后 codebase 状态是"#1 #3 仍是 usize/u64，#2 通过 `format_turn_id()` 集中构造，#4 已删除"——`turn-id-mvp` 解冻时仍需把 #2 从 `String` 升级为 `TurnId`，但工作量 < 5 行。

**接受**：3 个月后工作量从"无前置"变成"5 行前置"，仍远低于 D 方案的 80 行 + 协调成本。

**R2**：B + C 改 `ApprovalRequest::NetworkAccess` 的字段（删除），属于**破坏性变更**——如果外部用户构造 `NetworkAccess` 时传了 `turn_id`，会编译失败。

**接受**：项目记忆硬约束 `"Module split pattern: keep original file as 1-line pub use sub_module::* shim, never delete the original path"` 适用于类型定义，不适用于字段删除。`turn_id` 字段当前在 `synthia-guardian` 内部使用，外部 grep 0 处使用 `ApprovalRequest::network_access(id, turn_id, ...)` 6 参版本。

---

## Q6（执行问题）：B + C 实施的具体步骤

### 步骤清单

```bash
# 1. 新增集中格式化函数
echo "pub fn format_turn_id(iter: usize) -> String { format!(\"turn-{}\", iter) }" \
    >> crates/synthia-agent/src/turn_id.rs
# (或追加到 crates/synthia-agent/src/lib.rs)

# 2. 替换 builder.rs:360 的 format!
#    from: format!("turn-{}", ctx.iteration)
#    to:   crate::turn_id::format_turn_id(ctx.iteration)

# 3. 删除 ApprovalRequest::NetworkAccess.turn_id
#    from:
#        NetworkAccess { id, turn_id, target, host, protocol, port }
#    to:
#        NetworkAccess { id, target, host, protocol, port }
#    + 修改 ApprovalRequest::network_access() 构造函数（删除 turn_id 参数）
#    + 修改所有调用方（grep 0 处生产 caller，但需更新 guardian_coordinator.rs:112 的测试）

# 4. cargo check --workspace 验证
# 5. cargo test --workspace 验证
# 6. cargo fmt + cargo clippy 修复
# 7. OpenSpec artifacts (proposal/design/spec/tasks/plan/verify/retro)
# 8. 提交 + 归档
```

**变更文件清单**（预估）：
- `crates/synthia-agent/src/turn_id.rs` (新增, ~5 行)
- `crates/synthia-agent/src/stream_builder/builder.rs` (1 行 format 替换)
- `crates/synthia-guardian/src/approval_request.rs` (删除 1 字段 + 修改 1 构造函数)
- `crates/synthia-guardian/src/guardian_coordinator.rs` (更新 1 测试调用)
- `crates/synthia-guardian/tests/*` (grep 是否有 turn_id 字面量, 0 处)
- `openspec/changes/turn-id-unify/` (8 个 artifacts)

**总代码变更**：< 15 行

---

## 4 派决议汇总

| 决议 | 内容 | 投票 |
|------|------|------|
| **D1** | 实施方案 = B + C（集中格式化 + 删除孤儿字段） | 4/4 一致 |
| **D2** | 不引入 `TurnId(Uuid)`（留给 `turn-id-mvp` 解冻时） | 4/4 一致 |
| **D3** | 不实施类型别名（type TurnId = u64 无实际收益） | 4/4 一致 |
| **D4** | 保留 `LoopContext.iteration: usize` 和 `PrefixStabilityEvent.turn_id: u64` | 4/4 一致 |
| **D5** | 集中格式化函数路径：`synthia-agent::turn_id::format_turn_id` | 4/4 一致 |
| **D6** | `ApprovalRequest::NetworkAccess.turn_id` 字段删除（破坏性变更） | 4/4 一致（接受风险） |

---

## 后续路径

下一步：
1. 用 `4 派共识 D1-D6` 写 `proposal.md`（Why / What Changes / Capabilities / Impact）
2. 用 `D1-D6` 写 `design.md`（Context / Goals / Decisions / Risks / Migration）
3. 写 `specs/turn-id-unify/spec.md`（5-6 个 ADDED Requirements）
4. 写 `tasks.md`（7 个 task group）
5. 写 `plan.md` / `verify.md` / `retrospective.md`
6. 提交 + 归档

预计代码变更：< 15 行
预计 OpenSpec 文档：~400 行
预计完成时间：1 个 session
