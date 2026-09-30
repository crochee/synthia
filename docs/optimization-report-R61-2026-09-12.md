# R61 Results — 2026-09-12

Predecessor: [`optimization-report-R60-2026-09-12.md`](optimization-report-R60-2026-09-12.md)
(`b83171f9`).

`MINIMAL.md` is the document a new consumer reads first, and the one the
whole repo's pitch rests on ("the guide above is a program"). It had never
been compiled. Pointing a doctest gate at it took four minutes and found
four defects, one of them structural.

## What the audit found

| Finding | Evidence |
|---|---|
| **Step 1's code fence was never closed.** It opens at line 57; the next ```` ``` ```` is at line 135 | the rendered page shows one code block containing the "Step 2 — Assemble the agent" heading, its prose, and the *second* fence marker — the guide's two code steps were unusable as written |
| **`Content::text(self.answer.into())` does not compile** (E0283: `&str: Into<_>` is ambiguous) | first doctest run |
| **Step 4 pointed at `synthia::session::JsonlSessionSink`** | no such path; it is `synthia::session::jsonl::JsonlSessionSink` |
| **`synthia::agent::SubagentPool` did not exist** (R31's subagent lane was only reachable as `synthia::agent::agent::pool::SubagentPool`) | second doctest run |
| Step 1's prose promised "two surfaces to choose between" while the fence showed one | reading the repaired `MINIMAL.md` |

The malformed fence is the one that matters: a GitHub-rendered README is
how most people meet a project, and the start-here document was showing
its second half as code.

## What landed

### A. The tutorial is compiled

```rust
/// The tutorial, **compiled**.
#[cfg(doctest)]
#[doc = include_str!("../../../MINIMAL.md")]
pub struct MinimalGuide;
```

The `rust` fences in `MINIMAL.md` are now doctests. CI already runs
`cargo test -p synthia --doc` **twice** — default features, then the
seven-feature MVP subset — so the guide's central claim ("these seven
features are enough to build an agent") is compiled rather than asserted.

- **15 doctests pass with default features, 18 under the MVP subset.**
- Step 2's fence became `async fn assemble(provider: Arc<dyn ModelProvider>)`
  rather than a `main`: it reads as the assembly and nothing else, it
  compiles without a scripted provider inlined, and Step 1's provider is
  what you hand it.

### B. The four defects, fixed

- The fence is closed; the Step-2 heading and prose sit outside it again.
- `Content::text(self.answer)` (the `.into()` was never needed — the
  parameter is `impl Into<String>`).
- `synthia::session::jsonl::JsonlSessionSink`.
- Step 1's prose now says what the trait actually requires: "only
  `complete` is required; `complete_with_stream` has a default that calls
  it and emits the result as a single delta, so overriding it is an
  optimisation, not an obligation."

### C. One API wart the gate surfaced

`SubagentPool` — R31's two-lane admission state machine, a headline in the
README's feature table — was nameable only through the double-nested
`synthia::agent::agent::pool::SubagentPool`. It (and its satellites:
`Slot`, `Admission`, `AdmissionTicket`, `QueuedToken`, `InvocationRecord`,
`InvocationStatus`, `PoolCounts`, `SharedSubagentPool`) are now re-exported
at the agent crate root, so `synthia::agent::SubagentPool` is the name. The
nested path keeps working.

### D. Two report corrections

While computing this round's sweep total programmatically, the totals in
R58 and R59 were re-derived from their own tables and found **10 high**
(hand-summed). Both rows now carry the correct number and a note saying
where the correction came from. The per-crate numbers in those tables were
copied from the sweep output and are unaffected.

## Verification

| Check | Result |
|---|---|
| `cargo test -p synthia --doc` | 15 passed / 0 failed |
| `cargo test -p synthia --doc --no-default-features --features core,provider,context,tool,session,steering,agent` | 18 passed / 0 failed — the guide's promise, compiled |
| `cargo test -p synthia` | **20 passed / 0 failed** (17 before: +3 doctests from the guide) |
| Full per-crate sweep (19 crates) | **2617 passed / 0 failed** (computed from this round's output: core 108, telemetry 36, provider 744, context 95 (103 with `sqlite`), tool 312, session 137, steering 74, skill 56, agent 330, attachment 16, mcp 52, rag 38, scheduler 14, macros 31, eval 36, workflow 65, test-support 19, facade 20, server 434) |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace` | 0 warnings (R56's gate still holds) |
| `cargo +nightly fmt --all`, `cargo clippy --all-targets --all-features --tests --all -D warnings` | clean |
| `make ci`, `make examples` | green |

## Known gaps after this round

`docs/README.md` records the remaining ones. The `SEAMS.md` doctest row is
now half-done and says so: `MINIMAL.md` is gated, `SEAMS.md` is not (its
recipes are inline code spans inside a table, so nothing can extract them
without a heuristic or rustdoc JSON).

Still open, each with its reason: the builtin tools' runtime seam
(declined by design, R40), the `Timer` seam (R39), `GroupedRegistry`'s
residual name clones (R51), `cargo-deny` in CI (needs the binary and an
advisory fetch), the projection's remaining per-request schema clone
(re-scoped by R59), an in-tree LLM-judge scorer and a per-run strategy
override over HTTP (features, R50), and the two tracked lockfiles in
`synthia-web/` (a maintainer workflow decision, R57).
