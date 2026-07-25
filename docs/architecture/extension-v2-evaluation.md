# Extension-v2 Evaluation

## Extension trait vs ExtensionRegistry

### Current State

**`synthia-extension-v2::Extension` trait** (in `crates/synthia-extension-v2/src/lib.rs`):
- 19 typed event callbacks (on_session_start, on_pre_tool_use, etc.)
- Returns `ExtensionOutcome` (Allow/Deny/ForwardToMainAgent)
- Converts to `synthia_hook::HookOutcome` via `From` impl
- Has zero consumers — no crate depends on `synthia-extension-v2`

**`synthia-core::registry::ExtensionRegistry`** (used in Phase 1 wiring):
- Composes ToolRegistry + FragmentRegistry + SkillRegistry + PluginRegistry
- Already wired through AppState → AgentFactory → Controller → main_loop
- Used actively in the E2E wiring path

### Analysis

These are **completely different abstractions** sharing the "Extension" name:

| Aspect | Extension trait (v2) | ExtensionRegistry (core) |
|--------|---------------------|--------------------------|
| Purpose | Interceptor/hook for 19 typed events | Composite registry for tools/fragments/skills/plugins |
| Pattern | Observer/interceptor | Registry/facade |
| Relationship | Mirrors HookOutcome | Composes sub-registries |
| Usage | Zero consumers | Active in production path |

### Decision: KEEP BOTH — with name disambiguation

- `synthia-extension-v2::Extension` → rename crate to `synthia-extension-hook` to clarify it's a hook/interceptor pattern, not a registry
- `synthia-core::ExtensionRegistry` → keep name (it's a registry, correctly named)
- Bridge document: the Extension trait's `on_pre_tool_use`/`on_post_tool_use` callbacks are semantically identical to `InterceptorChain`'s `BeforeTool`/`AfterTool` events. When Extension v2 is activated, its callbacks should be routed through `InterceptorChain` rather than running as a parallel dispatch.

### Action Items

1. Rename `synthia-extension-v2` → `synthia-extension-hook` (future cycle)
2. Add `InterceptorChain` bridge that routes Extension callbacks through the existing interceptor pipeline
3. No merge needed — the two "Extension" concepts serve different purposes
