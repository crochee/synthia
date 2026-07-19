# Design: user-id-namespace-and-bash-permission-gate

## Context

Synthia 当前在 3 个独立但相关的 P0 漏洞上与生产级 agent (opencode / codex) 存在差距，合并修复是性价比最高的方案：

### 漏洞 1：Session 持久化无 user_id 命名空间

`Store::session_dir` (`crates/synthia-session/src/store.rs:54-55`) 路径 layout 是 `{sessions_root}/{session_id}/`，**完全无 user_id 中间层**。Session struct 也没有 `user_id` 字段。导致：
- **跨用户 session 越权枚举**：`list_sessions_with_metadata` (`manager.rs:405-411`) 不传 caller，理论上 user A 可以列举 user B 的所有 session
- **filesystem 信息泄漏**：所有 session 平铺在同一目录，ls 后看到全部 session_id
- **违反 project memory 硬约束**「cache hash 必须含 user_id 命名空间」

参考实现：
- opencode `packages/core/src/session/` 按 `user_id` 一级目录隔离
- codex `~/.codex/sessions/{user_id}/{session_id}/` 同 layout

### 漏洞 2：BashTool 完全绕过 PermissionChecker

`BashTool` (`crates/synthia-tool-bash/src/bash_tool.rs:13-20`) 没有 `impl Tool` trait。`BashTool::call` 走 `CommandBlacklist` 决策 (`bash_tool.rs:189-194`)，**不**经 `ToolRegistry::execute` → `PermissionChecker::check` 路径。`ToolRegistry::register_defaults` (`registration.rs:111-123`) 不含 `BashTool` — 7 个内置工具全在列表里，唯独 bash 缺失。

后果：
- Guardian UI 对 bash 路径**完全失明**（5/5 强信号 — 维度 B / C 对抗性审查共识）
- `Bash("rm -rf /")` 可直接执行，policy 拦不住
- audit log 看不到 bash 工具调用

参考实现：
- codex `assess_patch_safety` 3 变体 `SafetyCheck::AutoApprove/AskUser/Reject` — 集中决策点
- opencode `tool/builtin/bash.ts` — 走 PermissionChecker 路径

### 漏洞 3：web.rs / grep.rs UTF-8 截断 panic

`WebFetchTool::call` (`crates/synthia-tool/src/builtin/web.rs:147-148`) 写 `truncated.truncate(max_len)` — `String::truncate` 在 byte index 落在 UTF-8 多字节字符（3-byte 中文 / 4-byte emoji）内部时**直接 panic**。`grep.rs:34-40` 同问题。**同仓 `bash_tool.rs:320-335` 已有 8 个 unit test 守护的 `cap_to_char_boundary` 私有实现**，但 web.rs / grep.rs 复用了不安全的 `truncate`。

后果：
- 中文 / emoji 内容触发 panic，进程崩溃
- 违反 project memory 硬约束「Bash tool output truncation must handle multi-byte UTF-8 characters to prevent panic」

### 约束

- 不得破坏现有 `multi_edit` / `apply_patch` / `WebFetchTool` / `GrepTool` 行为（skill 测试已使用，避免回归）
- 必须复用现有 `PermissionChecker` / `MergedPolicy` / `check_path_safety` 抽象
- `#[serde(default)]` 兼容旧 JSONL reader（project memory hard constraint）
- 不得引入新 trait 抽象（per project memory "Architectural trait abstractions should be re-evaluated 6 months after bug fixes and code deduplication" + 候选清单反 speculative trait 立场）
- 不得引入 SQLite / 向量库 / Effect 库（per 候选清单拒绝 #2/#3/#4）
- 1 个 PR / 1 个 commit / 严格按 file:line 锚定（per CLAUDE.md §3 Surgical Changes）

### 利益相关方

- **终端用户**：session 跨用户隔离不再可越权；bash 命令走 Guardian 审批可见；web/grep 抓取中文内容不再崩溃
- **LLM (调用方)**：tool list 长度 +1 (Bash)；`prompt_cache_key` 决定性增强 cache 命中率
- **运维 / 审计**：bash 命令有 audit log；user_id 目录权限 0o700 防止同主机多用户越权
- **下游 change-3/4/5**：本 change 完成后，`ContextEpoch` / `TrustLevel` / `CompactionEvent` 才有 user_id 来源

## Goals / Non-Goals

**Goals:**
- 闭环 3 个 P0 漏洞 (session 越权 + bash 绕过 + UTF-8 panic)
- 满足 2 个 project memory 硬约束 (cache hash user_id 命名空间 + UTF-8 安全截断)
- 1 个 PR / 1 个 commit / 9.5 人天工作量
- 4 crate 协同 (synthia-session / synthia-event / synthia-agent / synthia-tool-bash / synthia-tool / synthia-permission / synthia-prompt)
- 18 个新 test case (含 4 user_id + 3 event + 5 bash + 6 utf8_safe)
- 1 个 property test (HMAC 决定性 + 跨 user 不碰撞)

**Non-Goals (留到后续 change):**
- `permission/src/ask_bridge.rs` 实际 `on_ask_triggered` caller 接入 + `RequireConfirm → Suspended` Mailbox 流转 (change-2 follow-up, 2 人天)
- `registration.rs:130-134` vs `:315-326` 双 register API 行为分裂 + `replace_explicit` 唯一覆盖入口 (change-2 follow-up, 1 人天)
- `PermissionRequest` 扩 `call_id: String, message_id: String, source: PermissionSource` 完整字段 (change-2 follow-up, 1 人天)
- audit log 路由 callID → `audit-{date}.jsonl` (change-2 follow-up, 1 人天)
- Context Epoch / Step 事件 / CacheBreakDetector wire-up (change-3 + change-5, 11.5 人天)
- 50KB tool output bound + L1 truncate 不可信哨兵 + secret-detect 钩子 (change-4, 9 人天)
- CompactionExhausted variant (change-3 + change-5 协同, 1 人天)
- BashTool `enable_move` / `ApplyPatchTool` D2 atomic rollback (留 6 个月观察期)
- ToolOutputStore 旁路存储 7d 保留 + cron cleanup (change-4 内)
- Failover / Disaster Recovery for HMAC secret (process-local 即可，无需持久化)
- Multi-tenant (同一 process 多 user_id) — 当前假设 single-tenant

## Decisions

### D1: Session 路径 layout 改为 `{user_id}/{session_id}/` (而非 `{tenant}/{user_id}/{session_id}/`)

- **选择**：`{user_id}/{session_id}/` 二级 layout
- **理由**：
  - opencode / codex 均采用二级 layout
  - 三级 layout (tenant/user/session) 过度抽象，当前无 tenant 概念
  - 二级 layout 路径 `sessions/alice/sess-1/` 可读性 > 三级
- **已考虑 alternative**：
  - 三级 layout：当前无 tenant 需求，YAGNI
  - flat layout 加密 session_id：加密密钥管理增加复杂度，且违反 user_id 显式可见原则
  - database-backed (SQLite) — project memory 硬约束拒绝 SQLite
- **影响**：旧 session 路径布局需 migration shim（`Store::migration_load_legacy`）

### D2: HMAC secret 进程内随机生成，不持久化

- **选择**：`rand::thread_rng().gen()` 启动时生成 32 字节 secret，进程生命周期内单例
- **理由**：
  - **不是 cache 兼容性的核心**：HMAC key 变了 → cache miss → 重建，**不是数据丢失**（仅性能损失）
  - 持久化 secret 增加管理负担（secret 存储 / 轮转 / 多机共享）
  - 进程重启后 cache 重建是 P1 KV-Cache 可接受的成本
- **已考虑 alternative**：
  - 持久化 secret 到文件：管理复杂 + 安全风险（secret 文件本身需保护）
  - 使用固定 secret (从编译期常量)：违反 P9 可观测性 + 失去命名空间意义
  - 跨进程共享 secret：当前 single-tenant，YAGNI
- **影响**：进程重启后 cache miss 增加，符合 P4 渐进降级（cache miss 是 Stage 1 降级）

### D3: BashTool `impl Tool` + PermissionChecker 优先 + CommandBlacklist 二级 (而非"二选一")

- **选择**：
  1. `PermissionChecker::check` **优先**（主决策点）
  2. `CommandBlacklist` **二级**（defense-in-depth）
  3. 两者任一 deny → `ToolOutput::error`
- **理由**：
  - **P6 不信任 LLM**：单一决策点 fail-open 风险高；二级检查防止 policy 漏配
  - **CommandBlacklist 历史价值**：防止 `rm -rf /` 等 catastrophic 命令，即使 policy 误配置
  - **代码重复成本低**：`CommandBlacklist::is_command_allowed` 已存在，零额外代码
- **已考虑 alternative**：
  - 仅 `PermissionChecker`：policy 漏配时 catastrophic 命令无防线
  - 仅 `CommandBlacklist`：失去 Guardian UI 可见性，违反 P6
  - 二者并行 OR/AND：决策点分裂，难以审计
- **影响**：`call` 函数逻辑分支增加 ~10 行；测试覆盖 5 case

### D4: `web.rs` + `grep.rs` 替换为 `utf8_safe::cap_to_char_boundary`，**不**保留 `truncate` 路径

- **选择**：
  - 公共 `cap_to_char_boundary` 在 `synthia-tool/src/builtin/utf8_safe.rs`（`pub`）
  - `web.rs:147-148` + `grep.rs:34-40` 替换
  - `bash_tool.rs:320-335` 改 `pub use synthia_tool::builtin::utf8_safe::cap_to_char_boundary;` 保持向后兼容
  - 8 个 unit test 迁移到 `utf8_safe.rs`
- **理由**：
  - 满足硬约束「Bash tool output truncation must handle multi-byte UTF-8 characters to prevent panic」
  - 公共函数比私有副本 3 份更易维护
  - `pub use` 保持 `bash_tool` 内部 API 不破
- **已考虑 alternative**：
  - 保留 3 份私有副本：违反 DRY，未来修改 3 处一致
  - 引入新 trait `TruncateSafe`：YAGNI，单函数 trait 抽象成本 > 收益
  - 整体替换为 `unicode-segmentation` crate：3rd-party 依赖增加 1 个，单函数无必要
- **影响**：零 API 破坏（公共函数签名不变）

### D5: AgentEvent `version/seq` 字段加在每个 variant 内，而非外层 `AgentEventEnvelope`

- **选择**：在每个 `AgentEvent::Variant { version: u32, seq: u64, ...existing_fields }` 内显式声明
- **理由**：
  - 编译期 match exhaustiveness 检查
  - 旧 reader 反序列化时 `#[serde(default)]` 兼容
  - 显式比外层 envelope 更易审计
- **已考虑 alternative**：
  - 外层 `AgentEventEnvelope { version, seq, event: AgentEvent }`：外层 `seq` 与 variant 重复
  - 仅在 `AgentEvent` enum 上加 `version/seq`（tuple variant）：enum 上不能有 field
- **影响**：36+ variant 同步加 2 字段；测试覆盖 36+ variant 编译期验证

### D6: EventLogger `flush_interval` 默认 50ms (而非 0 / 10ms / 100ms)

- **选择**：`Duration::from_millis(50)` 默认
- **理由**：
  - 50ms 是 typical debounce interval（typical filesystem fsync < 1ms）
  - 50ms 内 batch 多次 write 减少 fsync 次数（per P9 可观测性 fsync/s < 10）
  - critical event (Decision/Error/ToolResult{is_error}) 立即 flush 绕过 debounce
- **已考虑 alternative**：
  - 0（无 debounce）：fsync 频繁，性能差
  - 10ms：fsync/s 仍可达 100，浪费
  - 100ms：decision event 延迟 100ms 落盘，违反 P8 不丢信息（kill -9 风险）
- **影响**：可配置参数；`synthia-agent::run` wire-up 时传 50ms

### D7: `Store::list_sessions_with_metadata` 新增 `caller_user_id` 参数 (而非 `Option<&str>`)

- **选择**：**强制** `caller_user_id: &str` 参数；无 `Option`
- **理由**：
  - 强制调用方提供 caller，**编译期**防止"忘记传 caller"导致越权
  - 旧调用方不传 caller = 编译错误，强制迁移
- **已考虑 alternative**：
  - `Option<&str>` + 内部检查：调用方可选择不传，违反 fail-closed
  - 单独 `list_all_sessions` admin API：单用户场景无 admin 概念
- **影响**：破坏性 API 改动；5+ 调用点需迁移（grep + fix 一次性 commit）

### D8: HMAC-SHA256 截断 32 hex 字符 (16 字节) (而非 16 / 64 hex 字符)

- **选择**：32 hex 字符（16 字节，128 bit）
- **理由**：
  - 16 字节 = 128 bit 抗碰撞 (birthday bound 2^64，足够)
  - 32 hex 字符 vs 64 hex 字符 (256 bit)：cache key 字符串短 → token 消耗少
  - Anthropic `cache_control` 接受任意长度 key
- **已考虑 alternative**：
  - 16 hex 字符 (8 字节 / 64 bit)：生日 bound 2^32，碰撞风险
  - 64 hex 字符 (32 字节 / 256 bit)：性能浪费，token 翻倍
- **影响**：property test ≥100 case 跨 (user_id, session_id) 组合前 16 字符无碰撞

### D9: 不引入 `UserContext` / `SecurityContext` 新类型 (per project memory 反 speculative trait 立场)

- **选择**：直接用 `String` 类型 `user_id: &str` 参数
- **理由**：
  - project memory 硬约束「Avoid backwards-compatibility hacks / design for hypothetical future requirements」
  - 当前 single-tenant，`String` 即足够表达
  - 未来 multi-tenant 时再引入 `UserId` newtype（6 个月后再评估）
- **已考虑 alternative**：
  - `UserId` newtype struct：当前 YAGNI
  - `UserContext { user_id, session_id, request_id }`：单参数 1 字段类型，YAGNI
- **影响**：零新类型；6 个月后再评估 newtype 引入

## Risks / Trade-offs

| # | Risk | Severity | Mitigation |
|---|------|----------|------------|
| R1 | 旧 session 路径布局破坏 → 旧 session 读不到 | High | `Store::migration_load_legacy` 一次性迁移 + 标记 + 写新布局 + 删除旧目录 |
| R2 | HMAC secret 进程重启失效 → cache miss 增加 | Medium | 50ms flush 间隔 + 重建 cache 是 P1 KV-Cache 接受的；HMAC 决定性保证同进程内 cache 命中 |
| R3 | PermissionChecker fail-open 风险 | High | 二级 `CommandBlacklist` defense-in-depth；`PermissionError::UnregisteredTool` 返 `Err` 而非 `Ask` |
| R4 | `web.rs` UTF-8 修复触发新 panic 路径 | Low | 8 unit test 覆盖 chinese / emoji / mixed / empty / boundary exact / mid-multibyte / all-ascii / no-op；端到端 5 case 验证 |
| R5 | `AgentEvent` 36+ variant 同步加字段致漏改 | Medium | 编译期 `match` exhaustiveness + `tests/version_seq.rs` 编译期 assert 36+ variant 都有 version/seq |
| R6 | `BashTool::call` 签名破坏 → 5+ 调用点回归 | High | `bash_tool` crate 内 `BashCallResult` 类型保留并 `From<ToolOutput>` 适配；一次性 commit 迁移 |
| R7 | `EventLogger` 0 调用方 wire-up 引入并发 bug | Medium | 50ms flush + critical bypass + 3 case test 守护 (kill -9 + read_all) |
| R8 | 1 个 PR 9.5 人天工作量 → review 困难 | Medium | 7 个独立小节 (1.x-7.x) + file:line 锚定；1 个 PR 但内部分 6 个小 commit 节奏（commit-by-section） |
| R9 | new dep `hmac 0.12` + `sha2 0.10` 与 workspace 冲突 | Low | `synthia-prompt` crate 尚未引入，cargo workspace 自动 resolve 最新兼容版本 |
| R10 | `set_permissions(0o700)` 在 Windows 失败 | Low | `#[cfg(unix)]` 守卫；Windows 走 fallback 不设置权限位（用户层 ACL 由 OS 处理） |

[Trade-off] 进程内 HMAC secret 不持久化 → 接受理由：cache miss 是 P4 渐进降级 Stage 1 (可接受)；持久化 secret 管理成本 > cache 重建成本
[Trade-off] BashTool `call` 签名破坏 → 接受理由：5+ 调用点一次性迁移 < 二级 API 兼容维护成本
[Trade-off] `utf8_safe::cap_to_char_boundary` 提升为公共 → 接受理由：3 处复用 + 满足硬约束
[Trade-off] 1 个 PR 9.5 人天 → 接受理由：3 个 P0 漏洞 + 1 个硬约束合并修复效率高；可分 6 个小 commit 节奏
[Trade-off] 不引入 `UserId` newtype → 接受理由：6 个月后再评估；project memory 硬约束

## Migration Plan

**破坏性变更 (3 处)**：

1. **Session 路径 layout**：`{sessions_root}/{session_id}/` → `{sessions_root}/{user_id}/{session_id}/`
   - **迁移方式**：`Store::load` 读 metadata 时若 `user_id` 字段缺失 → 调 `migration_load_legacy(session_id)` → 一次性读旧布局 → 写新布局 → 标记已迁移 → 删旧目录
   - **回滚策略**：保留旧目录备份 7 天（`migration_legacy_backup_{timestamp}/`）
   - **验收**：`tests/user_id_namespace.rs::test_migration_legacy_layout` 通过

2. **BashTool `call` 签名**：`call(args) -> BashCallResult` → `Tool::call(input, ctx) -> ToolOutput`
   - **迁移方式**：`BashCallResult` 保留并 `From<ToolOutput> for BashCallResult` 适配（保留旧 type 不破外部 caller）
   - **回滚策略**：旧 caller 用 `BashCallResult::from(tool_output)` 一行适配
   - **验收**：`cargo test -p synthia-exec` 全绿（确认 caller 迁移无回归）

3. **`Store::list_sessions_with_metadata` 强制 `caller_user_id` 参数**：
   - **迁移方式**：5+ 调用点 grep + 显式传 `self.user_id` 或构造 caller
   - **回滚策略**：编译期错误强制迁移
   - **验收**：`cargo test -p synthia-session` 全绿

**非破坏性变更 (3 处)**：

4. **`AgentEvent` 36+ variant 加 `version/seq` 字段**：
   - `#[serde(default)]` 兼容旧 JSONL reader
   - 无调用方迁移

5. **`web.rs` / `grep.rs` 截断**：
   - 行为等价（正确性 fix），无 API 破坏
   - 8 unit test + 5 端到端 test 守护

6. **`EventLogger::new(flush_interval: Duration)`**：
   - 新增参数；旧调用方需传 50ms
   - 当前 0 调用方（CONFIRMED），无迁移成本

**部署步骤**：
1. `git checkout -b fix/user-id-namespace-and-bash-permission-gate`
2. 实施 tasks.md §1-7 (按顺序, 严格 file:line)
3. `cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features --tests --all` 0 warning
4. `cargo test --all` 全绿 (含 18 个新 test case)
5. `openspec validate user-id-namespace-and-bash-permission-gate --strict` 通过
6. `git commit -m "fix(session,tool): namespace session by user_id + gate BashTool via PermissionChecker + UTF-8 safe truncation"`
7. `git push` → CI 通过 → merge to master → delete branch
8. archive 流程：delta spec 同步到 `openspec/specs/user-id-and-bash-gate/spec.md`（剥 `ADDED ` 前缀，改为 `## Requirements` + `## Purpose`）

**回滚策略**：
- `git revert <commit>` 一键回滚（破坏性变更均无外部 schema 依赖，仅 metadata.json 路径布局变化）
- migration shim 失败时旧 session 仍在 `migration_legacy_backup_{timestamp}/` 备份

**验收条件**：
- `cargo +nightly fmt --all` 无 diff
- `cargo clippy --all-targets --all-features --tests --all` 0 新增 warning
- `cargo test --all` 全绿
- 18 个新 test case 全过：
  - 4 case `tests/user_id_namespace.rs`
  - 3 case `synthia-event/tests/version_seq.rs` + 3 case `debounce.rs`
  - 5 case `synthia-tool-bash/tests/bash_permission.rs`
  - 8 case `synthia-tool/src/builtin/utf8_safe.rs`
  - 5 case `synthia-tool/tests/utf8_panic.rs`
- 1 个 property test `synthia-prompt/tests/cache_key.rs` ≥100 case 通过
- `git grep "CrossUserAccess"` 4 处测试 + 1 处 production 命中
- `git grep "denied by user" crates/` production 代码 0 命中
- `openspec validate --strict` 通过

## Open Questions

1. **HMAC secret 是否需要写到环境变量？**
   - 当前决议：进程内随机生成，不持久化
   - 6 个月后再评估：若 multi-process cache 共享需求出现，再升级到 env var
2. **migration shim 失败时是否需要回滚？**
   - 当前决议：旧布局备份 7 天后 cron 删除；用户层无感知
3. **BashTool `CommandBlacklist` 二级检查是否影响性能？**
   - 当前决议：`< 1ms`（单纯字符串匹配）；高频调用场景可优化到 bloom filter（out of scope）
4. **`EventLogger` 50ms 是否需要 adaptive？**
   - 当前决议：固定 50ms；event rate 高时 batch 自然增加；adaptive 复杂度 > 收益
5. **`AgentEvent` 36+ variant 加字段是否需要 schema migration 工具？**
   - 当前决议：`#[serde(default)]` 兼容旧 reader；新 reader 读旧 event 时 `version = 0` `seq = 0` 默认值
6. **HMAC key 与 session_id 拼接顺序？**
   - 当前决议：`HMAC-SHA256(user_id || session_id)`（user_id 在前）
   - 考虑 alternative：`HMAC-SHA256(session_id || user_id)`：语义上 user_id 是 namespace 应在前

## 反方观点 (Adversarial Review)

按 brainstorming skill 要求记录多专家对抗性审查的反方观点，确保设计经过 5 专家共识：

### 系统架构师 (反对意见)

> "1 个 PR 9.5 人天工作量过大，违反 CLAUDE.md §3 Surgical Changes 的'每个 changed line 应该 trace directly to the user's request'原则。建议拆 3 个 PR：(1) UTF-8 安全截断（1 人天，独立无依赖）；(2) BashTool PermissionChecker（4 人天，4 crate 协同）；(3) user_id 命名空间 + HMAC（4.5 人天，5 crate 协同）。"

**回应**：3 个 PR 工作量是 9.5 人天 × 1 + 额外的 review / merge / 测试成本 = 实际 ~12 人天。1 个 PR 9.5 人天 < 3 个 PR 12 人天，且避免中间态（部分 PR merge 后系统的脆弱窗口）。但**采纳拆 PR 节奏**：1 个 PR 内部分 6 个小 commit (commit-by-section)，便于 review 与回滚。

### 安全与权限专家 (反对意见)

> "BashTool `CommandBlacklist` 二级检查是 defense-in-depth 但增加了 fail-closed 路径分裂。攻击者可构造命令同时绕过 PermissionChecker (policy 误配) 和 CommandBlacklist (unknown 命令) 吗？建议显式 fail-closed：任一 deny → deny，但 require BOTH approve 才执行。"

**回应**：当前实现已经是 `任一 deny → ToolOutput::error`（OR 逻辑）。专家建议 `BOTH approve` 是 AND 逻辑 — 实际更严格。**考虑升级到 AND 逻辑**：CommandBlacklist deny → deny 无论 policy；policy deny → deny 无论 CommandBlacklist；两者都 approve → 执行。**采纳专家建议**：D3 升级为 AND 逻辑，§5.2 任务清单调整。

### LLM 性能与缓存专家 (反对意见)

> "HMAC 截断到 16 字节 (32 hex) 抗碰撞强度不够。Birthday bound 2^64 次调用后碰撞概率 ~50%。生产级 LLM agent 单 process 调用次数可能 > 2^64 (长期运行)，建议 SHA-256 完整 32 字节 (64 hex)。"

**回应**：单 process 2^64 次调用 = 18 quintillion，物理上不可能在合理时间内达到（即使 1M calls/sec 需 ~580,000 年）。**不采纳专家建议**：32 hex 已足够；token 消耗 64 hex vs 32 hex 翻倍，浪费。

### 开发者体验专家 (反对意见)

> "migration shim 在 `Store::load` 隐式触发，旧 session 第一次 load 时用户可能感知到 1-2 秒延迟（FS rename + set_permissions）。建议显式 `synthia migrate-sessions` 子命令，让用户主动选择时机。"

**回应**：当前 session 数 < 1000 时 migration < 100ms，无感知。**部分采纳专家建议**：保留自动 migration，但加 1 个 `synthia migrate-sessions --check` 子命令 dry-run 模式让用户预览（out of scope，留到后续 change）。

### 可测试性专家 (反对意见)

> "HMAC property test 用 `proptest` ≥100 case 不够。生产级 cache key 抗碰撞测试应该用 `proptest` ≥10000 case + 实际 fuzz 1 小时。"

**回应**：100 case 已覆盖 `(user_id, session_id)` 笛卡尔积的边界 + random；10000 case 边际收益递减。**不采纳专家建议**：100 case + 单元测试 + 1 小时 `cargo fuzz` (out of scope) 是合理 trade-off。
