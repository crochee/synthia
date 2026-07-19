## Context

P0-1（移除 cache_breaker，commit `8bf1080`）移除了 `SystemContext.cache_breaker` 随机字段，commit message 声称"Cache namespace isolation is already handled by `prompt_cache_key` (user_id namespace) and applyCachePolicy (P0-2)"。但代码勘察（见 brainstorm.md 背景表 R1-R7）揭示这一前提不成立：

- `prompt_cache_key` 在代码库 0 匹配（R3）——spec 提及但未实现
- `applyCachePolicy` 在所有 4 处生产路径均为 `cache_policy: None`（R1）——实现完整但从未被触发
- 两个 `CacheControlMark` 同名异构（R4）——context 层算的 `CacheScope` hash 永远不传到 provider
- `compute_hash` 用 `DefaultHasher::new()` 随机 seed（R5）——违反 spec L57
- `CacheBreakDetector::check_cache_break` 逻辑破损（R6）——`if hash != 0` 永远为真
- `SystemContext`（git 信息）是死代码，未在 lib.rs 声明（R2）
- 无 `Source` trait 抽象（R7）——无法统一追踪前缀影响来源

**约束**：
- P1（前缀一致性）是最高优先级原则——违反代价 10x 成本放大
- AGENTS.md 运行规则：不引入新依赖除非必要；代码格式化与 clippy 必须通过
- 工作区已有 `ahash` 依赖（`mark.rs::hash_to_u64` 使用），确定性 hash 无新依赖
- `cache-policy-injection` spec 已定义 `supports_inline_cache_hints()` provider 守卫——接入生产路径无需新增分支

**stakeholders**：synthia SaaS（server-v2）、SDK、IDE Agent 三场景均依赖缓存前缀一致性控制成本。

## Goals / Non-Goals

**Goals:**
- 交付端到端可验证的缓存前缀一致性链路：source → snapshot → hash → policy → provider
- 引入 `trait Source` 抽象（opencode baseline/update/removed 生命周期）统一追踪 3 类前缀来源
- 统一两个同名异构的 `CacheControlMark` 为单一类型，让 `CacheScope` 端到端传到 provider 层
- 把 `applyCachePolicy` 接入 4 处生产路径，激活 Anthropic prompt caching
- 修复 `compute_hash` 确定性（ahash）与 `CacheBreakDetector` diff 逻辑（baseline vs current）
- 删除 `SystemContext`（git 信息）死代码

**Non-Goals:**
- 不实现 `prompt_cache_key` 的 OpenAI 翻译（Anthropic 翻译在 `cache-policy-injection` spec 已覆盖；OpenAI 翻译为后续 P2）
- 不实现 P1-2 Guardian role（独立审查机制，正交于 Source 抽象）
- 不实现 P1-5 OTel exporter（本提案的 `prefix_hash` 日志为其预留接口）
- 不改造 `SkillProvider` trait 的 lazy load delta 机制（`SkillListSource` 只声明 Source 实现，技能集合变化检测另立 change）
- 不引入 git 信息进 system prompt（如未来需要，应走 ephemeral tool_result 追加末尾，而非前缀）

## Decisions

### D1：范围选择——宽方案（端到端修复）而非窄方案（仅 Source trait）

- **选择**：方案 B（宽）——同时修 R1/R4/R5/R6/R7 + 决策 R2
- **理由**：这些风险点位于同一条数据流（source → snapshot → hash → policy → provider），任何一处断裂都使整条链路失效。窄方案会留下"Source trait 落地但 prompt caching 仍未启用"的中间态，无法端到端验证收益
- **已考虑 alternative**：方案 A（窄，仅 Source trait + CacheBreakDetector 改造，~500 行）——拒绝，因为 R1/R4/R5 是当前生产已坏的 bug，Source trait 落地后仍无法验证（applyCachePolicy 没接入、CacheScope 不传 provider、hash 不确定）

### D2：SystemContext 死代码——删除而非复活

- **选择**：删除 `crates/synthia-context/src/system_context.rs`
- **理由**：(1) 死代码零行为变化；(2) git_branch/git_status 半衰期短（C6：分钟级），放 system prompt 会破坏 P1（TTL 失效 → prefix 漂移）；(3) `SystemContext` 名字与 opencode 的 SystemContext（prefix 来源管理）概念冲突——本提案引入的 Source 抽象才是对应物
- **已考虑 alternative**：方案 B（借 Source epoch 复活 git 信息）——拒绝，git status 在长 session 中变化会触发 prefix 漂移，方向错误；opencode 也不放 git status 进 system prompt

### D3：CacheControlMark 统一——unify 而非 bridge

- **选择**：删除 provider 层 `CacheControlMark { ttl_seconds }`，统一用 context 层 `CacheControlMark { ttl, scope, pinned }`，下沉到公共 crate
- **理由**：(1) R4 根因是同名异构，任何保留双类型的方案维持根因；(2) 下沉公共 crate 反转依赖方向，避免 provider→context 循环依赖；(3) opencode 也是单一类型跨层使用
- **已考虑 alternative**：方案 B（bridge，保留双类型，apply 时拷贝 scope）——拒绝，两份 scope 容易不同步，维持根因
- **依赖方向**：新建 `crates/synthia-cache-mark/`（或复用 `synthia-types` 若已存在公共类型 crate），`CacheControlMark`/`CacheScope`/`CacheTtl` 迁入；`synthia-context` 与 `synthia-provider` 均依赖之

### D4：applyCachePolicy 接入——assembler 层无条件注入 + provider 层守卫

- **选择**：4 处生产调用点 `None` → `Some(CachePolicy::default())`；provider 层 `supports_inline_cache_hints()` 守卫（spec 已定义）
- **理由**：(1) `CompletionRequest.cache_policy` 是 provider-neutral 字段，注入不需要知道 provider；(2) `apply_cache_policy` 已 idempotent（spec L54），多次注入无副作用；(3) 改动面小，4 处单点修改无新增分支
- **已考虑 alternative**：方案 A（无条件 Some，无守卫）——拒绝，非 Anthropic provider（OpenAI）不支持 inline cache hints 会写出无意义字段；方案 B 已含守卫无需此虑
- **接入点**：`assembler/pipeline.rs:61`、`service.rs:171`、`summarizer/generator.rs:143`、`agent/context.rs`

### D5：Source trait 设计——opencode baseline/update/removed

- **选择**：
  ```rust
  pub trait Source: Send + Sync {
      fn id(&self) -> SourceId;
      fn baseline(&self) -> SourceContent;
      fn update(&mut self) -> SourceDelta;
  }
  pub enum SourceDelta { Changed(SourceContent), Unchanged, Removed }
  ```
  3 个初始实现：`SystemPromptSource`、`ToolSchemasSource`、`SkillListSource`
- **理由**：opencode `packages/core/src/system-context/` 验证过的模式；baseline 是初始 epoch，update 返回 delta；`Removed` variant 覆盖 source 被移除场景（如技能卸载）
- **已考虑 alternative**：用 `Option<SourceDelta>`（None=unchanged）——拒绝，无法表达 Removed 语义，三 variant 更清晰
- **CacheBreakDetector 改造**：`HashMap<String, TrackedState>` → `HashMap<SourceId, SourceEpoch>`，`SourceEpoch { baseline_hash, current_hash, content }`；`check_cache_break` 遍历所有 source 比对 `baseline_hash != current_hash`（修 R6）

### D6：确定性 hash——全量替换为 ahash

- **选择**：`compute_hash` 内 `DefaultHasher::new()` → `ahash::AHasher::default()`
- **理由**：(1) ahash 已是 `synthia-context` 依赖；(2) 影响字段 system_hash/tools_hash/prefix_hash，cache_control_hash 已正确不动；(3) 单点修复 ~5 行，零行为风险（进程内比对，无跨进程持久化）
- **已考虑 alternative**：保留 DefaultHasher 仅修 CacheBreakDetector——拒绝，spec L57 明确要求 ahash，违约必须修

### D7：CacheBreakDetector diff 重写——随 D5 自动修复

- **选择**：不单独处理 R6，借 D5 的 `SourceEpoch.baseline_hash` vs `current_hash` 天然修复
- **理由**：Q6 是 Q5 的子集——D5 落地后 `check_cache_break` 变为遍历 source 比对 baseline/current，R6 自动消失
- **已考虑 alternative**：独立修 R6（加 `prev_system_hash` 字段）——拒绝，与 D5 重复改造 CacheBreakDetector，浪费

## Risks / Trade-offs

- [Risk] 公共 crate 新增可能引入 workspace 配置开销 → Mitigation: 先确认是否已有 `synthia-types` 等公共类型 crate 可复用；若无，新建 `synthia-cache-mark` 是最小化选择（只放 3 个类型 + tests）
- [Risk] provider crate 公共 API 类型变化（breaking）可能影响下游消费者 → Mitigation: transform_request 序列化输出保持兼容（None cache_policy 走 Text variant）；breaking 仅限 Rust API 层，不影响 wire format
- [Risk] Source trait 设计可能不覆盖未来场景（如 OTel span 作为 source） → Mitigation: trait 只暴露 id/baseline/update 三方法，最小化抽象；OTel 集成时若需可扩展 trait 而非破坏
- [Risk] 4 处 `None` → `Some(default)` 注入可能触发未测试的 applyCachePolicy 路径 → Mitigation: applyCachePolicy 已有 idempotent 测试（spec L64-68）；新增端到端集成测试覆盖 transform_request 序列化
- [Trade-off] 删除 SystemContext 死代码意味着未来若需 git 信息要重新实现 → 接受理由：死代码复活比保留死代码成本低；且方向应是 ephemeral tool_result 而非 system prompt 前缀
- [Trade-off] SkillListSource 声明但 SkillProvider delta 机制另立 change → 接受理由：本提案的 Source trait 落地后，SkillListSource 的 `update()` 可先返回 `Unchanged`，待 SkillProvider 改造后激活；不阻塞端到端验证

## Migration Plan

**部署顺序**（无 endpoint/DB 变更，纯库内）：
1. 新建公共 crate（或确认复用）+ 迁入 CacheControlMark/CacheScope/CacheTtl
2. 修复 `compute_hash` 确定性（D6）——独立可验证
3. 引入 Source trait + 3 个实现（D5）
4. 改造 CacheBreakDetector 为 SourceEpoch（D5/D7）
5. 统一 CacheControlMark 类型（D3）——provider crate 切换依赖
6. 接入 applyCachePolicy 到 4 处生产路径（D4）
7. 删除 SystemContext 死代码（D2）
8. 更新 specs（system-context/cache-control-mark/cache-policy-injection）

**验收条件**（端到端，对应 brainstorm 验证标准）：
- `cargo check --workspace` 全绿
- `cargo clippy --all-targets --all-features --tests --all` 零警告
- `cargo test -p synthia-context` + `-p synthia-provider` 全通过
- 端到端：`CompletionRequest.cache_policy == Some(CachePolicy::default())`
- 端到端：`AnthropicProvider::transform_request` 序列化含 `cache_control: {type: ephemeral}`
- 端到端：`CacheBreakDetector::check_cache_break` 在 system prompt 未变时返回 `system_prompt_changed: false`
- 端到端：`compute_hash` 跨进程一致（同内容同 hash）

**rollback**：git revert 单 commit 即可（无数据迁移）；建议按部署顺序分 8 个 commit 便于 bisect

## Open Questions

- 是否已存在 `synthia-types` 或类似公共类型 crate 可复用？需在实现阶段确认 workspace 结构——若无则新建 `synthia-cache-mark`
- `crates/synthia-context/src/prefix_tracker/` 目录（lib.rs 第 9 行声明 `pub mod prefix_tracker;`）与 `prompt/cache/detector.rs` 是否有职责重叠？勘察未深入该目录——实现 D5 前需先读 `prefix_tracker/` 确认是否需合并
- `SkillListSource::update()` 在 SkillProvider 改造前返回 `Unchanged` 是否影响 prefix_stability_ratio 观测？预期不影响（技能列表在 session 内通常不变），但需在端到端测试中验证
