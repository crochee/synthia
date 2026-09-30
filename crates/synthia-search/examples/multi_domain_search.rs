//! `multi_domain_search` — register one Skill engine, one Tool
//! engine, one Memory engine, query across all three, print
//! agent-facing JSON. Zero network.

use std::sync::Arc;

use synthia_search::{
    CjkTokenizer,
    Embedder,
    HashingEmbedder,
    RecencyReranker,
    Registry,
    RuleReranker,
    SearchEngine,
    Tokenizer,
    hit::search_tool,
    types::{Memory, Skill, Tool},
};

fn vec_str(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn build_engine<T: synthia_search::Searchable>(
    dim: usize,
) -> (Arc<dyn Tokenizer>, Arc<dyn Embedder>, SearchEngine<T>) {
    let tk: Arc<dyn Tokenizer> = Arc::new(CjkTokenizer);
    let emb: Arc<dyn Embedder> =
        Arc::new(HashingEmbedder::new(dim, tk.clone()));
    let engine = SearchEngine::new(emb.clone(), tk.clone());
    (tk, emb, engine)
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Skill engine with a RuleReranker
    let (tk_sk, _emb_sk, skill_engine): (
        Arc<dyn Tokenizer>,
        Arc<dyn Embedder>,
        SearchEngine<Skill>,
    ) = build_engine(256);
    skill_engine
        .add_reranker(Box::new(RuleReranker::new(tk_sk.clone())))
        .add_all(vec![
            Skill {
                id: "pdf_extract".into(),
                name: "PDF 文本提取".into(),
                description: "从 PDF 提取文本".into(),
                when_to_use: vec_str(&["把 PDF 转成文本", "读 PDF"]),
                not_for: vec_str(&["扫描件 OCR"]),
                tags: vec_str(&["pdf", "document"]),
                examples: vec_str(&["提取报告里的表格"]),
            },
            Skill {
                id: "ocr_scan".into(),
                name: "扫描件 OCR".into(),
                description: "对图片做光学字符识别".into(),
                when_to_use: vec_str(&["识别图片文字"]),
                not_for: vec_str(&["数字版 PDF"]),
                tags: vec_str(&["ocr", "image"]),
                examples: vec_str(&["截图 OCR"]),
            },
            Skill {
                id: "sql_query".into(),
                name: "SQL 查询生成".into(),
                description: "自然语言生成 SQL".into(),
                when_to_use: vec_str(&["写 SQL", "查订单"]),
                not_for: vec_str(&["NoSQL 查询"]),
                tags: vec_str(&["sql", "database"]),
                examples: vec_str(&["查最近 7 天的订单"]),
            },
        ])
        .unwrap();

    // Tool engine
    let (_tk_t, _emb_t, tool_engine): (
        Arc<dyn Tokenizer>,
        Arc<dyn Embedder>,
        SearchEngine<Tool>,
    ) = build_engine(256);
    tool_engine
        .add_all(vec![Tool {
            id: "get_weather".into(),
            name: "天气查询".into(),
            description: "查询城市天气".into(),
            when_to_use: vec_str(&["查天气"]),
            not_for: vec![],
            tags: vec_str(&["weather"]),
            schema_hint: "args: { city: string }".into(),
        }])
        .unwrap();

    // Memory engine with a recency reranker
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let (_tk_m, _emb_m, memory_engine): (
        Arc<dyn Tokenizer>,
        Arc<dyn Embedder>,
        SearchEngine<Memory>,
    ) = build_engine(256);
    memory_engine
        .add_reranker(Box::new(RecencyReranker::<Memory>::system(90.0)))
        .add_all(vec![Memory {
            id: "mem_001".into(),
            summary: "用户偏好语言".into(),
            content: "后端首选 Rust".into(),
            tags: vec_str(&["preference", "language"]),
            created_at: now - 5.0 * 86_400.0,
            importance: 0.9,
        }])
        .unwrap();

    // Registry — one friendly domain label per engine. The label
    // rides on every `Hit.domain`, so the caller can tell which
    // catalog each answer came from.
    let registry = Registry::new();
    let _sh = registry.register_domain("skill", skill_engine);
    let _th = registry.register_domain("tool", tool_engine);
    let _mh = registry.register_domain("memory", memory_engine);

    // Queries
    let queries = [
        "把这份 PDF 转成文字",
        "北京天气",
        "用户喜欢什么语言",
        "全栈开发",
    ];
    for q in &queries {
        println!("\n=== {q} ===");
        println!("{}", search_tool(&registry, q, 3).await);
    }
}
