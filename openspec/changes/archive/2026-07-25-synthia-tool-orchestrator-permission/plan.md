# Implementation Plan: synthia-tool-orchestrator-permission

## Overview

**Change**: synthia-tool-orchestrator-permission (Change #3)
**Goal**: Connect materialization identity, category-based permissions, and capability integration to the tool execution pipeline
**Total PRs**: 12 (across 8 groups)
**Estimated effort**: 2-3 sessions

## Execution Order

### Session 1: Additive types (PRs 1.1, 3.1, 6.1) — Parallel
- PR 1.1: ToolCapabilities in ToolExecutionContext
- PR 3.1: ToolId on ToolCallRequest/Result
- PR 6.1: SandboxAttempt::Wasm stub

All three are additive struct/enum changes with no cross-dependencies.

### Session 2: Permission rewrite (PRs 2.1-2.3, 5.1-5.2) — Sequential
- PR 2.1: Category-based security_check
- PR 2.2: PermissionRule category pattern
- PR 2.3: ToolPermission deprecation
- PR 5.1: Provenance floor
- PR 5.2: Capability upgrade

### Session 3: Integration + quality (PRs 1.2, 3.2, 4.1, 7.1-7.3, 8.1)
- PR 1.2: CapabilityBroker gate in orchestrator
- PR 3.2: Orchestrator populates ToolId
- PR 4.1: OutputBound in execute_and_emit
- Quality gates

## Risk Mitigation

1. **Category-based checks miss tools**: Hybrid fallback ensures no regression
2. **OutputBound changes truncation behavior**: DefaultOutputBound matches existing caps
3. **Wasm stub is dead code**: Minimal (1 variant, 1 match arm), enables future integration without type changes
