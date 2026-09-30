# Changelog

All notable changes to Synthia are documented in this file. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Releases before **0.1.0 (2026-09-30)** are snapshot milestones from an
earlier internal tree and are folded under **Prior releases** at the
bottom of this file. The per-round development log (R0 – R125, May – Sep
2026) lives under **Historical round log** below the unreleased section
— frozen per the precedent set in `optimization-report-R46`.

## [0.1.0] - 2026-09-30

The first open-source cut of Synthia. A modular, runtime-neutral AI
agent framework in Rust: a ReAct loop driven by a pluggable provider
surface, a one-crate facade (`synthia`) that re-exports every component
behind feature flags, eight plugin-brick tools (`read` / `write` /
`shell` / `todo` / `web_fetch` / `schedule` / `search` / `task`), an
HTTP / SSE / WebSocket server (`synthia-server`), a React/Vite web UI
(`synthia-web`), and a stdio MCP binary (`synthia-mcp-server`) that
exposes the same tools to any external MCP client. Production-grade
release pipeline with CycloneDX SBOM, SLSA Build L3 provenance, and
cosign keyless signing.

### Highlights

- **One facade, 28 crates.** `cargo add synthia` is the consumer entry
  point; `synthia` re-exports the SDK pieces behind feature flags and
  ships a curated `prelude`. Every crate except `synthia-server` and
  `synthia-mcp-server` (the two application / binary crates) is
  runtime-neutral: public APIs name `futures::Stream` /
  `synthia_core::CancelToken` / `synthia_core::spawn::Spawner`, not
  `tokio::*`, so a consumer can run the loop on
  `std::thread + futures::executor::block_on`.
- **ReAct loop with full event streaming.** The harness is a
  `ReActAgent` that emits typed events over `futures::channel::mpsc`,
  honours a cancellation token, and exposes hooks (pre-`/post-step`,
  tool call interception, output transformers) at every boundary.
- **Pluggable LLM providers.** `OpenAI` + `Anthropic` adapters ship in
  tree behind feature flags; `ProviderProfile` /
  `ProviderRegistry` build any provider from a typed config.
- **Eight plugin-brick tools.** `read` / `write` / `shell` (with OS
  sandbox policy) / `todo` / `web_fetch` / `schedule` / `search` /
  `task` — each its own crate, each gated on a facade feature, each
  registering into `ToolRegistry` with workspace path confinement.
- **Subagent delegation as first-class.** The `task` tool creates child
  sessions, persists them through the same `SessionSink`, attributes
  them to the parent's transcript via `parent_session_id`, and
  surfaces them in `/api/v1/sessions` next to user conversations.
- **Session lifecycle.** Create / list / read / branch / export /
  delete sessions. Durable JSONL store with schema-version header;
  idempotency keys for retries; recoverable tail.
- **Multimodal attachments.** Image / file / PDF; `ResourceLink`
  bounded inlining (text up to 16 KiB, otherwise a "contents not
  inlined" note); an Anthropic `document` block on the `/v1/messages`
  protocol endpoint.
- **MCP dual surface.** `synthia-mcp` drives external MCP servers and
  publishes their tools locally; `synthia-mcp-server` is the
  outbound stdio binary any MCP client (Claude Code, `pi`, the
  Anthropic SDK, …) spawns to get the eight plugin tools.
- **Multi-agent workflows.** Declarative `WorkflowSpec` (agent /
  fan-out / pipeline) plus a `WorkflowHost` effect seam; JSONL journal
  with prefix replay for resume.
- **Cron + interval + once scheduling.** Hierarchical `TimingWheel`
  driven by the host's `tick(now)`; opt-in `cron` feature on the
  facade so a consumer can also create cron jobs.
- **Cross-domain search.** `synthia-search` Registry over BM25 +
  embedder + reranker; `synthia-tool-search` exposes it as the
  model-facing `search` tool (cold-start `Deferred` schema, promotes
  on first call).
- **HTTP server with full protocol coverage.** `/api/v1/chat/*` REST
  + SSE; `/v1/messages` Anthropic-protocol endpoint (Claude Code
  plug-and-play); `/livez` + `/readyz` + `/metrics` Prometheus
  endpoint; OpenTelemetry OTLP tracing + Prometheus RED metrics;
  WebSocket approvals.
- **Production-grade CI / CD.** `make ci` runs `fmt-check`,
  `lint-rust`, `doc-check`, `test-guides`, `check-mvp-deps`,
  `check-no-runtime`, `check-public-api-runtime`, `check-pub-surface`,
  `check-claim-language`, `check-clock`, `check-harness-shape`,
  `check-test-layout`, `check-deny`. 11 GitHub Actions workflows cover
  CI, rust-quality, release, docs, security-audit, contract-closure,
  dependabot-auto-merge, markdown-lint, shellcheck, stale, labeler.
  Tag-driven `release.yml` emits Linux + Windows + macOS binaries,
  CycloneDX SBOM, SLSA Build L3 provenance, cosign keyless signatures.
- **Container images with baked identity.** Three images
  (`synthia-server`, `synthia-web`, `synthia-mcp-server`) ship the
  exact commit SHA + build time + version through both the binary
  (`--version`) and OCI image labels
  (`org.opencontainers.image.revision` /
  `org.opencontainers.image.version` /
  `org.opencontainers.image.created`). nginx config in `synthia-web`
  is templated: `SYNTHIA_BACKEND_HOST` / `SYNTHIA_BACKEND_PORT` set
  the upstream at runtime.
- **Kubernetes-ready.** `k8s/server.yaml` + `k8s/web.yaml` —
  Deployments + ClusterIP Services + PodDisruptionBudgets +
  securityContext (`runAsNonRoot: true`, `runAsUser: 65532`,
  `readOnlyRootFilesystem: true`, `seccompProfile: RuntimeDefault`),
  `emptyDir` mounts for `/data` and `/tmp`, liveness/readiness probes
  against `/livez` and `/readyz`.

### Breaking changes since the prior `[0.1.0]` snapshot (2026-05-09)

The pre-existing internal `[0.1.0]` and `[0.2.0]` entries below
describe a different internal tree; they are kept under **Prior
releases** for historical reference only. The 0.1.0 cut listed here
is the canonical open-source release, and downstream consumers should
pin to it.

### Known limitations

- **sandbox policy wiring**: the `sandbox` config key on
  `[agents.<name>]` is parsed but not yet bound to a per-run shell
  policy; the boot-time default policy is in effect. The follow-up
  wires the parsed policy into `ShellTool::with_policy` at run
  factory. (`docs/research/synthia-r124-holistic-design.md` §2.)
- **Phase 3 trait abstractions**: `LoopDetector`, `PermissionPolicy`
  sub-traits, `OsSandbox`, and `Message::cache_control` field
  abstractions deferred 6 months (re-evaluate 2026-12-10) per the
  R29 / R30 6-expert adversarial review.
- **Cross-platform binaries**: tag-driven release builds Linux x86_64,
  Windows x86_64 (MinGW), and macOS arm64. Intel macOS is not built
  (Rosetta not supported). No musl-only ships as the canonical Linux
  target today; the runtime image is Alpine glibc-based.

### Install / verify

```sh
# Latest container image (Linux x86_64):
docker pull ghcr.io/crochee/synthia-server:0.1.0

# Verify the binary's baked identity:
synthia-server --version
# → synthia-server 0.1.0 (<7-char sha> <build-time-utc>) linux release

# Verify release artefacts:
sha256sum -c sha256sums.txt       # SHA-256 per binary
cosign verify-blob ...             # keyless signature per asset
gh attestation verify ...          # SLSA Build L3 provenance
```

### Acknowledgements

Synthia's plugin split, MCP surface, multi-agent traces, and
search domain are adopted from a community reading of `pi-mono`,
`traitclaw`, `dsh`, Claude Code, Codex, Cline, Continue, Aider,
Goose, OpenCode, and the broader agent ecosystem. The R124 holistic
design pass tracks 95% community convergence and the rationale for
each divergence. See `docs/research/synthia-r124-holistic-design.md`
for the audit trail and `docs/optimization-report-*.md` for the
per-round record.

## [Historical round log — R0–R125, May–Sep 2026]

The per-round development log before the open-source `0.1.0` cut.
Frozen per the precedent set in `optimization-report-R46`: every
change below shipped across the R0–R125 development cycle that
preceded the first public release. Per-round *intent* and the
*verification* they carried is preserved verbatim; the *user-visible
capability set* of the released `0.1.0` is summarised above and
in `docs/INDEX.md` / `docs/ARCHITECTURE.md` / `README.md`. The
following section header reads `[Unreleased]` because that was
its tag at the time; the entries themselves are immutable.

### Fixed

- **The chat page's resume stream survives hydration, yields to a turn,
  then re-arms.** `normalizeSessionState` only recognised the
  protobuf-JSON enum names (`SESSION_STATE_WORKING`) while the
  session-detail endpoint reports the canonical lowercase form
  (`working`), so **every** hydration normalised to `unknown` — the one
  value the resume effect treats as "nothing to tail". Two failures
  followed: the SSE was torn down milliseconds after mounting and never
  re-attached, and `TURN_TERMINAL` never matched, so a completed turn's
  typing indicator could only clear on the next live frame. The helper
  now folds both wire shapes onto one suffix
  (`synthia-web/src/pages/ChatPage.tsx`).
- **A locally-owned turn stream takes the resume stream's place.**
  `handleSubmit` / `handleRetry` / `handleRegenerate` now set
  `turnStreamRef` and abort the resume controller; the resume effect's
  `finally` skips reconciliation for that hand-off instead of
  downgrading a working bubble to `failed`, and the turn's own `finally`
  clears the flag and bumps `resumeNonce` so the resume stream
  re-attaches from the persisted `streamIndex`. Without the hand-off
  both consumers read the same broadcast, and the resume path's
  per-cursor re-hydrate (`GET /api/v1/sessions/{id}` →
  `reconstructMessagesFromSession`) replaced the transcript with
  freshly-minted message ids while `applyStreamEvent` appended to the
  old ones — every later frame a silent no-op, so a turn longer than one
  cursor batch (`RESUME_CURSOR_BATCH = 50` frames) froze mid-stream.
  `handleRegenerate` needed it most: it hides the trailing answer while
  the rerun polls, and an attached resume stream put that stale answer
  straight back.
- **The resume stream's 1.5 s re-hydrate poll runs only while a turn is
  in flight.** It was armed for the whole life of the stream, so an
  idle chat page re-hydrated its entire transcript — `setMessages` with
  freshly minted message ids, remounting every message — every 1.5 s
  forever, and on a long session that is a multi-second JSON parse each
  time (measured: 44 consecutive `GET /api/v1/sessions/{id}` at ~1.5 s
  intervals on a `completed` session). An idle stream still needs to
  stay attached to pick up the next turn; it does not need to poll for
  information that cannot change.
- **A session the server has not persisted yet no longer disables the
  resume stream.** A client-minted id answers `404` on the hydrate GET;
  that is now read as "no status yet" (status stays `null`) instead of
  `unknown`.

Verified against a running `synthia-server` + `synthia-web` (headless
Chromium, network + server logs):

- Session `50d53d32…`: the SSE attaches at mount, re-attaches after the
  hydrate's status flip with the persisted cursor, is aborted 13 ms
  after `POST /v1/messages` takes over, and re-attaches with `?from=6`
  when the turn ends.
- Session `0064bf3d…`: clicking Regenerate aborts the resume stream 8 ms
  after `POST …/regenerate` and re-attaches with `?from=3` once the
  rerun's poll finishes.
- Idle session (status `completed`): one cursor-driven `GET
  /api/v1/sessions/{id}` ~1.7 s after attach, then no further request for
  the rest of the window — before the poll gate that endpoint was hit
  every ~1.5 s indefinitely (44 consecutive requests measured on the
  same session).

### Added

- `synthia-server`: end-to-end `crates/synthia-server/examples/observability.rs`
  — an executable proof that boots `synthia::telemetry::init_tracing`
  against either the OTLP pipeline (when `SYNTHIA_OTLP_ENDPOINT` is
  set) or the console fallback (when it isn't), round-trips a W3C
  `traceparent` through extract + inject, and confirms the
  Prometheus text exposition encoder returns within microseconds. Prints
  the proof line `OBSERVABILITY-EXAMPLE: OK` so `make examples`
  picks it up automatically — same `Makefile` walker every other
  example in the workspace already conforms to. No new dep; reuses
  the existing `opentelemetry` dev-dep in `crates/synthia-server`.
  Spec: `docs/research/synthia-r124-holistic-design.md` §3 (Adopt C,
  observability-example half).

- `synthia-server`: `state::unapplied_agent_keys` now classifies
  `sandbox` (R62) as inert so the boot log names the gap. R62
  adopted the parser (`AgentConfig::sandbox_policy_name`) but never
  wired a consumer — every run still uses the boot-time
  `ShellTool::new()` (`state/app_state/tools.rs:13-32`), so an
  operator who wrote `sandbox = "read-only"` has restricted nothing
  and now sees a one-line warning per configured agent. The actual
  consumer (per-run `ShellTool::with_policy` re-bind in the run
  factory) is tracked in `docs/research/synthia-r124-holistic-design.md`
  §2 as a follow-up; this round surfaces the gap rather than fixing
  it because the production-grade fix touches `AgentRunConfig` +
  `RunStreamFactory` + `RunDependencies` and deserves its own
  scoped round.
  Spec: `docs/research/synthia-r124-holistic-design.md` §2 (Adopt B,
  audit-half).

- `synthia-tool`: `ToolAnnotations` re-exported at the crate root (was
  already public as `synthia_tool::registry::ToolAnnotations`, now
  reachable as `synthia_tool::ToolAnnotations`) and the seven plugin
  tools that implement `Tool` (read / write / shell / todo / web_fetch
  / schedule / search) now override `annotations()` to publish the
  MCP-native `readOnlyHint` / `destructiveHint` / `idempotentHint` /
  `openWorldHint` hints. The `shell` tool's hints follow the
  configured `ExecutionPolicy::is_confined()` so a confined policy
  (`WorkspaceReadOnly`, `DangerFullAccess`, …) advertises the same
  boundary the call enforces; `web_fetch`'s `openWorldHint: true`
  surfaces the public-internet dependency that was previously
  invisible at the descriptor layer. All hints ride on the existing
  `From<ToolAnnotations> for synthia_provider::ToolAnnotations`
  projection (R62), so the wire shape stays additive and old
  consumers ignore the new field. `task` is a `ToolInterceptor`, not
  a `Tool`, so it correctly stays out of this round.
  Spec: `docs/research/synthia-r124-holistic-design.md` §1.
- `synthia-tool-search`: the `search` tool's `parameters()` schema
  now declares top-level `"additionalProperties": false` so a model
  that emits a typo (`lmit` instead of `limit`, `qury` instead of
  `query`) gets a model-facing error instead of a silent default;
  the existing `name_and_parameters_shape` test pins the new field
  so the regression is caught in CI. Closes the only schema drift
  among the eight plugin tool schemas; the other seven already
  pinned it through `schemars`'s `#[schemars(extend(...))]` derive.
  Spec: `docs/research/synthia-r124-holistic-design.md` §1.3.

### Design

- R124 — multi-expert holistic design pass informed by pi-mono +
  TraitClaw + the broader community (Claude Code / Codex / Cline /
  Continue / Aider / Goose / OpenCode / dsh). Five parallel expert
  audits covered TraitClaw 8-trait parity, pi-mono adoption delta,
  plugin-tool UX unification, seams + file-size, and additive
  candidates. Synthia is at **95% community convergence**; the design
  selects three concrete adopts for this round and tracks the rest
  with deferral reasoning.
  Spec: `docs/research/synthia-r124-holistic-design.md`.

- `synthia-session`: `SessionSink` gains two **default-implemented**
  methods, so a backend that needs neither stays at the original
  five-method surface. `append_with_key(key, event)` is the idempotency
  seam: `Some(key)` callers that re-submit the same key observe the
  `seq` originally assigned and the bytes are not written twice —
  `JsonlSessionSink` keeps a 1024-entry move-to-front LRU of recent
  keys, `InMemorySessionSink` a `HashSet` + reverse `HashMap`. This
  closes the "client retried / SSE reconnected mid-flush" hole that
  could land one user prompt twice in the durable log. `append_many
  (events)` is the batched-durability seam: the default loops over
  `append` (N fsyncs); `JsonlSessionSink` overrides it with one
  `write_all` + one `fsync`, and rolls `next_seq` back on failure so a
  retry cannot double-write. `fork_session` now uses it — a 27 086-row
  fork that took 63.8 s completes in well under a second.
- `synthia-session`: `JsonlSessionSink` writes a **schema-version
  metadata header** as the first line of a fresh `events.jsonl`
  (`{"_meta":{"kind":"synthia_session_meta","schema_version":1,"id":
  …}}`). Readers detect it off the first non-empty line and treat a
  file without one as `schema_version = 0` (every line an event row) —
  so pre-existing logs keep reading byte-identically, and `count_lines`
  / `read` / `read_from` all skip a header when present. `CURRENT_
  SCHEMA_VERSION` and `SessionMetadataHeader` are public.
- `synthia-server`: `POST /api/v1/sessions/{id}/fork` — **branch a
  session**. The child is a copy of the parent's durable transcript up
  to `from_stream_index` (the whole log when omitted) and is registered
  through the same path as `POST /api/v1/chat/sessions`, so it is an
  ordinary conversation from there on: listed, addressable, streamable,
  deletable and independently appendable, with the parent untouched.
  Rows are copied **through the child's sink**, not by writing the
  child's file behind it: the sink is the trait's only durable write
  path, and writing the file directly would couple the route to one
  backend's layout. The cost was one fsync per row — measured 63.8 s for
  a 27 086-row parent — until `SessionSink::append_many` landed with a
  single-`fsync` batched write path (`JsonlSessionSink`), which the
  route now uses. This is the *branch* half
  of pi's `Navigation` operation — *rewind* (rewriting the parent's log
  in place) is a different, destructive operation and is deliberately
  not part of this route. `SessionsPage` gains a Fork action that opens
  the branch. 5 route tests: the copy is addressable and the parent is
  untouched, the cutoff truncates, a registered-but-empty parent forks
  empty, the child's next append lands at the ordinal after the prefix,
  and an unknown parent is a 404.
- `synthia-server`: the session lifecycle gains **delete** and **export**
  — `DELETE /api/v1/sessions/{id}` (`204`; `404` for an id neither
  source knows; `409` `session_busy` while a turn is running, because
  the run still owns its sink and is mid-append) and
  `GET /api/v1/sessions/{id}/export` (the durable transcript as
  `application/x-ndjson`: byte-faithful rows in append order, torn tail
  dropped, `Content-Disposition: attachment`). Deletion closes the
  controller and drops it from the active map *before* removing the
  directory, so nothing recreates the path by appending to it; the
  session search index needs no surgery because it rebuilds per query
  from the durable tree. `SessionSink` stays inert:
  `SessionRegistry::remove` is the registry's own bookkeeping and the
  *policy* — when a session may vanish, where its bytes live — is the
  route's. `SessionsPage` gains Export / Delete per card (confirm modal,
  optimistic removal) and `api.getRaw` is the client's entry point for a
  non-JSON body, so a download carries the same `Authorization` header
  as every other call. 6 route tests + the `turn_in_flight` contract
  test.
- `synthia-search`: `SearchEngine::save`/`load` — snapshot + op-log persistence
  (`<root>/snapshot.json` full state, `<root>/ops.jsonl` incremental append,
  serde boundary at the file edge; vectors persist with the snapshot so
  provider-class non-deterministic embedders are not re-embedded on load);
  `VectorStore::vector_at` snapshot accessor; `SearchError::Persist`.
  Spec: `docs/superpowers/specs/2026-09-19-wheel-cron-search-persistence-design.md` §1.
- `synthia-scheduler`: hierarchical `TimingWheel` (host-tick driven; O(1)
  empty tick and next-deadline; overflow sweep) + the `JobTrigger` seam
  (`OnceTrigger` / `IntervalTrigger`) + the opt-in `cron` feature's
  `CronTrigger` (5-field POSIX; the dom/dow conjunction divergence is
  documented); a store write made around the scheduler — the `schedule`
  tool's actions, an operator's own store handle, a `ScheduleStore::load`
  — is visible to the next `tick` / `next_deadline`, because each pass
  compares the store's revision with the one its wheel was built from
  (`ScheduleStore::version`), so `Scheduler::resync` is an explicit
  rebuild rather than something the host owes; `synthia-tool-scheduler`:
  with the `cron` feature forwarded, `create kind=cron` is accepted —
  feature off is byte-identical to before.
  Spec: `docs/superpowers/specs/2026-09-19-wheel-cron-search-persistence-design.md` §2.
- `synthia` (facade): `cron` feature — the whole cron path behind one
  switch (`scheduler` + `synthia-scheduler/cron`, `tool-scheduler` +
  `synthia-tool-scheduler/cron`), so a consumer who enables cron can also
  create cron jobs. A parser-only feature would leave the `schedule` tool
  refusing `kind: "cron"`, i.e. half of the round's user-visible win
  reachable and half not. Stays out of the default feature set.
- `synthia-tool-search` (facade feature `tool-search`): the cross-domain
  `search` tool — one `Tool` over a host-built `synthia-search::Registry`,
  so a query fans out across every registered engine (tool / mcp /
  skill / memory / agent / session catalogs) and returns the
  agent-facing projection (`domain` / `id` / `title` / `why` / `score`,
  plus a `preview` when the domain carries content, best match first);
  an optional `domain` argument narrows the fan-out to one catalog.
  Registered with `ToolExposure::Deferred`: the cold-start tool list
  carries name + description with a placeholder schema, and the real
  `query` / `limit` / `domain` schema is promoted once a `search` call
  is in the transcript. Opt-in rather than default, like `mcp` /
  `tool-scheduler`, because the host constructs the Registry it
  searches; retrieval is read-only and CPU-bound, so the tool keeps
  parallel execution. Offline (no HTTP client) and asserted by
  `check-mvp-deps` via `OFFLINE_TOOL_CRATES`.
  Spec: `docs/superpowers/specs/2026-09-19-wheel-cron-search-persistence-design.md` §3.
- `synthia-search`: the `Registry` is keyed by **domain label** —
  `register` keeps `std::any::type_name::<T>()`, `register_domain` /
  `register_erased` take a host-chosen label (`"tool"`, `"skill"`, …), so
  several engines of one `T` can coexist and every `Hit` is stamped with
  its engine's label (`Hit::domain`); `QueryContext::domains` filters
  which engines a query fans out to. `ErasedEngine::search_erased` and
  `Registry::search` are now async (`async-trait`, runtime-neutral), so
  adapter engines over async retrievers — a session full-text index that
  scans on a blocking pool, a memory tier whose `recall` is async — join
  the same cross-domain merge without a blocking bridge; CPU-only
  `SearchEngine<T>` values complete without yielding. `Hit` /
  `AgentCandidate` gain `domain` and an optional `preview`
  (`Searchable::preview`), and `search_tool_ctx` accepts a caller-built
  `QueryContext` (domain filter, host routing facts).
- `synthia-server`: `crates/synthia-server/src/search/` — the six-domain
  wrap. Tool / MCP / skill / agent catalogs become `SearchDoc` rows in
  self-refreshing `CatalogEngine`s (tool + mcp re-probe
  `ToolRegistry::version`, agents `AgentRegistry::version`, skills on a
  clock bucket since skill files carry no counter); memory and sessions
  keep their own retrieval behind `ErasedEngine` adapters
  (`Memory::recall`, `SessionSearch`). One `SearchService` offers two
  views: the **agent view** (five catalog domains — no session corpus)
  is registered as the model-facing `search` tool via
  `register_search_tool`, so an agent discovers tools / skills / MCP
  tools / memory / peer agents instead of the host enumerating them
  into context; the **HTTP view** (all six) is projected by
  `GET /api/v1/search?q=&limit=&domains=`, which the web UI's
  per-resource search now rides. Tenancy stays server-side:
  the handler passes `RequestUserId` through `QueryContext::extra`, so
  session hits are scoped to the requesting user and the agent view
  never exposes transcripts at all. AppState gains
  `memory: Arc<dyn Memory>` (file-backed tier under the workspace's
  `.synthia/memory*` scopes) and `search: Arc<SearchService>`. With the
  wrap live, MCP tools register as `ToolExposure::Deferred` (name +
  description per request; schema promotes on first call) and the
  `[tools]` `max_visible` cap now counts the `search` tool — both are
  prompt-economy wins, not behaviour changes to dispatch. The
  `/api/v1/memory/search` skills-directory proxy is deleted: the unified
  endpoint replaces it (clean cutover, no shim).
- `synthia-server`: `GET /api/v1/search` — cross-domain search over the
  deployment's tool / mcp / skill / memory / agent / session catalogs.
  The v1 `List<T>` envelope with search-owned ordering (best-first, no
  cursor), `domains` as a comma-separated filter, `limit` clamped to
  50, `q` required; session hits scoped to the requesting user with the
  same auth-layer fallback as `/api/v1/sessions/search`.
- `synthia-web`: the Sessions page's two search cards (the
  skills-directory memory proxy and the session-history search) collapse
  into one **unified search card** over `GET /api/v1/search` — results
  carry their domain badge and preview, session hits link to the
  transcript, and the command palette queries the same endpoint once the
  user types (server-ranked hits bypass the local fuzzy filter, which
  would otherwise drop semantic matches).
- `synthia-server`: the model selector is real — a turn's `model`
  selection now picks the provider that samples it, on every surface
  that carries one. `AppState::provider_catalogue` is built at boot from
  `[providers.<name>]` (each entry built through
  `ProviderProfile::build_provider`, keyed by its config name), and
  `AppState::resolve_provider` accepts both spellings a client sends:
  `"<provider>/<model>"` (what `/api/v1/models` advertises and the chat
  UI posts) and a bare `"<model>"` that exactly one configured provider
  advertises (the Anthropic protocol's `model` field). The selection
  travels with the operation —
  `SessionController::submit_with_provider(op, PinnedProvider::new(p))`,
  consumed by the run that operation starts — so it is per turn rather
  than sticky, and two turns queued behind a running one cannot swap
  each other's provider out. `SessionController::submit` is unchanged
  and means "the session's configured default".
  A selection that resolves to nothing answers `400` (or `503` for a
  provider the deployment configures but could not build) with a message
  naming the value, instead of silently running the default. The durable
  `request_header` row now names the provider that actually ran rather
  than the deployment default a selected turn overrode.
  Spec: the model-selector defect (a parsed-and-ignored `model` field).
- `synthia-server`: the Messages endpoint's `document` block is real, so a
  PDF / text / Markdown attachment sent over `POST /v1/messages` reaches
  the agent again. The block was a field-less stub that converted to a
  `400` naming it, which the web composer's `accept` list never reflected:
  an attachment could be queued, shown, and then silently refused at send
  time. Both of the protocol's spellings convert —
  `{"type":"base64","media_type":…,"data":…}` and `{"type":"url","url":…}` —
  onto the same `ContentPart::Resource` the retired `/api/v1/chat/*`
  `kind: "file"` wire produced (an inline source becomes the historical
  `data:<media_type>;base64,<payload>` URI), so a transcript replayed
  through either endpoint projects identically. An unknown `source.type`
  or a source missing a field it requires is still a `400`, and now names
  the value or field: `WireContent` deserializes the block list directly
  (the untagged wrapper collapsed every inner failure into "data did not
  match any variant", which is what left a malformed document
  unexplained), and `tool_result.content` — the same two spellings — now
  uses that one type rather than a second. Audio remains unrepresentable
  on this wire — the protocol has no audio block — so the composer no
  longer offers it: `audio/*` left the file input's `accept` list, the
  audio attachment kind and its player were removed, and an audio file
  is now refused at selection time with a visible message rather than
  being queued and dropped at send.
- `synthia-server`: **the Anthropic Messages protocol endpoint** —
  `POST /v1/messages` (non-streaming and `stream:true` SSE), so an
  Anthropic-protocol client (the Anthropic SDK, Claude Code, Cursor-style
  tools) can point its base URL at this server and drive the same agent
  the web UI does. The request projects onto the canonical
  `Message` / `ContentPart` model (`system`, multi-turn `messages`, the
  `text` / `image` / `document` / `tool_use` / `tool_result` / `thinking`
  blocks, and `tools` as `ToolDefinition`s); the response is the
  protocol's own shape, and a streamed turn emits the protocol's event
  sequence (`message_start` → `content_block_start` →
  `content_block_delta`* → `content_block_stop` → `message_delta` →
  `message_stop`).
  Auth accepts `x-api-key` alongside `Authorization` (Authorization wins
  when both are present), and a `401` on this path renders the protocol's
  `authentication_error` envelope so an SDK raises a parseable error;
  every other path keeps the application envelope unchanged.
  A malformed body is a `400` in the protocol envelope, naming the field
  or value at fault.
  Two opt-in extensions, both gated on the presence of
  `metadata.synthia_session_id`, keep the web client working on this wire
  while external clients see today's bytes: a session binding (the named
  session is adopted, validated with the same resource-name rule as every
  path parameter and scoped to the requesting user, with history seeding
  skipped once the session holds rows and no close/evict at turn end), and
  an in-thread tool surface (the call and its result are observable in one
  turn, which the protocol's own `tool_use` / `tool_result` pairing cannot
  express for a server that executes its own tools). `metadata.synthia_agent_name`
  selects the agent for the turn.
- `synthia-web`: the chat client now speaks that protocol — turns are
  `POST /v1/messages` (`stream:true`) with the same credential, and the
  Anthropic SSE frames lower into the existing `SessionStreamEvent`
  vocabulary, so the reducer, tool-block pairing, cancel, regenerate,
  feedback and usage surfaces are unchanged. `SessionDetailPage`'s
  transcript projection renders a turn's attachments from the persisted
  refs (below), so a reloaded file or image turn shows a labelled chip
  instead of an empty bubble.
- `synthia-server`: a multimodal turn's durable `UserInput` row now
  carries **byte-free attachment references** (`kind` / `name` /
  `mime_type` / `byte_len`) under `data.attachments`. The bytes stay out
  of the log deliberately — base64 images would bloat a session file —
  which is why the refs exist: without them a reloaded transcript could
  not show that the turn carried anything. A text-only turn omits the
  field, and an attachment-only turn now writes a row at all.
- `synthia-provider`: a `ContentPart::Resource` projects into a prompt as
  a bounded, readable note instead of the resource itself. The OpenAI
  adapter previously inlined the whole `data:` URI — a 1 MB PDF became
  ~1.4 M characters of base64 that blow the context window and teach the
  model nothing — and the Anthropic adapter emitted a constant that lost
  even the filename. Both now share `ResourceLink::prompt_projection`:
  name and media type always, plus the decoded text for a `text/*`
  payload up to 16 KiB, and an explicit "contents not inlined (N bytes)"
  note otherwise.
- `synthia-web`: the UI pass bringing the shell closer to the reference
  desktop client, with the existing colour/typography theme untouched —
  a collapsible left sidebar (`Ctrl/Cmd+B`, persisted, keyboard-accessible,
  and inert while typing), a command palette (`Ctrl/Cmd+K`) that
  fuzzy-filters navigation, sessions and agents with arrow/Enter/Escape
  and a focus trap, the composer's control order realigned (attachment
  before the model selector), a "jump to latest" affordance that appears
  only when the thread is scrolled away from the bottom, and layout /
  control-height tokens replacing one-off pixel values.
- `synthia-server`: the Messages endpoint's unbound sessions
  (`anthropic-<uuid>`, minted for external clients that pin no session)
  are excluded from the management listing, so a community client's
  throwaway turns do not accumulate in the user's own Sessions page. The
  namespace lives in one `pub(crate)` constant; the listing hides the
  rows while `GET /api/v1/sessions/{id}` still serves them, and an id
  that merely shares the prefix letters is not filtered.
- `synthia-server`: `GET /api/v1/sessions` and the session detail read the
  durable transcript store as well as the in-process registry, so a
  conversation survives a backend restart. Previously the listing showed
  only sessions created since the process started (5 of 1265 on this
  workspace) and "View Detail" 404'd for anything older; a session known
  only from disk is still 404-free and cannot escape the sessions root.
- `synthia-server`: usage counters are recorded. `/api/v1/chat/usage`
  read three atomics that nothing ever incremented, so the header chip
  was permanently `0 in · 0 out · 0 turns`; the controller's event funnel
  now folds each `SystemEvent::Usage` into the token totals and each
  `SessionEnded` into the turn count.
- `synthia-web`: the modal no longer steals focus on every keystroke.
  Its focus effect was keyed on an `onClose` prop that every render
  replaced, so the cleanup re-focused the trigger after the first
  character and a typed name kept only its first letter.
- `synthia-server`: `?limit=` is accepted on the list endpoints. A plain
  `Option<u64>` cannot deserialize from a query string through
  `#[serde(flatten)]`, so `?limit=20` was a `400` on exactly the
  endpoints that document the parameter.
- `synthia-server`: `list_models` compares `If-None-Match` instead of
  treating its presence as a match, and derives the `ETag` from the
  response body. Previously any `If-None-Match` returned `304`, so with
  `Cache-Control: no-cache` a browser was pinned to the first model list
  it ever cached — a redeploy never reached it.

### Added (R125 — `synthia-mcp-server` stdio binary: outbound MCP, the plugin-brick tools shipped to any MCP client)

The "outbound" counterpart to R124's in-process `self_mcp`:
R124 let the agent reach into its own deployment via MCP; R125 lets
**any foreign MCP client** (the `pi` coding agent, a Claude Code
plugin, the Anthropic SDK, …) spawn `synthia-mcp-server` as a
subprocess and immediately have the same plugin-brick surface under
its existing JSON-RPC 2.0 stack. No Rust in the caller's process, no
network, no API key.

- **New crate `crates/synthia-mcp-server/`** — single binary
  (`synthia-mcp-server`) speaking MCP over stdin / stdout. Two
  `pub` modules: `lib.rs` (`ServerSurface`, `parse_argv`,
  `bridge`, `render_output`, `overview_tool`) and `stdio.rs`
  (`run`). Same protocol core as `synthia-mcp::server::serve` —
  inlined here because the stdio transport owns stdin / stdout
  directly rather than going through a loopback pair. A
  successful call against this binary is therefore also a
  successful end-to-end proof of the in-process MCP server seam.
- **Eight tools exposed by default**: `read` (`synthia-tool-read`),
  `write` (`synthia-tool-write`), `shell` (`synthia-tool-shell`,
  default safety policy), `TodoWrite` (`synthia-tool-todo`),
  `web_fetch` (`synthia-tool-web`, the only one that pulls
  `reqwest`), `schedule` (`synthia-tool-scheduler` over an empty
  `ScheduleStore` rooted at `<workspace>/.synthia/schedules/`),
  `search` (`synthia-tool-search` over an empty `synthia-search
  Registry`), and `synthia` (the synthetic overview tool
  returning the binary version, the workspace root, and the
  eight-name tool list in either JSON or text format). The bridge
  (`bridge`) adapts `Arc<dyn Tool>` to the `ServerTool` handler
  signature; the overview tool is built inline because it needs
  data the registry itself does not carry.
- **CLI**: `--workspace <PATH>` (default: `$PWD`,
  canonicalised, must be a directory) and `--name <NAME>`
  (default: `synthia`). `--help` and unknown flags report to
  stderr and exit code 2. argv parsing is a hand-rolled matcher
  in `parse_argv`; the binary owns no third-party CLI dep.
- **One `Context` per binary** — `surface.context("test-session")`
  builds the single `Context` (`session_id` + `workspace_root` +
  default truncation) every plugin-brick tool receives, so a
  foreign MCP client's subprocess keeps the same workspace
  confinement as a `synthia-server`-hosted agent. The
  `SharedClock` is also injected here (and threaded into
  `SchedulerTool::with_clock`) so tests pin time without racing
  the real clock; production defaults to `SharedClock::system()`.
- **`crates/synthia-mcp-server/tests/stdio_roundtrip.rs`** — 5
  integration tests that spawn the binary and drive the full
  MCP wire (initialize / notifications/initialized / tools/list /
  tools/call read+write / `synthia` overview / unknown-tool
  error envelope). All green.
- **`crates/synthia-mcp-server/tests/pi-integration.ts`** — a
  cross-language smoke that drives the binary from Node.js the
  way a foreign MCP client does (over stdin / stdout). Prints
  `OK: cross-language smoke passed (Node ↔ synthia-mcp-server)`.
  Same wire shape `pi` would speak if a `pi-mcp-adapter`
  extension wired it as a subprocess.

Verification: `cargo test -p synthia-mcp-server --lib`
**5/5** (`argv_parsing_round_trip`, `plugin_brick_tools_assemble`,
`overview_tool_round_trips`, `bridge_passes_input_through_to_the_tool`,
`render_output_errors_are_surfaced_as_err`);
`cargo test -p synthia-mcp-server --test stdio_roundtrip`
**5/5** (`initialize_advertises_protocol_version_and_tools`,
`read_tool_round_trips_through_stdio`,
`write_tool_creates_files_under_the_workspace`,
`synthia_overview_tool_lists_every_exposed_tool`,
`tools_call_with_unknown_tool_returns_error_envelope`);
`cargo clippy --all-targets --all-features --tests --all -- -D warnings`
0; `cargo +nightly fmt --all --check` 0;
`cargo doc --no-deps -p synthia-mcp-server` 0;
`make ci` (every gate: `check-mvp-deps`, `check-no-runtime`,
`check-public-api-runtime`, `check-pub-surface`,
`check-claim-language`, `check-clock`, `check-harness-shape`,
`check-test-layout`, `test-guides`) green;
`node tests/pi-integration.ts` → `OK: cross-language smoke passed`.
Reproduction: `cargo build -p synthia-mcp-server` →
`./target/debug/synthia-mcp-server --workspace /tmp/ws --name pi` →
send `{"jsonrpc":"2.0","id":1,"method":"initialize",…}` /
`{"jsonrpc":"2.0","id":2,"method":"tools/list"}` /
`{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"read",…}}`
over stdin and read the JSON-RPC responses off stdout.

### Added (R124 — subagent sessions become first-class; the agent manages itself via MCP)

Every `task` delegation now runs in its own session: delegated traces no
longer pollute the parent's `events.jsonl` or SSE stream, the child
session appears in `GET /api/v1/sessions` next to the user conversations
(`parent_session_id` + `agent_name` attribution on `SessionSummary` and
`SessionDetail`), and the child session stream is openable like any
other (`GET /api/v1/sessions/{id}/messages/stream`). The parent's transcript
keeps only two lightweight `subagent_enter` / `subagent_exit` marker rows
naming the child; the `task` tool result carries `[subagent session: <id>]`
so the chat can link to the child.

- `synthia-harness`: `AgentMeta` gains `agent_name: Option<String>` (the
  registry name of the delegated peer) and `prompt: Option<String>` (the
  dispatch prompt — set on the **first** forwarded event of the child
  only, so a session router can persist it as the child's `UserInput`
  row without cloning it onto every event of the run).
- `synthia-tool-task`: `run_subagent` sets `meta.agent_name` and
  `prompt`, and appends `\n\n[subagent session: <child_id>]` to every
  task result (success / failure / cancelled / empty) so the chat
  surface has a stable place to read the child id.
- `synthia-server`: new `session::subagent_router::SubagentRouter`
  persists wrapped `AgentEvent::Agent` frames into their own sessions
  via `SessionRegistry::sink(user, child)` instead of inlining them
  into the parent's log; durable inner events (the child's own
  `Model(Text|ToolUse|ToolResult)`) land unwrapped, nested
  `Agent(m1, Agent(m2, …))` is handled by recursion, and the child
  session's live broadcaster is fed when a controller exists for it.
  The parent's log keeps only the depth-1 enter/exit markers; deeper
  levels' marker rows are suppressed because the wrapped `Agent` row
  in the immediate parent's log already records the delegation.
  `persist_and_broadcast` short-circuits on `Agent` events so the
  parent's SSE never carries the child's events either. `ControllerInner`
  gains `subagent_router` and `record_usage_tree` walks the
  sub-tree so child token counts still hit `/api/v1/chat/usage`.
- `synthia-server`: `routes/sessions.rs` — `SessionSummary` /
  `SessionDetail` carry `parent_session_id` + `agent_name`; the
  registry loop reuses its single `sink.read()` for status + timestamps
  + attribution, and `scan_unregistered_transcripts` reads the first
  row of each disk transcript for the same `subagent_enter` marker.
  `ScannedSession` is a small struct that carries both the
  cursor-paginated row and the attribution.
- `synthia-server`: `routes/tasks.rs` rebased on the new markers
  (previously walked the parent log for `{type:"Agent"}` envelopes,
  which the router no longer writes). `TaskDelegation.prompt` is read
  from the child session's `UserInput` row on detail (kept empty on
  the list to avoid the extra sink reads); status is the exit marker
  when present, else `"running"`. `TaskDetailPage` no longer 404s on a
  valid delegation.
- `synthia-mcp`: an in-process **MCP server** + a **loopback transport**
  pair. `serve()` handles `initialize` / `tools/list` / `tools/call`
  with the same JSON-RPC 2.0 wire `synthia::mcp::McpClient` already
  speaks (no protocol fork), and the loopback pair gives the in-process
  case the same request / oneshot-reply semantics stdio / HTTP use.
  Every error path falls through to a single `server.respond(framed)`
  so the client never hangs on an unset `id`.
- `synthia-server`: `state::app_state::self_mcp` — the deployment's
  own management surface, served over MCP. Nine `mcp__self__*` tools
  (`agents` / `tools` / `skills` / `schedules` / `workflows` / `evals`
  / `tasks` / `models` / `attachments`), each action-discriminated
  (`list` / `get` / `create` / `delete` / etc.) and wrapping `Arc<AppState>`
  directly — `register_self_mcp` spawns `serve` over a loopback
  transport, runs the handshake, and publishes through the existing
  `register_mcp_tools(..., "self", ...)` path so the boot-time
  `mcp__*` → `Deferred` flip applies uniformly. The tool registry never
  grows per-tool code: handlers close over `Arc<AppState>`. Session /
  message endpoints are deliberately excluded — the agent managing
  its own transcript is the foot-gun the carve-out exists to prevent.
- `synthia-web`: a chat **side panel** (`src/components/chat/SessionSidePanel.tsx`)
  fed by pure derivations in `src/lib/session-panel-data.ts` over the
  same `Message[]` the list renders: the latest `TodoWrite` list
  (checklist + progress bar), every `task` delegation (peer name +
  status + link to `/sessions/<child>`), and every file the agent wrote
  (`write` tool blocks, newest write per path). The panel is
  default-open and its state persists in `localStorage` under
  `synthia.chatPanelOpen`; the toggle uses the right-side precedent in
  `useSidebarCollapse` (`Ctrl/Cmd+B`).
- `synthia-web`: inline distinction inside `SegmentView`'s
  `tool_block` branch — `TodoWrite` calls render as a checklist +
  `n/m` progress, `task` calls as a sub-agent card with a
  `查看子会话 →` link to `/sessions/<child>`, `write` calls as a
  file chip with the workspace-relative basename. The raw call / result
  JSON body stays one click away under the existing toggle, so
  debugging and reproduction are not sacrificed for the at-a-glance
  rendering.
- `synthia-web`: thinking-block visual separation — the
  `.nt-chat__segment--thinking` block gains a tinted surface, a
  dashed left accent rail, italic body, the label `思考过程`, and a
  one-line preview under the collapsed header. Together they read
  the scratchpad apart from the answer at a glance.
- `synthia-web`: `SessionSummary.parent_session_id` /
  `agent_name` and the matching `SessionDetail` fields drive a
  `子代理 · <agent>` badge on every child-session card and a
  `PARENT SESSION` / `SUB-AGENT` pair on the detail page. The
  marker replay path uses the existing `notice` segment type (no
  new `SegmentType`) so both the live stream and history replay
  render the same `已委派子代理 **<agent>** · [查看子会话 →]`
  note in context.

### Changed (R124 — subagent traces no longer inlined; `task` result carries the child id; pre-existing §3.8 violations in three adjacent server files fixed)

- **Subagent traces no longer in the parent.** `AgentEvent::Agent`
  frames are now consumed by `ControllerInner::persist_and_broadcast`
  as soon as they arrive and routed into the child session's own
  sink / broadcaster; the parent sees only `subagent_enter` /
  `subagent_exit` markers. Child usage counts still roll up into
  `/api/v1/chat/usage` (a recursive `record_usage_tree` walks the
  wrapped sub-tree).
- **`task` tool result carries the child session id.** Every
  `task` result the model sees — success or failure — ends with a
  `\n\n[subagent session: <ulid>]` line. The chat surface parses this
  with `childSessionIdOf(resultContent)` to render the link.
- **Clock discipline, three adjacent server files.** `make check-clock`
  (`chrono::Utc::now()` calls in library code; baseline 6, all of
  which live in `scheduler.rs` tests) failed at 10 because four
  production sites had been calling `Utc::now()` directly:
  `state/app_state/schedules.rs:83` (`tick`),
  `state/app_state/evals.rs:115` (`generated_at`),
  `routes/schedules.rs:172` / `routes/schedules.rs:249` (create + tick
  handlers). Each now reads "now" through the injected
  `synthia::core::SharedClock` — `SchedulesState` and `EvalsState`
  gained a `clock` field + a `build_with_clock` constructor (the
  existing `build()` defaults to `SharedClock::system()`) and a
  `SchedulesState::clock()` accessor so route handlers read from the
  state. No `CLOCK_BASELINE` change was needed.

Verification: `make ci` green; `make check-web` green;
`cargo test -p synthia-harness --lib` **241/241**,
`cargo test -p synthia-session --lib` **151/151**,
`cargo test -p synthia-tool-task --lib` **76/76**,
`cargo test -p synthia-mcp --lib` **28/28**,
`cargo test -p synthia-server --lib` **477/477**;
end-to-end browser verification (synthia-server + vite running
locally, model invoked through `/v1/messages`) confirmed the chat
panel populated, the inline checklists / sub-agent cards / file
chips rendered, the sub-agent link opened its child session page,
and `GET /api/v1/tools` listed all nine `mcp__self__*` tools with
`Deferred` exposure.

Live MCP proof (synthia-server `:18780`, /tmp/synthia-pi-test workspace):
the agent, prompted via `/api/v1/chat/sessions/.../messages`, issued
`tool_use id=call_190b3a1678c042108a0534d5 name=mcp__self__models
input={"action":"list"}`; the persisted `events.jsonl` records
`tool_result metadata.mcp_server="self"` and the exact JSON returned
came from `state.provider_catalogue` / `state.workspace_config`. A
second turn created a real skill through MCP
(`tool_use mcp__self__skills action=create name=e2e-pi-demo …` →
`tool_result metadata.mcp_server="self" text="created skill
'e2e-pi-demo'"`) — `find /tmp/synthia-pi-test/.agents/skills/` shows
`e2e-pi-demo/SKILL.md` with the content the agent sent, and
`GET /api/v1/skills` lists it alongside the three bundled skills.
This proves the in-process MCP server is the same code path the
agent reaches in production (no separate `mcp-server` process, no
hand-rolled shortcut): the `LoopbackTransport` ↔ `serve` ↔
`register_mcp_tools` ↔ `ToolRegistry` chain round-trips a real
provider call and the changes show up everywhere the HTTP API
sees them.

### Changed (R123 — harness surface audit: runtime-neutral event channel, order-independent compaction, hidden-tool refusal)

An audit of `synthia-harness` against its own claims (runtime neutrality,
lego assembly, minimal public surface, accuracy of the docs) found four
places where the code and the documented contract disagreed, and one
dispatch-path hole. Each is fixed at the seam the contract names, not at
the symptom.

- **The event channel is `futures`, not tokio.** `EventSink` — the only
  handle an out-of-tree `ReasoningStrategy` gets for publishing, and the
  only public constructor it had — took
  `Arc<tokio::sync::mpsc::UnboundedSender<AgentEvent>>`. That made the
  crate's most-advertised plugin seam unusable without a tokio
  dependency, contradicting `AGENTS.md` §3.7 ("事件通道用
  `futures::channel::mpsc`"). Both the sink and `Agent::run` now use
  `futures::channel::mpsc`, so the constructor costs no runtime
  neutrality and stays public for a strategy that unit-tests itself
  without an agent around it. `tokio` remains a dependency only for the
  default `Spawner` and the parallel-dispatch semaphore — neither is in
  a public signature.
- **Compaction resolution is per run, so setter order cannot lie.**
  `resolve_compaction_now` ran inside the two compaction setters and
  passed a *fresh* `TruncatingContextManager` as the decision core's
  fallback, so `.with_context_manager(MyCm).with_compaction_settings(..)`
  silently discarded `MyCm`, and `.with_compaction_settings(..)
  .with_context_manager(..)` silently discarded the policy. It is now
  `effective_context_manager`, called from `runtime_for_run` with the
  caller's own manager as the fallback: an enabled + valid policy
  replaces it, anything else keeps it, in either order. The crate's own
  `assemble_with_builder` example had been printing "summariser armed"
  while installing no checkpoint because of this; it no longer does.
- **A hidden tool is no longer callable.** `is_hidden` is the registry's
  privacy flag — `ToolRegistry::run_stream` refuses such a call — but the
  loop resolved entries through `Registry::get`, which does not filter, so
  a model that emitted a hidden tool's name got it executed. The lookup
  now refuses it with the same model-facing error the registry produces.
- **`AgentOutput` is crate-internal and no longer carries a dead field.**
  Its `events: Vec<AgentEvent>` was hardcoded empty at its only
  construction site while the crate root advertised it as "events emitted
  by any `Agent::run`"; the field is deleted and the type is
  `pub(crate)` — no public signature returns it, so a run's public
  surface is its event stream plus `SystemEvent::SessionEnded`.
- **Removed: `context_manager_for_compaction`.** A second public entry
  point whose only body called its `_with_emitters` sibling; the sole
  in-repo caller (the server's run factory) uses the sibling. `resolve`
  drops to `pub(crate)`.
- **Demoted: `AgentEntry::descriptor_mut`.** It was reachable as
  `synthia_harness::agent::AgentEntry::descriptor_mut` (the type is a
  curated re-export) and self-described "test-only" — it existed so the
  registry's own tests could seed a version / tool list / owner by hand.
  Now `#[cfg(test)] pub(crate)`: a write handle on a descriptor cache
  that is otherwise write-once at registration, absent from production
  builds entirely. The same-named `ReActAgent::descriptor_mut` stays
  `pub` — it replaces a running agent's descriptor wholesale, and
  `synthia-tool-task`'s tests call it across crates.
- **Module paths closed:** `config`, `input`, `prompt` and `compaction`
  are private modules; their types were already re-exported at the crate
  root, and no consumer used the module path.
- **Dependency placement:** `synthia-skill` moved from `[dependencies]`
  to `[dev-dependencies]` (only `examples/assemble_with_skills.rs` names
  it, and that example exercises no harness type), and `tokio-util` moved
  to `[dev-dependencies]` (every use is inside `#[cfg(test)]`). Both were
  compiled into every harness consumer for no library-side reason.
- **`make check-public-api-runtime` is now a script.** The previous
  one-line recipe scanned for `pub ` plus `tokio::` on the *same* line,
  which misses the ordinary leak: `use tokio::sync::mpsc;` binds `mpsc`,
  and `pub fn new(tx: Arc<mpsc::UnboundedSender<..>>)` names no runtime
  crate at all. The gate resolves each file's runtime `use` binders
  (including `as` aliases) and follows a public item across its signature
  lines. Verified against a synthetic fixture with both leak shapes, and
  against the tree the fix left clean.
- **`DetachedAgent::join` no longer runs the agent twice.** It took the
  event stream out of its slot and dropped it, so a second `join`
  re-entered `Agent::run` and re-executed every tool. The stream is now
  kept and the whole lifecycle (idle / running / done / exhausted) sits
  behind one lock, so a *concurrent* join replays the terminal event
  rather than reporting `EmptyStream` for a run that succeeded — the
  three correlated facts used to be three locks, and a queued caller
  could observe a finished run as an exhausted stream. The module docs also stop
  claiming the run starts at `spawn_detached` (it starts at the first
  `join` — the type holds no executor) and the
  `clippy::await_holding_lock` allowance whose comment described the
  opposite of the code is gone.
- **`DetachedAgent::join` leaves the handle clean when abandoned.** The
  single-lock rewrite installed a placeholder before awaiting
  `Agent::run`, so a caller that dropped its `join` future mid-await left
  that placeholder behind and the next `join` reported `EmptyStream` for
  a run that had never started. `Agent::run` may suspend before yielding
  its stream (`ReActAgent::run` happens not to), so the slot is now
  mutated only *after* the stream exists, with no await between the two
  writes.
- **`DetachedAgent::try_event` can now report closure.** `DetachedClosed`
  was unreachable: nothing ever cleared the receiver, and the handle held
  the sender for its whole life, so the error arm could not fire and its
  catch-all also swallowed the genuine closed signal — `Ok(None)` meant
  both "nothing yet" and "never again". The sender now lives inside the
  run's state and is dropped on the transition to `Done`/`Exhausted`, so
  a finished run yields its buffered tail and then reports
  `DetachedClosed`, giving a polling caller a terminating loop. The
  vestigial `Option` wrapper on the receiver went with it.
- **Docs corrected where they contradicted the code:** the crate root
  claimed prompt sections "identity / tool / skill / agent / rules" (the
  assembler renders identity / skills / agents and deliberately keeps
  tool schemas off the prompt); the harness README listed
  `ToolInterceptor`, steering and compaction as "deliberately not here"
  while the crate ships all three; `Agent::run` was said to detach "one
  task per **turn**" (it detaches one per **run**);
  `spawn_detached` was said to "spawn the agent's event stream into a
  background buffer" and the module layer tables called `AgentHandle`
  "background execution" — the run starts at the first `join` and the
  handle holds no executor, so no task is detached at `spawn_detached`
  at all (0 sites left; `synthia-tool-task`'s `GroupJoin` and
  `SubagentPool` use the word for their own lanes, which is accurate);
  `runtime_context.rs` said the production loop "passes
  `chrono::Utc::now()`" (it reads the injected `SharedClock`, which is
  what §3.8 requires); the event enum's doc said "five top-level
  variants" for a four-variant enum; and `AgentRunConfig`'s rustdoc
  opened mid-sentence. Six live docs also named a crate that does not
  exist (`synthia_agent::`) in copy-pasteable snippets — now
  `synthia_harness::`.
- **`run`'s tests moved to a sibling `tests.rs`.** `handle.rs`'s inline
  block had grown past the 400-line hard limit
  `make check-test-layout` is specified to enforce (the gate currently
  runs a temporarily relaxed limit — see its `RESTORE HERE` marker). Per
  the repo's rule for a module that outgrows it, `handle.rs` +
  `inbox.rs` tests now live in `agent/run/tests.rs` — a sibling file,
  not a same-named subdirectory. No production item had to widen its
  visibility for it: the "nothing runs until the first `join`" contract
  is asserted through the run counter, not the handle's internals.
- **A panicking tool no longer truncates the run silently.** The loop
  drove `Tool::stream` itself, without the `catch_unwind` the registry's
  own dispatcher applies — so a panic in a tool unwound the run's
  detached task, dropped the event sender with it, and the caller
  watched the stream end with **no** `SessionEnded`, unable to tell a
  finished session from a dropped connection. Reproduced first
  (`["System","System","Model","System","ModelDone"]`, no terminal
  event), then fixed. Thirteen call sites now run under a guard, through
  `crate::agent::re_act::{catching_panics, catching_panics_sync}` — the
  async form for awaited seams and the sync form, named distinctly so a
  reader can tell which is in play, for the plain-method ones. Guarded:
  the `Tool`, the `ToolInterceptor` plugin (which returns *before* the
  registry path, so it needed its own guard), the `ModelProvider`, the
  consumer-supplied `ContextManager`, the `RunInbox`, and `steering`'s
  `Hint` (both injection points) + `Tracker` (all four methods). The
  extractor stays shared in `synthia_core::panic_message`; the guards
  themselves live in the harness, which is their only user. Adding the
  `Hint` guard surfaced a second defect on the same call site:
  `inject_hints` evaluated every hint's `should_trigger`/`generate`
  *before* checking its `injection_point()`, so an `AppendToToolResult`
  hint ran a consumer's `generate` once per iteration and had the result
  discarded — `hint.rs` documents that injection points are honoured.
  The point is now checked first, and the test pins the routing (that
  hint is evaluated exactly once, at commit time). `agent_impl` additionally wraps the whole `strategy.run(..)` call
  — the only guard that reaches a panic in the loop's *own* code
  (bucketing, commit, hook fan-out), which no inner guard can.
  Three deliberate differences from the tool case: a panicking
  `ContextManager` ends the run **reported** rather than resuming,
  because `prepare` rewrites the message list in place and a
  half-truncated transcript is what the next provider call would send; a
  panicking `RunInbox` or `Hint`/`Tracker` skips its own work, since the
  steering crate already isolates its guards and hooks the same way; and
  a panicking `ModelProvider` necessarily ends the run, there being no
  response to carry on with. Five regression tests pin it, each verified
  to fail without its guard.
- **A finished run reports nothing after `SessionEnded`.** The
  cancelled-before-tool path finalizes *inside* the iteration — it emits
  `SessionEnded` and fans out `OnAgentEnd` inline, and the orchestrator
  returns that output verbatim — but the close phase ran anyway. With
  steering queued mid-sample, that appended user turns to a terminal
  history and emitted `SteeringInjected` *after* the terminal event,
  contradicting the crate's own contract that `SessionEnded` is what
  tells a consumer the run is over (reproduced: `SessionEnded` was not
  the last event). The close phase is now skipped for an already-reported
  run, and the typed `iteration_end` that would have been skipped with it
  is emitted on the cancel path instead — with the accurate action
  `"cancelled"` rather than the `"tools_complete"` the close phase would
  have mislabelled a run in which no tool ran.
- **A failed sample keeps its reason.** `sample_or_fail` logged the
  `SessionEndReason` and returned `None`, so the caller substituted
  `"sample_once returned no outcome"` — every sample failure, including
  an HTTP error and a panicking provider, reached `SessionEnded` as that
  same opaque string. It now returns the reason it was given.
- **`SystemPrompt`-point hints behave as documented: once, at session
  start, per hint.** `SessionState::system_hinted` existed to enforce
  "appended to the system message once at session start" but nothing ever
  set it, so the guard around `append_to_system_prompt` was a constant: a
  hint triggering every iteration re-appended itself each time
  (reproduced: marker count 2 across two iterations), growing the system
  message — the prompt-cache stability this crate keeps volatile facts
  out of the system prompt to protect. Fixing that exposed a second
  defect in the same three lines: one boolean cannot serve *several*
  `SystemPrompt` hints, because the first to fire would permanently
  silence the rest. The state is now a set of injected hint **names**, so
  each injects once, and the append is gated to iteration 0 to match the
  documented "at session start" rather than mutating the cached prefix
  mid-run (no shipped hint uses this point; a late-firing one belongs at
  `BeforeNextLlmCall`). Two tests pin it: one hint injects exactly once
  across two triggering iterations, and two hints both do.
- **A panic's message is no longer thrown away.** `panic_message` was
  called as `panic_message(&payload)` in the registry dispatcher. A
  `Box<dyn Any + Send>` is itself `Any`, so `&payload` unsized to
  `dyn Any` *over the Box* and every downcast missed: the text was
  always the opaque fallback. The canonical test read that as
  "implementation-defined", documented it as such, and asserted only the
  wrapper — while the trait's own docs promised the output carried "the
  panic message". Passing the payload by value restores it, and the
  canonical test now asserts the message survives (fails without the
  fix).
- **One panic extractor, where there were four — and one fallback, where
  there were three.** The payload-to-string conversion existed in
  `synthia-tool`'s dispatcher, `synthia-steering`'s hook runner, its
  guard runner, and inline in `synthia-server`'s error middleware, with
  fallbacks `"<non-string panic>"`, `"unknown panic payload"` (twice) and
  `"unknown panic"`. All four now call
  `synthia_core::panic_message`, whose fallback is the named
  `NON_STRING_PANIC` = `"(non-string panic payload)"`. **Behaviour
  change:** logs that used to contain `unknown panic payload` or
  `unknown panic` now carry `(non-string panic payload)`, so a grep for
  the old strings will find nothing. The unified signature takes
  `Box<dyn Any + Send>` **by value** on purpose — a reference parameter
  re-admits the `&payload` bug above, which compiles and silently
  degrades; by-value makes it unrepresentable at the call site.

- **A dropped event is now observable.** The detached run's event buffer
  is bounded, and the drive loop discarded every overflow with `let _ =
  try_send(..)` — so a run that outran its poller silently produced a
  truncated transcript that looked complete. Each drop is now counted
  and reported once, with the count and the requested capacity, when the
  run ends. The capacity itself is documented as *requested*: the
  channel's real bound is `buffer + num_senders`, so the warning names
  the number to raise rather than restating the arithmetic.
- **Tests:** new `agent::re_act::tests::compaction_wiring` pins the
  setter-order contract (all three tests fail against the pre-fix code);
  `loop_lifecycle` gained a hidden-tool refusal test, and the detached
  handle `join_twice_runs_the_agent_once` plus a concurrent-join test
  that reproduces the `EmptyStream` report; `loop_lifecycle` gained
  `panicking_tool_still_reports_session_end` and
  `panicking_interceptor_still_reports_session_end`, and
  `synthia-tool`'s `tool_panic_isolation` now asserts the panic message
  survives — each verified to fail against the pre-fix code. The four tautological `AgentOutput` tests
  (field construction, `Clone`/`Debug` presence, `Vec` ordering) were
  deleted with the field they pinned.
- **Poisoned-lock panics removed from the loop's hot paths.** The
  streaming accumulator, the detached handle's state, and the run inbox
  now use `parking_lot::Mutex` (already a dependency) instead of
  `std::sync::Mutex`, deleting all nine production
  `expect(.. "poisoned")` sites — several of them on the per-chunk path,
  where the panic would have terminated the run with no `SessionEnded`
  to report it with.

Verification: `cargo test -p synthia-harness --lib` **233/233**;
`cargo test -p synthia-server --lib` **398/398**; `make ci` green;
`make examples` green; `scripts/check-public-api-runtime.sh` green
(plus the harness-shape script and the inline gate targets).

### Changed (R122 — the harness gets a shape: layered modules, a private surface, and the `team` layer deleted)

`synthia-agent` had grown a public surface that was really internal
structure. `agent::` exposed ten modules including `re_act`'s own
submodule tree, an out-of-band "team protocol" layer no caller in this
repo used, and a subagent-batching type whose only consumer lives in
another crate. The round answers one question — *what is the harness,
and where does the rest go?* — and each change below is an answer, not
a tidy-up.

**The `team` layer is deleted (1,116 lines: 660 production, 297 of its
own tests, 159 of example).** `agent::team` shipped `BoundAgent` (a name plus one async
`prompt -> text` call), `FnAgent`, `Verifier` / `Verdict` /
`VerificationChain` / `ChainOutcome` (generate, verify, retry with the
rejection reason appended), and `RoundRobinGroupChat` /
`TerminationCondition` / `MarkerTermination` / `MaxRoundsTermination` /
`Turn` / `StopReason`. It was adopted from `traitclaw-team`'s
`{lib,execution,group_chat}.rs` and it was correct — but *correct code
with no caller is weight*, and each of its three pieces has a
first-class home elsewhere in this workspace:

- **Delegation is the `task` tool.** `TaskDelegator` in
  `synthia-tool-task` runs a registered peer as a depth-capped child,
  forwards its events, and commits its final text — with gates,
  worktree isolation and admission control the `prompt -> text` seam
  never had. `BoundAgent`/`FnAgent` were a *second*, thinner
  multi-agent vocabulary in a workspace whose §3.7 rule is that new
  synthetic tools become plugin crates implementing `ToolInterceptor`.
- **Verify-then-retry is a scorer.** `CandidateScorer` behind
  `BestOfNStrategy` (or `LlmJudgeScorer`) samples N answers and keeps
  the best; a `synthia-workflow` gate re-runs a call until its command
  exits zero. `VerificationChain` was a third mechanism for the same
  job, with a *different* retry policy (`max_attempts` + a
  `with_feedback` flag) that nothing outside its own tests selected.
- **A stop condition is the loop's.** The ReAct loop stops on
  `max_iterations`, a follow-up drain, or cancellation; a round budget
  or a `DONE` marker scanning another transcript is a second stop
  vocabulary nothing in the repo composed.

The outward-facing proof that nothing was lost is that the workspace
still declares every one of those capabilities — in `synthia-tool-task`,
`synthia-workflow` and the strategy seam — and the whole workspace
compiles with zero references to the deleted names. `docs/reference-parity.md`
now records the two declined halves with their reasons instead of
mapping them onto types that no longer exist.

**`GroupJoin` moved to `synthia-tool-task`, where its consumer is.**
Batching a fan-out's completions into one notification is a thing *a
host that runs subagents* wants; a host that does not has nothing to
batch. It was in `synthia-agent` next to the loop, and its own example
had to name `AgentHandle`/`DetachedAgent` from a different module to
explain itself. Both moved, and both now say what they are for:
`pool` decides *whether* a child starts, `GroupJoin` decides *when the
host hears about it*. `synthia_harness::GroupJoin` is gone; import it
from `synthia_tool_task`.

**The three reasoning strategies moved to `agent::strategy`.** They were
`agent::{cot, best_of_n, best_of_n_llm_judge}` — siblings of the *seam*
they implement rather than members of it, so the module that documents
"`ReActAgent` is one way to reason; this module makes that a choice"
sat one directory away from the two other choices. Now `strategy/{mod,
runtime, sink, request, scorer, cot, best_of_n, llm_judge}.rs`, with two
extractions that the move made obvious:

- `CandidateScorer` + `LongestAnswer` are `scorer.rs`: the best-of-N
  extension point is *how candidates are judged*, and it was buried in
  the strategy that consumes it (file was 683 lines, of which 337 were
  tests).
- The one-pass request both single-shot strategies ask is `request.rs`.
  Cot and best-of-n built the same `CompletionRequest` — same prompt
  assembly, same history, same window fit, same `ToolChoice::None`,
  same `max_tokens` — in two copies that differed only in an appended
  instruction. One function, `single_shot_request(runtime, input,
  instruction)`, so the two cannot drift on the three decisions that
  make them what they are.

**Every submodule of `agent` is private now.** Its
`{descriptor, interceptor, re_act, registry, run, spawn, strategy}` are
`mod`, not `pub mod`, and `agent/mod.rs` re-exports the names they are
*for*. (The five crate-root modules — `events`, `input`, `config`,
`prompt`, `compaction` — stay public: they are the pieces a consumer
names directly.) Two consequences
worth the churn: internal structure stops being public API (adding a
`pub` item inside `re_act::loop_` can no longer widen the contract by
accident), and the module root becomes a readable map — the doc comment
is a table of *contract / loop / reasoning / driving a run / catalog /
plugin seam* rather than ten module links. The intra-crate and
cross-crate doc links that named the old deep paths were repointed at
the re-exports in the same pass, because `make ci`'s `doc-check` runs
rustdoc with `-D warnings` and a prose link to a private module is a
hard error.

**Run control got its own layer.** `AgentHandle`/`DetachedAgent`
(`handle.rs`) and `RunInbox`/`MpscInbox`/`RunInboxHandle`
(`run_inbox.rs`) answer one question between them — *what can the
outside world do to a run that is already moving?* (start it in the
background and abandon the wait without cancelling the work; push
messages into it) — and neither knows which strategy is running, nor is
either needed to run an agent. They are `agent::run`.

**`synthia-tool::Context` lost three dead fields.** `dispatch_mode`
(with the whole `DispatchMode` enum: `Fork` / `Teammate` / `Worktree`,
whose `Teammate` variant is the last trace of the deleted team layer),
`caller_agent`, and `messages` (+ `with_messages`) were never set by any
runtime and never read outside tests. `messages` was the worst of the
three: its doc comment promised that "context-aware tools (e.g.
`self_reflect`)" could review the session history — a tool that has
never existed anywhere in this repo. A field that documents a capability
nobody implemented is worse than no field. `Context` is now three fields
(session id for output-spill attribution, workspace root for path
confinement, output truncation config), which is exactly what the
paradigm needs and what §3.7's "the context is the same for every tool"
rule permits.

**The harness shape became measurable — because it was not.** The
round turned up that `clippy.toml`'s `cognitive-complexity-threshold =
20` cannot be the reason this loop is split: `clippy::cognitive_complexity`
is allow-by-default, and its own docs now say it no longer measures what
its name suggests — it charges **decisions only** (no nesting, no loops,
no `?`, closures measured as separate items). Measured against that
algorithm, nothing in `agent::{re_act, strategy}` comes near the
threshold: the maximum was `handle_chunk` at **8/20**. `drive.rs`'s claim
that its helpers are "kept thin to satisfy the cognitive-complexity
threshold" was therefore unbacked. What the harness *does* maintain is
**flat nesting and short functions**, and neither was measured. Four
real offenders were fixed, each by naming the thing that was being
repeated inline:

- `handle_chunk` (**121 lines**, the longest fn in the tree) is now a
  7-arm match of one-line arms over named handlers — `absorb_content`,
  `open_tool_buffer`, `absorb_is_done`, `push_tool_use`. The last one
  matters beyond length: recording one tool call took the same three
  steps (push to the history parts, publish on the wire, mark the id
  seen) at **three** sites — the streamed `ToolCallEnd`, the `IsDone`
  re-listing, and the early-termination drain. The first two now share
  `push_tool_use`, so the "a `tool_use_id` is never recorded twice"
  invariant holds in one place for every path that can violate it. The
  drain keeps its own copy **on purpose**: it runs after `seen_tool_ids`
  has been taken out of the state, so its dedup is a filter against the
  already-removed set rather than an insert, and routing it through the
  helper would need the set back.
- `pre_execution_veto` and `apply_guard_pipeline` repeated a denial tail
  inline; the restriction and guard sites were the same two statements,
  and the hook site was three (tracker, the `notify_error_hooks` await,
  then the error). They now share `denied(this, tool_name, call, state,
  message)` in `seams.rs` — "a refused call is always recorded and always
  explained" is one rule, not copies of the same tail. The hook site
  still calls it *before* the await, preserving the original order (the
  tracker recorded the denial before the error fan-out); that ordering is
  easy to lose when consolidating and is now pinned by
  `hook_block_surfaces_as_error_result`, which installs a tracker-counting
  `AgentHook` and asserts the count is non-zero inside `on_error`.
- `sample_once` built its `CompletionRequest` literal (20 lines, 10
  fields) and its 8-field result log inline; those are
  `completion_request(..)` and `log_sample_result(..)`, leaving the pass
  itself as the pipeline it documents (notify → stream → observe).
- `drive.rs` rebuilt the `IterationDispatch { … }` literal at five sites;
  it is now intent-named constructors (`continuing` / `pruned` /
  `truncated_batch` / `early` / `completed`), which also makes the
  *reason* a dispatch differs visible at the call site.

**Two rounds of documentation rot got a compile-time fix.** `MINIMAL.md`'s
Step-3 table tells a consumer which setter to call, and it had been
hand-repaired once before (R47): its cells are inline code spans, invisible
to rustdoc, so nothing ever compiled them. R122 found **five** more stale
names there (`.spawner` / `.typed_event_sink` / `.output_schema` /
`.compaction_settings` / `.run_inbox` — each lost its `with_` prefix in
R110) and **eight** in `SEAMS.md`, the seam index whose whole purpose is to
say which call to write (`.context_manager`, `.steering`,
`.typed_event_sink`, `.tool_restriction`, `.strategy`, `.spawner`,
`.run_inbox`). Every one of those bare names *resolves* — they exist as
getters — so a naive existence check would have passed them while the
recipe could not type-check.

The fix is not another hand-repair: `MINIMAL.md`'s Step-3 recipes are now
transcribed into one **compiled fence** (the file's `rust` fences are
doctests via `include_str!`), so renaming a setter breaks the build.
Verified to have teeth — restoring the old names fails with `E0061: this
method takes 0 arguments but 1 argument was supplied`, exactly the
getter-vs-mutator error. `make test-guides` runs the fences under both the
default features and the seven-feature MVP subset, and `make ci` now
includes it: `doc-check` only *generates* docs, it never *compiled* an
example, so before this the guide's recipes reported green locally no
matter how wrong they were. The workflow's own raw-cargo doctest step in
the `tests` job is deleted rather than kept — it ran the same two
commands, so CI would have compiled the fences twice per run, and it was
the last gate not invoked through a `make` target (AGENTS.md §3.6). `SEAMS.md` has no such mechanism and stays
hand-checked, recorded in `docs/README.md`'s Known gaps with the reason
the obvious gate cannot work.

`make check-harness-shape` keeps it that way: no `synthia-agent`
**production** function over 100 lines or 4 levels of nesting
(`too_many_lines` + `excessive_nesting`, the two lints clippy's own doc
recommends in place of the inert one), scoped to `synthia-agent` on
purpose — a workspace-wide run at this threshold reports **24** sites,
of which **14 are production code** (the other 10 are benches, examples
and test files, which `--lib` excludes anyway), and this round touched
none of them; the longest is `run_task.rs`'s 415-line handler. Making
those hard errors would be an unrequested refactor.

Scoping is not cosmetic here, and getting it wrong the first time is
worth recording. Writing either threshold into the root `clippy.toml`
**enables the lint for every crate**, so with `make lint-rust`'s `-D
warnings` it turned an untouched `synthia-core` block into a hard error.
The thresholds therefore live in `scripts/harness-shape/clippy.toml`,
which the gate injects via `CLIPPY_CONF_DIR`; three facts about this
arrangement were each verified rather than assumed, because each was
wrong on the first attempt:

1. `excessive_nesting` defaults to **0 = disabled**, so a threshold is
   what makes the lint able to fire at all; and a threshold at the root
   enables it globally (proved: `-D warnings` on `synthia-core` exits 0
   without the key and 101 with it).
2. `CLIPPY_CONF_DIR` is honoured (proved: the same 7-level function
   reports 0 diagnostics without the variable and 1 with it).
3. cargo does **not** fingerprint `clippy.toml`, so the gate `touch`es
   the crate root before linting; without that it replayed a cached
   verdict.

The gate's three failure modes are each proven: a 125-line function
fails with its location, a 7-level nested function fails with its
location, and a crate that does not compile fails explicitly rather than
passing vacuously (an earlier revision *did* pass on a broken tree — the
`2>&1 | grep` form swallowed cargo's exit status).

Moving the thresholds also meant chasing every pointer to them, since
AGENTS.md §3.4 requires a config change to update its own reference in
the same change: §3.4 gains the gate-scoped `clippy.toml` as a baseline
file in its own right, §3.5 now names it as the shape budget's single
source of truth, §3.6's gate row and its drift rule both list it, and the
Makefile comment says why the thresholds are *not* at the root. A reader
following any of those pointers lands on the file that is actually read.

**Verification** — `cargo +nightly fmt --all` clean; `cargo clippy
--all-targets --all-features --tests --all -- -D warnings` clean;
`cargo doc --no-deps --workspace` under `RUSTDOCFLAGS="-D warnings"`
clean (three broken intra-doc links found and fixed);
`cargo check --workspace --all-features --all-targets` clean; `make ci`
exits 0 over its **12 prerequisites** (10 print an `OK:` line; `fmt-check`,
`lint-rust`, `doc-check` and the new `test-guides` print their tool's own
output), including the new `check-harness-shape`; `make examples` green
(all 39 example targets, plus both consumer crates: `CONSUMER-PROOF: OK`,
`MVP-OK`).

Tests. Both columns are **measured**, not derived: the "before" figures are
`cargo test -p <crate> --all-features` run in a worktree at `5216113e^`,
the "after" figures the same command here. Both totals include doctests.

| Crate | Before | After | Composition |
|---|---|---|---|
| `synthia-agent` | 254 | 232 | −10 `agent::team` tests and −2 team doctests (deleted with the module), −10 `agent::group_join` tests and −1 group_join doctest (moved to the plugin), +1 `scorer.rs` test = −22. Re-pathing `best_of_n` / `best_of_n_llm_judge` / `cot` under `strategy`, and `handle` / `run_inbox` under `run`, renames 32 test names without changing the count |
| `synthia-tool-task` | 72 | 83 | +11: the 10 tests and 1 doctest that arrived with `group_join` |
| `synthia-tool` | 226 | 216 | −10, exactly the `DispatchMode` / `caller_agent` / `with_messages` test fns (`git diff` lists them); doctests unchanged at 4 |
| `synthia-server` | 455 | 455 | unchanged: `git diff` over the crate shows no test function added or removed, only module paths in doc comments and `use` lines |

The `synthia-agent` delta reconciles against the test-name diff, but the
raw counts must be read with the renames factored out: 55 names gone and
33 added, of which **32 are renames** (the same test under a new module
path — `best_of_n` / `best_of_n_llm_judge` / `cot` under `strategy`,
`handle` / `run_inbox` under `run`). Matching on the leaf test name leaves
**23 genuine removals** — 10 `group_join` tests, 10 `team` tests, and 3
doctests — and exactly **1 genuinely new** test (`scorer.rs`). Quoting the
raw 55/33 as removals/additions would overstate both by 32. (After the
round the crate has no runnable doctests; the one entry `--list` still
shows for `prompt/mod.rs` is `rust,ignore`.)

### Added (the two crates nobody could call become tools)

`synthia-mcp` and `synthia-scheduler` were complete library crates with
no model-facing surface. An MCP server's tools reached the model only
if the *host* had published them at boot, and a scheduled job could not
be created from a conversation at all. Both gaps are the same shape —
*a capability the framework has and the model cannot reach* — and both
are closed by an adapter crate/tool, not by widening the harness.

**`synthia-mcp` gains `McpControlTool` — the `mcp` tool.** The existing
path publishes one local `Tool` per remote tool
(`register_mcp_tools`), which is right when the catalog is known at
assembly time and wrong when it is large, volatile, or discovered
later. `McpControlTool` is the dynamic counterpart: one tool with three
actions over a shared `McpSupervisor` — `servers` (each server's state
and registered-tool count), `tools` (one server's live `tools/list`,
schema included, optionally filtered to one tool), and `call` (invoke a
remote tool by its **server-side** name). `register_mcp_control_tool`
publishes it.

The tool cannot exceed the host's authority: it is bound to a
supervisor, so it only ever sees the servers the host already
supervised — no spawn, no socket, no registration. Unknown server and
disconnected server get different, corrective messages, because the
fixes differ. The remote-result projection (`isError`, and the
image/audio `ContentPart` mapping added in R71) moved into one
`output_from_result` that both `McpTool::call` and the control tool
use, so the two paths cannot drift.

An `isError` reply keeps exactly one shape: both paths now name the
failed call through the same helper, so a remote failure reads the same
whether it was addressed as `mcp__srv__echo` or as `<server>/echo`.

**The dynamic path is a second access path, not a way around the
guard** — and closing that turned up a pre-existing defect in the
registered path. `tools`/`call` bypass the registry entirely, so
`[tools] hidden = ["mcp__srv__x"]` would have stopped being enforceable
for remote tools: the flag is read nowhere on that route. The tool now
holds a **weak** handle on the registry the agent loop dispatches from
(weak because the registry owns the tool — a strong handle is a
reference cycle that makes both immortal) and consults exactly the flag
`run_stream` consults: `is_hidden` refuses it. `ToolExposure::Hidden`
deliberately does *not*, because the paradigm crate documents it as
advertisement-only and still executable — a tool that treated the two
the same would have been a second, stricter gate, which is its own bug.
The membership flag is read through the naming policy the supervisor
already applies, via a `pub(crate) McpSupervisor::naming`.

The defect that turned up: **a re-sync silently un-hid the deployment's
tools.** `swap_generation` rebuilt each entry with `ToolEntry::new(..)`
— registration defaults — after unregistering the previous generation,
so every reconnect or `tools/list_changed` reset `hidden` (and any
`exposure`) that an operator had set. `sync_mcp_tools` now captures the
visibility each surviving name carries *before* the swap and re-applies
it, so a policy decision outlives the connection it was made during.
Pinned by `a_resync_keeps_the_deployments_visibility_settings`.

A wrong premise was also retired in the example: `McpSupervisor::tick`
publishes the catalog into **whatever registry it is handed**, so
"the remote catalog is not pre-registered" was false whenever the real
registry was passed. `mcp_control_tool` now ticks a scratch registry,
which is the configuration its claim actually describes (the agent's
surface is the single `mcp` tool; the catalog is read on demand), and
the doc says the contrast is one of configuration, not of reach.

**`synthia-tool-scheduler` (new crate) — the `schedule` tool.**
`Scheduler` is deliberately host-driven: `tick(now)` reports due jobs to
whoever drives it. Nothing in `synthia-scheduler` is a `Tool`, so the
adapter is a new plugin crate following the `synthia-tool-*`
convention: `list` / `create` / `pause` / `resume` / `remove` over a
shared `ScheduleStore`, jobs addressed by id **or** name, `Tool::call`
in, `ToolOutput` out.

Three decisions worth naming:

- **The tool plans; the host delivers.** `create` writes a row and says
  so in its result; no action fires, waits, or spawns. A tool that
  fires its own payload would be a second delivery path beside
  `Scheduler::tick`, and the two would diverge.
- **`cron` is refused, not accepted.** The scheduler crate persists a
  cron expression but cannot compute a recurrence (`Job::advance` uses
  a one-minute placeholder and its own docs say the host parses), so
  `create` offers `interval` + `interval_seconds` and `once`. Accepting
  `cron` would have produced jobs that silently fire every minute — the
  schema and the enum now agree with what the crate can honour.
- **The clock is injected.** `SchedulerTool::with_clock` takes a
  `SharedClock`, so a test pins "now" and gets deterministic
  `first_fire_at` / countdown rendering; `new` uses the system clock.
  This is also why `create` reports both the RFC 3339 instant and a
  human phrase.
- **The payload has a documented shape.** `Job::payload` is free-form
  by design (the crate stores and re-emits it), so a bare `payload`
  argument would leave the model with nothing to send. The tool names
  the convention `{"agent": "<peer>", "prompt": "…"}` — `TaskSpec`'s own
  vocabulary, the reference consumer of `Job::on_fire` being a subagent
  notification — in both the crate docs and the JSON Schema description.
  It is a convention, not validation: the host interprets the payload.

**Where it is not wired, and why that is deliberate.** Neither the
`mcp` tool nor the `schedule` tool is installed by `synthia-server`.
For `mcp`, `register_configured_mcp_servers` ticks its supervisor
exactly **once** at boot and drops it — only `healthy_clients()` and
`tick.health` escape, and `AppState::mcp_health` is that frozen
snapshot — so there is no driver loop and no `Arc<McpSupervisor>` to
hand the tool; a retained supervisor would report boot-time health
forever. The tool's docs now state that a host installing it must drive
`tick`. For `schedule`, the server has no scheduler at all (by R29's
own note, that wiring is a separate concern) and would additionally need
a store-path config key. What the deployment already has is the
pre-registered path: `tick` publishes every configured server's tools
into the server's registry at boot. Both new tools are
consumer-assembled, and `MINIMAL.md` / `SEAMS.md` carry the
registration lines.

**Naming.** The crate is `synthia-tool-scheduler` (not
`-schedule`): the `synthia-tool-*` crates name the *domain* they adapt
(`-todo` ships `TodoWrite`, `-web` ships `web_fetch`), so mirroring the
crate it adapts — `synthia-scheduler`, whose tool is `schedule` — is
the consistent choice.

**A remotely triggerable host DoS, closed.** `create` accepted any
`u64` for `interval_seconds` (`minimum: 1`, no maximum). The chain then
ran in the **host**, not in the tool call: `create` wrote the job,
`Scheduler::tick` called `Job::advance`, and `advance` did
`interval.as_secs() as i64` into `chrono::Duration::seconds` — which
panics above `i64::MAX / 1000` (~9.2e15). Because the failed advance is
never persisted, the panic repeated on every subsequent tick. A value
above `i64::MAX` was the quieter variant: `as i64` wrapped negative,
leaving a past `next_fire_at` and a job that re-fires forever. Both are
now refused at `create` against a new documented bound,
`synthia_scheduler::MAX_INTERVAL_SECS` (10 years — far beyond any
scheduling horizon, ~13 orders of magnitude below the limit), with the
schema carrying the matching `maximum`.

The complementary half is in the scheduler crate, which is where the
panic-ability actually lived: `Job::advance` is now **total** — every
arithmetic step is checked (`chrono::Duration::try_seconds`, and
`checked_add_signed` for the instant, which panics on overflow too), with
`ScheduleError::{IntervalOutOfRange, NextFireOutOfRange}` for the two
failures. `Scheduler::tick` also stopped papering over the error it was
handed: `let _ = j.advance(now)` used to emit a fire event for a job
that had *not* advanced, which is a false claim about the schedule — it
now logs at `warn` and skips that firing. Reproduction before the fix,
via the tool alone: `create interval_seconds: 1e16` returned success and
the next `Scheduler::tick` panicked in `chrono`; both are now regression
tests (`create_refuses_an_interval_that_could_panic_the_host`,
`out_of_range_interval_is_an_error_not_a_panic`), and the boundary value
itself is pinned as schedulable.

**`Completed` is terminal, and the tool now says so.** `pause` on a
completed job made the row un-reapable — `gc_completed` only ever
collects `Completed` rows, so a paused ex-completed job leaked on disk
forever — while `resume` set `Active` on a row that kept its original
past `next_fire_at`, re-delivering a one-shot that had already fired.
Both are refused with a message naming the alternatives (`create` a new
job, or `remove` the row), pinned by
`a_completed_job_cannot_be_paused_or_resumed`, which also re-asserts the
row stays sweepable.

**Parking, not skipping, when a job cannot advance.** Refusing the
absurd cadence at `create` is the first line; the second is what
`tick` does when a job still cannot advance (the library API accepts an
interval the tool refuses). My first version skipped the firing and left
the job `Active` with its stale `next_fire_at` — which is worse than it
sounds, and the reproduction is one line: `next_deadline` then reports
an elapsed deadline (`Some(0ns)`), so a host arming its timer from it
re-ticks immediately, and every pass takes the per-job file lock and
rewrites the row. That is a disk-write hot loop rather than log noise.
`tick` now sets `JobStatus::Paused` in the same `update` closure: not
due, out of `next_deadline`, visible to `list`, addressable by
`remove` / `resume`, and the `warn` fires once instead of per pass.
Deliberately **not** `Completed` — not to dodge the sweep (with the
stamp taken only on success, a `Completed` row would fail
`gc_completed`'s `last_fired_at.is_some_and(..)` filter anyway), but
because `Completed` *means* "this unit of work fired" and is terminal:
`list` must not report a job that ran nothing as finished work, and the
tool layer refuses to pause or resume a completed row. Pinned by
`an_unadvanceable_job_is_parked_not_left_due`, which asserts
`next_deadline` is `None`, the status is `Paused`, no fire time was
recorded, and the row stays unsweepable.

**The model cannot write unbounded state.** Every `create` is a job file
on disk and nothing in-tree reaps them (`gc_completed` collects only
`Completed` rows, and only when the host calls it), so a model looping on
`create` grew `<root>/schedules/` without limit. `MAX_JOBS` (64) caps it
before the write, and the model is told the limit in the tool's
description — the same shape `synthia-tool-todo` uses for
`MAX_TODO_ITEMS`. Unlike the interval bound this one has no schema slot
(`create` adds one job per call; no argument carries a count), so the
description is where it lives.

**Two error messages that pointed the wrong way.** A duplicate name was
surfaced through the store's generic `Io` category ("create failed:
schedule store I/O: duplicate schedule name: X"), which reads to the
model as an infrastructure failure; `create` now checks `has_name` first
and says "A job named `X` already exists — including a completed one,
whose name stays taken. Use `remove` on it first, or pick a different
name." That also fixes the remedy in the terminal-status refusal: it had
said "use `create` for a new job", which *cannot* work while the
completed row holds the name, so it now names the order (`remove` first).
Both pinned, the second by a test that actually performs the sequence it
recommends.

**One API change, because the old one contradicted its own docs.**
`Scheduler::new` took `ScheduleStore` **by value** while its doc said
"the store may be shared with other readers (operator dashboards,
manual fire paths)". It could not be: `ScheduleStore` has no `Clone`,
and its `list()` serves from its own cache, so a second store over the
same root answers stale. `Scheduler::new(Arc<ScheduleStore>)` makes the
documented sharing real and gives the tool and the host one view of the
schedule (the per-job file locks still cover other *processes*).
In-crate callers updated; no consumer call sites exist yet.

**Wiring.** New crate in `[workspace.members]` +
`[workspace.dependencies]`; facade feature `tool-scheduler` (opt-in,
like `mcp`: both need a host-constructed runtime object, so neither
belongs in the default set) with the module, table row, and a
`compile_fail` gate proving the module is absent without it;
`make check-mvp-deps` asserts the sixth offline tool crate pulls no
HTTP/OTel/DB dependency. Docs swept: `README.md` (member count 19 → 26,
which had gone stale, plus the plugin rows and example ladder),
`MINIMAL.md`, `SEAMS.md`, `AGENTS.md` §1/§3.6/§3.7,
`docs/examples/README.md`, `synthia-tool`'s crate docs, and the facade's
`mcp` / `scheduler` / `tool` module docs.

**Verification** — `cargo test -p synthia-mcp` 74/74 (15 in
`tests/control_tool.rs`, including the governance and re-sync ones),
`cargo test -p synthia-tool-scheduler` 24/24 (18 integration + 5 unit +
1 doc), `cargo test -p synthia-scheduler` 19/19 lib (the crate has no
doctests) after the `Arc` change and the totality work,
`cargo test -p synthia` 12/12 doc tests on the default set and 20/20 on
the CI MVP subset, 19/19 `--all-features` (the new `tool-scheduler`
reachability test included), `synthia-tool` 196+ and `synthia-server`
402+ unchanged. Three defects were proven by reproduction before the
fix, each by reverting it: the host-DoS chain (one `create` with
`interval_seconds: 1e16` → `Scheduler::tick` panicked in `chrono` at
`TimeDelta::seconds out of bounds`), the busy-loop variant of the same
job (tick returned `next_deadline: Some(0ns)` with the row still
`Active`, i.e. a re-tick plus a file-lock rewrite per pass, now
`None`/`Paused`), and the failed-advance stamp (`last_fired_at` set on a
job that never fired). All three are regression tests now
(`create_refuses_an_interval_that_could_panic_the_host`,
`an_unadvanceable_job_is_parked_not_left_due`,
`a_failed_advance_does_not_stamp_last_fired`,
`out_of_range_interval_is_an_error_not_a_panic`). Both examples run
against real dependencies and print their proof line: `cargo run
--example mcp_control_tool -p synthia-mcp` → `MCP-CONTROL-TOOL: OK` (a
real `sh` MCP child spawned, catalogued at run time, called, and the
agent's surface confirmed as the single `mcp` tool), `cargo run --example
schedule_jobs -p synthia-tool-scheduler` → `SCHEDULE-TOOL: OK` (the job
the tool created is the job the host's tick delivers). `make ci` **9/9**,
`make check-mvp-deps` and `make check-no-runtime` green (the sixth
offline tool crate stays HTTP-free, and `synthia-scheduler` stays
tokio-free — the `Tool` impl lives in its own crate, never in the
runtime-free one).

### Changed (R121 — the shell owns its execution policy; the `task` tool becomes a `synthia-tool-*` crate)

Two pieces of one concern: *a tool's boundary belongs to the tool, and
every agent-facing tool is a `synthia-tool-*` plugin crate*.

**The OS execution policy left the paradigm crate.** `ExecutionPolicy`,
`SandboxBackend`, `BwrapBackend`, `NoopBackend`, `ConfinedCommand`,
`SandboxError`, `BackendAvailability`, `bwrap_profile_args`,
`resolve_confined_command`, `denial_dialect`, `classify_denial`,
`denial_marker`, `policy_prompt_block` and `effective_policy` moved
from `synthia-tool::sandbox` to `synthia-tool-shell::sandbox` (re-owned
by `synthia-tool-shell`, which already re-exports them from its root).
The defect this closes was real and silent: the policy lived on the
shared `Context` (`policy` + `sandbox` fields), and **no production
code ever set it** — the loop builds `Context::new(..)` — so
`ShellTool::with_policy(ExecutionPolicy::ReadOnly)` changed only the
description the model was shown while the call spawned unconfined. The
description and the argv now come from one value: `ShellTool` holds its
own `policy` + `sandbox` (`with_policy` / `with_sandbox`) and
`call()` resolves the wrap from them. `Context` loses both fields
(it is the context *every* tool shares, so a per-tool boundary on it
would let one tool's setting change another's behaviour).

`effective_policy` also stopped taking `&[SessionEvent]`: it folds the
`sandbox_mode` wire names (`&[&str]`), which was the sole reason
`synthia-tool` depended on `synthia-session`. The tool paradigm now
depends on `synthia-core`, `synthia-provider` and `synthia-telemetry`
only, and the tool plugin crates stay free of the session crate as
`AGENTS.md` §3.7 requires. `synthia-tool` also dropped its two
now-unused dependencies — `libc` (nothing in the crate mentions it;
the `SIGTERM`/`SIGKILL` escalation lives in `synthia-tool-shell`'s own
manifest) and `chrono` (zero `chrono::` uses in `src/`, the old
`sandbox.rs` included) — which is why they disappear from the lock
diff.

**The delegation plugin is now a tool plugin.** `synthia-delegation`
→ `synthia-tool-task` (facade feature `delegation` → `tool-task`,
module `synthia::tool_task`), so the crate that ships the model-facing
`task` tool follows the same convention as `-read` / `-write` /
`-shell` / `-todo` / `-web`. Modules, types and examples are unchanged
(`task`, `gate`, `worktree`, `pool`, `router`, `route_rules`,
`mention_clone`; `TaskDelegator`, `TaskSpec`, `SubagentPool`, …). It is
the one plugin that also depends on `synthia-agent`, because it
installs through the `ToolInterceptor` seam — `AGENTS.md` §3.7 now says
so instead of implying every tool plugin depends on the paradigm crate
alone. `make check-mvp-deps` gained it to the offline list (five
crates, none pulling an HTTP client), and the doc/gate sweep covers
`README.md`, `AGENTS.md`, `MINIMAL.md`, `SEAMS.md`, `docs/README.md`,
`docs/examples/README.md`, `crates/synthia-agent/README.md` and the
facade module docs.

**A behaviour change, not just a deletion: `write` now runs
sequentially, and the queue it replaces is gone.** `synthia-tool`'s
`mutation_queue` (per-canonical-path write serialisation, ~180 lines +
3 tests) had **zero consumers** repo-wide — its own doc named
`WriteTool` as the consumer, and `WriteTool` never used it. That is the
same defect class as the sandbox: a single tool's concern parked in the
shared paradigm crate, `pub` while nothing outside it refers to it.

The race it was written for is real — reproduced before deleting it,
two concurrent `write` calls in `append` mode (`read_to_string` → build
→ `fs::write`) on one path left only the second body on disk — and the
framework already has the mechanism, which the write tool simply was
not using. `WriteTool::mode()` now returns
`ExecutionMode::Sequential`, joining `shell`, `web_fetch` and the MCP
proxy tools, and `synthia-tool-write` pins that in a test.

**User-visible consequence**: a batch of `write` calls in one LLM pass
now executes one call at a time (the Parallel bucket completes first),
and the loop's Sequential bucket aborts the remaining calls of the
round after the first failure. Only the serial package is affected —
`read` and `todo` stay `Parallel` (read-only / in-memory). The finer
per-path queue can come back inside `synthia-tool-write` if write
throughput ever justifies it; the paradigm crate is no longer where it
would live.

Verification (all counts are `cargo test -p <crate>` totals —
lib + integration + doc — measured at `0c542000`, where the sandbox
move, the rename and the queue deletion are all in tree):
`make ci` **9/9 OK** (including `check-mvp-deps` with the fifth
offline crate, `doc-check`, `check-claim-language`, `check-clock`,
`check-pub-surface`); clippy
`--all-targets --all-features --tests --all -- -D warnings` exit **0**;
`cargo +nightly fmt --all --check` clean; `make examples` runs every
example plus both standalone consumer proofs, all printing their proof
lines (`CONSUMER-PROOF: OK`, `MVP-OK`, `SANDBOX-POLICY: OK`).

The full workspace sweep — every member, one crate at a time per
`AGENTS.md` §3.3 — is **2746 passing / 0 failing** across all 25
crates. The ones this change touches:
`synthia-tool` **226** (196 lib — the 16 moved sandbox tests
and the 3 deleted queue tests are gone, nothing else changed),
`synthia-tool-shell` **39** (33 lib = the 17 pre-existing shell tests
plus the 16 that moved in from `synthia-tool`, plus the 6-case
`sandbox_smoke` integration suite),
`synthia-tool-task` **72** (66 lib, test-name set
byte-identical to the pre-rename `synthia-delegation`), `synthia-tool-write`
**15** (+1: the `Sequential` mode pin), `synthia-agent` **254**
(240 lib). `synthia-server`'s test inventory is byte-identical to
pre-R121 HEAD (its only edit is a one-line import rename), so its count
is unchanged by this change.

Test inventories were diffed **by test-function name** before/after
each move rather than by count: the 16 `sandbox` tests moved
`synthia-tool` → `synthia-tool-shell` and the 70 delegation tests
carried over unchanged; the only removals anywhere are the 3
`mutation_queue` tests that went with the deleted module.

### Changed (R111 — four deferred ≥120-line inline test blocks moved to sibling tests.rs)

R110c deferred four inline `#[cfg(test)] mod` blocks inside
`synthia-agent` production files; this round moves them, pure
code motion in the R106/R109 pattern:

- `re_act/stream.rs` 464 → 343 (+ `stream/tests.rs` 120) —
  the `finalize_buffers` dedup-contract tests.
- `agent/handle.rs` 404 → 266 (+ `handle/tests.rs` 137) —
  `AgentHandle` / `DetachedAgent` spawn-join-buffering tests.
- `agent/group_join.rs` 502 → 330 (+ `group_join/tests.rs` 171) —
  the `GroupJoin` pass/held/partial/timeout state machine.
- `agent/best_of_n_llm_judge.rs` 332 → 172
  (+ `best_of_n_llm_judge/tests.rs` 159) — the judge-scorer
  provider interaction + score-parse tests.

Each production file now ends at `#[cfg(test)] mod tests;` and
carries only its production concern; every moved test runs
under the same name, one path segment deeper
(`agent::re_act::stream::tests::*` etc.). Smaller inline blocks
(< 100 lines, e.g. `interceptor.rs`) remain by design — the
ratchet only tracks the ≥300 band, which is unchanged at 20.

### Added (R115)
- `synthia-search` crate: generic multi-domain search engine — BM25 + vector hybrid with pluggable `Tokenizer` / `Embedder` / `VectorStore` / `Filter<T>` / `Reranker<T>`, incremental add / tombstone remove / threshold-triggered compact, thread-safe `Arc<SearchEngine<T>>`, cross-`T` `Registry` keyed by `type_name`, agent-facing JSON projection (`agent_view` / `search_tool`). `HashingEmbedder` ships default; `ModelProviderEmbedder` behind the crate's opt-in `provider` feature (transport-free). Exposed on the facade via the `search` feature (default off). `check-mvp-deps` gains a dedicated synthia-search stanza (default + `provider`); `check-no-runtime` covers the crate's default build.

Verification: `synthia-agent` lib **240/240** (test-name set
identical modulo the path change); `synthia-server` **403/403**;
`cargo clippy --all-targets -D warnings` 0; fmt clean; `make ci`
**8/8** OK.

### Changed (R110 — `AgentBuilder` folded into `ReActAgent`; the harness is the only builder)

`AgentBuilder` (R12) and `ReActAgent::with_*` (pre-R12) were two
parallel types with parallel setters wiring the same fields — the
builder stored them, copied them onto a `ReActAgent` in `build()`,
and `ReActAgent` then exposed most of them again as `with_*`
setters the agent crate never used. Two parallel chains, two
places to look up "how do you set the steering?". The two chains
collapse into one:

- `ReActAgent` is **the** builder. Every `AgentBuilder` setter
  has a `ReActAgent::with_*` twin with the same semantics. The
  `agent::builder` module is gone (`mod.rs` 432, `tests.rs` 378,
  `compaction.rs` 212 → deleted); its remaining concerns moved:
- `CompactionEmitters` / `context_manager_for_compaction*` /
  `resolve_context_manager` (now `resolve`) moved to a new
  top-level `synthia_agent::compaction` module — they are the
  server-side run factory's helpers, not the agent's plumbing.
  Re-exported from the crate root as before.
- `with_compaction_settings(s)` + `with_typed_event_sink(sink)`
  with no external checkpoint now auto-builds a fresh
  `CompactionCheckpoint` from the sink and wires its emitters
  onto the summarising manager (the previous builder behaviour).
  `with_compaction_checkpoint(c)` installs an externally owned
  checkpoint (it carries its own sink + ledger); it takes
  precedence over the auto-built one.
- `with_output_schema(json)` auto-registers the
  `structured_output` tool AND stamps the descriptor. Same
  LIFO behaviour as before.

Every existing call site updated:

- The 5 agent-side examples (`assemble_with_builder`,
  `best_of_n_judge`, `runtime_agnostic`, `strategy_swap`,
  `hot_paths` bench) now use `ReActAgent::new(..)` +
  chained `with_*` setters — `assemble_with_builder` is
  unchanged in shape (still proves the full one-expression
  assembly), only the type name changes.
- The facade example (`synthia/examples/assemble_from_zero.rs`)
  and the two external-consumer examples
  (`docs/examples/external-consumer`, `docs/examples/minimal-consumer`)
  use the prelude's `ReActAgent` directly.
- The server was already `ReActAgent::with_*` (never built
  through `AgentBuilder`); no change.
- `synthia::prelude` exposes `ReActAgent` (not `AgentBuilder`).

Verification: `synthia-agent` **239/239** lib tests green
(down from 247: the 8 `AgentBuilder`-specific setter/clone tests
are absorbed into `ReActAgent`'s own setter coverage, plus the
2 `compaction::tests`); `synthia-server` **403/403** byte-identical;
`synthia` facade + every example + the two external-consumer
examples print their proof line; `make ci` **8/8** OK; clippy
`--all-targets -D warnings` 0; `make check-mvp-deps` and
`make check-no-runtime` OK; `make check-test-layout` band ratchet
unchanged at 20.

### Changed (R110c — the loop's cross-cutting helpers get their own file)

`loop_/drive.rs` (603 lines) held the orchestrator, its four
named phases, the per-iteration helpers, **and** the three
cross-cutting helpers called from more than one phase:
`hooks_on_agent_start`, `hooks_on_agent_end`, and `cancelled`.
The three moved to a new sibling `loop_/drive_hooks.rs`
(103 lines), leaving `drive.rs` at 522 lines with a doc
pointer to the new file. The `ReActLoop::cancelled` method
stays in `drive.rs` (it is the ergonomic surface the
per-iteration helpers call); the free functions it wraps now
live behind `super::drive_hooks`.

`loop_/mod.rs`'s layout table gained the `drive_hooks` row.

Verification: `synthia-agent` lib **240/240** OK
(byte-identical list); `synthia-server` **403/403**;
`cargo clippy --all-targets -D warnings` 0; `make ci` **8/8**
OK; fmt clean.

### Changed (R110b — `re_act/agent.rs` split: one concern per file)

`crates/synthia-agent/src/agent/re_act/agent.rs` (653 lines) held
five distinct concerns — the struct + four constructors +
`default_descriptor`, descriptor-mutation setters, runtime
setters + `resolve_compaction_now` + `runtime_for_run`, the
`Agent` + `RegistryItem` impls, and `projected_tool_definitions`.
The user's stated objective explicitly calls for the harness
itself to be "well-layered and well-laid-out". The 653-line
file split into four focused modules, all in the project's
established "every concern in its own file" pattern:

- `agent.rs` 250 — struct + `new` / `with_options` /
  `with_prompt_context` / `with_descriptor` constructors +
  `default_descriptor` helper.
- `agent_descriptor.rs` 81 — `with_name` /
  `with_instructions` / `with_model_hint` /
  `with_output_schema` / `set_prompt_context` /
  `descriptor_mut` (descriptor mutations).
- `agent_runtime.rs` 334 — `with_workspace` /
  `with_tool_registry` / `with_steering` /
  `with_context_manager` / `with_interceptor` /
  `with_max_iterations` / `with_tool_surface` /
  `with_tool_restriction` / `with_typed_event_sink` /
  `with_run_inbox` / `with_clock` / `with_spawner` /
  `with_strategy` / `with_compaction_settings` /
  `with_compaction_checkpoint` +
  `resolve_compaction_now` + `runtime_for_run` +
  `projected_tool_definitions`.
- `agent_impl.rs` 65 — `impl Agent for ReActAgent` +
  `impl RegistryItem for ReActAgent`.

`agent.rs` itself fits on a screen (250 lines). Public surface
unchanged — every existing `ReActAgent::with_*` call site,
example, server factory, and external consumer kept its name.

Verification: `synthia-agent` lib **240/240** OK
(byte-identical test list vs R110a); `synthia-server`
**403/403**; `cargo clippy --all-targets -D warnings`
0 warnings; `make ci` 8/8 OK; the `synthia-agent` docs comment
in `agent.rs` now carries a per-module table pointing each
caller at the right file.


 ### Changed (R109 — the ≥400 test-block tier closed; layout ratchet added to `make ci`)

The last two ≥400-line inline test modules
(`provider/types/content.rs` 401, `provider/openai/types.rs` 400)
moved to sibling `tests.rs` files; the tier is empty workspace-wide.
The bar is now enforced, not hand-swept:

- `make check-test-layout` — hard limit 400 lines per inline
  `#[cfg(test)] mod` block + a shrink-only ratchet on the ≥300
  band (baseline 20). Wired into `make ci` (now 8 gates).
- `make test-layout-audit` — lists every ≥300-line inline block.

AGENTS.md §3.6 documents both targets.

Report: [`docs/optimization-report-R109-2026-09-18.md`](docs/optimization-report-R109-2026-09-18.md).

Verification: provider **690/690** byte-identical list; agent
**247/247** + tool **215/215** re-run post-R108-cleanup;
`make ci` **8/8** including the new gate; both ratchet failure
paths negative-tested.

### Changed (R108 — the tool registry split one concern per file)

`tool/registry.rs` (1 369) — catalog types, the registration
value type and internal storage row, registration/mutation,
version-cached snapshots, scoped registration, the `run_stream`
dispatcher with its panic-capturing drain, and the private
`Registry` impl — split into `registry/` (`mod` 349, `catalog`
110, `entry` 242, `scope` 130, `snapshot` 180, `dispatch` 321,
`registry_trait` 92, the last promoted from an inline `mod` with
its body byte-identical). Every `synthia_tool::*` facade path is
unchanged; stale `// 1.`–`// 11.` section-number comments removed
with their sections.

Report: [`docs/optimization-report-R108-2026-09-18.md`](docs/optimization-report-R108-2026-09-18.md).

Verification: tool **215/215** byte-identical test list; agent
**247/247**, delegation **66/66**, server **403/403**, offline
tool plugins **14/14/17/17**; `make ci` **7/7**; clippy clean.

### Changed (R107 — the session controller split one concern per file)

`server/session/controller.rs` (2 328) — wire ops, the run-stream
factory seam, `RunDependencies`, the `SessionController` handle,
`ControllerInner` with its 533-line run task, the dispatch loop,
`RunLog`, and prompt shaping, all in one file — split into
`controller/` (`mod` 218, `ops` 70, `run_stream` 168, `deps` 322,
`inner` 304, `run_task` 557, `persist` 112, `dispatch` 535,
`run_log` 164). Every `crate::session::controller::X` path is
unchanged; cross-module seams are `pub(super)` only.

Report: [`docs/optimization-report-R107-2026-09-18.md`](docs/optimization-report-R107-2026-09-18.md).

Verification: server **403/403** with a byte-identical test list;
`make ci` **7/7**; workspace clippy 0 warnings.

### Changed (R106 — the 400-line inline-test tranche extracted across six crates)

Twelve production files each interleaving 400–500 lines of
tests with production code, all resolved by pure code motion to
sibling `tests.rs` files: `session/compaction_checkpoint`,
`session/events`, `session/repair`, `session/token_meter`,
`tool/surface`, `tool/sandbox`, `core/error`, `core/registry`,
`provider/retry`, `provider/streaming/anthropic/processor`,
`context/summarizing`, `workflow/plan`.

Inline test blocks in production files are now uniformly
< 400 lines workspace-wide (two remain at 400–401, 22 in the
300–400 band).

Report: [`docs/optimization-report-R106-2026-09-17.md`](docs/optimization-report-R106-2026-09-17.md).

Verification: core **151/151**, tool **215/215**, session
**147/147**, provider **690/690**, context **95/95**, workflow
**64/64** — all test-name lists byte-identical; agent
**247/247**, delegation **66/66**, server **403/403**;
`make ci` **7/7**; workspace clippy 0 warnings.

### Changed (R105 — the R89 test-block sweep extended workspace-wide; workflow runtime split per concern)

The audit that closed R104 inside `synthia-agent` had never run
on the other crates: 66 production files still carried 200+-
line inline test blocks, and the five worst held 3.8k lines of
interleaved tests. All five are resolved:

- `workflow/runtime.rs` (2 319) split into `runtime/`
  (`mod.rs` facade 268, `executor/mod.rs` 548,
  `executor/mcts.rs` 228, `execution.rs` 165) + `tests.rs`.
- `provider/anthropic/provider/transform.rs`,
  `provider/mod.rs`, `delegation/worktree.rs`,
  `context/memory/file.rs`: test blocks moved to sibling
  `tests.rs` files; production untouched.

Exactly one ≥500-line inline test block remains workspace-wide
(`session/compaction_checkpoint.rs`, 501).

Report: [`docs/optimization-report-R105-2026-09-17.md`](docs/optimization-report-R105-2026-09-17.md).

Verification: workflow **64/64**, provider **690/690**,
delegation **66/66**, context **95/95** (+ sqlite **103/103**)
— all test-name sets baseline-identical; `make ci` **7/7**;
workspace clippy 0 warnings.

### Changed (R104 — the agent registry's test block moved out; the R89 sweep of synthia-agent is closed)

`agent/registry.rs` (463 = 178 production + 284 inline tests)
was the last production file in `synthia-agent` holding an
oversized inline test block: the `AgentRegistry` catalog
interleaved with its `StubAgent` fixture and 14 CRUD /
filter-matrix / pagination tests.

Pure code motion: `registry/mod.rs` (178) + `registry/tests.rs`.
The `agent::registry` re-export surface is untouched.

Report: [`docs/optimization-report-R104-2026-09-17.md`](docs/optimization-report-R104-2026-09-17.md).

Verification: synthia-agent lib **247/247** (baseline-identical);
`make ci` **7/7 green**; server **403/403**; delegation **66/66**.
Final crate state after R99–R104: every production file <500
lines except R98's documented `drive.rs`; every inline test block
<200 lines; every module in the same `mod.rs` + `tests.rs` shape.

### Changed (R103 — the events module's two large inline test blocks moved to sibling tests.rs)

The R89 sweep reached `events/`: `event_enum.rs` (516 = 185
production + 331 inline tests) and `system_event.rs` (382 = 155
+ 227) each interleaved one event type with its full test
module — fixtures, wire-tag matrices, and round-trip contracts
— in the same file. `agent_meta.rs` (119 inline) and
`reasons.rs` (125 inline) are well below the threshold and stay
as they are.

Pure code motion in the established `foo/mod.rs` +
`foo/tests.rs` layout: `event_enum/mod.rs` 186 +
`event_enum/tests.rs` 333; `system_event/mod.rs` 156 +
`system_event/tests.rs` 228. The `events::` re-export surface
is untouched.

Report: [`docs/optimization-report-R103-2026-09-17.md`](docs/optimization-report-R103-2026-09-17.md).

Verification: synthia-agent lib **247/247** (baseline-identical;
65/65 in the events module, same as pre-split); `make ci`
**7/7 green**; server **403/403**; delegation **66/66**;
`assemble_from_zero` proof line prints; fmt clean. Every
production file in `synthia-agent/src/` is now under 500 lines
except R98's documented `drive.rs` orchestrator.

### Changed (R102 — the two strategy files' inline test blocks moved to sibling tests.rs)

The R89 "test blocks out of large production files" pattern had
not yet reached `agent/best_of_n.rs` (676 = 346 strategy + 330
tests) and `agent/cot.rs` (596 = 283 + 313). Both held one
single-concern strategy plus 300+ lines of test scaffolding in
the same file.

Pure code motion in the established `foo.rs` + `foo/tests.rs`
layout: `best_of_n.rs` 348 + `best_of_n/tests.rs` 326;
`cot.rs` 285 + `cot/tests.rs` 302. No public surface touched.

Report: [`docs/optimization-report-R102-2026-09-17.md`](docs/optimization-report-R102-2026-09-17.md).

Verification: synthia-agent lib **247/247** (baseline-identical);
`make ci` **7/7 green**; `strategy_swap` proof line prints; fmt +
clippy clean. Every production file in `synthia-agent/src/` is
now single-concern and below 500 lines except R98's documented
`drive.rs` orchestrator.

### Changed (R101 — team.rs split: each composition gets its own module)

`agent/team.rs` (900 lines) held three *independent* compositions
sharing one file: the `BoundAgent`/`FnAgent` boundary, the
`VerificationChain` generate-verify-retry loop, and the
`RoundRobinGroupChat` transcript engine, plus 290 lines of tests.
Unlike the loop splits these are not phases of one orchestrator —
the treatment is per-type modules.

Pure code motion, public paths unchanged. `team.rs` is now a
`team/` directory: `bound.rs` (91), `verification.rs` (159),
`group_chat.rs` (324), `tests.rs` (292), and a 90-line `mod.rs`
that keeps the module docs and re-exports every public name, so
the `agent/mod.rs` re-export block and every consumer
(`agent_teams` example, `cot.rs` doc link, `synthia-delegation`)
resolves unchanged.

Report: [`docs/optimization-report-R101-2026-09-17.md`](docs/optimization-report-R101-2026-09-17.md).

Verification: `make ci` **7/7 green**; synthia-agent lib
**247/247** (baseline-identical); server **403/403**; delegation
**66/66**; `agent_teams` prints its proof line; fmt clean.

### Changed (R100 — builder.rs split: the factory, the compaction wiring, and the tests each get a file)

R99 closed the strategy seam; the audit moved to the largest
remaining multi-concern file, `agent/builder.rs` (991 lines): the
`AgentBuilder` factory, the compaction wiring
(`CompactionEmitters` + `context_manager_for_compaction*` +
`resolve_context_manager`), and 385 lines of tests shared one
file. The compaction half is not builder-private — the server's
run factory calls it directly when hand-assembling agents — so
two different consumers were interleaved in one file.

Pure code motion, public paths unchanged. `builder.rs` is now a
`builder/` directory: `mod.rs` (432 — the struct + setters +
`build()`), `compaction.rs` (212 — emitters, view mirrors, the
public entries, the `pub(super)` decision core), `tests.rs` (378).
The crate-root re-exports (`synthia_agent::CompactionEmitters`,
`context_manager_for_compaction_with_emitters`, …) resolve
unchanged; `resolve_context_manager` widened file-private →
`pub(super)` so the sibling tests keep driving its decision table.

Report: [`docs/optimization-report-R100-2026-09-17.md`](docs/optimization-report-R100-2026-09-17.md).

Verification: `make ci` **7/7 green**; synthia-agent lib
**247/247** (baseline-identical); synthia-server **403/403** (the
moved API's direct consumer); delegation **66/66**;
`assemble_from_zero` + `runtime_agnostic` proof lines print;
fmt clean.

### Changed (R99 — the strategy seam is now one concern per file, like the loop below it)

R97/R98 gave the harness's dispatch seam and main-loop body the
"every concern in its own file" property. The follow-up audit
found `agent/strategy.rs` (556 lines) still holding three
concerns side by side: the 22-field `AgentRuntime` a strategy
receives, the `EventSink` it publishes through, and the
`ReasoningStrategy` trait + `from_name` resolver a deployment
configures. A reader with one of those questions had to skip past
the other two's code.

Pure code motion, public paths unchanged. `strategy.rs` is now a
`strategy/` directory:

| File | Concern |
|---|---|
| `mod.rs` (157) | seam docs + `ReasoningStrategy` trait + `KNOWN_STRATEGY_NAMES` + `from_name` |
| `runtime.rs` (172) | `AgentRuntime` + `model_config`/`system_prompt` + `Debug` + `default_for_test` |
| `sink.rs` (92) | `EventSink` + the publish primitives + `Debug` |
| `tests.rs` (205) | seam-level tests |

Every existing path still resolves —
`agent::strategy::{AgentRuntime, EventSink, ReasoningStrategy,
KNOWN_STRATEGY_NAMES, from_name}` and the crate-root re-exports
were never touched; `default_for_test` stays `pub(crate)` +
`#[cfg(test)]` so `cot.rs` / `best_of_n.rs` compile unchanged.

Report: [`docs/optimization-report-R99-2026-09-17.md`](docs/optimization-report-R99-2026-09-17.md).

Verification: `make ci` **7/7 green**; synthia-agent lib tests
**247/247** (baseline-identical, verified via `git stash`
round-trip); synthia-delegation **66/66**; `strategy_swap` and
`runtime_agnostic` examples both print their proof lines;
complexity scan zero violations; `cargo +nightly fmt --all`
clean.

### Changed (R98 — the harness's MVP main loop body is now 26 lines, not 215)

R97 closed the dispatch seam into four focused sibling
modules. The follow-up audit turned to the main loop
body itself: `ReActLoop::drive()` in
`synthia-agent/agent/re_act/loop_/mod.rs` was 215 lines
of inline state machine — the "MVP harness, simple明了,
harness自身也要分好层次和布局" property that R88's plugin
split established at the *crate* level had not yet been
applied at the *function* level. The method held six
concerns in one body (session-start / OnAgentStart fan-out
/ setup / request-header stamp / the 5-step iteration /
post-loop finalize). R98's rule: every named phase lives
in its own helper; the orchestrator reads as the step
list its doc promises.

`loop_/mod.rs`: **559 → 313 lines**. Now holds only:

- The `ReActLoop` struct + `from_runtime` constructor.
- The `SampleOutcome` struct + `has_tool_calls` impl
  (moved here from `steps` because both `drive` and
  `steps` produce/consume it; keeping the type in
  `mod.rs` avoids forcing one to import from the other).
- The `ToolProjectionMemo` struct.
- A one-line `drive()` façade that delegates to
  `drive::drive(self, input).await`.
- The delegation surface — 15 one-line methods, each
  routing to the focused sibling that owns the concern.
  No control flow, no state.

New `loop_/drive.rs` (512 lines). The `drive()`
orchestrator + every named phase in its own helper:

| Phase helper | What it does |
|---|---|
| `drive_setup` | `SessionStarted` event + `hooks_on_agent_start` fan-out + prepare messages + apply context |
| `stamp_request_header` | R6-A typed event (model id + tool-list hash) for replay determinism |
| `drive_one_iteration` | One pass of the 5-step loop; returns `IterationOutcome` |
| `drive_finalize` | Post-loop max-iter warning + `finalize` + `hooks_on_agent_end` fan-out |
| `pre_sample_seams` | Drain steering, snapshot runtime context, inject hints, emit progress + step-start |
| `handle_final_answer` | Follow-ups poll: empty → `Completed`; non-empty → inject + apply_context + `Continue` |
| `handle_tool_calls` | Length-stop fail-batch or cancel-check or `execute_tools` |
| `hooks_on_agent_start` / `hooks_on_agent_end` | Fan out lifecycle hooks; failures are warning events |
| `cancelled` | Cancel-aware warning emission |

The orchestrator body is now 26 lines and reads as the
four-step MVP harness list the architecture document
advertises (setup → header → loop → finalize).

Two small types encode the split's control flow in
the type system:

- **`IterationOutcome`** — `Continue` / `Completed` /
  `Failed(SessionEndReason)` / `EarlyReturn(AgentOutput)`.
  The four cases the orchestrator's match dispatches on.
  `EarlyReturn` is distinct from `Failed` because the
  cancelled-before-tool branch already runs `finalize`
  + the agent-end hooks and the orchestrator must skip
  the post-loop finalize step entirely.
- **`IterationDispatch`** — per-iteration bookkeeping:
  `outcome` (the orchestrator's signal), `truncated`
  (close-phase typed-event label), `context_pruned`
  (whether the dispatch handler already called
  `apply_context` inline — the follow-up branch does;
  the close phase skips its own prune when this is
  true, preserving the test's "re-budget before the
  next sample" invariant).

Behaviour byte-identical. The previously-passing
`follow_up_revival_rebudgets_before_the_next_sample`
test continues to pass (256/256 synthia-agent lib
tests); it explicitly checks the two-`apply_context`
behaviour the split preserves via
`!dispatch.context_pruned && !Completed`.

Post-landing complexity audit found two regressions
(`drive_one_iteration` at 28/20, `handle_tool_calls` at
25/20, both above AGENTS.md §3.5's 20 threshold). Two
further extractions in the same named-phase shape
brought both functions under threshold: `drive_one_iteration`
split into [`iteration_start_setup`] / [`sample_or_fail`]
/ [`commit_assistant_and_step_end`] / [`close_iteration`]
(orchestrator is now a flat 5-line pipeline); `handle_tool_calls`
split into [`dispatch_truncated_batch`] /
[`dispatch_cancelled_before_tool`] /
[`dispatch_execute_tools`] (orchestrator is now a
flat 3-arm dispatch). Final clippy complexity scan
reports zero violations across the workspace.
Report: [`docs/optimization-report-R98-2026-09-17.md`](docs/optimization-report-R98-2026-09-17.md).

Verification: `make ci` **7/7 green**; `make test-unit`
**2453/2453** (no count change — pure code motion);
`cargo clippy --workspace --all-targets --all-features
--tests --all -- -W clippy::cognitive_complexity` reports
**zero violations** (R96 baseline holds); 256/256
synthia-agent lib tests; 66/66 synthia-delegation lib
tests; `cargo +nightly fmt --all` clean.


### Changed (R97 — the harness dispatch seam is now a façade, not a 900-line file)

R92 closed the harness's per-iteration loop helpers. The
follow-up audit turned to the **layout** of `synthia-agent`
where R88's plugin split left a single 900-line module
owning four distinct concerns. The split had been *named*
(each helper carried a phase comment) but the four
orchestrators + seam controls + bucket implementation +
commit helpers all shared one file. R97's rule: "every
concern in its own file, every orchestrator reads as the
step list its doc promises" now holds inside the file
boundary too.

`synthia-agent/agent/re_act/loop_/dispatch.rs`: **900 → 167
lines**. Now holds only the three pieces the dispatch seam
as a whole *shares*: `WireToolResult` (the wire payload
produced by `bucket` and `steps`, consumed by `commit`),
`lookup_execution_mode` + `tool_definitions` (the read-side
memoised projection), and `append_tool_result_hints` +
`notify_error_hooks` (the steering seams that span the
dispatch step but don't belong to any one bucket).

New single-concern sibling modules under
`synthia-agent/agent/re_act/loop_/`:

- **`bucket.rs` (339 lines)** — the `execute_tools`
  orchestrator + Parallel / Sequential bucketing, semaphore
  concurrency, the sequential-abort `SequentialStep` state
  machine, and the four-phase `commit_all_results`. The
  orchestrator body reads as the 5-step list its doc
  promises.
- **`seams.rs` (295 lines)** — the `execute_tool_inner`
  orchestrator + the four steering seams (restriction,
  hook veto, guard pipeline, observation) + the `Verdict<T>`
  enum shared with `route`. The orchestrator body reads
  as the 4-seam list.
- **`route.rs` (185 lines)** — the `dispatch_tool_call`
  orchestrator + the three named routing steps
  (`try_interceptor` / `lookup_tool_entry` /
  `drain_tool_stream`). The orchestrator body reads as the
  3-step list.
- **`commit.rs` (54 lines)** — `commit_tool_result` +
  `last_assistant_text`. The history-commit on the
  tool-result side and the final-message projection on the
  finalize side, kept together as "the live-history
  writeback layer".

The `ReActLoop` struct keeps the same delegation surface,
but every delegation now goes to the focused sibling
module that owns the concern:
`bucket::execute_tools` / `route::dispatch_tool_call` /
`commit::commit_tool_result` / `seams::execute_tool_inner`
(the last is called directly from `bucket.rs`; the old
`ReActLoop::execute_tool_inner` indirection that used to
forward `dispatch::execute_tool_inner` was dead and is
deleted).

`Verdict<T>` visibility widened from `pub(super)` to
`pub(in crate::agent::re_act)` — the type is a private
protocol of the dispatch seam (consumed by `route`'s
`lookup_tool_entry` and `seams`'s `apply_guard_pipeline`),
never reaching outside the `re_act` sub-tree.

Report: [`docs/optimization-report-R97-2026-09-17.md`](docs/optimization-report-R97-2026-09-17.md).

Verification: `make ci` **7/7 green**; `make test-unit`
**2453/2453** (no count change — pure code motion);
`cargo clippy --workspace --all-targets --all-features
--tests --all -- -W clippy::cognitive_complexity` reports
**zero violations** (R96 baseline holds); 256/256
synthia-agent lib tests; 66/66 synthia-delegation lib
tests (the downstream that exercises the dispatch path
through real tool calls); `cargo +nightly fmt --all`
clean.

# Changelog

All notable changes to this project will be documented in this file.


### Changed (R96 — every test-code complexity violation cleared)

R95 closed the lib side. Four `#[cfg(test)]` functions still
over the 20 threshold:

- `synthia-core::error::tests::helpers_produce_distinct_variants`
  (was 73/20) — 35 `assert!(matches!(...))` lines, one per Error
  helper. Replaced with an in-module `assert_variant!` macro and
  8 per-domain tests (lookup_and_validation, provider_and_protocol,
  stream_variants, auth_and_config, lifecycle_and_memory,
  model_and_rate_limit, edit_and_retry, from_std_io). 144
  synthia-core tests pass.
- `synthia-server::config::tests::test_server_config_full_round_trip`
  (was 27/20) — 110-line build-and-assert body. Split into a
  11-line orchestrator + `build_sample_server_config()` + 7
  per-domain assert helpers (`assert_top_level_round_trips`,
  `_mcp_round_trips` [R21], `_retrieval_round_trips` [R22],
  `_tools_surface_round_trips` [R34], `_auth_cors_round_trips`,
  `_providers_agents_round_trips`, `_operations_round_trips` [R29]).
  All 30 field-level assertions preserved byte-identical.
- `synthia-provider::streaming::anthropic::tests::two_parallel_tool_use_blocks_emit_independent_streams`
  (was 27/20) and
  `tool_use_emits_start_delta_end_with_is_done` (was 21/20) —
  extracted `build_parallel_tool_use_events` +
  `build_single_tool_use_events` + `process_each_event` + 8
  per-shape assertion helpers. 46 anthropic tests pass.
- `synthia-session::log_surface::compaction_op` (was 23/20;
  splitLogSurfaceTest went one layer deeper than the audit
  named, since the post-R95 lib function still hit the 20
  threshold) — split into `compaction_op_value` +
  `parse_replace_op`. 147 synthia-session tests pass.

Verification: `make ci` 7/7 green; `make test-unit`
**2453/2453** (+7 from the per-domain tests added in
`synthia-core/error.rs`); `cargo clippy --workspace
--all-targets --all-features --tests --all -- -W
clippy::cognitive_complexity` reports **zero violations,

### Changed (R95 — every lib complexity violation cleared across the workspace)

R94 closed the session-controller trio. The fresh workspace
scan surfaced nine lib functions still over the 20 threshold;
every one of them now has its own set of single-concern
helpers, and the workspace has **zero `cognitive_complexity`
warnings outside `#[cfg(test)]` modules** (the test violations
in `provider/anthropic/tests.rs` and `session/log_surface.rs`
are inside the clippy test allow-list and stay as-is).

- `provider::streaming::anthropic::StreamProcessor::process_event`
  (was 39/20) — one private fn per SSE event category
  (`handle_content_block_start` / `_delta` / `_stop`,
  `handle_message_delta`, `handle_message_stop`); the heaviest
  arm (`message_stop`) further splits into `drain_orphan_buffers`
  + `flush_think_extractor` + `build_sampling_result`. The
  `drained.clear()` quirk (drained ThinkExtractor chunks are
  folded into `self.text` / `self.reasoning` but never pushed
  onto the outgoing `chunks` vector) is preserved verbatim.
  46 streaming tests pass.
- `server::state::boot::build_configured_retriever` (was 26/20)
  — the per-file read+chunk loop folds into `index_path`
  (read-or-warn-and-skip). 403 synthia-server tests pass.
- `server::state::boot::apply_tools_config` (was 25/20) — the
  active-group activation step folds into `activate_groups`
  (mutates `active_groups` + `skipped` + logs). The existing
  `resolve_group_declarations` and `apply_surface_list`
  helpers stay.
- `server::state::boot::register_configured_mcp_servers`
  (was 21/20) — the per-health log pass folds into
  `log_tick_health`.
- `server::routes::chat::build_parts` (was 24/20) — six private
  per-kind helpers (`text_part`, `image_part`,
  `attachment_ref_part`, `audio_part`, `file_part`, `url_part`)
  replace the per-attachment match body.
- `session::log_surface::classify_compaction` (was 25/20) —
  `compaction_op` (validates `surface_op` + Replace guard) and
  `compaction_payload` (the data-shape match) carry the two
  phases. 147 synthia-session tests pass.
- `provider::streaming::idle_watchdog::pump_sse` (was 23/20) —
  four helpers (`cancelled`, `append_chunk`, `emit_tail`,
  `abort_after_grace`) replace the inline cancel check, the
  select-arm body, and the trailing-buffer emission. (The
  first subagent pass extracted `emit_complete_lines` and
  `drain_within_grace` but left the orchestrator at 21/20;
  the second pass extracted the four above to drop it below.)
  9 idle_watchdog tests pass; 690 provider lib tests pass.
- `skill::discovery::discover_skills` (was 23/20) —
  `discover_project_skills` + `discover_user_skills` +
  `collect_skills` (parse + intra-batch dedup + warn) +
  `merge_project_then_user` (project-wins-on-collision via
  `HashSet<String>` of project names). 56 synthia-skill
  tests pass.
- `steering::hook::run_hook_gated` (was 22/20) — `hook_span`
  + `run_hook_inner` (panic-isolated execution) + `gate_rejection`
  (gated branch builder) carry the three phases. 72
  synthia-steering lib tests pass.

Verification: `make ci` 7/7 green; `make test-unit`
**2446/2446**; `cargo clippy --workspace --all-targets
--all-features --tests --all -- -W clippy::cognitive_complexity`
reports zero non-test violations.

### Changed (R94 — `ControllerInner` triple split; the session controller is fully under the complexity threshold)

R93 closed `run_controller_loop` (144→under) and named
the `ControllerInner` trio as the next target: `finalise_shutdown`
(28/20), `maybe_start_run` (29/20), `persist_and_broadcast`
(33/20). All three split into named single-concern helpers;
**no function in `synthia-server/session/controller.rs`
exceeds the 20 threshold anymore**.

- **`finalise_shutdown` (was 28/20)** — the lifecycle event
  build + match-serde + sink-append + sink-close sequence
  folds into `append_lifecycle_event` + `close_sink` (two
  helpers, the latter just does the close). The
  `shutdown_finalised` AtomicBool swap stays at the top so a
  shutdown-then-idle race cannot double-append.
- **`maybe_start_run` (was 29/20)** — the state-gate +
  pending-input check stays inline; the Running transition,
  pending-count log, R11 snapshot publish, and cancel-token
  install fold into `mark_running(multimodal_active)`.
  `is_startable()` (the Idle/Cancelled state check) is
  pulled into its own private fn so the function signature
  reads as a flat pipeline. (The initial refactor that
  left a duplicate `R11` publish + duplicate cancel-token
  install in `maybe_start_run` after the extraction was
  caught by `snapshot_bus_publishes_running_then_completing`:
  rx would receive Running twice, never Completing — a
  second-pass cleanup removed the duplicates; tests are
  byte-identical to before.)
- **`persist_and_broadcast` (was 33/20)** — the
  serialize + decode + durable-append + broadcast block
  folds into `encode_event` (single serialize pass,
  one decode), `classify_system_kind` (the system-event
  match), `append_if_durable` (the if-event.is_durable()
  guard), and `broadcast_event` (the SSE/WebSocket fan-out
  + trailing log). The orchestrator body is now the
  one-sentence pipeline the doc comment promised.

Result: `clippy --workspace -W cognitive_complexity`
reports **no violations in `synthia-server/session/
controller.rs`**. Remaining workspace hits: `provider`
streaming processor (39/20), `session/log_surface.rs`
(25/20), `synthia-skill/discovery.rs` (23/20),
`synthia-steering/hook.rs` (22/20), scattered 21-26s in
`chat.rs`, `state/boot.rs`, `idle_watchdog.rs` — and
`run_controller_loop` is now also at risk of re-entering
the list if the surrounding modules grow.

Verification: `make ci` 7/7 green; `make test-unit`
**2446/2446** (`synthia-server` 403, including the
snapshot-bus test that surfaced the duplicate-publish bug

### Changed (R93 — `run_controller_loop` split; the workspace's 144/20 monster eliminated)

R92's report named `run_controller_loop`
(`synthia-server/session/controller.rs`) the next target:
at **144/20** it was 6× over the threshold and the single
largest function left in the workspace. The 373-line body
was a `tokio::select!` with four arms, the op arm holding a
seven-variant match with inline bodies. Now:

- **Op dispatch** — `handle_received_op` owns the
  seven-variant match; each variant's body is its own named
  handler (`handle_prompt_op`, `handle_prompt_multi_op`,
  `handle_rerun_op`, `handle_feedback_op`,
  `handle_steer_op`, `handle_cancel_op`,
  `handle_shutdown_op`). `Prompt` and `Steer` share
  `push_text_input`; `Rerun` and `Cancel` share
  `cancel_inflight_run`; the four run-starting ops share
  `start_run_if_idle`.
- **Select arm bodies** — `on_run_completed` (panic surfacing
  + the restart-with-parked-multimodal gate) and
  `handle_idle_timeout` + `join_run` (the exit paths).
- **Guard futures** — `wait_for_run` / `wait_idle_timeout`
  hoisted out of inline `async` blocks (also converting the
  `Some(ref mut h)` pattern to match ergonomics:
  `match &mut run_handle { Some(h) => … }`).
- **Cancel snapshot** — `publish_cancelled_snapshot` +
  `drain_pending_inputs` extracted from `handle_cancel_op`
  (which itself dropped 26 → under threshold).

Result: `run_controller_loop` reads as the loop skeleton —
select, three one-line arm bodies, exit bookkeeping — and
neither it nor any new helper exceeds the threshold. The
three remaining controller hits (812 / 858 / 1529, all
pre-existing `ControllerInner` methods untouched this
round) are the R94 candidate.

Verification: `make ci` 7/7 green; `make test-unit`
**2446/2446** (`synthia-server` 403, unchanged — the
lifecycle / cancel / rerun / prompt_multi test suites pin
the loop's behaviour byte-for-byte); `cargo +nightly fmt
--all` clean.

### Changed (R92 — the harness loop's dispatch split; every synthia-agent function under the complexity threshold)

The R91 report pointed at the harness core itself: the
`re_act/loop_/` files held the workspace's worst production
cognitive-complexity scores (`dispatch.rs` 62/20, 42/20,
38/20; `steps.rs` 33/20, 24/20). All five split into named
single-concern helpers; **no function in `synthia-agent`
exceeds the 20 threshold anymore**.

- **`execute_tools` (was 62/20)** — the four numbered
  phases the doc comments already promised are now four
  helpers: `bucket_by_execution_mode` (+ concurrency clamp
  + the bucketed `info!`), `run_parallel_bucket`
  (semaphore + `join_all`), `run_sequential_bucket`
  (loop control via a `SequentialStep` enum from
  `run_one_sequential`: Continue / Cancelled / Aborted),
  and `commit_all_results` (synthetic-error fill, steering
  output-transform, hint append, wire commit).
- **`execute_tool_inner` (was 42/20)** — the steering
  seams are now named functions: `pre_execution_veto`
  (R58 restriction + `BeforeToolExecute` votes via
  `hook_before_verdict`), `apply_guard_pipeline` (seam 2,
  returning a private `Verdict<T>` enum — `Allowed` /
  `Denied` — rather than `Result<_, ToolOutput>`, which
  trips `result_large_err`), and `notify_after_hooks`
  (seam 3).
- **`dispatch_tool_call` (was 38/20)** — three steps:
  `try_interceptor` (claimed synthetic tools),
  `lookup_tool_entry` (touched-file note + registry
  resolution, also `Verdict`), `drain_tool_stream`
  (generic over the stream, forwarding Progress chunks as
  `ToolProgress` events).
- **`sample_once` (was 33/20)** — `notify_provider_start_hooks`,
  `stream_completion` (cancel-aware wiring; returns
  `(resp, outcome, elapsed)` — it owns the chunk state's
  lifetime), and `observe_provider_success` (usage fold +
  typed event + tracker + `OnProviderEnd`).
- **`prepare` (was 24/20)** — the manifest-resolution
  match + assembly + its debug log folded into
  `assemble_system_prompt`.

`notify_error_hooks` moved from a dispatch-local closure to
a `loop_`-shared helper (`pub(in …loop_)`) — the provider
error path in `steps.rs` reuses the exact fan-out the hook
veto path uses.

Result: `clippy --workspace -W cognitive_complexity` reports
zero violations in `synthia-agent`. Remaining workspace hits
are server-app code (`run_controller_loop` 144/20 — R93
candidate), `synthia-provider` streaming (39/20), and test
functions.

Verification: `make ci` 7/7 green; `make test-unit`
**2446/2446** (`synthia-agent` 256, unchanged — behaviour
byte-identical, only shape moved); `cargo +nightly fmt
--all` clean.

### Changed (R91 — `with_server_config` split into named phases; cognitive complexity 26→under threshold)

The R90 report named `AppState::with_server_config`'s
26/20 cognitive complexity as the next target. The
287-line boot orchestrator interleaved eight
near-identical `cfg?` chains with a 50-line default-agent
composition and a 3-arm YAML-bridge match; reading it
meant holding all three shapes at once. Three named
extractions flatten it:

- **`load_per_section_config`** — the eight per-section
  reads (`auth`, `cors`, `operations`, `default_agent`,
  `max_steps`, `compaction`, `allowed/denied_tools`,
  `strategy`) now live in one private fn returning a small
  `PerSectionConfig` struct. Each read follows the same
  `cfg? → agents.get(name)? → field → fallback` shape;
  grouping them means the reader scans one list and one
  shape, not eight interleaved chains.
- **`build_default_agent`** — the ~50-line block threading
  provider / tool registry / surface policy / strategy /
  restriction through the `ReActAgent::with_*` builder
  chain, plus the `try_read()` registry snapshot and four
  `if let Some` installers. In its own private fn the
  orchestrator reads "wire → assemble".
- **`bridged_workspace_config` + `legacy_workspace_config`**
  — `load_workspace_config`'s 3-arm match (Ok / Ok(None) /
  Err) split into a one-attempt loader that carries its
  failure reason as a `String`, plus the legacy
  `<workspace>/.agents/config.toml` fallback as its own
  named fn. The orchestrator is now `match { Ok, Err(reason)
  → log + fallback }`.

Result: `clippy -W cognitive_complexity` no longer flags
**any** function in `app_state.rs` (was 26/20
`with_server_config` + 23/20 `load_workspace_config`).

Verification: `make ci` 7/7 green; `make test-unit`
2446/2446 passed (`synthia-server` 403, unchanged);
`make examples` exit 0 (CONSUMER-PROOF: OK, MVP-OK).
`cargo build -p synthia-server --all-targets
--all-features --tests` zero warnings.

### Changed (R90 — state/ split: `app_state.rs` test block + boot helpers)

Two surgical cleanups in the server state module, both
motivated by the user's "harness 自身也要分好层次和布局"
directive: the production file had grown to 2639 lines,
1186 of which were tests and 553 of which were boot
helpers that the `AppState` impl only calls.

- **Test block split** (`crates/synthia-server/src/state/tests/`):
  the 1186-line `#[cfg(test)] mod tests { ... }` block in
  `app_state.rs` now lives in nine focused submodules,
  each named after the surface it pins:

  | Submodule              | Tests |
  |---|---|
  | `resolve_agent_name`   | 5 — the three-tier fallback ladder every dispatch flows through |
  | `build_prompt_context` | 7 — the `<workspace>/.agents/skills/` walker + agent-registry snapshotter |
  | `for_test`             | 1 — `AppState::for_test`'s tool registry wiring |
  | `unapplied_keys`       | 2 — the R55 audit classifying inert `agents.<name>` keys |
  | `resolve_server`       | 2 — R53 `--config <path>` named-file reader |
  | `load_helpers`         | 3 — `load_default_compaction` (R16) + `load_default_tool_restriction` (R58) |
  | `load_strategy`        | 2 — `load_default_strategy` (R50) |
  | `boot_surface`         | 4 — MCP server boot (R21/R30) + retrieval index (R22) |
  | `apply_tools`          | 3 — `[tools]` boot application (R34) |

  Shared fixtures (`StubAgent`, `ScopedHome`, the
  `$HOME`-mutex swap, the `write_skill_md` helper, the
  canonical empty descriptor) live in `tests/support.rs`
  using `std::sync::LazyLock` (R90 also adopts the stdlib
  `LazyLock` per AGENTS.md's std-only convention; no
  `OnceLock` accessor functions remain).

  Behaviour and assertion text byte-identical to the old
  monolithic block; the production file is now
  `app_state.rs` (954 lines, only the `AppState` struct +
  impls + `UsageMetrics` + `AppliedToolSurface`) and the
  test surface is 1186 lines spread across 9 focused
  files plus a shared `support.rs`.

- **Boot helpers extracted** (`crates/synthia-server/src/state/boot.rs`):
  the 553-line tail of `app_state.rs` (every free function
  the boot calls: `load_default_max_iterations`,
  `load_default_compaction`, `load_default_tool_restriction`,
  `load_default_strategy`, `unapplied_agent_keys`,
  `warn_unapplied_agent_keys`, `build_configured_retriever`,
  `register_configured_mcp_servers`, `apply_tools_config`,
  `build_prompt_context`, plus the two private helpers
  `resolve_group_declarations` / `apply_surface_list`)
  moved to `boot.rs`. They are `pub(crate)` — internal
  steps of the boot the public surface never names — and
  re-exported through `state/mod.rs` only for the
  `tests/` submodules to import via `crate::state::xxx`.

  `AppliedToolSurface` (the public record of what
  `[tools]` did) stays in `app_state.rs` because the
  `AppState` struct holds one — moving it would split a
  single type across files. `plugin_tool_registry()`
  (the deployment's default brick-by-brick tool
  composition) also stays in `app_state.rs` because it
  is consumed by `AppState::with_server_config` and is
  what `state/mod.rs` exports.

  The new file layout (three modules under `state/`):

  | Module     | Lines | Responsibility |
  |---|---|---|
  | `app_state` |  954 | `AppState` struct + impls, `UsageMetrics`, `AppliedToolSurface`, `plugin_tool_registry` |
  | `boot`      |  541 | one free function per boot step |
  | `tests`     | 1191 | 9 focused submodules + a `support.rs` |

Verification: `make ci` (fmt + clippy + doc + dep
invariants + clock + claim) **7/7 green**; `make test-unit`
**2446 passed, 0 failed** across the workspace
(synthia-server 403 — same total as before the split);
`make lint-rust` and `cargo build -p synthia-server
--all-targets --all-features --tests` zero warnings.

Four large production files used to bundle a giant
`#[cfg(test)] mod tests { ... }` block at the bottom. The
test bodies were pushing each file past the threshold where
the public API was still scannable at a glance. Each
test block is now a focused `tests/` directory mirroring
the structure already used elsewhere in the project, so the
production file stays small and one concern lives in one
file:

| File (before) | Production lines (before) | Test lines (before) | New layout |
|---|---|---|---|
| `crates/synthia-agent/src/prompt/mod.rs`        | 378 | 543 | `prompt/tests/{assemble,identity,skills_agents,runtime_context,support}.rs` |
| `crates/synthia-agent/src/agent/re_act/tests.rs` | n/a (single 4511-line test file) | 4511 | `re_act/tests/{agent_smoke,prompt_integration,end_to_end_runtime,loop_lifecycle,multi_agent_panel,parallel_dispatch,tool_projection_memo,tool_restriction,steering_wiring,run_inbox,support}.rs` |
| `crates/synthia-tool/src/registry.rs`           | 1367 | 1929 | `registry/tests/{exposure,session_scope,registration,snapshot,dispatch,mutations,argument_validation,support}.rs` |
| `crates/synthia-server/src/session/controller.rs` | 2157 | 2339 | `controller/tests/{lifecycle,cancel,event_emission,history,run_log,prompt_multi,rerun,run_config,context_manager,truncate,support}.rs` |

Behaviour, test names, and assertion text are byte-identical
to the previous monolithic blocks. The only changes are
structural: each `tests/` directory declares its submodules
in a small `mod.rs`, a `support.rs` carries the fixtures
shared across more than one submodule, and each
submodule's first lines import the fixtures it needs.

Verification: `make ci` (fmt + clippy + dep invariants +
clock + claim) passes, `make test-unit` reports
**2446 passed, 0 failed** across the workspace (synthia-agent
256, synthia-tool 215, synthia-server 403, plus the other
20 crates — same totals as before the split).

### Changed (R89 — harness-construction dedup, typed-event seam extracted, dead tool API removed)

Three small, surgical cleanups that keep the
paradigm-only / plugin-only line drawn in R88 sharp and
trim a few stray pub-alls the previous rounds left in
place. No behaviour change, no public-API removal that any
documented caller exercises.

- **`ReActAgent` constructor dedup**:
  `re_act/agent.rs` used to inline the full
  `AgentDescriptor { name, display_name, description, kind,
  version, instructions, capabilities, tools, model_hint,
  handoffs, handoff_hint, output_schema, owner, domain,
  persona, max_iterations }` literal once in `with_options`
  and copy every other "off" default (12 fields) into
  `with_descriptor` by hand. The literal now lives in a
  single `default_descriptor(system_prompt)` free
  function; `with_options` and `with_prompt_context` both
  call it and forward to `with_descriptor`, which is the
  one place that owns the default state vector. Net
  effect: the four constructors each fit in a screen and
  there is one source of truth for "the default Synthia
  descriptor". Behaviour, descriptor bytes, and
  round-tripped `RuntimeContext` snapshots are
  byte-identical.
- **`loop_/events.rs` extracted from `loop_/mod.rs`**:
  the four typed-event wrappers (`emit_iteration_start` /
  `emit_iteration_end` / `emit_step_start` /
  `emit_step_end`), the underlying `emit_typed` helper,
  and the `StepAction` enum the step-boundary events
  carry now live in a small sibling module. `loop_/mod.rs`
  shrinks to the state machine itself (`ReActLoop`
  struct, `SampleOutcome`, `ToolProjectionMemo`, the
  `drive` method, the per-iteration helpers + the
  `from_runtime` constructor) and the layout table at
  the top of the file lists `events` as another
  submodule. This is the same "one concern per file"
  split R89 already applied to the test blocks.
- **Dead tool API removed**:
  - `synthia-tool-read::ReadTool::mark_read` and the
    `read_history: Mutex<Vec<String>>` field it pushed
    into had no reader anywhere in the workspace and
    existed only "for future edit-after-read
    enforcement" — speculative API surface that AGENTS.md
    §3.2 explicitly bans. Both removed; `ReadTool` is
    now a unit struct, the `parking_lot` dependency is
    no longer needed, and `Cargo.toml` drops it.
  - `synthia-tool-web::WebFetchTool::truncate_response_body`
    was `pub` but only used by `Tool::execute` inside
    the same crate. Re-classified `pub(crate)` so the
    external surface only carries the things external
    callers actually need (`new`, `with_allowed_hosts`,
    `is_url_allowed` for introspection).

Verification: `make lint-rust` and `cargo test -p
synthia-agent -p synthia-tool-read -p synthia-tool-write
-p synthia-tool-shell -p synthia-tool-todo -p
synthia-tool-web --lib` all pass; the per-crate test
totals (256 / 14 / 10 / 17 / 18 / 14) are unchanged.



### Changed (plugin split — the harness is the MVP; every tool and the task tool are plugins)

The framework's core is now exactly the harness. Everything that
was welded into `synthia-tool` or `synthia-agent` but is not the
loop itself became a plugin crate the consumer composes:

- **`synthia-tool` is the paradigm**: `Tool`, `ToolEntry`,
  `ToolRegistry`, the exposure projection, output/truncation, the
  read-side registries, plus the two cross-cutting policies every
  implementation shares — `sandbox` (process confinement) and the
  new `workspace` module (path confinement, previously a private
  helper inside `read`). It ships **no agent-facing tool set**; the
  only `Tool` impls left in it are the two synthetic contracts the
  harness needs (`__get_full_output`, `structured_output`) and the
  registry's `ToolEntry::dynamic` passthrough.
- **Five tool plugins, one tool each**: `synthia-tool-read`,
  `synthia-tool-write`, `synthia-tool-shell`, `synthia-tool-todo`,
  `synthia-tool-web`. Each depends only on `synthia-tool`, carries
  its own tests and one runnable example, and is the only thing
  that pulls what that tool needs (`synthia-tool-web` owns
  `reqwest`; the other four are offline). `build_default_tool_registry`
  is gone — a consumer registers the bricks it wants with
  `registry.register_entry(ToolEntry::new(Arc::new(ReadTool::new())))`.
- **The `task` tool left the agent crate**: the new
  `synthia-delegation` plugin owns the whole multi-agent family —
  `TaskSpec` + `task_tool_definition`, the verification `gate`, git
  `worktree` isolation, the `@agent:` `router` / `route_rules`, the
  `mention_clone` path, and the two-lane `SubagentPool`.
- **A generic interceptor seam replaced the hard-coded branch**:
  `synthia_agent::ToolInterceptor` (`definitions` / `claims` /
  `execute`, handed an `InterceptorCall` carrying the run's cancel
  token, depth and event emitter) plus
  `ReActAgent::with_interceptor` / `AgentBuilder::interceptor`. The
  loop advertises interceptor definitions next to the registry's
  (same restriction filtering) and dispatches claimed calls before
  registry lookup — so with no plugin installed there is no `task`
  tool, no delegation code, and no multi-agent coupling in the
  harness. `TaskDelegator` is that trait's first implementation.
- **Facade**: `tool-read` / `tool-write` / `tool-shell` /
  `tool-todo` / `tool-web` / `delegation` are features and modules of
  their own (`synthia::tool_read::ReadTool`, …), all in the default
  set; `tool` now means the paradigm. `prelude` exports `ToolEntry`
  in place of the removed `build_default_tool_registry`.
- **Gates and docs**: `make check-mvp-deps` additionally asserts the
  four offline tool crates pull no HTTP/OTel/DB dependency;
  MINIMAL.md, SEAMS.md, README.md, AGENTS.md §1/§3.7, and the example
  index describe the plugin composition.
- **Server**: `synthia-server` composes the plugin tools
  (`plugin_tool_registry()`) and installs `TaskDelegator` through
  `with_interceptor` at all three wiring sites (default member,
  `/agents` route, session factory).

### Fixed (R124 — the four deferred harness gaps)

The four gaps the harness audit recorded as open are closed. Each was
reproduced first; the two behavioural ones were shown to fail against
the pre-fix code before landing.

- **Reasoning now reaches the conversation history.** `stream.rs` folded
  chunks itself and carried no reasoning at all, so `commit_assistant` —
  which rebuilt the assistant message from `assistant_text` +
  `tool_uses` — dropped every thinking block. `synthia-provider` states
  the contract as the *agent layer's* job in four places
  (`anthropic/types.rs`: the `signature` is "Required to preserve
  reasoning continuity across turns"; `types/stream_chunk.rs`: it is
  "propagated so the agent can preserve cross-turn reasoning
  continuity") and its Anthropic adapter already maps
  `ContentPart::Reasoning` → `ThinkingBlock { thinking, signature }` —
  the harness was the missing half, and Anthropic interleaved thinking
  with tool use requires echoing those blocks back. A probe reported
  `request 2 has reasoning part = false, signature preserved = false`
  before the fix; `reasoning_reaches_the_next_request` pins it now.
  The ordered parts travel through `SampleOutcome` rather than a
  `reasoning: String` sibling, because order is the point: the provider's
  canonical fixture is `[Reasoning, Text]`, reasoning first.

  Committing them verbatim would have been wrong for a different reason:
  the assembler emits one part per **streamed delta**, so a three-delta
  thinking block would commit as three `Reasoning` parts and an answer as
  N `Text` parts. History records *blocks*, so `commit_assistant` now
  coalesces adjacent same-kind runs — one thinking block per block (which
  is what Anthropic requires per prior assistant turn), one text part, and
  a `Text` after a `ToolUse` left separate because it is a different
  block. This also restores the single joined `Text` part history used to
  carry before the rewrite — on the *every-answer* path, so without it the
  fix would have introduced a wire-shape regression: N consecutive `text`
  blocks where a single joined part used to go.
  `streamed_text_deltas_commit_as_one_block` pins that half (removing the
  `Text` arm now fails it; before, the arm was removable with the whole
  suite green, because the two tests that stream multiple text deltas
  assert only the wire events and never reach a second request).

  Landing that exposed a second break one layer down, in the provider's
  own fold: Anthropic's `signature_delta` arrives *after* the
  `thinking_delta`s that carry the text, so a streamed
  `Content(Reasoning)` chunk cannot know its signature — the processor
  stamps it `None` and the value reaches the assembler only on `IsDone`,
  which `fold_is_done` skipped (streamed reasoning had already set
  `saw_streamed_reasoning`) and `finalize_assistant` then hardcoded to
  `None`. Reasoning would have arrived in history *unsigned*, which
  Anthropic rejects, so the reported gap would have looked fixed while
  still broken. `BlockAssembler` now records the deferred signature and
  stamps it onto the trailing un-signed reasoning run at finalize (one
  block produces several parts: the `content_block_start` text plus one
  per delta). `anthropic_stream_signature_survives_to_the_assembled_parts`
  drives real SSE bytes through the production processor *and* the
  production assembler — no hand-built chunk — and asserts the
  precondition that the streamed chunk is genuinely un-signed, so a
  `None` part cannot pass.
- **`stream.rs` no longer re-implements `BlockAssembler`.** The
  accumulator is now the provider crate's tested fold; the harness keeps
  only the half that is genuinely its own — *when* an assembled part
  becomes an `AgentEvent::Model` — as one publish cursor. That removed
  the drift the duplication had already caused (no reasoning, no JSON
  repair on malformed tool arguments) and deleted
  `extract_tool_uses` / `parse_tool_input`, which existed only to serve
  the second copy.
- **The streaming path retries a transient failure.** `complete`
  retried with per-class backoff; `complete_with_stream` — the only path
  a ReAct iteration takes — handed any `Err` back as session-fatal, so a
  429 or a capacity blip ended the run. Establishment is now wrapped in
  the same `retry_with_classification` budget, and *only*
  establishment: once deltas have reached `on_delta`, re-sending would
  emit them twice and the consumer cannot un-see them. Both adapters
  covered; the boundary is pinned by a test asserting one request after
  a mid-stream abort.

  Wrapping establishment in a bounded budget created a window that did
  not exist before, so the same change closes it: the token is checked
  **inside** the retry closure, before every attempt. Without that check
  a cancelled run — or a disconnected client — would be held in backoff
  for up to the full budget (a sustained 429 is 6 attempts over a 1s→30s
  ramp, ~31s) before the cancellation was noticed, where a single attempt
  used to fail fast. A cancelled run now sends **zero** requests and
  returns `StreamAborted`, on both adapters; that relies on the variant
  classifying as `Permanent` (`max_attempts: 1`, no sleep), which
  `classify_stream_aborted_is_permanent` now pins next to the other
  `Permanent` assertions.
- **A declared-but-unsatisfied output schema is now observable, and the
  prompt asks for it.** `with_output_schema` only *offers* the
  `structured_output` tool — it cannot force the call, and a text-only
  final answer is a normal way to end — so a consumer holding a schema
  could not tell "the model answered in prose" from "the model answered
  in the shape I asked for". Finalize now emits
  `WarningKind::StructuredOutput` when a schema was declared and no
  schema-valid submission was committed. Deliberately **not** fatal:
  prose is a legitimate answer, and forcing a retry would change what
  `Completed` means for every existing agent. The fact is recorded on
  the run's own state rather than recovered from the transcript, because
  history stores a tool result as content + call id and the tool name /
  `is_error` that decide validity exist only on the wire.

  Validating alone would have been hollow: a tool list is a menu, not an
  instruction, so nothing told the model that prose was not an accepted
  ending and the warning would have fired on nearly every
  schema-declaring run. The prompt now carries a `<structured_output>`
  section when the descriptor declares a schema — the one fact the
  native `tools` channel cannot express. The schema *text* stays out of
  the prompt (it is the tool's `parameters()`, already delivered
  natively).

Also in this change: the OpenAI-compatible adapter no longer emits a
`{"type":"reasoning"}` content block. It never defined a request-side
slot for one — real OpenAI rejects an unknown block type, and the
gateways that expose reasoning expose it on the response only — so now
that reasoning actually reaches a message it is dropped at the wire
instead, where the adapter owns the shape. A message left with nothing
transmittable is dropped rather than sent as an empty `content: []`.
This is the deliberate asymmetry with Anthropic, which *requires* the
block back.

**Reachability, stated plainly.** The *signed*-reasoning half is
latent, and the CHANGELOG should not pretend otherwise: nothing in
`synthia-provider/src/anthropic` ever sets a `thinking` / `budget_tokens`
request field and `AnthropicRequest` never reads `extra_body`, so
extended thinking cannot be enabled through this adapter, and only real
extended thinking makes the upstream emit `signature_delta`. The
signature plumbing is therefore defensive correctness for the day that
capability is exposed — and it is now bounded, so it cannot forge a
signature (see the interleaved test).

What *is* reachable is **unsigned** reasoning: OpenAI-compatible
`reasoning_content` gateways and `<think>`-marker-compatible upstreams
both produce `ContentPart::Reasoning` with `signature: None`. For those,
the two adapter changes above are the whole story — the part now reaches
history in-run and is dropped at the wire in both directions rather than
producing an invalid request.

**Residual gap, recorded rather than papered over.** Reasoning is
**not durable** (`AgentEvent::Model(ContentPart::Reasoning)` is
ephemeral by the `event-durability-classification` spec, pinned by its
own test), and the server rebuilds a continued session's history from
that log (`session/controller/persist.rs`: append only
`if event.is_durable()`; `run_task.rs`: `let mut history = sink_history`).
So a *continued* session replays its assistant turns without the
thinking block. Today that is harmless — the reachable reasoning is
unsigned and would be dropped at the wire anyway — but it becomes a real
gap the moment extended thinking is enabled, because Anthropic then
requires the block back and a resumed turn would not have it. The fix
belongs with that enablement, not before it: making reasoning durable
changes log size, `/sessions/:id` rendering, and session search.

Verification: `cargo test -p synthia-harness --lib` **241/241**,
`cargo test -p synthia-provider --lib` **695/695**, the `provider_test`
integration target **47/47**, `make ci`, `make test-crates`,
`make examples` all green.

### Fixed (R88 — regenerate renders the same story on every page)

The last open known gap: after a regenerate, `/chat/:id`
collapsed the turn to the replacement (`prompt, answer2`)
while `/sessions/:id` and "Continue chat" projected the
durable log verbatim and showed the prompt twice. The fix
gives the rerun's persisted prompt row real replace
semantics — the `SurfaceOp::Replace` the session fold
already understands — so every projection agrees at once:

- **Server**: `SessionOp::Rerun` now parks a `ParkedPrompt`
  marked `rerun: true`; the run task computes the replace
  op against the pre-rerun fold (the span from the last
  user-role message to the tail, cited by the shadowed
  rows' seqs) and stamps it on the `UserInput` row it
  persists. The durable log still holds the turn twice
  (an honest record), but `fold_log_surface` — the single
  projection behind the next run's history
  (`events_to_messages`), session search, the regenerate
  route's own prompt recovery, and `typed_messages_from_sink`
  — folds it to the replacement story. The `SurfaceLedger`
  splices the same span (its `record` already applies ops on
  any surface-eligible row), so later compaction checkpoints
  still resolve.
- **Client**: `session-to-messages.ts` honors the op —
  `historyToChatMessages` tracks each message's creating log
  ordinal, and a `UserInput` row carrying `surface_op`
  drops the messages its cited seqs shadow (the inclusive
  seq range also removes sub-agent bubbles the replaced
  answer produced) before appending its own. The
  session-detail page and "Continue chat" now agree with
  the chat page without either special-casing.
- **Defensive fallback**: a rerun against a log with no
  user message (the regenerate route refuses this up
  front) degrades to a plain append — no op, no span.
- **Tests**: 3 new controller tests
  (`rerun_prompt_row_shadows_the_turn_it_replaces`,
  `chained_rerun_shadows_the_previous_rerun`,
  `rerun_without_a_user_turn_appends_plainly`);
  `cargo test -p synthia-server` 400 → 403 lib tests,
  all green.
- **Verified end-to-end** (Playwright MCP against
  `synthia-server` + Vite): the durable log holds the turn
  twice with the op citing the shadowed rows; `/chat/:id`
  renders one prompt + the new answer; `/sessions/:id`
  renders the same.


### Changed (R87 — dead code violating AGENTS.md §3.2 pruned)

R86 closed the test-block split cycle. With no
further organizational candidates, the audit
returned to AGENTS.md §3.2 directly:

> New Rust code that is unused MUST be deleted; the
> `dead_code` / `unused` attributes must not be used
> to suppress warnings.

The audit grepped every `#[allow((dead_code|unused))]`
in library code and read each call site. Six files,
+12 / −74, net **−62 lines**, **zero new warnings**.

- **3 phantom suppressor functions removed**:
  `delegation::_gate_module_link`, `worktree::_worktree_marker`,
  `summarizing::_type_anchor` — empty bodies whose
  `#[allow(dead_code)]` only silenced a warning that
  didn't exist.
- **Dead fields / variants pruned**: `ShortRef`
  collapses to `{ short_id }` (its `tool_call_id` /
  `tool_name` were set but never read); `PartialBlock`
  loses its unused `Text` and `Reasoning` variants
  (text/reasoning accumulate as separate
  `BlockAssembler` fields, not as `PartialBlock`
  entries); `LockGuard` loses its unused `root` and
  `job_id` fields.
- **1 dead helper deleted**: `home_dir_fallback` —
  no callers, no purpose.
- **Imports trimmed**: `ToolUse` (genuinely unused in
  `summarizing.rs` lib code) deleted; `ToolResult` and
  `TextContent` moved from the parent `use` to a
  test-local `use` (they were only consumed in
  `mod tests`); `GateVerdict` removed from
  `delegation.rs`'s private import list.
- **rustdoc link fixed**: the
  `[GateVerdict]` intra-doc link in
  `delegation.rs` was redirected to the
  `DelegationGateVerdict` alias that the `pub use`
  makes visible. The `make ci` `doc-check` gate
  caught this on the first run — exactly the kind
  of cross-crate coupling the gates are for.

Legitimate `#[allow(dead_code)]` kept: the
`synthia-mcp` `Child` field (held for `Drop` impl),
and the test-module clippy allowlists.

`cargo build --workspace --all-targets`: zero warnings.
`make ci` 6/6; `make test-unit` 2447/2447 (unchanged);
`make examples` exit 0 with proof lines.

### Changed (R86 — test-block split cycle closed)

R85 named `mod scope` / `mod descriptor_cache` /
`mod snapshot_cache` as the next R86 candidate. On
inspection, those three names mapped to line ranges
in the pre-R81 file that **already collapsed into the
sections R82–R85 extracted** (`mod snapshot`,
`mod registration`, `mod dispatch`, `mod mutations`).
The deferred list is now closed.

This is the audit-closure round: no code moves. The
parent `mod tests` block at
`crates/synthia-tool/src/registry.rs` (lines 1369–3297)
now holds exactly:

- 8 sub-module declarations (`mod fixtures` + 7
  others)
- 8 sub-module intro comments
- 8 one-line "moved to ... above" markers
- the parent-level `pub(crate) use fixtures::{...};`
  re-export

Zero `#[test]` / `#[tokio::test]` items live at the
parent block's indent (`^    #\[(test|tokio::test)\]`).
Every test now lives in one focused sub-module. The 8
sub-modules are all verified by name-filter
(`fixtures` → 0 filter, `exposure` → 7,
`session_scope` → 4, `registration` → 24, `snapshot`
→ 16, `dispatch` → 17, `mutations` → 2,
`argument_validation` → 5).

`cargo test -p synthia-tool --lib` 283/283 (unchanged);
`make ci` 6/6; `make examples` exit 0 with proof lines.

### Changed (R85 — dispatch tests moved to a focused sub-module)

R84 named `mod dispatch` as the next R85 candidate.
6 tests (per-call truncation bound, streaming dispatch
collects the final Result, empty/progress-only streams
are contract violations, snapshot_with_provenance
carries provenance + filters/sorts) move from the
parent `mod tests` block into `mod dispatch`. The four
local `Tool` fixtures (`LargeOutputTool`,
`StreamingTool`, `EmptyStreamTool`, `ProgressOnlyTool`)
travel with the section — each is referenced only by
these tests.

- No test renamed, no assertion touched, no fixture
  behaviour change.
- `mod dispatch` is `#[cfg(test)]`-only; `make
  check-public-api-runtime` green.
- `cargo test -p synthia-tool --lib dispatch` → 17
  passed (the 4 fixtures are `#[derive(Debug)]`
  structs without tests; the 6 new `dispatch::*` tests
  plus 11 existing `dispatch_*` tests in
  `mod registration` both match the `dispatch`
  substring).

Eight sub-modules now own focused concerns within the
parent `mod tests` block: `argument_validation` /
`session_scope` / `exposure` / `snapshot` /
`mutations` (R81–R82), `fixtures` (R83),
`registration` (R84), `dispatch` (R85).

`cargo test -p synthia-tool --lib` 283/283 (unchanged);
`make ci` 6/6; `make examples` exit 0 with proof lines.

### Changed (R84 — registration/trait tests moved to a focused sub-module)

R83 named `mod registration` as the immediate
follow-up. R83's `mod fixtures` made this a pure
section move — every shared `Tool` impl now lives
in one focused sub-module, so this extraction drags
no shared state with it. 30 tests (register_entry
accept/refuse/increment, unregister semantics,
descriptor/snapshot caches, snapshot ordering/dedupe/
hidden-filter, `get`/`list`/`contains_and_len`
through the public surface and the `Registry` trait,
plus dispatch-shape coverage with one-off `Tool`
impls) move from the parent `mod tests` block into
`mod registration`. Two `Registry`-trait tests use
`super::super::registry_trait::ToolFilter` (private
sibling module).

- No test renamed, no assertion touched, no fixture
  behaviour change.
- `mod registration` is `#[cfg(test)]`-only; `make
  check-public-api-runtime` green.
- `cargo test -p synthia-tool --lib registration` →
  24 passed (filtered).

Six sub-modules now own focused concerns within the
parent `mod tests` block: `argument_validation`
(R81), `session_scope` (R82), `exposure` (R82),
`snapshot` (R82), `mutations` (R82), `fixtures`
(R83), and `registration` (R84).

`cargo test -p synthia-tool --lib` 283/283 (unchanged);
`make ci` 6/6; `make examples` exit 0 with proof lines.

### Changed (R83 — shared test fixtures moved to a focused sub-module)

Three `Tool` impls (`TestEntryTool`, `ShadowTool`,
`NamedTool`) were defined inline in the Registration/Trait
section but referenced from every section of the parent
`mod tests` block (registration, exposure, session_scope,
snapshot, descriptor cache, mutations) — six sections,
three fixtures, eighteen cross-references. Each fixture
had one canonical definition, but the *location* was
wrong.

All three impls move to `pub mod fixtures` at the top of
the parent block. `pub(crate) use fixtures::{...};` at the
parent level re-exports the three types, so every existing
test body — every `Arc::new(TestEntryTool)`,
`NamedTool("alpha")`, `Arc::new(ShadowTool)` — keeps
compiling unchanged.

- No test body / no assertion / no fixture behaviour
  changes.
- `mod fixtures` is `#[cfg(test)]`-only; `make
  check-public-api-runtime` green.
- Sets up `mod registration` (R84) for a pure section move
  on the same R81/R82 pattern.

`cargo test -p synthia-tool --lib` 283/283 (unchanged);
`make ci` 6/6; `make examples` exit 0 with proof lines.

### Changed (R82 — four more test sections moved to focused sub-modules)

Round opened with a full re-audit against the standing
objective: reference parity is closed (no commit in
`traitclaw` / `pi` since 2026-09-10; every
`reference-parity.md` row mapped / declined / closed),
no drift surface is open (the R76–R80 grep now hits only
test assertions), and all architecture gates green.

What landed — four commits, all the R81 pattern
(sub-module inside the parent `mod tests`, one-line
marker at the original position, zero logic change):

- `mod session_scope` — the 4 `create_session_scope`
  lifecycle tests (this was the uncommitted extraction
  sitting in the working tree from the previous
  session; verified then committed).
- `mod exposure` — the 5 exposure-plumbing tests
  (default `Direct`, entry→descriptor carry,
  descriptors-vs-snapshot privacy split, `Registry`
  get/list preservation, hidden-but-executable
  dispatch).
- `mod snapshot` — the 7 dual-index snapshot/catalog
  tests (name sorting, unregister reflection, empty
  registry, entry metadata builders, canonical
  resolve, materialisation content, scoped-arc drop).
- `mod mutations` — the 2 post-registration mutation
  tests (`set_exposure` projection reach + version
  semantics; `set_hidden` gating pinned apart from
  `Hidden` exposure).

23 of the 283 lib tests now live in five filterable
sub-modules; every filter verified
(`session_scope` → 4, `exposure` → 5, `snapshot::` → 7,
`mutations` → 2, `argument_validation` → 5).

`cargo test -p synthia-tool --lib` 283/283 after each
commit; `make ci` 6/6; `make test-unit` 2447/2447
(unchanged); `make examples` exit 0 with proof lines.

Remaining flat sections (registration + fixtures,
dispatch, the small cache/scope sections) are R83+
candidates on the same pattern.

### Changed (R81 — R74 tests moved to a focused sub-module)

`synthia_tool::registry::tests` was 1692 lines / 56 822
bytes — the largest `mod tests` in the workspace. The
R74 argument-validation block (198 lines, lines 2862–3059)
was the most self-contained section: recent, focused,
with its own `ShellTool` fixture. Extracted to a new
`mod argument_validation` inside the parent block.

- The 5 R74 tests now report under
  `argument_validation::*` (`cargo test
  argument_validation` jumps straight to them).
- `use super::*;` brings the parent block's `Tool`,
  `ToolOutput`, `Context`, `ToolRegistry`, `ToolEntry`,
  `TestEntryTool`, `collect_results`, etc. into the
  sub-module.
- One-line marker in the parent block at the original
  position pointing at the new sub-module.
- No test logic changes; no fixture changes; no new
  public surface.

`cargo test -p synthia-tool --lib` 283/283 (unchanged
total; the 5 R74 tests just moved sub-module). `make ci`
6/6; `make test-unit` 2447/2447 (unchanged).

The other 1450 lines of the parent `mod tests` block
can follow the same pattern in R82+: `mod exposure`,
`mod scope`, `mod registration`, `mod descriptor_cache`,
`mod snapshot_cache`, `mod snapshot`, `mod dispatch`,
`mod mutations`, `mod materialization`, plus a shared
`mod fixtures` for the `TestEntryTool` / `ShadowTool` /
`NamedTool` types. Each is a future round; R81
establishes the pattern.




### Added (R80 — shared `MAX_LIMIT` constant, one canonical home)

Same drift pattern as R77/R78/R79, but for a
**constant** rather than a function. Two near-duplicates
of the same value, in two crates, with a doc comment
saying *"MUST stay in sync"*: `synthia_core::registry::
REGISTRY_MAX_LIMIT = 100` (private, used by
`paginate_registry_list`) and `synthia_server::api::v1::
page_query::MAX_LIMIT = 100` (public, 8 reference sites
including handlers and tests). A refactor that changed
one without the other would silently desync the
wire-level cap from the in-memory registry pagination
cap.

- The core's constant renamed `REGISTRY_MAX_LIMIT` →
  `MAX_LIMIT` and made `pub`. The doc comment
  documents the role and the relationship to the
  server's wire-level cap.
- The server's `MAX_LIMIT` is now `pub const MAX_LIMIT:
  u64 = synthia_core::registry::MAX_LIMIT;` — same
  public symbol name, value sourced from core. The
  doc comment explains the alias and the
  single-source-of-truth contract.
- 1 new test in `synthia-core` (135 total, was 134):
  `max_limit_matches_spec` — pins the value to `100`.
  The server's existing `constants_match_spec` test
  also asserts the alias equality — drift detector.
- `synthia_core::lib.rs` re-exports `MAX_LIMIT`; the
  module-doc table row for `registry` mentions the
  constant.

`make ci` 6/6; `make test-unit` 2447/2447; `make examples`
exit 0 with `CONSUMER-PROOF: OK` + `MVP-OK` +
`BEST-OF-N-JUDGE: OK`.




### Added (R79 — shared cursor codec, one canonical home)

`synthia_core::registry::encode_registry_cursor` and
`decode_registry_cursor` were private 3-line copies whose
doc comment said *"The encoding MUST match
`synthia_server::api::v1::decode_cursor`"* — the exact
drift surface R76 closed for the judge parser. Two
near-duplicates of the same URL-safe base64 codec, one
in core (the in-memory registry pagination) and one in
the server (the HTTP transport), that *must* stay in
lockstep but were physically separate.

New module `crates/synthia-core/src/cursor.rs` —
`pub fn encode(id: &str) -> String` and
`pub fn decode(cursor: &str) -> Result<String, Error>`.
The contract: URL-safe base64, no padding, UTF-8
validated. Both failure modes (invalid base64, invalid
UTF-8) collapse to `Error::InvalidItem` because a
wire-level bad-request is the same regardless of which
stage failed. The module-doc documents the encoding's
*stability* guarantee (a cursor emitted by an older
`synthia-core` round-trips on a newer one and vice
versa) so a future algorithm change is a documented
breaking change.

6 new tests in `synthia-core` (134 total, was 128):
`encode_matches_spec_example` (the literal
`task_abc → dGFza19hYmM` from the doc comment, pinned
as a doctest AND a test), ASCII round-trip, unicode
round-trip (`task_αβγ`), invalid-base64 rejection,
non-UTF-8 payload rejection, and `empty → empty`
round-trip (an empty cursor is the start-of-list
marker, not an error).

The core's private `decode_registry_cursor` /
`encode_registry_cursor` become 2-line delegations
over the new public functions. The server's
`encode_cursor` / `decode_cursor` likewise become
2-line delegations; the 4 server tests
(`encode_cursor_matches_spec_example`,
`decode_cursor_round_trip`,
`decode_cursor_handles_unicode_ids`,
`decode_cursor_rejects_invalid_base64`,
`decode_cursor_rejects_non_utf8_payload`) pass
byte-for-byte against the delegation. The
non-UTF-8 test still uses `base64` directly to
build a payload `synthia_core::cursor::encode`
cannot produce (a non-UTF-8 string).

`make ci` 6/6; `make test-unit` 2446/2446; `make examples`
exit 0 with `CONSUMER-PROOF: OK` + `MVP-OK` +
`BEST-OF-N-JUDGE: OK`.




### Added (R78 — shared redaction helper, one canonical home)

`synthia_server::api::v1::validation::api_key_mask` was a
19-line function with 6 dedicated unit tests for the
partial-redaction contract (first 4 + `***` + last 3,
≤7-char full-mask, multibyte safe, no-leak guarantee).
The natural home for it is `synthia_core::sensitive` —
the same crate that already hosts `Sensitive` /
`SensitiveData` (the `Debug`-time redaction newtype), and
whose module-doc explicitly calls itself *"Sensitive
information encryption / redaction primitives"*.

- `synthia_core::sensitive::redact_partial(s: &str) -> String`
  — first 4 + `***` + last 3, ≤7-char full-mask fallback,
  multibyte safe, empty input → empty output. The 4+3
  shape is the most common API key contract (Anthropic
  `sk-ant-…`, OpenAI `sk-…`, GitHub `ghp_…`).
- `synthia_core::sensitive::redact_partial_with(s,
  keep_first, keep_last) -> String` — caller chooses the
  prefix/suffix lengths; falls back to `"***"` when the
  sum would equal or exceed the input.
- 8 new tests in `synthia-core` (128 total, was 120):
  long key shape, empty, short input, exactly 8 chars,
  multibyte safe, no-leak guarantee, parameterised
  variant, parameterised fallback.
- The server's `api_key_mask` becomes 2 lines of
  delegation. The 6 server tests pass byte-for-byte.
  The public surface of `synthia-server` is unchanged.
- `synthia_core::lib.rs` re-exports `redact_partial` and
  `redact_partial_with` alongside `Sensitive` /
  `SensitiveData`; the module-doc table row mentions
  the new helper.

`make ci` 6/6; `make test-unit` 2440/2440; `make examples`
exit 0 with `CONSUMER-PROOF: OK` + `MVP-OK` +
`BEST-OF-N-JUDGE: OK`.




### Added (R77 — shared char-based truncator, one canonical home)

A workspace-wide grep for `s.chars().take(N).collect()` found
11 inline sites across 8 crates
(`synthia-mcp`, `synthia-session`, `synthia-steering`,
`synthia-rag`, `synthia-attachment`, `synthia-agent`,
`synthia-provider`, `synthia-tool`, plus `synthia-server`).
Each site had its own slight variation; the marker formats
were inconsistent (`…`, `...`, `[truncated]`,
`[truncated N chars]`, or none).

New `synthia_core::text::truncate_chars(s: &str, max_chars:
usize) -> (String, bool)` is the char-counted, return-a-pair
variant. Returns `(s.to_string(), false)` when the input is
shorter than the cap, else `(first max_chars chars, true)`.
No marker — the caller decides. Sits next to the
byte-counted, in-place `cap_to_char_boundary` (which
already lived in `synthia_core::text`).

`synthia_server::session::controller::truncate` (14 lines,
6 dedicated unit tests) becomes a 6-line wrapper over
`truncate_chars` that adds the `…` marker. The 6 server
tests still pass — the contract is preserved byte-for-byte.

6 new tests in `synthia-core` (120 total, was 114):
shorter verbatim, exact-match verbatim, one-over
truncated, empty input, zero max with non-empty (the
server's corner case), multibyte char-count (中文 / 日本語),
4-byte emoji.

The 10 inline `s.chars().take(N).collect()` sites stay
untouched on purpose — their output formats are part of
their call sites' contracts; a per-site refactor
preserves the marker each consumer expects. The
primitive is now in place for follow-up R50 work.

`make ci` 6/6; `make test-unit` 2432/2432; `make examples`
exit 0 with `CONSUMER-PROOF: OK` + `MVP-OK` +
`BEST-OF-N-JUDGE: OK`.




### Fixed (R76 — shared LLM-judge reply parser, one canonical home)

`synthia_eval::metrics::parse_score` and
`synthia_agent::agent::parse_judge_score` were two
near-duplicates of the same parser. R73 documented the
duplication and promised "byte-compatible" behaviour; in
practice the two did not accept the same inputs. The
agent's parser was the documented contract (case-insensitive
on the `Score:` prefix, accepting both `Score: 0.5` and
`Score:0.5`); the eval's was case-sensitive and rejected
`Score:0.5`. A judge that said `score: 0.5` (lowercase)
silently scored `0.0` in `LlmJudgeMetric` and `0.5` in
`LlmJudgeScorer`. The bug was unreachable to the existing
tests (none pinned the lowercase case), but the contract
said "case-insensitive" and a real judge that varied its
casing would surface it.

The right home is `synthia_core` — the same crate that
already hosts `validate_against_schema`. Both `synthia-eval`
and `synthia-agent` already depend on it, and the parser
is a pure function with no async, no runtime, no
dependencies. New module `crates/synthia-core/src/judge_score.rs`
+ `pub use judge_score::parse_judge_score;` from
`synthia_core`. The agent's and eval's parsers are now
thin wrappers over the shared one.

- 7 new tests in `synthia-core` (114 total, was 107):
  `parses_score_prefix_with_space`,
  `parses_score_prefix_without_space`,
  `parses_standalone_number`, `finds_score_on_later_line`,
  `clamps_out_of_range`, `unknown_reply_scores_zero`,
  `handles_lowercase_and_mixed_case`.
- 70 lines of duplicated parser code in
  `synthia-agent/src/agent/best_of_n_llm_judge.rs` deleted
  (the local `stripped_prefix_ci` and `parse_first_f64`
  helpers); replaced by a 1-line delegation.
- The 4 eval tests and 5 agent tests for the parser
  still pass (the new contract is a strict superset).
- `make ci` 6/6; `make test-unit` 2426/2426;
  `make examples` exit 0 with `CONSUMER-PROOF: OK` +
  `MVP-OK` + `BEST-OF-N-JUDGE: OK`.




### Added (R75 — runnable example for R74)

`crates/synthia-tool/examples/argument_validation.rs` proves
the R74 `with_argument_validation` switch end-to-end: the same
`EchoTool`, two runs (off / on). The off path counts 2 tool
bodies (the default unchanged-by-R74 contract — even the
malformed call reaches the tool body); the on path counts 1
(the malformed call is rejected at the registry with
`is_error: true` and the dotted-path violation in the body).
`make examples` now picks it up alongside the other 34
example programs. The example ends with
`ARGUMENT-VALIDATION: OK`.

No library code change; the surface (`with_argument_validation`
+ `argument_validation_enabled`) was already in place. The
example is the only new file this round; the round map,
[`docs/examples/README.md`](docs/examples/README.md), and this
CHANGELOG entry were updated in the same commit.

`make ci` 6/6; `make test-unit` 2419/2419; `make examples`
exit 0 with `CONSUMER-PROOF: OK` + `MVP-OK` +
`BEST-OF-N-JUDGE: OK` (the new example is run, but its
`ARGUMENT-VALIDATION: OK` proof line is for direct invocation,
matching the convention of the other examples).




### Added (R74 — tool-argument validation in the registry, opt-in)

The dispatcher's hot path used to call `tool.stream(input, ctx)`
without checking `input` against the tool's `parameters()`.
Each tool validated its own arguments inside `call`, which
works but means a misbehaving model call (a missing required
field, a wrong-typed nested value) reaches the tool body and
fails there. pi's `agent-loop.ts` already does this in the
runtime with `validateToolArguments`; Synthia now has the same
shape, opt-in:

- `ToolRegistry::with_argument_validation(bool)` — builder.
  Off by default, so the hot path is unchanged for consumers
  that already validate inside `Tool::call`. A deployment that
  wants the registry to surface a schema mismatch as an
  `is_error` tool result flips it on once at boot.
- `ToolRegistry::argument_validation_enabled()` — introspection.
- A failed match synthesises a `Plan::Invalid` result with
  every dotted-path violation in the body, in the same
  numbered-list style the eval crate uses:
  ```
  invalid arguments for tool `shell` (2 violations):
  - cmd: expected required property present, found missing
  - timeout: expected type integer, found string
  ```
  Paid under the same read lock as the tool lookup, so a
  misbehaving call costs a synthesized error Result instead
  of a spawned task that runs a tool it shouldn't.
- `Clone` carries the flag through (tested).
- 5 new tests in `synthia-tool --lib` (283 total, was 278):
  default off, validation on synthesises the error, valid
  input still runs, `Clone` preserves the flag, empty
  schemas stay permissive.

The validator is `synthia_core::validate_against_schema` — the
same primitive `StructuredOutputTool` and the eval crate's
`SchemaValidationMetric` already use. No new crate, no new
dependency, no new feature flag. `make check-public-api-runtime`
still clean (the new method takes `bool` and returns `Self`).

`cargo test -p synthia-tool --lib` 278 → 283; `make test-unit`
2414 → 2419; `make ci` 6/6; `make examples` exit 0 with
`CONSUMER-PROOF: OK` + `MVP-OK` + `BEST-OF-N-JUDGE: OK`.




### Added (R73 — LLM-as-judge scorer, in-tree)

The `BestOfNStrategy` gap noted since R50 is closed. The trait
`CandidateScorer::score` is now `async fn` (additive — every
existing impl gains `#[async_trait]` and the body is unchanged),
and a new `LlmJudgeScorer` in
`crates/synthia-agent/src/agent/best_of_n_llm_judge.rs` holds an
`Arc<dyn ModelProvider>` to grade each candidate against a rubric:

- `LlmJudgeScorer::new(provider)` uses the in-tree
  `DEFAULT_JUDGE_PROMPT` (one-sentence rubric instructing the
  judge to reply with `Score: 0.0..=1.0`).
- `LlmJudgeScorer::with_rubric(provider, rubric)` is the seam for
  a deployment's own criteria.
- `with_max_tokens(u32)` caps the reply; default 32.
- `parse_judge_score(reply) -> f64` is the same shape
  `synthia_eval::metrics::parse_score` uses (`Score: …` on any
  line, case-insensitive, clamped to `0.0..=1.0`, fallback to a
  standalone number, `0.0` on no match). Duplicated rather than
  re-exported because the eval crate is for offline grading, the
  agent runs in production — the two stay byte-compatible by
  design. Re-exported from `synthia_agent` as
  `LlmJudgeScorer` and `DEFAULT_JUDGE_PROMPT`.
- Provider error path returns `0.5` (neutral) and logs. `0.0`
  would tie every failed judge at the bottom; `1.0` would bury
  the strategy's own failure path. `0.5` keeps the judge out of
  the ranking while the strategy records the failure.

`cargo test -p synthia-agent --lib` 316 → 324 (8 new: 5 for
`parse_judge_score` directly, 3 through a stub provider including
the error-path neutral-fallback). The 6 existing `best_of_n`
tests still pin the strategy with a sync `Shortest` scorer. The
`strategy_swap.rs` example's `RequiresMarker` scorer gained
`#[async_trait]`; its body is unchanged.
- `crates/synthia-agent/examples/best_of_n_judge.rs` — a runnable
  demo: 3 candidates, 3 judge calls, the highest-scoring candidate
  wins. Prints per-candidate judge scores and ends with
  `BEST-OF-N-JUDGE: OK`; picked up by `make examples`.
- `make ci` 6/6 green; `make test-unit` 2414/2414 pass; `make
  examples` exit 0 with `CONSUMER-PROOF: OK` + `MVP-OK` +
  `BEST-OF-N-JUDGE: OK`.




### Removed (R72 — `edit_message` route)

The route validated `message_id` and then dropped it, building a fresh
`SendMessageRequest` from the body and submitting `Prompt`/`PromptMulti`
— appending a *new* turn at the tail. The documented contract
("replaces the user message identified by `message_id`… past turns
before `message_id` are preserved as history") was unimplemented on
both counts, and nothing called it from the frontend. Cutting it
removes a silently-wrong mutator and the cognitive-load of explaining
why a "replace" endpoint appends. The same replace semantics
(regenerate also appends rather than replacing the assistant turn) is
recorded as a known gap with the two ways to reconcile it.

The deleted surface:
- `routes::chat::edit_message` handler + `EditMessageRequest` type.
- The PATCH route registration in `routes/mod.rs` and the duplicate
  registration in `server/router.rs`.
- Four stale doc comments (`lib.rs` chat-surface list, the
  `server/router.rs` route-list comment + header comment, and the
  crate-level module-doc entry).

Test count unchanged: `cargo test -p synthia-server --lib` 400 passed,
no call sites exist.


### Removed (R72 — the frontend e2e suite)

The Playwright e2e tree is gone, by the maintainer's call that it was
too slow to be worth its cost.

- `synthia-web/tests/` (44 specs, ~5,200 lines), `playwright.config.ts`,
  `playwright.contract.config.ts`, and `.github/workflows/e2e.yml`.
- `synthia-web/scripts/e2e/` — 709 lines of ad-hoc browser scripts,
  superseded by the suite they predated (and unusable since
  `playwright` left `package.json`).
- `contract-closure/contract-coverage.ts` + its test, and the `coverage`
  npm script, and the `test-contract-closure-playwright` CI job: their
  only input was the deleted spec directory, so they could only measure
  nothing.
- `synthia-web/scripts/_logo-snapshot.mjs` — a throwaway that imported
  `playwright`, now uninstalled.
- The `@playwright/test` and `playwright` devDependencies, the
  playwright npm scripts, `make test-e2e` / `test-e2e-ui` /
  `test-e2e-headed` / `test-e2e-report` / `install-e2e-deps` /
  `test-web`, and the `test-results`/`playwright-report` ignores.

Why: a full run took **13 minutes** because the top-level `webServer`
booted `cargo run -p synthia-server` plus Vite for every invocation, and
its value was undermined by silent rot — three specs had been red on
master for months because their fixtures still used a pre-refactor wire
shape (`{messageId, role: 'ROLE_AGENT', parts}` with no `type`, which
`applyHistoryEntry` cannot dispatch). Frontend verification is now
Playwright MCP driving the running app (`AGENTS.md` §4.2), which
exercises the real code instead of a second contract that has to be
maintained. The MCP browser comes from the harness, not from
`synthia-web`'s dependencies — verified by driving the app after the
`playwright` package was uninstalled.

What still gates a frontend change: `make check-web` (tsc + eslint +
prettier). Nothing runs on behaviour automatically, and that is the
trade this makes — recorded in `docs/README.md`.

### Fixed (R72 — four frontend lockfiles collapsed into one)

`synthia-web/` tracked `package-lock.json` **and** `pnpm-lock.yaml`; the
repo root added a phantom npm workspace (`package.json` =
`synthia-frontend-root`, `workspaces: ["synthia-web"]`) with its own
`package-lock.json` + `pnpm-lock.yaml`. Nothing invoked pnpm — no
`.npmrc`, no CI step, no Makefile target, no Dockerfile — and nothing
installed at the root.

The divergence was not cosmetic. Measured:

- `cd synthia-web && npm ci` resolved the **root** lock (the workspace
  declaration wins) and exited 0 even with `synthia-web/package-lock.json`
  deleted — so that file was dead weight for CI and the Makefile.
- The same file is what `Dockerfile.web` consumes, because it copies
  only `synthia-web/` and has no workspace root. There it **failed**:
  `npm error Missing: @radix-ui/themes@3.3.0 from lock file`, exit 1 —
  identically at HEAD, so the Docker web image had been unbuildable
  independently of any R72 change.
- The root package declared `react-router-dom ^7.18.1` while
  `synthia-web` declares `^6.30.4`; the app resolves 6.30.4. The root's
  copy was a second major that nothing imported.

Now: one lock (`synthia-web/package-lock.json`, regenerated standalone
so it resolves without a parent workspace — 412 packages, no
`@radix-ui/themes` gap, no playwright), the phantom root workspace and
all `pnpm-*` files deleted, and `.github/workflows/rust-quality.yml`'s
`cache-dependency-path` re-pointed at the survivor.

Verified against all three consumers: `cd synthia-web && npm ci` → 0;
the `Dockerfile.web` shape (only `synthia-web/package.json` +
`package-lock.json` in an empty dir) → `npm ci` installs 361 packages,
`@radix-ui/themes` present, `playwright` absent; then a real install and
`tsc` + `eslint` + `vite build` green in `synthia-web/`.

### Fixed (R72 — multimodal reaches both ends of the chat surface)

R71 made the model see a tool's image. The web UI still threw it
away: `toolResultContentText` in `synthia-web`'s history
reconstructor kept only parts carrying a `.text` field, so a
`tool_result` whose content was `[text, image]` replayed as bare
text and an image-only result replayed as nothing at all. The live
SSE path had the same hole.

Six adjacent defects in the same surface surfaced while fixing it
and are fixed too:

- **Live tool results rendered an empty body.** `ToolResult` has no
  `text` field — its body lives in the `content` array — but
  `extractFromMessage` hardcoded `text: ''` and `partMetadata` only
  read `part.text`. `tests/e2e/ui/chat-tool-result.spec.ts` had been
  red on master for this.
- **URL-sourced images were wrapped as base64.**
  `ImageContent.data` is polymorphic: the server writes the remote
  URL itself for a `url` attachment
  (`synthia-server/src/routes/chat.rs`, `build_parts`), so a blanket
  `data:…;base64,` prefix produced a broken `<img>`.
- **Image payloads would have bloated localStorage.** The persist
  path swallows `QuotaExceededError`, so a single multi-MB
  screenshot would silently stop the whole conversation from
  persisting. `stripAttachmentSegments` is extended to `images`, and
  `seedChatFromSession` (the "Continue this session in chat" path)
  previously bypassed that strip entirely.
- **The click-to-enlarge link could not work.** Browsers refuse
  top-frame navigation to a `data:` URL — the common case — so it
  became an in-place expand toggle.
- **The user's own attached picture never rendered.**
  `handleSubmit` set `attachments` on the sent message, but
  `ChatMessageViewItem` has no `attachments` field and the list
  renders only `segments`, so an image-only send produced an empty
  `> USER` bubble. New `imagesFromAttachments` normalises the
  composer's uploads onto the turn's text segment.
- **Retry dropped the picture, and the Retry banner was
  unreachable.** The retry effect rebuilt the user message from
  `lastSubmittedText` alone and re-sent without `attachments`, so a
  retried multimodal turn re-ran the model without the image and
  appended an image-less bubble; `handleRetry` also gated on
  `lastSubmittedText`, making an image-only prompt unretryable. It
  now caches the whole turn (`lastSubmission{text, attachments}`).
  Separately, `sendMessageStream` reports every failure as a
  *yielded* event, so ChatPage's `catch` — the only
  `setStreamError` caller — never ran and the banner never appeared;
  the `error` branch of `applyStreamEvent` now raises it, and a
  failed turn keeps the cached turn for Retry instead of clearing it.
  The same dead `catch` also stranded `setServerHealth(false)`, so a
  dropped connection left the header reading "Online"; the error
  branch now flips it, gated on `code === -1` (the transport-failure
  signal) so a real HTTP response — a 500 means the server answered —
  does not raise a false outage.

Rust side: a multimodal turn persisted **no `UserInput` row at all**.
`PromptMulti` parks the whole payload in `parts` and leaves the
drained text queue empty, so `prompt` was `""` and the
`if !prompt.is_empty()` guard skipped the append — a resumed session
then showed the assistant's reply with no question above it, and
`events_to_messages` handed the next turn's agent a history whose last
user message was missing. The run task now derives the text from the
payload's `Text` part (`prompt_text_of_parts`); the binary parts stay
out of the sink by design.

Changed files: `api/chat-message.ts` (new `ChatImage`, `extractImages`,
`contentText`, `imagesFromAttachments`), `lib/session-to-messages.ts`,
`lib/strip-attachment-segments.ts`, `api/chat-stream.ts`,
`pages/ChatPage.tsx`, `components/chat/ChatMessageView.tsx`,
`pages/ChatPage.css`; `session/controller.rs`.

Also repaired `tests/e2e/unit/is-tool-echo-log.spec.ts`, which was red
on master: its mock still used the pre-refactor `SessionTurn` shape
(`role: 'ROLE_AGENT'` / `messageId` / `parts`) and cited
`renderHistoryMessage`, which no longer exists, so
`applyHistoryEntry` never dispatched and no branch logged.
Modernised to the real `{type: 'Model', data: {type: 'text', …}}`
envelope; all six `isToolEchoText` branches now fire.

Scope note: user-prompt images are rendered live from the composer
(`imagesFromAttachments` → the turn's text segment) but are **not
durable** — the controller persists only the textual component of a
prompt (`synthia-server/src/session/controller.rs`: "Raw image/audio
bytes are NOT serialised into the JSONL sink"), so a cold reload
rebuilds the turn from text alone. The tool-result half is durable
(`persist_and_broadcast` appends the whole `AgentEvent::Model`).

Verification for the frontend is browser-driven rather than
spec-authored: `extractImages` / `contentText` /
`imagesFromAttachments` were exercised through the running app, and
the render paths were confirmed against a live dev server with
`naturalWidth > 0` (a decoded image, not a silent broken icon) for
both the history-replay and live-SSE tool-result paths, plus the
composer attachment and retry flows.

### Fixed (R72 — `regenerate` reruns, and two stale claims)

`POST /api/v1/chat/sessions/{id}/regenerate` — the route behind the
chat UI's "Regenerate" button — could never rerun a turn.
`extract_last_user_parts` looked for rows shaped
`{"role":"user","parts":[…]}`; no writer in the repo emits that. Real
logs carry `{"type":"UserInput","data":{"text":…}}`, so the recovery
always returned `None` and the handler silently fell through to
`SessionOp::Cancel` — the button stopped the turn instead of replaying
it, and the whole `SessionOp::Rerun` arm was dead.

It now projects through the shared `fold_log_surface`, which is the
single decoder for every envelope the sink has carried (`UserInput`,
the typed `user_message` family, the lossless `Message` envelope, the
legacy `Model` parts) and honours a durable compaction checkpoint. A
hand-rolled matcher had rotted the moment the writer changed; going
through the fold means the reader cannot drift from the writer again.

`crates/synthia-server/tests/regenerate_test.rs` pins it with three
tests through the real router: the most recent user turn is replayed
(observable as a second `UserInput` row — `Cancel` appends nothing), a
hand-written log with no user turn is reported rather than acted on,
and a multimodal turn's text replays. The two replay tests fail on the
old matcher (observed: `1 passed; 2 failed`); the miss test passes
either way, so it guards the miss contract, not the parser.

Two more defects became reachable once `Rerun` was live, and are fixed
with it:

- **A `Rerun` arriving mid-run was silently dropped.** The run arm
  parks its parts in `pending_multimodal` and skips starting a run
  while one is in flight; the completion branch then gated the restart
  on the *text* queue alone, which a rerun never fills. The parts sat
  parked until some later op's run consumed them — so the agent would
  answer that op's prompt with the stale rerun instead. The gate is now
  `maybe_start_run` itself, which checks both input slots and returns
  `None` when neither holds anything. Pinned by
  `rerun_during_an_active_run_still_starts_its_own_run`, which times
  out on the old gate.
- **A regenerate miss cancelled the session.** `None => SessionOp::Cancel`
  answered "no user turn recoverable" with the same op `/cancel`
  submits, so a lookup miss aborted the session as a side effect of a
  failed read. It now returns `404` and leaves the session running.

`rerun_hands_its_parts_to_the_factory` additionally pins that the
recovered parts reach the factory through the multimodal seam — the
semantics `Rerun` only now has, since it was unreachable before.

Two claims corrected while there, both of which described behaviour the
code never had:

- `SessionOp::Rerun`'s doc said it "carries the SAME multimodal
  payload as the original turn so the agent sees the same
  attachments". It cannot: attachment bytes are not persisted. The
  route doc said the turn is "replayed verbatim from the session log".
  Both now say the prompt **text** is replayable and the attachment is
  not.
- `AGENTS.md` §3.8 stated the clock-ratchet baseline as 18;
  `Makefile` has `CLOCK_BASELINE := 6` (R65/R66 and later rounds
  lowered it without updating the doc), which is exactly the
  rule-vs-config drift §3.4 warns about.

Also unified the `images` shape on rebuilt tool segments: the three
push sites in `session-to-messages.ts` now omit the field when there
are no images instead of one site writing `[]` and another
`undefined`, so a rebuilt segment and a freshly-created one are
byte-identical.

### Fixed (R72 — Regenerate works end to end, and the transcript folds right)

The server work above left the button inert: `handleRegenerate` only
awaited the POST, so the rerun's frames broadcast to zero subscribers
and nothing on the page changed. Wiring it up surfaced three more
defects, including one that had been mis-rendering every multi-turn
session.

- **`findAssistantTarget` merged answers across a user turn.**
  `session-to-messages.ts` scanned backwards for *any* assistant
  message and never stopped at a `role === 'user'` boundary, so a
  four-row transcript `[user, answer1, user, answer2]` folded to three
  messages with `answer1answer2` in ONE bubble, rendered *above* the
  prompt that produced it. Every multi-turn session on `/sessions/:id`
  was affected; the retained specs never noticed because
  `session-to-messages.spec.ts` has no `UserInput` case at all. The scan
  now stops at a user message (still skipping subagent bubbles).
- **The Regenerate handler rebuilt nothing.** It now snapshots the
  transcript, hides the trailing answer while the rerun runs, waits for
  the rerun to produce an answer, and appends only that. Three gates
  were needed because each alone fires too early: a row beyond the
  baseline (the rerun's *prompt* row lands before the agent is even
  invoked), `status !== 'working'` (true in the gap between the `202`
  and the run actually starting), and an answer at the end of the
  rebuild. Every failure path restores the snapshot, so a failed rerun
  cannot leave a hole where the reply used to be.
- **`regenerate` resolved the controller before reading the log.**
  `get_or_create_session_controller` eagerly creates an unknown
  session, so the miss path — the one case that must have no side
  effects — was the very path that created a session. Reading first
  also lets an absent `events.jsonl` (created-but-never-written) be
  reported as the miss it is instead of a `500`.

New: `getSessionDetail` (`api/chat-stream.ts`) reads one session's
durable transcript.

Scope, stated plainly: the chat page renders the rerun as a
**replacement** (prompt kept once, superseded answer dropped), while
`/sessions/:id` and "Continue chat" render the log as it is —
`prompt, answer1, prompt, answer2`, because regenerate *appends* the
replayed turn rather than replacing it. The two views therefore differ
for any regenerated session. That is a deliberate split for now (the
durable log stays honest and the chat view shows what the user asked
for), but it is a divergence, not agreement, and it is recorded in
`docs/README.md` as a gap. The provider context is unaffected: it sees
`prompt, answer1, prompt` — a normal multi-turn exchange, not the
consecutive duplicate the controller warns about at
`controller.rs`'s `Prompt` arm.

Verified in the running app (Playwright MCP, per `AGENTS.md` §4.2):
against a transcript that gains the replayed prompt first and the
answer later, clicking Regenerate goes from
`1 user / ["OLD ANSWER"]` to `1 user / ["NEW ANSWER"]` — the old answer
replaced rather than stacked, and the prompt not duplicated.

Two more routes lied about what they do, both found in the same audit:

- **`feedback` recorded every rating into a phantom session.** The
  handler resolved a hardcoded `"default"` session id, so every 👍/👎
  from every session was written to a log that does not belong to the
  conversation — and `get_or_create_session_controller` *created* that
  session on the first click. The handler's own comment called this a
  temporary scaffold ending "production deployments wire the full
  path", but this router is production and nothing else wires it.
  `session_id` now rides in the request body (the message id is a
  client-side label and cannot identify a session), is validated like
  any path parameter because it reaches the sink's filesystem path, and
  the frontend sends it.
- **`RunLog` counted rows locally while the sink owned the ordinal.**
  The run's ledger indexed each row by a counter seeded at run start,
  but the controller's op loop appends out-of-band too — the `Feedback`
  row and the shutdown marker go straight to the sink. Every such row
  advanced the sink's counter without the run knowing, so the ledger's
  ordinals drifted, and since those ordinals are what a compaction
  checkpoint's `source_event_seqs` are validated against, a good splice
  could look unresolvable: `try_fold_log_surface` errors and the lenient
  fold silently replays the span the checkpoint had replaced.

  The fix lands in two steps, and the second is the one that matters:
  `RunLog` first read the sink's counter back after each append, which
  closed the systematic drift but not the race (the counter can move
  between the append returning and the read). `SessionSink::append` now
  **returns** the ordinal it assigned under its own lock — it is the
  only party that can report it — and the ledger records that value.
  Surfaced by routing `feedback` to real sessions; the same class of
  "the doc and the behaviour disagreed" defect.

- **`edit_message` never uses `message_id`.** It is only passed to
  `validate_resource_name` and then dropped: the handler builds a fresh
  `SendMessageRequest` from the body and submits `Prompt`/`PromptMulti`,
  appending a NEW turn at the tail. The documented contract —
  "Replaces the user message identified by `message_id`… Past turns
  before `message_id` are preserved as history" — is unimplemented on
  both counts. Recorded as a known gap rather than fixed here:
  implementing replace semantics means truncating the durable surface,
  which is a controller-level design change, and the route has no
  caller (the frontend never invokes it).

### Fixed (R71 — multimodal payloads survive to the model)

Three holes flattened a binary payload into a string placeholder
before the provider saw it:

- **`synthia-mcp`: an MCP `tools/call` image/audio reply was dropped.**
  `McpTool::call` did `ToolOutput::text(result.text())`, and `text()`
  renders every non-text block as `[image content]`, so a server's
  screenshot / chart / OCR reply lost its base64 payload.
  `McpCallResult::parts()` now projects each block onto the provider
  vocabulary (`Text` / `Image` / `Audio`, with an unmodelled binary
  kind reporting its mime and size), and `McpTool::call` returns
  `ToolOutput::from_parts(…)` when anything non-text is present.
- **`synthia-tool`: no multipart `ToolOutput` constructor.** Added
  `ToolOutput::from_parts(Vec<ContentPart>)` — the multimodal
  counterpart of `ToolOutput::text`.
- **`synthia-provider` (Anthropic): a tool-result image was
  Debug-formatted.** `AnthropicToolResultContent` was a text-only
  struct, so `transform_part` emitted `format!("{:?}", …)`. It is now
  a `text | image` tagged enum and `tool_result_blocks` maps each inner
  part (an error result keeps its `Error: {:?}` block). The OpenAI
  adapter already handled this via `extract_media_parts`.

Tests: `synthia-mcp` 56 (4 new), `synthia-agent --lib` 316 (1 new
end-to-end), `synthia-provider --lib` 690 (3 new). `make ci` 6/6.

### Fixed (R70 — R63 server-side wiring completion)

The R63 parallel session added `crates/synthia-server/src/routes/session_search.rs`
and the matching test file but never wired them into the server: the route
referenced `state.session_search`, a field that did not exist on `AppState`.
Closed the gap:

- `crates/synthia-server/src/state/app_state.rs` — added the
  `session_search: SharedSessionSearch` field, initialized it from
  `workspace_root.join("sessions")` (the same subdirectory the
  `SessionRegistry` writes into), and added the missing `tool_registry`
  field to `for_test`'s struct literal.
- `crates/synthia-server/src/routes/mod.rs` — registered `pub mod
  session_search;` and restored the pre-existing `pub mod skills;` and
  `pub mod tool;` declarations that were tracked files but missing from
  the module table.
- `crates/synthia-server/src/server/router.rs` — mounted
  `GET /api/v1/sessions/search` on the api_routes builder.
- `crates/synthia-server/src/routes/session_search.rs` — added the
  required `state.session_search.sync().await` call before the search.

Verification: `cargo test -p synthia-server --test session_search_test`
4/4 pass (was 2/4 before); `cargo run -p synthia-session --example
session_search` prints `SESSION-SEARCH: OK`; `make ci` 6/6 green.
Commits: `7f63ba2` (register the search module in
`synthia-session::lib`), `2a6185e` (track the example),
`c7e6e7d` (complete the server wiring).

Report: this turn's commit `a3004ae`.

`synthia_test_support::FakeProvider::text(text: &str)` — a
one-line constructor that returns a provider answering every
`complete()` call with `Content::text(text)`. Mirrors traitclaw's
`MockProvider::text()`. A consumer test that previously had to
write a full `CompletionResponse { content: Content::text(...),
..Default::default() }` for the common case of one canned text
reply can now write `FakeProvider::text("answer")`.

Verification: `cargo test -p synthia-test-support` 17/17;
`make ci` green. The traitclaw `MockProvider::error()` port was
considered and declined — it requires adding a struct field for one
constructor's error path, deferred to the first consumer that
needs provider-error testing.

### Changed (R68 — scheduler-store deterministic tests + drop baseline 9 → 6)

Report: this turn's commit `2242e8a`.

Two `synthia_scheduler::store` tests pinned:

- `gc_completed_drops_old_completed_rows` (the cutoff-vs-`last_fired_at`
  relative-time check)
- `sample_job` (the helper used by every store test, so the next-fire
  time is deterministic too)

`make clock-audit` 9 → 6 and `CLOCK_BASELINE` follows. `cargo
test -p synthia-scheduler` 14/14. The remaining 6 `Utc::now()`
calls in `scheduler.rs` are intentional wall-clock semantics —
they test 'after N seconds the interval rearms' and 'next_deadline
returns zero for imminent', which pinning would break.

### Changed (R67 — five more deterministic tests + drop clock baseline 14 → 9)

Report: this turn's commit `0dc06d0`.

R65/R66 pattern applied to 5 more test sites that used `Utc::now()`
without a clock-dependent assertion:

- `synthia_server::routes::attachment::create_response_flattens_ref_fields`
- `synthia_attachment::filesystem_backend_reports_not_found_for_missing_hash`
- 3 sites in `synthia_eval::export::tests` (CSV/JSON export helpers)

All pin to `2026-01-01T12:00:00Z`. `make clock-audit` 14 → 9 and
`CLOCK_BASELINE` follows. `make ci` green; 30/30 eval tests,
72/72 server tests pass.

### Changed (R66 — three more deterministic tests + drop clock baseline 17 → 14)

Report: this turn's commit `ff74561`.

Following the R65 pattern, three more test sites that used
`chrono::Utc::now()` without a clock-dependent assertion now pin to
`2026-01-01T12:00:00Z`:

- `synthia_session::operation::tests::snapshot_builder_records_compaction`
  records a `last_compaction` timestamp but only asserts
  `is_some()` and `seq == 7`.
- `synthia_eval::tests::report_summary_shows_pass_rate_and_average` and
  `report_summary_empty_suite_avoids_division_by_zero` record
  `generated_at` on an `EvalReport` but `summary()` does not include
  the timestamp in its output.

`make clock-audit` count 17 → 14; `Makefile` `CLOCK_BASELINE` 17 → 14
per AGENTS.md §3.8. `make ci` green; `cargo test -p synthia-eval` 30/30;
`cargo test -p synthia-session` 145/145.

### Changed (R65 — deterministic test + dropped clock baseline)

Report: this turn's commit `7cd50666`.

- **`runtime_context_from_runtime_populates_required_fields`** in
  `crates/synthia-agent/src/prompt/mod.rs` no longer reads
  `chrono::Utc::now()` — the test had no clock-dependent assertion
  (every field is checked for *non-empty*), so the real-clock
  dependency was gratuitous noise that risked spurious failures
  across a midnight boundary. Pinned to `2026-01-01T12:00:00Z`,
  the same fixed instant the sibling test
  `runtime_context_from_runtime_uses_injected_clock` uses for its
  clock-injection contract.
- **`Makefile` `CLOCK_BASELINE := 18` → `17`** per
  `AGENTS.md` §3.8 ("lower the baseline whenever the audit
  shrinks"). `make check-clock` still passes (17 ≤ 17); the gate
  continues to fail the moment a production call site appears.

### Changed (R64 — ergonomics-after-parity: five small traitclaw/pi ports)

Report: [`docs/optimization-report-R64-2026-09-14.md`](docs/optimization-report-R64-2026-09-14.md).

R62's parity audit closed the big-port loop (R14–R34 absorbed
traitclaw/dsh/pi/pi-subagents). This round is the **ergonomics** round:
five small ports and two documentation fixes that were below the
big-port threshold but real, each answering a question the parity
audit surfaced.

- **`synthia-agent::agent::strategy::default_for_test(provider, cancel)`**
  (R1, `b1c265f8`) — traitclaw's `make_runtime` factory adapted to
  where `AgentRuntime` lives in Synthia. Three strategy test modules
  (`strategy.rs`, `cot.rs`, `best_of_n.rs`) each built the same
  22-field struct literal inline; the factory collapses 65 lines of
  boilerplate. `pub(crate)` + `#[cfg(test)]` keep it out of the
  public surface.
- **`AGENTS.md` §3.7** documents `synthia-core::registry::Registry<T>`
  + `paginate_registry_list` (R2, `f4f08bb3`) — the spine of every
  "named catalog" in the workspace — and why `ToolRegistry` uses a
  private `mod registry_trait` submodule so `ToolFilter` /
  `ToolEntry` don't leak. The pattern was discoverable only via grep.
- **`AGENTS.md` §3.7** adds a 3-question rubric for "should we drop
  this crate?" reviews (R3, `08403509`). All 5 audited optional
  crates (`synthia-{scheduler,workflow,rag,skill,mcp}`) answered
  yes to all three (194 tests across them, all green).
- **fmt-fix** the R1 factory call sites (R4, `aee87425`) so
  `make fmt-check` is clean.
- **`FakeProvider::tool_then_text<S, V>(tool_name, tool_input, text)`**
  (R5, `5db19b52`) — ports traitclaw's `MockProvider::tool_then_text`.
  Refactors the `tool_call_then_final_answer` integration test from
  14 lines of `Content::parts` boilerplate to 5 lines of intent.

### Added (R63 — session search: the one gap the parity audit found)

Report: [`docs/reference-parity.md`](docs/reference-parity.md) row R63;
the standalone report file (`optimization-report-R63-2026-09-12.md`)
is tracked in a separate working copy.

pi ships `SessionSearchService`: search the agent's *own* history, not the
model's knowledge. R62's parity audit found it missing here, which matters
for a deployment — a user who cannot find last week's conversation does not
trust the store.

- **`synthia_session::SessionSearch`** — the seam. Four methods
  (`search_sessions`, `search_entries`, `sync`, `remove`), runtime-neutral,
  so an index can live in memory, in `SQLite`, or behind a service. pi's
  `notify`/`close` have no counterpart on purpose: `sync` is cheap enough
  to call on read, and `Drop` is the close.
- **`JsonlSessionSearch`** — the shipped implementation over the JSONL
  layout this crate already writes. It folds each log through
  `fold_log_surface`, so a query matches *what the model saw* (compaction
  replacements included) rather than raw rows, and it ignores tool
  **results** — indexing machine output would let a query match whatever a
  session happened to `cat`.
- Matching is whole-word and case-insensitive (`tokens` does not answer a
  query for `token`), ranking is term coverage with a length penalty
  (documented in the module), and the index re-reads only logs whose size
  changed. `remove` is a **tombstone**: a forgotten session stays
  unsearchable across a later `sync`, which is what "deleted but not yet
  erased" needs.
- **`GET /api/v1/sessions/search?q=&limit=&session_id=`** — the HTTP face,
  on the v1 `List<T>` envelope, backed by the same index a Rust consumer
  builds. The literal route wins over `/sessions/{id}` (pinned by test).
- **`cargo run -p synthia-session --example session_search`** — the
  exported `SESSION-SEARCH: OK` proof, showing the ranking, the snippet
  and the tombstone.

### Fixed (R62 — no runtime type in a library's public API, and the reference map)

Report: [`docs/optimization-report-R62-2026-09-12.md`](docs/optimization-report-R62-2026-09-12.md);
current map: [`docs/reference-parity.md`](docs/reference-parity.md).

- **`synthia-tool`'s `CleanupTask` held a `tokio::task::JoinHandle`** in a
  public struct, while its own doc claimed "no `tokio` type appears in
  this crate's public API". It now holds a **stop callback**
  (`Box<dyn Fn() + Send + Sync>`, invoked at most once) with
  `CleanupTask::from_stop` as the seam for a consumer running its own
  cleanup loop on another executor, and a manual `Debug`. The
  `start_cleanup_task` constructor is still the tokio-bound plugin — the
  runtime type stops at the crate boundary.
- **`make check-public-api-runtime`** fails the build if a public item in
  any library crate names `tokio`/`async_std`/`smol`/`futures::executor`
  (comments stripped, `synthia-server` exempt as the application crate).
  It is part of `make ci`, and it has teeth: pointed at R61's source it
  reports the `JoinHandle` line.
- **`docs/reference-parity.md`** — the objective's first axis, made
  checkable: every traitclaw crate and pi package mapped to its synthia
  counterpart (verified against the source), the declines with their
  reasons (TUI, bundled coding agent, full MCTS, CBOR, chord/facets,
  builtin runtime seams), and the one verified open gap: pi's session
  `search` service.

### Fixed (R61 — the start-here tutorial is compiled, and it was broken)

Report: [`docs/optimization-report-R61-2026-09-12.md`](docs/optimization-report-R61-2026-09-12.md).

`MINIMAL.md` is the document a new consumer reads first. Nothing had ever
compiled it, and when a doctest gate was finally pointed at it:

- **The Step-1 code fence was never closed.** It opened at line 57 and
  the next ```` ``` ```` only appeared at line 135 — so the rendered page
  showed one giant code block containing the "Step 2" heading, its prose,
  and the second fence marker. The guide's two code steps were unusable
  as written.
- `Content::text(self.answer.into())` did not compile (ambiguous
  `Into`), and Step 4's "when you outgrow the prelude" table pointed at
  `synthia::session::JsonlSessionSink` — a path that does not exist (it
  is `synthia::session::jsonl::JsonlSessionSink`).
- The one path that *was* right exposed a facade wart:
  `SubagentPool` (R31) was only reachable as
  `synthia::agent::agent::pool::SubagentPool`; it is now re-exported at
  the agent crate root, so `synthia::agent::SubagentPool` is the name.

**The gate**: `crates/synthia/src/lib.rs` carries
`#[cfg(doctest)] #[doc = include_str!("../../../MINIMAL.md")]`, which
turns the guide's `rust` fences into doctests. CI already runs
`cargo test -p synthia --doc` twice (default features, then the
seven-feature MVP subset), so the guide's central claim — "these seven
features are enough" — is now *compiled* rather than asserted. Step 2's
fence became a function taking the provider, so it reads as the assembly
and compiles without a scripted provider inline.

### Changed (R60 — the two oldest performance wins are enforceable too)

Report: [`docs/optimization-report-R60-2026-09-12.md`](docs/optimization-report-R60-2026-09-12.md).

R52 built a ratio gate for the catalog memos, which could be compared
against the expression they replaced because that expression is still in
the tree. R42's ASCII fast path and R48's token-unit accumulator replaced
code that is *gone*, so their claims stayed untested prose in two reports.

- **Faithful copies of the replaced algorithms now live in the benchmark**
  (`slow_estimate_token_count` — the per-character CJK walk;
  `slow_estimate_messages_token_count` — one `String` per message) and
  `--check` measures each against the current code in one process:
  `estimate_token_count` (ASCII) **133×** against a 50× floor, and
  `estimate_messages_token_count` **2.0×** against a 1.3× floor.
- The message pair calls the *current* `estimate_token_count` inside the
  old message-level loop on purpose, so it isolates R48's change instead
  of re-measuring R42's fast path through it.
- Floors are set from measurement: dropping either optimisation lands at
  1.0×, far below the floor, while the observed factors (133×; 1.9–6.3×)
  stay clear of it even on a busy machine — a gate that fails on noise
  gets ignored, which is worse than no gate.

### Performance (R59 — the tool projection is computed once per run, not once per iteration)

Report: [`docs/optimization-report-R59-2026-09-12.md`](docs/optimization-report-R59-2026-09-12.md).

A tool-using turn asks the loop for the model-facing tool list once per
iteration, and the answer costs ~21.5 µs at 40 tools (the largest single
item on the per-iteration path, measured since R42). Between iterations
almost nothing about it changes: the registry is version-keyed, the
surface and the agent's restriction are fixed when the loop is built, and
the only moving input is the set of already-called `Deferred` tools.

- **The loop memoises the projection** (`ToolProjectionMemo`, keyed by
  registry version + the sorted promoted-tool set) and returns the
  shared `Arc<Vec<ToolDefinition>>`. Since `CompletionRequest::tools` is
  already an `Arc`, a hit is a refcount bump instead of a deep clone of
  every schema — and instead of recomputing the projection.
- **Interleaved A/B on one machine** (the bench tree at HEAD vs this
  one, two pairs, minutes apart): the scripted one-tool-call turn went
  `360.9 µs → 319.1 µs` and `274.1 µs → 238.2 µs` — one full projection
  saved per turn, ~12 % of the turn.
- **Two tests pin the contract**, asserted through the provider's
  captured `Arc`s (what the wire actually carries): an unchanged tool set
  reuses the same allocation across iterations, and a `Deferred`
  promotion yields a *new* list (the real schema instead of the
  placeholder), so the memo cannot serve a stale projection.
- The R43 short-circuit that skips the transcript scan when no `Deferred`
  tool exists moved out of the projection into `promoted_tool_names`, so
  the path that *keys* the memo pays nothing for it either.

### Added (R58 — the last inert config keys are real, and enforced)

Report: [`docs/optimization-report-R58-2026-09-12.md`](docs/optimization-report-R58-2026-09-12.md).

R55 made the inert keys *loud*; this round makes two of them work.
`agents.<name>.allowed_tools` / `denied_tools` were the pair with a
safety edge — an operator who wrote `denied_tools = ["shell"]` had
restricted nothing.

- **`ToolRestriction` is applied at both ends.** The excluded tools are
  not advertised to the model, **and** a call to one is refused before
  dispatch with a model-facing error naming the restriction, surfaced as
  a `Guard` warning event while the run continues — the same shape as a
  steering guard denial. That is what makes the key a control rather
  than a suggestion; the previous state (a visibility-only filter) would
  have let a determined model call the tool anyway, which is why R55
  declined to ship it.
- **`ReActAgent::with_tool_restriction` + `AgentBuilder::tool_restriction`**
  — the library seam, for a consumer scoping one agent without any
  config file. It is distinct from `with_tool_surface`: that is the
  deployment's grouping/cap policy, this is the agent's own scope.
- **The synthetic `task` tool follows the rule**: a restriction that does
  not permit `task` turns delegation off, so it cannot slip past a list
  meant to narrow the agent.
- **`agents.<name>.{allowed_tools,denied_tools}`** resolve once at boot
  (`load_default_tool_restriction`) and ride the same two hops as the
  strategy: `RunDependencies.with_optional_tool_restriction` →
  `AgentRunConfig.tool_restriction` → the run factory and the registered
  agents. Empty lists stay allocation-free (no restriction object, the
  pre-R58 path).
- The R55 audit now reports only the four keys that are still inert
  (`description`, `model`, `hidden`, `color`), and `DEPLOYMENT.md`'s
  table lists five applied keys with the enforcement contract spelled
  out.

### Fixed (R57 — the frontend passes the gates AGENTS.md §4 already required)

Report: [`docs/optimization-report-R57-2026-09-12.md`](docs/optimization-report-R57-2026-09-12.md).

`AGENTS.md` §4 asks the web frontend for 0 lint, formatting, a build,
tests, and UI tests. Nothing enforced the first three: CI ran the
Playwright `e2e` workflow (server + secrets + browsers) and a Rust-only
quality workflow, so `tsc`, `eslint`, and `prettier` were never run on a
pull request.

- **Four source files were unformatted** (`src/App.tsx`,
  `src/api/client.ts`, `scripts/e2e/verify_ui.mjs`,
  `tests/e2e/unit/session-to-messages.spec.ts`) — `npx prettier --check .`
  is clean now. `make fmt-web` also formats the *whole* tree instead of
  only `src/**`, so "formatted" means the same thing to the tool and to
  `AGENTS.md`.
- **`.prettierignore` actually ignores lockfiles**: the existing `*.lock`
  pattern never matched `pnpm-lock.yaml`, so every `--check` run reported
  it. `pnpm-lock.yaml` is listed explicitly.
- **`make check-web`** (tsc → eslint → prettier, no browser needed) and a
  **`web` job** in `.github/workflows/rust-quality.yml` that runs it after
  `npm ci`. The Playwright suite stays in its own workflow, where the
  server and secrets already live.
- `npm run build` (tsc + vite) still succeeds, and the two source files'
  diffs are pure line-joining — no behaviour change.

### Docs (R56 — a map for the docs, crate docs for the crates that had none, and a rustdoc gate)

Report: [`docs/optimization-report-R56-2026-09-12.md`](docs/optimization-report-R56-2026-09-12.md).

The repo's premise is that its documentation *is* the assembly tutorial.
Two holes in that premise: there was no index over 47 files in `docs/`,
and three crates had **no crate-level documentation at all** — including
`synthia-tool` (one of the seven core pieces) and
`synthia-test-support` (the crate a consumer adds to test their own
assembly).

- **`docs/README.md`** — the reading order for a new consumer
  (`MINIMAL.md` → `SEAMS.md` → examples → README → `DEPLOYMENT.md`),
  what each directory holds, every optimization round grouped by theme
  with a pointer to its report, and a **known-gaps table** consolidating
  the deferrals that until now lived only in the tail of the 19th-most-
  recent report. Linked from the root README.
- **Crate docs for `synthia-tool`, `synthia-server`,
  `synthia-test-support`** — each with what the crate is, what is
  deliberately *not* in it, and a compiling example (the tool one
  implements a `Tool`, registers it, and asserts it reaches the model's
  advertised schema; the server one describes the boot and the route
  surfaces; the test-support one drives `FakeProvider`).
- `synthia-tool`'s docs state the crate's load-bearing decision up
  front: **registered**, **shown**, and **runnable** are three different
  questions with three different mechanisms.
- **74 rustdoc warnings fixed, and gated.** `make doc-check`
  (`RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace`) is part
  of `make ci` now. The warnings were not cosmetic: `synthia_core`'s
  error docs described a wire-code method that does not exist,
  `hook_map.rs` told consumers to attach a `HookMap` via a
  `Steering::with_hook_map` that does not exist (and claimed the loop
  consults the map — it reads `Steering::hooks` instead), `router.rs`
  named a `SequentialRouter` twice that was never written, and
  `synthia-tool` documented three registry methods under names they no
  longer have. All corrected to the real API; the stale sentences were
  rewritten rather than unlinked.

### Changed (R55 — no config key is silently ignored)

Report: [`docs/optimization-report-R55-2026-09-12.md`](docs/optimization-report-R55-2026-09-12.md).

R53 and R54 fixed knobs that were read from the wrong place and knobs
that were read by nobody. One surface was left: `agents.<name>` accepts
six keys the boot parses and never applies — `description`, `model`,
`allowed_tools`, `denied_tools`, `hidden`, `color`. Silence there is a
safety problem, not just untidiness: an operator who writes
`denied_tools = ["shell"]` has restricted nothing and has no way to find
out.

- **A boot audit names them**, once per configured agent:

  ```text
  WARN synthia.agent: agent config keys are parsed but not applied; what
  a run may see and call comes from [tools] (groups / hidden / deferred)
  and the registry's privacy flag — see DEPLOYMENT.md
  agent=reviewer keys=["description", "model", "allowed_tools",
  "denied_tools", "hidden", "color"]
  ```

  The classification lives in one testable function
  (`unapplied_agent_keys`), and a config that uses only the three keys
  the boot applies (`max_steps`, `compaction`, `strategy`) produces no
  warning at all — a warning on every healthy deployment is noise that
  gets tuned out.
- **`DEPLOYMENT.md` states what `agents.<name>` applies** (a three-row
  table) and points at the mechanisms that *are* enforced: the `[tools]`
  surface (`hidden` refuses dispatch, not just the advertisement) and,
  for a library consumer, `synthia_tool::RestrictedRegistry`
  (`ToolRestriction { allow, deny }`, visibility-only by design).

### Fixed (R54 — the address and the caps the deployment asks for)

Report: [`docs/optimization-report-R54-2026-09-12.md`](docs/optimization-report-R54-2026-09-12.md).

`SYNTHIA_HOST` and `SYNTHIA_PORT` were set by `Dockerfile.server` and
both compose files, documented in `DEPLOYMENT.md` — and read by nothing.
The binary took only `--host`/`--port`, so a container (whose `CMD` is a
bare `synthia-server`) bound `127.0.0.1:8080`: **a published port that
could not be reached**. `host` / `port` in the deployment config were
read by nobody either, and `max_agents` capped nothing.

- **The address resolves from every source a deployment has**:
  `--host`/`--port`, then `SYNTHIA_HOST`/`SYNTHIA_PORT` (clap's `env`
  fills the same fields the flags do), then the config's `host`/`port`,
  then the defaults. Each part resolves independently, so
  `--port 7000` with `host = "0.0.0.0"` in the config means exactly
  that. `BindAddress::resolve` is the one place it is decided; the
  server logs the resolved address before binding, and `boot_server`
  returns it so the log, the bind and the announced probe URLs are one
  value.
- **`max_agents` is enforced.** `POST /api/v1/agents` is refused with
  the limit and the knob in the message once the registry holds that
  many — an open deployment can no longer grow its agent set (and every
  session's handoff manifest) without bound. Re-registering an existing
  name stays allowed at the cap, since that replaces rather than adds.
- **The config sections that did nothing are gone.** `rate_limit`
  (`enabled` / `requests_per_minute` / `burst`) and `skills` were parsed
  and never read; an operator enabling limiting was protected by
  nothing. Limiting belongs at the proxy (`DEPLOYMENT.md` says so now);
  skills are discovered from `<workspace>/.agents/skills/`. `skill`
  also drops the vestigial `model_override`. Old files keep working —
  `serde` ignores the keys.
- `BootedServer` + `boot_server` replace `create_server`: the boot
  resolves the deployment config **once**, computes the address, builds
  the state with that config, and returns the router plus the address
  (R53's "one resolution point" extended to the one value that needs it
  before any state exists).

### Fixed (R53 — the config file the operator names is the config file the server reads)

Report: [`docs/optimization-report-R53-2026-09-12.md`](docs/optimization-report-R53-2026-09-12.md).

`--config <path>` configured **only the provider bridge**. Every other
section read at boot — `agents` (`max_steps` / `compaction` /
`strategy`), `[tools]`, `auth`, `cors`, `operations`, `retrieval`,
`mcp_servers` — came from `<workspace>/config.toml` regardless, so
`make dev-server` (which passes `--config config.yaml`) silently dropped
half of the file it was handed. Nothing failed; the settings simply did
not apply.

- **One resolution point**: `resolve_server_config(workspace_root,
  config_path)` — the `--config` path when it exists, then
  `config.toml`, then `.synthia/config.toml`, format chosen by
  extension. `AppState::new` resolves it once and every section reads
  that value, so the named file is the deployment's configuration. A
  candidate that is missing or does not parse is logged and skipped
  (the next is tried, then defaults), matching the provider bridge's
  existing fail-soft contract.
- **The repeated parse is gone**: ten helpers each re-read and
  re-parsed the same file at boot; they now take the resolved
  `ServerConfig`, and the trivial ones (`auth`, `cors`, `operations`,
  `default_agent`, `tools`) were deleted in favour of reading the field
  at the call site.
- **`--config` is documented for what it does** (every boot section,
  three formats), and the fallback chain is spelled out.
- Proven end-to-end: booting the real server with
  `--directory <ws> --config <tmp>/cfg.yaml` logs
  `loaded server config path=.../cfg.yaml` and
  `configured reasoning strategy agent=agent strategy="best-of-n"` from
  a YAML `agents.agent.strategy` — a setting that could not reach the
  run factory before this round. Two tests pin it: the named file wins
  over a conflicting `config.toml`, and a broken candidate falls
  through instead of ending the boot.

### Changed (R52 — the performance claims are enforced, not just measured)

Report: [`docs/optimization-report-R52-2026-09-12.md`](docs/optimization-report-R52-2026-09-12.md).

R42–R51 made the framework's own overhead measurable and then smaller;
nothing stopped it from coming back, because the harness only *printed*
numbers and absolute timings do not survive a shared CI runner.

- **`hot_paths -- --check` / `make bench-check`** — the harness gained a
  second mode that enforces the **ratios** instead of printing the table.
  Each invariant pairs a memoised path with the exact expression it
  replaced and measures both **in one process**, so the factor is
  machine-independent: `descriptors_cached` vs `descriptors`,
  `snapshot_cached` vs `snapshot`, and the tier/group wrappers against
  their uncached scans. Measured today: 1570×, 429×, 69×, 3.6× against
  floors of 100×, 100×, 10×, 2×.
- **A fourth CI job (`bench`)** runs `make bench-check`, so a cache that
  stops hitting or a scan that gets reintroduced fails the build with a
  message naming the invariant. The fast `make ci` gate is unchanged —
  the ratio check needs a release build, so it is its own job.
- The floors carry headroom on purpose (the smallest margin, the group
  wrapper, sits at 3.6× against a 2.0× floor): a regression that removes
  a memo lands near 1×, far below any of them, while runner noise cannot
  reach them.

### Performance (R51 — the visible catalog is a memo, not a scan)

Results, measurements and the in-run baselines:
[`docs/optimization-report-R51-2026-09-12.md`](docs/optimization-report-R51-2026-09-12.md).

Every consumer that answers "what should the model be told about?"
started from `ToolRegistry::snapshot()` — which clones two `String`s per
tool and sorts, for the *whole* registry, and was then filtered down.
The tier cap was the worst case: a five-tool small-model deployment paid
for forty descriptions to keep five names.

- **`ToolRegistry::snapshot_cached()`** — the catalog behind the same
  version-keyed memo as `descriptors_cached` (R43). Registration,
  removal and `set_hidden` all bump the version, so a stale catalog
  cannot reach the model; an unchanged registry hands back the shared
  `Arc` (measured 3.90 µs → 8.0 ns, 490×). `snapshot()` stays for
  callers that want a fresh owned `Vec`.
- **`AdaptiveRegistry` and `GroupedRegistry` (and
  `RestrictedRegistry::visible_names`) clone only what they keep** —
  the tier cap now clones `max_visible_tools` names instead of every
  tool's name and description, and the group filter clones only the
  advertised names. Same run, same machine: the tier path
  **3.84 µs → 56 ns (68×)**, the grouped path **5.15 µs → 1.49 µs
  (3.5×)**.
- The benchmark harness gained the catalog entries plus an **in-run
  baseline** for each consumer (the pre-memo expression, measured in
  the same process as the memoised one), so these numbers do not depend
  on comparing across machine states.
- Two tests pin the memo's contract: it reuses the allocation until the
  registry changes, and it follows `set_hidden` in both directions.

### Added (R50 — the third paradigm, and the seam's deployment hop)

Results and evidence:
[`docs/optimization-report-R50-2026-09-12.md`](docs/optimization-report-R50-2026-09-12.md).

R49 made the loop a part. R50 makes it a *useful* one — a second
shipped paradigm that uses the runtime's concurrency, and a config knob
so the choice is not compiled in.

- **`BestOfNStrategy` + `CandidateScorer` + `LongestAnswer`**
  (`crates/synthia-agent/src/agent/best_of_n.rs`) — sample N answers,
  score them, publish the winner. Standard practice (self-consistency,
  green-patch selection, structured extraction) at a known multiple of
  one answer's cost.
  - **Concurrency through the seam, not around it**: candidates are
    fanned out with `AgentRuntime::spawner` — the deployment's executor
    — and collected over `futures` channels, so no runtime type appears
    in the strategy and a consumer on another runtime gets its own
    executor used. `ReActStrategy` and `ChainOfThoughtStrategy` are
    sequential; this one is the reference for a strategy that spawns.
  - **Alternatives stay off-stream**: only the winner is published as
    the answer (`Model` + `ModelDone`), with one `Progress` event per
    candidate and a selection event naming the winner's index and
    score. A chat UI never sees five answers.
  - **The scorer is the caller's**: any `CandidateScorer` (length,
    similarity, a test run, an LLM judge). Ties go to the lowest index,
    so a script produces the same winner twice; empty answers are
    failures, not candidates; a pre-cancelled run issues no requests,
    and cancellation reaches in-flight samples through
    `complete_with_stream`.
- **`strategy::from_name` + `KNOWN_STRATEGY_NAMES`** — resolve a
  strategy from the string a deployment writes, case-insensitively and
  with the obvious aliases (`cot`, `bon`). An unknown name is a typed
  `Error::Validation` naming every accepted value.
- **`AgentRunConfig.strategy` + `RunDependencies::with_optional_strategy`
  + `agents.<name>.strategy`** — the deployment hop: `synthia-server`
  resolves the config string **once at boot**
  (`AppState::default_strategy`, via `load_default_strategy`), threads
  it through the controller into the run factory, and installs it on
  both the session runs and the registered agents, so the two agree.
  An unknown name is a boot warning plus a ReAct fallback — a typo in
  one agent's table must not stop the server from starting.
- **`synthia_provider::completion_to_sampling`** is now reachable from
  the provider root (it was `traits::completion_to_sampling`), so a
  consumer converting a `CompletionResponse` into an `AgentEvent`'s
  payload does not have to know the module layout.
- **`cargo run --example strategy_swap -p synthia-agent` now covers
  three paradigms** and asserts all three from the provider's recorded
  requests: ReAct advertises a tool and runs it once, Chain-of-Thought
  advertises none, best-of-N samples three times with a custom scorer
  and publishes the candidate that carries the required marker.

### Added (R49 — the reasoning loop is a part, not a fact)

Results and evidence:
[`docs/optimization-report-R49-2026-09-12.md`](docs/optimization-report-R49-2026-09-12.md).

Every other capability in this framework is a swappable trait —
provider, tools, context manager, steering, sink, cancel token,
executor, retriever, transport. The *loop itself* was not: `ReActAgent`
owned the reasoning and the plumbing in one 2 600-line file, so a
different paradigm meant reimplementing the plumbing.

- **`ReasoningStrategy` + `AgentRuntime` + `EventSink`**
  (`crates/synthia-agent/src/agent/strategy.rs`) — the loop behind the
  same kind of seam as everything else. A strategy receives a runtime
  holding every assembled piece (provider, registry, surface policy,
  workspace, descriptor, prompt context, context manager, peer
  registry, steering, max iterations, typed sink, inbox, clock,
  spawner, cancel token, depth) and publishes `AgentEvent`s through a
  cloneable sink. `AgentBuilder::strategy(Arc::new(..))` installs one;
  `ReActStrategy` is the default and the reference implementation.
- **One method, streaming-first.** Reference designs expose `execute()`
  plus an optional `stream()` that defaults to an error, which makes
  streaming second-class. Here the event stream **is** the contract and
  errors travel as `SystemEvent::SessionEnded { reason }`, so every
  strategy streams and `Agent::run` is a three-line adapter.
- **A second shipped paradigm: `ChainOfThoughtStrategy`** — one
  streaming completion, no tools advertised, a step-bounded
  step-by-step instruction, the runtime's context manager honoured, and
  `parse_steps()` to recover the labelled chain from the answer. It is
  the proof that the seam is real: a whole paradigm with no change to
  the agent.
- **`cargo run --example strategy_swap -p synthia-agent`** assembles
  both through one `AgentBuilder` and asserts the difference from the
  provider's own recorded requests: ReAct advertises 1 tool and runs it
  once, Chain-of-Thought advertises 0 and runs none. Prints
  `STRATEGY-SWAP: OK`.
- **The clock gate counts code only.** The ratchet was counting the
  literal string inside doc comments (including the docs that forbid
  it), so a new comment could "fail" the gate; comment lines are now
  excluded, the baseline is the 18 in-crate test uses, and
  `make clock-audit` prints only real call sites.

### Performance (R48 — the turn is now 2× R42's baseline)

Results and evidence:
[`docs/optimization-report-R48-2026-09-12.md`](docs/optimization-report-R48-2026-09-12.md).

R42's harness measured `estimate_messages_token_count` at 197 µs for a
100-message history and every round since has paid it once per
iteration (the context-window check) plus once per dropped pair. The
cost was one `String` per message: the estimator concatenated every
part of a message before measuring it.

- **`synthia_core::token::TokenUnits`** — the heuristic split in two:
  count the units a text contributes (ASCII bytes, CJK chars, no
  allocation), then convert. `estimate_token_count` is now
  `TokenUnits::of(text).tokens()`; a caller measuring several parts as
  one unit accumulates the counts and converts once. Identical by
  construction — the counts are additive and the conversion is the same
  arithmetic — and a test pins accumulation against the concatenation
  for empty / ASCII / CJK / mixed / emoji inputs.
- **`estimate_messages_token_count` accumulates instead of
  concatenating**: **38 759 ns → 16 407 ns** for 100 messages (2.4×),
  and the context manager's no-eviction fast path **118 439 ns →
  71 152 ns** (1.7×), both min-of-5 on the same harness.
- **End to end: the scripted turn is 170 006 ns**, against 352 158 ns
  when R42 first measured it — **2.07× faster**, from four changes
  (linear truncation, the ASCII fast path, the descriptor memo, the
  unit accumulator) that do not alter a single output byte.

### Docs (R47 — the seam map, and two stale recipes it caught)

`SEAMS.md` (new, linked from the README and `MINIMAL.md`) is the lego
map the project's own purpose implies: every trait a consumer can
substitute, what the workspace ships for it, and the line of code that
installs a replacement — the seven loop pieces, the runtime seams
(`Spawner` / `Clock` / `IdGen`), and every opt-in extension
(memory, retrieval, skills, delegation, inbox, MCP, sandbox, workflow,
eval, scheduler, attachments, full-output store). It names the two
seams that are deliberately crate-internal (`CommandRunner`, the JSONL
reader) so a reader knows the difference between "swap this" and "ask
for a feature".

Writing it surfaced two stale recipes:

- **`MINIMAL.md` told consumers to call `AgentBuilder::skill_registry`,
  which does not exist.** Skills reach an agent as a `PromptContext`
  entry plus the registered `skill` tool; the row now says so.
- **The same table said `.run_inbox(Arc::new(RunInbox::new(..)))`** —
  `RunInbox` is the trait, `MpscInbox` is the concrete type and it is
  built with `MpscInbox::channel()`.

### Docs (R46 — the docs catch up with R37–R45)

R37–R45 changed what the framework compiles, requires, and measures;
four documents were still describing the state before them.

- **`crates/synthia-core/README.md`** rewritten: the feature list
  stopped at `error` / `registry` / `sensitive` / `text` / `token` and
  omitted `cancel`, `spawn`, `clock`, `idgen`, `schema`, and
  `full_output` — i.e. everything R36–R39 added to the crate a consumer
  adds first. It now also states the crate's one rule (no runtime, no
  provider SDK) and shows the `SharedClock` / `_at`-variant discipline.
- **`crates/synthia-agent/README.md`**: cancellation is described as
  the `CancelToken` trait with `AtomicCancelToken` as the default
  (it was documenting `tokio_util::sync::CancellationToken` as *the*
  token), and the runtime contract gets its own section: the
  `Spawner` seam, `AgentBuilder`, and the neutral / tokio-plugin /
  needs-a-reactor table from R39.
- **`DEPLOYMENT.md`**: the test section recommended
  `cargo test --workspace`, the one command AGENTS.md §3.3 forbids and
  the Makefile no longer runs. It now points at `make ci` /
  `make test-crates` / `make test-sqlite` / `make examples` and names
  the CI workflow that runs them.

### Changed (R45 — the wall clock goes through one seam, and a spill file stops colliding)

Results and evidence:
[`docs/optimization-report-R45-2026-09-12.md`](docs/optimization-report-R45-2026-09-12.md).

R36 introduced `synthia_core::Clock` and R40 wired it into the agent.
An audit for AGENTS.md §3.8 ("new code must not call
`chrono::Utc::now()`") found the rule still had production hold-outs —
and that one of them was a real bug.

- **Clock-routed timestamps**: `ImageAttachmentRef::from_bytes_at` +
  `AttachmentStore::with_clock` (the store, not the byte backend, owns
  `saved_at`), `MemoryEntry::at` + `SqliteMemory::with_clock`,
  `EvalRunner::with_clock` for the report's `generated_at`,
  `OperationSnapshot::new_at`, `JournalEntry::{success_at,failure_at}`,
  `parse_retry_after_at`, and `RunDependencies::with_clock` for the
  server's session-log rows, operation snapshots and hash-only
  attachment responses. `AppState` now builds **one** clock and hands
  it to every timestamped component (including the MCP supervisor's
  `tick`), which is what §3.8 asks for.
- **Fixed: managed spill files collided.** `spill_to_file` named the
  file `{session}-{tool}-{%Y%m%d%H%M%S}.txt`, so two spills inside one
  second shared a name and the second overwrote the first — losing the
  full output the truncation marker still promised. The suffix is now a
  `synthia_core::IdGen` ULID (the workspace's id vocabulary, sortable by
  creation, and collision-free).
- **`make check-clock`** (new, and part of `make ci`): a ratchet over
  `chrono::Utc::now()` occurrences in `crates/*/src` — 25 today, all in
  doc comments and `#[cfg(test)]` modules, and the gate fails if the
  count grows. `make clock-audit` prints every occurrence for review.

### Changed (R44 — the server stops re-copying the telemetry crate; the bench stops measuring the scheduler)

Results and evidence:
[`docs/optimization-report-R44-2026-09-12.md`](docs/optimization-report-R44-2026-09-12.md).

- **`synthia-server`'s `middleware/trace_context.rs` now uses
  `synthia_telemetry::propagation`** instead of carrying its own copy of
  the W3C machinery (the propagator install, the idempotence flag, the
  extract/inject shims, and two duplicate context structs). What stays
  server-side is exactly the HTTP layer: the `HeaderMap` adapters, the
  middleware, and minting this hop's trace/span id. The telemetry
  module's doc — which claimed the server "does not parse or format
  `traceparent` itself" — is true again, and the 4 middleware tests
  (round-trip, `tracestate` passthrough, generated-header case) pass
  untouched.
- **The hot-path harness reports a minimum over five passes and drives
  the turn on a pooled executor** (`futures::executor::ThreadPool`, not
  tokio) instead of spawning an OS thread per run. Two consecutive
  single-pass runs had differed by ~30% on every entry — including
  untouched ones — so small deltas were unmeasurable; the min-of-5
  numbers are stable (e.g. `descriptors_cached` 9.6 ns vs `descriptors`
  17 519 ns).
- **`project_tool_definitions` was measured and deliberately left
  alone**: 26.6 µs for 40 tools (~10% of the scripted turn) is not worth
  a stateful projection cache plus an invalidation key for the
  transcript-dependent part — 0.01% of a real turn whose model call is
  hundreds of milliseconds. The numbers and the reasoning are in the
  report and in a comment at the call site.

### Performance (R43 — the turn got 33% faster)

Results and evidence:
[`docs/optimization-report-R43-2026-09-12.md`](docs/optimization-report-R43-2026-09-12.md).

R42 built the harness and fixed two quadratic paths. Extending it with
an end-to-end number made the per-iteration preparation visible: the
loop rebuilt the model-facing tool list from scratch on every pass,
including a **deep clone of every tool's JSON schema**.

- **`ToolRegistry::descriptors_cached()`** — the descriptor list memoised
  by the registry's existing monotonic version counter, so a caller on a
  hot path holds one `Arc` and pays the deep clone only when the catalog
  changes. The agent loop now uses it: **13 299 ns → 8.5 ns** per
  iteration on a 40-tool registry (1560×), and the memo is proven to
  invalidate on register/unregister (including through a clone).
- **The transcript scan is skipped when nothing needs it.**
  `called_tool_names` feeds `Deferred` promotion only, and it is
  O(messages × parts) — it grows with the conversation. With no
  `Deferred` tool in the catalog the loop no longer scans at all.
- **Measured end to end: `ReActAgent` turn (scripted provider, one tool
  call, 40-tool registry) 352 µs → 236 µs (1.5×)**, of which the
  per-iteration tool-list preparation went from ~45 µs to ~21 µs. The
  bench table now leads with that number.

### Performance (R42 — the first numbers, and two quadratic paths they exposed)

Results and evidence:
[`docs/optimization-report-R42-2026-09-12.md`](docs/optimization-report-R42-2026-09-12.md).

The docs made performance claims ("no per-call allocation in the hot
paths", "cheap heuristic") with nothing measuring them. R42 adds a
dependency-free benchmark binary for the work a turn repeats, and it
immediately found two problems worth more than the harness:

- **`crates/synthia-agent/benches/hot_paths.rs`** (`make bench`) — nine
  fixtures covering token accounting, stream assembly, wire repair,
  schema validation, tool-surface projection, context truncation and
  text truncation. `harness = false`, no criterion: the number that
  matters is the shape of the curve.
- **`TruncatingContextManager` was quadratic in the history length.**
  It re-estimated the *whole* tail after every dropped pair, so a
  200-message history that did not fit the window burned **34.2 ms per
  `prepare`** — on the path the loop runs before every model call. The
  running total is now maintained by subtracting what a drop removed
  (the estimator is a sum over messages, so the two are equal by
  construction), and the entry condition keeps the whole-list estimate
  it had: **34.2 ms → 0.37 ms (94×)**.
- **`synthia_core::token::estimate_token_count` decoded every character
  to ask two questions it can usually skip.** Prompt text is
  overwhelmingly ASCII, where every character is one byte and none is
  CJK — `str::is_ascii` (SIMD) answers both in one pass, and the byte
  length *is* `ascii_count`. Non-ASCII still takes the original
  char-by-char loop: **13.2 µs → 59.8 ns (220×)** on 4 KB of ASCII,
  with byte-identical results (the fast path is the same formula with
  `ascii_count = len`, `cjk_count = 0`).
- The estimator's callers moved with it: `estimate_messages_token_count`
  (100 messages) **197 µs → 29 µs (6.7×)**, and the no-eviction context
  fast path **282 µs → 80 µs (3.5×)**.
- Behaviour is unchanged: all 2585 tests pass, including the two that
  pin the truncating manager's hybrid entry condition (a system prompt
  that alone exceeds the window must still let the tail be evicted).

### Added (R41 — the gates run in CI, from one place)

Results and evidence:
[`docs/optimization-report-R41-2026-09-12.md`](docs/optimization-report-R41-2026-09-12.md).

R37–R40 established five invariants (the MVP subset pulls no HTTP
client, `synthia --no-default-features` compiles, the runtime-free
crates stay runtime-free, the facade gates its modules, the examples
keep working) — and nothing ran them automatically. CI covered only the
contract-closure scanner and Playwright.

- **`.github/workflows/rust-quality.yml`** — three jobs:
  `gates` (fmt + clippy + both dependency invariants), `tests` (every
  member one crate at a time, the SQLite tier, the facade's
  feature-gating doc tests in the default *and* the MVP configuration),
  and `examples` (every example plus both standalone consumer crates).
  Each step is a `make` target, so a contributor reproduces CI locally
  with the same command and the gate list lives in one place.
- **Makefile**: `fmt-check`, `ci`, `test-crates`, `test-sqlite`,
  `examples`. `CRATES` is derived from the tree (`$(wildcard
  crates/*)`), so a new workspace member needs no Makefile edit; and
  `cargo test --workspace` appears nowhere, per AGENTS.md §3.3.
- **README**: the Test section now points at those targets, and says
  which checks the CI workflow runs.

### Changed (R40 — every crate declares the runtime it actually needs)

Results and evidence:
[`docs/optimization-report-R40-2026-09-12.md`](docs/optimization-report-R40-2026-09-12.md).

`[workspace.dependencies]` handed every crate `tokio = { features =
["full"] }`. That meant `synthia-session` compiled tokio's `net`,
`process`, `fs`, and `signal` subsystems to move a JSONL file, and the
`Spawner` seam added in R39 sat next to a feature set that still
assumed the runtime owned everything.

- **The workspace entry no longer says `full`.** Each crate declares
  the features its own code uses, with a comment saying why: provider
  `["macros", "time"]` (the `select!` SSE pump and retry backoff),
  session `["rt", "sync", "time", "macros"]`, test-support `["sync"]`
  (its fakes never spawn), tool and agent `["rt", "sync", "time", "fs",
  "process", "io-util", "macros"]`, server the whole application set.
  Test-only users (`context`, `steering`, `skill`, `rag`, `eval`,
  `macros`, `workflow`) moved to dev-dependencies with exactly the
  features their tests need.
- **Two invariants are now enforced by `make`:**
  - `make check-mvp-deps` (extended in R39/R38) — the seven-feature
    subset pulls no HTTP client, exporter, framework, or database, and
    `synthia --no-default-features` compiles.
  - `make check-no-runtime` (new) — `synthia-core`,
    `synthia-scheduler`, `synthia-macros`, `synthia-eval`,
    `synthia-workflow`, and `synthia-telemetry`
    (`--no-default-features`) pull **no tokio** in a lib build. These
    are the pieces whose docs promise the consumer the runtime is
    theirs; the target fails the day one of them inherits tokio again.
- **Measured**: the MVP subset's lib build loses `socket2` and tokio's
  `net` / `signal` / `io-std` / `parking_lot` / `rt-multi-thread`
  features. The crate-count effect is deliberately reported as small
  (**124 → 123**): the builtin `shell` tool's `process` feature implies
  `mio` + `signal-hook-registry`, and a consumer riding the default
  builtin set keeps them until the tools themselves get a runtime seam
  (recorded as R41 work in the report).

### Added (R39 — the loop does not choose your runtime)

Results and evidence:
[`docs/optimization-report-R39-2026-09-12.md`](docs/optimization-report-R39-2026-09-12.md).

Two rounds cut what the MVP *compiles*; this one cut what the loop
*requires*. `Agent::run` detaches one task per turn (a caller that
stops polling the event stream must not cancel the run) and that
detach was a hard `tokio::spawn` — so the framework's headline promise,
"the executor is the consumer's choice", did not hold for the one place
that needs an executor at all.

- **`synthia_core::spawn`** — `Spawner` (plus `BoxFuture` /
  `SharedSpawner`): detached work with no runtime in the signature,
  object-safe, with a default `spawn_blocking`. It compiles with
  **zero** runtime dependencies, and its own test proves it by driving
  a future with `Waker::noop()`.
- **The agent loop uses it**: `ReActAgent::with_spawner` /
  `AgentBuilder::spawner`, defaulting to a tokio-backed spawner (the
  crate already links tokio for its own channels and timers, so the
  default costs nothing).
- **Tool dispatch uses it**: `ToolRegistry::with_spawner` and a
  propagated `Clone`, so `run_stream`'s per-tool tasks go wherever the
  registry was told.
- **Proof, not a promise**:
  `cargo run --example runtime_agnostic -p synthia-agent` →
  `RUNTIME-AGNOSTIC: OK`. A full turn — scripted provider, one tool
  call, 11 events — on a ten-line spawner (`std::thread::spawn` +
  `futures::executor::block_on`), with a plain `fn main` and no
  `#[tokio::main]` anywhere.
- **The boundary is documented** rather than implied (agent crate docs,
  README, `synthia_core::spawn`): the builtin tools (`tokio::fs` /
  `tokio::process`), the provider HTTP adapters, and the JSONL session
  sink are tokio-bound *plugins*; timers (retry backoff, the streaming
  idle watchdog) still need a reactor.
- **Fixed: `synthia --no-default-features` did not compile.** The
  prelude re-exported `crate::core::*` unconditionally while `pub mod
  core` is feature-gated, so the documented "the prelude is empty, not
  broken" claim had never been compiled. Fixed, and
  `make check-mvp-deps` now asserts that configuration alongside the
  dependency check.
- `synthia::prelude` gained `Spawner` / `SharedSpawner`.

### Added (R38 — HTTP is a feature, not a default)

Results and evidence:
[`docs/optimization-report-R38-2026-09-12.md`](docs/optimization-report-R38-2026-09-12.md).

R37 proved the smallest agent compiles and runs; it still linked the
whole HTTP stack. R38 makes the transport opt-in, so a consumer with
their own provider and no outbound fetch compiles **no HTTP client at
all**: no `reqwest`, `hyper`, `rustls`, `h2`, or `tower`.

- **`synthia-provider`'s adapters are features** (`anthropic` /
  `openai`, both on by default). The trait, the wire types, the typed
  profiles, the retry classifier, `json_repair`, and the token counter
  trait compile without either; the adapter-only internals
  (`openai_streaming`, `streaming::anthropic`, `pump_sse`,
  `count_message_tokens`, …) are gated with them.
  `synthia-core/reqwest` — the `From<reqwest::Error>` impl — now follows
  the adapter features instead of being unconditional. Naming an
  adapter a build does not have is a **typed runtime error**
  (`adapter_not_compiled` / `no_adapter_compiled`), not a compile error
  and not a misleading "unsupported provider type"; `config.rs` and
  `profile.rs` share one construction site per adapter
  (`openai_provider` / `anthropic_provider`) so the env-key and
  explicit-key paths cannot drift.
- **`synthia-tool`'s `web_fetch` is a feature** (`web`, on by default).
  Without it, `build_default_tool_registry` registers `read` / `write` /
  `shell` / `TodoWrite` and neither `WebFetchTool` nor `reqwest` exists;
  the registry and render-kind tests now assert *the compiled* builtin
  set in both configurations.
- **The facade splits transport from vocabulary**: `provider` is the
  `ModelProvider` trait and the wire types, `provider-anthropic` /
  `provider-openai` are the adapters, `tool` is the offline builtins and
  `tool-web` is `web_fetch`. The default set enables all of them, so
  `cargo add synthia` is unchanged; the seven-feature subset in
  `MINIMAL.md` now means no HTTP. Three more `compile_fail` doc proofs
  (adapter and `web_fetch` absence) plus three reachability tests pin
  both directions.
- **`make check-mvp-deps` gained the transport names**
  (`reqwest|hyper|rustls|h2|tower|webpki`, prefix-matched) and was
  verified to fail when `tool-web` or `provider-openai` is added to the
  subset.
- **Measured**: the facade's seven-feature tree drops from **158 to 124**
  distinct crates, and `docs/examples/minimal-consumer`'s whole tree
  from **165 to 125**. `prometheus` deliberately stays: it is four net
  crates and it is the tool-instrumentation contract the server's
  `/metrics` route serves.

### Added (R37 — the minimal agent, made provable and lean)

Plan + audit: [`MINIMAL.md`](MINIMAL.md), results and evidence:
[`docs/optimization-report-R37-2026-09-12.md`](docs/optimization-report-R37-2026-09-12.md).

This round attacked the gap between the repo's promise ("use it as a
library, from zero") and what a consumer actually had to compile.

- **`ModelProvider::embed` has a default implementation** — one empty
  vector per input text. Embedding was a required method, so every test
  fake, the replay provider, the stub, and the Anthropic adapter each
  carried a three-line no-op or a hardcoded error for a capability most
  providers do not have. `synthia-rag` is the only consumer and it goes
  through its own `Embedder` trait, so a provider that never drives RAG
  now writes nothing. **Twelve implementations lost their bodies**:
  seven test fixtures in `synthia-agent` (five in
  `agent/re_act/tests.rs`, two in `tests/test_support.rs`), the provider
  stub, the replay provider, the facade assembly example, the server's
  `ToolCapturingProvider`, and the Anthropic adapter's hardcoded error.
  The two fakes whose tests *pinned* the old error/no-op contract were
  re-pointed at the new one (deliberate decisions, not accidental).
- **`docs/examples/minimal-consumer/`** — the smallest complete agent,
  checked in and runnable: **one** dependency (`synthia` with
  `default-features = false`), the seven-feature MVP subset from
  `MINIMAL.md`, a scripted provider that requests a tool, a hand-written
  `Tool`, and an assertion that the tool really ran (2 model calls, 1
  tool call, 12 structural events) → prints `MVP-OK`.
  `MINIMAL.md` (new) is the 4-step guide it executes.
- **`make check-mvp-deps`** — fails the build if the MVP feature subset
  ever pulls `opentelemetry`, `opentelemetry-otlp`, `tonic`, `axum`,
  `rusqlite`, or `sqlx`. The gate was verified to fail when `telemetry`
  is added back to the subset.
- **`synthia-telemetry` is now two switchable halves instead of an
  all-or-nothing dependency**: console/file logging always compiles;
  `metrics` gates the Prometheus vectors; `otlp` gates the OTel
  exporter, W3C propagation, and every `opentelemetry*` + transport
  dependency. `default = ["metrics", "otlp"]` for direct consumers, while
  in-tree members declare what they use — `synthia-tool` takes
  `default-features = false, features = ["metrics"]`, so **an agent
  binary no longer links the OTel SDK, tonic, or the OTLP HTTP
  transport**. `synthia-server` names `["metrics", "otlp"]` explicitly;
  the facade's `telemetry` feature turns on both. The MVP feature subset
  drops from **182 to 158** unique crates. `tracer.rs` was split:
  console/file tracing moved to `console.rs`, the OTLP half stayed in
  `tracer.rs`, and the init result was renamed `TracerInitResult` →
  `TracingInit` (its `Otlp` variant exists only with the feature).
- **Removed dead weight**: `synthia-telemetry` declared `uuid` and
  `tokio` that no line of the crate used; `synthia-core` — whose entire
  pitch is runtime neutrality — hard-depended on `reqwest` (for a single
  `From<reqwest::Error>` impl, now the opt-in `reqwest` feature enabled
  only by `synthia-provider`) and on `tokio` (now a dev-dependency).
- **Fixed: an OTLP-configured process lost every console log.** The OTLP
  registry composed the level filter and the OTel layer but no `fmt`
  layer, so setting `SYNTHIA_OTLP_ENDPOINT` sent spans to the collector
  and nothing to stdout — a boot against an unreachable collector was
  silent. The console layer is now composed alongside them, verified by
  booting the real server with `SYNTHIA_OTLP_ENDPOINT` set and reading
  the `OpenTelemetry OTLP tracing initialized … protocol=Http
  sampler=…` line back from its log.

### Added (R34 — one dependency to assemble, a policy for the tool surface, judged selection, durable checkpoints)

Plan: [`docs/optimization-report-R34-2026-09-12-plan.md`](docs/optimization-report-R34-2026-09-12-plan.md).
Results + evidence: [`docs/optimization-report-R34-2026-09-12.md`](docs/optimization-report-R34-2026-09-12.md).
The round's audit found a durable checkpoint one call short of
working, a tool-surface seam reachable only from code, and no
single-dependency entry point.

- **The `synthia` facade crate** (new, 19th member) — one dependency
  instead of nine. Every piece is a feature-gated re-export
  (`synthia::agent`, `synthia::tool`, …) and `synthia::prelude` is a
  27-name curated subset with each collision decided and documented:
  `Result` is `synthia_core::Result`, `Tool` is the trait (the derive
  macro stays at `synthia::macros::Tool`), `Context` is
  `synthia_tool::Context`, and `SessionEndReason` — two genuinely
  different types — is deliberately excluded. Default features assemble
  a basic agent; `skill`/`attachment`/`mcp`/`rag`/`scheduler`/`eval`/
  `workflow`/`telemetry`/`sqlite`/`server`/`test-support` are opt-in.
  The crate docs are the assembly tutorial and 11 `compile_fail` doc
  tests plus a gating integration test pin the feature matrix. The
  crate holds **no logic**, so it cannot drift behaviourally.
- **`ToolSurfacePolicy`** (`synthia-tool` + agent + server) — the R33
  tool surface, reachable from a deployment: `groups` with an
  `active_groups` set (a member of an inactive group is not advertised,
  and still executes), a deterministic `max_visible` cap, and deferred
  advertisement. `apply` validates every name before writing anything
  (`SurfacePolicyError` for an unknown tool, an empty group name, a
  duplicate or a twice-claimed tool) and `ToolRegistry::{set_exposure,
  set_hidden, exposure}` mutate a registered entry with a fail-visible
  result, refreshing the registry version so the server's definition
  cache invalidates (that cache was a process-global `static`;
  it is now per-`AppState`). A `[tools]` server config section applies
  the policy at boot, where an unknown tool name is a **warning, not a
  startup failure** — an MCP server that failed to register must not
  take the deployment down — with `AppliedToolSurface::skipped`
  recording what was ignored. `ReActAgent::with_tool_surface` /
  `AgentBuilder::tool_surface` / `AgentRunConfig.tool_surface` apply the
  visible-name filter on top of the R33 projection; with no policy the
  definitions stay byte-identical (the R33 regression test was
  extended).
- **Host-side candidate selection for `best_of`** (`synthia-workflow`) —
  `WorkflowHost::select_candidate(&SelectionRequest)` with a default
  body returning `None` (keep R33's first-passing-gate rule), so no host
  breaks and the document format is unchanged. `SelectionRequest` /
  `SelectionCandidate` / `CandidateOutcome` / `GateVerdict` give a
  judge (rubric, LLM, longest answer) what it needs. The choice is
  **recorded** (`JournalEntry.winner`, `CallRun.winner`), so a replay
  never asks the host again; naming a candidate that did not succeed is
  a typed `WorkflowError::Selection` that stops the run instead of
  hiding a wrong judgement behind a plausible winner; and the `Arc<T>`
  host forwarder was extended so an `Arc<MyHost>` cannot silently fall
  back to the default.
- **Durable compaction checkpoints, end to end** (`synthia-session`,
  `synthia-context`, `synthia-agent`, `synthia-server`) — the emitters
  were never installed, so the log never recorded a compaction and a
  resume replayed (and re-paid for) the pre-compaction history. Now:
  `SurfaceLedger` resolves provenance **through the log** (the manager's
  in-memory indices cannot address the surface, since the loop prepends
  a never-logged system prompt), `CompactionCheckpoint` writes the
  lifecycle triple immediately (a crash mid-compaction stays detectable
  by `orphaned_compactions`) and writes the `Replace` checkpoint only
  when its span is provable — an unprovable record is dropped with a
  warning instead of writing wrong provenance; records that arrive
  before the run's rows land park and are retried by `resolve_pending`.
  `log_surface::{fold_log_surface, try_fold_log_surface}` fold a raw log
  and the production resume projection uses them, so a resumed session
  rebuilds the compacted surface. Clean cutover: the old
  `compaction_emitter_bridge`, `CompactionRecordView::to_typed_event`
  and `TypedEventSink::record_compaction` are **removed** — all three
  wrote a `Replace` with the manager's indices and empty
  `source_event_seqs`, exactly the wrong-provenance write the design
  forbids.
- **Examples** — `assemble_from_zero` (facade/prelude only),
  `tool_surface_policy`, `compaction_checkpoint`; `workflow_best_of` now
  shows a judging host too. The ladder is 28 offline examples.

### Added (R33 — the tool surface, wire hygiene, and workflow selection)

Plan: [`docs/optimization-report-R33-2026-09-12-plan.md`](docs/optimization-report-R33-2026-09-12-plan.md).
Results + evidence: [`docs/optimization-report-R33-2026-09-12.md`](docs/optimization-report-R33-2026-09-12.md).
Reconnaissance found three mechanisms that existed as vocabulary but
that nothing consumed; R33 wires each of them up.

- **The tool surface, in one place** (`synthia-tool` + `synthia-agent` +
  `synthia-server`) — `ToolEntry::with_exposure` / `exposure()` carry a
  `ToolExposure` onto `ToolDescriptor`, and
  `synthia_tool::{project_tool_definitions, called_tool_names}` turn
  descriptors + an optional visible-name filter + the set of
  already-called names into the model-facing `Vec<ToolDefinition>`:
  `Direct` sends the full schema, `Deferred` sends name + description
  with a permissive schema until the transcript shows the tool was
  called (then promotes it), `Hidden`/`is_hidden` sends nothing.
  Promotion comes from the transcript, so it is replay-stable and holds
  no mutable state. Both definition builders (the agent loop's
  `tool_definitions` and the server's operator listing) now use the one
  projection, which is also how `AdaptiveRegistry`'s tier cap and
  `GroupedRegistry`'s groups finally get a consumer — both were
  schema-less projections with no caller. A registry with no
  `Deferred`/`Hidden` tools produces byte-identical definitions to
  before (pinned by test).
- **Wire hygiene** (`synthia-provider`) — `json_repair::{repair_json,
  parse_tool_input_reported, parse_tool_input_logged, ToolArgsQuality}`
  salvages tool-call arguments that are not valid JSON (raw control
  characters inside strings, invalid escapes) and **reports** whether
  the value came from a strict parse, a repair, or the raw fallback, so
  a production log shows a salvaged call. This deletes a duplicate
  private parser in `assembler.rs` whose fallback disagreed with the
  shared one (`Value::Null` vs `Value::String`), so identical input no
  longer yields different `ToolUse.input` depending on the path, and
  every finalization site routes through the single shared function.
  Refusing a *truncated* argument stays the length-stop guard's job —
  a half-written argument is reported, never silently completed.
- **Structured provider error signals** (`synthia-provider`) —
  `error_body::{ProviderErrorBody, parse_provider_error_body}` parse the
  OpenAI-shaped and Anthropic-shaped error bodies (plus a bounded
  excerpt for anything else), and
  `RetryClass::{ContextOverflow, Auth}` + `classify_provider_error_body`
  make quota exhaustion, context overflow, capacity overload and auth
  failures distinguishable from the body alone. HTTP 429 with
  `insufficient_quota` is `Quota` (one attempt: a dead balance never
  recovers in-session) while a plain 429 stays `RateLimit`; text-only
  bodies keep their previous class. Both adapters' 429 paths now read
  the body they were previously discarding.
- **`best_of` workflow selection** (`synthia-workflow`) — `Step::BestOf`
  / `BestOfStep` (wire kind `best_of`) run one call per candidate under
  the existing caps and concurrency bound and keep the first candidate
  whose gate passes; `CallStatus::Superseded` records the losers without
  ever dragging a passing run down, and `WorkflowRun::winner_of(step_id)`
  names the winner. An all-failed selection fails the step while still
  handing the caller every failure text, and journal prefix replay
  reproduces a `best_of` run without re-spawning. This is traitclaw's
  parallel-branch strategy expressed declaratively: no VM, no scoring
  closure — the selector is the gate the workflow already runs.
- **Examples** — `deferred_tools` (tool surface), `tool_arg_repair`
  (wire hygiene), `workflow_best_of` (selection). The ladder is now 25
  offline examples, all indexed in [`docs/examples/README.md`](docs/examples/README.md).

### Added (R32 — tool groups, SQLite memory, output transformers, teams + rule routing, example ladder)

Plan: [`docs/optimization-report-R32-2026-09-12-plan.md`](docs/optimization-report-R32-2026-09-12-plan.md).
Results + evidence: [`docs/optimization-report-R32-2026-09-12.md`](docs/optimization-report-R32-2026-09-12.md).
R32 also completes R31's Wave 2 (its plan's item P) and repairs
everything R31 left broken — see the R31 report.

- **`synthia_tool::GroupedRegistry`** (traitclaw `GroupedRegistry`
  parity) — named tool groups with independent activation. The
  read-side wrapper offers only the tools of **active** groups (plus
  tools no group claims) while dispatch is untouched, so a tool whose
  group is deactivated **stays executable**: visibility is a
  prompt-budget decision, not a security boundary. Declarations are
  validated (`GroupError` for an empty name, an unknown tool, a
  duplicate, or a tool claimed by two groups) and one tool belongs to
  one group, so visibility never depends on declaration order.
  `ToolRegistry::contains` is the new wiring-side lookup the
  validation needs (hidden tools included, same key dispatch uses).
- **`synthia_context::memory::sqlite::SqliteMemory`** (feature
  `sqlite`, traitclaw-memory-sqlite parity) — single-file durable
  memory: `FTS5`/BM25 long-term recall (queries are sanitised into
  quoted terms, so `NEAR(` and `*` are text rather than syntax
  errors, and a term-less query falls back to newest-first), a durable
  working-memory table, a session registry, read-only opens that
  refuse every write, and typed errors that keep `SQLite`'s own
  message. The conversation tier deliberately delegates to the
  sink-backed `Memory` (documented divergence: synthia's durable event
  log is already the conversation source of truth). New optional
  dependency: `rusqlite` with `bundled` (no system `libsqlite3`,
  `FTS5` compiled in); the default build is unchanged.
- **Output transformers + full-output retrieval** (traitclaw
  transformers parity) — `synthia_core::{FullOutputStore,
  InMemoryFullOutputStore}` (bounded by entries **and** bytes, true
  LRU where a `get` refreshes recency, handles never reused),
  `synthia_tool::__get_full_output` + `full_output_tool` (the
  model-facing retrieval path, `ToolOutput::error` for an unknown
  handle), and `synthia_steering::{TransformerChain, JsonExtractor,
  BudgetAwareTruncator}` (chain order matters: extract, then budget).
  A large tool result is now *stashed and cited* instead of dropped.
- **`synthia_agent::agent::team`** (traitclaw-team parity) —
  `BoundAgent` (a name and one `prompt -> text` call: no session,
  provider or runtime), `FnAgent`, `Verifier`/`Verdict`,
  `VerificationChain`/`ChainOutcome` (generate → verify → retry, with
  the rejection reason appended to the next prompt; a rejection is
  data, only a transport failure is an `Err`), and
  `RoundRobinGroupChat` with `TerminationCondition`,
  `MaxRoundsTermination`, `MarkerTermination`, per-turn attribution
  and an explicit `StopReason`.
- **`synthia_agent::agent::route_rules::ConditionalRouter`
  (traitclaw conditional-router parity) — ordered regex rules,
  first match wins, default fallback, `RoutePatternError` for a bad
  pattern (never a panic). It implements the existing `Router` trait,
  so rules and `@agent:` mentions are two strategies behind one seam.
- **The progressive example ladder** — 22 offline examples plus
  [`docs/examples/README.md`](docs/examples/README.md) as the index.
  New: `tool_groups`, `sandbox_policy`, `sqlite_memory`,
  `output_transformers`, `agent_teams`, `workflow_fanout`,
  `derive_tool`, `eval_suite`, `replay_provider`,
  `run_inbox_steering`, `runtime_context`, `token_meter`,
  `delegation_gate`, `worktree_isolation`.
- **Fixed — tests no longer need ambient credentials.** Four lib tests
  in `synthia-server` (`routes::health` ×3, `server::tests`) and eight
  integration tests (`boot_wiring_test`, `operation_endpoint_test`)
  booted the production path and therefore required
  `ANTHROPIC_API_KEY`/`OPENAI_API_KEY` to be exported; they now use a
  hermetic state or a temp workspace whose provider reads a key the
  test itself sets (`crates/synthia-server/tests/support/mod.rs`).
  The `assemble_with_provider_profile` example selected its default
  profile by insertion order (Anthropic) and so needed a credential to
  run despite documenting that it does not; it now selects the keyless
  stub explicitly.
- **Fixed — R31 code that had never faced a linter:** the workflow
  step bound (`Option::map`), two collapsible `if`s (workflow +
  grouped registry), `PathBuf`→`Path` in the sandbox smoke test, and
  `?` in `SubagentPool::release`.

### Added (R31 — file-backed memory, sandbox policy, subagent pool, workflow runtime)

Four reference designs landed, plus the two Wave-2 pieces R31's plan
named but did not ship (they are in R32 below). Results and the
repair record:
[`docs/optimization-report-R31-2026-09-12.md`](docs/optimization-report-R31-2026-09-12.md).

- **`synthia-context::memory::file`** (pi-subagents §memory) —
  `FileMemory`: scoped long-term memory (`local` / `project` / `user`
  directories), `MEMORY.md` index bounded to 200 lines plus one
  frontmatter file per ULID entry, deterministic keyword recall
  (name 3× / description 2× / body 1×, newest-id tie-break),
  read-only mode, and path defenses (agent-name whitelist, symlink and
  traversal refusal).
- **`synthia-tool::sandbox`** (dsh §sandbox) — `ExecutionPolicy`
  (`ReadOnly` / `WorkspaceWrite` / `DangerFullAccess`),
  `SandboxBackend::confine(policy, root) -> ConfinedCommand` with
  confinement as **argv construction** rather than a wrapper process,
  `BwrapBackend` (read-only bind of `/`, workspace bind, private
  `/tmp`, `--unshare-pid`, `--unshare-net` under `ReadOnly`,
  `--die-with-parent`), one-shot availability probing that is
  **fail-closed** (a confined policy with no usable backend refuses to
  run instead of running unconfined), denial dialect classification,
  and the `effective_policy` fold (`grant > session event >
  deployment default`). The `shell` tool consumes it end to end and
  renders `[sandbox: file access denied under <mode> mode]`.
- **`synthia-agent::agent::pool`** (pi-subagents §agent-manager) —
  `SubagentPool`: two independent lanes (bounded background with a
  FIFO queue, unbounded foreground), nested children uncharged so a
  parent awaiting a child can never deadlock behind a slot it holds,
  100 tombstones addressable by id, sticky process-wide pool
  discriminators so a stale ticket frees nothing, and
  `InvocationRecord` capturing requested-vs-effective model/tier plus
  usage and status.
- **`synthia-workflow`** (new crate, pi-subagents §workflow) —
  `WorkflowSpec` (`agent` / `fan_out` / `pipeline` steps) executed by
  `WorkflowRuntime` through a single `WorkflowHost` effect seam
  (`spawn_agent` / `run_gate`), so caps, concurrency and abort are
  enforceable and tests inject a fake host; `WorkflowCaps` (agents
  1000 / items 4096 / nested 256), `WorkflowControl`
  (pause/resume/skip/retry/abort), and `WorkflowJournal` (JSONL keyed
  by sha256 of a call's identity fields, torn-tail tolerant, replaying
  the longest matching prefix and re-running the tail).
- **Fixed (R32-opening repair):** R31 landed with a NUL-corrupted
  `tests/sandbox_smoke.rs`, an unformatted workspace, a
  `synthia-workflow` crate that never compiled, a `Waker` moved out of
  a shared reference, a `ModelTier` moved out of `&self` in
  `tier_substituted()`, four unused `delegation.rs` imports, and three
  tests asserting behaviour the code never had. All repaired; the
  sandbox smoke test is now a real six-test suite that drives the
  shell tool's spawn path, including a genuine bubblewrap denial.

### Added (R29 (cont.) — the remaining seven phases landed)

Phases A–F of the R29 plan shipped in `8f1821cf`; these are
G–M, the rest of the same plan. Plan + the binding contract
(including every deviation) live in
[`docs/optimization-report-R29-2026-09-11-plan.md`](docs/optimization-report-R29-2026-09-11-plan.md)
and
[`docs/optimization-report-R29-remaining-contract.md`](docs/optimization-report-R29-remaining-contract.md).

- **`synthia-scheduler`** (new crate, pi-subagents §3) — a
  runtime-neutral cron/interval/once dispatcher. Three modules:
  `Job` (typed `JobKind`, `JobStatus`, ULID ids),
  `Scheduler` (`tick(now) -> ScheduleTick` — the caller owns the
  timer, so the crate works on any executor), and
  `ScheduleStore` (one JSON file per job under
  `<root>/schedules/`, PID-tagged lock files with stale-holder
  reaping, `temp + rename` atomic writes). 14 tests cover due /
  paused / once / interval advance, lock release, reload
  round-trip, and the `gc_completed` sweep.
- **`synthia_agent::MentionClone` + `LeaderRouter::mention_clone_mode`**
  (pi-subagents §2) — route a `@agent: …` mention through a
  throwaway clone instead of the leader's own `task` tool call,
  so the dispatch leaves no reasoning or tool_use block in the
  leader's transcript. The clone runs at `parent_depth + 1` so
  its events ride the standard sub-agent notification path, and
  its tool registry is deliberately empty (a test pins that no
  builtin leaks into an invisible turn). `mention_clone_mode`
  defaults to `false` — existing callers keep the `task` path.
- **`SessionEvent::LifecycleShutdown` + `SessionController::close`**
  (pi-subagents §I) — closing a session now appends a terminal
  `lifecycle_shutdown` event to the durable log and closes the
  sink, after every child has been signalled. Both the loop's
  join and `close()`'s wait are bounded by **3 s**, so a hung run
  cannot keep the process alive. `close()` is idempotent. Test
  drives a live blocking run through `close()` twice and asserts
  exactly one shutdown event on disk.
- **Compaction lifecycle events** (dsh §12) —
  `SessionEvent::{CompactionStart, CompactionSummary,
  CompactionEnd}` plus `SurfaceToken` / `CompactionOutcome`.
  `SummarizingContextManager::with_compaction_lifecycle_emitter`
  fires `Start → Summary → End(Committed|Failed)` around the
  summariser call, so a crash mid-compaction is detectable as an
  unmatched `Start` rather than an ambiguous gap.
  `synthia_session::compaction_outcomes` is the loader-side view:
  matched lifecycles report their outcome, orphans report
  `Interrupted`.
- **`assistant_chunk` raw-delta preservation** (dsh §20) — every
  streamed provider delta is now written to the typed event log
  as `SessionEvent::AssistantChunk { chunk_seq, finish_reason,
  data.delta }`, alongside the assembled assistant turn.
  `synthia_session::assemble_chunks` folds a chunk run back into
  the exact streamed text. Off unless a typed sink is wired, so
  the in-memory test path pays nothing.
- **`ContextManager::set_compaction_details` + `CompactionDetails`**
  (pi §13) — the ReAct loop records the files a run read
  (`read`) or modified (`write`) and hands them to the context
  manager before the next `prepare`, so a compaction's summariser
  prompt can name what touched disk. Only the deterministic
  builtins participate — shell-command file inference is
  deliberately omitted as guesswork.
- **`OperationRequest` wire union + `POST
  /api/v1/chat/sessions/{id}/operation`** (pi §5) — the
  discriminated operation endpoint (`prompt | skill |
  prompt_template | compaction | navigation`), opt-in via
  `[operations] enabled = true`. `prompt` dispatches to the same
  path `/messages` uses (shared `submit_session_op` helper, so
  there is one validation + controller-resolution path); the
  declared-but-unbuilt kinds answer `501 not_implemented` with
  the standard envelope. When the flag is off the route is not
  registered at all — the path falls through to the normal 404.

### Changed

- `test-support` moved into `crates/` and renamed to
  **`synthia-test-support`** — the workspace now keeps every crate
  under `crates/` with the uniform `synthia-` prefix (dir name =
  package name, matching all 14 siblings). Consumers
  (`synthia-agent`, `synthia-server` `test-utils` feature +
  dev-deps, `synthia-tool` dev-deps) follow the rename; no public
  API changed — only the crate identity and paths.

### Internal / Verified (R29 (cont.))

- Per-crate lib tests: core 84, telemetry 26, provider 633,
  context 69, tool 224, skill 56, session 99, steering 57,
  agent 213, **scheduler 14**, attachment 15, mcp 6, rag 38,
  server 387, test-support 1 → **1922 passing / 0 failing**
  (R29 phases A–F baseline: 1877; +45).
- `synthia-server` integration suites: 387 lib + 4
  (`operation_endpoint_test`) + 2 + 2 + 10 + 4 + 5 + 4 doctests,
  all green.
- `cargo +nightly fmt --all` exit 0; `cargo clippy --workspace
  --all-targets --all-features --tests -- -D warnings` exit 0.
- All 8 examples exit 0 (6 under `synthia-agent`, 1 under
  `synthia-mcp`, 1 under `synthia-rag`); `docs/examples/external-consumer`
  prints `CONSUMER-PROOF: OK`.
- Live boot smoke on a real binary: retrieval index built
  (`documents=1`), skills seeded, `/livez` · `/readyz` ·
  `/api/v1/agents` · `/api/v1/tools` all 200, and the gated
  operation endpoint answered `200` (prompt queued), `501`
  (compaction), `422` (unknown kind). The queued prompt reached
  the agent loop and made a real provider call (rate-limited by
  the configured key — the pipeline, not the provider, is what
  this smoke proves).
- Test-robustness fix (post-G–M): the
  `file_provider_uses_discovery_under_the_hood` test read the
  developer's real `$HOME` skill set and flaked ~1-in-4 runs
  under parallel load (25 found vs 1 expected). The
  `HOME_LOCK` + `ScopedHome` + `with_scoped_home` trio moved
  from `discovery.rs`'s private test module into a shared
  `#[cfg(test)] test_util` module, and the provider test now
  runs inside the scope (driving the never-yielding
  `collect()` via `futures::executor::block_on`). Verified by
  30 consecutive full-suite runs with zero flakes.

### Deliberate divergences (R29 (cont.))

| Area | Plan / reference | synthia | Why |
|---|---|---|---|
| MentionClone tool set | one synthetic `agent` tool | empty registry | The peer-dispatch seam that would back it does not exist yet; emptiness is the honest state and the test pins that no builtin leaks in |
| Compaction lifecycle → log | emitter bridge | loader-side `compaction_outcomes` view | The bridge would be unused plumbing until the loop installs it; a real consumer view is testable and useful today |
| File-activity tracking | read / write / shell-with-mutation | read / write only | Inferring a shell command's file effects is a guess, and a wrong claim in the summariser prompt is worse than an absent one |
| Operation endpoint path | `/v1/runs/{agent}/operation` | `/api/v1/chat/sessions/{id}/operation` | Matches synthia's actual route tree; the planned prefix does not exist |
| Operation endpoint config | bare `enable_operation_endpoint: bool` | `[operations] enabled` | Every other subsystem uses a named config table (`cors`, `rate_limit`, `retrieval`); one convention, not two |

### Added (R29 — four-reference absorption: traitclaw + dsh + pi + pi-subagents)

- **`synthia_tool::ToolRestriction` + `RestrictedRegistry`** (dsh `ToolRestriction` parity, §10) — per-scope allow / deny filter for tool visibility. Two restrictions compose by **intersection** (the dsh "intersect-and-shadow" rule): every ancestor scope contributes a filter, and a tool is visible only if every ancestor permits it. Own registrations stay exempt so a delegated child keeps the tools it answers through. Wire format: `{ "allow": ["read", "write"], "deny": ["shell"] }`; default `{}` means pass-through. `RestrictedRegistry` is a cheap-to-clone wrapper exposing `is_visible` and `visible_names` for the agent loop. 12 tests cover empty / deny / allow / own / intersect.
- **`synthia_tool::ToolOutputDefinition::presentation_meta` + `finalize_content`** (dsh `ToolDefinition` parity, §8) — completes the R17 output contract. `presentation_meta(args, value)` is a pure projection for UI cards that lets a tool serve the LLM and a UI panel without leaking UI vocabulary into model-visible text. `finalize_content(output)` is the last-mile transform on every outcome (including pipeline failures) so tools can override the failure-path shape without touching the success path. 4 new tests pin the contract.
- **`synthia_provider::credential::normalize_api_key`** (dsh `INVALID_CREDENTIAL` parity, §6) — pre-send credential classifier. Trims ASCII whitespace, rejects empty / illegal-character / unknown-source keys BEFORE they reach the transport. `WorkspaceConfig::resolve_api_key` and `ProviderProfile::resolve_api_key` now run the classifier at resolve time and return `Error::config` with a stable message; the server boundary maps these to `INVALID_CREDENTIAL` (deliberately NOT in the default retryable set). 9 unit tests.
- **`CompletionRequest::replay_state` + `CompletionResponse::replay_state`** (dsh `assistantProvenance.replayState` parity, §7) — opaque provider-native state the adapter can mint on response and the runtime can pass back on the next request (Anthropic prompt cache id, OpenAI response id, …). Threaded through `re_act.rs::step_2_sample_once`; the adapter validates and either consumes or drops. Forward-compat: `#[serde(default)]` on both fields. 3 round-trip tests.
- **`synthia_tool::ToolSchemaBuilder` + `ToolFeatures`** (pi-subagents `invocation-config.ts:isolationParam` parity, §7) — small builder wrapping a JSON Schema with feature-gated field removal. `with_feature("worktree_isolation", &features, "isolation")` drops the property AND its `required` entry when the feature is off, so the LLM never sees a description for a refused capability. 9 tests cover empty / chain / merge / property add / drop / feature off / feature on / features default / features chain.
- **`AgentMeta::requested_model` / `requested_thinking` / `effective_model` / `effective_thinking` / `overrides_applied`** (pi-subagents `invocation-config.ts:overridden` parity, §6) — when caller-supplied model / thinking is outranked by descriptor / frontmatter, the `AgentEvent::Agent` envelope carries both pairs plus an `overrides_applied: bool` so UIs can render "model · thinking: low (asked max)" instead of silently substituting. `AgentMeta::with_overrides(...)` is the builder. 3 round-trip tests.

### Internal / Verified

- R29 runs on the post-R28 baseline; tree clean. Per-crate test totals: core 84, telemetry 26, provider 633, context 64, tool 224, skill 56, session 89, steering 57, agent 206, attachment 15, mcp 6, rag 38, server 379 → **1877 passing / 0 failing**.
- The 36 new R29 tests are: 12 ToolRestriction, 4 ToolOutputDefinition extension, 9 credential, 3 replay_state round-trip, 9 ToolSchemaBuilder, 3 AgentMeta overrides (sums: 40 — the report quote of 36 covers the lib-only test additions; the +4 is the agent_meta tests the per-crate count absorbed).
- `cargo +nightly fmt --all` exit 0; `cargo clippy --workspace --all-targets --all-features --tests -- -D warnings` exit 0.
- All 8 examples (`assemble_from_scratch`, `assemble_with_provider_profile`, `assemble_with_skills`, `assemble_with_tier_steering`, `assemble_with_builder`, `register_remote_tools`, `grounded_agent`, `fan_out_with_group_join`) exit 0; `docs/examples/external-consumer` still prints `CONSUMER-PROOF: OK`.
- `synthia-server --directory <ws> --port 18080` boots; `/livez` 200, `/readyz` 200, `/api/v1/agents` 200, `/api/v1/tools` 200.

### Added (R30 — correctness seams + production robustness + two DX/quality crates)

Eight designs from the reference repos landed. Plan: `docs/optimization-report-R30-2026-09-12.md`. Verified: fmt clean, clippy `--all-targets -D warnings` 0, **per-crate tests green**, `docs/examples/external-consumer` `CONSUMER-PROOF: OK`.

**Correctness / production** — `synthia-provider`: `RetryClass::{EmptyResponse, Quota}` (dsh llm-retry completion — empty completions become typed errors instead of silent success; quota exhaustion is its own class so it never re-cycles against a dead balance); shared `streaming::idle_watchdog` + `pump_sse` (per-read watchdog, defaults to 120s; SSE cancel-drain grace unified); `validation::is_empty_terminal_completion` shared between Anthropic + OpenAI parse seams (4 tests + 4 types/stream_chunk tests + 6 integration tests). `synthia-mcp`: `public_tool_name` + `NamingPolicy` (`mcp__<server>__<raw>`, sha-suffixed on lossy normalization — two servers with the same raw name now coexist via deterministic prefixing); `McpSupervisor` (tick(now) clock injection, per-outage reconnect budget, generation-swap re-sync on `tools/list_changed`, fail-soft per-server health surfaced through `AppState::mcp_health`); `StreamableHttpTransport` (reqwest-backed, full wiremock integration tests); `synthia-mcp/tests/{mcp_protocol,supervisor,http_transport}.rs` (52 passing). `synthia-session`: `token_meter::TokenMeter` pure fold (disjoint usage buckets, provider-usage anchoring, projected next-request tokens = `max(0, anchor + signed surface_delta)`); `SessionEvent::Usage.usage: Option<UsageBuckets>` (serde-default back-compat — old payloads still parse); `OperationSnapshot::usage_buckets + context_pressure` (serde default); `synthia-context::CompactionSettings::should_compact_anchored` opt-in (heuristic path unchanged). `synthia-agent`: `RunInbox` trait + `MpscInbox` (futures mpsc, runtime-neutral, default empty — byte-identical behavior for inbox-less callers); steering drained at iteration start + between tool commit and next sample + after compaction prep; follow-ups revive a stopping run; `SystemEvent::SteeringInjected` + `SteeringSource::{Steering, FollowUp}` (non-exhaustive match, observable in the event stream); **length-stop guard** — `stop_reason ∈ {length, max_tokens}` with tool calls fails the whole batch via model-facing error `ToolResult`s (no truncated JSON ever runs); `prompt::RuntimeContext` snapshot seam — volatile env facts leave the system prompt, render as an appended user-role snapshot only when the rendered body changed (chrono clock injected; dsh §`renderContextSnapshot` parity); `agent::gate` (verification command on the task tool — exactly-once verdict via injectable `CommandRunner`, killed gates report killed explicitly); `agent::worktree` (git-worktree isolation for delegated children — `synthia/agent-<ulid>` branch on dirty exit, removed on clean, fake git runner exercised by 6 tests; the `worktree_isolation` feature name `synthia-tool/schema_builder` R29 advertised is now real).

**DX / quality** — new crate `synthia-macros`: `#[derive(Tool)]` + helper `#[tool(name, description, mode)]` attribute; generates a full `impl synthia_tool::Tool` delegating to inherent `async fn execute(&self, &Context) -> ToolOutput`; schemars-derived `parameters()` (type-driven, per schemars 1.2 guidance — value-driven breaks for enums / `Option::None`); compile-error diagnostics for misuse (non-struct, generics, missing `execute`, missing description, bad attribute, bad mode); self-dev-dep integration suite, 30 tests. New crate `synthia-eval`: `EvalSuite` / `TestCase` / `Metric` (sync) / `AsyncMetric` / `EvalRunner` (case passes iff ALL metric scores ≥ threshold; no-metrics fallback = keyword); built-ins `KeywordMetric`, `LlmJudgeMetric` (local `JudgeProvider` seam + clamped score parse), `SchemaValidationMetric` (reuses `synthia_core::schema::validate_against_schema`); `EvalReport::summary() + to_json_string() + to_csv_string() + export_json + export_csv` (RFC4180 escaping); 30 tests. `synthia-test-support`: `ReplayProvider` implementing `synthia_provider::ModelProvider` from chunk scripts or session `events.jsonl`; per-run cursor, `assert_consumed` returning `Result<(), ReplayError>` (cancellation aborts undrained cursor; dsh `assertConsumed` parity); 17 tests.

### Deferred (R30-deferred → R31)

- **SubagentWorkflow scripting + journal prefix-replay** (pi-subagents §1) — vm-isolated scripts + JSONL prefix replay. The scripted-VM part is JS-shaped; porting to Rust means a declarative `WorkflowSpec` + caps + journal + gate orchestration as a new `synthia-workflow` crate. Sized ~1500 LOC, deserves a focused round with its own design phase.
- **OS-level sandbox seam + Landlock/bwrap backends** (dsh §sandbox) — argv construction + Landlock ABI ruleset builder + policy fold over `PersistedEvent`. New crate `synthia-sandbox` (or an `agent` module); kernel-ABI availability makes WSL2 the gating factor for tests.
- **SQLite durable sink + FTS5** (traitclaw memory-sqlite parity) — `SqliteSessionSink` implementing the `SessionSink` trait (rusqlite, bundled). New dev-dep `rusqlite` adds build cost; tracked separately.
- **GroupedRegistry / dynamic tool groups** (traitclaw registries parity) — operator-chosen focus sets where `get_tools()` returns active groups and `find_tool()` searches ALL groups.
- **File-backed scoped long-term memory** (pi-subagents memory parity) — `MemoryFile` impl of `synthia_context::Memory` long-term tier, with MEMORY.md index file + frontmatter entries + symlink defense.
- **Two-pool subagent concurrency manager** (pi-subagents agent-manager parity) — background 10-queued / foreground unbounded / nested no-slot, tombstones, invocation records.
- **Progressive numbered example ladder** (traitclaw `examples/` parity) — 15-25 micro-crates each demonstrating one seam. Currently the README's 8 example commands cover the main surface; a per-concept ladder is a docs/PR pass, not a code change.

### Original R30 deferred (still pending)

- **DeferredHandle / OperationRequest discriminated union** (pi §1 + §5) — provider-neutral cross-gateway polling. Needs a real gateway; we ship the wire shape only in R31.
- **Compaction log-only events + `SurfaceOp.replace` checkpoint** (dsh §12) — the durability seam is identified but not yet wired into `SummarizingContextManager`.
- **`assistant/chunk` raw-event preservation** (dsh §20) — every `StreamChunk` durably logged; doubles log size, enables live-resume.
- **SessionEventMap extensibility** (dsh §1+§2) — Rust has no TypeScript `interface` declaration merging; the `inventory` crate gives us a mechanical substitute but a one-round port is too thin to commit.
- **TUI widgets** (pi coding-agent) — synthia is HTTP, not TUI.
- **Cross-extension RPC envelopes** (pi-subagents §5) — synthia is a single binary, no event bus; HTTP gateway already carries the equivalent reply envelope.
- **FsAdapter / ShellOutputUpdate** (pi §10) — bash tool already has truncation + sanitization; a `Result<T, FsError>` boundary on the FsAdapter is a follow-up.
- **`shutdownChildSession` lifecycle event** (pi-subagents) — needs the `SessionController` refactor that R26's `GroupJoin` was the seed for.


### Deliberate divergences from the references



| Area | Reference | synthia R29 | Why |
|---|---|---|---|
| Tool output `render` | blanket trait impl | default trait method on `Tool` | R17: a blanket impl makes a concrete override a coherence error |
| Credential rejection | throw `INVALID_CREDENTIAL` directly | `Error::config("Invalid API key in <env>: …")` mapped at the server boundary | Synthia's `Error` taxonomy owns the variant; the transport layer is the only place that knows the HTTP code |
| `presentation_meta` | UI card JSON | `Option<serde_json::Value>` on the definition | The same value can ride the wire verbatim — no extra wrapper |
| `replay_state` | one field on `assistantProvenance` | one field on `CompletionRequest` + one on `CompletionResponse` | Synthia carries the state on the wire type, not on a separate `MessageSource` |

### Changed (R27 — `re_act.rs` split: 5896 → 2116 lines, no behaviour change)

- **The ReAct module is now a three-file unit with a real interface instead of one 5896-line file.** Pure code motion, verified by an unchanged test suite:
  - `agent/re_act.rs` — 2116 lines of production code (`ReActAgent`, `ReActLoop`, helpers) and the module map.
  - `agent/re_act/stream.rs` (402 lines) — **streaming chunk ingestion**: the buffers, the drain logic, the finalizer and their tests. Its interface to the loop is two items, `StreamSink` (typed event emitters) and `ChunkState` (the per-sampling-pass accumulator); everything else is `private`, which is what makes it a deep module: it is the only code that knows a provider's chunk shapes.
  - `agent/re_act/tests.rs` (3352 lines) — the test suite, moved out of the production file via the Rust 2018 child-module form (`mod tests;`), so the module path is unchanged and no test needed editing.
- Extraction was mechanical and compiler-verified: visibility on exactly five methods plus two types, imports resolved from the compiler's own diagnostics, and `cargo test -p synthia-agent --lib` unchanged at 203 passing before and after.

### Added (R26 — `GroupJoin`: batched completion notification for background agents)

- **`synthia_agent::GroupJoin`** — pi-subagents `group-join.ts` parity, and the last named reference gap. When an orchestrator fans out background subagents, notifying the main agent once per completion produces a burst of interruptions; `GroupJoin` holds those completions and emits **one** consolidated [`Delivery`] per group. Semantics: `Pass` for an ungrouped id (notify individually), `Held` while members are outstanding, an immediate `partial: false` delivery when the last member finishes, and on window expiry a `partial: true` delivery of the finished subset so a single slow straggler cannot stall the batch — the group then survives on a shorter straggler window (30 s / 15 s, pi's defaults), and a member is delivered **at most once** across partials.
- **Runtime-neutral by construction**: a pure state machine with no clock and no timer. The caller passes `now` (`chrono::DateTime<Utc>`) and arms its own timer from `GroupJoin::next_deadline()`, so it works on any executor and its tests are fully deterministic (no fake timers, unlike the reference implementation).
- Example `crates/synthia-agent/examples/fan_out_with_group_join.rs` — a 3-member panel delivering one notification, plus the timeout/straggler path; asserts nobody is delivered twice.
- 10 tests covering the state machine: pass-through, single delivery on completion, deadline arming, deadline expiry (silent before, partial after), straggler re-batching on the shorter window, no double delivery, empty-window re-arm, `forget`, empty registration, re-registration replacing a group, and two independent groups. A test caught a real leak on the first run (a retired group left its members' id→group mappings behind, so `is_grouped` stayed `true`); `forget` now drops pending, finished **and** delivered ids.

### Added / Fixed (R25 — production boot E2E)

- **MCP transports carry a `label`** (`StdioConfig::label`) and `StdioTransport` bounds its auto-generated description to 60 chars. Previously `server=stdio: sh -c <script>` dumped an entire multi-line script into every log line and never named the configured server; the server now passes `McpServerConfig::name`, so logs read `server=boot-e2e`.
- **`crates/synthia-server/tests/boot_wiring_test.rs`** — drives the real `AppState::new` boot path and then the real router, so the R21/R22 wiring is protected end-to-end: config → MCP spawn → tool publication → `GET /api/v1/tools` contains the remote tool, `AppState::retrieval` is populated, builtins survive, and a config-less deployment boots with no remote tools. Unit tests on the individual helpers could not catch a wiring regression (e.g. registering MCP tools after the registry is wrapped in its lock).

### Verified (R25 — real binary, not just tests)

- `synthia-server --directory <ws> --port 18080` boots and logs `synthia.mcp: spawned MCP server server=boot-e2e` → `registered MCP server tools tools=1` → `synthia.rag: built retrieval index documents=2` (a `.png` in the indexed tree was correctly skipped).
- Live HTTP: `/livez` 200, `/readyz` 200, `/api/v1/agents` 200, `/api/v1/tools` 200 listing **7 tools** — the 6 builtins plus `e2e_remote_tool` served by the spawned MCP child (confirmed alive as a child of the server process).

### Changed (R24 — error-handling convention audit)

- **The last three hand-rolled error enums now use `thiserror`** (AGENTS.md §3.1: library crates handle errors with `thiserror`), which also deletes their hand-written `Display` boilerplate:
  - `synthia_session::SessionError` — messages preserved verbatim (`"session is closed"`, `"{0}"` passthrough for the payload variants), `Display` + `Error` impls deleted.
  - `synthia_steering::AdmissionError` — messages preserved verbatim, impls deleted.
  - `synthia_session::FoldError` — **had no `Display` and no `Error` impl at all**, so it could not be erased into a boxed error or `?`-propagated. It now has both, with messages naming the offending event/range, pinned by a test.

### Verified (R24 — convention audit on the current tree)

- `SystemTime` appears exactly once (`bound_output.rs` mtime cutoff vs `metadata.modified()`) — the documented-correct use for filesystem timestamps; all wall-clock time is `chrono::DateTime<Utc>`.
- Error choice per crate: every library crate uses `thiserror` (or re-exports `synthia_core::Error`, itself `thiserror`); only `synthia-server` — the binary/app crate — uses `anyhow`. No crate uses both.
- Dependency versions are all `major.minor`; the only three-segment literal is `[workspace.package] version = "0.1.0"`, which is this workspace's own version, not a dependency.
- Public-API `tokio` leaks: **0** across the 12 library crates (visibility-aware audit over public fns, public struct fields, enum variants, type aliases). The 13th crate under `crates/` — `synthia-server` — is the application crate and legitimately names `tokio` types (e.g. its broadcast `subscribe()`); the neutrality rule is a library guarantee, so it is stated for libraries only.

### Fixed (R23 — runtime-neutrality audit)

- **`synthia-tool::start_cleanup_task` no longer returns `tokio::task::JoinHandle<()>`** — the last `tokio` type in any library crate's public signature, found by a visibility-aware audit over the 12 library crates (public fns, public struct fields, enum variants, type aliases). It now returns a runtime-neutral `CleanupTask` with `abort()` (also run on drop) and `detach()` (keep cleaning without the handle). R6 established the principle that lib crates expose only std / `futures` / workspace-own types in public signatures, with `tokio` as an implementation detail; this closes the one remaining violation. The audit now reports **0 leaks**; re-run it before claiming neutrality for a new crate.
- 2 behavioural tests: the cleanup loop really deletes stale managed files; dropping the handle aborts the loop; `detach` keeps it running.

### Verified (R23 — end-to-end audit on the current tree)

- **All 7 examples run green** (`assemble_from_scratch`, `assemble_with_builder`, `assemble_with_skills`, `assemble_with_provider_profile`, `assemble_with_tier_steering`, `register_remote_tools`, `grounded_agent`).
- **External-consumer proof re-run green**: `docs/examples/external-consumer` (independent crate, empty `[workspace]`, relative path deps only) assembles a descriptor + attachment round-trip + typed event sink + a full agent run and prints `CONSUMER-PROOF: OK`.
- `cargo test --workspace` → 1956 passed / 0 failed; fmt 0 diff; clippy 0 errors with `-D warnings`.

### Added / Changed (R22 — config-driven RAG grounding; one generic context-manager seam)

- **Retrieval grounding from config** (R22, completes the "every capability is reachable from deployment config" story alongside R16 compaction and R21 MCP): `ServerConfig.retrieval` (`enabled` / `paths` / `chunk_chars` / `top_k` / `char_budget`). At boot the server walks the configured paths, chunks every text-ish file (`.md`, `.txt`, `.rst`, `.toml`, `.json`, `.yaml`, `.yml`; `.git`/`target`/`node_modules` skipped), and builds a BM25 index — dependency-free, deterministic, **no network call**, so grounding turns on with config alone. Unreadable paths log `warn` and are skipped; an empty index is skipped rather than failing boot.
- **`AgentRunConfig.context_manager`, replacing R16's capability-specific `compaction` field.** The caller composes whatever manager it wants and hands it in; the loop factory just installs it (`None` keeps the loop default). One generic seam beats one field per capability — and it keeps `synthia-agent` free of any dependency on the compaction or retrieval crates. The run factory got simpler: it no longer clones a provider handle for compaction.
- **`ground_with_retrieval(retriever, inner)`** — public composition helper with the ordering contract pinned by test: the grounder injects retrieved context and *then* delegates inward, so compaction/truncation still trims to the window. Grounding never escapes the budget. With both capabilities configured the server composes exactly this chain.
- **`RunDependencies::{retrieval, with_retrieval, with_optional_retrieval, compose_context_manager}`** + `AppState::retrieval`.
- 6 tests: retrieval config round-trip, boot index build (nested dirs walked, non-text extensions ignored, retrieved content ranks for its own query), opt-in semantics (no config / disabled / nothing-indexable → no index), grounding-before-inner ordering (spy inner manager observes the injected block), and composed-manager opt-in.

### Added (R21 — config-driven MCP servers at boot)

- **`mcp_servers` in the server config** (R21, completes R19's production story): `ServerConfig.mcp_servers: Vec<McpServerConfig>` (`name` / `command` / `args` / `env` / `enabled`) with `to_stdio_config()` bridging to `synthia_mcp::StdioConfig`. At boot `register_configured_mcp_servers` spawns each enabled server, runs the MCP handshake, and publishes its tools into the local registry — so remote tools flow through the same guard pipeline, dispatch, metrics, and `output_definition` seam as builtins, exactly like `/api/v1/agents`-registered or builtin tools. Deployments add MCP capability with config alone.
- **Fail-soft boot**: a server that cannot spawn, handshake, or list is logged at `warn` and skipped; a broken optional integration never blocks startup (pinned by test — the builtin surface survives intact).
- **`AppState::mcp_clients`**: the live clients are retained for the process lifetime, because dropping a `StdioTransport` kills its child process (`kill_on_drop`). Consumers can address servers by name.
- 3 tests: config round-trip (incl. the MCP list), boot registration against a scripted `sh` MCP responder (tools visible in the registry + catalog, disabled servers skipped), and broken-server tolerance.

### Added (R20 — RAG: retrieval-augmented grounding)

- **New crate `synthia-rag`** (R20, traitclaw `crates/traitclaw-rag` parity with documented improvements):
  - **`Retriever`** seam + [`Document`]/`ScoredDocument` (score separated from the document — a score belongs to a *(document, query)* pair). Deterministic ranking: descending score, **id tie-break**, so the grounding prompt is reproducible for provider-side prompt caching.
  - **Chunkers** — `FixedSizeChunker` (character-counted so multi-byte text never splits mid-codepoint, optional overlap), `SentenceChunker` (ASCII + CJK terminators), `RecursiveChunker` (paragraph → line → sentence → hard-cut hierarchy). All deterministic.
  - **`KeywordRetriever`** — **real BM25** (`k1 = 1.2`, `b = 0.75`, probabilistic IDF with `+0.5` smoothing, length normalisation) with dependency-free tokenisation (lowercase, punctuation split, per-character CJK). traitclaw's version is `Σ tf/(tf+1)` with no IDF; a rare term must outrank a common one (pinned by test).
  - **`EmbeddingRetriever`** + **`Embedder`** trait, with `ModelProviderEmbedder` adapting any `synthia_provider::ModelProvider` (index-time batch embed, query-time single embed, cosine similarity). A retrieval failure logs at `warn` and degrades to "no grounding" — never breaks a turn, never silent.
  - **`HybridRetriever`** with two merge strategies: `CitationStrategy::ReciprocalRankFusion { k }` (**default**, `k = 60` — scale-robust: BM25 is unbounded, cosine is bounded) and `CitationStrategy::Weighted { keyword_weight }` (min-max normalised). Over-fetch factor `3`.
  - **`RagContextManager`** — implements `synthia_context::ContextManager`, so grounding composes with compaction / tier steering for free. Retrieves against the newest user turn, renders a delimited `<retrieved_context>` block with numbered citations, **replaces** its own previous injection instead of stacking, skips retrieval for very short queries, and delegates to an inner manager so grounding still fits the window.
- **Example** `crates/synthia-rag/examples/grounded_agent.rs` — chunk two sources → BM25 index → ranking → grounding prompt (printed) → RRF hybrid merge. No network, no embedder needed.
- 38 tests: chunker boundaries (incl. CJK + hard-cut fallback), BM25 ranking + IDF + CJK query, embedder contract violations, cosine edge cases, RRF/weighted merge, budget selection, grounding rendering, and the context-manager injection/replacement/trim behaviour.

### Added (R19 — MCP client: remote tools as local lego)

- **New crate `synthia-mcp`** (R19, traitclaw `crates/traitclaw-mcp` parity, reshaped for synthia's seams): a Model Context Protocol client.
  - **`McpTransport`** — the I/O seam (one JSON-RPC 2.0 request → one result). `StdioTransport` spawns a child process over newline-delimited JSON-RPC with a per-transport pair lock, stderr inherited, `kill_on_drop(true)`, and an id-matching read loop that skips server notifications. `InMemoryTransport` (public, not `#[cfg(test)]`) scripts responses so consumers test their own MCP wiring with **no process spawn**.
  - **`McpClient`** — `initialize` (protocolVersion `2025-06-18`, clientInfo, then the mandatory `notifications/initialized`), `tools_list`, `tools_call`.
  - **`McpTool`** — a remote tool adapted to `synthia_tool::Tool` (name/description/`inputSchema` from the spec; `RenderKind::Json` + `mcp_server` presentation hint; `isError` → error output; transport failure → error output).
  - **`register_mcp_tools`** — publish every advertised tool into a `ToolRegistry`, after which they flow through the same guard pipeline, dispatch, metrics, and `output_definition` seam as builtins.
- **Example** `crates/synthia-mcp/examples/register_remote_tools.rs` — spawns a POSIX `sh` responder over stdio, handshakes, registers `sum`, and calls it (success + `isError` paths). Runs with no network and no external MCP server.
- 16 tests: 10 protocol/adapter/registry integration (incl. a **real stdio round-trip** against a scripted `sh` child with 4 sequential id-matched calls) + 6 in-crate unit tests.

### Added (R18 — external-consumer proof, checked in)

- **`docs/examples/external-consumer/`** — a standalone crate that is deliberately NOT a workspace member (empty `[workspace]` table) and depends on the synthia crates by relative path. It assembles a complete agent in one `AgentBuilder` chain (provider + tier steering + `.output_schema` auto-tool + `.compaction_settings` + typed sink), round-trips an image through the `AttachmentStore`, drives a turn with a std-only `AtomicCancelToken`, and asserts the result. Run: `cd docs/examples/external-consumer && cargo run` → `CONSUMER-PROOF: OK`. This is direct evidence for the goal's "use this repo as a lib from zero" claim: an out-of-tree consumer builds and runs against the published surfaces with no tokio type in any synthia call. README gained the "Consume synthia from your own crate" recipe.

### Added (R17 — ToolOutputDefinition made real)

- **`Tool::output_definition` is now an overridable trait method** (R17-1, fixes the R10 coherence trap): R10's `HasOutputDefinition` extension trait shipped an `impl<T: Tool + ?Sized> HasOutputDefinition for T` blanket impl, which made any concrete override a **coherence error** — so no tool could ever opt in and the seam was inert by construction. The method now lives on `Tool` itself as a defaulted method (object-safe; returns `ToolOutputDefinition::passthrough(name)`), and the extension trait is deleted.
- **Builtin render contracts** (R17-2, dsh result-view parity): `web_fetch` → `RenderKind::WebFetch` + truncation-marker hint; `read` → `Read` + `line_numbered`; `write` → `Write`; `shell` → `Shell` + `exit_marker`; `TodoWrite` → `Todo`. Every builtin now carries a display title, so the LLM-facing projection and the UI renderer share one contract per tool instead of hard-coding shapes.
- **`ToolRegistry::output_definition(name)`** (R17-3): the consumer seam — returns the tool's contract (hidden/unknown → `None`), letting the agent loop / UI pick a renderer by name. `GET /api/v1/tools/{name}` now includes it in `ToolDetail`.
- **`RenderKind` serialises in `snake_case`** (`"shell"` / `"web_fetch"` / `"todo"` / …), matching the rest of the API surface (`ContentPart`, `OperationState`). Previously PascalCase — fixed at birth (nothing consumed it before R17).

### Added (R16 — server-config compaction surface)

- **`agents.<name>.compaction` server config** (R16, completes the R13/R15 production story): `AgentConfig` gains an optional `compaction` table (`enabled` / `reserve_tokens` / `keep_recent_tokens` / `min_messages_between_compaction`) deserialised straight into `CompactionSettings`. `AppState::default_compaction` is resolved at boot via `load_default_compaction` and threaded through `RunDependencies.with_optional_compaction` → `AgentRunConfig.compaction` → the run factory, which installs the provider-backed `SummarizingContextManager` via the new public `synthia_agent::context_manager_for_compaction(provider, settings)`. Deployments can now turn on LLM compaction with config alone. 3 tests (config-table parse with defaults filling the omitted throttle / loader read + unknown-agent + no-name → None / `AgentConfig` 8-field serde contract).
- **Fixed: flaky `mutation_queue::same_path_runs_sequentially`**: the test asserted the spawn order `[1, 2]`, but `tokio::spawn` gives no scheduling guarantee — the queue's real contract is mutual exclusion, not FIFO. Rewritten to observe the maximum number of mutations inside the critical section at once (`MUST be 1`) plus "both ran", which is deterministic under any scheduler interleaving.

### Added (R15 — CompactionSettings drives the SummarizingContextManager trigger)

- **`SummarizingContextManager::with_settings`** (R15-1, closes the R13-3 follow-on): installs a `CompactionSettings` policy that gates the batch pruning on `should_compact` (utilisation threshold + enabled switch + `min_messages_between_compaction` throttle via an interior `AtomicU32` counter reset on every successful splice). `None` (the default) keeps the legacy always-prune behaviour — all 6 pre-existing summarising tests pass unchanged. `AgentBuilder::compaction_settings` now threads the policy into the manager it constructs, so the R13 selection + this trigger form the complete pi-parity compaction story. 3 tests (under-threshold blocks / over-threshold prunes + throttle reset / disabled never prunes).
- **Fixed: `estimate_messages_token_count` dropped tool-result content** (found by the R15-1 gate tests): the public estimator matched only `ContentPart::Text` via `part.text()`, so tool-result text and tool-use JSON counted as **0 tokens** — tool-heavy conversations (exactly what compaction targets) estimated near-zero and could never cross the threshold. Now counts Text + Reasoning + ToolResult inner text + ToolUse name/input JSON through the shared `estimate_token_count` heuristic. Probe before: 4-message tool conversation → `est=0`; after: `est=393`.

 ### Added (R13 — attachment_ref resolve + output_schema injection + compaction wiring)

- **`attachment_ref` resolution at the chat boundary** (R13-1, closes the R12-2 follow-on): `build_parts` now takes `Option<&AttachmentStore>`; a `kind = "attachment_ref"` + `attachment_hash` attachment resolves through the R11 store and inlines the bytes as an `Image` part — the full hash→store→bytes→wire round trip, replay-safe via content addressing. Unknown hashes are skipped with a warn (the text part still ships; the turn isn't lost). Production call sites pass `Some(&state.attachment_store)`. 2 tests (round-trip + skip-on-unknown).
- **`AgentBuilder::output_schema` → auto `StructuredOutputTool` injection** (R13-2, closes the R12-3 loop-wiring deferral without re_act surgery): declaring a schema on the builder auto-registers `structured_output` (LIFO so it wins name lookups) into the tool registry at `build()` time AND threads the schema string onto `AgentDescriptor::output_schema` for the wire surface. The previously-inert descriptor field is now the source of the injection. 2 tests (inject + inert-by-default).
- **`AgentBuilder::compaction_settings` + `resolve_context_manager`** (R13-3, pi `harness/compaction` parity): when the policy is present + enabled + valid, `build()` upgrades the context manager from the truncating default to `SummarizingContextManager` whose summariser forks the agent's own provider (no second model wiring). Invalid settings warn + keep the default; disabled keeps the default. The pure decision core (`resolve_context_manager`) is directly tested across the full table (None/disabled/invalid/valid) including a behavioural prepare smoke on the replaced manager. 1 test.
 ### Added (R12 — AgentBuilder + schema validation + structured output + /status route)

- **`synthia_agent::builder::AgentBuilder`** (R12-1, traitclaw `examples/24-agent-factory` parity): fluent factory hiding the orchestrator wiring. `AgentBuilder::new(provider)` + chained `.workspace() / .tool_registry() / .steering() / .context_manager() / .typed_event_sink() / .peer_registry() / .max_iterations() / .name() / .instructions() / .model_hint()` → `.build()` produces a fully wired `ReActAgent`. Defaults: `/tmp` workspace, empty `ToolRegistry`, `Steering::noop()`, `TruncatingContextManager`, descriptor named `"agent"`. `Clone` for cheap branching. Closes the goal's "乐高式从零组装" axis. 4 tests.
- **`synthia_core::schema::{validate_against_schema, SchemaViolation}`** (R12-3a): minimal JSON-Schema-subset validator (type / required / properties / items; unknown keywords permissive per spec). Returns **all** violations at once with dotted paths (`"opts.depth"`, `"[2]"`) so the model can fix every field in one retry. 10 tests.
- **`synthia_tool::structured_output::{StructuredOutputTool, structured_output_tool, STRUCTURED_OUTPUT_TOOL_NAME}`** (R12-3b, pi-subagents `src/structured-output.ts` parity): synthetic tool whose `parameters()` IS the caller's schema — the provider fills the fields as tool-call arguments, `call()` re-validates via `validate_against_schema`, and either captures the canonical JSON payload or returns an `is_error` result enumerating the violations. The structured-output capture pattern without response_format machinery. 5 tests.
- **`WireAttachment.attachment_hash`** (R12-2): new optional wire field on the chat request — `kind = "attachment_ref"` + `attachment_hash` references a previously-uploaded attachment (from R11's `/api/v1/attachments`) by content hash. `WireAttachment` now derives `Default` + `Serialize` so the hash round-trips through serde. 1 round-trip test. (The agent-runtime resolve-through-store step is the R13 follow-on.)
- **`GET /api/v1/sessions/{id}/status`** (R12-4, closes the R10-14 deferral): returns the latest `OperationSnapshot` from the controller's snapshot bus (typed `state` / `iteration` / `usage` / `last_error`), or 404 when the session is unknown or has never run. Complements the string-label `infer_status` on `GET /sessions/{id}`. 1 bus-latest test pinning the late-subscriber contract the route depends on.
### Added (R11 — multimodal attachments + router/delegation bridge + snapshot emit)

- **New crate `synthia-attachment`** (R11, dsh `packages/attachment/attachment/src/index.ts` parity): content-addressed multimodal attachment store. `ImageAttachmentRef { hash, mime_type, byte_len, saved_at }` carries the SHA-256 identity through the LLM context instead of inline base64; `attachment_to_content_part()` renders the ref into `ContentPart::Image` for the provider wire format. `AttachmentStore` fronts a pluggable `AttachmentBackend` (`FilesystemBackend` sharded `root/{hash[0:2]}/{hash}.{ext}` with dedup-on-write, `InMemoryBackend` for tests) plus a bounded hot-read cache (256 entries) with `evict()`; `read_image` re-verifies the hash on every backend fetch and surfaces `AttachmentError::HashMismatch` on corruption. Validators: `SUPPORTED_IMAGE_MIME` (jpeg/png/gif/webp/bmp) + `DEFAULT_MAX_BYTES` (8 MiB). `read_base64_raw(hash)` serves hash-only lookups. 15 tests.
- **`POST/GET/DELETE /api/v1/attachments`** (R11): server surface for the attachment store. `AppState::attachment_store` (filesystem-backed at `<workspace>/.attachments`, in-memory fallback) is shared across handlers; the route validates MIME via a `validator` custom rule, decodes base64, saves through the store, and maps `AttachmentError` onto the unified envelope (`unsupported_mime` 400 / `attachment_too_large` 413 / `attachment_not_found` 404 / corruption 500). 6 route tests.
- **`mentions_to_task_specs`** (R11, closes the R10-4 loop-interception deferral): converts a `RoutingDecision` from `LeaderRouter::route(text)` into `Vec<TaskSpec>`s the existing delegation seam (`delegation::run_subagent`) understands — `@agent: prompt` text mentions become `task` tool calls without the model emitting a tool_use block. Re-exported from `synthia_agent`. 2 tests.
- **`SessionController::subscribe_snapshots` + snapshot publishing** (R11, closes the R10-5 controller-emit deferral): the controller now publishes `OperationSnapshot`s on the shared `SnapshotBus` at every run-state transition — `Running` in `maybe_start_run` (with resolved agent name + iteration cap), `Completing` when the factory stream ends, `Cancelled { reason }` in the Cancel op handler. `subscribe_snapshots()` on the public handle returns the receiver plus the latest snapshot for late subscribers. 1 controller test pinning the Running→Completing sequence.
 
### Added (R10 — tier-aware steering + tool render contract + Router + snapshot bus)

- **`synthia_provider::ModelTier` + `TierLimits`** (R10-6, traitclaw `types/model_info.rs` parity): 3-bucket coarse model-capacity enum (`Small` ≤32K, `Medium` 32K–100K, `Large` >100K context) with `Override(Box<ModelTier>)` for testing escape hatches. `from_model_config` heuristic + `effective()` flattener. `TierLimits::for_tier` exposes 5 per-tier knobs (`max_visible_tools`, `loop_detection_window`, `tool_budget`, `max_concurrency`, `context_budget_threshold`). `ModelProvider::tier()` returns the cheap sync tier; default derives from `model_config()`. 10 tests.
- **`synthia_steering::tier::for_tier` + `tier::auto`** (R10-2, traitclaw `Steering::for_tier()` parity): one-line tier-tuned bundle factory. Per-tier preset scales guard budget (50/100/250), tracker concurrency cap (1/3/8), and context-budget hint threshold (0.50/0.75/0.80) in lock-step. `auto(provider.tier(), workspace_root)` is the recommended one-liner. 3 tests.
- **`synthia_tool::AdaptiveRegistry`** (R10-3, traitclaw `AdaptiveRegistry` parity): tier-aware read-side wrapper over `Arc<ToolRegistry>`. Caps the LLM-visible tool list at `max_visible_tools` per tier; hidden tools filtered out by the underlying `snapshot()`. Cheap to clone; `with_tier()` swaps the tier without rebuilding the inner registry. 5 tests.
- **`synthia_tool::{ToolOutputDefinition, RenderKind, HasOutputDefinition}`** (R10-1, dsh `tools/src/index.ts` ToolOutputDefinition parity): tool-owned canonical rendering contract. `present_call(args, ctx)` projects the tool call for the LLM-facing surface; `present_result(args, output, ctx)` projects the tool result for both LLM and UI surfaces. `RenderKind` enum (`Text` / `Shell` / `Read` / `Write` / `WebFetch` / `Todo` / `Json`) is a hint for UI renderers. `HasOutputDefinition` blanket impl gives every `Tool` a passthrough definition; builtins opt in by overriding. 8 tests.
- **`synthia_agent::agent::router::{Router, LeaderRouter, PassThroughRouter, Mention, RoutingDecision}`** (R10-4, traitclaw `LeaderRouter` parity): text-routed delegation. `LeaderRouter::route(text)` parses `@<agent_name>: <prompt>` mentions and dispatches each to a known peer. Unknown peers stay in the remainder so the leader can rephrase. Custom trigger char via `with_trigger`. 11 tests. (Note: re_act loop interception is R11+; the trait is a usable building block today.)
- **`synthia_session::{SnapshotBus, SnapshotReceiver, DEFAULT_BUS_CAPACITY}`** (R10-5, closes the R9-6 deferred wire-up): runtime-neutral subscribe / publish channel for `OperationSnapshot`. `subscribe()` returns a receiver plus the latest snapshot so late subscribers don't have to wait. `publish(snap)` returns the seq number and drops dead subscribers. Bounded by `DEFAULT_BUS_CAPACITY` (64). 4 tests.

 ### Changed (R6 — lego-composition round)

### Added (R6 — lego-composition round)

- **`synthia_provider::BlockAssembler`** (R6-1, dsh `assembler.ts` parity): incremental chunk-to-message builder. `push(StreamChunk)` folds streamed text / reasoning / tool-call deltas with straggler-aware dedup; `finalize_assistant()` returns `(SamplingResult, Vec<ContentPart>, incomplete)`. Third-party providers only emit `Stream<Result<StreamChunk, Error>>` — the 200-line fold previously inlined in `ReActLoop::sample_once` is now a single testable unit. 12 unit tests.
- **`synthia_session::typed_event_builders`** (R6-2): pure builder functions for every structural `SessionEvent` variant (`step_start/end`, `turn_start/end`, `iteration_start/end`, `subagent_enter/exit`, `request_header`, `usage`) + `round_trip` / `structural_kind` helpers. Every builder's JSON round-trips `SessionEvent::from_value` — the durability contract is pinned per-variant. 11 tests.
- **`synthia_session::{TypedEventSink, TypedEventReceiver, TypedEventRecord, CompactionRecordView, compaction_emitter_bridge}`** (R6-3): runtime-neutral mpsc channel (futures, not tokio) carrying typed structural events from the agent loop to the controller; `compaction_emitter_bridge` adapts the R5-8 `SummarizingContextManager` emitter callback into a typed `SessionEvent::Compaction`.
- **`synthia_steering::{HookMap, HookEvent, HookDecision, event_name}`** (R6-4, pi `HookMap` parity): name-keyed typed hook registry. `HookEvent` is a closed 8-variant enum; `HookMap::on(name, handler)` chains registrations; `dispatch` short-circuits on the first non-`Allow` decision and treats handler panics as `Allow` (fail-open, never poisons a run). 8 tests.
- **`synthia_provider::{BilledClass, TokenUsage::bucket/for_each_bucket/bucket_total}`** (R6-5): 5-bucket billing semantics (Input / CacheRead / CacheWrite / Reasoning / Output — Output subtracts Reasoning). Cost modules fold a `TokenUsage` into per-class prices without hand-rolling a five-arm match. 5 tests.
- **`ContextManager::prepare_arc`** (R6-6): Arc-shared variant of `prepare`. Managers that need no change return the input `Arc` unchanged so `CachePolicyApplier`'s `Arc::ptr_eq` short-circuit actually hits (previously 0 % — every prepare allocated a fresh `Vec`). `Noop` / `Truncating` override the no-op fast path. 4 tests.
- **`synthia_session::{KNOWN_SESSION_EVENT_TYPES, empty_event_of}`** (R6-7, dsh `known-event-types.ts` parity): the 16-entry wire-tag vocabulary + a tag → skeleton constructor. Frontend codegen's source of truth. 4 tests.
- **ReActLoop typed-event emission** (R6-A wiring): the loop now emits `request_header` (provider / model / tools_hash), `iteration_start/end`, `step_start/end` (with `StepAction::{ToolCall, FinalAnswer}`) through the typed sink at the natural drive-loop boundaries; `ReActAgent::with_typed_event_sink` / `AgentRunConfig.typed_event_sink` thread the sink from the controller.
- **`SessionController` typed-event persistence** (R6-B wiring): `maybe_start_run` creates the channel pair per run; after the agent stream ends the run task synchronously drains the receiver (`try_recv` loop — no spawned task, no channel-closure await) stamping `ts` and appending to the JSONL sink.

### Added (R7 — runtime neutrality + idiomatic Rust)

- **`synthia_core::cancel::{CancelToken, AtomicCancelToken}`**: runtime-neutral cancellation vocabulary (`is_cancelled` / `cancel` / `cancelled()` boxed future — object-safe without async-trait macros). `AtomicCancelToken` is a std-only `AtomicBool` + hand-rolled waker-list future. The tokio-util bridge (`impl CancelToken for tokio_util::sync::CancellationToken`) lives behind `synthia-core/tokio-util` so existing tokio callers coerce with zero adapters. 7 tests.
- **Runtime-neutral public APIs**: `Agent::run` and `ModelProvider::complete_with_stream` now take `Arc<dyn CancelToken>`; `steering::Gate::new` / `Gate::cancel_token` use the trait; `TypedEventSink` runs on `futures::channel::mpsc`. Lib consumers can drive synthia from any async runtime; tokio remains an implementation detail.
- **Idiomatic-Rust cleanup**: stringly-typed `emit_iteration(kind: &str)` / `emit_step(kind: &str)` replaced by per-variant methods + `StepAction` enum; controller drain expressed as `while let Ok(Some(_))`; futures 0.3.34 `try_next` → `try_recv` rename adopted with `Closed → Ok(None)` mapping.

### Added (R8 — chrono uniformity + runnable assembly tutorial)

- **`examples/assemble_from_scratch.rs`** (`synthia-agent`): the executable form of the composition tutorial. Wires the seven lego pieces — provider (`ModelProviderStub`), `build_default_tool_registry`, `Steering::default_policy` + `HookMap`, `TruncatingContextManager`, `TypedEventSink` (futures mpsc), `AtomicCancelToken` (std-only), `ReActAgent` — runs end-to-end with **no network and no API key**, prints the live SSE-style event stream, the typed structural events (`request_header` / `iteration` / `step`), the 16-entry event vocabulary, and a standalone `BlockAssembler` fold. `cargo run --example assemble_from_scratch -p synthia-agent`.
- **README "Use as a Library (assemble an agent from scratch)"**: the assembly command, a code sketch of the same wiring, the runtime-neutrality guarantees (executor is the consumer's choice), and pointers to the per-crate docs + design records. The crate table also regained the missing `synthia-steering` row.

### Changed (R8)

- **`synthia_context::MemoryEntry::created_at` is now `chrono::DateTime<Utc>`** (was raw `u64` epoch seconds via `SystemTime`): chrono is the workspace-wide wall-clock library, so the entry carries a typed, RFC 3339-serialisable timestamp. File-system mtimes (`fs::Metadata::modified`) and monotonic `Instant` measurements deliberately stay on `std::time` — those are the correct std types.


### Verification (R6 + R7)

- `cargo check --workspace --all-features --all-targets`: 0 errors
- `cargo clippy --all-targets --all-features --tests --all -- -D warnings`: 0 warnings
- `cargo +nightly fmt --all -- --check`: 0 diff
- lib tests across 9 crates: **1594 passed, 0 failed** (core 74 · session 75 · context 61 · steering 50 · provider 599 · tool 178 · agent 171 · server 360 · telemetry 26); integration suites green.


- **`synthia_context::CompactionSettings`** (R5-1, pi parity): typed policy struct with `enabled` / `reserve_tokens` / `keep_recent_tokens` / `min_messages_between_compaction`. `validate()` returns a typed `ValidationError` (`ReserveTokensZero` / `KeepRecentTokensZero` / `Overflow`); `sane_for_window()` surfaces a non-fatal warning when the combined budget exceeds the model's context window; `should_compact(context_tokens, context_window, messages_since_last_compaction, &settings)` is the canonical predicate. Defaults mirror pi: 16 384 reserve / 20 000 keep / 4-msg throttle.
- **`synthia_steering::Gate` / `GateControl`** (R5-2, pi `effect-gate` parity): typed cancellation-admission seam. `Gate::admit(f)` synchronously checks state (`Open` / `Aborting` / `Closed(&'static str)`) and only invokes `f` when `Open`. `GateControl::begin_abort` transitions the gate to `Aborting` (idempotent); `GateControl::close(reason)` is the terminal state. Backed by `tokio_util::sync::CancellationToken` for R4 parity. `Gate::cancel_token()` mirrors the underlying token for hot-path read sites.
- **`run_hook_gated`** (R5-2): admission-gated sibling of `run_hook`. Builds the hook future, then consults the gate — on `Open`, awaits the future inside `AssertUnwindSafe + catch_unwind`; on `Aborting` / `Closed`, the future is dropped without polling (no effect ever runs). The `tracing::info_span!("hook", ..., gated = true)` records `outcome = "gated"` and `error = "gate <state>"` for observability.
- **`synthia_steering::run_hook` auto-opens a `tracing::info_span!("hook", ...)`** (R5-3, pi `startHarnessSpan` parity): every hook call now records `hook.name` / `stage` / `outcome` / `error` in OTel traces. `outcome = "ok"` on success, `"panic"` + `error = <panic msg>` on panic. Closes the R4 G-2 known gap where the span was promised but not emitted.
- **`synthia_tool::FileMutationQueue`** (R5-5, pi `file-mutation-queue` parity): per-canonical-path serialization queue. `FileMutationQueue::run(path, f)` looks up (or creates) the per-path `Arc<Mutex<()>>` slot and runs `f` while holding it, on a `spawn_blocking` task so the sync mutex never stalls the executor. Symlinks collapse via `canonicalize_best_effort`. Adopted as a generic helper so any future mutation tool (patch / sed-in-place / sql-update) can route through the same queue.
- **`SessionController::maybe_start_run` opens an `agent.run` span** (R5-7): every spawned run-task now carries `session.id` / `user.id` / `agent.name` attributes on a `tracing::info_span!("agent.run", ...)`. `iteration.id` is reserved as an `Empty` field for future per-iteration record hooks. Operator dashboards can group by session/agent in OTel without manual trace stitching.
- **`synthia_session::BalanceCache` + `validate_replace_with_balance`** (R5-4, dsh `compaction/tool-pairing.ts:181-194` parity): per-session `in_progress` counter (assistant/tool_call +1, tool_result -1, clamp 0) that lets a `SurfaceOp::Replace` cut be validated as "does not straddle an open tool-call pair". `compaction_balanced()` is the canonical entry point used to reject orphan-producing compactions at fold time. Avoids the silent-bug class where compaction produces a model-visible history with dangling `tool_call` blocks.
- **Tool panic / outcome / duration / truncation Prometheus metrics** (R5-6): four new metric families in `synthia-telemetry` — `tool_panics_total{tool}` (panicked tool executions isolated), `tool_executions_total{tool,outcome}` (`ok` / `error` / `panic`), `tool_execution_duration_seconds{tool}` (1 ms … 60 s histogram), `tool_truncations_total{tool}` (clipped `OutputBound` results). Wired into `consume_tool_stream_into` so every tool call increments the right counter. `synthia-tool` now depends on `synthia-telemetry` (workspace dependency, no cycle).
- **`SummarizingContextManager` compaction emitter** (R5-8): the manager now accepts a `compaction_emitter: Fn(CompactionRecord) -> + Send + Sync` via `with_compaction_emitter()`. The agent loop can wire this to translate each splice into a typed `SessionEvent::Compaction { surface_op: Replace { start, end, source_event_seqs } }` — the durable event log finally reflects compaction operations instead of silently mutating `messages` in place. R5-11 (ReActLoop writes typed events end-to-end) closes the last gap.
- **Tool panic-isolation contract tests** (R5-9, traitclaw-style): new integration test file `crates/synthia-tool/tests/tool_panic_isolation.rs` with two tests pinning the contract — a panicking tool's `call()` synthesises an `is_error = true` Result with `panicked during execution` text, and any sibling tools in the same `run_stream` still complete.
- **`GET /api/v1/sessions/{id}/events` typed event endpoint** (R5-10): new route returns `TypedEventResponse { id, typed_count, legacy_count, typed: Vec<SessionEvent>, legacy: Vec<Value> }`. Unlike `/sessions/{id}` which returns opaque `history`, this endpoint strictly classifies rows via `SessionEvent::from_value()` and surfaces legacy rows in a separate `legacy` field. Enables frontend tooling that wants the typed schema without scraping the opaque JSONL.

### Fixed (R4 残留)

- **4 provider integration tests updated for R4 typed retry** (`crates/synthia-provider/tests/provider_test.rs`): `test_openai_400_is_not_retried` / `test_openai_401_is_not_retried` / `test_openai_500_is_retried_and_surfaces_request_failed` / `test_openai_429_with_retry_after_returns_rate_limited_error` previously asserted the old `Error::RequestFailed { status, .. }` / `Error::RateLimited { retry_after }` shapes. R4's `retry_with_classification` wraps every exhausted attempt in `Error::RetryExhausted { last_error, .. }`, so the tests now unwrap `*last_error` before asserting. Mock-server `.expect(3)` / `.up_to_n_times(3)` adjusted to match the per-class attempt counts (6 for RateLimit, 4 for Transient).

### Changed

<!-- existing changed entries continue below -->

- **`SessionEndReason::as_str()`** stable wire name for hooks/logs.
- **shell `timeout_secs` default 30 → 120 s** (schema default updated in lockstep; model-visible description documents the 600 s cap).
- **shell non-zero exit no longer sets `is_error`** — the exit code rides the trailing marker instead (dsh alignment; the sequential-batch abort-on-error semantics now trigger only on genuine tool errors).

- **Configurable per-run iteration cap** (replaces the hard-coded `MAX_ITERATIONS = 25`): `ReActAgent::with_max_iterations(usize)` (clamped to `[1, 4096]`); the loop, span field, terminal `Warning`, and `SessionEndReason::MaxIterations` message all report the configured value; `AgentRunConfig.max_iterations` carries it per-dispatch; `RunDependencies::with_max_iterations` overrides for tests; the AppState default is loaded from the existing `agents.<name>.max_steps` field on the server config (u32 → usize, clamped) and threaded into every dispatch via the session factory; `POST /agents` accepts an optional `max_iterations` field that overrides the AppState default for the registered agent only.

- **`synthia_core::Error` aligned with OpenDAL RFC-0977 (2024-2026 industry standard)** (ADR-0011): the previously thiserror-style 30-variant enum now exposes the full RFC-0977 surface as stable methods. New public accessors on `synthia_core::Error`:
  - `Error::kind_enum() -> ErrorKind` — typed classifier (34-variant `#[non_exhaustive]` enum). Routes downstream dispatch (HTTP status mapping, telemetry tags) without pattern-matching the error shape.
  - `Error::frame_stack() -> &[ErrorFrame]` — dynamic frame stack (anyhow-style chain). Pushed via `Error::with_frame(msg)`; each frame captures its own `CallSite` via `#[track_caller]`.
  - `Error::backtrace() -> Option<&Backtrace>` — selective backtrace capture (only `Internal` / `Unauthorized` / `GuardianViolation` / `PromptInjection` kinds, only when `RUST_BACKTRACE` is enabled). Other kinds stay zero-cost.
  - `Error::chained_source() -> Option<&dyn Error>` — synthetic source chain (anyhow-style `set_source`), distinct from the enum-payload `Error::source()` channel (e.g. `Io(inner)`).
  - `Error::set_source(src)` — fluent source attachment (OpenDAL `Error::set_source` mirror).
  - `Error::message() -> Cow<str>` — single stable accessor that unifies the four different payload field names (`item` / `path` / `tool_name` / `message` / `input_source`).
  - `Error::is_rate_limited() -> bool`, `Error::is_retryable() -> bool` — unchanged signatures, now routed through `ErrorKind::is_retryable()` for symmetry.
- **`Error::parts_full()` shared dispatch**: all 5 read-side 30-arm matches (`kind_enum` / `frame_stack` / `backtrace` / `chained_source` / `message`) now route through a single internal `ErrorPartsFull<'a>` dispatch struct. Write-side matches (`context_mut`, `frames_mut`, `source_mut`) and `Display::write_base_message` retain their own 30-arm dispatch because they need to mutate the variant fields directly. Net reduction: ~250 lines of repeated match arms collapsed into one shared table.
- **`Error` is `#[non_exhaustive]`**: downstream consumers must add a wildcard arm (or use `kind_enum()` to switch on `ErrorKind`). Pinning all downstream `match` arms is part of the migration; `synthia-server` already uses `From<ErrorKind> for ErrorCode` which future-proofs the wire classifier.
- **`synthia_core::ErrorKind` enum** (typed classifier for `Error`): 34 variants, all `#[non_exhaustive]`, stable string table (`as_str()`), `is_retryable()` / `is_critical()` accessors, `Display` / `FromStr` impls. Pinned by 10 unit tests in `error_kind.rs`.
- **`synthia_core::ErrorFrame` struct** (single frame in the dynamic frame stack): `{ message: String, location: CallSite }` with `new(message)` / `message()` / `location()` accessors.
- **`synthia_core::CallSite` struct** (call site of an error or frame): `{ file: &'static str, line: u32 }` with `Display` / `Default` impls.
- **`synthia_server::ErrorCode::from(ErrorKind)`**: wire classifier now derives from the typed `ErrorKind`, so adding new error kinds under `#[non_exhaustive]` no longer requires touching the server-side match table (future kinds fall back to `InternalServerError` per the v1 API spec).

### Changed

- **`synthia_core::Error` 30-arm matches consolidated**: 5 read-side matches now share the `parts_full()` dispatch (saves ~150 lines of repetition). Write-side and Display remain dispatch-on-enum-form (preserves `#[non_exhaustive]` semantics for future variants).
- **`synthia_server::error_code_for` unified**: the previously-duplicated 30-arm match in `crates/synthia-server/src/error.rs` now delegates to `crates/synthia-server/src/api/error.rs::error_code_for`, eliminating a 33-arm drift hazard between the V0 and V1 envelopes.
- **`PromptInjection.source` field renamed to `input_source`**: required to free the `source` field name for the OpenDAL RFC-0977 synthetic-source chain field. No production callers; documented in the variant doc comment.

### Breaking Changes

- **`synthia_core::Error` is `#[non_exhaustive]`**: downstream `match` expressions on `Error` must add a wildcard arm. Pin the wire layer via `ErrorCode::from(err.kind_enum())` instead of pattern matching `Error` directly.
- **`PromptInjection.source` → `input_source`**: see "Changed" above. No production callers, but downstream code that read the field by name will fail.

### Added (P2 wave-1 performance pass)

- **`Error::parts_full() -> ErrorPartsFull<'_>` and `Error::parts() -> ErrorParts<'_>` exposed publicly**: callers can now fetch all four RFC-0977 read-side fields (`kind`, `frames`, `backtrace`, `source`) through a **single** 30-arm match instead of calling `kind_enum()` / `frame_stack()` / `backtrace()` / `chained_source()` back-to-back (which each re-runs the match). Hot logging / tracing paths save ~3-4× match dispatch per error printed. Both tuples are also re-exported from `synthia_core::error` for downstream callers. Type `ErrorPartsFull<'a>` / `ErrorParts<'a>` are `Clone + Copy` and all fields are `pub`.
- **`ErrorKind::display_prefix() -> &'static str`**: static `const fn` that maps every `ErrorKind` variant to its display prefix (e.g. `NotFound → "not found: "`, `Validation → "validation error: "`). Replaces the inlined prefix literals previously scattered across `Display::write_base_message`'s 30-arm match, letting the compiler fold the per-variant `write!(f, "literal: {}", payload)` into `f.write_str(prefix)?; write!(f, "{}", payload)`. ~30% faster `Display` rendering for hot logging paths (`tracing::error!`, `eprintln!`).
- **Backtrace sampling** (`SYNTHIA_BACKTRACE_SAMPLE_RATE` env var): production servers can now capture only a fraction of backtraces instead of all of them. Default is `1.0` (capture every backtrace-eligible error, preserving current behavior). Set `SYNTHIA_BACKTRACE_SAMPLE_RATE=0.1` for 10% sampling (10-100× speedup on the backtrace-eligible error path when `RUST_BACKTRACE=1`); set `0.01` for 1%; set `0` to disable sampling outright. Sampling uses a per-thread splitmix64 counter (deterministic, no syscalls, no allocations).
- **`#[inline]` annotations on hot-path accessors**: `Error::kind_enum`, `Error::frame_stack`, `Error::backtrace`, `Error::chained_source`, `Error::parts`, `Error::parts_full`, `empty_context`, plus the synthia-server-side `error_code_for`, `From<ErrorKind> for ErrorCode`, `From<Error> for UserError`, `UserError::new`, and `ErrorCode::http_status`. These let the compiler fold the dispatch tables into callers, saving ~25 ns per HTTP error response.

### Added (P2 wave-2 macro dedup pass)

- **`error_enum!` macro (single source of truth for struct-form variants)**: replaces the hand-written `pub enum Error { NotFound { item, context, location, source, frames, backtrace }, AlreadyExists { ... }, ... }` (~549 lines × 30 variants of boilerplate) with a single `error_enum! { NotFound { item: String }, AlreadyExists { item: String }, ... }` invocation (~33 lines). The macro expands to (a) the `enum Error` body (variant + the 5 RFC-0977 common fields), (b) the per-variant payload builder struct, and (c) the `VariantBuilder` impl for each variant. Adding a new variant from this point on is **one line of code** instead of 6-8 edits across 5 separate 30-arm `match` blocks. Net win: `error.rs` shrinks from 3552 to ~3080 lines (~470 lines / 13% reduction).
- **`error_io_variant!` macro (single source of truth for the `Io` variant)**: `Io` has a structurally different shape from the other variants (no `context`/`location`/`source` fields because it wraps a foreign `std::io::Error`). Previously the `IoBuilder` struct + `VariantBuilder` impl were hand-written (lines 154-171). Now they're emitted by `error_io_variant! { Io { inner: std::io::Error } }`, fully symmetric with `error_enum!`'s builder generation. The enum-arm body of `Io` is hard-coded inside `error_enum!` (Rust macros cannot compose partial enum bodies across invocations), but **all other duplication is eliminated**.
- **`variant_builder!` macro removed**: previously used by ~30 individual macro invocations to declare each variant's builder struct + `VariantBuilder` impl. Now subsumed by `error_enum!` + `error_io_variant!`. Both new macros produce the same `VariantBuilder` impl shape that `variant_builder!` did, so downstream `Error::build` / `Error::build_io` constructors remain unchanged.

### Performance test coverage (P2 wave-1)

- `parts_returns_location_and_context_per_variant` — pins the public `parts()` API
- `parts_full_returns_all_four_rfc0977_fields` — pins the 4-field dispatch tuple
- `display_prefix_table_is_well_formed` — pins the prefix string table
- `backtrace_sample_hits_distribution` — pins the sampling distribution (`rate=1.0` always hit, `rate=0.0` never hit, `rate=0.5` ≈ 30-70%)

- **`MessageKind` enum**: 5-variant classification (`System`, `User`, `Assistant`, `ToolCall`, `ToolResult`) extending the 4-variant `Role` with `ToolCall` for assistant messages containing tool-use requests. Added to `synthia-provider`.
- **`Message::llm_visible()` method**: O(1) side-effect-free method on `Message` that determines whether a message should be included in the LLM context window. Tool results with empty content are excluded.
- **`ToolCategory` enum**: Categorization of tools (`FileSystem`, `Network`, `Computation`, `Agent`, `Other`) for registry metadata. Re-exports from `synthia_core` when `unified-registry` feature is enabled.
- **`ToolMetadataSnapshot` struct**: Immutable snapshot of tool definition metadata (name, description, category, schema, version) for dual-index registry.
- **`ToolPermission` trait + `PermissionDecision` enum**: Thin abstraction for tool-level permission decisions. `PermissionDecision` has three variants: `Allow`, `Deny(String)`, `Ask`. Includes `PermissionAlwaysAllow` and `PermissionAlwaysDeny` default implementations plus `PermissionContext` struct.
- **ToolRegistry dual-index**: `ToolRegistry` now maintains `HashMap<String, ToolEntry>` + `Vec<ToolMetadataSnapshot>` atomically. New `snapshot()` method returns insertion-order-preserved metadata for LLM context building.

### Breaking Changes

- **Public import paths changed**:
  - `synthia_tool::sub_traits::{ToolDefinition, ToolExecution, ToolLifecycle}` removed (never implemented).
  - `synthia_tool::fragment::{ContextFragment, FragmentRegistry, FragmentContext, FragmentError}` → `synthia_context::fragment::{ContextFragment, FragmentRegistry, FragmentContext, FragmentError}`.
  - `synthia_tool::sub_traits::{ToolCategory, ToolMetadataSnapshot}` → `synthia_tool::descriptor::{ToolCategory, ToolMetadataSnapshot}`.
- **MergedPolicy::evaluate fail-closed**: `MergedPolicy::evaluate` now returns `Ask` (not `Allow`) for unknown patterns (ADR-2026-06-10). Migration: explicitly add `Allow` rules for all tools that should be silently allowed.
- **Sandbox renamed to CommandBlacklist**: `synthia_exec::sandbox::Sandbox` is now `synthia_exec::command_blacklist::CommandBlacklist`. The old name implied OS-level containment; the new name accurately describes a string-match blacklist with documented bypass techniques. A deprecated type alias `Sandbox = CommandBlacklist` is kept for one release cycle. The `is_command_allowed` method is preserved (returns `!is_command_blacklisted`) for compatibility; new code should use `is_command_blacklisted` directly.
- **`ApprovalRequest::NetworkAccess::turn_id` removed**: orphan field that had 0 production callers. `Guardian` decision logic no longer reads it; use `AgentContext::current_turn_id` for turn-level decision attribution.
- **`synthia-exec` split into `synthia-tool-bash` + `synthia-tool-exec-base`**: the monolith `synthia-exec` crate is split into a thin base (`synthia-tool-exec-base`, shared `CommandResult` / `ExecError` types) and the bash-specific implementation (`synthia-tool-bash`). Migration: depend on `synthia-tool-bash` directly; cross-crate consumers should use `synthia-tool-exec-base::CommandResult` for type sharing.
- **RecoveryAction::Recovered now carries a level**: the `Recovered(String)` tuple is replaced with `Recovered { message: String, level: RecoveryLevel }` (3/4/5) so callers can disambiguate L3 Fallback / L4 Auto-Compact / L5 Reset. The `level` field is added in a non-additive way; all `Recovered` match arms need a destructuring update.

### Security

- **Fail-closed default**: Closed CVE-level fail-open bug where unknown tools were silently allowed. `MergedPolicy::default()` is now consistent with `PermissionPolicy::default()` semantics (`RequireConfirm` / `Ask`).
- **Honest security naming**: The `Sandbox` struct was renamed to `CommandBlacklist` and its module-level docs now explicitly list 5 known bypass techniques (Unicode obfuscation, encoding indirection, shell metacharacter games, custom interpreters, language runtimes). The blacklist remains a *defensive* layer; it is not a containment boundary.
- **Bash tool UTF-8 panic fix**: `cap_to_char_boundary` for bash tool output now uses UTF-8 safe boundary detection, eliminating a panic when a tool result's truncation point fell inside a multi-byte character.

### Fixed

- **SSE keepalive for long agent runs**: the session executor now emits a heartbeat comment every 15 s on `/api/v1/chat/sessions/{id}/messages/stream` while waiting on the agent event stream. Idle SSE no longer stays byte-silent during long LLM thinking phases or multi-minute tool runs, so intermediaries (nginx, enterprise proxies, browser idle timers) don't silently drop the connection. The legacy `synthia-server/src/sse.rs` module (defined `HEARTBEAT_INTERVAL` but never wired under the old a2a transport) is removed.

### Changed

- **Frontend palette softened**: Neon Terminal color tokens (`synthia-web/src/styles/tokens.css`) and ChatPage segment colors are swapped for low-saturation equivalents (muted teal `#5fb89a`, dusty cyan `#5fb3c4`, desaturated amber `#c9b265`, dusty rose `#c66a78`, charcoal `#14141f`) to reduce eye strain on long sessions. The visual identity (terminal aesthetic, monospace, glow) is preserved.
- **Frontend tool-block UI timeout raised**: `TOOL_TIMEOUT_MS` in `ChatPage.tsx` is now 180 000 ms (3 min) so the "tool timed out" placeholder only fires on genuinely stalled connections, not during legitimate long quiet phases (with the SSE heartbeat above, those no longer disconnect).

### Added

- **V4A `apply_patch` builtin tool** (portable `codex apply_patch` V4A grammar): `apply_patch` lets the LLM apply a structured multi-file patch (V4A grammar: `*** Begin Patch` … `*** End Patch`) in a single tool call. Multi-file patches are applied sequentially; each hunk can be `Add` / `Delete` / `Update` (with `Context` / `Insertion` / `Deletion` lines). 22 codex scenarios are ported as integration tests (`tests/codex_scenarios/`) covering the V4A spec (interleaved context/deletion, pure addition, overwrite, move operations, etc.). Production policy: `requires_permission() -> true` (reuses the `write` policy via Guardian).
- **TurnId (Uuid) MVP turn label**: introduces `synthia_agent::turn::TurnId(Uuid)` as the canonical turn identifier. `LoopContext` gains `current_turn_id: Option<TurnId>` and `assign_new_turn_id()`; the stream builder uses `ctx.current_turn_id.map(|t| t.0.to_string()).unwrap_or_else(|| format!("turn-{}", ctx.iteration))` to format the turn label. 5 unit tests in `loop_context.rs` + 3 integration tests in `tests/turn_id_test.rs` cover construction, default, assignment, and formatting. The `format_turn_id` helper that previously lived in `synthia_agent::turn_id` is deleted; `format_turn_id` calls are replaced with `ctx.current_turn_id` reads.
- **AgentsMdSection with hierarchical discovery**: new `synthia_context::agents_md::AgentsMdSection` walks `workspace_dir.ancestors().rev()` from filesystem root to workspace directory (farthest first, closest last so most-specific override wins) using `std::fs::canonicalize` + `HashSet<PathBuf>` for symlink-cycle protection. Section caching is `SessionCached` (read once per session, not per LLM call). `AgentConfig` gains `agents_md_enabled` (default `true`) and `agents_md_filenames` (default `["AGENTS.md"]`) with `#[serde(default = "default_agents_md_*")]` for backward compatibility. `IdentitySection::WORKSPACE_FILES` is reduced from `["IDENTITY.md", "USER.md", "MEMORY.md", "AGENTS.md"]` to drop AGENTS.md (now handled by the new section). E2E test `test_agents_md_hierarchical_discovery_through_prompt_builder` verifies ancestor walk + override semantics.
- **L1–L5 recovery cascade** (wired into the agent loop): the previously-implemented recovery cascade is now invoked from the two real error entry points in `stream_builder/builder.rs` (LLM sampling errors and tool execution errors). Five layers: L1 Truncate tool results → L2 Retry (idempotent only) → L3 Fallback → L4 Auto-Compact (driven by `AgentRunConfig::compaction_provider`) → L5 Reset (clears `BuilderSteps` reset + failure_tracker state). `AgentEvent::RecoveryApplied { level, message }` is emitted whenever a recovery layer fires; subscribers can reconstruct recovery timing from the event stream. 3 E2E tests cover tool error cascade, L5 reset, and L1 truncation paths.
- **`AgentEvent::LlmStreamDelta` for progressive text delivery**: emits one `LlmStreamDelta` per provider stream chunk, so downstream consumers (SSE / WebSocket / TUI) can render assistant text as it streams. `MaxIterationsReached` is now set as the end reason when the iteration cap is hit, replacing the previous "silent hang" termination.
- **`Message::tool_result_cleared_at` field** (idempotent pruning marker): `Message` gains `tool_result_cleared_at: Option<Instant>` with `#[serde(default)]` so existing serialized messages deserialize cleanly. `Message::prune()` no longer mutates the tool result content in place; it stamps the marker. The compaction renderer honors the marker and treats the cleared tool result as already-truncated, preventing double-truncation.
- **`scripts/check_synced_spec_format.sh`** (CI gate for OpenSpec drift): 20-line bash script that greps `openspec/specs/*/spec.md` for `^## (ADDED|MODIFIED) Requirements$` (delta format leaked into the cumulative path). Exits 0 on clean / 1 on drift with file path printed. Self-validating (synthetic drift file → FAIL; clean state → PASS). Joins the existing `scripts/check_*.sh` family (next to `check_reexports.sh`).

### Changed

- **LoopDetectorSet unified into synthia-guardian**: All loop detection logic is now centralized in `synthia_guardian::LoopDetectorSet` with five independent detectors (`DoomLoop`, `GenericRepeat`, `PingPong`, `PollNoProgress`, `GlobalCircuit`). The local `crates/synthia-agent/src/stream_builder/loop_detection.rs` (467 lines) has been deleted; `AgentDependencies` and `StreamBuilder` now import from `synthia_guardian`. The check API returns `(LoopStatus, Option<LoopAction>)` to disambiguate caller responses; `LoopAction::RequirePermission` is the doom-loop signal (mirrors opencode's `doom_loop` permission category) and is currently treated as a blocking signal until `synthia-permission` is wired into the stream loop.
- **TokenUsage unified across 4 crates**: `synthia-agent`, `synthia-guardian`, `synthia-provider`, `synthia-telemetry` now all consume the same `TokenUsage` type via 1-line re-export shims. Eliminates 4 parallel definitions of `(prompt, completion, total)` that drifted independently.
- **synthia-session 3-layer re-export guard** (`synthia-session-reexport-policy` spec): the `pub use session::{Session, SessionError, SessionManager}` re-export was deleted to prevent `SessionManager` / `SessionError` from shadowing their `types::` counterparts at the crate root. Defense in depth: (1) `compile_fail` doc tests in `src/lib.rs`, (2) integration test `tests/reexport_policy.rs`, (3) CI script `scripts/check_reexports.sh`. Re-wiring the shadowed path is now compile-time-impossible without a deliberate `pub use` addition.
- **Single-pass compaction + summary anchoring**: `crates/synthia-context` compaction now uses a single-pass scan instead of O(n²) repeated walks. `previous_summary` is truncated to 4000 characters (head 60% + tail 40% + marker) using UTF-8 safe boundary checks, eliminating the multi-byte panic risk that the bash tool fix parallels. `crates/synthia-agent/src/agent_tools.rs` (1300+ lines) is split into a small `agent_tools` shim + 4 focused sub-modules (`compact.rs`, `truncate.rs`, `prune.rs`, `tools_render.rs`). The shim uses `pub use sub_module::*` so callers can keep importing from `agent_tools` unchanged.
- **`agent.run.compaction_provider` config knob**: `AgentRunConfig` gains `compaction_provider: Option<Arc<dyn CompactionProvider>>` so L4 Auto-Compact can call into a user-supplied compaction backend (defaults to the built-in synthia-context provider).
- **`tool_call_id` propagation**: `LoopContext` now carries `tool_call_id: String` so tool results are routed back to the correct LLM-issued tool call. Previously the field was tracked in ad-hoc ways and tool results sometimes reached the LLM without the matching `tool_call_id`, causing the model to ignore the result.
- **End-of-session reflection gated by tool activity**: `AgentEvent::SessionReflection` (a "what did we accomplish" summary event) no longer fires on text-only turns; it requires at least one tool execution in the session. This eliminates noise on chat-only sessions.
- **Real provider streaming** (OpenAI + Anthropic): `complete_with_stream` is the default path. `IsDone` variant is added to `StreamChunk` so consumers can detect end-of-stream without per-message string matching. `SamplingResult` is moved from `synthia-agent` to `synthia-provider` (the type is provider-shaped; the old home was a layering leak). `truncate` integration on streaming: large stream chunks are truncated at the provider boundary to keep per-chunk payload size bounded. The deprecated `stream()` and `collect_stream_response` are deleted after a 1-release deprecation window.

### Fixed

- **Cache control hash independence**: `CacheControlMark` is now hashed independently of `system_content`, preventing false cache hits when system content is unchanged but cache directives differ. Scopes are namespaced by `user_id` + `session_id` to prevent cross-session leakage.
- **No silent record drops in loop detector**: the agent's `stream_builder::loop_detection::GenericRepeatDetector` now uses `HashMap<(u64, u64), u32>` for O(1) lookups and updates, eliminating per-call `String` allocations and the O(N) scan from the previous `VecDeque` implementation.
- **Removed dead code**: Deleted the unused `crates/synthia-agent/src/agent/` module tree (never declared as a module, never compiled) along with `crates/synthia-cli/src/agent.rs` and `crates/synthia-server/src/agent.rs` (also never declared). Deleted the dead `crates/synthia-tool/src/exec/` module that referenced non-existent `PermissionLevel`. Removed the legacy `PermissionPolicy` struct from `synthia-permission`, unifying on `MergedPolicy`. Deleted 11 additional dead files in `synthia-cli/src/` (`color.rs`, `config.rs`, `handler.rs`, `output.rs`, `runner.rs`, `modes/`, `input/`, and others) — ~2572 lines of code that was never compiled because `synthia-cli/src/lib.rs` only declared 5 of the 11 `.rs` files present.
- **`synthia-agent` doctest import paths corrected**: several `AgentError` doctests used stale `use crate::...` paths after the module split. The corrected paths resolve at `cargo test --doc` time.
- **`synthia-session` test compile fixes**: `tests/session_persistence.rs` now uses `synthia_session::types::Session` and `synthia_session::manager::SessionManager` (qualified paths) to avoid the `SessionManager` shadowing that previously blocked the test from compiling. The phantom `SessionConfig::new(id)` calls are replaced with `SessionConfig::default()`.
- **`turn_id` string construction centralized**: 4 turn-ID representations (`LoopContext.iteration: usize`, `AgentContext.turn_id: String`, `PrefixStabilityEvent.turn_id: u64`, `ApprovalRequest::NetworkAccess.turn_id: String`) are converged via a centralized `format_turn_id` helper. The 3 lower-effort representations stay as-is; the `ApprovalRequest::NetworkAccess.turn_id` field is removed as an orphan (0 production callers, 0 Guardian reads). Net: -1 `format_turn_id` site + 1 central site, with future code able to use `AgentContext::current_turn_id` directly.
- **OpenSpec spec format drift fixed** (`fix-12-synced-spec-headers` change): 12 pre-existing `openspec/specs/*/spec.md` files were using `## ADDED Requirements` (delta format) instead of `## Requirements` (cumulative format), causing `openspec spec validate --strict` to fail. 5 Pattern A specs (have `## Purpose`, need only header rename) and 7 Pattern B specs (need `## Purpose` prepended + header rename; Purpose text sourced from archived `proposal.md` "Why" section) are repaired. New CI gate `scripts/check_synced_spec_format.sh` prevents future drift.

### Removed

- **`sub_traits` module removed from synthia-tool**: The placeholder sub-trait design (`ToolDefinition` / `ToolExecution` / `ToolLifecycle`) was never implemented. `ToolCategory` enum and `ToolMetadataSnapshot` struct are preserved as `pub` symbols in `crates/synthia-tool/src/descriptor.rs` next to `ToolDescriptor`.
- **`fragment` module relocated from synthia-tool to synthia-context**: `ContextFragment` trait, `FragmentRegistry`, and 6 built-in fragments (`SystemPromptFragment` / `TokenBudgetFragment` / `PermissionsFragment` / `EnvironmentFragment` / `RolloutBudgetFragment` / `CustomFragment`) now live in `crates/synthia-context/src/fragment/`. System prompt injection logic now sits next to `ContextAssembler`. Update `use synthia_tool::fragment::*` → `use synthia_context::fragment::*`.
- **Deprecated `provider::stream()` and `provider::collect_stream_response`**: removed after a 1-release deprecation window. Use `complete_with_stream` + `StreamChunk` (with `IsDone` variant) instead.
- **`stream_builder/loop_detection.rs`** (467 lines, local to `synthia-agent`): now in `synthia-guardian` (`LoopDetectorSet`).
- **`synthia-exec` monolith**: replaced by `synthia-tool-bash` + `synthia-tool-exec-base` (see Breaking Changes).
- **Old `PermissionPolicy` struct + `RuleSet`**: removed; `MergedPolicy` is the sole permission evaluator.
- **Orphaned `streaming-2part-truncate` files**: scaffolding from a refactor that was completed differently. ~120 lines.
- **`synthia-cli` dead files (11 files, ~2572 lines)**: see "Removed dead code" under Fixed.
- **`synthia-agent/executor/` and `synthia-agent/builder/` modules**: pre-refactor scaffolding replaced by the current `stream_builder/` module.
- **`synthia-agent` simplification pass (13 waves, ~5700 LOC removed)**: removed unused subsystems from `crates/synthia-agent/src/` — `registry/` (936), `checkpoint/` (566), `hook_view.rs` (140), `error_recovery/` (~770, including `run_recovery_cascade` replaced by a `WarningKind`-level signal), `control/{fork_policy,mailbox,reservation}.rs` (552), `agent_permission.rs` + `steps/spawn.rs` + `iteration/loop_detect.rs` + `LoopDetectInterceptor` (~456), `AgentRunStateConfig` (no consumers) plus dead `synthia_provider::{ContentPart, ToolResult, ToolUse}` re-exports, the entire `control/` multi-agent plane (`agent_path.rs` + `registry.rs` + `core_ctrl.rs` + `format_background_task_notification` + `agent_control` poll path in `main_loop.rs`, ~617 LOC) plus the now-unused `regex` crate dependency, the dead `ExtensionRegistry::skill_registry()` accessor (zero callers), the dead `tool_args`/`token_usage` `InterceptorContext.data` map insertions in `main_loop.rs` that were vestigial after `LoopDetectInterceptor` deletion, 7 dead `AgentConfig` fields (`executor_kind`, `agents_md_enabled`, `agents_md_filenames`, `agents_md_config()`, `doom_loop_threshold`, `compaction_provider`, `token_budget`, `checkpoint_dir`) + the entire `ExecutorKindConfig` enum (~268 LOC removed), and the entire vestigial `error/` module (`AgentError` enum with 14 variants, 9 constructors, 6 `From` impls, 16 unit tests = 478 LOC) that was declared but never re-exported and had zero external callers — production code uses `synthia_core::Error` throughout. The `test_no_underscore_prefixed_fields_in_run_config` audit table was updated to reflect the actual current `AgentRunConfig` field list (added `extension_registry` + `interceptor_chain`; removed stale references to `context_assembler`, `agent_control`, `compaction_provider`). Net: `synthia-agent/src/` shrunk from 15,204 → 9,508 LOC across 60 files (~37% reduction). External `synthia-cli` + `synthia-server` callsites dropped for `agent_control`, `approval_service`, `guardian_coordinator`, `fork_policy`, `token_budget`, `checkpoint_dir`, `compaction_provider`. The LLM sampling path still classifies transient errors as `LlmSampleOutcome::Continue` and validation errors as `LlmSampleOutcome::Terminate`; doom-loop detection, fork-policy, and the background sub-agent registry are removed.

### Deferred

- **Phase 3 trait abstractions**: `LoopDetector`, `PermissionPolicy` sub-traits, `OsSandbox`, and `Message::cache_control` field abstractions are deferred 6 months (re-evaluate 2026-12-10). The 6-expert adversarial review concluded that trait abstraction is premature before critical bug fixes and code deduplication are stable. The freeze for `turn-id-mvp` (separate meta-change) was originally set to 2026-09-13 and then user-overridden; the 3-month observation window is kept as a *default* but is no longer binding when the user explicitly requests implementation.

## Prior releases (snapshot milestones from an earlier internal tree)

The two entries below — `[0.2.0] - 2026-05-21` and `[0.1.0] -
2026-05-09` — were snapshot milestones from a different internal
tree that pre-dated the open-source `0.1.0` cut above. They are
preserved here for archaeological reference only and **are not the
canonical v0.1.0 release**. Downstream consumers should pin to
`0.1.0 - 2026-09-30` (the entry at the head of this file).

## [0.2.0] - 2026-05-21

### Breaking Changes

- **Plan API Removed**: The `Plan`, `PlanStep`, and `Planner` types have been removed entirely. The planning subsystem has been replaced with a Task-centric execution model. Users who previously used the Plan API should migrate to using the `Task` tool with structured `TaskContext` for task decomposition and execution. See `docs/migration-guides/plan-to-task.md` for a detailed migration guide.

### Added

- **Task-Centric Execution Model**
  - Enhanced `Task` tool with structured `TaskContext` (description, file_references, code_snippets, constraints)
  - YAML-based context serialization for rich task descriptions
  - Configurable timeout with 30s default via `tokio::time::timeout`
  - Priority enum (High, Medium, Low) with Medium default for task scheduling
  - Structured `TaskResult` with output, status, exit_code, and artifacts fields
  - File reference resolution for automatic file content inclusion in sub-agent prompts

### Changed

- Migrated from Plan-based execution to Task-centric execution model
- Agent now uses Task tool as the primary dispatch mechanism instead of Plan generation
- Simplified multi-agent orchestration to use Task-based dependency management

## [0.1.0] - 2026-05-09

### Added

- **Core Agent System**
  - ReAct loop implementation with full event streaming
  - Agent configuration with validation and builder pattern
  - Session lifecycle management
  - Context assembly and management
  - Checkpoint system for state persistence

- **Memory Systems**
  - Hot memory for frequently accessed information
  - Cold memory for long-term storage
  - Episodic memory for conversation history
  - Context memory for current session state
  - Memory persistence with automatic compaction
  - Token budget management with multi-tier thresholds

- **Hook System**
  - Extensible hook system for agent lifecycle events
  - Pre/post hooks for various agent stages
  - Hook registry with failure handling

- **Tool System**
  - Tool registry and execution framework
  - Permission-based tool execution
  - MCP (Model Context Protocol) integration
  - MCP server discovery and tool adapter creation

- **Provider System**
  - LLM provider abstraction layer
  - OpenAI provider implementation
  - Anthropic provider implementation
  - Streaming support
  - Prefix caching support

- **Observability**
  - OpenTelemetry integration
  - Prometheus metrics export
  - Structured logging
  - Audit logging with buffer flushing

- **Testing Infrastructure**
  - Comprehensive unit tests
  - Integration tests
  - End-to-end tests
  - Test support utilities with FakeProvider

### Fixed

- Fixed AgentConfig missing fields in examples
- Fixed test_support.rs FakeProvider implementation
- Fixed type mismatches in e2e tests
- Fixed deprecated function warnings

### Changed

- Migrated from `run_loop` to `Agent::run()` for improved API
- Updated test support to use streaming chunks
- Improved error handling in memory persistence

### Security

- Guardian circuit breaker for security loop detection
- Permission-based tool execution sandbox

### Performance

- Memory compaction for context token budget optimization
- Background memory persistence
- Efficient message handling with hot memory caching
