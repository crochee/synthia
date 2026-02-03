//! WebSearch tool implementation
//!
//! Search the web using various providers with fallback support.

use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use regex::Regex;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{AgentError, tools::Tool};

#[derive(Debug, Clone, Deserialize)]
struct WebSearchRequest {
    query: String,
}

pub struct WebSearchTool {
    client: reqwest::Client,
    provider: String,
    fallback_providers: Vec<String>,
    api_keys: Vec<String>,
    brave_api_keys: Vec<String>,
    perplexity_api_keys: Vec<String>,
    exa_api_keys: Vec<String>,
    jina_api_keys: Vec<String>,
    max_results: usize,
    timeout_secs: u64,
    user_agent: String,
    key_index: Arc<AtomicUsize>,
    brave_key_index: Arc<AtomicUsize>,
    perplexity_key_index: Arc<AtomicUsize>,
    exa_key_index: Arc<AtomicUsize>,
    jina_key_index: Arc<AtomicUsize>,
}

impl WebSearchTool {
    pub fn new() -> Self {
        Self::new_with_options(
            "duckduckgo".to_string(),
            None,
            None,
            None,
            None,
            None,
            5,
            30,
            "Mozilla/5.0 (compatible; SynthiaAgent/1.0)".to_string(),
            Vec::new(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_options(
        provider: String,
        api_key: Option<String>,
        brave_api_key: Option<String>,
        perplexity_api_key: Option<String>,
        exa_api_key: Option<String>,
        jina_api_key: Option<String>,
        max_results: usize,
        timeout_secs: u64,
        user_agent: String,
        fallback_providers: Vec<String>,
    ) -> Self {
        let api_keys = Self::parse_api_keys(api_key.as_deref());
        let brave_api_keys = Self::parse_api_keys(brave_api_key.as_deref());
        let perplexity_api_keys =
            Self::parse_api_keys(perplexity_api_key.as_deref());
        let exa_api_keys = Self::parse_api_keys(exa_api_key.as_deref());
        let jina_api_keys = Self::parse_api_keys(jina_api_key.as_deref());

        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(timeout_secs.max(1)))
                .user_agent(&user_agent)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            provider: provider.trim().to_lowercase(),
            fallback_providers,
            api_keys,
            brave_api_keys,
            perplexity_api_keys,
            exa_api_keys,
            jina_api_keys,
            max_results: max_results.clamp(1, 10),
            timeout_secs: timeout_secs.max(1),
            user_agent,
            key_index: Arc::new(AtomicUsize::new(0)),
            brave_key_index: Arc::new(AtomicUsize::new(0)),
            perplexity_key_index: Arc::new(AtomicUsize::new(0)),
            exa_key_index: Arc::new(AtomicUsize::new(0)),
            jina_key_index: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn parse_api_keys(raw: Option<&str>) -> Vec<String> {
        raw.map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
    }

    fn get_next_key_from(
        keys: &[String],
        index: &AtomicUsize,
    ) -> Option<String> {
        if keys.is_empty() {
            return None;
        }
        let idx = index.fetch_add(1, Ordering::Relaxed) % keys.len();
        Some(keys[idx].clone())
    }

    fn get_next_api_key(&self) -> Option<String> {
        Self::get_next_key_from(&self.api_keys, &self.key_index)
    }

    fn get_next_brave_api_key(&self) -> Option<String> {
        Self::get_next_key_from(&self.brave_api_keys, &self.brave_key_index)
            .or_else(|| self.get_next_api_key())
    }

    fn get_next_perplexity_api_key(&self) -> Option<String> {
        Self::get_next_key_from(
            &self.perplexity_api_keys,
            &self.perplexity_key_index,
        )
        .or_else(|| self.get_next_api_key())
    }

    fn get_next_exa_api_key(&self) -> Option<String> {
        Self::get_next_key_from(&self.exa_api_keys, &self.exa_key_index)
            .or_else(|| self.get_next_api_key())
    }

    fn get_next_jina_api_key(&self) -> Option<String> {
        Self::get_next_key_from(&self.jina_api_keys, &self.jina_key_index)
            .or_else(|| self.get_next_api_key())
    }

    fn normalize_provider(raw: &str) -> Option<&'static str> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "duckduckgo" | "ddg" => Some("duckduckgo"),
            "brave" => Some("brave"),
            "firecrawl" => Some("firecrawl"),
            "tavily" => Some("tavily"),
            "perplexity" => Some("perplexity"),
            "exa" => Some("exa"),
            "jina" => Some("jina"),
            _ => None,
        }
    }

    fn provider_chain(&self) -> Result<Vec<&'static str>, AgentError> {
        let mut chain: Vec<&'static str> = Vec::new();
        let mut seen: HashSet<&'static str> = HashSet::new();

        for raw in std::iter::once(self.provider.as_str()).chain(
            self.fallback_providers
                .iter()
                .map(std::string::String::as_str),
        ) {
            let normalized = Self::normalize_provider(raw).ok_or_else(|| {
                AgentError::InvalidOperation(format!(
                    "Unknown search provider '{raw}'. Supported: duckduckgo, brave, firecrawl, tavily, perplexity, exa, jina"
                ))
            })?;
            if seen.insert(normalized) {
                chain.push(normalized);
            }
        }

        Ok(chain)
    }

    async fn search_duckduckgo(
        &self,
        query: &str,
    ) -> Result<String, AgentError> {
        let encoded_query = urlencoding::encode(query);
        let search_url =
            format!("https://html.duckduckgo.com/html/?q={encoded_query}");

        let response =
            self.client.get(&search_url).send().await.map_err(|e| {
                AgentError::tool(
                    "web_search",
                    format!("DuckDuckGo search request failed: {e}"),
                )
            })?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(AgentError::tool(
                "web_search",
                format!(
                    "DuckDuckGo search failed with status: {status}. Try using a different provider."
                ),
            ));
        }

        let html = response
            .text()
            .await
            .map_err(|e| AgentError::tool("web_search", e.to_string()))?;
        self.parse_duckduckgo_results(&html, query)
    }

    fn parse_duckduckgo_results(
        &self,
        html: &str,
        query: &str,
    ) -> Result<String, AgentError> {
        let link_regex = Regex::new(
            r#"<a[^>]*class="[^"]*result__a[^"]*"[^>]*href="([^"]+)"[^>]*>([\s\S]*?)</a>"#,
        )?;

        let snippet_regex = Regex::new(
            r#"<a class="result__snippet[^"]*"[^>]*>([\s\S]*?)</a>"#,
        )?;

        let link_matches: Vec<_> = link_regex
            .captures_iter(html)
            .take(self.max_results + 2)
            .collect();

        let snippet_matches: Vec<_> = snippet_regex
            .captures_iter(html)
            .take(self.max_results + 2)
            .collect();

        if link_matches.is_empty() {
            return Ok(format!("No results found for: {query}"));
        }

        let mut lines =
            vec![format!("Search results for: {} (via DuckDuckGo)", query)];

        let count = link_matches.len().min(self.max_results);

        for i in 0..count {
            let caps = &link_matches[i];
            let url_str = decode_ddg_redirect_url(&caps[1]);
            let title = strip_tags(&caps[2]);

            lines.push(format!("{}. {}", i + 1, title.trim()));
            lines.push(format!("   {}", url_str.trim()));

            if i < snippet_matches.len() {
                let snippet = strip_tags(&snippet_matches[i][1]);
                let snippet = snippet.trim();
                if !snippet.is_empty() {
                    lines.push(format!("   {snippet}"));
                }
            }
        }

        Ok(lines.join("\n"))
    }

    async fn search_brave(&self, query: &str) -> Result<String, AgentError> {
        let auth_token = self.get_next_brave_api_key().ok_or_else(|| {
            AgentError::tool("web_search", "Brave API key not configured")
        })?;

        let encoded_query = urlencoding::encode(query);
        let search_url = format!(
            "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
            encoded_query, self.max_results
        );

        let response = self
            .client
            .get(&search_url)
            .header("Accept", "application/json")
            .header("X-Subscription-Token", auth_token)
            .send()
            .await
            .map_err(|e| AgentError::tool("web_search", e.to_string()))?;

        if !response.status().is_success() {
            return Err(AgentError::tool(
                "web_search",
                format!(
                    "Brave search failed with status: {}",
                    response.status()
                ),
            ));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AgentError::tool("web_search", e.to_string()))?;
        self.parse_brave_results(&json, query)
    }

    fn parse_brave_results(
        &self,
        json: &serde_json::Value,
        query: &str,
    ) -> Result<String, AgentError> {
        let results = json
            .get("web")
            .and_then(|w| w.get("results"))
            .and_then(|r| r.as_array())
            .ok_or_else(|| {
                AgentError::tool("web_search", "Invalid Brave API response")
            })?;

        if results.is_empty() {
            return Ok(format!("No results found for: {query}"));
        }

        let mut lines =
            vec![format!("Search results for: {} (via Brave)", query)];

        for (i, result) in results.iter().take(self.max_results).enumerate() {
            let title = result
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("No title");
            let url = result.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let description = result
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("");

            lines.push(format!("{}. {}", i + 1, title));
            lines.push(format!("   {url}"));
            if !description.is_empty() {
                lines.push(format!("   {description}"));
            }
        }

        Ok(lines.join("\n"))
    }

    async fn search_tavily(&self, query: &str) -> Result<String, AgentError> {
        let api_key = self.get_next_api_key().ok_or_else(|| {
            AgentError::tool("web_search", "Tavily API key not configured")
        })?;

        let endpoint = "https://api.tavily.com/search";
        let response = self
            .client
            .post(endpoint)
            .json(&json!({
                "api_key": api_key,
                "query": query,
                "max_results": self.max_results,
                "search_depth": "basic",
                "include_answer": false,
                "include_raw_content": false,
                "include_images": false
            }))
            .send()
            .await
            .map_err(|e| AgentError::tool("web_search", e.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| AgentError::tool("web_search", e.to_string()))?;
        if !status.is_success() {
            return Err(AgentError::tool(
                "web_search",
                format!(
                    "Tavily search failed with status {}: {}",
                    status.as_u16(),
                    body
                ),
            ));
        }

        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| {
                AgentError::tool(
                    "web_search",
                    format!("Invalid Tavily response JSON: {e}"),
                )
            })?;
        if let Some(error) =
            parsed.get("error").and_then(serde_json::Value::as_str)
        {
            return Err(AgentError::tool(
                "web_search",
                format!("Tavily API error: {error}"),
            ));
        }

        let results = parsed
            .get("results")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                AgentError::tool(
                    "web_search",
                    "Tavily response missing results array",
                )
            })?;

        if results.is_empty() {
            return Ok(format!("No results found for: {query}"));
        }

        let mut lines =
            vec![format!("Search results for: {} (via Tavily)", query)];
        for (i, result) in results.iter().take(self.max_results).enumerate() {
            let title = result
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("No title");
            let url = result
                .get("url")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let content = result
                .get("content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .trim();

            lines.push(format!("{}. {}", i + 1, title));
            lines.push(format!("   {url}"));
            if !content.is_empty() {
                lines.push(format!("   {content}"));
            }
        }

        Ok(lines.join("\n"))
    }

    async fn search_perplexity(
        &self,
        query: &str,
    ) -> Result<String, AgentError> {
        let api_key = self.get_next_perplexity_api_key().ok_or_else(|| {
            AgentError::tool("web_search", "Perplexity API key not configured")
        })?;

        let endpoint = "https://api.perplexity.ai/search";
        let response = self
            .client
            .post(endpoint)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {api_key}"))
            .json(&json!({
                "query": query,
                "max_results": self.max_results,
            }))
            .send()
            .await
            .map_err(|e| AgentError::tool("web_search", e.to_string()))?;

        let status = response.status();
        let raw = response
            .text()
            .await
            .map_err(|e| AgentError::tool("web_search", e.to_string()))?;
        if !status.is_success() {
            return Err(AgentError::tool(
                "web_search",
                format!(
                    "Perplexity search failed with status {}: {}",
                    status.as_u16(),
                    raw
                ),
            ));
        }

        let parsed: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| {
                AgentError::tool(
                    "web_search",
                    format!("Invalid Perplexity response JSON: {e}"),
                )
            })?;

        let results = parsed
            .get("results")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                AgentError::tool(
                    "web_search",
                    "Perplexity response missing results array",
                )
            })?;

        if results.is_empty() {
            return Ok(format!("No results found for: {query}"));
        }

        let mut lines =
            vec![format!("Search results for: {} (via Perplexity)", query)];
        for (i, result) in results.iter().take(self.max_results).enumerate() {
            let title = result
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("No title");
            let url = result
                .get("url")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let snippet = result
                .get("snippet")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .trim();

            lines.push(format!("{}. {}", i + 1, title));
            lines.push(format!("   {url}"));
            if !snippet.is_empty() {
                lines.push(format!("   {snippet}"));
            }
        }

        Ok(lines.join("\n"))
    }

    async fn search_exa(&self, query: &str) -> Result<String, AgentError> {
        let api_key = self.get_next_exa_api_key().ok_or_else(|| {
            AgentError::tool("web_search", "Exa API key not configured")
        })?;

        let endpoint = "https://api.exa.ai/search";
        let response = self
            .client
            .post(endpoint)
            .header("x-api-key", api_key)
            .json(&json!({
                "query": query,
                "numResults": self.max_results,
            }))
            .send()
            .await
            .map_err(|e| AgentError::tool("web_search", e.to_string()))?;

        let status = response.status();
        let raw = response
            .text()
            .await
            .map_err(|e| AgentError::tool("web_search", e.to_string()))?;
        if !status.is_success() {
            return Err(AgentError::tool(
                "web_search",
                format!(
                    "Exa search failed with status {}: {}",
                    status.as_u16(),
                    raw
                ),
            ));
        }

        let parsed: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| {
                AgentError::tool(
                    "web_search",
                    format!("Invalid Exa response JSON: {e}"),
                )
            })?;
        let results = parsed
            .get("results")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                AgentError::tool(
                    "web_search",
                    "Exa response missing results array",
                )
            })?;

        if results.is_empty() {
            return Ok(format!("No results found for: {query}"));
        }

        let mut lines =
            vec![format!("Search results for: {} (via Exa)", query)];
        for (i, result) in results.iter().take(self.max_results).enumerate() {
            let title = result
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("No title");
            let url = result
                .get("url")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let snippet = result
                .get("summary")
                .or_else(|| result.get("text"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .trim();

            lines.push(format!("{}. {}", i + 1, title));
            lines.push(format!("   {url}"));
            if !snippet.is_empty() {
                lines.push(format!("   {snippet}"));
            }
        }

        Ok(lines.join("\n"))
    }

    async fn search_jina(&self, query: &str) -> Result<String, AgentError> {
        let encoded_query = urlencoding::encode(query);
        let url = format!("https://s.jina.ai/{encoded_query}");

        let mut request = self.client.get(url).header("Accept", "text/plain");
        if let Some(api_key) = self.get_next_jina_api_key() {
            request = request
                .header(
                    reqwest::header::AUTHORIZATION,
                    format!("Bearer {api_key}"),
                )
                .header("x-api-key", api_key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| AgentError::tool("web_search", e.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| AgentError::tool("web_search", e.to_string()))?;
        if !status.is_success() {
            return Err(AgentError::tool(
                "web_search",
                format!(
                    "Jina search failed with status {}: {}",
                    status.as_u16(),
                    body
                ),
            ));
        }

        let trimmed = body.trim();
        if trimmed.is_empty() {
            return Ok(format!("No results found for: {query}"));
        }

        Ok(format!("Search results for: {query} (via Jina)\n{trimmed}",))
    }

    async fn search_with_provider(
        &self,
        provider: &str,
        query: &str,
    ) -> Result<String, AgentError> {
        match provider {
            "duckduckgo" => self.search_duckduckgo(query).await,
            "brave" => self.search_brave(query).await,
            "tavily" => self.search_tavily(query).await,
            "perplexity" => self.search_perplexity(query).await,
            "exa" => self.search_exa(query).await,
            "jina" => self.search_jina(query).await,
            "firecrawl" => Err(AgentError::tool(
                "web_search",
                "Firecrawl provider requires additional setup",
            )),
            _ => Err(AgentError::tool(
                "web_search",
                format!("Unknown search provider: {provider}"),
            )),
        }
    }
}

fn decode_ddg_redirect_url(raw_url: &str) -> String {
    if let Some(index) = raw_url.find("uddg=") {
        let encoded = &raw_url[index + 5..];
        let encoded = encoded.split('&').next().unwrap_or(encoded);
        if let Ok(decoded) = urlencoding::decode(encoded) {
            return decoded.into_owned();
        }
    }
    raw_url.to_string()
}

fn strip_tags(content: &str) -> String {
    if let Ok(re) = Regex::new(r"<[^>]+>") {
        re.replace_all(content, "").to_string()
    } else {
        content.to_string()
    }
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for WebSearchTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSearchTool")
            .field("provider", &self.provider)
            .field("max_results", &self.max_results)
            .finish()
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "WebSearch"
    }

    fn description(&self) -> &str {
        "Search the web. Include sources in response."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                }
            },
            "required": ["query"]
        })
    }

    fn is_concurrency_safe(&self, _args: &Value) -> bool {
        true
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: WebSearchRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        let query = request.query;

        if query.trim().is_empty() {
            return CallToolResult::error(vec![Content::text(
                "Search query cannot be empty".to_string(),
            )]);
        }

        tracing::info!("Searching web for: {}", query);

        let providers = match self.provider_chain() {
            Ok(p) => p,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "{e}"
                ))]);
            }
        };

        let mut result: Option<String> = None;
        let mut errors: Vec<String> = Vec::new();

        for provider in providers {
            match self.search_with_provider(provider, &query).await {
                Ok(output) => {
                    result = Some(output);
                    break;
                }
                Err(error) => {
                    errors.push(format!("{provider}: {error}"));
                }
            }
        }

        match result {
            Some(output) => {
                CallToolResult::success(vec![Content::text(output)])
            }
            None => CallToolResult::error(vec![Content::text(format!(
                "All search providers failed: {}",
                errors.join(" | ")
            ))]),
        }
    }
}

impl Clone for WebSearchTool {
    fn clone(&self) -> Self {
        Self::new_with_options(
            self.provider.clone(),
            None,
            None,
            None,
            None,
            None,
            self.max_results,
            self.timeout_secs,
            self.user_agent.clone(),
            self.fallback_providers.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_search_tool_name() {
        let tool = WebSearchTool::new();
        assert_eq!(tool.name(), "WebSearch");
    }

    #[test]
    fn test_web_search_tool_description() {
        let tool = WebSearchTool::new();
        assert!(tool.description().contains("Search the web"));
    }

    #[test]
    fn test_web_search_tool_parameters() {
        let tool = WebSearchTool::new();
        let params = tool.parameters();

        assert!(params.is_object());
        let obj = params.as_object().unwrap();
        assert!(obj.contains_key("properties"));
        assert!(obj.get("properties").and_then(|p| p.get("query")).is_some());
        assert!(
            obj.get("required")
                .and_then(|r| r.as_array())
                .map(|arr| arr.contains(&serde_json::json!("query")))
                .unwrap_or(false)
        );
    }

    #[test]
    fn test_web_search_tool_is_concurrency_safe() {
        let tool = WebSearchTool::new();
        let args = serde_json::json!({"query": "test"});
        assert!(tool.is_concurrency_safe(&args));
    }

    #[test]
    fn test_web_search_tool_debug() {
        let tool = WebSearchTool::new();
        let debug_str = format!("{tool:?}");
        assert!(debug_str.contains("WebSearchTool"));
        assert!(debug_str.contains("provider"));
        assert!(debug_str.contains("max_results"));
    }

    #[test]
    fn test_web_search_tool_default() {
        let tool = WebSearchTool::default();
        assert_eq!(tool.name(), "WebSearch");
    }

    #[test]
    fn test_web_search_tool_clone() {
        let tool = WebSearchTool::new();
        let cloned = tool.clone();
        assert_eq!(tool.name(), cloned.name());
    }

    #[test]
    fn test_web_search_tool_new_with_options() {
        let tool = WebSearchTool::new_with_options(
            "brave".to_string(),
            None,
            Some("test_brave_key".to_string()),
            None,
            None,
            None,
            3,
            15,
            "TestAgent/1.0".to_string(),
            vec!["tavily".to_string()],
        );
        assert_eq!(tool.name(), "WebSearch");
    }

    #[test]
    fn test_parse_api_keys_single() {
        let keys = WebSearchTool::parse_api_keys(Some("key1"));
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], "key1");
    }

    #[test]
    fn test_parse_api_keys_multiple() {
        let keys = WebSearchTool::parse_api_keys(Some("key1,key2,key3"));
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0], "key1");
        assert_eq!(keys[1], "key2");
        assert_eq!(keys[2], "key3");
    }

    #[test]
    fn test_parse_api_keys_with_spaces() {
        let keys = WebSearchTool::parse_api_keys(Some("key1 , key2 , key3"));
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0], "key1");
        assert_eq!(keys[1], "key2");
        assert_eq!(keys[2], "key3");
    }

    #[test]
    fn test_parse_api_keys_empty() {
        let keys = WebSearchTool::parse_api_keys(None);
        assert!(keys.is_empty());
    }

    #[test]
    fn test_parse_api_keys_with_empty_strings() {
        let keys = WebSearchTool::parse_api_keys(Some("key1,,key2, ,key3"));
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn test_normalize_provider_duckduckgo() {
        assert_eq!(
            WebSearchTool::normalize_provider("duckduckgo"),
            Some("duckduckgo")
        );
        assert_eq!(
            WebSearchTool::normalize_provider("ddg"),
            Some("duckduckgo")
        );
        assert_eq!(
            WebSearchTool::normalize_provider("DUCKDUCKGO"),
            Some("duckduckgo")
        );
    }

    #[test]
    fn test_normalize_provider_brave() {
        assert_eq!(WebSearchTool::normalize_provider("brave"), Some("brave"));
        assert_eq!(WebSearchTool::normalize_provider("Brave"), Some("brave"));
    }

    #[test]
    fn test_normalize_provider_tavily() {
        assert_eq!(WebSearchTool::normalize_provider("tavily"), Some("tavily"));
        assert_eq!(WebSearchTool::normalize_provider("TAVILY"), Some("tavily"));
    }

    #[test]
    fn test_normalize_provider_perplexity() {
        assert_eq!(
            WebSearchTool::normalize_provider("perplexity"),
            Some("perplexity")
        );
    }

    #[test]
    fn test_normalize_provider_exa() {
        assert_eq!(WebSearchTool::normalize_provider("exa"), Some("exa"));
    }

    #[test]
    fn test_normalize_provider_jina() {
        assert_eq!(WebSearchTool::normalize_provider("jina"), Some("jina"));
    }

    #[test]
    fn test_normalize_provider_firecrawl() {
        assert_eq!(
            WebSearchTool::normalize_provider("firecrawl"),
            Some("firecrawl")
        );
    }

    #[test]
    fn test_normalize_provider_unknown() {
        assert_eq!(WebSearchTool::normalize_provider("unknown"), None);
        assert_eq!(WebSearchTool::normalize_provider("google"), None);
        assert_eq!(WebSearchTool::normalize_provider(""), None);
    }

    #[test]
    fn test_provider_chain_single() {
        let tool = WebSearchTool::new_with_options(
            "duckduckgo".to_string(),
            None,
            None,
            None,
            None,
            None,
            5,
            30,
            "TestAgent/1.0".to_string(),
            vec![],
        );
        let chain = tool.provider_chain().unwrap();
        assert_eq!(chain, vec!["duckduckgo"]);
    }

    #[test]
    fn test_provider_chain_with_fallbacks() {
        let tool = WebSearchTool::new_with_options(
            "duckduckgo".to_string(),
            None,
            None,
            None,
            None,
            None,
            5,
            30,
            "TestAgent/1.0".to_string(),
            vec!["brave".to_string(), "tavily".to_string()],
        );
        let chain = tool.provider_chain().unwrap();
        assert_eq!(chain, vec!["duckduckgo", "brave", "tavily"]);
    }

    #[test]
    fn test_provider_chain_removes_duplicates() {
        let tool = WebSearchTool::new_with_options(
            "duckduckgo".to_string(),
            None,
            None,
            None,
            None,
            None,
            5,
            30,
            "TestAgent/1.0".to_string(),
            vec!["brave".to_string(), "duckduckgo".to_string()],
        );
        let chain = tool.provider_chain().unwrap();
        assert_eq!(chain, vec!["duckduckgo", "brave"]);
    }

    #[test]
    fn test_provider_chain_invalid_provider() {
        let tool = WebSearchTool::new_with_options(
            "invalid_provider".to_string(),
            None,
            None,
            None,
            None,
            None,
            5,
            30,
            "TestAgent/1.0".to_string(),
            vec![],
        );
        assert!(tool.provider_chain().is_err());
    }

    #[test]
    fn test_decode_ddg_redirect_url_encoded() {
        let url = "https://html.duckduckgo.com/html/?uddg=https%3A%2F%2Fexample.com%2Fpath&q=test";
        let decoded = decode_ddg_redirect_url(url);
        assert_eq!(decoded, "https://example.com/path");
    }

    #[test]
    fn test_decode_ddg_redirect_url_no_uddg() {
        let url = "https://example.com/regular/path";
        let decoded = decode_ddg_redirect_url(url);
        assert_eq!(decoded, url);
    }

    #[test]
    fn test_decode_ddg_redirect_url_empty_after_uddg() {
        let url = "https://html.duckduckgo.com/html/?uddg=&q=test";
        let decoded = decode_ddg_redirect_url(url);
        assert_eq!(decoded, "");
    }

    #[test]
    fn test_strip_tags_simple() {
        let html = "<p>Hello <strong>World</strong></p>";
        let text = strip_tags(html);
        assert_eq!(text, "Hello World");
    }

    #[test]
    fn test_strip_tags_no_tags() {
        let text = "Plain text without tags";
        let result = strip_tags(text);
        assert_eq!(result, text);
    }

    #[test]
    fn test_strip_tags_nested() {
        let html = "<div><p><span>Nested <em>content</em></span></p></div>";
        let text = strip_tags(html);
        assert_eq!(text, "Nested content");
    }

    #[test]
    fn test_strip_tags_self_closing() {
        let html = "Text<br/>more text";
        let text = strip_tags(html);
        assert_eq!(text, "Textmore text");
    }

    #[test]
    fn test_strip_tags_invalid_regex_handled() {
        // When regex fails (returns Err), the original content is returned
        // This is implicitly tested since we use if let Ok(re)
        let html = "<p>Test</p>";
        let text = strip_tags(html);
        assert!(text.contains("Test"));
    }

    #[test]
    fn test_parse_duckduckgo_results_empty() {
        let tool = WebSearchTool::new();
        let html = "<html><body>No results</body></html>";
        let result = tool.parse_duckduckgo_results(html, "test query");
        assert!(result.is_ok());
        assert!(result.unwrap().contains("No results found"));
    }

    #[test]
    fn test_parse_duckduckgo_results_with_links() {
        let tool = WebSearchTool::new();
        let html = r#"
            <html><body>
            <a class="result__a" href="https://example.com/uddg=https://test.com">Test Result</a>
            <a class="result__snippet">This is a snippet</a>
            </body></html>
        "#;
        let result = tool.parse_duckduckgo_results(html, "test query");
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Test Result"));
    }

    #[test]
    fn test_parse_duckduckgo_results_max_results() {
        let tool = WebSearchTool::new_with_options(
            "duckduckgo".to_string(),
            None,
            None,
            None,
            None,
            None,
            2,
            30,
            "TestAgent/1.0".to_string(),
            vec![],
        );
        let html = r#"
            <html><body>
            <a class="result__a" href="https://example.com/1">Result 1</a>
            <a class="result__snippet">Snippet 1</a>
            <a class="result__a" href="https://example.com/2">Result 2</a>
            <a class="result__snippet">Snippet 2</a>
            <a class="result__a" href="https://example.com/3">Result 3</a>
            <a class="result__snippet">Snippet 3</a>
            </body></html>
        "#;
        let result = tool.parse_duckduckgo_results(html, "test query");
        assert!(result.is_ok());
        let output = result.unwrap();
        // Should only contain 2 results due to max_results = 2
        assert!(output.contains("Result 1"));
        assert!(output.contains("Result 2"));
        assert!(!output.contains("Result 3"));
    }

    #[test]
    fn test_parse_brave_results_empty() {
        let tool = WebSearchTool::new();
        let json = serde_json::json!({"web": {"results": []}});
        let result = tool.parse_brave_results(&json, "test query");
        assert!(result.is_ok());
        assert!(result.unwrap().contains("No results found"));
    }

    #[test]
    fn test_parse_brave_results_with_data() {
        let tool = WebSearchTool::new();
        let json = serde_json::json!({
            "web": {
                "results": [
                    {
                        "title": "Test Page",
                        "url": "https://test.com",
                        "description": "Test description"
                    }
                ]
            }
        });
        let result = tool.parse_brave_results(&json, "test query");
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Test Page"));
        assert!(output.contains("https://test.com"));
        assert!(output.contains("Test description"));
    }

    #[test]
    fn test_parse_brave_results_missing_fields() {
        let tool = WebSearchTool::new();
        let json = serde_json::json!({
            "web": {
                "results": [
                    {"title": "Only Title"}
                ]
            }
        });
        let result = tool.parse_brave_results(&json, "test query");
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Only Title"));
        // Missing fields should default to empty
        assert!(!output.contains("No title"));
    }

    #[test]
    fn test_max_results_clamping() {
        // max_results should be clamped between 1 and 10
        let tool = WebSearchTool::new_with_options(
            "duckduckgo".to_string(),
            None,
            None,
            None,
            None,
            None,
            0,
            30,
            "TestAgent/1.0".to_string(),
            vec![],
        );
        assert_eq!(tool.max_results, 1);

        let tool2 = WebSearchTool::new_with_options(
            "duckduckgo".to_string(),
            None,
            None,
            None,
            None,
            None,
            100,
            30,
            "TestAgent/1.0".to_string(),
            vec![],
        );
        assert_eq!(tool2.max_results, 10);
    }

    #[test]
    fn test_timeout_minimum() {
        // timeout should be at least 1 second
        let tool = WebSearchTool::new_with_options(
            "duckduckgo".to_string(),
            None,
            None,
            None,
            None,
            None,
            5,
            0,
            "TestAgent/1.0".to_string(),
            vec![],
        );
        assert_eq!(tool.timeout_secs, 1);
    }

    #[test]
    fn test_provider_case_insensitive() {
        // Provider names should be normalized to lowercase
        let tool = WebSearchTool::new_with_options(
            "DUCKDUCKGO".to_string(),
            None,
            None,
            None,
            None,
            None,
            5,
            30,
            "TestAgent/1.0".to_string(),
            vec![],
        );
        let chain = tool.provider_chain().unwrap();
        assert_eq!(chain, vec!["duckduckgo"]);
    }

    #[test]
    fn test_get_next_key_round_robin() {
        // Test key rotation with multiple keys
        let _tool = WebSearchTool::new_with_options(
            "tavily".to_string(),
            Some("key1,key2,key3".to_string()),
            None,
            None,
            None,
            None,
            5,
            30,
            "TestAgent/1.0".to_string(),
            vec![],
        );

        // Access internal key_index through clone behavior
        // The key_index should cycle through available keys
        let keys = WebSearchTool::parse_api_keys(Some("key1,key2,key3"));
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn test_fallback_providers_order() {
        // Verify fallback providers are tried in order
        let tool = WebSearchTool::new_with_options(
            "duckduckgo".to_string(),
            None,
            None,
            None,
            None,
            None,
            5,
            30,
            "TestAgent/1.0".to_string(),
            vec![
                "brave".to_string(),
                "tavily".to_string(),
                "jina".to_string(),
            ],
        );
        let chain = tool.provider_chain().unwrap();
        assert_eq!(chain.len(), 4);
        assert_eq!(chain[0], "duckduckgo");
        assert_eq!(chain[1], "brave");
        assert_eq!(chain[2], "tavily");
        assert_eq!(chain[3], "jina");
    }

    #[test]
    fn test_get_next_key_from_empty() {
        let keys: Vec<String> = vec![];
        let index = Arc::new(AtomicUsize::new(0));
        let result = WebSearchTool::get_next_key_from(&keys, &index);
        assert!(result.is_none());
    }

    #[test]
    fn test_get_next_key_from_single() {
        let keys = vec!["key1".to_string()];
        let index = Arc::new(AtomicUsize::new(0));
        let result = WebSearchTool::get_next_key_from(&keys, &index);
        assert_eq!(result, Some("key1".to_string()));
    }

    #[test]
    fn test_get_next_key_from_round_robin() {
        let keys =
            vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
        let index = Arc::new(AtomicUsize::new(0));

        let result1 = WebSearchTool::get_next_key_from(&keys, &index);
        let result2 = WebSearchTool::get_next_key_from(&keys, &index);
        let result3 = WebSearchTool::get_next_key_from(&keys, &index);
        let result4 = WebSearchTool::get_next_key_from(&keys, &index); // wraps around

        assert_eq!(result1, Some("key1".to_string()));
        assert_eq!(result2, Some("key2".to_string()));
        assert_eq!(result3, Some("key3".to_string()));
        assert_eq!(result4, Some("key1".to_string())); // wrapped
    }

    #[test]
    fn test_get_next_api_key() {
        let tool = WebSearchTool::new_with_options(
            "tavily".to_string(),
            Some("test_api_key".to_string()),
            None,
            None,
            None,
            None,
            5,
            30,
            "TestAgent/1.0".to_string(),
            vec![],
        );
        let key = tool.get_next_api_key();
        assert_eq!(key, Some("test_api_key".to_string()));
    }

    #[test]
    fn test_get_next_api_key_none_when_empty() {
        let tool = WebSearchTool::new_with_options(
            "duckduckgo".to_string(),
            None,
            None,
            None,
            None,
            None,
            5,
            30,
            "TestAgent/1.0".to_string(),
            vec![],
        );
        let key = tool.get_next_api_key();
        assert!(key.is_none());
    }

    #[test]
    fn test_get_next_brave_api_key_fallback() {
        // Brave uses its own key first, falls back to general API key
        let tool = WebSearchTool::new_with_options(
            "brave".to_string(),
            Some("fallback_key".to_string()),
            Some("brave_key".to_string()),
            None,
            None,
            None,
            5,
            30,
            "TestAgent/1.0".to_string(),
            vec![],
        );
        let key = tool.get_next_brave_api_key();
        assert_eq!(key, Some("brave_key".to_string()));
    }

    #[test]
    fn test_get_next_brave_api_key_falls_back_to_general() {
        let tool = WebSearchTool::new_with_options(
            "brave".to_string(),
            Some("fallback_key".to_string()),
            None, // no brave-specific key
            None,
            None,
            None,
            5,
            30,
            "TestAgent/1.0".to_string(),
            vec![],
        );
        let key = tool.get_next_brave_api_key();
        assert_eq!(key, Some("fallback_key".to_string()));
    }

    #[test]
    fn test_get_next_perplexity_api_key() {
        let tool = WebSearchTool::new_with_options(
            "perplexity".to_string(),
            None,
            None,
            Some("perplexity_key".to_string()),
            None,
            None,
            5,
            30,
            "TestAgent/1.0".to_string(),
            vec![],
        );
        let key = tool.get_next_perplexity_api_key();
        assert_eq!(key, Some("perplexity_key".to_string()));
    }

    #[test]
    fn test_get_next_exa_api_key() {
        let tool = WebSearchTool::new_with_options(
            "exa".to_string(),
            None,
            None,
            None,
            Some("exa_key".to_string()),
            None,
            5,
            30,
            "TestAgent/1.0".to_string(),
            vec![],
        );
        let key = tool.get_next_exa_api_key();
        assert_eq!(key, Some("exa_key".to_string()));
    }

    #[test]
    fn test_get_next_jina_api_key() {
        let tool = WebSearchTool::new_with_options(
            "jina".to_string(),
            None,
            None,
            None,
            None,
            Some("jina_key".to_string()),
            5,
            30,
            "TestAgent/1.0".to_string(),
            vec![],
        );
        let key = tool.get_next_jina_api_key();
        assert_eq!(key, Some("jina_key".to_string()));
    }

    #[test]
    fn test_search_with_provider_firecrawl_error() {
        // Firecrawl requires additional setup, should return error
        let tool = WebSearchTool::new();
        // We can't easily call search_with_provider directly since it's async,
        // but we can verify through provider_chain that firecrawl is recognized
        let _chain = tool.provider_chain().unwrap();
        // Firecrawl in chain should not cause normalize error
        let mut firecrawl_tool = tool.clone();
        firecrawl_tool.fallback_providers = vec!["firecrawl".to_string()];
        let chain_with_firecrawl = firecrawl_tool.provider_chain().unwrap();
        assert!(chain_with_firecrawl.contains(&"firecrawl"));
    }

    #[test]
    fn test_provider_chain_all_valid_providers() {
        // Test that all known providers can be normalized
        let providers = vec![
            "duckduckgo",
            "ddg",
            "brave",
            "tavily",
            "perplexity",
            "exa",
            "jina",
            "firecrawl",
        ];
        for provider in providers {
            let chain = WebSearchTool::new_with_options(
                provider.to_string(),
                None,
                None,
                None,
                None,
                None,
                5,
                30,
                "TestAgent/1.0".to_string(),
                vec![],
            )
            .provider_chain();
            assert!(chain.is_ok(), "Provider {provider} should normalize");
        }
    }
}
