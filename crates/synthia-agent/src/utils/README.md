# Utils 模块

工具函数模块，提供 Agent 所需的通用工具函数。

## 核心组件

| 组件 | 功能描述 |
|------|----------|
| `conversation_fix` | 对话修复工具 |
| `message` | 消息处理工具 |

## conversation_fix 模块

```rust
pub use conversation_fix::fix_conversation;
```

提供对话修复功能，包括去重、文本合并、空白清理、工具调用修复等。

### 主要函数

| 函数 | 功能 |
|------|------|
| `fix_conversation` | 对话修复主入口，按顺序执行所有修复操作 |

## message 子模块

```rust
pub use message::{
    content_to_string,
    create_tool_message,
    extract_response_text,
    extract_text,
    extract_text_content,
    extract_text_from_result,
    extract_text_parts,
    extract_tool_uses,
    find_recent_text_message,
    sampling_content_to_string,
};
```

### 主要函数

| 函数 | 功能 |
|------|------|
| `content_to_string` | 将 Content 转换为字符串 |
| `create_tool_message` | 创建工具消息 |
| `extract_response_text` | 从工具响应提取文本 |
| `extract_text` | 从消息提取文本（仅处理 Single content） |
| `extract_text_content` | 提取文本内容 |
| `extract_text_from_result` | 从结果提取文本 |
| `extract_text_parts` | 提取文本部分 |
| `extract_tool_uses` | 提取工具调用 |
| `find_recent_text_message` | 查找最近指定角色的文本消息 |
| `sampling_content_to_string` | 将采样内容转换为字符串 |

## 使用示例

```rust
use synthia_agent::utils::{fix_conversation, extract_text_content};
use rmcp::model::SamplingMessage;

let (fixed, issues) = fix_conversation(messages);

for msg in &fixed {
    let text = extract_text_content(msg);
    if !text.is_empty() {
        println!("Message: {}", text);
    }
}
```
