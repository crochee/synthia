# synthia-provider

LLM provider abstraction: `ModelProvider` trait plus Anthropic /
OpenAI-compatible adapters. Each adapter is gated behind its own
feature flag (`provider-anthropic`, `provider-openai`); the
`ModelProvider` trait itself ships with no HTTP dependency.

> 详细 wire types 与公开 API 见 [`synthia-provider/src/lib.rs`](src/lib.rs)；
> 7 大组件的可替换点见 [`SEAMS.md`](../../SEAMS.md) §1。

## 用法

```rust
use synthia_provider::{AnthropicProvider, ProviderConfig};

let provider = AnthropicProvider::new(ProviderConfig {
    api_key: std::env::var("ANTHROPIC_API_KEY")?,
    ..Default::default()
})?;
```

## CI 契约

- `cargo test -p synthia-provider --lib` 绿；
- 默认 features 零 reqwest / hyper 依赖
  （`make check-mvp-deps`）；
- HTTP adapter 仅在 `provider-anthropic` / `provider-openai` feature
  下出现。
