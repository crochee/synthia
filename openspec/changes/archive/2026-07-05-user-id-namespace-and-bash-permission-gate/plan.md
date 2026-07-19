# user-id-namespace-and-bash-permission-gate Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task. **1 个 PR / 6 个小 commit / 9.5 人天**。
>
> 严格按 tasks.md 顺序实施，每节独立可读可回滚。

**Goal:** 闭环 3 个 P0 漏洞（session 越权 + bash 绕过 + UTF-8 panic）+ 满足 1 个 project memory 硬约束（cache hash user_id 命名空间 + UTF-8 安全截断）。

**Architecture:** 4 crate 协同（synthia-session / synthia-event / synthia-prompt / synthia-agent / synthia-tool-bash / synthia-tool / synthia-permission）— additive 改动，不引入新 trait 抽象。

**Tech Stack:** Rust, `hmac 0.12` + `sha2 0.10` (新依赖), `rand 0.8` (已存在), `tokio` (已存在), `serde` (已存在), `chrono` (已存在).

---

## 实施顺序（6 个小 commit，与 tasks.md §1-§6 对齐）

| Commit | tasks.md § | 范围 | 工作量 |
|--------|------------|------|--------|
| 1 | §1 | Session 持久化 user_id 命名空间 | 2.0d |
| 2 | §2 | LLM provider prompt_cache_key HMAC | 1.0d |
| 3 | §3 | AgentEvent version/seq 字段 | 0.5d |
| 4 | §4 | EventLogger debounced flush + wire-up | 0.5d |
| 5 | §5 | BashTool impl Tool + PermissionChecker | 4.0d |
| 6 | §6 | UTF-8 安全截断公共模块 | 1.5d |
| 7 | §7 | 验收与提交 (fmt + clippy + test + commit) | 0d |
| **总计** | - | - | **9.5d** |

---

## Task 1: Session 持久化 user_id 命名空间 (TDD)

**Files:**
- `crates/synthia-session/src/types.rs:122-133` (Session 加 user_id 字段)
- `crates/synthia-session/src/store.rs:18-27, 54-55, 61, 99-130` (SessionMetadata + session_dir + 0o700 + migration)
- `crates/synthia-session/src/manager.rs:78, 405-411` (HashMap 键 + list 过滤)
- `crates/synthia-session/src/error.rs` (新 HashChainError variant)
- `crates/synthia-session/src/session_constructor.rs` (新文件)
- `crates/synthia-session/tests/user_id_namespace.rs` (新测试)

- [ ] **Step 1.1:** 在 `Session` (`types.rs:122-133`) 加 `pub user_id: String` 字段，`#[serde(default)]`；`SessionMetadata` (`store.rs:18-27`) 加 `pub owner_user_id: String` 字段
- [ ] **Step 1.2:** `Store::session_dir` (`store.rs:54-55`) 改 `self.sessions_root.join(user_id).join(session_id)`；签名不变
- [ ] **Step 1.3:** `Store::ensure_session_dir` (`store.rs:61`) `fs::create_dir_all` 后紧跟 `#[cfg(unix)] set_permissions(0o700)`
- [ ] **Step 1.4:** 在 `error.rs` 新增 `pub enum HashChainError { CrossUserAccess { caller: String, owner: String }, MissingUserId { session_id: String } }`
- [ ] **Step 1.5:** 新建 `session_constructor.rs` (~30 行) `pub fn new_with_user(id: String, user_id: String) -> Result<Session, StoreError>` — 禁止空 `user_id`
- [ ] **Step 1.6:** `Manager` HashMap (`manager.rs:78`) 键改 `(String, String)` = `(user_id, session_id)`；提供 `get_mut` / `insert` 包装方法
- [ ] **Step 1.7:** `list_sessions_with_metadata` (`manager.rs:405-411`) 接收 `caller_user_id: &str`；越权返 `Err(HashChainError::CrossUserAccess)`
- [ ] **Step 1.8:** 在 `store.rs` 新增 `pub fn migration_load_legacy(session_id: &str) -> Result<Session>` — 旧布局一次性迁移
- [ ] **Step 1.9:** 写 `tests/user_id_namespace.rs` 4 case（写测试 → 跑 → 实现 → 跑）：
  - `test_cross_user_session_load_refused` — alice 不能 load bob 的 session
  - `test_path_namespace_layout` — 路径是 `sessions/alice/{sessid}/`
  - `test_user_id_directory_mode_0o700` — Unix 权限位 0o700
  - `test_serde_default_backward_compat` — 旧 JSONL 无 user_id 字段可读
- [ ] **Step 1.10:** `cargo test -p synthia-session --lib` — 4 case 全过
- [ ] **Step 1.11:** `cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features --tests -p synthia-session` — 0 新增 warning

**Commit:** `fix(session): namespace session by user_id with 0o700 directory permission`

---

## Task 2: LLM provider `prompt_cache_key` HMAC 注入 (TDD)

**Files:**
- `crates/synthia-prompt/Cargo.toml` (新依赖 hmac 0.12 + sha2 0.10)
- `crates/synthia-prompt/src/cache_key.rs` (新模块)
- `crates/synthia-prompt/src/process_secret.rs` (新模块)
- `crates/synthia-prompt/src/lib.rs` (导出 cache_key)
- `crates/synthia-prompt/tests/cache_key.rs` (新测试)
- `crates/synthia-agent/src/stream_builder/builder.rs:380-385` (注入)
- `crates/synthia-agent/tests/prompt_cache_key.rs` (新测试)

- [ ] **Step 2.1:** `synthia-prompt/Cargo.toml` 显式声明 `hmac = "0.12"` + `sha2 = "0.10"`
- [ ] **Step 2.2:** 新建 `cache_key.rs` (~50 行) `pub fn compute_prompt_cache_key(user_id: &str, session_id: &str, secret: &[u8]) -> String` — HMAC-SHA256 截断 32 hex
- [ ] **Step 2.3:** 新建 `process_secret.rs` (~20 行) `pub fn process_secret() -> [u8; 32]` — 进程内 `rand::thread_rng().gen()` 单例
- [ ] **Step 2.4:** `lib.rs` 导出 `pub mod cache_key;` + `pub mod process_secret;`
- [ ] **Step 2.5:** 写 `tests/cache_key.rs` 5 case + 1 property test (TDD)：
  - `test_hmac_deterministic_fixed_inputs` — 同输入 → 同输出
  - `test_different_users_different_keys` — alice 和 bob 同 session_id → 16 字符前缀不同
  - `test_empty_user_id_legal` — 空 user_id 不 panic
  - `test_32_hex_char_length` — 输出 32 hex
  - `test_no_collision_property` — `proptest` ≥100 case 跨 (user_id, session_id) 无碰撞
- [ ] **Step 2.6:** `cargo test -p synthia-prompt --lib` — 5 case + 1 property 全过
- [ ] **Step 2.7:** `stream_builder/builder.rs:380-385` 注入 `providerOptions.prompt_cache_key = compute_prompt_cache_key(&ctx.user_id, &ctx.session_id, &process_secret())`
- [ ] **Step 2.8:** 写 `tests/prompt_cache_key.rs` 2 case：
  - `test_wireup_injects_cache_key` — wire-up 后 providerOptions 含 cache_key
  - `test_empty_user_id_rejected` — 拒绝空 user_id
- [ ] **Step 2.9:** `cargo test -p synthia-agent --lib prompt_cache_key` — 2 case 全过
- [ ] **Step 2.10:** `cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features --tests -p synthia-prompt -p synthia-agent` — 0 新增 warning

**Commit:** `feat(prompt,agent): HMAC-SHA256 prompt_cache_key with user_id namespace`

---

## Task 3: AgentEvent version/seq 字段 (TDD)

**Files:**
- `crates/synthia-event/src/events.rs:78-238, 275-298` (36+ variant + AtomicU64)
- `crates/synthia-event/tests/version_seq.rs` (新测试)

- [ ] **Step 3.1:** `events.rs:78` 模块顶部加 `pub const AGENT_EVENT_SCHEMA_VERSION: u32 = 1;`
- [ ] **Step 3.2:** 36+ `AgentEvent` variant 全部加 `version: u32 = AGENT_EVENT_SCHEMA_VERSION` + `seq: u64` 字段，`#[serde(default)]`
- [ ] **Step 3.3:** `AgentEventEmitter::pair()` (`events.rs:275-298`) 用 `AtomicU64` 单调分配 seq
- [ ] **Step 3.4:** 写 `tests/version_seq.rs` 3 case（TDD）：
  - `test_old_reader_loads_new_event` — 新 schema 写入的 event 旧 reader 可读
  - `test_new_reader_loads_old_event` — 旧 schema 写入的 event 新 reader 可读
  - `test_seq_monotonically_increasing` — 100 次调用 seq 严格递增
- [ ] **Step 3.5:** `cargo test -p synthia-event --lib version_seq` — 3 case 全过
- [ ] **Step 3.6:** `cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features --tests -p synthia-event` — 0 新增 warning

**Commit:** `feat(event): add version and seq fields to AgentEvent variants`

---

## Task 4: EventLogger debounced flush + wire-up (TDD)

**Files:**
- `crates/synthia-event/src/log/mod.rs:27-99, 94-96` (flush_interval + critical bypass)
- `crates/synthia-event/tests/debounce.rs` (新测试)
- `crates/synthia-agent/src/run.rs` (wire-up)

- [ ] **Step 4.1:** `EventLogger::new(flush_interval: Duration)` — 启动 `tokio::time::interval` 后台 task，每 tick `sync_all`
- [ ] **Step 4.2:** 分类 flush：`Decision/Error/ToolResult{is_error}` 立即 `write_all + sync_all`（critical_flush），其他入 debounce
- [ ] **Step 4.3:** `synthia-agent/src/run.rs` wire-up `EventLogger::new(Duration::from_millis(50))` 启动
- [ ] **Step 4.4:** 写 `tests/debounce.rs` 3 case（TDD）：
  - `test_critical_event_flushed_immediately` — ToolResult{is_error: true} 立即落盘（kill -9 后可读）
  - `test_noncritical_event_uses_debounce` — LlmStreamDelta 50ms 批量
  - `test_flush_interval_zero_equals_critical` — `flush_interval = 0` 等价 critical
- [ ] **Step 4.5:** `cargo test -p synthia-event --lib debounce` — 3 case 全过
- [ ] **Step 4.6:** `cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features --tests -p synthia-event -p synthia-agent` — 0 新增 warning

**Commit:** `feat(event,agent): EventLogger debounced flush with critical bypass`

---

## Task 5: BashTool `impl Tool` + 接入 PermissionChecker (TDD)

**Files:**
- `crates/synthia-tool-bash/src/bash_tool.rs:13-20, 189-194, 320-335` (impl Tool + call + pub use)
- `crates/synthia-permission/src/types.rs:5-10` (Action::RunBash variant)
- `crates/synthia-permission/src/merged_policy.rs:62-73` (Err on unknown tool)
- `crates/synthia-tool/src/registry/registration.rs:111-123` (注册 BashTool)
- `crates/synthia-exec/src/lib.rs` (BashCallResult → ToolOutput 适配)
- `crates/synthia-tool-bash/tests/bash_permission.rs` (新测试)

- [ ] **Step 5.1:** `Action` enum (`types.rs:5-10`) 扩 `RunBash { command: String }` variant
- [ ] **Step 5.2:** `MergedPolicy::evaluate` (`merged_policy.rs:62-73`) 改返 `Result<PermissionAction, PermissionError>`；未注册 `tool_name` 返 `Err(PermissionError::UnregisteredTool)`；注册 `Bash` 默认 `Action::RunBash` → `PermissionAction::Ask`
- [ ] **Step 5.3:** `BashTool` (`bash_tool.rs:13-20`) `impl Tool` — 5 个 trait method 全部实现
- [ ] **Step 5.4:** `BashTool::call` (`bash_tool.rs:189-194`) 改走 PermissionChecker + CommandBlacklist AND 逻辑；返 `ToolOutput`
- [ ] **Step 5.5:** `bash_tool.rs:320-335` 改 `pub use synthia_tool::builtin::utf8_safe::cap_to_char_boundary;`（待 §6 创建公共模块）
- [ ] **Step 5.6:** `registration.rs:111-123` `register_defaults` 追加 `BashTool` 注册
- [ ] **Step 5.7:** `synthia-exec/src/lib.rs` 唯一接入点保留；`BashCallResult` 保留 + `From<ToolOutput>` 适配
- [ ] **Step 5.8:** 写 `tests/bash_permission.rs` 5 case（TDD）：
  - `test_bash_through_permission_checker` — `rm -rf /` 走 PermissionChecker Deny
  - `test_unknown_tool_hard_denied` — `BashX` 返 `Err(PermissionError::UnregisteredTool)`
  - `test_command_blacklist_defense_in_depth` — 即使 policy approve，CommandBlacklist 仍 deny
  - `test_bash_normal_command_executes` — `ls` 通过 policy + blacklist
  - `test_is_concurrency_safe_false` — 并发安全守护
- [ ] **Step 5.9:** `cargo test -p synthia-tool-bash --lib` — 5 case 全过
- [ ] **Step 5.10:** `cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features --tests -p synthia-tool-bash -p synthia-permission -p synthia-tool -p synthia-exec` — 0 新增 warning

**Commit:** `fix(tool,permission): gate BashTool via PermissionChecker with CommandBlacklist defense-in-depth`

---

## Task 6: UTF-8 安全截断公共模块 (TDD)

**Files:**
- `crates/synthia-tool/src/builtin/utf8_safe.rs` (新模块)
- `crates/synthia-tool/src/builtin/web.rs:147-148` (替换截断)
- `crates/synthia-tool/src/builtin/grep.rs:34-40` (替换截断)
- `crates/synthia-tool/src/builtin/mod.rs` (导出 utf8_safe)
- `crates/synthia-tool/tests/utf8_panic.rs` (新测试)

- [ ] **Step 6.1:** 新建 `utf8_safe.rs` (~50 行) `pub fn cap_to_char_boundary(s: &mut String, max_bytes: usize)` — 从 `bash_tool.rs:320-335` 提升并 `pub`
- [ ] **Step 6.2:** 8 unit test（写测试 → 跑 → 实现 → 跑）：
  - `test_chinese_3byte` — `"中文"` 截断到 byte 3 → panic 检查
  - `test_emoji_4byte` — `"😀😀"` 截断到 byte 4 → panic 检查
  - `test_mixed_multibyte` — `"abc中文"` 截断到 byte 4 → `"abc"`
  - `test_boundary_exact` — `"abc"` 截断到 byte 3 → `"abc"` (no-op)
  - `test_empty` — `""` 截断到任意 byte → `""`
  - `test_all_ascii` — `"hello"` 截断到 byte 3 → `"hel"`
  - `test_mid_multibyte_truncate_to_zero` — `"中"` 截断到 byte 1 → `""`
  - `test_truncate_no_op` — `s.len() <= max_bytes` 不变
- [ ] **Step 6.3:** `builtin/mod.rs` 导出 `pub mod utf8_safe;`
- [ ] **Step 6.4:** `web.rs:147-148` 替换 `truncated.truncate(max_len)` 为 `utf8_safe::cap_to_char_boundary(&mut truncated, max_len)`
- [ ] **Step 6.5:** `grep.rs:34-40` 同上
- [ ] **Step 6.6:** 写 `tests/utf8_panic.rs` 5 case（端到端）：
  - `test_web_truncate_cjk_emoji` — WebFetchTool 抓取含中文 → 不 panic
  - `test_web_truncate_emoji` — WebFetchTool 抓取含 emoji → 不 panic
  - `test_grep_cjk_search` — GrepTool 搜索中文 → 不 panic
  - `test_bash_output_cjk_truncate` — BashTool 输出含中文 + 截断 → 不 panic
  - `test_bash_output_emoji_truncate` — BashTool 输出含 emoji + 截断 → 不 panic
- [ ] **Step 6.7:** `cargo test -p synthia-tool --lib utf8` — 8 unit test + 5 端到端 全过
- [ ] **Step 6.8:** `cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features --tests -p synthia-tool` — 0 新增 warning

**Commit:** `fix(tool): UTF-8 safe truncation via public utf8_safe::cap_to_char_boundary`

---

## Task 7: 最终验收与合并 commit

- [ ] **Step 7.1:** `cargo +nightly fmt --all` — 无 diff
- [ ] **Step 7.2:** `cargo clippy --all-targets --all-features --tests --all` — 0 新增 warning
- [ ] **Step 7.3:** `cargo test -p synthia-session --lib` — 4 case 全过
- [ ] **Step 7.4:** `cargo test -p synthia-event --lib` — 6 case 全过
- [ ] **Step 7.5:** `cargo test -p synthia-prompt --lib` — 7 case 全过
- [ ] **Step 7.6:** `cargo test -p synthia-agent --lib` — 2 case 全过
- [ ] **Step 7.7:** `cargo test -p synthia-tool-bash --lib` — 5 case 全过
- [ ] **Step 7.8:** `cargo test -p synthia-tool --lib` — 8 unit test + 5 端到端 全过
- [ ] **Step 7.9:** `cargo test --all` — 全绿（确认无回归）
- [ ] **Step 7.10:** `git grep "CrossUserAccess"` — 4 处测试 + 1 处 production 命中
- [ ] **Step 7.11:** `git grep "denied by user" crates/` — production 代码 0 命中（排除 test 注释）
- [ ] **Step 7.12:** `openspec validate user-id-namespace-and-bash-permission-gate --strict` — 通过
- [ ] **Step 7.13:** manual smoke：实际跑 `BashTool("rm -rf /")` 走 PermissionChecker Deny（Guardian UI 可见）
- [ ] **Step 7.14:** 回填 `verify.md` 实际 commit hash + test delta
- [ ] **Step 7.15:** 合并 6 个小 commit 为 1 个 PR commit (squash merge)
- [ ] **Step 7.16:** 合并 commit message：`fix(session,tool): namespace session by user_id + gate BashTool via PermissionChecker + UTF-8 safe truncation`

**Commit (PR merge):** 1 个 PR 1 个 commit，6 个小 commit squash

---

## 验收 Checklist (PR 合并前)

- [ ] `cargo +nightly fmt --all` 无 diff
- [ ] `cargo clippy --all-targets --all-features --tests --all` 0 新增 warning
- [ ] `cargo test --all` 全绿，含 37 个新 test case + 1 property test
- [ ] `git grep "CrossUserAccess"` 4 处测试 + 1 处 production 命中
- [ ] `git grep "denied by user" crates/` production 代码 0 命中
- [ ] `openspec validate user-id-namespace-and-bash-permission-gate --strict` 通过
- [ ] `openspec/changes/user-id-namespace-and-bash-permission-gate/` 下 7 artifact 完整 (plan.md + brainstorm.md + design.md + proposal.md + specs/user-id-and-bash-gate/spec.md + tasks.md + verify.md)
- [ ] 3 个 P0 漏洞 (session 越权 + bash 绕过 + UTF-8 panic) 全部闭环
- [ ] 2 个 project memory 硬约束 (cache hash user_id 命名空间 + UTF-8 安全截断) 全部满足
- [ ] manual smoke 验证 BashTool PermissionChecker 工作

## 与不变式关系 (P1-P10)

| 不变式 | 影响 |
|--------|------|
| P1 KV-Cache 前缀一致性 | HMAC 字节级决定性 + AgentEvent version/seq 序号；不破坏 P1 |
| P2 Append-Only 上下文 | version/seq 字段追加，不改序列；不破坏 P2 |
| P3 按需加载一切 | 不预装任何可推迟信息；不破坏 P3 |
| P4 渐进降级 | cache miss 是 Stage 1 降级（可接受） |
| P5 末尾复述 | 本 change 不涉及 todo.md 复述；不破坏 P5 |
| **P6 不信任 LLM** | **BashTool PermissionChecker + CommandBlacklist AND 逻辑；满足 P6** |
| P7 可中断性 | 本 change 不涉及用户中断；不破坏 P7 |
| **P8 不丢信息** | **EventLogger critical bypass；满足 P8** |
| **P9 可观测性** | **cache_key 决定性 + HMAC proptest 100 case + UTF-8 8 unit test；满足 P9** |
| P10 文件即记忆 | Session 路径 layout 升级 + EventLogger 旁路存储；满足 P10 |

## 后续跟踪 (out of scope, 留到后续 change)

- Ask bridge 实际 caller 接入 + RequireConfirm → Suspended Mailbox 流转 (change-2 follow-up, 2d)
- registration 双 API 行为分裂 + replace_explicit 唯一覆盖入口 (change-2 follow-up, 1d)
- PermissionRequest 扩 call_id/message_id/source 全字段 (change-2 follow-up, 1d)
- audit log 路由 callID (change-2 follow-up, 1d)
- Context Epoch / Step 事件 / CacheBreakDetector wire-up (change-3 + change-5, 11.5d)
- 50KB tool output bound + L1 truncate 不可信哨兵 (change-4, 9d)
- CompactionExhausted variant (change-3 + change-5 协同, 1d)
- BashTool enable_move / ApplyPatchTool D2 atomic rollback (6 个月后)
- ToolOutputStore 旁路存储 7d 保留 + cron cleanup (change-4 内)
- HMAC secret 持久化评估 (6 个月后)
- synthia migrate-sessions --check 子命令 (DX 改进)
- 1 小时 cargo fuzz 验证 HMAC + bash 边界 (持续集成)
