use std::collections::HashMap;

use crate::prompt::section_trait::SectionCaching;

#[derive(Clone, Debug, Default)]
pub struct PromptState {
    global_cache: HashMap<String, String>,
    session_cache: HashMap<String, String>,
}

impl PromptState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear_session(&mut self) {
        self.session_cache.clear();
    }

    pub fn clear_all(&mut self) {
        self.global_cache.clear();
        self.session_cache.clear();
    }

    pub fn invalidate(&mut self, name: &str) {
        self.session_cache.remove(name);
    }

    pub fn get(&self, name: &str, caching: SectionCaching) -> Option<String> {
        match caching {
            SectionCaching::Cached => self.global_cache.get(name).cloned(),
            SectionCaching::SessionCached | SectionCaching::Volatile => {
                self.session_cache.get(name).cloned()
            }
            SectionCaching::Uncached => None,
        }
    }

    pub fn insert(
        &mut self,
        name: String,
        value: String,
        caching: SectionCaching,
    ) {
        match caching {
            SectionCaching::Cached => {
                self.global_cache.insert(name, value);
            }
            SectionCaching::SessionCached | SectionCaching::Volatile => {
                self.session_cache.insert(name, value);
            }
            SectionCaching::Uncached => {}
        }
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            global_entries: self.global_cache.len() as u64,
            session_entries: self.session_cache.len() as u64,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub global_entries: u64,
    pub session_entries: u64,
}

#[cfg(test)]
mod tests {
    use super::{super::super::test_support::make_test_context, *};

    #[test]
    fn test_prompt_context_debug() {
        let ctx = make_test_context();
        let debug = format!("{ctx:?}");
        assert!(debug.contains("test"));
        assert!(debug.contains("/tmp"));
    }

    #[test]
    fn test_prompt_state_new() {
        let state = PromptState::new();
        assert_eq!(state.stats().global_entries, 0);
        assert_eq!(state.stats().session_entries, 0);
    }

    #[test]
    fn test_prompt_state_insert_and_get() {
        let mut state = PromptState::new();

        // Insert with session caching
        state.insert(
            "key1".to_string(),
            "value1".to_string(),
            SectionCaching::SessionCached,
        );
        assert_eq!(
            state.get("key1", SectionCaching::SessionCached),
            Some("value1".to_string())
        );
        assert_eq!(state.get("key1", SectionCaching::Cached), None);

        // Insert with global caching
        state.insert(
            "key2".to_string(),
            "value2".to_string(),
            SectionCaching::Cached,
        );
        assert_eq!(
            state.get("key2", SectionCaching::Cached),
            Some("value2".to_string())
        );
        assert_eq!(state.get("key2", SectionCaching::SessionCached), None);

        // Uncached returns none
        assert_eq!(state.get("key1", SectionCaching::Uncached), None);
    }

    #[test]
    fn test_prompt_state_clear_session() {
        let mut state = PromptState::new();

        state.insert(
            "key1".to_string(),
            "value1".to_string(),
            SectionCaching::SessionCached,
        );
        state.insert(
            "key2".to_string(),
            "value2".to_string(),
            SectionCaching::Cached,
        );

        state.clear_session();

        assert_eq!(state.get("key1", SectionCaching::SessionCached), None);
        assert_eq!(
            state.get("key2", SectionCaching::Cached),
            Some("value2".to_string())
        );
    }

    #[test]
    fn test_prompt_state_clear_all() {
        let mut state = PromptState::new();

        state.insert(
            "key1".to_string(),
            "value1".to_string(),
            SectionCaching::SessionCached,
        );
        state.insert(
            "key2".to_string(),
            "value2".to_string(),
            SectionCaching::Cached,
        );

        state.clear_all();

        assert_eq!(state.stats().global_entries, 0);
        assert_eq!(state.stats().session_entries, 0);
    }

    #[test]
    fn test_prompt_state_invalidate() {
        let mut state = PromptState::new();

        state.insert(
            "key1".to_string(),
            "value1".to_string(),
            SectionCaching::SessionCached,
        );
        state.insert(
            "key2".to_string(),
            "value2".to_string(),
            SectionCaching::SessionCached,
        );

        state.invalidate("key1");

        assert_eq!(state.get("key1", SectionCaching::SessionCached), None);
        assert_eq!(
            state.get("key2", SectionCaching::SessionCached),
            Some("value2".to_string())
        );
    }

    #[test]
    fn test_cache_stats_debug() {
        let stats = CacheStats {
            global_entries: 5,
            session_entries: 3,
        };
        let debug = format!("{stats:?}");
        assert!(debug.contains("5"));
        assert!(debug.contains("3"));
    }

    #[test]
    fn test_prompt_state_cache_volatile() {
        let mut state = PromptState::new();
        state.insert(
            "v1".to_string(),
            "vv1".to_string(),
            SectionCaching::Volatile,
        );
        assert_eq!(
            state.get("v1", SectionCaching::Volatile),
            Some("vv1".to_string())
        );
        assert_eq!(
            state.get("v1", SectionCaching::SessionCached),
            Some("vv1".to_string())
        );
    }

    #[test]
    fn test_prompt_state_cache_uncached() {
        let mut state = PromptState::new();
        state.insert(
            "u1".to_string(),
            "uu1".to_string(),
            SectionCaching::Uncached,
        );
        assert_eq!(state.get("u1", SectionCaching::Uncached), None);
    }

    #[test]
    fn test_prompt_state_insert_replaces() {
        let mut state = PromptState::new();
        state.insert("k".to_string(), "v1".to_string(), SectionCaching::Cached);
        state.insert("k".to_string(), "v2".to_string(), SectionCaching::Cached);
        assert_eq!(
            state.get("k", SectionCaching::Cached),
            Some("v2".to_string())
        );
    }

    #[test]
    fn test_prompt_state_stats() {
        let mut state = PromptState::new();
        assert_eq!(state.stats().global_entries, 0);
        assert_eq!(state.stats().session_entries, 0);

        state.insert(
            "k1".to_string(),
            "v1".to_string(),
            SectionCaching::Cached,
        );
        state.insert(
            "k2".to_string(),
            "v2".to_string(),
            SectionCaching::SessionCached,
        );
        state.insert(
            "k3".to_string(),
            "v3".to_string(),
            SectionCaching::Volatile,
        );

        assert_eq!(state.stats().global_entries, 1);
        assert_eq!(state.stats().session_entries, 2);
    }
}
