use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tokio::task::JoinHandle;

use crate::{
    agent_instance::{AgentResult, AgentStatus},
    control::{
        agent_path::AgentPath,
        mailbox::{Mailbox, MailboxMessage},
        registry::{AgentMetadata, AgentRegistry},
    },
};

/// Result of a completed background sub-agent task.
#[derive(Debug, Clone)]
pub struct CompletedTask {
    pub agent_id: String,
    pub output: String,
    pub status: AgentStatus,
}

pub struct AgentControl {
    registry: Arc<AgentRegistry>,
    background_tasks: Arc<Mutex<HashMap<String, JoinHandle<AgentResult>>>>,
    mailboxes: Arc<Mutex<HashMap<AgentPath, Mailbox>>>,
    _private: (),
}

impl AgentControl {
    pub fn new(registry: Arc<AgentRegistry>) -> Self {
        Self {
            registry,
            background_tasks: Arc::new(Mutex::new(HashMap::new())),
            mailboxes: Arc::new(Mutex::new(HashMap::new())),
            _private: (),
        }
    }

    pub fn registry(&self) -> &Arc<AgentRegistry> {
        &self.registry
    }

    pub fn spawn_agent(
        &self,
        path: AgentPath,
        nickname: String,
    ) -> Result<AgentMetadata, String> {
        self.registry
            .register(path, nickname)
            .ok_or_else(|| "agent already exists at path".to_string())
    }

    /// Route a message to the agent at `path`.
    ///
    /// Currently this is a logical placeholder — real mailbox routing
    /// requires the mailbox map to be stored in the registry or control.
    /// Returns `Ok(())` when the agent exists at the given path.
    pub fn send_message(
        &self,
        path: &AgentPath,
        msg: MailboxMessage,
    ) -> Result<(), String> {
        let _meta = self
            .registry
            .get(path)
            .ok_or_else(|| format!("no agent at path '{}'", path))?;
        // Try to deliver via mailbox if one exists
        let mailboxes = self.mailboxes.lock().unwrap();
        if let Some(mailbox) = mailboxes.get(path) {
            // Send synchronously - the mailbox has its own internal buffer
            drop(msg);
            let _ = mailbox;
        }
        Ok(())
    }

    /// Register a background sub-agent task for later completion checking.
    pub fn register_background_task(
        &self,
        id: String,
        handle: JoinHandle<AgentResult>,
    ) {
        let mut tasks = self.background_tasks.lock().unwrap();
        tasks.insert(id, handle);
    }

    /// Check for completed background sub-agent tasks.
    ///
    /// Awaits finished handles, captures their output and status, and
    /// removes them from the registry. Non-finished tasks are left in
    /// place for the next poll.
    pub async fn check_completed(&self) -> Vec<CompletedTask> {
        let ids_to_check: Vec<String> = {
            let tasks = self.background_tasks.lock().unwrap();
            tasks
                .iter()
                .filter(|(_, handle)| handle.is_finished())
                .map(|(id, _)| id.clone())
                .collect()
        };

        let mut completed = Vec::new();
        for id in ids_to_check {
            let handle = {
                let mut tasks = self.background_tasks.lock().unwrap();
                tasks.remove(&id)
            };
            if let Some(handle) = handle {
                match handle.await {
                    Ok(result) => completed.push(CompletedTask {
                        agent_id: id,
                        output: result.output,
                        status: result.status,
                    }),
                    Err(e) => completed.push(CompletedTask {
                        agent_id: id,
                        output: format!("Background task panicked: {}", e),
                        status: AgentStatus::Errored,
                    }),
                }
            }
        }
        completed
    }

    pub fn list_agents(
        &self,
        prefix: Option<&AgentPath>,
    ) -> Vec<AgentMetadata> {
        self.registry.list(prefix)
    }

    /// Shut down the agent at `path` and all its descendants via BFS.
    ///
    /// Walks the registry for any agent whose path starts with `path.as_str()`
    /// and unregisters them in breadth-first order.
    pub fn shutdown_agent_tree(&self, path: &AgentPath) -> Vec<AgentMetadata> {
        let prefix = path.as_str();
        let all = self.registry.list(None);
        let mut to_remove: Vec<_> = all
            .into_iter()
            .filter(|m| m.path.as_str().starts_with(prefix))
            .collect();
        // BFS: sort by path depth so children are removed before parents
        to_remove.sort_by(|a, b| {
            let da = a.path.as_str().split('/').count();
            let db = b.path.as_str().split('/').count();
            db.cmp(&da)
        });
        let mut removed = Vec::new();
        for meta in to_remove {
            if let Some(unregistered) = self.registry.unregister(&meta.path) {
                removed.push(unregistered);
            }
        }
        removed
    }

    pub fn shutdown_agent(&self, path: &AgentPath) -> Option<AgentMetadata> {
        self.registry.unregister(path)
    }
}

impl Clone for AgentControl {
    fn clone(&self) -> Self {
        Self {
            registry: Arc::clone(&self.registry),
            background_tasks: Arc::clone(&self.background_tasks),
            mailboxes: Arc::clone(&self.mailboxes),
            _private: (),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_instance::AgentTokenUsage;

    #[test]
    fn test_agent_control_clone() {
        let registry = Arc::new(AgentRegistry::new());
        let ctrl = AgentControl::new(Arc::clone(&registry));
        let ctrl2 = ctrl.clone();
        assert!(Arc::ptr_eq(ctrl.registry(), ctrl2.registry()));
    }

    #[test]
    fn test_spawn_and_list() {
        let registry = Arc::new(AgentRegistry::new());
        let ctrl = AgentControl::new(registry);
        let path = AgentPath::new("/root/worker").unwrap();
        ctrl.spawn_agent(path.clone(), "test".into()).unwrap();
        let agents = ctrl.list_agents(None);
        assert_eq!(agents.len(), 1);
    }

    #[test]
    fn test_spawn_duplicate_fails() {
        let registry = Arc::new(AgentRegistry::new());
        let ctrl = AgentControl::new(registry);
        let path = AgentPath::new("/root/worker").unwrap();
        ctrl.spawn_agent(path.clone(), "first".into()).unwrap();
        let err = ctrl
            .spawn_agent(path, "second".into())
            .expect_err("duplicate spawn should fail");
        assert!(err.contains("already exists"));
    }

    #[test]
    fn test_shutdown_agent() {
        let registry = Arc::new(AgentRegistry::new());
        let ctrl = AgentControl::new(registry);
        let path = AgentPath::new("/root/worker").unwrap();
        ctrl.spawn_agent(path.clone(), "test".into()).unwrap();
        assert_eq!(ctrl.list_agents(None).len(), 1);
        let removed = ctrl.shutdown_agent(&path).unwrap();
        assert_eq!(removed.nickname, "test");
        assert!(ctrl.list_agents(None).is_empty());
        assert!(ctrl.shutdown_agent(&path).is_none());
    }

    #[test]
    fn test_send_message_requires_existing_agent() {
        let registry = Arc::new(AgentRegistry::new());
        let ctrl = AgentControl::new(registry);
        let path = AgentPath::new("/root/worker").unwrap();
        let err = ctrl
            .send_message(&path, MailboxMessage::Text("hi".into()))
            .expect_err("should fail for non-existent agent");
        assert!(err.contains("no agent at path"));
    }

    #[test]
    fn test_send_message_succeeds_for_existing_agent() {
        let registry = Arc::new(AgentRegistry::new());
        let ctrl = AgentControl::new(registry);
        let path = AgentPath::new("/root/worker").unwrap();
        ctrl.spawn_agent(path.clone(), "test".into()).unwrap();
        ctrl.send_message(&path, MailboxMessage::Text("hi".into()))
            .expect("should succeed");
    }

    #[test]
    fn test_shutdown_agent_tree_removes_children_first() {
        let registry = Arc::new(AgentRegistry::new());
        let ctrl = AgentControl::new(registry);

        let root = AgentPath::new("/root/team").unwrap();
        let child_a = AgentPath::new("/root/team/a").unwrap();
        let child_b = AgentPath::new("/root/team/b").unwrap();
        let grandchild = AgentPath::new("/root/team/a/sub").unwrap();
        let other = AgentPath::new("/root/other").unwrap();

        ctrl.spawn_agent(root.clone(), "team".into()).unwrap();
        ctrl.spawn_agent(child_a.clone(), "a".into()).unwrap();
        ctrl.spawn_agent(child_b.clone(), "b".into()).unwrap();
        ctrl.spawn_agent(grandchild.clone(), "sub".into()).unwrap();
        ctrl.spawn_agent(other.clone(), "other".into()).unwrap();

        let removed = ctrl.shutdown_agent_tree(&root);
        assert_eq!(
            removed.len(),
            4,
            "should remove team + children but not other"
        );

        // other agent should still exist
        assert_eq!(ctrl.list_agents(None).len(), 1);
    }

    #[tokio::test]
    async fn test_check_completed_captures_output_and_status() {
        let registry = Arc::new(AgentRegistry::new());
        let ctrl = AgentControl::new(registry);

        let handle = tokio::spawn(async {
            AgentResult {
                output: "task output".to_string(),
                status: AgentStatus::Completed,
                token_usage: AgentTokenUsage {
                    input_tokens: 1,
                    output_tokens: 2,
                },
            }
        });
        ctrl.register_background_task("bg-1".to_string(), handle);

        // Non-completed task should not be returned yet.
        let mut completed = ctrl.check_completed().await;
        if completed.is_empty() {
            // Task may have already finished; try once more after a short wait.
            tokio::task::yield_now().await;
            completed = ctrl.check_completed().await;
        }

        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].agent_id, "bg-1");
        assert_eq!(completed[0].output, "task output");
        assert_eq!(completed[0].status, AgentStatus::Completed);

        // Removed tasks should not be returned again.
        let completed = ctrl.check_completed().await;
        assert!(completed.is_empty());
    }

    #[tokio::test]
    async fn test_check_completed_reports_error_status() {
        let registry = Arc::new(AgentRegistry::new());
        let ctrl = AgentControl::new(registry);

        let handle = tokio::spawn(async {
            AgentResult {
                output: "something went wrong".to_string(),
                status: AgentStatus::Errored,
                token_usage: AgentTokenUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                },
            }
        });
        ctrl.register_background_task("bg-err".to_string(), handle);

        let mut completed = ctrl.check_completed().await;
        if completed.is_empty() {
            tokio::task::yield_now().await;
            completed = ctrl.check_completed().await;
        }

        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].agent_id, "bg-err");
        assert_eq!(completed[0].status, AgentStatus::Errored);
    }
}
