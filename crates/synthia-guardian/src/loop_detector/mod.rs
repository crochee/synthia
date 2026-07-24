//! Five-layer loop detection system.
//!
//! All detection is deterministic and hash-based, independent of LLM judgment.
//! Follows P6 (Distrust by Default): protection mechanisms are rule-based.
//!
//! # Detection Layers (in evaluation order)
//!
//! 1. [`doom_loop::DoomLoopDetector`]: 3 consecutive identical `(tool, args)` calls
//!    → `LoopAction::RequirePermission` (mirrors opencode's `doom_loop`).
//! 2. [`generic_repeat::GenericRepeatDetector`]: O(1) `HashMap<(u64, u64), u32>`
//!    counter per `(tool_id, args_hash)`. Warning at 2, block at 3.
//! 3. [`ping_pong::PingPongDetector`]: A-B-A-B alternation of two distinct tools.
//! 4. [`poll_no_progress::PollNoProgressDetector`]: Polling calls with identical
//!    results (10 consecutive). Checked via [`set::LoopDetectorSet::check_poll_result`],
//!    not the main flow.
//! 5. [`global_circuit::GlobalCircuitDetector`]: 30-iteration hard cap. Checked
//!    via the `iteration` argument passed to [`set::LoopDetectorSet::check`].
//!
//! # Module Layout
//!
//! - [`hash`]: Tool/args hashing utilities (allocation-free, AHasher-based).
//! - [`doom_loop`], [`generic_repeat`], [`ping_pong`],
//!   [`poll_no_progress`], [`global_circuit`]: One focused detector per module.
//! - [`set`]: [`set::LoopDetectorSet`] aggregator that runs all five detectors
//!   and short-circuits on the first non-`Ok` result.
//!
//! The `hash_tool_args` helper in [`hash`] stays `pub` for in-crate use by
//! detector unit tests; it is not re-exported here because the
//! `mod loop_detector;` declaration in `lib.rs` keeps the module private.

mod doom_loop;
mod generic_repeat;
mod global_circuit;
mod hash;
mod ping_pong;
mod poll_no_progress;
mod set;

pub use set::LoopDetectorSet;
