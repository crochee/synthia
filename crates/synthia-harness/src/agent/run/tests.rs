//! Behaviour tests for the `run` module: [`AgentHandle`] /
//! [`DetachedAgent`] (run control) and [`MpscInbox`]
//! (messages into a live run).
//!
//! Extracted from the two production files (R123). The justification is
//! two checkable facts, not a line count: `handle.rs` now carries **no**
//! inline `#[cfg(test)]` block, and the `handle` module below is over
//! the 400-line budget `make check-test-layout` is specified to enforce.
//! That gate currently runs a temporarily relaxed limit — see its
//! `RESTORE HERE` marker in the `Makefile`, which snaps both numbers
//! back to 400 / 0 once the in-flight restructure lands — so this
//! extraction keeps the crate inside the *intended* budget rather than
//! the current one.
//!
//! The policy for a module that outgrows it is a sibling `tests.rs` —
//! not a same-named subdirectory, which would read as another submodule.

#[cfg(test)]
mod handle {
    use std::{
        pin::Pin,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use futures::Stream;
    use synthia_core::CancelToken;

    use crate::{
        agent::{
            Agent,
            AgentEvent,
            AgentInput,
            descriptor::AgentDescriptor,
            run::{AgentHandle, DetachedClosed, DetachedError},
        },
        events::SystemEvent,
    };

    /// How a [`ProbeAgent`]'s stream behaves. One fixture rather than
    /// one struct per shape: the interesting difference between these
    /// cases is three lines of stream construction, not the fifteen
    /// lines of descriptor/`RegistryItem` boilerplate each would need.
    ///
    /// Not `Copy`: [`Probe::Gated`] carries an `Arc`.
    #[derive(Clone)]
    enum Probe {
        /// Two events, the second terminal.
        Terminal,
        /// The same, but yielding control once mid-stream so a driving
        /// `join` holds the slot across a suspension point.
        YieldThenTerminal,
        /// No events at all.
        Silent,
        /// `n` non-terminal events, then the terminal one — more than
        /// any test-sized buffer holds.
        Flood(usize),
        /// `Agent::run` itself suspends on the gate before returning
        /// its stream, which the trait permits.
        Gated(Arc<tokio::sync::Notify>),
    }

    struct ProbeAgent {
        descriptor: AgentDescriptor,
        probe: Probe,
        runs: Arc<AtomicUsize>,
    }

    impl synthia_core::registry::RegistryItem for ProbeAgent {
        fn name(&self) -> &str {
            &self.descriptor.name
        }

        fn description(&self) -> &str {
            &self.descriptor.description
        }
    }

    #[async_trait::async_trait]
    impl Agent for ProbeAgent {
        fn descriptor(&self) -> &AgentDescriptor {
            &self.descriptor
        }

        async fn run(
            &self,
            _input: AgentInput,
            _cancel: Arc<dyn CancelToken>,
        ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>> {
            use futures::stream;

            use crate::events::SessionEndReason;

            self.runs.fetch_add(1, Ordering::SeqCst);
            let ended = AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::Completed,
            });
            match &self.probe {
                Probe::Terminal => Box::pin(stream::iter(vec![
                    AgentEvent::System(SystemEvent::SessionStarted {
                        session_id: "test".to_string(),
                    }),
                    ended,
                ])),
                Probe::YieldThenTerminal => {
                    Box::pin(stream::once(async move {
                        tokio::task::yield_now().await;
                        ended
                    }))
                }
                Probe::Silent => Box::pin(stream::empty()),
                Probe::Flood(n) => {
                    let mut events: Vec<AgentEvent> = (0..*n)
                        .map(|i| {
                            AgentEvent::progress(format!("step {i}"), i, *n)
                        })
                        .collect();
                    events.push(ended);
                    Box::pin(stream::iter(events))
                }
                Probe::Gated(gate) => {
                    gate.notified().await;
                    Box::pin(stream::iter(vec![ended]))
                }
            }
        }
    }

    fn stub_descriptor() -> AgentDescriptor {
        AgentDescriptor {
            name: "stub".to_string(),
            description: "test agent".to_string(),
            kind: "stub".to_string(),
            version: "0.1.0".to_string(),
            instructions: String::new(),
            capabilities: Vec::new(),
            tools: Vec::new(),
            model_hint: None,
            handoffs: Vec::new(),
            handoff_hint: None,
            output_schema: None,
            owner: None,
            domain: None,
            persona: None,
            display_name: None,
            max_iterations: Some(1),
        }
    }

    fn probe(probe: Probe) -> (AgentHandle, Arc<AtomicUsize>) {
        let runs = Arc::new(AtomicUsize::new(0));
        let agent: Arc<dyn Agent> = Arc::new(ProbeAgent {
            descriptor: stub_descriptor(),
            probe,
            runs: Arc::clone(&runs),
        });
        (AgentHandle::new(agent), runs)
    }

    fn make_handle() -> (AgentHandle, Arc<dyn CancelToken>) {
        let (handle, _runs) = probe(Probe::Terminal);
        (handle, synthia_core::AtomicCancelToken::shared())
    }

    #[test]
    fn agent_handle_wraps_an_agent() {
        let (handle, _cancel) = make_handle();
        assert_eq!(handle.agent().descriptor().name, "stub");
    }

    #[tokio::test]
    async fn detached_handle_starts_idle() {
        let (handle, runs) = probe(Probe::Terminal);
        let detached = handle.spawn_detached(
            AgentInput::text("hello"),
            synthia_core::AtomicCancelToken::shared(),
        );

        // The documented contract is "nothing runs until the first
        // join", so assert the observable thing — that `Agent::run` was
        // not entered — rather than reaching into the handle's state.
        assert_eq!(
            runs.load(Ordering::SeqCst),
            0,
            "spawn_detached must not start the run"
        );

        detached.join().await.expect("join succeeds");
        assert_eq!(
            runs.load(Ordering::SeqCst),
            1,
            "join starts the run exactly once"
        );
    }

    #[test]
    fn detached_handle_cancel_token_is_the_same_arc() {
        let (handle, cancel) = make_handle();
        let cancel_clone = Arc::clone(&cancel);
        let detached = handle.spawn_detached(AgentInput::text("hello"), cancel);
        assert!(Arc::ptr_eq(&detached.cancel_token(), &cancel_clone));
    }

    #[tokio::test]
    async fn detached_handle_join_returns_session_ended() {
        let (handle, cancel) = make_handle();
        let detached = handle.spawn_detached(AgentInput::text("hello"), cancel);
        let final_event = detached.join().await.expect("join succeeds");
        assert!(matches!(
            final_event,
            AgentEvent::System(SystemEvent::SessionEnded { .. })
        ));
    }

    /// The `try_event` state machine over one run's lifetime.
    ///
    /// Three distinct answers, and the point is that they are
    /// distinct: before the first `join` the buffer is open and empty
    /// (`Ok(None)` — "nothing yet"); while driving, buffered events
    /// come out in order; once the run is over and the tail has
    /// drained, the sender is gone and it says so
    /// (`Err(DetachedClosed)` — "never again"). Collapsing the last two
    /// into a permanent `Ok(None)` would leave a polling caller — the
    /// whole reason this type exists — with no way to stop.
    #[tokio::test]
    async fn try_event_walks_the_buffer_lifecycle() {
        let (handle, cancel) = make_handle();
        let detached = handle.spawn_detached(AgentInput::text("hello"), cancel);

        assert!(matches!(detached.try_event(), Ok(None)));

        let _final = detached.join().await.expect("join succeeds");

        let mut drained = Vec::new();
        let terminator = loop {
            match detached.try_event() {
                Ok(Some(event)) => drained.push(event),
                other => break other,
            }
        };
        assert!(
            matches!(terminator, Err(DetachedClosed)),
            "a drained, finished run reports closure; got {terminator:?}"
        );
        assert!(
            drained.len() >= 2,
            "expected at least 2 buffered events, got {}",
            drained.len()
        );
        assert!(matches!(
            drained[0],
            AgentEvent::System(SystemEvent::SessionStarted { .. })
        ));
        assert!(
            matches!(detached.try_event(), Err(DetachedClosed)),
            "closure is sticky, so a polling loop terminates"
        );
    }

    /// Concurrent joins: the loser must replay the winner's terminal
    /// event, not observe an exhausted stream and report `EmptyStream`
    /// for a run that actually completed.
    ///
    /// The probe yields `Pending` once before its terminal event, so the
    /// first join parks *while holding the slot* and the second really
    /// does contend on it — without that yield the second join would
    /// only ever reach the finished state directly and the contention
    /// path would go untested.
    #[tokio::test]
    async fn concurrent_joins_both_see_the_terminal_event() {
        let (handle, _runs) = probe(Probe::YieldThenTerminal);
        let detached = handle.spawn_detached(
            AgentInput::text("hello"),
            synthia_core::AtomicCancelToken::shared(),
        );

        let (a, b) = futures::join!(detached.join(), detached.join());
        let a = a.expect("first join succeeds");
        let b = b.expect("concurrent join must replay, not fail");
        assert!(matches!(
            a,
            AgentEvent::System(SystemEvent::SessionEnded { .. })
        ));
        assert!(matches!(
            b,
            AgentEvent::System(SystemEvent::SessionEnded { .. })
        ));
        assert_eq!(a.kind(), b.kind());
    }

    /// A second `join` must replay the first run's terminal event, not
    /// start a second run: a run's tool side effects are not
    /// repeatable, and the previous implementation re-called
    /// `Agent::run` on every join.
    #[tokio::test]
    async fn join_twice_runs_the_agent_once() {
        let (handle, runs) = probe(Probe::Terminal);
        let detached = handle.spawn_detached(
            AgentInput::text("hello"),
            synthia_core::AtomicCancelToken::shared(),
        );

        let first = detached.join().await.expect("first join succeeds");
        let second = detached.join().await.expect("second join succeeds");
        assert_eq!(
            runs.load(Ordering::SeqCst),
            1,
            "joining twice must not run the agent twice"
        );
        assert!(
            matches!(
                second,
                AgentEvent::System(SystemEvent::SessionEnded { .. })
            ),
            "the replay is the terminal event"
        );
        assert_eq!(
            second.kind(),
            first.kind(),
            "the replay matches the original terminal event"
        );
    }

    /// Dropping a `join` future mid-await must not be mistaken for a
    /// finished run.
    ///
    /// `Agent::run` may suspend before yielding its stream, and the old
    /// ordering (install a placeholder, then await) left that
    /// placeholder behind on cancellation — so the next `join` reported
    /// `EmptyStream` for a run that had never started. Nothing is
    /// mutated until the stream exists, so there is nothing to leak.
    #[tokio::test]
    async fn dropping_a_join_mid_await_leaves_the_run_startable() {
        let gate = Arc::new(tokio::sync::Notify::new());
        let (handle, runs) = probe(Probe::Gated(Arc::clone(&gate)));
        let detached = handle.spawn_detached(
            AgentInput::text("hello"),
            synthia_core::AtomicCancelToken::shared(),
        );

        // First poll reaches the gate inside `Agent::run`, then the
        // future is abandoned.
        let mut abandoned = Box::pin(detached.join());
        assert!(
            futures::poll!(abandoned.as_mut()).is_pending(),
            "the gate should suspend `Agent::run`"
        );
        drop(abandoned);

        gate.notify_one();
        let final_event = detached
            .join()
            .await
            .expect("a dropped join must not poison the handle");
        assert!(
            matches!(
                final_event,
                AgentEvent::System(SystemEvent::SessionEnded { .. })
            ),
            "the retry runs the agent and gets its terminal event"
        );
        assert_eq!(
            runs.load(Ordering::SeqCst),
            2,
            "the aborted attempt and the completed one both entered \
             `Agent::run`; neither was replayed from a placeholder"
        );
    }

    /// A run whose stream ends without a terminal event is reported as
    /// `EmptyStream` — and *reported again* on retry, rather than
    /// panicking or silently succeeding. Matters because the
    /// load-bearing `expect` in the old implementation was reachable
    /// here.
    #[tokio::test]
    async fn empty_stream_is_reported_consistently_on_retry() {
        let (handle, _runs) = probe(Probe::Silent);
        let detached = handle.spawn_detached(
            AgentInput::text("hello"),
            synthia_core::AtomicCancelToken::shared(),
        );

        assert!(matches!(
            detached.join().await,
            Err(DetachedError::EmptyStream)
        ));
        assert!(
            matches!(detached.join().await, Err(DetachedError::EmptyStream)),
            "a retry reports the same outcome instead of panicking"
        );
    }

    /// An overflowing buffer truncates the *polled* transcript but not
    /// the run's outcome: `join` still hands back the terminal event,
    /// and the poller still reaches a clean closure rather than
    /// hanging. The loss is the documented trade — the capacity is what
    /// keeps a caller who stopped polling from growing memory without
    /// limit.
    #[tokio::test]
    async fn buffer_overflow_truncates_the_poll_but_not_the_outcome() {
        const CAPACITY: usize = 8;
        const EMITTED: usize = CAPACITY * 3;

        let (handle, _runs) = probe(Probe::Flood(EMITTED));
        let detached = handle.spawn_detached_with_capacity(
            AgentInput::text("hello"),
            synthia_core::AtomicCancelToken::shared(),
            CAPACITY,
        );

        // Nobody polls while it runs, so the surplus is dropped.
        let final_event = detached.join().await.expect("join succeeds");
        assert!(
            matches!(
                final_event,
                AgentEvent::System(SystemEvent::SessionEnded { .. })
            ),
            "the terminal event survives an overflow"
        );

        let mut polled = 0;
        let terminator = loop {
            match detached.try_event() {
                Ok(Some(_)) => polled += 1,
                other => break other,
            }
        };
        assert!(
            matches!(terminator, Err(DetachedClosed)),
            "an overflowed run still reports closure; got {terminator:?}"
        );
        assert!(
            polled < EMITTED,
            "the overflow must actually drop events, else this test \
             proves nothing; polled {polled} of {EMITTED}"
        );
        assert!(
            polled <= CAPACITY + 1,
            "the buffer must stay bounded by its requested capacity; \
             polled {polled} with capacity {CAPACITY}"
        );
    }

    #[test]
    fn spawn_detached_with_custom_capacity_succeeds() {
        let (handle, cancel) = make_handle();
        let _detached = handle.spawn_detached_with_capacity(
            AgentInput::text("hello"),
            cancel,
            8,
        );
    }
}

#[cfg(test)]
mod inbox {
    use async_trait::async_trait;
    use synthia_provider::Message;

    use crate::agent::run::{MpscInbox, RunInbox};

    /// Steering drains in FIFO order and empties: a second take
    /// returns nothing until more is sent.
    #[tokio::test]
    async fn steering_drains_fifo_and_empties() {
        let (inbox, handle) = MpscInbox::channel();
        handle
            .send_steering(Message::user("first"))
            .expect("receiver alive");
        handle
            .send_steering(Message::user("second"))
            .expect("receiver alive");

        let taken = inbox.take_steering().await;
        let texts: Vec<String> = taken
            .iter()
            .filter_map(|m| match &m.content {
                synthia_provider::Content::Single(
                    synthia_provider::ContentPart::Text(t),
                ) => Some(t.text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, vec!["first", "second"]);
        assert!(
            inbox.take_steering().await.is_empty(),
            "drain must empty the queue"
        );
    }

    /// Steering and follow-up queues are independent: draining
    /// one never consumes the other.
    #[tokio::test]
    async fn steering_and_follow_up_are_independent() {
        let (inbox, handle) = MpscInbox::channel();
        handle
            .send_steering(Message::user("steer"))
            .expect("receiver alive");
        handle
            .send_follow_up(Message::user("follow"))
            .expect("receiver alive");

        let follow = inbox.take_follow_up().await;
        assert_eq!(follow.len(), 1);
        // Taking follow-ups left the steering queue intact.
        let steer = inbox.take_steering().await;
        assert_eq!(steer.len(), 1);
        assert!(inbox.take_follow_up().await.is_empty());
    }

    /// The default trait implementations return empty vectors,
    /// so a partial implementation only overrides the half it
    /// needs.
    #[tokio::test]
    async fn defaults_return_empty() {
        struct SteeringOnly;
        #[async_trait]
        impl RunInbox for SteeringOnly {
            async fn take_steering(&self) -> Vec<Message> {
                vec![Message::user("hi")]
            }
        }

        let inbox = SteeringOnly;
        assert_eq!(inbox.take_steering().await.len(), 1);
        assert!(
            inbox.take_follow_up().await.is_empty(),
            "unoverridden follow-up default must be empty"
        );
    }

    /// Sending after the inbox is dropped reports an error
    /// instead of silently swallowing the message.
    #[tokio::test]
    async fn send_fails_once_inbox_dropped() {
        let (inbox, handle) = MpscInbox::channel();
        drop(inbox);
        assert!(handle.send_steering(Message::user("x")).is_err());
        assert!(handle.send_follow_up(Message::user("y")).is_err());
    }

    /// A dropped producer still yields its buffered messages
    /// before reporting empty.
    #[tokio::test]
    async fn buffered_messages_survive_producer_drop() {
        let (inbox, handle) = MpscInbox::channel();
        handle
            .send_steering(Message::user("late"))
            .expect("receiver alive");
        drop(handle);
        assert_eq!(inbox.take_steering().await.len(), 1);
    }
}
