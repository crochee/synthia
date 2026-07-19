---
slug: "p0-security-remediation"
createdAt: "2026-07-11T11:03:46.902Z"
---

# Proposal: p0-security-remediation

## Summary

Remediation of 3 P0 security/reliability vulnerabilities identified through multi-expert adversarial analysis. These vulnerabilities can cause data exfiltration, data loss, and trust degradation in production.

## Motivation

Three P0 vulnerabilities were identified via adversarial analysis:

1. **Tool Argument Injection** — The injection scanner only scans `text` fields, but `args_json` can contain path traversal and injection payloads (e.g., `{"path": "../../../etc/passwd"}`). Currently unprotected.

2. **Edit Conflict** — When both the agent and a user edit the same file simultaneously, there is no detection or recovery mechanism. Can result in silent data loss.

3. **Context Overflow** — Context compaction silently truncates content when headroom is exhausted. Users lose data without awareness, breaking trust.

## Scope

### V1: Tool Argument Injection Protection

- Extend injection scanner to cover `args_json` fields in tool calls
- Scan for path traversal patterns (`../`, absolute paths, null bytes)
- Apply same block/reject action as text injection
- Location: `synthia-tool/src/injection.rs` + `synthia-guardian/`

### V2: Edit Conflict Detection

- Detect concurrent file edits via filesystem timestamps or content hashing
- Present user with explicit conflict resolution UI before agent proceeds
- Provide merge/diff/retry options
- Location: `synthia-tool/src/edit_conflict.rs` (new) + `synthia-cli/`

### V3: Context Overflow Explicit Handling

- Detect when compaction will truncate content
- Surface explicit UI: "Context near limit, N tokens will be compacted"
- Preserve critical user content via headroom policy
- Location: `synthia-context/src/compaction.rs` + `synthia-cli/`

## Non-Goals

- No architectural changes to permission system
- No changes to LLM provider abstraction
- No MCP protocol changes

## Risks

- **Medium risk**: Injection scanner modification could introduce latency
- **Mitigation**: Benchmark before/after, keep scanning async
- **Low risk**: Edit conflict detection could produce false positives
- **Mitigation**: Require 2-of-3 timestamp confirmation before flagging
