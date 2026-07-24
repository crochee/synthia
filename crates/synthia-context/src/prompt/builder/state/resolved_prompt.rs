#[derive(Debug, Clone)]
pub struct ResolvedPrompt {
    pub static_content: String,
    pub dynamic_content: String,
    pub sections_used: Vec<String>,
    pub prefix_hash: String,
    pub static_hash: String,
}

impl ResolvedPrompt {
    pub fn full_prompt(&self) -> String {
        if self.dynamic_content.is_empty() {
            self.static_content.clone()
        } else {
            format!(
                "{}\n\n{}\n\n{}",
                self.static_content,
                crate::prompt::SYSTEM_PROMPT_DYNAMIC_BOUNDARY,
                self.dynamic_content
            )
        }
    }

    pub fn get_static_prefix(&self) -> &str {
        &self.static_content
    }

    pub fn get_dynamic_tail(&self) -> &str {
        &self.dynamic_content
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolved_prompt_full() {
        let resolved = ResolvedPrompt {
            static_content: "static".to_string(),
            dynamic_content: "dynamic".to_string(),
            sections_used: vec!["test".to_string()],
            prefix_hash: "abc".to_string(),
            static_hash: "def".to_string(),
        };

        let full = resolved.full_prompt();
        assert!(full.contains("static"));
        assert!(full.contains(crate::prompt::SYSTEM_PROMPT_DYNAMIC_BOUNDARY));
        assert!(full.contains("dynamic"));
    }

    #[test]
    fn test_resolved_prompt_no_dynamic() {
        let resolved = ResolvedPrompt {
            static_content: "static only".to_string(),
            dynamic_content: String::new(),
            sections_used: vec![],
            prefix_hash: "abc".to_string(),
            static_hash: "def".to_string(),
        };

        assert_eq!(resolved.full_prompt(), "static only");
    }

    #[test]
    fn test_resolved_prompt_get_static_prefix() {
        let resolved = ResolvedPrompt {
            static_content: "static-part".to_string(),
            dynamic_content: "dynamic-part".to_string(),
            sections_used: vec![],
            prefix_hash: "a".to_string(),
            static_hash: "b".to_string(),
        };
        assert_eq!(resolved.get_static_prefix(), "static-part");
    }

    #[test]
    fn test_resolved_prompt_get_dynamic_tail() {
        let resolved = ResolvedPrompt {
            static_content: "static-part".to_string(),
            dynamic_content: "dynamic-part".to_string(),
            sections_used: vec![],
            prefix_hash: "a".to_string(),
            static_hash: "b".to_string(),
        };
        assert_eq!(resolved.get_dynamic_tail(), "dynamic-part");
    }

    #[test]
    fn test_resolved_prompt_get_dynamic_tail_empty() {
        let resolved = ResolvedPrompt {
            static_content: "static-part".to_string(),
            dynamic_content: String::new(),
            sections_used: vec![],
            prefix_hash: "a".to_string(),
            static_hash: "b".to_string(),
        };
        assert_eq!(resolved.get_dynamic_tail(), "");
    }

    #[test]
    fn test_resolved_prompt_full_prompt_with_boundary() {
        let resolved = ResolvedPrompt {
            static_content: "static".to_string(),
            dynamic_content: "dynamic".to_string(),
            sections_used: vec![],
            prefix_hash: "a".to_string(),
            static_hash: "b".to_string(),
        };
        let full = resolved.full_prompt();
        assert!(full.contains("static"));
        assert!(full.contains("dynamic"));
        assert!(full.contains(crate::prompt::SYSTEM_PROMPT_DYNAMIC_BOUNDARY));
    }
}
