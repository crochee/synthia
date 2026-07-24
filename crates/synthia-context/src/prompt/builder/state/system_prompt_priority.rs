#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SystemPromptPriority {
    Override,
    Coordinator,
    Agent,
    Custom,
    #[default]
    Default,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_priority_order() {
        use SystemPromptPriority::*;

        let priorities = vec![Override, Coordinator, Agent, Custom, Default];
        for p in &priorities {
            let _ = format!("{p:?}");
        }
    }

    #[test]
    fn test_system_prompt_priority_default() {
        let priority = SystemPromptPriority::default();
        assert_eq!(priority, SystemPromptPriority::Default);
    }

    #[test]
    fn test_system_prompt_priority_partial_eq() {
        assert_eq!(
            SystemPromptPriority::Default,
            SystemPromptPriority::Default
        );
        assert_ne!(
            SystemPromptPriority::Default,
            SystemPromptPriority::Override
        );
    }
}
