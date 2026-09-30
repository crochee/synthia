//! Integration tests for the four new management surfaces
//! (schedules / workflows / evals / tasks) wired in the
//! "前端贯通 backend" turn. One smoke test per domain —
//! asserts the route is registered, the wire shape round-trips,
//! and the basic CRUD verbs work end-to-end.

use std::sync::Arc;

use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use synthia_server::{create_router, state::AppState};
use tower::ServiceExt;

async fn make_app() -> axum::Router {
    let temp = tempfile::TempDir::new().unwrap();
    let session_manager = synthia::session::manager::SessionRegistry::new(
        temp.path().to_path_buf(),
    );
    let state =
        AppState::for_test(session_manager, temp.path().to_path_buf()).await;
    create_router(Arc::new(state)).await
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

// --- Schedules -----------------------------------------------------

#[tokio::test]
async fn test_schedules_create_list_get_delete_round_trip() {
    let app = make_app().await;
    let name = format!("test_sched_{}", std::process::id());

    // POST /api/v1/schedules
    let create = Request::builder()
        .method("POST")
        .uri("/api/v1/schedules")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            json!({
                "name": name,
                "description": "smoke test schedule",
                "kind": { "kind": "once" },
                "payload": { "smoke": true }
            })
            .to_string(),
        ))
        .unwrap();
    let create_resp = app.clone().oneshot(create).await.unwrap();
    assert_eq!(create_resp.status(), StatusCode::CREATED, "create");
    let created = body_json(create_resp).await;
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["name"].as_str(), Some(name.as_str()));

    // GET /api/v1/schedules/{id}
    let get = Request::builder()
        .uri(format!("/api/v1/schedules/{id}"))
        .body(axum::body::Body::empty())
        .unwrap();
    let get_resp = app.clone().oneshot(get).await.unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);
    let detail = body_json(get_resp).await;
    assert_eq!(detail["name"].as_str(), Some(name.as_str()));

    // GET /api/v1/schedules
    let list = Request::builder()
        .uri("/api/v1/schedules?limit=10")
        .body(axum::body::Body::empty())
        .unwrap();
    let list_resp = app.clone().oneshot(list).await.unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let listed = body_json(list_resp).await;
    assert!(listed["data"].as_array().is_some());

    // DELETE /api/v1/schedules/{id}
    let del = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/schedules/{id}"))
        .body(axum::body::Body::empty())
        .unwrap();
    let del_resp = app.clone().oneshot(del).await.unwrap();
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);
}

// --- Workflows -----------------------------------------------------

#[tokio::test]
async fn test_workflows_create_get_run_delete_round_trip() {
    let app = make_app().await;

    // POST /api/v1/workflows
    let create = Request::builder()
        .method("POST")
        .uri("/api/v1/workflows")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            json!({
                "id": format!("smoke_{}", std::process::id()),
                "steps": [{
                    "kind": "agent",
                    "id": "step1",
                    "agent": "echo",
                    "prompt": "say hi"
                }],
                "phases": []
            })
            .to_string(),
        ))
        .unwrap();
    let create_resp = app.clone().oneshot(create).await.unwrap();
    assert_eq!(create_resp.status(), StatusCode::CREATED, "create workflow");
    let created = body_json(create_resp).await;
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["steps"].as_array().unwrap().len(), 1);

    // GET /api/v1/workflows/{id}
    let get = Request::builder()
        .uri(format!("/api/v1/workflows/{id}"))
        .body(axum::body::Body::empty())
        .unwrap();
    let get_resp = app.clone().oneshot(get).await.unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);
    let detail = body_json(get_resp).await;
    assert_eq!(detail["id"].as_str(), Some(id.as_str()));

    // POST /api/v1/workflows/{id}/run — returns a stubbed
    // WorkflowRunResult with one Superseded CallRun per planned call.
    let run = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/workflows/{id}/run"))
        .body(axum::body::Body::empty())
        .unwrap();
    let run_resp = app.clone().oneshot(run).await.unwrap();
    assert_eq!(run_resp.status(), StatusCode::OK);
    let run_json = body_json(run_resp).await;
    assert!(run_json["run_id"].as_str().is_some());
    let calls = run_json["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["status"].as_str(), Some("superseded"));

    // DELETE /api/v1/workflows/{id}
    let del = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/workflows/{id}"))
        .body(axum::body::Body::empty())
        .unwrap();
    let del_resp = app.clone().oneshot(del).await.unwrap();
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);
}

// --- Evals ----------------------------------------------------------

#[tokio::test]
async fn test_evals_create_run_delete_round_trip() {
    let app = make_app().await;
    let name = format!("smoke_{}", std::process::id());

    // POST /api/v1/evals
    let create = Request::builder()
        .method("POST")
        .uri("/api/v1/evals")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            json!({
                "name": name,
                "cases": [{
                    "id": "case1",
                    "input": "echo",
                    "expected_keywords": ["echo"],
                    "expected_output": "echo: hello"
                }]
            })
            .to_string(),
        ))
        .unwrap();
    let create_resp = app.clone().oneshot(create).await.unwrap();
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let created = body_json(create_resp).await;
    assert_eq!(created["name"].as_str(), Some(name.as_str()));

    // POST /api/v1/evals/{name}/run — exercises the keyword metric
    // path; the StubAgent returns "echo: hello" which matches the
    // expected_output, so the case should pass.
    let run = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/evals/{name}/run"))
        .body(axum::body::Body::empty())
        .unwrap();
    let run_resp = app.clone().oneshot(run).await.unwrap();
    assert_eq!(run_resp.status(), StatusCode::OK);
    let report = body_json(run_resp).await;
    assert_eq!(report["suite_name"].as_str(), Some(name.as_str()));
    assert_eq!(report["total"].as_u64(), Some(1));
    assert_eq!(report["passed"].as_u64(), Some(1));

    // DELETE /api/v1/evals/{name}
    let del = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/evals/{name}"))
        .body(axum::body::Body::empty())
        .unwrap();
    let del_resp = app.clone().oneshot(del).await.unwrap();
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);
}

// --- Tasks ----------------------------------------------------------

#[tokio::test]
async fn test_tasks_list_returns_empty_when_no_sessions() {
    // A fresh `for_test` workspace has no sessions — list_tasks
    // walks `state.session_manager.list_all()` which returns empty.
    // The endpoint must respond 200 with an empty list, not 404.
    let app = make_app().await;
    let req = Request::builder()
        .uri("/api/v1/tasks?limit=10")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert!(body["data"].is_array());
}
