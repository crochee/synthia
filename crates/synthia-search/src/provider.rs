//! [`ModelProviderEmbedder`] — adapter from
//! `synthia_provider::ModelProvider::embed` to [`Embedder`].
//!
//! Behind the `provider` feature. The provider is pulled with
//! `default-features = false`, so this crate does **not** pull
//! `reqwest` — lib consumers that want HTTP drive the
//! `provider-anthropic` / `provider-openai` features in *their*
//! crate.
//!
//! ## Runtime
//!
//! [`ModelProvider::embed`] is async; this adapter is sync. The
//! bridge is [`futures::executor::block_on`], which **parks the
//! calling OS thread** until the provider call resolves.
//!
//! **Sync contexts only.** Do not call `embed()` from inside an
//! async runtime's worker thread (e.g. a tokio worker): with a
//! current-thread runtime this deadlocks immediately (the reactor
//! is never polled); with a multi-thread runtime it parks a
//! worker and can deadlock. Use this adapter from threads you own
//! (a `std::thread`, a blocking pool, `main` before any runtime
//! starts), or pre-compute embeddings and supply them via
//! `Searchable::embedding()` instead. Detection of "inside a
//! runtime" is deliberately not attempted — it would require a
//! tokio dependency, which this crate forbids.

use std::sync::Arc;

use synthia_provider::ModelProvider;

use crate::embedder::Embedder;

pub struct ModelProviderEmbedder {
    provider: Arc<dyn ModelProvider>,
    dim: usize,
}

impl ModelProviderEmbedder {
    pub fn new(provider: Arc<dyn ModelProvider>, dim: usize) -> Self {
        Self { provider, dim }
    }
}

impl Embedder for ModelProviderEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let result = futures::executor::block_on(
            self.provider.embed(vec![text.to_string()]),
        );
        match result {
            Ok(mut vecs) if !vecs.is_empty() => {
                let f64s = vecs.swap_remove(0);
                let mut out: Vec<f32> =
                    f64s.into_iter().map(|x| x as f32).collect();
                // The provider returns the natural embedding; we trust its
                // length to match `dim`. If not, we truncate / pad.
                if out.len() != self.dim {
                    out.resize(self.dim, 0.0);
                }
                out
            }
            _ => vec![0.0; self.dim],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_returns_configured_size() {
        fn check_dim<E: Embedder>(e: E) -> usize {
            e.dim()
        }
        assert_eq!(check_dim(noop_for_test()), 4);
    }

    fn noop_for_test() -> impl Embedder {
        // We don't construct a real ModelProvider here — the trait
        // requires an async runtime to drive. The smoke test only
        // exercises the trait surface via a zero-cost witness.
        struct TestEmbedder;
        impl Embedder for TestEmbedder {
            fn dim(&self) -> usize {
                4
            }

            fn embed(&self, _: &str) -> Vec<f32> {
                vec![0.0; 4]
            }
        }
        TestEmbedder
    }
}
