//! R34 / R50 / R58 + factory wiring.
//!
//! Each `with_optional_*` knob on `RunDependencies` must reach
//! the `AgentRunConfig` the controller hands to the factory —
//! without that hop, a deployment's `max_visible` / `strategy`
//! / `tool_restriction` would never leave the config file. The
//! factory itself then installs each knob on the agent it
//! builds, so the wire tools / strategy / deny-list honour the
//! active configuration.
//!
//! Cover, in order: deps → config forwarding (tool_surface,
//! strategy, tool_restriction); factory → wire tools and
//! strategy (the `ToolCapturingProvider` records the request
//! tool list); R58 deny-list reaches the wire.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use futures::StreamExt;
use synthia::harness::{AgentEvent, AgentInput, AgentRunConfig, PromptContext};
use tokio_util::sync::CancellationToken;

use super::{
    super::{AgentRunStreamFactory, RunStreamFactory},
    support::{ToolCapturingProvider, VecFactory, test_deps, wait_for_runs},
};

#[tokio::test]
async fn run_config_carries_the_dependencies_tool_surface() {
    let policy = synthia::tool::ToolSurfacePolicy {
        max_visible: Some(4),
        ..synthia::tool::ToolSurfacePolicy::default()
    };
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(VecFactory::new(vec![], Arc::clone(&calls), None));
    let temp = tempfile::TempDir::new().unwrap();
    let manager = synthia::session::manager::SessionRegistry::new(
        temp.path().to_path_buf(),
    );
    manager
        .create_with_user("s1".to_string(), "alice".to_string())
        .await
        .unwrap();
    let deps = test_deps().with_optional_tool_surface(Some(policy.clone()));
    let controller = super::super::SessionController::spawn(
        "alice",
        "s1",
        manager.input_queue(),
        manager.sink("alice", "s1"),
        deps,
        Duration::from_secs(60),
        factory,
    );

    controller
        .submit(super::super::SessionOp::Prompt {
            content: "go".to_string(),
            priority: 1,
        })
        .await
        .unwrap();
    wait_for_runs(&calls, 1).await;

    let configs = calls.lock().unwrap();
    assert_eq!(
        configs[0].tool_surface.as_ref(),
        Some(&policy),
        "the controller must forward the deps' surface policy"
    );
    assert!(
        test_deps().tool_surface.is_none(),
        "no [tools] section means `None`, the R33 path"
    );
}

/// R50: the same hop for the reasoning loop — a strategy
/// installed on [`RunDependencies`] reaches the
/// [`AgentRunConfig`] the controller hands to the factory.
#[tokio::test]
async fn run_config_carries_the_dependencies_strategy() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(VecFactory::new(vec![], Arc::clone(&calls), None));
    let temp = tempfile::TempDir::new().unwrap();
    let manager = synthia::session::manager::SessionRegistry::new(
        temp.path().to_path_buf(),
    );
    manager
        .create_with_user("s1".to_string(), "alice".to_string())
        .await
        .unwrap();
    let strategy: Arc<dyn synthia::harness::agent::ReasoningStrategy> =
        synthia::harness::agent::from_name("chain-of-thought").unwrap();
    let deps = test_deps().with_optional_strategy(Some(Arc::clone(&strategy)));
    let controller = super::super::SessionController::spawn(
        "alice",
        "s1",
        manager.input_queue(),
        manager.sink("alice", "s1"),
        deps,
        Duration::from_secs(60),
        factory,
    );

    controller
        .submit(super::super::SessionOp::Prompt {
            content: "go".to_string(),
            priority: 1,
        })
        .await
        .unwrap();
    wait_for_runs(&calls, 1).await;

    let configs = calls.lock().unwrap();
    assert_eq!(
        configs[0].strategy.as_ref().map(|s| s.name().to_string()),
        Some("chain-of-thought".to_string()),
        "the controller must forward the deps' strategy"
    );
    assert!(
        test_deps().strategy.is_none(),
        "no `strategy =` in the agent config means the ReAct default"
    );
}

/// R58: the same hop for the agent's allow/deny list.
#[tokio::test]
async fn run_config_carries_the_dependencies_tool_restriction() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(VecFactory::new(vec![], Arc::clone(&calls), None));
    let temp = tempfile::TempDir::new().unwrap();
    let manager = synthia::session::manager::SessionRegistry::new(
        temp.path().to_path_buf(),
    );
    manager
        .create_with_user("s1".to_string(), "alice".to_string())
        .await
        .unwrap();
    let restriction = Arc::new(synthia::tool::ToolRestriction::deny(["shell"]));
    let deps = test_deps()
        .with_optional_tool_restriction(Some(Arc::clone(&restriction)));
    let controller = super::super::SessionController::spawn(
        "alice",
        "s1",
        manager.input_queue(),
        manager.sink("alice", "s1"),
        deps,
        Duration::from_secs(60),
        factory,
    );

    controller
        .submit(super::super::SessionOp::Prompt {
            content: "go".to_string(),
            priority: 1,
        })
        .await
        .unwrap();
    wait_for_runs(&calls, 1).await;

    let configs = calls.lock().unwrap();
    let carried = configs[0]
        .tool_restriction
        .as_ref()
        .expect("the controller must forward the deps' restriction");
    assert!(!carried.is_relevant("shell", false));
    assert!(carried.is_relevant("read", false));
    assert!(
        test_deps().tool_restriction.is_none(),
        "no allow/deny lists means no restriction"
    );
}

/// R34: the real [`AgentRunStreamFactory`] installs the config's
/// tool-surface policy on the agent it builds, so the tools on the
/// wire honour the active group and the cap — while the registry
/// still holds every tool.
#[tokio::test]
async fn run_factory_narrows_the_wire_tools_to_the_tool_surface() {
    let provider = Arc::new(ToolCapturingProvider {
        captured: parking_lot::Mutex::new(Vec::new()),
    });
    let registry = Arc::new(synthia::tool::registry::ToolRegistry::new());
    for name in ["gamma", "beta", "alpha"] {
        registry.register_entry(synthia::tool::ToolEntry::dynamic(
            name.to_string(),
            format!("the {name} tool"),
            serde_json::json!({"type": "object", "properties": {}}),
        ));
    }
    let policy = synthia::tool::ToolSurfacePolicy {
        max_visible: Some(1),
        groups: [(
            "core".to_string(),
            vec!["alpha".to_string(), "beta".to_string()],
        )]
        .into_iter()
        .collect(),
        active_groups: vec!["core".to_string()],
    };
    let config = AgentRunConfig {
        provider: Arc::clone(&provider)
            as Arc<dyn synthia::provider::traits::ModelProvider>,
        tool_registry: Arc::clone(&registry),
        workspace_root: PathBuf::from("/tmp"),
        system_prompt: "be brief".to_string(),
        prompt_context: Arc::new(PromptContext::default()),
        agent_resolver: None,
        agent_name: None,
        max_iterations: Some(1),
        steering: Arc::new(synthia::steering::Steering::noop()),
        agent_registry: None,
        typed_event_sink: None,
        context_manager: None,
        tool_surface: Some(policy),
        strategy: None,
        tool_restriction: None,
    };

    let stream = AgentRunStreamFactory.run_stream(
        config,
        AgentInput::text("go"),
        Arc::new(CancellationToken::new()),
    );
    let _events: Vec<AgentEvent> = stream.collect().await;

    let captured = provider.captured.lock();
    assert_eq!(captured.len(), 1);
    let names: Vec<&str> =
        captured[0].iter().map(|d| d.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["alpha"],
        "the wire list must be the active group capped at one"
    );
    assert!(
        registry.contains("gamma"),
        "the run factory must not unregister tools it did not \
         advertise"
    );
}

/// R50: the factory installs the configured strategy, so the loop
/// that actually runs is the one the deployment asked for. The two
/// runs differ in what they put on the wire: ReAct advertises the
/// registry's tool (and would call it), Chain-of-Thought advertises
/// none — same registry, same provider, same request path.
#[tokio::test]
async fn run_factory_runs_the_configured_strategy() {
    let registry = Arc::new(synthia::tool::registry::ToolRegistry::new());
    registry.register_entry(synthia::tool::ToolEntry::dynamic(
        "alpha".to_string(),
        "the alpha tool".to_string(),
        serde_json::json!({"type": "object", "properties": {}}),
    ));

    let mut advertised = Vec::new();
    for strategy in [
        None,
        Some(synthia::harness::agent::from_name("chain-of-thought").unwrap()),
    ] {
        let provider = Arc::new(ToolCapturingProvider {
            captured: parking_lot::Mutex::new(Vec::new()),
        });
        let config = AgentRunConfig {
            provider: Arc::clone(&provider)
                as Arc<dyn synthia::provider::traits::ModelProvider>,
            tool_registry: Arc::clone(&registry),
            workspace_root: PathBuf::from("/tmp"),
            system_prompt: "be brief".to_string(),
            prompt_context: Arc::new(PromptContext::default()),
            agent_resolver: None,
            agent_name: None,
            max_iterations: Some(2),
            steering: Arc::new(synthia::steering::Steering::noop()),
            agent_registry: None,
            typed_event_sink: None,
            context_manager: None,
            tool_surface: None,
            strategy,
            tool_restriction: None,
        };

        let stream = AgentRunStreamFactory.run_stream(
            config,
            AgentInput::text("go"),
            Arc::new(CancellationToken::new()),
        );
        let _events: Vec<AgentEvent> = stream.collect().await;

        let captured = provider.captured.lock();
        advertised.push(
            captured[0]
                .iter()
                .map(|d| d.name.clone())
                .collect::<Vec<_>>(),
        );
    }

    // R58: a configured restriction reaches the wire — the denied
    // tool is not advertised at all.
    let provider = Arc::new(ToolCapturingProvider {
        captured: parking_lot::Mutex::new(Vec::new()),
    });
    let config = AgentRunConfig {
        provider: Arc::clone(&provider)
            as Arc<dyn synthia::provider::traits::ModelProvider>,
        tool_registry: Arc::clone(&registry),
        workspace_root: PathBuf::from("/tmp"),
        system_prompt: "be brief".to_string(),
        prompt_context: Arc::new(PromptContext::default()),
        agent_resolver: None,
        agent_name: None,
        max_iterations: Some(2),
        steering: Arc::new(synthia::steering::Steering::noop()),
        agent_registry: None,
        typed_event_sink: None,
        context_manager: None,
        tool_surface: None,
        strategy: None,
        tool_restriction: Some(Arc::new(synthia::tool::ToolRestriction::deny(
            ["alpha"],
        ))),
    };
    let stream = AgentRunStreamFactory.run_stream(
        config,
        AgentInput::text("go"),
        Arc::new(CancellationToken::new()),
    );
    let _events: Vec<AgentEvent> = stream.collect().await;
    let captured = provider.captured.lock();
    let restricted: Vec<String> =
        captured[0].iter().map(|d| d.name.clone()).collect();
    drop(captured);

    assert_eq!(
        advertised[0],
        vec!["alpha".to_string()],
        "the default (no strategy configured) must stay ReAct, which \
         advertises the tool"
    );
    assert!(
        restricted.is_empty(),
        "the only tool is denied, so nothing is advertised: {restricted:?}"
    );
    assert!(
        advertised[1].is_empty(),
        "chain-of-thought must advertise no tools: {:?}",
        advertised[1]
    );
}
