//! Request validation helpers for V2 endpoints.

use crate::api::error::{ApiError, ErrorDetail};

pub fn validate_content_not_empty(content: &str) -> Result<(), ApiError> {
    if content.trim().is_empty() {
        Err(ApiError::validation_error(vec![ErrorDetail::new(
            Some("content"),
            "Content cannot be empty",
            "required",
        )]))
    } else {
        Ok(())
    }
}

pub fn validate_priority(_priority: u8) -> Result<(), ApiError> {
    // A `u8` already constrains the value to 0-255. This function exists to
    // make the validation boundary explicit for API consumers.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_content_fails_validation() {
        let err = validate_content_not_empty("").unwrap_err();
        assert_eq!(err.code, "validation_error");
        assert_eq!(err.details.len(), 1);
        assert_eq!(err.details[0].field.as_deref(), Some("content"));
    }

    #[test]
    fn whitespace_only_content_fails_validation() {
        let err = validate_content_not_empty("   \n\t  ").unwrap_err();
        assert_eq!(err.code, "validation_error");
    }

    #[test]
    fn non_empty_content_passes_validation() {
        assert!(validate_content_not_empty("hello").is_ok());
    }

    #[test]
    fn any_u8_priority_passes_validation() {
        assert!(validate_priority(0).is_ok());
        assert!(validate_priority(255).is_ok());
        assert!(validate_priority(128).is_ok());
    }
}
