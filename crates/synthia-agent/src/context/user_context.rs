use std::{
    path::Path,
    time::{Duration, Instant},
};

use parking_lot::Mutex;

static USER_CONTEXT_CACHE: Mutex<Option<(UserContext, Instant)>> =
    Mutex::new(None);

const USER_CONTEXT_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct UserContext {
    pub claude_md_content: Option<String>,
    pub date: String,
    pub cwd: String,
}

impl UserContext {
    pub fn new(cwd: String, date: String) -> Self {
        Self {
            claude_md_content: None,
            date,
            cwd,
        }
    }

    pub fn claude_md_content(&self) -> Option<&str> {
        self.claude_md_content.as_deref()
    }
}

pub fn get_user_context(workspace_dir: &Path) -> UserContext {
    let mut cache = USER_CONTEXT_CACHE.lock();

    if let Some((context, cached_at)) = cache.as_ref()
        && cached_at.elapsed() < USER_CONTEXT_TTL
    {
        let cwd = workspace_dir.to_string_lossy().to_string();
        if context.cwd == cwd {
            return context.clone();
        }
    }

    let cwd = workspace_dir.to_string_lossy().to_string();
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut context = UserContext::new(cwd, date);

    if let Ok(content) = read_claude_md(workspace_dir) {
        context.claude_md_content = Some(content);
    }

    *cache = Some((context.clone(), Instant::now()));
    context
}

pub fn clear_user_context_cache() {
    let mut cache = USER_CONTEXT_CACHE.lock();
    *cache = None;
}

fn read_claude_md(workspace_dir: &Path) -> Result<String, std::io::Error> {
    let claude_md_path = workspace_dir.join("CLAUDE.md");
    if claude_md_path.exists() {
        std::fs::read_to_string(&claude_md_path)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "CLAUDE.md not found",
        ))
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_user_context_new() {
        let ctx =
            UserContext::new("/test".to_string(), "2024-01-01".to_string());
        assert_eq!(ctx.cwd, "/test");
        assert_eq!(ctx.date, "2024-01-01");
        assert!(ctx.claude_md_content.is_none());
    }

    #[test]
    fn test_claude_md_accessor() {
        let mut ctx =
            UserContext::new("/test".to_string(), "2024-01-01".to_string());
        ctx.claude_md_content = Some("test content".to_string());
        assert_eq!(ctx.claude_md_content(), Some("test content"));
    }

    #[test]
    fn test_read_claude_md_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let result = read_claude_md(temp_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_get_user_context_without_claude_md() {
        let temp_dir = TempDir::new().unwrap();
        let ctx = get_user_context(temp_dir.path());
        assert!(ctx.claude_md_content.is_none());
        assert_eq!(
            ctx.date,
            chrono::Local::now().format("%Y-%m-%d").to_string()
        );
    }

    #[test]
    fn test_get_user_context_with_claude_md() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(
            temp_dir.path().join("CLAUDE.md"),
            "# Test\n\nHello world",
        )
        .unwrap();
        let ctx = get_user_context(temp_dir.path());
        assert_eq!(ctx.claude_md_content(), Some("# Test\n\nHello world"));
    }
}
