# model-provider-streaming Specification

## Purpose

Define the contract for `ModelProvider::complete_with_stream`, a callback-based streaming API that gives the agent control over stream ownership, enables cancellation, fallback, and incremental usage reporting, and fixes three real bugs in the existing `stream()` path.

## ADDED Requirements

### Requirement: ModelProvider SHALL expose a callback-based streaming method

`ModelProvider` SHALL provide an `async fn complete_with_stream(&self, req: CompletionRequest, mut on_delta: Box<dyn FnMut(StreamChunk) + Send>) -> Result<SamplingResult, ProviderError>` method. The default implementation SHALL invoke `complete()` and emit a single `StreamChunk::IsDone` carrying the result.

#### Scenario: Default implementation falls back to complete
- **WHEN** a `ModelProvider` does not override `complete_with_stream`
- **THEN** it SHALL call `self.complete(req).await` exactly once
- **THEN** it SHALL emit exactly one `StreamChunk::IsDone { result }` callback before returning
- **THEN** the returned `SamplingResult` SHALL equal the result passed to `IsDone`

#### Scenario: Override emits incremental deltas
- **WHEN** a `ModelProvider` (e.g., Anthropic, OpenAI) overrides `complete_with_stream` to do real streaming
- **THEN** it SHALL emit `StreamChunk::Content(ContentPart::Text(t))` for each text delta received from the upstream API
- **THEN** it SHALL emit `StreamChunk::ToolCallDelta { id, arguments_delta }` for each incremental tool-call argument fragment, never the full accumulated input as a single `Content(ToolUse{input: String})` snapshot
- **THEN** it SHALL emit exactly one terminal `StreamChunk::IsDone { result }` before returning `Ok(result)`

#### Scenario: Callback signature
- **WHEN** the agent calls `provider.complete_with_stream(req, on_delta)`
- **THEN** `on_delta` SHALL be a `Box<dyn FnMut(StreamChunk) + Send>`
- **THEN** the callback MAY be invoked from any task, but invocations SHALL be serialized (one at a time)
- **THEN** the callback SHALL NOT be invoked after `complete_with_stream` returns

### Requirement: StreamChunk SHALL carry an IsDone terminal variant

`StreamChunk` SHALL include a new `IsDone { result: SamplingResult }` variant alongside the existing `Content`, `Usage`, `Stop`, `ToolCallStart`, `ToolCallDelta`, and `ToolCallEnd` variants.

#### Scenario: IsDone carries the final result
- **WHEN** a provider emits `StreamChunk::IsDone { result }`
- **THEN** `result` SHALL contain the full `SamplingResult` including text, tool calls, usage, and finish reason
- **THEN** the agent SHALL treat `IsDone` as a terminal signal: no further `StreamChunk` of any variant SHALL be emitted in the same stream

#### Scenario: Existing variants remain
- **WHEN** the change is applied
- **THEN** all existing `StreamChunk` variants (`Content`, `Usage`, `Stop`, `ToolCallStart`, `ToolCallDelta`, `ToolCallEnd`) SHALL remain available
- **THEN** no existing variant SHALL be removed or renamed

### Requirement: ToolCallDelta SHALL carry incremental argument fragments

`StreamChunk::ToolCallDelta` SHALL carry only the fragment of the tool-call arguments received in the current upstream delta, not the full accumulated input. The agent SHALL concatenate fragments client-side to reconstruct the final argument string.

#### Scenario: Incremental emission
- **WHEN** an upstream SSE event provides `{"id":"x","delta":{"partial_json":"{\"na"}` and later `{\"id\":\"x\",\"delta":{"partial_json":"me\":\"r"}}`
- **THEN** the provider SHALL emit `StreamChunk::ToolCallDelta { id: "x", arguments_delta: "{\"na" }` for the first
- **THEN** the provider SHALL emit `StreamChunk::ToolCallDelta { id: "x", arguments_delta: "me\":\"r\"}" }` for the second
- **THEN** the provider SHALL NOT emit `Content(ContentPart::ToolUse(ToolUse { input: String(<full accumulated>) }))`

#### Scenario: O(n^2) transmission eliminated
- **WHEN** a tool call has 1000 characters of arguments and is delivered in 100 SSE events
- **THEN** the total bytes transmitted via `StreamChunk` for that tool call SHALL be approximately 1000 + Σ(fragments) ≈ O(n)
- **THEN** it SHALL NOT be O(n^2) (the agent SHALL NOT receive 1000+1999+2998+... bytes)

### Requirement: Streaming-OpenAI reasoning SHALL be emitted as Reasoning content, not sniffed from text

The OpenAI `complete_with_stream` implementation SHALL emit `StreamChunk::Content(ContentPart::Reasoning(text))` for any `reasoning_content` field present in the upstream response. The implementation SHALL NOT inspect `text` content for `<think>` or `</think>` substrings.

#### Scenario: reasoning_content field routed correctly
- **WHEN** an upstream SSE event provides `{"choices":[{"delta":{"reasoning_content":"thinking..."}}]}`
- **THEN** the OpenAI provider SHALL emit `StreamChunk::Content(ContentPart::Reasoning(TextContent { text: "thinking..." }))`
- **THEN** the agent SHALL receive reasoning and text on separate channels without text-content sniffing

#### Scenario: Bug 1 ends_with empty string eliminated
- **WHEN** the OpenAI provider handles a `text` delta
- **THEN** it SHALL NOT evaluate `content.ends_with("")` (the broken always-true check)
- **THEN** the file `crates/synthia-provider/src/streaming/openai.rs:151` (or its successor) SHALL NOT contain `ends_with("")`

### Requirement: Old stream() method SHALL be deprecated, not removed

`ModelProvider::stream(&self, req) -> Result<StreamResult, Error>` SHALL be marked `#[deprecated(note = "use complete_with_stream; stream() ownership prevents cancel/fallback")]`. The method SHALL remain callable for 1 release cycle.

#### Scenario: Deprecation warning emitted
- **WHEN** a caller invokes `provider.stream(req)`
- **THEN** the compiler SHALL emit a deprecation warning pointing to `complete_with_stream`
- **THEN** the call SHALL still execute successfully (backward compatibility)

#### Scenario: One release cycle
- **WHEN** 1 release cycle has elapsed since deprecation
- **THEN** `ModelProvider::stream` MAY be removed
- **THEN** no caller SHALL exist in the workspace that invokes the removed method

### Requirement: StreamError SHALL be added to synthia-core Error enum

`synthia_core::Error` SHALL include a new `StreamError { kind: StreamErrorKind, message: String }` variant.

#### Scenario: StreamError kind variants
- **WHEN** a streaming failure occurs
- **THEN** the provider SHALL classify the failure as one of: `HttpFailure { status: u16, recoverable: bool }`, `ProtocolError(String)`, `Aborted`, or `Internal(String)`
- **THEN** the `Error::StreamError` variant SHALL preserve the kind and a human-readable message

#### Scenario: Aborted emitted on cancel
- **WHEN** the `CancellationToken` is cancelled while a stream is in progress
- **THEN** the provider task SHALL stop emitting `StreamChunk` values
- **THEN** the provider SHALL return `Err(Error::StreamError { kind: StreamErrorKind::Aborted, .. }))`
