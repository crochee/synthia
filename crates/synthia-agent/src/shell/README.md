# Shell 模块

Shell 执行抽象层，为 Agent 提供统一的命令执行能力。

## 核心组件

| 组件 | 文件 | 功能描述 |
|------|------|----------|
| `ShellExecutor` | [mod.rs](mod.rs) | Shell 执行器 trait |
| `LocalShellExecutor` | [local.rs](local.rs) | 本地进程执行实现 |
| `ShellCommand` | [mod.rs](mod.rs) | Shell 命令结构 |
| `ShellOutput` | [mod.rs](mod.rs) | 命令输出结构 |
| `ChildHandle` | [mod.rs](mod.rs) | 子进程句柄 |

## 架构

```
┌─────────────────┐
│   Tool Layer    │  (exec, background_start, etc.)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ ShellExecutor   │  Trait for shell execution
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│LocalShellExecutor│  Local process execution
└─────────────────┘
```

## ShellExecutor Trait

```rust
#[async_trait]
pub trait ShellExecutor: Send + Sync {
    async fn execute(&self, cmd: ShellCommand) -> Result<ShellOutput>;
    async fn spawn(&self, cmd: ShellCommand) -> Result<ChildHandle>;
}
```

## ShellCommand

```rust
pub struct ShellCommand {
    pub command: String,      // 要执行的命令
    pub cwd: PathBuf,         // 工作目录
    pub timeout: Option<Duration>,  // 超时时间
}
```

## 使用示例

```rust
use synthia_agent::shell::{LocalShellExecutor, ShellCommand, ShellExecutor};
use std::path::PathBuf;
use std::time::Duration;

let executor = LocalShellExecutor::new();
let cmd = ShellCommand::new(
    "ls -la".to_string(),
    PathBuf::from("/tmp")
).with_timeout(Duration::from_secs(30));

let output = executor.execute(cmd).await?;
println!("Exit code: {}", output.exit_code);
println!("Stdout: {}", output.stdout_text());
```

## 特性

- **跨平台**: 自动检测 Windows (PowerShell) 和 Unix (bash) 环境
- **超时控制**: 支持命令执行超时设置
- **输出限制**: 自动限制输出行数 (MAX_OUTPUT_LINES = 100)
- **流式处理**: 支持异步读取 stdout 和 stderr
