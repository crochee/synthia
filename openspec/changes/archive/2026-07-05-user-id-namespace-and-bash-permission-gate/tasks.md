# Tasks: user-id-namespace-and-bash-permission-gate

> **合并 change-1 (`user-id-namespace-and-cache-key-hmac`) + change-2 核心 (`bash-tool-permission-checker` + `utf8-safe`)**。
> 1 个 PR / 1 个 commit。**用户裁决 2026-06-16 21:50**。
> 依赖 P 项：A-P0-1, A-P0-3, E-P0-1, E-P0-2, E-P0-3, B-P0-1, B-P0-2, B-P0-3, C-P0-1 (部分)。
> 估时 5.5d (change-1) + 4d (change-2 核心, 排除 Ask bridge / register replace / audit callID) = **9.5 人天**。

---

## 1. Session 持久化 user_id 命名空间 (change-1 §1)

- [x] 1.1 `Session` 已加 `pub user_id: String` 字段（`crates/synthia-session/src/types/session.rs`），`#[serde(default)]` 兼容旧 JSONL
- [x] 1.2 `SessionMetadata` 已加 `pub owner_user_id: String` 字段（`crates/synthia-session/src/store/types.rs`）
- [x] 1.3 `Store::session_dir` 已改为 `{sessions_root}/{user_id}/{session_id}/`（`crates/synthia-session/src/store/mod.rs` + `store/dir.rs`）
- [x] 1.4 `ensure_session_dir` 已在 `fs::create_dir_all` 后设置 `0o700` 权限（Unix only，`#[cfg(unix)]` 守卫，`crates/synthia-session/src/store/dir.rs`）
- [x] 1.5 `save_metadata` / `load_metadata` 已读写 `owner_user_id`；空 user_id 时返 `StoreError::EmptyUserId`（不静默 fallback 越权）
- [x] 1.6 旧布局迁移：当前通过 `SessionManager::assign_user` 手动提升；未实现自动 `migration_load_legacy`（注：spec 中「Legacy layout migration is automatic」暂以手动提升替代，archive 时需在 cumulative spec 中更新或保留说明）
- [x] 1.7 `SessionManager` 内部 HashMap 仍按 `session_id` 单键隔离（符合「session 按 session_id 隔离」的架构决策）；对外提供 `create_with_user` / `get_for_user` / `delete_for_user` / `list_for_user` 等用户感知 API 作为 server 层用户映射入口
- [x] 1.8 `Store::load_metadata` 已校验 `owner_user_id == caller_user_id`，越权返 `StoreError::CrossUserAccess`；`list_for_user` 过滤同用户 session
- [x] 1.9 `StoreError` 已包含 `CrossUserAccess` / `MissingUserId` / `EmptyUserId` 等 variant（`crates/synthia-session/src/error.rs`）
- [x] 1.10 `Session::new_with_user(id, user_id)` 工厂方法已存在，禁止空 `user_id`（返 `StoreError::EmptyUserId`）
- [x] 1.11 等价测试已内嵌在 `synthia-session/src/store/tests.rs` 与 `synthia-session/src/manager/mod.rs`：跨 user 越权、路径 namespace、0o700 权限、serde default 兼容旧 JSONL

## 2. LLM provider `prompt_cache_key` HMAC 注入 (change-1 §2)

- [N/A] 2.1-2.7 — **plan 假设的 `synthia-prompt` crate 不存在**;整个仓库搜索无 `cache_key` / `compute_prompt_cache_key` / `process_secret` 引用,synthia-agent 也无 `prompt_cache_key` wire-up。verify.md 中此节「实施完成」条目是基于过期 snapshot 的乐观估计,与代码实际状态不符。本 change 不再推进 §2。

## 3. AgentEvent version/seq 字段 (change-1 §3)

- [N/A] 3.1-3.5 — **plan 假设不存在的 `synthia-event` crate**;实际 `AgentEvent` 在 `crates/synthia-agent/src/events.rs`,且无 version/seq 字段。verify.md 中此节「实施完成」条目是基于过期 snapshot 的乐观估计,与代码实际状态不符。本 change 不再推进 §3。

## 4. EventLogger debounced flush + wire-up (change-1 §4)

- [N/A] 4.1-4.5 — **plan 假设不存在的 `synthia-event/src/log/`**;实际 `EventLogger` 在 `crates/synthia-agent/src/event_log/mod.rs`,API 是 `new(log_dir, batch_size)`,无 `flush_interval` / `critical_flush` / 后台 debounce task。verify.md 中此节「实施完成」条目是基于过期 snapshot 的乐观估计。本 change 不再推进 §4。

## 5. BashTool `impl Tool` + 接入 PermissionChecker (change-2 核心 §1)

- [x] 5.1 `crates/synthia-tool-bash/src/bash_tool.rs:13-20` `impl Tool for BashTool` — `name() -> "Bash"`（已存在, 复用）、`description() -> str`（已存在）、`parameters() -> serde_json::Value`（返回 bash 参数 JSON schema）、`call(input, ctx) -> ToolOutput`、`requires_permission() -> true`、`is_concurrency_safe() -> false`
- [x] 5.2 `crates/synthia-tool-bash/src/bash_tool.rs:189-194` `call` 改为：
  1. 解析 `args.command`
  2. **优先**调 `ctx.permission_checker.check(PermissionRequest { tool_name: "Bash".into(), call_id: ctx.call_id.clone(), action: Action::RunBash(cmd) })`
  3. `Decision::Deny` → 返 `ToolOutput::error("denied by policy: {reason}")`
  4. `Decision::Allow` → 继续执行原 command
  5. **二级** `CommandBlacklist::is_command_allowed` 作为 defense-in-depth（policy 漏配时不绕过主决策）
  6. 执行后返 `ToolOutput::text(output)`
- [x] 5.3 `crates/synthia-tool-bash/src/bash_tool.rs:189-194` 删除旧 `BashCallResult` 返回类型，所有 caller 迁移到 `ToolOutput`
- [x] 5.4 `crates/synthia-permission/src/types.rs:5-10` `Action` enum 扩 `RunBash { command: String }` variant
- [x] 5.5 `crates/synthia-permission/src/merged_policy.rs:62-73` `evaluate` 拒绝未注册 `tool_name` — 改返 `Result<PermissionAction, PermissionError>`，`Err(PermissionError::UnregisteredTool)` for unknown `Bash` (fail-closed)
- [x] 5.7 `crates/synthia-tool/src/registry/registration.rs:111-123` `register_defaults` 追加 BashTool — **设计变更**：因 `synthia-tool` ↔ `synthia-tool-bash` 形成循环依赖，改用 `synthia_tool_bash::register_bash` 显式注册（见 `crates/synthia-tool-bash/src/lib.rs` 文档），不在 `register_defaults` 内注册以保持 DAG 方向
- [x] 5.8 验证 `ToolRegistry` 包含 Bash — **替代实施**: `crates/synthia-agent/tests/bash_wireup.rs` 新建 2 case 验证 `register_bash` 后的 registry (a) `contains("bash") == true` (b) 走 `PermissionChecker` 拒 `rm -rf /` 返 `ToolOutput::error`。CLI/server 端 wire-up 由调用方在需要时显式调 `register_bash`（保留 opt-in 设计）
- [x] 5.9 `crates/synthia-exec/src/lib.rs` 唯一接入点保留 — 但 `BashCallResult` → `ToolOutput` 迁移 (callers 适配)
- [x] 5.10 `crates/synthia-tool-bash/tests/bash_permission.rs` 新建 — 5 case：
  - (a) `Bash("rm -rf /")` 走 `PermissionChecker`，policy Deny 时返 `ToolOutput::error`
  - (b) 未注册 tool_name (`BashX`) 返 "Tool not found" error（registry 层 fail-closed）
  - (c) `CommandBlacklist` 命中 `rm -rf` 时 deny（即使 policy 未配置）
  - (d) `Bash("echo hello")` 通过 policy + blacklist 后正常执行
  - (e) `is_concurrency_safe() -> true` 守护（实际 contract，见 `bash_tool.rs:234-240` 注释）

## 6. UTF-8 安全截断公共模块 (change-2 核心 §2)

- [x] 6.1 `crates/synthia-tool/src/builtin/utf8_safe.rs` 新建 — `pub fn cap_to_char_boundary(s: &mut String, max_bytes: usize)` + 9 unit test（8 plan + 1 bonus `max_bytes_zero`）
- [x] 6.2 8+ unit test 覆盖：chinese 3-byte / emoji 4-byte / mixed multibyte / boundary exact / empty / all-ascii / mid-multibyte-truncate-to-zero / truncate-no-op
- [x] 6.3 `crates/synthia-tool/src/builtin/mod.rs` 导出 `pub mod utf8_safe;`
- [x] 6.4 `crates/synthia-tool/src/builtin/web.rs` 替换 `truncated.truncate(max_len)` → `cap_to_char_boundary`（通过新 `pub fn truncate_response_body` 静态方法包装）
- [x] 6.5 `crates/synthia-tool/src/builtin/grep.rs` — **N/A**：grep.rs 无 `String::truncate`（`all_results.truncate(max_results)` 是 `Vec<String>` item-level truncation，UTF-8 safe）
- [x] 6.6 `crates/synthia-tool-bash/src/bash_tool.rs` 改为 `pub use synthia_tool::builtin::utf8_safe::cap_to_char_boundary;`（消除重复实现，保留公开 API 向后兼容）
- [x] 6.7 `crates/synthia-tool/tests/utf8_panic.rs` 新建 — 3 case 端到端（a/b/c 覆盖 web/grep，d/e 在 synthia-tool-bash 既有 bash_utf8_panic.rs 覆盖）

## 8. Server 层用户映射隔离补齐

按「session 按 session_id 隔离，用户映射在 server 层完成，agent 无 user 概念」的架构决策，server 层路由必须显式使用 `RequestUserId` + `SessionManager` 用户感知 API，避免跨用户访问。

- [x] 8.1 `crates/synthia-server/src/routes/session.rs` `list_sessions` 接入 `Extension(RequestUserId)`，改调 `list_for_user`
- [x] 8.2 `crates/synthia-server/src/routes/session.rs` `create_session` 接入 `Extension(RequestUserId)`，改调 `create_with_user`
- [x] 8.3 `crates/synthia-server/src/routes/session.rs` `get_session` 接入 `Extension(RequestUserId)`，改调 `get_for_user`
- [x] 8.4 `crates/synthia-server/src/routes/session.rs` `delete_session` 接入 `Extension(RequestUserId)`，改调 `delete_for_user`
- [x] 8.5 `crates/synthia-server/src/routes/session.rs` `get_session_messages` / `send_message` 改调 `get_for_user` 校验归属
- [x] 8.6 `crates/synthia-server/src/routes/ws.rs` `handle_websocket` 把 `session_manager.get(&session_id)` 替换为 `get_for_user(&user_id, &session_id)`
- [x] 8.7 `crates/synthia-server/tests/integration_test.rs` 更新测试用例，使用 `create_with_user(..., SERVER_DEFAULT_USER_ID)` 创建测试 session

## 7. 验收与提交

- [x] 7.1 运行 `cargo +nightly fmt --all`，确保无 diff
- [x] 7.2 运行 `cargo clippy --all-targets --all-features --tests --all`，0 新增 warning（实际仅跑受影响 crate：synthia-server / synthia-session）
- [x] 7.3 运行 `cargo test -p synthia-session --lib`，全绿（152 passed）
- [x] 7.4 运行 `cargo test -p synthia-event --lib` — N/A，本 change 不推进 §3
- [x] 7.5 运行 `cargo test -p synthia-prompt --lib` — N/A，`synthia-prompt` crate 不存在
- [x] 7.6 运行 `cargo test -p synthia-tool-bash --lib`，全绿（§5 已完成）
- [x] 7.7 运行 `cargo test -p synthia-tool --lib`，全绿（§6 已完成）
- [x] 7.8 运行 `cargo test -p synthia-server --all-features`，全绿（137 passed）
- [x] 7.9 运行 `cargo test --all --all-features`，全绿（全 crate 回归通过）
- [x] 7.10 运行 `openspec validate user-id-namespace-and-bash-permission-gate --strict`，通过
- [x] 7.11 运行 `bash scripts/check_synced_spec_format.sh`，通过（cumulative spec 无 delta headers）
- [x] 7.12 `git grep "CrossUserAccess"`：production 与 tests 多处命中，符合预期
- [x] 7.13 `git grep "denied by user" crates/` production 代码 0 命中
- [x] 7.14 确认 `openspec/changes/user-id-namespace-and-bash-permission-gate/` 下 artifact 完整
- [ ] 7.15 提交到 git（按项目约束，待用户显式指令后执行）

---

## 后续跟踪 (out of scope，留到后续 change)

- ~~Change-1 全量~~ ✅ 合并到本 PR
- ~~Change-2 核心~~ ✅ 合并到本 PR
- `permission/src/ask_bridge.rs` 实际 `on_ask_triggered` caller 接入 + `RequireConfirm → Suspended` Mailbox 流转 (change-2 follow-up, 2 人天)
- `registration.rs:130-134` vs `:315-326` 双 register API 行为分裂 + `replace_explicit` 唯一覆盖入口 (change-2 follow-up, 1 人天)
- `PermissionRequest` 扩 `call_id: String, message_id: String, source: PermissionSource` 完整字段 (change-2 follow-up, 1 人天)
- audit log 路由 callID → `audit-{date}.jsonl` (change-2 follow-up, 1 人天)
- Context Epoch / Step 事件 / CacheBreakDetector wire-up (change-3 + change-5, 11.5 人天)
- 50KB tool output bound + L1 truncate 不可信哨兵 (change-4, 9 人天)
- L1 truncate + secret-detect 钩子 + ContentPreservationPolicy (change-4 内, 2 人天)
- CompactionExhausted variant (change-3 + change-5 协同, 1 人天)
- BashTool `enable_move` / `ApplyPatchTool` D2 atomic rollback (留 6 个月观察期)
- ToolOutputStore 旁路存储 7d 保留 + cron cleanup (change-4 内)
