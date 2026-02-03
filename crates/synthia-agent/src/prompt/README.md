# Prompt 模块

系统提示词构建模块，提供模块化的提示词生成功能。

## 设计原则

1. **KV 缓存友好**: 静态内容在前，动态内容在后，支持 Prompt Caching 降低 API 成本
2. **Token 优化**: 精简冗余，仅保留必要信息，按需加载
3. **Section 组合**: 通过组合不同的 PromptSection 构建提示词

## Section 结构

### Cached（全局缓存 - KV 缓存友好）

跨会话缓存，静态内容可享受 KV Cache 加速。

```
1. IdentitySection        - Agent 身份
2. SystemSection         - 系统信息
3. TaskExecutionSection  - 任务指南 + 代码风格 + 验证
4. ToolUsageSection      - 工具优先级 + 并发 + 多工具调用
```

### SessionCached（会话缓存）

会话内缓存，会话间不共享。

```
5. SkillSection           - 技能 + 子代理指导
6. MemorySection          - 记忆文件
7. OutputStyleSection     - 输出样式
8. LanguageSection        - 语言偏好
```

### Volatile（每轮重新计算）

每次调用都重新生成。

```
9.  EnvironmentSection              - 环境信息
10. DynamicMcpInstructionsSection    - MCP 服务器指令
11. ProactiveSection                - 主动模式
12. TokenBudgetSection              - Token 预算
```

## SectionCaching

```rust
pub enum SectionCaching {
    Cached,         // 全局缓存，跨会话
    SessionCached,  // 会话缓存
    Volatile,       // 每轮重新计算
    Uncached,       // 不缓存
}
```

## 核心组件

| 组件 | 功能 |
|------|------|
| `PromptBuilder` | 提示构建器，从 sections 构建完整提示词 |
| `PromptContext` | 提示上下文，存储构建所需的所有信息 |
| `PromptSection` | Section trait，所有 section 实现此 trait |
| `PromptState` | 缓存状态管理，管理 Cached 和 SessionCached |
| `ResolvedPrompt` | 解析后的 prompt，包含静态/动态内容和 hash |
| `EffectivePromptConfig` | 优先级配置，支持 override/coordinator/agent/custom |
| `PromptLatches` | Beta Header 状态管理，sticky 模式支持 |
| `CacheBreakDetector` | KV Cache 断裂检测，用于监控缓存命中率 |
| `CacheStats` | 缓存统计 |
| `TrackedState` | 状态追踪 |
| `PromptStateSnapshot` | 快照结构 |
| `CacheBreakReport` | 缓存断裂报告 |

## Section 实现

| Section | 缓存策略 | 功能 |
|---------|----------|------|
| `IdentitySection` | Cached | Agent 身份 |
| `SystemSection` | Cached | 系统信息 |
| `TaskExecutionSection` | Cached | 任务执行指南 |
| `ToolUsageSection` | Cached | 工具使用说明 |
| `SkillSection` | SessionCached | 技能 + 会话引导 |
| `MemorySection` | SessionCached | 记忆文件 |
| `OutputStyleSection` | SessionCached | 输出样式 |
| `LanguageSection` | SessionCached | 语言偏好 |
| `EnvironmentSection` | Volatile | 环境信息 |
| `DynamicMcpInstructionsSection` | Volatile | MCP 服务器指令 |
| `ProactiveSection` | Volatile | 主动模式 |
| `TokenBudgetSection` | Volatile | Token 预算 |

## PromptContext

```rust
pub struct PromptContext<'a> {
    pub agent_name: &'a str,
    pub agent_description: &'a str,
    pub workspace_dir: &'a Path,
    pub skill_instructions: String,
    pub is_subagent: bool,
    pub session_id: Option<&'a str>,
    pub mcp_servers: &'a [McpServerInfo],
    pub additional_dirs: &'a [PathBuf],
    pub output_style: Option<&'a OutputStyleConfig>,
    pub language_preference: Option<&'a str>,
    pub is_proactive_mode: bool,
    pub model_name: Option<&'a str>,
    pub knowledge_cutoff: Option<&'a str>,
}
```

## ResolvedPrompt

解析后的 prompt 结构：

```rust
pub struct ResolvedPrompt {
    pub static_content: String,   // 静态内容（KV 缓存友好）
    pub dynamic_content: String,  // 动态内容（会话级）
    pub sections_used: Vec<String>,
    pub prefix_hash: String,      // 十六进制格式，用于缓存比对
}
```

使用 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 分隔两部分。

## EffectivePromptConfig

提示词优先级系统，支持多层叠加：

```rust
pub enum SystemPromptPriority {
    Override,    // 最高优先级，完全替换
    Coordinator, // 协调者模式
    Agent,       // Agent 特定提示词
    Custom,      // 用户自定义
    Default,     // 默认提示词
}

pub struct EffectivePromptConfig {
    pub override_prompt: Option<String>,
    pub coordinator_prompt: Option<String>,
    pub agent_prompt: Option<String>,
    pub custom_prompt: Option<String>,
    pub append_prompt: Option<String>,
    pub use_coordinator_mode: bool,
}
```

## PromptLatches

Beta Header 的 sticky 状态管理：

```rust
pub struct PromptLatches {
    afk_mode: bool,
    fast_mode: bool,
    cache_editing: bool,
    thinking_clear: bool,
    // ... latched 版本（一旦激活保持激活）
}
```

一旦激活会保持状态，直到 `/clear` 或 `/compact` 重置。

## CompactionType

压缩类型枚举：

```rust
pub enum CompactionType {
    Auto,      // 自动压缩
    Manual,    // 手动压缩
    ToolLoop,  // 工具循环压缩
}
```

### 压缩提示常量

```rust
pub const COMPACTION_SYSTEM_PROMPT: &str = /* ... */;
pub const COMPACTION_USER_PROMPT: &str = /* ... */;
pub const CONVERSATION_CONTINUATION_TEXT: &str = /* ... */;
pub const MANUAL_COMPACT_CONTINUATION_TEXT: &str = /* ... */;
pub const TOOL_LOOP_CONTINUATION_TEXT: &str = /* ... */;
```

### 压缩相关函数

| 函数 | 功能 |
|------|------|
| `render_compaction_prompt` | 渲染压缩提示 |
| `render_compaction_prompt_with_type` | 根据压缩类型渲染 |
| `format_compact_summary` | 格式化摘要（移除 `<analysis>` 标签，提取 `<summary>` 内容） |

## CacheBreakDetector

KV Cache 断裂检测器，用于监控缓存命中率：

```rust
pub struct CacheBreakDetector {
    state_by_source: HashMap<String, TrackedState>,
    max_tracked_sources: usize,
}

pub struct CacheBreakReport {
    pub reason: String,
    pub system_prompt_changed: bool,
    pub tool_schemas_changed: bool,
    pub model_changed: bool,
    pub fast_mode_changed: bool,
    pub cache_control_changed: bool,
    pub global_cache_strategy_changed: bool,
    pub betas_changed: bool,
    pub prev_cache_read_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub call_count: u64,
}
```

## 辅助函数

| 函数 | 功能 |
|------|------|
| `prepend_bullets` | 将字符串数组转换为带短划线前缀的列表 |
| `join_lines` | 将字符串数组用换行符连接 |
| `inject_workspace_file` | 注入工作区文件（支持 AGENTS.md、IDENTITY.md、USER.md、MEMORY.md 等） |

## 工作区文件注入

当 workspace 目录下存在以下文件时，会自动注入内容：

- `AGENTS.md`
- `IDENTITY.md`
- `USER.md`
- `MEMORY.md`

文件内容会被截断至 20,000 字符。

## 使用示例

```rust
use synthia_agent::prompt::{
    PromptBuilder, PromptContext, PromptState,
    EffectivePromptConfig, IdentitySection, TaskExecutionSection,
};
use std::path::PathBuf;

let ctx = PromptContext {
    agent_name: "Synthia",
    agent_description: "A general-purpose agent",
    workspace_dir: PathBuf::from("/workspace"),
    skill_instructions: String::new(),
    is_subagent: false,
    session_id: None,
    mcp_servers: &[],
    additional_dirs: &[],
    output_style: None,
    language_preference: None,
    is_proactive_mode: false,
    model_name: Some("claude-3-5-sonnet"),
    knowledge_cutoff: Some("2024-06"),
};

let mut state = PromptState::new();
let config = EffectivePromptConfig::new();

let prompt = PromptBuilder::new()
    .add_section(Box::new(IdentitySection))
    .add_section(Box::new(TaskExecutionSection))
    .add_section(Box::new(ToolUsageSection::new()))
    // ... 添加更多 sections
    .build_effective_prompt(&ctx, &mut state, config)
    .unwrap();
```

## 默认 Section 顺序

`PromptBuilder::default_with_sections()` 按以下顺序添加：

1. `IdentitySection`
2. `SystemSection`
3. `TaskExecutionSection`
4. `ToolUsageSection`
5. `EnvironmentSection`
6. `MemorySection`
7. `SkillSection`
8. `DynamicMcpInstructionsSection`
9. `OutputStyleSection`
10. `LanguageSection`
11. `ProactiveSection`
12. `TokenBudgetSection`
