//! [`SearchEngine`] — the generic, thread-safe search engine.
//!
//! All public methods take `&self`; the internal state is
//! `parking_lot::RwLock<EngineInner<T>>`, so callers share one
//! engine behind `Arc<SearchEngine<T>>`. Writers serialise; readers
//! run concurrently. parking_lot never poisons, so no
//! `LockPoisoned` ever escapes.
//!
//! ## Hot path
//!
//! `search` does one BM25 score pass over all alive docs, one
//! top-k vector search, then merges the two normalised scores
//! with the configured weights, applies filters and rerankers,
//! and truncates to `ctx.top_k`. No allocation per doc beyond
//! the BM25 score vector and the candidate set; the merged
//! `Vec<Hit>` grows proportional to the recall_k, not the corpus.
//!
//! ## Tombstone policy
//!
//! `add(id)` with an existing id first tombstones the old entry
//! (drops bm25 posting + vector slot), then inserts the new one.
//! `remove(id)` tombstones. Tombstones are counted; once
//! `n_deleted / n_alive > tombstone_ratio`, the next `add` rebuilds
//! the index in-line to keep the score pass fast.

use std::{collections::HashSet, path::PathBuf, sync::Arc};

use parking_lot::RwLock;

use crate::{
    bm25::Bm25Index,
    embedder::Embedder,
    error::{Result, SearchError},
    filter::Filter,
    hit::{Hit, QueryContext},
    reranker::Reranker,
    searchable::Searchable,
    tokenizer::Tokenizer,
    vector::VectorStore,
};

pub struct SearchEngine<T: Searchable> {
    pub(crate) inner: RwLock<EngineInner<T>>,
}

pub(crate) struct EngineInner<T: Searchable> {
    pub(crate) items: Vec<T>,
    pub(crate) id_to_idx: std::collections::HashMap<String, usize>,
    bm25: Bm25Index,
    pub(crate) vectors: Box<dyn VectorStore>,
    embedder: Arc<dyn Embedder>,
    tokenizer: Arc<dyn Tokenizer>,
    bm25_weight: f32,
    vector_weight: f32,
    recall_k: usize,
    rerankers: Vec<Box<dyn Reranker<T>>>,
    filters: Vec<Box<dyn Filter<T>>>,
    tombstone_ratio: f32,
    /// Persistence root, installed by `save` / `load` (the
    /// serde-bounded impl block in [`crate::persist`]). `None` — the
    /// default — means mutations are not op-logged.
    pub(crate) persist_root: Option<PathBuf>,
    /// Type-erased persistence hook: serialising `T` needs the
    /// `Serialize` bound only [`crate::persist`]'s impl block can
    /// prove, so the generic `add` reaches the op-log through this
    /// closure. Installed together with `persist_root`; disk failures
    /// warn and never fail the engine mutation.
    pub(crate) persist_hook: Option<crate::persist::PersistHook<T>>,
}

impl<T: Searchable> SearchEngine<T> {
    pub fn new(
        embedder: Arc<dyn Embedder>,
        tokenizer: Arc<dyn Tokenizer>,
    ) -> Self {
        let dim = embedder.dim();
        Self {
            inner: RwLock::new(EngineInner {
                items: Vec::new(),
                id_to_idx: std::collections::HashMap::new(),
                bm25: Bm25Index::new(1.5, 0.75),
                vectors: Box::new(crate::vector::FlatVectorStore::new(dim)),
                embedder,
                tokenizer,
                bm25_weight: 0.5,
                vector_weight: 0.5,
                recall_k: 50,
                rerankers: Vec::new(),
                filters: vec![Box::new(crate::filter::BasicFilter)],
                tombstone_ratio: 0.30,
                persist_root: None,
                persist_hook: None,
            }),
        }
    }

    pub fn set_weights(&self, bm25: f32, vector: f32) -> &Self {
        let mut g = self.inner.write();
        g.bm25_weight = bm25;
        g.vector_weight = vector;
        self
    }

    pub fn set_recall_k(&self, k: usize) -> &Self {
        self.inner.write().recall_k = k.max(1);
        self
    }

    pub fn set_tombstone_ratio(&self, r: f32) -> &Self {
        self.inner.write().tombstone_ratio = r.clamp(0.0, 1.0);
        self
    }

    pub fn add_reranker(&self, r: Box<dyn Reranker<T>>) -> &Self {
        self.inner.write().rerankers.push(r);
        self
    }

    pub fn add_filter(&self, f: Box<dyn Filter<T>>) -> &Self {
        self.inner.write().filters.push(f);
        self
    }

    /// Swap the backing vector store. Must be called before the
    /// first `add` — the store's `dim` must equal the embedder's,
    /// and any vectors already inserted into the previous store
    /// would be lost.
    ///
    /// # Errors
    ///
    /// Returns [`SearchError::NotBuilt`] if the engine already
    /// holds items (rebuilding mid-flight is not supported; use
    /// [`SearchEngine::compact`] semantics via a fresh engine
    /// instead).
    pub fn with_vector_store(
        &self,
        store: Box<dyn VectorStore>,
    ) -> Result<&Self> {
        let mut g = self.inner.write();
        if !g.items.is_empty() || !g.id_to_idx.is_empty() {
            return Err(SearchError::NotBuilt);
        }
        g.vectors = store;
        Ok(self)
    }

    pub fn add(&self, item: T) -> Result<usize> {
        let vector = match item.embedding() {
            Some(e) => e.to_vec(),
            None => {
                let g = self.inner.read();
                g.embedder.embed(&item.embed_text())
            }
        };
        self.add_with_vector(item, vector)
    }

    /// Internal: insert with a pre-computed vector (restore path — the
    /// embedder is not consulted). Mirrors `add` including tombstone /
    /// rebuild bookkeeping; tokens are still computed by the tokenizer
    /// (deterministic, cheap). Appends an `Op::Add` to the op-log when a
    /// persistence root is installed.
    pub(crate) fn add_with_vector(
        &self,
        item: T,
        vector: Vec<f32>,
    ) -> Result<usize> {
        let id = item.name().to_string();
        let existing = {
            let g = self.inner.read();
            g.id_to_idx.get(&id).copied()
        };
        if let Some(idx) = existing {
            let mut g = self.inner.write();
            g.bm25.mark_deleted(idx);
            g.vectors.mark_deleted(idx);
            g.id_to_idx.remove(&id);
        }

        let tokens = {
            let g = self.inner.read();
            g.tokenizer.tokenize(&item.embed_text())
        };

        let mut g = self.inner.write();
        // The op payload must be cloned before the moves below; only
        // bother when a hook is installed (non-persisted engines pay
        // nothing).
        let pending_op = match (&g.persist_hook, &g.persist_root) {
            (Some(_), Some(root)) => {
                Some(crate::persist::PersistCmd::AppendAdd {
                    root: root.clone(),
                    item: item.clone(),
                    vector: vector.clone(),
                })
            }
            _ => None,
        };
        g.bm25.add(&tokens);
        let idx = g.vectors.add(vector)?;
        g.items.push(item);
        g.id_to_idx.insert(id, idx);

        let should_rebuild = g.tombstone_ratio > 0.0
            && g.bm25.n_alive() > 0
            && (g.bm25.n_deleted() as f32 / g.bm25.n_alive() as f32)
                > g.tombstone_ratio;
        if should_rebuild {
            rebuild_locked(&mut g);
        }
        if let (Some(cmd), Some(hook)) = (pending_op, g.persist_hook.as_ref()) {
            hook(cmd);
        }
        Ok(idx)
    }

    pub fn add_all<I: IntoIterator<Item = T>>(&self, items: I) -> Result<()> {
        for it in items {
            self.add(it)?;
        }
        Ok(())
    }

    pub fn remove(&self, id: &str) -> bool {
        let idx = {
            let mut g = self.inner.write();
            match g.id_to_idx.remove(id) {
                Some(i) => i,
                None => return false,
            }
        };
        let mut g = self.inner.write();
        g.bm25.mark_deleted(idx);
        g.vectors.mark_deleted(idx);
        if let Some(root) = g.persist_root.clone() {
            crate::persist::append_remove_op(&root, id);
        }
        true
    }

    /// Rebuild the indexes with tombstones dropped. When a persistence
    /// root is installed, also rewrite the snapshot and truncate the
    /// op-log — under the same write lock, so no interleaved `add` can
    /// append an op that the trailing truncate would then drop. (The
    /// tombstone-ratio auto-rebuild inside `add` deliberately does not
    /// rewrite the snapshot; the op-log stays correct, it only grows.)
    pub fn compact(&self) {
        let mut g = self.inner.write();
        rebuild_locked(&mut g);
        if let (Some(hook), Some(root)) =
            (g.persist_hook.clone(), g.persist_root.as_ref())
        {
            let docs = crate::persist::collect_alive(&g);
            hook(crate::persist::PersistCmd::RewriteSnapshot {
                root: root.clone(),
                docs,
            });
        }
    }

    pub fn get(&self, id: &str) -> Option<T> {
        let g = self.inner.read();
        g.id_to_idx.get(id).and_then(|i| g.items.get(*i).cloned())
    }

    pub fn len(&self) -> usize {
        self.inner.read().id_to_idx.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn search(&self, ctx: &QueryContext) -> Vec<Hit> {
        let g = self.inner.read();
        if g.items.is_empty() {
            return Vec::new();
        }

        let q_tokens = g.tokenizer.tokenize(&ctx.text);
        let q_vec = g.embedder.embed(&ctx.text);

        let raw_bm25 = g.bm25.score_all(&q_tokens);
        let vec_pairs = g.vectors.search(&q_vec, g.recall_k);

        let bm25_n = normalize_by_max(&raw_bm25);
        let vec_max = vec_pairs.iter().map(|(_, s)| *s).fold(0.0f32, f32::max);
        let vec_denom = if vec_max > 1e-9 { vec_max } else { 1.0 };

        let mut candidate: HashSet<usize> = HashSet::new();
        for (i, s) in raw_bm25.iter().enumerate() {
            if *s > 0.0 {
                candidate.insert(i);
            }
        }
        for (i, _) in &vec_pairs {
            candidate.insert(*i);
        }

        let mut hits: Vec<Hit> = candidate
            .into_iter()
            .filter_map(|i| {
                if !g.bm25.is_alive(i) || !g.vectors.is_alive(i) {
                    return None;
                }
                let item = g.items.get(i)?;
                let v_score = vec_pairs
                    .iter()
                    .find(|(j, _)| *j == i)
                    .map(|(_, s)| *s)
                    .unwrap_or(0.0);
                let v_norm = v_score / vec_denom;
                Some(Hit {
                    item_idx: i,
                    domain: String::new(),
                    id: item.name().to_string(),
                    title: item.description().to_string(),
                    score: g.bm25_weight * bm25_n[i] + g.vector_weight * v_norm,
                    bm25: bm25_n[i],
                    vector: v_norm,
                    reasons: Vec::new(),
                    preview: item.preview(),
                })
            })
            .collect();

        hits.retain(|h| {
            let item = &g.items[h.item_idx];
            g.filters.iter().all(|f| f.allow(ctx, item))
        });

        for r in &g.rerankers {
            r.rerank(ctx, &g.items, &mut hits);
        }

        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(ctx.top_k);
        for h in hits.iter_mut() {
            if h.reasons.is_empty() {
                h.reasons.push("semantic match".into());
            }
        }
        hits
    }
}

fn rebuild_locked<T: Searchable>(g: &mut EngineInner<T>) {
    let mut new_bm25 = Bm25Index::new(g.bm25.k1(), g.bm25.b());
    let new_dim = g.vectors.dim();
    let mut new_vectors: Box<dyn VectorStore> =
        Box::new(crate::vector::FlatVectorStore::new(new_dim));

    // `id_to_idx` records the one live slot per id. A slot that is not
    // the recorded one is a tombstoned re-add of an id that is still
    // live under a later slot, and the rebuild must drop it: keeping
    // every item whose *name* survives would resurrect exactly the
    // duplicates `add` tombstoned, and they would come back as
    // same-id hits with identical scores.
    let live_slot: std::collections::HashMap<String, usize> =
        g.id_to_idx.clone();
    let old_items = std::mem::take(&mut g.items);
    let mut new_items: Vec<T> = Vec::with_capacity(old_items.len());
    let mut new_map = std::collections::HashMap::new();

    for (old_idx, item) in old_items.into_iter().enumerate() {
        if live_slot.get(item.name()) != Some(&old_idx) {
            continue;
        }
        let tokens = g.tokenizer.tokenize(&item.embed_text());
        new_bm25.add(&tokens);
        let v = match item.embedding() {
            Some(e) => e.to_vec(),
            None => g.embedder.embed(&item.embed_text()),
        };
        let idx = new_vectors.add(v).unwrap_or(0);
        new_map.insert(item.name().to_string(), idx);
        new_items.push(item);
    }

    g.items = new_items;
    g.bm25 = new_bm25;
    g.vectors = new_vectors;
    g.id_to_idx = new_map;
    // After rebuild, every alive doc is alive and there are no tombstones.
    g.bm25.reset_deleted_counter();
}

fn normalize_by_max(scores: &[f32]) -> Vec<f32> {
    let max = scores.iter().cloned().fold(0.0f32, f32::max);
    if max <= 1e-9 {
        return vec![0.0; scores.len()];
    }
    scores.iter().map(|s| s / max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        embedder::HashingEmbedder,
        tokenizer::CjkTokenizer,
        types::Skill,
    };

    fn engine() -> SearchEngine<Skill> {
        let tk: Arc<dyn Tokenizer> = Arc::new(CjkTokenizer);
        let emb: Arc<dyn Embedder> =
            Arc::new(HashingEmbedder::new(64, tk.clone()));
        SearchEngine::new(emb, tk)
    }

    fn skill(id: &str, name: &str, desc: &str) -> Skill {
        Skill {
            id: id.into(),
            name: name.into(),
            description: desc.into(),
            when_to_use: vec![],
            not_for: vec![],
            tags: vec![],
            examples: vec![],
        }
    }

    #[test]
    fn add_get_remove_lifecycle() {
        let e = engine();
        e.add(skill("a", "alpha", "first")).unwrap();
        e.add(skill("b", "beta", "second")).unwrap();
        assert_eq!(e.len(), 2);
        assert!(e.get("a").is_some());
        assert!(e.remove("a"));
        assert_eq!(e.len(), 1);
        assert!(e.get("a").is_none());
        assert!(!e.remove("a")); // already gone
    }

    #[test]
    fn add_with_existing_id_replaces() {
        let e = engine();
        e.add(skill("a", "alpha", "first")).unwrap();
        e.add(skill("a", "alpha", "second")).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e.get("a").unwrap().description, "second");
    }

    #[test]
    fn remove_unknown_id_returns_false() {
        let e = engine();
        assert!(!e.remove("nope"));
    }

    #[test]
    fn compact_drops_tombstones() {
        let e = engine();
        e.set_tombstone_ratio(0.0); // disable auto-compact
        for i in 0..10 {
            e.add(skill(&format!("s{i}"), &format!("skill {i}"), "d"))
                .unwrap();
        }
        for i in 0..5 {
            e.remove(&format!("s{i}"));
        }
        // 5 alive, 5 deleted
        assert_eq!(e.inner.read().bm25.n_deleted(), 5);
        e.compact();
        assert_eq!(e.inner.read().bm25.n_deleted(), 0);
        assert_eq!(e.len(), 5);
    }

    #[test]
    fn auto_compact_triggers_at_threshold() {
        let e = engine();
        e.set_tombstone_ratio(0.30);
        for i in 0..10 {
            e.add(skill(&format!("s{i}"), &format!("s{i}"), "d"))
                .unwrap();
        }
        // 3 deletes / 7 alive = ~0.43 > 0.30 → next add triggers rebuild.
        e.remove("s0");
        e.remove("s1");
        e.remove("s2");
        // No rebuild yet (we removed, didn't add).
        e.add(skill("sX", "new", "new")).unwrap();
        // After the add, the rebuild should have wiped tombstones.
        assert_eq!(e.inner.read().bm25.n_deleted(), 0);
    }

    #[test]
    fn search_returns_hits_descending() {
        let e = engine();
        e.add(skill("pdf", "PDF 提取", "提取 PDF 文本")).unwrap();
        e.add(skill("ocr", "OCR 扫描", "图片文字识别")).unwrap();
        e.add(skill("weather", "天气", "查询天气")).unwrap();
        let hits = e.search(&QueryContext::new("PDF 文本提取").top_k(3));
        assert!(!hits.is_empty());
        // The PDF hit should outrank the others for the PDF query.
        assert_eq!(hits[0].id, "pdf");
        // Sorted descending.
        for w in hits.windows(2) {
            assert!(w[0].score >= w[1].score);
        }
    }

    #[test]
    fn search_empty_engine_returns_empty() {
        let e = engine();
        let hits = e.search(&QueryContext::new("anything"));
        assert!(hits.is_empty());
    }

    #[test]
    fn fluent_setters_are_chainable() {
        let e = engine();
        let _ = e
            .set_weights(0.7, 0.3)
            .set_recall_k(10)
            .set_tombstone_ratio(0.5);
        // No assertion needed — chain compiles, returns &Self.
    }

    #[test]
    fn with_vector_store_rejects_non_empty_engine() {
        let e = engine();
        let store = Box::new(crate::vector::FlatVectorStore::new(64));
        assert!(e.with_vector_store(store).is_ok());
        e.add(skill("a", "alpha", "first")).unwrap();
        let store2 = Box::new(crate::vector::FlatVectorStore::new(64));
        assert!(matches!(
            e.with_vector_store(store2),
            Err(crate::error::SearchError::NotBuilt)
        ));
    }
}
