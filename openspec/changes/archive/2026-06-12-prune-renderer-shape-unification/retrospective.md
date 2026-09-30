# Retrospective: prune-renderer-shape-unification

> 完成时间：2026-06-13
> 关联：[proposal.md](./proposal.md) · [design.md](./design.md) · [tasks.md](./tasks.md) · [plan.md](./plan.md)
> 父 change：[compact-truncate-prune-convergence (archived)](../archive/2026-06-12-compact-truncate-prune-convergence/)

---

## 1. Commit 范围

| Commit | 范围 |
|--------|------|
| 待提交 | `fix(context): renderer honors tool_result_cleared_at for ContentPart::ToolResult shape` |

- 新增模块私有函数 `replace_first_text_anywhere` + `replace_first_in_tool_result`（在 `truncate.rs`）
- 修改 `truncate_messages` 的 cleared-placeholder 分支（用新 helper 替代 `extract_text().is_some()` gate）
- 5 个新单测覆盖 helper 的 5 个分支（Shape A / Shape B / Multi / Image / 空 ToolResult）
- 1 个新集成测试覆盖完整生产路径 `prune(0) → truncate_messages` 在 Shape A 下的端到端行为
- 2 个 delta spec：`prune-idempotent-marker` + `tool-output-truncate`（MODIFIED 增补 Shape A 场景）

---

## 2. 偏差（实际 vs 估算）

| 阶段 | 估算 | 实际 | 偏差 | 原因 |
|------|------|------|------|------|
| Design + Plan | 0.5 天 | ~0.3 天 | -0.2 | 改动范围小（单文件 + 单 helper），设计讨论快收敛 |
| 实现 | 0.5 天 | ~0.3 天 | -0.2 | helper 5 分支用 match 表达一次性写完，无返工 |
| 测试 | 0.3 天 | ~0.2 天 | -0.1 | 5 单测 + 1 集成测试一次过；`truncate_messages` 既有 8 个 cleared-placeholder 测试全部保留并继续通过 |
| Verify + Archive | 0.2 天 | ~0.1 天 | -0.1 | clippy 0 新警告（synthia-context lib test 既有的 21 个 warning 全部在 `compactor.rs` / `compaction_service.rs` / `service.rs` 的 `&format!(...)` 上，与本 change 无关）；`openspec validate` 一次过 |
| **合计** | **1.5 天** | **~0.9 天** | **-0.6** | TDD 顺序（先 helper 单测 + 既有 cleared 测试不变 + 后集成测试）让返工极少 |

### 关键偏差说明

1. **新增 2 个单测（实际 5 vs 估算 4）**：tasks.md 的 4 个分支测试是基础；`replace_first_text_empty_tool_result_content_returns_false` 是实现时**临时补的**——发现 `ContentPart::ToolResult({content: []})` 是合法状态（prune 之后，Shape A 工具可能先清空 inner content 再被 mark），需要明确 helper 行为（返回 `false`，不 panic）。这个 case 在原始 design 中没列出来。
2. **Delta spec header 格式一次正确**：相比父 change 的 retrospective 中"第一次用 `## Requirements` 报错"，本次直接用了 `## MODIFIED Requirements`（延续 FU.1 的 scope），`openspec validate` 一次通过。
3. **没有动既有 cleared-placeholder 测试**：原本担心 Shape A helper 会破坏 Shape B 行为，实际 helper 的 Shape B 分支退化为等价 `set_msg_text` 行为（修改顶层 `ContentPart::Text.text`），8 个既有测试 0 修改。

---

## 3. 经验沉淀

### 做得好

- **变更范围严格匹配 FU.1**：design.md §3 明确写了 5 个非目标（不改 `is_tool_result`、不动 `prune()`、不动 stream builder、不动 `Content` enum 形态），实现过程中守住了边界——没有顺手"重构"已有的 `set_msg_text` 也没有改 `is_tool_result` 的检测逻辑。`set_msg_text` 保留给 size-based 路径（D3 决策），避免一处改动两处回归。
- **TDD 顺序**：先写 helper（5 单测全过）→ 切 cleared-placeholder 分支（既有 8 测试不变）→ 写集成测试（5 Shape A 端到端）。如果颠倒顺序（先切分支再写 helper），单测会先在已坏的路径上失败，定位成本更高。
- **P8 守恒验证集成化**：`pipeline_prune_then_render_shape_a_full_production_path` 测试同时断言"placeholder 已注入" **和** "`tool_use_id` / `role` 保持不变"（line 311-317），把 P8 不变量做成可执行约束。如果实现时图省事 mutate 整个 `Content`（比如 `*content = Content::text(marker)`），这条测试会立即失败。
- **公共 API 0 变化**：helper 是 `fn`（非 `pub`），模块外不可见；签名用 `Content` + `&str` 而非 `&mut Message`，让 helper 的可组合性更强（未来可能给其他 `Content` 变体用）。`truncate_messages` 的对外签名完全不变——所有调用方零修改。

### 下次改

- **集成测试的"形状"注释**：第 19-25 行的 doc 注释里写的"prune uses the ToolResult shape, renderer uses the Text shape with `tool_call_id` and a manually-set `tool_result_cleared_at`"——这是**遗留描述**。本 change 修复后，Shape A 在 renderer 中也走通。下次重读这段测试 doc 的人可能会困惑："为什么这个测试还在手工 set `tool_result_cleared_at`？"答：因为 `prune()` 的 budget 在默认 40K 时不会 mark 这 1 个小 message，测试需要手 set 触发 cleared 分支。新加的 `pipeline_prune_then_render_shape_a_full_production_path` 测试用了 `prune(0)` 强制 mark all，避免了这个坑。**Follow-up 候选：给既有 4 个 pipeline 测试加一个 2-3 行注释解释"手工 set 是因为 budget"，否则下个人看 test 会再次困惑。**
- **OpenSpec delta 与 spec 同步时机**：`openspec archive` 之前没显式 `openspec sync` 检查——archive 操作会同步 delta 到 baseline，但顺序要保证 tasks.md 7.4 的"验证 2 specs synced to baseline"在 archive **之后** 跑。本次按顺序执行了。下次写 task 把 "sync check" 放在 archive 命令的"必须在它之后"标注里。
- **Clippy warning 噪音**：synthia-context lib test 当前有 21 个 warning（`needless_borrows_for_generic_args` 集中在 `compactor.rs` / `compaction_service.rs` / `service.rs` 的 `&format!(...)` 模式），与本 change 无关但在每个 PR 都会刷屏。**Follow-up：FU.3（cargo fmt baseline 统一）应该和"清理这些 needless_borrows"合并到一个 change**——两者都是"重构 vs 修 bug 噪音"。

### 关键数字

- **公开 API 兼容性**：100%（helper 是 module-private，外部零变化）
- **新增/修改测试**：synthia-context lib +5 单测（helper 5 分支），integration test +1（Shape A 端到端）；既有 8 个 cleared-placeholder 测试 0 修改
- **truncate.rs 增量**：+242 行（包含 doc comments + 5 单测；helper 核心 ~30 行）
- **新增/修改 spec**：0 ADDED spec，2 MODIFIED spec（`prune-idempotent-marker` + `tool-output-truncate` 各增补 Shape A 场景）

---

## 4. Follow-up（不在本 change）

| ID | 内容 | 优先级 | 状态 | 估算 |
|----|------|--------|------|------|
| **FU.1** | Prune + Renderer 形状统一 | P1 | **本 change 完成** | — |
| **FU.2** | `compact_with_fallback` 接受 `Option<usize>` 预计算 token 数 | P2 | 仍 open | 0.5 天 |
| **FU.3** | Cargo fmt baseline 统一 + 清理 synthia-context 既有 clippy warning | P3 | 仍 open | 0.5 天 |
| **FU.4** | `lifecycle_tools.rs` 308 → < 300 行 | P3 | 仍 open | 0.25 天 |
| **FU.5** | `previous_summary` 累积超 4K 字符截断 | P3 | 仍 open | 0.5 天 |
| **FU.6** | `prune()` 在 `StepCompact` 之前自动调用 | P2 | **仍 deferred**（怀疑派发现：生产循环不把 tool results 推入 ctx.messages，所以当前零作用） | 1 天 |
| **FU.7** | 集成测试 doc 注释更新（解释"手工 set cleared_at" 的原因） | P3 | 仍 open | 0.05 天 |
| **FU.8** | 下一处高价值差距评估：Codex session/Turn model / OpenCode v2 + ACP | — | 下一 change 启动 | — |

---

## 5. 下一处差距候选（2026-06-13 视角）

完成 FU.1 后，下一处高价值差距候选：

1. **Codex session/Turn model** — 仍是高价值候选。codex 的 `Turn` 模型把"单次 user → LLM → tool loop → response"作为一个不可分割的并发单元，与 opencode 的"流式增量 session"形成鲜明对比。需具体使用场景驱动才能决定是否移植。**风险：synthia 当前 event bus + 单一 agent loop 已能 cover 现有需求，过早引入 `Turn` 会增加调试面（怀疑派意见认为 Turn 是 codex 的 60% 路径，synthia 不需要）。**
2. **OpenCode v2 + ACP** — 高价值。opencode v2 引入 ACP（Agent Communication Protocol）作为 agent-to-agent 通信层，可以把当前 synthia 单 agent 扩展到多 agent 协作。**需评估：ACP 协议稳定度、是否值得 synthia 引入跨进程边界。**
3. **synthia-agent / synthia-exec 边界重构** — 中价值。当前 `synthia-exec` 内的 bash_tool.rs 已经在最近的 P0 修复（UTF-8 panic）后趋于稳定，但与 `synthia-agent` 内的权限沙箱存在职责重叠（exec 知道 role-based 命令黑名单，agent 知道 policy 决策点）。**候选：把 `synthia-exec` 拆成 `synthia-tool-bash` + `synthia-tool-exec-base`**，让 bash 工具可以独立升级（hotfix UTF-8 类问题不需要重新发布 agent crate）。
4. **本 change 的剩余 follow-up**（FU.2 / FU.5）— 立即可做，但价值密度低（清理 1 个 estimate 调用 + 1 个 summary 截断）。

**推荐下一处**：新开一个 change 评估 **Codex session/Turn model vs OpenCode v2 + ACP**，选 1 个跑 P0 验证。这两条都是 60% 价值的"是否要追"判断，结论应该基于：
- 是否未来 6 个月有 multi-agent 协作需求 → 走 ACP
- 是否未来 6 个月有并发 user session 需求（同一 agent 实例服务多 user）→ 走 Turn model
- 如果两个都不确定 → 维持当前架构 + 清 FU.2/FU.5

**不推荐**：直接动手移植 Turn model 或 ACP，缺乏具体使用场景会落入"早熟抽象"陷阱（与父 change retrospective 的 L4 compaction lesson 一致）。
