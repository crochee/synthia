# `synthia-search`

Generic, multi-domain search engine for Synthia agents.

- BM25 + vector hybrid scoring.
- Pluggable `Tokenizer` / `Embedder` / `VectorStore` / `Filter<T>` / `Reranker<T>`.
- Incremental `add` / tombstone `remove` / threshold-triggered `compact`.
- Thread-safe by construction: `Arc<SearchEngine<T>>`.
- Cross-`T` `Registry`, keyed by domain label: `register` uses
  `std::any::type_name::<T>()`, `register_domain` / `register_erased`
  take a host-chosen label (`"tool"`, `"skill"`, …). Every hit is
  stamped with its engine's label (`Hit::domain`), and
  `QueryContext::domains` filters which engines run.
- `Registry::search` is async: adapter engines over async retrievers
  (session full-text index, memory recall) join the same merge
  without a blocking bridge; CPU-only `SearchEngine<T>` values
  complete without yielding.
- Agent-facing JSON projection (`agent_view` / `search_tool`).
- Runtime-neutral (no `tokio` in public API).
- Zero `reqwest` / `hyper` / `rustls` dependencies.

## Features

| Feature | Default | What it enables |
|---|---|---|
| (default) | yes | `HashingEmbedder`, all backing indexes |
| `provider` | no | `ModelProviderEmbedder` (adapter over `synthia_provider::ModelProvider`); never pulls `reqwest` |

## Example

```rust
use std::sync::Arc;
use synthia_search::{
    CjkTokenizer, HashingEmbedder, SearchEngine, QueryContext, Tokenizer, Embedder,
};
use synthia_search::types::Skill;

let tk: Arc<dyn Tokenizer> = Arc::new(CjkTokenizer);
let emb: Arc<dyn Embedder> = Arc::new(HashingEmbedder::new(256, tk.clone()));
let engine: SearchEngine<Skill> = SearchEngine::new(emb, tk);
engine.add(Skill {
    id: "pdf".into(),
    name: "PDF 提取".into(),
    description: "从 PDF 提取文本".into(),
    when_to_use: vec!["把 PDF 转成文本".into()],
    not_for: vec!["扫描件 OCR".into()],
    tags: vec!["pdf".into()],
    examples: vec![],
}).unwrap();

let hits = engine.search(&QueryContext::new("PDF").top_k(5));
println!("{:#?}", hits);
```

See `examples/multi_domain_search.rs` for the cross-`T` registry demo.

## Persistence

`SearchEngine::save` / `SearchEngine::load` persist an engine to disk via a
serde boundary (no engine internals leak into the file format):

```rust
engine.save(&root)?;                                          // snapshot + flush op-log
let engine = SearchEngine::load(&root, embedder, tokenizer)?; // missing dir -> empty engine
```

Layout under a root directory:

- `snapshot.json` — full state, written atomically on save.
- `ops.jsonl` — incremental add/remove operations, appended as they happen;
  truncated on explicit `save()` / `compact()`.

Semantics worth knowing:

- Vectors are persisted with the snapshot, so `load` does **not** re-embed
  documents — non-deterministic (provider-class) embedders restore exactly.
- The methods exist only when `T: Serialize + DeserializeOwned`.
- A missing snapshot directory loads as an empty engine (first run).

## Spec

`docs/superpowers/specs/2026-09-19-synthia-search-design.md`
