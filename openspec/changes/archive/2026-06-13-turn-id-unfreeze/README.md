# turn-id-unfreeze

Meta-change: record OpenAI codex PR #28002 (`[codex] Send turn state through
compact requests`) and PR #27996 (`[codex] Send request-scoped turn state
over WebSocket`), both merged 2026-06-13, as the concrete-use-case evidence
that satisfies the first unfreeze condition for the FROZEN `turn-id-mvp`
change. Re-evaluates the 3-month freeze period (2026-06-13 → 2026-09-13)
and formalizes the decision to maintain the freeze period without
shortening it. Zero code changes; all TurnId MVP implementation remains
gated by the three prerequisite changes (`unify-token-usage-types`,
`turn-id-unify`, `recovery-path-explicit`).
