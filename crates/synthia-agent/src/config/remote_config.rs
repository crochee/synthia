use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::Value as JsonValue;

static REMOTE_CONFIG_CACHE: Mutex<Option<RemoteConfigCache>> = Mutex::new(None);

const DEFAULT_CONFIG_TTL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy)]
pub enum ConfigSource {
    Default,
    LocalStorage,
    RemoteGrowthBook,
}

#[derive(Debug, Clone)]
pub struct FeatureValue<T> {
    pub value: T,
    pub source: ConfigSource,
    pub cached_at: Instant,
}

impl<T> FeatureValue<T> {
    pub fn is_stale(&self, ttl: Duration) -> bool {
        self.cached_at.elapsed() > ttl
    }
}

#[derive(Debug, Clone)]
pub struct RemoteConfigCache {
    pub features: HashMap<String, JsonValue>,
    pub updated_at: Instant,
}

impl RemoteConfigCache {
    pub fn new(features: HashMap<String, JsonValue>) -> Self {
        Self {
            features,
            updated_at: Instant::now(),
        }
    }

    pub fn is_stale(&self) -> bool {
        self.updated_at.elapsed() > DEFAULT_CONFIG_TTL
    }
}

pub fn get_feature_value_cached<T: for<'de> Deserialize<'de>>(
    key: &str,
    default: T,
) -> T {
    let cache = REMOTE_CONFIG_CACHE.lock();

    if let Some(ref cache) = *cache
        && let Some(value) = cache.features.get(key)
        && let Ok(parsed) = serde_json::from_value(value.clone())
    {
        return parsed;
    }

    default
}

pub fn get_dynamic_config_cached<T: for<'de> Deserialize<'de> + Default>(
    key: &str,
) -> Partial<T> {
    let cache = REMOTE_CONFIG_CACHE.lock();

    if let Some(ref cache) = *cache
        && let Some(value) = cache.features.get(key)
        && let Ok(parsed) = serde_json::from_value(value.clone())
    {
        return Partial::new(parsed, true);
    }

    Partial::new(T::default(), false)
}

pub fn refresh_remote_config_async() {
    tokio::spawn(async {
        refresh_remote_config_internal().await;
    });
}

async fn refresh_remote_config_internal() {
    let Some(url) = std::env::var("SYNTHIA_GROWTHBOOK_URL").ok() else {
        return;
    };
    let Some(api_key) = std::env::var("SYNTHIA_GROWTHBOOK_API_KEY").ok() else {
        return;
    };

    let client = reqwest::Client::new();
    if let Ok(response) = client
        .get(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await
        && let Ok(features) =
            response.json::<HashMap<String, JsonValue>>().await
    {
        let cache = RemoteConfigCache::new(features);
        let mut global_cache = REMOTE_CONFIG_CACHE.lock();
        *global_cache = Some(cache);
    }
}

pub fn is_config_cache_stale() -> bool {
    let cache = REMOTE_CONFIG_CACHE.lock();
    if let Some(ref cache) = *cache {
        cache.is_stale()
    } else {
        true
    }
}

pub fn set_config_cache(cache: RemoteConfigCache) {
    let mut global_cache = REMOTE_CONFIG_CACHE.lock();
    *global_cache = Some(cache);
}

pub fn clear_config_cache() {
    let mut cache = REMOTE_CONFIG_CACHE.lock();
    *cache = None;
}

#[derive(Debug, Clone)]
pub struct Partial<T> {
    pub value: T,
    pub is_complete: bool,
}

impl<T> Partial<T> {
    pub fn new(value: T, is_complete: bool) -> Self {
        Self { value, is_complete }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_value_is_stale() {
        let value = FeatureValue {
            value: 42,
            source: ConfigSource::Default,
            cached_at: Instant::now() - Duration::from_secs(400),
        };
        assert!(value.is_stale(Duration::from_secs(300)));
    }

    #[test]
    fn test_feature_value_not_stale() {
        let value = FeatureValue {
            value: 42,
            source: ConfigSource::Default,
            cached_at: Instant::now() - Duration::from_secs(100),
        };
        assert!(!value.is_stale(Duration::from_secs(300)));
    }

    #[test]
    fn test_remote_config_cache_is_stale() {
        let cache = RemoteConfigCache::new(HashMap::new());
        assert!(!cache.is_stale());

        let stale_cache = RemoteConfigCache {
            features: HashMap::new(),
            updated_at: Instant::now() - Duration::from_secs(400),
        };
        assert!(stale_cache.is_stale());
    }

    #[test]
    fn test_get_feature_value_cached_default() {
        clear_config_cache();
        let value: i32 = get_feature_value_cached("nonexistent_key", 42);
        assert_eq!(value, 42);
    }

    #[test]
    fn test_get_feature_value_cached_with_value() {
        let mut features = HashMap::new();
        features.insert("test_key".to_string(), serde_json::json!(123));
        set_config_cache(RemoteConfigCache::new(features));

        let value: i32 = get_feature_value_cached("test_key", 42);
        assert_eq!(value, 123);
    }

    #[test]
    fn test_partial() {
        let partial = Partial::new(42, true);
        assert_eq!(partial.value, 42);
        assert!(partial.is_complete);

        let partial = Partial::new(String::new(), false);
        assert!(!partial.is_complete);
    }
}
