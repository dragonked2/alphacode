use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;

const MAX_SIZE: usize = 5 * 1024 * 1024; // 5MB
/// Cap on the text handed back to the model. Full pages routinely exceed 150 KB
/// (~40k tokens) which is rarely worth the context budget.
const MAX_OUTPUT_CHARS: usize = 30_000;
/// Links whose target exceeds this length are rendered as their anchor text
/// only. Long URLs are typically encoded payloads (pre-filled editors, tracking
/// parameters, data URIs) whose cost far exceeds their navigational value.
const MAX_URL_CHARS: usize = 300;
const DEFAULT_TIMEOUT: u64 = 30;
const MAX_TIMEOUT: u64 = 120;

/// User-Agent strings tried in order when anti-bot challenges are detected.
/// The first is a generic bot UA (fast, low footprint); subsequent ones mimic
/// real browsers to bypass Cloudflare/bot-detection heuristics.
const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (compatible; Alphacode/1.0)",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15",
];

/// Check if a hostname resolves to an RFC 1918 private IP range.
/// Used for SSRF protection to prevent fetching internal network resources.
fn is_private_ip(host: &str) -> bool {
    // Parse first octet for quick range checks
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    let octets: Vec<u8> = parts.iter().filter_map(|p| p.parse().ok()).collect();
    if octets.len() != 4 {
        return false;
    }
    match octets[0] {
        // 10.0.0.0/8
        10 => true,
        // 172.16.0.0/12
        172 if (16..=31).contains(&octets[1]) => true,
        // 192.168.0.0/16
        192 if octets[1] == 168 => true,
        // 169.254.0.0/16 (link-local)
        169 if octets[1] == 254 => true,
        _ => false,
    }
}

/// Returns `true` when `body` looks like an anti-bot challenge page rather
/// than real content. These pages typically have very little body text and
/// contain known challenge markers.
fn detect_anti_bot_page(body: &str) -> Option<&'static str> {
    let lower = body.to_ascii_lowercase();
    // Cloudflare challenge
    if lower.contains("cf-browser-verification")
        || lower.contains("checking your browser")
        || lower.contains("just a moment")
        || lower.contains("enable javascript and cookies")
        || lower.contains("ray id")
    {
        return Some("Cloudflare challenge");
    }
    // PerimeterX / HUMAN
    if lower.contains("px-captcha") || lower.contains("please verify you are human") {
        return Some("PerimeterX challenge");
    }
    // Generic "access denied" or captcha pages with minimal real content
    if (lower.contains("access denied") || lower.contains("captcha"))
        && body.len() < 50_000
        && body.split_whitespace().count() < 200
    {
        return Some("generic captcha/denial page");
    }
    None
}

pub struct WebFetchTool {
    client: reqwest::Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            client: crate::provider::shared_http_client(),
        }
    }
}

#[derive(Deserialize)]
struct WebFetchInput {
    url: String,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    headers: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    body: Option<String>,
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "webfetch"
    }

    fn description(&self) -> &str {
        "Fetch a URL and return its body as text, markdown, or raw HTML. \
         Supports GET, POST, PUT, DELETE methods. Can send custom headers \
         (including cookies for session auth) and request body. \
         Use for reading pages, API calls, form submissions, and authorized \
         security testing against in-scope targets."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "intent": super::intent_schema_property(),
                "url": {
                    "type": "string",
                    "description": "URL."
                },
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE", "PATCH"],
                    "description": "HTTP method. Default: GET."
                },
                "headers": {
                    "type": "object",
                    "description": "Custom HTTP headers as key-value pairs. Use for cookies, auth tokens, content-type, etc."
                },
                "body": {
                    "type": "string",
                    "description": "Request body for POST/PUT/PATCH."
                },
                "format": {
                    "type": "string",
                    "enum": ["text", "markdown", "html"],
                    "description": "Output format."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds."
                }
            }
        })
    }

    fn execution_class(&self, input: &Value) -> super::ToolExecutionClass {
        let method = input
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("GET")
            .trim()
            .to_ascii_uppercase();
        if method == "GET" {
            super::ToolExecutionClass::ReadOnly
        } else {
            super::ToolExecutionClass::ExternalEffect
        }
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: WebFetchInput = serde_json::from_value(input)?;

        // Validate URL
        if !params.url.starts_with("http://") && !params.url.starts_with("https://") {
            return Err(anyhow::anyhow!("URL must start with http:// or https://"));
        }

        // SSRF protection: block requests to internal/private network addresses,
        // cloud metadata endpoints, and localhost.
        if let Ok(parsed) = url::Url::parse(&params.url)
            && let Some(host) = parsed.host_str()
        {
            let host_lower = host.to_ascii_lowercase();
            // Block localhost variants
            if host_lower == "localhost"
                || host_lower == "0.0.0.0"
                || host_lower == "127.0.0.1"
                || host_lower == "::1"
                || host_lower.ends_with(".local")
            {
                return Err(anyhow::anyhow!(
                    "Blocked: URL targets localhost/internal host ({host}). \
                     Refusing SSRF request."
                ));
            }
            // Block cloud metadata endpoints
            if host_lower == "169.254.169.254"
                || host_lower == "100.100.100.200"
                || host_lower == "metadata.google.internal"
            {
                return Err(anyhow::anyhow!(
                    "Blocked: URL targets cloud metadata endpoint ({host}). \
                     Refusing SSRF request."
                ));
            }
            // Block RFC 1918 private ranges
            if is_private_ip(&host_lower) {
                return Err(anyhow::anyhow!(
                    "Blocked: URL targets private network address ({host}). \
                     Refusing SSRF request."
                ));
            }
        }

        let timeout = params.timeout.unwrap_or(DEFAULT_TIMEOUT).min(MAX_TIMEOUT);
        let format = params.format.as_deref().unwrap_or("markdown");
        let method = params.method.as_deref().unwrap_or("GET").to_uppercase();

        let mut last_err: Option<anyhow::Error> = None;

        for (attempt, &ua) in USER_AGENTS.iter().enumerate() {
            // Build request with method
            let mut request_builder = match method.as_str() {
                "POST" => self.client.post(&params.url),
                "PUT" => self.client.put(&params.url),
                "DELETE" => self.client.delete(&params.url),
                "PATCH" => self.client.patch(&params.url),
                _ => self.client.get(&params.url),
            };

            // Add User-Agent
            request_builder = request_builder
                .header(reqwest::header::USER_AGENT, ua)
                .timeout(Duration::from_secs(timeout));

            // Add custom headers
            if let Some(ref headers) = params.headers {
                for (key, value) in headers {
                    request_builder = request_builder.header(key.as_str(), value.as_str());
                }
            }

            // Add body for POST/PUT/PATCH
            if let Some(ref body) = params.body {
                // Auto-detect JSON if body starts with { or [
                let content_type =
                    if body.trim_start().starts_with('{') || body.trim_start().starts_with('[') {
                        "application/json"
                    } else {
                        "application/x-www-form-urlencoded"
                    };
                // Allow user-specified Content-Type to override
                let final_ct = params
                    .headers
                    .as_ref()
                    .and_then(|h| h.get("content-type").or_else(|| h.get("Content-Type")))
                    .map(|s| s.as_str())
                    .unwrap_or(content_type);
                request_builder = request_builder
                    .header(reqwest::header::CONTENT_TYPE, final_ct)
                    .body(body.clone());
            }

            let request = request_builder
                .build()
                .context("failed to build webfetch request")?;

            let response = if attempt == 0 {
                // First attempt uses the standard retry policy (handles transient HTTP errors).
                crate::alphacode_provider_core::retry::send_with_retry(
                    &self.client,
                    request,
                    &crate::alphacode_provider_core::retry::policy_with_tui_toast(
                        crate::alphacode_provider_core::retry::RetryPolicy::for_http_tools(),
                    ),
                    "webfetch",
                )
                .await
            } else {
                // Subsequent attempts (anti-bot fallback) are direct sends — the
                // first attempt already exhausted transient retries.
                self.client.execute(request).await.map_err(|e| e.into())
            };

            let response = match response {
                Ok(r) => r,
                Err(e) => {
                    last_err = Some(e.into());
                    continue;
                }
            };

            let status = response.status();
            if !status.is_success() {
                // Smart 404: don't waste UA retries on a missing page, and tell
                // the agent to discover the URL instead of guessing variants.
                if status == reqwest::StatusCode::NOT_FOUND {
                    return Err(anyhow::anyhow!(
                        "HTTP error: 404 Not Found for {}. URL does not exist — do NOT retry with different User-Agents or guess similar deep URLs (e.g. /uniswap/ vs /uniswap-v3/). Use websearch to discover the correct URL, try the site index, or check trailing-slash variant once.",
                        params.url
                    ));
                }
                last_err = Some(anyhow::anyhow!("HTTP error: {}", status));
                continue;
            }

            // Capture key headers before consuming response
            let resp_headers: HashMap<String, String> = response
                .headers()
                .iter()
                .filter(|(k, _)| {
                    matches!(
                        k.as_str(),
                        "set-cookie"
                            | "location"
                            | "content-type"
                            | "x-frame-options"
                            | "content-security-policy"
                    )
                })
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();

            // Check content length
            if let Some(len) = response.content_length()
                && len as usize > MAX_SIZE
            {
                return Err(anyhow::anyhow!(
                    "Response too large: {} bytes (max {} bytes)",
                    len,
                    MAX_SIZE
                ));
            }

            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();

            let mut body_bytes = Vec::new();
            let mut truncated = false;
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                let remaining = MAX_SIZE.saturating_sub(body_bytes.len());
                if chunk.len() > remaining {
                    body_bytes.extend_from_slice(&chunk[..remaining]);
                    truncated = true;
                    break;
                }
                body_bytes.extend_from_slice(&chunk);
            }

            let mut body = String::from_utf8_lossy(&body_bytes).into_owned();
            if truncated {
                body.push_str(&format!(
                    "...\n\n(truncated, showing first {} bytes)",
                    MAX_SIZE
                ));
            }

            // Anti-bot detection: if the page looks like a challenge and we
            // have more User-Agents to try, retry silently.
            if attempt + 1 < USER_AGENTS.len()
                && let Some(reason) = detect_anti_bot_page(&body)
            {
                crate::logging::info(&format!(
                    "webfetch: {reason} detected for {}, retrying with fallback User-Agent (attempt {}/{})",
                    params.url,
                    attempt + 2,
                    USER_AGENTS.len(),
                ));
                last_err = Some(anyhow::anyhow!(
                    "anti-bot challenge ({reason}), retrying with different browser fingerprint"
                ));
                continue;
            }

            // Format output
            let output = match format {
                "html" => body,
                "text" => html_to_text(&body),
                "markdown" => {
                    if content_type.contains("text/html") {
                        html_to_markdown(&body)
                    } else {
                        body
                    }
                }
                _ => {
                    if content_type.contains("text/html") {
                        html_to_markdown(&body)
                    } else {
                        body
                    }
                }
            };

            let full_len = output.len();
            let (output, output_truncated) = truncate_output(output);

            let note = if output_truncated {
                let saved_k = (full_len - output.len()) / 4 / 1000;
                format!(
                    "\n\n[Truncated: ~{saved_k}k tokens saved — fetch a more specific URL or anchor for the rest]"
                )
            } else {
                String::new()
            };

            // Compact header: show size in KB with one decimal, and line count
            // for quick context assessment.
            let line_count = output.lines().count();
            let mut header = format!(
                "Fetched {} ({:.1}KB, {} lines)\n",
                params.url,
                full_len as f64 / 1024.0,
                line_count,
            );
            if attempt > 0 {
                header.push_str("(retrieved with fallback User-Agent after anti-bot challenge)\n");
            }
            // Show important response headers
            for h in ["set-cookie", "location", "content-security-policy"] {
                if let Some(val) = resp_headers.get(h) {
                    header.push_str(&format!("{}: {}\n", h, val));
                }
            }
            return Ok(ToolOutput::new(format!("{}\n\n{}{}", header, output, note)));
        }

        // All User-Agents exhausted
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("webfetch: all attempts failed")))
    }
}

/// Truncate at a char boundary, preferring to cut at the last newline so the tail
/// is not a half-formed line.
pub(crate) fn truncate_output(output: String) -> (String, bool) {
    if output.len() <= MAX_OUTPUT_CHARS {
        return (output, false);
    }
    let mut cut = MAX_OUTPUT_CHARS;
    while cut > 0 && !output.is_char_boundary(cut) {
        cut -= 1;
    }
    let slice = &output[..cut];
    let cut = match slice.rfind('\n') {
        Some(nl) if nl > MAX_OUTPUT_CHARS / 2 => nl,
        _ => cut,
    };
    (output[..cut].to_string(), true)
}

pub(crate) mod html_regex {
    use regex::Regex;
    use std::sync::OnceLock;

    fn compile_regex(pattern: &str, label: &str) -> Option<Regex> {
        match Regex::new(pattern) {
            Ok(regex) => Some(regex),
            Err(err) => {
                crate::logging::warn(&format!(
                    "webfetch: failed to compile static regex {label}: {}",
                    err
                ));
                None
            }
        }
    }

    macro_rules! static_regex {
        ($name:ident, $pat:expr_2021) => {
            pub fn $name() -> Option<&'static Regex> {
                static RE: OnceLock<Option<Regex>> = OnceLock::new();
                RE.get_or_init(|| compile_regex($pat, stringify!($name)))
                    .as_ref()
            }
        };
    }

    static_regex!(script, r"(?is)<script[^>]*>.*?</script>");
    static_regex!(style, r"(?is)<style[^>]*>.*?</style>");
    // Match attribute values (which may themselves contain `>`) before falling
    // back to bare `>`-terminated content, so tags carrying JSON payloads such as
    // Parsoid's `data-mw` do not leak their contents into the output.
    static_regex!(
        tag,
        r#"(?s)</?[A-Za-z!/][^\s/>]*(?:\s+[^\s=/>]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]*))?)*\s*/?>"#
    );
    static_regex!(whitespace, r"\n\s*\n\s*\n");
    // Runs of empty markdown list items left behind after tag stripping.
    static_regex!(empty_bullets, r"(?m)^[ \t]*-[ \t]*$\n?");

    /// HTML elements whose content is non-prose by specification: navigation,
    /// complementary/tangential content, interactive controls, and embedded
    /// non-text resources. This is deliberately limited to elements whose *spec
    /// definition* excludes primary content, so it generalizes across sites
    /// rather than encoding any single site's markup.
    ///
    /// Notably excludes `<header>`, which commonly wraps the article `<h1>`,
    /// byline, and publication date, and `<footer>`, which can carry
    /// article-level attribution when nested inside `<article>`.
    const CHROME_TAGS: [&str; 10] = [
        "nav", "aside", "form", "noscript", "svg", "iframe", "template", "select", "dialog",
        "canvas",
    ];

    static CHROME: OnceLock<Vec<Regex>> = OnceLock::new();

    pub fn chrome() -> &'static [Regex] {
        CHROME.get_or_init(|| {
            CHROME_TAGS
                .iter()
                .filter_map(|tag| {
                    compile_regex(&format!(r"(?is)<{tag}\b[^>]*>.*?</{tag}\s*>"), "chrome")
                })
                .collect()
        })
    }
    static_regex!(link, r#"(?i)<a[^>]*href=["']([^"']+)["'][^>]*>([^<]*)</a>"#);
    static_regex!(strong, r"(?i)<(?:strong|b)>([^<]*)</(?:strong|b)>");
    static_regex!(em, r"(?i)<(?:em|i)>([^<]*)</(?:em|i)>");
    static_regex!(code, r"(?i)<code>([^<]*)</code>");
    static_regex!(pre_code, r"(?is)<pre[^>]*><code[^>]*>(.+?)</code></pre>");
    static_regex!(li, r"(?i)<li[^>]*>");
    // HTML comments frequently contain build metadata, conditional markup, and
    // commented-out blocks, none of which are rendered content.
    static_regex!(comment, r"(?s)<!--.*?-->");

    static H_OPEN: OnceLock<Option<[Regex; 6]>> = OnceLock::new();
    static H_CLOSE: OnceLock<Option<[Regex; 6]>> = OnceLock::new();

    pub fn h_open() -> Option<&'static [Regex; 6]> {
        H_OPEN
            .get_or_init(|| {
                let mut compiled = Vec::with_capacity(6);
                for i in 0..6 {
                    let pattern = format!(r"(?i)<h{}[^>]*>", i + 1);
                    compiled.push(compile_regex(&pattern, "heading open")?);
                }
                compiled.try_into().ok()
            })
            .as_ref()
    }

    pub fn h_close() -> Option<&'static [Regex; 6]> {
        H_CLOSE
            .get_or_init(|| {
                let mut compiled = Vec::with_capacity(6);
                for i in 0..6 {
                    let pattern = format!(r"(?i)</h{}>", i + 1);
                    compiled.push(compile_regex(&pattern, "heading close")?);
                }
                compiled.try_into().ok()
            })
            .as_ref()
    }
}

pub(crate) fn html_to_text(html: &str) -> String {
    let mut text = html.to_string();

    let (Some(script), Some(style), Some(tag), Some(whitespace)) = (
        html_regex::script(),
        html_regex::style(),
        html_regex::tag(),
        html_regex::whitespace(),
    ) else {
        return html.trim().to_string();
    };

    text = script.replace_all(&text, "").to_string();
    text = style.replace_all(&text, "").to_string();
    if let Some(comment) = html_regex::comment() {
        text = comment.replace_all(&text, "").to_string();
    }
    for re in html_regex::chrome() {
        text = re.replace_all(&text, "").to_string();
    }

    text = text.replace("<br>", "\n");
    text = text.replace("<br/>", "\n");
    text = text.replace("<br />", "\n");
    text = text.replace("</p>", "\n\n");
    text = text.replace("</div>", "\n");
    text = text.replace("</li>", "\n");
    text = text.replace("</tr>", "\n");

    text = tag.replace_all(&text, "").to_string();

    text = text.replace("&nbsp;", " ");
    text = text.replace("&lt;", "<");
    text = text.replace("&gt;", ">");
    text = text.replace("&amp;", "&");
    text = text.replace("&quot;", "\"");
    text = text.replace("&#39;", "'");

    text = whitespace.replace_all(&text, "\n\n").to_string();

    text.trim().to_string()
}

/// Render one anchor as markdown, dropping targets that cost more context than
/// they convey.
///
/// Three general cases, none specific to any site:
/// - Empty anchor text means the link is a bare icon or control. Emitting
///   `[](url)` conveys nothing, so the whole link is dropped.
/// - Overlong targets are encoded payloads rather than addresses; the anchor
///   text is kept and the target dropped.
/// - Pure in-page fragments (`#foo`) are navigation aids with no destination
///   content, so the text is kept and the target dropped.
pub(crate) fn render_link(href: &str, text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }
    let href = href.trim();
    if href.is_empty() || href.starts_with('#') || href.chars().count() > MAX_URL_CHARS {
        return text.to_string();
    }
    format!("[{text}]({href})")
}

pub(crate) fn html_to_markdown(html: &str) -> String {
    let mut md = html.to_string();

    let (
        Some(script),
        Some(style),
        Some(link),
        Some(strong),
        Some(em),
        Some(code),
        Some(pre_code),
        Some(li),
        Some(tag),
        Some(whitespace),
    ) = (
        html_regex::script(),
        html_regex::style(),
        html_regex::link(),
        html_regex::strong(),
        html_regex::em(),
        html_regex::code(),
        html_regex::pre_code(),
        html_regex::li(),
        html_regex::tag(),
        html_regex::whitespace(),
    )
    else {
        return html.trim().to_string();
    };

    md = script.replace_all(&md, "").to_string();
    md = style.replace_all(&md, "").to_string();
    if let Some(comment) = html_regex::comment() {
        md = comment.replace_all(&md, "").to_string();
    }
    for re in html_regex::chrome() {
        md = re.replace_all(&md, "").to_string();
    }

    if let (Some(h_open), Some(h_close)) = (html_regex::h_open(), html_regex::h_close()) {
        for i in 0..6 {
            let prefix = "#".repeat(i + 1);
            md = h_open[i]
                .replace_all(&md, &format!("\n{} ", prefix))
                .to_string();
            md = h_close[i].replace_all(&md, "\n").to_string();
        }
    }

    md = link
        .replace_all(&md, |caps: &regex::Captures<'_>| {
            render_link(
                caps.get(1).map_or("", |m| m.as_str()),
                caps.get(2).map_or("", |m| m.as_str()),
            )
        })
        .to_string();
    md = strong.replace_all(&md, "**$1**").to_string();
    md = em.replace_all(&md, "*$1*").to_string();
    md = code.replace_all(&md, "`$1`").to_string();
    md = pre_code.replace_all(&md, "\n```\n$1\n```\n").to_string();
    md = li.replace_all(&md, "\n- ").to_string();

    md = md.replace("<br>", "\n");
    md = md.replace("<br/>", "\n");
    md = md.replace("<br />", "\n");
    md = md.replace("</p>", "\n\n");

    md = tag.replace_all(&md, "").to_string();

    md = md.replace("&nbsp;", " ");
    md = md.replace("&lt;", "<");
    md = md.replace("&gt;", ">");
    md = md.replace("&amp;", "&");
    md = md.replace("&quot;", "\"");
    md = md.replace("&#39;", "'");

    if let Some(empty_bullets) = html_regex::empty_bullets() {
        md = empty_bullets.replace_all(&md, "").to_string();
    }
    md = whitespace.replace_all(&md, "\n\n").to_string();

    md.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_non_prose_elements() {
        let html = "<nav><a href='/x'>Menu</a></nav><p>Body text</p>\
                    <aside>Related</aside><form><select><option>Pick</option></select></form>";
        let md = html_to_markdown(html);
        assert!(md.contains("Body text"));
        assert!(!md.contains("Menu"), "nav should be dropped: {md}");
        assert!(!md.contains("Related"), "aside should be dropped: {md}");
        assert!(
            !md.contains("Pick"),
            "form controls should be dropped: {md}"
        );
    }

    #[test]
    fn keeps_article_header_and_footer_content() {
        // <header> usually holds the title/byline and <footer> can hold
        // article attribution, so neither is treated as chrome.
        let html = "<article><header><h1>Real Title</h1><p>By Author</p></header>\
                    <p>Body</p><footer>Published 2026</footer></article>";
        let md = html_to_markdown(html);
        for needle in ["Real Title", "By Author", "Body", "Published 2026"] {
            assert!(md.contains(needle), "{needle} missing from {md}");
        }
    }

    #[test]
    fn drops_empty_links_and_overlong_targets() {
        assert_eq!(render_link("https://example.com", ""), "");
        assert_eq!(render_link("#section", "Jump"), "Jump");
        let long = format!("https://example.com/?code={}", "a".repeat(MAX_URL_CHARS));
        assert_eq!(render_link(&long, "Run"), "Run");
        assert_eq!(
            render_link("https://example.com", "Home"),
            "[Home](https://example.com)"
        );
    }

    #[test]
    fn strips_html_comments() {
        let md = html_to_markdown("<p>Keep</p><!-- build:12345 drop me -->");
        assert!(md.contains("Keep"));
        assert!(!md.contains("drop me"), "comment retained: {md}");
    }

    #[test]
    fn does_not_leak_attributes_containing_angle_brackets() {
        // Parsoid-style tags embed JSON in attributes; a naive `<[^>]+>` regex
        // stops at the first `>` inside the value and dumps the rest as text.
        let html = r#"<span data-mw='{"wt":"[[a]] > [[b]]"}'>Visible</span>"#;
        let text = html_to_text(html);
        assert_eq!(text, "Visible");
    }

    #[test]
    fn caps_output_length() {
        let long = "line of text\n".repeat(MAX_OUTPUT_CHARS);
        let (out, truncated) = truncate_output(long);
        assert!(truncated);
        assert!(out.len() <= MAX_OUTPUT_CHARS);
    }

    #[test]
    fn keeps_short_output_intact() {
        let (out, truncated) = truncate_output("hello".to_string());
        assert!(!truncated);
        assert_eq!(out, "hello");
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        // Multi-byte chars straddling the cut must not panic or corrupt output.
        let long = "é".repeat(MAX_OUTPUT_CHARS);
        let (out, truncated) = truncate_output(long);
        assert!(truncated);
        assert!(out.chars().all(|c| c == 'é'));
    }
}
