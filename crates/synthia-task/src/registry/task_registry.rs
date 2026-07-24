use std::{collections::HashMap, sync::RwLock};

use async_trait::async_trait;
use synthia_core::{
    Error,
    registry::{Registry, RegistryItem},
};

use super::filter::TaskFilter;
use crate::{topology::Topology, types::Task};

pub struct TaskRegistry {
    tasks: RwLock<HashMap<String, Task>>,
    topology: RwLock<Topology>,
}

impl TaskRegistry {
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
            topology: RwLock::new(Topology::new()),
        }
    }

    pub fn add_dependency(
        &self,
        from: String,
        to: String,
    ) -> Result<(), Error> {
        let tasks = self.tasks.read().map_err(|_| {
            Error::Internal("Failed to acquire read lock on tasks".to_string())
        })?;
        if !tasks.contains_key(&from) {
            return Err(Error::NotFound(format!("task {from}")));
        }
        if !tasks.contains_key(&to) {
            return Err(Error::NotFound(format!("task {to}")));
        }
        drop(tasks);

        let mut topo = self.topology.write().map_err(|_| {
            Error::Internal(
                "Failed to acquire write lock on topology".to_string(),
            )
        })?;
        topo.add_dependency(from, to)
    }

    pub fn remove_dependency(&self, from: &str, to: &str) {
        if let Ok(mut topo) = self.topology.write() {
            topo.remove_dependency(from, to);
        }
    }

    pub fn get_dependencies(&self, id: &str) -> Vec<String> {
        self.topology
            .read()
            .map(|topo| topo.get_dependencies(id))
            .unwrap_or_default()
    }

    pub fn get_dependents(&self, id: &str) -> Vec<String> {
        self.topology
            .read()
            .map(|topo| topo.get_dependents(id))
            .unwrap_or_default()
    }

    pub fn topological_sort(&self) -> Result<Vec<String>, Error> {
        let topo = self.topology.read().map_err(|_| {
            Error::Internal(
                "Failed to acquire read lock on topology".to_string(),
            )
        })?;
        topo.topological_sort()
    }

    pub fn detect_cycle(&self) -> Option<Vec<String>> {
        self.topology
            .read()
            .ok()
            .and_then(|topo| topo.detect_cycle())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.tasks
            .read()
            .map(|t| t.contains_key(name))
            .unwrap_or(false)
    }

    pub fn len(&self) -> usize {
        self.tasks.read().map(|t| t.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.read().map(|t| t.is_empty()).unwrap_or(true)
    }
}

impl Default for TaskRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Registry<Task> for TaskRegistry {
    type Filter = TaskFilter;

    async fn register(&self, item: Task) -> Result<Task, Error> {
        let name = item.name().to_string();
        let mut tasks = self.tasks.write().map_err(|_| {
            Error::Internal("Failed to acquire write lock on tasks".to_string())
        })?;
        if tasks.contains_key(&name) {
            return Err(Error::AlreadyExists(name));
        }
        tasks.insert(name, item.clone());
        Ok(item)
    }

    async fn unregister(&self, name: &str) -> Result<(), Error> {
        let mut tasks = self.tasks.write().map_err(|_| {
            Error::Internal("Failed to acquire write lock on tasks".to_string())
        })?;
        let removed = tasks.remove(name);
        drop(tasks);

        if let Some(ref task) = removed {
            if let Ok(mut topo) = self.topology.write() {
                topo.remove_node(&task.id);
            }
        } else {
            return Err(Error::NotFound(name.to_string()));
        }

        Ok(())
    }

    async fn get(&self, name: &str) -> Result<Option<Task>, Error> {
        let tasks = self.tasks.read().map_err(|_| {
            Error::Internal("Failed to acquire read lock on tasks".to_string())
        })?;
        Ok(tasks.get(name).cloned())
    }

    async fn list(
        &self,
        filter: Option<Self::Filter>,
    ) -> Result<Vec<Task>, Error> {
        let filter = filter.unwrap_or_default();
        let tasks = self.tasks.read().map_err(|_| {
            Error::Internal("Failed to acquire read lock on tasks".to_string())
        })?;
        let result: Vec<Task> = tasks
            .values()
            .filter(|t| filter.accepts(t))
            .cloned()
            .collect();
        Ok(result)
    }
}
