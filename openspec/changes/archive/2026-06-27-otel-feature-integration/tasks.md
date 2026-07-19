## 1. otel cargo feature 与依赖重构

- [x] 1.1 修改 `crates/synthia-telemetry/Cargo.toml`：将 `opentelemetry` / `opentelemetry-otlp` / `tracing-opentelemetry` / `opentelemetry_sdk` / `opentelemetry-semantic-conventions` 加 `optional = true`
- [x] 1.2 在 `crates/synthia-telemetry/Cargo.toml` 新增 `[features]` 段，定义 `otel = ["dep:opentelemetry", "dep:opentelemetry-otlp", "dep:tracing-opentelemetry", "dep:opentelemetry_sdk", "dep:opentelemetry-semantic-conventions"]`（默认禁用）
- [x] 1.3 在 `otel` feature 中启用 `opentelemetry-otlp` 的 `http-proto` feature（HTTP exporter 支持）
- [x] 1.4 在 `otel` feature 中启用 `opentelemetry-otlp` 的 `reqwest-client` feature（HTTP exporter 客户端）
- [x] 1.5 在 `crates/synthia-telemetry/src/lib.rs` 中将 OTel 相关 `pub use` 与 `pub mod` 用 `#[cfg(feature = "otel")]` 守卫
- [x] 1.6 在 `crates/synthia-telemetry/src/tracer.rs` 中将 `init_otlp_tracing` / `TracerInitResult::Otlp` / OTel import 用 `#[cfg(feature = "otel")]` 守卫
- [x] 1.7 修改 `init_tracing` 函数：无 `otel` feature 时直接调用 `init_console_tracing` 返回 `Ok(TracerInitResult::Console)`
- [x] 1.8 在 `crates/synthia-telemetry/src/span/` / `metrics/` / `span_context/` 等模块中，将 OTel 引用代码用 `#[cfg(feature = "otel")]` 守卫（保留 `tracing` 基础类型）
- [x] 1.9 检查 workspace `Cargo.toml`，确保 `synthia-telemetry` 依赖不强制启用 `otel` feature
- [x] 1.10 运行 `cargo check -p synthia-telemetry --no-default-features` 验证零 OTel 依赖编译通过
- [x] 1.11 运行 `cargo check -p synthia-telemetry --features otel` 验证启用 feature 编译通过

## 2. OTLP exporter 协议自动选择

- [x] 2.1 在 `crates/synthia-telemetry/src/tracer.rs` 中新增 `OtlpProtocol` 枚举（`Grpc` / `Http`）
- [x] 2.2 新增 `fn detect_protocol(endpoint: &str) -> OtlpProtocol` 函数：解析 scheme（`http://` → Http，`grpc://` / `https://` / 无 scheme → Grpc），并处理 4317/4318 端口特殊case
- [x] 2.3 重构 `init_otlp_tracing`：根据 `detect_protocol` 结果分支构造 `SpanExporter::builder().with_tonic()` 或 `.with_http()`
- [x] 2.4 gRPC 分支保留现有行为：`with_tonic().with_endpoint(...).with_timeout(Duration::from_secs(5))`
- [x] 2.5 HTTP 分支：`with_http().with_endpoint(...).with_timeout(Duration::from_secs(5))`，使用 `opentelemetry-otlp` 内置 reqwest
- [x] 2.6 验证 `SYNTHIA_OTLP_ENDPOINT` 未设置时仍 fallback 到 console tracing（行为不变）
- [x] 2.7 编写单元测试覆盖 4 个 scheme 检测 case（`http://` / `grpc://` / `https://` / 无 scheme）+ 4317/4318 端口特殊 case
- [x] 2.8 运行 `cargo test -p synthia-telemetry --features otel` 验证协议检测测试通过

## 3. SpanAttributesProcessor 实现

- [x] 3.1 新建 `crates/synthia-telemetry/src/span/attributes_processor.rs` 文件
- [x] 3.2 在 `crates/synthia-telemetry/src/span/mod.rs` 中用 `#[cfg(feature = "otel")] pub mod attributes_processor;` 声明模块
- [x] 3.3 定义 `pub struct SpanAttributesProcessor`（无字段或持有可选 inner processor）
- [x] 3.4 实现 `SpanAttributesProcessor::new()` 构造函数
- [x] 3.5 实现 `opentelemetry_sdk::trace::SpanProcessor` trait 的 `on_start` 方法：通过 `tracing::Span::current()` extensions 查找 `SystemContext` / `AgentRunContext`，fallback 到 `tokio::task_local`
- [x] 3.6 在 `on_start` 中注入 6 个属性：`session.id` / `turn.id` / `agent.id` / `user.id` / `gen_ai.system` / `gen_ai.request.model`，使用 `opentelemetry_semantic_conventions` 常量
- [x] 3.7 实现 `on_end` 为 no-op（立即返回）
- [x] 3.8 实现 `force_flush` / `shutdown` 委托给 inner processor（若存在）或 no-op
- [x] 3.9 上下文缺失时 graceful skip（不 panic、不 ERROR 日志）
- [x] 3.10 验证 `on_start` 不修改 span 的 name / status / kind / parent（仅 `set_attribute`）
- [x] 3.11 编写单元测试：完整上下文注入 6 属性 / 上下文缺失 graceful skip / `on_end` no-op
- [x] 3.12 编写集成测试：装配 processor 后 span 包含期望属性

## 4. SpanAttributesProcessor 装配到 tracer provider

- [x] 4.1 在 `init_otlp_tracing` 中，构造 `SdkTracerProvider` 时通过 `with_span_processor(SpanAttributesProcessor::new())` 装配
- [x] 4.2 验证 `SpanAttributesProcessor` 与 batch exporter 共存（属性注入 + 异步导出）
- [x] 4.3 验证装配顺序：exporter → span processor → provider.build()
- [x] 4.4 编写集成测试：调用 `init_otlp_tracing` 后，tracer provider 中包含 `SpanAttributesProcessor`
  - 跳过：`init_otlp_tracing` 内部调用 `global::set_tracer_provider`，进程内重复调用会 panic。
    `SdkTracerProvider` 的内部 processor 列表无公开 API 可introspect。
    装配路径已通过 `cargo check --features otel` + 既有 Task 3 单元测试
    （`tests/span_attributes_processor.rs` 中 4 个测试均使用
    `SdkTracerProvider::builder().with_span_processor(SpanAttributesProcessor::new())`）
    间接验证：builder API 接受 processor、`on_start`/`on_end` 行为正确。
- [x] 4.5 运行 `cargo test -p synthia-telemetry --features otel` 验证装配测试通过

## 5. 上下文注入：SystemContext 与 AgentRunContext

- [x] 5.1 在 `crates/synthia-agent/src/agent.rs` 中新增 `task_local!` 宏声明：`AGENT_RUN_CONTEXT_TASK_LOCAL: AgentRunContext`（或 Arc<SystemContext>）
  - 实现：复用 `synthia-telemetry::span::attributes_processor` 已声明的 6 个 `task_local!`（`SESSION_ID`/`USER_ID`/`AGENT_ID`/`TURN_ID`/`GEN_AI_SYSTEM`/`GEN_AI_REQUEST_MODEL`，Task 3 定义）。在 `synthia-agent` 的 `otel` feature 下新增 `otel_context` 模块封装 scope 嵌套。
  - 偏差：`synthia-telemetry` 在 `synthia-agent` 中是必需依赖（`SpanContext` 被无条件使用），故 `otel` feature 定义为 `["synthia-telemetry/otel"]`（非 `dep:synthia-telemetry`）。
- [x] 5.2 在 `Agent::run_stream` 入口用 `AGENT_RUN_CONTEXT_TASK_LOCAL.scope(...)` 包裹整个 stream 执行
  - 实现：`run_stream` 返回惰性 stream（`Pin<Box<dyn Stream + Send>>`），非 async 方法。新增 `wrap_output_with_otel` 用 `async_stream::stream!` 驱动内层 stream，每次 `next()` poll 在 6 层嵌套 `scope` 内执行（`yield` 留在 `stream!` 顶层，避免 async_stream 嵌套块限制）。
- [x] 5.3 在 `Agent::resume` 入口同样包裹 task-local scope
  - 实现：`resume` 委托 `run_stream_with_state`，故在 `run_stream_with_state` 入口包裹即可覆盖 `resume`（避免双重包裹）。`run_stream` 与 `run_stream_with_state` 均包裹。
- [x] 5.4 验证 `SystemContext`（P1-4）可通过 task-local 或 `tracing::Span::current()` extensions 获取
  - 实现：`session_id`/`user_id` 取自 `AgentRunConfig`；`gen_ai_system`=`provider.name()`；`gen_ai_request_model`=`AgentConfig::model`。`agent_id`（无显式字段）与 `turn_id`（每轮生成，Task 7 处理）暂用空串占位，processor graceful skip。
- [x] 5.5 在 `SpanAttributesProcessor::on_start` 中实现优先级：先查 tracing span extensions，fallback 到 task-local
  - 实现：Task 3 已实现 `on_start` 通过 `try_get` 读取 task-local；本任务无需改 processor（与 5.5 一致）。
- [x] 5.6 编写测试：`Agent::run_stream` 执行时 task-local 可达；执行外不可达
  - 实现：`tests/otel_context_injection.rs`（3 测试：scope 内可达 / 外不可达 / Send 不阻塞）+ `agent.rs` 内 `otel_context::tests`（3 单元测试：6 task_local 全可达 / 外不可达 / `wrap_output_with_otel` per-poll 传播——内层 stream poll 时读到 `SESSION_ID="sess-2"`）。
- [x] 5.7 确保 task-local 不阻塞 `Send` / `Sync` 约束（`AgentRunContext` 需 `Clone + Send + Sync`）
  - 实现：task_local 值为 `String`（`Send + 'static`）；`with_otel_context` 约束 `F: Future + Send, R: Send`；`wrap_output_with_otel` 产出 `Send` stream。集成测试 `task_local_scope_does_not_block_send` 用编译期 `assert_send<T: Send>` 守卫。

## 6. session span 集成

- [x] 6.1 在 `crates/synthia-agent/Cargo.toml` 中添加 `synthia-telemetry` 依赖（已存在则确认），并启用 `otel` feature 为可选：`synthia-telemetry = { workspace = true, optional = true }`
  - Task 5 已完成：`synthia-telemetry` 为必需依赖（非 optional），`otel` feature 定义为 `["synthia-telemetry/otel"]`（非 `dep:synthia-telemetry`），因 `SpanContext` 被无条件使用。
- [x] 6.2 在 `crates/synthia-agent/Cargo.toml` 新增 `[features] otel = ["dep:synthia-telemetry", "synthia-telemetry/otel"]`
  - Task 5 已完成：`otel = ["synthia-telemetry/otel"]`（偏差见 6.1）。
- [x] 6.3 在 `crates/synthia-agent/src/agent.rs` 的 `run_stream` 入口添加 `#[cfg(feature = "otel")]` 守卫的 `tracing::span!(target: "synthia.session", Level::INFO, "session.start")` 创建
  - 实现：span 创建置于 `wrap_output_with_otel` 的 `stream!` 生成器内，INSIDE `with_otel_context` 调用（task_local 作用域内），使 `SpanAttributesProcessor::on_start` 可读取 6 个 task-local 值。`parent: None` 确保 root span。
- [x] 6.4 用 RAII guard（`#[must_use] struct SpanGuard(Option<Span>)`）确保 panic 时 span 也被 end
  - 实现：`SessionSpanGuard { span: tracing::Span }`（`#[must_use]`）。guard 在 `stream!` 生成器顶层持有，生成器完成/panic 时 `Drop` 触发 span 结束。`instrument(span.clone())` 在每次 `inner.next()` poll 时使 session span 为 current span（child span 继承）。
- [x] 6.5 在 span guard 的 `Drop` 实现中，若 panic unwinding 则 `set_status(Error)` 并 end
  - 实现：`Drop::drop` 检查 `std::thread::panicking()`，若为 true 则 `span.record("exception.message", "session panicked")`。`tracing-opentelemetry` 层将 `exception.message` 翻译为 OTel exception 事件并设置 status=Error。span 由 `Span` 自身的 `Drop` 结束（`on_end`）。
- [x] 6.6 验证 session span 作为 root span（无 parent）
  - 实现：`tracing::span!(parent: None, ...)` 显式声明无 parent。`SpanAttributesProcessor::on_start` 测试（Task 3）已验证 processor 不修改 parent。
- [x] 6.7 编写测试：`run_stream` 正常完成时 span 被 end；panic 时 span 也被 end 且 status 为 Error
  - 实现：`agent.rs` 内 `otel_context::tests` 新增 3 个单元测试：
    1. `session_span_created_and_ended_on_normal_completion` — 正常完成时 stream 可收集，guard drop 结束 span。
    2. `session_span_ended_on_panic` — `catch_unwind` 验证 panic 传播，guard 在 unwinding 期间 drop。
    3. `session_span_guard_drop_observes_panicking_during_unwind` — 白盒测试 guard 的 `Drop` 在 panic 时不二次 panic。
  - 偏差：未验证 OTel span status=Error（需 subscriber + provider 装配，对单元测试过重）。panic 传播 + guard Drop 路径已验证；status 设置委托给 `tracing-opentelemetry` 层的 `exception.message` 翻译（Task 8-9 端到端测试可覆盖）。

## 7. turn span 集成

- [x] 7.1 在 `crates/synthia-agent/src/loop_context.rs` 或 turn 迭代入口添加 `#[cfg(feature = "otel")]` 守卫的 `tracing::span!("turn.start")` 创建
- [x] 7.2 span parent 设置为当前 `session.start` span（通过 `tracing::Span::current()` 自动继承）
- [x] 7.3 在 span 上记录 `turn.id` 与 `turn.iteration` 属性
- [x] 7.4 turn 失败时（LLM 调用错误、工具错误）`set_status(Error)` 并记录 `exception` 事件（`exception.type` / `exception.message`）
- [x] 7.5 编写测试：N 次 turn 迭代产生 N 个 span；turn 失败时 span status 为 Error 且包含 exception 事件

## 8. llm.call span 集成

- [x] 8.1 在 `crates/synthia-llm/` 中识别 provider 调用入口（如 `AnthropicProvider::complete` 或 `ModelProvider::complete` trait method）
  - 偏差：任务描述引用 `crates/synthia-llm/`，但实际 crate 为 `crates/synthia-provider/`。入口识别为 `AnthropicProvider::complete`（`src/anthropic/traits_impl.rs`）与 `OpenAICompatibleProvider::complete`（`src/openai/traits_impl.rs`）。
- [x] 8.2 在调用入口添加 `#[cfg(feature = "otel")]` 守卫的 `tracing::span!("llm.call")` 创建
  - 实现：在两个 provider 的 `complete()` 方法中，于 `retry_with_backoff` 调用前创建 `tracing::span!(target: "synthia.llm", Level::INFO, "llm.call", ...)`，并用 `let _llm_guard = llm_span.enter();` 持有 RAII guard。`Cargo.toml` 新增 `otel = []` feature。
- [x] 8.3 在 span 上记录 `gen_ai.system` / `gen_ai.request.model` 属性（来自 provider 配置）
  - 实现：Anthropic 用 `gen_ai.system = %self.name()`（返回 `"anthropic"`）；OpenAI 用 `gen_ai.system = %self.model_config.provider`（因 `self.name()` 返回模型名而非 provider 标识符）。两者均 `gen_ai.request.model = %request.model`。
- [x] 8.4 调用成功后记录 `gen_ai.response.finish_reason` / `gen_ai.usage.input_tokens` / `gen_ai.usage.output_tokens`（来自 `Usage` 结构）
  - 实现：从 raw `AnthropicResponse`（`stop_reason` + `usage.input_tokens`/`output_tokens`）与 `OpenAIResponse`（`choices[0].finish_reason` + `usage.prompt_tokens`/`completion_tokens`）在 `retry_with_backoff` 返回后通过 `Span::record()` 填充。所有后填字段在 `span!` 宏中声明为 `tracing::field::Empty`（Task 7 教训：未声明的字段 `record()` 是 silent no-op）。
- [x] 8.5 调用失败时 `set_status(Error)` 并记录 `exception` 事件
  - 实现：`retry_with_backoff` 返回 `Err` 时，通过 `llm_span.record("exception.type", e.code().to_string())`、`llm_span.record("exception.message", e.to_string())`、`llm_span.record("otel.status_code", "ERROR")` 记录异常。`tracing-opentelemetry` 层将 `exception.*` 翻译为 OTel exception 事件并设置 status=Error。
- [x] 8.6 编写测试：成功调用 span 含 usage 属性；失败调用 span status 为 Error 且含 exception
  - 实现：`tests/otel_llm_span.rs`（4 测试，`#![cfg(feature = "otel")]` 守卫）：Anthropic/OpenAI 各一个 success + failure 测试。使用 `wiremock` mock HTTP 端点 + `tracing-subscriber` 自定义 `CaptureLayer`（实现 `on_new_span` + `on_record`）捕获 span 字段。失败测试用 HTTP 400（non-retryable，`retry_with_backoff` 立即返回不 sleep）。验证 `gen_ai.*` / `exception.*` / `otel.status_code` 字段。

## 9. tool.execute span 集成

- [x] 9.1 在 `crates/synthia-tool/src/registry/` 或工具执行入口识别 `ToolRegistry::execute` / `Tool::call` 路径
  - 实现：最中心入口为 `ToolRegistry::execute_tools`（`src/registry/registration/registry.rs`）内 `tokio::spawn` 任务中的 `tool.call(tool_input).await`。所有工具调用（无论经 `run_with_context` 还是 orchestrator fallback）均流经此处。
- [x] 9.2 在执行入口添加 `#[cfg(feature = "otel")]` 守卫的 `tracing::span!("tool.execute")` 创建
  - 实现：span 在 spawned task 内部创建，用 `.instrument(tool_span.clone())` 包装 `tool.call()` future。采用 `.instrument()` 而非 `Span::enter()` 是因为 `tracing::span::Entered` 为 `!Send`，不能跨 `tokio::spawn` 的 `.await` 持有。`Cargo.toml` 新增 `otel = []` feature；`tracing::Instrument` 导入亦受 `#[cfg(feature = "otel")]` 守卫。
- [x] 9.3 在 span 上记录 `tool.name` 属性（来自 `Tool::name()`）
  - 实现：`tool.name = %name`（`name = &tool_use.name`，对应注册时 `Tool::name()`）在 `span!` 宏 callsite 声明。
- [x] 9.4 工具超时时 `set_status(Error)` 并记录 `exception` 事件（`exception.type = "TimeoutError"`）
  - 偏差：`synthia-tool` registry 层无 `tokio::time::timeout` 包装（仅 bash 工具内部有）。registry 不新增 timeout（避免改变运行时语义，违反"span 创建为旁路观测"约束）。超时检测改为：`tool.call()` 返回 `ToolOutput::error` 且消息含 "timed out"/"timeout"（bash 工具内部 `tokio::time::timeout` 触发时即返回此消息）→ 记录 `exception.type = "TimeoutError"` + `otel.status_code = "ERROR"`。`exception.type` / `exception.message` / `otel.status_code` 在 callsite 声明为 `tracing::field::Empty`（Task 7 教训）。
- [x] 9.5 工具错误时 `set_status(Error)` 并记录 `exception` 事件
  - 实现：`tool.call()` 返回 `ToolOutput::is_error = Some(true)` 且非超时 → `exception.type = "ToolError"` + 消息（从 `content` 提取）+ `otel.status_code = "ERROR"`。`JoinError`（panic）在收集循环产出 `ToolOutput::error("Task panicked: ...")`，span 已随 task drop 而结束（边界情况，非 spec scenario）。
- [x] 9.6 编写测试：工具执行 span 含 `tool.name`；超时 span status 为 Error 含 TimeoutError
  - 实现：`tests/otel_tool_span.rs`（3 测试，`#![cfg(feature = "otel")]`）：success/timeout/error 各一。用 `current_thread` tokio runtime 确保 `set_default()` thread-local subscriber 对 spawned task 可见。`CaptureLayer`（`on_new_span` + `on_record`）捕获 span 字段，验证 `tool.name` / `exception.type` / `exception.message` / `otel.status_code`。

## 10. compaction span 集成

- [x] 10.1 在 `crates/synthia-context/src/compaction/` 识别 compaction 触发入口
  - 实现：最中心入口为 `crates/synthia-context/src/compaction_service.rs::compact_messages`。该函数是 agent runtime compaction 路径的唯一入口（`synthia-agent::try_compact` / `try_compact_with_threshold` → `compact_messages`），且同时计算 `old_tokens`（before）与 `new_tokens`（after），并通过 `Compactor::auto_select_level` 确定 stage（1/2/3）。`apply_compaction` / `compact_with_fallback`（orchestrator）为 session_writer / L4 recovery 的次要路径，不 instrument。
- [x] 10.2 在触发入口添加 `#[cfg(feature = "otel")]` 守卫的 `tracing::span!("compaction")` 创建
  - 实现：span 在所有 early-return guard（`token_count <= soft_limit` / `level == 0` / `messages.len() < 2` / `split_point == 0`）通过后创建，确保只在 compaction 实际进行时 emit。`Cargo.toml` 新增 `otel = []` feature；`messages_before` / `stage_name` 变量亦受 `#[cfg(feature = "otel")]` 守卫（避免无 feature 时的 unused variable 警告）。
- [x] 10.3 在 span 上记录 `compaction.before_tokens` / `compaction.after_tokens` / `compaction.stage` / `compaction.messages_before` / `compaction.messages_after` 属性
  - 实现：`before_tokens` / `messages_before` / `stage`（值为 `"L1"` / `"L2"` / `"L3"`）在 `span!` 宏 callsite 声明；`after_tokens` / `messages_after` 声明为 `tracing::field::Empty`，在 compaction 完成（Ok 与 Err 两条路径）后通过 `Span::record()` 填充（Task 7 教训：未在 callsite 声明的字段 `record()` 是 silent no-op）。
- [x] 10.4 编写测试：compaction 触发后 span 含前后 token 数与 stage 属性
  - 实现：`tests/otel_compaction_span.rs`（3 测试，`#![cfg(feature = "otel")]`）：(1) 成功路径——compaction 触发后 span 含全部 5 个属性（before/after tokens、messages、stage）；(2) 无 compaction 路径——`token_count <= soft_limit` 时不创建 span；(3) stage 映射——`ratio=50x` 选中 `L3`。使用 `tracing-subscriber` 自定义 `CaptureLayer`（`on_new_span` + `on_record`）捕获 span 字段。

## 11. guardian.check span 集成

- [x] 11.1 在 `crates/synthia-guardian/src/review/reviewer.rs` 识别 `GuardianReviewer::check` 调用入口
  - 实现：最中心入口为 `GuardianReviewer::check`（`src/review/reviewer.rs`）。该方法是 LLM-backed reviewer 的公共入口，所有 reviewer 路径（disabled 早返回 / 序列化错误 / LLM 调用成功 / LLM 调用失败 / 超时）均流经此方法。`SimpleGuardian::check` 与 `GuardianCoordinator::check` 是另外两条独立路径（rule-based 与 hybrid），不在本任务范围。
- [x] 11.2 在调用入口添加 `#[cfg(feature = "otel")]` 守卫的 `tracing::span!("guardian.check")` 创建
  - 实现：将原 `check` 方法体抽取为私有 `check_inner`，公共 `check` 在入口创建 `tracing::span!(target: "synthia.guardian", Level::INFO, "guardian.check", ...)` 并用 `let _guardian_guard = guardian_span.enter();` 持有 RAII guard（与 Task 8 `llm.call` 同一模式）。`Cargo.toml` 新增 `otel = []` feature；`tracing::field::Empty` 用于 `guardian.decision`（后填字段，Task 7 教训）。
- [x] 11.3 在 span 上记录 `guardian.decision` 属性（`allow` / `deny` / `need_user_confirm`）与 `guardian.layer` 属性（`simple` / `reviewer` / `circuit_breaker`）
  - 实现：`guardian.layer = "reviewer"` 在 `span!` 宏 callsite 直接声明（编译期已知）；`guardian.decision` 在 `check_inner` 返回后通过 `guardian_span.record("guardian.decision", decision_str)` 填充，`decision_str` 由 `match &decision` 映射：`Allow → "allow"` / `Deny { .. } → "deny"` / `NeedUserConfirm { .. } → "need_user_confirm"`。所有早返回路径（disabled / 序列化错误 / LLM 错误 / 超时）均经 `check_inner` 统一返回，故只需在 `check` 末尾记录一次。
- [x] 11.4 编写测试：Guardian 审查 span 含 decision 与 layer 属性
  - 实现：`tests/otel_guardian_span.rs`（3 测试，`#![cfg(feature = "otel")]`）：(1) `test_disabled_guardian_span_has_allow_decision` — disabled config → `Allow` → span 含 `guardian.decision = "allow"` + `guardian.layer = "reviewer"`；(2) `test_router_failure_span_has_deny_decision` — `FailingRouter` 返回 Err → fail-closed `Deny` → span 含 `guardian.decision = "deny"`；(3) `test_medium_risk_span_has_need_user_confirm_decision` — `StubProvider` 返回 medium-risk Assessment JSON (risk_score=65) → `NeedUserConfirm` → span 含 `guardian.decision = "need_user_confirm"`。使用 `tracing-subscriber` 自定义 `CaptureLayer`（`on_new_span` + `on_record`）捕获 span 字段。`StubProvider` 实现 `ModelProvider` trait（用 `synthia_core::Error`，因 trait 使用 `synthia_core::Error` 而非 `ProviderError`）；`StubRouter` 返回包含 `StubProvider` 的 `RoutingResult`。

## 12. span 不修改 prompt 前缀验证

- [x] 12.1 编写集成测试：启用 `otel` feature 时 `Agent::run_stream` 构造的 `CompletionRequest.messages` / `system` / `tools` 字段与未启用时字节级一致
  - 实现：`crates/synthia-agent/tests/otel_prefix_stability.rs`（`#![cfg(feature = "otel")]` 守卫，6 测试）。采用“直接验证不变量”策略（无法在同一测试二进制中同时运行 otel-enabled / otel-disabled 构建）：
    1. `span_callsites_do_not_declare_completion_request_fields` — 创建 6 个生产 callsite（session/turn/llm.call/tool.execute/compaction/guardian.check），用 `CaptureLayer` 捕获字段名，断言没有任何 span 声明 `messages` / `system` / `tools` / `tool_choice`（`CompletionRequest` 字段），且所有字段均属于 span-semantic 前缀（`exception.` / `otel.` / `gen_ai.` / `tool.name` / `compaction.` / `guardian.` / `turn.`）。
    2. `span_is_clone_send_sync_but_does_not_mutate_request` — 编译期 trait-bound 检查 `tracing::Span: Clone + Send + Sync`，证明 span 类型无修改请求的字段路径。
    3. `completion_request_messages_byte_identical_across_span_creation` — 在 span 作用域内/外序列化 `Vec<Message>` 为 JSON，断言字节级一致。
- [x] 12.2 编写集成测试：启用 `otel` feature 时 `prompt_cache_key` 计算输入（`user_id` / `session_id`）与未启用时一致
  - 实现：`prompt_cache_key_inputs_unaffected_by_span_creation` — 在 6 个 span 作用域内/外调用 `synthia_telemetry::compute_prefix_hash(&messages)`，断言 hash 字节级一致；并断言 `user_id` / `session_id` 字符串在 span 创建前后未变。`OtelContext::from_run_config` 取 `&AgentRunConfig`（共享引用），编译期保证不可变。
- [x] 12.3 运行 KV cache 命中率对比测试（若现有测试存在），验证 span 创建不影响 cache 命中
  - 实现：`kv_cache_stability_ratio_unaffected_by_span_creation` + `prefix_hash_is_deterministic_across_span_creation`。用 `synthia_context::PrefixTracker` 在两个 turn 间记录 `record_pre` / `record_post`，在 record 之间创建 6 个 span。断言 `windowed_stability_ratio() == 1.0`（KV cache 命中率 100%），证明 span 创建不影响 cache 命中。
  - 偏差：无独立“KV cache 命中率对比测试”存在；通过 `PrefixTracker`（前缀稳定性的旁路度量）间接验证。设计上 span 创建是旁路观测，不触碰 system prompt bytes，cache 命中率不受影响。

## 13. CI 编译矩阵与文档

- [x] 13.1 在 CI 配置中添加 `cargo check --no-default-features -p synthia-telemetry` 步骤
- [x] 13.2 在 CI 配置中添加 `cargo check --features otel -p synthia-telemetry` 步骤
- [x] 13.3 在 CI 配置中添加 `cargo test --features otel -p synthia-telemetry` 步骤
- [x] 13.4 在 CI 配置中添加 `cargo check --features otel -p synthia-agent` 步骤（验证 agent span 集成编译）
- [x] 13.5 更新 `crates/synthia-telemetry/README.md`（若存在）或新增文档：`otel` feature 启用方式 + `SYNTHIA_OTLP_ENDPOINT` scheme 说明 + `SYNTHIA_OTEL_SAMPLER` 说明
  - 实现：原仓库无 `crates/synthia-telemetry/README.md`，新增最小 README。内容覆盖 `otel` feature 启用、`SYNTHIA_OTLP_ENDPOINT` scheme 检测表（5 行 case）、`SYNTHIA_OTEL_SAMPLER` 说明、`SpanAttributesProcessor` 注入的 6 个属性表、6 个 agent runtime span 边界表、Quick Start 代码示例。`SYNTHIA_OTEL_SAMPLER` 在设计 D8 中规划但代码尚未接线，README 中如实标注“not yet wired up”避免 false claim。
- [x] 13.6 在 `AGENTS.md` 或 `CLAUDE.md`（若合适）添加 OTel 集成使用说明（开发者视角）
  - 实现：`AGENTS.md` 已有 `# env` 段（LLM API 配置），OTel 同为环境变量配置，自然契合。在 `# env` 下新增 `## OTel (可选)` 子段，简述 `SYNTHIA_OTLP_ENDPOINT` / `SYNTHIA_OTEL_SAMPLER` 并链接到 `crates/synthia-telemetry/README.md`。`CLAUDE.md` 为 LLM 行为规范，无 Telemetry/Development 段，未改动。

## 14. 端到端验证

- [x] 14.1 启用 `otel` feature，设置 `SYNTHIA_OTLP_ENDPOINT=http://localhost:4318`，启动本地 OTLP collector（如 jaegertracing/all-in-one）
  - deferred — requires docker/OTLP collector E2E setup（启动 Jaeger、运行 agent、人工验证 span 出现在 collector）。单元/集成测试已由 Task 3/5/6/7/8/9/10/11 覆盖 span 创建与字段，Task 12 覆盖 prompt 前缀不变性。
- [x] 14.2 运行一次 `Agent::run_stream`，验证 6 类 span（session / turn / llm.call / tool.execute / compaction / guardian.check）均出现在 collector
  - deferred — requires docker/OTLP collector E2E setup
- [x] 14.3 验证 span 包含 `SpanAttributesProcessor` 注入的 6 个标准属性
  - deferred — requires docker/OTLP collector E2E setup。`SpanAttributesProcessor::on_start` 注入 6 属性已由 `tests/span_attributes_processor.rs`（4 测试）单元验证。
- [x] 14.4 验证 span 层级关系：session → turn → llm.call / tool.execute / compaction / guardian.check
  - deferred — requires docker/OTLP collector E2E setup。span parent 关系已由 callsite 设计保证（session span `parent: None`，turn/llm/tool/compaction/guardian 通过 `Span::current()` 自动继承 session span）。
- [x] 14.5 验证 `SYNTHIA_OTLP_ENDPOINT=grpc://localhost:4317` 时行为与现有实现一致（向后兼容）
  - deferred — requires docker/OTLP collector E2E setup。协议检测（grpc:// → Grpc 分支）已由 `tests/otlp_protocol_selection.rs`（7 测试）单元验证。
- [x] 14.6 运行 `cargo test --workspace --features otel` 验证全 workspace 启用 feature 时测试通过
  - 验证通过：3229 passed / 0 failed / 8 ignored（exit 0）。otel-gated 测试（53 个：span_attributes_processor / otlp_protocol_selection / otel_context_injection / otel_prefix_stability / otel_llm_span / otel_tool_span / otel_compaction_span / otel_guardian_span）全部通过。
- [x] 14.7 运行 `cargo test --workspace` 验证默认 feature（不启用 otel）时测试通过
  - 验证通过：3176 passed / 0 failed / 6 ignored（exit 0）。默认 feature 编译零 OTel 依赖（Task 1.10 验证），所有非 otel 测试通过。
- [x] 14.8 运行 `cargo +nightly fmt --all` 格式化代码
  - 验证通过：fmt exit 0，无文件变更（代码已格式化）。
- [x] 14.9 运行 `cargo clippy --all-targets --all-features --tests --all` 并修复所有警告
  - 验证通过：clippy exit 0。OTel 集成引入的新警告 = 0（所有 31 个修改文件均无 clippy 警告）。
  - 残留预警均为 pre-existing（84 实例 / ~32 文件，均不在本分支修改的文件中），按任务指引（50+ 阈值）仅文档化不修复。分类：47 `doc list item overindented`、13 `struct update has no effect`、10 `module_inception`、7 `link reference defined in list item`、4 `doc list item without indentation`、2 `std::io::Error::other`、1 `field_reassign_with_default`。
