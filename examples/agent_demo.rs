use std::sync::Arc;

use anyhow::{Context, Result};
use futures::StreamExt;
use synthia_agent::{
    Agent,
    agent::AgentControl,
    config::AgentConfig,
    context::DefaultContextManager,
    fs::{
        DeleteTool,
        EditTool,
        GrepTool,
        ListDirectoryTool,
        ReadTool,
        WriteTool,
    },
    guardian::{Guardian, GuardianConfig, SimpleGuardian},
    hooks::{HookRegistry, LoggingHook},
    model_router::{FirstModelRouter, ModelRouter},
    session::{SessionFileStore, SessionManager},
    shell::LocalShellExecutor,
    thinking::SequentialThinkingTool,
    todo::TodoWriteTool,
    tom::ContextInjectTool,
    tools::{
        AskUserQuestionTool,
        ExecTool,
        QuestionSenderImpl,
        SkillTool,
        ToolRegistry,
    },
    types::AgentEvent,
    web::WebFetchTool,
};
use synthia_examples::{ProviderConfig, SamplingMessage};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = ProviderConfig::from_env_auto()?;

    println!("=== Agent Demo Example ===");
    println!("Provider: {}", config.provider_type());
    println!("Base URL: {}", config.base_url);
    println!("Model: {}", config.model_name);
    println!();
    println!();

    let tool_registry = Arc::new(ToolRegistry::new());

    let current_dir = std::env::current_dir()?;
    let shell_executor: Arc<dyn synthia_agent::shell::ShellExecutor> =
        Arc::new(LocalShellExecutor::new());

    tool_registry.register(Arc::new(ReadTool::new())).await;
    tool_registry.register(Arc::new(WriteTool::new())).await;
    tool_registry.register(Arc::new(EditTool::new())).await;
    tool_registry
        .register(Arc::new(ListDirectoryTool::new()))
        .await;
    tool_registry
        .register(Arc::new(ExecTool::new(Arc::clone(&shell_executor))))
        .await;
    tool_registry.register(Arc::new(GrepTool::new())).await;
    tool_registry.register(Arc::new(DeleteTool::new())).await;

    tool_registry.register(Arc::new(TodoWriteTool::new())).await;

    let skill_tool = SkillTool::new(current_dir.clone());
    tool_registry.register(Arc::new(skill_tool.clone())).await;
    tool_registry
        .register(Arc::new(ContextInjectTool::new()))
        .await;
    tool_registry
        .register(Arc::new(AskUserQuestionTool::new(Arc::new(
            QuestionSenderImpl::new(),
        ))))
        .await;
    tool_registry
        .register(Arc::new(SequentialThinkingTool::new_with_stdout()))
        .await;
    tool_registry.register(Arc::new(WebFetchTool::new())).await;

    println!(
        "Registered tools: {}",
        tool_registry.tool_names().join(", ")
    );

    let hook_registry = Arc::new(HookRegistry::new());
    hook_registry
        .register(Arc::new(LoggingHook::new()) as synthia_agent::hooks::HookPtr)
        .await;

    let mut model_config =
        synthia_agent::model_router::ModelConfig::openai(&config.model_name);
    {
        let info = model_config.model_info_mut();
        info.api_key = config.api_key.clone();
        info.base_url = Some(config.base_url.clone());
    }

    let agent_config = AgentConfig {
        name: "demo-agent".to_string(),
        models: vec![model_config],
        ..Default::default()
    };

    let model_router: Arc<dyn ModelRouter> =
        Arc::new(FirstModelRouter::default());
    let context_manager =
        Arc::new(DefaultContextManager::new(Arc::clone(&model_router)));

    // Use file-based session storage
    let session_store = Arc::new(SessionFileStore::new());
    let session_manager: Arc<dyn SessionManager> = session_store as _;

    let agent = Agent::new(
        Arc::new(agent_config),
        synthia_agent::agent::AgentDeps {
            tools: tool_registry,
            context: context_manager,
            session: session_manager,
            router: model_router,
            hooks: hook_registry,
            skills: Arc::new(skill_tool.clone()),
            guardian: Arc::new(SimpleGuardian::new(GuardianConfig::default()))
                as Arc<dyn Guardian>,
            control: Arc::new(AgentControl::new()),
        },
    );

    let session = agent.deps.session.create_session().await?;
    let session_config = synthia_agent::config::SessionConfig::from(session);

    let user_message = if config.api_key.is_some() {
        "请创建以下任务：1. 任务1（无依赖），2. 任务2（无依赖），3. 任务3（依赖任务1），4. 任务4（依赖任务2），5. 任务5（依赖任务3和4）。然后使用Task工具的list action并设置dag_analysis为true来分析任务图，查看哪些任务可以并行执行。"
    } else {
        "Please create the following tasks: 1. Task 1 (no dependencies), 2. Task 2 (no dependencies), 3. Task 3 (depends on Task 1), 4. Task 4 (depends on Task 2), 5. Task 5 (depends on Task 3 and 4). Then use the Task tool's list action with dag_analysis=true to analyze the task graph and see which tasks can be executed in parallel."
    };

    println!("=== Sending Message ===");
    println!("User: {user_message}");
    println!();
    println!("=== Agent Response ===");

    let cancel_token = CancellationToken::new();

    let user_msg = SamplingMessage::user_text(user_message);

    let event_stream = agent
        .reply(user_msg, &session_config, cancel_token)
        .await
        .context("Failed to create event stream")?;

    tokio::pin!(event_stream);
    while let Some(event_result) = event_stream.next().await {
        match event_result {
            Ok(event) => match event {
                AgentEvent::Message(msg) => {
                    for content in msg.content.iter() {
                        if let rmcp::model::SamplingMessageContent::Text(text) =
                            content
                        {
                            print!("{}", text.text);
                        }
                    }
                }
                AgentEvent::Status(status) => {
                    println!("\n\n=== Status ===");
                    println!("{status:?}");
                }
                _ => {}
            },
            Err(e) => {
                eprintln!("Error: {e}");
            }
        }
    }

    println!("\n=== Demo Complete ===");
    Ok(())
}
