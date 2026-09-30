# provider-trait Specification

## Purpose
TBD - created by archiving change agent-toolification-v3. Update Purpose after archive.
## Requirements
### Requirement: Provider Trait Abstraction

The `synthia-llm` crate MUST export a `Provider` trait that abstracts LLM client operations. The trait MUST be object-safe and SHALL expose at minimum: `complete()`, `stream()`, `model_id()`, and `capabilities()`. The `Agent` MUST NOT hold a concrete `LlmClient`; it MUST hold `Arc<dyn Provider>`.

#### Scenario: Provider Trait Surface

- **WHEN** a developer inspects `Provider`
- **THEN** it SHALL expose: `fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse>`, `fn stream(&self, req: CompletionRequest) -> impl Stream<Item = Result<Chunk>>>`, `fn model_id(&self) -> &str`, and `fn capabilities(&self) -> ProviderCapabilities`

#### Scenario: Agent Uses Provider Indirectly

- **WHEN** `Agent::step()` performs an LLM call
- **THEN** it MUST call through `Arc<dyn Provider>` and MUST NOT downcast to any concrete LLM client type

#### Scenario: Provider Replacement Without Agent Change

- **WHEN** a user swaps `Provider` from `OpenAIProvider` to `AnthropicProvider`
- **THEN** no code in `synthia-agent` crate SHALL require modification to compile and run

### Requirement: ProviderCapabilities Reporting

The `Provider` trait MUST return a `ProviderCapabilities` value indicating at minimum: `supports_streaming: bool`, `supports_tool_calls: bool`, `supports_vision: bool`, `max_context_tokens: u32`.

#### Scenario: Capability-Aware Agent Behavior

- **WHEN** `ProviderCapabilities::supports_streaming` is `false`
- **THEN** the agent MUST use `complete()` and MUST NOT call `stream()`

