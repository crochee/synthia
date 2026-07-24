//! Hook panic-safety: the [`safe_hook_fail_open`] wrapper +
//! the [`get_hook_name`] helper used by `register_hook`.

use std::panic::AssertUnwindSafe;

use futures::FutureExt;
use synthia_core::Error;
use ulid::Ulid;

use crate::traits::{AgentHook, FailPolicy};

/// `format!("{:?}", hook)` — used as the default `HookInfo::name`
/// since `AgentHook` does not require a `name()` method.
pub(super) fn get_hook_name(hook: &dyn AgentHook) -> String {
    format!("{:?}", hook)
}

/// Wraps a hook future in `catch_unwind` and converts any
/// panic into a `FailPolicy`-appropriate default. The hook's
/// own `Result::Err` is propagated unchanged.
pub(super) async fn safe_hook_fail_open<F, T>(
    f: F,
    hook_id: &Ulid,
    default: T,
    fail_closed_default: T,
    fail_policy: FailPolicy,
) -> Result<T, Error>
where
    F: std::future::Future<Output = Result<T, Error>>,
    T: Clone,
{
    match AssertUnwindSafe(f).catch_unwind().await {
        Ok(result) => result,
        Err(panic) => {
            let panic_msg = if let Some(s) = panic.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            tracing::error!(
                hook_id = %hook_id,
                %panic_msg,
                "Hook panicked (fail-open): returning default and continuing execution"
            );
            match fail_policy {
                FailPolicy::FailClosed => Ok(fail_closed_default),
                FailPolicy::FailOpen => Ok(default),
            }
        }
    }
}
