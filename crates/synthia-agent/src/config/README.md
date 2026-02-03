# Config 模块

配置管理模块，提供 Agent 所有配置的统一定义和管理。

## 核心组件

| 组件 | 文件 | 功能描述 |
|------|------|----------|
| `AgentConfig` | [agent.rs](agent.rs) | Agent 主配置 |
| `SessionConfig` | [session.rs](session.rs) | 会话配置 |
| `ContextConfig` | [context.rs](context.rs) | 上下文管理配置 |
| `ToolConfig` | [tool.rs](tool.rs) | 工具配置 |
| `ToolImportance` | [context.rs](context.rs) | 工具重要性级别 |
| `classify_tool_default` | [context.rs](context.rs) | 默认工具分类函数 |

## 模块导出

```rust
pub use agent::AgentConfig;
pub use context::{ContextConfig, ToolImportance, classify_tool_default};
pub use session::SessionConfig;
pub use tool::ToolConfig;
```

## AgentConfig

```rust
pub struct AgentConfig {
    pub name: String,
    pub models: Vec<ModelConfig>,
    pub description: String,
    pub allowed_tools: Vec<String>,
    pub denied_tools: Vec<String>,
    pub hidden: bool,
    pub workspace_dir: PathBuf,
    pub is_subagent: bool,
    pub guardian: GuardianConfig,
}
```

## SessionConfig

```rust
pub struct SessionConfig {
    pub id: String,
    pub parent_id: Option<String>,
    pub max_steps: u32,
    pub backoff: ExponentialBackoff,
    pub max_context_tokens: Option<usize>,
}
```

## ContextConfig

```rust
pub struct ContextConfig {
    pub trigger_threshold: f64,        // 触发压缩阈值 (默认: 0.8)
    pub reserved_tokens: usize,         // 预留 token 数 (默认: 20000)
    pub trigger_ratio: f64,            // 触发比率 (默认: 0.85)
    pub min_buffer_tokens: usize,       // 最小缓冲区 (默认: 5000)
    pub soft_threshold: f64,           // 软剪枝阈值 (默认: 0.5)
    pub hard_threshold: f64,           // 硬清除阈值 (默认: 0.75)
    pub critical_threshold: f64,       // 紧急截断阈值 (默认: 0.9)
    pub keep_recent_turns: usize,       // 保留最近轮次 (默认: 3)
    pub quality_check_enabled: bool,   // 启用质量检查 (默认: true)
    pub summary_max_tokens: usize,     // 摘要最大 token 数 (默认: 4096)
    pub tool_importance: HashMap<String, ToolImportance>,
    pub preserve_user_messages: bool,  // 保留用户消息 (默认: true)
    pub preserve_critical_tools: bool, // 保留关键工具 (默认: true)
    pub micro_threshold: f64,          // 微压缩阈值 (默认: 0.6)
    pub target_ratio: f64,             // 目标比率 (默认: 0.5)
    pub critical_tools: Vec<String>,   // 关键工具列表
}
```

## ToolImportance 枚举

```rust
pub enum ToolImportance {
    Critical,  // 关键工具 (skill, system, config)
    High,      // 高重要性 (read, write, edit)
    Normal,    // 普通 (search, grep, find)
    Low,       // 低优先级
}
```

## ToolConfig

```rust
pub struct ToolConfig {
    pub notification_interval_secs: u64,  // 通知间隔 (默认: 30)
    pub max_notifications: usize,        // 最大通知数 (默认: 3)
    pub max_concurrent_tools: usize,     // 最大并发工具数 (默认: 5)
}
```

## 使用示例

```rust
use synthia_agent::config::{AgentConfig, SessionConfig, ContextConfig, ToolConfig};

let agent_config = AgentConfig::default();
let session_config = SessionConfig::new("session-id");
let context_config = ContextConfig::default();
let tool_config = ToolConfig::default();
```
