use futures::Future;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::pin::Pin;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

impl From<std::io::Error> for ToolError {
    fn from(error: std::io::Error) -> Self {
        ToolError::IoError(error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

pub trait ToolTrait: Send + Sync {
    fn info(&self) -> ToolInfo;
    fn execute(&self, arguments: Value) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + Sync>>;
}

pub struct FileReadTool {
    base_path: PathBuf,
}

impl FileReadTool {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }
}

impl ToolTrait for FileReadTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "read_file".to_string(),
            description: "Read the contents of a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to read"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    fn execute(&self, arguments: Value) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + Sync>> {
        let base_path = self.base_path.clone();
        Box::pin(async move {
            let path = arguments
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArguments("Missing 'path' argument".to_string()))?;

            let full_path = base_path.join(path);

            match tokio::fs::read_to_string(&full_path).await {
                Ok(content) => Ok(serde_json::json!({
                    "success": true,
                    "content": content,
                    "path": path
                })),
                Err(e) => Err(ToolError::IoError(e.to_string())),
            }
        })
    }
}

pub struct FileWriteTool {
    base_path: PathBuf,
}

impl FileWriteTool {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }
}

impl ToolTrait for FileWriteTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "write_file".to_string(),
            description: "Write content to a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    fn execute(&self, arguments: Value) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + Sync>> {
        let base_path = self.base_path.clone();
        Box::pin(async move {
            let path = arguments
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArguments("Missing 'path' argument".to_string()))?;

            let content = arguments
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArguments("Missing 'content' argument".to_string()))?;

            let full_path = base_path.join(path);

            if let Some(parent) = full_path.parent() {
                if !parent.exists() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|e| ToolError::IoError(e.to_string()))?;
                }
            }

            match tokio::fs::write(&full_path, content).await {
                Ok(_) => Ok(serde_json::json!({
                    "success": true,
                    "path": path,
                    "message": "File written successfully"
                })),
                Err(e) => Err(ToolError::IoError(e.to_string())),
            }
        })
    }
}

pub struct ListDirTool {
    base_path: PathBuf,
}

impl ListDirTool {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }
}

impl ToolTrait for ListDirTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "list_dir".to_string(),
            description: "List directory contents".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the directory to list"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    fn execute(&self, arguments: Value) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + Sync>> {
        let base_path = self.base_path.clone();
        Box::pin(async move {
            let path = arguments
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArguments("Missing 'path' argument".to_string()))?;

            let full_path = base_path.join(path);

            match tokio::fs::read_dir(&full_path).await {
                Ok(mut entries) => {
                    let mut items = Vec::new();
                    while let Some(entry) = entries.next_entry().await.map_err(|e| ToolError::IoError(e.to_string()))? {
                        let metadata = entry.metadata().await.map_err(|e| ToolError::IoError(e.to_string()))?;
                        items.push(serde_json::json!({
                            "name": entry.file_name().to_string_lossy().to_string(),
                            "is_dir": metadata.is_dir(),
                            "is_file": metadata.is_file(),
                            "size": metadata.len()
                        }));
                    }
                    Ok(serde_json::json!({
                        "success": true,
                        "path": path,
                        "items": items
                    }))
                }
                Err(e) => Err(ToolError::IoError(e.to_string())),
            }
        })
    }
}

pub struct GrepTool {
    base_path: PathBuf,
}

impl GrepTool {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    fn search_in_file(
        content: &str,
        pattern: &str,
        file_path: &PathBuf,
    ) -> Result<Vec<serde_json::Value>, std::io::Error> {
        let mut matches = Vec::new();
        for (line_no, line) in content.lines().enumerate() {
            if line.contains(pattern) {
                matches.push(serde_json::json!({
                    "file": file_path.to_string_lossy(),
                    "line": line_no + 1,
                    "content": line.trim()
                }));
            }
        }
        Ok(matches)
    }

    fn find_files(
        dir: &PathBuf,
        pattern: &str,
        results: &mut Vec<PathBuf>,
    ) -> Result<(), std::io::Error> {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && !path.to_string_lossy().starts_with(".") {
                    Self::find_files(&path, pattern, results)?;
                } else if path.is_file() {
                    if let Some(ext) = path.extension() {
                        let ext_str = ext.to_string_lossy().to_string();
                        if pattern == "*" || pattern == format!("*.{}", ext_str) {
                            results.push(path);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl ToolTrait for GrepTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "grep".to_string(),
            description: "Search for a pattern in files".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Path to search in (default: current directory)"
                    },
                    "file_pattern": {
                        "type": "string",
                        "description": "File pattern to match (e.g., *.rs)"
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    fn execute(&self, arguments: Value) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + Sync>> {
        let base_path = self.base_path.clone();
        Box::pin(async move {
            let pattern = arguments
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArguments("Missing 'pattern' argument".to_string()))?;

            let path = arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");

            let file_pattern = arguments
                .get("file_pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("*");

            let search_path = base_path.join(path);

            let mut results = Vec::new();

            let mut files: Vec<PathBuf> = Vec::new();
            GrepTool::find_files(&search_path, file_pattern, &mut files)?;

            for file in files {
                match tokio::fs::read_to_string(&file).await {
                    Ok(content) => {
                        let matches = GrepTool::search_in_file(&content, pattern, &file)?;
                        if !matches.is_empty() {
                            results.extend(matches);
                        }
                    }
                    Err(e) => {
                        results.push(serde_json::json!({
                            "error": format!("Failed to read {}: {}", file.to_string_lossy(), e)
                        }));
                    }
                }
            }

            Ok(serde_json::json!({
                "success": true,
                "pattern": pattern,
                "path": path,
                "results": results
            }))
        })
    }
}

pub struct RunCommandTool {
    base_path: PathBuf,
}

impl RunCommandTool {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }
}

impl ToolTrait for RunCommandTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "run_command".to_string(),
            description: "Run a shell command".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Command to run"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    fn execute(&self, arguments: Value) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + Sync>> {
        let base_path = self.base_path.clone();
        Box::pin(async move {
            let command = arguments
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArguments("Missing 'command' argument".to_string()))?;

            let output = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(&base_path)
                .output()
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            Ok(serde_json::json!({
                "success": output.status.success(),
                "command": command,
                "stdout": stdout,
                "stderr": stderr,
                "exit_code": output.status.code()
            }))
        })
    }
}

pub struct GlobTool {
    base_path: PathBuf,
}

impl GlobTool {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }
}

impl ToolTrait for GlobTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "glob".to_string(),
            description: "Find files matching a pattern".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern (e.g., **/*.rs)"
                    },
                    "path": {
                        "type": "string",
                        "description": "Base path to search from"
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    fn execute(&self, arguments: Value) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + Sync>> {
        let base_path = self.base_path.clone();
        Box::pin(async move {
            let pattern = arguments
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidArguments("Missing 'pattern' argument".to_string()))?;

            let path = arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");

            let search_path = base_path.join(path);

            let mut results = Vec::new();

            fn walk_dir(dir: &PathBuf, pattern: &str, results: &mut Vec<String>) -> Result<(), std::io::Error> {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                        if matches_wildcard(file_name, pattern) {
                            results.push(path.to_string_lossy().replace("\\", "/"));
                        }

                        if path.is_dir() && !file_name.starts_with(".") {
                            walk_dir(&path, pattern, results)?;
                        }
                    }
                }
                Ok(())
            }

            fn matches_wildcard(name: &str, pattern: &str) -> bool {
                if pattern.contains("**/") || pattern.starts_with("**") {
                    let suffix = pattern
                        .trim_start_matches("**/")
                        .trim_start_matches("**");
                    if suffix.contains('/') {
                        name == suffix.split('/').next().unwrap_or(suffix)
                            || name.ends_with(suffix.trim_start_matches('*'))
                    } else {
                        wildcard_match(name, suffix)
                    }
                } else {
                    wildcard_match(name, pattern)
                }
            }

            fn wildcard_match(name: &str, pattern: &str) -> bool {
                let name_bytes = name.as_bytes();
                let pattern_bytes = pattern.as_bytes();
                let n = name_bytes.len();
                let m = pattern_bytes.len();
                let mut dp = vec![vec![false; m + 1]; n + 1];
                dp[0][0] = true;
                for j in 1..=m {
                    if pattern_bytes[j - 1] == b'*' {
                        dp[0][j] = dp[0][j - 1];
                    }
                }
                for i in 1..=n {
                    for j in 1..=m {
                        if pattern_bytes[j - 1] == b'*' {
                            dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
                        } else if pattern_bytes[j - 1] == b'?' || pattern_bytes[j - 1] == name_bytes[i - 1] {
                            dp[i][j] = dp[i - 1][j - 1];
                        } else {
                            dp[i][j] = false;
                        }
                    }
                }
                dp[n][m]
            }

            walk_dir(&search_path, pattern, &mut results)
                .map_err(|e| ToolError::IoError(e.to_string()))?;

            Ok(serde_json::json!({
                "success": true,
                "pattern": pattern,
                "path": path,
                "files": results
            }))
        })
    }
}

pub struct ToolManager {
    tools: std::collections::HashMap<String, Box<dyn ToolTrait>>,
}

impl ToolManager {
    pub fn new() -> Self {
        Self {
            tools: std::collections::HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Box<dyn ToolTrait>) {
        self.tools.insert(tool.info().name.clone(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn ToolTrait> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    pub fn list(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    pub fn get_definitions(&self) -> Vec<crate::clients::ToolDefinition> {
        self.tools
            .values()
            .map(|tool| {
                let info = tool.info();
                crate::clients::ToolDefinition {
                    name: info.name,
                    description: info.description,
                    parameters: info.parameters,
                }
            })
            .collect()
    }
}

pub fn default_tools(base_path: PathBuf) -> ToolManager {
    let mut manager = ToolManager::new();

    manager.register(Box::new(FileReadTool::new(base_path.clone())));
    manager.register(Box::new(FileWriteTool::new(base_path.clone())));
    manager.register(Box::new(ListDirTool::new(base_path.clone())));
    manager.register(Box::new(GrepTool::new(base_path.clone())));
    manager.register(Box::new(RunCommandTool::new(base_path.clone())));
    manager.register(Box::new(GlobTool::new(base_path.clone())));

    manager
}
