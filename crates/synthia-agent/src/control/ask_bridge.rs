//! Bridge between the permission AskNotifier and the agent control plane mailbox.
//!
//! When a tool requires user confirmation (`Ask` permission), this notifier
//! transitions the agent's mailbox to [`MailboxDeliveryPhase::Suspended`] so
//! no new messages are processed until the user resolves the Ask.

use std::sync::RwLock;

use synthia_permission::ask_notifier::{AskNotifier, AskResolution};

use crate::control::mailbox::MailboxDeliveryPhase;

/// Bridges [`AskNotifier`] events into mailbox phase transitions.
///
/// When `on_ask_triggered` is called the phase is set to `Suspended`.
/// When `on_ask_resolved` is called the phase is set to `NextTurn`,
/// unless the resolution is `Cancelled` (in which case it stays
/// `Suspended` until explicitly resumed).
pub struct AgentControlAskNotifier {
    phase: RwLock<MailboxDeliveryPhase>,
}

impl AgentControlAskNotifier {
    pub fn new() -> Self {
        Self {
            phase: RwLock::new(MailboxDeliveryPhase::CurrentTurn),
        }
    }

    /// Current delivery phase.
    pub fn phase(&self) -> MailboxDeliveryPhase {
        *self.phase.read().unwrap()
    }

    /// Whether the mailbox is currently suspended.
    pub fn is_suspended(&self) -> bool {
        self.phase().is_suspended()
    }

    /// Manually resume the mailbox (e.g. after user deny without going
    /// through `on_ask_resolved`).
    pub fn resume(&self) {
        let mut p = self.phase.write().unwrap();
        p.resume();
    }
}

impl Default for AgentControlAskNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl AskNotifier for AgentControlAskNotifier {
    fn on_ask_triggered(&self, _tool_name: &str, _pattern: &str) {
        let mut p = self.phase.write().unwrap();
        p.suspend();
    }

    fn on_ask_resolved(
        &self,
        _tool_name: &str,
        _pattern: &str,
        resolution: AskResolution,
    ) {
        match resolution {
            AskResolution::Allow | AskResolution::Deny => {
                let mut p = self.phase.write().unwrap();
                p.resume();
            }
            AskResolution::Cancelled => {
                // Stay suspended — the session was cancelled, not resolved.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use synthia_permission::ask_notifier::AskResolution;

    use super::*;

    #[test]
    fn ask_triggered_suspends_mailbox() {
        let notifier = AgentControlAskNotifier::new();
        assert!(!notifier.is_suspended());
        notifier.on_ask_triggered("bash", "rm *");
        assert!(notifier.is_suspended());
    }

    #[test]
    fn ask_resolved_allow_resumes_mailbox() {
        let notifier = AgentControlAskNotifier::new();
        notifier.on_ask_triggered("bash", "rm *");
        assert!(notifier.is_suspended());
        notifier.on_ask_resolved("bash", "rm *", AskResolution::Allow);
        assert!(!notifier.is_suspended());
    }

    #[test]
    fn ask_resolved_deny_resumes_mailbox() {
        let notifier = AgentControlAskNotifier::new();
        notifier.on_ask_triggered("write_file", "/etc/*");
        assert!(notifier.is_suspended());
        notifier.on_ask_resolved("write_file", "/etc/*", AskResolution::Deny);
        assert!(!notifier.is_suspended());
    }

    #[test]
    fn ask_resolved_cancelled_stays_suspended() {
        let notifier = AgentControlAskNotifier::new();
        notifier.on_ask_triggered("bash", "dangerous");
        assert!(notifier.is_suspended());
        notifier.on_ask_resolved("bash", "dangerous", AskResolution::Cancelled);
        assert!(notifier.is_suspended());
    }

    #[test]
    fn manual_resume_works() {
        let notifier = AgentControlAskNotifier::new();
        notifier.on_ask_triggered("bash", "rm *");
        assert!(notifier.is_suspended());
        notifier.resume();
        assert!(!notifier.is_suspended());
    }
}
