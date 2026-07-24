use std::collections::HashSet;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

use crate::{
    builtin::utf8_safe::cap_to_char_boundary,
    traits::Tool,
    types::{ToolInput, ToolOutput},
};

const MAX_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
struct WebFetchArgs {
    url: String,
    #[serde(default)]
    max_length: Option<usize>,
}

#[derive(Debug)]
pub struct WebFetchTool {
    allowed_hosts: HashSet<String>,
    client: Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            allowed_hosts: HashSet::new(),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::limited(5))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn with_allowed_hosts(mut self, hosts: HashSet<String>) -> Self {
        self.allowed_hosts = hosts;
        self
    }

    fn is_url_allowed(&self, url: &str) -> bool {
        if self.allowed_hosts.is_empty() {
            return true;
        }
        if let Ok(parsed) = url::Url::parse(url)
            && let Some(host) = parsed.host_str()
        {
            return self.allowed_hosts.contains(host);
        }
        false
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetches content from a specified URL"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to fetch content from"
                },
                "max_length": {
                    "type": "integer",
                    "description": "Maximum content length to return"
                }
            }
        })
    }

    fn requires_permission(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        // Pure GET request — no shared mutable state. Multiple parallel
        // fetches to different URLs are safe.
        true
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let args: WebFetchArgs =
            match serde_json::from_value(input.input.clone()) {
                Ok(a) => a,
                Err(e) => {
                    return ToolOutput::error(format!(
                        "Invalid arguments: {}",
                        e
                    ));
                }
            };

        if args.url.is_empty() {
            return ToolOutput::error("Missing URL parameter");
        }

        if !self.is_url_allowed(&args.url) {
            return ToolOutput::error(format!(
                "Host not allowed: {}. Configure allowed_hosts.",
                args.url
            ));
        }

        let response = match self.client.get(&args.url).send().await {
            Ok(r) => r,
            Err(e) => {
                return ToolOutput::error(format!(
                    "HTTP request failed: {}",
                    e
                ));
            }
        };

        let status = response.status();
        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => {
                return ToolOutput::error(format!(
                    "Failed to read response body: {}",
                    e
                ));
            }
        };

        let max_len = args.max_length.unwrap_or(MAX_RESPONSE_BYTES);
        let (body, _) = Self::truncate_response_body(body, max_len);

        let content = if body.is_empty() {
            format!("Status: {}\n\n(empty body)", status)
        } else {
            format!("Status: {}\n\n{}", status, body)
        };

        ToolOutput::text(content)
    }
}

impl WebFetchTool {
    /// Truncate a response body to at most `max_len` bytes using
    /// UTF-8 safe boundaries, appending a marker on truncation.
    ///
    /// This is a static, side-effect-free helper extracted from
    /// `call` so the truncation logic can be unit-tested without
    /// hitting the network. The full HTTP path is exercised by the
    /// agent-level integration tests; this helper covers the
    /// boundary-safety contract.
    ///
    /// Returns `(truncated_body, was_truncated)`.
    pub fn truncate_response_body(
        body: String,
        max_len: usize,
    ) -> (String, bool) {
        if body.len() > max_len {
            let mut truncated = body;
            cap_to_char_boundary(&mut truncated, max_len);
            let marker =
                format!("\n\n[Response truncated at {} bytes]", max_len);
            (format!("{}{}", truncated, marker), true)
        } else {
            (body, false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_fetch_is_concurrency_safe() {
        // Pure GET — parallel fetches to different URLs are safe.
        let tool = WebFetchTool::new();
        assert!(tool.is_concurrency_safe());
    }

    #[test]
    fn test_web_fetch_empty_allowed_hosts_allows_all() {
        // Default policy: empty allowlist = permit all.
        let tool = WebFetchTool::new();
        assert!(tool.is_url_allowed("https://example.com"));
    }

    #[test]
    fn test_web_fetch_allowed_hosts_filter() {
        let mut hosts = HashSet::new();
        hosts.insert("allowed.example".to_string());
        let tool = WebFetchTool::new().with_allowed_hosts(hosts);
        assert!(tool.is_url_allowed("https://allowed.example/path"));
        assert!(!tool.is_url_allowed("https://blocked.example/path"));
    }
}
