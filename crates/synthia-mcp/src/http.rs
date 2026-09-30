//! [`StreamableHttpTransport`] — MCP streamable-http transport.
//!
//! One JSON-RPC message per HTTP POST to the MCP endpoint,
//! exactly as the 2025-06-18 streamable-http revision
//! specifies: the client asks for `application/json,
//! text/event-stream`, the server may answer with either a plain
//! JSON body or a single-event SSE stream carrying the response.
//! Notifications POST the same way and expect no body.
//!
//! A server-assigned session id (`Mcp-Session-Id` response
//! header) is captured on the first response and replayed on
//! every later request, which is what lets a stateless HTTP
//! endpoint keep one MCP session across calls.
//!
//! This transport deliberately does NOT open the optional
//! server→client GET stream: server-initiated notifications
//! (e.g. `tools/list_changed`) are surfaced by the owner through
//! [`crate::McpSupervisor::notify_list_changed`] instead of a
//! background reader.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use reqwest::{
    StatusCode,
    header::{ACCEPT, CONTENT_TYPE},
};
use serde_json::Value;
use synthia_core::Error;

use crate::{
    MCP_PROTOCOL_VERSION,
    McpTransport,
    SharedTransport,
    notification_envelope,
    request_envelope,
    supervisor::TransportFactory,
};

/// Per-request HTTP timeout (connect + response).
pub const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Header carrying the server-assigned MCP session id.
const SESSION_HEADER: &str = "Mcp-Session-Id";

/// `Accept` value required by the streamable-http revision.
const ACCEPT_VALUE: &str = "application/json, text/event-stream";

/// MCP over HTTP POST (streamable-http revision).
pub struct StreamableHttpTransport {
    endpoint: String,
    client: reqwest::Client,
    /// Session id assigned by the server (`initialize`
    /// response), replayed on subsequent requests.
    session: parking_lot::Mutex<Option<String>>,
}

impl StreamableHttpTransport {
    /// Build a transport against `endpoint` — the MCP HTTP URL,
    /// e.g. `http://127.0.0.1:8080/mcp`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, Error> {
        let client = reqwest::Client::builder()
            .timeout(DEFAULT_HTTP_TIMEOUT)
            .build()
            .map_err(|e| Error::ToolExecution {
                message: format!("failed to build HTTP client: {e}"),
            })?;
        Ok(Self {
            endpoint: endpoint.into(),
            client,
            session: parking_lot::Mutex::new(None),
        })
    }

    /// The server-assigned session id, once seen.
    #[must_use]
    pub fn session_id(&self) -> Option<String> {
        self.session.lock().clone()
    }

    /// POST one JSON-RPC envelope, returning the raw response.
    async fn post(&self, envelope: &Value) -> Result<reqwest::Response, Error> {
        let mut request = self
            .client
            .post(&self.endpoint)
            .header(ACCEPT, ACCEPT_VALUE)
            .header("MCP-Protocol-Version", MCP_PROTOCOL_VERSION)
            .json(envelope);
        if let Some(session) = self.session.lock().clone() {
            request = request.header(SESSION_HEADER, session);
        }
        request.send().await.map_err(|e| {
            if e.is_timeout() {
                Error::Timeout {
                    message: format!("MCP HTTP request timed out: {e}"),
                }
            } else {
                Error::ToolExecution {
                    message: format!("MCP HTTP request failed: {e}"),
                }
            }
        })
    }

    /// Read one response: capture the session header, then parse
    /// the body as JSON or as a single-event SSE stream.
    async fn read_payload(
        &self,
        response: reqwest::Response,
    ) -> Result<Value, Error> {
        if let Some(session) = response
            .headers()
            .get(SESSION_HEADER)
            .and_then(|v| v.to_str().ok())
        {
            *self.session.lock() = Some(session.to_string());
        }
        let status = response.status();
        let is_sse = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.contains("text/event-stream"));
        let body = response.text().await.map_err(|e| Error::ToolExecution {
            message: format!("MCP HTTP body read failed: {e}"),
        })?;
        if !status.is_success() {
            return Err(status_error(status, &body));
        }
        let payload = if is_sse {
            first_sse_data(&body).ok_or_else(|| Error::Parse {
                message: "MCP HTTP stream carried no data event".to_string(),
            })?
        } else {
            body
        };
        serde_json::from_str(&payload).map_err(|e| Error::Parse {
            message: format!("MCP HTTP response parse: {e}"),
        })
    }
}

#[async_trait]
impl McpTransport for StreamableHttpTransport {
    async fn request(
        &self,
        id: u64,
        method: &str,
        params: Value,
    ) -> Result<Value, Error> {
        let envelope = request_envelope(id, method, &params);
        let response = self.post(&envelope).await?;
        let payload = self.read_payload(response).await?;
        crate::unwrap_response(payload)
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), Error> {
        let envelope = notification_envelope(method, &params);
        let response = self.post(&envelope).await?;
        if let Some(session) = response
            .headers()
            .get(SESSION_HEADER)
            .and_then(|v| v.to_str().ok())
        {
            *self.session.lock() = Some(session.to_string());
        }
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let body = response.text().await.unwrap_or_default();
        Err(status_error(status, &body))
    }

    fn describe(&self) -> String {
        format!("http: {}", self.endpoint)
    }
}

impl std::fmt::Debug for StreamableHttpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamableHttpTransport")
            .field("endpoint", &self.endpoint)
            .field("session", &self.session_id())
            .finish()
    }
}

/// Typed error for a non-success HTTP status.
fn status_error(status: StatusCode, body: &str) -> Error {
    let message = body.trim();
    let message = if message.is_empty() {
        "<empty body>"
    } else {
        message
    };
    match status {
        StatusCode::UNAUTHORIZED => Error::Unauthorized {
            message: format!(
                "MCP HTTP endpoint rejected the credentials: {message}"
            ),
        },
        StatusCode::FORBIDDEN => Error::Forbidden {
            message: format!(
                "MCP HTTP endpoint refused the request: {message}"
            ),
        },
        _ => Error::RequestFailed {
            status: status.as_u16(),
            message: message.to_string(),
        },
    }
}

/// First SSE event's `data` payload (multi-line `data:` fields
/// join with `\n`, per the event-stream grammar).
fn first_sse_data(body: &str) -> Option<String> {
    let mut data: Vec<&str> = Vec::new();
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            data.push(rest.strip_prefix(' ').unwrap_or(rest));
        } else if line.is_empty() && !data.is_empty() {
            break;
        }
    }
    if data.is_empty() {
        return None;
    }
    Some(data.join("\n"))
}

/// [`TransportFactory`] for HTTP endpoints: every `connect`
/// builds a fresh transport (and therefore a fresh session).
pub struct HttpTransportFactory {
    endpoint: String,
}

impl HttpTransportFactory {
    /// Factory targeting `endpoint`.
    #[must_use]
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }

    /// The configured endpoint.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

#[async_trait]
impl TransportFactory for HttpTransportFactory {
    async fn connect(&self) -> Result<SharedTransport, Error> {
        let transport = StreamableHttpTransport::new(self.endpoint.clone())?;
        Ok(Arc::new(transport))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_data_extracts_first_event() {
        let body = "event: message\ndata: {\"a\":1}\n\n\
                    data: {\"b\":2}\n\n";
        assert_eq!(first_sse_data(body).as_deref(), Some("{\"a\":1}"));
    }

    #[test]
    fn sse_data_joins_multiline_payloads() {
        // Two `data:` fields in one event; the event-stream
        // grammar joins them with a newline.
        let body = "data: {\"a\":\ndata: 1}\n\n";
        assert_eq!(first_sse_data(body).as_deref(), Some("{\"a\":\n1}"));
    }

    #[test]
    fn sse_data_missing_is_none() {
        assert_eq!(first_sse_data(""), None);
        assert_eq!(first_sse_data("event: ping\n\n"), None);
    }

    #[test]
    fn status_error_maps_auth_failures() {
        assert!(matches!(
            status_error(StatusCode::UNAUTHORIZED, "nope"),
            Error::Unauthorized { .. }
        ));
        assert!(matches!(
            status_error(StatusCode::FORBIDDEN, "nope"),
            Error::Forbidden { .. }
        ));
        assert!(matches!(
            status_error(StatusCode::BAD_GATEWAY, "upstream down"),
            Error::RequestFailed { status: 502, .. }
        ));
    }
}
