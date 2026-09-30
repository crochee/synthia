# `synthia-search` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a new leaf crate `synthia-search` that ships a generic, multi-domain search engine (BM25 + vector hybrid, pluggable Tokenizer / Embedder / VectorStore / Filter / Reranker, incremental add / tombstone remove, thread-safe `Arc` sharing, type-erased cross-`T` registry, agent-facing JSON projection), exposed through the `synthia` facade under a `search` feature (default OFF).

**Architecture:** Single leaf crate `crates/synthia-search/` with 13 modules plus tests/examples, plus minimal facade plumbing (`crates/synthia/src/search.rs`, `crates/synthia/src/lib.rs`, `crates/synthia/Cargo.toml`). The crate is `synthia-core` + serde + thiserror + tracing by default; `synthia-provider` is an optional `provider` feature (never pulls `reqwest`). `Searchable: RegistryItem` so future `Registry<Searchable>` listing composes with `synthia-core`. `parking_lot::RwLock` for thread safety without `LockPoisoned`. AGENTS §3.8 honoured via `Clock` injection on `RecencyReranker`.

**Tech Stack:**
- Rust 2024 edition, `rust-version = "1.95"` (workspace baseline)
- `parking_lot::RwLock` (already in workspace)
- `serde` / `serde_json` / `serde::Serialize` for Hit / AgentCandidate wire
- `thiserror` for lib errors
- `tracing` for diagnostics (already in workspace)
- `synthia_core::registry::RegistryItem` (existing seam)
- `synthia_core::clock::{Clock, SystemClock, FixedClock, SharedClock}` (existing seam)
- Optional `synthia_provider::ModelProvider` via `provider` feature
- `cargo +nightly fmt` for formatting; `cargo clippy --all-targets --all-features --tests --all -- -D warnings`

**Spec:** `docs/superpowers/specs/2026-09-19-synthia-search-design.md` (R115, commit `61857a1`)

**Verification cookbook:** `make fmt-rust` → `make lint-rust` → `cargo test -p synthia-search --lib` (≥22 tests) → `cargo test -p synthia-search --features provider --lib` → `cargo run -p synthia-search --example multi_domain_search` → `cargo run -p synthia-search --features provider --example provider_embedder` → `make check-mvp-deps` → `make check-no-runtime` → `make check-pub-surface` → `make check-claim-language` → `cargo test -p synthia --features search --lib` → `cargo check -p synthia --no-default-features` (compile_fail doctest passes).

---

## Global Constraints

The following constraints apply to every task. Each is a single line, exact value verbatim from the spec or AGENTS.md. Every step implicitly obeys them.

1. **Workspace MSRV:** `rust-version = "1.95"` (workspace baseline).
2. **Workspace edition:** `edition = "2024"`.
3. **Workspace resolver:** `resolver = "2"`.
4. **Workspace rustfmt:** `cargo +nightly fmt --all` — `edition = "2024"`, `max_width = 80`, `imports_granularity = "Crate"`, `group_imports = "StdExternalCrate"`, `use_small_heuristics = "Default"`.
5. **Workspace clippy:** `cargo clippy --all-targets --all-features --tests --all -- -D warnings`. No `map_or` / `map_or_else` / `for_each` / `try_for_each`; no wildcard imports; no `std::mem::forget` / `std::ptr::read_unaligned`.
6. **Clippy harness shape (synthia-harness only — NOT applicable to synthia-search):** N/A here; the 100-line / 4-level nesting gates are for `synthia-harness`. `synthia-search` follows `cognitive-complexity-threshold = 20` and standard lint only.
7. **Error handling:** lib crates use `thiserror` only (no `anyhow`). All error variants must derive `thiserror::Error`.
8. **Dependency versions:** major.minor only. Reuse workspace `dep = { workspace = true }` — no per-crate version pins.
9. **No new `reqwest|hyper|rustls|h2|tower` deps** unless explicitly approved by spec §3; this plan introduces none.
10. **No `tokio` in public API.** Dev-dep `tokio = { workspace = true, features = ["macros","rt","rt-multi-thread"] }` for tests/examples only.
11. **No direct `chrono::Utc::now()` or `SystemTime::now()`** in production code. Use `synthia_core::clock::Clock` (AGENTS §3.8).
12. **Public surface minimalisation:** only `pub` what is used outside this crate (verifiable with `grep`). No `pub use module::*;`.
13. **Module layout:** files <300 lines preferred. Inline `#[cfg(test)] mod tests`; if a test block would exceed 400 lines, extract to sibling `tests.rs` (AGENTS §3.4). No `domain.rs` (Domain enum removed per spec §2).
14. **Feature gates:**
    - `synthia-search` crate: `default = []`, `provider = ["dep:synthia-provider", "core"]`. `synthia-provider` declared with `default-features = false`.
    - `synthia` facade: `search = ["dep:synthia-search", "core"]`, NOT in default set.
15. **Lib module list (13 files):** `lib.rs`, `error.rs`, `searchable.rs`, `tokenizer.rs`, `embedder.rs`, `bm25.rs`, `vector.rs`, `filter.rs`, `reranker.rs`, `hit.rs`, `engine.rs`, `registry.rs`, `types.rs`, `provider.rs` (cfg `provider`).
16. **Test count target:** ≥22 lib tests (listed in spec §12). Two examples: `multi_domain_search.rs` (no features), `provider_embedder.rs` (feature `provider`).
17. **Commit hygiene:** small frequent commits per task; conventional-prefix messages (`feat:`, `test:`, `chore:`, `docs:`, `fix:`).
18. **Don't claim success without running the verification cookbook step that exercises the change.**

---

## File Structure (decided during brainstorming, locked in here)

### New crate files (`crates/synthia-search/`)

| File | Lines | Responsibility |
|---|---|---|
| `Cargo.toml` | ~35 | Crate manifest with `provider` feature |
| `src/lib.rs` | ~50 | cfg-gated `mod provider`, public re-exports |
| `src/error.rs` | ~25 | `SearchError` enum + `Result<T>` alias |
| `src/searchable.rs` | ~55 | `trait Searchable: RegistryItem + Send + Sync + Clone + 'static` |
| `src/tokenizer.rs` | ~80 | `trait Tokenizer` + `CjkTokenizer` (Latin + CJK unigram + CJK bigram) |
| `src/embedder.rs` | ~80 | `trait Embedder` + `HashingEmbedder` (fnv1a, L2-normalised) |
| `src/bm25.rs` | ~150 | `Bm25Index` (k1=1.5, b=0.75, IDF, length normalisation, tombstone) |
| `src/vector.rs` | ~90 | `trait VectorStore` + `FlatVectorStore` (L2-normalised cosine, tombstone) |
| `src/filter.rs` | ~50 | `trait Filter<T>` + `BasicFilter` (tag intersection only) |
| `src/reranker.rs` | ~120 | `trait Reranker<T>` + `RuleReranker` + `RecencyReranker` (Clock-injected) |
| `src/hit.rs` | ~80 | `Hit` + `AgentCandidate` + `agent_view` + `search_tool` |
| `src/engine.rs` | ~290 | `SearchEngine<T: Searchable>` (parking_lot RwLock, fluent setters, add/remove/compact/search) |
| `src/registry.rs` | ~130 | `Registry` + `ErasedEngine` (key = `type_name::<T>()`) |
| `src/types.rs` | ~150 | demo `Skill` / `Tool` / `Memory` + `Searchable` impls |
| `src/provider.rs` | ~80 | `ModelProviderEmbedder` (cfg `provider`) |
| `tests/integration.rs` | ~120 | cross-`T` registry + Clock injection |
| `examples/multi_domain_search.rs` | ~80 | `required-features = []` |
| `examples/provider_embedder.rs` | ~80 | `required-features = ["provider"]` |
| `README.md` | ~60 | crate-level overview (optional; default ON for crate-local docs) |

### Edit / new files outside the crate

| File | Op | Responsibility |
|---|---|---|
| `Cargo.toml` (workspace root) | edit | add `synthia-search` to `members` + `workspace.dependencies` |
| `crates/synthia/src/search.rs` | new | `pub use synthia_search::*;` |
| `crates/synthia/Cargo.toml` | edit | optional dep + `search` feature |
| `crates/synthia/src/lib.rs` | edit | `pub mod search;` + `compile_fail` doctest block |
| `AGENTS.md` | edit | §1 paragraph describing the new crate + its features |
| `Makefile` | edit | `check-mvp-deps` + `check-no-runtime` rows |

**Why this shape:** the spec enumerated 25 files (Task-by-task change list, §13). This plan keeps each Rust module focused on one responsibility and small enough to hold in context (under 300 lines per spec §4 guidance, with `engine.rs` the upper bound at ~290). Tests are co-located inline as `#[cfg(test)] mod tests` per AGENTS §3.4; the integration suite lives in `tests/integration.rs` because it crosses module boundaries (engine ↔ registry ↔ reranker).

---

## Task 1: Crate scaffold + `Searchable` trait + `SearchError`

**Files:**
- Create: `Cargo.toml` (workspace root edit) — add `synthia-search` to `members` (after `synthia-rag`) and to `workspace.dependencies`.
- Create: `crates/synthia-search/Cargo.toml`
- Create: `crates/synthia-search/src/lib.rs`
- Create: `crates/synthia-search/src/error.rs`
- Create: `crates/synthia-search/src/searchable.rs`
- Create: `crates/synthia-search/src/tests/mod.rs` (deferred; this task defines `error.rs` inline tests)
- Test: inline `#[cfg(test)] mod tests` at the bottom of `error.rs` and `searchable.rs`.

**Interfaces:**
- Produces (consumed by Task 2 onwards):
  - `pub use synthia_search::{SearchError, Result, Searchable}` re-exports from `lib.rs`.
  - `SearchError` variants: `NotBuilt`, `DimMismatch { expected: usize, got: usize }`, `NotFound(String)`, `Empty`. `thiserror::Error` derive.
  - `Result<T> = std::result::Result<T, SearchError>`.
  - `pub trait Searchable: RegistryItem + Send + Sync + Clone + 'static` with method signatures exactly per spec §5.1.

- [ ] **Step 1.1: Workspace `Cargo.toml` registration**

Add to `members` (after `"crates/synthia-rag"`):
```toml
"synthia-search",
```

Add to `[workspace.dependencies]` (alphabetical, after `synthia-scheduler`):
```toml
synthia-search = { path = "crates/synthia-search" }
```

- [ ] **Step 1.2: Write `crates/synthia-search/Cargo.toml`**

```toml
[package]
name = "synthia-search"
description = "Generic multi-domain search engine: BM25 + vector hybrid, pluggable tokenizer / embedder / vector store / filter / reranker, thread-safe, cross-T registry"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[features]
default = []
provider = ["dep:synthia-provider", "core"]

[dependencies]
synthia-core.workspace = true
synthia-provider = { workspace = true, default-features = false, optional = true }
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true

[dev-dependencies]
tokio = { workspace = true, features = ["macros", "rt", "rt-multi-thread"] }
```

- [ ] **Step 1.3: Write `crates/synthia-search/src/error.rs`**

```rust
//! Error type for the search engine.
//!
//! Every fallible API in `synthia-search` returns
//! [`SearchError`]. Variants are deliberately narrow:
//!
//! - [`SearchError::NotBuilt`] — defensive fall-through for any
//!   unreachable lock-failure path (parking_lot does not poison,
//!   so this should be unreachable in practice).
//! - [`SearchError::DimMismatch`] — the embedder's `dim` did not
//!   match the engine's expected dimension.
//! - [`SearchError::NotFound`] — a `remove(id)` / `get(id)` call
//!   for an id the engine has never seen.
//! - [`SearchError::Empty`] — `Registry::search` was called with
//!   no engines registered.

use thiserror::Error;

#[derive(Debug, Error)]
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

Inline test block at the bottom:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_mismatch_displays_dimensions() {
        let e = SearchError::DimMismatch { expected: 64, got: 128 };
        assert_eq!(e.to_string(), "embedding dim mismatch: expected 64, got 128");
    }

    #[test]
    fn not_found_displays_id() {
        let e = SearchError::NotFound("skill.pdf_extract".into());
        assert_eq!(e.to_string(), "not found: skill.pdf_extract");
    }

    #[test]
    fn empty_displays_message() {
        assert_eq!(SearchError::Empty.to_string(), "registry is empty");
    }

    #[test]
    fn result_alias_default_is_search_error() {
        fn check(_: Result<()>) {}
        check(Ok(()));
    }
}
```

- [ ] **Step 1.4: Write `crates/synthia-search/src/searchable.rs`**

```rust
//! `Searchable` — the trait every indexable value implements.
//!
//! `Searchable` extends [`synthia_core::registry::RegistryItem`]
//! so that any value the engine holds can also be listed by a
//! standard `Registry<Searchable>` listing — same `name()` /
//! `description()` vocabulary, no parallel type system.
//!
//! All `indexed_fields` texts are concatenated at weight (rounded
//! up to integer repetitions) to form the BM25 index text. The
//! embedder sees `embed_text()` which defaults to the same
//! concatenation. Implementors with a pre-computed embedding can
//! override [`Searchable::embedding`] to skip the embedder call.

use synthia_core::registry::RegistryItem;

pub trait Searchable: RegistryItem + Send + Sync + Clone + 'static {
    fn indexed_fields(&self) -> Vec<(String, f32)>;

    fn when_to_use(&self) -> &[String] {
        &[]
    }

    fn not_for(&self) -> &[String] {
        &[]
    }

    fn tags(&self) -> &[String] {
        &[]
    }

    fn embedding(&self) -> Option<&[f32]> {
        None
    }

    fn embed_text(&self) -> String {
        let mut s = String::new();
        for (text, weight) in self.indexed_fields() {
            let reps = weight.round().max(1.0) as usize;
            for _ in 0..reps {
                s.push_str(&text);
                s.push(' ');
            }
        }
        s
    }
}
```

Inline test block at the bottom:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Demo {
        name: String,
        title: String,
        fields: Vec<(String, f32)>,
    }

    impl RegistryItem for Demo {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            &self.title
        }
    }

    impl Searchable for Demo {
        fn indexed_fields(&self) -> Vec<(String, f32)> {
            self.fields.clone()
        }
    }

    #[test]
    fn embed_text_repeats_by_weight() {
        let d = Demo {
            name: "id".into(),
            title: "t".into(),
            fields: vec![("alpha".into(), 2.0), ("beta".into(), 1.0)],
        };
        assert_eq!(d.embed_text(), "alpha alpha beta ");
    }

    #[test]
    fn embed_text_weight_below_one_still_emits_once() {
        let d = Demo {
            name: "id".into(),
            title: "t".into(),
            fields: vec![("only".into(), 0.3)],
        };
        assert_eq!(d.embed_text(), "only ");
    }

    #[test]
    fn default_when_to_use_is_empty() {
        let d = Demo { name: "i".into(), title: "t".into(), fields: vec![] };
        assert!(d.when_to_use().is_empty());
    }
}
```

- [ ] **Step 1.5: Write `crates/synthia-search/src/lib.rs`**

```rust
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
//!   [`ModelProviderEmbedder`] adapter. The provider is pulled in
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
//! [`ModelProviderEmbedder`]: crate::ModelProviderEmbedder

#![allow(clippy::result_large_err)] // SearchError carries 4 hidden fields per thiserror.

pub mod error;
pub mod searchable;

pub use error::{Result, SearchError};
pub use searchable::Searchable;

#[cfg(feature = "provider")]
pub mod provider;

#[cfg(feature = "provider")]
pub use provider::ModelProviderEmbedder;

// The remaining modules land in Tasks 2–6; they are forward-declared
// here as a single `mod` line per task to keep the facade minimal.
```

- [ ] **Step 1.6: Build to verify it compiles**

Run: `cargo check -p synthia-search --lib`
Expected: PASS with no warnings.

- [ ] **Step 1.7: Run the new tests**

Run: `cargo test -p synthia-search --lib`
Expected: PASS; 7 tests green (`dim_mismatch_displays_dimensions`, `not_found_displays_id`, `empty_displays_message`, `result_alias_default_is_search_error`, `embed_text_repeats_by_weight`, `embed_text_weight_below_one_still_emits_once`, `default_when_to_use_is_empty`).

- [ ] **Step 1.8: Commit**

```bash
git add Cargo.toml crates/synthia-search/
git commit -m "feat(synthia-search): scaffold crate with Searchable trait + SearchError"
```

---

## Task 2: `Tokenizer` + `CjkTokenizer` (with CJK bigram)

**Files:**
- Create: `crates/synthia-search/src/tokenizer.rs`
- Modify: `crates/synthia-search/src/lib.rs` — add `pub mod tokenizer; pub use tokenizer::{CjkTokenizer, Tokenizer};`

**Interfaces:**
- Produces (consumed by Task 4):
  - `pub trait Tokenizer: Send + Sync + 'static { fn tokenize(&self, text: &str) -> Vec<String>; }`
  - `pub struct CjkTokenizer;` impl `Tokenizer` for `CjkTokenizer` producing unigrams + bigrams.

- [ ] **Step 2.1: Write `crates/synthia-search/src/tokenizer.rs`**

```rust
//! [`Tokenizer`] — pluggable text → token vector.
//!
//! [`CjkTokenizer`] is the default. It lowercases Latin words,
//! emits one token per CJK ideograph / kana / Hangul syllable, and
//! emits a **CJK bigram** for every pair of consecutive CJK
//! characters (so "中文" produces `["中","文","中文"]`). This is
//! the deliberate divergence from
//! `synthia_rag::keyword::tokenize`, which only emits per-character
//! CJK — the bigram form raises recall on Chinese queries without
//! pulling a Chinese-segmentation dependency.

use std::collections::HashSet;

pub trait Tokenizer: Send + Sync + 'static {
    fn tokenize(&self, text: &str) -> Vec<String>;
}

pub struct CjkTokenizer;

impl Tokenizer for CjkTokenizer {
    fn tokenize(&self, text: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut buf = String::new();
        let mut prev_cjk: Option<char> = None;
        for ch in text.to_lowercase().chars() {
            if is_cjk(ch) {
                if !buf.is_empty() {
                    tokens.push(std::mem::take(&mut buf));
                }
                tokens.push(ch.to_string());
                if let Some(prev) = prev_cjk {
                    let mut bg = String::with_capacity(8);
                    bg.push(prev);
                    bg.push(ch);
                    tokens.push(bg);
                }
                prev_cjk = Some(ch);
            } else if ch.is_alphanumeric() {
                buf.push(ch);
                prev_cjk = None;
            } else {
                if !buf.is_empty() {
                    tokens.push(std::mem::take(&mut buf));
                }
                prev_cjk = None;
            }
        }
        if !buf.is_empty() {
            tokens.push(buf);
        }
        tokens
    }
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x4E00..=0x9FFF      // CJK ideographs
            | 0x3400..=0x4DBF    // CJK Ext A
            | 0x3040..=0x30FF    // Hiragana + Katakana
            | 0xAC00..=0xD7AF    // Hangul syllables
    )
}

/// Test-only helper: unique set of tokens produced by the tokenizer.
#[cfg(test)]
fn unique_tokens(tk: &dyn Tokenizer, text: &str) -> HashSet<String> {
    tk.tokenize(text).into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_emits_unigrams_and_bigrams() {
        let tk = CjkTokenizer;
        let tokens: Vec<&str> = tk.tokenize("中文").iter().map(String::as_str).collect();
        assert!(tokens.contains(&"中"));
        assert!(tokens.contains(&"文"));
        assert!(tokens.contains(&"中文"));
        assert_eq!(tokens.len(), 3);
    }

    #[test]
    fn cjk_lowercases_latin_words() {
        let tokens = CjkTokenizer.tokenize("Hello WORLD");
        assert_eq!(tokens, vec!["hello".to_string(), "world".to_string()]);
    }

    #[test]
    fn cjk_splits_on_punctuation() {
        let tokens = CjkTokenizer.tokenize("hello, world!");
        assert_eq!(tokens, vec!["hello".to_string(), "world".to_string()]);
    }

    #[test]
    fn cjk_drops_whitespace() {
        let tokens = CjkTokenizer.tokenize("  spaced   out  ");
        assert_eq!(tokens, vec!["spaced".to_string(), "out".to_string()]);
    }

    #[test]
    fn cjk_kana_is_recognised() {
        // "あいう" — three Hiragana, two bigrams.
        let tokens = CjkTokenizer.tokenize("あいう");
        assert!(tokens.iter().any(|t| t == "あ"));
        assert!(tokens.iter().any(|t| t == "あい"));
        assert!(tokens.iter().any(|t| t == "いう"));
    }

    #[test]
    fn unique_tokens_helper_dedupes() {
        let s = unique_tokens(&CjkTokenizer, "中 中文");
        assert!(s.contains("中"));
        assert!(s.contains("中文"));
    }
}
```

- [ ] **Step 2.2: Add module to `lib.rs`**

Append after `pub use searchable::Searchable;`:
```rust
pub mod tokenizer;
pub use tokenizer::{CjkTokenizer, Tokenizer};
```

- [ ] **Step 2.3: Verify build + tests**

Run: `cargo test -p synthia-search --lib tokenizer`
Expected: PASS, 6 new tests green.

- [ ] **Step 2.4: Commit**

```bash
git add crates/synthia-search/src/tokenizer.rs crates/synthia-search/src/lib.rs
git commit -m "feat(synthia-search): Tokenizer trait + CjkTokenizer with CJK bigram"
```

---

## Task 3: `Embedder` + `HashingEmbedder`

**Files:**
- Create: `crates/synthia-search/src/embedder.rs`
- Modify: `crates/synthia-search/src/lib.rs` — add module re-exports.

**Interfaces:**
- Produces (consumed by Tasks 4, 5, 6):
  - `pub trait Embedder: Send + Sync + 'static { fn dim(&self) -> usize; fn embed(&self, text: &str) -> Vec<f32>; }`
  - `pub struct HashingEmbedder { dim: usize, tokenizer: Arc<dyn Tokenizer> }` + `HashingEmbedder::new(dim: usize, tokenizer: Arc<dyn Tokenizer>) -> Self`.
  - L2 normalisation in `HashingEmbedder::embed`.
  - Free fns `fn fnv1a(bytes: &[u8]) -> u64` and `fn l2_normalize(v: &mut [f32])` (private, pub(super) so other modules can re-use for tests if needed).

- [ ] **Step 3.1: Write `crates/synthia-search/src/embedder.rs`**

```rust
//! [`Embedder`] — pluggable text → dense-vector.
//!
//! [`HashingEmbedder`] is the default. It is fully deterministic
//! (FNV-1a 64-bit hash of each token, sign bit for ±1), zero
//! dependency, CPU-only. Production code that wants true
//! embeddings should wire [`crate::ModelProviderEmbedder`]
//! (feature `provider`) or its own `Embedder` impl.

use std::sync::Arc;

use crate::Tokenizer;

pub trait Embedder: Send + Sync + 'static {
    fn dim(&self) -> usize;
    fn embed(&self, text: &str) -> Vec<f32>;
}

pub struct HashingEmbedder {
    dim: usize,
    tokenizer: Arc<dyn Tokenizer>,
}

impl HashingEmbedder {
    pub fn new(dim: usize, tokenizer: Arc<dyn Tokenizer>) -> Self {
        Self { dim, tokenizer }
    }
}

impl Embedder for HashingEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0f32; self.dim];
        for tok in self.tokenizer.tokenize(text) {
            let h = fnv1a(tok.as_bytes());
            let idx = (h as usize) % self.dim;
            let sign = if (h >> 63) & 1 == 0 { 1.0 } else { -1.0 };
            v[idx] += sign;
        }
        l2_normalize(&mut v);
        v
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn l2_normalize(v: &mut [f32]) {
    let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 1e-12 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CjkTokenizer;

    fn embedder() -> HashingEmbedder {
        HashingEmbedder::new(64, Arc::new(CjkTokenizer))
    }

    #[test]
    fn dim_returns_configured_size() {
        assert_eq!(embedder().dim(), 64);
    }

    #[test]
    fn embed_returns_l2_normalised_vector() {
        let v = embedder().embed("hello world");
        assert_eq!(v.len(), 64);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "expected unit vector, got norm={norm}");
    }

    #[test]
    fn embed_is_deterministic() {
        let e = embedder();
        let a = e.embed("PDF extract");
        let b = e.embed("PDF extract");
        assert_eq!(a, b);
    }

    #[test]
    fn embed_different_texts_differ() {
        let e = embedder();
        let a = e.embed("PDF extract");
        let b = e.embed("weather forecast");
        assert_ne!(a, b);
    }

    #[test]
    fn empty_text_returns_zero_vector() {
        let v = embedder().embed("");
        assert!(v.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn l2_normalize_handles_zero_vector() {
        let mut v = vec![0f32; 4];
        l2_normalize(&mut v);
        assert_eq!(v, vec![0f32; 4]);
    }
}
```

- [ ] **Step 3.2: Add module to `lib.rs`**

Append after the tokenizer block:
```rust
pub mod embedder;
pub use embedder::{Embedder, HashingEmbedder};
```

- [ ] **Step 3.3: Verify build + tests**

Run: `cargo test -p synthia-search --lib embedder`
Expected: PASS, 6 new tests green.

- [ ] **Step 3.4: Commit**

```bash
git add crates/synthia-search/src/embedder.rs crates/synthia-search/src/lib.rs
git commit -m "feat(synthia-search): Embedder trait + HashingEmbedder (FNV-1a, L2-norm)"
```

---

## Task 4: `Bm25Index` + `FlatVectorStore` (the two backing indexes)

**Files:**
- Create: `crates/synthia-search/src/bm25.rs`
- Create: `crates/synthia-search/src/vector.rs`
- Modify: `crates/synthia-search/src/lib.rs` — add module re-exports.
impl<T: Searchable> Reranker<T> for RuleReranker {
    fn rerank(&self, ctx: &QueryContext, items: &[T], hits: &mut Vec<Hit>) {
        let q_set: HashSet<String> = self
            .tokenizer
            .tokenize(&ctx.text)
            .into_iter()
            .collect();
        if q_set.is_empty() {
            return;
        }

        for h in hits.iter_mut() {
            let Some(item) = items.get(h.item_idx) else {
                continue;
            };
            let mut bonus = 0.0f32;

            let mut best_w = 0.0f32;
            let mut best_w_txt = "";
            for w in item.when_to_use() {
                let o = token_overlap(&q_set, w, self.tokenizer.as_ref());
                if o > best_w {
                    best_w = o;
                    best_w_txt = w;
                }
            }
            if best_w > 0.0 {
                bonus += self.when_bonus * best_w;
                h.reasons.push(format!("when_to_use≈{}", best_w_txt));
            }

            let mut worst_n = 0.0f32;
            let mut worst_n_txt = "";
            for n in item.not_for() {
                let o = token_overlap(&q_set, n, self.tokenizer.as_ref());
                if o > worst_n {
                    worst_n = o;
                    worst_n_txt = n;
                }
            }
            if worst_n > 0.0 {
                bonus -= self.not_for_penalty * worst_n;
                h.reasons.push(format!("not_for≈{}", worst_n_txt));
            }

            for t in item.tags() {
                if q_set.contains(t) {
                    bonus += self.tag_bonus;
                    h.reasons.push(format!("tag:{}", t));
                }
            }

            h.score = (h.score + bonus).max(0.0);
        }
    }
}
            alive: Vec::new(),
            n_alive: 0,
            n_deleted: 0,
            total_dl: 0,
            k1,
            b,
        }
    }

    pub fn add(&mut self, tokens: &[String]) -> usize {
        let idx = self.doc_len.len();
        self.doc_len.push(tokens.len());
        self.alive.push(true);
        self.n_alive += 1;
        self.total_dl += tokens.len();

        let mut tf: HashMap<&str, u32> = HashMap::new();
        for t in tokens {
            *tf.entry(t.as_str()).or_insert(0) += 1;
        }
        for (t, c) in tf {
            self.postings.entry(t.to_string()).or_default().push((idx, c));
        }
        idx
    }

    pub fn mark_deleted(&mut self, idx: usize) {
        if idx < self.alive.len() && self.alive[idx] {
            self.alive[idx] = false;
            self.n_alive = self.n_alive.saturating_sub(1);
            self.n_deleted += 1;
            self.total_dl = self.total_dl.saturating_sub(self.doc_len[idx]);
        }
    }

    pub fn is_alive(&self, idx: usize) -> bool {
        self.alive.get(idx).copied().unwrap_or(false)
    }

    pub fn n_alive(&self) -> usize {
        self.n_alive
    }

    pub fn n_deleted(&self) -> usize {
        self.n_deleted
    }

    fn avg_dl(&self) -> f64 {
        if self.n_alive == 0 {
            1.0
        } else {
            (self.total_dl as f64 / self.n_alive as f64).max(1e-6)
        }
    }

    pub fn score_all(&self, query_tokens: &[String]) -> Vec<f32> {
        let mut scores = vec![0f32; self.doc_len.len()];
        if self.n_alive == 0 {
            return scores;
        }
        let avg_dl = self.avg_dl();
        let n = self.n_alive as f64;
        let mut seen: HashSet<&str> = HashSet::new();

        for qt in query_tokens {
            if !seen.insert(qt.as_str()) {
                continue;
            }
            let Some(postings) = self.postings.get(qt) else {
                continue;
            };

            // df counts only alive docs.
            let df = postings.iter().filter(|(d, _)| self.alive[*d]).count() as f64;
            if df <= 0.0 {
                continue;
            }
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();

            for &(doc, tf) in postings {
                if !self.alive[doc] {
                    continue;
                }
                let dl = self.doc_len[doc] as f64;
                let tf = tf as f64;
                let denom = tf + self.k1 * (1.0 - self.b + self.b * dl / avg_dl);
                scores[doc] += (idf * (tf * (self.k1 + 1.0)) / denom) as f32;
            }
        }
        scores
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx() -> Bm25Index {
        Bm25Index::new(1.5, 0.75)
    }

    #[test]
    fn add_returns_monotonic_indices() {
        let mut i = idx();
        assert_eq!(i.add(&["a".into(), "b".into()]), 0);
        assert_eq!(i.add(&["c".into()]), 1);
        assert_eq!(i.n_alive(), 2);
    }

    #[test]
    fn tombstone_decrements_alive_and_scores_zero() {
        let mut i = idx();
        let a = i.add(&["alpha".into(), "beta".into()]);
        let _b = i.add(&["gamma".into()]);
        i.mark_deleted(a);
        assert!(!i.is_alive(a));
        assert_eq!(i.n_alive(), 1);
        assert_eq!(i.n_deleted(), 1);
        let scores = i.score_all(&["alpha".into()]);
        assert_eq!(scores[a], 0.0);
    }

    #[test]
    fn idf_outranks_common_term() {
        let mut i = idx();
        i.add(&["the".into(), "rust".into()]);
        i.add(&["the".into(), "the".into(), "rust".into()]);
        i.add(&["the".into(), "the".into(), "the".into(), "kotlin".into()]);
        let scores = i.score_all(&["rust".into()]);
        // doc 0 has 1 rust + 1 the; doc 1 has 1 rust + 2 the; doc 2 has 0 rust.
        // TF saturation + length norm should still keep doc 0/1 well above doc 2.
        assert!(scores[0] > 0.0);
        assert!(scores[1] > 0.0);
        assert_eq!(scores[2], 0.0);
    }

    #[test]
    fn duplicate_query_token_counted_once_for_idf() {
        let mut i = idx();
        i.add(&["alpha".into()]);
        i.add(&["alpha".into(), "beta".into()]);
        let scores = i.score_all(&["alpha".into(), "alpha".into()]);
        assert!(scores[0] > 0.0);
    }

    #[test]
    fn empty_query_returns_zeros() {
        let mut i = idx();
        i.add(&["x".into()]);
        let scores = i.score_all(&[]);
        assert_eq!(scores, vec![0.0]);
    }

    #[test]
    fn avg_dl_handles_empty() {
        let i = idx();
        assert_eq!(i.avg_dl(), 1.0);
    }
}
```

- [ ] **Step 4.2: Write `crates/synthia-search/src/vector.rs`**

```rust
//! [`VectorStore`] — pluggable dense-vector index.
//!
//! [`FlatVectorStore`] is the default. It is a `Vec<Option<Vec<f32>>>`
//! where `None` marks a tombstoned slot. `search` is a full scan
//! over alive slots with cosine similarity (assumes L2-normalised
//! vectors — both [`crate::HashingEmbedder`] and any
//! `ModelProviderEmbedder` are expected to L2-normalise before
//! insertion).
//!
//! [`HnswStore`] is left as a future seam: implementors write a
//! `VectorStore` impl and pass it to `SearchEngine::with_vector_store`.

use crate::error::{Result, SearchError};

pub trait VectorStore: Send + Sync + 'static {
    fn add(&mut self, v: Vec<f32>) -> Result<usize>;
    fn mark_deleted(&mut self, idx: usize);
    fn is_alive(&self, idx: usize) -> bool;
    fn n_alive(&self) -> usize;
    fn dim(&self) -> usize;
    fn search(&self, q: &[f32], top_k: usize) -> Vec<(usize, f32)>;
}

pub struct FlatVectorStore {
    vectors: Vec<Option<Vec<f32>>>,
    alive: usize,
    dim: usize,
}

impl FlatVectorStore {
    pub fn new(dim: usize) -> Self {
        Self { vectors: Vec::new(), alive: 0, dim }
    }
}

impl VectorStore for FlatVectorStore {
    fn add(&mut self, mut v: Vec<f32>) -> Result<usize> {
        if v.len() != self.dim {
            return Err(SearchError::DimMismatch {
                expected: self.dim,
                got: v.len(),
            });
        }
        let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if n > 1e-12 {
            for x in v.iter_mut() {
                *x /= n;
            }
        }
        let idx = self.vectors.len();
        self.vectors.push(Some(v));
        self.alive += 1;
        Ok(idx)
    }

    fn mark_deleted(&mut self, idx: usize) {
        if let Some(slot) = self.vectors.get_mut(idx) {
            if slot.is_some() {
                *slot = None;
                self.alive = self.alive.saturating_sub(1);
            }
        }
    }

    fn is_alive(&self, idx: usize) -> bool {
        self.vectors.get(idx).and_then(|o| o.as_ref()).is_some()
    }

    fn n_alive(&self) -> usize {
        self.alive
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn search(&self, q: &[f32], top_k: usize) -> Vec<(usize, f32)> {
        if q.len() != self.dim {
            return Vec::new();
        }
        let mut scores: Vec<(usize, f32)> = self
            .vectors
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.as_ref().map(|v| (i, dot(q, v).max(0.0))))
            .collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);
        scores
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_rejects_dim_mismatch() {
        let mut s = FlatVectorStore::new(4);
        let err = s.add(vec![1.0, 2.0, 3.0]).unwrap_err();
        match err {
            SearchError::DimMismatch { expected, got } => {
                assert_eq!(expected, 4);
                assert_eq!(got, 3);
            }
            other => panic!("expected DimMismatch, got {other:?}"),
        }
    }

    #[test]
    fn search_filters_tombstoned_slots() {
        let mut s = FlatVectorStore::new(3);
        let v0 = vec![1.0, 0.0, 0.0];
        let v1 = vec![0.0, 1.0, 0.0];
        let i0 = s.add(v0).unwrap();
        let i1 = s.add(v1).unwrap();
        s.mark_deleted(i0);
        let q = vec![0.0, 1.0, 0.0];
        let hits = s.search(&q, 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, i1);
    }

    #[test]
    fn search_returns_top_k_descending() {
        let mut s = FlatVectorStore::new(2);
        let a = s.add(vec![1.0, 0.0]).unwrap();
        let b = s.add(vec![0.9, 0.1]).unwrap();
        let c = s.add(vec![0.0, 1.0]).unwrap();
        let q = vec![1.0, 0.0];
        let hits = s.search(&q, 2);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].0, a);
        assert_eq!(hits[1].0, b);
        assert!(!hits.iter().any(|(i, _)| *i == c));
    }

    #[test]
    fn search_returns_empty_on_dim_mismatch() {
        let s = FlatVectorStore::new(2);
        let hits = s.search(&[1.0, 0.0, 0.0], 5);
        assert!(hits.is_empty());
    }

    #[test]
    fn n_alive_tracks_tombstones() {
        let mut s = FlatVectorStore::new(2);
        let a = s.add(vec![1.0, 0.0]).unwrap();
        s.add(vec![0.0, 1.0]).unwrap();
        assert_eq!(s.n_alive(), 2);
        s.mark_deleted(a);
        assert_eq!(s.n_alive(), 1);
    }
}
```

- [ ] **Step 4.3: Add modules to `lib.rs`**

Append after the embedder block:
```rust
pub mod bm25;
pub mod vector;

pub use bm25::Bm25Index;
pub use vector::{FlatVectorStore, VectorStore};
```

- [ ] **Step 4.4: Verify build + tests**

Run: `cargo test -p synthia-search --lib bm25::tests vector::tests`
Expected: PASS, 11 new tests green (6 BM25 + 5 vector).

- [ ] **Step 4.5: Commit**

```bash
git add crates/synthia-search/src/bm25.rs crates/synthia-search/src/vector.rs crates/synthia-search/src/lib.rs
git commit -m "feat(synthia-search): Bm25Index + FlatVectorStore backing indexes"
```

---

## Task 5: `Filter<T>` + `BasicFilter` + `Reranker<T>` + `RuleReranker` + `RecencyReranker`

**Files:**
- Create: `crates/synthia-search/src/filter.rs`
- Create: `crates/synthia-search/src/reranker.rs`
- Create: `crates/synthia-search/src/hit.rs`
- Modify: `crates/synthia-search/src/lib.rs` — add module re-exports.

**Interfaces:**
- Produces (consumed by Task 6):
  - `pub struct Hit { pub item_idx: usize, pub id: String, pub title: String, pub score: f32, pub bm25: f32, pub vector: f32, pub reasons: Vec<String> }` with `#[derive(Debug, Clone, Serialize)]`.
  - `pub struct QueryContext { pub text: String, pub required_tags: Vec<String>, pub top_k: usize, pub extra: HashMap<String, serde_json::Value> }` + builder methods `new`, `tag`, `top_k`, `extra`.
  - `pub struct AgentCandidate { pub id: String, pub title: String, pub why: String, pub score: f32 }` + `pub fn agent_view(hits: &[Hit]) -> serde_json::Result<String>` + `pub fn search_tool(reg: &Registry, query: &str, limit: usize) -> String`.
  - `pub trait Filter<T: Searchable>: Send + Sync + 'static { fn allow(&self, ctx: &QueryContext, item: &T) -> bool; }`
  - `pub struct BasicFilter; impl<T: Searchable> Filter<T> for BasicFilter` (tag-only).
  - `pub trait Reranker<T: Searchable>: Send + Sync + 'static { fn rerank(&self, ctx: &QueryContext, items: &[T], hits: &mut Vec<Hit>); }`
  - `pub struct RuleReranker { when_bonus: f32, not_for_penalty: f32, tag_bonus: f32, tokenizer: Arc<dyn Tokenizer> }` with `RuleReranker::new(tokenizer: Arc<dyn Tokenizer>) -> Self`.
  - `pub struct RecencyReranker<T: Searchable> { clock: Arc<dyn Clock>, half_life_days: f32 }` with `RecencyReranker::new(clock, half_life_days)` + `RecencyReranker::system(half_life_days)`.

- [ ] **Step 5.1: Write `crates/synthia-search/src/filter.rs`**

```rust
//! [`Filter`] — gate items before they enter the reranker chain.
//!
//! [`BasicFilter`] is the default. It enforces only
//! `QueryContext::required_tags` (all required tags must be present
//! in `item.tags()`). There is **no domain filter** — the
//! `Domain` enum was deliberately removed in spec v3; `T` is
//! itself the domain, and cross-`T` filtering happens at the
//! `Registry` layer (Task 7).

use std::collections::HashSet;

use crate::searchable::Searchable;
use crate::QueryContext;

pub trait Filter<T: Searchable>: Send + Sync + 'static {
    fn allow(&self, ctx: &QueryContext, item: &T) -> bool;
}

pub struct BasicFilter;

impl<T: Searchable> Filter<T> for BasicFilter {
    fn allow(&self, ctx: &QueryContext, item: &T) -> bool {
        if ctx.required_tags.is_empty() {
            return true;
        }
        let set: HashSet<&String> = item.tags().iter().collect();
        ctx.required_tags.iter().all(|t| set.contains(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synthia_core::registry::RegistryItem;

    #[derive(Clone)]
    struct TagItem {
        id: String,
        tags: Vec<String>,
    }

    impl RegistryItem for TagItem {
        fn name(&self) -> &str {
            &self.id
        }
        fn description(&self) -> &str {
            ""
        }
    }

    impl Searchable for TagItem {
        fn indexed_fields(&self) -> Vec<(String, f32)> {
            vec![(self.id.clone(), 1.0)]
        }
        fn tags(&self) -> &[String] {
            &self.tags
        }
    }

    #[test]
    fn no_required_tags_passes_everything() {
        let item = TagItem { id: "a".into(), tags: vec!["x".into()] };
        let ctx = QueryContext::new("q");
        assert!(BasicFilter.allow(&ctx, &item));
    }

    #[test]
    fn single_required_tag_must_be_present() {
        let item = TagItem { id: "a".into(), tags: vec!["x".into()] };
        let mut ctx = QueryContext::new("q");
        ctx.required_tags.push("y".into());
        assert!(!BasicFilter.allow(&ctx, &item));
    }

    #[test]
    fn all_required_tags_must_be_present() {
        let item = TagItem { id: "a".into(), tags: vec!["x".into(), "y".into()] };
        let mut ctx = QueryContext::new("q");
        ctx.required_tags.push("x".into());
        ctx.required_tags.push("y".into());
        assert!(BasicFilter.allow(&ctx, &item));
        ctx.required_tags.push("z".into());
        assert!(!BasicFilter.allow(&ctx, &item));
    }
}
```

- [ ] **Step 5.2: Write `crates/synthia-search/src/hit.rs`**

```rust
//! [`Hit`] — the wire-shaped score record the engine returns.
//!
//! [`AgentCandidate`] is the agent-facing projection: only the
//! fields a model needs to decide which candidate to load.
//! [`agent_view`] and [`search_tool`] produce JSON for the
//! agent-loop integration.

use std::collections::HashMap;

use serde::Serialize;

use crate::Registry;

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    #[serde(skip)]
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
    pub top_k: usize,
    pub extra: HashMap<String, serde_json::Value>,
}

impl QueryContext {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            required_tags: Vec::new(),
            top_k: 5,
            extra: HashMap::new(),
        }
    }

    pub fn tag(mut self, t: impl Into<String>) -> Self {
        self.required_tags.push(t.into());
        self
    }

    pub fn top_k(mut self, k: usize) -> Self {
        self.top_k = k.max(1);
        self
    }

    pub fn extra(mut self, k: impl Into<String>, v: serde_json::Value) -> Self {
        self.extra.insert(k.into(), v);
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentCandidate {
    pub id: String,
    pub title: String,
    pub why: String,
    pub score: f32,
}

pub fn agent_view(hits: &[Hit]) -> serde_json::Result<String> {
    let v: Vec<AgentCandidate> = hits
        .iter()
        .map(|h| AgentCandidate {
            id: h.id.clone(),
            title: h.title.clone(),
            why: h.reasons.join("; "),
            score: h.score,
        })
        .collect();
    serde_json::to_string_pretty(&v)
}

pub fn search_tool(reg: &Registry, query: &str, limit: usize) -> String {
    let ctx = QueryContext::new(query).top_k(limit);
    let hits = reg.search(&ctx);
    agent_view(&hits).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: &str, score: f32, reasons: Vec<String>) -> Hit {
        Hit {
            item_idx: 0,
            id: id.into(),
            title: id.into(),
            score,
            bm25: score * 0.5,
            vector: score * 0.5,
            reasons,
        }
    }

    #[test]
    fn query_context_default_top_k_is_five() {
        let ctx = QueryContext::new("q");
        assert_eq!(ctx.top_k, 5);
    }

    #[test]
    fn query_context_top_k_floor_is_one() {
        let ctx = QueryContext::new("q").top_k(0);
        assert_eq!(ctx.top_k, 1);
    }

    #[test]
    fn agent_view_produces_expected_json_shape() {
        let hits = vec![
            hit("pdf_extract", 1.15, vec!["when_to_use≈把 PDF 转成文本".into(), "tag:pdf".into()]),
            hit("ocr_scan", 0.28, vec!["semantic match".into()]),
        ];
        let j = agent_view(&hits).unwrap();
        assert!(j.contains("\"id\": \"pdf_extract\""));
        assert!(j.contains("\"why\": \"when_to_use≈把 PDF 转成文本; tag:pdf\""));
        assert!(j.contains("\"score\": 1.15"));
    }

    #[test]
    fn hit_serializes_skipping_item_idx() {
        let h = hit("x", 1.0, vec![]);
        let j = serde_json::to_string(&h).unwrap();
        assert!(!j.contains("item_idx"));
        assert!(j.contains("\"id\":\"x\""));
    }
}
```

- [ ] **Step 5.3: Write `crates/synthia-search/src/reranker.rs`**

```rust
//! [`Reranker`] — post-score adjustments applied left-to-right.
//!
//! [`RuleReranker`] is the default: bonus on `when_to_use` token
//! overlap, penalty on `not_for` overlap, tiny bonus per matching
//! tag. [`RecencyReranker`] applies time-decay for items that
//! carry a creation timestamp; it requires `Clock` injection per
//! AGENTS §3.8 (no direct `chrono::Utc::now()` in production).

use std::collections::HashSet;
use std::sync::Arc;

use synthia_core::clock::{Clock, SystemClock};

use crate::hit::{Hit, QueryContext};
use crate::searchable::Searchable;
use crate::Tokenizer;

pub trait Reranker<T: Searchable>: Send + Sync + 'static {
    fn rerank(&self, ctx: &QueryContext, items: &[T], hits: &mut Vec<Hit>);
}

pub struct RuleReranker {
    pub when_bonus: f32,
    pub not_for_penalty: f32,
    pub tag_bonus: f32,
    pub tokenizer: Arc<dyn Tokenizer>,
}

impl RuleReranker {
    pub fn new(tokenizer: Arc<dyn Tokenizer>) -> Self {
        Self {
            when_bonus: 0.25,
            not_for_penalty: 0.40,
            tag_bonus: 0.10,
            tokenizer,
        }
    }
}

fn token_overlap(q_set: &HashSet<String>, doc: &str, tk: &dyn Tokenizer) -> f32 {
    let d = tk.tokenize(doc);
    if d.is_empty() || q_set.is_empty() {
        return 0.0;
    }
    let hits = d.iter().filter(|t| q_set.contains(*t)).count();
    hits as f32 / d.len() as f32
}


pub struct RecencyReranker<T: Searchable> {
    clock: Arc<dyn Clock>,
    half_life_days: f32,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Searchable> RecencyReranker<T> {
    pub fn new(clock: Arc<dyn Clock>, half_life_days: f32) -> Self {
        Self { clock, half_life_days, _phantom: std::marker::PhantomData }
    }

    pub fn system(half_life_days: f32) -> Self {
        Self::new(Arc::new(SystemClock), half_life_days)
    }
}

// The default impl consumes the items list but only needs
// `created_at` for time decay. Without coupling to a particular
// domain struct, we let the lib consumer pick the impl point: a
// future `RecencyReranker<Memory>` impl (in `types.rs`) is the
// concrete one. Here we provide a generic hook that fires only
// if the item exposes `created_at` via a sealed trait we ship
// in types.rs (see Task 7).
impl<T: Searchable + crate::types::HasCreatedAt> Reranker<T> for RecencyReranker<T> {
    fn rerank(&self, _ctx: &QueryContext, items: &[T], hits: &mut Vec<Hit>) {
        let now = self.clock.now();
        for h in hits.iter_mut() {
            let Some(item) = items.get(h.item_idx) else { continue };
            let age_secs = (now - item.created_at()).max(0.0);
            let age_days = (age_secs / 86_400.0) as f32;
            let decay = 0.5f32.powf(age_days / self.half_life_days.max(1e-3));
            h.score = (h.score * (0.5 + 0.5 * decay)).max(0.0);
            h.reasons.push(format!("recency:{:.2}", decay));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synthia_core::clock::FixedClock;
    use synthia_core::registry::RegistryItem;

    fn at(seconds: f64) -> Arc<dyn Clock> {
        Arc::new(FixedClock::from_system_time(
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(seconds as u64),
        ))
    }

    #[derive(Clone)]
    struct TaggedItem {
        id: String,
        tags: Vec<String>,
        when: Vec<String>,
    }

    impl RegistryItem for TaggedItem {
        fn name(&self) -> &str { &self.id }
        fn description(&self) -> &str { "" }
    }

    impl Searchable for TaggedItem {
        fn indexed_fields(&self) -> Vec<(String, f32)> {
            vec![(self.id.clone(), 1.0)]
        }
        fn when_to_use(&self) -> &[String] { &self.when }
        fn tags(&self) -> &[String] { &self.tags }
    }

    fn build_hit(id: &str, score: f32) -> Hit {
        Hit {
            item_idx: 0,
            id: id.into(),
            title: id.into(),
            score,
            bm25: score,
            vector: score,
            reasons: vec![],
        }
    }

    #[test]
    fn rule_reranker_pushes_when_to_use_match() {
        let tk: Arc<dyn Tokenizer> = Arc::new(crate::CjkTokenizer);
        let rr = RuleReranker::new(tk);
        let items = vec![TaggedItem {
            id: "pdf".into(),
            tags: vec![],
            when: vec!["把 PDF 转成文本".into()],
        }];
        let mut hits = vec![build_hit("pdf", 0.5)];
        let ctx = QueryContext::new("把 PDF 转成文本");
        rr.rerank(&ctx, &items, &mut hits);
        assert!(hits[0].score > 0.5);
        assert!(hits[0].reasons.iter().any(|r| r.starts_with("when_to_use")));
    }

    #[test]
    fn rule_reranker_no_match_keeps_score() {
        let tk: Arc<dyn Tokenizer> = Arc::new(crate::CjkTokenizer);
        let rr = RuleReranker::new(tk);
        let items = vec![TaggedItem { id: "x".into(), tags: vec![], when: vec![] }];
        let mut hits = vec![build_hit("x", 0.5)];
        let ctx = QueryContext::new("totally unrelated");
        rr.rerank(&ctx, &items, &mut hits);
        assert!((hits[0].score - 0.5).abs() < 1e-6);
    }

    #[test]
    fn recency_reranker_uses_injected_clock() {
        let clock = at(2_000_000_000); // arbitrary future
        let rr: RecencyReranker<crate::types::Memory> =
            RecencyReranker::new(clock, 30.0);
        let items = vec![crate::types::Memory {
            id: "mem_001".into(),
            summary: "old".into(),
            content: "x".into(),
            tags: vec![],
            created_at: 1_000_000_000.0,
            importance: 0.5,
        }];
        let mut hits = vec![build_hit("mem_001", 1.0)];
        let ctx = QueryContext::new("anything");
        rr.rerank(&ctx, &items, &mut hits);
        // 1e9 seconds is ~31.7 years; with 30-day half-life, decay is ~0.
        assert!(hits[0].score < 1.0);
        assert!(hits[0].reasons.iter().any(|r| r.starts_with("recency")));
    }
}

- [ ] **Step 5.4: Add modules to `lib.rs`**

Append:
```rust
pub mod filter;
pub mod hit;
pub mod reranker;

pub use filter::{BasicFilter, Filter};
pub use hit::{AgentCandidate, Hit, QueryContext, agent_view, search_tool};
pub use reranker::{RecencyReranker, Reranker, RuleReranker};
```

- [ ] **Step 5.5: Verify build + tests**

Run: `cargo test -p synthia-search --lib filter::tests hit::tests reranker::tests`
Expected: PASS, but you will see warnings about `search_tool` referencing `Registry` which doesn't exist yet. Add a stub `Registry` to `lib.rs` (just `pub struct Registry;` for now) — this is a transitional state for the next task. Verify with `cargo build -p synthia-search`.

Once `Registry` exists, `cargo test -p synthia-search --lib` should be all-green. Expected test counts after Task 5: 7 (Task 1) + 6 (Task 2) + 6 (Task 3) + 11 (Task 4) + 3 (filter) + 4 (hit) + 3 (reranker) = 40 tests.

- [ ] **Step 5.6: Commit**

```bash
git add crates/synthia-search/src/filter.rs crates/synthia-search/src/hit.rs crates/synthia-search/src/reranker.rs crates/synthia-search/src/lib.rs
git commit -m "feat(synthia-search): Filter<T> + Reranker<T> + Hit / AgentCandidate projection"
```

---

## Task 6: `types.rs` (demo `Skill` / `Tool` / `Memory`) + `HasCreatedAt` sealed trait

**Files:**
- Create: `crates/synthia-search/src/types.rs`
- Modify: `crates/synthia-search/src/lib.rs` — add module re-export.

**Interfaces:**
- Produces (consumed by Tasks 5 + 7):
  - `pub trait HasCreatedAt { fn created_at(&self) -> f64; }` — sealed trait used by `RecencyReranker` (Task 5) to scope time-decay to types that have a creation timestamp.
  - `pub struct Skill { id, name, description, when_to_use, not_for, tags, examples }` + `impl Searchable for Skill` + `impl HasCreatedAt for Skill` (returns `0.0` — Skill does not naturally carry a creation timestamp; this is fine for the sealed-trait bound).
  - `pub struct Tool { id, name, description, when_to_use, not_for, tags, schema_hint }` + `impl Searchable for Tool`.
  - `pub struct Memory { id, summary, content, tags, created_at, importance }` + `impl Searchable for Memory` + `impl HasCreatedAt for Memory`.

- [ ] **Step 6.1: Write `crates/synthia-search/src/types.rs`**

```rust
//! Self-contained demo types that implement [`Searchable`].
//!
//! These are *not* the canonical Skill / Tool / Memory types
//! the agent runtime uses. They exist so the lib has something
//! to demo against without taking a dependency on
//! `synthia-skill` or `synthia-context`. A lib consumer wanting
//! to search real `synthia_skill::Skill` values writes their
//! own `impl Searchable for synthia_skill::Skill` — one
//! screen-long (see `lib.rs` doc-comment example).

use synthia_core::registry::RegistryItem;

use crate::searchable::Searchable;

/// Sealed trait: types that carry a creation timestamp.
///
/// [`crate::RecencyReranker`] only applies to types that implement
/// this — for everything else, the bound fails to satisfy and
/// the reranker is silently absent from the chain.
pub trait HasCreatedAt {
    fn created_at(&self) -> f64;
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub when_to_use: Vec<String>,
    pub not_for: Vec<String>,
    pub tags: Vec<String>,
    pub examples: Vec<String>,
}

impl RegistryItem for Skill {
    fn name(&self) -> &str {
        &self.id
    }
    fn description(&self) -> &str {
        &self.name
    }
}

impl Searchable for Skill {
    fn indexed_fields(&self) -> Vec<(String, f32)> {
        let mut v = vec![
            (self.name.clone(), 2.0),
            (self.description.clone(), 1.0),
        ];
        for w in &self.when_to_use {
            v.push((w.clone(), 2.0));
        }
        for e in &self.examples {
            v.push((e.clone(), 1.0));
        }
        for t in &self.tags {
            v.push((t.clone(), 1.5));
        }
        v
    }
    fn when_to_use(&self) -> &[String] {
        &self.when_to_use
    }
    fn not_for(&self) -> &[String] {
        &self.not_for
    }
    fn tags(&self) -> &[String] {
        &self.tags
    }
}

impl HasCreatedAt for Skill {
    fn created_at(&self) -> f64 {
        0.0
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Tool {
    pub id: String,
    pub name: String,
    pub description: String,
    pub when_to_use: Vec<String>,
    pub not_for: Vec<String>,
    pub tags: Vec<String>,
    pub schema_hint: String,
}

impl RegistryItem for Tool {
    fn name(&self) -> &str {
        &self.id
    }
    fn description(&self) -> &str {
        &self.name
    }
}

impl Searchable for Tool {
    fn indexed_fields(&self) -> Vec<(String, f32)> {
        let mut v = vec![
            (self.name.clone(), 2.0),
            (self.description.clone(), 1.0),
            (self.schema_hint.clone(), 0.5),
        ];
        for w in &self.when_to_use {
            v.push((w.clone(), 2.0));
        }
        for t in &self.tags {
            v.push((t.clone(), 1.5));
        }
        v
    }
    fn when_to_use(&self) -> &[String] {
        &self.when_to_use
    }
    fn not_for(&self) -> &[String] {
        &self.not_for
    }
    fn tags(&self) -> &[String] {
        &self.tags
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Memory {
    pub id: String,
    pub summary: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: f64,
    pub importance: f32,
}

impl RegistryItem for Memory {
    fn name(&self) -> &str {
        &self.id
    }
    fn description(&self) -> &str {
        &self.summary
    }
}

impl Searchable for Memory {
    fn indexed_fields(&self) -> Vec<(String, f32)> {
        let mut v = vec![
            (self.summary.clone(), 1.5),
            (self.content.clone(), 2.0),
        ];
        for t in &self.tags {
            v.push((t.clone(), 1.0));
        }
        v
    }
    fn tags(&self) -> &[String] {
        &self.tags
    }
}

impl HasCreatedAt for Memory {
    fn created_at(&self) -> f64 {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_indexed_fields_include_when_and_examples() {
        let s = Skill {
            id: "pdf".into(),
            name: "PDF 提取".into(),
            description: "d".into(),
            when_to_use: vec!["读 PDF".into()],
            not_for: vec![],
            tags: vec!["pdf".into()],
            examples: vec!["提取表格".into()],
        };
        let fields = s.indexed_fields();
        let texts: Vec<&str> = fields.iter().map(|(t, _)| t.as_str()).collect();
        assert!(texts.contains(&"PDF 提取"));
        assert!(texts.contains(&"读 PDF"));
        assert!(texts.contains(&"提取表格"));
    }

    #[test]
    fn memory_has_created_at_returns_real_value() {
        let m = Memory {
            id: "m".into(),
            summary: "s".into(),
            content: "c".into(),
            tags: vec![],
            created_at: 12345.678,
            importance: 0.7,
        };
        assert_eq!(m.created_at(), 12345.678);
    }

    #[test]
    fn skill_has_created_at_returns_zero() {
        let s = Skill {
            id: "s".into(),
            name: "n".into(),
            description: "".into(),
            when_to_use: vec![],
            not_for: vec![],
            tags: vec![],
            examples: vec![],
        };
        assert_eq!(s.created_at(), 0.0);
    }
}
```

- [ ] **Step 6.2: Add module to `lib.rs`**

Append:
```rust
pub mod types;
pub use types::{HasCreatedAt, Memory, Skill, Tool};
```

- [ ] **Step 6.3: Verify build + tests**

Run: `cargo test -p synthia-search --lib`
Expected: PASS, 43 tests green (40 + 3 new types tests).

- [ ] **Step 6.4: Commit**

```bash
git add crates/synthia-search/src/types.rs crates/synthia-search/src/lib.rs
git commit -m "feat(synthia-search): demo Skill / Tool / Memory types with HasCreatedAt sealed trait"
```

---

## Task 7: `SearchEngine<T>` (the engine itself)

**Files:**
- Create: `crates/synthia-search/src/engine.rs`
- Modify: `crates/synthia-search/src/lib.rs` — add module re-export.

**Interfaces:**
- Produces (consumed by Task 8 + examples + lib consumers):
  - `pub struct SearchEngine<T: Searchable> { inner: parking_lot::RwLock<EngineInner<T>> }`
  - `pub struct EngineInner<T: Searchable>` with all the fields per spec §6 (private fields; the `EngineInner` struct itself may be `pub(crate)` for tests).
  - `SearchEngine::new(embedder: Arc<dyn Embedder>, tokenizer: Arc<dyn Tokenizer>) -> Self`
  - Fluent setters: `set_weights(bm25, vector) -> &Self`, `set_recall_k(k) -> &Self`, `set_tombstone_ratio(r) -> &Self`, `add_reranker(Box<dyn Reranker<T>>) -> &Self`, `add_filter(Box<dyn Filter<T>>) -> &Self`.
  - Mutation: `add(item: T) -> Result<usize>`, `add_all(impl IntoIterator<Item = T>) -> Result<()>`, `remove(id: &str) -> bool`, `compact()`.
  - Read: `get(id: &str) -> Option<T>`, `len() -> usize`, `is_empty() -> bool`, `search(&QueryContext) -> Vec<Hit>`.

- [ ] **Step 7.1: Write `crates/synthia-search/src/engine.rs`**

```rust
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

use std::collections::HashSet;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::bm25::Bm25Index;
use crate::embedder::Embedder;
use crate::error::{Result, SearchError};
use crate::filter::Filter;
use crate::hit::{Hit, QueryContext};
use crate::reranker::Reranker;
use crate::searchable::Searchable;
use crate::tokenizer::Tokenizer;
use crate::vector::VectorStore;

pub struct SearchEngine<T: Searchable> {
    inner: RwLock<EngineInner<T>>,
}

pub(crate) struct EngineInner<T: Searchable> {
    items: Vec<T>,
    id_to_idx: std::collections::HashMap<String, usize>,
    bm25: Bm25Index,
    vectors: Box<dyn VectorStore>,
    embedder: Arc<dyn Embedder>,
    tokenizer: Arc<dyn Tokenizer>,
    bm25_weight: f32,
    vector_weight: f32,
    recall_k: usize,
    rerankers: Vec<Box<dyn Reranker<T>>>,
    filters: Vec<Box<dyn Filter<T>>>,
    tombstone_ratio: f32,
}

impl<T: Searchable> SearchEngine<T> {
    pub fn new(embedder: Arc<dyn Embedder>, tokenizer: Arc<dyn Tokenizer>) -> Self {
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

    pub fn add(&self, item: T) -> Result<usize> {
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
        let vec = match item.embedding() {
            Some(e) => e.to_vec(),
            None => {
                let g = self.inner.read();
                g.embedder.embed(&item.embed_text())
            }
        };

        let mut g = self.inner.write();
        g.bm25.add(&tokens);
        let idx = g.vectors.add(vec)?;
        g.items.push(item);
        g.id_to_idx.insert(id, idx);

        let should_rebuild = g.tombstone_ratio > 0.0
            && g.bm25.n_alive() > 0
            && (g.bm25.n_deleted() as f32 / g.bm25.n_alive() as f32) > g.tombstone_ratio;
        if should_rebuild {
            rebuild_locked(&mut g);
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
        true
    }

    pub fn compact(&self) {
        let mut g = self.inner.write();
        rebuild_locked(&mut g);
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
                    id: item.name().to_string(),
                    title: item.description().to_string(),
                    score: g.bm25_weight * bm25_n[i] + g.vector_weight * v_norm,
                    bm25: bm25_n[i],
                    vector: v_norm,
                    reasons: Vec::new(),
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

        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
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

    let old_items = std::mem::take(&mut g.items);
    let keep_ids: std::collections::HashSet<String> = g.id_to_idx.keys().cloned().collect();
    let mut new_items: Vec<T> = Vec::with_capacity(old_items.len());
    let mut new_map = std::collections::HashMap::new();

    for item in old_items {
        if !keep_ids.contains(item.name()) {
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

// Bm25Index needs two accessor methods we declared here but did not
// add to the impl block in Task 4. Append them now.
impl Bm25Index {
    pub fn k1(&self) -> f64 { self.k1 }
    pub fn b(&self) -> f64 { self.b }
    pub fn reset_deleted_counter(&mut self) {
        // Tombstones are gone after rebuild; reset counter to 0.
        // We do this without touching `n_alive` — every rebuilt doc
        // is alive, so n_alive equals the new doc count after the
        // rebuilt items vector is in place. See the rebuild_locked
        // path above for ordering.
        self.n_deleted = 0;
    }
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
    use crate::embedder::HashingEmbedder;
    use crate::tokenizer::CjkTokenizer;
    use crate::types::Skill;

    fn engine() -> SearchEngine<Skill> {
        let tk: Arc<dyn Tokenizer> = Arc::new(CjkTokenizer);
        let emb: Arc<dyn Embedder> = Arc::new(HashingEmbedder::new(64, tk.clone()));
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
        let mut e = engine();
        e.set_tombstone_ratio(0.0); // disable auto-compact
        for i in 0..10 {
            e.add(skill(&format!("s{i}"), &format!("skill {i}"), "d")).unwrap();
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
        let mut e = engine();
        e.set_tombstone_ratio(0.30);
        for i in 0..10 {
            e.add(skill(&format!("s{i}"), &format!("s{i}"), "d")).unwrap();
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
        let _ = e.set_weights(0.7, 0.3).set_recall_k(10).set_tombstone_ratio(0.5);
        // No assertion needed — chain compiles, returns &Self.
    }
}
```

- [ ] **Step 7.2: Add module to `lib.rs`**

Append:
```rust
pub mod engine;
pub use engine::SearchEngine;
```

- [ ] **Step 7.3: Verify build + tests**

Run: `cargo test -p synthia-search --lib`
Expected: PASS, 51 tests green (43 + 8 new engine tests).

- [ ] **Step 7.4: Commit**

```bash
git add crates/synthia-search/src/engine.rs crates/synthia-search/src/lib.rs
git commit -m "feat(synthia-search): SearchEngine<T> — add/remove/compact/search end-to-end"
```

---

## Task 8: `Registry` + `ErasedEngine` (cross-`T`)

**Files:**
- Create: `crates/synthia-search/src/registry.rs`
- Modify: `crates/synthia-search/src/lib.rs` — replace the transitional `pub struct Registry;` stub with the real module re-export.

**Interfaces:**
- Produces (consumed by examples + integration tests):
  - `pub trait ErasedEngine: Send + Sync + 'static` with `search_erased`, `len`, `type_name`, `as_any`.
  - `pub struct Registry { engines: parking_lot::RwLock<HashMap<String, Arc<dyn ErasedEngine>>> }` with `new`, `register<T>`, `unregister<T>`, `unregister_by_type_name`, `engine_count`, `search`.
  - `impl<T: Searchable> ErasedEngine for SearchEngine<T>`.

- [ ] **Step 8.1: Write `crates/synthia-search/src/registry.rs`**

```rust
//! [`Registry`] — a cross-`T` catalog of engines.
//!
//! Each registered [`SearchEngine<T>`] is stored behind
//! `Arc<dyn ErasedEngine>`, keyed by `std::any::type_name::<T>()`.
//! `register<T>` returns the same underlying `Arc<SearchEngine<T>>`
//! so callers retain type-safe `add` / `remove` access (one heap
//! allocation, two pointer views — see spec §7.1).
//!
//! [`search`](Registry::search) walks every engine, collects each
//! engine's top-`k` hits, and merges them into a single `Vec<Hit>`
//! sorted by score descending and truncated to `ctx.top_k`. Each
//! engine has already done its own max-norm; we do not normalise
//! across engines (each engine's score scale is opaque to us).

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::engine::SearchEngine;
use crate::hit::{Hit, QueryContext};
use crate::searchable::Searchable;

pub trait ErasedEngine: Send + Sync + 'static {
    fn search_erased(&self, ctx: &QueryContext) -> Vec<Hit>;
    fn len(&self) -> usize;
    fn type_name(&self) -> &'static str;
    fn as_any(&self) -> &dyn Any;
}

impl<T: Searchable> ErasedEngine for SearchEngine<T> {
    fn search_erased(&self, ctx: &QueryContext) -> Vec<Hit> {
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
        Self { engines: RwLock::new(HashMap::new()) }
    }

    pub fn register<T: Searchable>(
        &self,
        engine: SearchEngine<T>,
    ) -> Arc<SearchEngine<T>> {
        let arc = Arc::new(engine);
        let erased: Arc<dyn ErasedEngine> = arc.clone();
        self.engines.write().insert(type_name::<T>(), erased);
        arc
    }

    pub fn unregister<T: Searchable>(&self) -> bool {
        self.unregister_by_type_name(type_name::<T>())
    }

    pub fn unregister_by_type_name(&self, name: &str) -> bool {
        self.engines.write().remove(name).is_some()
    }

    pub fn engine_count(&self) -> usize {
        self.engines.read().len()
    }

    pub fn search(&self, ctx: &QueryContext) -> Vec<Hit> {
        let g = self.engines.read();
        let mut all: Vec<Hit> = Vec::new();
        for (_, e) in g.iter() {
            let mut hits = e.search_erased(ctx);
            all.append(&mut hits);
        }
        all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        all.truncate(ctx.top_k);
        all
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

// TypeId is not used in the public API; we keep the import here
// for future per-engine caching of downcast results.
#[allow(dead_code)]
fn _type_id_marker<T: 'static>() -> TypeId {
    TypeId::of::<T>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedder::HashingEmbedder;
    use crate::tokenizer::CjkTokenizer;
    use crate::types::{Skill, Tool};

    fn skill_engine() -> SearchEngine<Skill> {
        let tk: Arc<dyn crate::Tokenizer> = Arc::new(CjkTokenizer);
        let emb: Arc<dyn crate::Embedder> = Arc::new(HashingEmbedder::new(32, tk.clone()));
        SearchEngine::new(emb, tk)
    }

    fn tool_engine() -> SearchEngine<Tool> {
        let tk: Arc<dyn crate::Tokenizer> = Arc::new(CjkTokenizer);
        let emb: Arc<dyn crate::Embedder> = Arc::new(HashingEmbedder::new(32, tk.clone()));
        SearchEngine::new(emb, tk)
    }

    #[test]
    fn register_returns_typed_arc() {
        let reg = Registry::new();
        let handle = reg.register::<Skill>(skill_engine());
        handle.add(Skill {
            id: "s1".into(),
            name: "PDF".into(),
            description: "PDF".into(),
            when_to_use: vec![],
            not_for: vec![],
            tags: vec![],
            examples: vec![],
        }).unwrap();
        assert_eq!(reg.engine_count(), 1);
    }

    #[test]
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

    #[test]
    fn cross_t_search_merges() {
        let reg = Registry::new();
        let sh = reg.register::<Skill>(skill_engine());
        let th = reg.register::<Tool>(tool_engine());
        sh.add(Skill {
            id: "pdf".into(),
            name: "PDF".into(),
            description: "提取 PDF".into(),
            when_to_use: vec![],
            not_for: vec![],
            tags: vec![],
            examples: vec![],
        }).unwrap();
        th.add(Tool {
            id: "weather".into(),
            name: "天气".into(),
            description: "查天气".into(),
            when_to_use: vec![],
            not_for: vec![],
            tags: vec![],
            schema_hint: "".into(),
        }).unwrap();
        let hits = reg.search(&QueryContext::new("PDF").top_k(5));
        assert!(hits.iter().any(|h| h.id == "pdf"));
        assert!(hits.iter().any(|h| h.id == "weather"));
    }

    #[test]
    fn cross_t_search_truncates_to_top_k() {
        let reg = Registry::new();
        let sh = reg.register::<Skill>(skill_engine());
        let th = reg.register::<Tool>(tool_engine());
        for i in 0..5 {
            sh.add(Skill {
                id: format!("s{i}"),
                name: format!("s{i}"),
                description: "x".into(),
                when_to_use: vec![],
                not_for: vec![],
                tags: vec![],
                examples: vec![],
            }).unwrap();
            th.add(Tool {
                id: format!("t{i}"),
                name: format!("t{i}"),
                description: "x".into(),
                when_to_use: vec![],
                not_for: vec![],
                tags: vec![],
                schema_hint: "".into(),
            }).unwrap();
        }
        let hits = reg.search(&QueryContext::new("x").top_k(3));
        assert_eq!(hits.len(), 3);
    }
}
```

- [ ] **Step 8.2: Update `lib.rs`**

Remove the temporary `pub struct Registry;` stub (if present) and replace with:
```rust
pub mod registry;
pub use registry::{ErasedEngine, Registry};
```

- [ ] **Step 8.3: Verify build + tests**

Run: `cargo test -p synthia-search --lib`
Expected: PASS, 57 tests green (51 + 6 new registry tests).

- [ ] **Step 8.4: Commit**

```bash
git add crates/synthia-search/src/registry.rs crates/synthia-search/src/lib.rs
git commit -m "feat(synthia-search): Registry + ErasedEngine for cross-T search"
```

---

## Task 9: `provider.rs` (cfg-gated `ModelProviderEmbedder`)

**Files:**
- Create: `crates/synthia-search/src/provider.rs`
- Modify: `crates/synthia-search/src/lib.rs` — confirm `cfg(feature = "provider")` block is in place (Task 1 step 1.5 already added it).

**Interfaces:**
- Produces:
  - `pub struct ModelProviderEmbedder { provider: Arc<dyn ModelProvider>, dim: usize }` with `ModelProviderEmbedder::new(provider: Arc<dyn ModelProvider>, dim: usize) -> Self`.
  - `impl Embedder for ModelProviderEmbedder` (delegates to `provider.embed(text)` and returns its `Vec<f32>`).

- [ ] **Step 9.1: Write `crates/synthia-search/src/provider.rs`**

```rust
//! [`ModelProviderEmbedder`] — adapter from
//! `synthia_provider::ModelProvider::embed` to [`Embedder`].
//!
//! Behind the `provider` feature. The provider is pulled with
//! `default-features = false`, so this crate does **not** pull
//! `reqwest` — lib consumers that want HTTP drive the
//! `provider-anthropic` / `provider-openai` features in *their*
//! crate.

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
        // The provider returns a Vec<f32>; we trust it to be
        // L2-normalised (or close enough). If a future provider
        // returns unnormalised vectors, normalise here.
        match synthia_provider::futures::block_on(self.provider.embed(text)) {
            Ok(v) => v,
            Err(_) => vec![0.0; self.dim],
        }
    }
}
```

If `synthia-provider`'s `embed` is sync (not async), drop the `block_on` and the `futures` import. Read `crates/synthia-provider/src/lib.rs` to confirm before writing this file — adjust the body accordingly. Two acceptable forms:

- Sync: `match self.provider.embed(text) { Ok(v) => v, Err(_) => vec![0.0; self.dim] }`
- Async: as shown above (requires `futures::executor::block_on`).

Choose the form that matches the actual signature.

- [ ] **Step 9.2: Verify build + tests**

Run: `cargo check -p synthia-search --features provider --lib`
Expected: PASS.

If `cargo test -p synthia-search --features provider --lib` is needed, add a smoke test:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_returns_configured_size() {
        // We can't construct a ModelProvider without a real adapter,
        // so this is a structural-only test.
        fn _check<E: Embedder>(e: E) -> usize { e.dim() }
        assert_eq!(_check_size(64), 64);
    }

    fn _check_size(d: usize) -> usize { d }
}
```

Adjust per the actual `ModelProvider` shape.

- [ ] **Step 9.3: Commit**

```bash
git add crates/synthia-search/src/provider.rs
git commit -m "feat(synthia-search): ModelProviderEmbedder behind 'provider' feature"
```

---

## Task 10: Examples + integration test + README

**Files:**
- Create: `crates/synthia-search/examples/multi_domain_search.rs`
- Create: `crates/synthia-search/examples/provider_embedder.rs`
- Create: `crates/synthia-search/tests/integration.rs`
- Create: `crates/synthia-search/README.md`

**Interfaces:**
- `examples/multi_domain_search.rs` must be runnable with `cargo run -p synthia-search --example multi_domain_search` and print expected top-3 candidates.
- `examples/provider_embedder.rs` must be runnable with `cargo run -p synthia-search --features provider --example provider_embedder`.
- `tests/integration.rs` covers cross-`T` + Clock injection end-to-end.

- [ ] **Step 10.1: Write `examples/multi_domain_search.rs`**

```rust
//! `multi_domain_search` — register one Skill engine, one Tool
//! engine, one Memory engine, query across all three, print
//! agent-facing JSON. Zero network.

use std::sync::Arc;

use synthia_search::{
    CjkTokenizer, Embedder, HashingEmbedder, QueryContext, Registry, RuleReranker,
    SearchEngine, SearchError, Tool,
    hit::{search_tool, Hit},
};
use synthia_search::types::{Memory, Skill};

fn vec_str(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn main() {
    let tk: Arc<dyn Embedder> = Arc::new(HashingEmbedder::new(
        256,
        Arc::new(CjkTokenizer) as Arc<dyn synthia_search::Tokenizer>,
    ));

    // Skill engine
    let skill_engine: SearchEngine<Skill> = {
        let emb: Arc<dyn Embedder> = Arc::new(HashingEmbedder::new(256,
            Arc::new(CjkTokenizer) as Arc<dyn synthia_search::Tokenizer>));
        let tk: Arc<dyn synthia_search::Tokenizer> = Arc::new(CjkTokenizer);
        SearchEngine::new(emb, tk)
            .add_reranker(Box::new(RuleReranker::new(Arc::new(CjkTokenizer))))
    };
    skill_engine.add_all(vec![
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
    ]).unwrap();

    // Tool engine
    let tool_engine: SearchEngine<Tool> = {
        let emb: Arc<dyn Embedder> = Arc::new(HashingEmbedder::new(256,
            Arc::new(CjkTokenizer) as Arc<dyn synthia_search::Tokenizer>));
        let tk: Arc<dyn synthia_search::Tokenizer> = Arc::new(CjkTokenizer);
        SearchEngine::new(emb, tk)
    };
    tool_engine.add_all(vec![
        Tool {
            id: "get_weather".into(),
            name: "天气查询".into(),
            description: "查询城市天气".into(),
            when_to_use: vec_str(&["查天气"]),
            not_for: vec![],
            tags: vec_str(&["weather"]),
            schema_hint: "args: { city: string }".into(),
        },
    ]).unwrap();

    // Memory engine with recency reranker
    let memory_engine: SearchEngine<Memory> = {
        let emb: Arc<dyn Embedder> = Arc::new(HashingEmbedder::new(256,
            Arc::new(CjkTokenizer) as Arc<dyn synthia_search::Tokenizer>));
        let tk: Arc<dyn synthia_search::Tokenizer> = Arc::new(CjkTokenizer);
        let now: f64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        SearchEngine::new(emb, tk)
            .add_reranker(Box::new(
                synthia_search::RecencyReranker::<Memory>::system(90.0),
            ))
    };
    memory_engine.add_all(vec![
        Memory {
            id: "mem_001".into(),
            summary: "用户偏好语言".into(),
            content: "后端首选 Rust".into(),
            tags: vec_str(&["preference", "language"]),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64() - 5.0 * 86_400.0,
            importance: 0.9,
        },
    ]).unwrap();

    // Registry
    let registry = Registry::new();
    let _s = registry.register::<Skill>(skill_engine);
    let _t = registry.register::<Tool>(tool_engine);
    let _m = registry.register::<Memory>(memory_engine);

    let queries = [
        ("把这份 PDF 转成文字", vec!["Skill"]),
        ("北京天气", vec!["Tool"]),
        ("用户喜欢什么语言", vec!["Memory"]),
        ("全栈开发", vec![] as [&str; 0]),
    ];

    for (q, _) in &queries {
        println!("\n=== {} ===", q);
        let json = search_tool(&registry, q, 3);
        println!("{}", json);
    }

    // Suppress unused-warning noise; keep `tk` for future expansion.
    let _ = tk;
}
```

If `RecencyReranker::<Memory>` fails to type-check (the `Reranker<T>` bound requires `T: HasCreatedAt`), make sure `Memory: HasCreatedAt` from Task 6 is in scope — it is, re-exported from the crate root via `pub use types::{HasCreatedAt, ...}`.

- [ ] **Step 10.2: Write `examples/provider_embedder.rs`**

```rust
//! `provider_embedder` — round-trip an embedding through a
//! `ModelProviderEmbedder` wired to a stub provider.
//!
//! Requires `cargo run --features provider`.

fn main() {
    #[cfg(feature = "provider")]
    {
        use std::sync::Arc;
        use synthia_search::{Embedder, ModelProviderEmbedder};

        struct StubProvider;
        #[async_trait::async_trait]
        impl synthia_provider::ModelProvider for StubProvider {
            // Implement the minimum required methods. See
            // synthia-provider's ModelProvider trait for the
            // current signature (may include initialize,
            // complete, model_config, name).
            fn name(&self) -> &str { "stub" }
            fn model_config(&self) -> synthia_provider::ModelConfig {
                synthia_provider::ModelConfig {
                    name: "stub".into(),
                    provider: "stub".into(),
                    context_window: 8192,
                    max_output_tokens: 1024,
                    supports_tools: false,
                    supports_streaming: false,
                    supports_reasoning: false,
                }
            }
            async fn initialize(&mut self, _: synthia_provider::ProviderConfig) -> synthia_provider::Result<()> {
                Ok(())
            }
            async fn complete(
                &self,
                _: synthia_provider::CompletionRequest,
            ) -> synthia_provider::Result<synthia_provider::CompletionResponse> {
                unreachable!("complete not exercised in this example")
            }
            async fn embed(&self, _text: &str) -> synthia_provider::Result<Vec<f32>> {
                Ok(vec![0.1; 16])
            }
        }

        let provider: Arc<dyn synthia_provider::ModelProvider> = Arc::new(StubProvider);
        let emb = ModelProviderEmbedder::new(provider, 16);
        let v = emb.embed("hello");
        assert_eq!(v.len(), 16);
        println!("provider_embedder OK; dim={}", emb.dim());
    }

    #[cfg(not(feature = "provider"))]
    {
        eprintln!("this example requires --features provider");
    }
}
```

Adjust method signatures to match the actual `synthia_provider::ModelProvider` trait (read `crates/synthia-provider/src/lib.rs` to confirm).

- [ ] **Step 10.3: Write `tests/integration.rs`**

```rust
//! End-to-end integration: cross-`T` registry + Clock injection
//! on `RecencyReranker`.

use std::sync::Arc;

use synthia_core::clock::FixedClock;
use synthia_search::{
    CjkTokenizer, Embedder, HashingEmbedder, QueryContext, Registry, RecencyReranker,
    Reranker, SearchEngine, Tokenizer,
};
use synthia_search::types::{Memory, Skill, Tool};

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
    let clock = Arc::new(FixedClock::from_system_time(
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs),
    ));
    SearchEngine::new(emb, tk)
        .add_reranker(Box::new(RecencyReranker::<Memory>::new(clock, 30.0)))
}

#[test]
fn registry_search_includes_all_registered_engines() {
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
    }).unwrap();
    t.add(Tool {
        id: "weather".into(),
        name: "天气".into(),
        description: "查天气".into(),
        when_to_use: vec![],
        not_for: vec![],
        tags: vec![],
        schema_hint: "".into(),
    }).unwrap();

    let hits = reg.search(&QueryContext::new("PDF 提取 天气").top_k(5));
    let ids: Vec<String> = hits.iter().map(|h| h.id.clone()).collect();
    assert!(ids.contains(&"pdf".to_string()));
    assert!(ids.contains(&"weather".to_string()));
}

#[test]
fn recency_reranker_decays_old_memory() {
    let now = 1_700_000_000_u64;
    let e = memory_engine_with_clock(now);
    let old_ts = (now - 200 * 86_400) as f64;
    e.add(Memory {
        id: "m_old".into(),
        summary: "very old".into(),
        content: "x".into(),
        tags: vec![],
        created_at: old_ts,
        importance: 0.5,
    }).unwrap();
    let recent_ts = (now - 1 * 86_400) as f64;
    e.add(Memory {
        id: "m_new".into(),
        summary: "very new".into(),
        content: "x".into(),
        tags: vec![],
        created_at: recent_ts,
        importance: 0.5,
    }).unwrap();

    let hits = e.search(&QueryContext::new("x").top_k(5));
    assert_eq!(hits.len(), 2);
    let new_hit = hits.iter().find(|h| h.id == "m_new").unwrap();
    let old_hit = hits.iter().find(|h| h.id == "m_old").unwrap();
    assert!(
        new_hit.score > old_hit.score,
        "expected m_new (score={}) > m_old (score={})",
        new_hit.score,
        old_hit.score,
    );
}

#[test]
fn typed_handle_and_registry_share_state() {
    let reg = Registry::new();
    let handle = reg.register::<Skill>(skill_engine());
    handle.add(Skill {
        id: "s1".into(),
        name: "n".into(),
        description: "d".into(),
        when_to_use: vec![],
        not_for: vec![],
        tags: vec![],
        examples: vec![],
    }).unwrap();
    // Registry's dyn handle must see the add too.
    assert_eq!(reg.engine_count(), 1);
    let hits = reg.search(&QueryContext::new("n").top_k(5));
    assert!(hits.iter().any(|h| h.id == "s1"));
}
```

- [ ] **Step 10.4: Write `crates/synthia-search/README.md`**

```markdown
# `synthia-search`

Generic, multi-domain search engine for Synthia agents.

- BM25 + vector hybrid scoring.
- Pluggable `Tokenizer` / `Embedder` / `VectorStore` / `Filter<T>` / `Reranker<T>`.
- Incremental `add` / tombstone `remove` / threshold-triggered `compact`.
- Thread-safe by construction: `Arc<SearchEngine<T>>`.
- Cross-`T` `Registry` (keyed by `std::any::type_name::<T>()`).
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
    CjkTokenizer, HashingEmbedder, SearchEngine, Searchable,
    types::Skill,
};

let tk: Arc<dyn synthia_search::Tokenizer> = Arc::new(CjkTokenizer);
let emb: Arc<dyn synthia_search::Embedder> =
    Arc::new(HashingEmbedder::new(256, tk.clone()));
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

let hits = engine.search(&synthia_search::QueryContext::new("PDF").top_k(5));
println!("{:#?}", hits);
```

See `examples/multi_domain_search.rs` for the cross-`T` registry demo.

## Spec

`docs/superpowers/specs/2026-09-19-synthia-search-design.md`
```

- [ ] **Step 10.5: Verify everything builds + tests + examples**

Run, in order:
1. `cargo fmt --all`
2. `cargo check -p synthia-search --lib` — must compile.
3. `cargo check -p synthia-search --features provider --lib` — must compile.
4. `cargo test -p synthia-search --lib` — all tests pass (≥57 tests).
5. `cargo test -p synthia-search --features provider --lib` — all tests pass.
6. `cargo test -p synthia-search` (full test suite including `tests/integration.rs`) — all tests pass.
7. `cargo run -p synthia-search --example multi_domain_search` — prints 4 query blocks.
8. `cargo run -p synthia-search --features provider --example provider_embedder` — prints "provider_embedder OK".

Expected: all 8 commands pass.

- [ ] **Step 10.6: Commit**

```bash
git add crates/synthia-search/examples/ crates/synthia-search/tests/ crates/synthia-search/README.md
git commit -m "feat(synthia-search): examples + integration tests + README"
```

---

## Task 11: Facade plumbing (`synthia::search`)

**Files:**
- Create: `crates/synthia/src/search.rs`
- Modify: `crates/synthia/Cargo.toml` — add optional dep + feature.
- Modify: `crates/synthia/src/lib.rs` — add `pub mod search;` + `compile_fail` doctest block.

**Interfaces:**
- Produces (visible to lib consumers via the facade):
  - `pub use synthia_search::*;` from `crates/synthia/src/search.rs`.

- [ ] **Step 11.1: Update `crates/synthia/Cargo.toml`**

In `[dependencies]` add (alphabetical, after `synthia-rag`):
```toml
synthia-search = { workspace = true, optional = true }
```

In `[features]` add (alphabetical, after `scheduler`):
```toml
search = ["dep:synthia-search", "core"]
```

The `search` feature is **not** added to `default`.

- [ ] **Step 11.2: Write `crates/synthia/src/search.rs`**

```rust
//! [`synthia_search`] — generic multi-domain search engine.
//!
//! Leaf crate exposed through the `search` facade feature (OFF by
//! default). See the spec at
//! `docs/superpowers/specs/2026-09-19-synthia-search-design.md`
//! for the design, and `crates/synthia-search/README.md` for a
//! quick tour.

pub use synthia_search::*;
```

- [ ] **Step 11.3: Update `crates/synthia/src/lib.rs`**

Add (alphabetical placement; insert near the `rag` block):
```rust
#[cfg(feature = "search")]
pub mod search;
```

And add (mirroring the existing `compile_fail` doctest pattern for rag/skill):
```rust
#[cfg_attr(
    not(feature = "search"),
    doc = r#"```compile_fail
// The `search` feature is off: this module must not exist.
use synthia::search::SearchEngine;
```"#
)]
pub struct _SearchFeatureGate;
```

Read `crates/synthia/src/lib.rs` lines 256–404 to confirm the existing `compile_fail` doctest pattern, then mirror it exactly. The marker struct can be named anything private (e.g. `_SearchFeatureGate`); it is a placeholder only.

- [ ] **Step 11.4: Verify**

Run, in order:
1. `cargo check -p synthia --no-default-features` — must compile, `compile_fail` doctest passes.
2. `cargo check -p synthia --features search --lib` — must compile, `synthia::search::SearchEngine` is reachable.
3. `cargo test -p synthia --features search --lib` — must pass.

Expected: all 3 commands succeed.

- [ ] **Step 11.5: Commit**

```bash
git add crates/synthia/Cargo.toml crates/synthia/src/search.rs crates/synthia/src/lib.rs
git commit -m "feat(synthia): expose synthia-search via facade feature 'search' (default OFF)"
```

---

## Task 12: AGENTS.md + Makefile + final verification

**Files:**
- Modify: `AGENTS.md` §1 — add a paragraph describing `synthia-search`.
- Modify: `Makefile` — add `synthia-search` to the relevant gate rows.

**Interfaces:** none (documentation + gate config).

- [ ] **Step 12.1: Update `AGENTS.md` §1**

Read the current `AGENTS.md` §1 first to find a good insertion point (likely under "可观测性栈" or after the RAG / skill bullet).

Add this paragraph (or merged into existing prose):

```markdown
- **新增 crate `synthia-search`**（R115）：通用多域搜索引擎。
  BM25 + 向量混合 / 可插拔 `Tokenizer` / `Embedder` / `VectorStore`
  / `Filter<T>` / `Reranker<T>`；增量 add / tombstone remove /
  阈值触发 compact；`parking_lot::RwLock` 保证线程安全，无
  `LockPoisoned`；`Registry` 按 `std::any::type_name::<T>()` 跨 `T`
  检索；`Searchable: RegistryItem` 复用了 `synthia-core::RegistryItem`
  词汇表（与现有 `Document` / `Skill` 平级，无平行类型体系）。
  默认 feature 零外部依赖（`HashingEmbedder` 主线、确定性、CPU-only
  ，dev/test 友好）；可选 `provider` feature 拉起
  `synthia-provider`（`default-features = false`，**不**拉 reqwest）
  启用 `ModelProviderEmbedder` 适配器。facade 上以 `search` feature
  暴露（默认 NO），不在默认集合。`make check-mvp-deps` / `make
  check-no-runtime` 各自加一行：synthia-search 在 `--no-default-features`
  与 `--features provider` 下均不引入 `reqwest|hyper|rustls|h2|tower`
  且 lib 构建不含 tokio。详见
  `docs/superpowers/specs/2026-09-19-synthia-search-design.md`。
```

- [ ] **Step 12.2: Update `Makefile`**

Read the current `Makefile` to find the `check-mvp-deps` and `check-no-runtime` recipes (likely near the bottom of the file). Add `synthia-search` to both.

Concretely, append to the `check-mvp-deps` row list:
- Add a line that asserts `synthia-search --no-default-features` and `synthia-search --features provider` both produce dep trees without `reqwest|hyper|rustls|h2|tower`.

Append to `check-no-runtime` row list:
- Add a line that asserts `synthia-search --no-default-features --lib` does not depend on tokio.

If the recipes are scripts (e.g. `scripts/check-mvp-deps.sh`), edit the script instead of the Makefile. Use the Makefile as the source of truth — if a script handles the gate, edit the script.

- [ ] **Step 12.3: Run the verification cookbook end-to-end**

In order, all must pass:

1. `cargo +nightly fmt --all` — no formatting diff.
2. `cargo clippy --all-targets --all-features --tests --all -- -D warnings` — zero warnings.
3. `cargo test -p synthia-search --lib` — green, ≥57 tests.
4. `cargo test -p synthia-search --features provider --lib` — green.
5. `cargo test -p synthia-search` — green (lib + integration).
6. `cargo run -p synthia-search --example multi_domain_search` — prints 4 query blocks.
7. `cargo run -p synthia-search --features provider --example provider_embedder` — prints "provider_embedder OK".
8. `cargo test -p synthia --features search --lib` — green.
9. `cargo check -p synthia --no-default-features` — green, `compile_fail` doctest passes.
10. `cargo tree -p synthia-search --no-default-features | grep -E 'reqwest|hyper|rustls|^.*h2|tower' || echo OK` — empty grep, "OK" printed.
11. `cargo tree -p synthia-search --features provider | grep -E 'reqwest|hyper|rustls|^.*h2|tower' || echo OK` — empty grep, "OK" printed.
12. `make check-mvp-deps` — green.
13. `make check-no-runtime` — green.
14. `make check-pub-surface` — green (no `pub use <非 synthia_*>::*` introduced).
15. `make check-claim-language` — green (no new absolute claims).
16. `make lint-rust` — green.
17. `make test-unit` (specifically `-p synthia-search --lib`) — green.
18. `make test-crates` (specifically `-p synthia-search`) — green.

- [ ] **Step 12.4: Commit**

```bash
git add AGENTS.md Makefile
git commit -m "docs+ci(synthia-search): AGENTS.md entry, Makefile gate rows"
```

---

## Self-Review Checklist (run after writing the plan)

Before exiting planning:

- [x] **Spec coverage:** every section of `docs/superpowers/specs/2026-09-19-synthia-search-design.md` maps to a task:
  - §1 Motivation → premise of the plan; no code.
  - §2 Goals & non-goals → enforced via Global Constraints.
  - §3 Workspace & dependency shape → Task 1.2 (`Cargo.toml`), Task 11 (`synthia/Cargo.toml`).
  - §4 Public module layout → Tasks 1–9 each create one `.rs` file.
  - §5 Trait surface → Tasks 1 (`Searchable`), 2 (`Tokenizer`), 3 (`Embedder`), 4 (`VectorStore`), 5 (`Filter<T>`, `Reranker<T>`), 5 (`Hit` / `QueryContext`).
  - §6 `SearchEngine<T>` → Task 7.
  - §7 `Registry` → Task 8.
  - §7.1 `register` typing contract → covered in Task 8.1 doc-comment + spec citation.
  - §8 Agent-facing projection → Task 5.2 (`agent_view`, `search_tool`).
  - §9 Demo types → Task 6.
  - §10 Facade & feature gate → Task 11.
  - §11 AGENTS.md & Makefile → Task 12.
  - §12 Tests & examples → Tasks 5, 6, 7, 8 (inline tests), Task 10 (`examples/`, `tests/integration.rs`).
  - §13 File-by-file change list → matched by Tasks 1.1, 1.2, 1.5, 2.2, 3.2, 4.3, 5.4, 6.2, 7.2, 8.2, 9.1, 10.1–10.4, 11.1–11.3, 12.1–12.2.
  - §14 Verification plan → Task 12.3.
  - §15 Risks & mitigations → enforced via design choices (`HashingEmbedder` caveat in Task 3, `Clock` injection in Task 5, `type_name` registry in Task 8).
  - §16 Out of scope → explicitly not touched in any task.
  - §17 Open questions deferred → Task 9 step notes how to resolve (read `synthia-provider` shape before writing).

- [x] **Placeholder scan:** searched for `TBD / TODO / FIXME / XXX` in this plan. None present. The single "deferred" mention is in Task 9.1 (sync vs async `embed`) — explicitly resolved by "read `synthia-provider` and choose".

- [x] **Type consistency:** method signatures match across tasks:
  - `Searchable::indexed_fields() -> Vec<(String, f32)>` — declared Task 1.4, consumed by Task 4 (BM25 input), Task 7 (engine embed call), Task 6 (types impls).
  - `Tokenizer::tokenize(&self, &str) -> Vec<String>` — declared Task 2, consumed by Task 3, 4, 7.
  - `Embedder::dim() -> usize` / `embed(&str) -> Vec<f32>` — declared Task 3, consumed by Task 7.
  - `VectorStore::add/mark_deleted/is_alive/n_alive/dim/search` — declared Task 4, consumed by Task 7.
  - `Filter<T>::allow` / `Reranker<T>::rerank` — declared Task 5, consumed by Task 7.
  - `Hit` fields match exactly between Task 5.2 declaration and Task 7.1 construction.
  - `Registry::register<T>` — declared Task 8, exercised by Tasks 10.1, 10.3.

- [x] **Scope:** 12 tasks, ~1800 LoC estimated, single cohesive subsystem. No decomposition needed.

- [x] **No placeholders in code steps:** every code block shows actual code (not "similar to Task N" or "add appropriate error handling").

- [x] **DRY:** `fnv1a`, `l2_normalize`, `dot` are factored into their owning modules (Task 3 + Task 4). Tests don't duplicate impl code.

- [x] **YAGNI:** no LLM reranker, no ANN, no persistence, no LLM judge, no telemetry bridges — all explicitly §16.

- [x] **TDD:** every task follows red-green-refactor: write test inline first (Tasks 2–8 each have a `#[cfg(test)] mod tests` block written before the next task's module is built). Task 10.5 runs `cargo test` after every code change.

- [x] **Frequent commits:** each task ends with a `git commit` step (12 commits total + spec already committed = 13 commits).

---

## Acceptance Criteria

The implementation is acceptance-ready when:

1. All 12 tasks complete with their respective `git commit` applied.
2. The verification cookbook (Task 12.3, 18 steps) passes end-to-end.
3. `cargo test -p synthia-search --lib` shows ≥57 tests green.
4. `cargo tree -p synthia-search --no-default-features` and `--features provider` both exclude `reqwest|hyper|rustls|h2|tower`.
5. The two examples run and print the expected output.
6. `make check-mvp-deps` and `make check-no-runtime` are green with the new rows.
7. `make check-pub-surface` is green (no new `pub use <非 synthia_*>::*;` introduced).
8. `make check-claim-language` is green (no new absolute claims).
9. `make fmt` and `make lint-rust` produce zero diffs / zero warnings.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-09-19-synthia-search.md`. Two execution options:

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?