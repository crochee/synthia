# Exec (Command Execution) Tool

命令行执行工具模块，提供在 Agent 工作目录中执行 Shell 命令的能力。

## 工具列表

| 工具名称 | 功能描述 |
|----------|----------|
| `bash` | 执行 Shell 命令 |

## 交互顺序

```
Agent决策 → bash工具调用 → 子进程执行 → 输出返回 → Agent继续推理
```

## 在 Agent 中的作用

1. **命令执行**: 运行 `git`、`npm`、`cargo` 等命令行工具
2. **构建操作**: 执行编译、构建命令
3. **测试运行**: 执行测试命令验证代码
4. **系统操作**: 进行文件操作、进程管理等

## Agent 运行机制

### 执行流程

```
1. Agent 决定需要执行命令
      ↓
2. 构建命令字符串
      ↓
3. 调用 exec 工具
      ↓
4. 在工作目录执行子进程
      ↓
5. 捕获 stdout/stderr
      ↓
6. 返回结果给 Agent
```

### 安全机制

- 命令在工作目录内执行
- 危险命令黑名单拦截
- 执行超时保护

## 使用示例

```json
{
  "name": "bash",
  "arguments": {
    "command": "cargo build --release"
  }
}
```

## 返回格式

```
stdout: 命令标准输出
stderr: 命令错误输出（如果有）
退出码: 命令执行状态
```

## 常见使用场景

- 项目构建: `npm install`, `cargo build`
- Git 操作: `git status`, `git commit`
- 测试执行: `cargo test`, `npm test`
- 文件操作: `ls`, `rm`, `mv`
