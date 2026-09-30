//! [`McpClient`] — the MCP protocol layer (initialize /
//! tools/list / tools/call) plus [`register_mcp_tools`], which
//! publishes remote tools into a local
//! [`synthia_tool::ToolRegistry`].

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use synthia_core::{Error, registry::Registry as _};
use synthia_provider::types::{
    AudioContent,
    ContentPart,
    ImageContent,
    TextContent,
};
use synthia_tool::{ToolEntry, ToolExposure, ToolRegistry};

use crate::{
    MCP_PROTOCOL_VERSION,
    SharedTransport,
    naming::NamingPolicy,
    tool::McpTool,
};

/// One tool advertised by an MCP server (`tools/list` entry).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpToolSpec {
    /// Remote tool name (becomes the local tool name).
    pub name: String,
    /// Model-facing description.
    #[serde(default)]
    pub description: String,
    /// JSON Schema for `arguments`.
    #[serde(default = "empty_object_schema", rename = "inputSchema")]
    pub input_schema: Value,
}

fn empty_object_schema() -> Value {
    json!({"type": "object"})
}

/// The result of one `tools/call`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpCallResult {
    /// Content blocks returned by the server.
    #[serde(default)]
    pub content: Vec<McpContentBlock>,
    /// MCP's `isError` flag (tool-level failure, not transport).
    #[serde(default, rename = "isError")]
    pub is_error: bool,
}

/// One content block in a `tools/call` result. Only the textual
/// shape is rendered today; other kinds are preserved so a
/// consumer can inspect them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpContentBlock {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub text: Option<String>,
    /// Any extra fields the server attached.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

impl McpCallResult {
    /// Flatten every textual block into one string (newline
    /// separated). Non-text blocks contribute a short placeholder
    /// so the model knows something was elided.
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::new();
        for block in &self.content {
            if let Some(text) = &block.text {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(text);
            } else if !out.is_empty() {
                out.push('\n');
                out.push_str(&format!("[{} content]", block.kind));
            } else {
                out.push_str(&format!("[{} content]", block.kind));
            }
        }
        out
    }

    /// Map every content block to a provider [`ContentPart`],
    /// **preserving** the multimodal payloads [`McpCallResult::text`]
    /// flattens into a placeholder.
    ///
    /// MCP servers return `{"type":"image","data":"<base64>",
    /// "mimeType":"image/png"}` (screenshot, chart, OCR and
    /// image-generation servers all do); flattening that to
    /// `[image content]` throws the bytes away before the model ever
    /// sees them. This method keeps them as [`ContentPart::Image`] /
    /// [`ContentPart::Audio`] so a multimodal provider receives the
    /// real payload.
    ///
    /// A block that is neither text nor a recognised binary shape
    /// still contributes its `[<kind> content]` placeholder, so no
    /// information about *what* came back is lost.
    #[must_use]
    pub fn parts(&self) -> Vec<ContentPart> {
        self.content.iter().map(block_to_part).collect()
    }
}

/// Project one MCP content block onto the provider wire vocabulary.
///
/// Binary kinds are checked first: an `image` block carries
/// `data` + `mimeType` and (per the MCP spec) no `text`, so testing
/// the payload is more reliable than testing the kind string alone.
fn block_to_part(block: &McpContentBlock) -> ContentPart {
    let data = block.extra.get("data").and_then(Value::as_str);
    let mime = block.extra.get("mimeType").and_then(Value::as_str);
    if let (Some(data), Some(mime_type)) = (data, mime) {
        let data = data.to_string();
        let mime_type = mime_type.to_string();
        return match block.kind.as_str() {
            "image" => ContentPart::Image(ImageContent {
                data,
                mime_type,
                detail: None,
            }),
            "audio" => ContentPart::Audio(AudioContent {
                data,
                mime_type,
                format: None,
            }),
            // A binary kind we do not model yet (e.g. `resource`
            // with an inline blob): keep the payload reachable
            // rather than dropping it on the floor.
            other => ContentPart::Text(TextContent {
                text: format!(
                    "[{other} content: {mime_type}, {} bytes base64]",
                    data.len()
                ),
                cache_control: None,
            }),
        };
    }
    if let Some(text) = &block.text {
        return ContentPart::Text(TextContent {
            text: text.clone(),
            cache_control: None,
        });
    }
    ContentPart::Text(TextContent {
        text: format!("[{} content]", block.kind),
        cache_control: None,
    })
}

/// MCP client bound to one transport.
pub struct McpClient {
    transport: SharedTransport,
    next_id: AtomicU64,
    /// Server identity captured at `initialize` (for logs).
    server_name: parking_lot::RwLock<String>,
}

impl McpClient {
    /// Build a client over `transport`. Call [`Self::initialize`]
    /// before any other method (MCP requires the handshake).
    pub fn new(transport: SharedTransport) -> Arc<Self> {
        Arc::new(Self {
            transport,
            next_id: AtomicU64::new(1),
            server_name: parking_lot::RwLock::new(String::new()),
        })
    }

    /// The transport's description (`"stdio: npx -y …"`).
    #[must_use]
    pub fn describe(&self) -> String {
        self.transport.describe()
    }

    /// Server name reported at handshake (empty before
    /// `initialize`).
    #[must_use]
    pub fn server_name(&self) -> String {
        self.server_name.read().clone()
    }

    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Perform the MCP handshake, then send
    /// `notifications/initialized`.
    pub async fn initialize(&self) -> Result<Value, Error> {
        let params = json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "synthia",
                "version": env!("CARGO_PKG_VERSION"),
            },
        });
        let id = self.next_id();
        let result = self
            .transport
            .request(id, "initialize", params.clone())
            .await?;
        if let Some(name) = result
            .get("serverInfo")
            .and_then(|s| s.get("name"))
            .and_then(Value::as_str)
        {
            *self.server_name.write() = name.to_string();
        }
        // The spec requires this notification after initialize.
        self.transport
            .notify("notifications/initialized", json!({}))
            .await?;
        Ok(result)
    }

    /// List the server's tools.
    pub async fn tools_list(&self) -> Result<Vec<McpToolSpec>, Error> {
        let id = self.next_id();
        let result =
            self.transport.request(id, "tools/list", json!({})).await?;
        let tools = result.get("tools").cloned().unwrap_or(json!([]));
        serde_json::from_value(tools).map_err(|e| Error::ToolExecution {
            message: format!("MCP tools/list shape: {e}"),
        })
    }

    /// Call a remote tool.
    pub async fn tools_call(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<McpCallResult, Error> {
        let id = self.next_id();
        let params = json!({ "name": name, "arguments": arguments });
        let result = self.transport.request(id, "tools/call", params).await?;
        serde_json::from_value(result).map_err(|e| Error::ToolExecution {
            message: format!("MCP tools/call shape: {e}"),
        })
    }

    /// Build one [`McpTool`] handle for `spec`.
    #[must_use]
    pub fn tool(&self, spec: &McpToolSpec, client: Arc<McpClient>) -> McpTool {
        McpTool::new(spec.clone(), client)
    }
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("transport", &self.transport.describe())
            .field("server_name", &self.server_name.read())
            .finish()
    }
}

/// The registry entries one MCP server generation owns.
///
/// A *generation* is the set of tools published from one
/// `tools/list` fetch. Swapping generations (re-sync,
/// reconnect) unregisters the previous set and registers the
/// next as one unit — a swap either lands whole or leaves the
/// previous generation live, never a partial mix.
#[derive(Clone, Default)]
pub struct McpToolGeneration {
    entries: Vec<(String, ToolEntry)>,
}

impl std::fmt::Debug for McpToolGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpToolGeneration")
            .field("names", &self.names())
            .finish()
    }
}

impl McpToolGeneration {
    /// The public names this generation registered, in
    /// `tools/list` order.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.entries.iter().map(|(name, _)| name.clone()).collect()
    }

    /// Number of tools in this generation.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether this generation registered no tools.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove this generation's tools from `registry`.
    ///
    /// Unregistering an absent name is a no-op, so calling this
    /// twice (or after a foreign takeover) is safe.
    pub fn unregister_from(&self, registry: &ToolRegistry) {
        for (name, _) in &self.entries {
            registry.unregister_by_name(name);
        }
    }

    /// Re-register this generation's original entries (rollback
    /// path for a failed swap).
    fn restore_into(&self, registry: &ToolRegistry) {
        for (name, entry) in &self.entries {
            if !registry.register_entry(entry.clone()) {
                tracing::warn!(
                    target: "synthia.mcp",
                    tool = %name,
                    "could not restore previous MCP tool registration"
                );
            }
        }
    }
}

/// Fetch the server's tool list and swap it into `registry` as
/// one generation (dsh two-phase parity).
///
/// Phase 1 (fetch): `tools/list`, derive public names under
/// `policy`. A duplicate public name or a transport failure
/// rejects with `previous` untouched — the live registrations
/// keep serving.
///
/// Phase 2 (swap): unregister `previous`, register the new
/// generation. A name collision with a foreign registration
/// rolls the swap back wholesale (the partial new set is removed
/// and `previous` is restored), so the model never sees a
/// partial generation.
///
/// A name the previous generation owned keeps the `hidden` /
/// `exposure` state the registry carried for it (`carried_visibility`
/// below), so re-syncing does not revoke a deployment's tool policy.
///
/// Returns the new generation; the caller replaces `previous`
/// with it only on `Ok`.
pub async fn sync_mcp_tools(
    registry: &ToolRegistry,
    client: &Arc<McpClient>,
    server: &str,
    policy: NamingPolicy,
    previous: &McpToolGeneration,
) -> Result<McpToolGeneration, Error> {
    let carried = carried_visibility(registry, previous).await;
    let planned = plan_generation(client, server, policy).await?;
    swap_generation(registry, client, planned, previous, &carried)
}

/// The visibility a name already carries in `registry`, captured
/// before the swap replaces its registration.
///
/// An operator's decision is about the **name** — `[tools] hidden =
/// ["mcp__srv__x"]` is applied after boot — and a re-sync must not
/// silently revoke it. Without this capture, every reconnect re-created
/// the generation's entries with registration defaults, so a hidden
/// remote tool came back advertised (and dispatchable) on the next
/// `tools/list_changed` or reconnect: the deployment's policy survived
/// only until the server blinked.
///
/// The existing entry is read through `Registry::get`, which — unlike
/// `snapshot` — deliberately sees hidden tools.
async fn carried_visibility(
    registry: &ToolRegistry,
    previous: &McpToolGeneration,
) -> HashMap<String, (bool, ToolExposure)> {
    let mut carried = HashMap::new();
    for (name, _) in &previous.entries {
        if let Ok(Some(entry)) = registry.get(name).await {
            carried.insert(name.clone(), (entry.is_hidden(), entry.exposure()));
        }
    }
    carried
}

/// Phase 1: fetch specs and derive `(public name, spec)` pairs.
///
/// A server listing the same tool twice (after namespacing)
/// rejects as an invalid list — shadowing within one server is
/// always a server bug.
async fn plan_generation(
    client: &Arc<McpClient>,
    server: &str,
    policy: NamingPolicy,
) -> Result<Vec<(String, McpToolSpec)>, Error> {
    let specs = client.tools_list().await?;
    let mut planned = Vec::with_capacity(specs.len());
    let mut seen = HashSet::new();
    for spec in specs {
        let public = policy.apply(server, &spec.name);
        if !seen.insert(public.clone()) {
            return Err(Error::InvalidItem {
                item: format!(
                    "MCP server `{server}` listed tool `{}` more than once",
                    spec.name
                ),
            });
        }
        planned.push((public, spec));
    }
    Ok(planned)
}

/// Phase 2: retire `previous`, register `planned` — atomically.
///
/// `carried` holds the visibility each re-planned name already had in
/// the registry (see [`carried_visibility`]); a name present in it is
/// re-registered with the operator's `hidden` / `exposure` decision
/// intact instead of the registration defaults.
fn swap_generation(
    registry: &ToolRegistry,
    client: &Arc<McpClient>,
    planned: Vec<(String, McpToolSpec)>,
    previous: &McpToolGeneration,
    carried: &HashMap<String, (bool, ToolExposure)>,
) -> Result<McpToolGeneration, Error> {
    // Names still owned by `previous` are ours to replace; any
    // OTHER occupant is a foreign squatter on this server's
    // namespace and aborts the swap before anything changes.
    let owned: Vec<String> = previous.names();
    if let Some(name) = foreign_squatter(registry, &planned, &owned) {
        return Err(Error::AlreadyExists {
            item: format!(
                "MCP tool name `{name}` is already registered by another \
                 source; refusing to shadow it"
            ),
        });
    }
    previous.unregister_from(registry);
    let mut entries = Vec::with_capacity(planned.len());
    for (public, spec) in planned {
        let tool =
            McpTool::with_public_name(spec, Arc::clone(client), public.clone());
        let mut entry = ToolEntry::new(Arc::new(tool));
        if let Some((hidden, exposure)) = carried.get(&public) {
            entry = entry.with_is_hidden(*hidden).with_exposure(*exposure);
        }
        if !registry.register_entry(entry.clone()) {
            // Unreachable for public-API registrants (checked
            // above); kept for the registry's core-tool
            // immutability guard.
            rollback(registry, &entries, previous);
            return Err(Error::AlreadyExists {
                item: format!("MCP tool name `{public}` was refused"),
            });
        }
        entries.push((public, entry));
    }
    tracing::info!(
        target: "synthia.mcp",
        server = %client.describe(),
        tools = entries.len(),
        "registered MCP tool generation"
    );
    Ok(McpToolGeneration { entries })
}

/// The first planned name already held by a registration this
/// server does not own, if any.
fn foreign_squatter(
    registry: &ToolRegistry,
    planned: &[(String, McpToolSpec)],
    owned: &[String],
) -> Option<String> {
    let occupied: Vec<String> =
        registry.snapshot().into_iter().map(|m| m.name).collect();
    planned
        .iter()
        .map(|(public, _)| public)
        .find(|public| occupied.contains(public) && !owned.contains(public))
        .cloned()
}

/// Undo a partially registered swap: drop the new entries and
/// bring `previous` back.
fn rollback(
    registry: &ToolRegistry,
    registered: &[(String, ToolEntry)],
    previous: &McpToolGeneration,
) {
    for (name, _) in registered {
        registry.unregister_by_name(name);
    }
    previous.restore_into(registry);
}

/// Publish every tool a server advertises into `registry` as a
/// fresh generation (namespaced by default).
///
/// Convenience wrapper for callers without a supervisor: fetch
/// `tools/list`, derive public names under `policy`, register
/// them, and return the owning generation. Each remote tool
/// becomes a plain [`synthia_tool::Tool`], so it flows through
/// the same guard pipeline, registry dispatch, and metrics as a
/// builtin. Keep the returned generation (or leave the tools
/// registered) — dropping it does NOT unregister.
pub async fn register_mcp_tools(
    registry: &ToolRegistry,
    client: Arc<McpClient>,
    server: &str,
    policy: NamingPolicy,
) -> Result<McpToolGeneration, Error> {
    sync_mcp_tools(
        registry,
        &client,
        server,
        policy,
        &McpToolGeneration::default(),
    )
    .await
}
