# synthia-harness

The harness: the core ReAct loop, `Agent` / `ToolInterceptor` / strategy
seams, `AgentRegistry`, type definitions.

The single place `ReActAgent::new(provider, registry)` turns a model +
a tool set into a `stream`-based `AgentEvent` flow.

> 详细 seam 与公开 API 见 [`synthia-harness/src/lib.rs`](src/lib.rs)；
> 7 大组件的可替换点见 [`SEAMS.md`](../../SEAMS.md)。

## 用法

```rust
use synthia_harness::{ReActAgent, Agent};

let agent = ReActAgent::new(provider, registry)
    .with_steering(steering);

let stream = agent.run("hello, what's the weather?").await?;
while let Some(event) = stream.next().await { /* ... */ }
```

## CI 契约

- `cargo test -p synthia-harness --lib` 绿；
- **形状预算**：生产代码函数 ≤ 100 行、嵌套 ≤ 4 层
  （`make check-harness-shape`）；
- 不引入运行时类型到 lib 公共 API
  （`make check-public-api-runtime`）。

## Examples（13 个，全部离线 + 跨 crate）

| Example | Shows |
| :--- | :--- |
| `assemble_with_builder` | 7 大组件一行链式装配（推荐起点） |
| `assemble_from_scratch` | 同一 agent 手写每个组件的形状 |
| `strategy_swap` | 同一 ReAct agent 跑 ReAct / ChainOfThought / BestOfN 三种策略 |
| `assemble_with_provider_profile` | `ProviderProfile` 替代散 setter |
| `assemble_with_skills` | `SkillRegistry` 与工具绑定 |
| `assemble_with_tier_steering` | `ModelTier::Small` 自动缩工具面 |
| `best_of_n_judge` | `BestOfNStrategy` + `LlmJudgeScorer` |
| `output_transformers` | commit-path transformer + retrieval round trip |
| `tool_surface_policy` | `ToolSurfacePolicy` 组激活 / `max_visible` cap |
| `run_inbox_steering` | `RunInbox` / `RunInboxHandle` 中途 steer |
| `runtime_context` | `prompt::RuntimeContext` 提供方 prefix 缓存钩子 |
| `token_meter` | `TokenMeter` 量测 / 锚点压缩 |
| `runtime_agnostic` | 同一 agent 跑非 tokio 执行器 |

跑： `cargo run --example <name> -p synthia-harness`。完整 proof line 索引见
[`docs/examples/README.md`](../../docs/examples/README.md) §1。
