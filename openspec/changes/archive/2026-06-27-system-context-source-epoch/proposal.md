## Why

P0-1 移除了 cache_breaker（commit `8bf1080`）声称由 `applyCachePolicy`（P0-2）与 `prompt_cache_key` 替代。但代码勘察揭示：applyCachePolicy 在所有 4 处生产路径均为 `cache_policy: None`（R1）；`prompt_cache_key` 代码库 0 匹配（R3）；两个 `CacheControlMark` 同名异构，CacheScope 算了 hash 却永不传到 provider（R4）；`compute_hash` 用随机 seed 的 `DefaultHasher`（R5）；`CacheBreakDetector::check_cache_break` 逻辑破损——`if hash != 0` 永远为真（R6）；`SystemContext` 是死代码未编入 lib.rs（R2）。整条缓存前缀一致性链路（source → snapshot → hash → policy → provider）多处断裂，导致 Anthropic prompt caching 实际未启用且跨用户隔离失效。现在处理因为每多一轮对话都在付全价 cache miss 成本（10x 放大）。

## What Changes

**引入 Source trait 抽象（opencode 风格 baseline/update/removed）**
- From: `CacheBreakDetector::state_by_source: HashMap<String, TrackedState>`，source 是裸 `&str` key，无类型化生命周期
- To: `HashMap<SourceId, SourceEpoch>`，每个 source 实现 `trait Source { id(); baseline(); update() -> SourceDelta }`
- Reason: 统一追踪 system prompt / tools schema / skill list 的前缀影响来源；顺带修复 R6（diff 逻辑破损）
- Impact: non-breaking（内部重构），但 CacheBreakDetector 行为变化（不再误报 changed）

**统一 CacheControlMark 类型**
- From: 两个同名异构类型——`synthia-context::prompt::mark::CacheControlMark { ttl, scope, pinned }`（有 scope）与 `synthia-provider::cache_policy::CacheControlMark { ttl_seconds }`（无 scope）
- To: 单一 `CacheControlMark { ttl, scope, pinned }` 下沉到公共 crate，context 和 provider 共用
- Reason: R4 根因是同名异构；CacheScope 算了 hash 但永远不传到 provider，跨用户 cache 泄露
- Impact: breaking（provider crate 公共 API 类型变化），但 transform_request 行为保持兼容

**接入 applyCachePolicy 到生产路径**
- From: 4 处生产调用点 `cache_policy: None`（assembler/pipeline.rs:61、service.rs:171、summarizer/generator.rs:143、agent/context.rs）
- To: `cache_policy: Some(CachePolicy::default())`，由 provider 层 `supports_inline_cache_hints()` 守卫
- Reason: R1——applyCachePolicy 实现完整但从未被触发，Anthropic prompt caching 实际未启用
- Impact: non-breaking（idempotent，provider 层自动忽略不支持的场景）

**修复确定性 hash**
- From: `compute_hash` 用 `DefaultHasher::new()`（SipHash13 随机 seed）
- To: `ahash::AHasher::default()`（确定性）
- Reason: R5——违反 `cache-control-mark` spec L57；进程重启后 hash 全变，prefix_stability_ratio 无法测量
- Impact: non-breaking（进程内比对，无跨进程持久化）

**删除 SystemContext 死代码**
- From: `crates/synthia-context/src/system_context.rs` 存在但未在 `lib.rs` 声明，`cargo check` 通过说明未编译
- To: 删除文件，更新 spec 移除 git 信息相关 Requirement
- Reason: R2——死代码 + 名字与 opencode SystemContext 概念冲突；git 信息半衰期短（C6）不应进 system prompt 前缀，应走 ephemeral tool_result
- Impact: non-breaking（删除未编译的代码零行为变化）

## Capabilities

### New Capabilities

- `prefix-source-trait`: Source trait 抽象与 SourceDelta/SourceEpoch 生命周期，统一追踪 prefix 来源（system prompt / tools schema / skill list）的 baseline/update 变化；包含 3 个初始 Source 实现

### Modified Capabilities

- `cache-control-mark`: 统一两个同名异构的 CacheControlMark 类型为单一类型并下沉公共 crate；CacheScope 跟随 scope 字段传到 provider 层；修复 `compute_hash` 用确定性 ahash
- `cache-policy-injection`: 在 4 处生产 assembler 路径注入 `Some(CachePolicy::default())`，激活 applyCachePolicy 端到端链路
- `system-context`: 移除 git 信息相关 Requirement（git_branch/git_status/TTL cache），删除死代码文件；保留 spec 作为 Source trait 的 spec 载体（重命名概念）

## Impact

**代码**：
- 新建 `crates/synthia-cache-mark/`（或复用现有公共类型 crate）承载统一的 CacheControlMark/CacheScope/CacheTtl
- 新建 `crates/synthia-context/src/source.rs`（或 `source/` 目录）承载 Source trait + 3 个实现
- 改造 `crates/synthia-context/src/prompt/cache/detector.rs`：HashMap key 类型化、diff 逻辑重写
- 修改 `crates/synthia-context/src/prompt/cache/types.rs`：`compute_hash` 换 ahash
- 修改 `crates/synthia-provider/src/cache_policy.rs`：删除本地 CacheControlMark，依赖公共 crate
- 修改 4 处 assembler/service/agent：`None` → `Some(CachePolicy::default())`
- 删除 `crates/synthia-context/src/system_context.rs`

**API**：provider crate 公共类型 CacheControlMark 字段变化（breaking），但 transform_request 序列化输出保持兼容

**依赖**：新增 `ahash`（已在 mark.rs 用，确认）；可能新增 `crates/synthia-cache-mark` workspace 成员

**系统**：无 endpoint / DB / 部署变更；纯库内重构 + bug 修复
