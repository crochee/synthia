use serde::{Deserialize, Serialize};

use super::{
    notification::Notification,
    progress_state::ProgressState,
    structured_output::StructuredOutput,
    task_status::TaskStatus,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    #[serde(default)]
    pub description: String,
    pub status: TaskStatus,
    pub progress: ProgressState,
    pub output: Vec<StructuredOutput>,
    pub notifications: Vec<Notification>,
    #[serde(default)]
    pub owner: Option<String>,
}

impl synthia_core::registry::RegistryItem for Task {
    fn name(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        ""
    }
}

impl Task {
    pub fn new(id: String, steps: usize) -> Self {
        Self {
            id,
            description: String::new(),
            status: TaskStatus::Pending,
            progress: ProgressState::new(steps),
            output: Vec::new(),
            notifications: Vec::new(),
            owner: None,
        }
    }

    pub fn with_description(self, description: String) -> Self {
        Self {
            description,
            ..self
        }
    }

    pub fn with_owner(mut self, owner: String) -> Self {
        self.owner = Some(owner);
        self
    }

    pub fn set_owner(&mut self, owner: Option<String>) {
        self.owner = owner;
    }

    pub fn start(&mut self) -> bool {
        if self.status != TaskStatus::Pending {
            return false;
        }
        self.status = TaskStatus::Running;
        true
    }

    pub fn complete(&mut self) -> bool {
        if self.status != TaskStatus::Running {
            return false;
        }
        self.status = TaskStatus::Done;
        self.progress.steps_completed = self.progress.steps_total;
        true
    }

    pub fn fail(&mut self) -> bool {
        if !matches!(self.status, TaskStatus::Running | TaskStatus::Pending) {
            return false;
        }
        self.status = TaskStatus::Failed;
        true
    }

    pub fn block(&mut self) -> bool {
        if self.status != TaskStatus::Running {
            return false;
        }
        self.status = TaskStatus::Blocked;
        true
    }

    pub fn unblock(&mut self) -> bool {
        if self.status != TaskStatus::Blocked {
            return false;
        }
        self.status = TaskStatus::Running;
        true
    }

    pub fn add_output(&mut self, output: StructuredOutput) {
        self.output.push(output);
    }

    pub fn completion_percentage(&self) -> f64 {
        if self.progress.steps_total == 0 {
            return 0.0;
        }
        (self.progress.steps_completed as f64
            / self.progress.steps_total as f64)
            * 100.0
    }
}
