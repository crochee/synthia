# Design: p0-trait-review-remediation

## 1. 总览

3 个独立 sub-task,顺序执行 (低风险先做,高风险后做),每 sub-task 独立
commit + 独立 review。Sub-task 共享质量门 (cargo check/clippy/fmt/test)。

```
Sub-task A (Retryable)     — commit A
   ↓ cargo check/test
Sub-task B (McpClientFacade) — commit B
   ↓ cargo check/test
Sub-task C (SessionManager) — commit C
   ↓ cargo check/test/clippy/fmt
Final verify.md filled
```

**为什么不并行 3 个 sub-task**: Sub-task A/B 都是删除 dead code,无相互
依赖,理论可并行;但本 change scope 小,顺序执行 commit 可读性更好,
每个 commit 1 个独立语义,reviewer 容易 review。

## 2. Sub-task A: 删除 Retryable trait

### 2.1 现状

[crates/synthia-provider/src/retry.rs:6-14](file:///home/crochee/workspace/synthia/crates/synthia-provider/src/retry.rs#L6-L14):

```rust
pub trait Retryable {
    fn is_retryable(&self) -> bool;
}

impl Retryable for Error {
    fn is_retryable(&self) -> bool {
        self.is_retryable()  // ← 调用 Error inherent method (无递归)
    }
}
```

[crates/synthia-core/src/error.rs:218](file:///home/crochee/workspace/synthia/crates/synthia-core/src/error.rs#L218) 已有 `pub fn is_retryable(&self) -> bool` inherent method。

**关键观察**: `impl Retryable for Error` 的 `self.is_retryable()` 调用,Rust
方法解析**优先 inherent method**,所以**不会无限递归**。trait 是
**纯死包装** (dead code wrapper)。

### 2.2 修改

- **删除**: [retry.rs:6-14](file:///home/crochee/workspace/synthia/crates/synthia-provider/src/retry.rs#L6-L14) 整个 trait + impl 块 (-9 行)
- **不改**:
  - [error.rs:218](file:///home/crochee/workspace/synthia/crates/synthia-core/src/error.rs#L218) inherent method (保留)
  - [retry.rs:98, 127, 198, 213, 250, 262](file:///home/crochee/workspace/synthia/crates/synthia-provider/src/retry.rs#L98) 等所有 `e.is_retryable()` 调用 (本来就是调 inherent, trait 删除后语义不变)
  - [retry.rs:163](file:///home/crochee/workspace/synthia/crates/synthia-provider/src/retry.rs#L163) `is_retryable_error(status: u16)` 自由函数 (无关)

### 2.3 验证

- `cargo check -p synthia-provider`: 0 errors
- `cargo test -p synthia-provider`: 0 regressions
- `grep -r 'Retryable' crates/`: 0 命中
- `grep -r 'retryable' crates/synthia-provider/src/`: 只剩 `is_retryable` (inherent) + `is_retryable_error` (自由函数) + `is_rate_limited` (无关)

### 2.4 风险

**极低 (0)**:
- 0 调用方 (workspace 内 `pub trait Retryable` 0 引用)
- 即使 trait 是 `pub`,搜索确认 0 外部使用
- 行为完全不变 (inherent method 已存在)

## 3. Sub-task B: 删除 McpClientFacade 重复定义

### 3.1 现状

**定义 1**: [crates/synthia-mcp/src/types.rs:94-105](file:///home/crochee/workspace/synthia/crates/synthia-mcp/src/types.rs#L94-L105)

```rust
#[async_trait::async_trait]
pub trait McpClientFacade: Send + Sync {
    async fn list_tools(&self) -> Result<Vec<ToolSummary>, McpError>;
    async fn get_tool_schema(&self, tool_name: &str) -> Result<serde_json::Value, McpError>;
    async fn call_tool(&self, tool_name: &str, args: serde_json::Value) -> Result<String, McpError>;
}
```

**定义 2**: [crates/synthia-mcp/src/traits.rs:15-28](file:///home/crochee/workspace/synthia/crates/synthia-mcp/src/traits.rs#L15-L28)

```rust
#[async_trait]
pub trait McpClientFacade: Send + Sync {
    async fn list_tools(&self) -> Result<Vec<ToolSummary>>;
    async fn get_tool_schema(&self, name: &str) -> Result<ToolDefinition>;
    async fn call_tool(&self, name: &str, args: serde_json::Value) -> Result<ToolOutput>;
}
```

**两个都 0 impl + 0 call site**。

**模块结构**:
- `types.rs` 是 `pub mod` + 被 `pub use types::*` 重导出 (lib.rs:13, 30)
- `traits.rs` 是 `pub mod` (lib.rs:12)
- Rust 允许因为不同 module path (`synthia_mcp::types::McpClientFacade` vs `synthia_mcp::traits::McpClientFacade`)
- **不是编译错误**, 是**语义重复** (recommendations.md 原描述"编译错误"略夸张)

### 3.2 修改

- **删除**: [types.rs:93-105](file:///home/crochee/workspace/synthia/crates/synthia-mcp/src/types.rs#L93-L105) 整个 trait 块 (-13 行,含 `#[]` 前的 `McpClient` struct 引用检查)
- **删除**: [traits.rs:1-29](file:///home/crochee/workspace/synthia/crates/synthia-mcp/src/traits.rs#L1-L29) 整个文件 (因为删完 trait 后只剩 import) -15 行
- **检查**: types.rs 删后, `ToolSummary` 在 types.rs 仍被其他代码使用,不能级联删除

### 3.3 验证

- `cargo check -p synthia-mcp`: 0 errors
- `cargo test -p synthia-mcp`: 0 regressions
- `grep -r 'McpClientFacade' crates/`: 0 命中
- `ls crates/synthia-mcp/src/traits.rs`: 不存在 (若决定删整个文件)

### 3.4 风险

**极低 (0)**:
- 0 impl + 0 call site + 0 dyn
- traits.rs 整个文件可删 (该文件**只**包含此 trait + imports,无其他内容)
- types.rs 中的 trait 是独立块,删后不破坏 struct `McpClient` 定义
- 实际 MCP client 调用走 [client.rs](file:///home/crochee/workspace/synthia/crates/synthia-mcp/src/client.rs) 的具体函数,不走 trait

### 3.5 注: 备选方案 (本 change 不采用)

若未来需 facade trait, 在 `lib.rs` 顶层加 1 个清晰版本,签名对齐 `traits.rs` 版本 (使用 `ToolDefinition`/`ToolOutput` 类型),删掉 `types.rs` 版本。**本 change 不做**,因为目前 0 用户。

## 4. Sub-task C: 拆分 SessionManager (C-1 方案)

### 4.1 现状

[crates/synthia-session/src/session.rs:110-160](file:///home/crochee/workspace/synthia/crates/synthia-session/src/session.rs#L110-L160) 定义 `SessionManager` (12 方法 + 2 默认方法)。

[crates/synthia-session/src/service.rs:20-60](file:///home/crochee/workspace/synthia/crates/synthia-session/src/service.rs#L20-L60) 定义 `PersistenceService` (7 方法)。

**方法重叠**:

| SessionManager | PersistenceService | 类别 |
|----------------|---------------------|------|
| `get_session` | `load_session` | R |
| `update_session` | `save_session` | W |
| `add_message` | `append_message` | W |
| `get_conversation` | `load_messages_recent/all` | R |
| `get_recent_conversations` | — (无对应) | R |
| `get_conversation_messages` | — (无对应) | R |
| `replace_conversation` | — (无对应) | W |
| `fix_conversation` | — (无对应) | W |
| `create_session` | — (无对应) | W |
| `delete_session` | — (无对应) | W |
| `get_message_count` (default) | — | R (默认) |
| `validate_append_only` (default) | — | R (默认) |

**12 = 10 抽象方法 + 2 默认方法**。

### 4.2 目标: 拆为 SessionReader + SessionWriter

**SessionReader (6 方法, 全部读操作)**:

```rust
#[async_trait]
pub trait SessionReader: Send + Sync {
    async fn get_session(&self, config: &SessionConfig) -> Result<Option<Session>>;
    async fn get_conversation(&self, config: &SessionConfig) -> Result<Vec<Message>>;
    async fn get_recent_conversations(&self, limit: usize, mark: Option<&str>) -> Result<(Vec<Session>, Option<String>, bool)>;
    async fn get_conversation_messages(&self, session_id: &str) -> Result<Vec<Message>>;
    async fn get_message_count(&self, config: &SessionConfig) -> Result<usize>;
    async fn validate_append_only(&self, config: &SessionConfig, expected: usize) -> Result<bool>;
}
```

**SessionWriter (5 方法, 全部写操作)**:

```rust
#[async_trait]
pub trait SessionWriter: Send + Sync {
    async fn create_session(&self) -> Result<Session>;
    async fn update_session(&self, session: &Session) -> Result<()>;
    async fn delete_session(&self, config: &SessionConfig) -> Result<()>;
    async fn add_message(&self, config: &SessionConfig, message: &Message) -> Result<()>;
    async fn replace_conversation(&self, config: &SessionConfig, conversation: &[Message]) -> Result<()>;
    async fn fix_conversation(&self, config: &SessionConfig) -> Result<Vec<Message>>;
}
```

注: `get_message_count` 和 `validate_append_only` 从默认方法改为 SessionReader 的抽象方法 (因为 trait 拆后 `get_message_count` 默认实现需要 `get_conversation`,而 `get_conversation` 已在 Reader 抽象)。**或者**把它们放在 Reader 抽象,让 impl 一次提供 2 个 trait 的所有方法。

**设计选择**: **保持原语义** — Reader 的 `get_message_count` 调 `get_conversation` (都在 Reader 内,可作为 Reader 的默认方法)。这样:

```rust
#[async_trait]
pub trait SessionReader: Send + Sync {
    async fn get_session(&self, config: &SessionConfig) -> Result<Option<Session>>;
    async fn get_conversation(&self, config: &SessionConfig) -> Result<Vec<Message>>;
    // ... 其他 Reader 方法

    async fn get_message_count(&self, config: &SessionConfig) -> Result<usize> {
        let conv = self.get_conversation(config).await?;
        Ok(conv.len())
    }

    async fn validate_append_only(&self, config: &SessionConfig, expected: usize) -> Result<bool> {
        Ok(self.get_message_count(config).await? >= expected)
    }
}
```

### 4.3 修改

1. **重构 [session.rs:110-160](file:///home/crochee/workspace/synthia/crates/synthia-session/src/session.rs#L110-L160)**:
   - 删除 `pub trait SessionManager`
   - 新增 `pub trait SessionReader` (6 方法, 2 默认)
   - 新增 `pub trait SessionWriter` (6 方法, 1 移过来: fix_conversation 写入操作)
   - 总行数: +10 行 (拆 2 trait 的样板)

2. **更新 impl**:
   - 找到 `impl SessionManager for ...` 块 (1 个,推测 `Store`)
   - 拆为 `impl SessionReader for Store` + `impl SessionWriter for Store`
   - impl 块行数 +5 (复制 Send + Sync bound)

3. **更新 call site**:
   - `grep -rn 'SessionManager' crates/`: 列出所有 trait bound 引用
   - 推断 1 impl (Store) + N 个 trait bound 调用方
   - 需对每个调用方决定:
     - 仅需读 → 改 `R: SessionReader`
     - 仅需写 → 改 `R: SessionWriter`
     - 都需 → 改 `R: SessionReader + SessionWriter` (此时可考虑用 store 句柄而非泛型)
   - 这是 sub-task C 的**主要工作量**

4. **回滚 anchor**:
   - commit C 包含 1 个 sub-task 全部改动
   - 若发现 call site 影响超出预期,可单独 revert commit C,不影响 A/B

### 4.4 验证

- `cargo check --workspace`: 0 errors (call site 全部更新)
- `cargo test --workspace`: 0 regressions
- `cargo clippy --all-targets --all-features --tests --all`: 0 warnings
- `grep -rn 'SessionManager' crates/`: 0 命中 (除了 commit message 和 docs)

### 4.5 风险

**中**:
- call site 更新是主要风险,需全工作区 grep
- 推测调用方数量: 5-15 (基于 trait 使用模式),需要逐一审视
- 实施期间若发现 `SessionReader + SessionWriter` 不可行 (例如某调用方需要原子读+写),可降级为保留 SessionManager (C-4 方案)
- 验证步骤: `cargo check` 在 commit 后立即反馈,失败可快速定位

## 5. 实施顺序与检查

```
[ ] A: delete retry.rs trait lines
[ ] A: cargo check synthia-provider && cargo test synthia-provider
[ ] A: git commit -m "..."
[ ] B: delete types.rs trait + traits.rs file
[ ] B: cargo check synthia-mcp && cargo test synthia-mcp
[ ] B: git commit -m "..."
[ ] C: refactor session.rs (拆 2 trait)
[ ] C: update impl Store
[ ] C: update all call sites
[ ] C: cargo check --workspace && cargo test --workspace
[ ] C: cargo clippy --workspace && cargo fmt
[ ] C: git commit -m "..."
[ ] Final: openspec validate --strict
[ ] Final: fill verify.md
[ ] Final: openspec archive
```

## 6. 不做的事 (留痕)

- ❌ 不重新做 trait 审视 (那已由 trait-abstraction-review 完成,本 change 只实施 P0)
- ❌ 不改其他 13 个 REMOVE_CANDIDATE trait (留给下个 P0 batch)
- ❌ 不重命名 `SessionReader`/`SessionWriter` (用户已选 C-1 命名,不改)
- ❌ 不动 `archive/2026-06-15-2026-06-15-trait-abstraction-review/` 内容
- ❌ 不重新审视 `McpClient` struct (与 trait dedup 无关,保留原样)
