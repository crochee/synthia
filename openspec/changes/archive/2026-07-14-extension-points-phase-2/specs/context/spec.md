# context

## ADDED Requirements

### Requirement: Context scope SHALL expose 7 extension points

The Context scope SHALL expose: `context.compact.trigger`, `context.compact.summarize`, `context.compact.replace`, `context.prefix.participate`, `context.observability.emit`, `context.token_budget.adjust`, `context.message_filter`.

#### Scenario: context.compact.summarize allows custom summary
- **WHEN** the orchestrator decides to compact
- **THEN** `context.compact.summarize` SHALL be fired with `SummarizeInput { head: String, previous_summary: Option<String>, max_tokens: u32 }`
- **AND** the extension SHALL return `Option<String>` (None = use default LLM summarization)
- **AND** if the extension returns a summary, the orchestrator SHALL use it instead of calling the LLM for summarization

#### Scenario: context.compact.replace allows custom replacement strategy
- **WHEN** `context.compact.replace` is fired
- **THEN** the extension SHALL receive `CompactPlan { tokens_to_remove: u64, strategy: CompactStrategy }` by mutable reference
- **AND** the extension MAY change `strategy` (e.g., from "drop_oldest" to "summarize_middle")
- **AND** the modified plan SHALL be executed

#### Scenario: context.message_filter runs before prefix snapshot
- **WHEN** `context.message_filter` is fired before the LLM call
- **THEN** the extension SHALL receive `Vec<Message>` by mutable reference
- **AND** the extension MAY remove, redact, or annotate messages
- **AND** the post-filter hash SHALL be the new prefix hash (P1)
- **AND** if the filter is a no-op (no changes), the prefix hash SHALL be unchanged

#### Scenario: context.token_budget.adjust is advisory
- **WHEN** `context.token_budget.adjust` is fired
- **THEN** the extension SHALL return `Option<TokenBudget { soft_limit: u32, hard_limit: u32 }>`
- **AND** if Some, the orchestrator SHALL use the new budget for compaction triggers
- **AND** if None, the default budget SHALL be used

### Requirement: Context extension points SHALL fire before prefix snapshot

All Context scope extension points that mutate the message list (`context.message_filter`, `context.compact.replace`) SHALL fire BEFORE the prefix hash is computed. The hash is then computed over the post-hook state. This is consistent with how `compact_context_tool` works in the archived change (the real compaction runs in the main loop, not in the Tool). Hooks in this scope SHALL therefore be ordered: snapshot-firing hooks run first, then the prefix hash is recomputed over the resulting state.

#### Scenario: no-op hook preserves hash
- **WHEN** `context.message_filter` returns Proceed (no changes)
- **THEN** the prefix hash SHALL be identical to the pre-hook hash
- **AND** the cache hit ratio SHALL be preserved

#### Scenario: modifying hook forces re-snapshot
- **WHEN** `context.message_filter` returns Modify with a different message list
- **THEN** the prefix hash SHALL be recomputed
- **AND** the previous cache entry SHALL be invalidated
- **AND** the agent SHALL log a `context.hash_invalidated` OTel event

### Requirement: Context used-by matrix SHALL be maintained per point

The Context scope SHALL maintain a "Used by / Reserved for" matrix for every extension point. The matrix SHALL be the single source of truth documenting which points are exercised by current code vs. reserved for future use.

| Extension point | Used by | Reserved for |
|---|---|---|
| `context.compact.trigger` | — (reserved) | External triggers (e.g., "user pressed /compact") |
| `context.compact.summarize` | — (reserved) | Custom summary strategies (e.g., extract action items only) |
| `context.compact.replace` | — (reserved) | Custom replacement strategies (e.g., "preserve last 5 turns only") |
| `context.prefix.participate` | — (reserved) | Plugins that need to be in the prefix hash computation (e.g., RAG index updates) |
| `context.observability.emit` | — (reserved) | Metrics emission (token usage, cache hit rate) |
| `context.token_budget.adjust` | — (reserved) | Per-session budget tuning |
| `context.message_filter` | — (reserved) | PII redaction, content moderation |

#### Scenario: used-by matrix SHALL be the source of truth for current consumers
- **WHEN** a developer checks which Context extension points are exercised by current code
- **THEN** the "Used by" column SHALL accurately list every internal call site
- **AND** the "Reserved for" column SHALL list at least one concrete future use case per point
- **AND** any discrepancy SHALL be reported as a documentation bug
