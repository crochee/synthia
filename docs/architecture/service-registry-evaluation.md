# synthia-service Evaluation

## ServiceRegistry vs ExtensionRegistry Boundary

### Current State

**`synthia-service::registry`**:
- `ServiceRegistry` — capability-based service registry
- Consumers: `synthia-agent`, `synthia-extension-hook`
- Provides service discovery and capability matching

**`synthia-agent::extension::ExtensionRegistry`** (defined in `crates/synthia-agent/src/extension.rs`):
- Composite of ToolRegistry + FragmentRegistry + SkillRegistry + PluginRegistry
- Active in E2E wiring path (AppState → AgentFactory → Controller → main_loop)
- Manages runtime extension registration and lookup

> Note: `FragmentRegistry` now lives at `synthia-context::fragment::FragmentRegistry` after the Wave 2 relocation.

### Responsibility Boundary

| Aspect | ServiceRegistry | ExtensionRegistry |
|--------|----------------|-------------------|
| Domain | Service discovery, capability routing | Tool/fragment/skill/plugin registration |
| Lifetime | Application-level singleton | Session-scoped, injected per-run |
| Lookup pattern | By capability | By type (tool/fragment/skill/plugin) |
| Mutation | Static after startup | Dynamic during session |
| Consumer | Agent orchestration | Main loop, system prompt, tool dispatch |

### Analysis

The two registries serve **orthogonal concerns**:

- `ServiceRegistry` answers: "What services are available and what can they do?"
- `ExtensionRegistry` answers: "What tools/fragments/skills/plugins are loaded for this session?"

There is no overlap — `ServiceRegistry` deals with coarse-grained service discovery (which external system handles what), while `ExtensionRegistry` deals with fine-grained session-level extension composition.

The naming similarity (`Service` vs `Extension`) is somewhat misleading. `ServiceRegistry` is closer to a service locator; `ExtensionRegistry` is closer to a plugin manager.

### Decision: KEEP BOTH — document boundary

No merge needed. The boundary is clean:

1. `ServiceRegistry` stays in `synthia-service` for service discovery
2. `ExtensionRegistry` stays in `synthia-core` for session-level extension management
3. Future: `ServiceRegistry` can resolve services that *provide* extensions to `ExtensionRegistry`, but this is a bridge pattern, not a merge

### Action Items

1. Add cross-reference docs in each registry module pointing to the other
2. No code changes needed — boundary is already clean
3. Consider renaming `ServiceRegistry` → `ServiceLocator` in a future cycle to reduce naming confusion
