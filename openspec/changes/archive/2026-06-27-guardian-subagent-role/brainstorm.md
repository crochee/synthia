<!--
Raw capture of brainstorming output.

本檔原樣捕捉 brainstorming skill 的產出，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。

不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，
兩者互補但不重疊。
-->

# Brainstorm: Guardian Subagent Role (P1-2)

## Background

### 现状 (As-Is)

synthia 已有 `synthia-guardian` crate，包含三层：

1. **`SimpleGuardian`** — 纯规则快速路径（substring 匹配 `rm -rf`/`sudo`/`chmod 777`/`curl -H`/`export SECRET` 等），无 LLM
2. **`GuardianReviewer`** — LLM-backed 深度审查，但**内联调用** LLM（`router.route()` → `provider.complete()`），在调用方 task 中同步执行，无独立 session、无上下文隔离
3. **`GuardianCoordinator`** — hybrid 层（SimpleGuardian + CircuitBreaker），**但不升级到 GuardianReviewer**（hybrid-layer spec 期望升级，实现未接）

**关键发现：`GuardianReviewer` 和 `GuardianCoordinator` 在生产 agent loop 中完全未接线（zero call sites in production code）。** 仅 `LoopDetectorSet`、`GuardianState`、`GuardianConfig` 被 production 引用。这意味着本次变更是**新建 subagent role + 接线**，而非重构活跃代码路径。

### codex 参考实现 (To-Be model)

codex Guardian（`codex-rs/core/src/guardian/review_session.rs`）核心模式：

- **Subagent 隔离上下文**：`run_codex_thread_interactive` 创建独立 `Codex` runtime（自有 Session、event channel、turn loop）
- **锁定配置防递归**：`approval_policy = Never`、`PermissionProfile::read_only()`、无 MCP、无 skills、无 hooks
- **Trunk + Ephemeral 模式**：trunk session 跨 review 复用（prompt-cache 稳定），ephemeral fork 处理并发 review
- **Forked history**：`InitialHistory::Forked(items)` 从父 session 快照播种子上下文
- **异步决策回流**：`oneshot::Receiver<ReviewDecision>` + `GuardianAssessment`/`GuardianWarning` 事件
- **Circuit breaker**：per-turn，3 consecutive / 10-of-50，触发 `InterruptTurn`
- **常量**：timeout 90s、max retries 3、reviewer name "guardian"、prompt-cache key `guardian:{parent_thread_id}`

### synthia 可复用基础设施

- **`SubagentSessionFactory` trait**（`synthia-agent/src/subagent/factory.rs:47-83`）：`create_child()` + `run_child()`
- **`AppStateSubagentFactory`**（`synthia-server/src/state/subagent_factory.rs`）：server 侧实现，`create_child` → `session_manager.create_child` + `get_or_create_session_controller_with_parent`
- **`ChildSessionHandle`**：`{ session_id, user_id, parent_event_sender }`，子事件镜像为 `AgentEvent::SubagentEvent`
- **`derive_subagent_permission`**：仅继承 Deny（P6 最严格，synthia 4 处领先之一）
- **`build_subagent_config`**：过滤父消息历史 + 配置权限规则

### P1-P10 原则映射

| 原则 | 本次设计约束 |
|------|-------------|
| P1 前缀一致性 | Guardian subagent 不修改父 session prompt；trunk 复用保持 cache key 稳定 |
| P2 Append-Only | Guardian 决策作为 `AgentEvent::GuardianConfirmationRequest`/`GuardianWarning` 追加到末尾 |
| P3 按需加载 | Guardian subagent 仅在 risk ≥ 50 时 spawn；低风险走 SimpleGuardian fast-path |
| P4 渐进降级 | Subagent 失败 → fallback 到 SimpleGuardian → fail-closed Deny |
| P5 末尾复述 | Guardian 决策结果回流到上下文末尾（tool_result/event） |
| P6 不信任 LLM | Guardian subagent 配置锁定（Never approval、read-only、无递归）；CircuitBreaker 独立于 LLM |
| P7 可中断性 | `cancel_token` 贯穿 subagent 生命周期；`AbortSignal` 终止 |
| P8 不丢信息 | Guardian 决策/警告写入事件日志（旁路存储） |
| P9 可观测性 | OTel span 覆盖 review 生命周期（status/duration/decision） |
| P10 文件即记忆 | Guardian 配置走文件系统，不引入新抽象 |

---

## 决议链 (Decision Chain)

### Q1: 是否复用现有 `SubagentSessionFactory`，还是新建 Guardian 专用 spawn 路径？

**选项 A：复用 `SubagentSessionFactory`** — Guardian 作为普通 subagent role spawn
- 优点：复用已有 `ChildSessionHandle`/`AppStateSubagentFactory`/事件镜像；与 `subagent-built-in-types` spec 一致；零新基础设施
- 缺点：现有 `run_child` 是同步等待 300s 的简单实现，无 trunk 复用、无并发 ephemeral fork

**选项 B：新建 Guardian 专用 spawn** — 仿 codex `run_codex_thread_interactive` 独立路径
- 优点：可精确控制 trunk/ephemeral 模式、prompt-cache key、forked history
- 缺点：~500+ 行新基础设施，与现有 subagent 框架重复；违反 "不造抽象"（P10）

**决议：选 A（复用 `SubagentSessionFactory`），Phase 0 不做 trunk 复用。**
- 理由：synthia 当前并发 review 需求低（单用户 CLI/IDE 场景），trunk+ephemeral 是 codex 多并发优化，过早引入违反 simplicity
- `run_child` 的 300s 超时可配置化为 `GuardianConfig::timeout`（默认 90s 对齐 codex）
- Trunk 复用留作 P2 follow-up（当 OTel 指标显示 cache miss 率高时再引入）
- **关键**：Guardian subagent 的 prompt-cache key 用 `guardian:{parent_session_id}`，通过 `SystemContext Source`（P1-4 已完成）注入，不破坏父 prefix

### Q2: Guardian subagent 配置如何锁定以防止递归？

codex 锁定策略：`approval_policy = Never` + `read_only` + 无 MCP/skills/hooks。

synthia 对应：
- `derive_subagent_permission` 已是 Deny-only 继承（P6 最严格）→ Guardian 子 session 默认全部 Deny
- Guardian 子 session 的 `AgentRunConfig` 设 `guardian_enabled: false`（防止 Guardian spawn Guardian）
- Guardian 子 session 不注册 bash/file_editing 等危险工具（只读 review：不调用任何工具，只输出 assessment）
- Guardian 子 session 的 `tool_registry` 为空或仅含 read-only 工具

**决议：三层锁定。**
1. Permission layer：`derive_subagent_permission` Deny-only（已有）
2. Config layer：`guardian_enabled: false` + `max_iterations: 1`（单轮 review，不循环）
3. Tool layer：Guardian 子 session 不注册任何工具（纯 LLM 文本输出 assessment）

### Q3: Guardian 决策如何回流到父 session？

codex 模式：`oneshot::Receiver<ReviewDecision>` + `GuardianAssessment`/`GuardianWarning` 事件。

synthia 现有：
- `AgentEvent::GuardianConfirmationRequest` — 已存在（durable event）
- `AgentEvent::GuardianWarning` — 已存在（ephemeral event）
- `GuardianDecision` enum：`Allow | Deny { reason } | NeedUserConfirm { ... }`

**决议：复用现有 `AgentEvent` + `GuardianDecision`，不新增事件类型。**
- Guardian subagent 的 `Finish { output }` 被父侧解析为 `GuardianDecision`（JSON 解析 assessment）
- 父侧 emit `AgentEvent::GuardianConfirmationRequest`（review 开始）+ `AgentEvent::GuardianWarning`（review 结果，若 Deny/NeedUserConfirm）
- 决策作为 `tool_result` 形式回流（Guardian review 本质是 tool 执行的 permission gate）

### Q4: 何时触发 Guardian subagent review？

codex：`routes_approval_to_guardian(turn)` 当 `approval_policy ∈ {OnRequest, Granular}` 且 `approvals_reviewer == AutoReview`。

synthia 现有 `GuardianCoordinator::check` 逻辑：
1. CircuitBreaker `should_interrupt` → Deny
2. `SimpleGuardian::check` → risk < 50 Allow / risk ≥ 80 Deny / 50-79 NeedUserConfirm
3. **缺失**：50-79 区间应升级到 LLM review，但实现未接

**决议：在 `GuardianCoordinator` 中补全 hybrid 升级路径。**
- risk < 50 → SimpleGuardian Allow（fast-path，不 spawn subagent）
- risk ≥ 80 → SimpleGuardian Deny（fast-path，不 spawn subagent）
- 50 ≤ risk < 80 → **升级到 Guardian subagent review**（本次新增）
- Guardian subagent 失败/超时 → fallback SimpleGuardian `NeedUserConfirm`（fail-closed）
- CircuitBreaker `should_interrupt` → Deny（优先于一切）

### Q5: Guardian subagent 的 review prompt 如何构建？

codex：`build_guardian_review_session_config` 替换 `base_instructions` 为 guardian policy prompt，包含：
- Task context（action JSON）
- Compressed history（forked from parent）
- Risk criteria（destructive ops、credential access、network transmission、data exfiltration）

synthia 现有 `build_review_prompt`（`reviewer.rs`）已实现：
- `collect_transcript_entries(conversation)` → 压缩历史
- `action_json` → 任务上下文
- Risk criteria 嵌入 system prompt

**决议：复用现有 `build_review_prompt`，作为 Guardian subagent 的 user message。**
- Guardian subagent 的 system prompt = guardian policy（独立于父 session 的 system prompt）
- User message = `build_review_prompt(transcript, action_json, None)`
- 期望输出 = JSON assessment `{ risk_level, user_authorization, outcome, rationale }`
- `max_iterations: 1` → subagent 单轮 LLM 调用后 `Finish`，不进入 tool loop

### Q6: 是否需要 `ThreadLifecycleContributor` 等价物？

codex 的 `ThreadLifecycleContributor` 用于在 thread start 时记录 `forked_from_thread_id`，供后续 Guardian fork 使用。

synthia 无 extension 系统，但 `SubagentSessionFactory::create_child` 已接收 `parent_session_id`，天然有 fork 关系。

**决议：不引入 `ThreadLifecycleContributor` 抽象。**
- `parent_session_id` 已通过 `ChildSessionHandle` 传递
- Forked history 通过 `build_subagent_config` 过滤父消息历史实现（已有）
- codex 的 extension API 是为第三方扩展设计，synthia 内置 Guardian 不需要这层抽象（P10 文件即记忆，不造抽象）

### Q7: 异步 vs 同步 review？

codex 提供两个入口：
- `review_approval_request` — async，直接返回 `ReviewDecision`
- `spawn_approval_request_review` — 独立 OS thread + tokio runtime，返回 `oneshot::Receiver`

synthia `SubagentSessionFactory::run_child` 已是 async，内部 `wait_for_child_completion` 阻塞等待 `AgentEvent::Finish`。

**决议：同步 async（`run_child` 模式），Phase 0 不做独立 thread。**
- `GuardianCoordinator::check` 已是 async，直接 `.await` subagent 结果
- 超时由 `GuardianConfig::timeout`（默认 90s）控制，超时 → fallback SimpleGuardian
- 独立 thread 模式（非阻塞 review）留作 P2 follow-up（当 CLI/Server 需要非阻塞 UI 时）

### Q8: 现有 5 个 Guardian spec 如何处理？

现有 spec 描述的是 inline-LLM `GuardianReviewer` 契约。subagent 模式改变了执行模型但不改变决策语义。

**决议：新增 `guardian-subagent-role` capability spec，修改 `guardian-hybrid-layer` spec。**
- `guardian-review`：保留（`check` 签名契约仍适用，subagent 是其实现方式之一）
- `guardian-circuit-breaker`：保留（CircuitBreaker 行为不变）
- `guardian-hybrid-layer`：**修改** — 明确 50-79 区间升级到 subagent review（当前 spec 只说 "escalate to GuardianReviewer"，需明确为 subagent 模式）
- `guardian-timeout-compression`：保留（timeout/压缩行为不变，subagent 复用同一 prompt builder）
- `guardian-action-confirmation`：保留（action-type 路由不变）
- **新增 `guardian-subagent-role`**：定义 subagent spawn、配置锁定、决策回流、递归防护

---

## 设计取舍 (Design Trade-offs)

### 取舍 1: Simplicity vs Trunk 复用
- **选择**：Phase 0 不做 trunk 复用，每次 review spawn 新 subagent
- **代价**：每次 review 丢失 prompt-cache 前缀（cache miss）
- **缓解**：Guardian review 频率低（仅 50-79 risk 区间触发）；OTel 指标监控 cache miss 率；若 >30% 再引入 trunk
- **原则映射**：P3 按需加载 > P1 前缀一致性（Guardian subagent 是独立 session，不影响父 prefix）

### 取舍 2: 同步阻塞 vs 异步非阻塞
- **选择**：同步 async（`.await` subagent）
- **代价**：父 session 在 review 期间阻塞（最长 90s）
- **缓解**：`GuardianConfig::timeout` 可配置；CLI/Server 可在 UI 层显示 "Guardian reviewing..."；review 期间父 session 不消耗 LLM token
- **原则映射**：P7 可中断性（`cancel_token` 可中断）；P4 渐进降级（超时 → fallback SimpleGuardian）

### 取舍 3: 复用 `SubagentSessionFactory` vs 专用路径
- **选择**：复用现有 trait
- **代价**：`run_child` 的 300s 硬编码超时需配置化
- **缓解**：`GuardianConfig::timeout` 传入 `run_child` 或在 `GuardianCoordinator` 层用 `tokio::time::timeout` 包装
- **原则映射**：P10 文件即记忆（不造抽象）；simplicity first（CLAUDE.md §2）

### 取舍 4: 空工具注册表 vs 只读工具
- **选择**：Guardian subagent 不注册任何工具（纯 LLM 文本输出）
- **代价**：Guardian 无法主动读取文件验证风险（如检查 patch 内容）
- **缓解**：review prompt 已包含 action JSON（含完整命令/patch 内容）；Guardian 基于信息做风险评估，不需主动探查
- **原则映射**：P6 不信任 LLM（空工具表 = 最小权限）；codex 也是 `read_only` + 无 MCP

---

## 不做清单 (Out of Scope)

- ✗ `ThreadLifecycleContributor` 抽象（synthia 无 extension 系统，过早抽象）
- ✗ Trunk + Ephemeral 并发 review 模式（Phase 0 单 review 足够）
- ✗ 独立 OS thread + `oneshot::Receiver` 非阻塞模式（留作 P2）
- ✗ Guardian 主动工具调用（read file 验证 patch 等）
- ✗ Guardian 跨 turn 状态累积（per-review 无状态，CircuitBreaker 已在父侧）
- ✗ 重写 `GuardianReviewer`（保留作为 subagent 内部的 LLM 调用逻辑，subagent 包装它）
- ✗ seccomp（landlock 是 P1-3，本次不涉及）
