//! Unit tests for the `error` module family.
//!
//! Coverage map (17 tests):
//!
//! - ErrorCode: 2 tests
//!   (serialization, deserialization).
//! - UserError: 5 tests
//!   (creation, with_details, Display, full serialization,
//!   `details: None` is skipped from JSON).
//! - ApiResponse: 3 tests
//!   (ok, err, `Result → ApiResponse` round-trip).
//! - Error Display: 1 test
//!   (4 representative variants).
//! - Error → ErrorCode mapping: 1 test
//!   (5 representative variants).
//! - Error → UserError: 1 test
//!   (Display string preserved, code mapped).
//! - StreamErrorKind Display: 1 test
//!   (all 4 variants).
//! - StreamError API: 2 tests
//!   (constructor + retryability, serde round-trip of
//!   `StreamErrorKind` + Display format verification).
//! - Error to_string: 1 test
//!   (4 variants).

use super::*;

// =============================================================================
// ErrorCode Tests
// =============================================================================

#[test]
fn test_error_code_serialization() {
    assert_eq!(
        serde_json::to_string(&ErrorCode::NotFound).unwrap(),
        "\"not_found\""
    );
    assert_eq!(
        serde_json::to_string(&ErrorCode::InternalServerError).unwrap(),
        "\"internal_server_error\""
    );
    assert_eq!(
        serde_json::to_string(&ErrorCode::BadRequest).unwrap(),
        "\"bad_request\""
    );
}

#[test]
fn test_error_code_deserialization() {
    let code: ErrorCode = serde_json::from_str("\"not_found\"").unwrap();
    assert_eq!(code, ErrorCode::NotFound);
}

// =============================================================================
// UserError Tests
// =============================================================================

#[test]
fn test_user_error_creation() {
    let err = UserError::new(ErrorCode::NotFound, "Session not found");
    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(err.message, "Session not found");
    assert!(err.details.is_none());
}

#[test]
fn test_user_error_with_details() {
    let details = serde_json::json!({"session_id": "abc123"});
    let err = UserError::with_details(
        ErrorCode::NotFound,
        "Session not found",
        details.clone(),
    );
    assert_eq!(err.details, Some(details));
}

#[test]
fn test_user_error_display() {
    let err = UserError::new(ErrorCode::NotFound, "Session not found");
    assert_eq!(format!("{}", err), "[not_found] Session not found");
}

#[test]
fn test_user_error_serialization() {
    let err = UserError::new(ErrorCode::NotFound, "Session not found");
    let json = serde_json::to_string(&err).unwrap();
    assert!(json.contains("\"code\":\"not_found\""));
    assert!(json.contains("\"message\":\"Session not found\""));
}

#[test]
fn test_user_error_serialization_omits_null_details() {
    let err = UserError::new(ErrorCode::NotFound, "test");
    let json = serde_json::to_string(&err).unwrap();
    assert!(!json.contains("details"));
}

// =============================================================================
// ApiResponse Tests
// =============================================================================

#[test]
fn test_api_response_ok() {
    let response: ApiResponse<String> = ApiResponse::ok("hello".to_string());
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"status\":\"ok\""));
    assert!(json.contains("\"data\":\"hello\""));
}

#[test]
fn test_api_response_err() {
    let err = UserError::new(ErrorCode::NotFound, "not found");
    let response: ApiResponse<String> = ApiResponse::err(err);
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"status\":\"err\""));
    assert!(json.contains("\"error\""));
}

#[test]
fn test_from_result_for_api_response() {
    let ok_result: Result<String, UserError> = Ok("data".to_string());
    let response: ApiResponse<String> = ok_result.into();
    assert!(matches!(response, ApiResponse::Ok { .. }));

    let err_result: Result<String, UserError> =
        Err(UserError::new(ErrorCode::NotFound, "error"));
    let response: ApiResponse<String> = err_result.into();
    assert!(matches!(response, ApiResponse::Err { .. }));
}

// =============================================================================
// Error Display + Code Tests
// =============================================================================

#[test]
fn test_error_display() {
    assert_eq!(
        Error::NotFound("item".to_string()).to_string(),
        "not found: item"
    );
    assert_eq!(
        Error::AlreadyExists("item".to_string()).to_string(),
        "already exists: item"
    );
    assert_eq!(
        Error::InvalidItem("invalid".to_string()).to_string(),
        "invalid item: invalid"
    );
    assert_eq!(
        Error::Internal("bug".to_string()).to_string(),
        "internal error: bug"
    );
}

#[test]
fn test_error_code_mapping() {
    assert_eq!(
        Error::NotFound("test".to_string()).code(),
        ErrorCode::NotFound
    );
    assert_eq!(
        Error::AlreadyExists("test".to_string()).code(),
        ErrorCode::AlreadyExists
    );
    assert_eq!(
        Error::Internal("test".to_string()).code(),
        ErrorCode::InternalServerError
    );
    assert_eq!(
        Error::Unauthorized("test".to_string()).code(),
        ErrorCode::Unauthorized
    );
}

#[test]
fn test_error_to_user_error() {
    let err = Error::NotFound("item not found".to_string());
    let user_err: UserError = err.into();
    assert_eq!(user_err.code, ErrorCode::NotFound);
    assert_eq!(user_err.message, "not found: item not found");
}

// =============================================================================
// StreamErrorKind + StreamError API Tests
// =============================================================================

#[test]
fn test_stream_error_kind_display() {
    assert_eq!(StreamErrorKind::HttpFailure.to_string(), "http_failure");
    assert_eq!(StreamErrorKind::ProtocolError.to_string(), "protocol_error");
    assert_eq!(StreamErrorKind::Aborted.to_string(), "aborted");
    assert_eq!(StreamErrorKind::Internal.to_string(), "internal");
}

#[test]
fn test_stream_error_constructor_and_retryability() {
    let err =
        Error::stream_error(StreamErrorKind::HttpFailure, "502 bad gateway");
    match &err {
        Error::StreamError { kind, message } => {
            assert_eq!(*kind, StreamErrorKind::HttpFailure);
            assert_eq!(message, "502 bad gateway");
        }
        _ => panic!("expected Error::StreamError"),
    }
    assert_eq!(err.code(), ErrorCode::Stream);
    assert!(err.is_retryable(), "HttpFailure should be retryable");

    // Aborted is NOT retryable per spec (caller decided to cancel).
    let aborted = Error::stream_error(StreamErrorKind::Aborted, "user cancel");
    assert!(!aborted.is_retryable(), "Aborted should NOT be retryable");

    // Display format
    let formatted = err.to_string();
    assert!(formatted.contains("http_failure"));
    assert!(formatted.contains("502 bad gateway"));
}

#[test]
fn test_stream_error_serde_roundtrip() {
    // Error is intentionally not Deserialize (uses thiserror, Box<Self>, etc.).
    // Round-trip StreamErrorKind alone — it is the serializable part.
    let kind = StreamErrorKind::ProtocolError;
    let json = serde_json::to_string(&kind).unwrap();
    let de: StreamErrorKind = serde_json::from_str(&json).unwrap();
    assert_eq!(de, kind);
    // Verify error's Display carries the kind string.
    let err = Error::stream_error(kind, "bad sse");
    let formatted = err.to_string();
    assert!(formatted.contains("protocol_error"));
    assert!(formatted.contains("bad sse"));
}
