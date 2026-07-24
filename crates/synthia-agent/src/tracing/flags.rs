//! Global flags for metrics-recorder installation.
//!
//! - [`RECORDER_INSTALLED`] (atomic bool) tracks whether
//!   [`crate::tracing::MetricsServer::start`] has installed
//!   the global Prometheus recorder.
//! - `INIT_METRICS` (Once) lets the metric-recording
//!   helpers defer counter registration to a one-time
//!   initialization path.
//! - [`is_recorder_installed`] is the public accessor.

use std::sync::{
    Once,
    atomic::{AtomicBool, Ordering},
};

pub(super) static RECORDER_INSTALLED: AtomicBool = AtomicBool::new(false);
pub(super) static INIT_METRICS: Once = Once::new();

/// Returns true if the recorder has been installed, meaning the
/// observability feature is active.
pub fn is_recorder_installed() -> bool {
    RECORDER_INSTALLED.load(Ordering::Relaxed)
}
