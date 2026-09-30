//! Test suite for the canonical ReAct loop and [`ReActAgent`].
//!
//! Split into focused submodules by concern. The shared
//! fixtures — `ScriptedStreamProvider`, `CapturingProvider`,
//! the chunk-sequence helpers, `run_and_collect`, and the
//! peer-agent descriptor — live in [`support`] so each
//! section can pull them via `super::support::*` without
//! re-declaring them.
//!
//! | Submodule               | What it covers                                              |
//! |-------------------------|-------------------------------------------------------------|
//! | [`agent_smoke`]         | Agent-level descriptor + smoke tests                        |
//! | [`prompt_integration`]  | Prompt assembler end-to-end (skills, peer agents, tools)    |
//! | [`end_to_end_runtime`]  | Provider-side injection: what the loop actually sends       |
//! | [`loop_lifecycle`]      | One full loop pass: chunks, tool dispatch, max-iter, cancel |
//! | [`multi_agent_panel`]   | Multi-expert code-review scenario + descriptor fixtures     |
//! | [`parallel_dispatch`]   | Parallel / sequential tool execution + descriptor pinning   |
//! | [`tool_projection_memo`]| Tool projection memoization across iterations (R59)        |
//! | [`tool_restriction`]    | R58 per-agent `denied_tools` / `allow_list` + interceptors  |
//! | [`compaction_wiring`]   | Which context manager a run uses (policy vs caller's)       |
//! | [`steering_wiring`]     | Steering: guards, hooks, sanitizers, hints, transformers    |
//! | [`run_inbox`]           | Run-inbox follow-up / steering seams + length-stop guard    |
//! | [`support`]             | Shared fixtures                                              |

use super::*;

mod agent_smoke;
mod compaction_wiring;
mod end_to_end_runtime;
mod loop_lifecycle;
mod multi_agent_panel;
mod parallel_dispatch;
mod prompt_integration;
mod run_inbox;
mod steering_wiring;

mod support;
mod tool_projection_memo;
mod tool_restriction;
