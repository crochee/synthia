use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use metrics_exporter_prometheus::PrometheusHandle;

static PROMETHEUS_HANDLE: std::sync::OnceLock<PrometheusHandle> = std::sync::OnceLock::new();

pub fn init_recorder() -> &'static PrometheusHandle {
    PROMETHEUS_HANDLE.get_or_init(|| {
        let recorder = metrics_exporter_prometheus::PrometheusBuilder::new()
            .build_recorder();
        let handle = recorder.handle();

        if metrics::set_global_recorder(recorder).is_err() {
            tracing::warn!("Prometheus recorder already installed globally");
        }

        handle
    })
}

pub async fn metrics_handler() -> impl IntoResponse {
    let handle = init_recorder();
    let body = handle.render();

    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type",
        HeaderValue::from_static("text/plain; version=0.0.4"),
    );

    (StatusCode::OK, headers, body)
}
