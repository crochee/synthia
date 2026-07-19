#![allow(deprecated)]
//! Integration test for OTel task-local context injection (Task 5).
//!
//! Verifies the `tokio::task_local` propagation pattern that
//! `Agent::run_stream` / `Agent::run_stream_with_state` rely on to make
//! the six context values reachable by `SpanAttributesProcessor::on_start`.
//!
//! A full `Agent::run_stream` end-to-end test requires constructing an
//! `Agent` with mocked LLM / tool dependencies, which is heavy. Instead,
//! these tests exercise the task-local mechanism directly: the same
//! `scope(...)` calls the agent uses internally. The agent-internal
//! `wrap_output_with_otel` wrapper (which drives the stream inside the
//! scopes) is covered by unit tests in `agent.rs`.

#![cfg(feature = "otel")]

use synthia_telemetry::span::attributes_processor::{
    AGENT_ID,
    GEN_AI_REQUEST_MODEL,
    GEN_AI_SYSTEM,
    SESSION_ID,
    TURN_ID,
    USER_ID,
};

#[tokio::test]
async fn task_local_reachable_inside_scope() {
    // Mirrors the nesting used by `with_otel_context` in `agent.rs`:
    // all six task-locals are in scope for the innermost future.
    SESSION_ID
        .scope("test-session".to_string(), async {
            USER_ID
                .scope("test-user".to_string(), async {
                    AGENT_ID
                        .scope("test-agent".to_string(), async {
                            TURN_ID
                                .scope("test-turn".to_string(), async {
                                    GEN_AI_SYSTEM
                                        .scope("anthropic".to_string(), async {
                                            GEN_AI_REQUEST_MODEL
                                                .scope(
                                                    "claude-3-5-sonnet"
                                                        .to_string(),
                                                    async {
                                                        assert_eq!(
                                                            SESSION_ID
                                                                .get()
                                                                .as_str(),
                                                            "test-session"
                                                        );
                                                        assert_eq!(
                                                            USER_ID
                                                                .get()
                                                                .as_str(),
                                                            "test-user"
                                                        );
                                                        assert_eq!(
                                                            AGENT_ID
                                                                .get()
                                                                .as_str(),
                                                            "test-agent"
                                                        );
                                                        assert_eq!(
                                                            TURN_ID
                                                                .get()
                                                                .as_str(),
                                                            "test-turn"
                                                        );
                                                        assert_eq!(
                                                            GEN_AI_SYSTEM
                                                                .get()
                                                                .as_str(),
                                                            "anthropic"
                                                        );
                                                        assert_eq!(
                                        GEN_AI_REQUEST_MODEL.get().as_str(),
                                        "claude-3-5-sonnet"
                                    );
                                                    },
                                                )
                                                .await
                                        })
                                        .await
                                })
                                .await
                        })
                        .await
                })
                .await
        })
        .await;
}

#[tokio::test]
async fn task_local_not_reachable_outside_scope() {
    // Outside any `scope()` call, `try_get` must return `Err` — this is
    // the "graceful skip" path the processor relies on when the agent
    // has not populated the task-locals (e.g. standalone tests, or the
    // `otel` feature is disabled).
    assert!(SESSION_ID.try_get().is_err());
    assert!(USER_ID.try_get().is_err());
    assert!(AGENT_ID.try_get().is_err());
    assert!(TURN_ID.try_get().is_err());
    assert!(GEN_AI_SYSTEM.try_get().is_err());
    assert!(GEN_AI_REQUEST_MODEL.try_get().is_err());
}

#[tokio::test]
async fn task_local_scope_does_not_block_send() {
    // Regression guard for Task 5.7: the scoped future must remain
    // `Send` so the agent's `Box<dyn Stream + Send>` contract holds.
    // This test compiles only if the scoped future is `Send`; the
    // `assert_send` helper enforces it at compile time.
    fn assert_send<T: Send>(_t: T) {}

    let scoped = SESSION_ID.scope("s".to_string(), async {
        USER_ID
            .scope("u".to_string(), async {
                AGENT_ID
                    .scope("a".to_string(), async {
                        TURN_ID
                            .scope("t".to_string(), async {
                                GEN_AI_SYSTEM
                                    .scope("sys".to_string(), async {
                                        GEN_AI_REQUEST_MODEL
                                            .scope("m".to_string(), async {})
                                            .await
                                    })
                                    .await
                            })
                            .await
                    })
                    .await
            })
            .await
    });
    assert_send(scoped);
}
