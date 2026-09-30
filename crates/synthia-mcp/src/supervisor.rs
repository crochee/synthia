//! [`McpSupervisor`] — generation lifecycle + reconnect for a
//! fleet of MCP servers (dsh `mcp-client` parity).
//!
//! The supervisor owns, per server, the live [`McpClient`] and
//! the [`McpToolGeneration`] currently registered in the tool
//! registry. It never spawns a task and never sleeps: every
//! [`McpSupervisor::tick`] is driven by the caller, who passes
//! `now` (typically `chrono::Utc::now()`) and gets back the
//! health snapshot plus the deadline for the next tick — the
//! same clock-injection discipline as `synthia-scheduler`.
//!
//! ## Outage accounting
//!
//! A server that stays connected is healthy. When its transport
//! dies, the caller reports it ([`McpSupervisor::report_failure`])
//! and the supervisor schedules reconnect attempts with doubling
//! delays ([`ReconnectPolicy::initial_delay`] → capped at
//! [`ReconnectPolicy::max_delay`]). Retries within one outage
//! share a budget ([`ReconnectPolicy::max_attempts`]); a
//! connection that stays up for at least `max_delay` closes the
//! outage, so the next disconnect starts a fresh budget. A
//! crash-looping server that only briefly connects therefore
//! still exhausts its budget. Exhaustion unregisters the
//! server's tools — the failure stays visible to the model
//! instead of silently serving stale ones.
//!
//! ## What a reconnect does
//!
//! One attempt = fresh transport from the [`TransportFactory`] →
//! fresh [`McpClient`] → `initialize` → `tools/list` → atomic
//! generation swap. Until the swap lands, the previous
//! generation stays registered, so tool calls during an outage
//! fail loudly at the transport instead of vanishing from the
//! tool surface.
//!
//! ## Re-syncs
//!
//! [`McpSupervisor::notify_list_changed`] performs the same
//! two-phase swap on a `tools/list_changed` notification: a
//! failed fetch keeps the previous generation registered, and a
//! registration conflict rolls the swap back wholesale.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use synthia_core::Error;
use synthia_tool::ToolRegistry;
use tokio::sync::Mutex;

use crate::{
    SharedTransport,
    client::{McpClient, McpToolGeneration, sync_mcp_tools},
    naming::NamingPolicy,
};

/// Reconnect policy for supervised servers.
///
/// Delays double per failed attempt (`initial`, `initial * 2`, …)
/// up to `max_delay`. `max_attempts` is the per-outage retry
/// budget; exceeding it unregisters the server's tools and stops
/// supervision for that server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconnectPolicy {
    /// Delay before the first reconnect attempt (1s default).
    pub initial_delay: ChronoDuration,
    /// Cap on the doubling delay (60s default). Also the
    /// stability window: a connection that stayed up this long
    /// resets the outage budget.
    pub max_delay: ChronoDuration,
    /// Failed attempts tolerated per outage before giving up
    /// (5 default).
    pub max_attempts: u32,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            initial_delay: ChronoDuration::seconds(1),
            max_delay: ChronoDuration::seconds(60),
            max_attempts: 5,
        }
    }
}

impl ReconnectPolicy {
    /// Delay before attempt number `failures` (1-based) of one
    /// outage: `initial * 2^(failures - 1)`, capped at
    /// `max_delay`.
    #[must_use]
    pub fn delay_for(&self, failures: u32) -> ChronoDuration {
        // The cap dominates after 6 doublings of the 1s default;
        // clamping the shift keeps the multiply far from
        // overflow for any caller-supplied initial delay.
        let shift =
            i32::try_from(failures.saturating_sub(1).min(16)).unwrap_or(16);
        let doubled = self.initial_delay * (1i32 << shift);
        doubled.min(self.max_delay)
    }
}

/// Establishes a fresh transport per connection attempt.
///
/// A reconnect MUST NOT reuse the dead transport (stdio pipes
/// are gone; HTTP sessions may be half-closed), so the
/// supervisor asks the factory for a new one on every attempt.
#[async_trait]
pub trait TransportFactory: Send + Sync {
    /// Open a new transport, or fail with a diagnostic.
    async fn connect(&self) -> Result<SharedTransport, Error>;
}

/// Lifecycle state of one supervised server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    /// Registered, no connection attempt has run yet.
    Pending,
    /// Connected; tools registered from the current generation.
    Connected,
    /// Down, with a reconnect attempt scheduled.
    Reconnecting,
    /// Retry budget exhausted; tools unregistered. Terminal —
    /// only a new supervisor (process restart) revives it.
    Exhausted,
}

/// Read-only health snapshot of one supervised server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerHealth {
    /// Configured server name (the namespace for its tools).
    pub name: String,
    /// Lifecycle state.
    pub state: HealthState,
    /// Tools registered from the live generation.
    pub registered_tools: usize,
    /// Consecutive failed attempts within the current outage.
    pub consecutive_failures: u32,
}

impl ServerHealth {
    /// Whether the server is connected right now.
    #[must_use]
    pub fn healthy(&self) -> bool {
        self.state == HealthState::Connected
    }
}

/// One [`McpSupervisor::tick`] result.
#[derive(Debug, Clone)]
pub struct SupervisorTick {
    /// Health of every supervised server after the tick.
    pub health: Vec<ServerHealth>,
    /// Earliest pending reconnect deadline, for the caller to
    /// arm its timer. `None` when nothing is scheduled.
    pub next_deadline: Option<DateTime<Utc>>,
}

impl SupervisorTick {
    /// Health of one server by name.
    #[must_use]
    pub fn server(&self, name: &str) -> Option<&ServerHealth> {
        self.health.iter().find(|h| h.name == name)
    }
}

/// Per-server supervision state.
struct SupervisedServer {
    name: String,
    factory: Arc<dyn TransportFactory>,
    /// Live client; `None` while down (or after exhaustion).
    client: Option<Arc<McpClient>>,
    /// Tools registered from the last successful sync.
    generation: McpToolGeneration,
    /// When the current connection was established; `None`
    /// while down.
    connected_since: Option<DateTime<Utc>>,
    /// Failed attempts since the last budget reset (persists
    /// across brief reconnects, mirroring dsh).
    failures: u32,
    /// Next reconnect attempt due; `None` when connected,
    /// pending, or exhausted.
    next_attempt: Option<DateTime<Utc>>,
    /// Retry budget exhausted — terminal.
    exhausted: bool,
}

impl SupervisedServer {
    fn health_state(&self) -> HealthState {
        if self.exhausted {
            HealthState::Exhausted
        } else if self.connected_since.is_some() {
            HealthState::Connected
        } else if self.next_attempt.is_some() {
            HealthState::Reconnecting
        } else {
            HealthState::Pending
        }
    }

    fn health(&self) -> ServerHealth {
        ServerHealth {
            name: self.name.clone(),
            state: self.health_state(),
            registered_tools: self.generation.len(),
            consecutive_failures: self.failures,
        }
    }

    fn is_connected(&self) -> bool {
        self.connected_since.is_some()
    }

    /// Whether a connection attempt is due at `now`.
    fn attempt_due(&self, now: DateTime<Utc>) -> bool {
        if self.exhausted || self.is_connected() {
            return false;
        }
        match self.next_attempt {
            Some(due) => due <= now,
            // Pending: first attempt runs on the first tick.
            None => true,
        }
    }
}

/// Supervises a fleet of MCP servers over a shared registry.
///
/// Register servers with [`Self::supervise`], then drive the
/// state machine with [`Self::tick`]. The supervisor is
/// runtime-neutral: it performs I/O only inside `tick` /
/// `notify_list_changed` and holds no timer.
pub struct McpSupervisor {
    policy: ReconnectPolicy,
    naming: NamingPolicy,
    servers: Mutex<Vec<SupervisedServer>>,
}

impl Default for McpSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl McpSupervisor {
    /// Supervisor with the default policy (namespaced tool
    /// names, 1s → 60s backoff, 5 attempts per outage).
    #[must_use]
    pub fn new() -> Self {
        Self {
            policy: ReconnectPolicy::default(),
            naming: NamingPolicy::default(),
            servers: Mutex::new(Vec::new()),
        }
    }

    /// Override the reconnect policy.
    #[must_use]
    pub fn with_reconnect_policy(mut self, policy: ReconnectPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Override the tool-naming policy (namespaced by default).
    #[must_use]
    pub fn with_naming(mut self, naming: NamingPolicy) -> Self {
        self.naming = naming;
        self
    }

    /// The active tool-naming policy.
    ///
    /// `pub(crate)` on purpose: [`McpControlTool`](crate::McpControlTool)
    /// needs it to derive the registered name of a remote tool (and so
    /// consult the registry's privacy flag for it), and it lives in
    /// this crate. A consumer never needs to re-derive names — the
    /// generation already registered them.
    pub(crate) fn naming(&self) -> NamingPolicy {
        self.naming
    }

    /// Add a server to supervise. `name` is the stable local
    /// namespace its tools are registered under; the first
    /// connection attempt happens on the first
    /// [`Self::tick`]. A duplicate name is ignored (the
    /// original registration wins) and logged.
    pub async fn supervise(
        &self,
        name: impl Into<String>,
        factory: Arc<dyn TransportFactory>,
    ) {
        let name = name.into();
        let mut servers = self.servers.lock().await;
        if servers.iter().any(|s| s.name == name) {
            tracing::warn!(
                target: "synthia.mcp",
                server = %name,
                "MCP server already supervised; ignoring duplicate"
            );
            return;
        }
        servers.push(SupervisedServer {
            name,
            factory,
            client: None,
            generation: McpToolGeneration::default(),
            connected_since: None,
            failures: 0,
            next_attempt: None,
            exhausted: false,
        });
    }

    /// Drive one supervision pass at `now`: run every due
    /// connection attempt, then report health and the next
    /// deadline.
    pub async fn tick(
        &self,
        registry: &ToolRegistry,
        now: DateTime<Utc>,
    ) -> SupervisorTick {
        let mut servers = self.servers.lock().await;
        let mut health = Vec::with_capacity(servers.len());
        let mut next_deadline: Option<DateTime<Utc>> = None;
        for server in servers.iter_mut() {
            if server.attempt_due(now) {
                self.attempt(server, registry, now).await;
            }
            if let Some(due) = server.next_attempt {
                next_deadline =
                    Some(next_deadline.map(|d| d.min(due)).unwrap_or(due));
            }
            health.push(server.health());
        }
        SupervisorTick {
            health,
            next_deadline,
        }
    }

    /// Report that a connected server's transport failed at
    /// `now`. Schedules the reconnect (or exhausts the budget,
    /// unregistering the server's tools); the previous generation
    /// stays registered until the reconnected swap lands, so
    /// calls keep failing loudly at the transport.
    ///
    /// No-op when the server is unknown, already down (one
    /// outage, one schedule), or exhausted.
    pub async fn report_failure(
        &self,
        registry: &ToolRegistry,
        name: &str,
        now: DateTime<Utc>,
    ) {
        let mut servers = self.servers.lock().await;
        let Some(server) = servers.iter_mut().find(|s| s.name == name) else {
            tracing::warn!(
                target: "synthia.mcp",
                server = %name,
                "failure reported for an unsupervised MCP server"
            );
            return;
        };
        if !server.is_connected() {
            return;
        }
        // A connection that stayed up for at least one full
        // backoff window closes the previous outage.
        if let Some(since) = server.connected_since
            && now - since >= self.policy.max_delay
        {
            server.failures = 0;
        }
        server.connected_since = None;
        server.client = None;
        server.failures += 1;
        tracing::warn!(
            target: "synthia.mcp",
            server = %name,
            attempt = server.failures,
            max = self.policy.max_attempts,
            "MCP connection lost; scheduling reconnect"
        );
        self.schedule_next(server, registry, now);
    }

    /// Handle a `tools/list_changed` notification: fetch the new
    /// list and swap the registration generation.
    ///
    /// A failed fetch keeps the previous generation registered
    /// (the server may still serve it) and returns the error; a
    /// registration conflict rolls the swap back wholesale.
    /// Unknown, disconnected, and exhausted servers return an
    /// error without side effects.
    pub async fn notify_list_changed(
        &self,
        registry: &ToolRegistry,
        name: &str,
    ) -> Result<(), Error> {
        let mut servers = self.servers.lock().await;
        let Some(server) = servers.iter_mut().find(|s| s.name == name) else {
            return Err(Error::NotFound {
                item: format!("MCP server `{name}` is not supervised"),
            });
        };
        let Some(client) =
            server.client.clone().filter(|_| server.is_connected())
        else {
            return Err(Error::Internal {
                message: format!(
                    "MCP server `{name}` is not connected; cannot re-sync"
                ),
            });
        };
        match sync_mcp_tools(
            registry,
            &client,
            &server.name,
            self.naming,
            &server.generation,
        )
        .await
        {
            Ok(generation) => {
                tracing::info!(
                    target: "synthia.mcp",
                    server = %server.name,
                    tools = generation.len(),
                    "re-synced MCP tool generation on tools/list_changed"
                );
                server.generation = generation;
                Ok(())
            }
            Err(e) => {
                tracing::warn!(
                    target: "synthia.mcp",
                    server = %server.name,
                    error = %e,
                    "MCP tool re-sync failed; keeping previous generation"
                );
                Err(e)
            }
        }
    }

    /// Health snapshot of every supervised server.
    pub async fn health(&self) -> Vec<ServerHealth> {
        self.servers
            .lock()
            .await
            .iter()
            .map(|s| s.health())
            .collect()
    }

    /// The live client of a connected server, if any.
    pub async fn client(&self, name: &str) -> Option<Arc<McpClient>> {
        self.servers
            .lock()
            .await
            .iter()
            .find(|s| s.name == name)
            .filter(|s| s.is_connected())
            .and_then(|s| s.client.clone())
    }

    /// Live clients of every healthy server, in registration
    /// order — the shape boot code retains for the process
    /// lifetime.
    pub async fn healthy_clients(&self) -> Vec<(String, Arc<McpClient>)> {
        self.servers
            .lock()
            .await
            .iter()
            .filter(|s| s.is_connected())
            .filter_map(|s| s.client.clone().map(|c| (s.name.clone(), c)))
            .collect()
    }

    /// One connection attempt: fresh transport, fresh client,
    /// handshake, generation sync.
    async fn attempt(
        &self,
        server: &mut SupervisedServer,
        registry: &ToolRegistry,
        now: DateTime<Utc>,
    ) {
        match self.try_connect(server, registry).await {
            Ok(()) => {
                tracing::info!(
                    target: "synthia.mcp",
                    server = %server.name,
                    tools = server.generation.len(),
                    failures = server.failures,
                    "MCP server connected"
                );
                server.connected_since = Some(now);
                server.next_attempt = None;
            }
            Err(e) => {
                tracing::warn!(
                    target: "synthia.mcp",
                    server = %server.name,
                    error = %e,
                    attempt = server.failures + 1,
                    "MCP connection attempt failed"
                );
                server.failures += 1;
                self.schedule_next(server, registry, now);
            }
        }
    }

    /// Establish transport + handshake + tool sync. Nothing is
    /// mutated on the server until every step succeeded.
    async fn try_connect(
        &self,
        server: &mut SupervisedServer,
        registry: &ToolRegistry,
    ) -> Result<(), Error> {
        let transport = server.factory.connect().await?;
        let client = McpClient::new(transport);
        client.initialize().await?;
        let generation = sync_mcp_tools(
            registry,
            &client,
            &server.name,
            self.naming,
            &server.generation,
        )
        .await?;
        server.generation = generation;
        server.client = Some(client);
        Ok(())
    }

    /// Either schedule the next retry or give up and unregister.
    fn schedule_next(
        &self,
        server: &mut SupervisedServer,
        registry: &ToolRegistry,
        now: DateTime<Utc>,
    ) {
        if server.failures > self.policy.max_attempts {
            server.next_attempt = None;
            server.connected_since = None;
            server.client = None;
            server.exhausted = true;
            tracing::error!(
                target: "synthia.mcp",
                server = %server.name,
                attempts = server.failures,
                "MCP retry budget exhausted; unregistering tools"
            );
            // Fail visible: the model must see the tools
            // disappear rather than call into a dead server.
            server.generation.unregister_from(registry);
            server.generation = McpToolGeneration::default();
            return;
        }
        let delay = self.policy.delay_for(server.failures);
        server.next_attempt = Some(now + delay);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_doubles_then_caps() {
        let policy = ReconnectPolicy {
            initial_delay: ChronoDuration::seconds(1),
            max_delay: ChronoDuration::seconds(60),
            max_attempts: 5,
        };
        assert_eq!(policy.delay_for(1), ChronoDuration::seconds(1));
        assert_eq!(policy.delay_for(2), ChronoDuration::seconds(2));
        assert_eq!(policy.delay_for(3), ChronoDuration::seconds(4));
        assert_eq!(policy.delay_for(6), ChronoDuration::seconds(32));
        assert_eq!(policy.delay_for(7), ChronoDuration::seconds(60));
        assert_eq!(policy.delay_for(40), ChronoDuration::seconds(60));
    }

    #[test]
    fn default_policy_matches_the_documented_shape() {
        let policy = ReconnectPolicy::default();
        assert_eq!(policy.initial_delay, ChronoDuration::seconds(1));
        assert_eq!(policy.max_delay, ChronoDuration::seconds(60));
        assert_eq!(policy.max_attempts, 5);
    }
}
