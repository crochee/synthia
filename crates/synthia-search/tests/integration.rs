//! End-to-end integration: cross-`T` registry + Clock injection
//! on `RecencyReranker`.

use std::sync::Arc;

use synthia_core::clock::FixedClock;
use synthia_search::{
    CjkTokenizer,
    Embedder,
    HashingEmbedder,
    QueryContext,
    RecencyReranker,
    Registry,
    SearchEngine,
    Tokenizer,
    types::{Memory, Skill, Tool},
};

fn vec_str(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn skill_engine() -> SearchEngine<Skill> {
    let tk: Arc<dyn Tokenizer> = Arc::new(CjkTokenizer);
    let emb: Arc<dyn Embedder> = Arc::new(HashingEmbedder::new(64, tk.clone()));
    SearchEngine::new(emb, tk)
}

fn tool_engine() -> SearchEngine<Tool> {
    let tk: Arc<dyn Tokenizer> = Arc::new(CjkTokenizer);
    let emb: Arc<dyn Embedder> = Arc::new(HashingEmbedder::new(64, tk.clone()));
    SearchEngine::new(emb, tk)
}

fn memory_engine_with_clock(secs: u64) -> SearchEngine<Memory> {
    let tk: Arc<dyn Tokenizer> = Arc::new(CjkTokenizer);
    let emb: Arc<dyn Embedder> = Arc::new(HashingEmbedder::new(64, tk.clone()));
    let clock: Arc<dyn synthia_core::clock::Clock> = Arc::new(FixedClock::new(
        chrono::DateTime::from_timestamp(secs as i64, 0).expect("valid secs"),
    ));
    let engine = SearchEngine::new(emb, tk);
    engine.add_reranker(Box::new(RecencyReranker::<Memory>::new(clock, 30.0)));
    engine
}

#[tokio::test]
async fn registry_search_includes_all_registered_engines() {
    let reg = Registry::new();
    let s = reg.register::<Skill>(skill_engine());
    let t = reg.register::<Tool>(tool_engine());
    s.add(Skill {
        id: "pdf".into(),
        name: "PDF".into(),
        description: "PDF 提取".into(),
        when_to_use: vec![],
        not_for: vec![],
        tags: vec_str(&["pdf"]),
        examples: vec![],
    })
    .unwrap();
    t.add(Tool {
        id: "weather".into(),
        name: "天气".into(),
        description: "查天气".into(),
        when_to_use: vec![],
        not_for: vec![],
        tags: vec![],
        schema_hint: "".into(),
    })
    .unwrap();

    let hits = reg
        .search(&QueryContext::new("PDF 提取 天气").top_k(5))
        .await;
    let ids: Vec<String> = hits.iter().map(|h| h.id.clone()).collect();
    assert!(ids.contains(&"pdf".to_string()));
    assert!(ids.contains(&"weather".to_string()));
}

#[test]
fn recency_reranker_decays_old_memory() {
    let clock_secs = 1_700_000_000_u64;
    let e = memory_engine_with_clock(clock_secs);
    let old_ts = (clock_secs - 200 * 86_400) as f64;
    e.add(Memory {
        id: "m_old".into(),
        summary: "very old".into(),
        content: "x".into(),
        tags: vec![],
        created_at: old_ts,
        importance: 0.5,
    })
    .unwrap();
    let recent_ts = (clock_secs - 86_400) as f64;
    e.add(Memory {
        id: "m_new".into(),
        summary: "very new".into(),
        content: "x".into(),
        tags: vec![],
        created_at: recent_ts,
        importance: 0.5,
    })
    .unwrap();

    let hits = e.search(&QueryContext::new("x").top_k(5));
    assert_eq!(hits.len(), 2);
    let new_hit = hits.iter().find(|h| h.id == "m_new").unwrap();
    let old_hit = hits.iter().find(|h| h.id == "m_old").unwrap();
    assert!(new_hit.score > old_hit.score);
}

#[tokio::test]
async fn typed_handle_and_registry_share_state() {
    let reg = Registry::new();
    let handle = reg.register::<Skill>(skill_engine());
    handle
        .add(Skill {
            id: "s1".into(),
            name: "n".into(),
            description: "d".into(),
            when_to_use: vec![],
            not_for: vec![],
            tags: vec![],
            examples: vec![],
        })
        .unwrap();
    assert_eq!(reg.engine_count(), 1);
    let hits = reg.search(&QueryContext::new("n").top_k(5)).await;
    assert!(hits.iter().any(|h| h.id == "s1"));
}

/// Re-adding an existing id tombstones the previous copy — and the
/// tombstone-ratio auto-rebuild must not resurrect it. A host whose
/// source carries duplicate ids (a catalog walked through two roots,
/// say) would otherwise get same-id hits with identical scores.
#[tokio::test]
async fn re_added_ids_collapse_to_one_live_hit_across_a_rebuild() {
    let engine = skill_engine();
    for i in 0..6 {
        // Distinct filler docs force the deleted/alive ratio past the
        // rebuild threshold as the duplicate is re-added.
        engine
            .add(Skill {
                id: format!("filler-{i}"),
                name: format!("filler {i}"),
                description: "shared token".into(),
                when_to_use: vec![],
                not_for: vec![],
                tags: vec![],
                examples: vec![],
            })
            .unwrap();
        engine
            .add(Skill {
                id: "dup".into(),
                name: "dup".into(),
                description: "shared token".into(),
                when_to_use: vec![],
                not_for: vec![],
                tags: vec![],
                examples: vec![],
            })
            .unwrap();
    }

    assert_eq!(engine.len(), 7, "one live entry per id");
    let hits = engine.search(&QueryContext::new("shared token").top_k(50));
    let dups = hits.iter().filter(|h| h.id == "dup").count();
    assert_eq!(dups, 1, "duplicate ids must collapse to one hit: {hits:?}");
    assert_eq!(hits.len(), 7, "every id appears exactly once: {hits:?}");
}
