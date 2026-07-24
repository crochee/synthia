//! 11 unit tests for the `server` module family.
//!
//! Coverage map:
//!
//! - [`super::service::MessageProxyServiceImpl::register`]:
//!   2 tests (rejects_empty_agent_id /
//!   succeeds_for_new_agent).
//! - [`super::service::MessageProxyServiceImpl::send`]:
//!   4 tests (rejects_message_without_id /
//!   rejects_message_without_recipient /
//!   to_unregistered_recipient_reports_failure /
//!   to_registered_but_idle_agent_reports_no_subscriber).
//! - [`super::service::MessageProxyServiceImpl::broadcast`]:
//!   2 tests (rejects_empty_recipients /
//!   skips_sender_in_recipient_list).
//! - [`super::service::MessageProxyServiceImpl::subscribe`]:
//!   3 tests (rejects_empty_agent_id /
//!   requires_prior_register / after_register_succeeds).

use tonic::Request;

use super::{service::MessageProxyServiceImpl, state::ProxyState};
use crate::{
    BroadcastRequest,
    Message,
    RegisterRequest,
    SubscribeRequest,
    message_proxy::message_proxy_service_server::MessageProxyService,
};

fn svc() -> MessageProxyServiceImpl {
    MessageProxyServiceImpl {
        state: ProxyState::default(),
    }
}

#[tokio::test]
async fn register_rejects_empty_agent_id() {
    let resp = svc()
        .register(Request::new(RegisterRequest {
            agent_id: String::new(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(!resp.success);
    assert!(resp.error.contains("agent_id"));
}

#[tokio::test]
async fn register_succeeds_for_new_agent() {
    let resp = svc()
        .register(Request::new(RegisterRequest {
            agent_id: "agent-x".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(resp.success);
    assert!(resp.error.is_empty());
}

#[tokio::test]
async fn send_rejects_message_without_id() {
    let resp = svc()
        .send(Request::new(Message {
            id: String::new(),
            from: "a".to_string(),
            to: "b".to_string(),
            payload: vec![],
            timestamp: 0,
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(!resp.success);
    assert!(resp.error.contains("id"));
}

#[tokio::test]
async fn send_rejects_message_without_recipient() {
    let resp = svc()
        .send(Request::new(Message {
            id: "m1".to_string(),
            from: "a".to_string(),
            to: String::new(),
            payload: vec![],
            timestamp: 0,
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(!resp.success);
    assert!(resp.error.contains("`to`"));
}

#[tokio::test]
async fn send_to_unregistered_recipient_reports_failure() {
    let resp = svc()
        .send(Request::new(Message {
            id: "m1".to_string(),
            from: "a".to_string(),
            to: "ghost".to_string(),
            payload: vec![],
            timestamp: 0,
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(!resp.success);
    assert!(resp.error.contains("not registered"));
}

#[tokio::test]
async fn send_to_registered_but_idle_agent_reports_no_subscriber() {
    let s = svc();
    s.register(Request::new(RegisterRequest {
        agent_id: "idle".to_string(),
    }))
    .await
    .unwrap();

    let resp = s
        .send(Request::new(Message {
            id: "m1".to_string(),
            from: "x".to_string(),
            to: "idle".to_string(),
            payload: b"hi".to_vec(),
            timestamp: 0,
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(!resp.success);
    assert!(resp.error.contains("no active subscriber"));
}

#[tokio::test]
async fn broadcast_rejects_empty_recipients() {
    let resp = svc()
        .broadcast(Request::new(BroadcastRequest {
            from: "a".to_string(),
            recipients: vec![],
            payload: vec![],
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(!resp.success);
    assert!(resp.error.contains("recipients"));
}

#[tokio::test]
async fn broadcast_skips_sender_in_recipient_list() {
    // The sender appears in the recipient list, but the only registered
    // subscriber also has zero active receivers — so the broadcast
    // should report no delivery but still be free of an "unregistered"
    // error caused by the sender.
    let s = svc();
    s.register(Request::new(RegisterRequest {
        agent_id: "a".to_string(),
    }))
    .await
    .unwrap();

    let resp = s
        .broadcast(Request::new(BroadcastRequest {
            from: "a".to_string(),
            recipients: vec!["a".to_string()],
            payload: b"x".to_vec(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.delivered_count, 0);
    assert!(!resp.success);
}

#[tokio::test]
async fn subscribe_rejects_empty_agent_id() {
    let result = svc()
        .subscribe(Request::new(SubscribeRequest {
            agent_id: String::new(),
        }))
        .await;
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("empty agent_id should error"),
    };
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn subscribe_requires_prior_register() {
    let result = svc()
        .subscribe(Request::new(SubscribeRequest {
            agent_id: "unregistered".to_string(),
        }))
        .await;
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("unregistered subscribe should fail"),
    };
    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    assert!(err.message().contains("registered"));
}

#[tokio::test]
async fn subscribe_after_register_succeeds() {
    let s = svc();
    s.register(Request::new(RegisterRequest {
        agent_id: "live".to_string(),
    }))
    .await
    .unwrap();
    let result = s
        .subscribe(Request::new(SubscribeRequest {
            agent_id: "live".to_string(),
        }))
        .await;
    let stream = match result {
        Ok(r) => r.into_inner(),
        Err(e) => panic!("subscribe should succeed, got: {e}"),
    };
    // We just verify the stream was returned; it stays open until the
    // server's `Sender` is dropped or `Closed` is observed.
    drop(stream);
}
