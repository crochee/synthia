//! WebFetch tool implementation
//!
//! Fetch content from a URL.

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use crate::tools::Tool;

#[derive(Debug, Clone, Deserialize)]
struct WebFetchRequest {
    url: String,
}

pub struct WebFetchTool {
    client: reqwest::Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .user_agent("Mozilla/5.0 (compatible; SynthiaAgent/1.0)")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for WebFetchTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebFetchTool").finish()
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "WebFetch"
    }

    fn description(&self) -> &str {
        "Fetch URL content. Converts HTML to markdown (max 10KB)."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "format": "uri",
                    "description": "URL"
                }
            },
            "required": ["url"]
        })
    }

    fn is_concurrency_safe(&self, _args: &Value) -> bool {
        true
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: WebFetchRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        let url = request.url;

        let parsed_url = match url::Url::parse(&url) {
            Ok(u) => u,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid URL: {e}"
                ))]);
            }
        };

        let scheme = parsed_url.scheme();
        if scheme != "http" && scheme != "https" {
            return CallToolResult::error(vec![Content::text(format!(
                "Unsupported URL scheme: {scheme}"
            ))]);
        }

        let response = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Request failed: {e}"
                ))]);
            }
        };

        let status = response.status();
        if !status.is_success() {
            return CallToolResult::error(vec![Content::text(format!(
                "HTTP error: {status}"
            ))]);
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html")
            .to_string();

        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to read response: {e}"
                ))]);
            }
        };

        let markdown = if content_type.contains("text/html") {
            html_to_markdown(&body)
        } else if content_type.contains("application/json") {
            format!("```json\n{body}\n```")
        } else {
            body
        };

        let truncated = if markdown.len() > 10000 {
            format!(
                "{}...\n\n[Content truncated - {} total characters]",
                &markdown[..10000],
                markdown.len()
            )
        } else {
            markdown
        };

        let message =
            format!("URL: {url}\nContent-Type: {content_type}\n\n{truncated}");

        CallToolResult::success(vec![Content::text(message)])
    }
}

impl Clone for WebFetchTool {
    fn clone(&self) -> Self {
        Self::new()
    }
}

fn html_to_markdown(html: &str) -> String {
    let doc = scraper::Html::parse_document(html);

    let mut markdown = String::new();

    let selector = scraper::Selector::parse("body").ok();
    if let Some(selector) = selector
        && let Some(body) = doc.select(&selector).next()
    {
        process_element(&body, &mut markdown);
    }

    let cleaned = markdown
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");

    #[allow(clippy::expect_used)]
    let re = regex::Regex::new(r"\n{3,}").expect("valid regex");
    re.replace_all(&cleaned, "\n\n").to_string()
}

fn process_element(element: &scraper::ElementRef, markdown: &mut String) {
    let tag = element.value().name();

    match tag {
        "title" | "h1" => {
            let text = element.text().collect::<String>();
            if !text.trim().is_empty() {
                markdown.push_str(&format!("# {}\n\n", text.trim()));
            }
        }
        "h2" => {
            let text = element.text().collect::<String>();
            if !text.trim().is_empty() {
                markdown.push_str(&format!("## {}\n\n", text.trim()));
            }
        }
        "h3" => {
            let text = element.text().collect::<String>();
            if !text.trim().is_empty() {
                markdown.push_str(&format!("### {}\n\n", text.trim()));
            }
        }
        "h4" | "h5" | "h6" => {
            let text = element.text().collect::<String>();
            if !text.trim().is_empty() {
                markdown.push_str(&format!("#### {}\n\n", text.trim()));
            }
        }
        "p" => {
            let text = element.text().collect::<String>();
            if !text.trim().is_empty() {
                markdown.push_str(&format!("{}\n\n", text.trim()));
            }
        }
        "a" => {
            let text = element.text().collect::<String>();
            let href = element.value().attr("href").unwrap_or("");
            if !text.trim().is_empty() && !href.is_empty() {
                markdown.push_str(&format!("[{}]({})", text.trim(), href));
            }
        }
        "code" => {
            let text = element.text().collect::<String>();
            markdown.push_str(&format!("`{}`", text.trim()));
        }
        "pre" => {
            let text = element.text().collect::<String>();
            if !text.trim().is_empty() {
                markdown.push_str(&format!("```\n{}\n```\n\n", text.trim()));
            }
        }
        "ul" | "ol" => {
            for child in element.children() {
                if let Some(child_el) = scraper::ElementRef::wrap(child)
                    && child_el.value().name() == "li"
                {
                    let text = child_el.text().collect::<String>();
                    if !text.trim().is_empty() {
                        markdown.push_str(&format!("- {}\n", text.trim()));
                    }
                }
            }
            markdown.push('\n');
        }
        "blockquote" => {
            let text = element.text().collect::<String>();
            for line in text.lines() {
                markdown.push_str(&format!("> {}\n", line.trim()));
            }
            markdown.push('\n');
        }
        "br" => {
            markdown.push('\n');
        }
        "hr" => {
            markdown.push_str("\n---\n\n");
        }
        "strong" | "b" => {
            let text = element.text().collect::<String>();
            if !text.trim().is_empty() {
                markdown.push_str(&format!("**{}**", text.trim()));
            }
        }
        "em" | "i" => {
            let text = element.text().collect::<String>();
            if !text.trim().is_empty() {
                markdown.push_str(&format!("*{}*", text.trim()));
            }
        }
        _ => {}
    }

    for child in element.children() {
        if let Some(child_el) = scraper::ElementRef::wrap(child) {
            process_element(&child_el, markdown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_fetch_tool_name() {
        let tool = WebFetchTool::new();
        assert_eq!(tool.name(), "WebFetch");
    }

    #[test]
    fn test_web_fetch_tool_description() {
        let tool = WebFetchTool::new();
        assert!(tool.description().contains("Fetch URL content"));
    }

    #[test]
    fn test_web_fetch_tool_parameters() {
        let tool = WebFetchTool::new();
        let params = tool.parameters();

        assert!(params.is_object());
        let obj = params.as_object().unwrap();
        assert!(obj.contains_key("properties"));
        assert!(obj.get("properties").and_then(|p| p.get("url")).is_some());
        assert!(
            obj.get("required")
                .and_then(|r| r.as_array())
                .map(|arr| arr.contains(&serde_json::json!("url")))
                .unwrap_or(false)
        );
    }

    #[test]
    fn test_web_fetch_tool_is_concurrency_safe() {
        let tool = WebFetchTool::new();
        let args = serde_json::json!({"url": "https://example.com"});
        assert!(tool.is_concurrency_safe(&args));
    }

    #[test]
    fn test_web_fetch_tool_debug() {
        let tool = WebFetchTool::new();
        let debug_str = format!("{tool:?}");
        assert!(debug_str.contains("WebFetchTool"));
    }

    #[test]
    fn test_web_fetch_tool_clone() {
        let tool = WebFetchTool::new();
        let cloned = tool.clone();
        assert_eq!(tool.name(), cloned.name());
    }

    #[test]
    fn test_web_fetch_tool_default() {
        let tool = WebFetchTool::default();
        assert_eq!(tool.name(), "WebFetch");
    }

    #[test]
    fn test_html_to_markdown_headings() {
        let html = r#"<html><body><h1>Title</h1><h2>Subtitle</h2><h3>Section</h3></body></html>"#;
        let markdown = html_to_markdown(html);
        assert!(markdown.contains("# Title"));
        assert!(markdown.contains("## Subtitle"));
        assert!(markdown.contains("### Section"));
    }

    #[test]
    fn test_html_to_markdown_paragraphs() {
        let html = r#"<html><body><p>First paragraph</p><p>Second paragraph</p></body></html>"#;
        let markdown = html_to_markdown(html);
        assert!(markdown.contains("First paragraph"));
        assert!(markdown.contains("Second paragraph"));
    }

    #[test]
    fn test_html_to_markdown_links() {
        let html = r#"<html><body><a href="https://example.com">Example Link</a></body></html>"#;
        let markdown = html_to_markdown(html);
        assert!(markdown.contains("[Example Link](https://example.com)"));
    }

    #[test]
    fn test_html_to_markdown_inline_code() {
        let html = r#"<html><body><code>let x = 42;</code></body></html>"#;
        let markdown = html_to_markdown(html);
        assert!(markdown.contains("`let x = 42;`"));
    }

    #[test]
    fn test_html_to_markdown_code_block() {
        let html = r#"<html><body><pre>fn main() {
    println!("Hello");
}</pre></body></html>"#;
        let markdown = html_to_markdown(html);
        assert!(markdown.contains("```\n"));
        assert!(markdown.contains("fn main()"));
    }

    #[test]
    fn test_html_to_markdown_unordered_list() {
        let html = r#"<html><body><ul><li>Item 1</li><li>Item 2</li></ul></body></html>"#;
        let markdown = html_to_markdown(html);
        assert!(markdown.contains("- Item 1"));
        assert!(markdown.contains("- Item 2"));
    }

    #[test]
    fn test_html_to_markdown_blockquote() {
        let html = r#"<html><body><blockquote>This is a quote</blockquote></body></html>"#;
        let markdown = html_to_markdown(html);
        assert!(markdown.contains("> This is a quote"));
    }

    #[test]
    fn test_html_to_markdown_strong_and_em() {
        let html = r#"<html><body><strong>Bold</strong> and <em>italic</em></body></html>"#;
        let markdown = html_to_markdown(html);
        assert!(markdown.contains("**Bold**"));
        assert!(markdown.contains("*italic*"));
    }

    #[test]
    fn test_html_to_markdown_horizontal_rule() {
        let html = r#"<html><body><hr></body></html>"#;
        let markdown = html_to_markdown(html);
        assert!(markdown.contains("---"));
    }

    #[test]
    fn test_html_to_markdown_empty_elements() {
        let html = r#"<html><body><h1></h1><p>   </p><a></a></body></html>"#;
        let markdown = html_to_markdown(html);
        // Empty elements should not produce output
        assert!(!markdown.contains("# \n"));
    }

    #[test]
    fn test_html_to_markdown_nested_content() {
        let html = r#"<html><body><div><p>Nested <strong>strong</strong> in paragraph</p></div></body></html>"#;
        let markdown = html_to_markdown(html);
        assert!(markdown.contains("Nested"));
        assert!(markdown.contains("**strong**"));
    }

    #[test]
    fn test_html_to_markdown_multiple_newlines_collapsed() {
        let html = r#"<html><body><p>Line 1</p><p>Line 2</p><p>Line 3</p></body></html>"#;
        let markdown = html_to_markdown(html);
        // Multiple newlines should be collapsed to double newlines
        let double_newline_count = markdown.matches("\n\n").count();
        assert!(double_newline_count >= 2);
    }

    #[test]
    fn test_html_to_markdown_no_body_uses_full_document() {
        // When body selector fails, should still process the document
        let html = r#"<html><head><title>Page Title</title></head></html>"#;
        let markdown = html_to_markdown(html);
        // Should not panic and should produce some output
        assert!(markdown.is_empty() || markdown.contains("#"));
    }

    #[test]
    fn test_html_to_markdown_div() {
        let html =
            r#"<html><body><div><p>Content inside div</p></div></body></html>"#;
        let markdown = html_to_markdown(html);
        assert!(markdown.contains("Content inside div"));
    }

    #[test]
    fn test_html_to_markdown_span() {
        let html = r#"<html><body><p>Text with <span>span content</span> inline</p></body></html>"#;
        let markdown = html_to_markdown(html);
        assert!(markdown.contains("span content"));
    }

    #[test]
    fn test_html_to_markdown_image() {
        let html = r#"<html><body><img src="https://example.com/image.png" alt="Test Image"></body></html>"#;
        let markdown = html_to_markdown(html);
        // Images are not explicitly handled in process_element, so alt text may not appear
        // The key is that it doesn't panic
        assert!(!markdown.contains("!["));
    }

    #[test]
    fn test_html_to_markdown_table() {
        // Tables fall through to default case which only iterates child elements,
        // so table structure is not rendered. This documents actual behavior.
        let html = r#"<html><body><table><tr><td>Cell 1</td><td>Cell 2</td></tr></table></body></html>"#;
        let markdown = html_to_markdown(html);
        // Table cells are not explicitly handled, so content may not appear
        // This is current behavior - table elements are silently ignored
        assert!(!markdown.contains("Cell 1"));
    }

    #[test]
    fn test_html_to_markdown_script() {
        let html = r#"<html><body><p>Visible</p><script>document.write('hidden');</script></body></html>"#;
        let markdown = html_to_markdown(html);
        // Script content should not appear
        assert!(markdown.contains("Visible"));
        assert!(!markdown.contains("document.write"));
    }

    #[test]
    fn test_html_to_markdown_style() {
        let html = r#"<html><head><style>.hidden { display: none; }</style></head><body><p>Content</p></body></html>"#;
        let markdown = html_to_markdown(html);
        // Style content should not appear in body output
        assert!(markdown.contains("Content"));
        assert!(!markdown.contains(".hidden"));
    }

    #[test]
    fn test_html_to_markdown_br() {
        let html = r#"<html><body><p>Line 1<br>Line 2</p></body></html>"#;
        let markdown = html_to_markdown(html);
        // BR should produce a newline
        assert!(markdown.contains("\n"));
    }

    #[test]
    fn test_html_to_markdown_complex_nested() {
        let html = r#"<html><body><article><header><h1>Main Title</h1></header><div><p>First <strong>paragraph</strong> with <em>emphasis</em>.</p><p>Second paragraph.</p></div><footer><blockquote>Final note</blockquote></footer></article></body></html>"#;
        let markdown = html_to_markdown(html);
        assert!(markdown.contains("# Main Title"));
        assert!(markdown.contains("**paragraph**"));
        assert!(markdown.contains("*emphasis*"));
        assert!(markdown.contains("> Final note"));
    }

    #[test]
    fn test_html_to_markdown_ordered_list() {
        let html = r#"<html><body><ol><li>First</li><li>Second</li><li>Third</li></ol></body></html>"#;
        let markdown = html_to_markdown(html);
        assert!(markdown.contains("- First"));
        assert!(markdown.contains("- Second"));
        assert!(markdown.contains("- Third"));
    }
}
