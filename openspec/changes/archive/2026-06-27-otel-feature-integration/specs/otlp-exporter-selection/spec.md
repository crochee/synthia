## ADDED Requirements

### Requirement: OTLP exporter SHALL 按 endpoint scheme 自动选择协议

`init_otlp_tracing` 函数 SHALL 解析 `SYNTHIA_OTLP_ENDPOINT` 环境变量的 URL scheme：
- `http://` scheme → 构造 HTTP exporter（基于 `opentelemetry-otlp` 内置 reqwest）
- `grpc://` 或 `https://` scheme → 构造 gRPC exporter（tonic）
- 无 scheme 或其他 scheme → 默认 gRPC exporter（向后兼容现有行为）

#### Scenario: http scheme 触发 HTTP exporter

- **WHEN** `SYNTHIA_OTLP_ENDPOINT` 设置为 `http://localhost:4318/v1/traces`
- **THEN** `init_otlp_tracing` SHALL 构造 `SpanExporter::builder().with_http()` 导出器，并使用该 endpoint

#### Scenario: grpc scheme 触发 gRPC exporter

- **WHEN** `SYNTHIA_OTLP_ENDPOINT` 设置为 `grpc://localhost:4317`
- **THEN** `init_otlp_tracing` SHALL 构造 `SpanExporter::builder().with_tonic()` 导出器，并使用该 endpoint

#### Scenario: https scheme 触发 gRPC exporter（TLS）

- **WHEN** `SYNTHIA_OTLP_ENDPOINT` 设置为 `https://collector.example.com:4317`
- **THEN** `init_otlp_tracing` SHALL 构造 gRPC exporter（向后兼容现有行为）

#### Scenario: 无 scheme 默认 gRPC

- **WHEN** `SYNTHIA_OTLP_ENDPOINT` 设置为 `localhost:4317`（无 scheme 前缀）
- **THEN** `init_otlp_tracing` SHALL 默认构造 gRPC exporter（向后兼容现有行为）

---

### Requirement: HTTP exporter SHALL 使用 `opentelemetry-otlp` 内置 reqwest 特性

HTTP exporter MUST 通过 `SpanExporter::builder().with_http()` 构造，不引入独立的 `reqwest` 或 `hyper` 依赖。`opentelemetry-otlp` crate 的 `http-proto` feature MUST 在 `otel` feature 中启用。

#### Scenario: 不引入独立 reqwest 依赖

- **WHEN** 检查 `crates/synthia-telemetry/Cargo.toml` 的 `[dependencies]` 段
- **THEN** SHALL 不存在独立的 `reqwest` 或 `hyper` 依赖条目；HTTP 支持通过 `opentelemetry-otlp` 的 feature 实现

#### Scenario: otel feature 启用 http-proto

- **WHEN** 检查 `synthia-telemetry` 的 `otel` feature 定义
- **THEN** 该 feature SHALL 包含 `opentelemetry-otlp/http-proto`（或等价配置）以启用 HTTP exporter 能力

---

### Requirement: exporter SHALL 配置 5 秒超时与批处理导出

无论 gRPC 或 HTTP exporter，均 MUST 配置 `with_timeout(Duration::from_secs(5))`。tracer provider MUST 使用 `with_batch_exporter`（批处理导出，默认 5s interval / 512 batch size）而非简单导出，以避免阻塞 agent 主循环。

#### Scenario: HTTP exporter 配置 5 秒超时

- **WHEN** HTTP exporter 被构造
- **THEN** 该 exporter SHALL 配置 `with_timeout(Duration::from_secs(5))`

#### Scenario: 批处理导出器装配

- **WHEN** `init_otlp_tracing` 构造 `SdkTracerProvider`
- **THEN** 该 provider SHALL 通过 `with_batch_exporter(exporter, runtime::Tokio)` 装配批处理导出

---

### Requirement: `SYNTHIA_OTLP_ENDPOINT` 行为 SHALL 向后兼容

现有 `SYNTHIA_OTLP_ENDPOINT` 环境变量的行为 MUST 保留：未设置时 fallback 到 console tracing；设置为 gRPC endpoint 时走 gRPC exporter。本 change 新增 scheme 检测逻辑，但不破坏现有用法。

#### Scenario: 环境变量未设置时 fallback console

- **WHEN** `SYNTHIA_OTLP_ENDPOINT` 环境变量未设置或为空字符串
- **THEN** `init_tracing` SHALL 返回 `Ok(TracerInitResult::Console)`，不尝试构造任何 exporter

#### Scenario: 现有 gRPC endpoint 行为不变

- **WHEN** `SYNTHIA_OTLP_ENDPOINT` 设置为现有的 gRPC endpoint 值（如 `http://localhost:4317`，注意：现有代码不区分 scheme，统一走 gRPC）
- **THEN** 出于严格向后兼容，当 endpoint 以 `http://` 开头但端口为 4317（gRPC 标准端口）时，SHALL 走 gRPC exporter；端口为 4318（HTTP 标准端口）时，SHALL 走 HTTP exporter；其他 `http://` endpoint SHALL 走 HTTP exporter

#### Scenario: 显式 grpc:// 强制 gRPC

- **WHEN** `SYNTHIA_OTLP_ENDPOINT` 设置为 `grpc://collector:4317`
- **THEN** 无论端口如何，SHALL 强制走 gRPC exporter
