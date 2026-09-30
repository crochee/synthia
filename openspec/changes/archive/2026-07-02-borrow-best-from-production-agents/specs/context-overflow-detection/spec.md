## ADDED Requirements

### Requirement: Context overflow detector SHALL match 21 provider-specific error patterns

The detector MUST recognize context overflow errors from at least 21 provider-specific regular expressions covering Anthropic, OpenAI, Google, and other supported providers. Each pattern MUST be documented with its provider source. Matching is case-insensitive unless the provider specifies exact case.

#### Scenario: Anthropic overflow error matched

- **WHEN** the provider returns an error containing "context length exceeded" or equivalent Anthropic-specific phrasing
- **THEN** the detector identifies it as a context overflow error
- **AND** triggers compaction or context truncation logic

#### Scenario: OpenAI overflow error matched

- **WHEN** the provider returns an error containing "maximum context length" or equivalent OpenAI-specific phrasing
- **THEN** the detector identifies it as a context overflow error
- **AND** triggers compaction or context truncation logic

---

### Requirement: Overflow detector SHALL exclude 3 non-overflow patterns

The detector MUST NOT classify the following as context overflow: (1) throttling errors, (2) rate limit errors, (3) "too many requests" errors. These MUST be excluded even if they contain the word "context" or "length".

#### Scenario: Rate limit error not classified as overflow

- **WHEN** the provider returns "Rate limit exceeded, please retry after 30s"
- **THEN** the detector does NOT classify it as overflow
- **AND** the error is propagated as a rate limit error for retry handling

#### Scenario: Throttling error not classified as overflow

- **WHEN** the provider returns "Request throttled"
- **THEN** the detector does NOT classify it as overflow
- **AND** the error is propagated as a throttling error

---

### Requirement: Detector SHALL identify silent overflow via usage metadata

When the provider returns a successful response with `usage.input_tokens + usage.cache_read_tokens > context_window_tokens`, the detector MUST classify this as a silent overflow and trigger context truncation. This catches cases where the provider silently truncates input without returning an error.

#### Scenario: Silent overflow detected from usage metadata

- **WHEN** a provider response has `usage.input_tokens = 50000`, `usage.cache_read_tokens = 80000`, and `context_window = 100000`
- **THEN** the detector identifies silent overflow (130000 > 100000)
- **AND** triggers context truncation before the next request

#### Scenario: No silent overflow when usage is within window

- **WHEN** `usage.input_tokens + usage.cache_read_tokens = 90000` and `context_window = 100000`
- **THEN** no silent overflow is flagged
- **AND** normal processing continues

---

### Requirement: Orphan tool calls SHALL synthesize empty result

When context truncation removes a tool call message without its corresponding tool result, the detector MUST synthesize an empty tool result message with content `"[orphan tool call - result truncated]"` and the correct `tool_use_id`. This prevents the next API call from failing due to missing tool_result.

#### Scenario: Orphan tool call after truncation

- **WHEN** context truncation removes a `tool_result` message but keeps the preceding `tool_use` message
- **THEN** the detector synthesizes a `tool_result` with `tool_use_id` matching the orphan
- **AND** the synthesized content is `"[orphan tool call - result truncated]"`
- **AND** the next API call succeeds without "missing tool_result" error
