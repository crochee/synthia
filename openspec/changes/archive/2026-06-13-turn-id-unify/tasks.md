# turn-id-unify Tasks

> **本 change 实施路径：B + C 组合（集中格式化函数 + 删除孤儿字段）**
> **代码变更目标：< 15 行**
> **不引入 `TurnId(Uuid)` 类型（留给 FROZEN 的 `turn-id-mvp` change 解冻时）**

---

## 1. 新增集中格式化函数（B）

- [ ] 1.1 创建 `crates/synthia-agent/src/turn_id.rs`（~5 行）
  - [ ] 1.1.1 `pub fn format_turn_id(iter: usize) -> String`
  - [ ] 1.1.2 函数体 `format!("turn-{}", iter)`
- [ ] 1.2 在 `crates/synthia-agent/src/lib.rs` 添加 `pub mod turn_id;`
- [ ] 1.3 在 `crates/synthia-agent/src/turn_id.rs` 末尾加 `#[cfg(test)] mod tests` 块（3 个测试）

## 2. 替换 builder.rs 中的 format! 字面量

- [ ] 2.1 修改 `crates/synthia-agent/src/stream_builder/builder.rs:360`
  - [ ] 2.1.1 from: `format!("turn-{}", ctx.iteration)`
  - [ ] 2.1.2 to: `crate::turn_id::format_turn_id(ctx.iteration)`
- [ ] 2.2 验证替换后该文件无其他 `format!("turn-{}", ...)` 字面量

## 3. 删除 ApprovalRequest::NetworkAccess.turn_id 孤儿字段（C）

- [ ] 3.1 修改 `crates/synthia-guardian/src/approval_request.rs`
  - [ ] 3.1.1 删除 `NetworkAccess { id, turn_id, target, host, protocol, port }` 中的 `turn_id` 字段
  - [ ] 3.1.2 修改 `NetworkAccess { id, target, host, protocol, port }`（5 字段）
  - [ ] 3.1.3 修改 `network_access()` 构造函数：从 6 参数到 5 参数（删除 `turn_id: impl Into<String>` 参数）
- [ ] 3.2 更新所有 `network_access()` 调用方
  - [ ] 3.2.1 `crates/synthia-guardian/src/guardian_coordinator.rs:112` 测试调用更新
  - [ ] 3.2.2 grep `crates/` 0 处其他调用

## 4. 验证

- [ ] 4.1 `cargo check --workspace` 0 错误
- [ ] 4.2 `cargo test --workspace` 100% 通过
- [ ] 4.3 `cargo +nightly fmt --all --check` 通过
- [ ] 4.4 `cargo clippy --all-targets --all-features --tests --all` 0 警告
- [ ] 4.5 `grep -rn 'format!("turn-{}"' crates/synthia-agent/` 仅返回 `crates/synthia-agent/src/turn_id.rs` 1 处
- [ ] 4.6 `grep -rn 'NetworkAccess.*{.*turn_id' crates/` 0 处
- [ ] 4.7 `grep -rn 'ApprovalRequest::network_access' crates/ --include='*.rs' | grep -v test` 0 处
- [ ] 4.8 `openspec validate turn-id-unify --type change` 通过
- [ ] 4.9 `openspec validate turn-id-unify --type change --strict` 通过

## 5. 提交

- [ ] 5.1 提交所有变更（< 15 行 + 4 个 OpenSpec artifacts 已先写）
- [ ] 5.2 提交信息：`refactor(agent): centralize turn_id string format + remove orphan NetworkAccess.turn_id`

## 6. 归档

- [ ] 6.1 复制 8 个 OpenSpec artifacts 到 `openspec/changes/archive/2026-06-13-turn-id-unify/`
- [ ] 6.2 创建 README.md（描述本 change）
- [ ] 6.3 创建 .openspec.yaml（schema: superpowers-bridge, created: 2026-06-13）
- [ ] 6.4 同步 spec 到 `openspec/specs/turn-id-unify/spec.md`（`## ADDED Requirements` → `## Requirements`）
- [ ] 6.5 `rm -rf openspec/changes/turn-id-unify/`
- [ ] 6.6 验证 `openspec list` 中 turn-id-unify 不再显示

## 7. 与 turn-id-mvp 解冻的衔接（不属本 change scope）

- [ ] 7.1 通知 `turn-id-mvp` change：本 change 已 archived，`format_turn_id` 函数可被未来 `turn-id-mvp` 解冻时删除
- [ ] 7.2 更新 `turn-id-mvp/design.md` R4 状态：`turn-id-unify` 已 archived，前置任务 #2 完成
- [ ] 7.3 监控 `turn-id-mvp` 剩余前置任务：`recovery-path-explicit`
