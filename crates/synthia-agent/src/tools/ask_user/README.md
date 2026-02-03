# Ask User Tools

用户交互工具模块，提供向用户提问和获取响应的能力，实现人机协作。

## 工具列表

| 工具名称 | 功能描述 |
|----------|----------|
| `AskUserQuestion` | 向用户提问 |

## 交互顺序

```
Agent → AskUserQuestion → 用户响应 → Agent处理 → 继续执行
```

## 在 Agent 中的作用

1. **信息确认**: 确认理解是否正确
2. **选择请求**: 请求用户做选择
3. **澄清需求**: 澄清模糊需求
4. **权限获取**: 请求执行敏感操作

## Agent 运行机制

### 提问流程

```
1. Agent 需要用户输入
      ↓
2. 构建 QuestionRequest
      ↓
3. 暂停 Agent 执行
      ↓
4. 用户响应
      ↓
5. 返回响应给 Agent
      ↓
6. Agent 继续执行
```

### 问题类型

- **single_select**: 单选
- **multi_select**: 多选
- **text**: 文本输入

### 问题结构

```rust
struct QuestionRequest {
    header: String,           // 问题标题
    question: String,         // 问题内容
    options: Vec<QuestionOption>, // 选项
    multi_select: bool,      // 是否多选
}
```

## 使用示例

### 单选问题

```json
{
  "name": "AskUserQuestion",
  "arguments": {
    "header": "确认功能",
    "question": "你希望使用哪种实现方式?",
    "options": [
      {"label": "方案A", "description": "简单实现"},
      {"label": "方案B", "description": "高性能实现"}
    ],
    "multi_select": false
  }
}
```

### 多选问题

```json
{
  "name": "AskUserQuestion",
  "arguments": {
    "header": "选择特性",
    "question": "需要哪些功能?",
    "options": [
      {"label": "用户认证", "description": "登录注册"},
      {"label": "数据分析", "description": "统计报表"}
    ],
    "multi_select": true
  }
}
```

## 设计理念

> 人机协作是关键

AskUserQuestion 解决的问题：
1. **不确定性**: Agent 不确定时询问用户
2. **选择权**: 让用户做重要决定
3. **信息获取**: 获取 Agent 没有的信息
4. **权限控制**: 敏感操作需要确认

## 典型场景

- 功能确认
- 方案选择
- 错误处理
- 权限请求
- 信息补充
