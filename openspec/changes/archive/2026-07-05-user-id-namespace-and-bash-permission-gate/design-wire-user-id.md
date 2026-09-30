# Design: wire user_id at CLI / server / cleanup boundary

> Status: **research proposal** — awaiting review before implementation.
> Date: 2026-06-17
> Closes: 12 `TODO(user-id-namespace)` sites in CLI / server / cleanup that
> currently pass `LEGACY_USER_ID = "_legacy_"` as a placeholder.

## 1. Background

The §1 user_id namespace landed `Store` with strict `user_id` checking
(`StoreError::EmptyUserId` / `StoreError::CrossUserAccess` guards, 0o700
on-disk, etc.), but the three entry points that *consume* `Store` were left
with placeholder shims:

| Entry point        | Files                                              | Sites |
|--------------------|----------------------------------------------------|-------|
| CLI REPL           | `crates/synthia-cli/src/repl_core/repl.rs`         | 4     |
| Server routes      | `state.rs` / `chat.rs` / `session.rs` / `ws.rs`    | 5     |
| Server cleanup     | `crates/synthia-server/src/cleanup.rs`             | 3     |
| **Total**          |                                                    | **12**|

All 12 call sites import `LEGACY_USER_ID` from
`synthia_session::store::LEGACY_USER_ID = "_legacy_"`. The CLI/server
therefore creates every session in a single shared `_legacy_` namespace
and the §1 isolation guarantee is, in production, a single shared bucket
with a magic string.

## 2. Three contexts, three strategies

The right way to source `user_id` depends on the trust and identity model
of each entry point. The three are **not** interchangeable.

### 2.1 CLI REPL — local single-user (A4: random, persisted)

The CLI is a local interactive prompt; there is no auth, no request
context, no notion of "who is the user" beyond the OS session.

**Recommendation**: generate a stable random `user_id` on first run,
persist to `~/.synthia/identity` (chmod 0o600). All subsequent REPL
invocations load it. If the file is missing, regenerate. If it is
corrupted, refuse to start (fail-closed) rather than silently fall back
to `LEGACY_USER_ID`.

Rationale:
- `whoami` + hostname is OS-specific and unstable (hostname changes on
  Docker, WSL, cloud shells).
- A env var `SYNTHIA_USER_ID` is convenient in CI but should override
  the persisted identity, not replace it.
- A random persistent ID gives the user a stable namespace per machine
  for free, matches the §1 invariant (`user_id` is a filesystem
  component).

### 2.2 Server — per-API-key mapping with safe fallback (B2 + B3)

The server's `AuthConfig` already carries `api_keys: Vec<String>`, so
multi-tenant identity is a one-field extension:

```toml
[auth]
enabled = true
api_keys = ["prod-key-1", "prod-key-2"]
key_to_user = { "prod-key-1" = "team-alpha", "prod-key-2" = "team-beta" }
```

Resolution order in the auth middleware:
1. If the request's `Authorization: Bearer <key>` is in `key_to_user`,
   use that user_id.
2. Otherwise, if the key is in `api_keys` but unmapped, derive
   `user_id = hex(sha256(key))[..16]` (stable, deterministic, key-bound;
   same key → same namespace; different keys → different namespaces).
3. Otherwise, reject (current behavior).

Rationale:
- The §1 invariant requires *some* non-empty `user_id`; deriving from
  the key is safer than a shared default and gives observability per
  caller without explicit config.
- The explicit `key_to_user` map covers the "I want this key to be
  alice" case.
- This is **fail-closed**: no `LEGACY_USER_ID` fallback. If a server
  is configured without API keys (`auth.enabled = false`), reject
  agent runs at the boundary rather than auto-deriving.

### 2.3 Cleanup daemon — `Store::list_user_ids` then per-user sweep (C2)

`Store` currently exposes `list_session_ids(user_id)` /
`list_sessions_with_metadata(user_id)` but no way to enumerate users.
Cleanup therefore can't sweep all namespaces.

**Recommendation**: add a single new method to `Store`:

```rust
pub fn list_user_ids(&self) -> Result<Vec<String>, StoreError>;
```

Implementation: read one level of `sessions_root`, return the immediate
subdirectory names that are valid `user_id` (non-empty, no `.` / `..`,
pass the same charset check `user_dir` uses). Cleanup iterates the
returned IDs and reuses the existing per-user cleanup loops.

Rationale:
- One small public method is cheaper than designing a generic iterator
  type that callers can hang themselves with.
- The existing per-user functions (`list_sessions_with_metadata`, etc.)
  are unchanged, so the call site is a tight `for user_id in
  store.list_user_ids()? { ... }` loop with no other refactoring.

## 3. Cross-cutting decisions

### 3.1 `LEGACY_USER_ID` — keep but rename

Rename `LEGACY_USER_ID` → `SERVER_DEFAULT_USER_ID` in
`synthia-session/src/store.rs`. Update the doc comment to say it is
**only** acceptable as an explicit single-tenant opt-in for the
server's "no auth configured" case, and add a `#[deprecated]` attribute
on the import site pointing to the resolution path. CLI / cleanup no
longer import it at all.

### 3.2 Server-side `user_id` propagation

The auth middleware must surface the resolved `user_id` to the route
handlers. Add an `axum::Extension<RequestUserId>` (newtype around
`String`) and have the auth middleware set it. Handlers read it via the
extractor, replacing every `LEGACY_USER_ID.to_string()` literal.

### 3.3 Server-side duplicate auth (orthogonal cleanup)

`crates/synthia-server/src/auth.rs` (v2 simple fn) and
`crates/synthia-server/src/middleware/auth.rs` (tower-based `Layer`) are
duplicate code with overlapping `is_public_path` / `get_api_key` /
`validate_token`. This is *out of scope* for this change but is
flagged for a follow-up; touching both at once is bigger than the
user_id work and conflates the security model.

## 4. Scope and effort

| File                                    | Change                                          | Lines |
|-----------------------------------------|-------------------------------------------------|-------|
| `synthia-session/src/store.rs`          | +`list_user_ids` + rename `LEGACY_USER_ID`      | ~30   |
| `synthia-session/tests/store_user_id.rs`| +5 cases                                        | ~80   |
| `synthia-cli/src/identity.rs` (new)     | `Identity::load_or_create` + 0o600 persist      | ~50   |
| `synthia-cli/src/repl_core/repl.rs`     | 4 TODO → `Identity` lookup                     | ~10   |
| `synthia-cli/tests/identity.rs` (new)   | +3 cases                                        | ~50   |
| `synthia-server/src/config/server.rs`   | +`key_to_user: HashMap`                        | ~5    |
| `synthia-server/src/middleware/auth.rs` | +`RequestUserId` extension + derivation         | ~30   |
| `synthia-server/src/routes/{chat,session,ws}.rs` | 5 TODO → extension read                  | ~10   |
| `synthia-server/src/state.rs`           | 1 TODO → extension read                         | ~3    |
| `synthia-server/src/cleanup.rs`         | 3 TODO → `list_user_ids` + per-user loop         | ~30   |
| `synthia-server/tests/auth_user_id.rs` (new) | +5 cases (mapping / hash fallback / rejection) | ~80   |
| **Total**                               |                                                 | **~380** |

Roughly 200 lines of code + 180 lines of tests, all surgical.

## 5. Open questions for review

1. **CLI identity regeneration**: should `synthia-cli identity
   regenerate` be a command? Useful for "start fresh" but is a separate
   sub-command. **Recommend deferring** to a follow-up.
2. **Server `key_to_user` precedence over derivation**: B2 says explicit
   map wins, then derived, then reject. Is that the order reviewers
   want, or should the derived case reject (forcing explicit config)?
3. **Cleanup batch size**: should `list_user_ids` be bounded (e.g. max
   1000) and return a streaming iterator? Current design returns a
   `Vec<String>` which is fine up to ~10K users; not a concern unless
   this scales to huge multi-tenant deployments.

## 6. Out of scope (deferred)

- Removing the duplicate `synthia-server/src/auth.rs` (separate change,
  see §3.3).
- HMAC secret persistence for the (N/A) `synthia-prompt` crate.
- `key_to_user` UI / hot-reload (TOML reload is already supported
  through `ServerConfig::load`; just needs a test).
- Migration script for existing `_legacy_` directories: if any exist on
  disk, they're left in place; new sessions go to the correct
  namespace. A `synthia migrate-sessions` command can be added as a
  follow-up.
