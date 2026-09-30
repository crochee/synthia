# R47 Results — 2026-09-12

Predecessor: [`optimization-report-R46-2026-09-12.md`](optimization-report-R46-2026-09-12.md)
(`532e72b3`).

The project's stated purpose is to be "the best assembly paradigm and
tutorial" for building an agent out of parts. R37–R46 added, moved, and
removed parts; what was missing was one page that says *where every part
plugs in*.

## What the audit found

| Finding | Evidence |
|---|---|
| **There was no seam index.** 41 public traits exist across the workspace; a consumer had to read 19 crate docs and 30 examples to learn which ones they are expected to implement and which are internal | `pub trait` across `crates/*/src`; `docs/examples/README.md` is organised by example, not by seam |
| **Two recipes in `MINIMAL.md` did not compile.** The Step-3 table told consumers to call `AgentBuilder::skill_registry(reg)` (no such method — skills arrive via `PromptContext::with_skill` plus the registered `skill` tool) and `.run_inbox(Arc::new(RunInbox::new(..)))` (`RunInbox` is the trait; `MpscInbox::channel()` is the constructor) | grep for `skill_registry` / `RunInbox::new` in the tree: only the doc uses them |

## What landed

### A. `SEAMS.md` — the lego map

One page, organised by *what you would replace*, with a `Trait | ships
in-tree | install yours` row for each:

- **§1 the seven pieces of the loop** — `ModelProvider`, `Tool`,
  `ContextManager`, `Steering` (and its five sub-seams `Guard` /
  `AgentHook` / `Hint` / `Tracker` / `OutputTransformer`), `SessionSink`,
  `CancelToken`, `Agent`.
- **§2 runtime and clock** — `Spawner`, `Clock`, `IdGen`, with the
  pointer to the documented runtime contract and the
  `runtime_agnostic` example that proves it.
- **§3 extensions** — memory, retrieval, embeddings, chunking, skills,
  delegation/peers, team protocols, run inbox, MCP transports, sandbox
  backend, workflow host, branch scoring, eval metrics/judge, job
  store, attachment backend, full-output store, token counter,
  `SensitiveData`.
- **§4 application-crate seams** — `RunStreamFactory`, `SessionLane`,
  `SessionController`, plus an explicit note that `CommandRunner` and
  the JSONL reader are **crate-internal by design**, so a reader can
  tell "swap this" from "ask for a feature".
- **Verifying a substitution** — the four gates and the example ladder.

Every row was written against the code, not from memory: the trait
names come from a `pub trait` inventory, and each installer
(`Steering::add_guard`, `RagContextManager::new`,
`MpscInbox::channel()`, `HeuristicScorer::new`, `WorkflowRuntime::new`,
`LlmJudgeMetric::new`, `full_output_tool`, …) was read in place before
being named. Writing it is what caught §B.

### B. Two `MINIMAL.md` recipes corrected

| Was | Is |
|---|---|
| `Skills: SkillRegistry::new()` + `.skill_registry(reg)` | `SkillRegistry::new()` + `reg.register(FileSkillProvider::discover(dir))`; surface with `PromptContext::with_skill(name, description)` and `register_skill_tool(&registry)` |
| `.run_inbox(Arc::new(RunInbox::new(...)))` | `let (inbox, handle) = MpscInbox::channel();` + `.run_inbox(Arc::new(inbox))` |

Both were reachable only through the doc, so nothing else failed — which
is exactly why a "recipes" document needs to be written against the
code.

### C. Linked from the two entry points

`README.md`'s "New here?" block gained a "Replacing a piece?" line, and
`MINIMAL.md`'s "Where to read next" now lists `SEAMS.md` first.

## Verification

| Check | Result |
|---|---|
| Every trait / installer named in `SEAMS.md` | read in place before writing (inventory of `pub trait` + grep of each constructor); the two internal seams are labelled as such |
| `make ci` | green (fmt, clippy, MVP deps, runtime-free, clock ratchet) |
| `make examples` / consumer proofs | green |
| `MINIMAL.md` grep for the two removed recipes | no hits |

## Deferred to R48 (recorded, not dropped)

- **A count accumulator to drop the per-message `String`** in
  `estimate_messages_token_count` (carried from R42).
- **A runtime seam for the builtin tools** (carried from R40) — the map
  now states this as a known limit rather than leaving it implicit.
- **A `Timer` seam** (carried from R39).
- **`cargo-deny` in CI** (carried from R41) — `deny.toml` is empty and
  the binary is unavailable locally.
- **`SEAMS.md` is prose, not a compile check.** A doctest-style gate
  (each row's installer as an ignored-but-compiled snippet) would catch
  the next stale recipe automatically; that means restructuring
  `SEAMS.md` into a doc-included file, which is its own change.
