// test fixture: synthia-server router.rs (synthia-style layout)
// Models a Router::new()...route()...route(); pattern, plus a few
// let VAR = Router::new()...; blocks that nest via Router::new().nest(...).

fn router() -> Router {
    // Top-level (no prefix)
    let public = Router::new()
        .route("/health", get(health_check))
        .route(
            "/.well-known/agent-card.json",
            get(get_agent_card),
        );

    // Nestable under "/api"
    let api_routes = Router::new()
        .route("/models", get(list_models))
        .route("/tasks", get(list_tasks))
        .route("/tasks", post(create_task))
        .route("/jobs/{key}", delete(remove_job))
        .route("/jobs/{key}/execute", post(execute_job));

    // Nestable under "/api/approvals"
    let approval_routes = Router::new()
        .route("/", get(list_approvals))
        .route("/{id}/resolve", post(resolve_approval));

    // One-line nested alias (covers ws_routes style: short chained call)
    let ws_routes =
        Router::new().route("/ws/approvals", get(ws_handler));

    Router::new()
        .merge(public)
        .nest("/api", api_routes)
        .nest("/api/approvals", approval_routes)
        .merge(ws_routes)
}
