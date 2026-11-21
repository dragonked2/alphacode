use super::browser::{firefox_run_bridge_command, tab_id_from_value};
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
use tokio::time::{Instant, timeout_at};
use url::Url;

const MAX_SIZE: usize = 5 * 1024 * 1024; // 5MB
const DEFAULT_TIMEOUT: u64 = 30;
const MAX_TIMEOUT: u64 = 120;
// Leave room for JSON escapes and metadata in the browser bridge response.
const MAX_RENDERED_HTML: usize = 150_000;

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

#[derive(Deserialize, Default)]
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
struct RenderedPage {
    url: String,
    html: String,
    title: Option<String>,
    status: Option<u16>,
    #[serde(rename = "readyState")]
    ready_state: Option<String>,
    #[serde(default)]
    truncated: bool,
}

#[async_trait]
impl Tool for ScraplingTool {
    fn name(&self) -> &str {
        "scrapling"
    }

    fn description(&self) -> &str {
        "Fetch and extract web content with HTTP, CSS selectors, or browser rendering for \
         JavaScript-heavy pages. Returns the final page URL in browser mode."
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
                    "description": "Fetch strategy. http=fast HTTP only. adaptive=DOM parsing with CSS selectors. browser=JavaScript rendering. auto=try HTTP first (default).",
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
        // Recovered rather than deserialized, so an alias key or a bare URL
        // string still reaches the fetch instead of failing with a bare serde
        // "missing field `url`".
        let url = super::coerce_url_arg(&input, "scrapling")?;
        let mut params: ScraplingInput =
            serde_json::from_value(input.clone()).unwrap_or(ScraplingInput {
                url: url.clone(),
                ..Default::default()
            });
        // `coerce_url_arg` accepts a scheme-less host, but this tool's
        // downstream stages need an absolute URL, so require the scheme
        // explicitly rather than promoting `example.com` to a request.
        params.url = url;

        // `0` means "unset": forwarding it gave the Python subprocess a zero
        // timeout and failed every fetch immediately.
        let timeout = match params.timeout {
            Some(0) | None => DEFAULT_TIMEOUT,
            Some(n) => n.min(MAX_TIMEOUT),
        };
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
        let requested = Url::parse(&params.url).context("Invalid Scrapling browser URL")?;
        anyhow::ensure!(
            matches!(requested.scheme(), "http" | "https") && requested.has_host(),
            "Scrapling browser mode requires an HTTP or HTTPS URL"
        );
        let deadline = Instant::now() + Duration::from_secs(timeout);
        let (page, backend) = timeout_at(deadline, async {
            match fetch_scrapling_python(&requested, deadline).await {
                Ok(page) => Ok((page, "Scrapling")),
                Err(python_error) => fetch_firefox_page(&requested, deadline, |action, params| {
                    firefox_run_bridge_command(action, params, ctx)
                })
                .await
                .map(|page| (page, "Firefox"))
                .map_err(|firefox_error| {
                    anyhow::anyhow!(
                        "Firefox rendering failed: {firefox_error:#}. Scrapling Python browser failed: {python_error:#}"
                    )
                }),
            }
        })
        .await
        .context("Timed out rendering the requested Scrapling page")??;

        self.format_rendered_page(&page, &requested, params, format, backend)
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
        self.fetch_browser(params, timeout, format, ctx).await
    }

    // ── HTTP helpers ────────────────────────────────────────────────────

    async fn build_and_send_request(
        &self,
        params: &ScraplingInput,
        timeout: u64,
    ) -> Result<reqwest::Response> {
        let custom_user_agent = params.headers.as_ref().and_then(|headers| {
            headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
                .map(|(_, value)| value.clone())
        });
        let user_agent = custom_user_agent.unwrap_or_else(|| {
            crate::alphacode_provider_core::with_alphacode_brand(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
            )
        });
        let mut request_builder = self
            .client
            .get(&params.url)
            .header(reqwest::header::USER_AGENT, user_agent)
            .header(
                reqwest::header::ACCEPT,
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header(reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.9")
            .timeout(Duration::from_secs(timeout));

        if let Some(headers) = &params.headers {
            for (key, value) in headers {
                if key.eq_ignore_ascii_case("user-agent") {
                    continue;
                }
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

    /// Detect anti-bot challenge pages in a response body.
    ///
    /// This is a *false-positive* risk, not just a false-negative one: the
    /// marker list used to include bare words like `cloudflare` and `captcha`,
    /// so a security write-up that merely discusses Cloudflare, or any page
    /// with the word "captcha" on it, was reported to the model as
    /// bot-blocked. That escalates `mode` to `browser` and burns a headless
    /// browser for a page that was fetched perfectly well — and, worse, stamps
    /// a false "content may be partial" caveat on real content.
    ///
    /// Two guards, both cheap:
    /// * Only look at a *short* body. A real interstitial is tiny; a page that
    ///   mentions Cloudflare in prose is not.
    /// * Require a *strong* marker. Weak/ambiguous words only count when the
    ///   body is very short, i.e. consistent with a challenge page.
    fn detect_anti_bot(&self, body: &str) -> bool {
        let lower = body.to_lowercase();
        // A challenge interstitial has essentially no content.
        let is_short = body.split_whitespace().count() < 200;
        if !is_short {
            return false;
        }

        // Unambiguous challenge/interstitial signatures.
        const STRONG: &[&str] = &[
            "cf-browser-verification",
            "checking if the site connection is secure",
            "enable javascript and cookies to continue",
            "challenge-platform",
            "just a moment...",
            "verifying you are human",
            "verify you are human",
            "please wait while we verify",
            "attention required! | cloudflare",
            "checking your browser before accessing",
            "ddos protection by",
            "unusual traffic from your computer network",
            // A Cloudflare Ray ID is emitted on challenge/error interstitials
            // and is not a word that appears in ordinary security prose. It is
            // only meaningful in a short body, which the guard above ensures.
            "ray id:",
        ];
        if STRONG.iter().any(|p| lower.contains(p)) {
            return true;
        }

        // Ambiguous words: only decisive on a *very* short body, where a page
        // is essentially just the challenge.
        if body.split_whitespace().count() < 60 {
            const WEAK: &[&str] = &[
                "cloudflare",
                "captcha",
                "recaptcha",
                "hcaptcha",
                "bot detected",
                "automated access",
                "access to this page has been denied",
            ];
            return WEAK.iter().any(|p| lower.contains(p));
        }
        false
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

    fn format_rendered_page(
        &self,
        page: &RenderedPage,
        requested: &Url,
        params: &ScraplingInput,
        format: &str,
        backend: &str,
    ) -> Result<ToolOutput> {
        let landed = validate_rendered_url(requested, &page.url)?;
        self.gate_validate(page.status.unwrap_or(200), "text/html", &page.html)?;
        let output = match params.extract.as_deref() {
            Some(selector) => {
                self.extract_by_selector(&Html::parse_document(&page.html), selector, format)?
            }
            None => self.format_output(&page.html, "text/html", format),
        };
        let (output, truncated) = truncate_output(output);
        let title = page.title.as_deref().filter(|title| !title.is_empty());
        let title = title.map_or_else(String::new, |title| format!("\nTitle: {title}"));
        let note = if page.truncated {
            "\n\n[Rendered document was partially captured]"
        } else if truncated {
            "\n\n[Truncated]"
        } else {
            ""
        };

        Ok(ToolOutput::new(format!(
            "Fetched {landed} (browser: {backend}){title}\n\n{output}{note}"
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

fn validate_rendered_url(requested: &Url, landed: &str) -> Result<Url> {
    let landed = Url::parse(landed).context("Browser returned an invalid document URL")?;
    let same_host = match (requested.host_str(), landed.host_str()) {
        (Some(requested_host), Some(landed_host)) => {
            requested_host
                .strip_prefix("www.")
                .unwrap_or(requested_host)
                == landed_host.strip_prefix("www.").unwrap_or(landed_host)
        }
        _ => false,
    };
    let same_scheme = requested.scheme() == landed.scheme();
    let https_upgrade = requested.scheme() == "http" && landed.scheme() == "https";
    anyhow::ensure!(
        same_host && (same_scheme || https_upgrade) && requested.port() == landed.port(),
        "Browser opened {landed} while rendering {requested}. Content from a different site was withheld."
    );
    Ok(landed)
}

async fn fetch_scrapling_python(requested: &Url, deadline: Instant) -> Result<RenderedPage> {
    let python = super::python::find_python()
        .await
        .context("Python not found")?;
    let script = r#"
import json
import sys
from scrapling.fetchers import DynamicFetcher

url, timeout, limit = sys.argv[1:]
page = DynamicFetcher.fetch(
    url, timeout=int(timeout), headless=True, google_search=False, retries=1,
)
limit = int(limit)
html = page.body.decode("utf-8", errors="replace")
title = page.css("title::text").get()
print(json.dumps({
    "url": str(page.url),
    "status": page.status,
    "title": title[:512] if title else None,
    "html": html[:limit],
    "truncated": len(html) > limit,
}, ensure_ascii=False))
"#;
    let mut command = tokio::process::Command::new(python);
    let remaining = deadline.saturating_duration_since(Instant::now());
    command
        .env("PYTHONIOENCODING", "utf-8")
        .args(["-c", script, requested.as_str()])
        .arg(remaining.as_millis().to_string())
        .arg(MAX_RENDERED_HTML.to_string());
    crate::alphacode_core::env::strip_internal_env(&mut command);
    let output =
        super::recon_common::run_command_bounded(command, "Scrapling Python browser", remaining)
            .await
            .map_err(anyhow::Error::msg)?;
    anyhow::ensure!(
        output.status.success(),
        "Scrapling Python browser failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(1200)
            .collect::<String>()
    );
    serde_json::from_slice(&output.stdout).context("Invalid Scrapling Python browser response")
}

async fn fetch_firefox_page<F, Fut>(
    requested: &Url,
    deadline: Instant,
    mut bridge: F,
) -> Result<RenderedPage>
where
    F: FnMut(&'static str, Value) -> Fut,
    Fut: std::future::Future<Output = Result<Value>>,
{
    let remaining = deadline.saturating_duration_since(Instant::now());
    let mut result = bridge(
        "createTab",
        json!({"url": requested.as_str(), "focus": false, "timeoutMs": remaining.as_millis()}),
    )
    .await?;
    let id =
        tab_id_from_value(&result).context("Firefox did not identify the new Scrapling tab")?;
    let script = format!(
        r#"(() => {{
            const html = document.documentElement?.outerHTML ?? "";
            return {{
                url: location.href,
                readyState: document.readyState,
                title: document.title.slice(0, 512).replace(/[\uD800-\uDBFF]$/, ""),
                html: html.slice(0, {MAX_RENDERED_HTML}).replace(/[\uD800-\uDBFF]$/, ""),
                truncated: html.length > {MAX_RENDERED_HTML}
            }};
        }})()"#
    );

    loop {
        anyhow::ensure!(
            tab_id_from_value(&result) == Some(id),
            "Firefox returned a different tab while loading {requested}"
        );
        let tab = result.get("tab").unwrap_or(&result);
        if tab["status"] == "complete" {
            let tab_url = tab["url"]
                .as_str()
                .context("Firefox omitted the Scrapling tab URL")?;
            if tab_url != "about:blank" {
                let mut landed = validate_rendered_url(requested, tab_url)?;
                let remaining = deadline.saturating_duration_since(Instant::now());
                let mut snapshot = bridge(
                    "evaluate",
                    json!({
                        "tabId": id,
                        "frameId": 0,
                        "pageWorld": false,
                        "script": &script,
                        "timeoutMs": remaining.as_millis()
                    }),
                )
                .await?;
                // The native CLI wraps evaluate results with engine metadata.
                let snapshot = snapshot
                    .as_object_mut()
                    .and_then(|object| object.remove("result"))
                    .unwrap_or(snapshot);
                let page: RenderedPage = serde_json::from_value(snapshot)
                    .context("Firefox omitted the rendered document URL or HTML")?;
                let mut document =
                    Url::parse(&page.url).context("Firefox returned an invalid document URL")?;
                // A fragment can change without a different document loading.
                landed.set_fragment(None);
                document.set_fragment(None);
                if document != landed && document.as_str() != "about:blank" {
                    anyhow::bail!(
                        "Firefox's Scrapling tab changed from {landed} to {document} during extraction"
                    );
                }
                if document == landed && page.ready_state.as_deref() == Some("complete") {
                    return Ok(page);
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(350)).await;
        result = bridge("getTab", json!({"tabId": id})).await?;
    }
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

    #[tokio::test]
    async fn scrapling_browser_uses_a_fresh_tab_and_applies_the_selector() {
        let tool = ScraplingTool::new();
        for (id, host, content) in [
            (41, "docs.example", "First"),
            (42, "market.example", "Second"),
        ] {
            let requested = Url::parse(&format!("https://{host}/old")).unwrap();
            let landed = format!("https://www.{host}/current");
            let mut actions = ["createTab", "getTab", "evaluate"].into_iter();
            let page = fetch_firefox_page(&requested, Instant::now() + Duration::from_secs(2), |action, input| {
                assert_eq!(actions.next(), Some(action));
                let result = match action {
                    "createTab" => {
                        assert_eq!(input["url"], requested.as_str());
                        assert_eq!(input["focus"], false);
                        json!({"tabId": id, "url": "about:blank", "status": "complete"})
                    }
                    "getTab" => {
                        assert_eq!(input["tabId"], id);
                        json!({"tab": {"tabId": id, "url": landed, "status": "complete"}})
                    }
                    "evaluate" => {
                        assert_eq!(input["tabId"], id);
                        assert_eq!(input["frameId"], 0);
                        json!({"engine": "firefox", "result": {"url": landed, "readyState": "complete", "html": format!("<div>drop me</div><main>{content}</main>")}})
                    }
                    _ => unreachable!(),
                };
                std::future::ready(Ok(result))
            })
            .await
            .unwrap();
            assert_eq!(actions.next(), None);
            let params = ScraplingInput {
                extract: Some("main".into()),
                ..Default::default()
            };
            let output = tool
                .format_rendered_page(&page, &requested, &params, "text", "Firefox")
                .unwrap()
                .output;
            assert!(output.starts_with(&format!("Fetched {landed}")));
            assert!(output.ends_with(content));
            assert!(!output.contains("drop me"));
        }
    }

    #[tokio::test]
    async fn scrapling_browser_rejects_unverified_or_stale_documents() {
        let requested = Url::parse("https://market.example/current").unwrap();
        for (allocated, snapshot, expected) in [
            (
                json!({"url": requested.as_str(), "status": "complete"}),
                Value::Null,
                "did not identify the new Scrapling tab",
            ),
            (
                json!({"tabId": 42, "url": "https://docs.example/changelog", "status": "complete"}),
                Value::Null,
                "different site",
            ),
            (
                json!({"tabId": 42, "url": requested.as_str(), "status": "complete"}),
                json!({"result": {"html": "Missing URL", "readyState": "complete"}}),
                "omitted the rendered document URL or HTML",
            ),
            (
                json!({"tabId": 42, "url": requested.as_str(), "status": "complete"}),
                json!({"url": "https://market.example/old", "html": "Stale", "readyState": "complete"}),
                "during extraction",
            ),
        ] {
            let result = fetch_firefox_page(
                &requested,
                Instant::now() + Duration::from_secs(2),
                |action, input| {
                    let result = match action {
                        "createTab" => allocated.clone(),
                        "evaluate" => {
                            assert_eq!(input["tabId"], 42);
                            assert_ne!(snapshot, Value::Null);
                            snapshot.clone()
                        }
                        _ => panic!("Unexpected bridge action {action}"),
                    };
                    std::future::ready(Ok(result))
                },
            )
            .await
            .expect_err("unverified content must never be returned");
            assert!(result.to_string().contains(expected), "{result:#}");
        }
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

    /// Regression: `detect_anti_bot` matched bare words like `cloudflare` and
    /// `captcha` in a body of any length, so a security write-up mentioning
    /// Cloudflare was reported as bot-blocked. That escalates to a headless
    /// browser and stamps a false "content may be partial" note on real
    /// content — a false positive in the exact place the tool is supposed to
    /// be trustworthy.
    #[test]
    fn anti_bot_detection_does_not_fire_on_ordinary_prose() {
        let tool = ScraplingTool::new();

        // A security blog post that merely talks about Cloudflare and captchas.
        let prose = format!(
            "{} Our WAF is fronted by Cloudflare. We recently added a captcha \
             on the login form to stop credential stuffing. The hCaptcha \
             integration was reviewed by the security team. Automated access \
             to the API is rate limited per token.",
            "Lorem ipsum dolor sit amet. ".repeat(40)
        );
        assert!(
            !tool.detect_anti_bot(&prose),
            "false positive on prose mentioning Cloudflare/captcha"
        );
    }

    #[test]
    fn anti_bot_detection_still_fires_on_real_challenge_pages() {
        let tool = ScraplingTool::new();
        assert!(tool.detect_anti_bot(
            "<html><head><title>Just a moment...</title></head><body>\
             Checking if the site connection is secure</body></html>"
        ));
        assert!(tool.detect_anti_bot(
            "<html><body>Please enable JavaScript and cookies to continue. \
             Ray ID: 8a1b2c3d</body></html>"
        ));
        assert!(tool.detect_anti_bot("<html><body>Verify you are human</body></html>"));
    }
}
