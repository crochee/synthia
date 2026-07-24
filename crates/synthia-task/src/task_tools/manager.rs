use std::sync::Arc;

use synthia_core::registry::Registry;
use ulid::Ulid;

use crate::{registry::TaskRegistry, types::Task};

#[derive(Clone)]
pub struct TaskManager {
    registry: Arc<TaskRegistry>,
}

impl TaskManager {
    pub fn new(registry: Arc<TaskRegistry>) -> Self {
        Self { registry }
    }

    pub async fn create(
        &self,
        content: &str,
        dependencies: Vec<String>,
        owner: Option<String>,
    ) -> Option<Task> {
        let id = Ulid::new().to_string();
        let mut task = Task::new(id, 1).with_description(content.to_string());
        if let Some(o) = owner {
            task = task.with_owner(o);
        }

        match self.registry.register(task).await {
            Ok(t) => {
                for dep in &dependencies {
                    if let Err(e) =
                        self.registry.add_dependency(t.id.clone(), dep.clone())
                    {
                        tracing::warn!("Failed to add dependency: {}", e);
                    }
                }
                Some(t)
            }
            Err(_) => None,
        }
    }

    pub async fn get(&self, id: &str) -> Option<Task> {
        self.registry.get(id).await.ok().flatten()
    }

    pub async fn list(&self) -> Vec<Task> {
        self.registry.list(None).await.unwrap_or_default()
    }

    pub async fn update(
        &self,
        id: &str,
        status: Option<crate::types::TaskStatus>,
        content: Option<&str>,
        dependencies: Option<Vec<String>>,
        owner: Option<Option<String>>,
    ) -> Option<Task> {
        let mut task = self.registry.get(id).await.ok().flatten()?;
        if let Some(s) = status {
            match s {
                crate::types::TaskStatus::Running => {
                    let _ = task.start();
                }
                crate::types::TaskStatus::Done => {
                    let _ = task.complete();
                }
                crate::types::TaskStatus::Failed => {
                    let _ = task.fail();
                }
                crate::types::TaskStatus::Blocked => {
                    let _ = task.block();
                }
                _ => {}
            }
        }
        if let Some(c) = content {
            task.description = c.to_string();
        }
        if let Some(deps) = dependencies {
            for dep in deps {
                if let Err(e) =
                    self.registry.add_dependency(id.to_string(), dep)
                {
                    tracing::warn!("Failed to add dependency: {}", e);
                }
            }
        }
        if let Some(o) = owner {
            task.set_owner(o);
        }
        Some(task)
    }

    pub async fn stop(&self, id: &str) -> Option<Task> {
        let mut task = self.registry.get(id).await.ok().flatten()?;
        let _ = task.fail();
        Some(task)
    }

    pub async fn delete(&self, id: &str) -> bool {
        self.registry.unregister(id).await.is_ok()
    }
}
