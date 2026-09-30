//! [`synthia_provider`] — the model seam and the
//! provider-agnostic wire format.
//!
//! [`ModelProvider`] is the only trait the agent loop needs from a
//! model; [`Message`] / [`ContentPart`] / [`CompletionRequest`] /
//! [`SamplingResult`] are the types that travel across it. The crate
//! also ships the two adapters this workspace uses
//! ([`AnthropicProvider`], [`OpenAICompatibleProvider`]) plus the
//! wire hygiene that does not belong in a loop: retry classification,
//! malformed tool-argument repair, credential normalization, prompt
//! cache policy, and token estimation.
//!
//! The adapters are behind the `provider-anthropic` /
//! `provider-openai` features; everything else in this module compiles
//! without either, which is what lets a consumer who implements
//! [`ModelProvider`] themselves drop the HTTP client entirely
//! (see [`MINIMAL.md`](https://github.com/crochee/synthia/blob/master/MINIMAL.md)).

pub use synthia_provider::*;
