# Skill Tools

技能工具模块，提供按需加载技能知识库的能力，支持延迟加载以最小化上下文使用。

## 工具列表

| 工具名称 | 功能描述 |
|----------|----------|
| `loadSkill` | 按名称加载技能 |

## 交互顺序

```
Agent → 决定需要技能 → loadSkill → 加载技能内容 → 使用技能知识
```

## 在 Agent 中的作用

1. **知识按需加载**: 不需要预先加载所有知识
2. **上下文优化**: 最小化 token 使用
3. **技能扩展**: 可扩展 Agent 能力
4. **结构化知识**: SKILL.md 格式规范

## Agent 运行机制

### 延迟加载流程

```
1. Agent 启动时
      ↓
2. 只索引技能名称和描述 (~50 tokens)
      ↓
3. Agent 决定需要某技能
      ↓
4. loadSkill 动态加载完整内容
      ↓
5. 使用技能知识完成任务
```

### 技能发现路径

Agent 会从以下位置发现技能：
- `~/.claude/skills/`
- `~/.config/agents/skills/`
- `{workspace}/.claude/skills/`
- `{workspace}/.agents/skills/`

### 技能格式

```markdown
---
name: skill-name
description: 技能描述
---

# Skill: skill-name

技能内容...
```

## 使用示例

### 加载技能

```json
{
  "name": "loadSkill",
  "arguments": {
    "name": "skill-creator"
  }
}
```

## 系统提示中的技能

在系统提示中，Agent 会看到可用技能列表：

```
You have these skills at your disposal. Use the loadSkill tool to load a skill when needed:

- skill-creator: 创建新技能
- mcp-builder: 构建 MCP 服务器
- pdf: PDF 处理
- code-review: 代码审查
```

## 设计理念

> "Load knowledge when you need it, not upfront"

Skill 系统解决的问题：
1. **上下文限制**: 不一次加载所有知识
2. **按需加载**: 只加载需要的技能
3. **技能扩展**: 易于添加新能力
4. **知识复用**: 技能可在多个会话复用

## 与 System Prompt 的区别

| 特性 | Skill | System Prompt |
|------|-------|---------------|
| 加载时机 | 按需 | 启动时 |
| 大小 | 可很大 | 受限 |
| 数量 | 无限 | 受限 |
| 更新 | 动态 | 需重启 |
