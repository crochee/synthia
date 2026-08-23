//! Integration tests for the `/api/v1/*` management surface.
//!
//! Asserts the versioned management routes behave correctly when
//! the synthetic `for_test` `AppState` is in play:
//!
//! - `GET /api/v1/tools`               → 200 with the registered tools.
//! - `GET /api/v1/memory/search?q=...` → 200 with a `data: []`
//!   envelope when no memories exist.
//! - `GET /api/v1/tasks/{id}`          → 404 for an unknown id.
//!
//! These tests catch regressions in route registration and the
//! cursor-paginated `List<T>` envelope contract.

use std::sync::Arc;

use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use synthia_server::{create_router, state::AppState};
use tower::ServiceExt;

async fn make_app() -> axum::Router {
    let temp = tempfile::TempDir::new().unwrap();
    let session_manager = synthia_session::manager::SessionRegistry::new(
        temp.path().to_path_buf(),
    );
    let state =
        AppState::for_test(session_manager, temp.path().to_path_buf()).await;
    create_router(Arc::new(state)).await
}

#[tokio::test]
async fn test_tools_list_returns_data_envelope() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/api/v1/tools")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let list: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    // Tools list uses the `List<T>` envelope (same as providers,
    // tasks, skills).
    assert!(
        list["data"].is_array(),
        "tools list must use `List<T>` envelope, got: {list}"
    );
}

#[tokio::test]
async fn test_memory_search_returns_data_envelope() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/api/v1/memory/search?q=hello")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let list: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    // Even with no memory entries, the route must return the
    // `List<T>` envelope so the frontend can iterate uniformly.
    assert!(
        list["data"].is_array(),
        "/api/v1/memory/search must use List<T> envelope, got: {list}"
    );
}

#[tokio::test]
async fn test_memory_search_missing_query_returns_4xx() {
    let app = make_app().await;

    // No `q` parameter — the route should reject the request
    // rather than silently returning all memories.
    let req = Request::builder()
        .uri("/api/v1/memory/search")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status().is_client_error(),
        "/api/v1/memory/search without `q` must be 4xx, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_session_get_returns_404_for_unknown_id() {
    let app = make_app().await;

    let req = Request::builder()
        .uri("/api/v1/sessions/this-does-not-exist")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_skills_crud_round_trip() {
    // CRUD was restored in turn 13 of the 2026-08-15 optimization
    // pass (Task 3: full lifecycle management for skill/tool/agent).
    // This test exercises POST → GET → DELETE on a freshly-created
    // skill and verifies that the response shape matches what the
    // `SkillsPage.tsx` UI consumes.
    use serde_json::json as j;
    let app = make_app().await;
    let name = format!("crud_test_{}", std::process::id());

    // POST /api/v1/skills
    let create_req = Request::builder()
        .uri("/api/v1/skills")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(j!({
            "name": name,
            "content": "---\nname: crud_test\ndescription: CRUD round-trip\n---\n\n# body\n"
        }).to_string()))
        .unwrap();
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK, "POST /skills");

    // GET /api/v1/skills/{name}
    let get_req = Request::builder()
        .uri(format!("/api/v1/skills/{name}"))
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let get_resp = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK, "GET /skills/{{name}}");

    // POST /api/v1/skills/reload
    let reload_req = Request::builder()
        .uri("/api/v1/skills/reload")
        .method("POST")
        .body(axum::body::Body::empty())
        .unwrap();
    let reload_resp = app.clone().oneshot(reload_req).await.unwrap();
    assert_eq!(reload_resp.status(), StatusCode::OK, "POST /skills/reload");

    // DELETE /api/v1/skills/{name}
    let del_req = Request::builder()
        .uri(format!("/api/v1/skills/{name}"))
        .method("DELETE")
        .body(axum::body::Body::empty())
        .unwrap();
    let del_resp = app.clone().oneshot(del_req).await.unwrap();
    assert_eq!(del_resp.status(), StatusCode::OK, "DELETE /skills/{{name}}");

    // GET on a deleted skill should now be NOT_FOUND.
    let get_after = Request::builder()
        .uri(format!("/api/v1/skills/{name}"))
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let get_after_resp = app.oneshot(get_after).await.unwrap();
    assert_eq!(
        get_after_resp.status(),
        StatusCode::NOT_FOUND,
        "GET on deleted skill must be 404"
    );
}

#[tokio::test]
async fn test_skills_create_duplicate_returns_409() {
    use serde_json::json as j;
    let app = make_app().await;
    let name = format!("dup_test_{}", std::process::id());
    let body = j!({
        "name": name,
        "content": "---\nname: dup\n---\n"
    })
    .to_string();
    let req = |body: String| {
        Request::builder()
            .uri("/api/v1/skills")
            .method("POST")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body))
            .unwrap()
    };
    let r1 = app.clone().oneshot(req(body.clone())).await.unwrap();
    assert_eq!(r1.status(), StatusCode::OK, "first POST must be 200");
    let r2 = app.clone().oneshot(req(body)).await.unwrap();
    assert_eq!(
        r2.status(),
        StatusCode::CONFLICT,
        "duplicate POST must be 409 Conflict"
    );
    // Clean up.
    let cleanup = Request::builder()
        .uri(format!("/api/v1/skills/{name}"))
        .method("DELETE")
        .body(axum::body::Body::empty())
        .unwrap();
    let _ = app.clone().oneshot(cleanup).await;
}

#[tokio::test]
async fn test_skills_delete_missing_returns_404() {
    let app = make_app().await;
    let req = Request::builder()
        .uri("/api/v1/skills/nonexistent_xyz")
        .method("DELETE")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "DELETE on missing skill"
    );
}

#[tokio::test]
async fn test_tools_register_and_unregister_round_trip() {
    use serde_json::json as j;
    let app = make_app().await;
    let name = format!("crud_tool_{}", std::process::id());

    // POST /api/v1/tools
    let create_req = Request::builder()
        .uri("/api/v1/tools")
        .method("POST")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            j!({
                "name": name,
                "description": "crud round-trip tool",
                "input_schema": {"type": "object"}
            })
            .to_string(),
        ))
        .unwrap();
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK, "POST /tools");

    // GET /api/v1/tools/{name}
    let get_req = Request::builder()
        .uri(format!("/api/v1/tools/{name}"))
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let get_resp = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK, "GET /tools/{{name}}");

    // DELETE /api/v1/tools/{name}
    let del_req = Request::builder()
        .uri(format!("/api/v1/tools/{name}"))
        .method("DELETE")
        .body(axum::body::Body::empty())
        .unwrap();
    let del_resp = app.clone().oneshot(del_req).await.unwrap();
    assert_eq!(del_resp.status(), StatusCode::OK, "DELETE /tools/{{name}}");

    // GET on a deleted tool should now be NOT_FOUND.
    let get_after = Request::builder()
        .uri(format!("/api/v1/tools/{name}"))
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let get_after_resp = app.oneshot(get_after).await.unwrap();
    assert_eq!(
        get_after_resp.status(),
        StatusCode::NOT_FOUND,
        "GET on deleted tool must be 404"
    );
}

#[tokio::test]
async fn test_tools_register_duplicate_returns_409() {
    use serde_json::json as j;
    let app = make_app().await;
    let name = format!("dup_tool_{}", std::process::id());
    let body = j!({
        "name": name,
        "description": "x",
        "input_schema": {"type": "object"}
    })
    .to_string();
    let req = |body: String| {
        Request::builder()
            .uri("/api/v1/tools")
            .method("POST")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body))
            .unwrap()
    };
    let r1 = app.clone().oneshot(req(body.clone())).await.unwrap();
    assert_eq!(r1.status(), StatusCode::OK, "first POST /tools");
    let r2 = app.clone().oneshot(req(body)).await.unwrap();
    assert_eq!(
        r2.status(),
        StatusCode::CONFLICT,
        "duplicate POST /tools must be 409"
    );
    // Clean up.
    let cleanup = Request::builder()
        .uri(format!("/api/v1/tools/{name}"))
        .method("DELETE")
        .body(axum::body::Body::empty())
        .unwrap();
    let _ = app.clone().oneshot(cleanup).await;
}

#[tokio::test]
async fn test_tools_unregister_missing_returns_404() {
    // Restored in turn 13: DELETE /api/v1/tools/{name} returns
    // 404 when the tool is not registered (was 405 when unbound).
    let app = make_app().await;
    let req = Request::builder()
        .uri("/api/v1/tools/nonexistent_xyz")
        .method("DELETE")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "DELETE /tools/{{name}} on missing tool"
    );
}
