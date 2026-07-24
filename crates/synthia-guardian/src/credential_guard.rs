//! Credential protection: prevents accidental credential leaks in tool output.
//!
//! Scans and redacts common credential patterns from logs and output to prevent
//! accidental exposure of secrets, API keys, and tokens.

use regex::Regex;

use crate::types::{SecurityEvent, SecurityEventType, SecuritySeverity};

/// Compiles a regex pattern, panicking if the pattern is invalid.
/// All patterns are compile-time constants, so this is safe.
fn re(pattern: &str) -> Regex {
    #[allow(clippy::expect_used)]
    Regex::new(pattern).expect("invalid regex pattern in credential guard")
}

/// A detected credential match
#[derive(Debug)]
pub struct CredentialMatch {
    /// The pattern that matched
    pub pattern_name: String,
    /// The matched text
    pub matched_text: String,
    /// Start position
    pub start: usize,
    /// End position
    pub end: usize,
}

impl CredentialMatch {
    /// Creates a security event from this credential match
    pub fn to_event(&self) -> SecurityEvent {
        SecurityEvent::new(
            SecurityEventType::CredentialLeak,
            format!(
                "Credential pattern detected: {} matched '{}...'",
                self.pattern_name,
                &self.matched_text[..self.matched_text.len().min(20)]
            ),
            None,
            SecuritySeverity::High,
        )
    }
}

/// Pattern entry for credential detection
struct CredentialPattern {
    regex: Regex,
    name: &'static str,
}

impl CredentialPattern {
    fn new(pattern: &str, name: &'static str) -> Self {
        Self {
            regex: re(pattern),
            name,
        }
    }
}

/// Guards against credential exposure in tool output.
///
/// Scans output for common credential patterns and provides redaction.
/// All detection is deterministic and rule-based (P6: Distrust by Default).
pub struct CredentialGuard {
    patterns: Vec<CredentialPattern>,
}

impl CredentialGuard {
    /// Creates a new credential guard with comprehensive patterns
    #[must_use]
    pub fn new() -> Self {
        let patterns = vec![
            // API keys and tokens (generic)
            CredentialPattern::new(
                r"(?i)(?:api[_-]?key|apikey)\s*[:=]\s*[a-zA-Z0-9]{16,}",
                "api_key",
            ),
            CredentialPattern::new(
                r"(?i)(?:token|access_token|auth_token)\s*[:=]\s*[a-zA-Z0-9]{16,}",
                "token",
            ),
            CredentialPattern::new(
                r"(?i)(?:password|passwd|pwd)\s*[:=]\s*\S+",
                "password",
            ),
            CredentialPattern::new(
                r"(?i)(?:secret|client_secret)\s*[:=]\s*[a-zA-Z0-9]{16,}",
                "secret",
            ),
            // AWS credentials
            CredentialPattern::new(r"AKIA[0-9A-Z]{16}", "aws_access_key"),
            CredentialPattern::new(
                r"(?i)aws[_-]?secret[_-]?access[_-]?key\s*[:=]\s*[a-zA-Z0-9/+=]{40}",
                "aws_secret_key",
            ),
            // GitHub tokens
            CredentialPattern::new(
                r"ghp_[a-zA-Z0-9]{36}",
                "github_personal_token",
            ),
            CredentialPattern::new(
                r"gho_[a-zA-Z0-9]{36}",
                "github_oauth_token",
            ),
            // Private keys
            CredentialPattern::new(
                r"-----BEGIN (?:RSA |EC )?PRIVATE KEY-----",
                "private_key_header",
            ),
            // Connection strings with credentials
            CredentialPattern::new(
                r"(?i)(?:mongodb|postgres|mysql|redis)://[^:]+:[^@]+@",
                "connection_string",
            ),
            // JWT tokens
            CredentialPattern::new(
                r"eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}",
                "jwt_token",
            ),
        ];

        Self { patterns }
    }

    /// Scans output for credentials and returns redacted version.
    ///
    /// Returns a tuple of (redacted_output, found_credentials).
    #[must_use]
    pub fn scan_and_redact(&self, output: &str) -> (String, bool) {
        let mut found = false;
        let mut result = output.to_string();

        for pattern in &self.patterns {
            if pattern.regex.is_match(&result) {
                found = true;
                result = pattern
                    .regex
                    .replace_all(&result, "[REDACTED]")
                    .to_string();
            }
        }

        (result, found)
    }

    /// Checks if output contains any credential patterns.
    #[must_use]
    pub fn contains_credentials(&self, output: &str) -> bool {
        self.patterns.iter().any(|p| p.regex.is_match(output))
    }

    /// Scans for specific credential matches with details.
    #[must_use]
    pub fn scan_detailed(&self, output: &str) -> Vec<CredentialMatch> {
        let mut matches = Vec::new();

        for pattern in &self.patterns {
            if let Some(m) = pattern.regex.find(output) {
                matches.push(CredentialMatch {
                    pattern_name: pattern.name.to_string(),
                    matched_text: output[m.start()..m.end()].to_string(),
                    start: m.start(),
                    end: m.end(),
                });
            }
        }

        matches
    }

    /// Returns the number of configured patterns.
    #[must_use]
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }
}

impl Default for CredentialGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_api_key() {
        let guard = CredentialGuard::new();
        let output = "api_key: abcdefghijklmnopqrstuvwxyz123456";
        let (redacted, found) = guard.scan_and_redact(output);
        assert!(found);
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn test_detect_aws_key() {
        let guard = CredentialGuard::new();
        let output = "Found key: AKIAIOSFODNN7EXAMPLE";
        let (redacted, found) = guard.scan_and_redact(output);
        assert!(found);
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn test_detect_github_token() {
        let guard = CredentialGuard::new();
        let output = "token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
        let (redacted, found) = guard.scan_and_redact(output);
        assert!(found);
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn test_detect_private_key() {
        let guard = CredentialGuard::new();
        let output = "-----BEGIN RSA PRIVATE KEY-----\nMIIE...";
        let (redacted, found) = guard.scan_and_redact(output);
        assert!(found);
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn test_clean_output() {
        let guard = CredentialGuard::new();
        let output = "Hello world, this is clean output";
        let (redacted, found) = guard.scan_and_redact(output);
        assert!(!found);
        assert_eq!(redacted, output);
    }

    #[test]
    fn test_contains_credentials() {
        let guard = CredentialGuard::new();
        assert!(guard.contains_credentials("password: secret123"));
        assert!(!guard.contains_credentials("clean output"));
    }

    #[test]
    fn test_scan_detailed() {
        let guard = CredentialGuard::new();
        let output = "api_key: abcdefghijklmnopqrstuvwxyz123456";
        let matches = guard.scan_detailed(output);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].pattern_name, "api_key");
    }

    #[test]
    fn test_jwt_detection() {
        let guard = CredentialGuard::new();
        let output = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        assert!(guard.contains_credentials(output));
    }

    #[test]
    fn test_connection_string_detection() {
        let guard = CredentialGuard::new();
        let output = "mongodb://user:password123@localhost:27017/db";
        assert!(guard.contains_credentials(output));
    }
}
