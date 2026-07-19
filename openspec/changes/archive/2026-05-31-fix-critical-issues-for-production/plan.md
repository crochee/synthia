# Synthia 生产级修复实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复所有 P0 严重问题，实现 0 lint，让 CLI 和 Server 可实际运行

**Architecture:** 按模块依次修复：MCP → Memory → Plugin → Execution。优先修复阻塞性问题，确保每次修改后运行 clippy 验证。

**Tech Stack:** Rust, tokio, sqlx, axum

---

## Task 1: MCP 心跳机制

**Files:**
- Modify: `crates/synthia-mcp/src/connection.rs`
- Create: `crates/synthia-mcp/src/heartbeat.rs` (新模块)

- [ ] **Step 1: 在 McpConnection 中添加 last_ping_sent 字段**

```rust
// crates/synthia-mcp/src/connection.rs
// 在 McpConnection 结构体中添加:
pub last_ping_sent: AtomicU64,
```

- [ ] **Step 2: 添加 heartbeat 方法**

```rust
// McpConnection impl 块中添加:
pub async fn start_heartbeat(&self, interval_secs: u64) {
    let interval = Duration::from_secs(interval_secs);
    loop {
        tokio::time::sleep(interval).await;
        if self.state == ConnectionState::Connected {
            let last = self.last_ping_sent.load(Ordering::SeqCst);
            let now = Utc::now().timestamp() as u64;
            if now - last >= interval_secs {
                if let Err(e) = self.send_ping().await {
                    tracing::error!(error = %e, "Ping failed");
                    self.state = ConnectionState::Error;
                    break;
                }
            }
        }
    }
}
```

- [ ] **Step 3: 添加 send_ping 和 handle_pong**

```rust
async fn send_ping(&self) -> Result<(), McpError> {
    let now = Utc::now().timestamp() as u64;
    self.last_ping_sent.store(now, Ordering::SeqCst);
    let request = crate::jsonrpc::JsonRpcRequest::ping();
    // ... send request
}
```

- [ ] **Step 4: 验证编译通过**

```bash
cargo check -p synthia-mcp 2>&1
```

- [ ] **Step 5: 提交**

```bash
git add crates/synthia-mcp/src/connection.rs crates/synthia-mcp/src/heartbeat.rs
git commit -m "feat(mcp): add heartbeat mechanism"
```

---

## Task 2: SSE 传输重写

**Files:**
- Modify: `crates/synthia-mcp/src/sse_transport.rs`

- [ ] **Step 1: 阅读当前 sse_transport.rs 实现 (line 1-151)**

```bash
cat -n crates/synthia-mcp/src/sse_transport.rs
```

- [ ] **Step 2: 删除 DuplexStream 模拟逻辑，添加真实 HTTP SSE 客户端**

```rust
// 替换 SseTransport::new 中的 duplex 创建逻辑
// 使用 reqwest 流式读取代替:
let response = self.client.get(&self.sse_url).send().await?;
let mut stream = response.bytes_stream();
while let Some(chunk) = stream.next().await {
    // 解析 SSE 事件
}
```

- [ ] **Step 3: 确保 POST 端发送 JSON-RPC**

```rust
// 在 stdin_writer 实现中:
async fn write_message(&mut self, msg: &str) -> Result<(), std::io::Error> {
    self.client
        .post(&self.post_url)
        .header("Content-Type", "application/json")
        .body(msg.to_string())
        .send()
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(())
}
```

- [ ] **Step 4: 验证编译通过**

```bash
cargo check -p synthia-mcp 2>&1
```

- [ ] **Step 5: 提交**

```bash
git add crates/synthia-mcp/src/sse_transport.rs
git commit -m "fix(mcp): rewrite SSE transport with real HTTP streams"
```

---

## Task 3: Memory 冷查询优化

**Files:**
- Modify: `crates/synthia-memory/src/cold/sqlite.rs:497-506`

- [ ] **Step 1: 定位 search_with_mode 方法 (line 497)**

```rust
// 当前代码:
pub async fn search_with_mode(&self, query: &str, limit: usize, mode: RetrievalMode) -> Result<Vec<crate::traits::SearchResult>, synthia_core::Error> {
    let all_entries = self.load_all_entries().await?;  // 问题在这
    crate::retrieval::retrieve(&self.pool, &all_entries, query, limit, mode).await
}
```

- [ ] **Step 2: 重写为 SQL 推送查询**

```rust
pub async fn search_with_mode(&self, query: &str, limit: usize, mode: RetrievalMode) -> Result<Vec<crate::traits::SearchResult>, synthia_core::Error> {
    // 使用 LIKE 进行基础搜索，更高级的 BM25 可后续优化
    let pattern = format!("%{}%", query.to_lowercase());
    let rows: Vec<(String, String, String, String, f64, i64)> = sqlx::query_as(
        r#"
        SELECT f.entry_id, f.content, m.metadata, m.created_at, m.importance_score, m.access_count
        FROM cold_entries_fts f
        JOIN cold_entries_meta m ON m.entry_id = f.entry_id
        WHERE f.content LIKE ?
        ORDER BY m.importance_score DESC
        LIMIT ?
        "#
    )
    .bind(&pattern)
    .bind(limit as i64)
    .fetch_all(&self.pool)
    .await
    .map_err(|e| synthia_core::Error::Io(std::io::Error::other(format!(
        "Failed to search cold entries: {}",
        e
    ))))?;

    let entries: Vec<ColdEntry> = rows
        .into_iter()
        .map(|(id, content, metadata, created_at_str, importance_score, access_count)| {
            let parsed_metadata: serde_json::Value =
                serde_json::from_str(&metadata).unwrap_or(serde_json::Value::Null);
            ColdEntry {
                id,
                content,
                metadata: parsed_metadata,
                created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                importance_score,
                access_count: access_count as u64,
                ..Default::default()
            }
        })
        .collect();

    Ok(entries.into_iter().map(|e| crate::traits::SearchResult { entry: e, score: 1.0 }).collect())
}
```

- [ ] **Step 3: 修复 created_at 时间戳问题**

```rust
// 在 load_all_entries 方法中 (line 523):
// 修改这一行:
|(_, _, _, _created_at, importance_score, access_count)| {
// 改为:
|(_, _, _, created_at_str, importance_score, access_count)| {
// 并且在里面使用:
created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
    .map(|dt| dt.with_timezone(&chrono::Utc))
    .unwrap_or_else(|_| chrono::Utc::now()),
```

- [ ] **Step 4: 验证 clippy 通过**

```bash
cargo clippy -p synthia-memory -- -D warnings 2>&1
```

- [ ] **Step 5: 提交**

```bash
git add crates/synthia-memory/src/cold/sqlite.rs
git commit -m "fix(memory): push query filtering to SQL, preserve created_at"
```

---

## Task 4: Plugin 错误处理

**Files:**
- Modify: `crates/synthia-plugin/src/hook_runner.rs:336`

- [ ] **Step 1: 定位 panic! 位置 (line 336)**

```rust
match handle.await {
    Ok(Ok(output)) => output,
    Ok(Err(e)) => panic!("Command failed: {e}"),  // 这行
    Err(_) => panic!("Task panicked"),            // 这行
}
```

- [ ] **Step 2: 替换为错误传播**

```rust
match handle.await {
    Ok(Ok(output)) => output,
    Ok(Err(e)) => return Err(HookRunnerError::ExecutionFailed(format!(
        "Command failed: {e}"
    ))),
    Err(_) => return Err(HookRunnerError::Timeout(timeout_secs)),
}
```

- [ ] **Step 3: 验证 clippy 通过**

```bash
cargo clippy -p synthia-plugin -- -D warnings 2>&1
```

- [ ] **Step 4: 提交**

```bash
git add crates/synthia-plugin/src/hook_runner.rs
git commit -m "fix(plugin): replace panic with error propagation"
```

---

## Task 5: Execution 沙箱修复

**Files:**
- Modify: `crates/synthia-command/src/registry.rs` (或沙箱检测逻辑所在文件)
- Modify: `crates/synthia-session/src/session.rs`

- [ ] **Step 1: 查找沙箱检测逻辑**

```bash
grep -rn "curl" crates/synthia-command/src/ --include="*.rs" | head -20
```

- [ ] **Step 2: 添加混淆模式检测**

```rust
// 在沙箱检测正则中添加:
let dangerous_patterns = [
    r"curl\s*https?",
    r"curlhttps://",      // 新增
    r"hxxps?://",         // 新增
    r"wget\s*https?",
    r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}",  // IP 地址
];

// 使用 case-insensitive 检测
let cmd_lower = cmd.to_lowercase();
for pattern in &dangerous_patterns {
    if Regex::new(&format!("(?i){}", pattern)).unwrap().is_match(cmd) {
        return Err(CommandError::SandboxViolation);
    }
}
```

- [ ] **Step 3: 添加白名单机制**

```rust
pub struct SandboxConfig {
    pub allowed_domains: Vec<String>,
}

// 在检测前先检查白名单
for domain in &config.allowed_domains {
    if cmd.contains(domain) {
        return Ok(());  // 白名单跳过检测
    }
}
```

- [ ] **Step 4: 修复僵尸进程 (查找 get_child 或进程管理代码)**

```bash
grep -rn "wait" crates/synthia-session/src/ --include="*.rs" | head -10
```

- [ ] **Step 5: 确保使用 wait() 而非 detach()**

```rust
// 如果使用 detach，改为:
let status = child.wait().await.map_err(|e| SessionError::ProcessWait(e.to_string()))?;
```

- [ ] **Step 6: 验证 clippy 通过**

```bash
cargo clippy -p synthia-command -p synthia-session -- -D warnings 2>&1
```

- [ ] **Step 7: 提交**

```bash
git add crates/synthia-command/src/ crates/synthia-session/src/
git commit -m "fix(execution): add obfuscation detection, fix zombie processes"
```

---

## Task 6: Examples 构建修复

**Files:**
- Modify: `examples/plugin_example.rs`

- [ ] **Step 1: 检查 Cargo.toml 中的依赖**

```bash
grep -A5 "\[dependencies\]" examples/plugin_example.rs | head -20
# 或者查看 examples/plugin_example.rs 需要的依赖
```

- [ ] **Step 2: 修复依赖声明**

```toml
# Cargo.toml 中添加:
[dependencies]
synthia-plugin = { path = "crates/synthia-plugin" }
tracing = "0.1"
tracing-subscriber = "0.3"
```

- [ ] **Step 3: 验证编译**

```bash
cargo check -p synthia-examples 2>&1
```

- [ ] **Step 4: 提交**

```bash
git add examples/plugin_example.rs
git commit -m "fix(examples): add missing dependencies to plugin_example"
```

---

## Task 7: 最终验证

- [ ] **Step 1: 运行完整 clippy 检查**

```bash
cargo clippy --all-targets -- -D warnings 2>&1
```

- [ ] **Step 2: 运行测试**

```bash
cargo test 2>&1 | tail -50
```

- [ ] **Step 3: 验证 CLI 启动 (如果适用)**

```bash
cargo run --bin synthia-cli -- --help 2>&1 | head -20
```

- [ ] **Step 4: 最终提交**

```bash
git add -A
git commit -m "fix: resolve all P0 issues for production deployment"
```

---

## 执行选项

**1. Subagent-Driven (推荐)** - 我为每个任务启动独立 subagent，任务间审查，快速迭代

**2. Inline Execution** - 在当前 session 中按批次执行，带检查点

请选择执行方式。