use std::{collections::HashMap, sync::Arc};

use parking_lot::Mutex;
use tokio::process::Child;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct BackgroundCommand {
    pub id: String,
    pub command: String,
    pub pid: Option<u32>,
}

pub struct CommandManager {
    processes: Arc<Mutex<HashMap<String, Child>>>,
    metadata: Arc<Mutex<HashMap<String, BackgroundCommand>>>,
}

impl CommandManager {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
            metadata: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn register(&self, command: &str, child: Child) -> String {
        let id = Uuid::new_v4().to_string();
        let pid = child.id();

        let meta = BackgroundCommand {
            id: id.clone(),
            command: command.to_string(),
            pid,
        };

        self.processes.lock().insert(id.clone(), child);
        self.metadata.lock().insert(id.clone(), meta);

        id
    }

    pub fn get_child(&self, id: &str) -> Option<Child> {
        self.processes.lock().remove(id)
    }

    pub fn list(&self) -> Vec<BackgroundCommand> {
        self.metadata.lock().values().cloned().collect()
    }

    pub fn remove(&self, id: &str) {
        self.processes.lock().remove(id);
        self.metadata.lock().remove(id);
    }
}

impl Default for CommandManager {
    fn default() -> Self {
        Self::new()
    }
}
