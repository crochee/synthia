//! `search_tool_demo` — the `search` tool over one two-domain
//! registry, driven the way the agent loop drives it.
//!
//! The host owns the catalogue: it builds one
//! [`SearchEngine`](synthia_search::SearchEngine) per domain — here a
//! skill catalogue and a memory store — registers both under one
//! [`Registry`](synthia_search::Registry), and hands that registry to
//! `register_search_tool`. The tool knows nothing about skills or
//! memories: it fans the query out across whatever the registry holds
//! and returns the agent-facing projection (`id` / `title` / `why` /
//! `score`, best match first), which is exactly what the model needs to
//! decide what to load next.
//!
//! Two properties are printed and asserted:
//!
//! 1. **Deferred exposure** — before the transcript mentions `search`,
//!    the tool list carries name + description with a deliberately
//!    permissive placeholder schema; once a `search` call is in the
//!    transcript, the real `query` / `limit` schema is advertised.
//! 2. **Cross-domain recall** — one query reaches both engines, which
//!    is why this is a plugin over `synthia-search` instead of a pair
//!    of per-domain tools.
//!
//! Offline by construction: no HTTP client, no API key, and the
//! deterministic `HashingEmbedder` (so the output is reproducible).
//! Tail line: `SEARCH-TOOL: OK`.
//!
//! ```bash
//! cargo run --example search_tool_demo -p synthia-tool-search
//! ```

use std::{collections::HashSet, sync::Arc};

use serde_json::json;
use synthia_search::{
    CjkTokenizer,
    Embedder,
    HashingEmbedder,
    Memory,
    Registry,
    SearchEngine,
    Skill,
    Tokenizer,
};
use synthia_tool::{
    Context,
    Tool,
    ToolExposure,
    ToolOutput,
    ToolRegistry,
    project_tool_definitions,
};
use synthia_tool_search::{SEARCH_TOOL_NAME, SearchTool, register_search_tool};

/// The textual projection of a `ToolOutput` — the JSON the model reads.
fn text_of(output: &ToolOutput) -> String {
    output
        .content
        .iter()
        .filter_map(|part| part.text().map(str::to_string))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A tokenizer + embedder pair for one engine. Hashing, not learned:
/// deterministic and CPU-only, which is what keeps this demo offline.
fn tokenizer_and_embedder() -> (Arc<dyn Tokenizer>, Arc<dyn Embedder>) {
    let tokenizer: Arc<dyn Tokenizer> = Arc::new(CjkTokenizer);
    let embedder: Arc<dyn Embedder> =
        Arc::new(HashingEmbedder::new(128, Arc::clone(&tokenizer)));
    (tokenizer, embedder)
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // ---- the host's catalogue: two domains, two engines ------------
    let (tokenizer, embedder) = tokenizer_and_embedder();
    let skills: SearchEngine<Skill> = SearchEngine::new(embedder, tokenizer);
    skills
        .add_all(vec![
            Skill {
                id: "skill:pdf".into(),
                name: "PDF 提取".into(),
                description: "把 PDF 解析成文本".into(),
                when_to_use: vec!["用户给出 PDF 或论文".into()],
                not_for: vec![],
                tags: vec!["pdf".into(), "提取".into()],
                examples: vec!["提取表格".into()],
            },
            Skill {
                id: "skill:chart".into(),
                name: "图表渲染".into(),
                description: "把数据渲染成折线图".into(),
                when_to_use: vec!["需要看图说话".into()],
                not_for: vec![],
                tags: vec!["chart".into()],
                examples: vec![],
            },
        ])
        .expect("skill documents are indexable");

    let (tokenizer, embedder) = tokenizer_and_embedder();
    let memories: SearchEngine<Memory> = SearchEngine::new(embedder, tokenizer);
    memories
        .add_all(vec![
            Memory {
                id: "memory:pdf-pref".into(),
                summary: "PDF 提取偏好".into(),
                content: "用户偏好把 PDF 提取为纯文本，保留段落".into(),
                tags: vec!["pdf".into()],
                created_at: 0.0,
                importance: 1.0,
            },
            Memory {
                id: "memory:tone".into(),
                summary: "回复语气".into(),
                content: "回答保持简短".into(),
                tags: vec![],
                created_at: 0.0,
                importance: 0.5,
            },
        ])
        .expect("memory documents are indexable");

    let registry = Registry::new();
    registry.register(skills);
    registry.register(memories);
    let registry = Arc::new(registry);
    assert_eq!(registry.engine_count(), 2, "two domains are registered");

    // ---- the host registers the tool over that registry -----------
    let tools = ToolRegistry::new();
    assert!(
        register_search_tool(&tools, Arc::clone(&registry)),
        "no core tool owns the name `search`"
    );
    assert_eq!(
        tools.exposure(SEARCH_TOOL_NAME),
        Some(ToolExposure::Deferred)
    );
    // The tool the registry dispatches to, built over the same
    // registry — a second query domain registered later through this
    // `Arc` is visible to it without re-registering the tool.
    let tool = SearchTool::new(Arc::clone(&registry));

    // ---- 1. deferred exposure: placeholder, then the real schema --
    println!("== the tool list before the transcript mentions `search`");
    let cold =
        project_tool_definitions(&tools.descriptors(), &HashSet::new(), None);
    assert_eq!(cold.len(), 1, "one tool registered");
    assert_eq!(
        cold[0].input_schema["additionalProperties"], true,
        "a deferred tool advertises a permissive placeholder schema"
    );
    println!("{}: {}", cold[0].name, cold[0].description);
    println!("schema : {}", cold[0].input_schema);

    println!("\n== after a `search` call is in the transcript");
    let called = HashSet::from([SEARCH_TOOL_NAME.to_string()]);
    let warm = project_tool_definitions(&tools.descriptors(), &called, None);
    assert_eq!(
        warm[0].input_schema,
        tool.parameters(),
        "promotion advertises the tool's own argument schema"
    );
    println!("schema : {}", warm[0].input_schema);

    // ---- 2. one cross-domain call ----------------------------------
    println!("\n== one query, both domains: `pdf 提取`");
    let output = tool
        .call(
            json!({"query": "pdf 提取", "limit": 5}),
            &Context::default(),
        )
        .await;
    assert_eq!(output.is_error, None, "a discovery call is not an error");
    let text = text_of(&output);
    println!("{text}");

    let hits: Vec<serde_json::Value> =
        serde_json::from_str(&text).expect("the projection is JSON");
    let ids: Vec<&str> = hits
        .iter()
        .map(|hit| hit["id"].as_str().unwrap_or_default())
        .collect();
    assert!(
        ids.iter().any(|id| id.starts_with("skill:")),
        "the skills engine must answer: {ids:?}"
    );
    assert!(
        ids.iter().any(|id| id.starts_with("memory:")),
        "the memory engine must answer: {ids:?}"
    );
    assert!(
        hits.iter()
            .all(|hit| hit["title"].as_str().is_some_and(|t| !t.is_empty())),
        "every hit carries a title: {text}"
    );
    assert!(
        hits.iter().any(|hit| {
            hit["why"].as_str().is_some_and(|why| !why.is_empty())
        }),
        "hits explain why they matched: {text}"
    );

    println!("\nSEARCH-TOOL: OK");
}
