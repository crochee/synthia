# R116 — Remove `synthia-rag` — Design

**Date:** 2026-09-19
**Status:** Draft (pending implementation)
**Scope:** Delete the `synthia-rag` crate and every consumer in the workspace. The grounding story it owned is being retired in favour of a future redesign; `synthia-search` (R115) already covers the cross-`T` catalog-selection need that motivated this round.

## §1. Motivation

`Synthia-rag` was introduced in R20 to provide BM25 / dense retrieval + a `ContextManager` adapter that injects grounded chunks into the prompt as a system message. Adoption signals over the past few rounds are weak:

- Only one first-party consumer wires it end-to-end: `synthia-server`'s boot pipeline (`build_retriever` from configured paths → `KeywordRetriever` → `RagContextManager` → context manager stack). Config defaults to `enabled: false`.
- The `Retriever` trait in `synthia-rag` is not consumed by any other crate.
- The `ModelProviderEmbedder` shipped *inside* `synthia-rag::embed` exists only to bridge `ModelProvider::embed` into `synthia_rag::Embedder`. After R116 there is still one consumer (`synthia-search::provider::ModelProviderEmbedder`) but it lives in `synthia-search`, not in rag.

Meanwhile `synthia-search` (R115) landed a general catalog-selection engine that already addresses the "agent picks which Skill / Tool / Memory to load this turn" problem — a separate concern from grounding. Rag's grounding layer is the only piece not yet superseded, and the user has elected to retire the crate outright rather than maintain it alongside.

This is the AGENTS §3.7 "optional crate deletion" flow:

> Before deleting a member, name it, then prove there is no dependency on it. If there is no first-party crate that needs it, mark it deprecated for a release, then remove it.

The first-party-crate check passes — only `synthia-server` references rag, and `synthia-server` is being rewritten in the same change to drop the retrieval assembly path.

## §2. Goals & non-goals

**Goals**

- Delete `crates/synthia-rag/` entirely (Cargo.toml + src/ + examples/).
- Drop the workspace `members` entry + `workspace.dependencies` entry.
- Drop `synthia-rag` from `crates/synthia/Cargo.toml` (`[dependencies]` + `rag` feature) and `crates/synthia/src/lib.rs` (`pub mod rag;`, compile_fail doctest block, feature-table row).
- Drop the `synthia-server::config::retrieval` config block, the `build_retriever` / `index_path` helpers, the `ground_with_retrieval` factory, the `RunDependencies::retrieval` field and `with_retrieval` builder, the `AppState.retrieval` field, and the server tests that exercise the retrieval path.
- Drop every doc-comment reference to `synthia-rag` / `RagContextManager` / `KeywordRetriever` / `EmbeddingRetriever` / `HybridRetriever` / `Retriever` / `Document` in other crates (`synthia-search`, `synthia-session`, `synthia-provider`, `AGENTS.md`, `MINIMAL.md`, `SEAMS.md`, `README.md`, `docs/examples/`, etc.).
- Keep `ModelProvider::embed` (still used by `synthia-search::ModelProviderEmbedder`); only its doc-comment loses the "synthia-rag is the only consumer" wording.
- All `make ci` gates still green (12 gates).
- All 24 remaining crates' `make test-crates` green (was 25 with rag).

**Non-goals**

- No CHANGELOG entry (user policy for this round: "本身就不需要").
- No deprecation window — the crate is gone in this single round.
- No migration helper for any deployment that enabled `retrieval.enabled = true`. The config field is deleted.
- No worktree isolation (user policy: "不要"). Changes land directly on master; failures roll back via `git revert`.
- No redesign of the grounding story (deferred to a future round).

## §3. File inventory

### 3.1 Deletions

- `crates/synthia-rag/Cargo.toml`
- `crates/synthia-rag/src/lib.rs`
- `crates/synthia-rag/src/retriever.rs`
- `crates/synthia-rag/src/keyword.rs`
- `crates/synthia-rag/src/embed.rs`
- `crates/synthia-rag/src/hybrid.rs`
- `crates/synthia-rag/src/chunk.rs`
- `crates/synthia-rag/src/context_manager.rs`
- `crates/synthia-rag/examples/grounded_agent.rs`
- `crates/synthia/src/rag.rs`

### 3.2 Edits

| File | Edit |
|---|---|
| `Cargo.toml` | Remove `synthia-rag` from `members` and from `workspace.dependencies`. |
| `crates/synthia/Cargo.toml` | Remove optional dep `synthia-rag`; remove `rag = [...]` feature entry. |
| `crates/synthia/src/lib.rs` | Remove `#[cfg(feature = "rag")] pub mod rag;`; remove the `not(feature = "rag")` compile_fail doctest block; remove the `\| rag \| `synthia::rag` \| ... \| no \|` row from the crate-doc feature table. |
| `crates/synthia-provider/src/traits.rs` | Doc-comment for `ModelProvider::embed` (around lines 121–129): drop the "synthia-rag is the only consumer of `embed`, and it goes through its own Embedder trait" wording. The method itself + default impl stay (consumed by `synthia-search::ModelProviderEmbedder`). |
| `crates/synthia-server/Cargo.toml` | Remove `synthia-rag.workspace = true` from `[dependencies]`. |
| `crates/synthia-server/src/config/server.rs` | Delete the `retrieval` block inside `ServerConfig` (struct fields, default impl, `#[serde(...)]` annotations). Drop any sub-config struct that becomes empty. |
| `crates/synthia-server/src/session/controller/deps.rs` | Remove `retrieval` field on `RunDependencies`; remove the `with_retrieval` and `retrieval` builder methods; remove the `ground_with_retrieval` helper. Replace any consumer site (`build_run_dependencies` etc.) with the no-retrieval branch. |
| `crates/synthia-server/src/state/boot.rs` | Remove `build_retriever`, `index_path`, and the `use synthia_rag::Chunker as _;` import. Drop the `config.retrieval` branch in `boot_app_state` (returns `None` always now). |
| `crates/synthia-server/src/state/app_state/mod.rs` | Remove `retrieval` field; update any doc-comment that referenced it. |
| `crates/synthia-server/src/tests/context_manager.rs` | Delete any test whose name or body references `RagContextManager`, `KeywordRetriever`, or the grounded-retrieval flow. If a test still has value without grounding (e.g. exercising the truncating inner manager), keep it after removing the rag setup. |
| `crates/synthia-server/src/tests/` (other) | Audit and remove any test code that constructs `synthia_rag::Document` / `synthia_rag::KeywordRetriever` etc. |
| `crates/synthia-search/src/tokenizer.rs` | Lines 7–10 currently read: *"This is the deliberate divergence from `synthia_rag::keyword::tokenize`, which only emits per-character CJK — the bigram form raises recall on Chinese queries without pulling a Chinese-segmentation dependency."* Replace with a description that no longer references rag (e.g. *"Lowercases Latin words; emits one token per CJK ideograph / kana / Hangul syllable; emits a CJK bigram for every pair of consecutive CJK characters. The bigram form raises recall on Chinese queries without pulling a Chinese-segmentation dependency."*). |
| `crates/synthia-session/src/search/mod.rs` | Lines 38–40 currently read: *"the behaviour a user expects from a history search. A deployment that wants BM25 instead has `synthia-rag`'s `KeywordRetriever`, or can implement [`SessionSearch`] and keep its own index."* Replace with: *"the behaviour a user expects from a history search. A deployment that wants BM25-style retrieval over session history can implement [`SessionSearch`] and keep its own index."* |
| `AGENTS.md` | §1 line 8: remove "rag" from the `members` enumeration. §1 rag paragraph: drop (the section "## 新增 crate `synthia-rag`（R20）" no longer applies). §1 "feature 集合 / 离线 tool crate" enumeration: remove `rag`. §1 MVP_FEATURES: keep as-is (the MVP list itself doesn't reference rag, but verify). §1 check-mvp-deps: keep as-is (no rag in MVP_FORBIDDEN / OFFLINE_TOOL_CRATES). §1 check-claim-language CLAIM_FORBIDDEN: drop the rag mentions in the rationale paragraph. |
| `MINIMAL.md` | Any `synthia-rag` mention (rust fences or prose). |
| `SEAMS.md` | Any `synthia-rag` mention. |
| `README.md` | Any `synthia-rag` mention. |
| `docs/examples/README.md` | Any `synthia-rag` mention. |
| `docs/examples/external-consumer/` | Audit `Cargo.toml` + `src/main.rs` for rag refs. |
| `Makefile` | Audit for `rag` and `synthia-rag` references; remove if any. |
| `clippy.toml` | Audit for rag-specific thresholds; remove if any. |
| `scripts/check-harness-shape.sh` | Audit for rag refs (none expected). |
| `synthia-rag/README.md` (which is the README inside the deleted crate directory) | Deleted with the directory. |

### 3.3 Audit sweep

A `git grep -nE 'synthia_rag|synthia-rag|RagContextManager|KeywordRetriever|EmbeddingRetriever|HybridRetriever|Document::new|citation_label|Chunker as _|GROUNDING_PREFIX|render_grounding|ContextWindowStrategy|CitationStrategy::Weighted|rank_and_truncate|FixedSizeChunker|SentenceChunker|RecursiveChunker' .` (run before commit) enumerates any miss.

## §4. Verification plan

After Phase 2c lands:

1. `cargo fmt --all`.
2. `make ci` — 12/12 gates green.
3. `make test-crates` — every remaining crate passes (24 crates; the count drops by 1 vs pre-R116).
4. `cargo tree -p synthia-rag 2>&1 | head -3` — must say `error: package not found: synthia-rag` (proof of deletion).
5. Audit `git grep -nE 'synthia_rag|synthia-rag|RagContextManager|...' .` — must return zero hits.

## §5. Risks & mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| A server test I missed still imports `synthia_rag` → breaks `cargo test -p synthia-server`. | Med | The audit sweep (§3.3) catches them before commit; if any escape, Phase 2b's reviewer flags them and we apply a fix-round before Phase 3. |
| `synthia-server::config::ServerConfig.retrieval` removal breaks an external deployment with `retrieval.enabled = true`. | Med (real config in the wild) | User accepted the deletion without migration; document via the AGENTS.md edit (no replacement config — feature gone). |
| `ModelProvider::embed` default impl is no-op; without rag, anthropic / openai providers still implement it correctly. | Low | The trait method has a default impl (`Ok(Vec::new())`); anthropic + openai override. We don't touch the trait method — only its wording. |
| `make check-test-layout` / `check-harness-shape` baselines shift after deletion. | Low | R116 only removes code, doesn't add; both are "no function > 100 lines" gates. Should remain green. |
| `make check-clock` baseline (6 `chrono::Utc::now()` calls in lib code) shifts. | Low | Rag doesn't call `Utc::now()` (it uses `now.timestamp()`); count should remain 6. |

## §6. Out of scope

- No CHANGELOG entry.
- No deprecation window.
- No redesign of the grounding story.
- No migration helper.
- No worktree isolation (changes land on master directly).