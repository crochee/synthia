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
