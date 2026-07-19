# turn-id-unify Plan

> **本 plan 是 4 派对抗性审查 + 共识形成后的执行计划**
> **目标：< 15 行代码变更 + 8 个 OpenSpec artifacts 完整 + 验证通过 + 归档**

---

## 执行时间表

| 步骤 | 工作内容 | 预估产出 | 验证 |
|------|----------|----------|------|
| 1 | 写 8 个 OpenSpec artifacts | `proposal.md` / `design.md` / `specs/turn-id-unify/spec.md` / `tasks.md` / `plan.md` / `verify.md` / `retrospective.md` / `README.md` | `openspec show turn-id-unify` 8/8 |
| 2 | 新增 `format_turn_id` 函数 | `crates/synthia-agent/src/turn_id.rs` ~5 行 | `cargo check` 0 错 |
| 3 | 替换 `builder.rs:360` | 1 行替换 | `grep format!("turn-{}"` 仅 1 处 |
| 4 | 删除 `NetworkAccess.turn_id` | 字段 + 构造函数少 1 参数 | grep 0 处使用 |
| 5 | 更新 guardian_coordinator.rs 测试 | 1 处调用更新 | `cargo test` 100% 通过 |
| 6 | `cargo check` / `test` / `fmt` / `clippy` | 4 个验证 | 全部 0 错 0 警 |
| 7 | `openspec validate` 双重验证 | 双重通过 | exit 0 |
| 8 | 提交 | 1 个 commit | `git log -1` 显示 |
| 9 | 手动归档（6 步） | `archive/2026-06-13-turn-id-unify/` + `specs/turn-id-unify/spec.md` | `openspec list` 不再显示 |

---

## 关键风险与缓解

### 风险 1：`network_access()` 破坏性 API 变更导致项目内编译失败

- **检测命令**：`grep -rn 'ApprovalRequest::network_access' crates/ --include='*.rs' | grep -v test`
- **预期结果**：0 处
- **如果失败**：手动修复调用方（本 change 不修外部使用方，仅修项目内）

### 风险 2：`format_turn_id` 路径命名与未来 `turn.rs::TurnId` 混淆

- **缓解**：`turn_id` 模块（helper 函数）vs `turn` 模块（未来 `TurnId` 类型）路径分离
- **可读性**：函数名 `format_turn_id` 是 verb，类型名 `TurnId` 是 noun

### 风险 3：cargo clippy 警告 `format_turn_id` 函数可优化

- **检测**：`cargo clippy --all-targets --all-features --tests --all 2>&1 | grep format_turn_id`
- **预期**：无警告
- **如果失败**：根据 clippy hint 调整（`#[allow(clippy::xxx)]` 谨慎使用）

---

## 任务依赖图

```
[1. 写 8 OpenSpec artifacts]
    ↓
[2-5. 代码变更] ──→ [6. cargo 验证] ──→ [7. openspec validate] ──→ [8. 提交] ──→ [9. 归档]
```

无并行任务（4 派共识决策 + 顺序实施）。

---

## 决策记录（不实施项）

| 选项 | 决策 | 理由 |
|------|------|------|
| `From<usize> for String` impl | ❌ 不实施 | 0 caller，4 派拒绝 |
| `pub fn format_turn_id_str` | ❌ 不实施 | 0 caller，4 派拒绝 |
| 类型别名 `type TurnId = u64` | ❌ 不实施 | Rust type alias 不创建新类型，0 实际收益 |
| 提前 `TurnId(Uuid)` | ❌ 不实施 | 与 `turn-id-mvp` 协调成本，4 派拒绝 |
| 删除 #1 `LoopContext.iteration: usize` | ❌ 不实施 | 内部计数器，零外部影响 |
| 替换 #3 `PrefixStabilityEvent.turn_id: u64` | ❌ 不实施 | 内部遥测字段，零外部影响 |
| 加 Guardian turn_id 到其他 4 variant | ❌ 不实施 | 越界 scope |

---

## 与 3 个前置任务的进度

| 前置任务 | 状态 |
|----------|------|
| `unify-token-usage-types` | ✓ archived 2026-06-12 |
| `turn-id-unify` | 🔄 **本 change 实施中** |
| `recovery-path-explicit` | ⏳ 未启动（剩余 1 个前置任务） |

完成本 change 后，`turn-id-mvp` 解冻前置任务进度：**2/3 完成**（剩 `recovery-path-explicit`）。
