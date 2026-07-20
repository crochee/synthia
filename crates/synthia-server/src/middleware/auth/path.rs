/// Paths that bypass authentication (health checks, A2A discovery).
pub(super) const PUBLIC_PATHS: &[&str] =
    &["/health", "/.well-known/agent-card.json"];

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
