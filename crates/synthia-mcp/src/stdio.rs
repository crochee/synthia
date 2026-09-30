//! [`McpTransport`] over a child process's stdin/stdout.
//!
//! Spawns `command args…`, writes newline-delimited JSON-RPC 2.0
//! requests to the child's stdin, and reads one response line
//! per request from its stdout. stderr is inherited so server
//! logs reach the operator.
//!
//! `kill_on_drop(true)` guarantees the child dies with the
//! transport — a dropped client never leaks an MCP server.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use synthia_core::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

use crate::{
    McpTransport,
    SharedTransport,
    StdioConfig,
    notification_envelope,
    request_envelope,
    supervisor::TransportFactory,
};

/// One child process speaking MCP over stdio.
pub struct StdioTransport {
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
    /// Held so the child is killed when the transport drops
    /// (`kill_on_drop` on the spawn). Never read directly.
    #[allow(dead_code)]
    child: Mutex<Child>,
    /// Serialises request/response pairs (stdio is one ordered
    /// stream; interleaving writers would desync the reader).
    /// Per-transport, NOT process-wide.
    io_lock: Mutex<()>,
    describe: String,
}

impl StdioTransport {
    /// Spawn the child described by `config`.
    pub async fn spawn(config: &StdioConfig) -> Result<Arc<Self>, Error> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args)
            .envs(config.env.iter())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            // stderr inherited: server diagnostics stay visible.
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| Error::ToolExecution {
            message: format!(
                "failed to spawn MCP server `{}`: {e}",
                config.command
            ),
        })?;
        let stdin = child.stdin.take().ok_or_else(|| Error::ToolExecution {
            message: "MCP child has no stdin".to_string(),
        })?;
        let stdout =
            child.stdout.take().ok_or_else(|| Error::ToolExecution {
                message: "MCP child has no stdout".to_string(),
            })?;

        // A long argument list (e.g. `sh -c <script>`) must not be
        // dumped into every log line; prefer the caller's label and
        // bound the fallback.
        let describe = config.label.clone().unwrap_or_else(|| {
            const MAX: usize = 60;
            let mut joined =
                format!("stdio: {} {}", config.command, config.args.join(" "));
            if joined.chars().count() > MAX {
                joined = joined.chars().take(MAX).collect::<String>();
                joined.push('…');
            }
            joined
        });
        tracing::debug!(target: "synthia.mcp", server = %describe, "spawned MCP server");

        Ok(Arc::new(Self {
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
            child: Mutex::new(child),
            io_lock: Mutex::new(()),
            describe,
        }))
    }

    /// Write one JSON line to the child and flush.
    async fn write_line(&self, value: &Value) -> Result<(), Error> {
        let mut line =
            serde_json::to_string(value).map_err(|e| Error::ToolExecution {
                message: format!("MCP serialize: {e}"),
            })?;
        line.push('\n');
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(line.as_bytes()).await.map_err(|e| {
            Error::ToolExecution {
                message: format!("MCP write: {e}"),
            }
        })?;
        stdin.flush().await.map_err(|e| Error::ToolExecution {
            message: format!("MCP flush: {e}"),
        })
    }

    /// Read one JSON line from the child.
    async fn read_line(&self) -> Result<Value, Error> {
        let mut buf = String::new();
        let mut stdout = self.stdout.lock().await;
        let n = stdout.read_line(&mut buf).await.map_err(|e| {
            Error::ToolExecution {
                message: format!("MCP read: {e}"),
            }
        })?;
        if n == 0 {
            return Err(Error::ToolExecution {
                message: "MCP server closed stdout".to_string(),
            });
        }
        serde_json::from_str(buf.trim()).map_err(|e| Error::ToolExecution {
            message: format!("MCP parse `{}`: {e}", buf.trim()),
        })
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn request(
        &self,
        id: u64,
        method: &str,
        params: Value,
    ) -> Result<Value, Error> {
        let envelope = request_envelope(id, method, &params);
        let _guard = self.io_lock.lock().await;
        self.write_line(&envelope).await?;
        // Skip server-initiated notifications (no `id`) until the
        // response matching our id arrives.
        loop {
            let line = self.read_line().await?;
            if line.get("id").and_then(Value::as_u64) == Some(id) {
                return crate::unwrap_response(line);
            }
            tracing::debug!(
                target: "synthia.mcp",
                server = %self.describe,
                line = %line,
                "ignoring MCP notification while awaiting response"
            );
        }
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), Error> {
        let envelope = notification_envelope(method, &params);
        let _guard = self.io_lock.lock().await;
        self.write_line(&envelope).await
    }

    fn describe(&self) -> String {
        self.describe.clone()
    }
}

/// [`TransportFactory`] spawning a fresh child process per
/// connection attempt.
///
/// The supervisor needs this for reconnects: a dead child's
/// pipes cannot be reused, so every attempt gets its own
/// process.
pub struct StdioTransportFactory {
    config: StdioConfig,
}

impl StdioTransportFactory {
    /// Factory spawning the child described by `config`.
    #[must_use]
    pub fn new(config: StdioConfig) -> Self {
        Self { config }
    }

    /// The launch configuration.
    #[must_use]
    pub fn config(&self) -> &StdioConfig {
        &self.config
    }
}

#[async_trait]
impl TransportFactory for StdioTransportFactory {
    async fn connect(&self) -> Result<SharedTransport, Error> {
        let transport = StdioTransport::spawn(&self.config).await?;
        Ok(transport as SharedTransport)
    }
}
