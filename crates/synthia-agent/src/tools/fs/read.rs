//! Read tool implementation
//!
//! Unified read tool that handles text files, media files, and provides head/tail functionality.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;
use tokio::io::AsyncBufReadExt;

use crate::{AgentError, tools::Tool};

const DEFAULT_LIMIT: usize = 2000;
const MAX_LINE_LENGTH: usize = 2000;
const MAX_BYTES: usize = 200 * 1024; // 200KB for text
const MAX_MEDIA_SIZE: usize = 10 * 1024 * 1024; // 10MB for media

#[derive(Debug, Clone, Deserialize)]
struct ReadRequest {
    file_path: String,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    head: Option<usize>,
    #[serde(default)]
    tail: Option<usize>,
}

fn default_limit() -> usize {
    DEFAULT_LIMIT
}

#[derive(Debug, Clone)]
pub struct ReadTool;

impl Default for ReadTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadTool {
    pub fn new() -> Self {
        Self
    }

    fn is_media_file(path: &Path) -> bool {
        if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            matches!(
                ext.as_str(),
                "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "webp"
                    | "bmp"
                    | "svg"
                    | "ico"
                    | "tiff"
                    | "tif"
                    | "mp4"
                    | "avi"
                    | "mov"
                    | "mkv"
                    | "wmv"
                    | "flv"
                    | "webm"
                    | "mp3"
                    | "wav"
                    | "flac"
                    | "aac"
                    | "ogg"
                    | "m4a"
            )
        } else {
            false
        }
    }

    fn get_mime_type(path: &Path) -> &'static str {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match ext.as_deref() {
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            Some("bmp") => "image/bmp",
            Some("svg") => "image/svg+xml",
            Some("ico") => "image/x-icon",
            Some("tiff") | Some("tif") => "image/tiff",
            Some("mp3") => "audio/mpeg",
            Some("wav") => "audio/wav",
            Some("ogg") => "audio/ogg",
            Some("flac") => "audio/flac",
            Some("aac") => "audio/aac",
            Some("m4a") => "audio/mp4",
            Some("mp4") => "video/mp4",
            Some("webm") => "video/webm",
            Some("avi") => "video/x-msvideo",
            Some("mov") => "video/quicktime",
            Some("mkv") => "video/x-matroska",
            _ => "application/octet-stream",
        }
    }

    async fn read_media_file(path: &PathBuf) -> CallToolResult {
        let metadata = match tokio::fs::metadata(path).await {
            Ok(m) => m,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to get file metadata: {e}"
                ))]);
            }
        };

        let file_size = metadata.len() as usize;
        if file_size > MAX_MEDIA_SIZE {
            return CallToolResult::error(vec![Content::text(format!(
                "File is too large ({file_size} bytes). Maximum allowed size for media files is {MAX_MEDIA_SIZE} bytes."
            ))]);
        }

        let content = match tokio::fs::read(path).await {
            Ok(c) => c,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to read file: {e}"
                ))]);
            }
        };

        let mime_type = Self::get_mime_type(path);
        let base64_data = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &content,
        );

        let media_type = if mime_type.starts_with("image/") {
            "image"
        } else if mime_type.starts_with("audio/") {
            "audio"
        } else if mime_type.starts_with("video/") {
            "video"
        } else {
            "blob"
        };

        let result = serde_json::json!({
            "type": media_type,
            "data": base64_data,
            "mimeType": mime_type,
            "size": file_size,
            "path": path.display().to_string()
        });

        CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )])
    }

    async fn read_head_lines(
        path: &PathBuf,
        num_lines: usize,
    ) -> Result<(Vec<String>, usize), AgentError> {
        let file = tokio::fs::File::open(path).await.map_err(|e| {
            AgentError::InvalidOperation(format!("Failed to open file: {e}"))
        })?;

        let reader = tokio::io::BufReader::new(file);
        let mut lines = Vec::with_capacity(num_lines);
        let mut total_lines = 0;
        let mut lines_iter = reader.lines();

        while let Some(line) = lines_iter.next_line().await.map_err(|e| {
            AgentError::InvalidOperation(format!("Failed to read file: {e}"))
        })? {
            total_lines += 1;
            if lines.len() < num_lines {
                lines.push(line);
            }
        }

        Ok((lines, total_lines))
    }

    async fn read_tail_lines(
        path: &PathBuf,
        num_lines: usize,
    ) -> Result<(Vec<String>, usize), AgentError> {
        let file = tokio::fs::File::open(path).await.map_err(|e| {
            AgentError::InvalidOperation(format!("Failed to open file: {e}"))
        })?;

        let reader = tokio::io::BufReader::new(file);
        let mut total_lines = 0;
        let mut last_lines: Vec<String> = Vec::with_capacity(num_lines);
        let mut lines_iter = reader.lines();

        while let Some(line) = lines_iter.next_line().await.map_err(|e| {
            AgentError::InvalidOperation(format!("Failed to read file: {e}"))
        })? {
            total_lines += 1;
            if last_lines.len() < num_lines {
                last_lines.push(line);
            } else {
                last_lines.push(line);
                last_lines.remove(0);
            }
        }

        Ok((last_lines, total_lines))
    }

    async fn read_offset_limit(
        path: &PathBuf,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<String>, usize), AgentError> {
        let file = tokio::fs::File::open(path).await.map_err(|e| {
            AgentError::InvalidOperation(format!("Failed to open file: {e}"))
        })?;

        let reader = tokio::io::BufReader::new(file);
        let mut lines = Vec::with_capacity(limit);
        let mut total_lines = 0;
        let mut lines_iter = reader.lines();

        while let Some(line) = lines_iter.next_line().await.map_err(|e| {
            AgentError::InvalidOperation(format!("Failed to read file: {e}"))
        })? {
            total_lines += 1;

            if total_lines < offset {
                continue;
            }

            if lines.len() < limit {
                lines.push(line);
            } else {
                break;
            }
        }

        Ok((lines, total_lines))
    }

    async fn read_full(path: &PathBuf) -> Result<(String, usize), AgentError> {
        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            AgentError::InvalidOperation(format!("Failed to read file: {e}"))
        })?;

        let total_lines = content.lines().count();
        let bytes = content.len();

        if bytes > MAX_BYTES {
            return Err(AgentError::InvalidOperation(format!(
                "File is too large ({bytes} bytes). Maximum allowed size is {MAX_BYTES} bytes. Use 'head' or 'tail' parameter to read a portion of the file."
            )));
        }

        Ok((content, total_lines))
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "Read"
    }

    fn description(&self) -> &str {
        "Read file content. Supports head/tail/offset/limit."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "File path"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max lines",
                    "default": 2000
                },
                "offset": {
                    "type": "integer",
                    "description": "Start line (1-based)"
                },
                "head": {
                    "type": "integer",
                    "description": "First N lines"
                },
                "tail": {
                    "type": "integer",
                    "description": "Last N lines"
                }
            },
            "required": ["file_path"]
        })
    }

    fn is_concurrency_safe(&self, _args: &Value) -> bool {
        true
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: ReadRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid arguments: {e}"
                ))]);
            }
        };

        let path = PathBuf::from(&request.file_path);
        if !path.is_absolute() {
            return CallToolResult::error(vec![Content::text(format!(
                "Path must be absolute. Received relative path: '{}'. Please provide an absolute path.",
                request.file_path
            ))]);
        }

        if !path.exists() {
            return CallToolResult::error(vec![Content::text(format!(
                "File does not exist: {}",
                path.display()
            ))]);
        }

        if !path.is_file() {
            return CallToolResult::error(vec![Content::text(format!(
                "Path is not a file: {}",
                path.display()
            ))]);
        }

        if Self::is_media_file(&path) {
            return Self::read_media_file(&path).await;
        }

        if request.head.is_some() && request.tail.is_some() {
            return CallToolResult::error(vec![Content::text(
                "Cannot specify both 'head' and 'tail' parameters simultaneously".to_string(),
            )]);
        }

        let offset = request.offset.unwrap_or(1).max(1);
        let limit = request.limit.min(DEFAULT_LIMIT);

        let (lines, total_lines) = if let Some(num_lines) = request.head {
            match Self::read_head_lines(&path, num_lines).await {
                Ok(r) => r,
                Err(e) => {
                    return CallToolResult::error(vec![Content::text(
                        format!("Failed to read file: {e}"),
                    )]);
                }
            }
        } else if let Some(num_lines) = request.tail {
            match Self::read_tail_lines(&path, num_lines).await {
                Ok(r) => r,
                Err(e) => {
                    return CallToolResult::error(vec![Content::text(
                        format!("Failed to read file: {e}"),
                    )]);
                }
            }
        } else if offset > 1 || limit < DEFAULT_LIMIT {
            match Self::read_offset_limit(&path, offset, limit).await {
                Ok(r) => r,
                Err(e) => {
                    return CallToolResult::error(vec![Content::text(
                        format!("Failed to read file: {e}"),
                    )]);
                }
            }
        } else {
            let (content, total) = match Self::read_full(&path).await {
                Ok(r) => r,
                Err(e) => {
                    return CallToolResult::error(vec![Content::text(
                        format!("Failed to read file: {e}"),
                    )]);
                }
            };
            let lines: Vec<&str> = content.lines().collect();
            let start = (offset - 1).min(lines.len());
            let end = (start + limit).min(lines.len());
            let selected: Vec<String> =
                lines[start..end].iter().map(ToString::to_string).collect();
            (selected, total)
        };

        let start_line = if request.tail.is_some() {
            total_lines.saturating_sub(lines.len()).saturating_add(1)
        } else {
            offset.min(total_lines + 1)
        };
        let end_line = (start_line - 1 + lines.len()).min(total_lines);

        let mut output_lines: Vec<String> = Vec::new();
        output_lines.push(format!(
            "File: {} (lines {}-{} of {})",
            path.display(),
            start_line,
            end_line,
            total_lines
        ));
        output_lines.push(String::new());

        let mut truncated_line_numbers: Vec<usize> = Vec::new();
        let mut n_bytes = 0;

        for (idx, line) in lines.iter().enumerate() {
            let line_num = start_line + idx;
            let truncated = if line.len() > MAX_LINE_LENGTH {
                truncated_line_numbers.push(line_num);
                format!("{}...", &line[..MAX_LINE_LENGTH])
            } else {
                line.clone()
            };
            n_bytes += truncated.len();
            output_lines.push(format!("{line_num:6}→{truncated}"));

            if n_bytes >= MAX_BYTES {
                break;
            }
        }

        let mut message = if !lines.is_empty() {
            format!(
                "{} lines read from file starting from line {}.",
                end_line.saturating_sub(start_line) + 1,
                start_line
            )
        } else {
            "No lines read from file.".to_string()
        };

        if end_line < total_lines {
            message += &format!(
                " End of file reached. ({} more lines)",
                total_lines - end_line
            );
        }

        if !truncated_line_numbers.is_empty() {
            let total_len: usize = truncated_line_numbers
                .iter()
                .map(|n| n.to_string().len() + 2)
                .sum();
            let mut nums_str = String::with_capacity(total_len);
            for (i, n) in truncated_line_numbers.iter().enumerate() {
                if i > 0 {
                    nums_str.push_str(", ");
                }
                nums_str.push_str(&n.to_string());
            }
            message += &format!(" Lines {nums_str} were truncated.");
        }

        if n_bytes >= MAX_BYTES {
            message += &format!(" Max {MAX_BYTES} bytes reached.");
        }

        CallToolResult::success(vec![Content::text(output_lines.join("\n"))])
    }
}
