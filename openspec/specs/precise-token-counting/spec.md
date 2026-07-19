# precise-token-counting Specification

## Purpose
TBD - created by archiving change token-counting-compaction-config. Update Purpose after archive.
## Requirements
### Requirement: Agent SHALL count tokens using tiktoken before each LLM call

Before each LLM sampling call, the system SHALL count tokens in `ctx.messages` using `tiktoken-rs` BPE encoding, producing a precise token count for the current context.

#### Scenario: Accurate token count before sampling
- **WHEN** the context contains 2000 characters of messages
- **THEN** tiktoken SHALL produce an accurate token count (approximately 500-600 tokens for English text)
- **AND** this count SHALL be used for compaction threshold decisions

### Requirement: tiktoken encoding model SHALL be selected based on AgentConfig.model

The system SHALL use `AgentConfig.model` to select the appropriate tiktoken encoding. If the model is not a recognized OpenAI model, the system SHALL fall back to `cl100k_base` encoding.

#### Scenario: Known model uses correct encoding
- **WHEN** `AgentConfig.model = "gpt-4o"`
- **THEN** the `gpt-4o` encoding SHALL be used with tiktoken

#### Scenario: Unknown model falls back to cl100k_base
- **WHEN** `AgentConfig.model = "custom-model-v2"`
- **THEN** `cl100k_base` encoding SHALL be used
- **AND** a warning SHALL be logged

### Requirement: Token counting SHALL replace character-length estimation

The `chars / 4` estimation previously used in `StreamBuilder` SHALL be replaced by tiktoken counting. No dual-mode fallback SHALL exist.

#### Scenario: Estimation no longer used
- **WHEN** `cargo test -p synthia-agent` runs token-counting related tests
- **THEN** the token count SHALL come from tiktoken, not from character division

