//! [`CatalogEngine`] — a self-refreshing engine over a live catalog.
//!
//! The static domains (tool / mcp / skill / agent) are snapshots of
//! mutable server state, so the engine pairs a **version probe**
//! (cheap, sync) with a **snapshot source**. On every search it
//! re-checks the probe and rebuilds the inner
//! [`SearchEngine<SearchDoc>`] only when the catalog actually moved
//! — a steady-state query costs one atomic load plus the BM25 pass.
//!
//! `len` is approximate by design: it answers from whatever the
//! cached engine holds right now and never triggers a rebuild of
//! its own.

use std::sync::Arc;

use synthia::search::{
    ErasedEngine,
    Hit,
    QueryContext,
    SearchEngine,
    Searchable,
};

use super::doc::SearchDoc;

/// Build the CPU engine the catalog refreshes into. Hashing
/// embedder + CJK tokenizer: deterministic, offline, and it keeps
/// Chinese and Latin text in one index.
fn build_engine(docs: Vec<SearchDoc>) -> Arc<SearchEngine<SearchDoc>> {
    let tk: Arc<dyn synthia::search::Tokenizer> =
        Arc::new(synthia::search::CjkTokenizer);
    let emb: Arc<dyn synthia::search::Embedder> =
        Arc::new(synthia::search::HashingEmbedder::new(128, Arc::clone(&tk)));
    let engine = SearchEngine::new(emb, tk);
    if let Err(e) = engine.add_all(docs) {
        tracing::warn!(error = %e, "search: failed to index catalog docs");
    }
    Arc::new(engine)
}

struct CachedCatalog {
    version: u64,
    engine: Arc<SearchEngine<SearchDoc>>,
}

/// One search domain over a snapshot-able catalog.
pub(crate) struct CatalogEngine {
    version: Box<dyn Fn() -> u64 + Send + Sync>,
    source: Box<dyn Fn() -> Vec<SearchDoc> + Send + Sync>,
    cached: parking_lot::RwLock<CachedCatalog>,
}

impl CatalogEngine {
    /// `u64::MAX` as the initial version so the first access always
    /// snapshots — a fresh registry legitimately reports version 0,
    /// and "0 == 0" must not read as "already fresh".
    pub(crate) fn new(
        version: Box<dyn Fn() -> u64 + Send + Sync>,
        source: Box<dyn Fn() -> Vec<SearchDoc> + Send + Sync>,
    ) -> Self {
        Self {
            version,
            source,
            cached: parking_lot::RwLock::new(CachedCatalog {
                version: u64::MAX,
                engine: build_engine(Vec::new()),
            }),
        }
    }

    fn current(&self) -> Arc<SearchEngine<SearchDoc>> {
        let version = (self.version)();
        {
            let g = self.cached.read();
            if g.version == version {
                return Arc::clone(&g.engine);
            }
        }
        let engine = build_engine((self.source)());
        let mut g = self.cached.write();
        // A concurrent refresh may have already installed a newer
        // snapshot; ours is at least as fresh (same probe), so
        // installing unconditionally is fine.
        *g = CachedCatalog {
            version,
            engine: Arc::clone(&engine),
        };
        engine
    }
}

#[async_trait::async_trait]
impl ErasedEngine for CatalogEngine {
    async fn search_erased(&self, ctx: &QueryContext) -> Vec<Hit> {
        self.current().search(ctx)
    }

    fn len(&self) -> usize {
        self.cached.read().engine.len()
    }

    fn type_name(&self) -> &'static str {
        "synthia_server::search::CatalogEngine"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// `SearchDoc: Searchable` is the whole reason this module compiles;
// the assertion keeps a future field change from silently dropping
// the impl.
const _: fn(SearchDoc) = |d| {
    fn assert_searchable<T: Searchable>(_: &T) {}
    assert_searchable(&d);
};
