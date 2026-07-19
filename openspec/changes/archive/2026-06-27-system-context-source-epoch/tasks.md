## 1. 前置勘察与决策确认

- [x] 1.1 读取 `crates/synthia-context/src/prefix_tracker/` 目录全部文件，确认与 `prompt/cache/detector.rs` 是否有职责重叠；若有重叠，记录合并方案到 worktree 笔记
- [x] 1.2 确认 workspace 是否已存在公共类型 crate（如 `synthia-types`）：运行 `ls crates/` 并检查 Cargo.toml workspace members；若无则决定新建 `crates/synthia-cache-mark/`
- [x] 1.3 检查 `crates/synthia-context/Cargo.toml` 确认 `ahash` 已在依赖中（`mark.rs::hash_to_u64` 使用）

## 2. 公共 crate 承载统一 CacheControlMark（D3）

- [x] 2.1 若 1.2 决定新建：创建 `crates/synthia-cache-mark/` 目录，初始化 `Cargo.toml`（package name `synthia-cache-mark`，依赖 `serde`、`ahash`），加入 workspace members
- [x] 2.2 将 `crates/synthia-context/src/prompt/mark.rs` 中的 `CacheControlMark` / `CacheScope` / `CacheTtl` 迁移到 `crates/synthia-cache-mark/src/lib.rs`（保留全部字段与 derive）
- [x] 2.3 在 `synthia-cache-mark` 中为 `CacheControlMark` 实现 `hash_to_u64(&self) -> u64`（用 `ahash::AHasher::default()`，canonical form `(ttl, scope.0, pinned)`）
- [x] 2.4 在 `synthia-context` 的 `Cargo.toml` 添加 `synthia-cache-mark` 依赖；`mark.rs` 改为 `pub use synthia_cache_mark::{CacheControlMark, CacheScope, CacheTtl};`（保持对外 re-export 兼容）
- [x] 2.5 在 `synthia-provider` 的 `Cargo.toml` 添加 `synthia-cache-mark` 依赖
- [x] 2.6 删除 `crates/synthia-provider/src/cache_policy.rs` 中本地定义的 `CacheControlMark { ttl_seconds }` 类型
- [x] 2.7 运行 `cargo check -p synthia-cache-mark -p synthia-context -p synthia-provider` 确认编译通过
- [x] 2.8 运行 `cargo +nightly fmt --all` && `cargo clippy --all-targets --all-features --tests --all` 修复所有警告

## 3. 修复确定性 hash（D6 / R5）

- [x] 3.1 修改 `crates/synthia-context/src/prompt/cache/types.rs` 中 `compute_hash`：`std::collections::hash_map::DefaultHasher::new()` → `ahash::AHasher::default()`
- [x] 3.2 添加单元测试：同一内容在两次独立 `compute_hash` 调用中产生相同 hash（验证确定性）
- [x] 3.3 运行 `cargo test -p synthia-context` 确认现有测试与新增测试通过

## 4. 引入 Source trait 与基础类型（D5）

- [x] 4.1 新建 `crates/synthia-context/src/source.rs`（或 `source/mod.rs`），定义 `trait Source { fn id(&self) -> SourceId; fn baseline(&self) -> SourceContent; fn update(&mut self) -> SourceDelta; }` with `Send + Sync` bound
- [x] 4.2 定义 `SourceId(pub &'static str)` newtype with `Eq + Hash + Clone + Debug`
- [x] 4.3 定义 `SourceContent(pub Vec<u8>)` newtype with `hash(&self) -> u64` using `ahash::AHasher::default()`
- [x] 4.4 定义 `enum SourceDelta { Changed(SourceContent), Unchanged, Removed }` with `Clone, Debug`
- [x] 4.5 在 `crates/synthia-context/src/lib.rs` 添加 `pub mod source;`（或 `pub mod source;` 声明）
- [x] 4.6 添加单元测试：`SourceContent::hash` 确定性、`SourceDelta` variant 构造
- [x] 4.7 运行 `cargo check -p synthia-context` && `cargo test -p synthia-context`

## 5. 实现 SourceEpoch（D5）

- [x] 5.1 在 `crates/synthia-context/src/source.rs`（或 `source/epoch.rs`）定义 `struct SourceEpoch { baseline_hash: u64, current_hash: u64, content: SourceContent, removed: bool }`
- [x] 5.2 实现 `SourceEpoch::new(content: SourceContent) -> Self`（baseline_hash == current_hash == content.hash()）
- [x] 5.3 实现 `SourceEpoch::is_changed(&self) -> bool`（`baseline_hash != current_hash && !removed`）
- [x] 5.4 实现 `SourceEpoch::apply_delta(&mut self, delta: SourceDelta)`（Changed 更新 current_hash+content；Unchanged 不动；Removed 置 removed=true）
- [x] 5.5 添加单元测试覆盖：new 构造 is_changed=false、Changed delta 翻转 is_changed=true、Unchanged 保留状态、Removed 标记、零 hash 不误报
- [x] 5.6 运行 `cargo test -p synthia-context`

## 6. 实现 3 个 Source 实现（D5）

- [x] 6.1 新建 `crates/synthia-context/src/source/system_prompt.rs`：`SystemPromptSource` 持 `text: String` + `prev_hash: u64`；`id()` 返回 `SourceId("system-prompt")`；`baseline()` 从 text 构造 SourceContent；`update()` 比对当前 text hash 与 prev_hash 返回 Changed/Unchanged
- [x] 6.2 新建 `crates/synthia-context/src/source/tool_schemas.rs`：`ToolSchemasSource` 持 canonical JSON bytes；`id()` 返回 `SourceId("tool-schemas")`；`new(tools: &[ToolDefinition])` 按 name 排序后 `serde_json` 序列化
- [x] 6.3 新建 `crates/synthia-context/src/source/skill_list.rs`：`SkillListSource` 持排序+join 后的 skill_ids；`id()` 返回 `SourceId("skill-list")`；`update()` 初始实现恒返回 `Unchanged`
- [x] 6.4 添加单元测试：SystemPromptSource 相同 text→Unchanged、不同 text→Changed；ToolSchemasSource 重排→相同 hash、增删→不同 hash；SkillListSource→Unchanged
- [x] 6.5 运行 `cargo test -p synthia-context`

## 7. 改造 CacheBreakDetector 为 SourceEpoch（D5/D7 / R6）

- [x] 7.1 修改 `crates/synthia-context/src/prompt/cache/detector.rs`：`state_by_source: HashMap<String, TrackedState>` → `HashMap<SourceId, SourceEpoch>`
- [x] 7.2 实现 `CacheBreakDetector::record_source(&mut self, source: &dyn Source)`：lookup id，absent→insert new epoch，present→update() + apply_delta()
- [x] 7.3 重写 `check_cache_break(&self) -> CacheBreakReport`：遍历 `state_by_source`，`epoch.is_changed()` → 加入 `changed_sources`；`epoch.removed` → 加入 `removed_sources`；派生 `system_prompt_changed` / `tool_schemas_changed` 布尔
- [x] 7.4 定义/更新 `CacheBreakReport` 结构：`changed_sources: Vec<SourceId>`, `removed_sources: Vec<SourceId>`, `system_prompt_changed: bool`, `tool_schemas_changed: bool`
- [x] 7.5 删除旧的 `record_prompt_state` / 破损的 `if hash != 0` 逻辑
- [x] 7.6 更新现有 CacheBreakDetector 测试以适配新 API；添加测试：未变→empty report、system prompt 变→changed_sources 含 system-prompt、Removed→removed_sources、零 hash 不误报
- [x] 7.7 运行 `cargo test -p synthia-context`

## 8. 统一 CacheControlMark 在 provider 层的使用（D3）

- [x] 8.1 修改 `crates/synthia-provider/src/cache_policy.rs`：`apply_cache_policy` 操作的 `CacheControl` 字段类型改为引用统一 `CacheControlMark`（从 `synthia-cache-mark` import）
- [x] 8.2 在 `CompletionRequest` 的 `tools`/`messages` 上注入 `cache_control` 时，携带从 `CacheControlMark.scope` 派生的 namespace 信息（若 CompletionRequest 的 cache_control 字段是 wire 类型，确保 scope 能映射到 provider 缓存 key）
- [x] 8.3 修改 `AnthropicProvider::transform_request`：从 `request.cache_policy` 读取统一 `CacheControlMark`（含 scope），在构造 `cache_control` JSON 时纳入 namespace 派生字段
- [x] 8.4 更新 `crates/synthia-provider/src/anthropic/provider/mod.rs` 中的 3 处测试（L89/L111/L136）以适配统一类型
- [x] 8.5 运行 `cargo test -p synthia-provider`

## 9. 接入 applyCachePolicy 到 4 处生产路径（D4 / R1）

- [x] 9.1 修改 `crates/synthia-context/src/assembler/pipeline.rs:61`：`cache_policy: None` → `cache_policy: Some(CachePolicy::default())`
- [x] 9.2 修改 `crates/synthia-context/src/service.rs:171`：同上
- [x] 9.3 修改 `crates/synthia-context/src/summarizer/generator.rs:143`：同上
- [x] 9.4 修改 `crates/synthia-agent/src/context.rs` 的 `assemble_context`：同上
- [x] 9.5 确保 4 处注入点在构造 `CacheControlMark` 时传入 `CacheScope::new(user_id, session_id)`（若该路径已有 user_id/session_id 上下文；若无，需从 session context 获取）
- [x] 9.6 添加端到端集成测试：`ContextAssembler::prepare` 产出的 request.cache_policy == Some(default)；非 Anthropic provider 路径 byte-identical to None
- [x] 9.7 运行 `cargo test -p synthia-context -p synthia-agent`

## 10. 删除 SystemContext 死代码（D2 / R2）

- [x] 10.1 删除 `crates/synthia-context/src/system_context.rs`
- [x] 10.2 全代码库 grep 确认无 `SystemContext` / `get_system_context` / `clear_system_context_cache` 引用（应已是 0）
- [x] 10.3 运行 `cargo check --workspace` 确认无破坏

## 11. 端到端验证与回归

- [x] 11.1 运行 `cargo check --workspace` 全绿
- [x] 11.2 运行 `cargo +nightly fmt --all`
- [x] 11.3 运行 `cargo clippy --all-targets --all-features --tests --all` 零警告（按 rust.md 规范）
- [x] 11.4 运行 `cargo test --workspace` 全通过
- [x] 11.5 端到端验证：构造 ContextAssembler 产出 CompletionRequest，断言 cache_policy == Some(CachePolicy::default())
- [x] 11.6 端到端验证：AnthropicProvider::transform_request 序列化出的 JSON 在 last tool / last user msg / system block 含 `cache_control: {type: ephemeral}`
- [x] 11.7 端到端验证：CacheBreakDetector::check_cache_break 在 system prompt 未变时返回 system_prompt_changed: false（修 R6）
- [x] 11.8 端到端验证：compute_hash 在同一内容上跨子进程调用一致（可用两个 cargo test 进程比对，或用 std::process::Command 跑两次）
- [x] 11.9 端到端验证：两个不同 user_id 的 CacheControlMark 产生不同 cache_control_hash（跨用户隔离）

## 12. 更新 specs 与文档对齐

- [x] 12.1 确认 `openspec/changes/system-context-source-epoch/specs/` 下 4 个 spec 文件与实现一致（本 change 的 delta specs）
- [x] 12.2 运行 `openspec validate system-context-source-epoch` 确认 spec 格式通过
- [x] 12.3 不主动创建/修改任何 README 或外部文档（除非用户明确要求）
