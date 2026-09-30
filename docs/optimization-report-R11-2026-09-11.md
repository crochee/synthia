# Synthia R11 — Multimodal Attachments + Router/Delegation Bridge + Snapshot Emit

> R10 shipped the tier-aware steering facade, AdaptiveRegistry,
> ToolOutputDefinition, LeaderRouter, and SnapshotBus — with two
> deferrals: the router→delegation wiring and the controller
> snapshot publishing. **R11 closes both deferrals and adds the
> multimodal layer** (the 多模态 axis of the project goal),
> closing the gap the four-reference survey flagged as the
> largest remaining: dsh's `AttachmentStore`.

## 1. Landed Items

### R11-1 — `synthia-attachment` crate (multimodal)

Adopted from dsh `packages/attachment/attachment/src/index.ts`
(`AttachmentStore`) + `packages/llm/llm/src/types.ts`
(`ImageBlock` / `ImageAttachmentRef`).

- `ImageAttachmentRef { hash, mime_type, byte_len, saved_at }` —
  content-addressed identity (lowercase-hex SHA-256) that rides
  through the LLM context instead of inline base64. chrono
  `DateTime<Utc>` for `saved_at` (R8 wall-clock convention).
- `AttachmentStore` — pluggable `AttachmentBackend` trait:
  - `FilesystemBackend`: sharded layout
    `root/{hash[0:2]}/{hash}.{ext}`, dedup-on-write (existing
    file ⇒ no-op), `NotFound` on missing hash.
  - `InMemoryBackend`: `Arc<RwLock<HashMap>>` for tests and
    ephemeral use.
  - Bounded hot-read cache (default 256 entries) with `evict()`;
    `read_image` re-verifies the hash on every backend fetch and
    returns `AttachmentError::HashMismatch` on corruption.
- Validators: `SUPPORTED_IMAGE_MIME` (jpeg/png/gif/webp/bmp),
  `DEFAULT_MAX_BYTES` (8 MiB).
- `attachment_to_content_part(ref, base64, detail)` — the adapter
  hook that renders a ref into `ContentPart::Image` for the
  provider wire format.
- `read_base64_raw(hash)` — hash-only lookup for the GET route.
- 15 unit tests (round-trip, dedup, corrupt-backend detection,
  filesystem backend, cache evict, MIME/size validation).

### R11-2 — `/api/v1/attachments` HTTP surface

- `POST /attachments` → `201` + flattened `ImageAttachmentRef`
  (MIME validated via a `validator` custom rule delegating to
  `synthia_attachment::validate_mime`).
- `GET /attachments/{hash}` → base64 bytes + ref metadata.
- `DELETE /attachments/{hash}` → cache evict (content-addressed
  files are never delete-on-evict; dedup is a separate decision).
- `AppState::attachment_store`: filesystem-backed at
  `<workspace>/.attachments` with in-memory fallback; shared
  between `new()` and `for_test()`.
- `AttachmentError → AppError` mapping onto the unified envelope:
  `unsupported_mime` 400 / `attachment_too_large` 413 /
  `attachment_not_found` 404 / corruption 500.
- 6 route tests (serde flatten, validator, store round-trip,
  error-code mapping).

### R11-3 — `mentions_to_task_specs` (closes R10-4 deferral)

`LeaderRouter::route(text)` → `Vec<TaskSpec>` bridge. `@agent:
prompt` text mentions become `task`-tool-shaped specs the
existing delegation seam (`delegation::run_subagent`) already
executes — no new loop surgery, the model's text routing rides
the proven interception path. Re-exported from `synthia_agent`.
2 tests.

### R11-4 — Controller snapshot publishing (closes R10-5 deferral)

- `ControllerInner.snapshot_bus` + `SessionController.snapshot_bus`
  share one `SnapshotBus` (Clone shares the `Arc` interior).
- Publishes at every run-state transition:
  - `Running` in `maybe_start_run` (resolved agent name +
    `default_max_iterations`),
  - `Completing` when the factory stream ends,
  - `Cancelled { reason }` in the Cancel op handler.
- `SessionController::subscribe_snapshots()` — public accessor
  returning `(SnapshotReceiver, Option<OperationSnapshot>)` so
  late subscribers get the current state immediately.
- 1 controller test pinning the Running → Completing sequence
  end-to-end through a real spawned controller.

## 2. Verification

| Gate | Result |
|---|---|
| `cargo check --workspace --all-features --all-targets` | 0 errors |
| `cargo clippy --workspace --all-features --all-targets -- -D warnings` | 0 warnings |
| `cargo +nightly fmt --all -- --check` | 0 diff |
| `cargo test --workspace` | **1861 passed, 0 failed** |

R10 baseline: 1831 tests. R11 added **+30** (attachment crate 15,
attachment routes 6, router conversion 2, controller snapshot 1,
plus count drift from fmt-normalized modules).

## 3. Lego perspective

R11 adds the multimodal brick the survey called the biggest
remaining gap:

| Brick | Crate | What it lets you do |
|---|---|---|
| `ImageAttachmentRef` + `AttachmentStore` + backends | `synthia-attachment` | Store images content-addressed; move refs (not bytes) through the LLM context; verify integrity on read |
| `POST/GET/DELETE /api/v1/attachments` | `synthia-server` | Upload / fetch / evict attachments over HTTP with the unified error envelope |
| `mentions_to_task_specs` | `synthia-agent` | Bridge text-routed `@agent:` mentions into the existing `task` delegation seam |
| `subscribe_snapshots` + transition publishing | `synthia-server` | Observe run lifecycle (Running/Completing/Cancelled) without touching run-task internals |

## 4. Out of Scope (R12+ backlog)

- dsh Cordis-style kernel (multi-round large refactor).
- MCP integration (traitclaw).
- RAG (traitclaw HybridRetriever).
- Attachment → chat-pipeline auto-inline (the store + ref types
  ship; wiring `ImageAttachmentRef` into `SendMessageRequest`
  attachments as an alternative to inline base64 is R12).
- Full Lane reducer + 13-leaf OperationState.
- dsh external subagent drivers (codex / claude-code / acp / fork).

## 5. Reference Evidence Map

- R11-1: dsh `packages/attachment/attachment/src/index.ts:1-90`;
  `packages/llm/llm/src/types.ts` ImageBlock/ImageAttachmentRef.
- R11-2: dsh HTTP attachment route conventions + synthia's
  existing AppError envelope (`api/error.rs`).
- R11-3: traitclaw `team/router.rs` `@agent:` syntax + synthia
  `delegation.rs::TaskSpec`.
- R11-4: pi `harness/runtime/types.ts` (read-only OperationState)
  + R10 `SnapshotBus`.

## 6. Cumulative R1–R11 Overview

| Round | Theme | Tests added | Total |
|---|---|---|---|
| R3 + R5 | Steering / HookMap / EffectGate / CompactionSettings | +46 | 1640 |
| R4 | TokenUsage + cache + retry typed errors | +13 | 1653 |
| R6 | BlockAssembler + HookMap + typed events + Arc-msg | +43 | 1696 |
| R7 | Runtime neutrality (CancelToken, futures mpsc) | -1 | 1695 |
| R8 | chrono + assemble_from_scratch tutorial | 0 | 1695 |
| R9 | ProviderProfile + SkillProvider + state hooks + spawn_detached + OperationSnapshot | +44 | 1739 |
| R10 | ModelTier + Steering::for_tier + AdaptiveRegistry + ToolOutputDefinition + Router + SnapshotBus | +92 | 1831 |
| **R11** | **AttachmentStore + /attachments routes + mentions bridge + snapshot emit** | **+30** | **1861** |

All 1861 tests pass; clippy 0 warnings; fmt 0 diff; check 0
errors. Zero regressions across the workspace.
