//! Standard V2 response envelope.

use axum::{Json, response::IntoResponse};
use serde::{Deserialize, Serialize};

/// Generic V2 success envelope wrapping a single data value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiResponse<T> {
    pub data: T,
}

/// Wrap `data` in the standard `{ "data": ... }` JSON response envelope.
pub fn json_data<T: Serialize>(data: T) -> impl IntoResponse {
    Json(ApiResponse { data })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_response_serializes_to_data_envelope() {
        let response = ApiResponse { data: 42 };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#"{"data":42}"#);
    }

    #[tokio::test]
    async fn json_data_helper_returns_data_envelope() {
        let response = json_data("hello".to_string()).into_response();
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value, serde_json::json!({ "data": "hello" }));
    }
}
