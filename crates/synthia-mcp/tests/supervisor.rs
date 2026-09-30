//! Supervisor behavior: boot connect, reconnect backoff,
//! per-outage budget, stability reset, exhaustion, and
//! `tools/list_changed` re-sync — all driven by a scripted clock
//! and the in-memory transport (no process, no socket).

use std::sync::Arc;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::json;
use synthia_mcp::{
    HealthState,
    InMemoryTransport,
    InMemoryTransportFactory,
    McpSupervisor,
    ReconnectPolicy,
    ServerHealth,
};
use synthia_tool::ToolRegistry;

/// One supervised server over a shared, scriptable transport.
struct Fixture {
    supervisor: McpSupervisor,
    registry: ToolRegistry,
    wire: Arc<InMemoryTransport>,
    factory: Arc<InMemoryTransportFactory>,
    t0: DateTime<Utc>,
}

impl Fixture {
    fn new() -> Self {
        Self::with_policy(ReconnectPolicy::default())
    }

    fn with_policy(policy: ReconnectPolicy) -> Self {
        let wire = Arc::new(InMemoryTransport::new().with_echo_server());
        Self {
            supervisor: McpSupervisor::new().with_reconnect_policy(policy),
            registry: ToolRegistry::new(),
            wire: Arc::clone(&wire),
            factory: Arc::new(InMemoryTransportFactory::new(wire)),
            t0: Utc::now(),
        }
    }

    async fn boot(&mut self) -> Vec<ServerHealth> {
        self.supervisor
            .supervise("srv", Arc::clone(&self.factory) as _)
            .await;
        self.supervisor.tick(&self.registry, self.t0).await.health
    }

    fn at(&self, seconds: i64) -> DateTime<Utc> {
        self.t0 + ChronoDuration::seconds(seconds)
    }

    fn names(&self) -> Vec<String> {
        self.registry
            .snapshot()
            .into_iter()
            .map(|m| m.name)
            .collect()
    }

    fn echo_registered(&self) -> bool {
        self.names().iter().any(|n| n == "mcp__srv__echo")
    }
}

#[tokio::test]
async fn boot_tick_connects_and_registers_namespaced_tools() {
    let mut fx = Fixture::new();
    let health = fx.boot().await;

    assert_eq!(health.len(), 1, "one supervised server");
    assert_eq!(health[0].name, "srv");
    assert!(health[0].healthy(), "{:?}", health[0]);
    assert_eq!(health[0].registered_tools, 1);
    assert_eq!(health[0].consecutive_failures, 0);
    assert!(fx.echo_registered(), "{:?}", fx.names());

    let clients = fx.supervisor.healthy_clients().await;
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].0, "srv");
    assert!(fx.supervisor.client("srv").await.is_some());
    assert!(fx.supervisor.client("nope").await.is_none());
}

#[tokio::test]
async fn reconnect_after_failure_uses_doubling_delays() {
    let mut fx = Fixture::new();
    fx.boot().await;

    // Outage: tools stay registered (fail-visible) while down.
    fx.wire.set_alive(false);
    fx.supervisor
        .report_failure(&fx.registry, "srv", fx.at(5))
        .await;
    let health = fx.supervisor.health().await;
    assert_eq!(health[0].state, HealthState::Reconnecting);
    assert_eq!(health[0].consecutive_failures, 1);
    assert!(fx.echo_registered(), "outage must stay fail-visible");

    // First retry is due 1s later (default initial delay); it
    // fails, doubling the next delay.
    let tick = fx.supervisor.tick(&fx.registry, fx.at(5)).await;
    assert_eq!(tick.next_deadline, Some(fx.at(6)), "1s after the failure");
    let tick = fx.supervisor.tick(&fx.registry, fx.at(6)).await;
    assert_eq!(tick.health[0].consecutive_failures, 2);
    assert_eq!(tick.next_deadline, Some(fx.at(8)), "delay doubled to 2s");
    assert_eq!(tick.health[0].state, HealthState::Reconnecting);

    // Recovery: the next due attempt reconnects and re-registers.
    fx.wire.set_alive(true);
    let tick = fx.supervisor.tick(&fx.registry, fx.at(8)).await;
    assert_eq!(tick.health[0].state, HealthState::Connected);
    assert_eq!(tick.next_deadline, None);
    assert!(fx.echo_registered(), "{:?}", fx.names());
}

#[tokio::test]
async fn budget_exhaustion_unregisters_the_tools() {
    let mut fx = Fixture::with_policy(ReconnectPolicy {
        initial_delay: ChronoDuration::seconds(1),
        max_delay: ChronoDuration::seconds(60),
        max_attempts: 2,
    });
    fx.boot().await;
    assert!(fx.echo_registered());

    fx.wire.set_alive(false);
    fx.supervisor
        .report_failure(&fx.registry, "srv", fx.at(0))
        .await;
    // Two retries are still within budget…
    fx.supervisor.tick(&fx.registry, fx.at(1)).await;
    fx.supervisor.tick(&fx.registry, fx.at(3)).await;
    // …the third exceeds it: give up and unregister.
    let tick = fx.supervisor.tick(&fx.registry, fx.at(7)).await;
    assert_eq!(tick.health[0].state, HealthState::Exhausted);
    assert_eq!(tick.health[0].registered_tools, 0);
    assert!(!fx.echo_registered(), "exhaustion must be fail-visible");
    assert!(fx.supervisor.healthy_clients().await.is_empty());

    // Terminal: further ticks and failure reports change nothing.
    let before = fx.wire.methods().len();
    fx.supervisor
        .report_failure(&fx.registry, "srv", fx.at(1000))
        .await;
    let tick = fx.supervisor.tick(&fx.registry, fx.at(1000)).await;
    assert_eq!(tick.health[0].state, HealthState::Exhausted);
    assert_eq!(fx.wire.methods().len(), before, "no further attempts");
}

#[tokio::test]
async fn stable_connection_resets_the_outage_budget() {
    let mut fx = Fixture::with_policy(ReconnectPolicy {
        initial_delay: ChronoDuration::seconds(1),
        max_delay: ChronoDuration::seconds(10),
        max_attempts: 5,
    });
    fx.boot().await;

    // Short outage (uptime 2s < 10s stability window): the count
    // carries over.
    fx.wire.set_alive(false);
    fx.supervisor
        .report_failure(&fx.registry, "srv", fx.at(2))
        .await;
    fx.wire.set_alive(true);
    fx.supervisor.tick(&fx.registry, fx.at(3)).await;
    fx.wire.set_alive(false);
    fx.supervisor
        .report_failure(&fx.registry, "srv", fx.at(5))
        .await;
    assert_eq!(
        fx.supervisor.health().await[0].consecutive_failures,
        2,
        "an unstable connection accumulates failures"
    );

    // Long outage (uptime ~93s >= 10s): budget restarts.
    // The retry after two failures is due 2s later, at t+7.
    fx.wire.set_alive(true);
    let tick = fx.supervisor.tick(&fx.registry, fx.at(7)).await;
    assert_eq!(tick.health[0].state, HealthState::Connected);
    fx.wire.set_alive(false);
    fx.supervisor
        .report_failure(&fx.registry, "srv", fx.at(100))
        .await;
    assert_eq!(
        fx.supervisor.health().await[0].consecutive_failures,
        1,
        "a stable connection closes the outage"
    );
}

#[tokio::test]
async fn failed_reconnect_keeps_the_previous_generation_live() {
    let mut fx = Fixture::new();
    fx.boot().await;

    fx.wire.set_alive(false);
    fx.supervisor
        .report_failure(&fx.registry, "srv", fx.at(0))
        .await;
    fx.supervisor.tick(&fx.registry, fx.at(1)).await;
    fx.supervisor.tick(&fx.registry, fx.at(3)).await;

    // Still down, still registered: calls fail loudly at the
    // transport instead of the tools silently vanishing.
    assert!(fx.echo_registered(), "{:?}", fx.names());
    assert_eq!(fx.supervisor.health().await[0].registered_tools, 1);
}

#[tokio::test]
async fn list_changed_swaps_the_registration_generation() {
    let mut fx = Fixture::new();
    fx.boot().await;

    fx.wire.set_script(
        "tools/list",
        json!({"tools": [
            {"name": "echo", "description": "Echo the input back"},
            {"name": "reverse", "description": "Reverse a string"}
        ]}),
    );
    fx.supervisor
        .notify_list_changed(&fx.registry, "srv")
        .await
        .expect("re-sync");

    let names = fx.names();
    assert!(names.contains(&"mcp__srv__echo".to_string()), "{names:?}");
    assert!(
        names.contains(&"mcp__srv__reverse".to_string()),
        "{names:?}"
    );
    assert_eq!(fx.supervisor.health().await[0].registered_tools, 2);

    // A later list that drops a tool retires it.
    fx.wire.set_script(
        "tools/list",
        json!({"tools": [{"name": "reverse", "description": "Reverse"}]}),
    );
    fx.supervisor
        .notify_list_changed(&fx.registry, "srv")
        .await
        .expect("re-sync");
    let names = fx.names();
    assert!(!names.contains(&"mcp__srv__echo".to_string()), "{names:?}");
    assert!(names.contains(&"mcp__srv__reverse".to_string()));
}

#[tokio::test]
async fn list_changed_fetch_failure_keeps_the_previous_generation() {
    let mut fx = Fixture::new();
    fx.boot().await;

    fx.wire.fail_next("tools/list exploded");
    let err = fx
        .supervisor
        .notify_list_changed(&fx.registry, "srv")
        .await
        .expect_err("fetch failure must surface");
    assert!(err.to_string().contains("exploded"), "{err}");

    // Previous generation untouched, connection still healthy.
    assert!(fx.echo_registered(), "{:?}", fx.names());
    assert_eq!(
        fx.supervisor.health().await[0].state,
        HealthState::Connected
    );
}

#[tokio::test]
async fn registration_conflict_rolls_the_swap_back_wholesale() {
    let mut fx = Fixture::new();
    fx.boot().await;
    assert!(fx.echo_registered());

    // A foreign registration squats on this server's namespace.
    fx.registry.register_entry(synthia_tool::ToolEntry::dynamic(
        "mcp__srv__other".to_string(),
        "foreign".to_string(),
        json!({"type": "object"}),
    ));

    fx.wire.set_script(
        "tools/list",
        json!({"tools": [
            {"name": "echo", "description": "Echo the input back"},
            {"name": "other", "description": "Collides"}
        ]}),
    );
    let err = fx
        .supervisor
        .notify_list_changed(&fx.registry, "srv")
        .await
        .expect_err("conflict must surface");
    assert!(err.to_string().contains("mcp__srv__other"), "{err}");

    // No partial generation: the squatter is untouched and the
    // previous generation (echo) is still live exactly once.
    let names = fx.names();
    assert_eq!(
        names.iter().filter(|n| *n == "mcp__srv__echo").count(),
        1,
        "{names:?}"
    );
    assert!(names.contains(&"mcp__srv__other".to_string()));

    // A clean list afterwards still re-syncs.
    fx.wire.set_script(
        "tools/list",
        json!({"tools": [{"name": "echo", "description": "Echo"}]}),
    );
    fx.supervisor
        .notify_list_changed(&fx.registry, "srv")
        .await
        .expect("clean re-sync");
    assert!(fx.echo_registered());
}

#[tokio::test]
async fn notify_list_changed_requires_a_connected_known_server() {
    let fx = Fixture::new();
    let err = fx
        .supervisor
        .notify_list_changed(&fx.registry, "ghost")
        .await
        .expect_err("unknown server");
    assert!(err.to_string().contains("ghost"), "{err}");

    fx.supervisor
        .supervise("srv", Arc::clone(&fx.factory) as _)
        .await;
    let err = fx
        .supervisor
        .notify_list_changed(&fx.registry, "srv")
        .await
        .expect_err("never connected");
    assert!(err.to_string().contains("not connected"), "{err}");
}

#[tokio::test]
async fn tick_reports_the_earliest_pending_deadline() {
    let mut fx = Fixture::new();
    fx.boot().await;
    assert_eq!(
        fx.supervisor
            .tick(&fx.registry, fx.at(0))
            .await
            .next_deadline,
        None,
        "healthy servers need no timer"
    );

    // A second server joins and fails immediately: its retry
    // deadline is the tick's next deadline.
    let wire2 = Arc::new(InMemoryTransport::new().with_echo_server());
    wire2.set_alive(false);
    let factory2 = Arc::new(InMemoryTransportFactory::new(wire2));
    fx.supervisor.supervise("srv2", factory2).await;
    let tick = fx.supervisor.tick(&fx.registry, fx.at(10)).await;
    let srv2 = tick.server("srv2").expect("srv2 health");
    assert_eq!(srv2.consecutive_failures, 1);
    assert_eq!(tick.next_deadline, Some(fx.at(11)));
    assert_eq!(
        tick.server("srv").map(|h| h.state),
        Some(HealthState::Connected)
    );
}

#[tokio::test]
async fn duplicate_supervise_is_ignored() {
    let fx = Fixture::new();
    fx.supervisor
        .supervise("srv", Arc::clone(&fx.factory) as _)
        .await;
    fx.supervisor
        .supervise("srv", Arc::clone(&fx.factory) as _)
        .await;
    assert_eq!(fx.supervisor.health().await.len(), 1);
}

#[tokio::test]
async fn report_failure_for_an_unknown_server_is_a_noop() {
    let fx = Fixture::new();
    fx.supervisor
        .report_failure(&fx.registry, "ghost", Utc::now())
        .await;
    assert!(fx.supervisor.health().await.is_empty());
}
