//! `StreamableHttpTransport` against a real local HTTP server
//! (wiremock): JSON bodies, SSE-framed responses, the
//! `Mcp-Session-Id` handshake, and status-error mapping.

use serde_json::{Value, json};
use synthia_core::Error;
use synthia_mcp::{
    HttpTransportFactory,
    MCP_PROTOCOL_VERSION,
    McpClient,
    McpTransport,
    StreamableHttpTransport,
    TransportFactory,
};
use wiremock::{
    Mock,
    MockServer,
    ResponseTemplate,
    matchers::{body_partial_json, header, method, path},
};

/// A shared transport for `server` plus the client bound to it.
fn client_for(
    server: &MockServer,
) -> (
    std::sync::Arc<StreamableHttpTransport>,
    std::sync::Arc<McpClient>,
) {
    let transport = std::sync::Arc::new(
        StreamableHttpTransport::new(format!("{}/mcp", server.uri()))
            .expect("build transport"),
    );
    let shared: std::sync::Arc<dyn McpTransport> =
        std::sync::Arc::clone(&transport) as std::sync::Arc<dyn McpTransport>;
    (transport, McpClient::new(shared))
}

#[tokio::test]
async fn initialize_over_json_sends_the_streamable_http_contract() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/mcp"))
        .and(body_partial_json(json!({"method": "initialize"})))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Mcp-Session-Id", "sess-42")
                .set_body_json(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "protocolVersion": MCP_PROTOCOL_VERSION,
                        "capabilities": {},
                        "serverInfo": {"name": "http-server", "version": "1"}
                    }
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    // The mandatory post-handshake notification lands as a
    // second POST; it must not be answered with the mock above.
    Mock::given(method("POST"))
        .and(path("/mcp"))
        .and(body_partial_json(
            json!({"method": "notifications/initialized"}),
        ))
        .respond_with(ResponseTemplate::new(202))
        .expect(1)
        .mount(&server)
        .await;

    let (transport, client) = client_for(&server);
    let result = client.initialize().await.expect("initialize");

    assert_eq!(result["serverInfo"]["name"], "http-server");
    assert_eq!(client.server_name(), "http-server");
    // The server-assigned session id is captured for later calls.
    assert_eq!(transport.session_id().as_deref(), Some("sess-42"));
    let requests = server.received_requests().await.expect("recorded");
    // `initialize` plus the mandatory `notifications/initialized`.
    assert_eq!(requests.len(), 2);
    let request = &requests[0];
    assert_eq!(
        request.headers.get("accept").and_then(|v| v.to_str().ok()),
        Some("application/json, text/event-stream")
    );
    assert_eq!(
        request
            .headers
            .get("mcp-protocol-version")
            .and_then(|v| v.to_str().ok()),
        Some(MCP_PROTOCOL_VERSION)
    );
    assert!(
        request
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("application/json"))
    );
    // The notification that follows the handshake reaches the
    // server too (a second POST, no response body expected).
    assert!(
        server.received_requests().await.expect("recorded").len() >= 2,
        "initialize must be followed by notifications/initialized"
    );
}

#[tokio::test]
async fn tools_list_parses_an_sse_framed_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/mcp"))
        .and(body_partial_json(json!({"method": "tools/list"})))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "event: message\n\
             data: {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":\
             {\"tools\":[{\"name\":\"echo\",\
             \"description\":\"Echo\",\
             \"inputSchema\":{\"type\":\"object\"}}]}}\n\n",
            "text/event-stream",
        ))
        .expect(1)
        .mount(&server)
        .await;

    let transport =
        StreamableHttpTransport::new(format!("{}/mcp", server.uri()))
            .expect("build transport");
    let result = transport
        .request(7, "tools/list", json!({}))
        .await
        .expect("tools/list over SSE");
    assert_eq!(result["tools"][0]["name"], "echo");
}

#[tokio::test]
async fn session_id_is_replayed_on_later_requests() {
    let server = MockServer::start().await;
    // Only a request carrying the session header matches this
    // mock; a client that forgets it gets a 404.
    Mock::given(method("POST"))
        .and(path("/mcp"))
        .and(body_partial_json(json!({"method": "tools/list"})))
        .and(header("mcp-session-id", "sess-9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"tools": []}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(body_partial_json(json!({"method": "initialize"})))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Mcp-Session-Id", "sess-9")
                .set_body_json(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {"serverInfo": {"name": "http-server"}}
                })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let transport =
        StreamableHttpTransport::new(format!("{}/mcp", server.uri()))
            .expect("build transport");
    let _ = transport.request(1, "initialize", json!({})).await;
    assert_eq!(transport.session_id().as_deref(), Some("sess-9"));
    let result = transport.request(2, "tools/list", json!({})).await;
    assert!(result.is_ok(), "session id must be replayed: {result:?}");
}

#[tokio::test]
async fn notify_posts_a_notification_without_an_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/mcp"))
        .and(body_partial_json(
            json!({"method": "notifications/initialized"}),
        ))
        .respond_with(ResponseTemplate::new(202))
        .expect(1)
        .mount(&server)
        .await;

    let transport =
        StreamableHttpTransport::new(format!("{}/mcp", server.uri()))
            .expect("build transport");
    transport
        .notify("notifications/initialized", json!({}))
        .await
        .expect("notify accepted");

    let requests = server.received_requests().await.expect("recorded");
    let body: Value =
        serde_json::from_slice(&requests[0].body).expect("jsonrpc body");
    assert_eq!(body["method"], "notifications/initialized");
    assert!(body.get("id").is_none(), "{body}");
}

#[tokio::test]
async fn non_success_statuses_map_to_typed_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_partial_json(json!({"method": "tools/list"})))
        .respond_with(ResponseTemplate::new(401).set_body_string("bad token"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(body_partial_json(json!({"method": "tools/call"})))
        .respond_with(
            ResponseTemplate::new(500).set_body_string("upstream down"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let transport =
        StreamableHttpTransport::new(format!("{}/mcp", server.uri()))
            .expect("build transport");
    let err = transport
        .request(1, "tools/list", json!({}))
        .await
        .expect_err("401");
    assert!(matches!(&err, Error::Unauthorized { .. }), "{err:?}");
    assert!(err.to_string().contains("bad token"), "{err}");

    let err = transport
        .request(2, "tools/call", json!({"name": "echo"}))
        .await
        .expect_err("500");
    match err {
        Error::RequestFailed { status, .. } => assert_eq!(status, 500),
        other => panic!("expected RequestFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn factory_builds_a_fresh_transport_per_connect() {
    let factory = HttpTransportFactory::new("http://127.0.0.1:9/mcp");
    assert_eq!(factory.endpoint(), "http://127.0.0.1:9/mcp");
    let first = factory.connect().await.expect("connect");
    let second = factory.connect().await.expect("connect");
    assert!(first.describe().contains("127.0.0.1:9/mcp"));
    assert_eq!(first.describe(), second.describe());
    // Distinct transports: HTTP sessions must not be shared
    // across reconnect attempts.
    assert_ne!(
        std::sync::Arc::as_ptr(&first),
        std::sync::Arc::as_ptr(&second)
    );
}
