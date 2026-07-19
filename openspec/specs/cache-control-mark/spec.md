# cache-control-mark Specification

## Purpose

Define a provider-neutral `CacheControlMark` struct that allows `CacheBreakDetector` to track cache control directives independently of system prompt content. Each mark carries a `CacheScope` that **MUST** include `user_id` to prevent cross-session cache leakage (per security review R2/R5/R6).
## Requirements
### Requirement: CacheControlMark struct shall encode TTL, scope, and pinned flag

`CacheControlMark` SHALL be a struct with three fields:
- `ttl: CacheTtl` — TTL class (Ephemeral / Extended / Long)
- `scope: CacheScope` — namespace string that **MUST** contain `user_id`
- `pinned: bool` — whether the cache entry is pinned to a specific prefix

`CacheTtl` SHALL be a `Copy + Eq + Hash` enum with variants `Ephemeral`, `Extended`, `Long`.

`CacheScope` SHALL be a tuple struct wrapping a `String` and SHALL be constructable via `CacheScope::new(user_id, session_id)`.

`CacheControlMark`, `CacheScope`, and `CacheTtl` SHALL be defined in a single shared crate (the public `synthia-cache-mark` crate, or a shared `synthia-types` crate if one exists) that BOTH `synthia-context` and `synthia-provider` depend on. This eliminates the previous same-name-different-shape split where `synthia-provider::cache_policy::CacheControlMark` had only `ttl_seconds` (no `scope` field).

The `synthia-provider::cache_policy::CacheControlMark` type (with `ttl_seconds` field) SHALL be removed. The provider crate SHALL re-export or use the unified `CacheControlMark` from the shared crate.

#### Scenario: Default cache control is Ephemeral

- **WHEN** a snapshot is created without explicit cache control
- **THEN** the default `CacheControlMark` SHALL have `ttl = CacheTtl::Ephemeral`, `scope = CacheScope::new("anonymous", "default")`, `pinned = false`

#### Scenario: CacheScope must contain user_id

- **WHEN** `CacheScope::new("alice", "s1")` is called
- **THEN** the resulting `CacheScope.0` SHALL contain the substring `u=alice`
- **THEN** the resulting `CacheScope.0` SHALL contain the substring `s=s1`

#### Scenario: Different users produce different scopes

- **WHEN** two `CacheControlMark` instances differ only in `scope` (different users)
- **THEN** their hashes SHALL differ

#### Scenario: Single CacheControlMark type shared across crates

- **WHEN** `synthia_context` and `synthia_provider` both reference `CacheControlMark`
- **THEN** they SHALL reference the SAME type from the shared crate
- **AND** there SHALL NOT exist a separate `CacheControlMark` in `synthia_provider::cache_policy`

#### Scenario: Provider crate carries scope field through to transform

- **WHEN** `apply_cache_policy` is invoked with a `CacheControlMark` carrying `scope = CacheScope::new("alice", "s1")`
- **THEN** the `scope` SHALL be available to `AnthropicProvider::transform_request` for inclusion in the provider cache key
- **AND** the scope SHALL NOT be silently dropped between context and provider layers

---

### Requirement: CacheBreakDetector shall hash CacheControlMark independently of system content

`create_prompt_snapshot(system_content, tools_content, model, fast_mode, cache_mark)` SHALL compute:
- `system_hash` = hash of `system_content`
- `tools_hash` = hash of `tools_content`
- `cache_control_hash` = hash of `cache_mark` (NOT `system_content`)
- `prefix_hash` = hash of `system_content`

ALL hashes (`system_hash`, `tools_hash`, `cache_control_hash`, `prefix_hash`) SHALL be computed using `ahash::AHasher::default()` (deterministic across process restarts). The previous `compute_hash` implementation using `std::collections::hash_map::DefaultHasher::new()` (which uses random SipHash13 seeds) SHALL be replaced with `ahash::AHasher::default()`.

The `cache_control_hash` SHALL be computed deterministically using `ahash::AHasher` over the canonical form `(ttl, scope.0, pinned)`.

#### Scenario: cache_control changes are detectable

- **WHEN** two snapshots have identical `system_content` but different `cache_mark`
- **THEN** `system_hash` SHALL be equal
- **THEN** `cache_control_hash` SHALL differ
- **THEN** `CacheBreakDetector::check_cache_break` SHALL report `cache_control_changed = true`

#### Scenario: cache_control hash is deterministic

- **WHEN** the same `cache_mark` is hashed twice via `create_prompt_snapshot`
- **THEN** both invocations SHALL produce the same `cache_control_hash` value

#### Scenario: cache_control hash uses ahash

- **WHEN** `create_prompt_snapshot` computes `cache_control_hash`
- **THEN** the implementation SHALL use `ahash::AHasher::default()` (NOT `DefaultHasher` which uses random seeds)

#### Scenario: system_hash is deterministic across process restarts

- **WHEN** `compute_hash("identical content")` is called in two separate process invocations
- **THEN** both invocations SHALL produce the same hash value
- **AND** the implementation SHALL use `ahash::AHasher::default()` (NOT `DefaultHasher::new()`)

#### Scenario: tools_hash is deterministic across process restarts

- **WHEN** `compute_hash` is applied to the same canonical tool schema JSON in two separate process invocations
- **THEN** both invocations SHALL produce the same `tools_hash` value

---

### Requirement: Cross-session cache leakage shall be prevented by namespace enforcement

Two `CacheControlMark` instances from different users SHALL have different `cache_control_hash` values, regardless of identical `ttl` and `pinned` values.

The provider layer SHALL translate `CacheControlMark` to provider-specific format (Anthropic `cache_control`, OpenAI `prompt_cache_key`, etc.) **including** the `scope` field as part of the cache key. The unified `CacheControlMark` type (carrying `scope`) SHALL flow from the context layer to the provider layer without intermediate lossy conversions; the provider layer SHALL NOT re-construct a separate mark type that omits `scope`.

For the Anthropic provider, when `transform_request` constructs `cache_control` JSON for tools / system / messages, the resulting cache entry SHALL be namespaced by the `scope.0` value so that two different users with otherwise identical prompts SHALL NOT share an Anthropic cache entry.

#### Scenario: Same TTL, different users, different hash

- **WHEN** `cache_mark_a = CacheControlMark { ttl: Long, scope: CacheScope::new("alice", "s1"), pinned: true }`
- **AND** `cache_mark_b = CacheControlMark { ttl: Long, scope: CacheScope::new("bob", "s1"), pinned: true }`
- **THEN** `cache_control_hash_a != cache_control_hash_b`

#### Scenario: Provider layer namespaces cache keys

- **WHEN** `provider.translate_cache_control(mark)` is called for Anthropic
- **THEN** the resulting `cache_control` JSON SHALL include both `type` (e.g. "ephemeral") AND a namespace field derived from `mark.scope.0`
- **THEN** the resulting `cache_control` JSON SHALL NOT collide with any other user's cache key

#### Scenario: Scope is not dropped between context and provider

- **WHEN** `ContextAssembler::assemble` produces a `CompletionRequest` with `cache_policy` carrying a `CacheControlMark` whose `scope = CacheScope::new("alice", "s1")`
- **AND** `AnthropicProvider::transform_request` consumes the request
- **THEN** the transform path SHALL have access to `scope` (via the unified `CacheControlMark` type)
- **AND** the scope SHALL NOT be silently dropped by an intermediate lossy type conversion

