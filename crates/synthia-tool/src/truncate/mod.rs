//! Output truncation configuration, asynchronous truncation, spill, and cleanup.

mod bound_output;

pub use bound_output::{
    OutputBound,
    OverflowStrategy,
    SanitizationPolicy,
    bound_output,
    start_cleanup_task,
};
