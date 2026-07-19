# Deep Review: `AuditWriter`

**Location**: `crates/synthia-agent/src/audit.rs:17`
**Signals**: 1 impl / 1 methods / 0 generics / 0 call sites / 0 dyn

## 目的
审计条目写入器抽象,允许后端可扩展 (文件、远程日志、stdout)。1 个方法 `write(&mut self, entry: &AuditEntry)`。

## 存在价值
- 当前 1 impl: `FileAuditWriter` (写文件)
- 0 dyn 引用: 调度方直接用 `FileAuditWriter`
- doc: "enabling backend extensibility"

## 替代方案
- **A) 直接用 `FileAuditWriter` 具体类型**: 失去后端可替换性
- **B) 保留 trait**: 1 方法粒度已最小,无法简化
- **C) 拆 trait**: 1 个方法无法拆

## 推荐
**REMOVE_CANDIDATE** (移除 trait, 直接用 `FileAuditWriter`; 当出现第二 impl 时再引入)

## 理由
1 impl + 0 dyn 是典型的"预留但不必要"模式。YAGNI 原则支持移除 — 未来要加 `RemoteAuditWriter` 时,再引入 trait 是 30 秒工作量,但保留"可能永远用不到"的抽象会让 API 表面持续扩大。`mut self` 已经是单写者语义,trait 不增加价值。**这是 silent 1-impl trait 的真实 YAGNI 例子**。

## 4-party 检查

- **怀疑派**: 1 impl + 0 dyn = 纯预留抽象,YAGNI。**REMOVE_CANDIDATE**。
- **架构派**: trait 是"接口稳定性"投资。但当调用方都没有 dyn 时,接口稳定性无意义。**REMOVE_CANDIDATE**。
- **生产派**: 当前生产仅需 1 后端,移除不影响。**REMOVE_CANDIDATE**。
- **简化派**: 直接 `FileAuditWriter` 更简单。**REMOVE_CANDIDATE**。

**共识**: 4 派一致 (4-0) — **REMOVE_CANDIDATE**。

### 实现建议
```rust
// 替换为:
pub struct FileAuditWriter { path: PathBuf }
impl FileAuditWriter {
    pub async fn write(&mut self, entry: &AuditEntry) -> Result<(), Error> { ... }
}
// 当需要第二后端时,反向提取 trait (重构成本低,且此时 trait 是"必需"而非"预留")
```
