---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 技能编写指南

## 1. 概述

技能是 Synthia Agent 的专业知识模块，通过 Markdown 文档形式提供特定领域的指导原则、最佳实践和示例。本指南说明如何编写高质量的技能文件。

## 2. 技能文件结构

### 2.1 基本结构

```markdown
# 技能名称

简短的技能描述（1-2句话）。

## 使用场景

描述技能适用的场景。

## 核心原则

列出技能的核心原则。

## 操作指南

详细的操作步骤和指导。

## 最佳实践

经过验证的最佳实践。

## 示例

具体的代码或使用示例。

## 注意事项

需要注意的问题和限制。

## 相关资源

相关文档和资源链接。
```

### 2.2 文件位置

技能文件存放在项目的 `.trae/skills/` 目录：

```
project/
└── .trae/
    └── skills/
        ├── code-review.md
        ├── test-generator.md
        ├── documentation.md
        └── security-audit.md
```

## 3. 技能编写原则

### 3.1 单一职责

每个技能应该专注于一个特定领域：

```markdown
# 好的示例：专注单一领域
# Code Review Skill

本技能专注于代码审查，识别潜在问题和改进机会。

# 不好的示例：过于宽泛
# Development Skill

本技能涵盖代码编写、测试、部署等所有开发活动。
```

### 3.2 可操作性

提供具体的操作指导而非抽象概念：

```markdown
# 好的示例：具体指导
## 检查项

1. **命名规范**
   - 变量名使用 snake_case
   - 常量名使用 SCREAMING_SNAKE_CASE
   - 函数名使用动词开头

2. **错误处理**
   - 所有公共函数必须有错误处理
   - 使用 `?` 操作符传播错误
   - 避免使用 `unwrap()` 和 `expect()`

# 不好的示例：抽象概念
## 代码质量

确保代码质量良好，遵循最佳实践。
```

### 3.3 可验证性

提供可验证的标准和示例：

```markdown
## 代码风格检查

### 函数长度
- 单个函数不超过 50 行
- 超过 30 行考虑拆分

### 示例

好的示例：
\`\`\`rust
fn calculate_total(items: &[Item]) -> Result<f64, Error> {
    items
        .iter()
        .map(|item| item.price * item.quantity)
        .sum::<f64>()
}
\`\`\`

不好的示例：
\`\`\`rust
fn process(data: &Data) -> Result<Output, Error> {
    // 100+ 行代码...
}
\`\`\`
```

## 4. 技能内容编写

### 4.1 使用场景

清晰描述技能的适用场景：

```markdown
## 使用场景

本技能适用于以下场景：

1. **代码审查**
   - Pull Request 审查
   - 代码质量检查
   - 安全漏洞扫描

2. **重构指导**
   - 识别代码异味
   - 提出重构建议
   - 评估重构风险

3. **知识传递**
   - 团队代码规范培训
   - 新成员代码指导
```

### 4.2 核心原则

列出技能的核心原则：

```markdown
## 核心原则

### 1. 安全优先
- 不执行危险操作（如 `rm -rf`）
- 验证所有输入参数
- 使用最小权限原则

### 2. 可读性优先
- 代码应该自解释
- 避免过度优化
- 使用有意义的命名

### 3. 测试驱动
- 先写测试，后写代码
- 保持测试覆盖率 > 80%
- 测试应该快速可靠
```

### 4.3 操作指南

提供详细的操作步骤：

```markdown
## 操作指南

### 步骤 1：理解代码上下文

1. 阅读相关文档
2. 理解业务需求
3. 识别依赖关系

### 步骤 2：检查代码结构

1. 检查文件组织
2. 检查模块划分
3. 检查依赖关系

### 步骤 3：审查代码质量

1. 检查命名规范
2. 检查代码风格
3. 检查错误处理
4. 检查性能问题

### 步骤 4：提供反馈

1. 列出发现的问题
2. 提供改进建议
3. 给出代码示例
```

### 4.4 最佳实践

提供经过验证的最佳实践：

```markdown
## 最佳实践

### 错误处理

**使用 Result 类型**

\`\`\`rust
// 好的做法
fn read_config(path: &Path) -> Result<Config, Error> {
    let content = fs::read_to_string(path)?;
    let config: Config = toml::from_str(&content)?;
    Ok(config)
}

// 不好的做法
fn read_config(path: &Path) -> Config {
    let content = fs::read_to_string(path).unwrap();
    toml::from_str(&content).unwrap()
}
\`\`\`

**提供上下文信息**

\`\`\`rust
// 好的做法
.map_err(|e| Error::Config(format!("Failed to parse {}: {}", path.display(), e)))?;

// 不好的做法
.map_err(|e| Error::Config(e.to_string()))?;
\`\`\`
```

### 4.5 示例

提供具体的代码示例：

```markdown
## 示例

### 完整的代码审查示例

**原始代码：**

\`\`\`rust
fn process(data: &str) -> String {
    let mut result = String::new();
    for line in data.lines() {
        if line.contains("error") {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}
\`\`\`

**审查意见：**

1. **命名问题**：`process` 过于泛化，建议改为 `extract_error_lines`
2. **性能问题**：使用 `String::new()` 和 `push_str` 效率较低
3. **错误处理**：缺少对空输入的处理

**改进后代码：**

\`\`\`rust
fn extract_error_lines(data: &str) -> String {
    if data.is_empty() {
        return String::new();
    }
    
    data.lines()
        .filter(|line| line.contains("error"))
        .collect::<Vec<_>>()
        .join("\n")
}
\`\`\`
```

### 4.6 注意事项

列出需要注意的问题：

```markdown
## 注意事项

### 安全相关

1. **不要审查敏感信息**
   - 密码、API 密钥
   - 个人身份信息
   - 商业机密

2. **避免危险建议**
   - 不要建议使用 `unwrap()` 处理关键路径
   - 不要建议禁用安全检查
   - 不要建议使用不安全的依赖

### 性能相关

1. **避免过早优化**
   - 先保证正确性
   - 后考虑性能
   - 基于测量优化

2. **考虑上下文**
   - 不同场景有不同标准
   - 权衡可读性和性能
   - 考虑维护成本
```

## 5. 技能模板

### 5.1 代码审查技能模板

```markdown
# Code Review Skill

提供专业的代码审查能力，识别潜在问题和改进机会。

## 使用场景

- Pull Request 审查
- 代码质量检查
- 重构建议

## 检查清单

### 代码风格
- [ ] 命名规范
- [ ] 代码格式
- [ ] 注释质量

### 逻辑正确性
- [ ] 边界条件处理
- [ ] 错误处理
- [ ] 并发安全

### 性能
- [ ] 算法效率
- [ ] 内存使用
- [ ] I/O 优化

### 安全
- [ ] 输入验证
- [ ] 权限检查
- [ ] 敏感信息处理

## 最佳实践

[具体实践内容]

## 示例

[代码示例]
```

### 5.2 测试生成技能模板

```markdown
# Test Generator Skill

自动生成单元测试和集成测试。

## 使用场景

- 单元测试生成
- 集成测试生成
- 测试覆盖率提升

## 测试类型

### 单元测试
- 测试单个函数或方法
- 使用 mock 隔离依赖
- 快速执行

### 集成测试
- 测试模块间交互
- 使用真实依赖
- 验证端到端流程

## 测试原则

1. **AAA 模式**
   - Arrange: 准备测试数据
   - Act: 执行被测代码
   - Assert: 验证结果

2. **FIRST 原则**
   - Fast: 快速执行
   - Independent: 独立运行
   - Repeatable: 可重复
   - Self-validating: 自验证
   - Timely: 及时编写

## 测试模板

\`\`\`rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_function_name_scenario() {
        // Arrange
        let input = "test_input";
        let expected = "expected_output";
        
        // Act
        let result = function_name(input);
        
        // Assert
        assert_eq!(result, expected);
    }
}
\`\`\`
```

### 5.3 文档生成技能模板

```markdown
# Documentation Generator Skill

自动生成项目文档。

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

### 代码文档
- 模块说明
- 函数文档
- 类型文档

## 文档原则

1. **清晰简洁**
   - 使用简单语言
   - 避免技术术语
   - 提供示例

2. **结构化**
   - 使用标题层级
   - 使用列表和表格
   - 使用代码块

3. **可维护**
   - 保持更新
   - 版本控制
   - 自动生成

## 文档模板

\`\`\`markdown
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

## API

### function_name(param)

描述函数功能。

**参数：**
- `param` (Type): 参数描述

**返回：**
- Type: 返回值描述

**示例：**
\`\`\`javascript
const result = function_name('value');
\`\`\`
\`\`\`
```

## 6. 技能配置

### 6.1 配置文件

在 `config.yaml` 中配置技能：

```yaml
skills:
  - name: "code-review"
    path: ".trae/skills/code-review.md"
  
  - name: "test-generator"
    path: ".trae/skills/test-generator.md"
  
  - name: "documentation"
    path: ".trae/skills/documentation.md"
```

### 6.2 技能加载

技能采用按需加载策略：

1. **首次使用**：完整指南（~2K tokens）作为 tool_result 返回
2. **后续使用**：关键提醒（~100 tokens）
3. **长期未用**：重新加载完整指南

## 7. 技能测试

### 7.1 手动测试

```bash
# 加载技能
curl -X POST http://localhost:8080/skills/code-review/load

# 查看技能内容
curl http://localhost:8080/skills/code-review
```

### 7.2 集成测试

```rust
#[tokio::test]
async fn test_skill_loading() {
    let skill_tool = SkillTool::new(PathBuf::from(".trae/skills"));
    
    let content = skill_tool.load_skill("code-review").await.unwrap();
    
    assert!(content.contains("Code Review"));
    assert!(content.contains("最佳实践"));
}
```

## 8. 最佳实践总结

### 8.1 编写原则

1. **专注单一领域**：每个技能只覆盖一个专业领域
2. **提供具体指导**：给出可操作的具体步骤
3. **包含代码示例**：用代码展示最佳实践
4. **保持简洁**：避免冗长，突出重点

### 8.2 维护原则

1. **定期更新**：跟随技术发展更新技能
2. **收集反馈**：根据使用反馈改进技能
3. **版本控制**：使用 Git 管理技能文件
4. **文档化**：记录技能的变更历史

### 8.3 组织原则

1. **合理分类**：按领域组织技能文件
2. **命名规范**：使用清晰、一致的命名
3. **避免重复**：合并相似技能
4. **模块化**：将大型技能拆分为多个小技能

## 9. 相关文档

- [技能系统](../core-concepts/skill-system.md)
- [工具开发指南](tool-development.md)

## 10. 参考资料

- [Manus Skill System](https://github.com/microsoft/manus)
- [Agent-Zero Knowledge Directory](https://github.com/frdel/agent-zero)
- [OpenClaw Skill Lazy Loading](https://github.com/openclaw/agent)
