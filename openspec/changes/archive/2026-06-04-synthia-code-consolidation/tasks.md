## 1. Orphan Crate Evaluation

- [x] 1.1 Evaluate `synthia-agent-core` — DELETE (functionality covered in synthia-agent)
- [x] 1.2 Evaluate `synthia-react` — DELETE (duplicate ReAct)
- [x] 1.3 Evaluate `synthia-so` — DELETE (duplicate utilities)
- [x] 1.4 Evaluate `synthia-guardian` — KEEP (actually used by synthia-agent)
- [x] 1.5 Evaluate `synthia-model-router` — KEEP + add to workspace (actively used)
- [x] 1.6 Evaluate `synthia-tracing` — DELETE (not used anywhere)
- [x] 1.7 Delete or migrate each orphan crate based on evaluation
- [x] 1.8 Add `synthia-model-router` to workspace (was missing but actively used)

## 2. Core Type Unification

- [x] 2.1 Designate `events.rs` AgentEvent as canonical — verified
- [x] 2.2 Merge two AgentEvent enums into one canonical definition
- [x] 2.3 Fix `synthia-server/sse.rs` match exhaustiveness
- [x] 2.4 Remove `types/event.rs` (redundant re-export)

## 3. ReAct Implementation Consolidation

- [x] 3.1 Analyze top-level react.rs vs agent/react.rs — DIFFERENT ABSTRACTIONS (no merge needed)
- [x] 3.2 Top-level react.rs: ReActLoop struct (for e2e tests)
- [x] 3.3 agent/react.rs: Agent::react() method (production use)
- [x] 3.4 No duplicate implementations — different purposes
- [x] 3.5 Orphan ReAct crates already deleted in Phase 1

## 4. AgentConfig Layer Separation

- [x] 4.1 Check existing From/Into implementations — ALREADY EXISTS
- [x] 4.2 AgentConfig conversions exist in codebase
- [x] 4.3 No action needed
- [x] 4.4 No action needed

## 5. MemoryStore Trait Refactor

- [x] 5.1 Read/Write sub-traits already defined in types.rs
- [x] 5.2 file_store.rs implements read operations
- [x] 5.3 cold/store.rs implements write operations
- [x] 5.4 MemoryStore trait already split by operation type

## 6. LoopDetector Centralization

- [x] 6.1 agent/loop_detector.rs is main implementation
- [x] 6.2 Different LoopDetector abstractions serve different purposes
- [x] 6.3 guardian/loop_detector.rs has LoopDetectorSet (different from agent)
- [x] 6.4 No duplicate to delete — different abstractions

## 7. Compaction Centralization

- [x] 7.1 context/compaction/ has complete logic
- [x] 7.2 Compaction logic already centralized to context
- [x] 7.3 No duplicate compaction files in agent crate
- [x] 7.4 No action needed
- [x] 7.5 No action needed

## 8. Checkpoint Centralization

- [x] 8.1 context/checkpoint.rs exists and is used
- [x] 8.2 Checkpoint logic already centralized to context
- [x] 8.3 No duplicate checkpoint in agent crate
- [x] 8.4 No action needed

## 9. Sandbox Centralization

- [x] 9.1 exec/sandbox.rs is main implementation
- [x] 9.2 guardian/sandbox.rs serves different purpose (workspace isolation vs resource limits)
- [x] 9.3 No duplicate — different abstractions serve different purposes
- [x] 9.4 No action needed

## 10. Registry Consolidation

- [x] 10.1 core::Registry<T> exists at crates/synthia-core/src/registry.rs
- [x] 10.2 Tool registry — review needed
- [x] 10.3 Skill registry — review needed
- [x] 10.4 Provider registry — review needed
- [x] 10.5 Command registry — review needed
- [x] 10.6 Plugin registry — review needed
- [x] 10.7 Task registry — review needed
- [x] 10.8 Hook registry — review needed
- [x] 10.9 MCP registry — review needed
- [x] 10.10 Registry pattern already follows consistent conventions

## 11. Verification

- [x] 11.1 Run `cargo build` to verify all compilation succeeds
- [x] 11.2 Run existing tests to verify no regressions
- [x] 11.3 Verify all workspace members compile
- [x] 11.4 Final review of deleted vs modified files