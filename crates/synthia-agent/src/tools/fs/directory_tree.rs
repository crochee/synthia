//! Directory tree tool implementation
//!
//! Get a recursive tree view of files and directories as a JSON structure.

use std::path::PathBuf;

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{AgentError, tools::Tool};

const DEFAULT_MAX_DEPTH: usize = 5;
const DEFAULT_MAX_ENTRIES: usize = 500;
const MAX_OUTPUT_CHARS: usize = 50_000;

const DEFAULT_EXCLUDE_PATTERNS: &[&str] = &[
    "node_modules",
    ".git",
    ".svn",
    ".hg",
    "target",
    "build",
    "dist",
    ".idea",
    ".vscode",
    ".vs",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".tox",
    "*.pyc",
    "*.pyo",
    "*.pyd",
    ".DS_Store",
    "Thumbs.db",
    "*.log",
    "*.tmp",
    "*.temp",
    ".env",
    ".venv",
    "venv",
    "env",
    ".direnv",
    "vendor",
    "Cargo.lock",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
];

#[derive(Debug, Clone, Deserialize)]
struct DirectoryTreeRequest {
    path: String,
    #[serde(default)]
    exclude_patterns: Vec<String>,
    #[serde(default = "default_max_depth")]
    max_depth: usize,
    #[serde(default = "default_max_entries")]
    max_entries: usize,
}

fn default_max_depth() -> usize {
    DEFAULT_MAX_DEPTH
}

fn default_max_entries() -> usize {
    DEFAULT_MAX_ENTRIES
}

#[derive(Debug, Clone, Serialize)]
struct TreeEntry {
    name: String,
    #[serde(rename = "type")]
    entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    children: Option<Vec<TreeEntry>>,
}

#[derive(Debug, Clone, Serialize)]
struct DirectoryTreeResult {
    root: String,
    entries: Vec<TreeEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stats: Option<TreeStats>,
}

#[derive(Debug, Clone, Serialize)]
struct TreeStats {
    total_files: usize,
    total_dirs: usize,
    truncated: bool,
    truncation_reason: Option<String>,
    max_depth_reached: bool,
    max_entries_reached: bool,
}

struct TreeBuildContext {
    exclude_patterns: Vec<String>,
    max_depth: usize,
    max_entries: usize,
    current_entries: std::sync::atomic::AtomicUsize,
    truncated: std::sync::atomic::AtomicBool,
    truncation_reason: std::sync::atomic::AtomicUsize,
    total_files: std::sync::atomic::AtomicUsize,
    total_dirs: std::sync::atomic::AtomicUsize,
}

impl TreeBuildContext {
    fn new(
        exclude_patterns: Vec<String>,
        max_depth: usize,
        max_entries: usize,
    ) -> Self {
        Self {
            exclude_patterns,
            max_depth,
            max_entries,
            current_entries: std::sync::atomic::AtomicUsize::new(0),
            truncated: std::sync::atomic::AtomicBool::new(false),
            truncation_reason: std::sync::atomic::AtomicUsize::new(0),
            total_files: std::sync::atomic::AtomicUsize::new(0),
            total_dirs: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn should_stop(&self) -> bool {
        self.current_entries
            .load(std::sync::atomic::Ordering::Relaxed)
            >= self.max_entries
    }

    fn increment_entries(&self) -> bool {
        let current = self
            .current_entries
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        current < self.max_entries
    }

    fn set_truncated(&self, reason: usize) {
        self.truncated
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.truncation_reason
            .store(reason, std::sync::atomic::Ordering::Relaxed);
    }
}

pub struct DirectoryTreeTool;

impl DirectoryTreeTool {
    pub fn new() -> Self {
        Self
    }

    fn should_exclude(
        name: &str,
        relative_path: &str,
        patterns: &[String],
    ) -> bool {
        for pattern in patterns {
            if pattern.contains('*')
                && let Ok(glob) = glob::Pattern::new(pattern)
                && (glob.matches(name) || glob.matches(relative_path))
            {
                return true;
            } else if name == pattern || relative_path == pattern {
                return true;
            }
        }
        false
    }

    async fn build_tree(
        current_path: &PathBuf,
        root_path: &PathBuf,
        current_depth: usize,
        ctx: &TreeBuildContext,
    ) -> Result<Vec<TreeEntry>, AgentError> {
        if ctx.should_stop() {
            ctx.set_truncated(1);
            return Ok(Vec::new());
        }

        if current_depth > ctx.max_depth {
            ctx.set_truncated(0);
            return Ok(Vec::new());
        }

        let mut entries = match tokio::fs::read_dir(current_path).await {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(
                    "Failed to read directory {}: {}",
                    current_path.display(),
                    e
                );
                return Ok(Vec::new());
            }
        };

        let mut result: Vec<TreeEntry> = Vec::new();

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            AgentError::InvalidOperation(format!("Failed to read entry: {e}"))
        })? {
            if ctx.should_stop() {
                ctx.set_truncated(1);
                break;
            }

            let name = entry.file_name().to_string_lossy().to_string();
            let entry_path = entry.path();
            let relative_path = entry_path
                .strip_prefix(root_path)
                .unwrap_or(&entry_path)
                .to_string_lossy()
                .to_string();

            if Self::should_exclude(
                &name,
                &relative_path,
                &ctx.exclude_patterns,
            ) {
                continue;
            }

            let file_type = match entry.file_type().await {
                Ok(ft) => ft,
                Err(e) => {
                    tracing::warn!(
                        "Failed to get file type for {}: {}",
                        entry_path.display(),
                        e
                    );
                    continue;
                }
            };

            if !ctx.increment_entries() {
                ctx.set_truncated(1);
                continue;
            }

            if file_type.is_dir() {
                ctx.total_dirs
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                let children = if current_depth < ctx.max_depth {
                    Some(
                        Box::pin(Self::build_tree(
                            &entry_path,
                            root_path,
                            current_depth + 1,
                            ctx,
                        ))
                        .await?,
                    )
                } else {
                    ctx.set_truncated(0);
                    None
                };

                result.push(TreeEntry {
                    name,
                    entry_type: "directory".to_string(),
                    children,
                });
            } else {
                ctx.total_files
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                result.push(TreeEntry {
                    name,
                    entry_type: "file".to_string(),
                    children: None,
                });
            }
        }

        result.sort_by(|a, b| {
            if a.entry_type != b.entry_type {
                if a.entry_type == "directory" {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            } else {
                a.name.cmp(&b.name)
            }
        });

        Ok(result)
    }
}

impl Default for DirectoryTreeTool {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for DirectoryTreeTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectoryTreeTool").finish()
    }
}

#[async_trait]
impl Tool for DirectoryTreeTool {
    fn name(&self) -> &str {
        "DirectoryTree"
    }

    fn description(&self) -> &str {
        "Recursive tree view as JSON. Excludes common dirs (node_modules, .git, target). Use max_depth/max_entries to limit."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The absolute path to the directory to get tree for. Must be an absolute path (e.g., '/home/user/project')."
                },
                "exclude_patterns": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Additional patterns to exclude from the tree (e.g., ['tests', 'docs', '*.md']). Default patterns are always applied.",
                    "default": []
                },
                "max_depth": {
                    "type": "integer",
                    "description": "Maximum recursion depth (default: 5, max: 10)",
                    "default": 5,
                    "minimum": 1,
                    "maximum": 10
                },
                "max_entries": {
                    "type": "integer",
                    "description": "Maximum number of entries to return (default: 500, max: 2000)",
                    "default": 500,
                    "minimum": 1,
                    "maximum": 2000
                }
            },
            "required": ["path"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: DirectoryTreeRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid arguments: {e}"
                ))]);
            }
        };

        let path = PathBuf::from(&request.path);
        if !path.is_absolute() {
            return CallToolResult::error(vec![Content::text(format!(
                "Path must be absolute. Received relative path: '{}'. Please provide an absolute path.",
                request.path
            ))]);
        }

        if !path.exists() {
            return CallToolResult::error(vec![Content::text(format!(
                "Directory does not exist: {}",
                path.display()
            ))]);
        }

        if !path.is_dir() {
            return CallToolResult::error(vec![Content::text(format!(
                "Path is not a directory: {}",
                path.display()
            ))]);
        }

        let max_depth = request.max_depth.clamp(1, 10);
        let max_entries = request.max_entries.clamp(1, 2000);

        let mut exclude_patterns: Vec<String> = DEFAULT_EXCLUDE_PATTERNS
            .iter()
            .map(ToString::to_string)
            .collect();
        exclude_patterns.extend(request.exclude_patterns);

        let ctx = TreeBuildContext::new(
            exclude_patterns.clone(),
            max_depth,
            max_entries,
        );
        let entries = match Self::build_tree(&path, &path, 0, &ctx).await {
            Ok(e) => e,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to build tree: {e}"
                ))]);
            }
        };

        let truncated =
            ctx.truncated.load(std::sync::atomic::Ordering::Relaxed);
        let truncation_reason = ctx
            .truncation_reason
            .load(std::sync::atomic::Ordering::Relaxed);
        let total_files =
            ctx.total_files.load(std::sync::atomic::Ordering::Relaxed);
        let total_dirs =
            ctx.total_dirs.load(std::sync::atomic::Ordering::Relaxed);

        let stats = TreeStats {
            total_files,
            total_dirs,
            truncated,
            truncation_reason: if truncated {
                Some(match truncation_reason {
                    0 => "max_depth reached".to_string(),
                    1 => "max_entries reached".to_string(),
                    _ => "unknown".to_string(),
                })
            } else {
                None
            },
            max_depth_reached: truncated && truncation_reason == 0,
            max_entries_reached: truncated && truncation_reason == 1,
        };

        let result = DirectoryTreeResult {
            root: path.display().to_string(),
            entries,
            stats: Some(stats),
        };

        let mut json = match serde_json::to_string_pretty(&result) {
            Ok(j) => j,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to serialize tree: {e}"
                ))]);
            }
        };

        if json.len() > MAX_OUTPUT_CHARS {
            let truncated_json =
                json.chars().take(MAX_OUTPUT_CHARS).collect::<String>();
            json = format!(
                "{}\n\n... [OUTPUT TRUNCATED - {} total characters, showing first {}]\n\nTo see more:\n- Use a smaller subdirectory\n- Add more exclude_patterns\n- Reduce max_depth or max_entries",
                truncated_json,
                json.len(),
                MAX_OUTPUT_CHARS
            );
        }

        CallToolResult::success(vec![Content::text(json)])
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_default_max_depth_constant() {
        assert_eq!(DEFAULT_MAX_DEPTH, 5);
    }

    #[test]
    fn test_default_max_entries_constant() {
        assert_eq!(DEFAULT_MAX_ENTRIES, 500);
    }

    #[test]
    fn test_max_output_chars_constant() {
        assert_eq!(MAX_OUTPUT_CHARS, 50_000);
    }

    #[test]
    fn test_default_exclude_patterns() {
        assert!(DEFAULT_EXCLUDE_PATTERNS.contains(&"node_modules"));
        assert!(DEFAULT_EXCLUDE_PATTERNS.contains(&".git"));
        assert!(DEFAULT_EXCLUDE_PATTERNS.contains(&"target"));
        assert!(DEFAULT_EXCLUDE_PATTERNS.contains(&".git"));
        assert!(DEFAULT_EXCLUDE_PATTERNS.contains(&"*.pyc"));
    }

    #[test]
    fn test_default_exclude_patterns_count() {
        assert!(DEFAULT_EXCLUDE_PATTERNS.len() > 20);
    }

    #[test]
    fn test_should_exclude_exact_match() {
        let patterns: Vec<String> =
            vec!["node_modules".to_string(), "*.log".to_string()];

        assert!(DirectoryTreeTool::should_exclude(
            "node_modules",
            "node_modules",
            &patterns
        ));
    }

    #[test]
    fn test_should_exclude_glob_match() {
        let patterns: Vec<String> =
            vec!["node_modules".to_string(), "*.log".to_string()];

        assert!(DirectoryTreeTool::should_exclude(
            "test.log",
            "logs/test.log",
            &patterns
        ));
    }

    #[test]
    fn test_should_exclude_no_match() {
        let patterns: Vec<String> =
            vec!["node_modules".to_string(), "*.log".to_string()];

        assert!(!DirectoryTreeTool::should_exclude("src", "src", &patterns));
        assert!(!DirectoryTreeTool::should_exclude(
            "test.txt", "test.txt", &patterns
        ));
    }

    #[test]
    fn test_should_exclude_relative_path_match() {
        let patterns: Vec<String> = vec!["tests".to_string()];

        assert!(DirectoryTreeTool::should_exclude(
            "tests",
            "src/tests",
            &patterns
        ));
    }

    #[test]
    fn test_should_exclude_empty_patterns() {
        let patterns: Vec<String> = vec![];
        assert!(!DirectoryTreeTool::should_exclude(
            "node_modules",
            "node_modules",
            &patterns
        ));
        assert!(!DirectoryTreeTool::should_exclude("src", "src", &patterns));
    }

    #[test]
    fn test_should_exclude_glob_only_matches_name_or_path() {
        let patterns: Vec<String> = vec!["*.tmp".to_string()];

        assert!(DirectoryTreeTool::should_exclude(
            "file.tmp", "file.tmp", &patterns
        ));
        assert!(DirectoryTreeTool::should_exclude(
            "file.tmp",
            "dir/file.tmp",
            &patterns
        ));
        assert!(!DirectoryTreeTool::should_exclude(
            "tmpdir",
            "tmpdir/file.rs",
            &patterns
        ));
    }

    #[test]
    fn test_tree_build_context_new() {
        let ctx =
            TreeBuildContext::new(vec!["node_modules".to_string()], 5, 100);

        assert!(!ctx.should_stop());
        assert_eq!(ctx.max_depth, 5);
        assert_eq!(ctx.max_entries, 100);
    }

    #[test]
    fn test_tree_build_context_should_stop() {
        let ctx = TreeBuildContext::new(vec![], 5, 2);

        assert!(!ctx.should_stop());
        ctx.increment_entries();
        assert!(!ctx.should_stop());
        ctx.increment_entries();
        assert!(ctx.should_stop());
    }

    #[test]
    fn test_tree_build_context_increment_entries() {
        let ctx = TreeBuildContext::new(vec![], 5, 5);

        // increment_entries returns true if count was < max before increment
        // With max=5, entries 0-4 return true, entry 5 returns false
        assert!(ctx.increment_entries()); // 0 < 5 -> true, count becomes 1
        assert!(ctx.increment_entries()); // 1 < 5 -> true, count becomes 2
        assert!(ctx.increment_entries()); // 2 < 5 -> true, count becomes 3
        assert!(ctx.increment_entries()); // 3 < 5 -> true, count becomes 4
        assert!(ctx.increment_entries()); // 4 < 5 -> true, count becomes 5
        assert!(!ctx.increment_entries()); // 5 < 5 is false -> false
    }

    #[test]
    fn test_tree_build_context_increment_entries_at_limit() {
        let ctx = TreeBuildContext::new(vec![], 5, 1);

        // With max=1, first increment succeeds, second fails
        assert!(ctx.increment_entries()); // 0 < 1 -> true, count becomes 1
        assert!(!ctx.increment_entries()); // 1 < 1 is false -> false
    }

    #[test]
    fn test_tree_build_context_set_truncated() {
        let ctx = TreeBuildContext::new(vec![], 5, 100);

        // set_truncated does NOT affect should_stop - only entry count does
        ctx.set_truncated(0);
        // should_stop still returns false because we haven't added entries
        assert!(!ctx.should_stop());
    }

    #[test]
    fn test_tree_entry_serialization() {
        let entry = TreeEntry {
            name: "test.rs".to_string(),
            entry_type: "file".to_string(),
            children: None,
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("test.rs"));
        assert!(json.contains("file"));
    }

    #[test]
    fn test_tree_entry_with_children_serialization() {
        let entry = TreeEntry {
            name: "src".to_string(),
            entry_type: "directory".to_string(),
            children: Some(vec![TreeEntry {
                name: "main.rs".to_string(),
                entry_type: "file".to_string(),
                children: None,
            }]),
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("src"));
        assert!(json.contains("directory"));
        assert!(json.contains("main.rs"));
    }

    #[test]
    fn test_tree_stats_serialization() {
        let stats = TreeStats {
            total_files: 10,
            total_dirs: 5,
            truncated: true,
            truncation_reason: Some("max_depth reached".to_string()),
            max_depth_reached: true,
            max_entries_reached: false,
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("10"));
        assert!(json.contains("5"));
        assert!(json.contains("max_depth reached"));
    }

    #[test]
    fn test_directory_tree_result_serialization() {
        let result = DirectoryTreeResult {
            root: "/test".to_string(),
            entries: vec![TreeEntry {
                name: "file.rs".to_string(),
                entry_type: "file".to_string(),
                children: None,
            }],
            stats: Some(TreeStats {
                total_files: 1,
                total_dirs: 0,
                truncated: false,
                truncation_reason: None,
                max_depth_reached: false,
                max_entries_reached: false,
            }),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("/test"));
        assert!(json.contains("file.rs"));
    }
}
