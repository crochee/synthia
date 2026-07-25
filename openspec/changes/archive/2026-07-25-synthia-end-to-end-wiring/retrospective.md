# retrospective.md — synthia-end-to-end-wiring

## What went well

1. **Phase 1 (wiring) completed in a single cycle** — all 40 tasks from AppState through main_loop are wired and verified.

2. **Crate consolidation partially succeeded** — session-v2 merged cleanly into session, reducing crate count by 1.

3. **Evaluation tasks produced clear decisions** — extension-v2 and synthia-service evaluations concluded "keep both" with documented boundaries.

4. **Compat strategy worked** — the `is_some()` guard pattern allowed incremental wiring without breaking the fallback path.

## What could be improved

1. **Cyclic dependency blocked event-v2 merge** — the `synthia-core` ↔ `synthia-context` cycle was not anticipated in the original plan. Pre-flight dependency graph analysis would have caught this earlier.

2. **message-proxy was always standalone** — the task to merge it into synthia-server should have been evaluated as a "should we?" before "can we?" — the answer was clearly no.

3. **Compile_fail doctests caught a subtle re-export issue** — `pub use session_v2::*` would have violated the re-export policy. The doctests caught it before it became a runtime issue.

## Lessons for future cycles

1. **Check dependency graphs before planning crate merges** — `cargo tree -p <crate>` would reveal cycles.

2. **Separate "should we merge?" from "can we merge?"** — the evaluation step should come before the implementation step.

3. **Wildcard re-exports (`pub use x::*`) are dangerous in crates with name-shadowing policies** — prefer explicit re-exports or require qualified access.

## Decisions for next cycle

1. **Extract PrefixTracker** — move `synthia_context::PrefixTracker` into a new `synthia-shared-crypto` crate (or `synthia-core`) to break the cycle and enable event-v2 merge.

2. **Rename extension-v2** — rename crate to `synthia-extension-hook` to disambiguate from `ExtensionRegistry`.

3. **Keep message-proxy standalone** — defer until a consumer needs in-process gRPC proxy.
