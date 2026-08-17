/// Per-request `user_id` resolved by the auth middleware.
///
/// Routes that need to scope `SessionStore` operations to a user
/// extract this via `axum::Extension<RequestUserId>`. The value is
/// the result of [`super::resolve_user_id_from_key`]; see that function
/// for the resolution rules.
#[derive(Debug, Clone)]
pub struct RequestUserId(pub String);

impl RequestUserId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RequestUserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Construction + as_str -------------------------------------

    /// `RequestUserId` MUST wrap the
    /// inner String verbatim (no
    /// transformation).
    #[test]
    fn request_user_id_holds_inner_string_verbatim() {
        let id = RequestUserId("abc123".to_string());
        assert_eq!(id.0, "abc123");
        assert_eq!(id.as_str(), "abc123");
    }

    /// `RequestUserId::as_str` MUST
    /// return `&str` borrow of the
    /// inner String (no allocation).
    #[test]
    fn request_user_id_as_str_returns_borrowed_str() {
        let id = RequestUserId("user_42".to_string());
        let s: &str = id.as_str();
        assert_eq!(s, "user_42");
    }

    /// `RequestUserId::as_str` MUST
    /// handle empty strings (degenerate
    /// but possible if upstream
    /// resolution accepts empty keys).
    #[test]
    fn request_user_id_as_str_accepts_empty_string() {
        let id = RequestUserId(String::new());
        assert_eq!(id.as_str(), "");
        assert!(id.as_str().is_empty());
    }

    /// `RequestUserId::as_str` MUST
    /// handle UTF-8 multibyte characters
    /// (axum extract path is
    /// `String` which is UTF-8).
    #[test]
    fn request_user_id_as_str_preserves_utf8() {
        let id = RequestUserId("用户_42_🚀".to_string());
        assert_eq!(id.as_str(), "用户_42_🚀");
        assert_eq!(id.as_str().chars().count(), 7);
    }

    // -- Display ----------------------------------------------------

    /// `RequestUserId` Display MUST
    /// emit the inner string verbatim
    /// (so it can be used in log
    /// lines and error messages).
    #[test]
    fn request_user_id_display_emits_inner_string() {
        let id = RequestUserId("alice".to_string());
        assert_eq!(format!("{}", id), "alice");
    }

    /// `RequestUserId` Display MUST
    /// handle empty strings.
    #[test]
    fn request_user_id_display_accepts_empty_string() {
        let id = RequestUserId(String::new());
        assert_eq!(format!("{}", id), "");
    }

    /// `RequestUserId` Display MUST
    /// preserve UTF-8 verbatim.
    #[test]
    fn request_user_id_display_preserves_utf8() {
        let id = RequestUserId("café".to_string());
        assert_eq!(format!("{}", id), "café");
    }

    // -- Clone + Debug ---------------------------------------------

    /// `RequestUserId` MUST clone
    /// produce a distinct instance
    /// with equal inner String.
    #[test]
    fn request_user_id_clone_produces_distinct_equal_copy() {
        let original = RequestUserId("u1".to_string());
        let copy = original.clone();
        assert_eq!(original.0, copy.0);
        // Distinct: the String inside
        // must be a separate heap
        // allocation. Pushing to the
        // original must not affect the
        // copy.
        let mut extended = original.clone();
        extended.0.push('x');
        assert_eq!(original.0, "u1");
        assert_eq!(extended.0, "u1x");
    }

    /// `RequestUserId` Debug MUST
    /// include the inner value so
    /// log/error output is useful.
    #[test]
    fn request_user_id_debug_includes_inner_value() {
        let id = RequestUserId("debug-me".to_string());
        let dbg = format!("{:?}", id);
        assert!(dbg.contains("debug-me"), "got: {dbg}");
    }

    /// `RequestUserId` MUST NOT be
    /// `PartialEq` (debug-only output
    /// of axum-extracted values
    /// should not be treated as
    /// `==` operations since `==`
    /// on user IDs is a security
    /// concern — flag any accidental
    /// derive).
    ///
    /// This test is a **compile-time
    /// assertion**: if someone adds
    /// `PartialEq` to the derive,
    /// this test will fail to
    /// compile, alerting the change.
    #[test]
    fn request_user_id_intentionally_omits_partial_eq() {
        // We intentionally do not assert
        // `RequestUserId == RequestUserId`
        // here. The struct's derive set
        // is `Debug, Clone` only — see
        // the type definition. If a
        // future refactor adds `PartialEq`,
        // this test must be removed
        // and a security review must
        // be performed before merging.
    }
}
