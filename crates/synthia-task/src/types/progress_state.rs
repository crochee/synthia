use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProgressState {
    pub steps_total: usize,
    pub steps_completed: usize,
}

impl ProgressState {
    pub fn new(total: usize) -> Self {
        Self {
            steps_total: total,
            steps_completed: 0,
        }
    }

    pub fn advance(&mut self) {
        self.steps_completed = (self.steps_completed + 1).min(self.steps_total);
    }

    pub fn advance_by(&mut self, count: usize) {
        self.steps_completed =
            (self.steps_completed + count).min(self.steps_total);
    }

    pub fn is_complete(&self) -> bool {
        self.steps_completed >= self.steps_total
    }

    pub fn percentage(&self) -> f64 {
        if self.steps_total == 0 {
            return 0.0;
        }
        (self.steps_completed as f64 / self.steps_total as f64) * 100.0
    }
}
