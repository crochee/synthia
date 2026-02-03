#[derive(Debug, Clone, Default)]
pub struct OutputStyleConfig {
    pub name: String,
    pub prompt: String,
    pub keep_coding_instructions: bool,
}

pub const SYSTEM_PROMPT_DYNAMIC_BOUNDARY: &str =
    "__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__";

pub const TITLE_SYSTEM_PROMPT: &str = "Generate a short title (4-7 words max) for this conversation. Reply only the title.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_style_config_default() {
        let config = OutputStyleConfig::default();
        assert!(config.name.is_empty());
        assert!(config.prompt.is_empty());
        assert!(!config.keep_coding_instructions);
    }

    #[test]
    fn test_output_style_config_debug() {
        let config = OutputStyleConfig {
            name: "test".to_string(),
            prompt: "prompt text".to_string(),
            keep_coding_instructions: true,
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("test"));
        assert!(debug.contains("prompt text"));
        assert!(debug.contains("true"));
    }

    #[test]
    fn test_output_style_config_clone() {
        let original = OutputStyleConfig {
            name: "clone".to_string(),
            prompt: "test prompt".to_string(),
            keep_coding_instructions: false,
        };
        let cloned = original.clone();
        assert_eq!(cloned.name, original.name);
        assert_eq!(cloned.prompt, original.prompt);
        assert_eq!(
            cloned.keep_coding_instructions,
            original.keep_coding_instructions
        );
    }

    #[test]
    fn test_system_prompt_dynamic_boundary() {
        assert_eq!(
            SYSTEM_PROMPT_DYNAMIC_BOUNDARY,
            "__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__"
        );
    }

    #[test]
    fn test_title_system_prompt() {
        assert!(TITLE_SYSTEM_PROMPT.contains("4-7 words"));
        assert!(TITLE_SYSTEM_PROMPT.contains("title"));
    }
}
