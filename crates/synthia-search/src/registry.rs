//! [`Registry`] — a cross-`T` catalog of engines.
//!
//! Each registered engine is stored behind
//! `Arc<dyn ErasedEngine>`, keyed by a **domain label** — `"tool"`,
//! `"skill"`, `"memory"`, … — the host picks at registration time.
//! [`Registry::register`] keeps its historical behaviour of using
//! `std::any::type_name::<T>()` as the label, so one engine per
//! Rust type works with no label bookkeeping; hosts that register
//! several engines of the same `T` (or adapter engines that wrap
//! an async retriever) use [`Registry::register_domain`] /
//! [`Registry::register_erased`].
//!
//! `register*` returns the same underlying `Arc<SearchEngine<T>>`
//! so callers retain type-safe `add` / `remove` access (one heap
//! allocation, two pointer views — see spec §7.1).
//!
//! [`search`](Registry::search) walks every engine whose label the
//! query did not filter out ([`QueryContext::domains`]), collects
//! each engine's top-`k` hits, stamps every hit with the engine's
//! domain label, and merges them into a single `Vec<Hit>` sorted
//! by score descending and truncated to `ctx.top_k`. Each engine
//! has already done its own max-norm; we do not normalise across
//! engines (each engine's score scale is opaque to us).
//!
//! ## Why the erased layer is async
//!
//! Two real retrievers in a host deployment are async by nature: a
//! session full-text index that walks logs on a blocking pool, and
//! a memory tier whose `recall` is an async trait method.
//! Bridging those back to a sync signature inside a runtime worker
//! would block that worker, so [`ErasedEngine::search_erased`] is
//! an `async_trait` method and [`Registry::search`] awaits it.
//! CPU-only `SearchEngine<T>` values implement it by returning
//! immediately — no yield, no allocation beyond the boxed future.

use std::{
    any::{Any, type_name},
    collections::HashMap,
    sync::Arc,
};

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::{
    engine::SearchEngine,
    hit::{Hit, QueryContext},
    searchable::Searchable,
};

/// A type-erased engine the [`Registry`] can query.
///
/// `search_erased` is async so adapter engines over async
/// retrievers (session full-text, memory recall) join the same
/// cross-domain merge without a blocking bridge.
#[async_trait]
pub trait ErasedEngine: Send + Sync + 'static {
    async fn search_erased(&self, ctx: &QueryContext) -> Vec<Hit>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn type_name(&self) -> &'static str;
    fn as_any(&self) -> &dyn Any;
}

#[async_trait]
impl<T: Searchable> ErasedEngine for SearchEngine<T> {
    async fn search_erased(&self, ctx: &QueryContext) -> Vec<Hit> {
        self.search(ctx)
    }

    fn len(&self) -> usize {
        SearchEngine::len(self)
    }

    fn type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct Registry {
    engines: RwLock<HashMap<String, Arc<dyn ErasedEngine>>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            engines: RwLock::new(HashMap::new()),
        }
    }

    /// Register `engine` under `type_name::<T>()` as its domain
    /// label. One engine per Rust type needs no label
    /// bookkeeping; several engines of the same type (or friendly
    /// labels for the wire) want [`Registry::register_domain`].
    pub fn register<T: Searchable>(
        &self,
        engine: SearchEngine<T>,
    ) -> Arc<SearchEngine<T>> {
        self.insert_erased(type_name::<T>().to_string(), engine)
    }

    /// Register `engine` under an explicit domain label
    /// (`"tool"`, `"skill"`, …). Re-registering a label replaces
    /// the previous engine — the lazy-refresh pattern a host uses
    /// when its live catalogs change.
    pub fn register_domain<T: Searchable>(
        &self,
        domain: &str,
        engine: SearchEngine<T>,
    ) -> Arc<SearchEngine<T>> {
        self.insert_erased(domain.to_string(), engine)
    }

    /// Register a pre-erased adapter engine (one wrapping an
    /// async retriever, e.g. a session full-text index) under an
    /// explicit domain label. Returns the engine it replaced, if
    /// any.
    pub fn register_erased(
        &self,
        domain: &str,
        engine: Arc<dyn ErasedEngine>,
    ) -> Option<Arc<dyn ErasedEngine>> {
        self.engines.write().insert(domain.to_string(), engine)
    }

    fn insert_erased<T: Searchable>(
        &self,
        key: String,
        engine: SearchEngine<T>,
    ) -> Arc<SearchEngine<T>> {
        let arc = Arc::new(engine);
        let erased: Arc<dyn ErasedEngine> = arc.clone();
        self.engines.write().insert(key, erased);
        arc
    }

    pub fn unregister<T: Searchable>(&self) -> bool {
        self.unregister_by_type_name(type_name::<T>())
    }

    pub fn unregister_by_type_name(&self, name: &str) -> bool {
        self.engines.write().remove(name).is_some()
    }

    /// Remove the engine registered under an explicit domain
    /// label. Same operation as
    /// [`unregister_by_type_name`](Registry::unregister_by_type_name);
    /// named for symmetry with [`register_erased`](Registry::register_erased).
    pub fn unregister_domain(&self, domain: &str) -> bool {
        self.unregister_by_type_name(domain)
    }

    /// The domain labels currently registered, sorted for
    /// deterministic display.
    pub fn domains(&self) -> Vec<String> {
        let mut labels: Vec<String> =
            self.engines.read().keys().cloned().collect();
        labels.sort();
        labels
    }

    pub fn engine_count(&self) -> usize {
        self.engines.read().len()
    }

    /// Fan the query out across every engine, stamp each hit with
    /// the engine's domain label, merge, sort by score descending,
    /// truncate to `ctx.top_k`.
    ///
    /// [`QueryContext::domains`] filters which engines run: empty
    /// means all of them, non-empty means only engines whose label
    /// appears in the list.
    ///
    /// The engine handles are cloned out of the read lock before
    /// the first `await`, so the parking_lot guard never lives
    /// across a yield point — the returned future stays `Send`
    /// even when an adapter engine awaits a real retriever.
    pub async fn search(&self, ctx: &QueryContext) -> Vec<Hit> {
        let engines: Vec<(String, Arc<dyn ErasedEngine>)> = {
            let g = self.engines.read();
            g.iter()
                .filter(|(label, _)| {
                    ctx.domains.is_empty()
                        || ctx.domains.iter().any(|d| d == *label)
                })
                .map(|(label, e)| (label.clone(), Arc::clone(e)))
                .collect()
        };
        let mut all: Vec<Hit> = Vec::new();
        for (label, engine) in engines {
            let mut hits = engine.search_erased(ctx).await;
            for h in &mut hits {
                h.domain = label.clone();
            }
            all.extend(hits);
        }
        all.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        all.truncate(ctx.top_k);
        all
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        embedder::HashingEmbedder,
        tokenizer::CjkTokenizer,
        types::{Skill, Tool},
    };

    fn skill_engine() -> SearchEngine<Skill> {
        let tk: Arc<dyn crate::Tokenizer> = Arc::new(CjkTokenizer);
        let emb: Arc<dyn crate::Embedder> =
            Arc::new(HashingEmbedder::new(32, tk.clone()));
        SearchEngine::new(emb, tk)
    }

    fn tool_engine() -> SearchEngine<Tool> {
        let tk: Arc<dyn crate::Tokenizer> = Arc::new(CjkTokenizer);
        let emb: Arc<dyn crate::Embedder> =
            Arc::new(HashingEmbedder::new(32, tk.clone()));
        SearchEngine::new(emb, tk)
    }

    fn demo_skill(id: &str, description: &str) -> Skill {
        Skill {
            id: id.into(),
            name: description.into(),
            description: description.into(),
            when_to_use: vec![],
            not_for: vec![],
            tags: vec![],
            examples: vec![],
        }
    }

    fn demo_tool(id: &str, description: &str) -> Tool {
        Tool {
            id: id.into(),
            name: description.into(),
            description: description.into(),
            when_to_use: vec![],
            not_for: vec![],
            tags: vec![],
            schema_hint: "".into(),
        }
    }

    #[test]
    fn register_returns_typed_arc() {
        let reg = Registry::new();
        let handle = reg.register::<Skill>(skill_engine());
        handle.add(demo_skill("s1", "PDF")).unwrap();
        assert_eq!(reg.engine_count(), 1);
    }

    #[test]
    #[allow(non_snake_case)]
    fn register_two_Ts_keeps_both() {
        let reg = Registry::new();
        let s = reg.register::<Skill>(skill_engine());
        let t = reg.register::<Tool>(tool_engine());
        assert_eq!(reg.engine_count(), 2);
        assert_eq!(s.len(), 0);
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn unregister_by_type() {
        let reg = Registry::new();
        reg.register::<Skill>(skill_engine());
        assert!(reg.unregister::<Skill>());
        assert_eq!(reg.engine_count(), 0);
        assert!(!reg.unregister::<Skill>());
    }

    #[test]
    fn unregister_by_type_name() {
        let reg = Registry::new();
        reg.register::<Skill>(skill_engine());
        let name = std::any::type_name::<Skill>();
        assert!(reg.unregister_by_type_name(name));
        assert_eq!(reg.engine_count(), 0);
    }

    #[tokio::test]
    async fn cross_t_search_merges() {
        let reg = Registry::new();
        let sh = reg.register::<Skill>(skill_engine());
        let th = reg.register::<Tool>(tool_engine());
        sh.add(demo_skill("pdf", "提取 PDF")).unwrap();
        th.add(demo_tool("weather", "查天气")).unwrap();
        let hits = reg.search(&QueryContext::new("PDF").top_k(5)).await;
        assert!(hits.iter().any(|h| h.id == "pdf"));
        assert!(hits.iter().any(|h| h.id == "weather"));
    }

    #[tokio::test]
    async fn cross_t_search_truncates_to_top_k() {
        let reg = Registry::new();
        let sh = reg.register::<Skill>(skill_engine());
        let th = reg.register::<Tool>(tool_engine());
        for i in 0..5 {
            sh.add(demo_skill(&format!("s{i}"), "x")).unwrap();
            th.add(demo_tool(&format!("t{i}"), "x")).unwrap();
        }
        let hits = reg.search(&QueryContext::new("x").top_k(3)).await;
        assert_eq!(hits.len(), 3);
    }

    #[tokio::test]
    async fn domain_labels_stamp_hits() {
        let reg = Registry::new();
        let sh = reg.register_domain("skill", skill_engine());
        sh.add(demo_skill("pdf", "提取 PDF")).unwrap();
        let hits = reg.search(&QueryContext::new("PDF")).await;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].domain, "skill");
        assert_eq!(reg.domains(), vec!["skill".to_string()]);
    }

    #[tokio::test]
    async fn two_engines_same_t_under_distinct_domains() {
        let reg = Registry::new();
        let a = reg.register_domain("tool", skill_engine());
        let b = reg.register_domain("skill", skill_engine());
        assert_eq!(reg.engine_count(), 2);
        a.add(demo_skill("read", "读文件")).unwrap();
        b.add(demo_skill("pdf", "提取 PDF")).unwrap();
        let hits = reg
            .search(&QueryContext::new("PDF").domains(["skill"]))
            .await;
        assert!(hits.iter().all(|h| h.domain == "skill"));
        assert!(hits.iter().any(|h| h.id == "pdf"));
    }

    #[tokio::test]
    async fn domains_filter_skips_unlisted_engines() {
        let reg = Registry::new();
        let sh = reg.register_domain("skill", skill_engine());
        let th = reg.register_domain("tool", tool_engine());
        sh.add(demo_skill("pdf", "提取 PDF")).unwrap();
        th.add(demo_tool("weather", "查天气")).unwrap();
        let hits = reg
            .search(&QueryContext::new("PDF 天气").domains(["tool"]))
            .await;
        assert!(hits.iter().all(|h| h.domain == "tool"));
        assert!(hits.iter().any(|h| h.id == "weather"));
    }

    #[tokio::test]
    async fn register_erased_adapter_joins_the_merge() {
        struct FixedEngine;
        #[async_trait]
        impl ErasedEngine for FixedEngine {
            async fn search_erased(&self, _ctx: &QueryContext) -> Vec<Hit> {
                vec![Hit {
                    item_idx: 0,
                    domain: String::new(),
                    id: "session:s01".into(),
                    title: "earlier chat".into(),
                    score: 0.75,
                    bm25: 0.75,
                    vector: 0.0,
                    reasons: vec!["transcript match".into()],
                    preview: Some("we discussed PDF extraction".into()),
                }]
            }

            fn len(&self) -> usize {
                1
            }

            fn type_name(&self) -> &'static str {
                "FixedEngine"
            }

            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        let reg = Registry::new();
        let sh = reg.register_domain("skill", skill_engine());
        sh.add(demo_skill("pdf", "提取 PDF")).unwrap();
        let replaced = reg.register_erased("session", Arc::new(FixedEngine));
        assert!(replaced.is_none());
        let hits = reg.search(&QueryContext::new("PDF")).await;
        assert!(hits.iter().any(|h| h.domain == "session"
            && h.id == "session:s01"
            && h.preview.as_deref() == Some("we discussed PDF extraction")));
        assert!(reg.unregister_domain("session"));
        assert_eq!(reg.engine_count(), 1);
    }

    #[tokio::test]
    async fn re_registering_a_domain_replaces_the_engine() {
        let reg = Registry::new();
        let old = reg.register_domain("skill", skill_engine());
        old.add(demo_skill("stale", "旧技能")).unwrap();
        let fresh = reg.register_domain("skill", skill_engine());
        fresh.add(demo_skill("pdf", "提取 PDF")).unwrap();
        assert_eq!(reg.engine_count(), 1);
        let hits = reg.search(&QueryContext::new("技能")).await;
        assert!(hits.iter().all(|h| h.id != "stale"));
    }
}
