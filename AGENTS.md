## Browser Automation

Use `agent-browser` for web automation. Run `agent-browser --help` for all commands.

Core workflow:

1. `agent-browser open <url>` - Navigate to page
2. `agent-browser snapshot -i` - Get interactive elements with refs (@e1, @e2)
3. `agent-browser click @e1` / `fill @e2 "text"` - Interact using refs
4. Re-snapshot after page changes

<br />

# Project Rules (MUST READ before starting any task)

Before starting any task, read the following files:

- `.trae/rules/agent_rule.md` — P1-P10 design principles (prefix consistency > append-only > interruptibility > distrust LLM > progressive degradation > lazy loading > recency anchoring > no information loss > observability > file as memory)
- `.trae/rules/rust.md` — Rust coding conventions (`cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features --tests --all`)
- `CLAUDE.md` — Behavioral guidelines (think before coding, simplicity first, surgical changes, goal-driven execution, skills, task-centric execution)

Key constraint: Do not proactively ask questions; explore the best path independently.

# env

真实llm api配置在.env中

## OTel (可选)

启用 `otel` cargo feature 后，通过环境变量配置 OpenTelemetry tracing：

- `SYNTHIA_OTLP_ENDPOINT` — OTLP collector 地址，scheme 自动选择 gRPC/HTTP
  （`grpc://` / `https://` / 无 scheme → gRPC；`http://` → HTTP，4317 端口例外走 gRPC）
  未设置时退化为 console tracing。
- `SYNTHIA_OTEL_SAMPLER` — 采样器覆盖（`always_on` / `always_off` / `trace_id_ratio:0.1`），
  默认 `ParentBased(AlwaysOn)`。设置后包裹 `ParentBased` 以兼容父 trace 采样决策。

详见 [crates/synthia-telemetry/README.md](crates/synthia-telemetry/README.md)。

# 代码同步规范
- 不主动push代码到远程仓库
