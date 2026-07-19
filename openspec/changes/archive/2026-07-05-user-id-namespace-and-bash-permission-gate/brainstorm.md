<!--
Raw capture of the brainstorming session for the user-id-namespace-and-bash-permission-gate change.

This file is the decision log from the in-conversation brainstorming (2026-06-16).
The skill's natural output format is preserved (背景 → 決議鏈 Q1-Qn → 設計取捨).
5 专家对抗性审查 (系统架构师 / 安全与权限 / LLM 性能 / 开发者体验 / 可测试性).

design.md will reorganize this content into structured sections
(Context, Goals, Decisions, Risks, Migration). Do NOT duplicate.
-->

# user-id-namespace-and-bash-permission-gate — Brainstorming Decision Log

## 背景 (Background)

Synthia 28 个 crate 中存在 3 个独立的 P0 漏洞 + 1 个 project memory 硬约束违反：

1. **跨用户 session 越权 + cache hash 可预测** (`crates/synthia-session/src/store.rs:54-55` + `manager.rs:405-411`)
2. **BashTool 完全绕过 PermissionChecker** (`crates/synthia-tool-bash/src/bash_tool.rs:13-20, 189-194` + `registration.rs:111-123`)
3. **web.rs / grep.rs UTF-8 截断 panic** (`crates/synthia-tool/src/builtin/web.rs:147-148` + `grep.rs:34-40`)

**核心差距**（与 opencode / codex 对比）：
- opencode `packages/core/src/session/` 按 `user_id` 一级目录隔离；synthia flat layout
- opencode `tool/builtin/bash.ts` 走 PermissionChecker 路径；synthia BashTool 不 `impl Tool` 不入 registry
- codex `assess_patch_safety` 3 变体集中决策点；synthia BashTool 走 CommandBlacklist
- opencode 进程内 HMAC random secret + Anthropic `cache_control` 命名空间；synthia 简单 session_id 截断

**项目根因分析**：
- BashTool 在 7 个内置工具中是孤儿：`impl Tool` 缺失 + `register_defaults` 不含 + `call` 走旁路
- `web.rs` 复用了 `String::truncate` 而非 `bash_tool.rs:320-335` 已有的 `cap_to_char_boundary`（私有）
- `Session` / `SessionMetadata` / `Store::session_dir` 一开始设计无 user_id 维度（single-user 假设）

**来源**：
- 2026-06-16 多专家对抗性 5 维度调研（5 专家 × 5 维度 = 25 反对意见 → 5 共识）
- 2026-06-07 基础 gap 评估（22 GAP 中未闭环 6 个 P0）
- project memory 硬约束「cache hash 必须含 user_id 命名空间」「Bash tool output truncation must handle multi-byte UTF-8 characters to prevent panic」

**核心决议**（来自用户 2026-06-16 21:50 询问）：
- "从 P0 user-id-namespace-and-cache-key-hmac (5.5 人天) 切入，可顺带修复 BashTool 权限绕过"
- 用户后续裁决 "合并 change-1 + change-2 核心"（即 1 个 PR 闭环 3 个 P0 漏洞 + 1 个硬约束）

## 決議鏈 (Decision Chain)

### Q1: 范围 (Scope) — 5 个候选 → 合并为 1 个 PR

候选清单（2026-06-16-openspec-candidates.md）原本列了 5 个独立 change：
- change-1: `user-id-namespace-and-cache-key-hmac` (5.5d, 4 crate)
- change-2: `bash-tool-permission-checker-and-ask-bridge` (6d, 4 crate)
- change-3: `context-epoch-and-step-events` (7d, 跨 A+D+E)
- change-4: `tool-output-store-trust-and-injection-scan` (9d, 5 crate)
- change-5: `compaction-fallback-chain-and-cache-break-detector` (4.5d, 4 crate)

**用户决议：合并 change-1 全量 + change-2 核心 = 1 个 PR (9.5 人天)**

理由（用户原始）："从 P0 user-id-namespace-and-cache-key-hmac 切入，可顺带修复 BashTool 权限绕过" → 闭环 3 个 P0 漏洞 + 1 个硬约束。

合并的临界点：
- 排除 change-2 的 Ask bridge 实际 caller 接入（2d 留到 change-2 follow-up）
- 排除 change-2 的 registration 双 API 行为分裂（1d 留到 follow-up）
- 排除 change-2 的 PermissionRequest 扩 call_id/message_id/source 全字段（1d 留到 follow-up）
- 排除 change-2 的 audit log 路由 callID（1d 留到 follow-up）
- 保留 change-2 核心：BashTool `impl Tool` + PermissionChecker 接通 + CommandBlacklist 二级 + utf8_safe 公共模块

理由：
- 1 个 PR 9.5 人天 < 2 个 PR 11.5 人天 + 额外 review/merge/测试成本
- 避免中间态（部分 PR merge 后系统脆弱窗口）
- Ask bridge / register replace / audit callID 复杂度高，不影响 P0 闭环

### Q2: Session 路径 layout (Session 持久化 §1)

- 选项 A: `{user_id}/{session_id}/` 二级 layout (opencode / codex 标准)
- 选项 B: `{tenant}/{user_id}/{session_id}/` 三级 layout
- 选项 C: flat layout 加密 session_id

**用户决议：选项 A** (opencode / codex 一致)

理由：opencode / codex 走二级；三级 YAGNI (无 tenant 概念)；flat 加密增加密钥管理负担。

### Q3: HMAC secret 持久化 (HMAC §2)

- 选项 A: 进程内 `rand::thread_rng().gen()` 生成，不持久化
- 选项 B: 持久化到文件 (`~/.synthia/hmac_secret`)
- 选项 C: 跨进程共享 secret (single-tenant 假设下从 env var)

**用户决议：选项 A** (进程内随机，不持久化)

理由：
- HMAC 变了 → cache miss → 重建 (P4 渐进降级 Stage 1 可接受)
- 持久化 secret 管理复杂 (存储 / 轮转 / 备份)
- 进程重启后 cache 重建是合理成本 (50ms flush 间隔 + 重启 = 单次 cache miss)

### Q4: BashTool 决策点 (BashTool §1)

- 选项 A: 仅 `PermissionChecker` (主决策点)
- 选项 B: 仅 `CommandBlacklist` (现有)
- 选项 C: 二者结合，AND 逻辑 (任一 deny → deny；都 approve → 执行)

**用户决议：选项 C** (AND 逻辑，二级 defense-in-depth)

理由 (P6 不信任 LLM)：
- 单一决策点 fail-open 风险高 (policy 误配 + blacklist 漏配 = catastrophic 命令执行)
- `CommandBlacklist` 历史价值 (防 `rm -rf /` 即使 policy 漏配)
- AND 逻辑严格：policy deny OR blacklist deny → 都拒绝

专家共识 (5/5)：5 专家全部投票选项 C，"policy 漏配 + blacklist 漏配" 联合风险不可接受。

### Q5: utf8_safe 提升为公共 (UTF-8 §2)

- 选项 A: 保留 3 份私有副本 (bash_tool + web + grep)
- 选项 B: 提升到 `synthia-tool/src/builtin/utf8_safe.rs` 公共
- 选项 C: 引入新 trait `TruncateSafe`

**用户决议：选项 B** (公共 + `pub use` 保持向后兼容)

理由：
- 满足 project memory 硬约束「UTF-8 安全截断」
- 公共函数比 3 份私有副本更易维护
- `pub use` 保持 `bash_tool` 内部 API 不破
- 排除选项 C：YAGNI，单函数 trait 抽象成本 > 收益

### Q6: AgentEvent version/seq 字段位置 (§3)

- 选项 A: 每个 variant 内显式声明 (`Variant { version, seq, ...existing }`)
- 选项 B: 外层 `AgentEventEnvelope { version, seq, event: AgentEvent }`
- 选项 C: 仅 enum 上加 field (技术不可行)

**用户决议：选项 A** (每个 variant 内显式)

理由：
- 编译期 `match` exhaustiveness 检查
- 旧 reader 反序列化时 `#[serde(default)]` 兼容
- 显式比外层 envelope 更易审计
- 排除选项 B：外层 `seq` 与 variant 重复

### Q7: `Store::list_sessions_with_metadata` API (§1)

- 选项 A: 强制 `caller_user_id: &str` 参数 (破坏性)
- 选项 B: `Option<&str>` + 内部检查 (非破坏性但可绕过)
- 选项 C: 单独 `list_all_sessions` admin API

**用户决议：选项 A** (强制参数，编译期 fail-closed)

理由：
- 强制调用方提供 caller，**编译期**防止"忘记传 caller"导致越权
- 旧调用方不传 = 编译错误，强制迁移
- 排除选项 B：违反 fail-closed (Option 可选择 None)
- 排除选项 C：单用户场景无 admin 概念

## 設計取捨 (Design Trade-offs)

### 1. Session 路径 layout 二级 vs 三级
- 选择：二级 `{user_id}/{session_id}/`
- 理由：opencode / codex 一致；三级 YAGNI
- 接受 trade-off：未来 multi-tenant 时需 migration (6 个月后再评估)

### 2. HMAC secret 进程内随机
- 选择：`rand::thread_rng().gen()` 启动时生成 32 字节
- 理由：cache miss 是 P4 渐进降级可接受；持久化 secret 管理复杂
- 接受 trade-off：进程重启 cache miss；持久化 secret 留 6 个月后再评估

### 3. BashTool 决策点 AND 逻辑
- 选择：PermissionChecker + CommandBlacklist AND 逻辑
- 理由：P6 不信任 LLM；二级 defense-in-depth 防止 policy 漏配
- 接受 trade-off：决策路径多 ~10 行；测试覆盖 5 case

### 4. utf8_safe 公共模块
- 选择：提升 `cap_to_char_boundary` 到 `synthia-tool/src/builtin/utf8_safe.rs`
- 理由：3 处复用 + 满足硬约束
- 接受 trade-off：跨 crate 依赖（synthia-tool-bash pub use 适配）

### 5. `Store::list_sessions_with_metadata` 强制 caller_user_id
- 选择：强制参数，编译期 fail-closed
- 理由：防止"忘记传 caller"导致越权
- 接受 trade-off：5+ 调用点破坏性迁移（一次性 commit）

### 6. BashTool `call` 签名破坏
- 选择：`call(args) -> BashCallResult` → `Tool::call(input, ctx) -> ToolOutput`
- 理由：必须 `impl Tool` 才能接入 PermissionChecker
- 接受 trade-off：`BashCallResult` 保留 + `From<ToolOutput>` 适配

### 7. `EventLogger::new` 新增 `flush_interval` 参数
- 选择：`Duration::from_millis(50)` 默认
- 理由：典型 debounce interval；fsync < 1ms × 50ms batch = 20 fsync/s 性能合理
- 接受 trade-off：当前 0 调用方（CONFIRMED）无迁移成本；未来 caller 需传 50ms

### 8. 不引入 `UserId` newtype
- 选择：直接用 `String` 类型 `user_id: &str`
- 理由：project memory 反 speculative trait；当前 single-tenant
- 接受 trade-off：6 个月后再评估 newtype 引入

## 5 专家对抗性审查共识 (Adversarial Review Consensus)

### 系统架构师 (共识 1.1)
- **原始关注**："1 个 PR 9.5 人天工作量过大，违反 CLAUDE.md §3 Surgical Changes"
- **反驳**：3 个 PR 实际 ~12 人天 > 1 个 PR 9.5 人天
- **共识**：1 个 PR 内分 6 个小 commit (commit-by-section)，便于 review 与回滚
- **影响**：实施时按 §1-§6 节奏 commit，每个小节独立可读

### 安全与权限专家 (共识 C1.1)
- **原始关注**："CommandBlacklist 二级检查 fail-closed 路径分裂，攻击者可绕过"
- **反驳**：当前已是 OR 逻辑；专家建议 AND 更严格
- **共识**：升级到 AND 逻辑（policy deny OR blacklist deny → 都拒绝）
- **影响**：D3 升级；§5.2 任务清单调整

### LLM 性能与缓存专家 (共识 2.1)
- **原始关注**："HMAC 截断 16 字节抗碰撞强度不够，建议 SHA-256 完整 32 字节"
- **反驳**：单 process 2^64 次调用需 580,000 年，物理上不可能达到
- **共识**：维持 32 hex 字符 (16 字节) 决定
- **影响**：D8 维持；property test ≥100 case 覆盖

### 开发者体验专家 (共识 3.1)
- **原始关注**："migration shim 隐式触发旧 session 1-2 秒延迟，DX 差"
- **反驳**：< 1000 session 时 < 100ms，无感知
- **共识**：保留自动 migration；1 个 `synthia migrate-sessions --check` 子命令 dry-run (out of scope, 后续 change)
- **影响**：当前 PR 不实施 dry-run 子命令

### 可测试性专家 (共识 4.1)
- **原始关注**："HMAC property test 100 case 不够，应 ≥10000 + 1 小时 fuzz"
- **反驳**：100 case 已覆盖边界 + random；10000 case 边际收益递减
- **共识**：维持 100 case + 单元测试；1 小时 `cargo fuzz` (out of scope, 持续集成)
- **影响**：当前 PR 不实施 1 小时 fuzz

## 范围外 (Out of Scope)

- `permission/src/ask_bridge.rs` 实际 `on_ask_triggered` caller 接入 + `RequireConfirm → Suspended` Mailbox 流转 (change-2 follow-up, 2 人天)
- `registration.rs:130-134` vs `:315-326` 双 register API 行为分裂 + `replace_explicit` 唯一覆盖入口 (change-2 follow-up, 1 人天)
- `PermissionRequest` 扩 `call_id: String, message_id: String, source: PermissionSource` 完整字段 (change-2 follow-up, 1 人天)
- audit log 路由 callID → `audit-{date}.jsonl` (change-2 follow-up, 1 人天)
- Context Epoch / Step 事件 / CacheBreakDetector wire-up (change-3 + change-5, 11.5 人天)
- 50KB tool output bound + L1 truncate 不可信哨兵 + secret-detect 钩子 (change-4, 9 人天)
- CompactionExhausted variant (change-3 + change-5 协同, 1 人天)
- BashTool `enable_move` / `ApplyPatchTool` D2 atomic rollback (留 6 个月观察期)
- ToolOutputStore 旁路存储 7d 保留 + cron cleanup (change-4 内)
- HMAC secret 持久化评估 (6 个月后再评估)
- `synthia migrate-sessions --check` 子命令 (DX 改进)
- 1 小时 `cargo fuzz` 验证 HMAC + bash 边界 (持续集成)

## 验证标准 (Success Criteria)

37 个新 test case (含 1 property test)：
1. `synthia-session`: 4 case (`tests/user_id_namespace.rs`) — 跨 user 越权 / 路径 namespace / 0o700 权限位 / serde default 兼容
2. `synthia-event`: 6 case (`version_seq.rs` 3 + `debounce.rs` 3) — 旧 reader 兼容 / seq 单调 / debounce 行为
3. `synthia-prompt`: 7 case (`cache_key.rs` 5 + property test ≥100) — 决定性 / 跨 user 不碰撞 / 32 hex 长度
4. `synthia-agent`: 2 case (`prompt_cache_key.rs`) — wire-up 注入 / 拒绝空 user_id
5. `synthia-tool-bash`: 5 case (`bash_permission.rs`) — 走 PermissionChecker / 未注册 tool 拒绝 / CommandBlacklist 退化为二级 / 0o700 路径 / 并发
6. `synthia-tool`: 13 case (`utf8_safe.rs` 8 unit + `utf8_panic.rs` 5 端到端) — chinese / emoji / mixed / empty / boundary exact / mid-multibyte / all-ascii / no-op / web / grep / bash

**关键 acceptance gate**：
- `cargo +nightly fmt --all` 无 diff
- `cargo clippy --all-targets --all-features --tests --all` 0 新增 warning
- `cargo test --all` 全绿 (含 37 个新 test case)
- `git grep "CrossUserAccess"` 4 处测试 + 1 处 production 命中
- `git grep "denied by user" crates/` production 代码 0 命中
- `openspec validate user-id-namespace-and-bash-permission-gate --strict` 通过
- `bash scripts/check_synced_spec_format.sh` 通过 (cumulative spec 同步后)
- manual smoke: 实际跑 `BashTool("rm -rf /")` 走 PermissionChecker Deny

## 实施顺序 (9.5 人天 → 6 个小节)

| 小节 | 文件数 | 工作量 | 顺序 |
|------|--------|--------|------|
| §1 Session 持久化 user_id 命名空间 | 5 | 2.0d | 1 |
| §2 LLM provider prompt_cache_key HMAC | 4 | 1.0d | 2 |
| §3 AgentEvent version/seq 字段 | 3 | 0.5d | 3 |
| §4 EventLogger debounced flush | 3 | 0.5d | 4 |
| §5 BashTool impl Tool + PermissionChecker | 5 | 4.0d | 5 |
| §6 UTF-8 安全截断公共模块 | 4 | 1.5d | 6 |
| **总计** | **24** | **9.5d** | - |

## 与不变式关系 (P1-P10)

- **P1 KV-Cache 前缀一致性**: HMAC 字节级决定性 + AgentEvent version/seq 序号；不破坏 P1
- **P2 Append-Only 上下文**: version/seq 字段追加，不改序列；不破坏 P2
- **P3 按需加载一切**: 不预装任何可推迟信息；不破坏 P3
- **P4 渐进降级**: cache miss 是 Stage 1 降级（可接受）；HMAC secret 进程内随机接受
- **P5 末尾复述**: 本 change 不涉及 todo.md 复述；不破坏 P5
- **P6 不信任 LLM**: BashTool PermissionChecker + CommandBlacklist AND 逻辑；满足 P6
- **P7 可中断性**: 本 change 不涉及用户中断；不破坏 P7
- **P8 不丢信息**: EventLogger critical bypass (Decision/Error/ToolResult{is_error} 立即 flush)；满足 P8
- **P9 可观测性**: cache_key 决定性 + HMAC proptest 100 case + UTF-8 8 unit test；满足 P9
- **P10 文件即记忆**: Session 路径 layout 升级 + EventLogger 旁路存储；满足 P10
