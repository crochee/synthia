# `synthia-search`: Generic Multi-Domain Search Engine — Design

**Date:** 2026-09-19
**Status:** Draft (pending user review)
**Authors:** Synthia agent + user (collaborative brainstorming, R115+ cycle)
**Scope:** Add a new leaf crate `synthia-search` that ships a generic,
multi-domain search engine (BM25 + vector hybrid, pluggable
Tokenizer/Embedder/VectorStore/Filter/Reranker, incremental add /
tombstone remove, thread-safe `Arc` sharing, type-erased
cross-`T` registry, agent-facing JSON projection). The crate is exposed
through the `synthia` facade under a `search` feature (default OFF).

## §1. Motivation

Synthia agents need to look up skills, tools, memories, and other
"named catalogs" on every turn. The pieces that exist today answer
this in pieces but never with one vocabulary:

- `synthia-rag::Retriever` grounds a single `Document` corpus
  (chunked text), with a `Retriever` seam and `KeywordRetriever`
  (BM25) / `EmbeddingRetriever` (any `Embedder`) / `HybridRetriever`
  (RRF) impls. It is a *grounding* primitive: chunks flow into a
  context manager.
- `synthia-skill::SkillRegistry` deduplicates `Skill` values from
  `SkillProvider`s by name + `SkillRank`. It is a *registry* of
  providers, not a search index.
- `synthia-context::MemoryEntry` retrieval is sqlite/keyword-based
  in the optional `sqlite` feature. It is domain-specific.

What is missing is a generic engine that, given a value of any
`Searchable` type, can:

1. hold many of them concurrently in an in-memory index;
2. retrieve the top-k by a hybrid BM25 + vector score, with
   pluggable tokenizer / embedder / vector store / filter / reranker;
3. add and remove incrementally (tombstone + threshold-triggered
   `compact`, no full rebuild on every `add`);
4. share across threads behind an `Arc<SearchEngine<T>>`;
5. host multiple `SearchEngine<T>` instances under one `Registry`
   keyed by `T`, returning the merged top-k across all engines
   (cross-`T` search);
6. project to a tiny JSON for the agent loop to consume.

The user reports that pure RAG is not enough: the agent needs a
catalog-level "what skill / tool / memory fits this turn?" query
**before** grounding the chosen document. This spec is the
catalog-level answer.

## §2. Goals & non-goals

**Goals**

- One generic engine `SearchEngine<T: Searchable>`, monomorphised
  per `T`. Zero `dyn` indirection on the hot retrieval path.
- Pluggable: `Tokenizer`, `Embedder`, `VectorStore`,
  `Filter<T>`, `Reranker<T>` — all `Send + Sync + 'static` traits.
- Incremental: `add` is O(new-doc-token-count) BM25 + O(dim) vector;
  `remove` is tombstone; `compact` rebuilds when tombstone ratio
  exceeds a threshold.
- Thread-safe by construction: `Arc<SearchEngine<T>>` is the handle;
  internal `parking_lot::RwLock` serialises writers and allows
  concurrent readers. No `LockPoisoned` ever escapes.
- Cross-`T` registry: `Registry` holds `Arc<dyn ErasedEngine>` keyed
  by `std::any::type_name::<T>()`. `register` returns a typed
  `Arc<SearchEngine<T>>` so the caller retains type-safe
  `add` / `remove` after registration. `Registry::search` returns
  the merged top-k across every engine.
- Agent-facing projection: `agent_view(&[Hit])` and
  `search_tool(&Registry, &str, usize)` return pretty JSON of the top
  candidates with reasons.
- Runtime-neutral: no `tokio` / `async-std` / `smol` in the public
  API. The trait surface is sync (search is pure CPU). No HTTP client.
- `Searchable: RegistryItem` so every `T` participates in
  `synthia_core::registry::RegistryItem` and any future
  `Registry<Searchable>` listing will Just Work without a parallel
  type system.

**Non-goals**

- No persistence layer. Index lives in process memory.
- No ANN. `FlatVectorStore` is provided; `HnswStore` is left as a
  trait seam for a future PR. The flat store is correct for lib
  consumer scale (~10k items per engine).
- No domain-specific rerankers beyond the two shipped
  (`RuleReranker`, `RecencyReranker`).
- No LLM-based reranker in this PR.
- No edits to `synthia-rag` / `synthia-skill` / `synthia-context` /
  `synthia-harness` / `synthia-server`. This PR only adds the new
  crate and the facade plumbing.

## §3. Workspace & dependency shape

```
synthia-search                  # new leaf crate
  └─ synthia-core               # RegistryItem, Clock, Result, Error
  └─ serde, serde_json          # Hit / AgentCandidate wire
  └─ thiserror                  # lib error enum
  └─ tracing                    # diagnostics
  └─ synthia-provider           # OPTIONAL, gated by feature "provider"
     (default-features = false; never pulls reqwest)
```

The `provider` feature exists so a lib consumer that wants
`ModelProviderEmbedder` (an adapter from
`synthia_provider::ModelProvider::embed` to `Embedder`) can opt in
without forcing every consumer to download `reqwest`. The feature is
**OFF by default**.

Verification:

- `cargo tree -p synthia-search --no-default-features` excludes
  `reqwest|hyper|rustls|h2|tower`.
- `cargo tree -p synthia-search --features provider` excludes
  `reqwest|hyper|rustls|h2|tower` (provider with default-features
  off has no HTTP client; the lib consumer who actually wants HTTP
  flips the `provider-anthropic` / `provider-openai` features in
  *their* crate).
- `cargo check -p synthia-search --lib` excludes `tokio`.

The facade crate `synthia` gains an optional dependency on
`synthia-search` and a `search` feature (OFF by default). The crate
ships its own surface `synthia::search` re-exporting
`synthia-search`'s public items.

## §4. Public module layout

```
crates/synthia-search/src/
  lib.rs            # zero-logic facade; cfg-gated `mod provider`
  error.rs          # SearchError / Result<T>
  searchable.rs     # trait Searchable: RegistryItem + Send + Sync + Clone + 'static
  tokenizer.rs      # trait Tokenizer + CjkTokenizer (with CJK bigram)
  embedder.rs       # trait Embedder + HashingEmbedder (fnv1a hashing, default ON)
  bm25.rs           # struct Bm25Index (k1 = 1.5, b = 0.75)
  vector.rs         # trait VectorStore + FlatVectorStore
  filter.rs         # trait Filter<T> + BasicFilter (tag-only after §7 change)
  reranker.rs       # trait Reranker<T> + RuleReranker + RecencyReranker(Clock)
  hit.rs            # Hit + AgentCandidate + agent_view + search_tool
  engine.rs         # struct SearchEngine<T: Searchable>
  registry.rs       # struct Registry + ErasedEngine (type_name keyed)
  types.rs          # self-contained Skill / Tool / Memory demo values + Searchable impls
  provider.rs       # ModelProviderEmbedder (cfg-gated by feature "provider")
```

No `domain.rs`: the `Domain` enum is removed in v3 per the user's
direction. The type parameter `T` *is* the domain.

## §5. Trait surface

### §5.1 `Searchable`

```rust
use synthia_core::registry::RegistryItem;

pub trait Searchable: RegistryItem + Send + Sync + Clone + 'static {
    /// (text, weight) pairs to feed BM25. Weight rounds up to
    /// repetitions in the concatenated index text so a high-weight
    /// phrase (e.g. a skill name) effectively outranks a long body.
    fn indexed_fields(&self) -> Vec<(String, f32)>;

    /// Triggers: short phrases that describe when this item is the
    /// right pick. Consumed by `RuleReranker`.
    fn when_to_use(&self) -> &[String] { &[] }

    /// Anti-triggers: short phrases that describe when this item is
    /// the wrong pick. Consumed by `RuleReranker`.
    fn not_for(&self) -> &[String] { &[] }

    /// Tag list, intersected by `BasicFilter::required_tags`.
    fn tags(&self) -> &[String] { &[] }

    /// Optional pre-computed embedding. `None` → call
    /// `embedder.embed(embed_text())`.
    fn embedding(&self) -> Option<&[f32]> { None }

    /// Text the embedder sees. Default: concatenate `indexed_fields`
    /// at weight.
    fn embed_text(&self) -> String;
}
```

`RegistryItem` (from `synthia-core`) requires `name() -> &str` and
`description() -> &str`. The standard pattern for a `Searchable`
impl is `name() == id()` and `description() == title() + when_to_use
join`. Doc examples show the pattern.

### §5.2 `Tokenizer`

```rust
pub trait Tokenizer: Send + Sync + 'static {
    fn tokenize(&self, text: &str) -> Vec<String>;
}
```

`CjkTokenizer` (default ON, in `tokenizer.rs`):

- lowercase Latin words;
- emit one token per CJK ideograph / kana / Hangul syllable;
- emit a **CJK bigram** for every pair of consecutive CJK characters
  (so "中文" produces `中 / 文 / 中文`, raising recall for queries
  that use bigram vocabulary);
- strip everything else.

This is a deliberate divergence from
`synthia_rag::keyword::tokenize`, which only emits per-character CJK.
The user's spec calls for bigram; we duplicate the implementation
inside `synthia-search` per the user's "self-contained" requirement
(spec Q2 answer).

### §5.3 `Embedder`

```rust
pub trait Embedder: Send + Sync + 'static {
    fn dim(&self) -> usize;
    fn embed(&self, text: &str) -> Vec<f32>;
}
```

`HashingEmbedder` (default ON):

- FNV-1a 64-bit hash of each token, mapped into `[0, dim)` with a
  sign bit for ±1;
- sums signed counts into a `Vec<f32>(dim)`, then L2-normalises;
- deterministic, dependency-free, CPU-only.

`ModelProviderEmbedder` (feature `provider`, in `provider.rs`):

- wraps `Arc<dyn ModelProvider>` and delegates `embed` to
  `ModelProvider::embed` (which the provider declares in
  `synthia-provider`'s trait surface).

### §5.4 `VectorStore`

```rust
pub trait VectorStore: Send + Sync + 'static {
    fn add(&mut self, v: Vec<f32>) -> Result<usize, SearchError>;
    fn mark_deleted(&mut self, idx: usize);
    fn is_alive(&self, idx: usize) -> bool;
    fn n_alive(&self) -> usize;
    fn dim(&self) -> usize;
    fn search(&self, q: &[f32], top_k: usize) -> Vec<(usize, f32)>;
}
```

`FlatVectorStore` (default ON):

- `Vec<Option<Vec<f32>>>` with `None` for tombstoned slots;
- `search` is a full scan, dot-product with cosine (assumes
  L2-normalised vectors), returns top-k by descending score;
- filters `is_alive` at retrieval time;
- rejects vectors whose length differs from `dim` with
  `SearchError::DimMismatch`.

### §5.5 `Filter<T>`

```rust
pub trait Filter<T: Searchable>: Send + Sync + 'static {
    fn allow(&self, ctx: &QueryContext, item: &T) -> bool;
}
```

`BasicFilter` (default ON, in `filter.rs`): tag intersection against
`ctx.required_tags`. There is **no domain filter** — the `Domain`
enum is gone (§2).

### §5.6 `Reranker<T>`

```rust
pub trait Reranker<T: Searchable>: Send + Sync + 'static {
    fn rerank(&self, ctx: &QueryContext, items: &[T], hits: &mut Vec<Hit>);
}
```

`RuleReranker` (default ON): bonus for `when_to_use` token overlap,
penalty for `not_for` overlap, small bonus per matching tag.

`RecencyReranker` (default ON, requires `T: Memory`): the half-life
decay formula in the user's spec, with **time injection via
`synthia_core::clock::Clock`** so tests can use `FixedClock` and
production uses `SharedClock::system()`. AGENTS §3.8 forbids direct
`chrono::Utc::now()` / `SystemTime::now()` in production.

```rust
impl<T> RecencyReranker<T> {
    pub fn new(clock: Arc<dyn Clock>, half_life_days: f32) -> Self;
    /// Convenience: wrap `SharedClock::system()`.
    pub fn system(half_life_days: f32) -> Self;
}
```

### §5.7 `Hit` / `QueryContext` / `AgentCandidate`

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub item_idx: usize,
    pub id: String,
    pub title: String,
    pub score: f32,
    pub bm25: f32,
    pub vector: f32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct QueryContext {
    pub text: String,
    pub required_tags: Vec<String>,
    pub top_k: usize,             // default 5
    pub extra: HashMap<String, serde_json::Value>,
}
```

`Hit` does **not** carry a `domain` field. `QueryContext` does **not**
carry `domains`. Both removals are mandated by §2's "no domain
variant".

## §6. `SearchEngine<T>`

```rust
pub struct SearchEngine<T: Searchable> {
    inner: parking_lot::RwLock<EngineInner<T>>,
}

struct EngineInner<T: Searchable> {
    items: Vec<T>,
    bm25: Bm25Index,
    vectors: Box<dyn VectorStore>,
    embedder: Arc<dyn Embedder>,
    tokenizer: Arc<dyn Tokenizer>,
    bm25_weight: f32,            // default 0.5
    vector_weight: f32,          // default 0.5
    recall_k: usize,             // default 50
    rerankers: Vec<Box<dyn Reranker<T>>>,
    filters: Vec<Box<dyn Filter<T>>>,
    tombstone_ratio: f32,        // default 0.30; 0.0 disables auto-compact
}
```

Constructor: `SearchEngine::new(embedder, tokenizer)`.

Public methods (all `&self`, no `&mut self`):

- `set_weights(bm25, vector) -> &Self` — fluent.
- `set_recall_k(k) -> &Self`.
- `set_tombstone_ratio(r) -> &Self`.
- `add_reranker(Box<dyn Reranker<T>>) -> &Self`.
- `add_filter(Box<dyn Filter<T>>) -> &Self`.
- `add(item: T) -> Result<usize>` — same id → tombstone old, insert new.
- `add_all(impl IntoIterator<Item = T>) -> Result<()>`.
- `remove(id: &str) -> bool` — tombstone.
- `compact()` — full rebuild, drops tombstones.
- `get(id: &str) -> Option<T>`.
- `len() -> usize` (alive count).
- `is_empty() -> bool`.
- `search(&QueryContext) -> Vec<Hit>`.

`add` tombstone policy:

1. read-lock to check `id_to_idx`;
2. if present: write-lock, tombstone old `bm25` + `vectors`,
   remove from `id_to_idx`;
3. write-lock, append tokens to `bm25`, append vector to
   `vectors`, append item, insert `id_to_idx`;
4. after write-lock release, check tombstone ratio; if exceeded,
   re-acquire write-lock and `compact_locked`.

Errors:

```rust
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("engine not built")]
    NotBuilt,
    #[error("embedding dim mismatch: expected {expected}, got {got}")]
    DimMismatch { expected: usize, got: usize },
    #[error("not found: {0}")]
    NotFound(String),
    #[error("registry is empty")]
    Empty,
}

pub type Result<T> = std::result::Result<T, SearchError>;
```

`LockPoisoned` is intentionally absent: `parking_lot::RwLock` does
not poison. `parking_lot` lock acquisition returns guards directly
(no `Result`), so no defensive wrapper exists;
`SearchError::NotBuilt` is instead reused as the "engine already
initialised" rejection from `SearchEngine::with_vector_store`.

## §7. `Registry` (cross-`T`)

```rust
pub trait ErasedEngine: Send + Sync + 'static {
    fn search_erased(&self, ctx: &QueryContext) -> Vec<Hit>;
    fn len(&self) -> usize;
    fn type_name(&self) -> &'static str;
    fn as_any(&self) -> &dyn std::any::Any;
}

pub struct Registry {
    engines: parking_lot::RwLock<HashMap<String, Arc<dyn ErasedEngine>>>,
}
```

Methods:

- `new() -> Self`.
- `register<T: Searchable>(engine: SearchEngine<T>) -> Arc<SearchEngine<T>>`:
  inserts at key `std::any::type_name::<T>()`, returns typed `Arc`.
- `unregister<T: Searchable>() -> bool` (by type).
- `unregister_by_type_name(&str) -> bool` (escape hatch when the
  caller no longer holds the typed `Arc`).
- `engine_count() -> usize`.
- `search(&QueryContext) -> Vec<Hit>`: for each engine, call
  `search_erased` and merge. Each engine internally does its own
  max-norm (already done by `SearchEngine::search`); the merged
  vector is sorted descending by `score` and truncated to
  `ctx.top_k`. There is **no cross-engine normalisation** — we
  reject that responsibility: each engine author knows the scale of
  its own scores.

The "typed handle" limitation (`Arc<dyn ErasedEngine>` →
`Arc<SearchEngine<T>>` is impossible without storing the typed
`Arc` separately) is documented in the `register` doc comment and
re-surfaced in the README.

### §7.1 `register` typing contract

`Registry::register<T>` stores the engine as
`Arc<dyn ErasedEngine>` *and* returns the same underlying
`Arc<SearchEngine<T>>` to the caller. Internally this is one
`Arc::clone`: the engine is constructed once, wrapped in `Arc`,
and both the dyn slot and the typed return value share the same
heap allocation. The lib consumer's typed handle and the
registry's dyn handle are two pointers to one buffer — there is
no double-storage and no risk of drift.

When `Registry::search` runs, it walks the dyn handles. When the
lib consumer runs `engine.add(...)` on their typed handle, the
write goes through the same buffer the dyn handle points at.
This is the canonical model and the only one that makes the
"typed handle after registration" pattern type-safe without a
second storage site.

`as_any` on `ErasedEngine` is intentionally retained (not relied
upon for the typed handle) so that *ad-hoc* lib consumers who
didn't keep the typed `Arc` can still attempt a `downcast` — but
that path returns nothing useful (the `Arc<dyn ErasedEngine>`
cannot be downcast to `Arc<SearchEngine<T>>` for ownership
reasons). The doc comment on `register` is the source of truth:
keep your typed `Arc<SearchEngine<T>>` returned from `register`.

## §8. Agent-facing projection

```rust
#[derive(Serialize)]
pub struct AgentCandidate {
    pub id: String,
    pub title: String,
    pub why: String,    // reasons.join("; ")
    pub score: f32,
}

pub fn agent_view(hits: &[Hit]) -> serde_json::Result<String>;
pub fn search_tool(reg: &Registry, query: &str, limit: usize) -> String;
```

`search_tool` is the one entry point a server / harness wiring
would call: `QueryContext::new(query).top_k(limit)` → `search` →
`agent_view`. Returns pretty JSON.

## §9. Demo types (`types.rs`)

Three self-contained demo types ship with the crate:

```rust
#[derive(Clone, Serialize, Deserialize)]
pub struct Skill { pub id: String, pub name: String, pub description: String,
                   pub when_to_use: Vec<String>, pub not_for: Vec<String>,
                   pub tags: Vec<String>, pub examples: Vec<String> }

#[derive(Clone, Serialize, Deserialize)]
pub struct Tool  { pub id: String, pub name: String, pub description: String,
                   pub when_to_use: Vec<String>, pub not_for: Vec<String>,
                   pub tags: Vec<String>, pub schema_hint: String }

#[derive(Clone, Serialize, Deserialize)]
pub struct Memory { pub id: String, pub summary: String, pub content: String,
                    pub tags: Vec<String>, pub created_at: f64,
                    pub importance: f32 }
```

Each implements `Searchable` with the obvious mapping. They are
**demo values**: a lib consumer wanting to search real `Skill`s from
`synthia-skill` writes their own `impl Searchable for
synthia_skill::Skill` (one-screen snippet shown in `lib.rs`
doc-comment). No coupling between `synthia-search` and
`synthia-skill` is introduced.

## §10. Facade & feature gate

`crates/synthia/Cargo.toml`:

- Add `synthia-search = { workspace = true, optional = true }` to
  `[dependencies]`.
- Add `search = ["dep:synthia-search", "core"]` to `[features]`.
- The `search` feature is **not** in `default`.

`crates/synthia/src/lib.rs`:

- Add `#[cfg(feature = "search")] pub mod search;`.
- Add the `compile_fail` doctest block mirroring the rag / skill
  pattern: with the feature off, `use synthia::search::SearchEngine;`
  must fail; with the feature on, the module must exist.

`crates/synthia/src/search.rs` (new): `pub use synthia_search::*;`
— pure propagation, zero logic (matches AGENTS §3.7 facade rule).

## §11. AGENTS.md & Makefile updates

`AGENTS.md §1`: add a section describing `synthia-search` as a leaf
crate available through the facade `search` feature (default OFF),
with an optional `provider` feature inside the new crate for
`ModelProviderEmbedder`. State the dependency rule
(`reqwest|hyper|rustls|h2|tower` excluded under both `default` and
`provider`). State the runtime rule (no `tokio` in lib builds).

`Makefile`:

- `check-mvp-deps` list: add `synthia-search` to the no-HTTP-tool
  group and to the seven-feature MVP subset verification.
- `check-no-runtime` list: add `synthia-search` (lib builds exclude
  `tokio`).

Both changes preserve the gate's existing semantics; this PR adds
two new rows to existing recipes, no new recipes.

## §12. Tests & examples

The crate must satisfy the AGENTS §3.7 "three-yes" gate
(lib tests ≥14, one example minimum, facade feature present):

Lib tests (target ≥14, all in inline `#[cfg(test)] mod tests`
following the AGENTS §3.4 layout rule):

1. `searchable_index_text_repeats_by_weight`
2. `cjk_tokenizer_emits_unigrams_and_bigrams`
3. `cjk_tokenizer_lowercases_latin_words`
4. `hashing_embedder_is_l2_normalised`
5. `hashing_embedder_is_deterministic`
6. `bm25_index_add_remove_tombstone_score`
7. `bm25_idf_outranks_common_term`
8. `flat_vector_store_dim_mismatch_rejected`
9. `flat_vector_store_tombstone_filters_in_search`
10. `search_engine_add_get_remove_lifecycle`
11. `search_engine_hybrid_recall_merges_bm25_and_vector`
12. `search_engine_compact_drops_tombstones`
13. `search_engine_auto_compact_triggers_at_threshold`
14. `basic_filter_tag_intersection`
15. `rule_reranker_boosts_when_to_use_overlap`
16. `recency_reranker_uses_injected_clock` (with `FixedClock`)
17. `registry_register_search_returns_typed_arc`
18. `registry_cross_t_merge_truncates_top_k`
19. `registry_unregister_by_type_name`
20. `agent_view_produces_expected_json`
21. `search_tool_round_trips_through_registry`
22. `hit_serializes_with_reasons`

Examples:

- `examples/multi_domain_search.rs` (`required-features = []`):
  register one `SearchEngine<Skill>`, one `SearchEngine<Tool>`,
  one `SearchEngine<Memory>`, all using `HashingEmbedder`;
  query "把这份 PDF 转成文字" / "天气怎么样" / "做全栈开发"
  cross-`T`; print `search_tool` output. Zero network.
- `examples/provider_embedder.rs` (`required-features = ["provider"]`):
  register a `SearchEngine<Skill>` whose embedder is a
  `ModelProviderEmbedder` wrapping a `ReplayProvider` (from
  `synthia-test-support`); assert the embedding is forwarded.

## §13. File-by-file change list

| File | Op | Lines | Note |
|---|---|---|---|
| `Cargo.toml` (workspace root) | edit | +2 | `members` + `workspace.dependencies` |
| `crates/synthia-search/Cargo.toml` | new | ~35 | features: `default = []`, `provider = ["dep:synthia-provider", "core"]` |
| `crates/synthia-search/src/lib.rs` | new | ~50 | cfg-gated `mod provider`, public re-exports |
| `crates/synthia-search/src/error.rs` | new | ~25 | `SearchError` + `Result` |
| `crates/synthia-search/src/searchable.rs` | new | ~55 | `Searchable` trait |
| `crates/synthia-search/src/tokenizer.rs` | new | ~80 | `Tokenizer` + `CjkTokenizer` (bigram) |
| `crates/synthia-search/src/embedder.rs` | new | ~80 | `Embedder` + `HashingEmbedder` |
| `crates/synthia-search/src/bm25.rs` | new | ~150 | `Bm25Index` |
| `crates/synthia-search/src/vector.rs` | new | ~90 | `VectorStore` + `FlatVectorStore` |
| `crates/synthia-search/src/filter.rs` | new | ~50 | `Filter<T>` + `BasicFilter` |
| `crates/synthia-search/src/reranker.rs` | new | ~120 | `Reranker<T>` + `RuleReranker` + `RecencyReranker` |
| `crates/synthia-search/src/hit.rs` | new | ~80 | `Hit` + `AgentCandidate` + `agent_view` + `search_tool` |
| `crates/synthia-search/src/engine.rs` | new | ~290 | `SearchEngine<T>` |
| `crates/synthia-search/src/registry.rs` | new | ~130 | `Registry` + `ErasedEngine` |
| `crates/synthia-search/src/types.rs` | new | ~150 | demo `Skill` / `Tool` / `Memory` |
| `crates/synthia-search/src/provider.rs` | new | ~80 | `ModelProviderEmbedder` (cfg `provider`) |
| `crates/synthia-search/tests/integration.rs` | new | ~120 | cross-`T` + Clock injection |
| `crates/synthia-search/examples/multi_domain_search.rs` | new | ~80 | `required-features = []` |
| `crates/synthia-search/examples/provider_embedder.rs` | new | ~80 | `required-features = ["provider"]` |
| `crates/synthia-search/README.md` | new | ~60 | crate-level (optional, decided at plan) |
| `crates/synthia/src/search.rs` | new | ~10 | `pub use synthia_search::*;` |
| `crates/synthia/Cargo.toml` | edit | +3 | optional dep + feature `search` |
| `crates/synthia/src/lib.rs` | edit | +6 | `pub mod search;` + compile_fail doctest |
| `AGENTS.md` §1 | edit | +25 | describe `synthia-search` + feature matrix |
| `Makefile` | edit | +2 | `check-mvp-deps` + `check-no-runtime` rows |

Total: 25 files (23 new, 5 edited at the workspace level).
Estimated LoC: ~1 800 new Rust, ~95 new documentation / config.

## §14. Verification plan

After implementation:

1. `cargo fmt --all` (mandatory by AGENTS §3.2).
2. `cargo clippy --all-targets --all-features --tests --all -- -D warnings`.
3. `cargo test -p synthia-search --lib` (must pass; ≥22 tests).
4. `cargo test -p synthia-search --features provider --lib`
   (must pass; same tests + `ModelProviderEmbedder` round-trip).
5. `cargo test -p synthia-search` (lib + integration tests).
6. `cargo run -p synthia-search --example multi_domain_search`
   (must print expected top-3 candidates).
7. `cargo run -p synthia-search --features provider --example provider_embedder`
   (must round-trip an embedding through `ReplayProvider`).
8. `cargo test -p synthia --features search --lib`
   (must compile and find `synthia::search::SearchEngine`).
9. `cargo check -p synthia --no-default-features` (must still compile
   with `search` absent, satisfying the compile_fail doctest in §10).
10. `cargo tree -p synthia-search --no-default-features` (no
    `reqwest|hyper|rustls|h2|tower`).
11. `cargo tree -p synthia-search --features provider` (still no
    `reqwest|hyper|rustls|h2|tower`).
12. `make check-mvp-deps` (gates pass with the new row).
13. `make check-no-runtime` (gates pass with the new row).
14. `make check-pub-surface` (no `pub use <非 synthia_*>::*` introduced).
15. `make check-claim-language` (no new absolute claims
    "default registry" / "ships no implementations" etc. in current-state
    docs).
16. `make lint-rust` (zero warnings).
17. `make test-unit` (`-p synthia-search --lib` passes).
18. `make test-crates` (`-p synthia-search` passes end-to-end).

The PR is acceptance-ready when 1–18 all pass.

## §15. Risks & mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `HashingEmbedder` quality too low for production | Med | Low | It is the default for tests / demos; production lib consumers wire `ModelProviderEmbedder` or their own `Embedder`. Document this clearly in `embedder.rs` module docs. |
| `FlatVectorStore` O(N) search too slow at lib consumer scale | Med | Med | Document the scale assumption; ship the trait seam so an `HnswStore` can replace it without touching the engine. |
| `parking_lot::RwLock` poisons in production (should not) | VLow | High | Defence-in-depth wrapper turns any lock failure into `SearchError::NotBuilt`; tests do not exercise this path because it is unreachable in normal use. |
| `RecencyReranker` clock injection breaks existing callers | Low | Low | Both `RecencyReranker::new(clock, …)` and `RecencyReranker::system(…)` are public; demo uses `system`. Doc-comment shows `FixedClock` for tests. |
| Facade `search` feature default OFF surprises consumers | Low | Low | Same posture as `rag` / `skill` / `scheduler` / `mcp`. Documented in AGENTS §1 update. |
| Cross-`T` score scale incompatibility in `Registry::search` | Med | Med | Each engine already does internal max-norm. Cross-engine raw score merge is "best effort"; lib consumers who need calibrated ranking can implement `Reranker<T>` to re-score. Documented. |

## §16. Out of scope (explicit)

- No edits to `synthia-rag`, `synthia-skill`, `synthia-context`,
  `synthia-harness`, `synthia-server`.
- No `HnswStore` implementation.
- No persistence / serialisation of the index to disk.
- No LLM reranker.
- No metrics or tracing-opentelemetry bridges (consumers wrap the
  `search` call site).
- No edits to `synthia-skill`'s `SkillProvider` or to
  `synthia-context`'s `MemoryEntry`. Future PRs may add an
  `impl Searchable for synthia_skill::Skill` and
  `impl Searchable for synthia_context::MemoryEntry` as
  opt-in adapter crates; they are not in this PR.

## §17. Open questions deferred to plan

- Exact `synthia-provider` features to enable when
  `provider = ["dep:synthia-provider", "core"]` is set.
  The plan must run `cargo tree -p synthia-search --features provider`
  and confirm no `reqwest`; if `synthia-provider` minimum features
  pull any HTTP bits, the feature definition is tightened
  (`synthia-provider = { workspace = true, default-features = false,
  features = [...] }`) so the rule still holds.
- README presence: spec keeps it optional; the writing-plans step
  will decide based on file size budget.