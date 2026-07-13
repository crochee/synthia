//! Wire-protocol client: speaks HTTP `POST /submission` and WebSocket
//! `GET /ws` against a synthia server that implements Round 6 of
//! `synthia-session-v2.md`.
//!
//! Used by the `--wire <SERVER_URL>` CLI flag in lieu of the
//! in-process REPL.

use std::io::{self, Write};

use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use synthia_protocol::{EventMsg, Op, Submission, SubmissionId};

/// Connect to `server_url` and stream `EventMsg` events to stdout.
///
/// The function POSTs an empty `UserInput` submission to bootstrap the
/// session and then opens the WebSocket to read responses. Events are
/// pretty-printed as JSON lines to stdout. The caller is responsible
/// for wiring stdin; this function exits when the WebSocket closes.
pub async fn run_wire_client(server_url: &str) -> Result<()> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let user_id = format!("cli-{}", whoami_like());

    // Subscribe to /ws first, then send the submission. This avoids
    // missing the turn-started event.
    let ws_url = ws_endpoint(server_url);
    let (_ws_sender, mut ws_receiver) =
        connect_ws(&ws_url, &user_id, &session_id)
            .await
            .with_context(|| format!("connect to {ws_url} failed"))?;

    // Emit a single bootstrap submission so the server has something
    // to dispatch.
    let submission = Submission {
        id: SubmissionId::new(),
        op: Op::UserInput {
            items: vec![synthia_protocol::InputItem::Text {
                text: "(wire client connected)".to_string(),
            }],
            final_output_json_schema: None,
            additional_context: None,
        },
        client_user_message_id: None,
        trace: None,
    };
    post_submission(server_url, &user_id, &session_id, &submission)
        .await
        .with_context(|| "POST /submission failed")?;

    // Read & print events until the server closes the socket.
    let stdout = io::stdout();
    let mut out = stdout.lock();
    while let Some(event_res) = ws_receiver.next().await {
        match event_res {
            Ok(msg) => {
                if let Ok(text) = msg.into_text() {
                    if let Ok(event) = serde_json::from_str::<EventMsg>(&text) {
                        let line =
                            format!("{}\n", serde_json::to_string(&event)?);
                        out.write_all(line.as_bytes())?;
                        out.flush()?;
                    } else {
                        let line = format!("{text}\n");
                        out.write_all(line.as_bytes())?;
                        out.flush()?;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "ws stream ended");
                break;
            }
        }
    }

    Ok(())
}

fn ws_endpoint(server_url: &str) -> String {
    if let Some(rest) = server_url.strip_prefix("https://") {
        format!("wss://{rest}/ws")
    } else if let Some(rest) = server_url.strip_prefix("http://") {
        format!("ws://{rest}/ws")
    } else {
        format!("ws://{server_url}/ws")
    }
}

/// Synthesize a stable per-process CLI identity without pulling in a
/// new dependency.
fn whoami_like() -> String {
    let pid = std::process::id();
    format!("user-{pid}")
}

async fn post_submission(
    server_url: &str,
    user_id: &str,
    session_id: &str,
    submission: &Submission,
) -> Result<()> {
    let url = format!("{}/submission", server_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "user_id": user_id,
        "session_id": session_id,
        "submission": submission,
    });
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("server returned {status}: {text}");
    }
    Ok(())
}

async fn connect_ws(
    url: &str,
    user_id: &str,
    session_id: &str,
) -> Result<(
    futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tokio_tungstenite::tungstenite::Message,
    >,
    futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
)> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let req = url.into_client_request()?;
    let (ws, _resp) = tokio_tungstenite::connect_async(req).await?;
    let (mut sender, receiver) = ws.split();
    let filter = serde_json::json!({
        "user_id": user_id,
        "session_id": session_id,
    });
    sender
        .send(tokio_tungstenite::tungstenite::Message::Text(
            filter.to_string(),
        ))
        .await?;
    Ok((sender, receiver))
}
