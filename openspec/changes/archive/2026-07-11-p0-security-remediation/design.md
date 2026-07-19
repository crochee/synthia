# Design: p0-security-remediation

## Overview

Three independent P0 security/reliability fixes applied to synthia-tool, synthia-guardian, and synthia-context. Each vulnerability is addressed in its own module with dedicated tests.

## Goals

1. Block injection attacks via args_json path traversal
2. Detect and resolve edit conflicts before data loss
3. Surface explicit context overflow warnings to users
4. Maintain <5ms per-tool-call overhead for injection scanning
5. Keep 100% test pass rate throughout

## Constraints

- **No architectural changes** — surgical fixes only
- **Zero functional regressions** — existing behavior preserved
- **Defense in depth** — args_json scanned in addition to text
- **User agency** — conflict and overflow require user action, never silent

## Technical Approach

### V1: Tool Argument Injection Protection

**File**: `synthia-tool/src/injection.rs` (extend) + `synthia-guardian/`

**Approach**:
1. Add `scan_args_json(args: &Value) -> ScanResult` to `InjectionScanner`
2. Detect patterns: `../`, `..\\`, absolute paths (`/etc/`, `C:\`), null bytes (`\0`), URL-encoded traversal (`%2e%2e`)
3. Call `scan_args_json` alongside existing `scan_text` in tool execution pipeline
4. Return `Blocked(reason)` if any pattern found

**Key insight**: args_json is structured JSON, so traversal can be nested (`{"path": "../../etc"}`) or escaped. Must deserialize and traverse all string values.

**Performance**: Pre-compile regex patterns once at scanner init. Expected <2ms for typical args_json.

### V2: Edit Conflict Detection

**File**: `synthia-tool/src/edit_conflict.rs` (new)

**Approach**:
1. On `ToolCall::Edit` (file write), record `initial_mtime` when file is first read
2. Before write, compare `current_mtime` vs `initial_mtime`
3. If changed, emit `EditConflict { path, initial_content, current_content }` event
4. CLI layer presents UI: merge / overwrite / cancel
5. Agent waits for user resolution before proceeding

**Conflict signal**: Use mtime with 1-second tolerance (avoid false positives from filesystem jitter). For NFS/coverage gaps, also hash content on read and compare before write.

**Race prevention**: Use file locking (`flock`) during read-write sequence to ensure atomicity.

### V3: Context Overflow Explicit Handling

**File**: `synthia-context/src/compaction.rs` (extend) + `synthia-cli/`

**Approach**:
1. Track `context_tokens` and `headroom_tokens` in `ContextState`
2. Before compaction, compute `compaction_size = context_tokens - headroom_threshold`
3. If `headroom_tokens < 512`, emit `ContextOverflowWarning { tokens_to_compact, headroom }`
4. CLI renders: "⚠️ Context near limit. N tokens will be compacted. [View] [Adjust] [Compact Now]"
5. User can: view what will be compacted, increase headroom, or trigger manual compaction
6. If user is idle >30s, auto-compact with least-recently-used eviction

**Headroom policy**: Default 512 token headroom. Configurable via `SYNTHIA_CONTEXT_HEADROOM` env var.

## Impacted Files / Modules

| File | Change |
|------|--------|
| `synthia-tool/src/injection.rs` | Add `scan_args_json` method |
| `synthia-tool/src/edit_conflict.rs` | New — conflict detection module |
| `synthia-tool/src/lib.rs` | Export new module |
| `synthia-context/src/compaction.rs` | Add overflow warning emission |
| `synthia-context/src/context_state.rs` | Track headroom_tokens |
| `synthia-cli/src/ui.rs` | Render conflict and overflow UIs |
| `synthia-guardian/src/scanner.rs` | Wire args_json scanning into guardian pipeline |

## Alternatives Considered

- **V1**: Scan args_json only on "suspicious" tool names. Rejected — path traversal can appear in benign tool calls.
- **V1**: Hash-based detection instead of regex. Rejected — too many false positives with legitimate paths.
- **V2**: Timestamp-only detection. Rejected — mtime has 1-second resolution on some filesystems.
- **V2**: Auto-merge via 3-way diff. Rejected — user must consent to overwrite.
- **V3**: Silent compaction with undo. Rejected — user trust is paramount; explicit warning required.

## Risks and Mitigations

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| V1: False positives on legitimate paths | Low | Allowlist common safe paths (home dir, project root) |
| V1: Performance regression | Low | Benchmark; async scan if >5ms |
| V2: False positives on NFS/virt filesystems | Medium | Hash fallback + 1s tolerance |
| V3: Warning fatigue | Low | Only warn when headroom < 512 tokens |
