## 1. ReActLoop Deprecation

- [ ] 1.1 Add `#[deprecated(note = "Use StreamBuilder in stream_builder/builder.rs; ReActLoop will be removed once external consumers migrate. See openspec/changes/agent-architecture-optimization/")]` to `ReActLoop` struct in `react.rs`
- [ ] 1.2 Update `synthia-e2e/reasoning_tracking.rs` — migrate away from `ReActLoop` to test reasoning tracking behavior via `StreamBuilder` or direct behavior assertions
- [ ] 1.3 Verify `cargo test -p synthia-agent` passes after deprecation attribute added

## 2. AgentConfig Naming Cleanup

- [ ] 2.1 Delete `crates/synthia-agent/src/config/agent.rs` file
- [ ] 2.2 Run `cargo test -p synthia-agent` to confirm no breakage from deletion
- [ ] 2.3 If tests in `agent.rs` (the file being deleted) break, fix them — they should not reference the deleted `AgentConfig`

## 3. Steering Channel Wiring

- [ ] 3.1 In `stream_builder/builder.rs::BuilderSteps`, ensure `steering_channel: Option<Arc<dyn SteeringChannel>>` is stored (not dropped)
- [ ] 3.2 In `run_with_steps` loop, at iteration start call `steering_channel.try_recv()` — if `Some(msg)`, yield `AgentEvent::SteeringReceived { session_id, message }` and inject as `Message::User` at front of `ctx.messages`
- [ ] 3.3 Run steering e2e tests: `e2e_steering_injection_test.rs`, `span_hierarchy_test.rs`, `e2e_event_sequence_test.rs`, `e2e_llm_test.rs`, `synthia-server/tests/e2e_server_sse_test.rs` — all should pass

## 4. Cleanup

- [ ] 4.1 Run `cargo clippy` to confirm clean
- [ ] 4.2 Run `cargo test -p synthia-agent` to confirm all tests pass