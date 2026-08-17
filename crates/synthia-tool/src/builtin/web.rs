use std::collections::HashSet;

use reqwest::Client;
use schemars_derive::JsonSchema;
use serde::Deserialize;
use synthia_core::cap_to_char_boundary;

use crate::{
    traits::{ExecutionMode, Tool},
    types::{Context, ToolOutput},
};

const MAX_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(extend("additionalProperties" = false))]
struct WebFetchArgs {
    #[schemars(description = "URL to fetch content from.")]
    url: String,
    #[serde(default)]
    #[schemars(
        range(min = 1),
        extend("default" = MAX_RESPONSE_BYTES),
        description = "Maximum response length to return (bytes). Default: 65536."
    )]
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

#[async_trait::async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetches content from a specified URL"
    }

    fn parameters(&self) -> serde_json::Value {
        // Schema is generated from `WebFetchArgs` via `schemars`,
        // so the type and the LLM-facing schema cannot drift —
        // including `additionalProperties: false` and the
        // `max_length` default, all declared inline via
        // `#[schemars(extend(...))]`.
        serde_json::to_value(schemars::schema_for!(WebFetchArgs))
            .expect("WebFetchArgs schema is always serializable")
    }

    fn mode(&self) -> ExecutionMode {
        // web_fetch hits external services; treat as
        // sequential so the orchestrator does not parallelise
        // it with sibling mutating tools in the same batch.
        ExecutionMode::Sequential
    }

    async fn call(
        &self,
        input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        let args: WebFetchArgs = match serde_json::from_value(input) {
            Ok(a) => a,
            Err(e) => {
                return ToolOutput::error(format!("Invalid arguments: {}", e));
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
    use crate::types::Context;

    fn make_context() -> Context {
        Context::new("s1".to_string(), std::path::PathBuf::from("/tmp"))
    }

    #[tokio::test]
    async fn test_web_fetch_empty_url_returns_error() {
        let tool = WebFetchTool::new();
        let output = tool
            .call(serde_json::json!({"url": ""}), &make_context())
            .await;
        assert!(output.is_error.unwrap_or(false));
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

    /// Pin the JSON-Schema shape for `web_fetch` so future drift in
    /// either the schema, the typed `WebFetchArgs`, or the runtime
    /// `#[serde(default)]` semantics breaks here instead of at
    /// the LLM boundary.
    #[test]
    fn parameters_schema_is_self_consistent() {
        let tool = WebFetchTool::new();
        let params = tool.parameters();
        assert_eq!(params["type"], "object");
        let required: Vec<&str> = params["required"]
            .as_array()
            .expect("required")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(required, vec!["url"]);

        let props = params["properties"].as_object().expect("properties");
        let url = props["url"].as_object().expect("url");
        assert_eq!(url["type"], "string");
        assert!(url["description"].as_str().is_some());

        let max_len = props["max_length"].as_object().expect("max_length");
        let ty = &max_len["type"];
        assert!(
            ty == "integer"
                || ty.as_array().is_some_and(|arr| {
                    arr.iter().any(|v| v == "integer")
                        && arr.iter().any(|v| v == "null")
                }),
            "max_length type should be integer or [integer, null], got: {ty}"
        );
        assert_eq!(
            max_len["minimum"].as_f64().unwrap() as u64,
            1,
            "max_length must require a positive integer"
        );
        assert_eq!(
            max_len["default"], MAX_RESPONSE_BYTES,
            "max_length schema default must match runtime default"
        );

        assert_eq!(
            params["additionalProperties"], false,
            "additional fields must be rejected to match serde_json::from_value"
        );
    }
}
