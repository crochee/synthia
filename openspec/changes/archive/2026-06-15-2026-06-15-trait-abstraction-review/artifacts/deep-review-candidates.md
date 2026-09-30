# Deep-review candidates

> Auto-selected from decision matrix. Cap = 15.
> Source: `trait-inventory-classified.md` (56 traits, classified 2026-06-14)
> Strategy: include all REVIEW (3) + top 12 REMOVE_CANDIDATE by call_sites ascending (most underused first)

| # | trait | file:line | impl | methods | generics | calls | category |
|---|-------|-----------|------|---------|----------|-------|----------|
| 01 | `Job` | `crates/synthia-job/src/job.rs:8` | 1 | 3 | 0 | 9 | REVIEW |
| 02 | `Policy` | `crates/synthia-core/src/pbac/policy.rs:346` | 1 | 4 | 0 | 3 | REVIEW |
| 03 | `SteeringChannel` | `crates/synthia-agent/src/steering.rs:69` | 1 | 4 | 0 | 14 | REVIEW |
| 04 | `AuditWriter` | `crates/synthia-agent/src/audit.rs:17` | 1 | 1 | 0 | 0 | REMOVE_CANDIDATE |
| 05 | `EventStream` | `crates/synthia-server/src/event_stream.rs:64` | 1 | 1 | 0 | 0 | REMOVE_CANDIDATE |
| 06 | `Retryable` | `crates/synthia-provider/src/retry.rs:6` | 1 | 1 | 0 | 0 | REMOVE_CANDIDATE |
| 07 | `PersistenceService` | `crates/synthia-session/src/service.rs:20` | 1 | 7 | 0 | 0 | REMOVE_CANDIDATE |
| 08 | `DoomLoopHandler` | `crates/synthia-agent/src/doom_loop_handler.rs:71` | 1 | 1 | 0 | 0 | REMOVE_CANDIDATE |
| 09 | `ShellExecutor` | `crates/synthia-agent/src/shell/mod.rs:84` | 1 | 2 | 0 | 0 | REMOVE_CANDIDATE |
| 10 | `SkillMatcher` | `crates/synthia-skill/src/matcher.rs:9` | 1 | 1 | 0 | 0 | REMOVE_CANDIDATE |
| 11 | `ShellExecutor` | `crates/synthia-agent/src/shell/README.md:37` | 1 | 2 | 0 | 0 | REMOVE_CANDIDATE |
| 12 | `SkillProvider` | `crates/synthia-skill/src/traits.rs:9` | 1 | 10 | 0 | 0 | REMOVE_CANDIDATE |
| 13 | `SessionManager` | `crates/synthia-session/src/session.rs:110` | 1 | 12 | 0 | 1 | REMOVE_CANDIDATE |
| 14 | `SessionWriter` | `crates/synthia-context/src/session_writer.rs:6` | 1 | 2 | 0 | 1 | REMOVE_CANDIDATE |
| 15 | `SkillActivator` | `crates/synthia-task/src/dispatcher.rs:29` | 1 | 1 | 0 | 2 | REMOVE_CANDIDATE |

## Deferred (3 candidates not in this round)

- `McpClient` (1/1/0, calls=2) — borderline, deferred
- `RiskEvaluator` (1/1/0, calls=2) — borderline, deferred
- `AuditLogger` (1/2/0, calls=2) — borderline, deferred

(REMOVE_CANDIDATE total = 16, but cap = 15. Top-12 picked by call_sites ascending.)

## Out-of-scope (not deep-reviewed per spec)

- `KEEP-dead?` traits (8) — no impl, no call. Already classified as KEEP-dead.
  These are NOT in this list because they fall outside the "high-signal"
  REVIEW/REMOVE_CANDIDATE filter.
