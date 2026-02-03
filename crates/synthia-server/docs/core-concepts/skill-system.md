---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 技能系统

## 1. 概述

技能系统允许为 Agent 提供专业化的能力和知识。技能是 Markdown 格式的文档，包含特定任务的指导原则、最佳实践和示例。

## 2. 技能架构

### 2.1 技能文件格式

技能文件使用 Markdown 格式，存储在 `.trae/skills/` 目录：

```
.trae/skills/
├── code-review.md       # 代码审查技能
├── test-generator.md    # 测试生成技能
└── documentation.md     # 文档生成技能
```

### 2.2 技能文件结构

```markdown
# 技能名称

## 概述
技能的简要描述和用途。

## 使用场景
- 场景1
- 场景2

## 指导原则
1. 原则1
2. 原则2

## 最佳实践
- 最佳实践1
- 最佳实践2

## 示例
提供具体的使用示例。

## 注意事项
需要注意的问题和限制。
```

## 3. 技能加载

### 3.1 配置技能

```yaml
skills:
  - name: "code-review"
    path: ".trae/skills/code-review.md"
  - name: "test-generator"
    path: ".trae/skills/test-generator.md"
```

### 3.2 按需加载

技能采用按需加载策略：

1. **首次使用**：完整指南（~2K tokens）附着在 tool_result
2. **后续使用**：关键提醒（~100 tokens）
3. **长期未用**：重新加载完整指南

### 3.3 技能注入

```rust
// 技能内容作为 tool_result 返回
let skill_content = tokio::fs::read_to_string(&skill_path).await?;
let message = SamplingMessage::assistant_text(skill_content);
yield AgentEvent::Message(message);
```

## 4. 内置技能

### 4.1 代码审查技能

```markdown
# Code Review Skill

## 概述
提供专业的代码审查能力，识别潜在问题和改进机会。

## 检查项
- 代码风格和格式
- 潜在bug和错误
- 性能问题
- 安全漏洞
- 可维护性

## 最佳实践
1. 从整体架构开始审查
2. 关注关键路径和边界条件
3. 检查错误处理
4. 验证测试覆盖
```

### 4.2 测试生成技能

```markdown
# Test Generator Skill

## 概述
自动生成单元测试和集成测试。

## 测试类型
- 单元测试
- 集成测试
- 端到端测试

## 最佳实践
1. 测试正常路径
2. 测试边界条件
3. 测试错误处理
4. 使用有意义的测试名称
```

## 5. 技能管理API

### 5.1 列出技能

```bash
curl http://localhost:8080/skills
```

### 5.2 添加技能

```bash
curl -X POST http://localhost:8080/skills \
  -H "Content-Type: application/json" \
  -d '{
    "name": "code-review",
    "path": ".trae/skills/code-review.md",
    "description": "代码审查技能"
  }'
```

### 5.3 加载技能

```bash
curl -X POST http://localhost:8080/skills/code-review/load
```

## 6. 编写技能

### 6.1 技能设计原则

1. **单一职责**：每个技能专注一个领域
2. **可组合性**：技能可以组合使用
3. **可测试性**：提供可验证的示例

### 6.2 技能示例

```markdown
# Documentation Generator

## 概述
自动生成项目文档，包括 README、API 文档和代码注释。

## 文档类型

### README 文档
- 项目介绍
- 安装指南
- 使用示例
- 配置说明

### API 文档
- 端点描述
- 请求/响应格式
- 错误码说明

### 代码注释
- 函数说明
- 参数描述
- 返回值说明

## 最佳实践
1. 使用清晰简洁的语言
2. 提供代码示例
3. 保持文档更新
4. 使用标准格式

## 示例

### README 模板
```markdown
# 项目名称

简短描述项目功能。

## 安装

\`\`\`bash
npm install package-name
\`\`\`

## 使用

\`\`\`javascript
const module = require('package-name');
module.doSomething();
\`\`\`
```

## 注意事项
- 避免过度文档化
- 保持文档与代码同步
- 使用版本控制
```

## 7. 相关文档

- [技能编写指南](../guides/skill-writing.md)
- [Agent执行流程](agent-execution.md)

## 8. 参考资料

- [Manus Skill System](https://github.com/microsoft/manus)
- [Agent-Zero Knowledge Directory](https://github.com/frdel/agent-zero)
