use crate::rule::PermissionAction;

/// Match a permission pattern against a request path.
/// Pattern format: `segment1:segment2:segment3` with `*` glob support per segment.
/// `*` matches any characters except `:` within the same segment.
pub fn match_pattern(pattern: &str, request: &str) -> bool {
    let pattern_segments: Vec<&str> = pattern.split(':').collect();
    let request_segments: Vec<&str> = request.split(':').collect();

    if pattern_segments.len() != request_segments.len() {
        return false;
    }

    for (p, r) in pattern_segments.iter().zip(request_segments.iter()) {
        if !glob_match(p, r) {
            return false;
        }
    }
    true
}

fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 1 {
            return pattern == text;
        }
        let mut last_end = 0;
        for part in parts {
            if part.is_empty() {
                continue;
            }
            if let Some(pos) = text[last_end..].find(part) {
                last_end = pos + part.len();
            } else {
                return false;
            }
        }
        true
    } else {
        pattern == text
    }
}

/// Evaluate a pattern list against a request, returning the first matching action.
pub fn evaluate_patterns(
    patterns: &[(String, PermissionAction)],
    request: &str,
) -> Option<PermissionAction> {
    for (pattern, action) in patterns {
        if match_pattern(pattern, request) {
            return Some(*action);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert!(match_pattern("bash:rm", "bash:rm"));
        assert!(!match_pattern("bash:rm", "bash:ls"));
    }

    #[test]
    fn test_glob_match() {
        assert!(match_pattern("bash:rm*", "bash:rm"));
        assert!(match_pattern("bash:rm*", "bash:rm -rf /"));
        assert!(!match_pattern("bash:rm*", "bash:ls"));
    }

    #[test]
    fn test_multi_segment() {
        assert!(match_pattern("bash:rm:*", "bash:rm:-rf"));
        assert!(match_pattern("*:ls", "anything:ls"));
    }

    #[test]
    fn test_length_mismatch() {
        assert!(!match_pattern("bash:rm", "bash:rm:force"));
        assert!(!match_pattern("bash:rm:force", "bash:rm"));
    }

    #[test]
    fn test_evaluate_patterns_first_match_wins() {
        let patterns = vec![
            ("bash:*".to_string(), PermissionAction::Deny),
            ("bash:ls".to_string(), PermissionAction::Allow),
        ];
        assert_eq!(
            evaluate_patterns(&patterns, "bash:ls"),
            Some(PermissionAction::Deny)
        );
        assert_eq!(
            evaluate_patterns(&patterns, "bash:rm -rf /"),
            Some(PermissionAction::Deny)
        );
        assert_eq!(evaluate_patterns(&patterns, "read:foo"), None);
    }
}
