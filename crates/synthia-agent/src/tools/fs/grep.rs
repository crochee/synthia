//! Grep tool implementation
//!
//! A powerful search tool built on ripgrep.
//! Also supports file listing (glob) functionality.

use std::{path::Path, time::Duration};

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;
use tokio::{process::Command, time::timeout};

use crate::{AgentError, tools::Tool};

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 2000;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
struct GrepRequest {
    #[serde(default)]
    pattern: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    output_mode: Option<String>,
    #[serde(default)]
    case_insensitive: Option<bool>,
    #[serde(default)]
    line_numbers: Option<bool>,
    #[serde(default)]
    after_context: Option<usize>,
    #[serde(default)]
    before_context: Option<usize>,
    #[serde(default)]
    context: Option<usize>,
    #[serde(default)]
    glob: Option<String>,
    #[serde(rename = "type")]
    #[serde(default)]
    file_type: Option<String>,
    #[serde(default)]
    head_limit: Option<usize>,

    #[serde(default)]
    directory: Option<String>,
    #[serde(default = "default_true")]
    include_dirs: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum OutputMode {
    Content,
    #[default]
    FilesWithMatches,
    Count,
    FilesOnly,
}

impl std::str::FromStr for OutputMode {
    type Err = AgentError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "content" => Ok(Self::Content),
            "files_with_matches" => Ok(Self::FilesWithMatches),
            "count" => Ok(Self::Count),
            "files_only" => Ok(Self::FilesOnly),
            _ => Err(AgentError::InvalidOperation(format!(
                "Invalid output mode: {s}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GrepTool;

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GrepTool {
    pub fn new() -> Self {
        Self
    }

    fn get_file_type_glob(file_type: &str) -> Option<&'static str> {
        match file_type {
            "js" | "javascript" => Some("*.js"),
            "ts" | "typescript" => Some("*.ts"),
            "tsx" => Some("*.tsx"),
            "jsx" => Some("*.jsx"),
            "py" | "python" => Some("*.py"),
            "rs" | "rust" => Some("*.rs"),
            "go" => Some("*.go"),
            "java" => Some("*.java"),
            "c" => Some("*.c"),
            "cpp" | "cxx" => Some("*.cpp"),
            "h" => Some("*.h"),
            "hpp" => Some("*.hpp"),
            "rb" | "ruby" => Some("*.rb"),
            "php" => Some("*.php"),
            "swift" => Some("*.swift"),
            "kt" | "kotlin" => Some("*.kt"),
            "scala" => Some("*.scala"),
            "lua" => Some("*.lua"),
            "sh" | "shell" | "bash" => Some("*.sh"),
            "json" => Some("*.json"),
            "yaml" | "yml" => Some("*.yaml"),
            "xml" => Some("*.xml"),
            "html" => Some("*.html"),
            "css" => Some("*.css"),
            "scss" => Some("*.scss"),
            "md" | "markdown" => Some("*.md"),
            "toml" => Some("*.toml"),
            "sql" => Some("*.sql"),
            _ => None,
        }
    }

    fn validate_glob_pattern(pattern: &str) -> Result<(), AgentError> {
        if pattern.starts_with("**") {
            return Err(AgentError::InvalidOperation(
                "Pattern cannot start with '**' as it would recursively search all directories. \
                 Use more specific patterns instead."
                    .to_string(),
            ));
        }

        if pattern.contains("..") {
            return Err(AgentError::InvalidOperation(
                "Pattern cannot contain '..' for security reasons.".to_string(),
            ));
        }

        Ok(())
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "Grep"
    }

    fn description(&self) -> &str {
        "Search file contents using regex."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern"
                },
                "path": {
                    "type": "string",
                    "description": "Search path"
                },
                "directory": {
                    "type": "string",
                    "description": "Directory (alt to path)"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count", "files_only"],
                    "description": "Output mode"
                },
                "case_insensitive": {"type": "boolean", "description": "Case insensitive"},
                "line_numbers": {"type": "boolean", "description": "Show line numbers"},
                "after_context": {"type": "integer", "description": "After context"},
                "before_context": {"type": "integer", "description": "Before context"},
                "context": {"type": "integer", "description": "Context"},
                "glob": {"type": "string", "description": "Glob pattern"},
                "type": {"type": "string", "description": "File type"},
                "head_limit": {"type": "integer", "description": "Max results"},
                "include_dirs": {"type": "boolean", "description": "Include dirs", "default": true}
            },
            "required": []
        })
    }

    fn is_concurrency_safe(&self, _args: &Value) -> bool {
        true
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: GrepRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid arguments: {e}"
                ))]);
            }
        };

        let search_path_str = if !request.path.is_empty() {
            request.path.clone()
        } else if let Some(dir) = &request.directory {
            dir.clone()
        } else {
            return CallToolResult::error(vec![Content::text(
                "Either 'path' or 'directory' must be provided".to_string(),
            )]);
        };

        let search_path = Path::new(&search_path_str);
        if !search_path.is_absolute() {
            return CallToolResult::error(vec![Content::text(format!(
                "Path must be absolute. Received relative path: '{search_path_str}'. Please provide an absolute path."
            ))]);
        }

        if !search_path.exists() {
            return CallToolResult::error(vec![Content::text(format!(
                "Path does not exist: {}",
                search_path.display()
            ))]);
        }

        let output_mode = match request.output_mode.as_deref().map(str::parse) {
            Some(Ok(m)) => m,
            Some(Err(e)) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid output mode: {e}"
                ))]);
            }
            None => OutputMode::FilesWithMatches,
        };

        if output_mode == OutputMode::FilesOnly {
            let glob_pattern = if let Some(glob) = &request.glob {
                glob.clone()
            } else if let Some(file_type) = &request.file_type {
                Self::get_file_type_glob(file_type)
                    .unwrap_or("*")
                    .to_string()
            } else {
                return CallToolResult::error(vec![Content::text(
                    "'glob' or 'type' parameter is required for files_only mode".to_string(),
                )]);
            };

            if let Err(e) = Self::validate_glob_pattern(&glob_pattern) {
                return CallToolResult::error(vec![Content::text(format!(
                    "{e}"
                ))]);
            }

            let head_limit =
                request.head_limit.unwrap_or(MAX_LIMIT).min(MAX_LIMIT);

            let search_results = match run_rg_files_only(
                &glob_pattern,
                search_path,
                request.include_dirs,
                head_limit,
            )
            .await
            {
                Ok(r) => r,
                Err(e) => {
                    return CallToolResult::error(vec![Content::text(
                        format!("Search failed: {e}"),
                    )]);
                }
            };

            let output = format_files_only_output(
                &glob_pattern,
                search_path,
                search_results,
            );

            return CallToolResult::success(vec![Content::text(output)]);
        }

        let pattern = request.pattern.trim();
        if pattern.is_empty() {
            return CallToolResult::error(vec![Content::text(
                "pattern must not be empty".to_string(),
            )]);
        }

        let case_insensitive = request.case_insensitive.unwrap_or(false);
        let line_numbers = request
            .line_numbers
            .unwrap_or(output_mode == OutputMode::Content);
        let after_context = request.after_context.unwrap_or(0);
        let before_context = request.before_context.unwrap_or(0);
        let context = request.context.unwrap_or(0);
        let head_limit =
            request.head_limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);

        let glob_pattern = request.glob.or_else(|| {
            request
                .file_type
                .as_deref()
                .and_then(Self::get_file_type_glob)
                .map(std::string::ToString::to_string)
        });

        let search_results = match run_rg_search(SearchParams {
            pattern,
            search_path,
            output_mode,
            case_insensitive,
            line_numbers,
            after_context,
            before_context,
            context,
            glob: glob_pattern.as_deref(),
            head_limit,
        })
        .await
        {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Search failed: {e}"
                ))]);
            }
        };

        let output =
            format_output(output_mode, &request.pattern, search_results);

        CallToolResult::success(vec![Content::text(output)])
    }
}

struct SearchParams<'a> {
    pattern: &'a str,
    search_path: &'a Path,
    output_mode: OutputMode,
    case_insensitive: bool,
    line_numbers: bool,
    after_context: usize,
    before_context: usize,
    context: usize,
    glob: Option<&'a str>,
    head_limit: usize,
}

async fn run_rg_search(
    params: SearchParams<'_>,
) -> Result<Vec<String>, AgentError> {
    let mut command = Command::new("rg");

    command
        .arg("--regexp")
        .arg(params.pattern)
        .arg("--no-messages");

    match params.output_mode {
        OutputMode::FilesWithMatches => {
            command.arg("--files-with-matches");
        }
        OutputMode::Count => {
            command.arg("--count");
        }
        OutputMode::Content => {}
        OutputMode::FilesOnly => {
            unreachable!("FilesOnly mode should be handled separately");
        }
    }

    if params.case_insensitive {
        command.arg("-i");
    }

    if params.line_numbers {
        command.arg("-n");
    }

    let ctx = params
        .context
        .max(params.after_context)
        .max(params.before_context);
    if ctx > 0 {
        command.arg("-C").arg(ctx.to_string());
    } else {
        if params.after_context > 0 {
            command.arg("-A").arg(params.after_context.to_string());
        }
        if params.before_context > 0 {
            command.arg("-B").arg(params.before_context.to_string());
        }
    }

    if let Some(g) = params.glob {
        command.arg("--glob").arg(g);
    }

    if params.output_mode == OutputMode::Content
        && params.head_limit < MAX_LIMIT
    {
        command.arg("-m").arg(params.head_limit.to_string());
    }

    command.arg("--").arg(params.search_path);

    let output = timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| AgentError::timeout("rg timed out after 30 seconds"))?
        .map_err(|err| {
            AgentError::tool(
                "grep",
                format!(
                    "failed to launch rg: {err}. Ensure ripgrep is installed and on PATH."
                ),
            )
        })?;

    match output.status.code() {
        Some(0) => {
            let results = parse_results(&output.stdout, params.head_limit);
            Ok(results)
        }
        Some(1) => Ok(Vec::new()),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(AgentError::tool("grep", format!("rg failed: {stderr}")))
        }
    }
}

async fn run_rg_files_only(
    glob_pattern: &str,
    search_path: &Path,
    include_dirs: bool,
    head_limit: usize,
) -> Result<Vec<String>, AgentError> {
    let mut command = Command::new("rg");
    command.arg("--files").arg("--no-messages");

    command.arg("--glob").arg(glob_pattern);

    command.arg("--").arg(search_path);

    let output = timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| AgentError::timeout("rg timed out after 30 seconds"))?
        .map_err(|err| {
            AgentError::tool(
                "grep",
                format!(
                    "failed to launch rg: {err}. Ensure ripgrep is installed and on PATH."
                ),
            )
        })?;

    let mut results = match output.status.code() {
        Some(0) => parse_results(&output.stdout, head_limit),
        Some(1) => Vec::new(),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AgentError::tool(
                "grep",
                format!("rg failed: {stderr}"),
            ));
        }
    };

    if include_dirs {
        let dir_results =
            find_directories(glob_pattern, search_path, head_limit).await?;
        results.extend(dir_results);
        results.sort();
        results.dedup();
        if results.len() > head_limit {
            results.truncate(head_limit);
        }
    }

    Ok(results)
}

async fn find_directories(
    _glob_pattern: &str,
    search_path: &Path,
    head_limit: usize,
) -> Result<Vec<String>, AgentError> {
    let mut command = Command::new("find");
    command
        .arg(search_path)
        .arg("-type")
        .arg("d")
        .arg("-not")
        .arg("-path")
        .arg("*/\\.*");

    let output = timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| AgentError::timeout("find timed out after 30 seconds"))?
        .map_err(|err| {
            AgentError::tool("grep", format!("failed to launch find: {err}"))
        })?;

    let mut results = Vec::new();

    if output.status.success() {
        for line in output.stdout.split(|byte| *byte == b'\n') {
            if line.is_empty() {
                continue;
            }
            if let Ok(text) = std::str::from_utf8(line) {
                let trimmed = text.trim_end();
                if trimmed.is_empty() {
                    continue;
                }

                if let Ok(rel_path) =
                    Path::new(trimmed).strip_prefix(search_path)
                {
                    let rel_str = rel_path.to_string_lossy().to_string();
                    if !rel_str.is_empty() {
                        results.push(rel_str);
                    } else {
                        results.push(".".to_string());
                    }
                }

                if results.len() >= head_limit {
                    break;
                }
            }
        }
    }

    Ok(results)
}

fn parse_results(stdout: &[u8], limit: usize) -> Vec<String> {
    let mut results = Vec::new();
    for line in stdout.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        if let Ok(text) = std::str::from_utf8(line) {
            let trimmed = text.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            results.push(trimmed.to_string());
            if results.len() == limit {
                break;
            }
        }
    }
    results
}

fn format_output(
    output_mode: OutputMode,
    pattern: &str,
    results: Vec<String>,
) -> String {
    match output_mode {
        OutputMode::FilesWithMatches => {
            if results.is_empty() {
                format!("No files found matching pattern '{pattern}'")
            } else {
                format!(
                    "Found {} file(s) with matches for '{pattern}':\n{}",
                    results.len(),
                    results.join("\n")
                )
            }
        }
        OutputMode::Content => {
            if results.is_empty() {
                format!("No matches found for pattern '{pattern}'")
            } else {
                format!(
                    "Found matches for '{pattern}':\n{}",
                    results.join("\n")
                )
            }
        }
        OutputMode::Count => {
            if results.is_empty() {
                format!("No matches found for pattern '{pattern}'")
            } else {
                format!("Match counts for '{pattern}':\n{}", results.join("\n"))
            }
        }
        OutputMode::FilesOnly => {
            unreachable!("FilesOnly mode should use format_files_only_output")
        }
    }
}

fn format_files_only_output(
    pattern: &str,
    search_path: &Path,
    results: Vec<String>,
) -> String {
    if results.is_empty() {
        format!("No matches found for pattern '{pattern}'")
    } else {
        format!(
            "Found {} match(es) for pattern '{}' in '{}':\n{}",
            results.len(),
            pattern,
            search_path.display(),
            results.join("\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_true() {
        assert!(default_true());
    }

    #[test]
    fn test_default_limit_constant() {
        assert_eq!(DEFAULT_LIMIT, 100);
    }

    #[test]
    fn test_max_limit_constant() {
        assert_eq!(MAX_LIMIT, 2000);
    }

    #[test]
    fn test_command_timeout_constant() {
        assert_eq!(COMMAND_TIMEOUT, Duration::from_secs(30));
    }

    #[test]
    fn test_output_mode_parsing() {
        assert_eq!(
            "content".parse::<OutputMode>().unwrap(),
            OutputMode::Content
        );
        assert_eq!(
            "files_with_matches".parse::<OutputMode>().unwrap(),
            OutputMode::FilesWithMatches
        );
        assert_eq!("count".parse::<OutputMode>().unwrap(), OutputMode::Count);
        assert_eq!(
            "files_only".parse::<OutputMode>().unwrap(),
            OutputMode::FilesOnly
        );
        assert_eq!(
            "FILES_WITH_MATCHES".parse::<OutputMode>().unwrap(),
            OutputMode::FilesWithMatches
        );
        assert!("invalid".parse::<OutputMode>().is_err());
    }

    #[test]
    fn test_output_mode_default() {
        let mode: OutputMode = Default::default();
        assert_eq!(mode, OutputMode::FilesWithMatches);
    }

    #[test]
    fn test_get_file_type_glob() {
        assert_eq!(GrepTool::get_file_type_glob("js"), Some("*.js"));
        assert_eq!(GrepTool::get_file_type_glob("javascript"), Some("*.js"));
        assert_eq!(GrepTool::get_file_type_glob("ts"), Some("*.ts"));
        assert_eq!(GrepTool::get_file_type_glob("typescript"), Some("*.ts"));
        assert_eq!(GrepTool::get_file_type_glob("tsx"), Some("*.tsx"));
        assert_eq!(GrepTool::get_file_type_glob("jsx"), Some("*.jsx"));
        assert_eq!(GrepTool::get_file_type_glob("py"), Some("*.py"));
        assert_eq!(GrepTool::get_file_type_glob("python"), Some("*.py"));
        assert_eq!(GrepTool::get_file_type_glob("rs"), Some("*.rs"));
        assert_eq!(GrepTool::get_file_type_glob("rust"), Some("*.rs"));
        assert_eq!(GrepTool::get_file_type_glob("go"), Some("*.go"));
        assert_eq!(GrepTool::get_file_type_glob("java"), Some("*.java"));
        assert_eq!(GrepTool::get_file_type_glob("c"), Some("*.c"));
        assert_eq!(GrepTool::get_file_type_glob("cpp"), Some("*.cpp"));
        assert_eq!(GrepTool::get_file_type_glob("cxx"), Some("*.cpp"));
        assert_eq!(GrepTool::get_file_type_glob("h"), Some("*.h"));
        assert_eq!(GrepTool::get_file_type_glob("hpp"), Some("*.hpp"));
        assert_eq!(GrepTool::get_file_type_glob("rb"), Some("*.rb"));
        assert_eq!(GrepTool::get_file_type_glob("ruby"), Some("*.rb"));
        assert_eq!(GrepTool::get_file_type_glob("php"), Some("*.php"));
        assert_eq!(GrepTool::get_file_type_glob("swift"), Some("*.swift"));
        assert_eq!(GrepTool::get_file_type_glob("kt"), Some("*.kt"));
        assert_eq!(GrepTool::get_file_type_glob("kotlin"), Some("*.kt"));
        assert_eq!(GrepTool::get_file_type_glob("scala"), Some("*.scala"));
        assert_eq!(GrepTool::get_file_type_glob("lua"), Some("*.lua"));
        assert_eq!(GrepTool::get_file_type_glob("sh"), Some("*.sh"));
        assert_eq!(GrepTool::get_file_type_glob("shell"), Some("*.sh"));
        assert_eq!(GrepTool::get_file_type_glob("bash"), Some("*.sh"));
        assert_eq!(GrepTool::get_file_type_glob("json"), Some("*.json"));
        assert_eq!(GrepTool::get_file_type_glob("yaml"), Some("*.yaml"));
        assert_eq!(GrepTool::get_file_type_glob("yml"), Some("*.yaml"));
        assert_eq!(GrepTool::get_file_type_glob("xml"), Some("*.xml"));
        assert_eq!(GrepTool::get_file_type_glob("html"), Some("*.html"));
        assert_eq!(GrepTool::get_file_type_glob("css"), Some("*.css"));
        assert_eq!(GrepTool::get_file_type_glob("scss"), Some("*.scss"));
        assert_eq!(GrepTool::get_file_type_glob("md"), Some("*.md"));
        assert_eq!(GrepTool::get_file_type_glob("markdown"), Some("*.md"));
        assert_eq!(GrepTool::get_file_type_glob("toml"), Some("*.toml"));
        assert_eq!(GrepTool::get_file_type_glob("sql"), Some("*.sql"));
        assert_eq!(GrepTool::get_file_type_glob("unknown"), None);
        assert_eq!(GrepTool::get_file_type_glob(""), None);
    }

    #[test]
    fn test_validate_glob_pattern_valid() {
        assert!(GrepTool::validate_glob_pattern("*.rs").is_ok());
        assert!(GrepTool::validate_glob_pattern("src/**/*.rs").is_ok());
        assert!(GrepTool::validate_glob_pattern("test_*.rs").is_ok());
        assert!(GrepTool::validate_glob_pattern("foo/bar/baz.rs").is_ok());
    }

    #[test]
    fn test_validate_glob_pattern_rejects_double_star_start() {
        let result = GrepTool::validate_glob_pattern("**/foo.rs");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("cannot start with '**'"));
    }

    #[test]
    fn test_validate_glob_pattern_rejects_parent_traversal() {
        let result = GrepTool::validate_glob_pattern("../foo.rs");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("cannot contain '..'"));
    }

    #[test]
    fn test_parse_results() {
        let input = b"file1.rs:10: fn test()\nfile2.rs:20: let x = 1\n";
        let results = parse_results(input, 100);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], "file1.rs:10: fn test()");
        assert_eq!(results[1], "file2.rs:20: let x = 1");
    }

    #[test]
    fn test_parse_results_respects_limit() {
        let input =
            b"file1.rs:10: line1\nfile2.rs:20: line2\nfile3.rs:30: line3\n";
        let results = parse_results(input, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], "file1.rs:10: line1");
        assert_eq!(results[1], "file2.rs:20: line2");
    }

    #[test]
    fn test_parse_results_handles_empty_lines() {
        let input = b"file1.rs:10: line1\n\nfile2.rs:20: line2\n";
        let results = parse_results(input, 100);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_parse_results_empty_input() {
        let input = b"";
        let results = parse_results(input, 100);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_results_filters_empty_strings() {
        let input = b"\n\n";
        let results = parse_results(input, 100);
        assert!(results.is_empty());
    }

    #[test]
    fn test_format_output_files_with_matches() {
        let results = vec!["file1.rs".to_string(), "file2.rs".to_string()];
        let output =
            format_output(OutputMode::FilesWithMatches, "fn test", results);
        assert!(output.contains("2 file(s)"));
        assert!(output.contains("file1.rs"));
        assert!(output.contains("file2.rs"));
        assert!(output.contains("fn test"));
    }

    #[test]
    fn test_format_output_files_with_matches_empty() {
        let results = Vec::<String>::new();
        let output =
            format_output(OutputMode::FilesWithMatches, "nonexistent", results);
        assert!(output.contains("No files found"));
        assert!(output.contains("nonexistent"));
    }

    #[test]
    fn test_format_output_content() {
        let results = vec![
            "file1.rs:10: fn test1()".to_string(),
            "file2.rs:20: fn test2()".to_string(),
        ];
        let output = format_output(OutputMode::Content, "fn test", results);
        assert!(output.contains("Found matches"));
        assert!(output.contains("file1.rs:10: fn test1()"));
        assert!(output.contains("file2.rs:20: fn test2()"));
    }

    #[test]
    fn test_format_output_content_empty() {
        let results = Vec::<String>::new();
        let output = format_output(OutputMode::Content, "nonexistent", results);
        assert!(output.contains("No matches found"));
    }

    #[test]
    fn test_format_output_count() {
        let results = vec!["file1.rs:5".to_string(), "file2.rs:10".to_string()];
        let output = format_output(OutputMode::Count, "fn", results);
        assert!(output.contains("Match counts"));
        assert!(output.contains("file1.rs:5"));
        assert!(output.contains("file2.rs:10"));
    }

    #[test]
    fn test_format_output_count_empty() {
        let results = Vec::<String>::new();
        let output = format_output(OutputMode::Count, "nonexistent", results);
        assert!(output.contains("No matches found"));
    }

    #[test]
    fn test_format_files_only_output() {
        let results = vec!["file1.rs".to_string(), "file2.rs".to_string()];
        let path = Path::new("/tmp/search");
        let output = format_files_only_output("*.rs", path, results);
        assert!(output.contains("2 match(es)"));
        assert!(output.contains("*.rs"));
        assert!(output.contains("/tmp/search"));
        assert!(output.contains("file1.rs"));
    }

    #[test]
    fn test_format_files_only_output_empty() {
        let results = Vec::<String>::new();
        let path = Path::new("/tmp/search");
        let output = format_files_only_output("*.rs", path, results);
        assert!(output.contains("No matches found"));
    }

    #[test]
    fn test_grep_tool_name() {
        let tool = GrepTool::new();
        assert_eq!(tool.name(), "Grep");
    }

    #[test]
    fn test_grep_tool_description() {
        let tool = GrepTool::new();
        assert!(tool.description().contains("regex"));
    }

    #[test]
    fn test_grep_tool_parameters() {
        let tool = GrepTool::new();
        let params = tool.parameters();
        assert!(params.is_object());
        let props = params.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("pattern"));
        assert!(props.contains_key("path"));
        assert!(props.contains_key("output_mode"));
        assert!(props.contains_key("case_insensitive"));
        assert!(props.contains_key("line_numbers"));
        assert!(props.contains_key("after_context"));
        assert!(props.contains_key("before_context"));
        assert!(props.contains_key("context"));
        assert!(props.contains_key("glob"));
        assert!(props.contains_key("type"));
        assert!(props.contains_key("head_limit"));
    }

    #[test]
    fn test_grep_tool_is_concurrency_safe() {
        let tool = GrepTool::new();
        let args = serde_json::json!({});
        assert!(tool.is_concurrency_safe(&args));
    }

    #[test]
    fn test_default_limits() {
        assert_eq!(DEFAULT_LIMIT, 100);
        assert_eq!(MAX_LIMIT, 2000);
    }
}
