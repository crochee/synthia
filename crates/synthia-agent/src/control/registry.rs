use std::{
    collections::HashMap,
    sync::{
        Arc,
        RwLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

use crate::control::agent_path::AgentPath;

#[derive(Debug, Clone)]
pub struct AgentMetadata {
    pub path: AgentPath,
    pub nickname: String,
    pub started_at: Instant,
    pub thread_count: Arc<AtomicUsize>,
}

impl AgentMetadata {
    pub fn new(path: AgentPath, nickname: String) -> Self {
        Self {
            path,
            nickname,
            started_at: Instant::now(),
            thread_count: Arc::new(AtomicUsize::new(1)),
        }
    }

    pub fn thread_count(&self) -> usize {
        self.thread_count.load(Ordering::SeqCst)
    }
}

pub struct AgentRegistry {
    agents: RwLock<HashMap<AgentPath, AgentMetadata>>,
    nickname_pool: RwLock<Vec<String>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            nickname_pool: RwLock::new(vec![
                "worker".into(),
                "helper".into(),
                "analyzer".into(),
                "reviewer".into(),
                "debugger".into(),
                "executor".into(),
                "coordinator".into(),
                "assistant".into(),
            ]),
        }
    }

    pub fn register(
        &self,
        path: AgentPath,
        nickname: String,
    ) -> Option<AgentMetadata> {
        let mut agents = self.agents.write().unwrap();
        if agents.contains_key(&path) {
            return None;
        }
        let metadata = AgentMetadata::new(path.clone(), nickname);
        let result = metadata.clone();
        agents.insert(path, metadata);
        Some(result)
    }

    pub fn unregister(&self, path: &AgentPath) -> Option<AgentMetadata> {
        let mut agents = self.agents.write().unwrap();
        agents.remove(path)
    }

    pub fn get(&self, path: &AgentPath) -> Option<AgentMetadata> {
        let agents = self.agents.read().unwrap();
        agents.get(path).cloned()
    }

    pub fn list(&self, prefix: Option<&AgentPath>) -> Vec<AgentMetadata> {
        let agents = self.agents.read().unwrap();
        agents
            .values()
            .filter(|m| match prefix {
                Some(p) => m.path.as_str().starts_with(p.as_str()),
                None => true,
            })
            .cloned()
            .collect()
    }

    pub fn alloc_nickname(&self) -> String {
        let mut pool = self.nickname_pool.write().unwrap();
        if let Some(nick) = pool.pop() {
            return nick;
        }
        format!("agent-{}", pool.capacity())
    }

    pub fn release_nickname(&self, nickname: &str) {
        let mut pool = self.nickname_pool.write().unwrap();
        if !pool.iter().any(|n| n == nickname) {
            pool.push(nickname.to_string());
        }
    }

    pub fn len(&self) -> usize {
        self.agents.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.agents.read().unwrap().is_empty()
    }

    /// Ensure metadata exists for the given path.
    ///
    /// If the path already has metadata, returns a clone.
    /// Otherwise, creates new metadata with a nickname from the pool.
    pub fn ensure(&self, path: &AgentPath) -> AgentMetadata {
        // Check if already exists
        {
            let agents = self.agents.read().unwrap();
            if let Some(meta) = agents.get(path) {
                return meta.clone();
            }
        }
        // Create with placeholder nickname
        let metadata = AgentMetadata {
            path: path.clone(),
            nickname: String::new(),
            started_at: Instant::now(),
            thread_count: Arc::new(AtomicUsize::new(0)),
        };
        let result = metadata.clone();
        let mut agents = self.agents.write().unwrap();
        agents.insert(path.clone(), metadata);
        result
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_get() {
        let registry = AgentRegistry::new();
        let path = AgentPath::new("/root/worker").unwrap();
        let metadata = registry.register(path.clone(), "test".into()).unwrap();
        assert_eq!(metadata.nickname, "test");
        assert_eq!(metadata.path, path);
        assert_eq!(metadata.thread_count(), 1);
        let retrieved = registry.get(&path).unwrap();
        assert_eq!(retrieved.nickname, "test");
    }

    #[test]
    fn test_register_duplicate_returns_none() {
        let registry = AgentRegistry::new();
        let path = AgentPath::new("/root/worker").unwrap();
        assert!(registry.register(path.clone(), "first".into()).is_some());
        assert!(registry.register(path, "second".into()).is_none());
    }

    #[test]
    fn test_unregister() {
        let registry = AgentRegistry::new();
        let path = AgentPath::new("/root/worker").unwrap();
        registry.register(path.clone(), "test".into()).unwrap();
        let removed = registry.unregister(&path).unwrap();
        assert_eq!(removed.nickname, "test");
        assert!(registry.get(&path).is_none());
    }

    #[test]
    fn test_list_with_prefix() {
        let registry = AgentRegistry::new();
        registry
            .register(AgentPath::new("/root/a").unwrap(), "a".into())
            .unwrap();
        registry
            .register(AgentPath::new("/root/b").unwrap(), "b".into())
            .unwrap();
        registry
            .register(AgentPath::new("/root/sub/c").unwrap(), "c".into())
            .unwrap();

        assert_eq!(registry.list(None).len(), 3);

        let sub = registry.list(Some(&AgentPath::new("/root/sub").unwrap()));
        assert_eq!(sub.len(), 1);
        assert_eq!(sub[0].path.as_str(), "/root/sub/c");

        let root = registry.list(Some(&AgentPath::new("/root").unwrap()));
        assert_eq!(root.len(), 3);
    }

    #[test]
    fn test_nickname_pool_drains() {
        let registry = AgentRegistry::new();
        let mut taken = Vec::new();
        for _ in 0..8 {
            taken.push(registry.alloc_nickname());
        }
        assert_eq!(taken.len(), 8);
        for n in &taken {
            assert!(!n.is_empty());
        }
        // Pool exhausted -> fallback
        let extra = registry.alloc_nickname();
        assert!(extra.starts_with("agent-"));
    }

    #[test]
    fn test_release_nickname_returns_to_pool() {
        let registry = AgentRegistry::new();
        let nick = registry.alloc_nickname();
        // Drain the rest
        for _ in 0..7 {
            let _ = registry.alloc_nickname();
        }
        // Pool now empty (or contains only the fallback form)
        registry.release_nickname(&nick);
        let recycled = registry.alloc_nickname();
        assert_eq!(recycled, nick);
    }

    #[test]
    fn test_thread_count_shared() {
        let registry = AgentRegistry::new();
        let path = AgentPath::new("/root/worker").unwrap();
        let metadata = registry.register(path.clone(), "test".into()).unwrap();
        let counter = Arc::clone(&metadata.thread_count);
        counter.fetch_add(1, Ordering::SeqCst);
        counter.fetch_add(1, Ordering::SeqCst);
        let retrieved = registry.get(&path).unwrap();
        assert_eq!(retrieved.thread_count(), 3);
    }
}
