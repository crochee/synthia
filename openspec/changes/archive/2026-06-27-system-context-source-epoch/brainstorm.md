<!--
Raw capture of design exploration for system-context-source-epoch.

本档原样捕捉设计探索过程，作为决策日志（背景 → 决议链 Q1-Qn → 设计取捨）。
design.md 从本档萃取并重新整理为结构化设计文档。

不使用 superpowers:brainstorming 交互式 skill——依据 AGENTS.md 运行规则
"不要主动向我提问，自己探索最佳路径实施"，本档基于代码勘察结果独立完成
设计探索，所有决策都有代码证据支撑。
-->

# Brainstorm: SystemContext Source/Epoch

## 背景（代码勘察证据）

P0-1（移除 cache_breaker，commit `8bf1080`）声称由 `prompt_cache_key`（命名空间隔离）
与 `applyCachePolicy`（P0-2）替代。但代码勘察揭示二者均**未在生产路径接入**，留下真实空缺：

| # | 风险 | 证据 | 违反原则 |
|---|------|------|----------|
| R1 | `applyCachePolicy` 在所有 4 处生产路径均为 `cache_policy: None` | `assembler/pipeline.rs:61`、`service.rs:171`、`summarizer/generator.rs:143`、`agent/context.rs` | P1（前缀一致性）失效——Anthropic prompt caching 实际未启用 |
| R2 | `SystemContext` 是死代码——`crates/synthia-context/src/lib.rs` 无 `pub mod system_context;` 声明 | `cargo check -p synthia-context` 通过；全代码库 0 处引用 | spec 与现实不符；5min TTL 若启用会重蹈 cache_breaker 覆辙 |
| R3 | `prompt_cache_key` 在代码库 0 个匹配——spec 提及但未实现 | Grep `prompt_cache_key` → 无结果 | spec 契约未履行 |
| R4 | 两个 `CacheControlMark` 同名异构：context 层 `{ttl, scope, pinned}` 有 scope；provider 层 `{ttl_seconds}` 无 scope | `mark.rs:49-72` vs `cache_policy.rs:72-75` | R5/R6 跨用户 cache 泄露——CacheScope 算了 hash 但永远不传到 provider |
| R5 | `compute_hash` 用 `DefaultHasher::new()`（随机 seed） | `types.rs:3-7` | 违反 `cache-control-mark` spec L57；进程重启后 hash 全变，跨进程无法比对（P9 可观测性失效） |
| R6 | `CacheBreakDetector::check_cache_break` 逻辑破损——`if state.system_hash != 0` 永远为真 | `detector.rs:86-106`；`TrackedState` 无 `prev_system_hash` 字段 | diff 永远报"system prompt changed"，无法定位真正变化来源 |
| R7 | 无 `Source` trait 抽象——`state_by_source` 是 `HashMap<String, TrackedState>`，source 是裸 `&str` key | `detector.rs:5-8` | 无法统一追踪 system prompt / tools schema / skill list / git context 的前缀影响来源 |

记忆中 P1-4 描述（~500 行）基于"cache_breaker 已被 applyCachePolicy 替代"的前提——**前提不成立**。
真实工作量需覆盖：填空缺（R1/R4）+ 修 bug（R5/R6）+ 引入 Source 抽象（R7）+ 决策死代码（R2）。

---

## 决议链

### Q1: 本提案的范围边界——只做 Source trait，还是覆盖整个断裂链路？

**取舍**：
- **方案 A（窄）**：仅引入 `trait Source` 抽象 + baseline/update/removed 生命周期，改造 `CacheBreakDetector`。
  优点：范围聚焦，~500 行符合记忆估算。
  缺点：R1/R4/R5 留作后续 change，但它们是**当前生产已坏**的 bug——Source trait 落地后仍无法验证收益，
  因为 applyCachePolicy 没接入、CacheScope 不传 provider、hash 不确定。
- **方案 B（宽）**：Source trait + 修 R1（接入 applyCachePolicy）+ 修 R4（统一 CacheControlMark）+ 修 R5（确定性 hash）+ 修 R6（CacheBreakDetector diff）+ 决策 R2。
  优点：交付一个**端到端可验证**的缓存前缀一致性机制。
  缺点：工作量上升至 ~800-1000 行；但每一步都是同一个数据流的修复，拆开反而引入中间态风险。

**决议**：**方案 B**。理由——这些风险点位于同一条数据流（source → snapshot → hash → policy → provider），
任何一处断裂都使整条链路失效。窄方案会留下"Source trait 落地但 prompt caching 仍未启用"的尴尬中间态。
但 R2（SystemContext 死代码）独立处理，见 Q2。

---

### Q2: SystemContext 死代码（R2）——删除还是借 Source epoch 复活？

**取舍**：
- **方案 A（删除）**：删除 `crates/synthia-context/src/system_context.rs`，更新 spec 标记为 deprecated。
  理由：git_branch/git_status 信息半衰期短（C6：分钟级），放进 system prompt 会破坏 P1（每次 TTL 失效 → prefix 漂移）；
  且 opencode 的 SystemContext 语义不同（指 prefix 来源管理，不是 git 信息）。
- **方案 B（复活）**：将 git 信息作为 Source 之一接入 epoch 机制。
  问题：git status 在长 session 中可能变化（用户 commit/checkout），TTL 失效会让 prefix 漂移——
  这正是 cache_breaker 当初想解决的问题，但方向错了（随机扰动不是解法，epoch 化才是）。
  然而git 信息是否真的需要进 system prompt？opencode 不放 git status。

**决议**：**方案 A（删除死代码）**。理由：
1. git_branch/git_status 当前根本未接入 system prompt（死代码），删除零行为变化；
2. 即便未来需要 git 信息，也应作为**独立的 ephemeral tool_result**（C6：分钟级半衰期）追加到末尾，而非 system prompt 前缀；
3. 复用 `SystemContext` 这个名字会与 opencode 的 SystemContext（prefix 来源管理）概念混淆——
   本提案引入的 Source 抽象才是 opencode SystemContext 的对应物。
4. 更新 `openspec/specs/system-context/spec.md`：移除 git 信息相关 Requirement，重定义为 Source trait 的 spec 载体。

---

### Q3: 两个 `CacheControlMark` 同名异构（R4）——unify 还是 bridge？

**取舍**：
- **方案 A（unify）**：删除 provider 层的 `CacheControlMark { ttl_seconds }`，统一使用 context 层的
  `CacheControlMark { ttl, scope, pinned }`。provider transform 时从统一类型翻译到 Anthropic/OpenAI 格式。
  优点：单一数据源，scope 自然随附到 provider。
  缺点：provider crate 依赖 context crate 的 `mark` 模块——增加 crate 耦合。
- **方案 B（bridge）**：保留两个类型，在 `apply_cache_policy` 时把 context 层的 scope 字段拷贝到 provider 层新加的 scope 字段。
  问题：两份 scope 容易不同步；同名异构是 R4 的根因，bridge 维持了根因。

**决议**：**方案 A（unify）**。理由：
1. R4 的根因是"同名异构"——任何保留双类型的方案都维持根因；
2. provider→context 的依赖方向可以反转——把 `CacheControlMark` / `CacheScope` / `CacheTtl` 下沉到一个
   轻量的 `synthia-cache-mark` 子 crate（或直接放 `synthia-provider` 的公共 types 模块），
   context 和 provider 都依赖它，避免循环依赖；
3. opencode 的 `CacheControlMark` 也是单一类型跨层使用。

**依赖方向**：新建 `crates/synthia-cache-mark/`（或复用 `synthia-types` 若已存在公共类型 crate），
`CacheControlMark` / `CacheScope` / `CacheTtl` 迁入。`synthia-context` 和 `synthia-provider` 均依赖之。

---

### Q4: 如何安全地把 `applyCachePolicy` 接入生产路径（R1）？

**取舍**：
- **方案 A（无条件 Some）**：所有 `ContextAssembler::prepare` / `DefaultContextService::assemble` 把 `cache_policy: None` 改为 `Some(CachePolicy::default())`。
  问题：非 Anthropic provider（OpenAI）不支持 inline cache hints，会写出无意义字段。
- **方案 B（provider 感知）**：在 assembler 层注入 `CachePolicy::default()`，
  但在 `AnthropicProvider::transform_request` 内部用 `supports_inline_cache_hints()` 守卫（spec L120-143 已定义）。
  非支持 provider 的 transform 路径忽略 cache_policy。
  问题：assembler 层不知道 provider 是否支持——它构造的是 provider-neutral `CompletionRequest`。

**决议**：**方案 B + 在 assembler 层无条件注入**。理由：
1. spec `cache-policy-injection` L120-143 已定义 `supports_inline_cache_hints()` 守卫——provider 层会自动忽略；
2. `CompletionRequest.cache_policy: Option<CachePolicy>` 是 provider-neutral 字段，注入不需要知道 provider；
3. `apply_cache_policy` 已是 idempotent（spec L54），多次注入无副作用；
4. 改动面小：4 处 `None` → `Some(CachePolicy::default())`，无新增分支。

**接入点**（4 处）：
- `crates/synthia-context/src/assembler/pipeline.rs:61`
- `crates/synthia-context/src/service.rs:171`
- `crates/synthia-context/src/summarizer/generator.rs:143`
- `crates/synthia-agent/src/context.rs`（assemble_context）

---

### Q5: Source trait 设计——opencode baseline/update/removed 如何落地？

**opencode 参考**（`packages/core/src/system-context/`）：
- `trait Source`：`id() -> SourceId`、`baseline() -> Vec<u8>`、`update() -> Option<SourceDelta>`、`removed() -> bool`
- Source 的 baseline 是初始 epoch 内容；update 返回 delta（Some 表示变化，None 表示不变）；removed 表示 source 被移除。
- `SystemContext` 持有 `Vec<Box<dyn Source>>`，每次 build_context 时遍历所有 source，计算 epoch。

**synthia 落地**：
```rust
pub trait Source: Send + Sync {
    fn id(&self) -> SourceId;           // &'static str 或 Cow<'static, str>
    fn baseline(&self) -> SourceContent; // 初始内容
    fn update(&mut self) -> SourceDelta; // Some(changed) | None(unchanged) | Removed
}

pub enum SourceDelta {
    Changed(SourceContent),
    Unchanged,
    Removed,
}
```

**Source 实现**（初始集，3 个）：
1. `SystemPromptSource` —— system prompt 文本（含技能索引行）
2. `ToolSchemasSource` —— tools schema 的 canonical JSON
3. `SkillListSource` —— 技能列表（lazy load 后技能集合变化时触发 delta）

**不**纳入 Source 的：
- `SystemContext`（git 信息）—— Q2 已决定删除
- messages / tool_results —— 这些是 append-only 末尾内容，不是前缀来源

**CacheBreakDetector 改造**：
- `state_by_source: HashMap<String, TrackedState>` → `HashMap<SourceId, SourceEpoch>`
- `SourceEpoch { baseline_hash: u64, current_hash: u64, content: SourceContent }`
- `check_cache_break` 真正比对 `baseline_hash != current_hash`（修 R6）

---

### Q6: 确定性 hash 修复（R5）范围？

**决议**：全量替换 `compute_hash` 内的 `DefaultHasher::new()` 为 `ahash::AHasher::default()`。
- `ahash` 已是 `synthia-context` 的依赖（`mark.rs` 的 `hash_to_u64` 已用）；
- 影响字段：`system_hash` / `tools_hash` / `prefix_hash`（均经 `compute_hash`）；
- `cache_control_hash` 已正确用 ahash，不动；
- 这是单点修复，~5 行改动，零行为风险（hash 值会变，但 CacheBreakDetector 是进程内比对，无跨进程持久化依赖）。

---

### Q7: CacheBreakDetector diff 逻辑重写（R6）范围？

**当前 bug**（`detector.rs:86-106`）：
```rust
if state.system_hash != 0 {
    report.system_prompt_changed = true;  // ← 永远为真
}
```
没有 `prev_system_hash` 比对。

**重写方案**：借 Q5 的 Source 抽象——`SourceEpoch` 自带 `baseline_hash` 和 `current_hash`，
diff 逻辑天然变成 `baseline_hash != current_hash`。`check_cache_break` 变为遍历 `state_by_source`，
对每个 source 报告 `SourceDelta`，聚合为 `CacheBreakReport`。

**与 Q5 的关系**：Q6 是 Q5 的子集——Q5 落地后 R6 自动修复。不再单独处理。

---

## 设计取捨总结

| 决议 | 选择 | 理由 |
|------|------|------|
| Q1 范围 | 方案 B（宽） | 同一数据流的断裂点必须一起修，否则端到端不可验证 |
| Q2 SystemContext | 方案 A（删除） | 死代码 + 名字冲突 + 概念错位；git 信息应走 ephemeral tool_result |
| Q3 CacheControlMark | 方案 A（unify） | 同名异构是根因；下沉到公共 crate 解决依赖方向 |
| Q4 接入生产 | 方案 B（provider 感知） | spec 已定义守卫；assembler 无条件注入 Some(default) |
| Q5 Source trait | opencode baseline/update/removed | 3 个初始 Source；CacheBreakDetector 改造为 SourceEpoch |
| Q6 hash 确定性 | 全量替换为 ahash | 单点修复，零行为风险 |
| Q7 CacheBreakDetector | 随 Q5 自动修复 | baseline_hash != current_hash |

---

## 不在本提案范围

- **P1-2 Guardian role**：独立审查机制，与 Source 抽象正交，另立 change。
- **P1-3 landlock fallback**：容器沙箱，无关前缀一致性。
- **P1-5 OTel 集成**：遥测，本提案的可观测性（prefix_hash 日志）为其预留接口但不实现 OTel exporter。
- **prompt_cache_key 的 OpenAI 翻译**：本提案只统一 CacheControlMark 携带 scope，Anthropic 翻译在
  `cache-policy-injection` spec 已覆盖；OpenAI `prompt_cache_key` 翻译作为后续 P2。
- **skill 列表的 lazy load delta 机制**：Q5 的 `SkillListSource` 只声明 trait 实现，实际技能列表变化检测
  依赖 `SkillProvider` trait，其改造另立 change。

---

## 验证标准（端到端）

本提案完成后，以下端到端链路必须可验证：
1. `ContextAssembler::assemble` 产出的 `CompletionRequest.cache_policy == Some(CachePolicy::default())`
2. `AnthropicProvider::transform_request` 序列化出的 JSON 包含 `cache_control: {type: ephemeral}` 在 last tool / last user msg / system block
3. `cache_control` JSON 携带由 `CacheScope` 派生的 namespace 字段（user_id 隔离）
4. `CacheBreakDetector::check_cache_break` 在 system prompt 未变时返回 `system_prompt_changed: false`（修 R6）
5. `compute_hash` 的输出在同一内容上跨进程一致（修 R5）
6. `SystemContext`（git 信息）从代码库删除，`cargo check` 全绿
7. `prefix_stability_ratio` 可观测（Source epoch 日志记录 baseline/current hash）
