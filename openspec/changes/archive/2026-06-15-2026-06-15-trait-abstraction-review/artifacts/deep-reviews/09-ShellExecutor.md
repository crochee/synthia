# Deep Review: `ShellExecutor`

**Location**: `crates/synthia-agent/src/shell/mod.rs:84`
**Signals**: 1 impl / 2 methods / 0 generics / 0 call sites / 0 dyn

> Note: also defined in `crates/synthia-agent/src/shell/README.md:37` (same name, different file)

## 目的
Shell 命令执行抽象,2 个方法:`execute` (同步执行) 和 `spawn` (异步派生子进程)。agent tool 的核心能力。

## 存在价值
- 1 impl: `LocalShellExecutor` (本地 shell)
- 0 dyn 引用: 调用方直接用 `LocalShellExecutor`
- 安全敏感 (sandbox 强制点是核心设计)

## 替代方案
- **A) 直接用 `LocalShellExecutor`**: 失去远程 shell/sandboxed shell 的可替换性
- **B) 保留 trait**: 2 方法不可简化
- **C) 拆 trait**: `execute` vs `spawn` 关注点不同,可拆。但当前 2 方法共存合理

## 推荐
**KEEP** (高安全 + 多 sandbox 后端需求真实)

## 理由
虽然 1 impl + 0 dyn 是"预留"模式,但 shell 执行是**安全关键路径**。未来需要:
- `DockerShellExecutor` (sandbox)
- `SshShellExecutor` (远程)
- `MockShellExecutor` (测试)

这些是 agent 安全的核心可替换点。trait 在这里**不是 YAGNI**,而是安全架构的边界。`async fn execute/spawn` 的设计也是正确的(异步 + spawn 分离)。

## 4-party 检查

- **怀疑派**: 0 dyn + 1 impl,YAGNI 警告。**REMOVE_CANDIDATE**。
- **架构派**: shell 是 sandbox 边界,安全关键。DIP 价值高。**KEEP**。
- **生产派**: Docker/sandbox 需求真实存在。**KEEP**。
- **简化派**: 当前调用方未使用 dyn,但 trait 价值在"未来需要时无需修改调用方"。**KEEP**。

**共识**: 3 派 KEEP,1 派 REMOVE。最终:**KEEP**。

### Issue: 重复定义
`ShellExecutor` 在两个文件定义:
- `crates/synthia-agent/src/shell/mod.rs:84` (实际代码)
- `crates/synthia-agent/src/shell/README.md:37` (文档示例)

README 中的 `pub trait ShellExecutor` 编译时不会被处理(markdown 不是 .rs),但**结构上重复**。建议:在 README 中用 ` ```rust ` 示例代码块时,标注"示例代码"或使用 `// example` 注释,避免 grep 误判。

### 实现建议
- **保留 trait**
- **清理 README 重复定义**:将 README 中的 trait 示例改为类型注释或函数签名片段
