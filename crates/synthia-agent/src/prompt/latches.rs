#[derive(Debug, Clone, Default)]
pub struct PromptLatches {
    afk_mode: bool,
    fast_mode: bool,
    cache_editing: bool,
    thinking_clear: bool,
    afk_mode_latched: bool,
    fast_mode_latched: bool,
    cache_editing_latched: bool,
    thinking_clear_latched: bool,
}

impl PromptLatches {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_afk_mode(&mut self, enabled: bool) {
        if enabled {
            self.afk_mode = true;
            self.afk_mode_latched = true;
        }
    }

    pub fn should_include_afk_header(&self) -> bool {
        self.afk_mode_latched
    }

    pub fn is_afk_mode_active(&self) -> bool {
        self.afk_mode
    }

    pub fn set_fast_mode(&mut self, enabled: bool) {
        if enabled {
            self.fast_mode = true;
            self.fast_mode_latched = true;
        }
    }

    pub fn should_include_fast_mode_header(&self) -> bool {
        self.fast_mode_latched
    }

    pub fn is_fast_mode_active(&self) -> bool {
        self.fast_mode
    }

    pub fn set_cache_editing(&mut self, enabled: bool) {
        if enabled {
            self.cache_editing = true;
            self.cache_editing_latched = true;
        }
    }

    pub fn should_include_cache_editing_header(&self) -> bool {
        self.cache_editing_latched
    }

    pub fn is_cache_editing_active(&self) -> bool {
        self.cache_editing
    }

    pub fn set_thinking_clear(&mut self, enabled: bool) {
        if enabled {
            self.thinking_clear = true;
            self.thinking_clear_latched = true;
        }
    }

    pub fn should_include_thinking_clear_header(&self) -> bool {
        self.thinking_clear_latched
    }

    pub fn is_thinking_clear_active(&self) -> bool {
        self.thinking_clear
    }

    pub fn is_latched(&self) -> bool {
        self.afk_mode_latched
            || self.fast_mode_latched
            || self.cache_editing_latched
            || self.thinking_clear_latched
    }

    pub fn clear_beta_header_latches(&mut self) {
        self.afk_mode_latched = false;
        self.fast_mode_latched = false;
        self.cache_editing_latched = false;
        self.thinking_clear_latched = false;
    }

    pub fn get_active_latch_names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.afk_mode_latched {
            names.push("afk_mode");
        }
        if self.fast_mode_latched {
            names.push("fast_mode");
        }
        if self.cache_editing_latched {
            names.push("cache_editing");
        }
        if self.thinking_clear_latched {
            names.push("thinking_clear");
        }
        names
    }

    pub fn generate_beta_headers(&self) -> String {
        let mut headers = Vec::new();

        if self.should_include_afk_header() {
            headers.push("AFK_MODE_BETA_HEADER=true");
        }
        if self.should_include_fast_mode_header() {
            headers.push("FAST_MODE_BETA_HEADER=true");
        }
        if self.should_include_cache_editing_header() {
            headers.push("CACHE_EDITING_BETA_HEADER=true");
        }
        if self.should_include_thinking_clear_header() {
            headers.push("THINKING_CLEAR_BETA_HEADER=true");
        }

        if headers.is_empty() {
            String::new()
        } else {
            format!("### Beta Headers\n{}\n", headers.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_afk_mode_latch() {
        let mut latches = PromptLatches::new();
        assert!(!latches.should_include_afk_header());
        latches.set_afk_mode(true);
        assert!(latches.should_include_afk_header());
        assert!(latches.is_afk_mode_active());
    }

    #[test]
    fn test_fast_mode_latch() {
        let mut latches = PromptLatches::new();
        assert!(!latches.should_include_fast_mode_header());
        latches.set_fast_mode(true);
        assert!(latches.should_include_fast_mode_header());
        assert!(latches.is_fast_mode_active());
    }

    #[test]
    fn test_cache_editing_latch() {
        let mut latches = PromptLatches::new();
        assert!(!latches.should_include_cache_editing_header());
        latches.set_cache_editing(true);
        assert!(latches.should_include_cache_editing_header());
        assert!(latches.is_cache_editing_active());
    }

    #[test]
    fn test_thinking_clear_latch() {
        let mut latches = PromptLatches::new();
        assert!(!latches.should_include_thinking_clear_header());
        latches.set_thinking_clear(true);
        assert!(latches.should_include_thinking_clear_header());
        assert!(latches.is_thinking_clear_active());
    }

    #[test]
    fn test_clear_beta_header_latches() {
        let mut latches = PromptLatches::new();
        latches.set_afk_mode(true);
        latches.set_fast_mode(true);
        assert!(latches.should_include_afk_header());
        assert!(latches.should_include_fast_mode_header());
        latches.clear_beta_header_latches();
        assert!(!latches.should_include_afk_header());
        assert!(!latches.should_include_fast_mode_header());
    }

    #[test]
    fn test_generate_beta_headers() {
        let mut latches = PromptLatches::new();
        let headers = latches.generate_beta_headers();
        assert!(headers.is_empty());

        latches.set_afk_mode(true);
        latches.set_fast_mode(true);
        let headers = latches.generate_beta_headers();
        assert!(headers.contains("AFK_MODE_BETA_HEADER=true"));
        assert!(headers.contains("FAST_MODE_BETA_HEADER=true"));
    }

    #[test]
    fn test_get_active_latch_names() {
        let mut latches = PromptLatches::new();
        let names = latches.get_active_latch_names();
        assert!(names.is_empty());

        latches.set_afk_mode(true);
        latches.set_cache_editing(true);
        let names = latches.get_active_latch_names();
        assert!(names.contains(&"afk_mode"));
        assert!(names.contains(&"cache_editing"));
        assert!(!names.contains(&"fast_mode"));
    }

    #[test]
    fn test_is_latched() {
        let mut latches = PromptLatches::new();
        assert!(!latches.is_latched());

        latches.set_afk_mode(true);
        assert!(latches.is_latched());
    }
}
