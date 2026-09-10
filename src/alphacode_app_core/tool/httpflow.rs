use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use url::Url;

const MAX_RESPONSE_SIZE: usize = 5 * 1024 * 1024; // 5MB
const MAX_REDIRECTS: usize = 10;

/// Shared cookie jar across requests in a session.
type CookieJar = Arc<RwLock<HashMap<String, String>>>;

pub struct HttpFlowTool {
    client: reqwest::Client,
    sessions: Arc<RwLock<HashMap<String, CookieJar>>>,
}

impl HttpFlowTool {
    pub fn new() -> Self {
        Self {
            client: crate::provider::shared_http_client(),
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[derive(Deserialize)]
struct HttpFlowInput {
    action: String,
    #[serde(default)]
    session: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    headers: Option<HashMap<String, String>>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    follow_redirects: Option<bool>,
    #[serde(default)]
    extract_csrf: Option<bool>,
    #[serde(default)]
    csrf_selector: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct HttpResponse {
    status: u16,
    headers: HashMap<String, String>,
    body: String,
    url: String,
    redirect_chain: Vec<String>,
    cookies: Vec<String>,
}

#[async_trait]
impl Tool for HttpFlowTool {
    fn name(&self) -> &str {
        "httpflow"
    }

    fn description(&self) -> &str {
        "Stateful HTTP flow tool with cookie-jar sessions, CSRF token extraction, and correct \
         redirect handling. Maintains session state across requests. Handles POST→303→GET \
         redirect chains correctly (unlike curl -L which re-POSTs). Use for multi-step web \
         workflows: login, form submission, API chaining, authenticated testing."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "intent": super::intent_schema_property(),
                "action": {
                    "type": "string",
                    "enum": ["request", "get_csrf", "clear_session", "show_cookies"],
                    "description": "Action: request=send HTTP request with session, get_csrf=extract CSRF token from page, clear_session=delete session cookies, show_cookies=view current cookies."
                },
                "session": {
                    "type": "string",
                    "description": "Session name for cookie persistence (default: 'default')."
                },
                "url": {
                    "type": "string",
                    "description": "Target URL."
                },
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"],
                    "description": "HTTP method (default: GET)."
                },
                "headers": {
                    "type": "object",
                    "description": "Custom HTTP headers.",
                    "additionalProperties": { "type": "string" }
                },
                "body": {
                    "type": "string",
                    "description": "Request body (for POST/PUT/PATCH)."
                },
                "follow_redirects": {
                    "type": "boolean",
                    "description": "Follow 3xx redirects with correct method switching (default: true). POST→303/301/302 becomes GET."
                },
                "extract_csrf": {
                    "type": "boolean",
                    "description": "Extract CSRF token from response HTML and include in output."
                },
                "csrf_selector": {
                    "type": "string",
                    "description": "CSS selector for CSRF token input (default: 'input[name=csrf], input[name=_token], input[name=csrf_token], input[name=_csrf]')."
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params: HttpFlowInput = serde_json::from_value(input)?;
        let session_name = params.session.as_deref().unwrap_or("default");

        match params.action.as_str() {
            "request" => self.send_request(&params, session_name, &ctx).await,
            "get_csrf" => self.get_csrf_token(&params, session_name).await,
            "clear_session" => self.clear_session(session_name).await,
            "show_cookies" => self.show_cookies(session_name).await,
            _ => Err(anyhow::anyhow!(
                "Unknown action: {}. Use request, get_csrf, clear_session, or show_cookies.",
                params.action
            )),
        }
    }
}

impl HttpFlowTool {
    async fn get_session_cookies(&self, session_name: &str) -> CookieJar {
        let mut sessions = self.sessions.write().await;
        sessions
            .entry(session_name.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(HashMap::new())))
            .clone()
    }

    fn parse_cookies_from_headers(headers: &reqwest::header::HeaderMap) -> HashMap<String, String> {
        let mut cookies = HashMap::new();
        for value in headers.get_all(reqwest::header::SET_COOKIE).iter() {
            if let Ok(cookie_str) = value.to_str() {
                // Parse "name=value; ..." format
                if let Some(pair) = cookie_str.split(';').next() {
                    if let Some((name, value)) = pair.split_once('=') {
                        cookies.insert(name.trim().to_string(), value.trim().to_string());
                    }
                }
            }
        }
        cookies
    }

    fn build_cookie_header(cookies: &HashMap<String, String>) -> String {
        cookies
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("; ")
    }

    async fn send_request(
        &self,
        params: &HttpFlowInput,
        session_name: &str,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        let url = params
            .url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("URL is required"))?;

        let method = params.method.as_deref().unwrap_or("GET").to_uppercase();
        let follow = params.follow_redirects.unwrap_or(true);
        let cookies = self.get_session_cookies(session_name).await;

        let mut redirect_chain = Vec::new();
        let mut current_url = url.to_string();
        let mut current_method = method.clone();
        let mut current_body = params.body.clone();
        let mut all_cookies: Vec<String> = Vec::new();

        for _ in 0..MAX_REDIRECTS {
            // Build request
            let _parsed_url = Url::parse(&current_url)
                .context(format!("Invalid URL: {}", current_url))?;

            let mut request_builder = match current_method.as_str() {
                "GET" => self.client.get(&current_url),
                "POST" => self.client.post(&current_url),
                "PUT" => self.client.put(&current_url),
                "DELETE" => self.client.delete(&current_url),
                "PATCH" => self.client.patch(&current_url),
                "HEAD" => self.client.head(&current_url),
                "OPTIONS" => self.client.request(reqwest::Method::OPTIONS, &current_url),
                _ => self.client.get(&current_url),
            };

            // Add cookies from jar
            let cookie_guard = cookies.read().await;
            if !cookie_guard.is_empty() {
                let cookie_header = Self::build_cookie_header(&cookie_guard);
                request_builder = request_builder.header("cookie", &cookie_header);
            }
            drop(cookie_guard);

            // Add custom headers
            if let Some(headers) = &params.headers {
                for (key, value) in headers {
                    request_builder = request_builder.header(key.as_str(), value.as_str());
                }
            }

            // Add body for POST/PUT/PATCH
            if let Some(ref body) = current_body {
                if !body.is_empty() {
                    // Check if body looks like JSON
                    if body.trim_start().starts_with('{') || body.trim_start().starts_with('[') {
                        request_builder = request_builder
                            .header("content-type", "application/json")
                            .body(body.clone());
                    } else {
                        request_builder = request_builder
                            .header("content-type", "application/x-www-form-urlencoded")
                            .body(body.clone());
                    }
                }
            }

            // Set referer
            if redirect_chain.last().is_some() {
                request_builder = request_builder.header("referer", &current_url);
            }

            let response = request_builder
                .send()
                .await
                .context(format!("Request to {} failed", current_url))?;

            let status = status_from_response(&response);
            let response_url = response.url().to_string();
            let response_headers: HashMap<String, String> = response
                .headers()
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();

            // Store cookies from response
            let new_cookies = Self::parse_cookies_from_headers(response.headers());
            {
                let mut jar = cookies.write().await;
                for (k, v) in &new_cookies {
                    jar.insert(k.clone(), v.clone());
                    all_cookies.push(format!("{}={}", k, v));
                }
            }

            // Read response body
            let body_bytes = response
                .bytes()
                .await
                .context("Failed to read response body")?;

            if body_bytes.len() > MAX_RESPONSE_SIZE {
                return Err(anyhow::anyhow!(
                    "Response too large: {} bytes",
                    body_bytes.len()
                ));
            }

            let body = String::from_utf8_lossy(&body_bytes).into_owned();

            // Handle redirects
            if follow && is_redirect(status) {
                let location = response_headers
                    .get("location")
                    .or_else(|| response_headers.get("Location"));

                if let Some(location) = location {
                    // Resolve relative URL
                    let base = Url::parse(&current_url)?;
                    let redirect_url = base
                        .join(location)
                        .context(format!("Invalid redirect URL: {}", location))?
                        .to_string();

                    redirect_chain.push(current_url.clone());

                    // POST → 303 always becomes GET
                    // POST → 301/302 becomes GET (browser convention)
                    // Other redirects preserve method
                    current_method = if matches!(status, 301 | 302 | 303)
                        && current_method == "POST"
                    {
                        current_body = None; // GET has no body
                        "GET".to_string()
                    } else {
                        current_method
                    };

                    current_url = redirect_url;
                    continue;
                }
            }

            // Terminal response
            let csrf_token = if params.extract_csrf.unwrap_or(false) {
                self.extract_csrf_token(&body, params.csrf_selector.as_deref())
            } else {
                None
            };

            let result = HttpResponse {
                status,
                headers: response_headers.clone(),
                body: body.clone(),
                url: response_url.clone(),
                redirect_chain: redirect_chain.clone(),
                cookies: all_cookies.clone(),
            };

            let mut output = format!(
                "HTTP {} {}\nURL: {}\n",
                result.status, status_text(result.status), result.url
            );

            if !result.redirect_chain.is_empty() {
                output.push_str(&format!(
                    "Redirect chain: {}\n",
                    result.redirect_chain.join(" → ")
                ));
            }

            if !result.cookies.is_empty() {
                output.push_str(&format!("Cookies set: {}\n", result.cookies.join("; ")));
            }

            if let Some(token) = csrf_token {
                output.push_str(&format!("\nCSRF token: {}\n", token));
            }

            output.push_str(&format!("\n{}", truncate_body(&body, 30000)));

            return Ok(ToolOutput::new(output));
        }

        Err(anyhow::anyhow!(
            "Too many redirects ({}). Last URL: {}",
            MAX_REDIRECTS,
            current_url
        ))
    }

    async fn get_csrf_token(
        &self,
        params: &HttpFlowInput,
        session_name: &str,
    ) -> Result<ToolOutput> {
        let url = params
            .url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("URL is required"))?;

        // First fetch the page
        let cookies = self.get_session_cookies(session_name).await;
        let mut request_builder = self.client.get(url);

        let cookie_guard = cookies.read().await;
        if !cookie_guard.is_empty() {
            let cookie_header = Self::build_cookie_header(&cookie_guard);
            request_builder = request_builder.header("cookie", &cookie_header);
        }
        drop(cookie_guard);

        let response = request_builder
            .send()
            .await
            .context(format!("Failed to fetch {}", url))?;

        // Store cookies
        let new_cookies = Self::parse_cookies_from_headers(response.headers());
        {
            let mut jar = cookies.write().await;
            for (k, v) in &new_cookies {
                jar.insert(k.clone(), v.clone());
            }
        }

        let body = response
            .text()
            .await
            .context("Failed to read response body")?;

        // Extract CSRF token
        let selector = params
            .csrf_selector
            .as_deref()
            .unwrap_or("input[name=csrf], input[name=_token], input[name=csrf_token], input[name=_csrf]");

        match self.extract_csrf_token(&body, Some(selector)) {
            Some(token) => Ok(ToolOutput::new(format!(
                "CSRF token extracted: {}\n\nSelector: {}",
                token, selector
            ))),
            None => Ok(ToolOutput::new(format!(
                "No CSRF token found.\nSelector tried: {}\n\nTip: Inspect the page HTML and provide a custom csrf_selector.",
                selector
            ))),
        }
    }

    fn extract_csrf_token(&self, html: &str, selector: Option<&str>) -> Option<String> {
        let document = Html::parse_document(html);
        let selector_str = selector.unwrap_or(
            "input[name=csrf], input[name=_token], input[name=csrf_token], input[name=_csrf]",
        );

        if let Ok(sel) = Selector::parse(selector_str) {
            for el in document.select(&sel) {
                if let Some(value) = el.value().attr("value") {
                    return Some(value.to_string());
                }
            }
        }

        // Fallback: look for meta tags
        if let Ok(sel) = Selector::parse("meta[name=csrf-token], meta[name=_csrf]") {
            for el in document.select(&sel) {
                if let Some(content) = el.value().attr("content") {
                    return Some(content.to_string());
                }
            }
        }

        None
    }

    async fn clear_session(&self, session_name: &str) -> Result<ToolOutput> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_name);
        Ok(ToolOutput::new(format!(
            "Session '{}' cleared. Cookies deleted.",
            session_name
        )))
    }

    async fn show_cookies(&self, session_name: &str) -> Result<ToolOutput> {
        let sessions = self.sessions.read().await;
        match sessions.get(session_name) {
            Some(jar) => {
                let jar = jar.read().await;
                if jar.is_empty() {
                    Ok(ToolOutput::new(format!(
                        "Session '{}': no cookies stored.",
                        session_name
                    )))
                } else {
                    let mut output = format!("Session '{}': {} cookies\n\n", session_name, jar.len());
                    for (k, v) in jar.iter() {
                        output.push_str(&format!("  {}={}\n", k, v));
                    }
                    Ok(ToolOutput::new(output))
                }
            }
            None => Ok(ToolOutput::new(format!(
                "Session '{}': not found (no requests made yet).",
                session_name
            ))),
        }
    }
}

fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

fn status_from_response(response: &reqwest::Response) -> u16 {
    response.status().as_u16()
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "",
    }
}

fn truncate_body(body: &str, max_chars: usize) -> String {
    if body.len() <= max_chars {
        body.to_string()
    } else {
        format!(
            "{}...\n\n[truncated, {} bytes total]",
            &body[..max_chars],
            body.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_redirect() {
        assert!(is_redirect(301));
        assert!(is_redirect(302));
        assert!(is_redirect(303));
        assert!(is_redirect(307));
        assert!(is_redirect(308));
        assert!(!is_redirect(200));
        assert!(!is_redirect(404));
    }

    #[test]
    fn test_status_text() {
        assert_eq!(status_text(200), "OK");
        assert_eq!(status_text(301), "Moved Permanently");
        assert_eq!(status_text(404), "Not Found");
    }

    #[test]
    fn test_truncate_body() {
        assert_eq!(truncate_body("hello", 10), "hello");
        assert!(truncate_body("hello world", 5).contains("truncated"));
    }

    #[test]
    fn test_parse_cookies() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "set-cookie",
            "session=abc123; Path=/; HttpOnly".parse().unwrap(),
        );
        let cookies = HttpFlowTool::parse_cookies_from_headers(&headers);
        assert_eq!(cookies.get("session"), Some(&"abc123".to_string()));
    }

    #[test]
    fn test_build_cookie_header() {
        let mut cookies = HashMap::new();
        cookies.insert("a".to_string(), "1".to_string());
        cookies.insert("b".to_string(), "2".to_string());
        let header = HttpFlowTool::build_cookie_header(&cookies);
        assert!(header.contains("a=1"));
        assert!(header.contains("b=2"));
    }

    #[test]
    fn test_extract_csrf_from_input() {
        let tool = HttpFlowTool::new();
        let html = r#"<html><body>
            <form method="POST">
                <input type="hidden" name="_token" value="abc123xyz">
            </form>
        </body></html>"#;
        let token = tool.extract_csrf_token(html, None);
        assert_eq!(token.as_deref(), Some("abc123xyz"));
    }

    #[test]
    fn test_extract_csrf_from_meta() {
        let tool = HttpFlowTool::new();
        let html = r#"<html><head>
            <meta name="csrf-token" content="meta_token_456">
        </head><body></body></html>"#;
        let token = tool.extract_csrf_token(html, None);
        assert_eq!(token.as_deref(), Some("meta_token_456"));
    }
}
