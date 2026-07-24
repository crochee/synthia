//! LRU message cache. The cache holds a JSON-serialised copy of
//! the most recently read messages and the access order needed to
//! evict the least-recently-used entry when the cap is hit.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::types::CachedMessages;
use crate::manager::SessionManager;

impl SessionManager {
    pub async fn load_messages_recent_cached<T>(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<T>>
    where
        T: for<'de> Deserialize<'de> + Serialize + Clone,
    {
        let cached = {
            let cache = self.message_cache.read().expect("RwLock poisoned");
            cache.get(session_id).cloned()
        };

        if let Some(cached) = cached
            && cached.messages.len() >= limit
        {
            let start = cached.messages.len().saturating_sub(limit);
            let result: Vec<T> = cached.messages[start..]
                .iter()
                .filter_map(|m| serde_json::from_value(m.clone()).ok())
                .collect();
            {
                let mut counter =
                    self.cache_access_counter.write().expect("RwLock poisoned");
                *counter += 1;
                let order = *counter;
                let mut c =
                    self.message_cache.write().expect("RwLock poisoned");
                if let Some(entry) = c.get_mut(session_id) {
                    entry.access_order = order;
                }
            }
            return Ok(result);
        }

        let user_id = self.user_id_for(session_id)?;
        let messages: Vec<T> = self.store.load_messages_recent(
            user_id.as_str(),
            session_id,
            limit,
        )?;
        let json_messages: Vec<serde_json::Value> = messages
            .iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap_or_default();

        {
            let mut cache =
                self.message_cache.write().expect("RwLock poisoned");
            if cache.len() >= super::types::MAX_CACHED_SESSIONS {
                let key_to_remove = cache
                    .iter()
                    .min_by_key(|(_, v)| v.access_order)
                    .map(|(k, _)| k.clone());
                if let Some(key) = key_to_remove {
                    cache.remove(&key);
                }
            }

            let mut counter =
                self.cache_access_counter.write().expect("RwLock poisoned");
            *counter += 1;
            cache.insert(
                session_id.to_string(),
                CachedMessages {
                    messages: json_messages,
                    access_order: *counter,
                },
            );
        }

        Ok(messages)
    }

    pub async fn load_messages_all_cached<T>(
        &self,
        session_id: &str,
    ) -> Result<Vec<T>>
    where
        T: for<'de> Deserialize<'de> + Serialize + Clone,
    {
        let cached = {
            let cache = self.message_cache.read().expect("RwLock poisoned");
            cache.get(session_id).cloned()
        };

        if let Some(cached) = cached {
            let order;
            {
                let mut counter =
                    self.cache_access_counter.write().expect("RwLock poisoned");
                *counter += 1;
                order = *counter;
            }
            {
                let mut cache =
                    self.message_cache.write().expect("RwLock poisoned");
                if let Some(entry) = cache.get_mut(session_id) {
                    entry.access_order = order;
                }
            }
            let result: Vec<T> = cached
                .messages
                .iter()
                .filter_map(|m| serde_json::from_value(m.clone()).ok())
                .collect();
            return Ok(result);
        }

        let user_id = self.user_id_for(session_id)?;
        let messages: Vec<T> =
            self.store.load_messages_all(user_id.as_str(), session_id)?;
        let json_messages: Vec<serde_json::Value> = messages
            .iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap_or_default();

        {
            let mut cache =
                self.message_cache.write().expect("RwLock poisoned");
            if cache.len() >= super::types::MAX_CACHED_SESSIONS {
                let key_to_remove = cache
                    .iter()
                    .min_by_key(|(_, v)| v.access_order)
                    .map(|(k, _)| k.clone());
                if let Some(key) = key_to_remove {
                    cache.remove(&key);
                }
            }

            let mut counter =
                self.cache_access_counter.write().expect("RwLock poisoned");
            *counter += 1;
            cache.insert(
                session_id.to_string(),
                CachedMessages {
                    messages: json_messages,
                    access_order: *counter,
                },
            );
        }

        Ok(messages)
    }

    pub async fn invalidate_cache(&self, session_id: &str) {
        let mut cache = self.message_cache.write().expect("RwLock poisoned");
        cache.remove(session_id);
    }
}
