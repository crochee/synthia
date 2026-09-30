//! `synthia-mcp-server` — a stdio MCP server that exposes synthia's
//! plugin-brick tools (`read` / `write` / `shell` / `todo` /
//! `web_fetch` / `schedule` / `search` + a `synthia` overview tool) to
//! any MCP client (e.g. the `pi` coding agent).
//!
//! ## Why this crate exists
//!
//! `synthia-server` exposes its management surface over HTTP + an
//! in-process MCP server that lets the deployment's own agents
//! manage themselves. That's the **inbound** path (the agent reaches
//! into its own deployment). This crate is the **outbound** path:
//! any foreign MCP client can spawn
//! `synthia-mcp-server` as a subprocess, send newline-delimited
//! JSON-RPC 2.0 over its stdin, and read the responses on its
//! stdout — without dragging synthia into its own process.
//!
//! The two paths share the same protocol implementation:
//! [`synthia_mcp::server::serve`] drives the request loop over a
//! [`synthia_mcp::LoopbackServer`] inbox, and the same call wires
//! the plugin-brick tools here. A successful call against this
//! binary is therefore also a successful end-to-end proof of the
//! in-process MCP server seam — the only difference is the
//! transport: here it is newline-delimited JSON over stdio, where
//! `synthia-server` registers one in its own address space.
//!
//! ## Wire
//!
//! ```text
//! → {"jsonrpc":"2.0","id":1,"method":"initialize","params":{…}}
//! ← {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":…}}
//! → {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
//! ← {"jsonrpc":"2.0","id":2,"result":{"tools":[…]}}
//! → {"jsonrpc":"2.0","id":3,"method":"tools/call",
//!    "params":{"name":"read","arguments":{"file_path":"…"}}}
//! ← {"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text",
//!    "text":"…"}],"isError":false}}
//! ```
//!
//! Every request gets a response. stderr carries the operator logs;
//! stdout is reserved for JSON-RPC. The server exits when the
//! client side closes its stdin (EOF).
//!
//! ## Runtime
//!
//! The library surface (`build`, `tools`) is runtime-neutral —
//! `serve` returns a future any executor drives. The `main`
//! binary pins tokio because it owns a stdin reader that must be
//! spawned; consumers driving `serve` themselves do not inherit
//! that dependency.

use std::{path::PathBuf, sync::Arc};

use futures::future::BoxFuture;
use serde_json::{Value, json};
use synthia_core::SharedClock;
use synthia_mcp::ServerTool;
use synthia_tool::{Context, Tool, ToolOutput};
use synthia_tool_read::ReadTool;
use synthia_tool_scheduler::SchedulerTool;
use synthia_tool_search::SearchTool;
use synthia_tool_shell::ShellTool;
use synthia_tool_todo::TodoWriteTool;
use synthia_tool_web::WebFetchTool;
use synthia_tool_write::WriteTool;

pub mod stdio;

pub use stdio::run;

/// Re-export so `main.rs` (and any consumer writing their own
/// binary) does not have to reach into `ServerConfig`'s
/// implementation.
pub use crate::config::parse_argv;

mod config {
    use std::path::PathBuf;

    /// CLI args parsed by [`crate::main`] (see `Args` in
    /// `main.rs`). The library exposes them through
    /// [`parse_argv`] so a consumer writing their own `main` (or a
    /// test harness) does not duplicate the parsing.
    #[derive(Debug, Clone)]
    pub struct ServerConfig {
        /// Workspace root the plugin-brick tools are confined to.
        pub workspace_root: PathBuf,
        /// Server name the JSON-RPC handshake reports back.
        pub server_name: String,
    }

    impl ServerConfig {
        /// Parse argv: `[--workspace <PATH>] [--name <NAME>]`.
        ///
        /// `--workspace` defaults to the current directory;
        /// `--name` defaults to `synthia`.
        pub fn parse_argv(
            argv: impl IntoIterator<Item = String>,
        ) -> Result<Self, String> {
            let mut workspace_root: Option<PathBuf> = None;
            let mut server_name: Option<String> = None;
            let mut iter = argv.into_iter();
            while let Some(arg) = iter.next() {
                match arg.as_str() {
                    "--workspace" | "-w" => {
                        let value = iter.next().ok_or_else(|| {
                            format!("{arg} requires a PATH argument")
                        })?;
                        workspace_root = Some(PathBuf::from(value));
                    }
                    "--name" | "-n" => {
                        let value = iter.next().ok_or_else(|| {
                            format!("{arg} requires a NAME argument")
                        })?;
                        server_name = Some(value);
                    }
                    "--help" | "-h" => {
                        return Err(usage());
                    }
                    other
                        if other.starts_with("--")
                            || other.starts_with('-') =>
                    {
                        return Err(format!(
                            "unknown flag `{other}`\n\n{}",
                            usage()
                        ));
                    }
                    other => {
                        return Err(format!(
                            "unexpected positional argument `{other}`\n\n{}",
                            usage()
                        ));
                    }
                }
            }
            let workspace_root = workspace_root
                .map(canonicalize)
                .transpose()
                .map_err(|e| format!("--workspace: {e}"))?
                .unwrap_or_else(|| {
                    std::env::current_dir()
                        .unwrap_or_else(|_| PathBuf::from("."))
                });
            if !workspace_root.is_dir() {
                return Err(format!(
                    "--workspace `{}` is not a directory",
                    workspace_root.display()
                ));
            }
            Ok(Self {
                workspace_root,
                server_name: server_name
                    .unwrap_or_else(|| "synthia".to_string()),
            })
        }
    }

    /// Best-effort canonicalize: a non-existent path is reported
    /// as `No such file or directory` and the caller surfaces it.
    fn canonicalize(path: PathBuf) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(&path).or_else(|_| {
            if path.exists() {
                Ok(path)
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("path does not exist: {}", path.display()),
                ))
            }
        })
    }

    fn usage() -> String {
        "Usage: synthia-mcp-server [--workspace <PATH>] [--name <NAME>]\n\n\
         --workspace  PATH   root directory for file-touching tools \
         (default: $PWD).\n\
         --name       NAME   server name reported in the JSON-RPC \
         handshake (default: synthia)."
            .to_string()
    }

    /// Re-export the parse function for binary code paths that
    /// only need the parse, not the parsed struct.
    pub fn parse_argv(
        argv: impl IntoIterator<Item = String>,
    ) -> Result<ServerConfig, String> {
        ServerConfig::parse_argv(argv)
    }
}

/// Tool surface this server publishes — every plugin brick the
/// facade ships behind a feature, plus one synthetic `synthia`
/// overview tool that returns the registered tool list, the
/// workspace root, and the binary version. Callers that want a
/// smaller set build their own [`ServerSurface`] (filtering the
/// `tools` vector) and pass it to [`stdio::run`].
pub struct ServerSurface {
    /// Workspace root file-touching tools read from / write to.
    pub workspace_root: PathBuf,
    /// Wall clock the schedule and search tools read "now" from.
    pub clock: SharedClock,
}

impl ServerSurface {
    /// Build the surface over `workspace_root` and
    /// [`SharedClock::system()`].
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            clock: SharedClock::system(),
        }
    }

    /// Same, with an injected clock — how tests pin time.
    #[must_use]
    pub fn with_clock(
        workspace_root: impl Into<PathBuf>,
        clock: SharedClock,
    ) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            clock,
        }
    }

    /// The [`Context`] every plugin-brick tool is called with.
    /// One per binary: a stateless process keeps one session id,
    /// the workspace it was started in, and the default truncation
    /// config the [`Context::default`] provides.
    pub fn context(&self, session_id: &str) -> Context {
        Context::new(session_id.to_string(), self.workspace_root.clone())
    }

    /// Assemble the full plugin-brick surface as
    /// [`synthia_mcp::ServerTool`]s the in-process server can serve.
    ///
    /// The returned tools share one [`Context`] (built once over
    /// `session_id`) — every call sees the same workspace root and
    /// truncation config, exactly as a foreign MCP client's
    /// subprocess would experience it.
    pub fn tools(&self, session_id: &str) -> Vec<ServerTool> {
        let ctx = Arc::new(self.context(session_id));
        let read = bridge(Arc::new(ReadTool::new()), Arc::clone(&ctx));
        let write = bridge(Arc::new(WriteTool::new()), Arc::clone(&ctx));
        let shell = bridge(Arc::new(ShellTool::new()), Arc::clone(&ctx));
        let todo = bridge(Arc::new(TodoWriteTool::new()), Arc::clone(&ctx));
        let web = bridge(Arc::new(WebFetchTool::new()), Arc::clone(&ctx));
        // The schedule tool needs a `ScheduleStore` rooted at the
        // workspace's `.synthia/schedules/` directory. Boot reload
        // is best-effort (handled by the tool itself).
        let schedule_root =
            self.workspace_root.join(".synthia").join("schedules");
        std::fs::create_dir_all(&schedule_root).ok();
        let store =
            Arc::new(synthia_scheduler::ScheduleStore::new(&schedule_root));
        let schedule = bridge(
            Arc::new(SchedulerTool::with_clock(store, self.clock.clone())),
            Arc::clone(&ctx),
        );
        // The search tool needs a `Registry`. The empty registry is
        // a valid registry: `register_engine` is the host-side
        // decision, not the tool's.
        let search_registry = Arc::new(synthia_search::Registry::default());
        let search = bridge(
            Arc::new(SearchTool::new(search_registry)),
            Arc::clone(&ctx),
        );
        // The `synthia` overview tool: not a plugin brick — a
        // synthetic tool that lists everything the server exposes.
        let synthia = overview_tool(
            self.workspace_root.clone(),
            env!("CARGO_PKG_VERSION").to_string(),
            session_id.to_string(),
        );
        vec![read, write, shell, todo, web, schedule, search, synthia]
    }

    /// The tools assembled for this surface — exposed for tests
    /// that want to introspect the surface without driving the
    /// stdio pump.
    #[must_use]
    pub fn tools_vec(&self, session_id: &str) -> Vec<ServerTool> {
        self.tools(session_id)
    }
}

/// Adapt an `Arc<dyn Tool>` to the [`synthia_mcp::ServerTool`]
/// handler signature: the call site is exactly the
/// `synthia-mcp` in-process server's contract.
fn bridge(tool: Arc<dyn Tool>, ctx: Arc<Context>) -> ServerTool {
    let name = tool.name().to_string();
    let description = tool.description().to_string();
    let input_schema = tool.parameters();
    let handler = Arc::new(move |input: Value| {
        let tool = Arc::clone(&tool);
        let ctx = Arc::clone(&ctx);
        Box::pin(async move {
            let output = tool.call(input, &ctx).await;
            render_output(output)
        }) as BoxFuture<'static, Result<String, String>>
    });
    ServerTool {
        name,
        description,
        input_schema,
        handler,
    }
}

/// Render a [`ToolOutput`] into the `String` the in-process
/// server writes into a `text` content block.
///
/// The shape mirrors what the JSON wire would have carried: a
/// single text block, plain or the JSON of any structured
/// payload. Errors surface as `Err(_)` so the server marks the
/// call `isError: true` (preserving the failure for the caller)
/// instead of dropping the request.
///
/// Plugin-brick tools return a single-part `Text` content block
/// on the happy path. If a future tool returns multimodal parts
/// (an image, audio), we fall back to JSON so the wire shape is
/// never lossy.
fn render_output(output: ToolOutput) -> Result<String, String> {
    let is_error = output.is_error == Some(true);
    let rendered = match output.content.as_slice() {
        [synthia_provider::types::ContentPart::Text(text)] => text.text.clone(),
        _ => serde_json::to_string_pretty(&output.content)
            .unwrap_or_else(|_| "<unserializable ToolOutput>".to_string()),
    };
    if is_error {
        return Err(rendered);
    }
    Ok(rendered)
}

/// The synthetic `synthia` overview tool: returns the binary
/// version, the workspace root, and the list of every tool the
/// server exposes. It exists so a foreign MCP client has one
/// stable entry point to discover what is wired in this process.
fn overview_tool(
    workspace_root: PathBuf,
    version: String,
    session_id: String,
) -> ServerTool {
    let input_schema = json!({
        "type": "object",
        "properties": {
            "format": {
                "type": "string",
                "enum": ["json", "text"],
                "description": "Output shape. Defaults to `json`."
            }
        },
        "required": [],
    });
    let handler = Arc::new(move |input: Value| {
        let workspace_root = workspace_root.clone();
        let version = version.clone();
        let session_id = session_id.clone();
        Box::pin(async move {
            let format = input
                .get("format")
                .and_then(Value::as_str)
                .unwrap_or("json");
            let tools = vec![
                "read",
                "write",
                "shell",
                "TodoWrite",
                "web_fetch",
                "schedule",
                "search",
                "synthia",
            ];
            let payload = json!({
                "name": "synthia",
                "version": version,
                "session_id": session_id,
                "workspace_root": workspace_root.display().to_string(),
                "tools": tools,
            });
            Ok(match format {
                "text" => {
                    let mut out = format!("synthia-mcp-server {version}\n");
                    out.push_str(&format!("session: {session_id}\n"));
                    out.push_str(&format!(
                        "workspace: {}\n",
                        workspace_root.display()
                    ));
                    out.push_str("tools:\n");
                    for tool in tools {
                        out.push_str(&format!("  - {tool}\n"));
                    }
                    out
                }
                _ => serde_json::to_string_pretty(&payload)
                    .unwrap_or_else(|_| payload.to_string()),
            })
        }) as BoxFuture<'static, Result<String, String>>
    });
    ServerTool {
        name: "synthia".to_string(),
        description: "Overview of this synthia-mcp-server process: \
             version, workspace root, and the full list of tools it \
             exposes. The entry point a foreign MCP client calls once \
             to discover what is wired here."
            .to_string(),
        input_schema,
        handler,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Argument parsing: defaults, overrides, and a couple of
    /// common errors. The same parser backs the binary; pinning it
    /// here means a flag rename does not silently slip into the
    /// binary.
    #[test]
    fn argv_parsing_round_trip() {
        let cfg = parse_argv(Vec::<String>::new()).unwrap();
        assert_eq!(cfg.server_name, "synthia");
        // The workspace defaults to the cwd; we only assert the
        // path is non-empty here — the exact value is environment
        // dependent.
        assert!(!cfg.workspace_root.as_os_str().is_empty());

        let cfg = parse_argv(
            ["--name", "pi", "--workspace", "."]
                .iter()
                .map(|s| s.to_string()),
        )
        .unwrap();
        assert_eq!(cfg.server_name, "pi");

        let err = parse_argv(vec!["--nope".to_string()]).unwrap_err();
        assert!(err.contains("unknown flag"), "{err}");

        let err = parse_argv(vec!["--workspace".to_string()]).unwrap_err();
        assert!(err.contains("requires a PATH"), "{err}");
    }

    /// The `synthia` overview tool returns the seven plugin-brick
    /// tools plus itself; the format knob picks between JSON and
    /// the text rendering.
    #[tokio::test]
    async fn overview_tool_round_trips() {
        let workspace = tempdir_in_cwd();
        let tool = overview_tool(
            workspace.clone(),
            "0.1.0".to_string(),
            "s1".to_string(),
        );
        let json_out = (tool.handler)(json!({})).await.unwrap();
        let parsed: Value = serde_json::from_str(&json_out).unwrap();
        let names: Vec<&str> = parsed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for expected in [
            "read",
            "write",
            "shell",
            "TodoWrite",
            "web_fetch",
            "schedule",
            "search",
            "synthia",
        ] {
            assert!(
                names.contains(&expected),
                "overview missing `{expected}`: {names:?}"
            );
        }

        let text_out = (tool.handler)(json!({"format": "text"})).await.unwrap();
        assert!(text_out.contains("synthia-mcp-server 0.1.0"));
        assert!(text_out.contains("read"));
    }

    /// Every plugin-brick tool builds and the in-process server
    /// exposes all eight tools. This is the structural proof that
    /// the stdio binary will advertise the documented surface.
    #[test]
    fn plugin_brick_tools_assemble() {
        let workspace = tempdir_in_cwd();
        let surface = ServerSurface::new(&workspace);
        let tools = surface.tools("test-session");
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        for expected in [
            "read",
            "write",
            "shell",
            "TodoWrite",
            "web_fetch",
            "schedule",
            "search",
            "synthia",
        ] {
            assert!(
                names.contains(&expected),
                "missing tool `{expected}` in {names:?}"
            );
        }
    }

    /// The `bridge` adapter calls the underlying plugin-brick tool
    /// with the assembled context and surfaces its output as the
    /// MCP call result text.
    #[tokio::test]
    async fn bridge_passes_input_through_to_the_tool() {
        let workspace = tempdir_in_cwd();
        let path = workspace.join("hello.txt");
        std::fs::write(&path, "first line\nsecond line\n").unwrap();

        let ctx = Arc::new(Context::new("test".into(), workspace.clone()));
        let read = Arc::new(ReadTool::new());
        let bridged = bridge(read, Arc::clone(&ctx));
        let result = (bridged.handler)(json!({
            "file_path": path.to_string_lossy(),
            "offset": 1,
            "limit": 1,
        }))
        .await
        .unwrap();
        assert!(result.contains("first line"), "got: {result}");
        assert!(!result.contains("second line"), "got: {result}");
    }

    /// `render_output` flips `is_error: true` into `Err(_)` so
    /// the in-process server marks the call as a tool-level
    /// failure rather than dropping the response.
    #[test]
    fn render_output_errors_are_surfaced_as_err() {
        let ok = ToolOutput::text("hello");
        assert_eq!(render_output(ok).unwrap(), "hello");

        let err = ToolOutput::error("boom");
        let err_str = render_output(err).unwrap_err();
        assert!(err_str.contains("boom"), "{err_str}");
    }

    fn tempdir_in_cwd() -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("synthia-mcp-server-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}

// Re-export the helper that drives the server from the binary's
// `main` so the library surface stays self-contained for tests.
