# provider-anthropic-signature Specification

## Purpose

Preserve Anthropic's `signature_delta` token from streaming responses so that multi-turn reasoning continuity is maintained across tool execution boundaries.

## Requirements

### Requirement: ReasoningContent carries an optional signature

The Provider `ReasoningContent` struct MUST contain a `text: String`
field and a `signature: Option<String>` field. The `signature` field
MUST be `serde(skip_serializing_if = "Option::is_none", default)`.

#### Scenario: Anthropic streaming attaches signature to ReasoningContent
- **WHEN** the Anthropic v2 streaming module encounters a `signature_delta` event
- **THEN** the signature value MUST be attached to the most recent `ContentPart::Reasoning` chunk in the accumulator
- **AND** the final aggregated `SamplingResult.reasoning_signature` MUST be the last non-`None` signature seen during the stream

#### Scenario: Non-Anthropic providers leave signature None
- **WHEN** a Provider that does not emit `signature_delta` (e.g. OpenAI) is used
- **THEN** `ReasoningContent.signature` MUST be `None`
- **AND** `SamplingResult.reasoning_signature` MUST be `None`

---

### Requirement: Cross-turn reasoning continuity is preserved

The agent MUST pass through prior reasoning blocks with their attached
signatures unchanged on every subsequent `Provider::complete` call,
so that the upstream provider can verify them.

#### Scenario: Next complete call receives prior reasoning signatures
- **WHEN** the agent invokes the Provider for a subsequent turn
- **THEN** the message history MUST include the prior reasoning blocks with their original signatures intact
- **AND** the upstream Provider MUST accept them as valid signed reasoning

---

### Requirement: ContentPart::Reasoning internal type is ReasoningContent

The `ContentPart::Reasoning` variant MUST wrap `ReasoningContent`
(rather than the legacy `TextContent`).

#### Scenario: Constructing Reasoning ContentPart uses ReasoningContent
- **WHEN** code constructs `ContentPart::Reasoning`
- **THEN** the inner type MUST be `ReasoningContent { text, signature }`
- **AND** not the legacy `TextContent { text }`
