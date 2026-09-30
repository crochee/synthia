# turn-id-unify

Converge 4 `turn_id` representations in the Synthia codebase via two
minimal-viable changes: (a) centralize the `"turn-{N}"` string construction
used by `AgentContext.turn_id` into a single
`synthia_agent::turn_id::format_turn_id(iter: usize) -> String` helper, and
(b) delete the orphan `turn_id: String` field from
`ApprovalRequest::NetworkAccess` (the only one of 5 `ApprovalRequest`
variants that had a `turn_id` field, with zero production callers and no
Guardian decision logic that reads it). This is the second of three
orthogonal prerequisites for thawing the FROZEN `turn-id-mvp` change.
Net code change: < 30 lines. Zero new types introduced. Zero coordination
cost with `turn-id-mvp`.
