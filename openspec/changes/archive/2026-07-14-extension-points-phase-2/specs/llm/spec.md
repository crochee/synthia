# llm

## ADDED Requirements

### Requirement: LLM scope SHALL expose 8 extension points

The LLM scope SHALL expose: `system_prompt.transform`, `messages.transform`, `chat.params { temperature, top_p, top_k, max_tokens }`, `chat.headers.inject`, `tool_choice.override`, `model.select`, `cache.breakpoint.set`, `response.transform`.

#### Scenario: chat.params allows modification
- **WHEN** `chat.params` is fired before an LLM call
- **THEN** the extension SHALL receive `ChatParams { temperature: f32, top_p: f32, top_k: u32, max_tokens: u32 }` by mutable reference
- **AND** any field the extension modifies SHALL be reflected in the actual LLM request
- **AND** P1 prefix consistency SHALL be preserved (modifications after the prefix hash is computed invalidate the cache — extensions modifying chat.params MUST be deterministic)

#### Scenario: messages.transform rewrites history
- **WHEN** `messages.transform` is fired before an LLM call
- **THEN** the extension SHALL receive `Vec<Message>` by mutable reference
- **AND** the extension MAY reorder, redact, or annotate messages
- **AND** the modified message list SHALL be the one sent to the LLM
- **AND** the post-transform hash SHALL be the new prefix hash

#### Scenario: cache.breakpoint placement
- **WHEN** `cache.breakpoint.set` is fired
- **THEN** the extension SHALL return `Vec<CacheBreakpoint { scope: CacheScope, ttl: CacheTtl }>`
- **AND** the orchestrator SHALL honor the breakpoints in the actual LLM request

#### Scenario: response.transform is fire-and-forget
- **WHEN** `response.transform` is fired after the LLM responds
- **THEN** the extension SHALL receive `Message` by mutable reference
- **AND** the extension MAY annotate or rewrite the response before it's added to the conversation
- **AND** the post-transform message SHALL be the one stored in the session

### Requirement: LLM extension points SHALL preserve P1 prefix consistency

LLM extension points that modify data flowing into the LLM call (`system_prompt.transform`, `messages.transform`, `chat.params`, `chat.headers.inject`, `tool_choice.override`, `response.transform`) SHALL cause the prefix hash to be recomputed after the hook chain. The hook implementations SHALL be deterministic across calls (same input → same output) to preserve the cache hit ratio.

#### Scenario: deterministic transform preserves hash
- **WHEN** `messages.transform` is fired twice with the same input
- **THEN** the post-transform hash SHALL be identical
- **AND** the cache hit ratio SHALL be unchanged

#### Scenario: non-deterministic transform is detected
- **WHEN** `messages.transform` produces different outputs for the same input
- **THEN** the agent SHALL log a warning (`extension.non_deterministic` OTel event)
- **AND** the agent MAY force a cache miss for safety

### Requirement: LLM used-by matrix SHALL be maintained per point

The LLM scope SHALL maintain a "Used by / Reserved for" matrix for every extension point. The matrix SHALL be the single source of truth documenting which points are exercised by current code vs. reserved for future use.

| Extension point | Used by | Reserved for |
|---|---|---|
| `system_prompt.transform` | — (reserved) | Adding project-specific instructions, prompt injection defenses |
| `messages.transform` | — (reserved) | Redaction (PII removal), redaction of tool results |
| `chat.params` | — (reserved) | Per-iteration temperature adjustment, A/B testing |
| `chat.headers.inject` | — (reserved) | Tracing IDs, auth tokens |
| `tool_choice.override` | — (reserved) | Forced function calling for specific scenarios |
| `model.select` | — (reserved) | Multi-model routing (cheap vs expensive) |
| `cache.breakpoint.set` | — (reserved) | Per-conversation cache tuning |
| `response.transform` | — (reserved) | Post-LLM annotations, language detection |

#### Scenario: used-by matrix SHALL be the source of truth for current consumers
- **WHEN** a developer checks which LLM extension points are exercised by current code
- **THEN** the "Used by" column SHALL accurately list every internal call site
- **AND** the "Reserved for" column SHALL list at least one concrete future use case per point
- **AND** any discrepancy SHALL be reported as a documentation bug
