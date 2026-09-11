use super::webfetch::{html_to_markdown, html_to_text, truncate_output};
use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use futures::StreamExt;
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;

const MAX_SIZE: usize = 5 * 1024 * 1024; // 5MB
const DEFAULT_TIMEOUT: u64 = 30;
const MAX_TIMEOUT: u64 = 120;

pub struct ScraplingTool {
    client: reqwest::Client,
}

impl ScraplingTool {
    pub fn new() -> Self {
        Self {
            client: crate::provider::shared_http_client(),
        }
    }
}

#[derive(Deserialize)]
struct ScraplingInput {
    url: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    extract: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PythonScraplingResult {
    #[serde(default)]
    status: Option<u16>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    html: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[async_trait]
impl Tool for ScraplingTool {
    fn name(&self) -> &str {
        "scrapling"
    }

    fn description(&self) -> &str {
        "Smart web scraping with adaptive extraction. Fetches URLs using multiple strategies: \
         fast HTTP, intelligent DOM extraction with CSS selectors, or browser-based rendering \
         for JavaScript-heavy pages. Supports anti-detection via Scrapling Python library. \
         Use for security research, content extraction, and scraping sites that block simple HTTP."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "intent": super::intent_schema_property(),
                "url": {
                    "type": "string",
                    "description": "URL to fetch and extract content from."
                },
                "mode": {
                    "type": "string",
                    "enum": ["http", "adaptive", "browser", "auto"],
                    "description": "Fetch strategy. http=fast HTTP only. adaptive=DOM parsing with CSS selectors. browser=JS rendering via Scrapling Python. auto=try HTTP, escalate if blocked (default).",
                    "default": "auto"
                },
                "extract": {
                    "type": "string",
                    "description": "CSS selector for targeted extraction (adaptive/browser modes). Examples: 'article', '.content', '#main', 'table.data'"
                },
                "format": {
                    "type": "string",
                    "enum": ["text", "markdown", "html", "json"],
                    "description": "Output format (default: markdown)."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30, max: 120)."
                },
                "headers": {
                    "type": "object",
                    "description": "Custom HTTP headers to send with the request.",
                    "additionalProperties": { "type": "string" }
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params: ScraplingInput = serde_json::from_value(input)?;

        if !params.url.starts_with("http://") && !params.url.starts_with("https://") {
            return Err(anyhow::anyhow!("URL must start with http:// or https://"));
        }

        let timeout = params.timeout.unwrap_or(DEFAULT_TIMEOUT).min(MAX_TIMEOUT);
        let format = params.format.as_deref().unwrap_or("markdown");
        let mode = params.mode.as_deref().unwrap_or("auto");

        match mode {
            "http" => self.fetch_http(&params, timeout, format).await,
            "adaptive" => self.fetch_adaptive(&params, timeout, format).await,
            "browser" => self.fetch_browser(&params, timeout, format, &ctx).await,
            "auto" => self.fetch_auto(&params, timeout, format, &ctx).await,
            _ => Err(anyhow::anyhow!(
                "Unknown mode: {}. Use 'http', 'adaptive', 'browser', or 'auto'.",
                mode
            )),
        }
    }
}

impl ScraplingTool {
    /// Strategy 1: Fast HTTP fetch (same as webfetch but with custom headers)
    async fn fetch_http(
        &self,
        params: &ScraplingInput,
        timeout: u64,
        format: &str,
    ) -> Result<ToolOutput> {
        let response = self.build_and_send_request(params, timeout).await?;
        let (status, content_type, body) = self.read_response(response).await?;

        self.gate_validate(status, &content_type, &body)?;

        let output = self.format_output(&body, &content_type, format);
        let (output, truncated) = truncate_output(output);
        let note = if truncated {
            "\n\n[Truncated — use 'extract' selector for targeted content]"
        } else {
            ""
        };

        Ok(ToolOutput::new(format!(
            "Fetched {} (HTTP mode)\n\n{}{}",
            params.url, output, note
        )))
    }

    /// Strategy 2: Adaptive extraction with DOM parsing and CSS selectors
    async fn fetch_adaptive(
        &self,
        params: &ScraplingInput,
        timeout: u64,
        format: &str,
    ) -> Result<ToolOutput> {
        let response = self.build_and_send_request(params, timeout).await?;
        let (status, content_type, body) = self.read_response(response).await?;

        self.gate_validate(status, &content_type, &body)?;

        let document = Html::parse_document(&body);

        let output = if let Some(selector_str) = &params.extract {
            // CSS selector extraction
            self.extract_by_selector(&document, selector_str, format)?
        } else {
            // Intelligent main content extraction
            self.extract_main_content(&document, &content_type, format)?
        };

        let (output, truncated) = truncate_output(output);
        let note = if truncated {
            "\n\n[Truncated — use 'extract' selector for targeted content]"
        } else {
            ""
        };

        Ok(ToolOutput::new(format!(
            "Fetched {} (adaptive mode)\n\n{}{}",
            params.url, output, note
        )))
    }

    /// Strategy 3: Browser-based fetch via Scrapling Python or fallback to browser tool
    async fn fetch_browser(
        &self,
        params: &ScraplingInput,
        timeout: u64,
        format: &str,
        ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        // Try Scrapling Python first
        if let Ok(result) = self.try_scrapling_python(params, timeout).await {
            return self.format_python_result(&result, &params.url, format);
        }

        // Fallback: use the existing browser tool's get_content
        self.fallback_to_browser_tool(params, format, ctx).await
    }

    /// Auto mode: try HTTP, escalate if blocked
    async fn fetch_auto(
        &self,
        params: &ScraplingInput,
        timeout: u64,
        format: &str,
        ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        // Level 1: Try HTTP
        if let Ok(response) = self.build_and_send_request(params, timeout).await {
            let (status, content_type, body) = self.read_response(response).await?;

            // Check if we got a valid response
            if self.gate_validate(status, &content_type, &body).is_ok() {
                // Check for anti-bot signals
                if self.detect_anti_bot(&body) {
                    // Escalate to adaptive
                    let document = Html::parse_document(&body);
                    let output = self.extract_main_content(&document, &content_type, format)?;
                    let (output, truncated) = truncate_output(output);
                    let note = if truncated {
                        "\n\n[Truncated — anti-bot detected, content may be partial]"
                    } else {
                        "\n\n[Anti-bot detected — use mode='browser' for full rendering]"
                    };
                    return Ok(ToolOutput::new(format!(
                        "Fetched {} (auto: adaptive fallback)\n\n{}{}",
                        params.url, output, note
                    )));
                }

                let output = self.format_output(&body, &content_type, format);
                let (output, truncated) = truncate_output(output);
                let note = if truncated { "\n\n[Truncated]" } else { "" };
                return Ok(ToolOutput::new(format!(
                    "Fetched {} (auto: HTTP)\n\n{}{}",
                    params.url, output, note
                )));
            }
        }
        // Fall through to adaptive/browser

        // Level 2: Try adaptive extraction (needs a new HTTP fetch)
        if let Ok(response) = self.build_and_send_request(params, timeout).await
            && let Ok((status, content_type, body)) = self.read_response(response).await
            && self.gate_validate(status, &content_type, &body).is_ok()
        {
            let document = Html::parse_document(&body);
            if let Ok(output) = self.extract_main_content(&document, &content_type, format) {
                let (output, truncated) = truncate_output(output);
                let note = if truncated { "\n\n[Truncated]" } else { "" };
                return Ok(ToolOutput::new(format!(
                    "Fetched {} (auto: adaptive)\n\n{}{}",
                    params.url, output, note
                )));
            }
        }

        // Level 3: Browser fallback
        self.fetch_browser(params, timeout, format, ctx).await
    }

    // ── HTTP helpers ────────────────────────────────────────────────────

    async fn build_and_send_request(
        &self,
        params: &ScraplingInput,
        timeout: u64,
    ) -> Result<reqwest::Response> {
        let mut request_builder = self
            .client
            .get(&params.url)
            .header(
                reqwest::header::USER_AGENT,
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
            )
            .header(
                reqwest::header::ACCEPT,
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header(reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.9")
            .timeout(Duration::from_secs(timeout));

        if let Some(headers) = &params.headers {
            for (key, value) in headers {
                request_builder = request_builder.header(key.as_str(), value.as_str());
            }
        }

        let request = request_builder
            .build()
            .context("failed to build scrapling request")?;

        let response = crate::alphacode_provider_core::retry::send_with_retry(
            &self.client,
            request,
            &crate::alphacode_provider_core::retry::RetryPolicy::for_http_tools(),
            "scrapling",
        )
        .await?;

        Ok(response)
    }

    async fn read_response(&self, response: reqwest::Response) -> Result<(u16, String, String)> {
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let mut body_bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let remaining = MAX_SIZE.saturating_sub(body_bytes.len());
            if chunk.len() > remaining {
                body_bytes.extend_from_slice(&chunk[..remaining]);
                break;
            }
            body_bytes.extend_from_slice(&chunk);
        }

        let body = String::from_utf8_lossy(&body_bytes).into_owned();
        Ok((status, content_type, body))
    }

    // ── 7-Gate Validator ────────────────────────────────────────────────

    fn gate_validate(&self, status: u16, content_type: &str, body: &str) -> Result<()> {
        // Gate 1: HTTP status
        if !(200..400).contains(&status) {
            return Err(anyhow::anyhow!("HTTP error: status {}", status));
        }

        // Gate 2: Content-type sanity
        if content_type.contains("application/octet-stream")
            && body.len() > 1000
            && !body.is_ascii()
        {
            return Err(anyhow::anyhow!(
                "Binary content detected (content-type: {})",
                content_type
            ));
        }

        // Gate 3: Size check
        if body.len() > MAX_SIZE {
            return Err(anyhow::anyhow!(
                "Response too large: {} bytes (max {})",
                body.len(),
                MAX_SIZE
            ));
        }

        // Gate 4: Empty body check (allow for some status codes)
        if body.trim().is_empty() && status == 200 {
            return Err(anyhow::anyhow!("Empty response body"));
        }

        // Gate 5: Encoding sanity (UTF-8 or ASCII)
        // Already handled by from_utf8_lossy — replacement chars indicate issues
        if body.contains('\u{FFFD}') && body.matches('\u{FFFD}').count() > 10 {
            return Err(anyhow::anyhow!(
                "Response encoding issues (too many replacement characters)"
            ));
        }

        // Gate 6: Anti-bot detection (not a hard fail, just a flag)
        // Handled separately in detect_anti_bot()

        // Gate 7: Content quality — check it's not just an error page
        let lower = body.to_lowercase();
        if lower.contains("access denied")
            && lower.contains("you don't have permission")
            && body.len() < 5000
        {
            return Err(anyhow::anyhow!("Access denied by server"));
        }

        Ok(())
    }

    /// Detect anti-bot patterns in response body
    fn detect_anti_bot(&self, body: &str) -> bool {
        let lower = body.to_lowercase();
        let patterns = [
            "cf-browser-verification",
            "checking if the site connection is secure",
            "enable javascript and cookies to continue",
            "ray id:",
            "cloudflare",
            "challenge-platform",
            "just a moment",
            "verifying you are human",
            "please wait while we verify",
            "captcha",
            "recaptcha",
            "hcaptcha",
            "bot detected",
            "automated access",
            "access to this page has been denied",
        ];
        patterns.iter().any(|p| lower.contains(p))
    }

    // ── DOM Extraction (scraper crate) ──────────────────────────────────

    fn extract_by_selector(
        &self,
        document: &Html,
        selector_str: &str,
        format: &str,
    ) -> Result<String> {
        let selector = Selector::parse(selector_str)
            .map_err(|e| anyhow::anyhow!("Invalid CSS selector: {}", e))?;

        let elements: Vec<String> = document
            .select(&selector)
            .map(|el| self.element_to_output(el, format))
            .collect();

        if elements.is_empty() {
            return Ok(format!(
                "No elements matched selector: {}\n\nTip: Use browser dev tools to verify the selector.",
                selector_str
            ));
        }

        let output = elements.join("\n\n---\n\n");
        Ok(output)
    }

    fn extract_main_content(
        &self,
        document: &Html,
        _content_type: &str,
        format: &str,
    ) -> Result<String> {
        // Priority 1: Semantic tags
        let main_content = self.try_semantic_extraction(document);

        // Priority 2: Heuristic scoring (longest text block with low link density)
        let scored_content =
            if main_content.is_none() || main_content.as_deref().unwrap_or("").len() < 100 {
                self.try_heuristic_extraction(document)
            } else {
                main_content
            };

        match scored_content {
            Some(content) if !content.trim().is_empty() => {
                let output = match format {
                    "html" => content,
                    "text" => html_to_text(&content),
                    "markdown" => html_to_markdown(&content),
                    _ => html_to_markdown(&content),
                };
                Ok(output)
            }
            _ => {
                // Fallback: convert entire document
                let html_str = document.html();
                let output = match format {
                    "html" => html_str,
                    "text" => html_to_text(&html_str),
                    "markdown" => html_to_markdown(&html_str),
                    _ => html_to_markdown(&html_str),
                };
                Ok(output)
            }
        }
    }

    /// Try semantic HTML extraction: <article>, <main>, <section>
    fn try_semantic_extraction(&self, document: &Html) -> Option<String> {
        let selectors = [
            "article",
            "main",
            "[role='main']",
            ".post-content",
            ".article-content",
            ".entry-content",
            ".content",
            "#content",
            ".markdown-body",
        ];

        for sel_str in &selectors {
            if let Ok(sel) = Selector::parse(sel_str) {
                for el in document.select(&sel) {
                    let html = el.html();
                    if html.len() > 200 {
                        return Some(html);
                    }
                }
            }
        }
        None
    }

    /// Heuristic extraction: score elements by text density and link ratio
    fn try_heuristic_extraction(&self, document: &Html) -> Option<String> {
        // Score candidates: prefer elements with high text-to-HTML ratio
        // and low link density (navigation has lots of links, content has few)
        let candidates = ["div", "section", "td", "article"];
        let mut best: Option<(String, f64)> = None;

        for tag in &candidates {
            if let Ok(sel) = Selector::parse(tag) {
                for el in document.select(&sel) {
                    let html = el.html();
                    let text = el.text().collect::<String>();
                    let text_len = text.len() as f64;
                    let html_len = html.len() as f64;

                    if text_len < 100.0 {
                        continue;
                    }

                    // Text density: ratio of text to HTML tags
                    let text_density = text_len / html_len.max(1.0);

                    // Link density: ratio of link text to total text
                    let link_text: String = el
                        .select(&Selector::parse("a").ok()?)
                        .map(|a| a.text().collect::<String>())
                        .collect();
                    let link_ratio = link_text.len() as f64 / text_len.max(1.0);

                    // Score: high text density + low link density = good content
                    let score = text_density * (1.0 - link_ratio);

                    if score > 0.1 && (best.is_none() || score > best.as_ref().unwrap().1) {
                        best = Some((html, score));
                    }
                }
            }
        }

        best.map(|(html, _)| html)
    }

    fn element_to_output(&self, el: scraper::ElementRef, format: &str) -> String {
        match format {
            "html" => el.html(),
            "text" => {
                let text: String = el.text().collect();
                text.trim().to_string()
            }
            "markdown" => html_to_markdown(&el.html()),
            "json" => {
                let text: String = el.text().collect();
                let elem = el.value();
                let attrs: HashMap<String, String> = elem
                    .attrs
                    .iter()
                    .map(|(name, value)| (name.local.to_string(), value.to_string()))
                    .collect();
                json!({
                    "tag": elem.name(),
                    "text": text.trim(),
                    "html": el.html(),
                    "attributes": attrs,
                })
                .to_string()
            }
            _ => html_to_markdown(&el.html()),
        }
    }

    // ── Python Scrapling Integration ────────────────────────────────────

    async fn try_scrapling_python(
        &self,
        params: &ScraplingInput,
        timeout: u64,
    ) -> Result<PythonScraplingResult> {
        // Check if Python is available
        let python = find_python().ok_or_else(|| anyhow::anyhow!("Python not found"))?;

        // Check if scrapling is installed
        let check = tokio::process::Command::new(&python)
            .args(["-c", "import scrapling; print(scrapling.__version__)"])
            .output()
            .await;

        match check {
            Ok(output) if output.status.success() => {}
            _ => return Err(anyhow::anyhow!("Scrapling Python library not installed")),
        }

        // Create a temporary Python script
        let script = r#"
import json, sys
try:
    from scrapling import Fetcher
    url = sys.argv[1]
    timeout = int(sys.argv[2])
    fetcher = Fetcher(auto_match=False)
    page = fetcher.get(url, timeout=timeout)
    result = {
        "status": page.status if hasattr(page, 'status') else None,
        "title": page.css_first('title').text() if hasattr(page, 'css_first') else None,
        "text": page.get_all_text(separator='\n') if hasattr(page, 'get_all_text') else None,
        "html": str(page.html_content) if hasattr(page, 'html_content') else None,
        "url": str(page.url) if hasattr(page, 'url') else None,
    }
    print(json.dumps(result))
except Exception as e:
    print(json.dumps({"error": str(e)}))
    sys.exit(1)
"#
        .to_string();

        let temp_dir = tempfile::tempdir()?;
        let script_path = temp_dir.path().join("scrapling_fetch.py");
        tokio::fs::write(&script_path, &script).await?;

        let output = tokio::time::timeout(
            Duration::from_secs(timeout),
            tokio::process::Command::new(&python)
                .arg(script_path.to_str().unwrap_or(""))
                .arg(&params.url)
                .arg(timeout.to_string())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Scrapling Python execution timed out"))?
        .context("Failed to execute Scrapling Python script")?;

        // Cleanup
        let _ = tokio::fs::remove_file(&script_path).await;
        let _ = temp_dir.close();

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Scrapling Python error: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: PythonScraplingResult = serde_json::from_str(&stdout)
            .map_err(|e| anyhow::anyhow!("Failed to parse Scrapling output: {}", e))?;

        Ok(result)
    }

    fn format_python_result(
        &self,
        result: &PythonScraplingResult,
        url: &str,
        format: &str,
    ) -> Result<ToolOutput> {
        if let Some(err) = &result.error {
            return Err(anyhow::anyhow!("Scrapling error: {}", err));
        }

        let body = match format {
            "html" => result.html.as_deref().unwrap_or(""),
            "text" => result.text.as_deref().unwrap_or(""),
            "markdown" => result.text.as_deref().unwrap_or(""),
            _ => result.text.as_deref().unwrap_or(""),
        };

        let (output, truncated) = truncate_output(body.to_string());
        let title = result.title.as_deref().unwrap_or("untitled");
        let note = if truncated { "\n\n[Truncated]" } else { "" };

        Ok(ToolOutput::new(format!(
            "Fetched {} (browser: Scrapling)\nTitle: {}\n\n{}{}",
            url, title, output, note
        )))
    }

    // ── Browser Tool Fallback ───────────────────────────────────────────

    async fn fallback_to_browser_tool(
        &self,
        params: &ScraplingInput,
        format: &str,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        // Use the existing browser tool's get_content action
        let input = json!({
            "action": "get_content",
            "url": &params.url,
            "format": match format {
                "html" => "html",
                "text" | "markdown" => "text",
                _ => "text",
            }
        });

        // Auto-install browser bridge if missing
        let bin = crate::browser::browser_binary_path();
        if !bin.exists() {
            crate::browser::ensure_browser_setup().await?;
        }
        let bin = crate::browser::browser_binary_path();
        if !bin.exists() {
            return Err(anyhow::anyhow!(
                "Browser bridge installation failed. Use mode='http' or 'adaptive' instead."
            ));
        }

        let params_json = serde_json::to_string(&input)?;
        let output = tokio::process::Command::new(&bin)
            .arg("getContent")
            .arg(&params_json)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .context("Failed to run browser bridge for scrapling fallback")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Browser fallback failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: Value =
            serde_json::from_str(&stdout).unwrap_or_else(|_| json!({ "raw": stdout.to_string() }));

        let content = result["content"]
            .as_str()
            .or_else(|| result["text"].as_str())
            .or_else(|| result["html"].as_str())
            .unwrap_or(&stdout);

        let (output, truncated) = truncate_output(content.to_string());
        let note = if truncated { "\n\n[Truncated]" } else { "" };

        Ok(ToolOutput::new(format!(
            "Fetched {} (browser: Firefox fallback)\n\n{}{}",
            params.url, output, note
        )))
    }

    // ── Format Helpers ──────────────────────────────────────────────────

    fn format_output(&self, body: &str, content_type: &str, format: &str) -> String {
        match format {
            "html" => body.to_string(),
            "text" => html_to_text(body),
            "markdown" => {
                if content_type.contains("text/html") {
                    html_to_markdown(body)
                } else {
                    body.to_string()
                }
            }
            "json" => {
                // Try to parse as JSON and pretty-print
                if let Ok(val) = serde_json::from_str::<Value>(body) {
                    serde_json::to_string_pretty(&val).unwrap_or_else(|_| body.to_string())
                } else if content_type.contains("text/html") {
                    html_to_markdown(body)
                } else {
                    body.to_string()
                }
            }
            _ => {
                if content_type.contains("text/html") {
                    html_to_markdown(body)
                } else {
                    body.to_string()
                }
            }
        }
    }
}

/// Find available Python interpreter
fn find_python() -> Option<String> {
    for name in &["python3", "python", "py"] {
        if std::process::Command::new(name)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(name.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anti_bot_detection() {
        let tool = ScraplingTool::new();
        assert!(tool.detect_anti_bot("Just a moment... Please wait while we verify"));
        assert!(tool.detect_anti_bot("Checking if the site connection is secure"));
        assert!(tool.detect_anti_bot("Enable JavaScript and cookies to continue"));
        assert!(tool.detect_anti_bot("Ray ID: abc123"));
        assert!(!tool.detect_anti_bot("Hello world, this is normal content"));
        assert!(!tool.detect_anti_bot("<html><body>Normal page</body></html>"));
    }

    #[test]
    fn test_gate_validate_rejects_errors() {
        let tool = ScraplingTool::new();
        assert!(tool.gate_validate(404, "text/html", "Not Found").is_err());
        assert!(
            tool.gate_validate(500, "text/html", "Server Error")
                .is_err()
        );
        assert!(tool.gate_validate(200, "text/html", "").is_err());
    }

    #[test]
    fn test_gate_validate_accepts_valid() {
        let tool = ScraplingTool::new();
        assert!(
            tool.gate_validate(200, "text/html", "<html><body>OK</body></html>")
                .is_ok()
        );
        assert!(
            tool.gate_validate(200, "application/json", r#"{"key":"value"}"#)
                .is_ok()
        );
        assert!(tool.gate_validate(301, "text/html", "").is_ok()); // redirects can have empty body
    }

    #[test]
    fn test_extract_by_selector() {
        let tool = ScraplingTool::new();
        let html = r#"<html><body><div class="content"><p>Hello</p></div></body></html>"#;
        let doc = Html::parse_document(html);
        let result = tool.extract_by_selector(&doc, ".content", "text").unwrap();
        assert!(result.contains("Hello"));
    }

    #[test]
    fn test_semantic_extraction() {
        let tool = ScraplingTool::new();
        let html = r#"<html><body><nav>Menu</nav><article><p>Main article content here with enough text to pass the minimum length threshold for semantic extraction to kick in. This needs to be at least two hundred characters total so the algorithm considers it substantial content worth extracting from the page structure.</p></article></body></html>"#;
        let doc = Html::parse_document(html);
        let result = tool.try_semantic_extraction(&doc);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Main article content"));
    }

    #[test]
    fn test_format_output() {
        let tool = ScraplingTool::new();
        let html = "<p>Hello <b>world</b></p>";
        assert!(
            tool.format_output(html, "text/html", "html")
                .contains("<b>")
        );
        assert!(
            !tool
                .format_output(html, "text/html", "text")
                .contains("<b>")
        );
        assert!(
            tool.format_output(html, "text/html", "markdown")
                .contains("**world**")
        );
    }
}
