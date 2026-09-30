//! Workflows state: in-memory registry of `WorkflowSpec`s, with
//! CRUD helpers. Execution lands in a follow-up turn — the
//! runtime needs a `WorkflowHost` implementation that calls back
//! into the agent harness, and that wiring is its own R-series
//! change. This turn ships the storage + read/list/create/replace/
//! delete surface the frontend commits to.

use std::{collections::BTreeMap, sync::Arc};

use synthia::workflow::{
    WorkflowCaps,
    WorkflowError,
    WorkflowPlan,
    WorkflowSpec,
};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct WorkflowsState {
    inner: Arc<RwLock<BTreeMap<String, WorkflowSpec>>>,
}

impl WorkflowsState {
    /// Build an empty registry. Workflows are in-memory this turn;
    /// disk persistence mirrors the `ScheduleStore` pattern in a
    /// follow-up turn if/when needed.
    pub fn build() -> Self {
        Self {
            inner: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub async fn list(&self) -> Vec<WorkflowSpec> {
        let g = self.inner.read().await;
        g.values().cloned().collect()
    }

    pub async fn get(&self, id: &str) -> Option<WorkflowSpec> {
        let g = self.inner.read().await;
        g.get(id).cloned()
    }

    pub async fn create(&self, spec: WorkflowSpec) -> Result<(), String> {
        let mut g = self.inner.write().await;
        if g.contains_key(&spec.id) {
            return Err(format!("workflow '{}' already exists", spec.id));
        }
        // Plan with default caps so caps-violation errors surface at
        // create time, not run time.
        spec.plan(&WorkflowCaps::default())
            .map_err(|e| e.to_string())?;
        g.insert(spec.id.clone(), spec);
        Ok(())
    }

    pub async fn replace(
        &self,
        id: &str,
        spec: WorkflowSpec,
    ) -> Result<(), String> {
        if spec.id != id {
            return Err(format!(
                "url id '{id}' does not match spec.id '{}'",
                spec.id
            ));
        }
        let mut g = self.inner.write().await;
        spec.plan(&WorkflowCaps::default())
            .map_err(|e| e.to_string())?;
        g.insert(spec.id.clone(), spec);
        Ok(())
    }

    pub async fn remove(&self, id: &str) -> bool {
        let mut g = self.inner.write().await;
        g.remove(id).is_some()
    }

    /// Plan a spec against the current caps. Returns the plan or an
    /// error string. Exposed so the route handler can validate a
    /// submitted document without running it.
    pub async fn plan(
        &self,
        spec: &WorkflowSpec,
    ) -> Result<WorkflowPlan, WorkflowError> {
        spec.plan(&WorkflowCaps::default())
    }
}
