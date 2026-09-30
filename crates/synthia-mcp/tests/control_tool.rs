//! [`McpControlTool`] — the `mcp` tool end to end over scripted
//! transports (no process, no socket).
//!
//! The tool is the dynamic counterpart of the pre-registered
//! [`McpTool`](synthia_mcp::McpTool): the host supervises servers,
//! and the model discovers and calls their tools at run time instead
//! of the host publishing every remote schema up front.

use std::sync::Arc;

use chrono::Utc;
use serde_json::{Value, json};
use synthia_core::registry::Registry as _;
use synthia_mcp::{
    InMemoryTransport,
    InMemoryTransportFactory,
    McpControlTool,
    McpSupervisor,
    register_mcp_control_tool,
};
use synthia_tool::{Context, Tool, ToolOutput, ToolRegistry};

/// A supervisor with two servers: `alpha` (connected, one `echo`
/// tool) and `beta` (transport dead at boot, so it lands in
/// `reconnecting`).
///
/// The registry is the `Arc` the tool and the supervisor both observe —
/// the same handle an agent loop would dispatch from.
struct Fixture {
    supervisor: Arc<McpSupervisor>,
    registry: Arc<ToolRegistry>,
    alpha_wire: Arc<InMemoryTransport>,
    beta_wire: Arc<InMemoryTransport>,
}

impl Fixture {
    async fn boot() -> Self {
        let alpha_wire = Arc::new(InMemoryTransport::new().with_echo_server());
        let beta_wire = Arc::new(InMemoryTransport::new().with_echo_server());
        beta_wire.set_alive(false);

        let supervisor = Arc::new(McpSupervisor::new());
        for (name, wire) in [
            ("alpha", Arc::clone(&alpha_wire)),
            ("beta", Arc::clone(&beta_wire)),
        ] {
            supervisor
                .supervise(
                    name,
                    Arc::new(InMemoryTransportFactory::new(wire)) as _,
                )
                .await;
        }
        let registry = Arc::new(ToolRegistry::new());
        let t0 = Utc::now();
        supervisor.tick(&registry, t0).await;
        Self {
            supervisor,
            registry,
            alpha_wire,
            beta_wire,
        }
    }

    fn tool(&self) -> McpControlTool {
        McpControlTool::new(Arc::clone(&self.supervisor), &self.registry)
    }
}

/// The textual projection of a `ToolOutput`.
fn text_of(output: &ToolOutput) -> String {
    output
        .content
        .iter()
        .filter_map(|part| part.text().map(str::to_string))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Call the tool and return the output, asserting nothing about it.
async fn call(tool: &McpControlTool, arguments: Value) -> ToolOutput {
    tool.call(arguments, &Context::default()).await
}

#[tokio::test]
async fn servers_reports_each_state_and_tool_count() {
    let fx = Fixture::boot().await;
    let out = call(&fx.tool(), json!({"action": "servers"})).await;

    assert_eq!(out.is_error, None);
    let text = text_of(&out);
    assert!(text.contains("alpha [connected]"), "{text}");
    assert!(text.contains("beta [reconnecting]"), "{text}");
    assert!(text.contains("1 tool(s) registered"), "{text}");
    assert!(text.contains("1 consecutive failure(s)"), "{text}");
    // The listing teaches the next call.
    assert!(text.contains("action `call`"), "{text}");
}

#[tokio::test]
async fn tools_lists_the_live_catalog_with_argument_schemas() {
    let fx = Fixture::boot().await;
    let out =
        call(&fx.tool(), json!({"action": "tools", "server": "alpha"})).await;

    assert_eq!(out.is_error, None);
    let text = text_of(&out);
    assert!(text.contains("advertises 1 tool(s)"), "{text}");
    assert!(text.contains("- echo: Echo the input back"), "{text}");
    assert!(text.contains(r#""required":["text"]"#), "{text}");
}

#[tokio::test]
async fn tools_filters_to_one_named_tool() {
    let fx = Fixture::boot().await;
    let out = call(
        &fx.tool(),
        json!({"action": "tools", "server": "alpha", "tool": "echo"}),
    )
    .await;
    assert_eq!(out.is_error, None);
    assert!(text_of(&out).contains("- echo:"));

    let miss = call(
        &fx.tool(),
        json!({"action": "tools", "server": "alpha", "tool": "nope"}),
    )
    .await;
    assert_eq!(miss.is_error, Some(true));
    let text = text_of(&miss);
    assert!(
        text.contains("does not advertise a tool named `nope`"),
        "{text}"
    );
    // The correction names what the server does advertise.
    assert!(text.contains("echo"), "{text}");
}

#[tokio::test]
async fn tools_distinguishes_an_unknown_server_from_a_disconnected_one() {
    let fx = Fixture::boot().await;

    let unknown =
        call(&fx.tool(), json!({"action": "tools", "server": "ghost"})).await;
    assert_eq!(unknown.is_error, Some(true));
    assert!(text_of(&unknown).contains("No MCP server named `ghost`"));

    let down =
        call(&fx.tool(), json!({"action": "tools", "server": "beta"})).await;
    assert_eq!(down.is_error, Some(true));
    let text = text_of(&down);
    assert!(text.contains("`beta` is not connected"), "{text}");
    assert!(text.contains("reconnecting"), "{text}");
}

#[tokio::test]
async fn call_invokes_the_remote_tool_by_its_server_side_name() {
    let fx = Fixture::boot().await;
    let out = call(
        &fx.tool(),
        json!({
            "action": "call",
            "server": "alpha",
            "tool": "echo",
            "arguments": {"text": "hi"}
        }),
    )
    .await;

    assert_eq!(out.is_error, None);
    assert_eq!(text_of(&out), "echoed");
    // The presentation hint names the server the call landed on.
    assert_eq!(out.metadata.get("mcp_server"), Some(&json!("alpha")));

    let sent = fx
        .alpha_wire
        .recorded()
        .into_iter()
        .find(|call| call.method == "tools/call")
        .expect("the call reached the wire");
    assert_eq!(sent.params["name"], json!("echo"));
    assert_eq!(sent.params["arguments"]["text"], json!("hi"));
}

#[tokio::test]
async fn call_defaults_missing_arguments_to_an_empty_object() {
    let fx = Fixture::boot().await;
    let _ = call(
        &fx.tool(),
        json!({"action": "call", "server": "alpha", "tool": "echo"}),
    )
    .await;

    let sent = fx
        .alpha_wire
        .recorded()
        .into_iter()
        .find(|call| call.method == "tools/call")
        .expect("the call reached the wire");
    assert_eq!(sent.params["arguments"], json!({}));
}

#[tokio::test]
async fn call_maps_a_remote_is_error_to_an_error_output() {
    let fx = Fixture::boot().await;
    fx.alpha_wire.set_script(
        "tools/call",
        json!({
            "content": [{"type": "text", "text": "bad input"}],
            "isError": true
        }),
    );
    let out = call(
        &fx.tool(),
        json!({"action": "call", "server": "alpha", "tool": "echo"}),
    )
    .await;
    assert_eq!(out.is_error, Some(true));
    assert_eq!(text_of(&out), "bad input");
}

#[tokio::test]
async fn call_preserves_a_multimodal_payload() {
    use synthia_provider::types::ContentPart;

    let b64 = "iVBORw0KGgoAAAANSUhEUg==";
    let fx = Fixture::boot().await;
    fx.alpha_wire.set_script(
        "tools/call",
        json!({
            "content": [
                {"type": "text", "text": "screenshot taken"},
                {"type": "image", "data": b64, "mimeType": "image/png"}
            ],
            "isError": false
        }),
    );
    let out = call(
        &fx.tool(),
        json!({"action": "call", "server": "alpha", "tool": "screenshot"}),
    )
    .await;

    let image = out
        .content
        .iter()
        .find_map(|part| match part {
            ContentPart::Image(image) => Some(image),
            _ => None,
        })
        .expect("the image block survives the control path");
    assert_eq!(image.data, b64);
    assert_eq!(image.mime_type, "image/png");
}

#[tokio::test]
async fn call_on_a_disconnected_server_never_reaches_the_wire() {
    let fx = Fixture::boot().await;
    let out = call(
        &fx.tool(),
        json!({"action": "call", "server": "beta", "tool": "echo"}),
    )
    .await;

    assert_eq!(out.is_error, Some(true));
    assert!(text_of(&out).contains("is not connected"));
    assert!(
        !fx.beta_wire.methods().contains(&"tools/call".to_string()),
        "a refused call must not be sent"
    );
}

#[tokio::test]
async fn malformed_calls_are_model_facing_errors() {
    let fx = Fixture::boot().await;

    let unknown = call(&fx.tool(), json!({"action": "launch"})).await;
    assert_eq!(unknown.is_error, Some(true));
    assert!(text_of(&unknown).contains("Unknown `action` `launch`"));

    let missing = call(&fx.tool(), json!({"action": "call"})).await;
    assert_eq!(missing.is_error, Some(true));
    assert!(text_of(&missing).contains("requires `server`"));

    let typo =
        call(&fx.tool(), json!({"action": "servers", "servers": "alpha"}))
            .await;
    assert_eq!(typo.is_error, Some(true));
    assert!(text_of(&typo).contains("Invalid arguments"));
}

#[tokio::test]
async fn registration_publishes_the_mcp_tool_as_sequential() {
    let fx = Fixture::boot().await;
    assert!(register_mcp_control_tool(
        &fx.registry,
        Arc::clone(&fx.supervisor)
    ));

    let entry = fx.registry.get("mcp").await.unwrap().expect("registered");
    let tool = entry.tool_instance();
    assert_eq!(tool.name(), "mcp");
    assert_eq!(tool.mode(), synthia_tool::ExecutionMode::Sequential);
    assert_eq!(
        tool.output_definition().kind,
        synthia_tool::RenderKind::Json
    );
    // The remote tools the supervisor published keep their
    // namespaced names, so the bare `mcp` name is free.
    assert!(fx.registry.get("mcp__alpha__echo").await.unwrap().is_some());
}

/// A deployment that hides a remote tool means it: the tool is absent
/// from `tools` (advertising it would be the leak the flag exists to
/// prevent) and `call` refuses without touching the wire.
///
/// This is the `is_hidden` privacy flag — what `[tools] hidden = [...]`
/// writes — not `ToolExposure::Hidden`, which is advertisement-only and
/// deliberately stays callable.
#[tokio::test]
async fn a_hidden_remote_tool_is_neither_listed_nor_callable() {
    let fx = Fixture::boot().await;
    assert!(fx.registry.set_hidden("mcp__alpha__echo", true));
    let tool = fx.tool();

    let listed =
        call(&tool, json!({"action": "tools", "server": "alpha"})).await;
    assert_eq!(listed.is_error, None, "an empty catalog is not an error");
    let text = text_of(&listed);
    assert!(text.contains("advertises no tools"), "{text}");
    assert!(!text.contains("echo"), "the hidden tool must not appear");

    // Asked for by name, the answer is the same "not advertised" the
    // registry's own listing would give — not an acknowledgement that
    // the tool exists.
    let named = call(
        &tool,
        json!({"action": "tools", "server": "alpha", "tool": "echo"}),
    )
    .await;
    assert_eq!(named.is_error, Some(true));
    assert!(text_of(&named).contains("does not advertise a tool named `echo`"));

    let invoked = call(
        &tool,
        json!({"action": "call", "server": "alpha", "tool": "echo"}),
    )
    .await;
    assert_eq!(invoked.is_error, Some(true));
    assert!(text_of(&invoked).contains("disabled by this deployment"));
    assert!(
        !fx.alpha_wire.methods().contains(&"tools/call".to_string()),
        "a policy refusal must not reach the server"
    );

    // Unhiding restores both paths.
    assert!(fx.registry.set_hidden("mcp__alpha__echo", false));
    let again =
        call(&tool, json!({"action": "tools", "server": "alpha"})).await;
    assert_eq!(again.is_error, None);
    assert!(text_of(&again).contains("- echo:"));
}

/// The second dispatch path must not be a way *around* the first: a
/// tool the registry's own listing drops is dropped here too.
#[tokio::test]
async fn call_agrees_with_the_registry_about_what_is_dispatchable() {
    let fx = Fixture::boot().await;
    assert!(fx.registry.set_hidden("mcp__alpha__echo", true));

    // What the registry says: not dispatcheable (its listing drops the
    // name entirely).
    assert!(
        !fx.registry
            .snapshot()
            .iter()
            .any(|meta| meta.name == "mcp__alpha__echo")
    );

    // What the tool says: the same.
    let out = call(
        &fx.tool(),
        json!({"action": "call", "server": "alpha", "tool": "echo"}),
    )
    .await;
    assert_eq!(out.is_error, Some(true));
}

/// A tool the deployment merely *deferred* is still executable by
/// design — visibility ≠ executability — so the control tool must not
/// become a second, stricter gate.
#[tokio::test]
async fn exposure_alone_does_not_block_a_call() {
    use synthia_tool::ToolExposure;

    let fx = Fixture::boot().await;
    assert!(
        fx.registry
            .set_exposure("mcp__alpha__echo", ToolExposure::Hidden)
    );

    let out = call(
        &fx.tool(),
        json!({"action": "call", "server": "alpha", "tool": "echo"}),
    )
    .await;
    assert_eq!(out.is_error, None, "{}", text_of(&out));
    assert_eq!(text_of(&out), "echoed");
}

/// Re-syncing a server must not revoke the deployment's policy: a
/// reconnect re-registers the generation, and the visibility the
/// registry carried for each surviving name travels with it.
#[tokio::test]
async fn a_resync_keeps_the_deployments_visibility_settings() {
    let fx = Fixture::boot().await;
    assert!(fx.registry.set_hidden("mcp__alpha__echo", true));
    assert!(fx.registry.set_exposure(
        "mcp__alpha__echo",
        synthia_tool::ToolExposure::Deferred
    ));

    fx.supervisor
        .notify_list_changed(&fx.registry, "alpha")
        .await
        .expect("re-sync");

    let entry = fx
        .registry
        .get("mcp__alpha__echo")
        .await
        .unwrap()
        .expect("the tool is still registered");
    assert!(entry.is_hidden(), "the privacy flag must survive a re-sync");
    assert_eq!(
        entry.exposure(),
        synthia_tool::ToolExposure::Deferred,
        "so must the exposure setting"
    );
    // And the control tool still refuses it.
    let out = call(
        &fx.tool(),
        json!({"action": "call", "server": "alpha", "tool": "echo"}),
    )
    .await;
    assert_eq!(out.is_error, Some(true));
    assert!(text_of(&out).contains("disabled by this deployment"));
}
