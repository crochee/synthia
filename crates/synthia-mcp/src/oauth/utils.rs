//! 2 helper functions used by the OAuth flow.

/// Simple URL encoding for OAuth parameters.
pub(super) fn url_encode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC)
        .to_string()
}

/// Generate a random state string for OAuth authorization code flow.
pub(super) fn generate_state() -> String {
    use rand::Rng;
    let random_bytes: [u8; 32] = rand::thread_rng().r#gen();
    hex::encode(random_bytes)
}
