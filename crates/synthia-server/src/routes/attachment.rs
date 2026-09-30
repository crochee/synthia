//! R11 — Multimodal attachment routes.
//!
//! `POST   /attachments` — upload raw image bytes (MIME +
//! base64-encoded body) and receive an [`ImageAttachmentRef`].
//! `GET    /attachments/{hash}` — read the stored bytes back.
//! `DELETE /attachments/{hash}` — evict from the cache (the
//! backing store keeps the file; we never delete-on-evict from
//! this layer because the storage is content-addressed and
//! dedup is a separate decision).
//!
//! The handler holds an [`AttachmentStore`] inside
//! [`AppState::attachment_store`] (added R11). The route is
//! gated behind the auth layer like every other `/api/v1/*`
//! surface.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use base64::{
    Engine as _,
    engine::general_purpose::STANDARD as BASE64_STANDARD,
};
use serde::{Deserialize, Serialize};
use synthia::{
    attachment::{AttachmentError, AttachmentStore, ImageAttachmentRef},
    core::Clock,
};

use crate::{
    api::{AppError, AppJson},
    state::AppState,
};

/// `POST /attachments` request body.
#[derive(Debug, Deserialize, validator::Validate)]
pub struct CreateAttachmentRequest {
    /// MIME type (one of `SUPPORTED_IMAGE_MIME`).
    #[validate(custom(function = "validate_mime_field"))]
    pub mime_type: String,
    /// Base64-encoded bytes.
    pub data_base64: String,
}

/// `validator` custom rule for the MIME allow-list. Delegates to
/// `synthia::attachment::validate_mime` and converts the failure
/// into the `validator::ValidationError` shape.
fn validate_mime_field(value: &str) -> Result<(), validator::ValidationError> {
    match synthia::attachment::validate_mime(value) {
        Ok(()) => Ok(()),
        Err(_) => Err(validator::ValidationError::new("unsupported_mime")),
    }
}

/// `POST /attachments` response body.
#[derive(Debug, Serialize)]
pub struct CreateAttachmentResponse {
    /// The canonical, content-addressed ref.
    #[serde(flatten)]
    pub attachment: ImageAttachmentRef,
}

/// `POST /attachments` — save and dedup-by-hash.
pub async fn create_attachment(
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<CreateAttachmentRequest>,
) -> Result<(StatusCode, Json<CreateAttachmentResponse>), AppError> {
    let store: &AttachmentStore = state.attachment_store.as_ref();
    let bytes =
        BASE64_STANDARD
            .decode(req.data_base64.as_bytes())
            .map_err(|e| {
                AppError::new(StatusCode::BAD_REQUEST, anyhow::anyhow!(e))
                    .with_code("invalid_base64")
                    .with_message("invalid base64 payload")
            })?;
    let attachment = store
        .save_image(&bytes, &req.mime_type)
        .map_err(map_attachment_error)?;
    Ok((
        StatusCode::CREATED,
        Json(CreateAttachmentResponse { attachment }),
    ))
}

/// `GET /attachments/{hash}` — return the stored bytes
/// (base64-encoded) plus the canonical ref metadata.
#[derive(Debug, Serialize)]
pub struct GetAttachmentResponse {
    /// The canonical ref metadata.
    #[serde(flatten)]
    pub attachment: ImageAttachmentRef,
    /// Base64-encoded bytes.
    pub data_base64: String,
}

/// `GET /attachments/{hash}`.
pub async fn get_attachment(
    State(state): State<Arc<AppState>>,
    Path(hash): Path<String>,
) -> Result<Json<GetAttachmentResponse>, AppError> {
    let store: &AttachmentStore = state.attachment_store.as_ref();
    let bytes = store.read_base64_raw(&hash).map_err(map_attachment_error)?;
    let byte_len = bytes.len();
    // The backend answers by hash only, so the ref is rebuilt here;
    // its timestamp comes from the store's clock rather than a fresh
    // wall-clock read.
    let attachment = ImageAttachmentRef {
        hash,
        mime_type: "application/octet-stream".to_string(),
        byte_len,
        saved_at: store.clock().now(),
    };
    Ok(Json(GetAttachmentResponse {
        attachment,
        data_base64: BASE64_STANDARD.encode(&bytes),
    }))
}

/// `DELETE /attachments/{hash}` — evict the cache entry.
pub async fn delete_attachment(
    State(state): State<Arc<AppState>>,
    Path(hash): Path<String>,
) -> Result<StatusCode, AppError> {
    let store: &AttachmentStore = state.attachment_store.as_ref();
    store.evict(&hash);
    Ok(StatusCode::NO_CONTENT)
}

/// Map an [`AttachmentError`] to an [`AppError`]. Keeps the HTTP
/// layer's error envelope consistent across the API surface.
fn map_attachment_error(e: AttachmentError) -> AppError {
    match e {
        AttachmentError::UnsupportedMime { mime } => {
            AppError::new(
                StatusCode::BAD_REQUEST,
                anyhow::anyhow!("unsupported MIME type: {mime}"),
            )
            .with_code("unsupported_mime")
        }
        AttachmentError::TooLarge { bytes, limit } => AppError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            anyhow::anyhow!("attachment too large: {bytes} bytes (limit {limit})"),
        )
        .with_code("attachment_too_large"),
        AttachmentError::NotFound { hash } => AppError::new(
            StatusCode::NOT_FOUND,
            anyhow::anyhow!("attachment not found: {hash}"),
        )
        .with_code("attachment_not_found"),
        AttachmentError::HashMismatch { expected, actual } => AppError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            anyhow::anyhow!(
                "attachment hash mismatch (storage corrupt): expected {expected}, \
                 got {actual}"
            ),
        )
        .with_code("attachment_hash_mismatch"),
        AttachmentError::Io(e) => AppError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            anyhow::anyhow!("attachment I/O: {e}"),
        )
        .with_code("attachment_io"),
        AttachmentError::Storage(msg) => AppError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            anyhow::anyhow!("attachment storage: {msg}"),
        )
        .with_code("attachment_storage"),
    }
}

/// Mount the attachment routes onto an [`axum::Router`].
pub fn attachment_router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/attachments", post(create_attachment))
        .route(
            "/attachments/{hash}",
            get(get_attachment).delete(delete_attachment),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1x1 transparent PNG payload (69 bytes).
    fn png_bytes() -> Vec<u8> {
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00,
            0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
            0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89,
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63,
            0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x6F, 0x32,
            0x4D, 0x0E, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
            0x42, 0x60, 0x82,
        ]
    }

    #[test]
    fn create_response_flattens_ref_fields() {
        let resp = CreateAttachmentResponse {
            attachment: ImageAttachmentRef {
                hash: "abc123".to_string(),
                mime_type: "image/png".to_string(),
                byte_len: 69,
                saved_at: chrono::DateTime::parse_from_rfc3339(
                    "2026-01-01T12:00:00Z",
                )
                .unwrap()
                .with_timezone(&chrono::Utc),
            },
        };
        let json = serde_json::to_value(&resp).unwrap();
        // `#[serde(flatten)]` — the ref's fields ride at the top
        // level, not nested under `attachment`.
        assert_eq!(json["hash"], "abc123");
        assert_eq!(json["mime_type"], "image/png");
        assert_eq!(json["byte_len"], 69);
    }

    #[test]
    fn mime_validator_rejects_unknown_and_accepts_png() {
        assert!(validate_mime_field("image/png").is_ok());
        assert!(validate_mime_field("video/mp4").is_err());
    }

    #[tokio::test]
    async fn create_attachment_round_trip_through_store() {
        // Directly exercise the store behind the handler so the
        // handler's save → read pipeline is pinned without a
        // full HTTP round-trip.
        let store = AttachmentStore::default();
        let bytes = png_bytes();
        let b64 = BASE64_STANDARD.encode(&bytes);
        // Simulate the decode the handler performs.
        let decoded = BASE64_STANDARD.decode(b64).unwrap();
        let ref_ = store.save_image(&decoded, "image/png").unwrap();
        assert_eq!(ref_.byte_len, bytes.len());
        // Read back through the hash-only path the GET handler
        // uses.
        let read = store.read_base64_raw(&ref_.hash).unwrap();
        assert_eq!(read, bytes);
    }

    #[test]
    fn map_error_translates_not_found_to_404() {
        let err = AttachmentError::NotFound {
            hash: "deadbeef".to_string(),
        };
        let app = map_attachment_error(err);
        assert_eq!(app.status_code, StatusCode::NOT_FOUND);
        assert_eq!(app.code, "attachment_not_found");
    }

    #[test]
    fn map_error_translates_unsupported_mime_to_400() {
        let err = AttachmentError::UnsupportedMime {
            mime: "video/mp4".to_string(),
        };
        let app = map_attachment_error(err);
        assert_eq!(app.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(app.code, "unsupported_mime");
    }

    #[test]
    fn map_error_translates_too_large_to_413() {
        let err = AttachmentError::TooLarge {
            bytes: 9_000_000,
            limit: 8_388_608,
        };
        let app = map_attachment_error(err);
        assert_eq!(app.status_code, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(app.code, "attachment_too_large");
    }
}
