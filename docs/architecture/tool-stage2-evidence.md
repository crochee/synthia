# Stage 2 Evidence — `synthia-tool` Responsibility Convergence

**Source plan:** `docs/superpowers/plans/2026-08-02-synthia-tool-responsibility-convergence.md` (Stage 2)
**Captured:** Stage 2 execution session
**Stage 1 commits:** `b72dc0f3` (consolidate registry into one module), `9ee7801c` (make `ToolFilter` crate-private)

## Stage 2 Decision

> **Keep** `RegistrationScope`, `RegistrationToken`, `register_scoped`, and `create_session_scope`.
> **Delete** `ToolProvider` and `register_provider` in Stage 4 (after `register_scoped_arc` is added).

The two Task 2.1 evidence gates both pass: `RegistrationScope` is a live RAII contract referenced by production code, while `synthia_tool::ToolProvider` has no production caller outside `synthia-tool`'s own test fixtures.

---

## Task 2.1 Step 1 — `RegistrationScope` live-contract evidence

### Grep A — `RegistrationScope` consumers in `loop_context.rs`

```bash
grep -n "RegistrationScope\|registration_scope\|with_registration_scope" \
  crates/synthia-agent/src/loop_context.rs
```

Output:

```
5:use synthia_tool::RegistrationScope;
42:    pub registration_scope: Option<RegistrationScope>,
60:            registration_scope: None,
80:    /// Attach a [`RegistrationScope`] so that tools registered during
83:    pub fn with_registration_scope(mut self, scope: RegistrationScope) -> Self {
84:        self.registration_scope = Some(scope);
116:            registration_scope: None,
```

**Interpretation:** `RegistrationScope` is imported (`line 5`), stored on `LoopContext` (`line 42`), constructed via `with_registration_scope` (`line 83`), and reset on `Drop`/`clone` (`lines 60, 116`). This is a live production contract — deleting it would break the agent loop.

### Grep B — `register_provider` / `register_scoped` / `create_session_scope` callers (whole workspace)

```bash
grep -RIn "register_provider\|register_scoped\|create_session_scope" \
  crates/synthia-tool/src crates/synthia-tool/tests \
  crates/synthia-agent/src crates/synthia-agent/tests \
  crates/synthia-skill/src crates/synthia-server/src \
  crates/synthia-server/tests test-support
```

Output (filtered to non-test paths; the `registry.rs` test fixture matches are listed separately):

| Caller | Line | Kind |
|---|---|---|
| `crates/synthia-tool/src/registry.rs:351` | `pub async fn register_provider(...)` | definition |
| `crates/synthia-tool/src/registry.rs:631` | `pub async fn register_scoped(...)` | definition |
| `crates/synthia-tool/src/registry.rs:665` | `pub async fn register_scoped_with_namespace(...)` | definition |
| `crates/synthia-tool/src/registry.rs:703` | `pub fn create_session_scope(...)` | definition |
| **`crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs:191`** | `extension_registry.tool_registry().create_session_scope()` | **production caller** |
| `crates/synthia-tool/src/registry.rs` (28 occurrences) | inside `mod tests` | unit-test fixtures only |

Full breakdown:

```
crates/synthia-tool/src/registry.rs:351:    pub async fn register_provider(
crates/synthia-tool/src/registry.rs:433:            // `register_provider`. The caller has no way to recover the
crates/synthia-tool/src/registry.rs:631:    pub async fn register_scoped(
crates/synthia-tool/src/registry.rs:635:        let token = self.register_provider(provider).await?;
crates/synthia-tool/src/registry.rs:665:    pub async fn register_scoped_with_namespace(
crates/synthia-tool/src/registry.rs:670:        let token = self.register_provider(provider).await?;
crates/synthia-tool/src/registry.rs:696:    /// Unlike [`register_scoped`](Self::register_scoped), this does not
crates/synthia-tool/src/registry.rs:703:    pub fn create_session_scope(self: &Arc<Self>) -> RegistrationScope {
crates/synthia-tool/src/registry.rs:921:/// Created by [`ToolRegistry::register_scoped`] or
crates/synthia-tool/src/registry.rs:922:/// [`ToolRegistry::register_scoped_with_namespace`]. When the scope
crates/synthia-tool/src/registry.rs:1420:        let token = registry.register_provider(provider).await.unwrap();   // test
crates/synthia-tool/src/registry.rs:1447:        let token1 = registry.register_provider(p1).await.unwrap();     // test
crates/synthia-tool/src/registry.rs:1448:        let _token2 = registry.register_provider(p2).await.unwrap(); // test
crates/synthia-tool/src/registry.rs:1478:        let token1 = registry.register_provider(p1).await.unwrap();     // test
crates/synthia-tool/src/registry.rs:1479:        let _token2 = registry.register_provider(p2).await.unwrap(); // test
crates/synthia-tool/src/registry.rs:1501:        let token = registry.register_provider(provider).await.unwrap(); // test
crates/synthia-tool/src/registry.rs:1537:        let _token = registry.register_provider(provider).await.unwrap(); // test
crates/synthia-tool/src/registry.rs:1559:            let scope = registry.register_scoped(provider).await.unwrap();  // test
crates/synthia-tool/src/registry.rs:1580:            .register_scoped_with_namespace(provider, "my-namespace")    // test
crates/synthia-tool/src/registry.rs:1608:        let scope1 = registry.register_scoped(p1).await.unwrap();         // test
crates/synthia-tool/src/registry.rs:1609:        let scope2 = registry.register_scoped(p2).await.unwrap();         // test
crates/synthia-tool/src/registry.rs:1634:        let scope = registry.register_scoped(provider).await.unwrap();  // test
crates/synthia-tool/src/registry.rs:1654:        let scope = registry.register_scoped(provider).await.unwrap();  // test
crates/synthia-tool/src/registry.rs:1673:        let scope = registry.register_scoped(provider).await.unwrap();  // test
crates/synthia-tool/src/registry.rs:1689:            let scope = registry.register_scoped(provider).await.unwrap(); // test
crates/synthia-tool/src/registry.rs:1716:        let token = registry.register_provider(provider).await.unwrap(); // test
crates/synthia-tool/src/registry.rs:1945:    // ── create_session_scope tests ──────────────────────────────────────
crates/synthia-tool/src/registry.rs:1948:    fn create_session_scope_returns_valid_token() {
crates/synthia-tool/src/registry.rs:1950:        let scope = registry.create_session_scope();
crates/synthia-tool/src/registry.rs:1960:    fn create_session_scope_drop_is_noop_when_no_tools_registered() {
crates/synthia-tool/src/registry.rs:1964:            let _scope = registry.create_session_scope();
crates/synthia-tool/src/registry.rs:1971:    fn create_session_scope_subsequent_tokens_are_monotonic() {
crates/synthia-tool/src/registry.rs:1973:        let scope1 = registry.create_session_scope();
crates/synthia-tool/src/registry.rs:1974:        let scope2 = registry.create_session_scope();
crates/synthia-tool/src/registry.rs:1982:    async fn create_session_scope_drop_does_not_affect_other_tools() {
crates/synthia-tool/src/registry.rs:1991:        let _token = registry.register_provider(provider).await.unwrap(); // test
crates/synthia-tool/src/registry.rs:1996:            let _scope = registry.create_session_scope();
crates/synthia-tool/src/registry.rs:2004:    fn create_session_scope_noop_when_registry_dropped_first() {
crates/synthia-tool/src/registry.rs:2006:        let scope = registry.create_session_scope();
crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs:191:                let scope = extension_registry.tool_registry().create_session_scope();
```

**Interpretation:**
- `create_session_scope` has **one production caller** in `synthia-agent/src/stream_builder/builder/run/main_loop.rs:191` — this is the live RAII contract for the agent's per-loop scope.
- `register_provider` has **zero production callers** outside the test fixtures inside `crates/synthia-tool/src/registry.rs` (the `// test` annotations in the table above). The `register_scoped` and `register_scoped_with_namespace` methods internally call `register_provider`, but no external crate calls them via `ToolProvider` either (see Task 2.1 Step 2 evidence below).

**Decision:** keep `RegistrationScope`, `RegistrationToken`, `register_scoped`, `register_scoped_with_namespace`, `create_session_scope`, `unregister_by_token`. Delete `register_provider` in Stage 4 (alongside `ToolProvider` and the `SimpleProvider`/`MultiProvider` test fixtures, after `register_scoped_arc` lands as the replacement).

---

## Task 2.1 Step 2 — `ToolProvider` deletion evidence

### Grep C — qualified references to `synthia_tool::provider` / `synthia_tool::ToolProvider`

```bash
grep -RIn "synthia_tool::provider\|synthia_tool::ToolProvider\|ToolProvider\b" \
  crates/synthia-tool crates/synthia-agent crates/synthia-skill \
  crates/synthia-server crates/synthia-context \
  test-support
```

**Important note on grep coverage:** the literal regex above includes an unqualified `ToolProvider\b` segment that matches *two distinct traits* in the workspace:
1. `synthia_tool::provider::ToolProvider` (the deletion candidate, defined at `crates/synthia-tool/src/provider.rs:11`).
2. `synthia_agent::tools::dynamic_provider::tool_provider::ToolProvider` (defined at `crates/synthia-agent/src/tools/dynamic_provider/tool_provider.rs:64` — this is the agent-side provider abstraction for the extension manager, NOT a tool registry provider).

The unqualified matches are all (2). The qualified matches below are exactly (1).

### Qualified-only matches (the deletion candidate)

```bash
grep -RIn "synthia_tool::provider\|synthia_tool::ToolProvider\|crate::registry::.*ToolProvider" \
  crates/synthia-tool crates/synthia-agent crates/synthia-skill \
  crates/synthia-server crates/synthia-context \
  test-support
```

Output:

```
(no output — zero matches)
```

So **no external crate references `synthia_tool::provider::ToolProvider` or `synthia_tool::ToolProvider` by its qualified path**. The internal references within `synthia-tool` itself:

| File | Line | Kind |
|---|---|---|
| `crates/synthia-tool/src/provider.rs:1` | `//! ToolProvider trait — the registration contract for tools.` | doc comment on definition |
| `crates/synthia-tool/src/provider.rs:11` | `pub trait ToolProvider: Send + Sync + 'static {` | definition |
| `crates/synthia-tool/src/registry.rs:24` | `provider::ToolProvider,` | use-stmt (intra-crate) |
| `crates/synthia-tool/src/registry.rs:351`–`670` | `provider: Arc<dyn ToolProvider>` parameter | inherent method bodies |

### Unqualified matches (both traits — informational)

For completeness, the unqualified `ToolProvider\b` matches (all of which are the unrelated synthia-agent trait except inside `synthia-tool`):

```
crates/synthia-tool/src/provider.rs:1,11
crates/synthia-tool/src/registry.rs:24, 353, 411, 633, 667, 1321–1986 (uses + test fixtures)
crates/synthia-agent/src/tools/dynamic_provider/{mod,tool_provider,extension_manager,extension_context}.rs
crates/synthia-agent/src/tools/providers/{external_hook,tool_search,bash,file,monitor}_tools_provider.rs
crates/synthia-agent/src/tools/providers/mod.rs:5,18
crates/synthia-agent/tests/{9_abstractions,bash_tools_provider_test}.rs:24,8
```

The agent-side `ToolProvider` trait is part of the dynamic extension manager subsystem (10 files) and is structurally unrelated to the `ToolRegistry` provider shim. **Deleting `synthia_tool::provider::ToolProvider` does not affect the agent-side trait.**

**Decision:** `synthia_tool::provider::ToolProvider` and `ToolRegistry::register_provider` are safe to delete in Stage 4. Stage 4 must add `register_scoped_arc` (the scoped-cleanup replacement that takes a `ToolEntry` directly, bypassing the `ToolProvider` indirection) and migrate the test fixtures before deletion.

---

## Stage 2 Outcome Summary

| Gate | Outcome | Decision |
|---|---|---|
| `RegistrationScope` live contract | Imported + stored + constructed by `LoopContext` | **Keep** in Stage 2 |
| `register_provider` external callers | Zero (only used inside `synthia-tool` tests) | **Delete in Stage 4** after `register_scoped_arc` lands |
| `register_scoped` / `register_scoped_with_namespace` callers | Zero external | **Keep** (legacy provider-based path; deleted together with `register_provider` in Stage 4) |
| `create_session_scope` external caller | `main_loop.rs:191` | **Keep** |
| `synthia_tool::ToolProvider` external references | Zero | **Delete trait + module in Stage 4** |