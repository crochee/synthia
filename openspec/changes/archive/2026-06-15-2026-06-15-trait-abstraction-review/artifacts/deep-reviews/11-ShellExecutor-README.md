# Deep Review: `ShellExecutor` (README.md duplicate)

**Location**: `crates/synthia-agent/src/shell/README.md:37`
**Signals**: 1 impl / 2 methods / 0 generics / 0 call sites / 0 dyn

> Note: 重复定义,实际代码在 `crates/synthia-agent/src/shell/mod.rs:84`。

## 目的
README 文档中作为示例展示的 `ShellExecutor` trait 块。不是真实代码。

## 存在价值
- 0 真实 impl (markdown 不编译)
- 0 dyn 引用
- 仅文档价值

## 替代方案
- 在 README 中保留 trait 描述,但**不**用 `pub trait ... { ... }` 完整语法
- 改用 ` ```text ` 或 ` ```rust,no_run ` 块

## 推荐
**REMOVE_CANDIDATE** (清理 README 重复定义)

## 理由
**这是 grep 信号污染源** — 让脚本误以为有第二个 trait 需要追踪。清理后可让 inventory 准确反映真实 trait 数 (54 → 53 在 .rs 中)。这是**研究产物**而非代码 bug。

## 4-party 检查

- **怀疑派**: 文档中重复定义 trait 名,无任何价值。**REMOVE_CANDIDATE**。
- **架构派**: 文档应说明,不应编译时元数据。**REMOVE_CANDIDATE**。
- **生产派**: 不影响生产,但污染 grep。**REMOVE_CANDIDATE**。
- **简化派**: README 用伪代码即可。**REMOVE_CANDIDATE**。

**共识**: 4 派一致 (4-0) — **REMOVE_CANDIDATE**。

### 实现建议
将 README 中:
```rust
pub trait ShellExecutor: Send + Sync {
    async fn execute(&self, cmd: ShellCommand) -> Result<ShellOutput>;
    async fn spawn(&self, cmd: ShellCommand) -> Result<ChildHandle>;
}
```
改为描述性文本:
```
`ShellExecutor` 提供两个异步方法:
- `execute(cmd)` 同步执行命令
- `spawn(cmd)` 派生子进程
```
