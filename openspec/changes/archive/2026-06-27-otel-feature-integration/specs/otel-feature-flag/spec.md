## ADDED Requirements

### Requirement: OTel 依赖 SHALL 置于 `otel` cargo feature 下

`synthia-telemetry` crate 的 `Cargo.toml` SHALL 新增 `otel` cargo feature（默认禁用）。`opentelemetry` / `opentelemetry-otlp` / `tracing-opentelemetry` / `opentelemetry_sdk` / `opentelemetry-semantic-conventions` 依赖 MUST 标记为 `optional = true` 并通过 `[features] otel = [...]` 引入。所有引用上述依赖的代码 MUST 置于 `#[cfg(feature = "otel")]` 守卫下。

#### Scenario: 默认编译无 OTel 依赖

- **WHEN** 执行 `cargo check -p synthia-telemetry`（不启用 `otel` feature）
- **THEN** 编译 SHALL 成功，且 `Cargo.lock` 中 SHALL 不包含 `opentelemetry` / `opentelemetry-otlp` / `tracing-opentelemetry` / `opentelemetry_sdk` / `opentelemetry-semantic-conventions`

#### Scenario: 启用 otel feature 编译包含 OTel 依赖

- **WHEN** 执行 `cargo check -p synthia-telemetry --features otel`
- **THEN** 编译 SHALL 成功，且 OTel 相关代码（`init_otlp_tracing`、`SpanAttributesProcessor` 等）SHALL 被编译进二进制

#### Scenario: OTel 代码引用受 cfg 守卫

- **WHEN** 在 `synthia-telemetry` 源码中搜索 `opentelemetry::` / `opentelemetry_otlp::` / `tracing_opentelemetry::` 引用
- **THEN** 每处引用 SHALL 位于 `#[cfg(feature = "otel")]` 守卫的模块、函数或 impl 块内

---

### Requirement: 无 `otel` feature 时 `init_tracing` SHALL 退化为 console tracing

当 `synthia-telemetry` 在未启用 `otel` feature 时编译，`init_tracing` 函数 SHALL 调用 `init_console_tracing` 并返回 `Ok(TracerInitResult::Console)`，不引用任何 OTel API。`TracerInitResult::Otlp` variant MUST 在 `#[cfg(feature = "otel")]` 下，无 feature 时该 variant 不存在。

#### Scenario: 无 feature 时 init_tracing 返回 Console

- **WHEN** 在未启用 `otel` feature 的构建中调用 `init_tracing(&TelemetryConfig::default())`
- **THEN** 返回值 SHALL 为 `Ok(TracerInitResult::Console)`，且 `tracing_subscriber` 的 fmt layer SHALL 被初始化

#### Scenario: TracerInitResult 在无 feature 时无 Otlp variant

- **WHEN** 在未启用 `otel` feature 的构建中检查 `TracerInitResult` 枚举
- **THEN** 该枚举 SHALL 仅包含 `Console` variant；`Otlp` variant SHALL 不存在（受 `#[cfg(feature = "otel")]` 守卫）

---

### Requirement: 启用 `otel` feature 时行为 SHALL 与当前实现一致

当 `otel` feature 启用时，`init_otlp_tracing` 的行为 SHALL 与本 change 前的现有实现完全一致（gRPC tonic exporter，endpoint 来自 `SYNTHIA_OTLP_ENDPOINT`，未设置时 fallback 到 console）。

#### Scenario: otel feature 启用且无 endpoint 时 fallback console

- **WHEN** 启用 `otel` feature 且 `SYNTHIA_OTLP_ENDPOINT` 环境变量未设置或为空
- **THEN** `init_tracing` SHALL 返回 `Ok(TracerInitResult::Console)`，且 console tracing 被初始化

#### Scenario: otel feature 启用且有 endpoint 时初始化 OTLP

- **WHEN** 启用 `otel` feature 且 `SYNTHIA_OTLP_ENDPOINT` 设置为有效 endpoint（如 `grpc://localhost:4317`）
- **THEN** `init_tracing` SHALL 返回 `Ok(TracerInitResult::Otlp(provider))`，且 OTLP gRPC exporter SHALL 被配置
