//! # synthia-test-support
//!
//! Deterministic fakes for testing an assembly — the crates a consumer
//! adds as a `[dev-dependencies]` so their own agent can be exercised
//! without a network, a clock, or a provider key.
//!
//! The whole point is that the framework's seams make this trivial: a
//! `ModelProvider` is a trait, so a fake is a struct; an `Agent` streams
//! events, so a test collects them. Nothing here patches, mocks, or
//! reflects.
//!
//! | fake | use it when |
//! |---|---|
//! | [`FakeProvider`] | you need *a* provider and do not care what it answers |
//! | [`ReplayProvider`] | you want a recorded run reproduced exactly (it errors on an unmatched request rather than inventing an answer) |
//! | [`FakeTool`] | you need a registered tool with a controllable result |
//! | [`collect_results`] | you have an agent event stream and want the final text + tool results |
//!
//! ```rust
//! use synthia_provider::{
//!     CompletionRequest,
//!     CompletionResponse,
//!     Content,
//!     ModelProvider as _,
//! };
//! use synthia_test_support::FakeProvider;
//!
//! # async fn example() {
//! // A provider that answers the first call with "42" and the second
//! // with "" — deterministic, no network, no key.
//! let provider = FakeProvider::new(vec![
//!     CompletionResponse {
//!         content: Content::text("42"),
//!         ..CompletionResponse::default()
//!     },
//!     CompletionResponse::default(),
//! ]);
//!
//! let answer = provider.complete(CompletionRequest::default()).await;
//! assert!(answer.is_ok());
//! # }
//! ```
//!
//! Hand that `provider` to `ReActAgent::new(..)` in your own
//! dev-dependency test and the whole loop runs offline; the
//! `replay_provider` example drives a recorded run the same way, and
//! `synthia-harness`'s tests are the reference for the patterns
//! (scripted tool calls, cancelling mid-stream, forcing a provider
//! error).

pub mod fake_provider;
pub mod fake_tool;
pub mod replay;
pub mod run_stream;

// Explicit names, not globs: every new `pub` item inside the
// fixture modules would otherwise appear in this crate's surface
// (and through the facade) without anyone deciding it should.
pub use fake_provider::FakeProvider;
pub use fake_tool::FakeTool;
pub use replay::{ReplayError, ReplayProvider};
pub use run_stream::collect_results;
