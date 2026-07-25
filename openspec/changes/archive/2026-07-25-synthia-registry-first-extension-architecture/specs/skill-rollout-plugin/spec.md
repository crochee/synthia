# Spec: skill-rollout-plugin

## ADDED Requirements

### Requirement: Skill 系统 — SHALL manage skill registration and activation

技能 SHALL 作为提示模板 + 工具组合 + 上下文片段的预定义包。

- **R1.1**: `Skill` trait 包含 `name() -> &str`、`description() -> &str`、`instructions() -> String`、`tools() -> Vec<ToolName>`、`detect_invocation(&str) -> bool`、`provenance() -> SkillProvenance`
- **R1.2**: `SkillProvenance` 枚举包含 `Builtin`、`User`、`Plugin(String)` 三个变体
- **R1.3**: `SkillRegistry` 管理一组 `Arc<dyn Skill>`
- **R1.4**: `SkillRegistry::register()`、`unregister()`、`list()`、`get()` 方法
- **R1.5**: `SkillRegistry::detect_skills(input: &str) -> Vec<Arc<dyn Skill>>` 从用户输入中检测应激活的技能
- **R1.6**: 技能激活时，其 `instructions()` 注入到 system prompt（通过 SkillsFragment）
- **R1.7**: 技能激活时，其 `tools()` 声明需要的工具（确保工具已注册）

#### Scenario: skill activation from user input
- **WHEN** user input matches `detect_invocation()` for a registered skill
- **THEN** the skill's instructions are injected into the system prompt and its declared tools are ensured registered

### Requirement: 内建 Skills — SHALL provide default coding, search, and debug skills

系统 SHALL 提供默认的 CodingSkill、SearchSkill 和 DebugSkill。
- **R2.2**: `SearchSkill` — 代码搜索技能（需要 grep、find 等工具）
- **R2.3**: `DebugSkill` — 调试技能（需要 bash、read_file 等工具）
- **R2.4**: 技能定义支持从 Markdown frontmatter 文件加载（借鉴 pi-mono）

#### Scenario: built-in skills available by default
- **WHEN** SkillRegistry is initialized
- **THEN** CodingSkill, SearchSkill, and DebugSkill are registered and detectable from user input

### Requirement: RolloutTracker — SHALL track file changes and token budget

`RolloutTracker` SHALL 追踪文件变更版本控制和 token 预算。

- **R3.1**: `RolloutTracker` 追踪 `Vec<FileChange>` 和 `TokenBudget`
- **R3.2**: `FileChange` 包含 `path: PathBuf`、`change_type: ChangeType`、`timestamp: Instant`、`content_hash: String`
- **R3.3**: `ChangeType` 枚举包含 `Created`、`Modified`、`Deleted`
- **R3.4**: `TokenBudget` 包含 `total: u32`、`used: AtomicU32`、`remaining: AtomicU32`
- **R3.5**: `RolloutTracker::record_change()` 记录文件变更
- **R3.6**: `RolloutTracker::record_token_usage()` 记录 token 消耗
- **R3.7**: `RolloutTracker::summary()` 返回变更摘要（文件数、变更类型分布、token 使用率）
- **R3.8**: `RolloutTracker` 通过 `RolloutBudgetFragment` 向 LLM 提示当前变更预算

#### Scenario: rollout tracking and budget reporting
- **WHEN** file changes and token usage are recorded via RolloutTracker
- **THEN** `summary()` returns accurate change counts and token usage, and `RolloutBudgetFragment` exposes the budget to the LLM

### Requirement: PluginRegistry — SHALL dynamically discover and load third-party plugins

`PluginRegistry` SHALL 动态发现和加载第三方扩展包。

- **R4.1**: `Plugin` trait 包含 `name() -> &str`、`version() -> &str`、`capabilities() -> PluginCapabilitySummary`、`tools() -> Vec<Arc<dyn ToolProvider>>`、`skills() -> Vec<Arc<dyn Skill>>`、`fragments() -> Vec<Arc<dyn ContextFragment>>`
- **R4.2**: `PluginCapabilitySummary` 描述插件提供的工具、技能、片段数量
- **R4.3**: `PluginRegistry` 管理一组 `Arc<dyn Plugin>`
- **R4.4**: `PluginRegistry::load(plugin)` 加载插件，注册其提供的 tools、skills、fragments
- **R4.5**: `PluginRegistry::unload(name)` 卸载插件，清理所有注册的 tools、skills、fragments
- **R4.6**: 插件加载返回 `RegistrationScope`，卸载时 Scope Drop 自动清理
- **R4.7**: `PluginRegistry::discover(dir: &Path)` 从指定目录发现插件（文件系统扫描）

#### Scenario: plugin load and unload with scope
- **WHEN** a plugin is loaded via `PluginRegistry::load()`
- **THEN** its tools, skills, and fragments are registered; when unloaded or when the `RegistrationScope` drops, all registrations are cleaned up
