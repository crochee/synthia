---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 上下文管理

## 1. 概述

上下文管理是 Synthia Agent 的核心功能之一，负责管理对话历史、优化 token 使用、确保长会话稳定运行。本文档详细说明上下文结构、压缩策略、KV Cache 优化和 Token 管理。

## 2. 上下文结构

### 2.1 上下文组成

Agent 的完整上下文由以下部分组成：

```
┌─────────────────────────────────────────────────────────────┐
│                      Agent Context                           │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │              System Prompt (固定前缀)                │    │
│  │  - Agent 角色和指令                                  │    │
│  │  - 工具列表和说明                                    │    │
│  │  - 技能指南                                          │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │           Conversation History (动态部分)            │    │
│  │  - 用户消息                                          │    │
│  │  - 助手消息                                          │    │
│  │  - 工具调用                                          │    │
│  │  - 工具结果                                          │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │           Current Request (当前请求)                 │    │
│  │  - 最新用户消息                                      │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 消息类型

上下文中的消息分为以下类型：

| 类型 | 说明 | 重要性 |
|------|------|--------|
| `UserText` | 用户文本消息 | 最高（必须保留） |
| `AssistantText` | 助手文本消息 | 高 |
| `ToolUse` | 工具调用 | 中 |
| `ToolResult` | 工具结果 | 低（可压缩） |
| `Other` | 其他消息 | 低 |

### 2.3 消息分类

```rust
pub enum MessageClassification {
    UserText,        // 用户文本消息
    AssistantText,   // 助手文本消息
    ToolUse,         // 工具调用
    ToolResult,      // 工具结果
    Other,           // 其他
}

pub fn classify_messages(
    messages: &[SamplingMessage],
) -> Vec<(usize, MessageClassification)> {
    messages
        .iter()
        .enumerate()
        .map(|(idx, msg)| {
            let classification = match msg.content.iter().find_map(|c| match c {
                SamplingMessageContent::ToolUse(_) => Some(MessageClassification::ToolUse),
                SamplingMessageContent::ToolResult(_) => Some(MessageClassification::ToolResult),
                _ => None,
            }) {
                Some(classification) => classification,
                None if is_user_text_message(msg) => MessageClassification::UserText,
                None if msg.role == Role::Assistant => MessageClassification::AssistantText,
                None => MessageClassification::Other,
            };
            (idx, classification)
        })
        .collect()
}
```

## 3. KV Cache 优化

### 3.1 前缀一致性原则（P1）

**核心原则**：连续 API 调用之间，prompt 的前缀必须保持字节级一致。

**原因**：
- Anthropic Prompt Caching: 前缀匹配 → $0.30/M（缓存价）
- 前缀不匹配 → $3.00/M（全价）
- 前缀变动 → 10倍成本放大 + I/O 瓶颈

**实现要求**：

1. **System Prompt 不可变**
   - 构建完成后永远不变
   - 不放时间戳、不动态增删工具 schema
   - 技能指南附着在 tool_result 上，不改 system prompt

2. **Append-Only 策略**
   - 信息只追加到末尾
   - 不在中间插入新消息
   - 不修改已发送的 tool_result 内容

3. **确定性序列化**
   - JSON key 排序
   - 浮点格式一致
   - 空格/换行规范化

### 3.2 Append-Only 策略（P2）

**允许的操作**：
- ✓ 在末尾追加 user / assistant / tool_result 消息
- ✓ 对旧消息做"原地缩减"（Soft Trim）
- ✓ 对旧消息做"原地替换"（Hard Clear）
- ✓ 整段替换为压缩摘要

**禁止的操作**：
- ✗ 在中间插入新消息
- ✗ 重排消息顺序
- ✗ 修改已有消息的内容（Soft Trim/Hard Clear 除外）
- ✗ 删除消息后让后续消息"前移"

### 3.3 缓存命中率优化

**监控指标**：

| 指标 | 说明 | 目标值 |
|------|------|--------|
| `prefix_stability_ratio` | 连续调用间 prefix 不变的比例 | >85% |
| `cache_hit_ratio` | Prompt Caching 命中率 | >85% |

**优化策略**：

1. **稳定 System Prompt**
   ```rust
   // 不好的做法：每次都重新生成
   let system_prompt = build_system_prompt(current_time, available_tools);
   
   // 好的做法：固定前缀
   let system_prompt = STATIC_SYSTEM_PROMPT;
   ```

2. **延迟加载技能**
   ```rust
   // 不好的做法：预装所有技能
   let system_prompt = format!("{}\n\n{}", base_prompt, all_skills);
   
   // 好的做法：按需加载
   if needs_skill {
       // 技能指南作为 tool_result 返回
       yield AgentEvent::SystemNotification(skill_guide);
   }
   ```

## 4. 五层渐进式压缩策略

### 4.1 压缩策略概览

```
┌─────────────────────────────────────────────────────────────┐
│                  Progressive Compaction                      │
│                                                              │
│  使用率 = 当前 tokens / 上下文窗口限制                        │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ Level 0: None                                       │    │
│  │ 使用率 < 50% (soft_threshold)                       │    │
│  │ → 不执行任何压缩                                     │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ Level 1: Micro Compact                              │    │
│  │ 50% <= 使用率 < 60% (micro_threshold)               │    │
│  │ → 替换旧工具结果为轻量级占位符                        │    │
│  │ → 保留最近 N 个工具结果                              │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ Level 2: Soft Pruning                               │    │
│  │ 60% <= 使用率 < 75% (hard_threshold)                │    │
│  │ → 根据工具重要性执行软剪枝                           │    │
│  │ → Critical: 完全保留                                 │    │
│  │ → High: 软剪枝（保留首尾）                           │    │
│  │ → Normal: 硬剪枝（简化提示）                         │    │
│  │ → Low: 清除（替换为占位符）                          │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ Level 3: Hard Clearing                              │    │
│  │ 75% <= 使用率 < 90% (critical_threshold)            │    │
│  │ → 清除所有非关键工具结果                             │    │
│  │ → 保留关键工具结果                                   │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ Level 4: Summarization                              │    │
│  │ 使用率 >= 90% (critical_threshold)                  │    │
│  │ → 使用 LLM 生成对话摘要                              │    │
│  │ → 保留用户消息和最近 N 轮完整对话                    │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 Level 1: Micro Compact

**触发条件**：`soft_threshold (50%) <= 使用率 < micro_threshold (60%)`

**操作**：
- 将旧的工具结果替换为轻量级占位符
- 保留最近 N 个工具结果（默认：3）

**示例**：

```rust
use synthia_agent::context::micro_compact;

let mut messages = get_conversation();
micro_compact(&mut messages, 3); // 保留最近 3 个工具结果
```

**效果**：
- 损失：旧工具结果的详细内容
- 收益：前缀完全不变 → 零 cache 失效

### 4.3 Level 2: Soft Pruning

**触发条件**：`micro_threshold (60%) <= 使用率 < hard_threshold (75%)`

**操作**：根据工具重要性执行差异化剪枝

**工具重要性分级**：

| 重要性 | 说明 | 处理方式 |
|--------|------|----------|
| `Critical` | 关键工具（read, write, edit） | 完全保留 |
| `High` | 重要工具（grep, glob） | 软剪枝（保留首尾） |
| `Normal` | 普通工具 | 硬剪枝（简化提示） |
| `Low` | 低重要性工具 | 清除（替换为占位符） |

**示例**：

```rust
use synthia_agent::context::prune_tools_with_importance;
use synthia_agent::config::ToolImportance;

let pruned = prune_tools_with_importance(&messages, |name| {
    match name {
        "read" | "write" | "edit" => ToolImportance::Critical,
        "grep" | "glob" => ToolImportance::High,
        _ => ToolImportance::Normal,
    }
});
```

**软剪枝示例**：

```
原始工具结果（1000行）：
┌─────────────────────────────────┐
│ Line 1: ...                     │
│ Line 2: ...                     │
│ ...                             │
│ Line 500: ...                   │
│ ...                             │
│ Line 999: ...                   │
│ Line 1000: ...                  │
└─────────────────────────────────┘

软剪枝后：
┌─────────────────────────────────┐
│ Line 1: ...                     │
│ Line 2: ...                     │
│ ...                             │
│ [TRUNCATED: 495 lines]          │
│ ...                             │
│ Line 999: ...                   │
│ Line 1000: ...                  │
└─────────────────────────────────┘
```

### 4.4 Level 3: Hard Clearing

**触发条件**：`hard_threshold (75%) <= 使用率 < critical_threshold (90%)`

**操作**：
- 清除所有非关键工具结果
- 保留关键工具结果（read, write, edit）
- 保留用户消息

**效果**：
- 损失：大部分工具结果的详细内容
- 收益：大量释放空间

### 4.5 Level 4: Summarization

**触发条件**：`使用率 >= critical_threshold (90%)`

**操作**：
- 使用 LLM 生成对话摘要
- 保留用户消息
- 保留最近 N 轮完整对话（默认：5）

**摘要质量检查**：

```rust
pub struct SummaryQuality {
    pub has_required_sections: bool,      // 有必需章节
    pub identifier_integrity: bool,       // 标识符完整性
    pub user_request_reflected: bool,     // 用户请求被反映
    pub has_file_paths: bool,             // 有文件路径
    pub has_user_requests: bool,          // 有用户请求
    pub has_key_decisions: bool,          // 有关键决策
    pub overall_score: f64,               // 总体质量分数 (0.0 - 1.0)
}
```

**质量权重**：
- 必需章节：0.25
- 标识符完整性：0.15
- 用户请求反映：0.15
- 文件路径：0.15
- 用户请求：0.15
- 关键决策：0.15

## 5. Token 管理

### 5.1 Token 估算

**估算算法**：

```rust
pub fn estimate_tokens(messages: &[SamplingMessage]) -> usize {
    let total_bytes: usize = messages
        .iter()
        .map(|msg| {
            msg.content
                .iter()
                .map(|c| match c {
                    SamplingMessageContent::Text(text) => text.len(),
                    SamplingMessageContent::Image(_) => 1000, // 估算
                    SamplingMessageContent::ToolUse(tool_use) => {
                        serde_json::to_string(&tool_use.input)
                            .map(|s| s.len())
                            .unwrap_or(0)
                    }
                    SamplingMessageContent::ToolResult(result) => {
                        result.content.iter().map(|c| {
                            match c {
                                rmcp::model::Content::Text(text) => text.text.len(),
                                rmcp::model::Content::Image(_) => 1000,
                                rmcp::model::Content::Resource(_) => 500,
                            }
                        }).sum()
                    }
                })
                .sum()
        })
        .sum();

    // 4 字节 ≈ 1 token，加 20% 安全边距
    (total_bytes / 4) * 12 / 10
}
```

**估算精度**：
- 文本消息：±10%
- 包含图片：±20%
- 工具结果：±15%

### 5.2 Token 预算

**配置参数**：

```rust
pub struct ContextConfig {
    pub trigger_threshold: f64,         // 触发压缩阈值 (默认 0.8)
    pub reserved_tokens: usize,         // 预留 token 数 (默认 20000)
    pub trigger_ratio: f64,             // 触发比率 (默认 0.85)
    pub min_buffer_tokens: usize,       // 最小缓冲区 (默认 5000)
    pub soft_threshold: f64,            // 软剪枝阈值 (默认 0.5)
    pub micro_threshold: f64,           // 微压缩阈值 (默认 0.6)
    pub hard_threshold: f64,            // 硬清除阈值 (默认 0.75)
    pub critical_threshold: f64,        // 摘要生成阈值 (默认 0.9)
    pub keep_recent_turns: usize,       // 保留最近 N 轮对话 (默认 5)
    pub summary_max_tokens: usize,      // 摘要最大 token 数 (默认 4096)
}
```

**计算公式**：

```
有效限制 = 上下文窗口大小 - 预留 tokens
使用率 = 当前 tokens / 有效限制
```

### 5.3 Token 优化策略

**策略 1：按需加载**

```rust
// 不好的做法：预装大量信息
let system_prompt = format!(
    "{}\n\n{}\n\n{}",
    base_prompt,
    all_skills,
    all_tools
);

// 好的做法：按需加载
let system_prompt = base_prompt;
if needs_skill {
    // 技能作为 tool_result 返回
}
```

**策略 2：优先级保留**

```rust
// 保留优先级：用户消息 > 关键工具 > 普通工具
let preserved = messages
    .iter()
    .filter(|msg| {
        is_user_text_message(msg) || is_critical_tool(msg)
    })
    .collect();
```

**策略 3：智能截断**

```rust
// 截断大文件内容
if content.len() > MAX_CONTENT_SIZE {
    let truncated = format!(
        "{}\n\n[TRUNCATED: {} lines]\n\n{}",
        &content[..KEEP_HEAD_SIZE],
        content.len() - KEEP_HEAD_SIZE - KEEP_TAIL_SIZE,
        &content[content.len() - KEEP_TAIL_SIZE..]
    );
}
```

## 6. 上下文压缩流程

### 6.1 压缩决策流程

```
┌─────────────────────────────────────────────────────────────┐
│                  Compaction Decision Flow                    │
│                                                              │
│  1. 估算当前 tokens                                          │
│     │                                                        │
│     ▼                                                        │
│  2. 计算使用率 = tokens / effective_limit                    │
│     │                                                        │
│     ▼                                                        │
│  3. 判断压缩级别                                             │
│     ├── 使用率 < 50% → None                                  │
│     ├── 使用率 < 60% → Micro Compact                         │
│     ├── 使用率 < 75% → Soft Pruning                          │
│     ├── 使用率 < 90% → Hard Clearing                         │
│     └── 使用率 >= 90% → Summarization                        │
│     │                                                        │
│     ▼                                                        │
│  4. 执行压缩操作                                             │
│     │                                                        │
│     ▼                                                        │
│  5. 验证压缩结果                                             │
│     ├── 检查 token 减少                                      │
│     ├── 检查关键信息保留                                     │
│     └── 检查前缀一致性                                       │
│     │                                                        │
│     ▼                                                        │
│  6. 返回压缩结果                                             │
│     ├── CompactionResult                                     │
│     └── CompactionMetadata                                   │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 6.2 压缩结果

```rust
pub struct CompactionResult {
    pub reason: String,                    // 压缩原因
    pub messages: Vec<SamplingMessage>,    // 压缩后的消息
    pub metadata: CompactionMetadata,      // 压缩元数据
}

pub struct CompactionMetadata {
    pub original_count: usize,             // 原始消息数
    pub compacted_count: usize,            // 压缩后消息数
    pub tokens_saved: usize,               // 节省的 tokens
    pub strategy: CompactionStrategy,      // 使用的策略
    pub compacted_at: DateTime<Utc>,       // 压缩时间
    pub usage_ratio_before: f64,           // 压缩前使用率
    pub usage_ratio_after: f64,            // 压缩后使用率
}
```

### 6.3 使用示例

```rust
use synthia_agent::context::DefaultContextManager;

let context_manager = DefaultContextManager::new(model_router);

// 执行压缩
let result = context_manager.compact(&conversation).await?;

if let Some(compaction_result) = result {
    println!("压缩策略: {}", compaction_result.metadata.strategy);
    println!("节省 tokens: {}", compaction_result.metadata.tokens_saved);
    println!("使用率: {:.2}% → {:.2}%",
        compaction_result.metadata.usage_ratio_before * 100.0,
        compaction_result.metadata.usage_ratio_after * 100.0
    );
}
```

## 7. 对话规范化

### 7.1 规范化操作

**修复工具对**：

```rust
use synthia_agent::context::fix_tool_pairs;

let fixed = fix_tool_pairs(&messages);
```

**操作**：
- 为没有结果的工具调用插入占位符
- 移除孤立的工具结果

**确保调用输出存在**：

```rust
use synthia_agent::context::ensure_call_outputs_present;

ensure_call_outputs_present(&mut messages);
```

**移除孤立输出**：

```rust
use synthia_agent::context::remove_orphan_outputs;

remove_orphan_outputs(&mut messages);
```

### 7.2 图片处理

当模型不支持图片时，替换为占位符：

```rust
use synthia_agent::context::strip_images_when_unsupported;

strip_images_when_unsupported(&mut messages, model_supports_images);
```

## 8. 最佳实践

### 8.1 配置合理的阈值

```rust
let config = ContextConfig {
    soft_threshold: 0.5,           // 50% 触发微压缩
    micro_threshold: 0.6,          // 60% 触发软剪枝
    hard_threshold: 0.75,          // 75% 触发硬清除
    critical_threshold: 0.9,       // 90% 触发摘要
    keep_recent_turns: 5,          // 保留最近 5 轮
    reserved_tokens: 20000,        // 预留 20k tokens
};
```

### 8.2 监控压缩效果

```rust
// 记录压缩统计
if let Some(result) = context_manager.compact(&conversation).await? {
    metrics::counter!("compaction_total", 1);
    metrics::counter!("tokens_saved", result.metadata.tokens_saved as u64);
    metrics::gauge!("usage_ratio_after", result.metadata.usage_ratio_after);
}
```

### 8.3 保留关键信息

```rust
// 配置关键工具
let config = ContextConfig {
    critical_tools: vec![
        "read".to_string(),
        "write".to_string(),
        "edit".to_string(),
    ],
    preserve_user_messages: true,
    preserve_critical_tools: true,
};
```

### 8.4 定期检查上下文健康

```rust
let tokens = estimate_tokens(&conversation);
let limit = context_manager.effective_limit();
let ratio = tokens as f64 / limit as f64;

if ratio > 0.8 {
    tracing::warn!(
        "Context usage high: {:.2}% ({} / {} tokens)",
        ratio * 100.0,
        tokens,
        limit
    );
}
```

## 9. 故障排查

### 9.1 压缩未触发

**症状**：上下文接近限制但未压缩

**排查步骤**：
1. 检查 `trigger_threshold` 配置
2. 检查 token 估算是否准确
3. 检查压缩器是否被正确调用

### 9.2 压缩后信息丢失

**症状**：关键信息在压缩后丢失

**排查步骤**：
1. 检查工具重要性配置
2. 检查 `preserve_user_messages` 设置
3. 检查 `critical_tools` 列表

### 9.3 缓存命中率低

**症状**：Prompt Caching 命中率低于预期

**排查步骤**：
1. 检查 System Prompt 是否稳定
2. 检查是否有动态内容插入前缀
3. 检查序列化是否确定性

## 10. 相关文档

- [Agent执行流程](agent-execution.md)
- [记忆系统](memory-system.md)
- [配置说明](../configuration/CONFIGURATION.md)

## 11. 参考资料

- [Anthropic Prompt Caching](https://www.anthropic.com/news/prompt-caching)
- [DualPath: Efficient LLM Inference](https://arxiv.org/abs/2402.10534)
- [Lost in the Middle](https://arxiv.org/abs/2307.03172)
