# Tasks: Compact / Truncate / Prune Convergence + agent_tools.rs Split

> 输入：[`design.md`](./design.md)（架构决策 + 迁移路径）
> 输出：5 阶段可独立 commit 的微任务清单
> 关联：[`proposal.md`](./proposal.md)、[`specs/*.md`](./specs/)

---

## 阶段总览

| Phase | 范围 | 估算 | commit 前缀 | 关键验证 |
|-------|------|------|-------------|----------|
| P0 | bash UTF-8 panic 修复 | 0.5 天 | `fix(exec)` | `cargo test -p synthia-exec` + regression test |
| P1 | `tool_result_cleared_at` + `prune()` | 1.5 天 | `feat(context)` | `cargo test -p synthia-context` + 序列化兼容 |
| P2 | `apply_compaction` 单遍化 + `<previous-summary>` | 1 天 | `perf(context)` | 现有 compactor 单测全绿 + benchmark |
| P3 | `agent_tools.rs` 拆分 | 1 天 | `refactor(agent)` | `cargo test --workspace` 全绿 |
| Verify | 集成测试 + clippy + fmt | 0.5 天 | `test` + `chore` | 端到端 + 0 warning |

合计：~4.5 天（5 个独立 commit）

---

## 阶段 P0：bash UTF-8 panic 修复（[D1](./design.md#L59-L69)）

> Spec: [`bash-utf8-safe-truncate/spec.md`](./specs/bash-utf8-safe-truncate/spec.md)
> 风险：**生产 P0**（panic on multi-byte UTF-8 in bash output）

### P0.1 单元测试先行

- [x] **P0.1.1** 在 `crates/synthia-exec/tests/bash_utf8_panic.rs` 写 regression test：
  - 输入：含中文（3 字节 UTF-8）+ emoji（4 字节 UTF-8）混合的 stdout
  - 强制 `max_output_length` 切点落在多字节字符中段
  - 期望：不 panic + 实际截断字节数 ≤ `max_output_length` + 保留 `[stdout truncated at N bytes]` marker
- [x] **P0.1.2** 同样测试覆盖 stderr 路径

### P0.2 修复实现

- [x] **P0.2.1** 在 `crates/synthia-exec/src/bash_tool.rs` 新增私有函数 `fn cap_to_char_boundary(s: &mut String, max_bytes: usize)`：
  - 接受 `&mut String` 和 `max_bytes`
  - 若 `s.len() <= max_bytes` → no-op
  - 否则在 `[0, max_bytes]` 区间内用 `s.is_char_boundary(idx)` 反向扫描找最近 char boundary
  - `s.truncate(boundary_idx)` 截断
- [x] **P0.2.2** 替换 `bash_tool.rs:125` 的 `s.truncate(self.max_output_length)` → `cap_to_char_boundary(&mut s, self.max_output_length)`
- [x] **P0.2.3** 替换 `bash_tool.rs:136` 的同样调用

### P0.3 验证

- [x] **P0.3.1** `cargo test -p synthia-exec` 全绿（58 lib + 5 integration）
- [x] **P0.3.2** 新增的 `bash_utf8_panic` regression test 单独跑也绿
- [x] **P0.3.3** `cargo clippy -p synthia-exec --all-targets --all-features --tests` 无 warning

### P0.4 提交

- [x] **P0.4.1** `git add` 对应文件
- [x] **P0.4.2** `git commit -m "fix(exec): cap_to_char_boundary for bash_tool UTF-8 panic"`（参照仓库 commit 风格）— commit `1884e9f`

---

## 阶段 P1：time.compacted 幂等机制（[D2](./design.md#L71-L86) + [D3](./design.md#L87-L110)）

> Specs:
> - [`prune-idempotent-marker/spec.md`](./specs/prune-idempotent-marker/spec.md)
> - [`tool-output-truncate/spec.md`](./specs/tool-output-truncate/spec.md)（扩展）
> 基础：为 P2/P3 提供幂等标记 + 单遍扫描原语

### P1.1 `Message::tool_result_cleared_at` 字段

- [x] **P1.1.1** 在 `crates/synthia-context/src/lib.rs` 的 `Message` 结构体加 `#[serde(default)] pub tool_result_cleared_at: Option<Instant>` 字段
- [x] **P1.1.2** 单元测试：序列化"已清除"消息 → JSON 含 ISO8601 时间戳
- [x] **P1.1.3** 单元测试：反序列化"无 `tool_result_cleared_at` 字段的旧 JSON" → 成功 + 字段为 `None`（向后兼容）
- [x] **P1.1.4** 单元测试：反序列化"含 `tool_result_cleared_at` 字段的新 JSON" → 成功 + 字段为 `Some(...)`

### P1.2 `prune()` 函数

- [x] **P1.2.1** 在 `crates/synthia-context/src/pruning.rs` 新增：
  - `pub const PRUNE_PROTECT_TOKENS: u32 = 40_000;`
  - `pub struct PruneStats { pruned_count: usize, kept_tokens: u32 }`
  - `pub fn prune(messages: &mut Vec<Message>, protect_tokens: u32) -> PruneStats`
- [x] **P1.2.2** 实现 D3 算法（单遍反向扫描）：
  - 维护 `total_kept` 累加器
  - 遍历 `messages.iter().rev()`
  - 遇到 `tool_result_cleared_at.is_some()` → 立即 `break`（幂等停止）
  - role=Tool 且 `total_kept + token_estimate(msg) > protect_tokens` → 标记 `tool_result_cleared_at = Some(Instant::now())`
  - 否则累加 `total_kept`
- [x] **P1.2.3** 单元测试：空消息列表 → `PruneStats { 0, 0 }`
- [x] **P1.2.4** 单元测试：所有消息都在 40K 内 → 0 个被 prune
- [x] **P1.2.5** 单元测试：超出 40K → 超出部分被 prune（从尾部最老开始）
- [x] **P1.2.6** 单元测试：再次调用 → 遇到已 cleared 立即停止（幂等）
- [x] **P1.2.7** 单元测试：PRUNE_PROTECT=0 → 所有 tool 消息被 prune
- [x] **P1.2.8** 单元测试：非 tool role（User/Assistant）不被 prune（即使超 40K）

### P1.3 渲染层识别

- [x] **P1.3.1** 在 `crates/synthia-context/src/truncate.rs` 的 `truncate_messages` 函数（或其他渲染点），识别 `msg.tool_result_cleared_at.is_some()` → 输出占位符：
  ```
  [Old tool result content cleared at {ISO8601}]
  ```
  跳过原 content 输出
- [x] **P1.3.2** 单元测试：消息 `tool_result_cleared_at=Some(...)` → 渲染输出占位符、不含原 content

### P1.4 验证

- [x] **P1.4.1** `cargo test -p synthia-context` 全绿（494 lib + tests）
- [x] **P1.4.2** 序列化兼容测试覆盖：旧消息反序列化无破坏（4 个新 serde 单元测试）
- [x] **P1.4.3** `cargo clippy -p synthia-context --all-targets --all-features --tests` 无 warning（仅遗留 18 个 pre-existing `needless_borrows_for_generic_args` 警告，与本 change 无关）

### P1.5 提交

- [x] **P1.5.1** `git commit -m "feat(context): Message.tool_result_cleared_at + prune() with PRUNE_PROTECT=40K"` — commit `de9cdb6`

---

## 阶段 P2：apply_compaction 单遍化 + previous-summary 锚定（[D4](./design.md#L111-L131) + [D5](./design.md#L132-L157)）

> Specs:
> - [`compaction-single-pass/spec.md`](./specs/compaction-single-pass/spec.md)
> - [`context-compaction/spec.md`](./specs/context-compaction/spec.md)（扩展）
> 收益：消除 3×O(n) token estimate + 决策连续性

### P2.1 `apply_compaction` 单次 estimate

- [ ] **P2.1.1** 阅读 `crates/synthia-context/src/compaction/compactor.rs:818-876` 的当前 `apply_compaction` 实现，记录 3 次 `estimate_tokens` 的位置
- [ ] **P2.1.2** 重构为：函数顶部 `let original_tokens = estimate_tokens(msgs_to_compact);` 仅 1 次
- [ ] **P2.1.3** 复用 `original_tokens` 给 L1→L2→L3 退化链（不再每次重 estimate）
- [ ] **P2.1.4** 验证 `CompactionResult.original_tokens` / `compacted_tokens` 字段语义不变

### P2.2 `compact_level1` 接受 `previous_summary`

- [ ] **P2.2.1** 修改 `compact_level1` 签名增加 `previous_summary: Option<&str>` 参数
- [ ] **P2.2.2** prompt 模板分支：
  - `Some(s)` → 注入 `<previous-summary>{s}</previous-summary>` 块，提示"更新锚定摘要"
  - `None` → 提示"创建新锚定摘要"
- [ ] **P2.2.3** 单元测试：`previous_summary=Some("...")` → prompt 含 `<previous-summary>` 块
- [ ] **P2.2.4** 单元测试：`previous_summary=None` → prompt 不含 `<previous-summary>` 块、含"创建新"
- [ ] **P2.2.5** L2/L3 函数签名保持不变（不接 `previous_summary`，呼应 D5）

### P2.3 `apply_compaction` 传递 `previous_summary`

- [ ] **P2.3.1** 在 `apply_compaction` 调用 `compact_level1` 处，传入 `previous_summary` 参数（从 session state 读取）
- [ ] **P2.3.2** L1 成功后把新摘要记录到 session state 作为下次 L1 的 `previous_summary`
- [ ] **P2.3.3** 单元测试：连续 2 次 `apply_compaction` 调用 → 第 2 次 L1 收到第 1 次的摘要

### P2.4 `recovery_cascade` 共享 `original_tokens`

- [ ] **P2.4.1** 阅读 `crates/synthia-agent/src/error_recovery/recovery_cascade.rs::try_l4_compact` 当前实现
- [ ] **P2.4.2** 共享一次 `original_tokens` 计算（消除 L4 触发的 3 次 O(n)）
- [ ] **P2.4.3** 单元测试：L4 触发时 `estimate_tokens` 只调 1 次

### P2.5 验证

- [ ] **P2.5.1** 所有现有 `apply_compaction` / `compact_level1` 单测必须通过（行为锁定）
- [ ] **P2.5.2** 新增 `previous_summary` 相关单测全绿
- [ ] **P2.5.3** `cargo test -p synthia-context -p synthia-agent` 全绿
- [ ] **P2.5.4** `cargo clippy --all-targets --all-features --tests --all` 无 warning
- [ ] **P2.5.5** 简单 benchmark 验证 3×O(n) → 1×O(n) 收益（可选）

### P2.6 提交

- [ ] **P2.6.1** `git commit -m "perf(context): apply_compaction single-pass + previous_summary anchor"`

---

## 阶段 P3：agent_tools.rs 拆分（[D6](./design.md#L158-L188)）

> Spec: [`agent-tools-split/spec.md`](./specs/agent-tools-split/spec.md)
> 收益：1300+ 行单文件 → 7 个 < 300 行文件，零行为变化

### P3.1 子文件结构

- [ ] **P3.1.1** 创建目录 `crates/synthia-agent/src/tools/agent/`
- [ ] **P3.1.2** 创建子目录 `crates/synthia-agent/src/tools/agent/tools/`

### P3.2 文件拆分

- [ ] **P3.2.1** `agent/mod.rs`（重导出 + `register_builtin_tools`）
- [ ] **P3.2.2** `agent/message_bus.rs`（`AgentMessage` / `MessageBus` / `SendError` / `ReceiveError`）
- [ ] **P3.2.3** `agent/instance.rs`（`AgentInstance` / lifecycle）
- [ ] **P3.2.4** `agent/manager.rs`（`SubagentManager` / spawn coordination）
- [ ] **P3.2.5** `agent/tools/mod.rs`（重导出）
- [ ] **P3.2.6** `agent/tools/spawn.rs`（`AgentTool` / `create_spawn_agent_tool`）
- [ ] **P3.2.7** `agent/tools/send.rs`（`SendMessageTool`）
- [ ] **P3.2.8** `agent/tools/team_create.rs`（`TeamCreateTool`）
- [ ] **P3.2.9** `agent/tools/team_delete.rs`（`TeamDeleteTool`）

### P3.3 Shim 兼容

- [ ] **P3.3.1** **保留** `crates/synthia-agent/src/tools/agent_tools.rs`，改为：
  ```rust
  pub use crate::tools::agent::*;
  ```
- [ ] **P3.3.2** 验证 `agent_tools::*` 所有原导入路径仍可用

### P3.4 验证

- [ ] **P3.4.1** `cargo check --workspace` 无错
- [ ] **P3.4.2** `cargo test --workspace` 全绿（行为锁定）
- [ ] **P3.4.3** `cargo clippy --all-targets --all-features --tests --all` 无 warning
- [ ] **P3.4.4** `wc -l crates/synthia-agent/src/tools/agent/*.rs crates/synthia-agent/src/tools/agent/tools/*.rs` → 每个 < 300 行

### P3.5 提交

- [ ] **P3.5.1** `git commit -m "refactor(agent): split agent_tools.rs into 7 modules (1300→<300 lines each)"`

---

## 阶段 Verify：集成测试 + 收尾

### V.1 集成测试

- [ ] **V.1.1** 创建 `crates/synthia-context/tests/compact_truncate_pipeline.rs`
- [ ] **V.1.2** 端到端场景 1：tool 输出 > 16K 含多字节 UTF-8 → bash UTF-8 安全截断 → message 进 LLM（不 panic）
- [ ] **V.1.3** 端到端场景 2：构造超出 PRUNE_PROTECT 的消息列表 → `prune()` 调用 → 验证尾部最老被清除 + 幂等停止
- [ ] **V.1.4** 端到端场景 3：连续 2 次 `apply_compaction` → 验证 L1 收到第 1 次的 `previous_summary`
- [ ] **V.1.5** 端到端场景 4：`prune()` 后消息渲染层识别 `tool_result_cleared_at` → 输出占位符

### V.2 代码质量

- [ ] **V.2.1** `cargo +nightly fmt --all`（参考 [rust.md](file:///home/crochee/workspace/synthia/.trae/rules/rust.md)）
- [ ] **V.2.2** `cargo clippy --all-targets --all-features --tests --all` 无 warning
- [ ] **V.2.3** 修复所有 clippy 警告
- [ ] **V.2.4** `cargo test --workspace` 全绿

### V.3 提交

- [ ] **V.3.1** `git commit -m "test: integration tests for compact/truncate/prune pipeline"`
- [ ] **V.3.2** `git commit -m "chore: cargo fmt + clippy cleanup"`（如有格式/警告修复）

---

## 阶段 Archive：归档 + 复盘

### A.1 OpenSpec 同步

- [ ] **A.1.1** 同步 4 个新 spec 到 `openspec/specs/`（脱离 change 目录成为基线）：
  - `bash-utf8-safe-truncate`
  - `prune-idempotent-marker`
  - `compaction-single-pass`
  - `agent-tools-split`
- [ ] **A.1.2** 验证 2 个修改的 spec 也同步：`context-compaction`、`tool-output-truncate`

### A.2 归档

- [ ] **A.2.1** `openspec archive compact-truncate-prune-convergence --yes`
- [ ] **A.2.2** 验证 change 移至 `openspec/changes/archive/`

### A.3 复盘

- [ ] **A.3.1** 在 `openspec/changes/archive/compact-truncate-prune-convergence/retrospective.md` 写复盘：
  - commit 范围（5 个 commit 的 hash + 标题）
  - 偏差（实际 vs 估算 + 原因）
  - 经验沉淀（哪些做得好、哪些下次改）
  - Follow-up（OQ4 summary 截断 / OQ5 prune 集成 / 下一处差距候选）

---

## 风险监控

- **R1** (P0)：UTF-8 切点"多走几步" → regression test 必须包含 "实际字节 ≤ max_output_length" 断言
- **R2** (P1)：序列化 schema 变化 → `serde(default)` 已加，序列化兼容测试是底线
- **R3** (P2)：compactor 退化路径行为变化 → 保留 L1→L2→L3 顺序、所有现有单测必须通过
- **R4** (P2)：summary 膨胀 → 本 change 不实现，OQ4 留作 follow-up
- **R5** (P3)：use path 漏掉 → shim 重导出 + workspace 全绿验证
- **R6** (调度)：5 阶段串行，不并发，tasks.md 强制顺序

---

## 验证矩阵

| 验证项 | 触发阶段 | 验证方式 | 通过标准 |
|--------|----------|----------|----------|
| bash UTF-8 panic 修复 | P0 | `cargo test -p synthia-exec` | 全绿 + regression test |
| 序列化向后兼容 | P1 | 单元测试反序列化旧 JSON | 成功 + 字段为 None |
| `prune()` 幂等性 | P1 | 连续调用 2 次 → 第二次 0 改变 | stats 不变 |
| PRUNE_PROTECT=40K | P1 | 单元测试：超 40K 触发清除 | 统计正确 |
| compactor 行为锁定 | P2 | 所有现有 compactor 单测 | 100% 通过 |
| `previous_summary` 注入 | P2 | 单元测试 prompt 模板分支 | 内容正确 |
| `agent_tools::*` 兼容 | P3 | `cargo test --workspace` | 全绿 |
| 文件 < 300 行 | P3 | `wc -l` | 7 个文件均 < 300 |
| 集成端到端 | Verify | `tests/compact_truncate_pipeline.rs` | 4 场景全过 |
| Clippy 0 warning | Verify | `cargo clippy --all-targets` | 0 warning |
| Rust 编码规范 | Verify | `cargo +nightly fmt` | 无 diff |

---

## 提交时间线

```
P0 (~0.5d) → P1 (~1.5d) → P2 (~1d) → P3 (~1d) → Verify (~0.5d) → Archive
   ↓           ↓             ↓           ↓             ↓              ↓
fix(exec)  feat(context)  perf(ctx)  refactor(ag)  test + chore  openspec archive
```

---

## Follow-up（不在本 change）

- **FU.1**：OQ4 — `previous_summary` 累积 4K 字符截断
- **FU.2**：OQ5 — `prune()` 在 `StepCompact` 之前自动调用集成
- **FU.3**：全量收敛 4 套 truncate 到 `synthia_context::truncate`（需要 R1 大型重构授权）
- **FU.4**：codex 风格 modular tool spec/handler 重构
- **FU.5**：下一处差距评估（候选：Codex turn model / OpenCode v2 + ACP）

