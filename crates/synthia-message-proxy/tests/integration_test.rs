//! Integration tests for the `MessageProxy` gRPC service.
//!
//! These tests stand up a real `MessageProxyServer` on a per-test Unix
//! Domain Socket, connect two or more `MessageBusProxy` clients, and verify
//! that `Send`, `Broadcast`, `Subscribe`, and reconnection behave as
//! documented in the service contract.

use std::time::Duration;

use futures::StreamExt;
use synthia_message_proxy::{
    MessageBus,
    MessageBusProxy,
    MessageProxyServer,
    ProxyError,
};
use tempfile::TempDir;
use tokio::time::timeout;

type ServerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// Reserved UDS path inside a per-test temp directory.
struct TestServer {
    addr: String,
    handle: tokio::task::JoinHandle<ServerResult>,
    _dir: TempDir,
}

impl TestServer {
    fn start() -> Self {
        let dir = tempfile::tempdir().expect("create temp dir");
        let addr = dir
            .path()
            .join("proxy.sock")
            .to_str()
            .expect("utf8 path")
            .to_string();

        let server = MessageProxyServer::new(addr.clone());
        let handle = tokio::spawn(async move {
            server.serve().await.map_err(|e| {
                Box::<dyn std::error::Error + Send + Sync>::from(e.to_string())
            })
        });

        // The server uses `connect_lazy` semantics on the client side, but
        // the UDS listener itself needs a moment to bind. Poll the path
        // briefly to avoid racing the first RPC.
        for _ in 0..50 {
            if std::path::Path::new(&addr).exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        Self {
            addr,
            handle,
            _dir: dir,
        }
    }

    async fn stop(self) {
        self.handle.abort();
        let _ = self.handle.await;
    }
}

/// Wait for a subscription stream to surface the next message, panicking
/// with a useful diagnostic if the timeout elapses first.
async fn next_msg(
    stream: &mut (
             dyn futures::Stream<
        Item = Result<synthia_message_proxy::Message, tonic::Status>,
    > + Send
                 + Unpin
         ),
) -> synthia_message_proxy::Message {
    timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("timed out waiting for message")
        .expect("stream ended unexpectedly")
        .expect("stream returned error")
}

/// Extract the gRPC `Status` from a `ProxyError`, panicking if the error
/// originated on the client (e.g. transport) rather than the server.
fn status(err: ProxyError) -> tonic::Status {
    match err {
        ProxyError::Status(s) => s,
        other => panic!("expected tonic::Status, got: {other}"),
    }
}

// --- Point-to-point -------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_agents_point_to_point() {
    let server = TestServer::start();

    let alice =
        MessageBusProxy::connect_to("alice".to_string(), server.addr.clone())
            .await
            .expect("alice connect");
    let bob =
        MessageBusProxy::connect_to("bob".to_string(), server.addr.clone())
            .await
            .expect("bob connect");

    alice.register("alice").await.expect("register alice");
    bob.register("bob").await.expect("register bob");

    let mut bob_sub = bob.subscribe("bob").await.expect("bob subscribe");

    // Give the server a tick to register the subscriber.
    tokio::time::sleep(Duration::from_millis(50)).await;

    alice
        .send("alice", "bob", b"hello bob".to_vec())
        .await
        .expect("send alice->bob");

    let msg = next_msg(bob_sub.as_mut()).await;
    assert_eq!(msg.from, "alice");
    assert_eq!(msg.to, "bob");
    assert_eq!(msg.payload, b"hello bob");

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_to_unknown_recipient_fails() {
    let server = TestServer::start();
    let alice =
        MessageBusProxy::connect_to("alice".to_string(), server.addr.clone())
            .await
            .expect("alice connect");
    alice.register("alice").await.expect("register alice");

    let err = alice
        .send("alice", "ghost", b"hi".to_vec())
        .await
        .expect_err("send to unregistered should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("ghost") && msg.contains("not registered"),
        "unexpected error: {msg}"
    );

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_to_idle_subscriber_fails() {
    // The recipient is registered but never subscribes; delivery must be
    // reported as failed because the broadcast channel has zero receivers.
    let server = TestServer::start();
    let alice =
        MessageBusProxy::connect_to("alice".to_string(), server.addr.clone())
            .await
            .expect("alice connect");
    let bob =
        MessageBusProxy::connect_to("bob".to_string(), server.addr.clone())
            .await
            .expect("bob connect");
    alice.register("alice").await.expect("register alice");
    bob.register("bob").await.expect("register bob");

    let err = alice
        .send("alice", "bob", b"hi".to_vec())
        .await
        .expect_err("send with no subscriber should fail");
    assert!(
        err.to_string().contains("no active subscriber"),
        "unexpected error: {err}"
    );

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn subscribe_before_register_returns_failed_precondition() {
    let server = TestServer::start();
    let bob =
        MessageBusProxy::connect_to("bob".to_string(), server.addr.clone())
            .await
            .expect("bob connect");

    let result = bob.subscribe("bob").await;
    let s = match result {
        Err(e) => status(e),
        Ok(_) => panic!("subscribe without register should fail"),
    };
    assert_eq!(s.code(), tonic::Code::FailedPrecondition);
    assert!(s.message().contains("registered"), "got: {}", s.message());

    server.stop().await;
}

// --- Broadcast ------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn broadcast_reaches_all_registered_subscribers() {
    let server = TestServer::start();

    let a = MessageBusProxy::connect_to("a".to_string(), server.addr.clone())
        .await
        .unwrap();
    let b = MessageBusProxy::connect_to("b".to_string(), server.addr.clone())
        .await
        .unwrap();
    let c = MessageBusProxy::connect_to("c".to_string(), server.addr.clone())
        .await
        .unwrap();
    let d = MessageBusProxy::connect_to("d".to_string(), server.addr.clone())
        .await
        .unwrap();

    for (id, client) in [("a", &a), ("b", &b), ("c", &c), ("d", &d)] {
        client.register(id).await.expect("register");
    }

    let mut b_sub = b.subscribe("b").await.expect("b subscribe");
    let mut c_sub = c.subscribe("c").await.expect("c subscribe");
    let mut d_sub = d.subscribe("d").await.expect("d subscribe");

    // Wait for the server to attach the three subscribers.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // a broadcasts to b, c, d (and itself, which the server filters).
    let delivered = a
        .broadcast(
            "a",
            vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string(),
            ],
            b"shout".to_vec(),
        )
        .await
        .expect("broadcast");
    assert_eq!(delivered, 3, "expected delivery to b, c, d");

    for (name, stream) in
        [("b", &mut b_sub), ("c", &mut c_sub), ("d", &mut d_sub)]
    {
        let msg = next_msg(stream.as_mut()).await;
        assert_eq!(msg.from, "a", "wrong sender for {name}");
        assert_eq!(msg.to, name, "wrong recipient for {name}");
        assert_eq!(msg.payload, b"shout", "wrong payload for {name}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn broadcast_with_empty_recipients_fails() {
    let server = TestServer::start();
    let a = MessageBusProxy::connect_to("a".to_string(), server.addr.clone())
        .await
        .unwrap();
    a.register("a").await.unwrap();

    let err = a
        .broadcast("a", vec![], b"x".to_vec())
        .await
        .expect_err("empty broadcast should fail");
    assert!(err.to_string().contains("recipients"));

    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn broadcast_to_unregistered_subset_reports_partial() {
    // One of the recipients is unknown; the server should still deliver to
    // the registered ones and return their count.
    let server = TestServer::start();
    let a = MessageBusProxy::connect_to("a".to_string(), server.addr.clone())
        .await
        .unwrap();
    let b = MessageBusProxy::connect_to("b".to_string(), server.addr.clone())
        .await
        .unwrap();
    a.register("a").await.unwrap();
    b.register("b").await.unwrap();
    let mut b_sub = b.subscribe("b").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let delivered = a
        .broadcast(
            "a",
            vec!["b".to_string(), "ghost".to_string()],
            b"ping".to_vec(),
        )
        .await
        .expect("partial broadcast should still report success");
    assert_eq!(delivered, 1, "only `b` should receive the message");

    let msg = next_msg(b_sub.as_mut()).await;
    assert_eq!(msg.from, "a");
    assert_eq!(msg.to, "b");
    assert_eq!(msg.payload, b"ping");
}

// --- Reconnection ---------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_recovers_after_server_restart() {
    // First incarnation of the server.
    let dir = tempfile::tempdir().expect("temp dir");
    let addr = dir.path().join("proxy.sock").to_str().unwrap().to_string();

    let s1 = MessageProxyServer::new(addr.clone());
    let h1 = tokio::spawn(async move {
        s1.serve().await.map_err(|e| {
            Box::<dyn std::error::Error + Send + Sync>::from(e.to_string())
        })
    });
    for _ in 0..50 {
        if std::path::Path::new(&addr).exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let alice = MessageBusProxy::connect_to("alice".to_string(), addr.clone())
        .await
        .unwrap();
    let bob = MessageBusProxy::connect_to("bob".to_string(), addr.clone())
        .await
        .unwrap();
    alice.register("alice").await.unwrap();
    bob.register("bob").await.unwrap();
    let mut bob_sub = bob.subscribe("bob").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    alice
        .send("alice", "bob", b"first".to_vec())
        .await
        .expect("send pre-restart");
    let msg = next_msg(bob_sub.as_mut()).await;
    assert_eq!(msg.payload, b"first");

    // Kill the server and immediately start a fresh one on the same socket
    // path. The lazy `Channel` in the client should reconnect transparently
    // on the next RPC.
    h1.abort();
    let _ = h1.await;
    // Give the OS a beat to release the socket.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let s2 = MessageProxyServer::new(addr.clone());
    let h2 = tokio::spawn(async move {
        s2.serve().await.map_err(|e| {
            Box::<dyn std::error::Error + Send + Sync>::from(e.to_string())
        })
    });
    for _ in 0..50 {
        if std::path::Path::new(&addr).exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // New server has empty state, so re-register before re-subscribing.
    alice
        .register("alice")
        .await
        .expect("register after restart");
    bob.register("bob").await.expect("register after restart");
    let mut bob_sub2 =
        bob.subscribe("bob").await.expect("subscribe after restart");
    tokio::time::sleep(Duration::from_millis(50)).await;

    alice
        .send("alice", "bob", b"second".to_vec())
        .await
        .expect("send post-restart");
    let msg = next_msg(bob_sub2.as_mut()).await;
    assert_eq!(msg.payload, b"second");

    h2.abort();
    let _ = h2.await;
}
