# Synthia Quick Start — 5 minutes to your first agent

No API key, no network, no framework object. One crate, one provider,
one agent. When this works, you have the same shape every Synthia
deployment uses.

```bash
cargo new hello-synthia --bin
cd hello-synthia
```

`Cargo.toml`:

```toml
[dependencies]
synthia = { git = "https://github.com/crochee/synthia", features = [
    "core", "provider", "context", "tool", "session", "steering", "harness",
] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

`src/main.rs`:

```rust,no_run
use std::sync::Arc;
use synthia::prelude::*;

struct OneShot(&'static str);

#[async_trait::async_trait]
impl ModelProvider for OneShot {
    async fn initialize(&mut self, _: ProviderConfig) -> Result<(), Error> { Ok(()) }
    fn name(&self) -> &str { "one-shot" }
    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "one-shot-1".into(),
            provider: "one-shot".into(),
            context_window: 8_192,
            max_output_tokens: 1_024,
            supports_tools: false,
            supports_streaming: false,
            supports_reasoning: false,
        }
    }
    async fn complete(&self, _: CompletionRequest)
        -> Result<CompletionResponse, Error>
    {
        Ok(CompletionResponse { content: Content::text(self.0),
                                 ..CompletionResponse::default() })
    }
}

#[tokio::main]
async fn main() {
    let provider = Arc::new(OneShot("hello, world"));
    let registry = Arc::new(ToolRegistry::new());
    let cancel: Arc<dyn CancelToken> = AtomicCancelToken::shared();
    let agent = ReActAgent::new(provider, registry)
        .with_steering(Arc::new(Steering::noop()))
        .with_context_manager(Arc::new(TruncatingContextManager));
    let mut stream = agent.run(AgentInput::text("hi"), cancel).await;
    while let Some(_event) = futures::StreamExt::next(&mut stream).await {}
}
```

```bash
cargo run
```

That's it. You have a working Synthia agent. From here:

| Want to … | Go to |
|---|---|
| Plug in a real model | [`MINIMAL.md` §1](../MINIMAL.md#step-1--write-the-provider) |
| Add tools (`read` / `write` / `shell` / `web_fetch`) | [`MINIMAL.md` §3](../MINIMAL.md#step-3--add-pieces-only-when-you-need-them) |
| Replace a piece (provider / steering / context) | [`SEAMS.md`](../SEAMS.md) |
| Ship an HTTP server | [`DEPLOYMENT.md`](../DEPLOYMENT.md) |
| Adopt commercially | [`COMMERCIAL.md`](COMMERCIAL.md) |

## The promise this builds on

The seven features in `Cargo.toml` compile **no** agent-facing tool, **no**
HTTP client, **no** OTel exporter, **no** database. Verified locally:

```bash
# clone synthia, then in the workspace root:
make check-mvp-deps
# → OK: core,provider,context,tool,session,steering,harness pulls no
#      (opentelemetry|tonic|axum|rusqlite|sqlx|reqwest|hyper|rustls|h2|tower|webpki)
```

That is the smallest legal agent. Anything else is opt-in.
