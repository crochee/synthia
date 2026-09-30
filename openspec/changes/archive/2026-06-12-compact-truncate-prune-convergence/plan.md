# Plan: Compact / Truncate / Prune Convergence + agent_tools.rs Split

> 输入：[`brainstorm.md`](./brainstorm.md) + [`design.md`](./design.md) + [`tasks.md`](./tasks.md) + [`proposal.md`](./proposal.md)
> 输出：分阶段实施计划 + 风险缓解 + 验证策略
> 关联：[`specs/*.md`](./specs/)

---

## 1. 实施原则

| 原则 | 体现 |
|------|------|
| **TDD 优先** | 每个能力先写失败测试，再写实现 |
| **5 阶段串行** | P0 → P1 → P2 → P3 → Verify，**不并发**（避免 merge 冲突） |
| **独立 commit** | 5 个 commit 各自可单独 revert，回应 P10/R6 |
| **零行为破坏** | P3 shim 重导出 100% 向后兼容；D2 字段 `serde(default)` |
| **质量门** | 每个阶段结束前必须 clippy 0 warning + 全测通过 |

---

## 2. 阶段详解

### 阶段 1：P0 - bash UTF-8 panic 修复

**目标**：消除生产 P0 风险
**工作量**：0.5 天
**风险等级**：低（仅替换 2 行 + 加辅助函数）

**前置条件**：
- 已读 [`specs/bash-utf8-safe-truncate/spec.md`](./specs/bash-utf8-safe-truncate/spec.md)
- 确认 [`bash_tool.rs:125, 136`](file:///home/crochee/workspace/synthia/crates/synthia-exec/src/bash_tool.rs#L125) 是 panic 源

**关键步骤**：
1. 写 regression test（中文 + emoji 混合输入，切点强制落中段）
2. 实现 `cap_to_char_boundary` 辅助函数
3. 替换 2 处 `s.truncate(...)` 调用
4. 验证 `cargo test -p synthia-exec` 全绿

**commit**：`fix(exec): cap_to_char_boundary for bash_tool UTF-8 panic`

**回滚**：单 commit revert，无副作用

---

### 阶段 2：P1 - time.compacted 幂等机制

**目标**：引入 `tool_result_cleared_at` 幂等标记 + `prune()` 单遍扫描函数
**工作量**：1.5 天
**风险等级**：中（触及核心 Message 类型 + 渲染层）

**前置条件**：
- P0 已 commit
- 已读 [`specs/prune-idempotent-marker/spec.md`](./specs/prune-idempotent-marker/spec.md)
- 已确认 D2 字段位置选择（`synthia_context::Message`）

**关键步骤**：
1. **P1.1** `Message` 加 `#[serde(default)] tool_result_cleared_at: Option<Instant>` 字段
   - 验证：序列化/反序列化双向兼容测试
2. **P1.2** `pruning.rs` 新增 `prune()` 函数 + `PRUNE_PROTECT_TOKENS` 常量
   - 验证：6+ 单元测试覆盖单遍扫描、幂等停止、40K 保护
3. **P1.3** 渲染层（`truncate_messages`）识别 `tool_result_cleared_at` → 输出占位符
   - 验证：占位符渲染测试

**commit**：`feat(context): Message.tool_result_cleared_at + prune() with PRUNE_PROTECT=40K`

**回滚**：单 commit revert；D2 字段 `serde(default)` 旧消息无破坏

---

### 阶段 3：P2 - apply_compaction 单遍化 + 锚定

**目标**：消除 3×O(n) 重复扫描 + 加 `<previous-summary>` 决策连续性
**工作量**：1 天
**风险等级**：中（修改 compactor 核心逻辑）

**前置条件**：
- P1 已 commit（`prune()` 是兄弟能力）
- 已读 [`specs/compaction-single-pass/spec.md`](./specs/compaction-single-pass/spec.md) 和 [`specs/context-compaction/spec.md`](./specs/context-compaction/spec.md)
- 已读 [`compactor.rs:818-876`](file:///home/crochee/workspace/synthia/crates/synthia-context/src/compaction/compactor.rs#L818-L876) 当前实现

**关键步骤**：
1. **P2.1** `apply_compaction` 重构为单次 `estimate_tokens`
   - 验证：所有现有 compactor 单测通过（行为锁定）
2. **P2.2** `compact_level1` 接受 `previous_summary: Option<&str>` 参数
   - 验证：prompt 模板分支单测
3. **P2.3** `apply_compaction` 传递 `previous_summary` 给 L1，成功后回写
   - 验证：连续 2 次 apply 测试
4. **P2.4** `recovery_cascade::try_l4_compact` 共享 `original_tokens`
   - 验证：L4 触发时 estimate 只 1 次

**commit**：`perf(context): apply_compaction single-pass + previous_summary anchor`

**回滚**：单 commit revert；行为变化通过现有测试锁定

---

### 阶段 4：P3 - agent_tools.rs 拆分

**目标**：1300+ 行单文件 → 7 个 < 300 行子文件，零行为变化
**工作量**：1 天
**风险等级**：低（纯文件拆分 + shim 重导出）

**前置条件**：
- P0/P1/P2 已 commit（公共 API 已稳定）
- 已读 [`specs/agent-tools-split/spec.md`](./specs/agent-tools-split/spec.md)

**关键步骤**：
1. **P3.1** 创建目录 `tools/agent/` + `tools/agent/tools/`
2. **P3.2** 拆分 7 个子文件（D6 树状结构）
3. **P3.3** `agent_tools.rs` 改为 `pub use crate::tools::agent::*;` shim
4. **P3.4** 验证 `cargo test --workspace` 全绿 + `wc -l` < 300 行/文件

**commit**：`refactor(agent): split agent_tools.rs into 7 modules (1300→<300 lines each)`

**回滚**：单 commit revert；shim 公开 API 无破坏

---

### 阶段 5：Verify - 集成测试 + 收尾

**目标**：端到端验证 4 个能力联动 + 满足 Rust 编码规范
**工作量**：0.5 天

**前置条件**：P0/P1/P2/P3 全部 commit

**关键步骤**：
1. **V.1** 写 4 个端到端场景的集成测试
2. **V.2** `cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features --tests --all`
3. **V.3** 修复所有 clippy 警告
4. **V.4** 跑全 workspace 测试套件

**commits**：
- `test: integration tests for compact/truncate/prune pipeline`
- `chore: cargo fmt + clippy cleanup`（如有需要）

**回滚**：测试 commit 可单独 revert，不影响生产代码

---

### 阶段 6：Archive - 归档 + 复盘

**目标**：完成 change 生命周期
**工作量**：0.25 天

**关键步骤**：
1. 同步 4 个新 spec + 2 个修改的 spec 到 `openspec/specs/`
2. `openspec archive compact-truncate-prune-convergence --yes`
3. 写 `retrospective.md`（commit hash 列表 + 偏差 + 经验 + follow-up）

---

## 3. 风险缓解矩阵

| ID | 风险 | 缓解策略 | 验证点 |
|----|------|----------|--------|
| **R1** | UTF-8 切点"多走几步" 字节数不准 | regression test 断言"实际 ≤ max_output_length" | P0.3.2 |
| **R2** | `tool_result_cleared_at` 序列化破坏旧消息 | `#[serde(default)]` + 反序列化兼容测试 | P1.1.3 |
| **R3** | compactor 退化路径行为变化 | L1→L2→L3 顺序保留 + 所有现有单测通过 | P2.5.1 |
| **R4** | `previous_summary` 体积膨胀 | 调用方在成功后截断 4K（OQ4 留作 follow-up） | 不在本 change |
| **R5** | `agent_tools::*` use path 漏掉 | shim 重导出 + workspace 全绿 | P3.4.2 |
| **R6** | 5 阶段并行 merge 冲突 | 串行顺序，tasks.md 强制 | 调度策略 |
| **R7** | `prune()` 函数位置分歧 | OQ1 已确认放 `pruning` 子模块 | 实施一致 |
| **R8** | shim 重导出 indirection 审美 | 编译期 `pub use` 0 损耗 | 性能无影响 |

---

## 4. 质量门（每个阶段必须通过）

| 阶段 | 质量门 | 通过标准 |
|------|--------|----------|
| **P0** | 单元测试 | `cargo test -p synthia-exec` 全绿 |
| **P0** | Clippy | `cargo clippy -p synthia-exec --all-targets` 0 warning |
| **P1** | 序列化兼容 | 旧 JSON 反序列化无破坏 |
| **P1** | 单元测试 | `cargo test -p synthia-context` 全绿 |
| **P1** | 幂等性 | 连续 2 次 `prune()` 第二次 stats=0 |
| **P2** | 行为锁定 | 所有现有 compactor 单测 100% 通过 |
| **P2** | 性能 | benchmark 显示 3×O(n) → 1×O(n) |
| **P3** | 行为锁定 | `cargo test --workspace` 全绿 |
| **P3** | 文件大小 | 7 个文件均 < 300 行 |
| **Verify** | 集成测试 | 4 个端到端场景全过 |
| **Verify** | Clippy | `cargo clippy --all-targets --all-features --tests --all` 0 warning |
| **Verify** | Format | `cargo +nightly fmt --all` 无 diff |
| **Archive** | Spec 同步 | 6 个 spec 文件已就位 |
| **Archive** | 复盘 | retrospective.md 完整 |

---

## 5. 调度策略

### 串行约束

- 5 阶段**严格串行**，禁止并发 subagent
- 原因：P1/P2/P3 都触及 `synthia_context::compaction` 和 `synthia-agent::tools`，并发会冲突
- 例外：P0 可与 P1 准备（仅读，不改文件）并行

### 单 subagent 模式

- 推荐 1-2 人串行实施
- 每阶段结束前必须 0 warning + 全测通过
- 失败立即回滚到上一个 commit 状态

### 中断恢复

- 每阶段独立 commit，意外中断可恢复到上一 commit
- 5 个 commit 互不依赖
- 任何中间状态可单独 revert

---

## 6. 验收标准（Definition of Done）

本 change 完成的标志：

- [ ] 5 个 commit 全部合并到目标分支
- [ ] 4 个新 spec + 2 个修改 spec 已同步到 `openspec/specs/`
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --all-targets --all-features --tests --all` 0 warning
- [ ] `cargo +nightly fmt --all` 无 diff
- [ ] `openspec archive` 成功执行
- [ ] `retrospective.md` 已写
- [ ] Bash UTF-8 panic 风险消除（regression test 覆盖）
- [ ] `agent_tools.rs` 拆分后每个文件 < 300 行
- [ ] 公开 API 100% 向后兼容
- [ ] Follow-up 列表已记录到 retrospective

---

## 7. 变更影响面

### 受影响 crate

| Crate | 改动类型 | 风险 |
|-------|----------|------|
| `synthia-exec` | 新增辅助函数 + 2 行替换 | 低 |
| `synthia-context` | 新增字段 + 新增函数 + 渲染层分支 + compactor 重构 | 中 |
| `synthia-agent` | 新增 error_recovery 共享 + 拆分 agent_tools | 中 |

### 不受影响 crate

- `synthia-provider`：保持现状
- `synthia-session`：保持现状
- `synthia-cli`：仅 `agent_tools::*` 路径重导出，无功能变化
- `synthia-server`：保持现状

### 兼容性

- **公开 API**：100% 兼容（shim 重导出 + `serde(default)`）
- **JSON Schema**：新增可选字段，旧消息无破坏
- **配置**：本 change 不引入新 config 字段（PRUNE_PROTECT 写死为常量 40K）

---

## 8. 实施时间线（甘特图）

```
Day 1 (P0)   |█ bash UTF-8 fix + regression test
Day 2-3 (P1) |██ Message 字段 + prune() + 渲染层
Day 3-4 (P2) |█ compactor 单遍化 + previous_summary
Day 4-5 (P3) |█ agent_tools 拆分
Day 5 (V)    |█ 集成测试 + fmt + clippy
Day 5 (A)    |█ archive + retrospective
```

合计：~4.5 工作日（5 个独立 commit）

---

## 9. 后续衔接

完成本 change 后，下一处高价值差距候选（来自 2026-06-12 评估）：

1. **C2 候选**：Codex session/Turn model（**已推迟 6 个月**，等待具体使用场景）
2. **C3 候选**：OpenCode v2 + ACP（**已推迟 6 个月**，等待具体使用场景）
3. **R1 大重构**：codex 风格 modular tool spec/handler
4. **FU.1-2**：OQ4 summary 截断 / OQ5 prune 集成

每次新 change 都应经过**多专家对抗性评估**确定优先级，避免过早抽象。

