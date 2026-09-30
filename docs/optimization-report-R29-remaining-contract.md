# R29 remaining phases — shared contract

Binding interface for the three parallel workstreams. Do NOT
deviate. Everything below is Rust; edition 2024; workspace rules
apply (lib crates use `thiserror`; deps via
`workspace = true`; `chrono` for time; `major.minor` versions;
no `dead_code`/`unused` suppression; no wildcard imports).

## Workstream A — `crates/synthia-session` (session only)

### A1. New `SessionEvent` variants (in `src/events.rs`)

Add to the existing `#[serde(tag = "type", rename_all = "snake_case")]`
enum `SessionEvent`. All are **log-only** (never surface-eligible
— do NOT add them to `is_surface_eligible`).

```rust
/// One streamed provider delta, preserved verbatim for
/// replay fidelity. Log-only.
#[serde(rename = "assistant_chunk")]
AssistantChunk {
    seq: u64,
    ts: String,
    /// Monotonic ordinal of this chunk within its turn
    /// (0-based, assigned by the emitter).
    chunk_seq: u64,
    /// Provider stop reason, when this chunk is terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    finish_reason: Option<String>,
    data: Value,
},

/// Compaction began. Log-only. An unmatched `CompactionStart`
/// (no matching `CompactionEnd`) is the crash marker the
/// loader detects.
#[serde(rename = "compaction_start")]
CompactionStart {
    seq: u64,
    ts: String,
    /// Opaque id correlating Start/Summary/End.
    token: String,
    data: Value,
},

/// Compaction produced a summary. Log-only.
#[serde(rename = "compaction_summary")]
CompactionSummary {
    seq: u64,
    ts: String,
    token: String,
    data: Value,
},

/// Compaction finished (success or failure). Log-only.
#[serde(rename = "compaction_end")]
CompactionEnd {
    seq: u64,
    ts: String,
    token: String,
    /// `"committed"` | `"failed"` | `"interrupted"`.
    outcome: String,
    data: Value,
},

/// Session shutdown lifecycle. Log-only. Emitted by the
/// server's `SessionController::close` after all child
/// sessions have been signalled.
#[serde(rename = "session_shutdown")]
LifecycleShutdown {
    seq: u64,
    ts: String,
    data: Value,
},
```

Update `seq()` and any exhaustive matches (e.g.
`ts()` helpers, `empty_event_of`) to cover the new variants.
Add the new `type` tags to `KNOWN_SESSION_EVENT_TYPES`.

### A2. `SurfaceToken` + `CompactionOutcome` value types

```rust
/// Opaque correlation id for one compaction lifecycle.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SurfaceToken(String);

impl SurfaceToken {
    #[must_use]
    pub fn new() -> Self { Self(ulid::Ulid::generate().to_string()) }
    #[must_use]
    pub fn from_string(s: impl Into<String>) -> Self { Self(s.into()) }
    #[must_use]
    pub fn as_str(&self) -> &str { &self.0 }
}
impl Default for SurfaceToken { fn default() -> Self { Self::new() } }
impl std::fmt::Display for SurfaceToken { /* write_str */ }

/// Terminal state of one compaction lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionOutcome { Committed, Failed, Interrupted }
```

`synthia-session` may need `ulid` as a dep — it is not currently
listed. Add `ulid.workspace = true` to its `Cargo.toml`
`[dependencies]`. If `ulid` is already there, leave it.

### A3. Fold helper `assemble_chunks` (in `src/surface.rs`)

```rust
/// Join a run of `AssistantChunk` events (filtered from a
/// session log, in log order) into the assembled assistant
/// text. Chunks are ordered by `chunk_seq` ascending; `data`
/// carries `{"delta": "<text>"}`. Non-text chunks contribute
/// nothing. Returns the concatenated string.
#[must_use]
pub fn assemble_chunks(events: &[SessionEvent]) -> String
```

### A4. Orphan detector (in `src/repair.rs`)

```rust
/// A `CompactionStart` with no matching `CompactionEnd`. The
/// crash marker: the process died mid-compaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanedCompaction {
    /// `token` of the unmatched Start.
    pub token: String,
    /// `seq` of the unmatched Start event.
    pub start_seq: u64,
}

/// Scan a session log for compaction lifecycles that began but
/// never ended. Returned in log order. Pure.
#[must_use]
pub fn orphaned_compactions(events: &[SessionEvent]) -> Vec<OrphanedCompaction>
```

### A5. Tests (in `synthia-session`, `#[cfg(test)]`)

- `assistant_chunk` round-trips through serde with the
  `assistant_chunk` tag; `finish_reason` omitted when `None`.
- `assemble_chunks` on 5 chunks with `chunk_seq` 0..5 and
  `data.delta` = ["a","b","c","d","e"] yields `"abcde"`.
- `orphaned_compactions`: `[Start(t1), End(t1), Start(t2)]`
  yields exactly `[{token: t2, ...}]`.
- `LifecycleShutdown` round-trips.
- `CompactionOutcome` serde is `snake_case`.

Do NOT edit anything outside `crates/synthia-session/`.

## Workstream B — `crates/synthia-server` (server only)

Depends on A1: `synthia_session::SessionEvent::LifecycleShutdown`
exists after A lands. You may write code that names it; the
crate will not compile until A lands — that is expected, do NOT
try to fix it by editing synthia-session.

### B1. Phase I — session shutdown lifecycle

In `src/session/controller.rs`:

- Add `pub async fn close(&self)` on the controller type
  (find the actual controller type in that file — it owns the
  run handle and the child-session bookkeeping).
- `close` MUST:
  1. Signal every child session it tracks (cancel their runs).
  2. Emit a `LifecycleShutdown` typed event into the session
     sink **after** all children are signalled.
  3. Bound the whole shutdown with a **3-second** deadline
     (`tokio::time::timeout`), so a hung child cannot keep the
     process alive. On timeout, log a `warn!` and return.
- `close` MUST be idempotent: a second call is a no-op.
- If the controller uses `synthia_session::TypedEventSink`,
  emit via that; otherwise append a serialized
  `SessionEvent::LifecycleShutdown` through the existing sink
  path the file already uses for other typed events.

Add a test in `controller.rs`'s `#[cfg(test)]` module:
- Build a controller with 2 tracked children (use the existing
  test fixtures in that file — `VecFactory` etc.).
- `close().await` completes; both children received the
  shutdown signal; a second `close()` is a no-op.

### B2. Phase M — `POST /api/v1/runs/{agent}/operation`

New wire union `OperationRequest` (place in
`src/api/operation.rs`; register in `src/api/mod.rs`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationRequest {
    Prompt { text: String },
    Skill { name: String, args: serde_json::Value },
    PromptTemplate { name: String, vars: serde_json::Value },
    Compaction { force: bool },
    Navigation { target: String, args: serde_json::Value },
}
```

Config: add `enable_operation_endpoint: bool` (default `false`,
`#[serde(default)]`) to the server config struct in
`src/config/server.rs`.

Route: in `src/routes/` add the handler + register
`POST /api/v1/runs/{agent}/operation` **only when the flag is
on**. The handler:
- deserializes `OperationRequest`,
- dispatches `Prompt` to the same code path the existing chat
  prompt route uses (find it in `src/routes/chat.rs` — reuse the
  existing helper; do NOT duplicate logic),
- for `Skill`/`PromptTemplate`/`Compaction`/`Navigation`,
  return `501 NOT_IMPLEMENTED` with the standard error envelope
  (they are declared for wire-compat; the server does not yet
  implement them),
- when the flag is off, the route is absent (router falls
  through to the existing 404 fallback — that is the expected
  behaviour, do not add a stub 404 route).

Since the router is built at boot, gate it at build time:
`create_router` reads the flag off `AppState` (or config) and
conditionally chains the route.

Tests (server):
- round-trip each `OperationRequest` variant through
  serde_json and assert the `kind` tag.
- with the flag off, `POST .../operation` → 404.
- with the flag on, `POST .../operation` with
  `{"kind":"prompt","text":"hi"}` reaches the prompt path (200
  or the same status the existing prompt route returns for an
  unknown agent).

Do NOT edit anything outside `crates/synthia-server/`.

## Workstream C — `crates/synthia-context` (context only)

### C1. `CompactionDetails` (new type in `src/compaction_settings.rs`)

```rust
/// File-activity detail recorded alongside one compaction so
/// the *next* compaction's summariser prompt can name what
/// touched disk in the most recent run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionDetails {
    #[serde(default)]
    pub read_files: Vec<std::path::PathBuf>,
    #[serde(default)]
    pub modified_files: Vec<std::path::PathBuf>,
}
```

### C2. Extend `CompactionSettings`

`CompactionSettings` is currently `#[derive(Clone, Copy, ...)]`.
Adding `PathBuf` vectors breaks `Copy`. **Remove `Copy`**, keep
`Clone`. `read_files`/`modified_files` do NOT belong on the
policy struct — the policy is a copy-able knob; details are
per-compaction data.

Instead:
- Keep `CompactionSettings` as-is but drop `Copy` only if
  needed — if you can avoid touching it, do not change it.
- Add a **separate** knob to `SummarizingContextManager`:
  `with_compaction_details(details: CompactionDetails)` (a
  builder method that stores `Option<CompactionDetails>`), and
  a getter. The manager passes `details` into
  `serialise_batch_for_summariser` (append a short
  "files touched since the last compaction: …" line to the
  prompt).

### C3. Tests (context)

- `CompactionDetails` serde round-trips; default is empty.
- A `SummarizingContextManager` built with
  `with_compaction_details(CompactionDetails { read_files:
  ["a.rs"], modified_files: ["b.rs"] })` includes both paths in
  the serialized batch handed to the summariser (use a capturing
  `SummariseFn`).

Do NOT edit anything outside `crates/synthia-context/`.

## Verification (parent only — subagents MUST NOT run cargo)

Subagents must NOT run `cargo build` / `cargo test` /
`cargo clippy`. The parent runs them after all three land.
