## Why

Synthia 当前 session 持久化层和 bash 工具层存在 3 个独立的 P0 攻击面/正确性漏洞，对应 opencode / codex 同类实现已经全部闭环：

1. **跨用户 session 越权 + cache hash 可预测**：`Store::session_dir` (`crates/synthia-session/src/store.rs:54-55`) 直接 `self.sessions_root.join(session_id)` — 没有 `user_id` 命名空间；`SessionMetadata` (`store.rs:18-27`) 和 `Session` (`types.rs:122-133`) 没有 `owner_user_id` / `user_id` 字段。`list_sessions_with_metadata` (`manager.rs:405-411`) 不传 `caller_user_id`，用户 A 可以枚举到用户 B 的 session。同时 LLM provider 的 `prompt_cache_key` 当前使用 `session_id` 截断或简单 hash（`builder.rs:380-385`）— 跨用户 session_id 空间是公开可枚举的，cache key 没有任何 namespace 隔离。**违反 project memory 硬约束「cache hash 必须含 user_id 命名空间」**。

2. **BashTool 完全绕过 `PermissionChecker`**：`BashTool` (`crates/synthia-tool-bash/src/bash_tool.rs:13-20`) 没有 `impl Tool` trait；`BashTool::call` (`bash_tool.rs:189-194`) 直接调 `self.sandbox.is_command_allowed(command)` 走 `CommandBlacklist`，**不**经 `ToolRegistry::execute` 的 `PermissionChecker::check` 路径。`ToolRegistry::register_defaults` (`crates/synthia-tool/src/registry/registration.rs:111-123`) 不注册 `BashTool` — 7 个内置工具（read/write/glob/grep/multi_edit/apply_patch/web_fetch）全在列表里，唯独 bash 缺失。Guardian UI 对 bash 路径**完全失明**（5/5 强信号 — 维度 B 对抗性审查共识 1.1 + 维度 C 对抗性审查共识 C1.1）。

3. **web.rs 在 CJK/emoji 上 panic**：`WebFetchTool` (`crates/synthia-tool/src/builtin/web.rs:147-148`) 写 `truncated.truncate(max_len)` — `String::truncate` 在 byte index 落在 UTF-8 多字节字符内部时**直接 panic**。同仓 `bash_tool.rs:320-335` 已有 8 个 unit test 守护的 `cap_to_char_boundary` 实现，但仅 `pub(super)` 私有，web.rs / grep.rs 复用了不安全的 `truncate`。

参考 `~/workspace/opencode` 的 session store（按 `user_id` 一级目录隔离 + Anthropic `cache_control` 命名空间）和 `~/workspace/codex` 的 `core/src/apply_patch.rs`（`assess_patch_safety` 3 变体决策点）— 这两个 P0 漏洞在生产级 agent 中是 fail-closed 默认行为。本次合并修复的目的：**1 个 PR，1 个 commit，闭环 3 个 P0 漏洞 + 满足 1 个 project memory 硬约束**。

## What Changes

### 1. Session 持久化 user_id 命名空间 (Change-1 范围)
- From: `Session` 无 `user_id` 字段；`SessionMetadata` 无 `owner_user_id` 字段；`Store::session_dir` 路径 `{sessions_root}/{session_id}/`；`fs::create_dir_all` 后无 `set_permissions`；`list_sessions_with_metadata` 不传 caller
- To: `Session` 扩 `user_id: String`；`SessionMetadata` 扩 `owner_user_id: String`；`session_dir` 改 `{user_id}/{session_id}`；`create_dir_all` 后紧跟 `set_permissions(0o700)`；`list_sessions_with_metadata(caller_user_id)` 越权返 `Err(HashChainError::CrossUserAccess)`；所有新字段 `#[serde(default)]` 兼容旧 JSONL
- Reason: 满足 project memory 硬约束 + 防止跨用户 session 越权枚举
- Impact: 破坏性 — 旧 session 路径无 user_id 中间层，需 migration shim (在 `Store::load` 兼容旧布局)

### 2. LLM provider `prompt_cache_key` HMAC 注入 (Change-1 范围)
- From: `builder.rs:380-385` 使用 `session_id` 简单截断或字符串 fallback 作 `prompt_cache_key`
- To: 注入 `providerOptions.prompt_cache_key = HMAC-SHA256(user_id || session_id)[:32]`，Anthropic 走 `cache_control` 命名空间隔离
- Reason: 满足硬约束「cache hash 必须含 user_id 命名空间」；opencode 模式改造（拒绝 #1 session.id.slice(4) 原样）
- Impact: 内部模块，对外只暴露 1 个 `compute_prompt_cache_key(user_id, session_id) -> String` 公共函数

### 3. AgentEvent version/seq 字段 (Change-1 范围)
- From: 36+ `AgentEvent` variant 无 `version` / `seq` 字段
- To: 全部 variant 加 `version: u32` + `seq: u64`，`#[serde(default)]` 兼容；`AgentEventEmitter::pair()` 用 `AtomicU64` 单调分配
- Reason: 为后续 P1 change (context-epoch-and-step-events) 提供版本/序号基础
- Impact: 内部 — 旧 reader 不破

### 4. EventLogger debounced flush + wire-up (Change-1 范围)
- From: `EventLogger::new` 无 `flush_interval`；`synthia-agent::run` 0 调用方 (CONFIRMED)
- To: `EventLogger::new(flush_interval: Duration)` 启动 50ms flush task；`Decision/Error/ToolResult{is_error}` 立即 `write_all + sync_all` 不入 debounce；`synthia-agent::run` wire-up
- Reason: 决策/错误事件不丢失（per P8 不丢信息）；opencode pattern
- Impact: `synthia-agent` 增加 1 个 `EventLogger` 依赖

### 5. BashTool `impl Tool` + 接入 PermissionChecker (Change-2 核心)
- From: `BashTool` 无 `impl Tool`；`BashTool::call` 走 `CommandBlacklist` 不经 `PermissionChecker`；`register_defaults` 不含 `BashTool`
- To: `BashTool` `impl Tool { name, description, parameters, call -> ToolOutput, requires_permission -> true, is_concurrency_safe -> false }`；`call` 内 `PermissionChecker::check(PermissionRequest { call_id: tu.id, message_id, source: User, action: RunBash(cmd) })` 优先于执行；`CommandBlacklist` 退化为 defense-in-depth 二级检查（不主决策）；`register_defaults` 追加 `BashTool::new().into_arc()`
- Reason: 闭环 P0 漏洞 #2；BashTool 接 PermissionChecker 后 Guardian UI 审计可达（per P6 不信任 LLM）；保留 CommandBlacklist 作为 defense-in-depth 防止 policy 漏配
- Impact: 破坏性 — 旧 `BashTool::call(args) -> BashCallResult` 调用点全部迁移到 `Tool::call(input, ctx) -> ToolOutput`；permission checker 接收新 `Bash` tool_name 规则

### 6. UTF-8 安全截断公共模块 (Change-2 核心)
- From: `web.rs:147-148` 用 `String::truncate(max_len)` 在 CJK/emoji 上 panic；`grep.rs:34-40` 同问题；`bash_tool.rs:320-335` 的 `cap_to_char_boundary` 私有
- To: 新建 `crates/synthia-tool/src/builtin/utf8_safe.rs` 公共模块，导出 `cap_to_char_boundary`；`web.rs` + `grep.rs` 替换；8 个 unit test 上提到公共位置
- Reason: 闭环 P0 漏洞 #3；满足 project memory 硬约束「Bash tool output truncation must handle multi-byte UTF-8 characters to prevent panic」
- Impact: 内部 — 行为等价（正确性 fix），无 API 破坏

## Capabilities

### New Capabilities
- `user-id-and-bash-gate`: 闭环 (a) session 跨用户 user_id 命名空间 + 0o700 目录权限 + serde 兼容；(b) LLM provider `prompt_cache_key` HMAC-SHA256(user_id || session_id) 注入；(c) AgentEvent version/seq 字段 + EventLogger debounced flush + critical bypass；(d) BashTool `impl Tool` + 接入 `PermissionChecker` + register_defaults；(e) `utf8_safe::cap_to_char_boundary` 公共模块 + web.rs/grep.rs 替换 + 8 边界 case test

### Modified Capabilities
- (none — 不修改现有 spec 的 requirement，仅在 `synthia-session` / `synthia-event` / `synthia-agent` / `synthia-tool-bash` / `synthia-tool` / `synthia-permission` crate 内 additive 改动)

## Impact

**Affected code**:
- `crates/synthia-session/src/types.rs:122-133` — `Session` 扩 `user_id: String` 字段
- `crates/synthia-session/src/store.rs:18-27` — `SessionMetadata` 扩 `owner_user_id: String` 字段
- `crates/synthia-session/src/store.rs:54-55` — `session_dir` 改 `{user_id}/{session_id}`
- `crates/synthia-session/src/store.rs:61` — `fs::create_dir_all` 后 `set_permissions(0o700)`
- `crates/synthia-session/src/store.rs:99-130` — `save_metadata` + `load_metadata` 写入/读取新字段 + migration shim (旧布局 fallback)
- `crates/synthia-session/src/manager.rs:78` — HashMap 键改 `(String, String)` = `(user_id, session_id)`
- `crates/synthia-session/src/manager.rs:405-411` — `list_sessions_with_metadata(caller_user_id)` 过滤 + 越权 `Err(CrossUserAccess)`
- `crates/synthia-event/src/log/types.rs:6-15` — `EventLogEntry` 扩 `user_id` + `session_seq: u64`
- `crates/synthia-event/src/events.rs:78-238` — 36+ variant 加 `version: u32` + `seq: u64`，`#[serde(default)]`
- `crates/synthia-event/src/events.rs:275-298` — `AgentEventEmitter::pair()` 用 `AtomicU64` 单调分配 seq
- `crates/synthia-event/src/log/mod.rs:27-99` — `EventLogger::new(flush_interval: Duration)` + 50ms flush task + critical bypass
- `crates/synthia-agent/src/stream_builder/builder.rs:360-362` — 删除 `format!("turn-{}", ctx.iteration)` 字符串 fallback，改 `TurnId::next()` + `iteration: u64` 双字段
- `crates/synthia-agent/src/stream_builder/builder.rs:380-385` — 注入 `providerOptions.prompt_cache_key = HMAC-SHA256(user_id ‖ session_id)[:32]`
- `crates/synthia-agent/src/run.rs` — wire-up `EventLogger::new(Duration::from_millis(50))` 启动
- `crates/synthia-tool-bash/src/bash_tool.rs:13-20` — `impl Tool for BashTool`
- `crates/synthia-tool-bash/src/bash_tool.rs:189-194` — `call` 改走 `PermissionChecker::check` + 返 `ToolOutput`
- `crates/synthia-tool/src/registry/registration.rs:111-123` — `register_defaults` 追加 `BashTool::new().into_arc()`
- `crates/synthia-tool/src/builtin/utf8_safe.rs` — 新建公共模块（`cap_to_char_boundary` + 8 unit test）
- `crates/synthia-tool/src/builtin/web.rs:147-148` — 替换 `truncate(max_len)` 为 `cap_to_char_boundary(&mut s, max_len)`
- `crates/synthia-tool/src/builtin/grep.rs:34-40` — 同上

**Affected APIs**:
- `Session` struct — 新增 `user_id: String` 字段 (backward compat via `#[serde(default)]`)
- `SessionMetadata` struct — 新增 `owner_user_id: String` 字段 (backward compat)
- `Store::session_dir` — 签名不变但路径 layout 变化
- `Store::list_sessions_with_metadata` — 新增 `caller_user_id: &str` 参数
- `AgentEvent` (36+ variants) — 新增 `version: u32` + `seq: u64` 字段
- `EventLogger::new` — 新增 `flush_interval: Duration` 参数
- `BashTool::call` — 签名从 `(args: &Value) -> BashCallResult` 改为 `Tool::call(input, ctx) -> ToolOutput`
- `synthia-tool::builtin::utf8_safe` — 新增公共模块

**Affected systems**:
- LLM provider protocol — `providerOptions.prompt_cache_key` 注入；旧 session_id 截断方式不再使用
- Tool registry — `Bash` 加入默认注册列表，LLM tool list 长度 +1
- Permission checker — `Bash` tool_name 规则注册；`CommandBlacklist` 退化为 defense-in-depth
- Event sourcing — `AgentEvent` version/seq 字段；EventLogger debounce 行为
- Filesystem layout — session 路径从 `{session_id}/` 改 `{user_id}/{session_id}/`，需要 migration shim

**Dependencies**: 1 个新增 `hmac` + `sha2` 来自 `RustCrypto`（已间接依赖 `sha2` 用于其他模块；新增 `hmac` 0.12 + `sha2` 0.10 显式声明在 `synthia-prompt/Cargo.toml`）。需在 `Cargo.toml` 显式声明 `hmac` 0.12 (workspace 尚未引入)。

**Test coverage**:
- 4 case `tests/user_id_namespace.rs` — 跨 user 越权 / 路径 namespace / 0o700 权限位 / serde default 兼容
- 3 case `synthia-event` — 旧 reader 兼容 / seq 单调 / debounce 行为
- 6 case `synthia-tool` — utf8_safe (chinese/emoji/mixed/empty/all-ascii/mid-multibyte)
- 5 case `tests/bash_fail_closed.rs` — BashTool 走 PermissionChecker / 未注册 tool 拒绝 / HMAC 决定性 / CommandBlacklist 退化为二级 / 0o700 路径验证
- property test — HMAC `proptest` ≥100 case 跨 (user_id, session_id) 组合无碰撞前缀
- 现有 `cargo test --all` 全部通过（确认无回归）

**Out of scope (留到后续 change)**:
- `permission/src/ask_bridge.rs` 实际 `on_ask_triggered` caller 接入（change-2 follow-up）
- `registration.rs:130-134` 双 register API 行为分裂 + `replace_explicit` 唯一覆盖入口
- `PermissionRequest` 扩 `call_id/message_id/source` 全字段（change-2 后续 PR）
- audit log 路由 callID
- 50KB tool output bound + L1 truncate 不可信哨兵 (change-4 `tool-output-store-trust-and-injection-scan`)
- Context Epoch / Step 事件 / CacheBreakDetector wire-up (change-3 / change-5)
