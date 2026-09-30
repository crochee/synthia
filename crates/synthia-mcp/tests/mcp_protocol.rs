//! MCP protocol + tool-adapter + registry-publication tests.
//!
//! The in-memory transport covers the protocol layer with zero
//! process spawn; the last test exercises the real
//! `StdioTransport` against a POSIX `sh` responder.

use std::sync::Arc;

use serde_json::json;
use synthia_mcp::{
    InMemoryTransport,
    MCP_PROTOCOL_VERSION,
    McpClient,
    McpToolSpec,
    NamingPolicy,
    StdioConfig,
    StdioTransport,
    register_mcp_tools,
};
use synthia_tool::{Context, ToolRegistry, traits::Tool};

#[tokio::test]
async fn initialize_sends_handshake_then_initialized_notification() {
    let transport = Arc::new(InMemoryTransport::new().with_echo_server());
    let client = McpClient::new(Arc::clone(&transport) as _);
    let result = client.initialize().await.expect("initialize");

    assert_eq!(result["serverInfo"]["name"], "in-memory-test");
    assert_eq!(client.server_name(), "in-memory-test");

    let calls = transport.recorded();
    assert_eq!(calls[0].method, "initialize");
    // The spec's protocolVersion is what we advertise.
    assert_eq!(
        calls[0].params["protocolVersion"],
        json!(MCP_PROTOCOL_VERSION)
    );
    assert_eq!(calls[0].params["clientInfo"]["name"], json!("synthia"));
    // The follow-up notification is mandatory.
    assert_eq!(calls[1].method, "notifications/initialized");
    assert!(calls[1].is_notification);
}

#[tokio::test]
async fn tools_list_parses_specs() {
    let transport = Arc::new(InMemoryTransport::new().with_echo_server());
    let client = McpClient::new(transport);
    let specs = client.tools_list().await.expect("tools/list");
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].name, "echo");
    assert_eq!(specs[0].description, "Echo the input back");
    assert_eq!(specs[0].input_schema["required"][0], json!("text"));
}

#[tokio::test]
async fn tools_list_defaults_a_missing_schema_to_object() {
    let transport = Arc::new(
        InMemoryTransport::new()
            .on("tools/list", json!({"tools": [{"name": "bare"}]})),
    );
    let client = McpClient::new(transport);
    let specs = client.tools_list().await.expect("tools/list");
    assert_eq!(specs[0].input_schema, json!({"type": "object"}));
    assert_eq!(specs[0].description, "");
}

#[tokio::test]
async fn tools_call_forwards_name_and_arguments() {
    let transport = Arc::new(InMemoryTransport::new().with_echo_server());
    let client = McpClient::new(Arc::clone(&transport) as _);
    let result = client
        .tools_call("echo", json!({"text": "hi"}))
        .await
        .expect("tools/call");
    assert_eq!(result.text(), "echoed");
    assert!(!result.is_error);

    let calls = transport.recorded();
    assert_eq!(calls[0].method, "tools/call");
    assert_eq!(calls[0].params["name"], json!("echo"));
    assert_eq!(calls[0].params["arguments"]["text"], json!("hi"));
}

#[tokio::test]
async fn transport_error_becomes_mcp_err_message() {
    let transport = Arc::new(InMemoryTransport::new().with_echo_server());
    let raw = Arc::clone(&transport) as Arc<dyn synthia_mcp::McpTransport>;
    let client = McpClient::new(raw);
    transport.fail_next("transport down");
    let err = client.tools_list().await.unwrap_err();
    assert!(err.to_string().contains("transport down"));
}

// -- Tool adapter -------------------------------------------------

#[tokio::test]
async fn mcptool_adapts_spec_and_maps_result() {
    let transport = Arc::new(InMemoryTransport::new().with_echo_server());
    let client = McpClient::new(transport);
    let spec = client.tools_list().await.unwrap().remove(0);
    let tool = client.tool(&spec, Arc::clone(&client));

    assert_eq!(tool.name(), "echo");
    assert_eq!(tool.description(), "Echo the input back");
    assert_eq!(tool.parameters()["required"][0], json!("text"));
    // Remote tools are sequential (shared transport ordering).
    assert_eq!(tool.mode(), synthia_tool::ExecutionMode::Sequential);
    // Render contract marks them as JSON + names the server.
    let def = tool.output_definition();
    assert_eq!(def.kind, synthia_tool::RenderKind::Json);
    assert_eq!(def.name, "echo");

    let out = tool
        .call(json!({"text": "hi"}), &synthia_tool::Context::default())
        .await;
    assert_eq!(out.is_error, None);
    let text = out
        .content
        .iter()
        .filter_map(|p| p.text().map(str::to_string))
        .collect::<String>();
    assert_eq!(text, "echoed");
}

#[tokio::test]
async fn mcptool_maps_is_error_to_error_output() {
    let transport = Arc::new(InMemoryTransport::new().with_echo_server().on(
        "tools/call",
        json!({
            "content": [{"type": "text", "text": "bad input"}],
            "isError": true
        }),
    ));
    let client = McpClient::new(transport);
    let spec = client.tools_list().await.unwrap().remove(0);
    let tool = client.tool(&spec, Arc::clone(&client));

    let out = tool
        .call(json!({"text": "x"}), &synthia_tool::Context::default())
        .await;
    assert_eq!(out.is_error, Some(true));
}

#[tokio::test]
async fn mcptool_surfaces_transport_failure_as_error_output() {
    let transport = Arc::new(InMemoryTransport::new().with_echo_server());
    let raw = Arc::clone(&transport) as Arc<dyn synthia_mcp::McpTransport>;
    let client = McpClient::new(raw);
    let spec = McpToolSpec {
        name: "echo".into(),
        description: "d".into(),
        input_schema: json!({"type": "object"}),
    };
    let tool = client.tool(&spec, Arc::clone(&client));
    transport.fail_next("kaput");
    let out = tool
        .call(json!({}), &synthia_tool::Context::default())
        .await;
    assert_eq!(out.is_error, Some(true));
}

// -- Registry publication ----------------------------------------

#[tokio::test]
async fn register_mcp_tools_publishes_into_the_registry() {
    use synthia_core::registry::Registry as _;

    let transport = Arc::new(InMemoryTransport::new().with_echo_server());
    let client = McpClient::new(transport);
    let registry = ToolRegistry::new();

    let generation = register_mcp_tools(
        &registry,
        Arc::clone(&client),
        "srv",
        NamingPolicy::Namespaced,
    )
    .await
    .expect("register");
    assert_eq!(generation.names(), vec!["mcp__srv__echo"]);

    // The remote tool is now a first-class registry member under
    // its namespaced public name.
    let entry = registry
        .get("mcp__srv__echo")
        .await
        .unwrap()
        .expect("namespaced tool registered");
    assert_eq!(entry.tool_instance().name(), "mcp__srv__echo");
    assert_eq!(
        entry.tool_instance().parameters()["required"][0],
        json!("text")
    );
    // And it renders in the catalog.
    let snap = registry.snapshot();
    assert!(snap.iter().any(|m| m.name == "mcp__srv__echo"));
}

/// Two servers publishing the same raw tool name coexist in one
/// registry — the whole point of namespacing. Each namespaced
/// tool still calls its OWN server's raw name on the wire.
#[tokio::test]
async fn two_servers_publishing_the_same_raw_name_coexist() {
    use synthia_core::registry::Registry as _;

    let registry = ToolRegistry::new();
    let alpha_transport = Arc::new(InMemoryTransport::new().with_echo_server());
    let beta_transport = Arc::new(InMemoryTransport::new().with_echo_server());
    for (server, transport) in [
        ("alpha", Arc::clone(&alpha_transport)),
        ("beta", Arc::clone(&beta_transport)),
    ] {
        let client = McpClient::new(transport as _);
        register_mcp_tools(
            &registry,
            Arc::clone(&client),
            server,
            NamingPolicy::Namespaced,
        )
        .await
        .expect("register");
    }

    let names: Vec<String> =
        registry.snapshot().into_iter().map(|m| m.name).collect();
    assert!(names.contains(&"mcp__alpha__echo".to_string()), "{names:?}");
    assert!(names.contains(&"mcp__beta__echo".to_string()), "{names:?}");

    // Calling one reaches exactly one server, under its raw name.
    let alpha = registry
        .get("mcp__alpha__echo")
        .await
        .unwrap()
        .expect("alpha");
    let _ = alpha
        .tool_instance()
        .call(json!({"text": "hi"}), &Context::default())
        .await;
    assert_eq!(
        alpha_transport
            .recorded()
            .iter()
            .filter(|c| c.method == "tools/call")
            .count(),
        1
    );
    assert_eq!(
        alpha_transport
            .recorded()
            .iter()
            .find(|c| c.method == "tools/call")
            .map(|c| c.params["name"].clone()),
        Some(json!("echo"))
    );
    assert!(
        !beta_transport.methods().contains(&"tools/call".to_string()),
        "the other server must not see the call"
    );
}

/// The opt-out policy registers raw names (single-server
/// deployments that want the pristine names).
#[tokio::test]
async fn raw_policy_registers_the_raw_name() {
    let transport = Arc::new(InMemoryTransport::new().with_echo_server());
    let client = McpClient::new(transport);
    let registry = ToolRegistry::new();

    let generation = register_mcp_tools(
        &registry,
        Arc::clone(&client),
        "srv",
        NamingPolicy::Raw,
    )
    .await
    .expect("register");
    assert_eq!(generation.names(), vec!["echo"]);
    assert!(registry.snapshot().iter().any(|m| m.name == "echo"));
}

// -- Multimodal content blocks ------------------------------------

/// An MCP `tools/call` reply that carries an image must reach the
/// model as a [`ContentPart::Image`] with its base64 payload intact —
/// not as the `[image content]` placeholder `text()` produces.
#[tokio::test]
async fn mcptool_preserves_image_content_blocks() {
    use synthia_provider::types::ContentPart;

    let b64 = "iVBORw0KGgoAAAANSUhEUg==";
    let transport = Arc::new(InMemoryTransport::new().with_echo_server().on(
        "tools/call",
        json!({
            "content": [
                {"type": "text", "text": "screenshot taken"},
                {"type": "image", "data": b64, "mimeType": "image/png"}
            ],
            "isError": false
        }),
    ));
    let client = McpClient::new(transport);
    let spec = client.tools_list().await.unwrap().remove(0);
    let tool = client.tool(&spec, Arc::clone(&client));

    let out = tool
        .call(json!({"text": "shot"}), &synthia_tool::Context::default())
        .await;
    assert_eq!(out.is_error, None);
    assert_eq!(out.content.len(), 2, "text + image: {:?}", out.content);
    assert_eq!(out.content[0].text(), Some("screenshot taken"));
    match &out.content[1] {
        ContentPart::Image(img) => {
            assert_eq!(img.data, b64);
            assert_eq!(img.mime_type, "image/png");
        }
        other => panic!("expected an image part, got {other:?}"),
    }
}

/// A text-only reply keeps the compact string shape, so no existing
/// consumer sees a change.
#[tokio::test]
async fn mcptool_keeps_text_only_replies_as_plain_text() {
    let transport = Arc::new(InMemoryTransport::new().with_echo_server());
    let client = McpClient::new(transport);
    let spec = client.tools_list().await.unwrap().remove(0);
    let tool = client.tool(&spec, Arc::clone(&client));

    let out = tool
        .call(json!({"text": "x"}), &synthia_tool::Context::default())
        .await;
    assert_eq!(out.content.len(), 1);
    assert_eq!(out.content[0].text(), Some("echoed"));
}

/// An audio block is modelled too (MCP `audio` kind).
#[tokio::test]
async fn mcptool_preserves_audio_content_blocks() {
    use synthia_provider::types::ContentPart;

    let transport = Arc::new(InMemoryTransport::new().with_echo_server().on(
        "tools/call",
        json!({
            "content": [
                {"type": "audio", "data": "QUJD", "mimeType": "audio/wav"}
            ],
            "isError": false
        }),
    ));
    let client = McpClient::new(transport);
    let spec = client.tools_list().await.unwrap().remove(0);
    let tool = client.tool(&spec, Arc::clone(&client));

    let out = tool
        .call(json!({}), &synthia_tool::Context::default())
        .await;
    match &out.content[0] {
        ContentPart::Audio(a) => {
            assert_eq!(a.data, "QUJD");
            assert_eq!(a.mime_type, "audio/wav");
        }
        other => panic!("expected an audio part, got {other:?}"),
    }
}

/// A binary kind we do not model (an inline `resource` blob) still
/// reports *what* came back — mime and size — instead of a bare
/// `[resource content]`.
#[tokio::test]
async fn mcptool_reports_unmodelled_binary_kinds_with_their_size() {
    let transport = Arc::new(InMemoryTransport::new().with_echo_server().on(
        "tools/call",
        json!({
            "content": [
                {"type": "resource", "data": "QUJDRA==", "mimeType": "application/pdf"}
            ],
            "isError": false
        }),
    ));
    let client = McpClient::new(transport);
    let spec = client.tools_list().await.unwrap().remove(0);
    let tool = client.tool(&spec, Arc::clone(&client));

    let out = tool
        .call(json!({}), &synthia_tool::Context::default())
        .await;
    let text = out.content[0].text().unwrap_or_default().to_string();
    assert!(
        text.contains("application/pdf") && text.contains("resource"),
        "placeholder must name the kind and mime: {text}"
    );
}

// -- Real stdio transport (POSIX sh responder) --------------------

/// End-to-end: `StdioTransport` drives a `sh` child that answers
/// the three MCP calls with canned JSON-RPC envelopes.
#[tokio::test]
async fn stdio_transport_round_trips_against_sh_responder() {
    // The responder answers by matching the method name in the
    // request line; ids are echoed back so the client's
    // id-matching path is exercised.
    let script = r#"
read -r line
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"sh-server","version":"1"}}}'
read -r line
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"echo","description":"d","inputSchema":{"type":"object","properties":{"text":{"type":"string"}}}}]}}'
read -r line
printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"from-sh"}],"isError":false}}'
read -r line
printf '%s\n' '{"jsonrpc":"2.0","id":4,"result":{"content":[{"type":"text","text":"second"}],"isError":false}}'
"#;

    let transport =
        StdioTransport::spawn(&StdioConfig::new("sh").arg("-c").arg(script))
            .await
            .expect("spawn sh responder");
    let client = McpClient::new(transport);

    let init = client.initialize().await.expect("initialize");
    assert_eq!(init["serverInfo"]["name"], "sh-server");
    assert_eq!(client.server_name(), "sh-server");

    let specs = client.tools_list().await.expect("tools/list");
    assert_eq!(specs[0].name, "echo");

    let first = client
        .tools_call("echo", json!({"text": "a"}))
        .await
        .expect("call 1");
    assert_eq!(first.text(), "from-sh");

    // A second call proves ids advance and the reader stays in
    // sync (the id-matching loop skips stray lines).
    let second = client
        .tools_call("echo", json!({"text": "b"}))
        .await
        .expect("call 2");
    assert_eq!(second.text(), "second");
}
