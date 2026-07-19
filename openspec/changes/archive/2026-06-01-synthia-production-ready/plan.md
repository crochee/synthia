# Synthia Production-Ready Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade synthia-agent from ~55% to production-ready by adding tool timeout control, progressive context degradation, cron system bridge, memory search, observability, and error recovery.

**Architecture:** Six new modules layered on existing ReAct loop without modifying core agent logic. Tool execution gets timeout/retry/truncate wrappers at step.rs call level. Context management adds Soft Trim → Hard Clear → pruning as compaction predecessors. Cron system bridges existing time-wheel scheduler to agent execution via three modes. Memory system adds JSONL event log with desensitization and ripgrep-based search. All modules expose metrics via Prometheus.

**Tech Stack:** Rust 2024 edition, tokio (async runtime), tokio::time (timeout), tokio-util (CancellationToken), sha2 (SHA-256 for prefix_hash), serde/serde_json (JSONL), prometheus-client (metrics), ripgrep CLI (memory_search)

---

### Task 1.1: Fix rmcp library API incompatibility

**Files:**
- Modify: `crates/synthia-agent/src/mcp_bridge.rs` (lines with `.client()` and `.get()` calls)

- [ ] **Step 1: Identify all rmcp API errors**

Run: `cargo check -p synthia-agent 2>&1 | grep "rmcp\|client()\|Annotated"`
Expected: ~10 errors related to `client()` method and `Annotated<RawContent>` `.get()`

- [ ] **Step 2: Fix client() method calls**

Replace `bridge.client()` calls with the correct API. Check the `rmcp` crate version in Cargo.toml and use the actual available methods. Likely needs `bridge.as_client()` or direct method access.

```rust
// Before (incorrect):
let response = bridge.client().call_tool(...).await?;

// After (check rmcp docs for correct API):
let response = bridge.call_tool(...).await?;
```

- [ ] **Step 3: Fix Annotated<RawContent> .get() calls**

Replace `.get()` with the correct accessor. `Annotated<T>` likely wraps the content differently.

```rust
// Before (incorrect):
let content = annotated.get();

// After (check rmcp types):
let content = annotated.inner() // or annotated.0 or annotated.content
```

- [ ] **Step 4: Verify rmcp errors are resolved**

Run: `cargo check -p synthia-agent 2>&1 | grep -c "error"`
Expected: 0 rmcp-related errors

---

### Task 1.2: Fix agent_runtime.rs type mismatch

**Files:**
- Modify: `crates/synthia-agent/src/agent_runtime.rs` (line with `Ok(Ok(()))`)

- [ ] **Step 1: Locate the type mismatch**

Run: `cargo check -p synthia-agent 2>&1 | grep "Ok(Ok"`
Expected: Type mismatch error showing `Result<Result<(), Error>, _>` vs `Result<(), Error>`

- [ ] **Step 2: Fix the nested Ok**

```rust
// Before (incorrect):
Ok(Ok(()))

// After (correct - single Result layer):
Ok(())
```

- [ ] **Step 3: Verify fix**

Run: `cargo check -p synthia-agent 2>&1 | grep "error\[E"`
Expected: 0 errors

---

### Task 1.3: Verify workspace compiles

**Files:**
- All workspace crates

- [ ] **Step 1: Run full workspace check**

Run: `cargo check --workspace --all-targets`
Expected: 0 errors

- [ ] **Step 2: Run clippy to check for warnings**

Run: `cargo clippy --workspace --all-targets 2>&1 | grep "warning" | head -20`
Expected: Only pre-existing warnings, no new ones from our changes

- [ ] **Step 3: Commit the fixes**

```bash
git add -A
git commit -m "fix: resolve rmcp API incompatibility and type mismatches"
```

---

### Task 2.1: Create tool executor config

**Files:**
- Create: `crates/synthia-agent/src/tool_executor/config.rs`
- Modify: `crates/synthia-agent/src/tool_executor/mod.rs` (module declaration)

- [ ] **Step 1: Define tool category enum**

```rust
// crates/synthia-agent/src/tool_executor/config.rs
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolCategory {
    FileReadWrite,
    ShellExecute,
    NetworkFetch,
    Search,
    Subagent,
    MemorySearch,
    CronOperation,
    Other,
}

impl ToolCategory {
    pub fn from_tool_name(name: &str) -> Self {
        match name {
            "read_file" | "write_file" | "read_multiple_files" => Self::FileReadWrite,
            "bash" | "shell" | "exec" => Self::ShellExecute,
            "web_fetch" | "fetch" | "http" => Self::NetworkFetch,
            "grep" | "ripgrep" | "glob" | "list_directory" => Self::Search,
            "subagent" | "agent" | "team" => Self::Subagent,
            "memory_search" => Self::MemorySearch,
            "cron_add" | "cron_list" | "cron_remove" | "cron_pause" | "cron_resume" => Self::CronOperation,
            _ => Self::Other,
        }
    }
}
```

- [ ] **Step 2: Define timeout and retry config**

```rust
// Add to config.rs
pub const MAX_SHELL_TIMEOUT_SECS: u64 = 600;
pub const DEFAULT_SHELL_TIMEOUT_SECS: u64 = 60;
pub const TRUNCATE_THRESHOLD_BYTES: usize = 16 * 1024; // 16KB
pub const EVENT_LOG_OUTPUT_LIMIT_BYTES: usize = 10 * 1024; // 10KB

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTimeoutConfig {
    pub default_timeout: Duration,
    pub max_timeout: Duration,
    pub max_retries: u32,
}

impl ToolTimeoutConfig {
    pub fn for_category(category: ToolCategory) -> Self {
        match category {
            ToolCategory::FileReadWrite => Self {
                default_timeout: Duration::from_secs(5),
                max_timeout: Duration::from_secs(30),
                max_retries: 0,
            },
            ToolCategory::ShellExecute => Self {
                default_timeout: Duration::from_secs(DEFAULT_SHELL_TIMEOUT_SECS),
                max_timeout: Duration::from_secs(MAX_SHELL_TIMEOUT_SECS),
                max_retries: 0,
            },
            ToolCategory::NetworkFetch => Self {
                default_timeout: Duration::from_secs(30),
                max_timeout: Duration::from_secs(120),
                max_retries: 2,
            },
            ToolCategory::Search => Self {
                default_timeout: Duration::from_secs(15),
                max_timeout: Duration::from_secs(60),
                max_retries: 0,
            },
            ToolCategory::Subagent => Self {
                default_timeout: Duration::from_secs(300), // 5 minutes
                max_timeout: Duration::from_secs(600),
                max_retries: 0,
            },
            ToolCategory::MemorySearch => Self {
                default_timeout: Duration::from_secs(10),
                max_timeout: Duration::from_secs(30),
                max_retries: 1,
            },
            ToolCategory::CronOperation => Self {
                default_timeout: Duration::from_secs(5),
                max_timeout: Duration::from_secs(15),
                max_retries: 0,
            },
            ToolCategory::Other => Self {
                default_timeout: Duration::from_secs(30),
                max_timeout: Duration::from_secs(120),
                max_retries: 0,
            },
        }
    }

    pub fn clamp(&self, requested: Duration) -> Duration {
        requested.min(self.max_timeout)
    }

    pub fn is_retryable(&self) -> bool {
        self.max_retries > 0
    }
}
```

- [ ] **Step 3: Add module declaration**

```rust
// In crates/synthia-agent/src/tool_executor/mod.rs (new file)
pub mod config;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p synthia-agent`
Expected: 0 errors

---

### Task 2.2: Create tool executor timeout wrapper

**Files:**
- Create: `crates/synthia-agent/src/tool_executor/timeout.rs`
- Modify: `crates/synthia-agent/src/tool_executor/mod.rs` (add module)

- [ ] **Step 1: Define execution result types**

```rust
// crates/synthia-agent/src/tool_executor/timeout.rs
use std::future::Future;
use std::time::Duration;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use super::config::ToolTimeoutConfig;

#[derive(Debug)]
pub enum ExecutionError {
    Timeout(Duration),
    Cancelled,
    ToolError(String),
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout(d) => write!(f, "Tool execution timed out after {:?}", d),
            Self::Cancelled => write!(f, "Tool execution was cancelled"),
            Self::ToolError(msg) => write!(f, "Tool error: {}", msg),
        }
    }
}
```

- [ ] **Step 2: Implement timeout wrapper with cancellation**

```rust
// Add to timeout.rs
pub async fn execute_with_timeout<F, T>(
    future: F,
    config: &ToolTimeoutConfig,
    cancel_token: CancellationToken,
) -> Result<T, ExecutionError>
where
    F: Future<Output = Result<T, String>>,
{
    tokio::select! {
        _ = cancel_token.cancelled() => {
            Err(ExecutionError::Cancelled)
        }
        result = timeout(config.default_timeout, future) => {
            match result {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(e)) => Err(ExecutionError::ToolError(e)),
                Err(_) => Err(ExecutionError::Timeout(config.default_timeout)),
            }
        }
    }
}
```

- [ ] **Step 3: Implement retry wrapper**

```rust
// Add to timeout.rs
pub async fn execute_with_retry<F, T, Fut>(
    mut future_factory: impl FnMut() -> Fut,
    config: &ToolTimeoutConfig,
    cancel_token: CancellationToken,
) -> Result<T, ExecutionError>
where
    Fut: Future<Output = Result<T, String>>,
{
    let mut attempt = 0;
    let max_attempts = config.max_retries + 1;
    let mut backoff_ms = 1000u64; // Start with 1s

    loop {
        attempt += 1;
        let result = execute_with_timeout(
            future_factory(),
            &ToolTimeoutConfig {
                default_timeout: config.default_timeout / std::cmp::max(max_attempts - attempt as u32, 1),
                ..config.clone()
            },
            cancel_token.clone(),
        ).await;

        match result {
            Ok(value) => return Ok(value),
            Err(e) => {
                if attempt >= max_attempts || !config.is_retryable() {
                    return Err(e);
                }
                // Exponential backoff: 1s, 3s
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = backoff_ms.saturating_mul(3).min(10_000);
            }
        }
    }
}
```

- [ ] **Step 4: Update mod.rs**

```rust
// crates/synthia-agent/src/tool_executor/mod.rs
pub mod config;
pub mod timeout;
```

- [ ] **Step 5: Write unit tests**

```rust
// crates/synthia-agent/src/tool_executor/timeout.rs (at bottom)
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_within_timeout() {
        let config = ToolTimeoutConfig {
            default_timeout: Duration::from_millis(100),
            max_timeout: Duration::from_secs(1),
            max_retries: 0,
        };
        let cancel = CancellationToken::new();
        let result = execute_with_timeout(
            async { Ok::<_, String>("success") },
            &config,
            cancel,
        ).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_timeout_triggers() {
        let config = ToolTimeoutConfig {
            default_timeout: Duration::from_millis(10),
            max_timeout: Duration::from_secs(1),
            max_retries: 0,
        };
        let cancel = CancellationToken::new();
        let result = execute_with_timeout(
            async {
                tokio::time::sleep(Duration::from_secs(10)).await;
                Ok::<_, String>("should not reach")
            },
            &config,
            cancel,
        ).await;
        assert!(matches!(result, Err(ExecutionError::Timeout(_))));
    }

    #[tokio::test]
    async fn test_cancellation() {
        let config = ToolTimeoutConfig {
            default_timeout: Duration::from_secs(10),
            max_timeout: Duration::from_secs(30),
            max_retries: 0,
        };
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            cancel_clone.cancel();
        });
        let result = execute_with_timeout(
            async {
                tokio::time::sleep(Duration::from_secs(10)).await;
                Ok::<_, String>("should not reach")
            },
            &config,
            cancel,
        ).await;
        assert!(matches!(result, Err(ExecutionError::Cancelled)));
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p synthia-agent tool_executor::timeout --lib`
Expected: 3 tests pass

---

### Task 2.3: Create truncate module

**Files:**
- Create: `crates/synthia-agent/src/tool_executor/truncate.rs`

- [ ] **Step 1: Implement truncate function**

```rust
// crates/synthia-agent/src/tool_executor/truncate.rs
use super::config::{TRUNCATE_THRESHOLD_BYTES, EVENT_LOG_OUTPUT_LIMIT_BYTES};

pub fn truncate_output(output: &str) -> (String, bool) {
    let bytes = output.as_bytes();
    let len = bytes.len();

    if len <= TRUNCATE_THRESHOLD_BYTES {
        return (output.to_string(), false);
    }

    let head_size = TRUNCATE_THRESHOLD_BYTES / 2; // 8KB
    let tail_size = TRUNCATE_THRESHOLD_BYTES / 2; // 8KB

    let head = String::from_utf8_lossy(&bytes[..head_size]);
    let tail = String::from_utf8_lossy(&bytes[len - tail_size..]);
    let omitted = len - TRUNCATE_THRESHOLD_BYTES;

    (
        format!(
            "{}\n\n[... truncated {} bytes ...]\n\n{}",
            head, omitted, tail
        ),
        true,
    )
}

pub fn limit_for_event_log(output: &str) -> (String, Option<String>) {
    let bytes = output.as_bytes();
    let len = bytes.len();

    if len <= EVENT_LOG_OUTPUT_LIMIT_BYTES {
        return (output.to_string(), None);
    }

    let limited = String::from_utf8_lossy(&bytes[..EVENT_LOG_OUTPUT_LIMIT_BYTES]).to_string();
    let hash = format!("sha256:{:x}", sha2::Sha256::digest(bytes));

    (limited, Some(hash))
}
```

- [ ] **Step 2: Add sha2 dependency to Cargo.toml**

```toml
# In crates/synthia-agent/Cargo.toml
[dependencies]
sha2 = "0.10"
```

- [ ] **Step 3: Add to mod.rs**

```rust
// Add to crates/synthia-agent/src/tool_executor/mod.rs
pub mod truncate;
```

- [ ] **Step 4: Write unit tests**

```rust
// In truncate.rs at bottom
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_output_not_truncated() {
        let (result, was_truncated) = truncate_output("small output");
        assert_eq!(result, "small output");
        assert!(!was_truncated);
    }

    #[test]
    fn test_large_output_is_truncated() {
        let large = "x".repeat(TRUNCATE_THRESHOLD_BYTES + 1000);
        let (result, was_truncated) = truncate_output(&large);
        assert!(was_truncated);
        assert!(result.contains("[... truncated 1000 bytes ...]"));
        assert!(result.len() < large.len());
    }

    #[test]
    fn test_event_log_limit() {
        let large = "x".repeat(EVENT_LOG_OUTPUT_LIMIT_BYTES + 1000);
        let (limited, hash) = limit_for_event_log(&large);
        assert_eq!(limited.len(), EVENT_LOG_OUTPUT_LIMIT_BYTES);
        assert!(hash.is_some());
        assert!(hash.unwrap().starts_with("sha256:"));
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p synthia-agent tool_executor::truncate --lib`
Expected: 3 tests pass

---

### Task 2.4: Create ToolExecutor main struct

**Files:**
- Modify: `crates/synthia-agent/src/tool_executor/mod.rs`

- [ ] **Step 1: Implement ToolExecutor**

```rust
// crates/synthia-agent/src/tool_executor/mod.rs
pub mod config;
pub mod timeout;
pub mod truncate;

use std::future::Future;
use tokio_util::sync::CancellationToken;

use config::{ToolCategory, ToolTimeoutConfig};
use timeout::{execute_with_retry, ExecutionError};
use truncate::truncate_output;

pub struct ToolExecutor {
    cancel_token: CancellationToken,
}

impl ToolExecutor {
    pub fn new(cancel_token: CancellationToken) -> Self {
        Self { cancel_token }
    }

    pub async fn execute<F, Fut>(&self, tool_name: &str, future_factory: impl FnMut() -> Fut) -> Result<String, ExecutionError>
    where
        Fut: Future<Output = Result<String, String>>,
    {
        let category = ToolCategory::from_tool_name(tool_name);
        let config = ToolTimeoutConfig::for_category(category);

        let result = execute_with_retry(
            future_factory,
            &config,
            self.cancel_token.clone(),
        ).await?;

        let (output, was_truncated) = truncate_output(&result);

        if was_truncated {
            // Log truncation event (placeholder for event_log integration)
            tracing::warn!(tool = tool_name, "Tool output was truncated");
        }

        Ok(output)
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p synthia-agent`
Expected: 0 errors

---

### Task 2.5: Integrate ToolExecutor into step.rs

**Files:**
- Modify: `crates/synthia-agent/src/agent/step.rs` (execute_single_tool function, around line 360-446)

- [ ] **Step 1: Find execute_single_tool in step.rs**

Read the function to understand current tool execution flow. It likely looks like:
```rust
let result = tool.call(input, deps).await;
```

- [ ] **Step 2: Wrap with ToolExecutor**

```rust
// In execute_single_tool, replace direct tool.call with:
let tool_name = tool.name().to_string();
let cancel_token = executor.cancel_token();

let result = executor.execute(&tool_name, || {
    let tool_clone = tool.clone(); // or Arc reference
    let input_clone = input.clone();
    let deps_clone = deps.clone();
    async move {
        tool_clone.call(&input_clone, &deps_clone).await
            .map_err(|e| e.to_string())
    }
}).await;
```

- [ ] **Step 3: Ensure ToolExecutor is available in ReAct state**

Add ToolExecutor to the dependencies or state struct that's passed to execute_single_tool.

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p synthia-agent`
Expected: 0 errors

---

### Task 2.6: Fix Subagent to wait for results

**Files:**
- Modify: `crates/synthia-agent/src/tools/agent_tools.rs` (AgentTool.call, around line 497-541)

- [ ] **Step 1: Find the current fire-and-forget code**

The current code likely does:
```rust
// Spawn and return immediately
tokio::spawn(async move { agent.run().await });
result.push_str("Waiting for result...");
ToolOutput::text(result)
```

- [ ] **Step 2: Replace with wait-for-result pattern**

```rust
use tokio::time::{timeout, Duration};

// ... in AgentTool.call:
let result_handle = tokio::spawn(async move {
    agent.run().await
});

match timeout(Duration::from_secs(300), result_handle).await {
    Ok(Ok(agent_result)) => ToolOutput::text(agent_result),
    Ok(Err(e)) => ToolOutput::text(format!("Subagent failed: {}", e)),
    Err(_) => ToolOutput::text("Subagent timed out after 5 minutes".to_string()),
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p synthia-agent`
Expected: 0 errors

---

### Task 3.1: Create event log module

**Files:**
- Create: `crates/synthia-agent/src/event_log/mod.rs`
- Create: `crates/synthia-agent/src/event_log/types.rs`

- [ ] **Step 1: Define event types**

```rust
// crates/synthia-agent/src/event_log/types.rs
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    #[serde(rename = "tool_call")]
    ToolCall {
        tool: String,
        input: serde_json::Value,
        output: String,
        truncated: bool,
        output_hash: Option<String>,
    },
    #[serde(rename = "decision")]
    Decision {
        content: String,
    },
    #[serde(rename = "error")]
    Error {
        tool: String,
        message: String,
        step: u32,
    },
    #[serde(rename = "file_modified")]
    FileModified {
        path: String,
        action: String,
    },
    #[serde(rename = "cron_exec")]
    CronExec {
        job_id: String,
        task: String,
        result: String,
        success: bool,
    },
}

impl Event {
    pub fn with_session(self, session_id: &str, step: u32) -> EventRecord {
        EventRecord {
            timestamp: Utc::now(),
            session_id: session_id.to_string(),
            step,
            event: self,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub timestamp: DateTime<Utc>,
    pub session_id: String,
    pub step: u32,
    #[serde(flatten)]
    pub event: Event,
}
```

- [ ] **Step 2: Implement async event log writer**

```rust
// crates/synthia-agent/src/event_log/mod.rs
pub mod types;

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::fs::{OpenOptions, create_dir_all};
use types::EventRecord;

pub struct EventLogger {
    base_dir: PathBuf,
    buffer: Arc<Mutex<Vec<EventRecord>>>,
    flush_interval: std::time::Duration,
}

impl EventLogger {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            buffer: Arc::new(Mutex::new(Vec::with_capacity(100))),
            flush_interval: std::time::Duration::from_secs(1),
        }
    }

    pub async fn log(&self, record: EventRecord) {
        let mut buffer = self.buffer.lock().await;
        buffer.push(record);

        if buffer.len() >= 100 {
            drop(buffer);
            self.flush().await;
        }
    }

    pub async fn flush(&self) {
        let mut buffer = self.buffer.lock().await;
        if buffer.is_empty() {
            return;
        }

        let events: Vec<EventRecord> = buffer.drain(..).collect();
        drop(buffer);

        if let Err(e) = self.write_events(&events).await {
            tracing::error!(error = %e, "Failed to flush event log");
        }
    }

    async fn write_events(&self, events: &[EventRecord]) -> std::io::Result<()> {
        let today = chrono::Utc::now().format("%Y-%m-%d");
        let dir = self.base_dir.join("events");
        create_dir_all(&dir).await?;

        let file_path = dir.join(format!("{}.jsonl", today));
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .await?;

        for event in events {
            let line = serde_json::to_string(event)?;
            use tokio::io::AsyncWriteExt;
            file.write_all(line.as_bytes()).await?;
            file.write_all(b"\n").await?;
        }

        file.sync_all().await?; // fsync
        Ok(())
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p synthia-agent`
Expected: 0 errors

---

### Task 3.3: Integrate credential_guard desensitization

**Files:**
- Modify: `crates/synthia-agent/src/event_log/mod.rs`
- Reference: `crates/synthia-guardian/src/credential_guard.rs` (existing credential scanner)

- [ ] **Step 1: Find existing credential_guard**

Read the existing credential scanning logic to understand how it detects and redacts secrets.

- [ ] **Step 2: Apply desensitization before writing events**

```rust
// In EventLogger.log(), before storing:
use crate::guardian::credential_guard::scan_and_redact;

pub async fn log(&self, record: EventRecord) {
    let redacted = self.redact_sensitive_data(record);
    let mut buffer = self.buffer.lock().await;
    buffer.push(redacted);
    // ... rest same
}

fn redact_sensitive_data(&self, record: EventRecord) -> EventRecord {
    // Apply credential_guard redaction to all string fields in the event
    // This uses the existing scan_and_redact function
    match record.event {
        Event::ToolCall { output, .. } => {
            let redacted = scan_and_redact(&output);
            // Reconstruct with redacted output
        }
        // ... handle other event types
    }
}
```

---

### Task 4.1: Create context thresholds module

**Files:**
- Create: `crates/synthia-agent/src/context/thresholds.rs`

- [ ] **Step 1: Define threshold constants and checks**

```rust
// crates/synthia-agent/src/context/thresholds.rs
pub const HARD_MIN_TOKENS: usize = 16_000;
pub const WARN_BELOW_TOKENS: usize = 32_000;
pub const STAGE1_THRESHOLD_PCT: f64 = 0.30;
pub const STAGE2_THRESHOLD_PCT: f64 = 0.50;
pub const STAGE3_THRESHOLD_PCT: f64 = 0.70;
pub const EMERGENCY_THRESHOLD_PCT: f64 = 0.95;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextStatus {
    Normal,
    Warning,
    Stage1Pruning,
    Stage2Pruning,
    Stage3Pruning,
    Emergency,
    Critical,
}

pub fn check_context_status(available_tokens: usize, max_tokens: usize) -> ContextStatus {
    let utilization = 1.0 - (available_tokens as f64 / max_tokens as f64);

    if available_tokens < HARD_MIN_TOKENS {
        return ContextStatus::Critical;
    }
    if utilization >= EMERGENCY_THRESHOLD_PCT {
        return ContextStatus::Emergency;
    }
    if utilization >= STAGE3_THRESHOLD_PCT || available_tokens < WARN_BELOW_TOKENS {
        return ContextStatus::Stage3Pruning;
    }
    if utilization >= STAGE2_THRESHOLD_PCT {
        return ContextStatus::Stage2Pruning;
    }
    if utilization >= STAGE1_THRESHOLD_PCT {
        return ContextStatus::Stage1Pruning;
    }
    if available_tokens < WARN_BELOW_TOKENS {
        return ContextStatus::Warning;
    }
    ContextStatus::Normal
}
```

- [ ] **Step 2: Write unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_context() {
        assert_eq!(check_context_status(100_000, 200_000), ContextStatus::Normal);
    }

    #[test]
    fn test_critical_context() {
        assert_eq!(check_context_status(10_000, 200_000), ContextStatus::Critical);
    }

    #[test]
    fn test_warning_below() {
        assert_eq!(check_context_status(30_000, 200_000), ContextStatus::Stage3Pruning);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p synthia-agent context::thresholds --lib`
Expected: 3 tests pass

---

### Task 4.2-4.5: Create pruning module

**Files:**
- Create: `crates/synthia-agent/src/context/pruning.rs`

- [ ] **Step 1: Define pruning stage enum and result**

```rust
// crates/synthia-agent/src/context/pruning.rs
use super::thresholds::ContextStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneLevel {
    SoftTrim,
    HardClear,
    CompressLevel1, // Decision/Error - keep full
    CompressLevel2, // FileModified - one line summary
    CompressLevel3, // FileRead/Output - remove
}

pub struct PruneResult {
    pub was_pruned: bool,
    pub stage: Option<ContextStatus>,
    pub tokens_saved: usize,
}
```

- [ ] **Step 2: Implement Soft Trim**

```rust
pub fn soft_trim_tool_result(content: &str, head_tokens: usize, tail_tokens: usize) -> (String, usize) {
    let original_len = content.len();
    let chars: Vec<char> = content.chars().collect();
    let total_chars = chars.len();

    if total_chars <= head_tokens + tail_tokens {
        return (content.to_string(), 0);
    }

    let head: String = chars[..head_tokens].iter().collect();
    let tail: String = chars[total_chars - tail_tokens..].iter().collect();
    let omitted = total_chars - head_tokens - tail_tokens;

    (
        format!("{}\n\n[trimmed: omitted {} characters]\n\n{}", head, omitted, tail),
        original_len,
    )
}
```

- [ ] **Step 3: Implement Hard Clear**

```rust
pub fn hard_clear_tool_result() -> String {
    "[cleared]".to_string()
}
```

- [ ] **Step 4: Implement Level-based compression**

```rust
pub fn compress_event(event_type: &str, content: &str, level: PruneLevel) -> Option<String> {
    match level {
        PruneLevel::CompressLevel1 => Some(content.to_string()), // Keep full
        PruneLevel::CompressLevel2 => {
            // One line summary
            Some(format!("{}: {}", event_type, content.lines().next().unwrap_or("")))
        }
        PruneLevel::CompressLevel3 => None, // Remove from context (kept in event log)
        _ => Some(content.to_string()),
    }
}
```

- [ ] **Step 5: Write unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_soft_trim() {
        let content = "x".repeat(2000);
        let (result, saved) = soft_trim_tool_result(&content, 500, 500);
        assert!(result.contains("[trimmed: omitted"));
        assert!(result.len() < content.len());
    }

    #[test]
    fn test_hard_clear() {
        assert_eq!(hard_clear_tool_result(), "[cleared]");
    }

    #[test]
    fn test_compress_level1() {
        let result = compress_event("Decision", "kept content", PruneLevel::CompressLevel1);
        assert_eq!(result, Some("kept content".to_string()));
    }

    #[test]
    fn test_compress_level3() {
        let result = compress_event("FileRead", "removed content", PruneLevel::CompressLevel3);
        assert_eq!(result, None);
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p synthia-agent context::pruning --lib`
Expected: 4+ tests pass

---

### Task 5.1-5.3: Create Cron tools

**Files:**
- Create: `crates/synthia-agent/src/tools/cron_store.rs`
- Create: `crates/synthia-agent/src/tools/cron_wrapper.rs`
- Create: `crates/synthia-agent/src/tools/cron_tool.rs`

- [ ] **Step 1: Create CronFileStore**

```rust
// crates/synthia-agent/src/tools/cron_store.rs
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tokio::fs::{OpenOptions, create_dir_all};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJobDefinition {
    pub id: String,
    pub cron: String,
    pub task: String,
    pub mode: String, // standalone | inject | session
    pub enabled: bool,
    pub created_at: String,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
    pub success_count: u64,
    pub failure_count: u64,
}

pub struct CronFileStore {
    file_path: PathBuf,
}

impl CronFileStore {
    pub fn new(base_dir: PathBuf) -> Self {
        let file_path = base_dir.join("cron_jobs.jsonl");
        Self { file_path }
    }

    pub async fn load(&self) -> Vec<CronJobDefinition> {
        if !self.file_path.exists() {
            return Vec::new();
        }

        let content = tokio::fs::read_to_string(&self.file_path).await.unwrap_or_default();
        content
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    pub async fn save(&self, job: &CronJobDefinition) -> std::io::Result<()> {
        if let Some(parent) = self.file_path.parent() {
            create_dir_all(parent).await?;
        }

        let line = serde_json::to_string(job)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)
            .await?;

        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.sync_all().await?;
        Ok(())
    }
}
```

- [ ] **Step 2: Create cron_add/cron_list tools**

```rust
// crates/synthia-agent/src/tools/cron_tool.rs
use async_trait::async_trait;
use crate::tools::{Tool, ToolInput, ToolOutput};

pub struct CronAddTool {
    store: std::sync::Arc<super::cron_store::CronFileStore>,
}

#[async_trait]
impl Tool for CronAddTool {
    fn name(&self) -> &str { "cron_add" }

    fn description(&self) -> &str {
        "Add a scheduled cron job. Parameters: cron (expression), task (description), mode (auto|standalone|inject|session)"
    }

    async fn call(&self, input: &ToolInput, deps: &crate::AgentDeps) -> Result<ToolOutput, Box<dyn std::error::Error + Send + Sync>> {
        let cron = input.get("cron").ok_or("Missing 'cron' parameter")?;
        let task = input.get("task").ok_or("Missing 'task' parameter")?;
        let mode = input.get("mode").unwrap_or("auto");

        // Validate minimum interval (1 minute)
        if cron == "* * * * * *" || cron.starts_with("*/0 ") {
            return Ok(ToolOutput::text("Error: Minimum interval is 1 minute".to_string()));
        }

        let job = super::cron_store::CronJobDefinition {
            id: format!("job_{}", uuid::Uuid::new_v4().to_string()[..8].to_string()),
            cron: cron.clone(),
            task: task.clone(),
            mode: mode.to_string(),
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
            last_run: None,
            next_run: None,
            success_count: 0,
            failure_count: 0,
        };

        self.store.save(&job).await?;

        Ok(ToolOutput::text(format!(
            "Cron job created:\nID: {}\nCron: {}\nTask: {}\nMode: {}",
            job.id, job.cron, job.task, job.mode
        )))
    }
}
```

---

### Task 7.1: Create Context Trace module

**Files:**
- Create: `crates/synthia-agent/src/observability/context_trace.rs`

- [ ] **Step 1: Implement context trace recording**

```rust
// crates/synthia-agent/src/observability/context_trace.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs::{create_dir_all, OpenOptions};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Serialize, Deserialize)]
pub struct ContextTrace {
    pub timestamp: String,
    pub session_id: String,
    pub step: u32,
    pub message_count: usize,
    pub total_tokens: usize,
    pub context_utilization: f64,
    pub prefix_hash: String,
    pub prefix_changed: bool,
    pub cache_hit: bool,
    pub pruning_stage: String,
    pub sections: Vec<SectionInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SectionInfo {
    pub name: String,
    pub tokens: usize,
    pub priority: u8,
}

pub async fn record_trace(base_dir: PathBuf, trace: ContextTrace) -> std::io::Result<()> {
    let dir = base_dir.join("traces");
    create_dir_all(&dir).await?;

    let file_path = dir.join(format!("context_{}_{}.jsonl", trace.session_id, trace.step));
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&file_path)
        .await?;

    let line = serde_json::to_string(&trace)?;
    file.write_all(line.as_bytes()).await?;
    file.sync_all().await?;
    Ok(())
}
```

---

### Task 9.1-9.7: Final verification

- [ ] **Step 1: Format all code**

Run: `cargo +nightly fmt --all`

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --all-targets --all-features --tests --all`
Expected: 0 warnings (fix any)

- [ ] **Step 3: Run all tests**

Run: `cargo test --lib`
Expected: All tests pass

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "feat: synthia-agent production-ready upgrade (tool timeout, context pruning, cron, memory, observability, error recovery)"
```
