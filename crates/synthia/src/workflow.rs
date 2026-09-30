//! [`synthia_workflow`] — the declarative multi-agent
//! workflow runtime.
//!
//! [`WorkflowSpec`] is a serde document (steps, fan-outs, selections,
//! pipelines, phases) and [`WorkflowRuntime`] executes it against a
//! [`WorkflowHost`] — the only seam through which a run touches the
//! world, which is what makes caps, concurrency, abort, and journal
//! prefix-replay enforceable instead of hopeful.

pub use synthia_workflow::*;
