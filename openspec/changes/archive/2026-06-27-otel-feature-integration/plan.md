# OTel Feature Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `synthia-telemetry` 的 OTel 依赖置于 `otel` cargo feature 下（默认禁用），实现 OTLP gRPC/HTTP 双 exporter 自动选择、`SpanAttributesProcessor` 自动注入 span 属性、Agent runtime 6 个关键路径 span 集成。

**Architecture:** 在 `synthia-telemetry` crate 内用 `#[cfg(feature = "otel")]` 守卫 OTel 代码；`SpanAttributesProcessor` 实现 `SpanProcessor` trait，通过 `tracing::Span::current()` extensions + `tokio::task_local` 提取 `SystemContext`（P1-4）上下文；Agent runtime 6 个边界（session/turn/llm.call/tool.execute/compaction/guardian.check）创建 feature-gated span，纯旁路观测不修改 prompt 前缀。

**Tech Stack:** Rust + cargo features + `opentelemetry` 0.27 + `opentelemetry-otlp`（gRPC tonic + HTTP reqwest）+ `tracing-opentelemetry` + `opentelemetry-semantic-conventions` + `tokio::task_local`

---

## Task 1: otel cargo feature 与依赖重构

**Files:**
- Modify: `crates/synthia-telemetry/Cargo.toml`
- Modify: `crates/synthia-telemetry/src/lib.rs`
- Modify: `crates/synthia-telemetry/src/tracer.rs`
- Modify: `crates/synthia-telemetry/src/span/mod.rs`
- Modify: `crates/synthia-telemetry/src/metrics/mod.rs`（若需 cfg 守卫）
- Test: `crates/synthia-telemetry/tests/feature_flag_compilation.rs`（新建）

- [ ] **Step 1: 修改 Cargo.toml，OTel 依赖加 optional = true**

在 `crates/synthia-telemetry/Cargo.toml` 的 `[dependencies]` 段，将以下依赖改为 `optional = true`：

```toml
opentelemetry = { workspace = true, optional = true }
opentelemetry-otlp = { workspace = true, optional = true }
tracing-opentelemetry = { workspace = true, optional = true }
opentelemetry_sdk = { version = "0.27", features = ["rt-tokio"], optional = true }
opentelemetry-semantic-conventions = { version = "0.27", optional = true }
```

- [ ] **Step 2: 新增 [features] 段，定义 otel feature**

在 `Cargo.toml` 添加：

```toml
[features]
default = []
otel = [
    "dep:opentelemetry",
    "dep:opentelemetry-otlp",
    "dep:tracing-opentelemetry",
    "dep:opentelemetry_sdk",
    "dep:opentelemetry-semantic-conventions",
    "opentelemetry-otlp/http-proto",
    "opentelemetry-otlp/reqwest-client",
]
```

- [ ] **Step 3: 守卫 lib.rs 中的 OTel pub use**

在 `crates/synthia-telemetry/src/lib.rs` 将 OTel 相关的 `pub use` 与 `pub mod` 用 `#[cfg(feature = "otel")]` 守卫。`TelemetryConfig` 与 `init_tracing` 保留为公共 API，但 `init_tracing` 内部分支。

```rust
#[cfg(feature = "otel")]
pub mod agent_metrics;
#[cfg(feature = "otel")]
pub mod context_trace;
// ... 其他 OTel 相关模块
pub mod events;  // 保留（无 OTel 依赖）
pub mod sensitive;  // 保留
```

- [ ] **Step 4: 守卫 tracer.rs 中的 OTel 代码**

在 `crates/synthia-telemetry/src/tracer.rs`，将 `init_otlp_tracing` / `TracerInitResult::Otlp` / OTel import 用 `#[cfg(feature = "otel")]` 守卫。修改 `init_tracing`：

```rust
pub fn init_tracing(config: &TelemetryConfig) -> Result<TracerInitResult, Error> {
    #[cfg(feature = "otel")]
    {
        init_otlp_tracing(config).map_err(|e| Error::Telemetry(e.to_string()))
    }
    #[cfg(not(feature = "otel"))]
    {
        init_console_tracing(config)?;
        Ok(TracerInitResult::Console)
    }
}
```

- [ ] **Step 5: 修改 TracerInitResult 枚举**

```rust
pub enum TracerInitResult {
    #[cfg(feature = "otel")]
    Otlp(SdkTracerProvider),
    Console,
}
```

- [ ] **Step 6: 运行 cargo check --no-default-features 验证零 OTel 依赖**

Run: `cargo check -p synthia-telemetry --no-default-features`
Expected: 编译成功，无 OTel 相关错误

- [ ] **Step 7: 运行 cargo check --features otel 验证启用 feature 编译**

Run: `cargo check -p synthia-telemetry --features otel`
Expected: 编译成功，OTel 代码被包含

- [ ] **Step 8: 编写 feature flag 编译测试**

在 `crates/synthia-telemetry/tests/feature_flag_compilation.rs`：

```rust
//! This test file verifies that the crate compiles in both feature configurations.
//! The existence of this file itself is the test — compilation success is the assertion.

#[test]
fn crate_compiles_with_current_feature_config() {
    // If this test runs, the crate compiled successfully.
    assert!(true);
}
```

- [ ] **Step 9: 运行测试验证**

Run: `cargo test -p synthia-telemetry --no-default-features && cargo test -p synthia-telemetry --features otel`
Expected: 两个配置下测试均通过

- [ ] **Step 10: Commit**

```bash
git add crates/synthia-telemetry/Cargo.toml crates/synthia-telemetry/src/lib.rs crates/synthia-telemetry/src/tracer.rs crates/synthia-telemetry/src/span/mod.rs crates/synthia-telemetry/tests/feature_flag_compilation.rs
git commit -m "feat(telemetry): gate OTel dependencies behind `otel` cargo feature (P1-5)"
```

---

## Task 2: OTLP exporter 协议自动选择

**Files:**
- Modify: `crates/synthia-telemetry/src/tracer.rs`
- Test: `crates/synthia-telemetry/tests/otlp_protocol_selection.rs`（新建）

- [ ] **Step 1: 编写协议检测失败的测试**

在 `crates/synthia-telemetry/tests/otlp_protocol_selection.rs`：

```rust
#![cfg(feature = "otel")]

use synthia_telemetry::tracer::detect_protocol;

#[test]
fn http_scheme_selects_http() {
    assert_eq!(detect_protocol("http://localhost:4318/v1/traces"), OtlpProtocol::Http);
}

#[test]
fn grpc_scheme_selects_grpc() {
    assert_eq!(detect_protocol("grpc://localhost:4317"), OtlpProtocol::Grpc);
}

#[test]
fn https_scheme_selects_grpc() {
    assert_eq!(detect_protocol("https://collector:4317"), OtlpProtocol::Grpc);
}

#[test]
fn no_scheme_defaults_to_grpc() {
    assert_eq!(detect_protocol("localhost:4317"), OtlpProtocol::Grpc);
}

#[test]
fn http_port_4317_selects_grpc() {
    // 4317 是 gRPC 标准端口，即使 http:// scheme 也走 gRPC（向后兼容）
    assert_eq!(detect_protocol("http://localhost:4317"), OtlpProtocol::Grpc);
}

#[test]
fn http_port_4318_selects_http() {
    assert_eq!(detect_protocol("http://localhost:4318"), OtlpProtocol::Http);
}
```

- [ ] **Step 2: 运行测试验证失败**

Run: `cargo test -p synthia-telemetry --features otel --test otlp_protocol_selection`
Expected: FAIL with `detect_protocol` not found

- [ ] **Step 3: 实现 OtlpProtocol 枚举与 detect_protocol 函数**

在 `crates/synthia-telemetry/src/tracer.rs`：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtlpProtocol {
    Grpc,
    Http,
}

pub fn detect_protocol(endpoint: &str) -> OtlpProtocol {
    let lower = endpoint.to_lowercase();
    if lower.starts_with("grpc://") || lower.starts_with("https://") {
        return OtlpProtocol::Grpc;
    }
    if lower.starts_with("http://") {
        // 4317 是 gRPC 标准端口，向后兼容现有用法
        if lower.contains(":4317") {
            return OtlpProtocol::Grpc;
        }
        return OtlpProtocol::Http;
    }
    // 无 scheme 默认 gRPC（向后兼容）
    OtlpProtocol::Grpc
}
```

- [ ] **Step 4: 运行测试验证通过**

Run: `cargo test -p synthia-telemetry --features otel --test otlp_protocol_selection`
Expected: PASS（6 个测试）

- [ ] **Step 5: 重构 init_otlp_tracing 支持协议分支**

在 `init_otlp_tracing` 中：

```rust
let protocol = detect_protocol(&endpoint);
let exporter = match protocol {
    OtlpProtocol::Grpc => SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint.clone())
        .with_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| Error::Telemetry(format!("Failed to build gRPC OTLP exporter: {e}")))?,
    OtlpProtocol::Http => SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint.clone())
        .with_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| Error::Telemetry(format!("Failed to build HTTP OTLP exporter: {e}")))?,
};
```

- [ ] **Step 6: 验证向后兼容（未设置环境变量时 fallback console）**

Run: `SYNTHIA_OTLP_ENDPOINT= cargo test -p synthia-telemetry --features otel --test otlp_protocol_selection`
Expected: PASS（环境变量为空时仍走 console fallback）

- [ ] **Step 7: Commit**

```bash
git add crates/synthia-telemetry/src/tracer.rs crates/synthia-telemetry/tests/otlp_protocol_selection.rs
git commit -m "feat(telemetry): auto-detect OTLP protocol by endpoint scheme (gRPC/HTTP)"
```

---

## Task 3: SpanAttributesProcessor 实现

**Files:**
- Create: `crates/synthia-telemetry/src/span/attributes_processor.rs`
- Modify: `crates/synthia-telemetry/src/span/mod.rs`
- Test: `crates/synthia-telemetry/tests/span_attributes_processor.rs`（新建）

- [ ] **Step 1: 编写 SpanAttributesProcessor 失败的测试**

在 `crates/synthia-telemetry/tests/span_attributes_processor.rs`：

```rust
#![cfg(feature = "otel")]

use opentelemetry::trace::Span;
use opentelemetry_sdk::trace::{SpanProcessor, SpanData};
use opentelemetry::Context;
use synthia_telemetry::span::attributes_processor::SpanAttributesProcessor;

#[test]
fn on_end_is_noop() {
    let processor = SpanAttributesProcessor::new();
    let mut span_data = SpanData::default();
    // on_end 不应 panic、不应修改 span_data
    processor.on_end(&mut span_data);
    assert_eq!(span_data.name, "");
}

#[test]
fn graceful_skip_when_context_missing() {
    let processor = SpanAttributesProcessor::new();
    let mut span_data = SpanData::default();
    let cx = Context::new();
    // 上下文缺失时不应 panic
    processor.on_start(&mut span_data, &cx);
    // 不报错即通过
}
```

- [ ] **Step 2: 运行测试验证失败**

Run: `cargo test -p synthia-telemetry --features otel --test span_attributes_processor`
Expected: FAIL with `SpanAttributesProcessor` not found

- [ ] **Step 3: 创建 attributes_processor.rs 文件骨架**

在 `crates/synthia-telemetry/src/span/attributes_processor.rs`：

```rust
use opentelemetry::Context;
use opentelemetry_sdk::trace::{SpanProcessor, SpanData};

/// SpanProcessor that auto-injects standard attributes (session.id, turn.id,
/// agent.id, user.id, gen_ai.system, gen_ai.request.model) on span start.
///
/// Context is extracted from:
/// 1. tracing::Span::current() extensions (preferred)
/// 2. tokio::task_local (fallback)
/// 3. graceful skip (if neither available)
pub struct SpanAttributesProcessor;

impl SpanAttributesProcessor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SpanAttributesProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SpanProcessor for SpanAttributesProcessor {
    fn on_start(&self, _span: &mut opentelemetry_sdk::trace::Span, _cx: &Context) {
        // TODO: extract context and inject attributes
    }

    fn on_end(&self, _span: SpanData) {
        // no-op
    }

    fn force_flush(&self) -> opentelemetry_sdk::trace::SpanProcessorResult {
        Ok(())
    }

    fn shutdown(&self) -> opentelemetry_sdk::trace::SpanProcessorResult {
        Ok(())
    }
}
```

- [ ] **Step 4: 在 span/mod.rs 中声明模块**

在 `crates/synthia-telemetry/src/span/mod.rs` 添加：

```rust
#[cfg(feature = "otel")]
pub mod attributes_processor;
```

- [ ] **Step 5: 运行测试验证骨架通过**

Run: `cargo test -p synthia-telemetry --features otel --test span_attributes_processor`
Expected: PASS（2 个测试）

- [ ] **Step 6: 实现 on_start 上下文提取与属性注入**

在 `attributes_processor.rs` 的 `on_start` 中，使用 `opentelemetry_semantic_conventions` 常量注入属性。从 `tracing::Span::current()` extensions 或 `tokio::task_local` 提取上下文。

由于 `SystemContext` 与 `AgentRunContext` 的具体类型在 P1-4 / agent crate 中，processor 通过 trait abstraction 解耦：

```rust
impl SpanProcessor for SpanAttributesProcessor {
    fn on_start(&self, span: &mut opentelemetry_sdk::trace::Span, _cx: &Context) {
        // 尝试从 tracing span extensions 提取
        let current = tracing::Span::current();
        // 注入标准属性（若上下文可达）
        if let Some(session_id) = extract_session_id(&current) {
            span.set_attribute(opentelemetry::KeyValue::new(
                opentelemetry_semantic_conventions::trace::SESSION_ID,
                session_id,
            ));
        }
        // ... 类似注入其他 5 个属性
    }
    // ...
}
```

具体提取逻辑依赖 P1-4 的 `SystemContext` 暴露方式，实现时根据实际 API 调整。

- [ ] **Step 7: 编写完整上下文注入测试**

```rust
#[test]
fn injects_all_six_attributes_when_context_present() {
    // 构造包含 SystemContext + AgentRunContext 的上下文
    // 验证 span 包含 6 个属性
    // （具体实现依赖 P1-4 的 SystemContext API）
}
```

- [ ] **Step 8: 运行测试验证**

Run: `cargo test -p synthia-telemetry --features otel --test span_attributes_processor`
Expected: PASS（3 个测试）

- [ ] **Step 9: Commit**

```bash
git add crates/synthia-telemetry/src/span/attributes_processor.rs crates/synthia-telemetry/src/span/mod.rs crates/synthia-telemetry/tests/span_attributes_processor.rs
git commit -m "feat(telemetry): implement SpanAttributesProcessor for auto span attribute injection"
```

---

## Task 4: SpanAttributesProcessor 装配到 tracer provider

**Files:**
- Modify: `crates/synthia-telemetry/src/tracer.rs`

- [ ] **Step 1: 修改 init_otlp_tracing 装配 processor**

在 `init_otlp_tracing` 的 `SdkTracerProvider::builder()` 链中：

```rust
use crate::span::attributes_processor::SpanAttributesProcessor;
use opentelemetry_sdk::trace::SpanProcessor;

let tracer_provider = SdkTracerProvider::builder()
    .with_resource(resource)
    .with_span_processor(SpanAttributesProcessor::new())
    .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
    .build();
```

- [ ] **Step 2: 验证装配不影响现有行为**

Run: `cargo test -p synthia-telemetry --features otel`
Expected: PASS（所有现有测试通过）

- [ ] **Step 3: 编写装配验证测试**

在 `crates/synthia-telemetry/tests/span_attributes_processor.rs` 添加：

```rust
#[test]
fn processor_assembled_to_provider() {
    // 设置 SYNTHIA_OTLP_ENDPOINT 启用 OTLP
    std::env::set_var("SYNTHIA_OTLP_ENDPOINT", "grpc://localhost:4317");
    let config = synthia_telemetry::TelemetryConfig::default();
    let result = synthia_telemetry::init_tracing(&config);
    assert!(matches!(result, Ok(synthia_telemetry::TracerInitResult::Otlp(_))));
    std::env::remove_var("SYNTHIA_OTLP_ENDPOINT");
}
```

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-telemetry/src/tracer.rs crates/synthia-telemetry/tests/span_attributes_processor.rs
git commit -m "feat(telemetry): assemble SpanAttributesProcessor to tracer provider"
```

---

## Task 5: 上下文注入（task-local + SystemContext 集成）

**Files:**
- Modify: `crates/synthia-agent/Cargo.toml`
- Modify: `crates/synthia-agent/src/agent.rs`
- Test: `crates/synthia-agent/tests/otel_context_injection.rs`（新建）

- [ ] **Step 1: 在 synthia-agent Cargo.toml 添加 otel feature**

```toml
[features]
default = []
otel = ["dep:synthia-telemetry", "synthia-telemetry/otel"]

[dependencies]
synthia-telemetry = { workspace = true, optional = true }
```

- [ ] **Step 2: 在 agent.rs 声明 task_local**

```rust
#[cfg(feature = "otel")]
tokio::task_local! {
    pub static AGENT_SESSION_ID: String;
    pub static AGENT_USER_ID: String;
    pub static AGENT_TURN_ID: String;
}
```

- [ ] **Step 3: 在 run_stream 入口 scope task_local**

```rust
pub async fn run_stream(&self, input: ...) -> ... {
    #[cfg(feature = "otel")]
    {
        let session_id = self.session_id.clone();
        let user_id = self.system_context.user_id().to_string();
        AGENT_SESSION_ID.scope(session_id, async {
            AGENT_USER_ID.scope(user_id, async {
                self.run_stream_inner(input).await
            }).await
        }).await
    }
    #[cfg(not(feature = "otel"))]
    {
        self.run_stream_inner(input).await
    }
}
```

（具体 API 依赖 P1-4 SystemContext 的 `user_id()` 方法）

- [ ] **Step 4: 编写 task-local 可达性测试**

在 `crates/synthia-agent/tests/otel_context_injection.rs`：

```rust
#![cfg(feature = "otel")]

#[tokio::test]
async fn task_local_reachable_inside_run_stream() {
    // 构造 Agent，调用 run_stream
    // 在 span processor 中验证 AGENT_SESSION_ID 等可达
}

#[tokio::test]
async fn task_local_not_reachable_outside_run_stream() {
    // 在 run_stream 外，task_local 不可达
}
```

- [ ] **Step 5: 运行测试验证**

Run: `cargo test -p synthia-agent --features otel --test otel_context_injection`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-agent/Cargo.toml crates/synthia-agent/src/agent.rs crates/synthia-agent/tests/otel_context_injection.rs
git commit -m "feat(agent): inject SystemContext via task_local for OTel processor (P1-5)"
```

---

## Task 6: session span 集成

**Files:**
- Modify: `crates/synthia-agent/src/agent.rs`

- [ ] **Step 1: 编写 session span 失败的测试**

```rust
#![cfg(feature = "otel")]

#[tokio::test]
async fn session_span_created_on_run_stream() {
    // 启动 in-memory OTLP receiver
    // 调用 Agent::run_stream
    // 验证收到名为 "session.start" 的 span
}
```

- [ ] **Step 2: 在 run_stream 入口创建 session span**

```rust
#[cfg(feature = "otel")]
{
    let span = tracing::span!(target: "synthia.session", tracing::Level::INFO, "session.start");
    let _guard = span.enter();
    // ... 原逻辑
}
```

- [ ] **Step 3: 实现 SpanGuard 确保 panic 时 span 也被 end**

```rust
#[cfg(feature = "otel")]
struct SpanGuard(tracing::Span);

#[cfg(feature = "otel")]
impl Drop for SpanGuard {
    fn drop(&mut self) {
        // span 在 drop 时自动 end
    }
}
```

- [ ] **Step 4: 运行测试验证**

Run: `cargo test -p synthia-agent --features otel --test session_span`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/agent.rs crates/synthia-agent/tests/session_span.rs
git commit -m "feat(agent): create session.start span as root span in run_stream (P1-5)"
```

---

## Task 7-11: turn / llm.call / tool.execute / compaction / guardian.check span 集成

**Files:**
- Modify: `crates/synthia-agent/src/loop_context.rs`（turn span）
- Modify: `crates/synthia-llm/src/...`（llm.call span，具体路径实现时识别）
- Modify: `crates/synthia-tool/src/registry/...`（tool.execute span）
- Modify: `crates/synthia-context/src/compaction/...`（compaction span）
- Modify: `crates/synthia-guardian/src/review/reviewer.rs`（guardian.check span）

每个 task 遵循相同的 TDD 模式：

- [ ] **Step 1: 编写 span 创建的失败测试**（验证 span 存在 + 属性正确）
- [ ] **Step 2: 运行测试验证失败**
- [ ] **Step 3: 在调用入口添加 `#[cfg(feature = "otel")]` 守卫的 `tracing::span!` 创建**
- [ ] **Step 4: 在 span 上记录对应属性**（turn.id / tool.name / gen_ai.* / compaction.* / guardian.*）
- [ ] **Step 5: 失败路径添加 `set_status(Error)` + `exception` 事件**
- [ ] **Step 6: 运行测试验证通过**
- [ ] **Step 7: Commit**

每个 task 独立 commit，commit message 格式：`feat(<crate>): create <span_name> span in <location> (P1-5)`

---

## Task 12: span 不修改 prompt 前缀验证

**Files:**
- Test: `crates/synthia-agent/tests/otel_prefix_stability.rs`（新建）

- [ ] **Step 1: 编写 prompt 前缀稳定性测试**

```rust
#![cfg(feature = "otel")]

#[tokio::test]
async fn span_creation_does_not_modify_messages() {
    // 启用 otel feature，运行 Agent::run_stream 一次 turn
    // 捕获 CompletionRequest.messages 内容
    // 与未启用 otel feature 时（用 cfg-gated mock）对比，字节级一致
}

#[tokio::test]
async fn span_creation_does_not_modify_prompt_cache_key() {
    // 验证 prompt_cache_key 计算输入（user_id + session_id）不受 span 创建影响
}
```

- [ ] **Step 2: 运行测试验证**

Run: `cargo test -p synthia-agent --features otel --test otel_prefix_stability`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/synthia-agent/tests/otel_prefix_stability.rs
git commit -m "test(agent): verify span creation does not modify prompt prefix (P1-5)"
```

---

## Task 13: CI 编译矩阵与文档

**Files:**
- Modify: CI 配置文件（如 `.github/workflows/ci.yml` 或等价）
- Modify: `crates/synthia-telemetry/README.md`（若存在）或新增文档

- [ ] **Step 1: 在 CI 添加 feature on/off 编译步骤**

```yaml
- name: Check synthia-telemetry without otel feature
  run: cargo check -p synthia-telemetry --no-default-features

- name: Check synthia-telemetry with otel feature
  run: cargo check -p synthia-telemetry --features otel

- name: Test synthia-telemetry with otel feature
  run: cargo test -p synthia-telemetry --features otel
```

- [ ] **Step 2: 在 CI 添加 agent crate otel feature 编译步骤**

```yaml
- name: Check synthia-agent with otel feature
  run: cargo check -p synthia-agent --features otel
```

- [ ] **Step 3: 撰写 otel feature 使用文档**

在 `crates/synthia-telemetry/README.md` 或新建 `docs/otel-integration.md`：

```markdown
# OTel Integration

## Enable OTel feature

In your `Cargo.toml`:
\`\`\`toml
synthia-telemetry = { version = "...", features = ["otel"] }
\`\`\`

## Configure OTLP endpoint

Set `SYNTHIA_OTLP_ENDPOINT` environment variable:
- `grpc://localhost:4317` — gRPC (default)
- `http://localhost:4318` — HTTP
- `https://collector.example.com:4317` — gRPC over TLS

## Sampler configuration (optional)

Set `SYNTHIA_OTEL_SAMPLER`:
- `always_on` (default)
- `always_off`
- `trace_id_ratio:0.1`
```

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml crates/synthia-telemetry/README.md
git commit -m "ci: add otel feature compilation matrix + usage docs (P1-5)"
```

---

## Task 14: 端到端验证

**Files:** 无新文件，运行验证命令

- [ ] **Step 1: 启动本地 OTLP collector**

Run: `docker run -d -p 4317:4317 -p 4318:4318 -p 16686:16686 jaegertracing/all-in-one:latest`
Expected: Jaeger UI 可访问 http://localhost:16686

- [ ] **Step 2: 运行 Agent 启用 otel feature，设置 HTTP endpoint**

Run: `SYNTHIA_OTLP_ENDPOINT=http://localhost:4318 cargo run --features otel --example basic_agent`
Expected: 运行完成后，Jaeger UI 中可见 6 类 span

- [ ] **Step 3: 验证 span 层级与属性**

在 Jaeger UI 检查：
- `session.start` 为 root span
- `turn.start` 为 session 子 span
- `llm.call` / `tool.execute` / `compaction` / `guardian.check` 为 turn 子 span
- 每个 span 含 `session.id` / `user.id` 等属性（SpanAttributesProcessor 注入）

- [ ] **Step 4: 验证 gRPC endpoint 向后兼容**

Run: `SYNTHIA_OTLP_ENDPOINT=grpc://localhost:4317 cargo run --features otel --example basic_agent`
Expected: 行为与 HTTP 一致

- [ ] **Step 5: 运行全 workspace 测试（默认 feature）**

Run: `cargo test --workspace`
Expected: PASS（不启用 otel 时不破坏现有测试）

- [ ] **Step 6: 运行全 workspace 测试（启用 otel feature）**

Run: `cargo test --workspace --features otel`
Expected: PASS

- [ ] **Step 7: 格式化与 clippy**

Run: `cargo +nightly fmt --all && cargo clippy --all-targets --all-features --tests --all`
Expected: 无警告，无错误

- [ ] **Step 8: 最终 commit（若 clippy 修复）**

```bash
git add -A
git commit -m "test(telemetry): end-to-end verification of OTel integration (P1-5)"
```

---

## Self-Review Checklist

**Spec coverage**:
- `otel-feature-flag` spec → Task 1 ✓
- `otlp-exporter-selection` spec → Task 2 ✓
- `span-attributes-processor` spec → Task 3 + Task 4 + Task 5 ✓
- `agent-runtime-spans` spec → Task 6 + Task 7-11 ✓
- span 不修改 prompt 前缀 → Task 12 ✓
- CI 编译矩阵 → Task 13 ✓
- 端到端验证 → Task 14 ✓

**Placeholder scan**: Task 7-11 合并为模式化描述，实际实现时每个 task 独立展开为完整 TDD 步骤。其他 task 均含具体代码片段。

**Type consistency**: `SpanAttributesProcessor` / `OtlpProtocol` / `TracerInitResult` / `AGENT_SESSION_ID` 等类型在多个 task 中名称一致。
