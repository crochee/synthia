## Context

### Background

Synthia 是一个 Rust AI Agent 框架（21 crate，~7k LOC），由用户主导设计实现。Agent 运行时的核心反应循环（react loop）、会话管理（session）、LLM 调用抽象已建立，但与生产级 AI agent（opencode / codex / pi-mono）相比，在以下方面存在差距：

- **Tool 抽象薄弱**：Tool trait 有 12 个方法但缺少 sub-trait 切分，导致 registry 复杂
- **Session/Provider 耦合**：Agent 直接持有 LLM client，缺乏三层分离
- **Message/视图抽象缺失**：没有 `llm_visible()` 抽象，导致内部状态与 LLM 视野耦合
- **可扩展性受限于核心机制**：compress、permission、subagent 等被迫嵌入 react_loop

用户原话（触发本次 change）：

> "除了主逻辑 react loop 和 session 之外，其他功能尽量抽象为 tool 实现"

### Current State（基线 2026-07-12 校准后）

| 实体 | 状态 | 备注 |
|------|------|------|
| `AgentRunConfig` | 丢弃 11 个 `_xxx` 字段 | `_prompt_template_name`, `_model_id` 也是无效字段 |
| `AgentMessage` / `MessageRole` | 无 `llm_visible()` 抽象 | 直接 enum 分支 |
| `Tool` trait | 12 方法（拆/不拆待定） | baseline 错算 7 |
| `AgentTool` | 已实现 + 已注册，**factory 未串** | gap 是 wiring 不是实现 |
| `compress` | 已在 react_loop 调用 | baseline 漏报 |
| `react_loop` | should_continue + act + observe + compress | 核心机制，**不重构** |
| `Session` | OrphanRule 已修 | 核心机制，**不重构** |

### Stakeholders

- **Owner**: Synthia 框架维护者（用户本人）
- **Consumers**: Synthia 用户（CLI / 嵌入式 / 实验）
- **Reference**: opencode / codex / pi-mono（参考但不强对齐）

### Constraints

1. **核心不变**：react_loop 和 session 不重构（用户硬约束）
2. **渐进式**：Phase 1 = 10 个 PR（6 周），每 PR ≤ 3 天
3. **向后兼容**：优先非破坏 API 演进
4. **类型安全**：遵循 `.trae/rules/rust.md`（strict types / clippy / miri）

---

## Goals / Non-Goals

**Goals:**

1. **Tool 抽象现代化**：将 Tool trait 拆为 Definition / Execution / Lifecycle 三个 sub-trait，每个 ≤ 5 方法
2. **AgentMessage 视图抽象**：加 `llm_visible() -> bool` + `MessageKind` 区分，让内部状态与 LLM 视野解耦
3. **三层架构**：Tool / Session / Provider 分离，Agent 不直接持有 LLM
4. **Tool registry 双索引**：HashMap + Vec<ToolMetadata> 加速 LLM 视野 lookup
5. **Compaction 抽象为 Tool**：默认实现保留，可注入自定义（`CompressionTool`）
6. **AgentTool factory wiring**：将已实现但未串的 AgentTool 接到主流程
7. **代码质量清理**：`_xxx` 字段全部清掉；11 个 silent-drop 字段处理（删除 / 重命名 / 启用）
8. **TypeScript-style `llm_visible()` 单点解耦**：借鉴 pi-mono 最强烈推荐
9. **渐进交付**：每个 PR 独立可合并、独立可验证

**Non-Goals:**

1. ❌ 重构 react_loop（核心机制，用户硬约束）
2. ❌ 重构 Session 生命周期（核心机制）
3. ❌ 引入 Subscription/OAuth 模型（API key 路线够用）
4. ❌ 引入 VFS sandbox（取决于部署目标，Phase 3 评估）
5. ❌ AppServer 进程模型（我们是库不是服务）
6. ❌ V8 引擎直搬（与 Rust 哲学不符）
7. ❌ 30+ overloads（TS 包袱，不学）
8. ❌ task-local OTel（pi-mono 已被反方否决）
9. ❌ 改 Provider trait 抽象（Phase 3 评估）
10. ❌ 改 AgentMessage 为 enum（破坏性变更，下次 major 再做）

---

## Decisions

### D1：范围 = 全栈对标 + Tool 化（来自 Q1 选择 A）

- **选择**：全栈对标 + Tool 化（不限 3-5 个模式）
- **理由**：用户明确选择 A，覆盖 opencode / codex / pi-mono 全部借鉴点
- **替代方案 B**：仅借鉴 3-5 个模式 → 拒绝：用户要求"全面对标"
- **替代方案 C**：仅核心机制对齐 → 拒绝：用户原话是"其他功能尽量抽象为 tool"

### D2：深度 = 概念 + Trait 草案（来自 Q2 选择 B）

- **选择**：每个模式给出概念 + Trait 草案，不写完整实现
- **理由**：避免过度工程化，先把架构定义清楚
- **替代方案 C**：概念 + Trait + 测试骨架 → 拒绝：测试应在每个 PR 中写，不在 design.md
- **替代方案 D**：完整实现 → 拒绝：scope 失控

### D3：Progressive Toolification 原则（反方挑战让步）

- **选择**：一个候选功能 tool 化必须满足 4 条件中的 ≥ 3 个：
  1. 外部可达（用户/LLM 可触发）
  2. 副作用显式（产出可观察 artifact）
  3. 上下文相关（需要 LLM 上下文决定何时调用）
  4. 可降级（Tool 不可用时能 fallback 到核心 loop）
- **理由**：避免"全部 tool 化"的过度抽象陷阱
- **判定结果**：
  - ✅ Tool 化：compression / subagent / file ops / permission gating / tool search
  - ❌ 不 Tool 化：react_loop / session lifecycle / event emission / telemetry span / cancel token / provider routing

### D4：Tool trait 拆为 3 个 sub-trait

- **选择**：`Tool` = `ToolDefinition` + `ToolExecution` + `ToolLifecycle`
- **理由**：每个 sub-trait ≤ 5 方法，职责清晰；实现者可按需选择实现 sub-trait
- **替代方案 A**：维持 12 方法 → 拒绝：API surface 大，难学
- **替代方案 B**：保留 + 加 `category` 字段 → 拒绝：不解决职责不清

### D5：AgentMessage 加 `llm_visible() -> bool`（非破坏）

- **选择**：在 `AgentMessage` 加 method + 引入 `MessageKind` enum
- **理由**：最小改动，向后兼容，符合 pi-mono 最强烈推荐
- **替代方案 B**：改为 `Message` enum → 拒绝：破坏性变更
- **替代方案 C**：加 `MessageView` newtype → 拒绝：多一层间接，不如 method

### D6：ToolRegistry 双索引

- **选择**：`HashMap<String, Arc<dyn Tool>>` + `Vec<ToolMetadata>`
- **理由**：lookup 加速 O(1)；Vec 顺序保留为 LLM 视野稳定的 sequence
- **替代方案 A**：仅 HashMap → 拒绝：lookup O(n) 在大 registry 下变慢
- **替代方案 B**：HashMap + Vec<String> → 拒绝：仍需 hash lookup metadata

### D7：Compaction 抽象为 `CompressionTool`

- **选择**：保留默认实现于 react_loop，但抽象为 trait，允许注入自定义
- **理由**：符合 Progressive 原则；保持核心 loop 不重构
- **替代方案 A**：保持内置 → 拒绝：与 Tool 化目标冲突
- **替代方案 C**：双模式（默认内置，可换 Tool）→ 拒绝：复杂，本质同 D7 选择

### D8：三层架构 Tool / Session / Provider

- **选择**：Agent 不直接持有 LLM client，通过 Provider trait 间接调用
- **理由**：opencode 验证过的模式；跨 Provider 可共用 reaction loop
- **替代方案**：直接持有 → 拒绝：难以替换 Provider

### D9：AgentTool factory wiring

- **选择**：将已实现 + 已注册的 AgentTool 接到主流程（factory 调用）
- **理由**：AgentTool 缺口是 wiring，不是实现
- **替代方案**：重写 AgentTool → 拒绝：现有实现已合格

### D10：`_xxx` 字段全部清掉

- **选择**：11 个 `_xxx` 字段全部处理（删除 / 重命名 / 启用）
- **理由**：silent-drop 是代码 smell，违背 P9（observability）
- **替代方案**：保留 → 拒绝：违反代码质量

### D11：Phase 切分

- **选择**：
  - Phase 1（6 周）：10 个 P0 PR（low/mid cost）
  - Phase 2（4 周）：5 个 P1 架构级 refactor
  - Phase 3（4 周）：可选高级模式（subscription / VFS / provider routing）
- **理由**：渐进交付，每 PR 独立可合并
- **替代方案**：一次大重构 → 拒绝：风险高、不可回滚

---

## Risks / Trade-offs

### R1：抽象成本 vs 收益

[Risk] 拆 Tool trait 为 3 sub-trait 会增加类型擦除开销（`Arc<dyn ToolDefinition + ToolExecution + ToolLifecycle>`）
→ Mitigation: Phase 1 测 benchmark；如果开销 < 5% 接受；否则合并 2 个 sub-trait

### R2：向后兼容压力

[Risk] AgentMessage 加 `llm_visible()` 可能误用为性能 hot path
→ Mitigation: 文档明确"该 method 应为 O(1) bool，无副作用"；加 linter 规则

### R3：Tool Registry 双索引同步

[Risk] HashMap + Vec 双写可能不一致
→ Mitigation: 封装为 `ToolRegistry::insert` 单一 API，内部维护同步

### R4：Compaction 抽象过度

[Risk] `CompressionTool` 抽象让简单项目也要实现 trait
→ Mitigation: 提供 `DefaultCompactionTool` 默认实现；用户不实现也能用

### R5：Phase 1 时间估算乐观

[Risk] 10 PR × ≤ 3 天 = 30 天，但实际可能因 review / 测试拖到 6 周+
→ Mitigation: PR 切分时已留 buffer；如某 PR 超期则重新切分

### R6：OpenSpec 流程本身成本

[Risk] 8 个 artifact（brainstorm → design → proposal → specs → tasks → plan → verify → retrospective）流程长
→ Mitigation: 已有综合报告作为底稿；每个 artifact 是结构化重组而非从零写

[Trade-off] 全栈对标 vs 范围失控
→ 接受理由：用户已选 A，且 Phase 切分限制每阶段 scope

[Trade-off] Tool 化 vs 性能开销
→ 接受理由：抽象层 overhead 远小于 LLM 调用 latency（典型 100-500ms vs <1μs）

---

## Migration Plan

### Phase 1（6 周，10 PR）

每个 PR 独立可合并、可回滚：

| PR | 标题 | 估时 | 依赖 |
|----|------|------|------|
| #P0-1 | `_xxx` 字段清理（11 个） | 2 天 | — |
| #P0-2 | Tool trait 拆 3 sub-trait | 3 天 | — |
| #P0-3 | ToolRegistry 双索引 | 1 天 | #P0-2 |
| #P0-4 | AgentMessage + llm_visible() + MessageKind | 2 天 | — |
| #P0-5 | Provider trait 引入（最小可用） | 3 天 | — |
| #P0-6 | AgentTool factory wiring | 1 天 | #P0-2 |
| #P0-7 | CompactionTool trait + DefaultCompactionTool | 3 天 | — |
| #P0-8 | ToolPermission trait（借鉴 codex C） | 2 天 | #P0-2 |
| #P0-9 | AgentToolCategory + 工具分类 metadata | 1 天 | #P0-2 |
| #P0-10 | CompressionTool 默认实现 + wire 到 react_loop | 2 天 | #P0-7 |

### Phase 2（4 周，5 个 P1 refactor）

待 Phase 1 完成后设计。

### Phase 3（4 周，可选）

- SubscriptionGate（codex B 模式，**待评估 OAuth 需求**）
- VFS sandbox（codex F，**待评估部署目标**）
- Provider routing（多 Provider 切换）

### Rollback Strategy

- **每 PR 独立分支 + revert**：如果某 PR 引入 regression，立即 revert
- **Feature flag**：Tool sub-trait 引入时保留 `ToolV1` alias，2 个 minor 后删除
- **版本策略**：Phase 1 全在 minor version 内（不破坏 semver）

### Acceptance Criteria

- [ ] 全部 10 PR merged
- [ ] `cargo +nightly fmt --all` 无差异
- [ ] `cargo clippy --all-targets --all-features --tests --all` 0 warning
- [ ] `cargo test --all` 通过
- [ ] `cargo miri test` 通过（关键 unsafe）
- [ ] 文档更新（每个 PR 含 CHANGELOG.md）

---

## Open Questions

1. **Subscription 模型是否需要？** — 当前 API key 路线够用；如未来有 OAuth 需求，加 `SubscriptionGate` (codex B 模式)
2. **VFS sandbox 是否引入？** — 取决于部署目标；如果是 CLI 工具则需要 (codex F)
3. **AgentMessage 破坏性 vs 非破坏性** — 优先非破坏（加 `llm_visible()` + `MessageKind`），下次 major 时再切 enum
4. **Provider routing** — 单 Provider 内置，多 Provider 可作为 Phase 3
5. **OTel integration 是否激进** — synthia-telemetry 已做，需确认 Provider 端是否需要 trace propagation（pi-mono 反对 task-local）
6. **Tool sub-trait 拆法的最终边界** — 当前 ≤5 方法/个；具体每个 sub-trait 含哪些方法待 tasks.md 细化
7. **Phase 2 / 3 优先级** — 待 Phase 1 完成后通过 OpenSpec explore 模式再评估