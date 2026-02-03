use std::{
    process::Command,
    time::{Duration, Instant},
};

use parking_lot::Mutex;

static SYSTEM_CONTEXT_CACHE: Mutex<Option<(SystemContext, Instant)>> =
    Mutex::new(None);

const SYSTEM_CONTEXT_TTL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
pub struct SystemContext {
    pub git_branch: Option<String>,
    pub git_status: Option<String>,
    pub cache_breaker: String,
    pub beta_headers: Vec<String>,
}

impl SystemContext {
    pub fn new(cache_breaker: String) -> Self {
        Self {
            git_branch: None,
            git_status: None,
            cache_breaker,
            beta_headers: Vec::new(),
        }
    }

    pub fn git_branch(&self) -> Option<&str> {
        self.git_branch.as_deref()
    }

    pub fn git_status(&self) -> Option<&str> {
        self.git_status.as_deref()
    }
}

pub fn get_system_context() -> SystemContext {
    let mut cache = SYSTEM_CONTEXT_CACHE.lock();

    if let Some((context, cached_at)) = cache.as_ref()
        && cached_at.elapsed() < SYSTEM_CONTEXT_TTL
    {
        return context.clone();
    }

    let mut context = SystemContext::new(generate_cache_breaker());

    if let Ok(branch) = get_git_branch() {
        context.git_branch = Some(branch);
        context.git_status = get_git_status();
        *cache = Some((context.clone(), Instant::now()));
        return context;
    }

    *cache = Some((context.clone(), Instant::now()));
    context
}

pub fn clear_system_context_cache() {
    let mut cache = SYSTEM_CONTEXT_CACHE.lock();
    *cache = None;
}

fn generate_cache_breaker() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let value: u32 = rng.r#gen();
    format!("cb_{value:08x}")
}

fn get_git_branch() -> Result<String, std::io::Error> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()?;

    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(branch)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "not a git repository",
        ))
    }
}

fn get_git_status() -> Option<String> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let status = String::from_utf8_lossy(&output.stdout);
    if status.trim().is_empty() {
        Some("clean".to_string())
    } else {
        let lines: Vec<&str> = status.lines().take(3).collect();
        let summary = if lines.len() > 3 {
            format!("{} + {} more", lines.join("; "), lines.len() - 3)
        } else {
            lines.join("; ")
        };
        Some(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_context_new() {
        let ctx = SystemContext::new("test_breaker".to_string());
        assert_eq!(ctx.cache_breaker, "test_breaker");
        assert!(ctx.git_branch.is_none());
        assert!(ctx.git_status.is_none());
        assert!(ctx.beta_headers.is_empty());
    }

    #[test]
    fn test_cache_breaker_format() {
        let breaker = generate_cache_breaker();
        assert!(breaker.starts_with("cb_"));
        assert_eq!(breaker.len(), 11);
    }

    #[test]
    fn test_system_context_git_accessors() {
        let mut ctx = SystemContext::new("test".to_string());
        ctx.git_branch = Some("main".to_string());
        ctx.git_status = Some("clean".to_string());

        assert_eq!(ctx.git_branch(), Some("main"));
        assert_eq!(ctx.git_status(), Some("clean"));
    }

    #[test]
    fn test_clear_cache() {
        clear_system_context_cache();
        let cache = SYSTEM_CONTEXT_CACHE.lock();
        assert!(cache.is_none());
    }
}
