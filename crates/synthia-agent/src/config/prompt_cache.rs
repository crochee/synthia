use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use parking_lot::Mutex;

static PROMPT_CACHE_1H_CACHE: Mutex<Option<PromptCache1hCache>> =
    Mutex::new(None);

const PROMPT_CACHE_1H_TTL: Duration = Duration::from_secs(3600);

#[derive(Clone)]
pub struct PromptCache1hConfig {
    pub ttl: Duration,
    pub allowlist: Vec<String>,
    pub enabled: bool,
}

impl Default for PromptCache1hConfig {
    fn default() -> Self {
        Self {
            ttl: Duration::from_secs(3600),
            allowlist: Vec::new(),
            enabled: false,
        }
    }
}

#[derive(Debug)]
pub struct PromptCache1hCache {
    pub allowed_sessions: HashSet<String>,
    pub cached_at: Instant,
}

impl PromptCache1hCache {
    pub fn new() -> Self {
        Self {
            allowed_sessions: HashSet::new(),
            cached_at: Instant::now(),
        }
    }

    pub fn is_stale(&self) -> bool {
        self.cached_at.elapsed() > PROMPT_CACHE_1H_TTL
    }
}

impl Default for PromptCache1hCache {
    fn default() -> Self {
        Self::new()
    }
}

pub fn get_prompt_cache_1h_allowlist() -> Vec<String> {
    let cache = PROMPT_CACHE_1H_CACHE.lock();

    if let Some(ref cache) = *cache
        && !cache.is_stale()
    {
        return cache.allowed_sessions.iter().cloned().collect();
    }
    Vec::new()
}

pub fn is_session_eligible_for_prompt_cache_1h(session_id: &str) -> bool {
    let cache = PROMPT_CACHE_1H_CACHE.lock();

    if let Some(ref cache) = *cache {
        return !cache.is_stale()
            && cache.allowed_sessions.contains(session_id);
    }
    false
}

pub fn add_session_to_prompt_cache_1h(session_id: &str) {
    let mut cache = PROMPT_CACHE_1H_CACHE.lock();

    if let Some(ref mut cache) = *cache {
        if cache.is_stale() {
            *cache = PromptCache1hCache::new();
        }
        cache.allowed_sessions.insert(session_id.to_string());
    } else {
        let mut new_cache = PromptCache1hCache::new();
        new_cache.allowed_sessions.insert(session_id.to_string());
        *cache = Some(new_cache);
    }
}

pub fn clear_prompt_cache_1h() {
    let mut cache = PROMPT_CACHE_1H_CACHE.lock();
    *cache = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_cache_1h_config_default() {
        let config = PromptCache1hConfig::default();
        assert_eq!(config.ttl, Duration::from_secs(3600));
        assert!(config.allowlist.is_empty());
        assert!(!config.enabled);
    }

    #[test]
    fn test_prompt_cache_1h_cache_is_stale() {
        let cache = PromptCache1hCache::new();
        assert!(!cache.is_stale());
    }

    #[test]
    fn test_clear_prompt_cache_1h() {
        add_session_to_prompt_cache_1h("test-session");
        clear_prompt_cache_1h();
        assert!(!is_session_eligible_for_prompt_cache_1h("test-session"));
    }
}
