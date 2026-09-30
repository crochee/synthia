# Verify: user-id-namespace-and-bash-permission-gate

> **2026-07-05 实际完成状态**
>
> 按「session 按 `session_id` 隔离，用户映射在 server 层完成，agent 本身无 user 概念」的架构决策，本 change 实际交付：
>
> - ✅ §1 `synthia-session` user_id namespace（代码已落地，server 层通过 `RequestUserId` + `SessionManager` 用户感知 API 完成映射）
> - ✅ §5 `BashTool` permission gate（已落地）
> - ✅ §6 UTF-8 safe truncation（已落地）
> - ✅ 新增 server 层路由隔离补齐（legacy `/api/sessions/*` + WebSocket 校验归属）
> - ❌ §2 / §3 / §4 N/A（plan 假设的 `synthia-prompt` / `synthia-event` crate 不存在，相关需求待后续 change）

---

## 0. Evidence (实际)

- **架构边界**：
  - `synthia-session` 内部仍以 `session_id` 为 HashMap 键；用户隔离通过 `create_with_user` / `get_for_user` / `delete_for_user` / `list_for_user` 等 API 在调用层实现。
  - `synthia-server` 的 auth middleware 从 API key 解析/派生 `user_id`，通过 `Extension(RequestUserId)` 注入路由；路由再调用 `SessionManager` 用户感知 API。
  - `AgentRunConfig` 仍保留 `user_id` 字段用于持久化 namespace，但 agent 本身不做授权决策（授权在 server 层 + `SessionManager` 完成）。

- **实际交付的 crate 改动**：
  - `crates/synthia-session/src/types/session.rs` — `Session::user_id` + `new_with_user`
  - `crates/synthia-session/src/store/types.rs` — `SessionMetadata::owner_user_id`
  - `crates/synthia-session/src/store/dir.rs` — `{root}/{user_id}/{session_id}/` + `0o700`
  - `crates/synthia-session/src/store/metadata.rs` — `load_metadata` 越权校验
  - `crates/synthia-session/src/manager/core.rs` — `create_with_user` / `get_for_user` / `delete_for_user` / `list_for_user`
  - `crates/synthia-session/src/error.rs` — `CrossUserAccess` / `EmptyUserId` / `MissingUserId`
  - `crates/synthia-session/src/store/tests.rs` / `src/manager/mod.rs` — user namespace 测试
  - `crates/synthia-server/src/routes/session.rs` — legacy `/api/sessions/*` 全面接入 `RequestUserId`
  - `crates/synthia-server/src/routes/ws.rs` — WebSocket 连接前校验 session 归属
  - `crates/synthia-server/tests/integration_test.rs` — 测试用例适配用户感知 API
  - `crates/synthia-tool-bash/src/bash_tool.rs` — `impl Tool` + `PermissionChecker`
  - `crates/synthia-permission/src/types.rs` — `Action::RunBash`
  - `crates/synthia-tool/src/builtin/utf8_safe.rs` — `cap_to_char_boundary`

- **Subagent dispatches**: 0
- **New external dependencies**: 0

---

## 1. Spec Compliance (实际进度)

| Requirement | 实际 Status |
|-------------|--------|
| Session Persistence User-ID Namespace — 4 scenario | ✅ 实施完成；legacy 自动迁移改为 `assign_user` 手动提升 |
| promptCacheKey HMAC Includes User-ID Namespace | ❌ N/A (`synthia-prompt` crate 不存在) |
| AgentEvent Version and Sequence Fields | ❌ N/A (`synthia-event` crate 不存在) |
| EventLogger Debounced Flush With Critical Bypass | ❌ N/A (`synthia-event` crate 不存在) |
| BashTool Routes Through PermissionChecker — 5 scenario | ✅ 实施完成 |
| UTF-8 Safe Truncation Public Helper — 5 scenario | ✅ 实施完成 |

---

## 2. Verification Results

| Check | Result |
|-------|--------|
| `cargo +nightly fmt --all` | ✅ 无 diff — 2026-07-05 |
| `cargo clippy -p synthia-server --all-targets --all-features --tests` | ✅ 0 新增 warning — 2026-07-05 |
| `cargo clippy -p synthia-session --all-targets --all-features --tests` | ✅ 0 新增 warning — 2026-07-05 |
| `cargo test -p synthia-session --lib` | ✅ 152 passed — 2026-07-05 |
| `cargo test -p synthia-server --all-features` | ✅ 137 passed — 2026-07-05 |
| `cargo test -p synthia-tool-bash --lib` | ✅ 已通过（§5 历史完成） |
| `cargo test -p synthia-tool --lib` | ✅ 已通过（§6 历史完成） |
| `cargo test -p synthia-agent --test bash_wireup` | ✅ 已通过（§5 历史完成） |
| `openspec validate user-id-namespace-and-bash-permission-gate --strict` | ⏳ 待归档前运行 |
| `bash scripts/check_synced_spec_format.sh` | ⏳ 待 spec 同步到 cumulative 时 |

---

## 3. 关键 Acceptance Gate

- [x] `synthia-session` 路径 namespace `{root}/{user_id}/{session_id}/` 正确
- [x] `synthia-session` 目录权限 `0o700`
- [x] `Store::load_metadata` 越权返 `CrossUserAccess`
- [x] `SessionManager::list_for_user` / `get_for_user` / `delete_for_user` 仅返回/操作当前用户 session
- [x] `synthia-server` legacy `/api/sessions/*` 路由全部接入 `RequestUserId`
- [x] `synthia-server` WebSocket 连接校验 session 归属
- [x] `synthia-tool-bash` 走 `PermissionChecker`
- [x] `synthia-tool` UTF-8 截断无 panic

---

## 4. Cross-Crate Compatibility

- **synthia-session**: user_id namespace 相关 API 已稳定
- **synthia-server**: legacy 路由补齐用户隔离；与 V2 路由共用同一套 `RequestUserId` + `SessionManager` 用户感知 API
- **synthia-permission**: `Action::RunBash`
- **synthia-tool-bash**: `impl Tool` + `register_bash`
- **synthia-tool**: `utf8_safe::cap_to_char_boundary`

---

## 5. Delta Spec Sync

Delta spec 保留在 `openspec/changes/user-id-namespace-and-bash-permission-gate/specs/user-id-and-bash-gate/spec.md`（`## ADDED Requirements` 格式）。

归档时需同步到 cumulative spec `openspec/specs/user-id-and-bash-gate/spec.md`：
- 添加 `## Purpose`
- `## ADDED Requirements` → `## Requirements`
- 保留实际实施的 requirements；§2-§4 标记为 N/A 或移除
- 更新 legacy migration scenario 描述（自动迁移 → `assign_user` 手动提升）

---

## 6. Open Items (out of scope)

- `synthia-prompt` cache_key HMAC 注入
- `AgentEvent` version/seq 字段
- `EventLogger` debounced flush + critical bypass
- `SessionManager` 内部 HashMap 是否改为 `(user_id, session_id)` 键（当前架构决策：保持 `session_id` 单键，server 层做用户映射）
- `AgentRunConfig.user_id` 是否下沉到 `SessionStore` 包装（长期架构清理）
