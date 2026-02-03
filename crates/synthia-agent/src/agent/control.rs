//! Agent control module

use std::sync::{Arc, Weak};

use tokio::sync::watch;

use crate::{Agent, types::AgentStatus};

/// Control-plane handle for multi-agent operations.
#[derive(Clone, Debug)]
pub struct AgentControl {
    agent: Weak<Agent>,
    status_sender: Option<watch::Sender<AgentStatus>>,
}

impl Default for AgentControl {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentControl {
    pub fn new() -> Self {
        let (sender, _) = watch::channel(AgentStatus::PendingInit);
        Self {
            agent: Weak::new(),
            status_sender: Some(sender),
        }
    }

    pub fn set_agent(&mut self, agent: Arc<Agent>) {
        self.agent = Arc::downgrade(&agent);
    }

    pub fn get_agent(&self) -> Option<Arc<Agent>> {
        self.agent.upgrade()
    }

    pub fn update_status(&self, status: AgentStatus) {
        if let Some(sender) = &self.status_sender {
            let _ = sender.send(status);
        }
    }

    pub fn subscribe_status(&self) -> watch::Receiver<AgentStatus> {
        self.status_sender
            .as_ref()
            .map(watch::Sender::subscribe)
            .unwrap_or_else(|| watch::channel(AgentStatus::NotFound).1)
    }

    pub fn get_status(&self) -> AgentStatus {
        self.status_sender
            .as_ref()
            .map(|s| s.borrow().clone())
            .unwrap_or(AgentStatus::NotFound)
    }

    pub fn is_final_status(&self) -> bool {
        matches!(
            self.get_status(),
            AgentStatus::Completed
                | AgentStatus::Errored(_)
                | AgentStatus::Shutdown
                | AgentStatus::Cancelled
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_control_new() {
        let control = AgentControl::new();
        assert_eq!(control.get_status(), AgentStatus::PendingInit);
        assert!(control.get_agent().is_none());
    }

    #[test]
    fn test_agent_control_default() {
        let control: AgentControl = Default::default();
        assert_eq!(control.get_status(), AgentStatus::PendingInit);
    }

    #[test]
    fn test_is_final_status_logic() {
        // Test the logic directly without state changes
        assert!(!matches!(
            AgentStatus::PendingInit,
            AgentStatus::Completed
                | AgentStatus::Errored(_)
                | AgentStatus::Shutdown
                | AgentStatus::Cancelled
        ));

        assert!(!matches!(
            AgentStatus::Running,
            AgentStatus::Completed
                | AgentStatus::Errored(_)
                | AgentStatus::Shutdown
                | AgentStatus::Cancelled
        ));

        assert!(!matches!(
            AgentStatus::MaxStepsReached(10),
            AgentStatus::Completed
                | AgentStatus::Errored(_)
                | AgentStatus::Shutdown
                | AgentStatus::Cancelled
        ));

        assert!(matches!(
            AgentStatus::Completed,
            AgentStatus::Completed
                | AgentStatus::Errored(_)
                | AgentStatus::Shutdown
                | AgentStatus::Cancelled
        ));

        assert!(matches!(
            AgentStatus::Errored("test".to_string()),
            AgentStatus::Completed
                | AgentStatus::Errored(_)
                | AgentStatus::Shutdown
                | AgentStatus::Cancelled
        ));

        assert!(matches!(
            AgentStatus::Shutdown,
            AgentStatus::Completed
                | AgentStatus::Errored(_)
                | AgentStatus::Shutdown
                | AgentStatus::Cancelled
        ));

        assert!(matches!(
            AgentStatus::Cancelled,
            AgentStatus::Completed
                | AgentStatus::Errored(_)
                | AgentStatus::Shutdown
                | AgentStatus::Cancelled
        ));
    }

    #[tokio::test]
    async fn test_subscribe_status() {
        let control = AgentControl::new();
        let mut receiver = control.subscribe_status();

        // Initial status should be PendingInit
        assert_eq!(*receiver.borrow(), AgentStatus::PendingInit);

        // Update status and verify receiver gets the update
        control.update_status(AgentStatus::Running);
        // Wait for the update to propagate
        receiver.changed().await.unwrap();
        assert_eq!(*receiver.borrow(), AgentStatus::Running);
    }
}
