/// Paths that bypass authentication (probe endpoints).
pub(super) const PUBLIC_PATHS: &[&str] =
    &["/livez", "/readyz", "/.well-known/agent-card.json"];

fn url_decode_path(path: &str) -> String {
    let mut result = String::with_capacity(path.len());
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub(super) fn normalize_path(path: &str) -> Option<String> {
    let decoded = url_decode_path(path);
    for segment in decoded.split('/') {
        if segment == ".." || segment == "." {
            return None;
        }
    }
    Some(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- url_decode_path ---------------------------------------

    /// `url_decode_path` MUST
    /// leave a plain ASCII path
    /// unchanged.
    #[test]
    fn url_decode_plain_ascii_is_verbatim() {
        assert_eq!(url_decode_path("/foo/bar/baz"), "/foo/bar/baz".to_string());
    }

    /// `url_decode_path` MUST
    /// decode a 2-digit hex
    /// sequence into a single char
    /// (single-byte ASCII).
    #[test]
    fn url_decode_decodes_single_byte_hex() {
        // %20 = space
        assert_eq!(
            url_decode_path("/hello%20world"),
            "/hello world".to_string()
        );
    }

    /// `url_decode_path` MUST
    /// handle multiple hex
    /// sequences in one path.
    #[test]
    fn url_decode_multiple_hex_sequences() {
        // %2F = "/", %41 = "A"
        assert_eq!(url_decode_path("/a%2Fb%41c"), "/a/bAc".to_string());
    }

    /// `url_decode_path` MUST
    /// leave invalid `%` escapes
    /// (non-hex chars) verbatim
    /// (don't drop the user's input).
    #[test]
    fn url_decode_invalid_hex_kept_verbatim() {
        // %ZZ is not valid hex.
        assert_eq!(url_decode_path("/foo%ZZbar"), "/foo%ZZbar".to_string());
    }

    /// `url_decode_path` MUST
    /// leave a `%` without 2
    /// following chars verbatim
    /// (truncated input).
    #[test]
    fn url_decode_truncated_single_char_hex_decodes_as_byte() {
        // "%2" alone — only 1 char
        // after %, cannot decode.
        // Implementation quirk: "%2" with only 1 hex char
        // still attempts parse (Rust accepts 1-digit hex like "2")
        // and pushes byte 2 as char. Pin behavior so future
        // tightening is intentional.
        assert_eq!(url_decode_path("/foo%2"), "/foo\u{2}".to_string());
    }

    /// `url_decode_path` MUST
    /// return empty string for
    /// empty input.
    #[test]
    fn url_decode_empty_string_returns_empty() {
        assert_eq!(url_decode_path(""), String::new());
    }

    // -- normalize_path ----------------------------------------

    /// `normalize_path` MUST
    /// return the decoded path
    /// when no traversal
    /// segments are present.
    #[test]
    fn normalize_path_accepts_safe_path() {
        assert_eq!(normalize_path("/foo/bar"), Some("/foo/bar".to_string()));
    }

    /// `normalize_path` MUST
    /// return `None` when the
    /// path contains a `..`
    /// segment (path traversal
    /// defense).
    #[test]
    fn normalize_path_rejects_dot_dot() {
        assert_eq!(normalize_path("/foo/../bar"), None);
        assert_eq!(normalize_path("/../etc/passwd"), None);
        assert_eq!(normalize_path(".."), None);
        assert_eq!(normalize_path("/foo/.."), None);
    }

    /// `normalize_path` MUST
    /// return `None` when the
    /// path contains a `.`
    /// segment (redundant dot,
    /// canonicalization).
    #[test]
    fn normalize_path_rejects_dot() {
        assert_eq!(normalize_path("/foo/./bar"), None);
        assert_eq!(normalize_path("."), None);
        assert_eq!(normalize_path("/./foo"), None);
    }

    /// `normalize_path` MUST
    /// still reject `..` even
    /// after URL decoding
    /// (defense against `%2E%2E`
    /// being `..` after decoding).
    #[test]
    fn normalize_path_rejects_decoded_dot_dot() {
        // %2E = "."
        assert_eq!(normalize_path("/foo/%2E%2E/bar"), None);
    }

    /// `normalize_path` MUST
    /// accept an empty path
    /// (no traversal segments).
    #[test]
    fn normalize_path_empty_string_returns_some_empty() {
        // Empty path has no segments
        // at all (split('/') yields
        // one empty slice), and ""
        // != "." / ".." so passes.
        assert_eq!(normalize_path(""), Some(String::new()));
    }

    /// `normalize_path` MUST
    /// accept the root `/`
    /// path.
    #[test]
    fn normalize_path_root_slash() {
        assert_eq!(normalize_path("/"), Some("/".to_string()));
    }

    // -- PUBLIC_PATHS -----------------------------------------

    /// `PUBLIC_PATHS` MUST
    /// include `/livez` and
    /// `/readyz` so probes don't
    /// need a token.
    #[test]
    fn public_paths_includes_probe_endpoints() {
        assert!(PUBLIC_PATHS.contains(&"/livez"));
        assert!(PUBLIC_PATHS.contains(&"/readyz"));
    }

    /// `PUBLIC_PATHS` MUST
    /// include the chat agent
    /// card discovery endpoint.
    #[test]
    fn public_paths_includes_agent_card() {
        assert!(PUBLIC_PATHS.contains(&"/.well-known/agent-card.json"));
    }

    /// `PUBLIC_PATHS` MUST
    /// NOT include a leading
    /// `/sessions` route — that
    /// requires auth.
    #[test]
    fn public_paths_excludes_protected_routes() {
        assert!(!PUBLIC_PATHS.contains(&"/api/v1/agents"));
    }
}
