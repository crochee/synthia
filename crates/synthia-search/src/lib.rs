//! # synthia-search
//!
//! Generic multi-domain search engine: BM25 + vector hybrid with
//! pluggable [`Tokenizer`], [`Embedder`], [`VectorStore`],
//! [`Filter<T>`] and [`Reranker<T>`]. See the spec at
//! `docs/superpowers/specs/2026-09-19-synthia-search-design.md`
//! for the full design.
//!
//! ## Default vs optional features
//!
//! - `default = []` — no external dependencies beyond `synthia-core`,
//!   serde, thiserror, tracing. The default [`HashingEmbedder`] ships
//!   in this build.
//! - `provider = ["dep:synthia-provider", "core"]` — opt in for the
//!   `ModelProviderEmbedder` adapter. The provider is pulled in
//!   with `default-features = false`, so no `reqwest` is fetched.
//!
//! ## Runtime neutrality
//!
//! No `tokio` / `async-std` / `smol` in the public API. All retrieval
//! is sync, CPU-only, and uses `parking_lot::RwLock` for thread
//! safety without poisoning.
//!
//! [`Tokenizer`]: crate::Tokenizer
//! [`Embedder`]: crate::Embedder
//! [`VectorStore`]: crate::VectorStore
//! [`Filter<T>`]: crate::Filter
//! [`Reranker<T>`]: crate::Reranker
//! [`HashingEmbedder`]: crate::HashingEmbedder
//!
//! ## Implementing `Searchable` for your own type
//!
//! Any value can join a search engine: implement
//! `synthia_core::registry::RegistryItem` (`name()` = stable id,
//! `description()` = human title) plus `Searchable`, and register a
//! `SearchEngine<YourType>` with the cross-`T` [`Registry`].
//!
//! ```
//! use synthia_core::registry::RegistryItem;
//! use synthia_search::{Searchable, SearchEngine};
//! # use synthia_search::{CjkTokenizer, HashingEmbedder, QueryContext, Tokenizer, Embedder};
//! # use std::sync::Arc;
//!
//! #[derive(Clone)]
//! struct Doc { id: String, title: String, body: String }
//!
//! impl RegistryItem for Doc {
//!     fn name(&self) -> &str { &self.id }
//!     fn description(&self) -> &str { &self.title }
//! }
//!
//! impl Searchable for Doc {
//!     fn indexed_fields(&self) -> Vec<(String, f32)> {
//!         vec![(self.title.clone(), 2.0), (self.body.clone(), 1.0)]
//!     }
//! }
//!
//! # fn main() {
//! let tk: Arc<dyn Tokenizer> = Arc::new(CjkTokenizer);
//! let emb: Arc<dyn Embedder> = Arc::new(HashingEmbedder::new(64, tk.clone()));
//! let engine: SearchEngine<Doc> = SearchEngine::new(emb, tk);
//! engine.add(Doc {
//!     id: "d1".into(),
//!     title: "release notes".into(),
//!     body: "search crate landed".into(),
//! }).unwrap();
//! assert_eq!(engine.len(), 1);
//! assert!(!engine.search(&QueryContext::new("release notes")).is_empty());
//! # }
//! ```
//!
//! [`Registry`]: crate::Registry

#![allow(clippy::result_large_err)] // SearchError carries 4 hidden fields per thiserror.

pub mod bm25;
pub mod embedder;
pub mod error;
pub mod searchable;
pub mod tokenizer;
pub mod vector;

pub use bm25::Bm25Index;
pub use embedder::{Embedder, HashingEmbedder};
pub use error::{Result, SearchError};
pub use searchable::Searchable;
pub use tokenizer::{CjkTokenizer, Tokenizer};
pub use vector::{FlatVectorStore, VectorStore};

#[cfg(feature = "provider")]
pub mod provider;

#[cfg(feature = "provider")]
pub use provider::ModelProviderEmbedder;

pub mod registry;
pub use registry::{ErasedEngine, Registry};

pub mod filter;
pub mod hit;
pub mod reranker;

pub use filter::{BasicFilter, Filter};
pub use hit::{
    AgentCandidate,
    Hit,
    QueryContext,
    agent_view,
    search_tool,
    search_tool_ctx,
};
pub use reranker::{RecencyReranker, Reranker, RuleReranker};
pub mod types;
pub use types::{HasCreatedAt, Memory, Skill, Tool};
pub mod engine;
pub use engine::SearchEngine;

pub mod persist;
