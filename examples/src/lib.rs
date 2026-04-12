pub mod config;
pub mod mcp;

use std::sync::Arc;

use anyhow::Result;
pub use config::ProviderConfig;
use futures_util::StreamExt;
use rmcp::model::{CallToolResult, RawContent};
pub use rmcp::model::{
    Content,
    CreateMessageRequestParams,
    JsonObject,
    ModelHint,
    ModelPreferences,
    RawTextContent,
    Role,
    SamplingContent,
    SamplingMessage,
    SamplingMessageContent,
    Tool as RmcpTool,
    ToolResultContent,
    ToolUseContent,
};
use synthia_agent::{
    config::SessionConfig,
    session::{SessionFileStore, SessionManager},
    tools::{Tool, ToolRegistry, register_builtin_tools, value_to_object},
    utils::create_tool_message,
};
pub use synthia_provider::{ModelProvider, OpenAICompatibleProvider};
use tokio_util::sync::CancellationToken;

pub fn get_tool_definitions(registry: &ToolRegistry) -> Vec<RmcpTool> {
    let tools = futures::executor::block_on(registry.filtered_tools(&[], &[]));
    tools
        .iter()
        .map(|t: &Arc<dyn Tool>| {
            RmcpTool::new(
                t.name().to_string(),
                t.description().to_string(),
                Arc::new(value_to_object(t.parameters())),
            )
        })
        .collect()
}

pub const DEFAULT_MAX_TOKENS: u32 = 4096;

#[allow(deprecated)]
pub async fn create_session(
    text: &str,
) -> Result<(Arc<dyn SessionManager>, SessionConfig)> {
    let session_manager: Arc<dyn SessionManager> =
        Arc::new(SessionFileStore::new());
    let session = session_manager.create_session().await?;
    let session_config = SessionConfig::from(session);
    let user_msg = SamplingMessage::user_text(text);
    session_manager
        .add_message(&session_config, &user_msg)
        .await?;
    Ok((session_manager, session_config))
}

pub async fn create_tool_registry() -> Arc<ToolRegistry> {
    let registry = Arc::new(ToolRegistry::new());
    register_builtin_tools(&registry).await;
    registry
}

pub fn create_request_params(
    model: &str,
    messages: Vec<SamplingMessage>,
    tools: Option<Vec<RmcpTool>>,
    system_prompt: Option<String>,
) -> CreateMessageRequestParams {
    CreateMessageRequestParams {
        messages,
        model_preferences: Some(ModelPreferences {
            hints: Some(vec![ModelHint {
                name: Some(model.to_string()),
            }]),
            cost_priority: None,
            speed_priority: None,
            intelligence_priority: None,
        }),
        max_tokens: DEFAULT_MAX_TOKENS,
        system_prompt,
        temperature: None,
        stop_sequences: None,
        metadata: None,
        meta: None,
        task: None,
        include_context: None,
        tools,
        tool_choice: None,
    }
}

pub async fn process_stream(
    cancel_token: CancellationToken,
    provider: &impl ModelProvider,
    session_manager: Arc<dyn SessionManager>,
    session_config: &SessionConfig,
    params: CreateMessageRequestParams,
) -> Result<Vec<ToolUseContent>> {
    let mut stream = provider.stream(params, cancel_token).await?;

    let mut pending_tool_calls = Vec::new();

    while let Some(result) = stream.next().await {
        match result {
            Ok(create_result) => {
                for content in create_result.message.content.iter() {
                    match content {
                        SamplingMessageContent::Text(text) => {
                            print!("{}", text.text);
                        }
                        SamplingMessageContent::ToolUse(tool_use) => {
                            println!("\n\n=== Tool Call Request ===");
                            println!("Tool: {}", tool_use.name);
                            println!("ID: {}", tool_use.id);
                            println!("Arguments: {:?}", tool_use.input);
                            pending_tool_calls.push(tool_use.clone());
                        }
                        _ => {}
                    }
                }
                session_manager
                    .add_message(session_config, &create_result.message)
                    .await?;
            }
            Err(e) => {
                println!("\nError: {e:?}");
            }
        }
    }

    Ok(pending_tool_calls)
}

pub async fn handle_tool_calls_with_session(
    session_manager: Arc<dyn SessionManager>,
    session_config: &SessionConfig,
    tool_registry: &ToolRegistry,
    pending_tool_calls: Vec<ToolUseContent>,
) -> Result<()> {
    println!("\n\n=== Processing Tool Calls ===");

    for tool_use in &pending_tool_calls {
        let id = &tool_use.id;
        let name = &tool_use.name;
        let input = &tool_use.input;
        println!("Processing tool: {id} name: {name}");

        let args = serde_json::Value::Object(input.clone());
        let cancel_token = tokio_util::sync::CancellationToken::new();
        let result: CallToolResult = match tool_registry
            .execute_with_tool(name, &args, &cancel_token)
            .await
        {
            Ok(r) => r,
            Err(e) => CallToolResult::success(vec![Content::text(format!(
                "工具执行错误: {e}"
            ))]),
        };

        println!("\n=== Tool Execution Result ===");
        for content in &result.content {
            if let RawContent::Text(text) = &content.raw {
                println!("{}", text.text);
            }
        }

        let tool_msg = create_tool_message(id.clone(), result);
        session_manager
            .add_message(session_config, &tool_msg)
            .await?;
    }
    Ok(())
}

#[allow(deprecated)]
pub async fn send_message(
    cancel_token: CancellationToken,
    provider: &impl ModelProvider,
    session_manager: Arc<dyn SessionManager>,
    session_config: &SessionConfig,
    model: &str,
    tools: Option<Vec<RmcpTool>>,
    system_prompt: Option<String>,
) -> Result<Vec<ToolUseContent>> {
    let conversation = session_manager.fix_conversation(session_config).await?;

    let params =
        create_request_params(model, conversation, tools, system_prompt);

    let tool_uses = process_stream(
        cancel_token,
        provider,
        Arc::clone(&session_manager),
        session_config,
        params,
    )
    .await?;
    Ok(tool_uses)
}
