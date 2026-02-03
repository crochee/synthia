//! Output handling module
//!
//! This module provides output formatting and display functionality
//! for the CLI, including message rendering, tool output, and
//! rate-limited printing.

use std::{
    io::{self, Write},
    time::Duration,
};

use async_trait::async_trait;
use synthia_agent::{event_handler::AgentEventHandler, types::AgentEvent};

use crate::color::colors;

const IMAGE_PREVIEW_WIDTH: u32 = 80;
const IMAGE_PREVIEW_HEIGHT: u32 = 40;
const IMAGE_BOX_WIDTH: usize = 40;
const IMAGE_BOX_HEIGHT: usize = 20;

const SUPPORTED_IMAGE_PREFIXES: &[&str] = &[
    "data:image/png;base64,",
    "data:image/jpeg;base64,",
    "data:image/gif;base64,",
    "data:image/webp;base64,",
];

pub const MAX_JSON_DISPLAY_LINES: usize = 20;
pub const MAX_TOOL_RESULT_CHARS: usize = 500;
pub const DEFAULT_TYPING_DELAY_MS: u64 = 8;

#[derive(Clone, Debug)]
pub struct OutputConfig {
    pub show_reasoning: bool,
    pub typing_delay_ms: u64,
    pub image_preview: ImagePreviewMode,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum ImagePreviewMode {
    #[default]
    Auto,
    On,
    Off,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            show_reasoning: true,
            typing_delay_ms: DEFAULT_TYPING_DELAY_MS,
            image_preview: ImagePreviewMode::Auto,
        }
    }
}

pub struct CliEventHandler {
    config: OutputConfig,
}

impl CliEventHandler {
    pub fn new(config: OutputConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl AgentEventHandler for CliEventHandler {
    async fn on_event(&self, agent_name: &str, event: &AgentEvent) {
        handle_agent_event(agent_name, event, &self.config).await;
    }
}

pub fn is_reasoning_text(raw_text: &rmcp::model::RawTextContent) -> bool {
    raw_text.meta.as_ref().is_some_and(|meta| {
        meta.0.get("type").and_then(|v| v.as_str()) == Some("reasoning")
    })
}

pub async fn print_with_rate(text: &str, color: Option<&str>, delay_ms: u64) {
    if delay_ms == 0 {
        if let Some(c) = color {
            print!("{}{}{}", c, text, colors::RESET);
        } else {
            print!("{}", text);
        }
        io::stdout().flush().ok();
        return;
    }

    for ch in text.chars() {
        if let Some(c) = color {
            print!("{}{}{}", c, ch, colors::RESET);
        } else {
            print!("{}", ch);
        }
        io::stdout().flush().ok();
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
}

pub async fn handle_agent_event(
    agent_name: &str,
    event: &AgentEvent,
    config: &OutputConfig,
) {
    match event {
        AgentEvent::Message(msg) => {
            handle_message_content(msg.clone(), config).await;
        }
        AgentEvent::McpNotification((name, _)) => {
            println!();
            println!(
                "{}[{}] MCP Notification: {}{}",
                colors::YELLOW,
                agent_name,
                name,
                colors::RESET
            );
        }
        AgentEvent::ModelChange { model, mode: _ } => {
            println!();
            println!(
                "{}[{}] Model changed to: {}{}",
                colors::YELLOW,
                agent_name,
                model,
                colors::RESET
            );
        }
        AgentEvent::HistoryReplaced(_) => {}
        AgentEvent::SystemNotification(notif) => {
            println!();
            println!(
                "{}[{}] System: {:?}{}",
                colors::YELLOW,
                agent_name,
                notif,
                colors::RESET
            );
        }
        AgentEvent::Status(status) => {
            println!();
            println!(
                "{}[{}] Status: {:?}{}",
                colors::YELLOW,
                agent_name,
                status,
                colors::RESET
            );
        }
        AgentEvent::TurnStarted { .. } => {}
        AgentEvent::TurnComplete { .. } => {}
        AgentEvent::TurnCompleteDetail { .. } => {}
        AgentEvent::TurnAborted { .. } => {}
        AgentEvent::ToolProgress { .. } => {}
    }
}

pub async fn handle_message_content(
    msg: rmcp::model::SamplingMessage,
    config: &OutputConfig,
) {
    for content in msg.content.iter() {
        match content {
            rmcp::model::SamplingMessageContent::Text(raw_text) => {
                if is_reasoning_text(raw_text) {
                    if config.show_reasoning {
                        print_with_rate(
                            &raw_text.text,
                            Some(colors::GRAY),
                            config.typing_delay_ms,
                        )
                        .await;
                    }
                } else {
                    print_with_rate(
                        &raw_text.text,
                        None,
                        config.typing_delay_ms,
                    )
                    .await;
                }
            }
            rmcp::model::SamplingMessageContent::ToolUse(tool_use) => {
                handle_tool_use(tool_use);
            }
            rmcp::model::SamplingMessageContent::ToolResult(tool_result) => {
                handle_tool_result(tool_result);
            }
            rmcp::model::SamplingMessageContent::Image(img) => {
                handle_image_content(img, config);
            }
            rmcp::model::SamplingMessageContent::Audio(audio) => {
                handle_audio_content(audio);
            }
        }
    }
}

fn handle_image_content(
    image: &rmcp::model::RawImageContent,
    config: &OutputConfig,
) {
    println!();
    println!(
        "{}[Image: {}]{}",
        colors::MAGENTA,
        image.mime_type,
        colors::RESET
    );

    if config.image_preview != ImagePreviewMode::Off {
        try_image_preview(image, config);
    }
}

fn try_image_preview(
    image: &rmcp::model::RawImageContent,
    config: &OutputConfig,
) {
    let data_url = &image.data;

    let base64_data = SUPPORTED_IMAGE_PREFIXES
        .iter()
        .find_map(|prefix| data_url.strip_prefix(prefix));

    if let Some(base64_data) = base64_data {
        let should_preview = config.image_preview == ImagePreviewMode::On
            || (config.image_preview == ImagePreviewMode::Auto
                && std::env::var("TERM_PROGRAM").is_ok());
        if should_preview {
            print_image_preview(base64_data);
        }
    }
}

fn handle_audio_content(audio: &rmcp::model::RawAudioContent) {
    println!();
    println!(
        "{}[Audio: {}]{}",
        colors::MAGENTA,
        audio.mime_type,
        colors::RESET
    );
}

fn print_image_preview(base64_data: &str) {
    use base64::{Engine, engine::general_purpose::STANDARD};

    let Ok(data) = STANDARD.decode(base64_data) else {
        return;
    };

    let Ok(img) = image::load_from_memory(&data) else {
        return;
    };

    let thumbnail = img.thumbnail(IMAGE_PREVIEW_WIDTH, IMAGE_PREVIEW_HEIGHT);

    println!();
    println!(
        "{}┌{}─{}┐{}",
        colors::CYAN,
        "─".repeat(IMAGE_BOX_WIDTH),
        "─".repeat(IMAGE_BOX_HEIGHT),
        colors::RESET
    );

    let rgba = thumbnail.to_rgba8();
    let (width, height) = rgba.dimensions();

    for y in 0..height {
        print!("{}", colors::CYAN);
        print!("│");
        for x in 0..width {
            let pixel = rgba.get_pixel(x, y);
            let chars = get_ascii_char(pixel[0], pixel[1], pixel[2], pixel[3]);
            print!("{}", chars);
        }
        println!("{}│{}", colors::CYAN, colors::RESET);
    }

    println!(
        "{}└{}─{}┘{}",
        colors::CYAN,
        "─".repeat(IMAGE_BOX_WIDTH),
        "─".repeat(IMAGE_BOX_HEIGHT),
        colors::RESET
    );
    println!();
}

fn get_ascii_char(r: u8, g: u8, b: u8, alpha: u8) -> &'static str {
    if alpha < 128 {
        return " ";
    }

    let brightness = (r as u16 + g as u16 + b as u16) / 3;
    let idx = (brightness / 32) as usize;

    let chars = [" ", "░", "▒", "▓", "█"];
    chars.get(idx).unwrap_or(&"█")
}

pub fn handle_tool_use(tool_use: &rmcp::model::ToolUseContent) {
    println!();
    println!(
        "{}─── Tool: {} ───{}",
        colors::YELLOW,
        tool_use.name,
        colors::RESET
    );
    if !tool_use.input.is_empty() {
        println!("{}Arguments:{}", colors::GRAY, colors::RESET);
        if let Ok(json) = serde_json::to_string_pretty(&tool_use.input) {
            for line in json.lines().take(MAX_JSON_DISPLAY_LINES) {
                println!("{}  {}{}", colors::GRAY, line, colors::RESET);
            }
            if json.lines().count() > MAX_JSON_DISPLAY_LINES {
                println!("{}  ... (truncated){}", colors::GRAY, colors::RESET);
            }
        }
    }
}

pub fn handle_tool_result(tool_result: &rmcp::model::ToolResultContent) {
    println!();
    if tool_result.is_error.unwrap_or(false) {
        println!("{}─── Tool Error ───{}", colors::RED, colors::RESET);
    } else {
        println!("{}─── Tool Result ───{}", colors::GREEN, colors::RESET);
    }
    for content in &tool_result.content {
        if let Some(text) = content.as_text() {
            let output = if text.text.len() > MAX_TOOL_RESULT_CHARS {
                let truncated = text
                    .text
                    .char_indices()
                    .take_while(|(idx, _)| *idx < MAX_TOOL_RESULT_CHARS)
                    .map(|(_, c)| c)
                    .collect::<String>();
                format!("{}...\n[truncated]", truncated)
            } else {
                text.text.clone()
            };
            println!("{}", output);
        }
    }
}

pub fn print_help() {
    use crate::color::{print_help_item, print_separator};

    println!("Type your message or use commands:");
    print_help_item("/quit, /exit", "Exit the chat");
    print_help_item("/clear", "Clear conversation history");
    print_help_item("/compact", "Compact conversation history");
    print_help_item("/session", "Create a new session");
    print_help_item(
        "/session <session_id>",
        "Switch to session and show details",
    );
    print_help_item("/sessions", "List all sessions");
    print_help_item("/help", "Show this help message");
    print_help_item("/reasoning", "Toggle reasoning display (on/off)");
    print_help_item("/speed <ms>", "Set typing speed (0=instant, default=8)");
    print_help_item("/attach <path>", "Attach a file to next message");
    print_help_item("/attachments", "List current attachments");
    print_help_item("/clear-attachments", "Clear all attachments");
    print_help_item("/export <format>", "Export conversation (json/markdown)");
    print_help_item("/history [n]", "Show last n messages");
    print_help_item("/token", "Show token usage");
    print_separator('─', 60);
    println!();
    println!(
        "{}Media input:{} @<image> !<audio> in message",
        colors::CYAN,
        colors::RESET
    );
}

pub fn print_tools(tools: &[String]) {
    println!(
        "{}✓{} Tools available: {}{}{}",
        colors::GREEN,
        colors::RESET,
        colors::MAGENTA,
        tools.join(", "),
        colors::RESET
    );
}

pub fn print_error(e: &dyn std::fmt::Display) {
    eprintln!("{}Error: {}{}", colors::RED, e, colors::RESET);
}

pub fn print_info(msg: &str) {
    println!("{}{}{}", colors::GREEN, msg, colors::RESET);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(MAX_JSON_DISPLAY_LINES, 20);
        assert_eq!(MAX_TOOL_RESULT_CHARS, 500);
        assert_eq!(DEFAULT_TYPING_DELAY_MS, 8);
    }

    #[test]
    fn test_is_reasoning_text_false_no_meta() {
        let raw_text = rmcp::model::RawTextContent {
            text: "Hello".to_string(),
            meta: None,
        };
        assert!(!is_reasoning_text(&raw_text));
    }

    #[test]
    fn test_is_reasoning_text_false_wrong_type() {
        let mut map = serde_json::Map::new();
        map.insert("type".to_string(), serde_json::json!("text"));
        let raw_text = rmcp::model::RawTextContent {
            text: "Hello".to_string(),
            meta: Some(rmcp::model::Meta(map)),
        };
        assert!(!is_reasoning_text(&raw_text));
    }

    #[test]
    fn test_is_reasoning_text_true() {
        let mut map = serde_json::Map::new();
        map.insert("type".to_string(), serde_json::json!("reasoning"));
        let raw_text = rmcp::model::RawTextContent {
            text: "Thinking...".to_string(),
            meta: Some(rmcp::model::Meta(map)),
        };
        assert!(is_reasoning_text(&raw_text));
    }

    #[test]
    fn test_output_config_default() {
        let config = OutputConfig::default();
        assert!(config.show_reasoning);
        assert_eq!(config.typing_delay_ms, DEFAULT_TYPING_DELAY_MS);
        assert_eq!(config.image_preview, ImagePreviewMode::Auto);
    }

    #[test]
    fn test_image_preview_mode() {
        assert_eq!(ImagePreviewMode::Auto, ImagePreviewMode::default());
        assert_ne!(ImagePreviewMode::On, ImagePreviewMode::Off);
    }
}
