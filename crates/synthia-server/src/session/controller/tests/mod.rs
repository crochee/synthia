//! Test suite for `SessionController`.
//!
//! Split into focused submodules by concern, so the controller
//! itself stays a single file. Each submodule tests one slice of
//! the lifecycle (state machine, persistence, multi-modal turn,
//! rerun, dependencies wiring); cross-cutting fixtures live in
//! [`support`].
//!
//! | Submodule        | What it covers                                                |
//! |------------------|---------------------------------------------------------------|
//! | [`support`]      | Shared fixtures (`VecFactory`, `BlockingFactory`, `RecordingFactory`, `ToolCapturingProvider`, `test_deps`, `make_manager_and_controller`, `wait_for_runs`, …) |
//! | [`truncate`]     | The `truncate` helper — char-counted, ellipsis-on-truncation contract |
//! | [`lifecycle`]    | State transitions — concurrent prompt dedup, idle shutdown, post-shutdown error, snapshot bus, idempotent `close` |
//! | [`cancel`]       | `Cancel` op — terminates run, idempotent, safe before run starts |
//! | [`event_emission`] | Persist + broadcast of agent events, durable/ephemeral split, `request_header` epoch marker |
//! | [`history`]      | Multi-turn memory: second prompt seeds `history` from sink, user-role reconstruction, compaction-checkpoint fold |
//! | [`run_log`]      | `RunLog` ledger-feed and ordinal provenance (sink-assigned, not local counter) |
//! | [`prompt_multi`] | `PromptMulti` — lossless parts round-trip + text-only persistence of multimodal turn |
//! | [`rerun`]        | `Rerun` — parts reach factory, prompt row shadows the replaced turn, chained reruns compose, defensive no-user-turn fallback, mid-run rerun |
//! | [`run_config`]   | R34/R50/R58 + factory wiring: `tool_surface` / `strategy` / `tool_restriction` deps-to-config hop, factory narrows wire tools, runs the configured strategy |
//! | [`usage`]        | Process-wide usage counters: `SystemEvent::Usage` token totals and one turn per `SystemEvent::SessionEnded` (any reason) |

mod cancel;
mod event_emission;
mod history;
mod lifecycle;
mod prompt_multi;
mod rerun;
mod run_config;
mod run_log;
mod support;
mod truncate;
mod usage;
