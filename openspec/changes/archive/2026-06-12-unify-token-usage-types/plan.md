# unify-token-usage-types Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task.

**Goal:** 把 4 处 `TokenUsage` 类型定义收敛为 1 处 canonical type（`synthia_provider::types::TokenUsage`），通过 1-line shim 模式保持向后兼容。

**Architecture:** canonical type 选在最低层 crate（synthia-provider），3 处下游类型用 `pub use` 替换，1 处 `TokenUsageSnapshot` 直接删除并替换为 canonical type。序列化用 `#[serde(default)]` 保护前向兼容。

**Tech Stack:** Rust, serde, serde_json, chrono, OpenSpec

---

## Task 1: 升级 synthia-provider::TokenUsage 为 canonical type

**Files:**
- `crates/synthia-provider/src/types.rs` (modify, lines 400-406)

- [ ] **Step 1:** 读取 `crates/synthia-provider/src/types.rs:400-406` 当前内容，确认字段名和顺序
- [ ] **Step 2:** 在 derive 列表中追加 `Serialize, Deserialize`：
  ```rust
  #[derive(Clone, Debug, Default, Serialize, Deserialize)]
  pub struct TokenUsage {
      pub prompt_tokens: usize,
      pub completion_tokens: usize,
      pub total_tokens: usize,
      #[serde(default)]
      pub cached_prompt_tokens: Option<usize>,
  }
  ```
- [ ] **Step 3:** 在 struct 之前添加文档注释：`/// Canonical token usage type, used by all crates. Has 4 fields including cached_prompt_tokens for cache-aware accounting.`
- [ ] **Step 4:** 运行 `cargo check -p synthia-provider` → 期望 0 错误
- [ ] **Step 5:** 提交：`git add crates/synthia-provider/src/types.rs && git commit -m "feat(provider): make TokenUsage the canonical type with Serialize/Deserialize"`

---

## Task 2: 替换 synthia-session::TokenUsage 为 1-line shim

**Files:**
- `crates/synthia-session/src/types.rs` (modify, lines 41-47)

- [ ] **Step 1:** 读取 `crates/synthia-session/src/types.rs:41-47` 当前 `TokenUsage` 定义（约 6 行）
- [ ] **Step 2:** 删除 `pub struct TokenUsage { prompt_tokens, completion_tokens, total_tokens, cached_prompt_tokens }` 整段
- [ ] **Step 3:** 在删除位置插入 `pub use synthia_provider::types::TokenUsage;`
- [ ] **Step 4:** 运行 `cargo check -p synthia-session` → 期望 0 错误
- [ ] **Step 5:** 运行 `cargo test -p synthia-session --test session_manager_integration` → 验证 `synthia_session::TokenUsage` 引用继续工作
- [ ] **Step 6:** 提交：`git add crates/synthia-session/src/types.rs && git commit -m "refactor(session): re-export TokenUsage from synthia-provider"`

---

## Task 3: 替换 synthia-agent::events::TokenUsage 为 1-line shim

**Files:**
- `crates/synthia-agent/src/events.rs` (modify, lines 20-25)

- [ ] **Step 1:** 读取 `crates/synthia-agent/src/events.rs:20-25` 当前 `TokenUsage` 定义（约 5 行）
- [ ] **Step 2:** 删除 `pub struct TokenUsage { prompt_tokens, completion_tokens, total_tokens }` 整段
- [ ] **Step 3:** 在删除位置插入 `pub use synthia_provider::types::TokenUsage;`
- [ ] **Step 4:** 运行 `cargo check -p synthia-agent` → 期望 0 错误
- [ ] **Step 5:** 验证 `crates/synthia-agent/src/stream_builder/builder.rs:413, 479` 的 `crate::events::TokenUsage { ... }` 构造调用继续编译（应自动通过 cargo check 验证）
- [ ] **Step 6:** 运行 `cargo test -p synthia-agent` → 期望现有测试 100% 通过
- [ ] **Step 7:** 提交：`git add crates/synthia-agent/src/events.rs && git commit -m "refactor(agent): re-export TokenUsage from synthia-provider"`

---

## Task 4: 删除 synthia-context::TokenUsageSnapshot

**Files:**
- `crates/synthia-context/src/checkpoint.rs` (modify, lines 37-42 + 引用点)

- [ ] **Step 1:** 读取 `crates/synthia-context/src/checkpoint.rs:37-42` 当前 `TokenUsageSnapshot` 定义
- [ ] **Step 2:** 删除 `pub struct TokenUsageSnapshot { ... }` 整段（约 6 行）
- [ ] **Step 3:** 在 `Checkpoint` struct（约 line 44-58）的 `token_usage: TokenUsageSnapshot` 字段改为 `token_usage: synthia_provider::types::TokenUsage`
- [ ] **Step 4:** 全文搜索 `TokenUsageSnapshot` 并替换所有引用为 `synthia_provider::types::TokenUsage`
- [ ] **Step 5:** 运行 `cargo check -p synthia-context` → 期望 0 错误
- [ ] **Step 6:** 运行 `cargo test -p synthia-context` → 期望现有测试 100% 通过
- [ ] **Step 7:** 运行 `grep -rn "TokenUsageSnapshot" crates/` → 期望 0 匹配
- [ ] **Step 8:** 提交：`git add crates/synthia-context/src/checkpoint.rs && git commit -m "refactor(context): remove TokenUsageSnapshot, use canonical TokenUsage"`

---

## Task 5: 跨 crate 验证与质量保证

**Files:**
- 全 workspace

- [ ] **Step 1:** 运行 `cargo check --workspace` → 期望 0 错误
- [ ] **Step 2:** 运行 `cargo test --workspace` → 期望 100% 测试通过（特别关注 32 处引用点）
- [ ] **Step 3:** 验证外部 import 路径继续工作：
  - `synthia_server::tests::e2e_server_sse_test::137`（使用 `synthia_agent::types::TokenUsage`）
  - `synthia_server::tests::e2e_server_ws_test::110, 278`（使用 `synthia_provider::types::TokenUsage`）
  - `synthia_server::tests::sse_stream_test::143`（使用 `synthia_agent::types::TokenUsage`）
- [ ] **Step 4:** 运行 `cargo +nightly fmt --all` → 期望无变更（验证格式已统一）
- [ ] **Step 5:** 运行 `cargo clippy --all-targets --all-features --tests --all` → 修复所有警告
- [ ] **Step 6:** 验证 grep 审计：
  - `grep -rn "pub struct TokenUsage" crates/` → 期望 1 处（provider）
  - `grep -rn "TokenUsageSnapshot" crates/` → 期望 0 处
- [ ] **Step 7:** 在 `crates/synthia-context/src/checkpoint.rs` 添加测试：
  ```rust
  #[test]
  fn test_checkpoint_token_usage_canonical() {
      let ckpt = Checkpoint::new(...);
      let json = serde_json::to_string(&ckpt).unwrap();
      let restored: Checkpoint = serde_json::from_str(&json).unwrap();
      assert_eq!(restored.token_usage.prompt_tokens, ckpt.token_usage.prompt_tokens);
  }
  ```
- [ ] **Step 8:** 提交：`git add -A && git commit -m "test: add checkpoint token usage canonical type roundtrip"`

---

## Task 6: OpenSpec 收尾

- [ ] **Step 1:** 运行 `openspec validate unify-token-usage-types --strict` → 期望 0 错误
- [ ] **Step 2:** 运行 `git log --oneline -10` 确认所有 commit 已落
- [ ] **Step 3:** 推送：`git push origin <branch>`
- [ ] **Step 4:** 等待 CI 通过（如有）
- [ ] **Step 5:** 运行 `openspec archive unify-token-usage-types` 归档 change

---

## 风险与回滚

- **风险 1:** 序列化格式变化导致老 checkpoint 文件反序列化失败
  - **缓解:** `#[serde(default)]` 保护（`cached_prompt_tokens` 缺字段 → `None`）
  - **回滚:** `git revert <commit>` 即可
- **风险 2:** 32 处引用中某处使用字段顺序（而非字段名）反序列化
  - **缓解:** 项目统一用 `serde_json`（字段顺序无关）
  - **回滚:** `git revert <commit>` 即可
- **风险 3:** 外部用户依赖 `TokenUsageSnapshot` 路径
  - **缓解:** 项目内 `grep` 0 处使用；外部用户在 changelog 标注
  - **回滚:** `git revert <commit>` 即可
</content>
</invoke>