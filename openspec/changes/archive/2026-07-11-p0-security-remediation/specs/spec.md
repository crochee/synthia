# Spec: p0-security-remediation

## Requirements

### V1: Tool Argument Injection Protection

| ID | Requirement | Criteria |
|----|-------------|----------|
| V1.1 | Inject scanner scans `args_json` field | All tool call args_json scanned for injection patterns |
| V1.2 | Path traversal detection | Blocks `../`, absolute paths, null bytes, urlencode variants |
| V1.3 | Consistent action | Same block/reject action as text injection |
| V1.4 | Performance | <5ms overhead per tool call |

### V2: Edit Conflict Detection

| ID | Requirement | Criteria |
|----|-------------|----------|
| V2.1 | Conflict detection | Detects when file modified between read and write |
| V2.2 | User notification | Presents conflict UI before agent overwrites |
| V2.3 | Resolution options | User can merge, overwrite, or cancel |
| V2.4 | No silent loss | Agent never overwrites user changes without consent |

### V3: Context Overflow Explicit Handling

| ID | Requirement | Criteria |
|----|-------------|----------|
| V3.1 | Pre-compaction warning | Warns user before content is truncated |
| V3.2 | Headroom policy | Minimum 512 token headroom preserved for user content |
| V3.3 | Explicit UI | Shows "Context near limit, N tokens will be compacted" |
| V3.4 | User control | User can dismiss, adjust, or trigger compaction manually |

### Build Integrity

| Requirement | Criteria |
|-------------|----------|
| Clippy clean | `cargo clippy --all-targets --all-features --tests --all` returns 0 warnings/errors |
| Tests pass | `cargo test --workspace` passes 100% |
| No regressions | All existing tests continue to pass |

## Behavior

After all 3 vulnerabilities remediated:
- Tool calls with malicious args_json are blocked
- Edit conflicts trigger explicit user consent flow
- Context overflow surfaces warning UI before truncation
- All existing functionality preserved

## Acceptance Criteria

1. `cargo clippy --all-targets --all-features --tests --all` passes with 0 warnings
2. `cargo test --workspace` passes 100%
3. V1: Unit test confirms args_json injection blocked (path traversal patterns)
4. V2: Integration test confirms conflict UI appears on concurrent edit
5. V3: Unit test confirms pre-compaction warning shown when headroom < 512 tokens
