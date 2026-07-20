//! Instance lifecycle — `spawn` / `instance_exists` / `stop` /
//! `stop_tree` / `wrap_as_tool` / `list_instances` /
//! `instance_count`.

use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use synthia_core::{Error, generate_session_id};

use super::agent_registry::AgentRegistry;
use crate::registry::{
    instance::AgentStatus,
    tool_wrapper::AgentToolWrapper,
    types::AgentDefinition,
};

/// Internal representation of a live agent instance managed by
/// [`AgentRegistry`].
pub(crate) struct RegistryInstance {
    pub definition: Option<AgentDefinition>,
    pub state: AgentStatus,
    pub parent_id: Option<String>,
}

impl AgentRegistry {
    /// Spawn a new agent instance from a loaded definition.
    ///
    /// Returns the new instance id. The instance is registered
    /// in `self.instances` before this function returns. If
    /// `parent_id` is `Some` and the parent already has its
    /// own parent (i.e. this would push the tree past
    /// `self.max_depth`), the call fails with
    /// [`Error::InvalidItem`].
    pub fn spawn(
        &self,
        agent_name: &str,
        parent_id: Option<String>,
        _token_budget: Option<synthia_session::types::TokenBudget>,
    ) -> Result<String, Error> {
        if let Some(ref pid) = parent_id {
            let instances = self.instances.read();
            if let Some(parent) = instances.get(pid) {
                let inner = parent.lock().unwrap();
                if inner.parent_id.is_some() {
                    return Err(Error::InvalidItem(
                        "depth limit exceeded (max: 1)".to_string(),
                    ));
                }
            }
        }

        let defs = self.definitions.read();
        let definition = defs
            .get(agent_name)
            .ok_or_else(|| {
                Error::NotFound(format!("agent '{}' not found", agent_name))
            })?
            .clone();
        drop(defs);

        let instance_id = generate_session_id();

        let instance = RegistryInstance {
            definition: Some(definition),
            state: AgentStatus::Idle,
            parent_id,
        };

        let mut instances = self.instances.write();
        instances.insert(instance_id.clone(), Arc::new(Mutex::new(instance)));

        Ok(instance_id)
    }

    /// Check whether a live instance with the given id exists.
    pub fn instance_exists(&self, instance_id: &str) -> bool {
        let instances = self.instances.read();
        instances.contains_key(instance_id)
    }

    /// Stop and remove a single instance.
    pub async fn stop(&self, instance_id: &str) -> Result<(), Error> {
        let mut instances = self.instances.write();

        if let Some(instance) = instances.get(instance_id) {
            let mut inner = instance.lock().unwrap();
            inner.state = AgentStatus::Cancelled;
            drop(inner);
            instances.shift_remove(instance_id);

            Ok(())
        } else {
            Err(Error::NotFound(format!(
                "agent instance '{}' not found",
                instance_id
            )))
        }
    }

    /// Stop and remove an instance plus all of its
    /// descendants (the entire sub-tree rooted at
    /// `instance_id`).
    pub async fn stop_tree(&self, instance_id: &str) -> Result<(), Error> {
        let mut instances = self.instances.write();

        fn collect_descendants(
            instances: &IndexMap<String, Arc<Mutex<RegistryInstance>>>,
            target_id: &str,
        ) -> Vec<String> {
            let mut result = Vec::new();
            for (id, instance) in instances.iter() {
                let inner = instance.lock().unwrap();
                if inner.parent_id.as_deref() == Some(target_id) {
                    result.push(id.clone());
                    drop(inner);
                    result.extend(collect_descendants(instances, id));
                }
            }
            result
        }

        let descendants = collect_descendants(&instances, instance_id);
        for desc_id in descendants {
            if let Some(instance) = instances.get(&desc_id) {
                let mut inner = instance.lock().unwrap();
                inner.state = AgentStatus::Cancelled;
                drop(inner);
            }
            instances.shift_remove(&desc_id);
        }

        if let Some(instance) = instances.get(instance_id) {
            let mut inner = instance.lock().unwrap();
            inner.state = AgentStatus::Cancelled;
            drop(inner);
            instances.shift_remove(instance_id);
        } else {
            drop(instances);
            return Err(Error::NotFound(format!(
                "agent instance '{}' not found",
                instance_id
            )));
        }

        Ok(())
    }

    /// Wrap a live instance as an [`AgentToolWrapper`] so it
    /// can be exposed as a callable tool to a parent agent.
    pub fn wrap_as_tool(
        &self,
        instance_id: &str,
    ) -> Result<AgentToolWrapper, Error> {
        let instances = self.instances.read();
        let instance = instances
            .get(instance_id)
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "agent instance '{}' not found",
                    instance_id
                ))
            })?
            .clone();

        let inner = instance.lock().unwrap();
        let definition = inner
            .definition
            .clone()
            .expect("definition should always be set for spawned instances");
        drop(inner);

        Ok(AgentToolWrapper {
            instance_id: instance_id.to_string(),
            definition,
            agent_registry: Arc::new(self.clone()),
        })
    }

    /// Snapshot of all live instance ids.
    pub fn list_instances(&self) -> Vec<String> {
        let instances = self.instances.read();
        instances.keys().cloned().collect()
    }

    /// Number of live instances.
    pub fn instance_count(&self) -> usize {
        let instances = self.instances.read();
        instances.len()
    }
}
