# SystemContext Source/Epoch Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task.

**Goal:** 修复 cache_breaker 移除后断裂的缓存前缀一致性链路，引入 Source trait 抽象统一追踪 prefix 来源，激活 applyCachePolicy 端到端。

**Architecture:** 新建 `synthia-cache-mark` 公共 crate 承载统一的 CacheControlMark（含 CacheScope）；在 `synthia-context` 引入 opencode 风格 `Source` trait（baseline/update/removed 生命周期）+ 3 个实现；改造 CacheBreakDetector 用 SourceEpoch 的 baseline_hash vs current_hash 替代破损的 `if hash != 0`；在 4 处生产 assembler 路径注入 `Some(CachePolicy::default())`；修复 compute_hash 确定性（ahash）；删除 SystemContext 死代码。

**Tech Stack:** Rust（workspace 多 crate）、ahash（确定性 hash）、serde_json（canonical 序列化）、parking_lot::Mutex、openspec（spec 验证）

---

## Task 1: 前置勘察与决策确认

- [ ] **Step 1.1:** 读取 `crates/synthia-context/src/prefix_tracker/` 全部文件（`LS` + `Read`），确认与 `prompt/cache/detector.rs` 是否职责重叠；若重叠，在 worktree 笔记记录合并方案
- [ ] **Step 1.2:** 运行 `ls crates/` + 读根 `Cargo.toml` 的 `[workspace] members`，确认是否已有公共类型 crate；若无则决定新建 `crates/synthia-cache-mark/`
- [ ] **Step 1.3:** 读 `crates/synthia-context/Cargo.toml` 确认 `ahash` 在 `[dependencies]`（`mark.rs::hash_to_u64` 已用）
- [ ] **Commit:** `chore: confirm prefix_tracker overlap and workspace structure for system-context-source-epoch`

---

## Task 2: 公共 crate 承载统一 CacheControlMark（D3）

- [ ] **Step 2.1:** 若 1.2 决定新建：创建 `crates/synthia-cache-mark/Cargo.toml`（`[package] name = "synthia-cache-mark"`，`[dependencies] serde = { version = "...", features = ["derive"] }, ahash = "..."`）；在根 `Cargo.toml` 的 `[workspace] members` 加入 `"crates/synthia-cache-mark"`
- [ ] **Step 2.2:** 创建 `crates/synthia-cache-mark/src/lib.rs`，将 `crates/synthia-context/src/prompt/mark.rs` 中的 `CacheControlMark`/`CacheScope`/`CacheTtl`（含全部 derive 和 `impl`）迁移过去
- [ ] **Step 2.3:** 在 `synthia-cache-mark/src/lib.rs` 实现 `impl CacheControlMark { pub fn hash_to_u64(&self) -> u64 { ... } }` 用 `ahash::AHasher::default()` over `(ttl, &scope.0, pinned)`
- [ ] **Step 2.4:** 在 `crates/synthia-context/Cargo.toml` 加 `synthia-cache-mark = { path = "../synthia-cache-mark" }`；修改 `crates/synthia-context/src/prompt/mark.rs` 为 `pub use synthia_cache_mark::{CacheControlMark, CacheScope, CacheTtl};`（保持对外 re-export）
- [ ] **Step 2.5:** 在 `crates/synthia-provider/Cargo.toml` 加 `synthia-cache-mark = { path = "../synthia-cache-mark" }`
- [ ] **Step 2.6:** 删除 `crates/synthia-provider/src/cache_policy.rs` 中本地 `CacheControlMark { ttl_seconds }` 定义；改 import 统一类型
- [ ] **Step 2.7:** 运行 `cargo check -p synthia-cache-mark -p synthia-context -p synthia-provider` 确认编译
- [ ] **Step 2.8:** 运行 `cargo +nightly fmt --all` && `cargo clippy --all-targets --all-features --tests --all` 修警告
- [ ] **Test:** `cargo test -p synthia-cache-mark`（新增 hash_to_u64 确定性测试：同 mark 两次 hash 相等）
- [ ] **Commit:** `refactor(cache-mark): unify CacheControlMark into shared synthia-cache-mark crate`

---

## Task 3: 修复确定性 hash（D6 / R5）

- [ ] **Step 3.1:** 读 `crates/synthia-context/src/prompt/cache/types.rs` 确认 `compute_hash` 当前用 `DefaultHasher::new()`
- [ ] **Step 3.2:** 修改 `compute_hash`：`std::collections::hash_map::DefaultHasher::new()` → `ahash::AHasher::default()`；保留函数签名
- [ ] **Step 3.3:** 添加测试 `test_compute_hash_deterministic`：`assert_eq!(compute_hash("abc"), compute_hash("abc"))`
- [ ] **Step 3.4:** 添加测试 `test_compute_hash_differs_for_different_content`：`assert_ne!(compute_hash("abc"), compute_hash("abd"))`
- [ ] **Test:** `cargo test -p synthia-context compute_hash`
- [ ] **Commit:** `fix(cache): use deterministic ahash for compute_hash (R5)`

---

## Task 4: 引入 Source trait 与基础类型（D5）

- [ ] **Step 4.1:** 创建 `crates/synthia-context/src/source/mod.rs`，定义 `pub trait Source: Send + Sync { fn id(&self) -> SourceId; fn baseline(&self) -> SourceContent; fn update(&mut self) -> SourceDelta; }`
- [ ] **Step 4.2:** 在同文件定义 `#[derive(Debug, Clone, Eq, PartialEq, Hash)] pub struct SourceId(pub &'static str);`
- [ ] **Step 4.3:** 定义 `#[derive(Debug, Clone)] pub struct SourceContent(pub Vec<u8>);` + `impl SourceContent { pub fn hash(&self) -> u64 { let mut h = ahash::AHasher::default(); h.write(&self.0); h.finish() } }`
- [ ] **Step 4.4:** 定义 `#[derive(Debug, Clone)] pub enum SourceDelta { Changed(SourceContent), Unchanged, Removed }`
- [ ] **Step 4.5:** 在 `crates/synthia-context/src/lib.rs` 加 `pub mod source;`
- [ ] **Step 4.6:** 添加测试 `source::tests::test_source_content_hash_deterministic`：同 bytes 两次 hash 相等
- [ ] **Step 4.7:** 添加测试 `source::tests::test_source_delta_variants`：Unchanged/Removed 是 unit variant，Changed 携带 SourceContent
- [ ] **Test:** `cargo test -p synthia-context source::`
- [ ] **Commit:** `feat(context): introduce Source trait with SourceId/SourceContent/SourceDelta`

---

## Task 5: 实现 SourceEpoch（D5）

- [ ] **Step 5.1:** 创建 `crates/synthia-context/src/source/epoch.rs`，定义 `#[derive(Debug, Clone)] pub struct SourceEpoch { baseline_hash: u64, current_hash: u64, content: SourceContent, removed: bool }`
- [ ] **Step 5.2:** 实现 `SourceEpoch::new(content: SourceContent) -> Self`：`baseline_hash = current_hash = content.hash()`, `removed = false`
- [ ] **Step 5.3:** 实现 `SourceEpoch::is_changed(&self) -> bool`：`self.baseline_hash != self.current_hash && !self.removed`
- [ ] **Step 5.4:** 实现 `SourceEpoch::apply_delta(&mut self, delta: SourceDelta)`：match Changed→更新 current_hash+content；Unchanged→不动；Removed→removed=true
- [ ] **Step 5.5:** 在 `source/mod.rs` 加 `pub mod epoch;` + `pub use epoch::SourceEpoch;`
- [ ] **Step 5.6:** 测试 `test_new_epoch_not_changed`：new 后 is_changed == false
- [ ] **Step 5.7:** 测试 `test_changed_delta_flips_is_changed`：apply_delta(Changed) 后 is_changed == true 且 current_hash == new hash
- [ ] **Step 5.8:** 测试 `test_unchanged_delta_preserves_state`：apply_delta(Unchanged) 后状态不变
- [ ] **Step 5.9:** 测试 `test_removed_delta_marks_removed`：apply_delta(Removed) 后 removed == true
- [ ] **Step 5.10:** 测试 `test_zero_hash_no_false_positive`：构造 content 使 hash 恰好为 0（或用 mock），baseline==current==0 时 is_changed == false
- [ ] **Test:** `cargo test -p synthia-context source::epoch`
- [ ] **Commit:** `feat(context): implement SourceEpoch with baseline/current hash diff`

---

## Task 6: 实现 3 个 Source 实现（D5）

- [ ] **Step 6.1:** 创建 `crates/synthia-context/src/source/system_prompt.rs`：`pub struct SystemPromptSource { text: String, baseline_content: SourceContent, prev_hash: u64 }`；`impl Source`：`id()=SourceId("system-prompt")`，`baseline()` 返回 baseline_content clone，`update()` 比对当前 text hash 与 prev_hash
- [ ] **Step 6.2:** 测试 SystemPromptSource：相同 text→update 返回 Unchanged；不同 text→Changed
- [ ] **Step 6.3:** 创建 `crates/synthia-context/src/source/tool_schemas.rs`：`pub struct ToolSchemasSource { canonical: SourceContent }`；`new(tools: &[ToolDefinition])` 按 name 排序后 `serde_json::to_string_pretty`（确认 serde_json 是否支持 sorted keys，若无用 `serde_json::Value` + 手动 sort）
- [ ] **Step 6.4:** 测试 ToolSchemasSource：`[A,B,C]` 与 `[B,A,C]` baseline hash 相等；`[A,B]` 与 `[A,B,C]` 不同
- [ ] **Step 6.5:** 创建 `crates/synthia-context/src/source/skill_list.rs`：`pub struct SkillListSource { ... }`；`new(skill_ids: Vec<String>)` sort+join 后存 baseline；`update()` 恒返回 `Unchanged`
- [ ] **Step 6.6:** 测试 SkillListSource：`update()` 返回 Unchanged；`["a","b"]` 与 `["a","c"]` baseline hash 不同
- [ ] **Step 6.7:** 在 `source/mod.rs` 加 `pub mod system_prompt; pub mod tool_schemas; pub mod skill_list;` + re-exports
- [ ] **Test:** `cargo test -p synthia-context source::`
- [ ] **Commit:** `feat(context): implement SystemPromptSource, ToolSchemasSource, SkillListSource`

---

## Task 7: 改造 CacheBreakDetector 为 SourceEpoch（D5/D7 / R6）

- [ ] **Step 7.1:** 读 `crates/synthia-context/src/prompt/cache/detector.rs` 全文，理解当前 `state_by_source: HashMap<String, TrackedState>` 与 `record_prompt_state` / `check_cache_break`
- [ ] **Step 7.2:** 修改 `CacheBreakDetector.state_by_source` 类型为 `HashMap<SourceId, SourceEpoch>`；删除 `TrackedState` 依赖（若 types.rs 中 TrackedState 仅此处用，一并清理）
- [ ] **Step 7.3:** 实现 `pub fn record_source(&mut self, source: &dyn Source)`：match self.state_by_source.entry(source.id()) → absent 时 `insert(SourceEpoch::new(source.baseline()))`；present 时 `entry.and_modify(|epoch| { let delta = source.update(); epoch.apply_delta(delta); })`
- [ ] **Step 7.4:** 重写 `check_cache_break(&self) -> CacheBreakReport`：遍历 `state_by_source.iter()`，`epoch.is_changed()` → push 到 `changed_sources`；`epoch.removed` → push 到 `removed_sources`；`system_prompt_changed = changed_sources.iter().any(|id| *id == SourceId("system-prompt"))`；`tool_schemas_changed` 同理
- [ ] **Step 7.5:** 定义 `#[derive(Debug, Clone, Default)] pub struct CacheBreakReport { pub changed_sources: Vec<SourceId>, pub removed_sources: Vec<SourceId>, pub system_prompt_changed: bool, pub tool_schemas_changed: bool }`
- [ ] **Step 7.6:** 删除旧的 `record_prompt_state` 函数与 `if state.system_hash != 0` 逻辑
- [ ] **Step 7.7:** 更新现有 CacheBreakDetector 测试以适配 `record_source` 新 API
- [ ] **Step 7.8:** 添加测试 `test_unchanged_sources_empty_report`：3 个 source 无 Changed → changed_sources 为空
- [ ] **Step 7.9:** 添加测试 `test_system_prompt_change_attributed`：仅 system prompt 变 → changed_sources 含 system-prompt，tool_schemas_changed == false
- [ ] **Step 7.10:** 添加测试 `test_removed_source_reported_separately`：source 返回 Removed → removed_sources 含之，changed_sources 不含
- [ ] **Step 7.11:** 添加测试 `test_zero_hash_no_false_positive`：hash==0 但 baseline==current → 不报 changed
- [ ] **Test:** `cargo test -p synthia-context cache::detector`
- [ ] **Commit:** `refactor(cache): rewrite CacheBreakDetector to use SourceEpoch (fixes R6)`

---

## Task 8: 统一 CacheControlMark 在 provider 层的使用（D3）

- [ ] **Step 8.1:** 读 `crates/synthia-provider/src/cache_policy.rs` 全文，理解 `apply_cache_policy` 如何注入 `CacheControl` 到 `CompletionRequest.tools`/`messages`
- [ ] **Step 8.2:** 修改 `apply_cache_policy`：从 `synthia-cache-mark` import 统一 `CacheControlMark`；确保注入的 `cache_control` 字段能携带 scope 信息（若 `CompletionRequest` 的 cache_control 字段类型是 wire `CacheControl { r#type: String }`，需扩展为 `CacheControl { r#type, namespace: Option<String> }` 或在 transform 时从 policy 读取 scope）
- [ ] **Step 8.3:** 读 `crates/synthia-provider/src/anthropic/provider/mod.rs` 的 `transform_request`，确认 cache_control JSON 构造点
- [ ] **Step 8.4:** 修改 `transform_request`：在构造 `AnthropicTool.cache_control` / `AnthropicContentBlock.cache_control` / `AnthropicSystemBlock.cache_control` 时，若 `request.cache_policy` 含 `CacheScope`，将 scope.0 纳入 cache key namespace（Anthropic API 支持在 cache_control 中通过额外字段或通过不同的 cache key 隔离）
- [ ] **Step 8.5:** 更新 `anthropic/provider/mod.rs` 的 3 处测试（L89/L111/L136）适配统一类型
- [ ] **Step 8.6:** 添加测试 `test_scope_flows_to_provider`：构造带 `CacheScope::new("alice","s1")` 的 request，transform 后的 JSON 含 namespace 派生字段
- [ ] **Test:** `cargo test -p synthia-provider`
- [ ] **Commit:** `refactor(provider): use unified CacheControlMark with scope through transform`

---

## Task 9: 接入 applyCachePolicy 到 4 处生产路径（D4 / R1）

- [ ] **Step 9.1:** 读 `crates/synthia-context/src/assembler/pipeline.rs:61` 上下文，确认 `cache_policy: None` 改为 `Some(CachePolicy::default())` 的改动点
- [ ] **Step 9.2:** 修改 `pipeline.rs:61`：`cache_policy: None` → `cache_policy: Some(CachePolicy::default())`
- [ ] **Step 9.3:** 读 `crates/synthia-context/src/service.rs:171`，同样改为 `Some(CachePolicy::default())`
- [ ] **Step 9.4:** 读 `crates/synthia-context/src/summarizer/generator.rs:143`，同样改
- [ ] **Step 9.5:** 读 `crates/synthia-agent/src/context.rs` 的 `assemble_context`，同样改
- [ ] **Step 9.6:** 确保 4 处在构造 `CacheControlMark` 时传入 `CacheScope::new(user_id, session_id)`——检查每处是否有 user_id/session_id 上下文；若无，从 session context 或 Config 获取（若 assembler 不持有 user_id，需在 CachePolicy 上额外携带 scope，或在 transform 时由 provider 注入）
- [ ] **Step 9.7:** 添加集成测试 `test_assembler_injects_default_cache_policy`：`ContextAssembler::prepare` 产出 request.cache_policy == Some(default)
- [ ] **Step 9.8:** 添加测试 `test_non_anthropic_provider_ignores_cache_policy`：mock provider supports_inline_cache_hints()==false，request byte-identical to None case
- [ ] **Test:** `cargo test -p synthia-context -p synthia-agent`
- [ ] **Commit:** `feat(context): inject Some(CachePolicy::default()) in 4 production paths (R1)`

---

## Task 10: 删除 SystemContext 死代码（D2 / R2）

- [ ] **Step 10.1:** 用 `DeleteFile` 删除 `crates/synthia-context/src/system_context.rs`
- [ ] **Step 10.2:** Grep `SystemContext|get_system_context|clear_system_context_cache` 全代码库，确认 0 匹配（应已是 0，因为死代码）
- [ ] **Step 10.3:** 运行 `cargo check --workspace` 确认无破坏
- [ ] **Test:** `cargo check --workspace`
- [ ] **Commit:** `refactor(context): remove dead SystemContext code (R2)`

---

## Task 11: 端到端验证与回归

- [ ] **Step 11.1:** 运行 `cargo check --workspace`，确认全绿
- [ ] **Step 11.2:** 运行 `cargo +nightly fmt --all`
- [ ] **Step 11.3:** 运行 `cargo clippy --all-targets --all-features --tests --all`，修复所有警告（按 rust.md 规范，不允许 dead_code/unused）
- [ ] **Step 11.4:** 运行 `cargo test --workspace`，确认全通过
- [ ] **Step 11.5:** 端到端测试：构造 ContextAssembler 产出 CompletionRequest，断言 `request.cache_policy == Some(CachePolicy::default())`
- [ ] **Step 11.6:** 端到端测试：AnthropicProvider::transform_request 序列化 JSON，断言 last tool / last user msg / system block 含 `cache_control: {type: ephemeral}`
- [ ] **Step 11.7:** 端到端测试：CacheBreakDetector::check_cache_break 在 system prompt 未变时返回 `system_prompt_changed: false`
- [ ] **Step 11.8:** 端到端测试：compute_hash 跨进程一致——用 `std::process::Command` 跑两个子进程各算一次同内容 hash，比对相等
- [ ] **Step 11.9:** 端到端测试：两个不同 user_id 的 CacheControlMark 产生不同 cache_control_hash（`CacheScope::new("alice","s1")` vs `CacheScope::new("bob","s1")`）
- [ ] **Commit:** `test: add end-to-end verification for cache prefix consistency chain`

---

## Task 12: 更新 specs 与文档对齐

- [ ] **Step 12.1:** 运行 `openspec validate system-context-source-epoch` 确认 spec 格式通过
- [ ] **Step 12.2:** 若 validate 报错（如 scenario hashtag 层级、SHALL 关键字缺失），修正对应 spec 文件
- [ ] **Step 12.3:** 确认 `openspec/changes/system-context-source-epoch/specs/` 下 4 个文件与实现一致
- [ ] **Commit:** `docs(spec): validate and align system-context-source-epoch specs`
