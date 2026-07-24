//! Internal helper that turns a panic in a hook body into
//! `Result::Err(())`, allowing the lifecycle / domain methods to
//! fail-open with a safe default (log a warning, return Proceed / ()).
//!
//! Used by [`super::lifecycle`] and [`super::domain`] via
//! `super::catch_unwind::catch_unwind`.

use std::panic::AssertUnwindSafe;

use futures::FutureExt;
use synthia_core::Error;

/// Catches panics from hook execution.
pub(super) async fn catch_unwind<F, T>(future: F) -> std::result::Result<T, ()>
where
    F: std::future::Future<Output = Result<T, Error>>,
{
    match AssertUnwindSafe(future).catch_unwind().await {
        Ok(result) => match result {
            Ok(val) => Ok(val),
            Err(_) => Err(()),
        },
        Err(_) => Err(()),
    }
}
