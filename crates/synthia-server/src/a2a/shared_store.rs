//! `SharedTaskStore` — a `Clone`-friendly `TaskStore` that wraps
//! `InMemoryTaskStore`'s internal `RwLock<HashMap>` in an `Arc`,
//! letting the upstream `DefaultRequestHandler` and the
//! `SynthiaHandler` fallback path observe the *same* backing map.
//!
//! `a2a_server::InMemoryTaskStore` is intentionally not `Clone`:
//! cloning the inner `RwLock` would split the lock and break
//! invariants (writers to one clone wouldn't be visible to readers
//! holding the other). Upstream's `DefaultRequestHandler::new`
//! takes an *owned* `impl TaskStore`, so once the store is moved
//! into the inner handler there is no API to query it from outside.
//!
//! This type is the workaround: it owns an `Arc<RwLock<HashMap>>`
//! directly, so `Clone` only bumps the `Arc` — every clone points
//! at the same map, and reads/writes are linearised through the
//! single `RwLock`.

use std::{collections::HashMap, sync::Arc};

use a2a::{A2AError, ListTasksResponse, Task, TaskId};
use a2a_server::{TaskStore, task_store::TaskVersion};
use async_trait::async_trait;
use tokio::sync::RwLock;

/// Stored task + version, kept in the same shape as upstream
/// `InMemoryTaskStore` so the API surface is drop-in compatible.
#[derive(Clone)]
struct StoredEntry {
    task: Task,
    version: TaskVersion,
}

/// `Clone`-friendly TaskStore that delegates to a shared map.
///
/// Cloning produces a new handle pointing at the same underlying
/// state, which is the entire point of this type — Synthia needs
/// one handle inside `DefaultRequestHandler` and one inside the
/// `SynthiaHandler` wrapper's fallback path, and they must observe
/// the same writes.
#[derive(Clone)]
pub struct SharedTaskStore {
    tasks: Arc<RwLock<HashMap<TaskId, StoredEntry>>>,
}

impl SharedTaskStore {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for SharedTaskStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TaskStore for SharedTaskStore {
    async fn create(&self, task: Task) -> Result<TaskVersion, A2AError> {
        let mut store = self.tasks.write().await;
        if store.contains_key(&task.id) {
            return Err(A2AError::internal("task already exists"));
        }
        let id = task.id.clone();
        store.insert(id, StoredEntry { task, version: 1 });
        Ok(1)
    }

    async fn update(&self, task: Task) -> Result<TaskVersion, A2AError> {
        let mut store = self.tasks.write().await;
        let entry = store
            .get_mut(&task.id)
            .ok_or_else(|| A2AError::task_not_found(&task.id))?;
        entry.version += 1;
        entry.task = task;
        Ok(entry.version)
    }

    async fn get(&self, task_id: &str) -> Result<Option<Task>, A2AError> {
        let store = self.tasks.read().await;
        Ok(store.get(task_id).map(|e| e.task.clone()))
    }

    async fn list(
        &self,
        req: &a2a::ListTasksRequest,
    ) -> Result<ListTasksResponse, A2AError> {
        let store = self.tasks.read().await;
        let mut tasks: Vec<Task> = store
            .values()
            .filter(|entry| {
                if let Some(ref ctx_id) = req.context_id
                    && entry.task.context_id != *ctx_id
                {
                    return false;
                }
                if let Some(ref status) = req.status
                    && entry.task.status.state != *status
                {
                    return false;
                }
                true
            })
            .map(|e| e.task.clone())
            .collect();

        tasks.sort_by(|a, b| a.id.cmp(&b.id));

        let page_size = match req.page_size {
            Some(size) if size > 0 => size as usize,
            _ => 50,
        };
        // `page_token` is the task id of the LAST item the client
        // already saw. We resume after it so the next page does
        // NOT contain duplicates of the previous one.
        //
        // Upstream `InMemoryTaskStore::list` treated `page_token`
        // as a numeric index; Synthia's v1 cursor wraps a task
        // id (base64-decoded by the route handler), so we look
        // the id up instead. If the cursor points at an id that
        // is no longer in the store (deleted), `position` returns
        // `None` and we start from the end — i.e. an empty page,
        // matching the existing `paginate` contract in
        // `synthia-server::routes::helpers`.
        let start = match req.page_token.as_deref() {
            None | Some("") => 0,
            Some(id) => tasks
                .iter()
                .position(|t| t.id == id)
                .map(|p| p + 1)
                .unwrap_or(usize::MAX),
        };
        let total_size = tasks.len();
        if start >= total_size {
            return Ok(ListTasksResponse {
                tasks: Vec::new(),
                next_page_token: String::new(),
                page_size: page_size as i32,
                total_size: total_size as i32,
            });
        }
        let end = (start + page_size).min(total_size);
        let page: Vec<Task> = tasks.drain(start..end).collect();
        let last_id = page.last().map(|t| t.id.clone()).unwrap_or_default();
        let next_page_token = if end < total_size {
            last_id
        } else {
            String::new()
        };

        // Upstream `InMemoryTaskStore::list` ignores this — we
        // match that behaviour for now (callers don't request
        // history truncation in our flow).
        Ok(ListTasksResponse {
            tasks: page,
            next_page_token,
            page_size: page_size as i32,
            total_size: total_size as i32,
        })
    }
}

#[cfg(test)]
mod tests {
    use a2a::{ListTasksRequest, TaskStatus};
    use a2a_server::TaskStore;

    use super::*;

    fn make_task(id: &str, state: a2a::TaskState) -> Task {
        Task {
            id: id.to_string(),
            context_id: "ctx".to_string(),
            status: TaskStatus {
                state,
                message: None,
                timestamp: None,
            },
            artifacts: None,
            history: None,
            metadata: None,
        }
    }

    #[tokio::test]
    async fn clones_share_state() {
        let a = SharedTaskStore::new();
        let b = a.clone();
        a.create(make_task("t1", a2a::TaskState::Submitted))
            .await
            .unwrap();
        // The clone must observe the create.
        let got = b.get("t1").await.unwrap().expect("t1 visible to b");
        assert_eq!(got.id, "t1");
    }

    #[tokio::test]
    async fn duplicate_create_returns_error() {
        let store = SharedTaskStore::new();
        store
            .create(make_task("dup", a2a::TaskState::Submitted))
            .await
            .unwrap();
        let err = store
            .create(make_task("dup", a2a::TaskState::Submitted))
            .await
            .unwrap_err();
        // The shared store returns the same internal-error variant
        // upstream uses for duplicate creates; we only assert it
        // errors, not the exact code.
        assert_eq!(err.code, a2a::errors::error_code::INTERNAL_ERROR);
    }

    #[tokio::test]
    async fn update_unknown_task_returns_not_found() {
        let store = SharedTaskStore::new();
        let err = store
            .update(make_task("nope", a2a::TaskState::Working))
            .await
            .unwrap_err();
        let _ = err; // any A2AError is acceptable; the call must Err
    }

    #[tokio::test]
    async fn list_filters_by_status() {
        let store = SharedTaskStore::new();
        store
            .create(make_task("t1", a2a::TaskState::Submitted))
            .await
            .unwrap();
        store
            .create(make_task("t2", a2a::TaskState::Working))
            .await
            .unwrap();
        let req = ListTasksRequest {
            context_id: None,
            status: Some(a2a::TaskState::Working),
            page_size: None,
            page_token: None,
            history_length: None,
            status_timestamp_after: None,
            include_artifacts: None,
            tenant: None,
        };
        let resp = store.list(&req).await.unwrap();
        assert_eq!(resp.tasks.len(), 1);
        assert_eq!(resp.tasks[0].id, "t2");
    }

    #[tokio::test]
    async fn update_via_clone_visible_to_original() {
        let a = SharedTaskStore::new();
        let b = a.clone();
        a.create(make_task("t1", a2a::TaskState::Submitted))
            .await
            .unwrap();
        b.update(make_task("t1", a2a::TaskState::Working))
            .await
            .unwrap();
        let got = a.get("t1").await.unwrap().unwrap();
        assert_eq!(got.status.state, a2a::TaskState::Working);
    }

    // -- new() / default() ----------------------------------------

    /// `SharedTaskStore::new` MUST start with 0 tasks.
    #[tokio::test]
    async fn new_starts_empty() {
        let store = SharedTaskStore::new();
        let req = ListTasksRequest {
            context_id: None,
            status: None,
            page_size: None,
            page_token: None,
            history_length: None,
            status_timestamp_after: None,
            include_artifacts: None,
            tenant: None,
        };
        let resp = store.list(&req).await.unwrap();
        assert!(resp.tasks.is_empty());
        assert_eq!(resp.total_size, 0);
    }

    /// `SharedTaskStore::default` MUST match `new`.
    #[tokio::test]
    async fn default_matches_new() {
        let a = SharedTaskStore::default();
        let b = SharedTaskStore::new();
        // Both must be empty and share NO state (separate instances).
        let req = ListTasksRequest {
            context_id: None,
            status: None,
            page_size: None,
            page_token: None,
            history_length: None,
            status_timestamp_after: None,
            include_artifacts: None,
            tenant: None,
        };
        assert!(a.list(&req).await.unwrap().tasks.is_empty());
        assert!(b.list(&req).await.unwrap().tasks.is_empty());
    }

    // -- get --------------------------------------------------------

    /// `get` MUST return `Ok(None)` for an unknown task id.
    #[tokio::test]
    async fn get_returns_none_for_unknown() {
        let store = SharedTaskStore::new();
        let result = store.get("nope").await.unwrap();
        assert!(result.is_none());
    }

    // -- update -----------------------------------------------------

    /// `update` MUST increment the version monotonically.
    #[tokio::test]
    async fn update_increments_version() {
        let store = SharedTaskStore::new();
        let v1 = store
            .create(make_task("t", a2a::TaskState::Submitted))
            .await
            .unwrap();
        assert_eq!(v1, 1);
        let v2 = store
            .update(make_task("t", a2a::TaskState::Working))
            .await
            .unwrap();
        assert_eq!(v2, 2);
        let v3 = store
            .update(make_task("t", a2a::TaskState::Completed))
            .await
            .unwrap();
        assert_eq!(v3, 3);
    }

    // -- list pagination -------------------------------------------

    /// `list` MUST respect `page_size` and emit a `next_page_token`
    /// that names the last task on this page.
    #[tokio::test]
    async fn list_paginates_with_page_size() {
        let store = SharedTaskStore::new();
        for i in 0..5 {
            store
                .create(make_task(&format!("t{i}"), a2a::TaskState::Submitted))
                .await
                .unwrap();
        }
        // Sorted by id: t0, t1, t2, t3, t4. page_size=2 →
        // first page = [t0, t1]; next_page_token = "t1" (the
        // last id on this page) so the route handler can
        // resume after it.
        let req = ListTasksRequest {
            context_id: None,
            status: None,
            page_size: Some(2),
            page_token: None,
            history_length: None,
            status_timestamp_after: None,
            include_artifacts: None,
            tenant: None,
        };
        let resp = store.list(&req).await.unwrap();
        assert_eq!(resp.tasks.len(), 2);
        assert_eq!(resp.tasks[0].id, "t0");
        assert_eq!(resp.tasks[1].id, "t1");
        assert_eq!(resp.next_page_token, "t1");
        assert_eq!(resp.total_size, 5);
        assert_eq!(resp.page_size, 2);
    }

    /// `list` MUST resume AFTER the `page_token` so the next
    /// page contains no ids from the previous one.
    #[tokio::test]
    async fn list_resumes_after_page_token_id() {
        let store = SharedTaskStore::new();
        for i in 0..5 {
            store
                .create(make_task(&format!("t{i}"), a2a::TaskState::Submitted))
                .await
                .unwrap();
        }
        // Resume after "t1" (the cursor we just emitted).
        let req = ListTasksRequest {
            context_id: None,
            status: None,
            page_size: Some(2),
            page_token: Some("t1".to_string()),
            history_length: None,
            status_timestamp_after: None,
            include_artifacts: None,
            tenant: None,
        };
        let resp = store.list(&req).await.unwrap();
        assert_eq!(resp.tasks.len(), 2);
        assert_eq!(resp.tasks[0].id, "t2");
        assert_eq!(resp.tasks[1].id, "t3");
        assert_eq!(resp.next_page_token, "t3");
    }

    /// `list` MUST return an empty page when the cursor points
    /// at an id that no longer exists (deleted between pages).
    #[tokio::test]
    async fn list_with_stale_cursor_returns_empty() {
        let store = SharedTaskStore::new();
        store
            .create(make_task("t0", a2a::TaskState::Submitted))
            .await
            .unwrap();
        let req = ListTasksRequest {
            context_id: None,
            status: None,
            page_size: Some(10),
            page_token: Some("deleted".to_string()),
            history_length: None,
            status_timestamp_after: None,
            include_artifacts: None,
            tenant: None,
        };
        let resp = store.list(&req).await.unwrap();
        assert!(resp.tasks.is_empty());
        assert_eq!(resp.next_page_token, "");
        assert_eq!(resp.total_size, 1);
    }

    /// `list` MUST sort by task id ascending.
    #[tokio::test]
    async fn list_sorts_by_id_ascending() {
        let store = SharedTaskStore::new();
        // Insert in random order.
        for id in ["t3", "t1", "t2"] {
            store
                .create(make_task(id, a2a::TaskState::Submitted))
                .await
                .unwrap();
        }
        let req = ListTasksRequest {
            context_id: None,
            status: None,
            page_size: None,
            page_token: None,
            history_length: None,
            status_timestamp_after: None,
            include_artifacts: None,
            tenant: None,
        };
        let resp = store.list(&req).await.unwrap();
        assert_eq!(resp.tasks[0].id, "t1");
        assert_eq!(resp.tasks[1].id, "t2");
        assert_eq!(resp.tasks[2].id, "t3");
    }

    /// `list` MUST filter by `context_id`.
    #[tokio::test]
    async fn list_filters_by_context_id() {
        let store = SharedTaskStore::new();
        let mut t1 = make_task("t1", a2a::TaskState::Submitted);
        t1.context_id = "ctx-A".to_string();
        let mut t2 = make_task("t2", a2a::TaskState::Submitted);
        t2.context_id = "ctx-B".to_string();
        store.create(t1).await.unwrap();
        store.create(t2).await.unwrap();

        let req = ListTasksRequest {
            context_id: Some("ctx-A".to_string()),
            status: None,
            page_size: None,
            page_token: None,
            history_length: None,
            status_timestamp_after: None,
            include_artifacts: None,
            tenant: None,
        };
        let resp = store.list(&req).await.unwrap();
        assert_eq!(resp.tasks.len(), 1);
        assert_eq!(resp.tasks[0].id, "t1");
        assert_eq!(resp.tasks[0].context_id, "ctx-A");
    }

    /// `list` MUST default `page_size` to 50 when `None`.
    #[tokio::test]
    async fn list_defaults_page_size_to_50() {
        let store = SharedTaskStore::new();
        for i in 0..3 {
            store
                .create(make_task(&format!("t{i}"), a2a::TaskState::Submitted))
                .await
                .unwrap();
        }
        let req = ListTasksRequest {
            context_id: None,
            status: None,
            page_size: None,
            page_token: None,
            history_length: None,
            status_timestamp_after: None,
            include_artifacts: None,
            tenant: None,
        };
        let resp = store.list(&req).await.unwrap();
        assert_eq!(resp.page_size, 50);
    }
}
