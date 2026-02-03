# Context 模块

上下文管理模块，提供对话上下文管理和压缩功能，确保长会话稳定运行。

## 核心组件

| 组件 | 文件 | 功能描述 |
|------|------|----------|
| `ContextManager` | [mod.rs](mod.rs) | 上下文管理 trait |
| `DefaultContextManager` | [manager.rs](manager.rs) | ContextManager 默认实现 |
| `estimator` | [estimator.rs](estimator.rs) | Token 估算 |
| `pruning` | [pruning.rs](pruning.rs) | 剪枝策略（消息分类、工具重要性、微压缩、软剪枝、硬清除） |
| `summarizer` | [summarizer.rs](summarizer.rs) | 摘要生成与质量检查 |
| `normalize` | [normalize.rs](normalize.rs) | 对话规范化 |
| `transcript` | [transcript.rs](transcript.rs) | 转录管理 |
| `types` | [types.rs](types.rs) | 类型定义（CompactionStrategy、CompactionResult 等） |

## 模块依赖

```
context/
├── mod.rs          # ContextManager trait 和模块导出
├── types.rs        # 类型定义（CompactionStrategy、CompactionResult、SummaryQuality）
├── estimator.rs    # Token 估算（无依赖）
├── normalize.rs    # 对话规范化（无依赖）
├── transcript.rs   # 转录管理（无依赖）
├── pruning.rs      # 剪枝策略（依赖 types, config）
├── summarizer.rs   # 摘要生成（依赖 types, model_router, prompt）
├── manager.rs      # 上下文管理器（依赖所有子模块）
└── tests.rs        # 测试模块
```

## 五层渐进式压缩策略

### Level 0: None (使用率 < soft_threshold)

不执行任何压缩操作。

### Level 1: Micro Compact (soft_threshold <= 使用率 < micro_threshold)

将旧的工具结果替换为轻量级占位符，保留最近 N 个工具结果。

### Level 2: Soft Pruning (micro_threshold <= 使用率 < hard_threshold)

根据工具重要性执行软剪枝：
- **Critical**: 完全保留
- **High**: 软剪枝（保留首尾，中间截断）
- **Normal**: 硬剪枝（保留首尾，简化提示）
- **Low**: 清除（替换为占位符）

### Level 3: Hard Clearing (hard_threshold <= 使用率 < critical_threshold)

执行硬清除：清除所有非关键工具结果，保留关键工具结果。

### Level 4: Summarization (使用率 >= critical_threshold)

使用 LLM 生成对话摘要，保留用户消息和最近 N 轮完整对话。

## 主要功能

### Token 估算

```rust
use synthia_agent::context::estimate_tokens;

let tokens = estimate_tokens(&messages);
```

估算算法：基于字节数估算（4 字节 ≈ 1 token），20% 安全边距。

### 消息分类

```rust
use synthia_agent::context::{
    is_user_text_message, is_tool_use, is_tool_result,
    classify_messages, MessageClassification,
};
```

### 工具重要性剪枝

```rust
use synthia_agent::context::prune_tools_with_importance;

let pruned = prune_tools_with_importance(&messages, |name| {
    ToolImportance::High
});
```

### 微压缩

```rust
use synthia_agent::context::micro_compact;

micro_compact(&mut messages, 3); // 保留最近 3 个工具结果
```

### 修复工具对

```rust
use synthia_agent::context::fix_tool_pairs;

let fixed = fix_tool_pairs(&messages);
```

### 对话规范化

- `ensure_call_outputs_present`: 为没有结果的工具调用插入占位符
- `remove_orphan_outputs`: 移除孤立的工具结果
- `strip_images_when_unsupported`: 在不支持图片时替换为占位符

## 配置参数

```rust
pub struct ContextConfig {
    pub trigger_threshold: f64,         // 触发压缩阈值 (默认 0.8)
    pub reserved_tokens: usize,        // 预留 token 数 (默认 20000)
    pub trigger_ratio: f64,            // 触发比率 (默认 0.85)
    pub min_buffer_tokens: usize,       // 最小缓冲区 (默认 5000)
    pub soft_threshold: f64,           // 软剪枝阈值 (默认 0.5)
    pub micro_threshold: f64,          // 微压缩阈值 (默认 0.6)
    pub hard_threshold: f64,           // 硬清除阈值 (默认 0.75)
    pub critical_threshold: f64,       // 摘要生成阈值 (默认 0.9)
    pub keep_recent_turns: usize,      // 保留最近 N 轮对话
    pub quality_check_enabled: bool,   // 启用质量检查 (默认 true)
    pub summary_max_tokens: usize,     // 摘要最大 token 数 (默认 4096)
    pub tool_importance: HashMap<String, ToolImportance>,
    pub preserve_user_messages: bool, // 保留用户消息 (默认 true)
    pub preserve_critical_tools: bool,// 保留关键工具 (默认 true)
    pub micro_threshold: f64,          // 微压缩阈值 (默认 0.6)
    pub target_ratio: f64,             // 目标比率 (默认 0.5)
    pub critical_tools: Vec<String>,   // 关键工具列表 (默认: ["read", "write", "edit"])
}
```

## 测试

```bash
cargo test -p synthia-agent context:: --lib
```
