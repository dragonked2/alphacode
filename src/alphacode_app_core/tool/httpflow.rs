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

async fn read_response_body(mut response: reqwest::Response) -> Result<String> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .context("Failed to read response body")?
    {
        if chunk.len() > MAX_RESPONSE_SIZE.saturating_sub(body.len()) {
            anyhow::bail!("Response exceeds the {} byte limit", MAX_RESPONSE_SIZE);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// Shared cookie jar across requests in a session.
type CookieJar = Arc<RwLock<HashMap<String, String>>>;
type CookieSessions = HashMap<(String, String, String), CookieJar>;

pub struct HttpFlowTool {
    client: reqwest::Client,
    sessions: Arc<RwLock<CookieSessions>>,
}

impl HttpFlowTool {
    pub fn new() -> Self {
        // Dedicated client for this tool. We deliberately do NOT use reqwest's
        // built-in cookie_store: it would share cookies across every named
        // session. Instead, each named session gets its own jar below and we
        // attach the `cookie` header manually so sessions stay isolated.
        let client = reqwest::Client::builder()
            .user_agent(crate::alphacode_provider_core::ALPHACODE_USER_AGENT)
            .connect_timeout(std::time::Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|error| {
                crate::logging::warn(&format!(
                    "httpflow client setup failed: {error}; retrying without optional settings"
                ));
                reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .expect("httpflow requires a client with automatic redirects disabled")
            });

        Self {
            client,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[derive(Deserialize)]
struct HttpFlowInput {
    // `action` is optional on purpose: models frequently call httpflow with
    // just `{"url": "..."}`. Defaulting to `request` lets those calls succeed
    // instead of failing with "missing field `action`".
    #[serde(default)]
    action: Option<String>,
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
    #[serde(
        default,
        deserialize_with = "super::serde_coerce::opt_u64_from_string_or_number"
    )]
    timeout: Option<u64>,
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
            "properties": {
                "intent": super::intent_schema_property(),
                "action": {
                    "type": "string",
                    "enum": ["request", "get_csrf", "clear_session", "show_cookies"],
                    "description": "Action: request=send HTTP request with session (default when url is given), get_csrf=extract CSRF token from page, clear_session=delete session cookies, show_cookies=view current cookies."
                },
                "session": {
                    "type": "string",
                    "description": "Session name for cookie persistence (default: 'default')."
                },
                "url": {
                    "type": "string",
                    "description": "Target URL. Required for request and get_csrf."
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
                },
                "timeout": {
                    "type": "integer",
                    "description": "Per-request timeout in seconds (default: 60, max: 120)."
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        // Be maximally tolerant of what providers send:
        // - stringified JSON (`"{\"action\": ...}"`, double-encoded)
        // - single-element array wrapping the object
        // - bare URL string
        // - null / empty (model called with no args)
        // All of these previously surfaced as
        // "arguments must be a JSON object, got ..." which models could not
        // recover from. Normalize first, then give a *specific* error.
        let input = normalize_httpflow_input(input);
        let params: HttpFlowInput = serde_json::from_value(input).map_err(|err| {
            anyhow::anyhow!(
                "httpflow arguments not understood ({err}). Send an object like \
                 {{\"action\": \"request\", \"url\": \"https://...\", \
                 \"method\": \"GET\"}}. `action` is one of \
                 request, get_csrf, clear_session, show_cookies (defaults to \
                 `request` when `url` is given)."
            )
        })?;
        // Default: a bare `{"url": "..."}` means `request`.
        let action = params
            .action
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if params.url.as_deref().is_some_and(|u| !u.trim().is_empty()) {
                    "request"
                } else {
                    ""
                }
            });
        if action.is_empty() {
            return Err(anyhow::anyhow!(
                "httpflow needs `action` (or `url`). Send an object like \
                 {{\"action\": \"request\", \"url\": \"https://...\", \"method\": \"GET\"}}. \
                 Valid actions: request, get_csrf, clear_session, show_cookies."
            ));
        }
        let session_name = params.session.as_deref().unwrap_or("default");

        match action {
            "request" => self.send_request(&params, session_name, &ctx).await,
            "get_csrf" => {
                self.get_csrf_token(&params, session_name, &ctx.session_id)
                    .await
            }
            "clear_session" => self.clear_session(session_name, &ctx.session_id).await,
            "show_cookies" => self.show_cookies(session_name, &ctx.session_id).await,
            _ => Err(anyhow::anyhow!(
                "Unknown action: {}. Use request, get_csrf, clear_session, or show_cookies.",
                action
            )),
        }
    }
}

fn normalize_httpflow_input(input: Value) -> Value {
    match &input {
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Value::Object(serde_json::Map::new());
            }
            // Bare URL sent as a raw string.
            if (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
                && !trimmed.contains(char::is_whitespace)
                && trimmed.len() < 4096
            {
                let mut map = serde_json::Map::new();
                map.insert("url".to_string(), Value::String(trimmed.to_string()));
                return Value::Object(map);
            }
            // Try up to 3 levels of stringified JSON (double-encoded).
            let mut current = trimmed.to_string();
            for _ in 0..3 {
                if current.is_empty() {
                    break;
                }
                match serde_json::from_str::<Value>(&current) {
                    Ok(Value::Object(_)) => {
                        return serde_json::from_str(&current)
                            .unwrap_or(Value::Object(serde_json::Map::new()));
                    }
                    Ok(Value::String(inner)) => {
                        current = inner;
                        continue;
                    }
                    Ok(Value::Array(arr)) if arr.len() == 1 => {
                        return normalize_httpflow_input(arr.into_iter().next().expect("len 1"));
                    }
                    Ok(_) => break,
                    Err(_) => break,
                }
            }
            Value::Object(serde_json::Map::new())
        }
        Value::Array(arr) if arr.len() == 1 => {
            normalize_httpflow_input(arr.first().cloned().unwrap_or(Value::Null))
        }
        Value::Object(_) => input,
        // Null, number, bool, empty array: let serde report the specific
        // missing field instead of a generic type error.
        _ => Value::Object(serde_json::Map::new()),
    }
}

impl HttpFlowTool {
    async fn get_session_cookies(
        &self,
        owner: &str,
        session_name: &str,
        url: &str,
    ) -> Result<CookieJar> {
        let parsed = Url::parse(url)?;
        anyhow::ensure!(
            matches!(parsed.scheme(), "http" | "https")
                && parsed.host_str().is_some()
                && parsed.username().is_empty()
                && parsed.password().is_none(),
            "httpflow requires an HTTP(S) URL without embedded credentials"
        );
        let origin = parsed.origin().ascii_serialization();
        let mut sessions = self.sessions.write().await;
        Ok(sessions
            .entry((owner.to_string(), session_name.to_string(), origin))
            .or_insert_with(|| Arc::new(RwLock::new(HashMap::new())))
            .clone())
    }

    fn parse_cookies_from_headers(headers: &reqwest::header::HeaderMap) -> HashMap<String, String> {
        let mut cookies = HashMap::new();
        for value in headers.get_all(reqwest::header::SET_COOKIE).iter() {
            if let Ok(cookie_str) = value.to_str() {
                // Parse "name=value; ..." format
                if let Some(pair) = cookie_str.split(';').next()
                    && let Some((name, value)) = pair.split_once('=')
                {
                    cookies.insert(name.trim().to_string(), value.trim().to_string());
                }
            }
        }
        cookies
    }

    async fn send_request(
        &self,
        params: &HttpFlowInput,
        session_name: &str,
        ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        let url = params
            .url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("URL is required"))?;

        let method = params.method.as_deref().unwrap_or("GET").to_uppercase();
        let follow = params.follow_redirects.unwrap_or(true);
        let initial_origin = Url::parse(url)?.origin();

        let mut redirect_chain = Vec::new();
        let mut current_url = url.to_string();
        let mut current_method = method.clone();
        let mut current_body = params.body.clone();
        let mut all_cookies: Vec<String> = Vec::new();

        for _ in 0..MAX_REDIRECTS {
            // Build request
            let parsed_url =
                Url::parse(&current_url).context(format!("Invalid URL: {}", current_url))?;
            let cookies = self
                .get_session_cookies(&ctx.session_id, session_name, &current_url)
                .await?;

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

            // Per-request timeout (default 60s, max 120s) so a slow server
            // cannot block the agent turn indefinitely.
            //
            // `0` is treated as "unset" rather than passed through. `Duration::from_secs(0)`
            // fails every request instantly, so a caller writing
            // `{"timeout": 0}` to mean "no limit" got a wall of transport
            // errors instead of a working request.
            let timeout_secs = match params.timeout {
                Some(0) | None => 60,
                Some(n) => n.min(120),
            };
            request_builder = request_builder.timeout(std::time::Duration::from_secs(timeout_secs));

            // Attach cookies from the jar to this request.
            {
                let jar = cookies.read().await;
                if !jar.is_empty() {
                    let cookie_header: Vec<String> =
                        jar.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                    request_builder = request_builder.header("cookie", cookie_header.join("; "));
                }
            }

            // Add custom headers
            if let Some(headers) = &params.headers
                && parsed_url.origin() == initial_origin
            {
                // If Host header is overridden, we need to ensure cookies still
                // transmit correctly. The reqwest client may not attach cookies
                // when the Host header differs from the URL's host.
                let host_overridden = headers.contains_key("host") || headers.contains_key("Host");
                for (key, value) in headers {
                    request_builder = request_builder.header(key.as_str(), value.as_str());
                }
                // When Host is overridden, explicitly pin the Cookie header from
                // the jar to ensure it's sent (reqwest may skip cookie injection
                // when Host doesn't match the URL's original host).
                if host_overridden {
                    let jar = cookies.read().await;
                    if !jar.is_empty() {
                        let cookie_header: Vec<String> =
                            jar.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                        request_builder =
                            request_builder.header("cookie", cookie_header.join("; "));
                    }
                }
            }

            // Add body for POST/PUT/PATCH
            if let Some(ref body) = current_body
                && !body.is_empty()
            {
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

            // Set referer
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
            let body = read_response_body(response).await?;

            // Detect bot protection (Datadome, Cloudflare, etc.)
            if Self::detect_bot_protection(status, &body, &response_headers) {
                return Err(anyhow::anyhow!(
                    "Bot protection detected (status {}). The target is using Datadome/Cloudflare bot protection. \
                     Use the browser tool instead of httpflow for this target. \
                     The browser tool can handle JavaScript challenges and CAPTCHAs.",
                    status
                ));
            }

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

                    // A redirect is the cheapest way for an allowed request to
                    // leave the engagement, so every hop is re-checked against
                    // the session scope before it is followed. This loop had no
                    // validation at all before: not scope, and not even the
                    // SSRF guard `webfetch` applies.
                    if let Err(reason) = crate::alphacode_app_core::tool::scope::check_redirect(
                        ctx.session_id.as_str(),
                        "httpflow",
                        url,
                        &redirect_url,
                    ) {
                        return Err(anyhow::anyhow!("{reason}"));
                    }

                    // POST → 303 always becomes GET
                    // POST → 301/302 becomes GET (browser convention)
                    // Other redirects preserve method
                    current_method = if matches!(status, 301..=303) && current_method == "POST" {
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
                result.status,
                status_text(result.status),
                result.url
            );

            if !result.redirect_chain.is_empty() {
                output.push_str(&format!(
                    "Redirect chain: {}\n",
                    result.redirect_chain.join(" → ")
                ));
            }

            // Show key response headers (Set-Cookie, Location, Content-Type)
            let important_headers = ["set-cookie", "location", "content-type", "x-frame-options"];
            for h in &important_headers {
                if let Some(val) = result.headers.get(*h) {
                    output.push_str(&format!("{}: {}\n", h, val));
                }
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
        owner: &str,
    ) -> Result<ToolOutput> {
        let url = params
            .url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("URL is required"))?;

        // First fetch the page
        let cookies = self.get_session_cookies(owner, session_name, url).await?;
        let mut request_builder = self.client.get(url);
        let timeout_secs = match params.timeout {
            Some(0) | None => 60,
            Some(n) => n.min(120),
        };
        request_builder = request_builder.timeout(std::time::Duration::from_secs(timeout_secs));
        if let Some(headers) = &params.headers {
            for (key, value) in headers {
                request_builder = request_builder.header(key.as_str(), value.as_str());
            }
        }

        // Attach cookies from jar
        {
            let jar = cookies.read().await;
            if !jar.is_empty() {
                let cookie_header: Vec<String> =
                    jar.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                request_builder = request_builder.header("cookie", cookie_header.join("; "));
            }
        }

        let response = request_builder
            .send()
            .await
            .context(format!("Failed to fetch {}", url))?;
        if response.status().is_redirection() {
            return Err(anyhow::anyhow!(
                "CSRF page redirected; use the request action to follow only permitted redirects before extracting a token"
            ));
        }

        // Store cookies
        let new_cookies = Self::parse_cookies_from_headers(response.headers());
        {
            let mut jar = cookies.write().await;
            for (k, v) in &new_cookies {
                jar.insert(k.clone(), v.clone());
            }
        }

        let body = read_response_body(response).await?;

        // Extract CSRF token
        let selector = params.csrf_selector.as_deref().unwrap_or(
            "input[name=csrf], input[name=_token], input[name=csrf_token], input[name=_csrf]",
        );

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

    async fn clear_session(&self, session_name: &str, owner: &str) -> Result<ToolOutput> {
        let mut sessions = self.sessions.write().await;
        sessions
            .retain(|(session_owner, name, _), _| session_owner != owner || name != session_name);
        Ok(ToolOutput::new(format!(
            "Session '{}' cleared. Cookies deleted.",
            session_name
        )))
    }

    async fn show_cookies(&self, session_name: &str, owner: &str) -> Result<ToolOutput> {
        let sessions = self.sessions.read().await;
        let matching: Vec<_> = sessions
            .iter()
            .filter(|((session_owner, name, _), _)| session_owner == owner && name == session_name)
            .map(|((_, _, origin), jar)| (origin.clone(), Arc::clone(jar)))
            .collect();
        drop(sessions);
        if matching.is_empty() {
            return Ok(ToolOutput::new(format!(
                "Session '{}': not found (no requests made yet).",
                session_name
            )));
        }
        let mut output = format!("Session '{}': cookies by origin\n", session_name);
        for (origin, jar) in matching {
            let jar = jar.read().await;
            output.push_str(&format!("\n{}: {} cookies\n", origin, jar.len()));
            for (k, v) in jar.iter() {
                output.push_str(&format!("  {}={}\n", k, v));
            }
        }
        Ok(ToolOutput::new(output))
    }

    /// Detect bot protection (Datadome, Cloudflare, etc.)
    ///
    /// Returns true if the response indicates bot protection is in place.
    fn detect_bot_protection(status: u16, body: &str, headers: &HashMap<String, String>) -> bool {
        // Check for common bot protection status codes
        if matches!(status, 403 | 429 | 503) {
            // Check for Datadome
            if body.contains("datadome") || body.contains("DataDome") {
                return true;
            }
            // Check for Cloudflare
            if body.contains("cloudflare") || body.contains("Cloudflare") {
                return true;
            }
            // Check for PerimeterX
            if body.contains("perimeterx") || body.contains("PerimeterX") {
                return true;
            }
            // Check for Akamai
            if body.contains("akamai") || body.contains("Akamai") {
                return true;
            }
            // Check for generic CAPTCHA
            if body.contains("captcha") || body.contains("CAPTCHA") {
                return true;
            }
            // Check for "Access Denied" or "Blocked"
            if body.contains("Access Denied") || body.contains("access denied") {
                return true;
            }
            if body.contains("blocked") || body.contains("Blocked") {
                return true;
            }
        }

        // Check headers for bot protection indicators
        if let Some(server) = headers.get("server")
            && (server.contains("cloudflare") || server.contains("Cloudflare"))
        {
            return true;
        }

        if let Some(cf_ray) = headers.get("cf-ray")
            && !cf_ray.is_empty()
        {
            return true;
        }

        false
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
    if body.chars().count() <= max_chars {
        body.to_string()
    } else {
        let truncated: String = body.chars().take(max_chars).collect();
        format!(
            "{}...\n\n[truncated, {} chars total]",
            truncated,
            body.chars().count()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cookie_jars_are_isolated_by_agent_session_and_origin() {
        let tool = HttpFlowTool::new();
        let first = tool
            .get_session_cookies("agent-a", "default", "https://one.example/path")
            .await
            .unwrap();
        first
            .write()
            .await
            .insert("session".into(), "owned-a".into());
        let same = tool
            .get_session_cookies("agent-a", "default", "https://one.example/other")
            .await
            .unwrap();
        let other_agent = tool
            .get_session_cookies("agent-b", "default", "https://one.example/path")
            .await
            .unwrap();
        let other_origin = tool
            .get_session_cookies("agent-a", "default", "https://two.example/path")
            .await
            .unwrap();
        assert_eq!(
            same.read().await.get("session").map(String::as_str),
            Some("owned-a")
        );
        assert!(other_agent.read().await.is_empty());
        assert!(other_origin.read().await.is_empty());
        tool.clear_session("default", "agent-a").await.unwrap();
        assert!(
            tool.get_session_cookies("agent-a", "default", "https://one.example")
                .await
                .unwrap()
                .read()
                .await
                .is_empty()
        );
    }

    #[tokio::test]
    async fn oversized_response_is_rejected_while_reading() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/large", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 2048];
            let _n = socket.read(&mut request).await.unwrap();
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_RESPONSE_SIZE + 1
            );
            socket.write_all(headers.as_bytes()).await.unwrap();
            let _ = socket.write_all(&vec![b'x'; MAX_RESPONSE_SIZE + 1]).await;
        });
        let tool = HttpFlowTool::new();
        let ctx = ToolContext {
            session_id: "httpflow-size-fixture".into(),
            message_id: "m".into(),
            tool_call_id: "c".into(),
            working_dir: None,
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: super::super::ToolExecutionMode::Direct,
        };
        let error = tool.execute(json!({"url": url}), ctx).await.unwrap_err();
        assert!(error.to_string().contains("exceeds"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn csrf_fetch_uses_the_supplied_authentication_header() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/form", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let n = socket.read(&mut request).await.unwrap();
            let body = "<input name=\"_token\" value=\"fixture-token\">";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&request[..n]).to_ascii_lowercase()
        });
        let tool = HttpFlowTool::new();
        let ctx = ToolContext {
            session_id: "csrf-header-fixture".into(),
            message_id: "m".into(),
            tool_call_id: "c".into(),
            working_dir: None,
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: super::super::ToolExecutionMode::Direct,
        };
        let output = tool.execute(json!({"action":"get_csrf", "url":url, "headers":{"Authorization":"Bearer fixture"}}), ctx).await.unwrap();
        assert!(output.output.contains("fixture-token"));
        assert!(
            server
                .await
                .unwrap()
                .contains("authorization: bearer fixture")
        );
    }

    #[tokio::test]
    async fn redirects_are_processed_by_the_tool_not_reqwest() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            for _ in 0..3 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut data = vec![0; 4096];
                let size = socket.read(&mut data).await.unwrap();
                let request = String::from_utf8_lossy(&data[..size]).into_owned();
                let end = request.starts_with("GET /end ");
                requests.push(request);
                let response = if end {
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                } else {
                    "HTTP/1.1 302 Found\r\nLocation: /end\r\nSet-Cookie: sid=owned; Path=/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                };
                socket.write_all(response.as_bytes()).await.unwrap();
            }
            requests
        });
        let tool = HttpFlowTool::new();
        let context = || ToolContext {
            session_id: "httpflow-redirect-fixture".into(),
            message_id: "m".into(),
            tool_call_id: "c".into(),
            working_dir: None,
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: super::super::ToolExecutionMode::Direct,
        };
        let no_follow = tool
            .execute(
                json!({"url": format!("{base}/start"), "follow_redirects": false}),
                context(),
            )
            .await
            .unwrap();
        assert!(no_follow.output.starts_with("HTTP 302"));
        let followed = tool
            .execute(
                json!({"url": format!("{base}/start"), "follow_redirects": true}),
                context(),
            )
            .await
            .unwrap();
        assert!(followed.output.starts_with("HTTP 200"));
        let requests = tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
        assert!(requests[0].starts_with("GET /start "));
        assert!(requests[1].starts_with("GET /start "));
        assert!(requests[2].starts_with("GET /end "));
        assert!(
            requests[2]
                .to_ascii_lowercase()
                .contains("cookie: sid=owned")
        );
    }

    #[tokio::test]
    async fn cross_origin_redirect_does_not_forward_session_credentials() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let destination = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let destination_url = format!("http://{}/end", destination.local_addr().unwrap());
        let source = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source_url = format!("http://{}/start", source.local_addr().unwrap());
        let source_task = tokio::spawn(async move {
            let (mut socket, _) = source.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let _n = socket.read(&mut bytes).await.unwrap();
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: {destination_url}\r\nSet-Cookie: sid=private; Path=/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });
        let destination_task = tokio::spawn(async move {
            let (mut socket, _) = destination.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let n = socket.read(&mut bytes).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();
            String::from_utf8_lossy(&bytes[..n]).to_ascii_lowercase()
        });
        let tool = HttpFlowTool::new();
        let ctx = ToolContext {
            session_id: "httpflow-cross-origin-fixture".into(),
            message_id: "m".into(),
            tool_call_id: "c".into(),
            working_dir: None,
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: super::super::ToolExecutionMode::Direct,
        };
        let output = tool.execute(json!({"url":source_url, "headers":{"Authorization":"Bearer controlled-test", "X-Api-Key":"controlled-key"}}), ctx).await.unwrap();
        assert!(output.output.starts_with("HTTP 200"));
        source_task.await.unwrap();
        let request = destination_task.await.unwrap();
        assert!(!request.contains("authorization:"));
        assert!(!request.contains("x-api-key:"));
        assert!(!request.contains("cookie:"));
    }

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
