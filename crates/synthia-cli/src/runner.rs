//! Non-interactive runner module
//!
//! This module provides functionality to run queries in non-interactive mode,
//! useful for testing and automation.

use std::path::Path;

use anyhow::{Context, Result};
use futures::StreamExt;
use rmcp::model::{Role, SamplingMessage, SamplingMessageContent};
use serde::Serialize;
use synthia_agent::{Agent, config::SessionConfig};
use tokio::fs;
use tokio_util::sync::CancellationToken;

use crate::cli::OutputFormat;

/// Result of a single query execution
#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub query: String,
    pub success: bool,
    pub response: String,
    pub tool_calls: Vec<ToolCallInfo>,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

/// Information about a tool call
#[derive(Debug, Serialize)]
pub struct ToolCallInfo {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub result: String,
}

/// Non-interactive runner for executing queries
pub struct NonInteractiveRunner {
    agent: Agent,
    session_config: SessionConfig,
    output_format: OutputFormat,
}

impl NonInteractiveRunner {
    pub fn new(
        agent: Agent,
        session_config: SessionConfig,
        output_format: OutputFormat,
    ) -> Self {
        Self {
            agent,
            session_config,
            output_format,
        }
    }

    /// Run a single query and return the result
    pub async fn run_single(&self, query: &str) -> Result<QueryResult> {
        let start_time = std::time::Instant::now();
        let cancel_token = CancellationToken::new();

        let user_msg = SamplingMessage {
            role: Role::User,
            content: rmcp::model::SamplingContent::Multiple(vec![
                SamplingMessageContent::Text(rmcp::model::RawTextContent {
                    text: query.to_string(),
                    meta: None,
                }),
            ]),
            meta: None,
        };

        let mut response_text = String::new();
        let mut tool_calls = Vec::new();
        let mut success = true;
        let mut error_msg = None;

        match self
            .agent
            .reply(user_msg, &self.session_config, cancel_token)
            .await
        {
            Ok(event_stream) => {
                tokio::pin!(event_stream);

                while let Some(event_result) = event_stream.next().await {
                    match event_result {
                        Ok(event) => {
                            self.process_event(
                                &event,
                                &mut response_text,
                                &mut tool_calls,
                            )
                            .await;
                        }
                        Err(e) => {
                            success = false;
                            error_msg = Some(format!("Agent error: {}", e));
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                success = false;
                error_msg = Some(format!("Failed to start agent: {}", e));
            }
        }

        let execution_time_ms = start_time.elapsed().as_millis() as u64;

        Ok(QueryResult {
            query: query.to_string(),
            success,
            response: response_text.trim().to_string(),
            tool_calls,
            error: error_msg,
            execution_time_ms,
        })
    }

    /// Run multiple queries from a file
    pub async fn run_from_file(
        &self,
        file_path: &Path,
    ) -> Result<Vec<QueryResult>> {
        let content =
            fs::read_to_string(file_path).await.with_context(|| {
                format!("Failed to read query file: {}", file_path.display())
            })?;

        let queries: Vec<&str> = content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();

        let mut results = Vec::new();
        for query in queries {
            let result = self.run_single(query).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Process an agent event and extract relevant information
    async fn process_event(
        &self,
        event: &synthia_agent::types::AgentEvent,
        response_text: &mut String,
        tool_calls: &mut Vec<ToolCallInfo>,
    ) {
        use synthia_agent::types::AgentEvent;

        match event {
            AgentEvent::Message(msg) => match &msg.content {
                rmcp::model::SamplingContent::Single(content) => {
                    if let SamplingMessageContent::Text(text_content) = content
                    {
                        response_text.push_str(&text_content.text);
                        response_text.push('\n');
                    }
                }
                rmcp::model::SamplingContent::Multiple(contents) => {
                    for content in contents {
                        if let SamplingMessageContent::Text(text_content) =
                            content
                        {
                            response_text.push_str(&text_content.text);
                            response_text.push('\n');
                        }
                    }
                }
            },
            AgentEvent::SystemNotification(notification) => {
                // Extract tool call information from notification data
                if let Some(ref data) = notification.data
                    && let Some(tool_name) = data.get("tool")
                    && let Some(args) = data.get("arguments")
                {
                    let tool_call = ToolCallInfo {
                        tool_name: tool_name
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string(),
                        arguments: args.clone(),
                        result: data
                            .get("result")
                            .map(|r| r.to_string())
                            .unwrap_or_default(),
                    };
                    tool_calls.push(tool_call);
                }
            }
            _ => {
                // Handle other event types if needed
            }
        }
    }

    /// Output the result in the specified format
    pub fn output_result(&self, result: &QueryResult) {
        match self.output_format {
            OutputFormat::Text => {
                println!("Query: {}", result.query);
                println!("Success: {}", result.success);
                if let Some(ref error) = result.error {
                    println!("Error: {}", error);
                }
                println!("Response:\n{}", result.response);
                if !result.tool_calls.is_empty() {
                    println!("\nTool Calls:");
                    for (i, tool_call) in result.tool_calls.iter().enumerate() {
                        println!(
                            "  {}. {}: {:?}",
                            i + 1,
                            tool_call.tool_name,
                            tool_call.arguments
                        );
                    }
                }
                println!("Execution time: {}ms", result.execution_time_ms);
                println!("---");
            }
            OutputFormat::Json => {
                if let Ok(json) = serde_json::to_string_pretty(result) {
                    println!("{json}");
                } else {
                    eprintln!("Failed to serialize query result");
                }
            }
            OutputFormat::Jsonl => {
                if let Ok(json) = serde_json::to_string(result) {
                    println!("{json}");
                } else {
                    eprintln!("Failed to serialize query result");
                }
            }
        }
    }

    /// Output multiple results
    pub fn output_results(&self, results: &[QueryResult]) {
        match self.output_format {
            OutputFormat::Text => {
                for result in results {
                    self.output_result(result);
                }
            }
            OutputFormat::Json => {
                if let Ok(json) = serde_json::to_string_pretty(results) {
                    println!("{json}");
                } else {
                    eprintln!("Failed to serialize results");
                }
            }
            OutputFormat::Jsonl => {
                for result in results {
                    if let Ok(json) = serde_json::to_string(result) {
                        println!("{json}");
                    } else {
                        eprintln!("Failed to serialize result");
                    }
                }
            }
        }
    }
}

/// Run a query in non-interactive mode
pub async fn run_query(
    agent: Agent,
    session_config: SessionConfig,
    query: &str,
    output_format: OutputFormat,
) -> Result<()> {
    let runner =
        NonInteractiveRunner::new(agent, session_config, output_format);
    let result = runner.run_single(query).await?;
    runner.output_result(&result);
    Ok(())
}

/// Run queries from a file in non-interactive mode
pub async fn run_queries_from_file(
    agent: Agent,
    session_config: SessionConfig,
    file_path: &Path,
    output_format: OutputFormat,
) -> Result<()> {
    let runner =
        NonInteractiveRunner::new(agent, session_config, output_format);
    let results = runner.run_from_file(file_path).await?;
    runner.output_results(&results);
    Ok(())
}
