<!--
Raw capture of brainstorming output.

本檔原樣捕捉 brainstorming 的產出，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。

不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，
兩者互補但不重疊。
-->

# Brainstorm: Build Complete E2E, UT, and Benchmarks

## Background

Synthia has a growing test surface (22 test files in synthia-agent alone) but lacks:
- Unified benchmark infrastructure for performance regression detection
- Systematic test organization with clear naming conventions
- Comprehensive e2e coverage for all major interaction patterns
- Coverage for modules like synthia-tool, synthia-memory, synthia-context

The goal is to add test and benchmark infrastructure without touching production code behavior.

## Design Exploration

### Q1: Which benchmark framework?

**Option A: Standard `test` benchmark harness**
- Simple, built-in, no dependencies
- No statistical analysis, no charts

**Option B: `criterion`**
- Rust standard for rigorous benchmarks
- Statistical output, regression detection, chart generation
- More setup but much more valuable

**Decision**: Use `criterion` for Phase 1. Standard `test` harness is insufficient for meaningful performance tracking.

---

### Q2: Where to put benchmarks?

**Option A: Workspace-level `benches/`**
- Unified location, easy to run all benchmarks together
- Less clear ownership per crate

**Option B: Crate-level `benches/` subdirectory**
- Co-located with measured code
- Clear ownership

**Decision**: Crate-level `benches/` under `synthia-agent`. Other crates can add benchmarks later as needed. Workspace-level `benches/` deferred.

---

### Q3: How to categorize tests?

**Option A: By file location (`src/tests/`, `tests/`)**
- Already partially done but inconsistent

**Option B: By naming convention prefix (`e2e_`, `unit_`, `integration_`)**
- Self-documenting, works across directories
- Easier to filter with glob patterns

**Decision**: Naming convention prefix. `e2e_<scenario>_test.rs` for e2e, `<module>_test.rs` for unit, `<feature>_integration_test.rs` for integration. This extends existing patterns (e.g., `e2e_llm_test.rs` already exists).

---

### Q4: How many e2e scenarios?

Synthia has these major interaction patterns that need e2e coverage:
1. Single-turn interaction
2. Multi-turn conversation (already partially covered in e2e_llm_test? need to verify)
3. Session pause/resume
4. Tool call sequence
5. Guardian permission gate
6. Session teardown

**Decision**: Add 5 new e2e tests covering the above scenarios. Existing `e2e_llm_test.rs` covers single-turn; verify multi-turn coverage and extend if needed.

---

### Q5: Should benchmarks block CI?

**Option A: Hard gate (fail PR on regression)**
- Strongest protection against performance regressions
- Fragile due to CI noise

**Option B: Informational only (no blocking)**
- CI noise makes gating unreliable
- Developers run benchmarks locally

**Decision**: Phase 1 = informational only. Document how to run benchmarks locally. After baseline stabilizes (3-5 runs), can revisit gating.

---

## Key Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Benchmark framework | `criterion` | Statistical rigor, regression detection, industry standard |
| Benchmark location | `synthia-agent/benches/` | Co-located, clear ownership |
| Test naming | Prefix convention (`e2e_`, `<module>_`) | Self-documenting, easy filtering |
| E2E scenario count | 5 new tests | Cover all major interaction patterns |
| CI benchmark gate | Informational only | CI noise too high for Phase 1 |

## Open Questions (deferred)

- `proptest` for property-based tests? (low priority, defer)
- `cargo-fuzz` for adversarial input? (defer to future phase)
- Workspace-level benchmark runner? (defer; crate-level sufficient for now)
