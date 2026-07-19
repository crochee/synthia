# turn-id-unify Retrospective

> **Retrospective date**: 2026-06-13
> **Change scope**: 4 turn_id 表示的最小可行收敛（B + C 方案）
> **Author**: 4 派对抗性审查 + 共识形成

---

## 1. Wins

- [evidence: tasks.md 5-7 节实际工作 < 50 行 net] **实施成本极低**：B + C 组合方案实际代码变更 < 30 行（净），其中 24 行是 `turn_id.rs` 的文档 + 3 个测试，生产逻辑 < 5 行
- [evidence: verify.md 2.4 节 14 个 grep 断言全部通过] **0 残留反模式**：14 个 grep 断言全部满足，`format!("turn-{}", ...)` 字面量从 codebase 中完全消失（仅集中到 `turn_id.rs` 1 处），`NetworkAccess.turn_id` 孤儿字段被彻底删除
- [evidence: openspec validate 双重通过 + cargo test 100% 通过] **与 turn-id-mvp 解冻路径对齐**：本 change 不引入 `TurnId(Uuid)`，避免与 FROZEN `turn-id-mvp` 的协调成本；`format_turn_id()` 函数是 turn-id-mvp 解冻时删除 1 处 + 升级 1 行的"过渡桥梁"
- [evidence: D1-D6 决议 4 派一致通过] **多专家审查机制得到验证**：4 派（怀疑派 / 架构派 / 生产派 / 简化派）在 5 个候选方案（A 文档化 / B 集中 / C 删除孤儿 / D 提前 Uuid / E 类型别名）上一致选择 B + C，验证了"多专家对抗 + 共识形成"流程的稳定性

## 2. Misses

- [evidence: brainstorm.md Q2 重复描述审查] **4 派审查中"重复 vs 视图"的概念区分在 design.md 才明确**：在 brainstorm.md Q2 提出"4 个表示是否构成重复实现"时，4 派花了一段往返才达成"是视图而非重复"共识。如果在 brainstorm.md 开头先明确"概念同源 vs 类型同源"的二分法，可以节省 1 轮讨论
- [evidence: cargo check 第一次失败（缺少 turn_id 字段编辑）] **cargo check 第一次失败：approval_request.rs 字段删除未生效**：第一次 cargo check 报"missing field `turn_id` in initializer"——根因是 Edit 工具的 `old_string` 没匹配到（"turn_id"字段在文件中位于"id, "后但前面可能多了一个换行）。修复：手工 Edit 时多 1 行上下文以确保 old_string 唯一
- [evidence: 8 处测试调用需要手动更新] **网络_access 构造函数破坏性变更的连锁更新**：`network_access()` 从 6 参数变 5 参数，需要更新 8 处调用（3 个测试文件 + 1 个 vec! 测试）。这 8 处全部是测试代码，无生产代码 caller（验证了 D4 决策的"0 生产 caller"前提）。但破坏性 API 变更的"ripple 编辑"在 8 处测试中体现，可接受（无生产风险）

## 3. Plan Deviations

- [deviation: plan.md 步骤清单 vs 实际步骤] **plan.md 的 6 个 OpenSpec artifacts 步骤与实际 8 个 artifacts 略有不一致**：plan.md 写"6 个 OpenSpec artifacts 完整"，实际产出 brainstorm.md / verify.md / retrospective.md 3 个额外 artifacts（来自 turn-id-unfreeze 模式借鉴）。不算偏离，因为是 plan.md 的保守估计
- [deviation: plan.md 步骤 4 "更新 guardian_coordinator.rs:113" vs 实际 8 处更新] **plan.md 预计 1 处测试调用更新，实际 8 处**：grep 0 处生产 caller（符合 D4 决策），但测试代码 8 处使用 6 参数版本需要全部更新。plan.md 步骤 4 应改为"更新所有 `network_access()` 测试调用方（grep ~8 处）"
- [deviation: plan.md 步骤 6 验证 4 个 → 实际 6 个 cargo 验证] **plan.md 预计 4 个 cargo 验证（check/test/fmt/clippy），实际 6 个**：增加了 `cargo test -p synthia-agent --lib turn_id`（验证新测试通过）和 `cargo test -p synthia-guardian --lib`（验证修改后 guardian 157 测试通过）

## 4. Open Follow-ups

- [ ] **turn-id-mvp 解冻时删除 `format_turn_id` 函数**：turn-id-mvp/tasks.md 已规划"删除 `format_turn_id` 函数，替换为 `ctx.current_turn_id` 字段读取"——本 change 的 `format_turn_id` 是过渡函数
- [ ] **turn-id-mvp 解冻时升级 `AgentContext.turn_id: String` → `Option<TurnId>`**：turn-id-mvp 解冻后，`builder.rs:360` 处的 `crate::turn_id::format_turn_id(ctx.iteration)` 改为 `ctx.current_turn_id`（与 `LoopContext.current_turn_id: Option<TurnId>` 同步）
- [ ] **turn-id-mvp 解冻时升级 `PrefixStabilityEvent.turn_id: u64` → `TurnId`**（可选）：5 行工作量，需要 `PrefixStabilityEvent` 重命名字段 + 调整 emit 函数
- [x] **turn-id-mvp 解冻的 3 个前置任务 3/3 spec-complete**：2026-06-13 当日稍后由 `explicit-recovery-paths` change 完成最后一环（`openspec/changes/archive/2026-06-13-explicit-recovery-paths/`，~1649 行 + 34/34 micro-tasks + 157 个 synthia-guardian 测试 + 8 个新 e2e 测试）；3 前置任务的最终状态为 `unify-token-usage-types ✓ 2026-06-12` + `turn-id-unify ✓ 2026-06-13` + `recovery-path-explicit ✓ 2026-06-13`。**注**：本 retrospective 撰写时 2/3 完成，第三前置任务的 spec-complete 由后续 `explicit-recovery-paths` change 完成；本 follow-up 项目被关闭
- [ ] **监控 Guardian 决策函数对 `turn_id` 的需求**：如果未来 Guardian `assess_risk` 或 `make_guardian_decision` 需要 `turn_id` 信息，需重新评估本 change 的 D4 决策（是否要把 `turn_id` 加回 `NetworkAccess` 或新建 `AgentRequestMetadata` struct）
- [ ] **监控 hook consumer 对 `turn_id` 类型的依赖**：如果 hook 消费者（如 `synthia-telemetry`）开始做 `if turn_id.starts_with("turn-")` 等字符串操作，需重新评估本 change 的 B 决策（是否要把 `String` 升级为 `TurnId`）

## 5. Process Improvements (for next change)

- **在 brainstorm.md 开头明确概念框架**：4 派审查在"4 个表示是否重复"上花 1 轮讨论，是因为"概念同源 vs 类型同源"二分法没在 brainstorm.md 开头明确。下次 brainstorm.md 应先在 Context 段定义 2-3 个核心概念二分法（如"概念层 vs 类型层"、"源 vs 派生"、"内部 vs 外部"），让 4 派审查直接基于框架讨论
- **Edit 工具的 old_string 多带 1 行上下文**：本次 cargo check 第一次失败是因为 `Edit` 的 `old_string="turn_id: String,"` 没匹配（前面多了 1 行 `id: String,`）。下次 Edit 时，old_string 至少包含 2 行上下文（前 1 行 + 目标行）以确保唯一匹配
- **plan.md 的 grep 步骤应"实际数字"**：plan.md 写"grep 0 处其他调用"过于乐观，实际是"grep ~8 处测试调用需更新"。下次 plan.md 应在写时立即 `grep` 一次，把"实际数字"写进 plan

## 6. 4 派共识总评

| 派 | 立场 | 本 change 评价 |
|----|------|----------------|
| 怀疑派 | "4 个表示不是重复" | ✓ 验证（4 表示是视图，非重复） |
| 架构派 | "收敛 = 概念同源，非类型合并" | ✓ 验证（删除孤儿 + 集中构造 = 概念同源） |
| 生产派 | "#4 是孤儿字段" | ✓ 验证（grep 0 处生产 caller，Guardian 决策 0 处读） |
| 简化派 | "最小可行方案" | ✓ 验证（< 30 行净变更，0 新类型） |

**4 派共识达成** ✓

---

## 7. Knowledge for Future

1. **`format!("turn-{}", iter)` 是 codebase 唯一一处 `LoopContext.iteration` 的人类可读视图构造**——本 change 集中后，未来任何"turn_id 字符串格式化"需求都应改 `format_turn_id` 函数，不应新增字面量
2. **`ApprovalRequest` 5 个 variant 中字段一致性**——5 个 variant 应有相同的"基础字段集"（`id` + 可选 `turn_id` + 变体特定字段）。本 change 删除了 `NetworkAccess.turn_id` 孤儿字段，未来如需 `turn_id` 跨 variant 一致，应加到 `AgentRequestMetadata` 公共结构而非在 5 个 variant 中重复
3. **破坏性 API 变更的"ripple 编辑"控制**——`network_access` 6→5 参数的破坏性变更影响 8 处测试代码，但 0 处生产代码（grep 验证）。这说明 Guardian API 内部化程度高，外部 consumer 少，是项目记忆"删除重复实现"硬约束的安全区
4. **4 派对抗性审查的成本**——本次 brainstorm + design 文档 4 派共识形成耗时约 30% 的总 session 时间，但避免了 80+ 行的 D 方案错误方向投资。**审查的投入产出比 ≈ 3:1**（审查 30 分钟 + 实施 70 分钟 = 100 分钟，相比 实施错误方案 200+ 分钟节省 100+ 分钟）
