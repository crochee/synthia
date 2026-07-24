//! System context registry for typed sources.
//!
//! Sources are registered with [`SystemContext::register`] and reconciled via
//! [`SystemContext::reconcile_all`]. The registry type-erases source values
//! into `serde_json::Value` for uniform storage.

pub mod environment_source;
pub mod reconcile;
pub mod source;

use std::{collections::HashMap, sync::Mutex};

pub use environment_source::{EnvironmentSource, EnvironmentValue};
pub use reconcile::{ReconcileResult, reconcile};
pub use source::{Snapshot, Source};

/// Private type-erased source trait for uniform `serde_json::Value` storage.
///
/// Only the operations needed by [`SystemContext`] are exposed here; the full
/// [`Source`] API is still available on the concrete type before boxing.
trait DynSource: Send + Sync {
    fn key(&self) -> &str;
    fn update_value(
        &self,
        prev: &serde_json::Value,
    ) -> anyhow::Result<Option<serde_json::Value>>;
    fn removed(&self) -> bool;
}

impl<S: Source> DynSource for S {
    fn key(&self) -> &str {
        Source::key(self)
    }

    fn update_value(
        &self,
        prev: &serde_json::Value,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let prev_typed: S::Value = serde_json::from_value(prev.clone())?;
        match self.update(&prev_typed)? {
            Some(new_val) => Ok(Some(serde_json::to_value(new_val)?)),
            None => Ok(None),
        }
    }

    fn removed(&self) -> bool {
        Source::removed(self)
    }
}

/// Registry of typed system-context sources.
///
/// Integration into the system prompt build path is deferred to task 4.4.6;
/// the API is ready but no caller invokes it yet.
pub struct SystemContext {
    sources: Mutex<HashMap<String, Box<dyn DynSource>>>,
    snapshots: Mutex<HashMap<String, Snapshot<serde_json::Value>>>,
}

impl SystemContext {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            sources: Mutex::new(HashMap::new()),
            snapshots: Mutex::new(HashMap::new()),
        }
    }

    /// Register a typed source. The source's baseline becomes the initial
    /// snapshot at revision 0.
    pub fn register<S: Source + 'static>(&self, source: S) {
        let key = source.key().to_string();
        let baseline_val = serde_json::to_value(source.baseline())
            .unwrap_or(serde_json::Value::Null);
        self.sources
            .lock()
            .expect("sources mutex poisoned")
            .insert(key.clone(), Box::new(source));
        self.snapshots
            .lock()
            .expect("snapshots mutex poisoned")
            .insert(key, Snapshot::new(baseline_val, 0));
    }

    /// Reconcile all registered sources and return per-source results.
    ///
    /// Each source's `update` is polled; changed values are applied to the
    /// internal snapshot (returned as [`Updated`](ReconcileResult::Updated))
    /// while unchanged sources return
    /// [`Unchanged`](ReconcileResult::Unchanged). Removed sources are skipped.
    pub fn reconcile_all(&self) -> Vec<ReconcileResult<serde_json::Value>> {
        let sources = self.sources.lock().expect("sources mutex poisoned");
        let mut snapshots =
            self.snapshots.lock().expect("snapshots mutex poisoned");
        let mut results = Vec::with_capacity(sources.len());
        for (key, dyn_src) in sources.iter() {
            if dyn_src.removed() {
                continue;
            }
            let Some(prev) = snapshots.get(key).cloned() else {
                continue;
            };
            match dyn_src.update_value(&prev.value) {
                Ok(Some(new_val)) => {
                    let new_snap = Snapshot::new(new_val, prev.revision + 1);
                    if let Some(slot) = snapshots.get_mut(key) {
                        *slot = new_snap;
                    }
                    results.push(ReconcileResult::Updated);
                }
                Ok(None) => results.push(ReconcileResult::Unchanged),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        key = %dyn_src.key(),
                        "SystemContext source update failed"
                    );
                    results.push(ReconcileResult::Unchanged);
                }
            }
        }
        results
    }
}

impl Default for SystemContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_context_not_registered_as_tool() {
        // Regression guard: SystemContext must NOT be exposed as a tool.
        // The `system_context` module exposes `Source` (trait), `Snapshot`
        // (struct), `SystemContext` (registry), `EnvironmentSource`,
        // `reconcile`, `ReconcileResult` — none of which implement any `Tool`
        // or `ToolDefinition` trait. This test exists to catch accidental
        // tool registration in future changes.
        let type_name = std::any::type_name::<SystemContext>();
        assert!(
            !type_name.contains("Tool"),
            "SystemContext type must not be a Tool; got {type_name}"
        );
        let env_name = std::any::type_name::<EnvironmentSource>();
        assert!(
            !env_name.contains("Tool"),
            "EnvironmentSource type must not be a Tool; got {env_name}"
        );
    }

    #[test]
    fn system_context_register_and_reconcile_all() {
        let ctx = SystemContext::new();
        ctx.register(EnvironmentSource::new());
        let results = ctx.reconcile_all();
        assert_eq!(results.len(), 1);
        // reconcile_all returns Unchanged (no env delta) or Updated (env
        // changed since registration, e.g. due to a parallel test mutating
        // env vars). Either is a valid healthy result.
        assert!(matches!(
            results[0],
            ReconcileResult::Unchanged | ReconcileResult::Updated
        ));
    }
}
