//! Agent guards module

use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use parking_lot::Mutex;

#[derive(Clone, Default, Debug)]
pub struct Guards {
    max_threads: Option<u32>,
    active_count: Arc<AtomicU32>,
    active_threads: Arc<Mutex<Vec<String>>>,
    next_id: Arc<AtomicU32>,
}

impl Guards {
    pub fn new(max_threads: Option<u32>) -> Self {
        Self {
            max_threads,
            active_count: Arc::new(AtomicU32::new(0)),
            active_threads: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn reserve(&self, name: &str) -> Result<Guard, String> {
        let current = self.active_count.load(Ordering::Relaxed);

        if let Some(max) = self.max_threads
            && current >= max
        {
            return Err(format!(
                "Maximum number of agents ({max}) reached, current active: {current}"
            ));
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let thread_id = format!("{name}-{id}");

        self.active_count.fetch_add(1, Ordering::Relaxed);
        self.active_threads.lock().push(thread_id.clone());

        Ok(Guard {
            active_count: Arc::clone(&self.active_count),
            active_threads: Arc::clone(&self.active_threads),
            thread_id,
        })
    }

    pub fn active_thread_count(&self) -> usize {
        self.active_count.load(Ordering::Relaxed) as usize
    }

    pub fn max_threads(&self) -> Option<u32> {
        self.max_threads
    }

    pub fn is_at_limit(&self) -> bool {
        self.max_threads
            .is_some_and(|max| self.active_count.load(Ordering::Relaxed) >= max)
    }
}

#[derive(Debug)]
pub struct Guard {
    active_count: Arc<AtomicU32>,
    active_threads: Arc<Mutex<Vec<String>>>,
    thread_id: String,
}

impl Guard {
    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.active_count.fetch_sub(1, Ordering::Relaxed);
        let mut threads = self.active_threads.lock();
        if let Some(pos) = threads.iter().position(|id| id == &self.thread_id) {
            threads.swap_remove(pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_basic() {
        let guards = Guards::new(Some(2));

        assert_eq!(guards.active_thread_count(), 0);
        assert!(!guards.is_at_limit());

        let guard1 = guards.reserve("test").unwrap();
        assert_eq!(guards.active_thread_count(), 1);
        assert!(!guards.is_at_limit());
        drop(guard1);

        assert_eq!(guards.active_thread_count(), 0);
    }

    #[test]
    fn test_guard_limit() {
        let guards = Guards::new(Some(2));

        let _guard1 = guards.reserve("test").unwrap();
        let _guard2 = guards.reserve("test").unwrap();
        assert!(guards.is_at_limit());

        let result = guards.reserve("test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Maximum"));
    }

    #[test]
    fn test_guard_unlimited() {
        let guards = Guards::new(None);
        let guards_list: Vec<_> = (0..100)
            .map(|i| guards.reserve(&format!("test{i}")).unwrap())
            .collect();

        assert_eq!(guards.active_thread_count(), 100);
        drop(guards_list);
        assert_eq!(guards.active_thread_count(), 0);
    }

    #[test]
    fn test_guard_unique_ids() {
        let guards = Guards::new(None);

        let guard1 = guards.reserve("worker").unwrap();
        let guard2 = guards.reserve("worker").unwrap();

        assert_ne!(guard1.thread_id(), guard2.thread_id());
        assert!(guard1.thread_id().starts_with("worker-"));
        assert!(guard2.thread_id().starts_with("worker-"));
    }

    #[test]
    fn test_guard_drop_releases() {
        let guards = Guards::new(Some(1));

        let guard1 = guards.reserve("test").unwrap();
        assert!(guards.is_at_limit());

        drop(guard1);
        assert!(!guards.is_at_limit());

        let _guard2 = guards.reserve("test").unwrap();
        assert!(guards.is_at_limit());
    }
}
