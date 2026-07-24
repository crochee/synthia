use std::sync::OnceLock;

/// Notifier for Ask trigger/resolve events.
/// Used by the permission system to notify the agent control plane
/// when an Ask is triggered (suspend mailbox) and when it resolves (resume).
pub trait AskNotifier: Send + Sync {
    fn on_ask_triggered(&self, tool_name: &str, pattern: &str);

    fn on_ask_resolved(
        &self,
        tool_name: &str,
        pattern: &str,
        resolution: AskResolution,
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskResolution {
    Allow,
    Deny,
    Cancelled,
}

impl AskNotifier for () {
    fn on_ask_triggered(&self, _: &str, _: &str) {}

    fn on_ask_resolved(&self, _: &str, _: &str, _: AskResolution) {}
}

static NOOP_NOTIFIER: OnceLock<NoopAskNotifier> = OnceLock::new();

/// Returns the global no-op AskNotifier instance.
pub fn noop_notifier() -> &'static NoopAskNotifier {
    NOOP_NOTIFIER.get_or_init(|| NoopAskNotifier)
}

/// No-op AskNotifier for single-agent CLI mode.
/// All methods are no-ops that do nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopAskNotifier;

impl AskNotifier for NoopAskNotifier {
    fn on_ask_triggered(&self, _: &str, _: &str) {}

    fn on_ask_resolved(&self, _: &str, _: &str, _: AskResolution) {}
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    struct RecordingNotifier {
        triggered: Mutex<Vec<(String, String)>>,
        resolved: Mutex<Vec<(String, String, AskResolution)>>,
    }

    impl RecordingNotifier {
        fn triggered(&self) -> Vec<(String, String)> {
            self.triggered.lock().unwrap().clone()
        }

        fn resolved(&self) -> Vec<(String, String, AskResolution)> {
            self.resolved.lock().unwrap().clone()
        }
    }

    impl AskNotifier for RecordingNotifier {
        fn on_ask_triggered(&self, tool_name: &str, pattern: &str) {
            self.triggered
                .lock()
                .unwrap()
                .push((tool_name.to_string(), pattern.to_string()));
        }

        fn on_ask_resolved(
            &self,
            tool_name: &str,
            pattern: &str,
            resolution: AskResolution,
        ) {
            self.resolved.lock().unwrap().push((
                tool_name.to_string(),
                pattern.to_string(),
                resolution,
            ));
        }
    }

    #[test]
    fn test_record_notifier_captures_events() {
        let notifier = RecordingNotifier::default();
        notifier.on_ask_triggered("bash", "rm *");
        notifier.on_ask_resolved("bash", "rm *", AskResolution::Allow);

        assert_eq!(
            notifier.triggered(),
            vec![("bash".to_string(), "rm *".to_string())]
        );
        assert_eq!(
            notifier.resolved(),
            vec![(
                "bash".to_string(),
                "rm *".to_string(),
                AskResolution::Allow
            )]
        );
    }

    #[test]
    fn test_arc_notifier_dispatches() {
        let notifier: Arc<dyn AskNotifier> =
            Arc::new(RecordingNotifier::default());
        notifier.on_ask_triggered("write_file", "/etc/*");
        notifier.on_ask_resolved("write_file", "/etc/*", AskResolution::Deny);
    }

    #[test]
    fn test_unit_notifier_is_noop() {
        let notifier: Arc<dyn AskNotifier> = Arc::new(());
        notifier.on_ask_triggered("bash", "anything");
        notifier.on_ask_resolved("bash", "anything", AskResolution::Cancelled);
    }

    #[test]
    fn test_ask_resolution_variants() {
        assert_eq!(AskResolution::Allow, AskResolution::Allow);
        assert_ne!(AskResolution::Allow, AskResolution::Deny);
        assert_ne!(AskResolution::Deny, AskResolution::Cancelled);
    }

    #[test]
    fn test_noop_notifier_is_noop() {
        let notifier: Arc<dyn AskNotifier> = Arc::new(NoopAskNotifier);
        notifier.on_ask_triggered("bash", "rm *");
        notifier.on_ask_resolved("write_file", "/etc/*", AskResolution::Allow);
        notifier.on_ask_resolved("bash", "rm *", AskResolution::Deny);
        notifier.on_ask_resolved("bash", "rm *", AskResolution::Cancelled);
    }

    #[test]
    fn test_noop_notifier_returns_same_instance() {
        let a: &'static NoopAskNotifier = noop_notifier();
        let b: &'static NoopAskNotifier = noop_notifier();
        assert!(std::ptr::eq(a, b));
    }
}
