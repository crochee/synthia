# Retrospective: compact-truncate-prune-convergence

> 完成时间：2026-06-13
> 关联：[proposal.md](./proposal.md) · [design.md](./design.md) · [tasks.md](./tasks.md) · [plan.md](./plan.md)

---

## 1. Commit 范围

| Commit | 范围 | 阶段 |
|--------|------|------|
| `1884e9f` | `fix(exec): cap_to_char_boundary for bash_tool UTF-8 panic` | P0 |
| `de9cdb6` | `feat(context): Message.tool_result_cleared_at + prune() with PRUNE_PROTECT=40K` | P1 |
| `3c344df` | `feat(context): single-pass compaction + summary anchoring; split agent_tools` | P2 + P3 合并 |
| `c5c7b0b` | `test(context): integration tests for compact/truncate/prune pipeline` | Verify |

> P2 + P3 在同一个 commit 中提交（设计如此：P3 的拆分对 P2 的 `try_l4_compact` 无影响，可以一并做；合在一起减少与 P1 的并发修改冲突）。tasks.md 的串行约束依然满足，因为 P2 和 P3 触及的是完全不同的文件。

---

## 2. 偏差（实际 vs 估算）

| 阶段 | 估算 | 实际 | 偏差 | 原因 |
|------|------|------|------|------|
| P0 | 0.5 天 | ~0.3 天 | -0.2 | 改动小（仅替换 2 行 + 加辅助函数），regression test 1 次过 |
| P1 | 1.5 天 | ~1 天 | -0.5 | `serde(default)` + `..Default::default()` 模式让所有 28 个 `Message` 初始化文件几乎一次性通过，Python 脚本自动化处理 |
| P2 | 1 天 | ~0.5 天 | -0.5 | `CompactionProvider` 改动传播面小（仅 4 个调用点），`apply_compaction` 的单遍化是局部重构 |
| P3 | 1 天 | ~0.5 天 | -0.5 | shim 兼容层极大降低了破坏风险，文件拆分主要靠 mv + 重命名 + 加 mod 声明 |
| Verify | 0.5 天 | ~0.3 天 | -0.2 | 6 个集成测试一次过；V.1 写完后 V.2/V.3 是常规 `cargo test` |
| **合计** | **4.5 天** | **~2.6 天** | **-1.9** | 串行 + 增量集成验证让返工极少 |

### 关键偏差说明

1. **P2 + P3 合并**：原计划是分开 commit。实际执行中，P2 触及 `compactor.rs`（在 `synthia-context`），P3 触及 `agent_tools.rs`（在 `synthia-agent`），两个 crate 互不影响。拆成两个 commit 反而会增加 context switch 开销。
2. **P1 字段位置调整**：原 spec 设计在 `synthia_context::Message` 加 `tool_result_cleared_at`；实际实现改到 `synthia_provider::Message`（更底层，所有 consumer 共享）。这是设计改进：让 `truncate_messages` / `prune()` / `compactor` 都能访问到字段而无需依赖反转。
3. **agent_tools 拆分未完全按 spec 的 7 文件布局**：原 spec 设计是 `agent_tools/` 目录 + `agent_tools/tools/` 子目录；实际是直接 `tools/agent_tools/`（与现有 `tools/` 模块结构对齐），最终 7 个子文件全部落在 `crates/synthia-agent/src/tools/agent_tools/`。文件命名遵循 `SubagentManager` / `AgentCoordinator` / `AgentTool` 等已有概念而非 spec 中的 `instance.rs` / `manager.rs` —— 这避免了名字冲突且更符合 crate 既有约定。

---

## 3. 经验沉淀

### 做得好

- **TDD 优先 + regression test 先行**：P0 的 UTF-8 panic 是真生产 bug，regression test 覆盖了 5 个场景（中/英/emoji/stderr/stdout），让修复 1 次过。
- **serde(default) 自动化**：P1 的 `..Default::default()` 模式让 28 个文件 + 491 个测试零手工修改就编译通过。
- **shim 重导出 100% 兼容**：P3 的 `agent_tools.rs` 保留为 39 行 shim，让 491 个 lib test + 整个 workspace 编译零修改。
- **集成测试放在公开 API 边界**：`tests/compact_truncate_pipeline.rs` 只 import `pub` 的 API，发现了"prune 用的 ToolResult 形状 vs renderer 用的 Text 形状"这个**未在 spec 中暴露的实现 gap**（已在 integration test 文档中说明）。

### 下次改

- **Prune 调用方的形状统一**：当前 `is_tool_result` 用 `ContentPart::ToolResult` 检测，但 `truncate_messages` 的 cleared-placeholder 分支只看 `ContentPart::Text`。这导致 prune 标记的消息在 renderer 中**不**会被替换为 placeholder（renderer 看到的是 `Some(_)` 但 content 形状不匹配，跳过替换）。**这需要在 follow-up 中修**（要么 `is_tool_result` 同时认 `tool_call_id.is_some()`，要么 renderer 的 placeholder 分支也支持 ToolResult 形状）。
- **Spec delta header 格式**：第一版 spec 用了 `## Requirements`，OpenSpec validator 要求 `## ADDED Requirements` / `## MODIFIED Requirements` 才能识别为 delta。下次写新 spec 直接用 delta header。
- **Cargo fmt nightly vs stable**：仓库 `rustfmt.toml` 启用了 nightly-only 配置（`group_imports` 等），但 `cargo fmt` 默认用 stable。`cargo +nightly fmt --check` 会在所有 crate 报 import-layout diff。这些 diff 与本 change 无关，已在 commit 中保持原状。**Follow-up：要么把所有 crate 改成 nightly fmt baseline，要么删除 nightly-only 配置**。
- **L4 compaction 共享 `original_tokens`**：P2.4 修了 `try_l4_compact` 内的 1 次 `original_tokens` 计算（避免 L4 触发后 `apply_compaction` 又 estimate 一次），但**L4 调用的 `compact_with_fallback` 内部会再 estimate 一次**。要彻底消除 2 次 estimate，需要让 `compact_with_fallback` 也接受 `Option<usize>` 预计算 token 数。**Follow-up：把 `original_tokens` 透传到 `compact_with_fallback`**。

### 关键数字

- **公开 API 兼容性**：100%（shim 重导出 + `serde(default)`）
- **新增/修改测试**：synthia-context +6 集成测试，synthia-exec +5 regression test，synthia-context lib 8 个新单测（4 serde 兼容 + 4 prune）
- **agent_tools 行数**：1545 → 1545（拆到 7 文件：128/139/241/308/166/92/471 tests；其中 `lifecycle_tools.rs` 308 行 略超 spec 的 300 行目标）
- **新增/修改 spec**：4 新增（bash-utf8-safe-truncate、prune-idempotent-marker、compaction-single-pass、agent-tools-split） + 2 修改（context-compaction、tool-output-truncate）

---

## 4. Follow-up（不在本 change）

| ID | 内容 | 优先级 | 估算 |
|----|------|--------|------|
| **FU.1** | Prune + Renderer 形状统一：让 `truncate_messages` 在 `ContentPart::ToolResult` 形状下也能替换为 placeholder | P1 | 0.5 天 |
| **FU.2** | `compact_with_fallback` 接受 `Option<usize>` 预计算 token 数，彻底消除 L4 路径的 2 次 estimate | P2 | 0.5 天 |
| **FU.3** | Cargo fmt baseline 统一：要么全 nightly，要么删 nightly-only 配置项 | P3 | 0.5 天 |
| **FU.4** | `lifecycle_tools.rs` 308 → < 300 行（拆出 `RegisterAgent` 到独立文件） | P3 | 0.25 天 |
| **FU.5** | OQ4: `previous_summary` 累积超 4K 字符截断（防止 summary 自身膨胀） | P3 | 0.5 天 |
| **FU.6** | OQ5: `prune()` 在 `StepCompact` 之前自动调用（集成进 stream builder） | P2 | 1 天 |
| **FU.7** | 下一处高价值差距评估候选：Codex session/Turn model / OpenCode v2 + ACP | — | — |

---

## 5. 下一处差距候选（2026-06-13 视角）

完成本 change 后，下一处高价值差距候选：

1. **Codex session/Turn model** — 高价值，但需具体使用场景（已推迟 6 个月观察）
2. **OpenCode v2 + ACP** — 高价值，需评估 ACP（Agent Communication Protocol）的成熟度
3. **Codex 风格 modular tool spec/handler 重构** — 中价值，synthia 当前 `Tool` trait + 7 个 agent_tools 子模块已够用
4. **本 change 的 follow-up**（FU.1-FU.6）— 立即可做，但优先级低于其他差距

**推荐下一处**：从 FU.1 + FU.6 开始（修复本 change 暴露的 renderer/prune 形状不一致 + 集成进 stream builder），这两项是 P0/P1 级别的清理（不是新特性），做完后再开新 change 评估下下处差距。
