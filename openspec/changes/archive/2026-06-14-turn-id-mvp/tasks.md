## 1. 冻结期（2026-06-13 → 2026-09-13）

- [x] 1.1 监控是否有"按 turn 维度查询"的真实 caller（被动监控）
- [x] 1.2 等待 3 个正交前置任务完成：
  - [x] 1.2.1 `unify-token-usage-types` change（2026-06-12 archived）
  - [x] 1.2.2 `turn-id-unify` change（2026-06-13 archived）
  - [x] 1.2.3 `recovery-path-explicit` / `explicit-recovery-paths` change（2026-06-13 archived）
- [x] 1.3 冻结期不做任何代码变更
- [x] 1.4 **2026-06-13 用户主动请求解冻**（提前 3 个月）→ 进入 Phase 2 实施

## 2. 解冻后实施（仅在满足解冻条件时）

### 2.1 创建 TurnId 类型

- [x] 2.1.1 创建 `crates/synthia-agent/src/turn.rs`（< 30 行）
- [x] 2.1.2 写入 `pub struct TurnId(pub Uuid)`，derive `Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize`
- [x] 2.1.3 写入 `impl TurnId { pub fn new() -> Self { Self(Uuid::new_v4()) } }`
- [x] 2.1.4 验证文件 < 30 行
- [x] 2.1.5 在 `crates/synthia-agent/src/lib.rs` 添加 `pub mod turn;` 导出
- [x] 2.1.6 删 `turn_id.rs` + `format_turn_id`（已统一到 `turn::TurnId` + `t.0.to_string()`）

### 2.2 修改 LoopContext

- [x] 2.2.1 在 `crates/synthia-agent/src/loop_context.rs` 的 `LoopContext` struct 加 `pub current_turn_id: Option<TurnId>` 字段
- [x] 2.2.2 在 `LoopContext::new` 初始化 `current_turn_id: None`
- [x] 2.2.3 验证 `LoopContext.iteration: usize` 字段保留
- [x] 2.2.4 验证 `with_messages`/`with_iteration`/`with_token_limit` builder 方法不破坏
- [x] 2.2.5 新增 `assign_new_turn_id()` helper 返回新 `TurnId`

### 2.3 修改 StreamBuilder

- [x] 2.3.1 替换 `crates/synthia-agent/src/stream_builder/builder.rs:360` 的 `crate::turn_id::format_turn_id(ctx.iteration)` 为 `ctx.current_turn_id.map(|t| t.0.to_string()).unwrap_or_else(|| format!("turn-{}", ctx.iteration))`
- [x] 2.3.2 验证 `AgentContext::new` 的 `turn_id` 参数类型保持 `String`（MVP 阶段不升级为 `TurnId`，避免 hook 消费者 ripple）
- [x] 2.3.3 验证 `builder.rs` 中无新 `format!("turn-{}", ...)` 调用

### 2.4 验证

- [x] 2.4.1 运行 `cargo check --workspace --all-targets` 期望 0 错误
- [x] 2.4.2 运行 `cargo test -p synthia-agent` 期望 100% 通过（502 lib + 79 integration）
- [x] 2.4.3 运行 `cargo +nightly fmt --all` 期望无变更
- [x] 2.4.4 运行 `cargo clippy -p synthia-agent --all-targets --all-features --tests` 0 新增 warning
- [x] 2.4.5 grep 审计：
  - [x] 2.4.5.1 `grep -rn "pub struct TurnId" crates/` → 1 处（`synthia-agent/src/turn.rs`）
  - [x] 2.4.5.2 `grep -rn "pub struct Turn\b" crates/` → 0 行
  - [x] 2.4.5.3 `grep -rn "pub enum TurnStatus" crates/` → 0 行
  - [x] 2.4.5.4 `grep -rn "TurnStarted\|TurnCompleted\|TurnFailed\|TurnAborted" crates/` → 仅 pre-existing dead match arms in `synthia-cli/src/output.rs:162-164`（不在本 change 范围）
  - [x] 2.4.5.5 `grep -rn "save_turn\|load_turn\|append_turn\|turns.jsonl" crates/` → 0 行

### 2.5 测试

- [x] 2.5.1 `TurnId` 单元测试（为遵守 spec 2.4.5.1 file < 30 行约束，从 `turn.rs` 移至 `crates/synthia-agent/tests/turn_id_test.rs` integration test）：
  - [x] 2.5.1.1 `test_turn_id_new_returns_unique_uuids`
  - [x] 2.5.1.2 `test_turn_id_serializes_to_json` (roundtrip)
  - [x] 2.5.1.3 `test_turn_id_hash_eq_consistency`
- [x] 2.5.2 在 `crates/synthia-agent/src/loop_context.rs` 添加测试：
  - [x] 2.5.2.1 `test_loop_context_default_current_turn_id_is_none`
  - [x] 2.5.2.2 `test_loop_context_assign_new_turn_id`
  - [x] 2.5.2.3 `test_loop_context_with_messages_preserves_turn_id`

### 2.6 OpenSpec 收尾

- [x] 2.6.1 运行 `openspec validate turn-id-mvp --strict` 期望 0 错误（通过）
- [x] 2.6.2 提交 commit：`d433d2d feat(agent): introduce TurnId(Uuid) as MVP turn label`
- [ ] 2.6.3 推送 `git push origin master`（未推送：本地 master 领先 origin/master 45 commits，独立决策；与本 change 无关）
- [x] 2.6.4 等待 CI 通过（本地 CI 等价检查通过：fmt + clippy + 584 tests + re-export policy 5/5）
- [ ] 2.6.5 运行 `openspec archive turn-id-mvp`（正在执行）

## 3. 6 个月硬截止（2026-12-13）

- [x] 3.1 **N/A**（本 change 在 2026-06-13 用户主动解冻时已实施，不再适用"6 个月未解冻归档"路径）
</content>
</invoke>