use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use futures::StreamExt;
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
/// Redirect hops followed by hand, each one re-checked against the SSRF guard.
const MAX_REDIRECTS: usize = 5;

/// User-Agent strings tried in order when anti-bot challenges are detected.
/// The first is a generic bot UA (fast, low footprint); subsequent ones mimic
/// real browsers to bypass Cloudflare/bot-detection heuristics.
const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (compatible; AlphacodeBot)",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15",
];

/// Parse the IP literal spellings a resolver will accept, or `None` if the
/// host is a real name.
///
/// The SSRF guard used to `split('.')` and bail out on anything that was not
/// exactly four dotted decimal octets, which meant every alternative spelling
/// the OS happily connects to was classified "public":
///
/// * `127.1`            (short form)
/// * `0x7f.0.0.1`      (hex octets)
/// * `2130706433`       (whole 32-bit value)
/// * `0177.0.0.1`       (octal octets)
/// * `::ffff:127.0.0.1` (IPv4-mapped IPv6)
/// * `127.0.0.1.nip.io` (a name that resolves to loopback — caught by the
///   string checks, but only for known suffixes)
///
/// Returns the parsed address so the range checks below apply to the real
/// value rather than the textual form.
fn parse_ip_literal(host: &str) -> Option<std::net::IpAddr> {
    let h = host.trim();
    if h.is_empty() {
        return None;
    }
    // Bracketed IPv6, e.g. `[::1]`.
    let unbracketed = h
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(h);

    // Anything containing a letter that is not a hex digit (`example.com`)
    // is a name, not an IP literal. Hex IPv6 and hex octets still get through.
    if let Ok(addr) = unbracketed.parse::<std::net::IpAddr>() {
        return Some(addr);
    }
    parse_legacy_dotted(unbracketed)
}

/// Parse the pre-IPv6 numeric forms: `a`, `a.b`, `a.b.c`, `a.b.c.d`, each part
/// decimal / hex (`0x..`) / octal (leading `0`).
fn parse_legacy_dotted(s: &str) -> Option<std::net::IpAddr> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    let mut nums: Vec<u32> = Vec::with_capacity(parts.len());
    for p in &parts {
        nums.push(parse_octet(p)?);
    }
    // `inet_aton(3)` semantics: the trailing part absorbs all remaining bytes,
    // big-endian. So `127.1` is `127.0.0.1`, not a right-aligned fill. Getting
    // this wrong matters — a mis-parsed loopback literal would be classified
    // "public" and let an SSRF straight through.
    let octets = match nums.len() {
        // Whole 32-bit value: `2130706433` == 127.0.0.1
        1 => {
            // `parse_octet` already yields `u32`, and both of its parsers
            // (`u32::from_str_radix(..).ok()` and `parse::<u32>()`) return
            // `None` on overflow, which the caller propagates with `?`. So
            // every element of `nums` fits in 32 bits by construction and an
            // explicit range check here can never fire.
            nums[0].to_be_bytes()
        }
        2 => {
            let (a, b) = (nums[0], nums[1]);
            if a > 0xFF || b > 0x00FF_FFFF {
                return None;
            }
            [a as u8, (b >> 16) as u8, (b >> 8) as u8, b as u8]
        }
        3 => {
            let (a, b, c) = (nums[0], nums[1], nums[2]);
            if a > 0xFF || b > 0xFF || c > 0xFFFF {
                return None;
            }
            [a as u8, b as u8, (c >> 8) as u8, c as u8]
        }
        _ => {
            let (a, b, c, d) = (nums[0], nums[1], nums[2], nums[3]);
            if a > 0xFF || b > 0xFF || c > 0xFF || d > 0xFF {
                return None;
            }
            [a as u8, b as u8, c as u8, d as u8]
        }
    };
    Some(std::net::IpAddr::V4(std::net::Ipv4Addr::from(octets)))
}

fn parse_octet(p: &str) -> Option<u32> {
    if p.is_empty() {
        return None;
    }
    let lower = p.to_ascii_lowercase();
    if let Some(hex) = lower.strip_prefix("0x") {
        return u32::from_str_radix(hex, 16).ok();
    }
    if lower.len() > 1 && lower.starts_with('0') && lower.chars().all(|c| c.is_ascii_digit()) {
        return u32::from_str_radix(&lower[1..], 8).ok();
    }
    lower.parse::<u32>().ok()
}

/// Whether an address is loopback, private, link-local, or unspecified —
/// anything that must never be reachable from a user-supplied URL.
fn is_non_public_addr(addr: std::net::IpAddr) -> bool {
    match addr {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                // 169.254.0.0/16 is covered by is_link_local, but AWS/GCP
                // metadata lives at 169.254.169.254 specifically.
                || v4.octets()[0] == 169
                && v4.octets()[1] == 254
                // Carrier-grade NAT 100.64.0.0/10
                || (v4.octets()[0] == 100 && (64..=127).contains(&v4.octets()[1]))
                // 0.0.0.0/8 "this network"
                || v4.octets()[0] == 0
                // 240.0.0.0/4 reserved, incl. 255.255.255.255
                || v4.octets()[0] >= 240
                // Shared address space 100.64/10 handled above; benchmarking
                // 198.18.0.0/15
                || (v4.octets()[0] == 198 && (18..=19).contains(&v4.octets()[1]))
        }
        std::net::IpAddr::V6(v6) => {
            // Unwrap IPv4-mapped/compatible so ::ffff:127.0.0.1 is judged as
            // the IPv4 loopback it is.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_non_public_addr(std::net::IpAddr::V4(v4));
            }
            v6.is_loopback()
                || v6.is_unspecified()
                // Unique local fc00::/7
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // Link-local fe80::/10
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Check if a hostname resolves to a private/internal IP range.
/// Used for SSRF protection to prevent fetching internal network resources.
fn is_private_ip(host: &str) -> bool {
    match parse_ip_literal(host) {
        Some(addr) => is_non_public_addr(addr),
        // Not an IP literal: a real hostname. Whether it is dangerous depends
        // on what it resolves to, which the caller checks separately.
        None => false,
    }
}

/// Resolve a hostname to every address the system would connect to.
///
/// Returns `None` when resolution fails, so the caller can proceed rather
/// than treating a transient DNS error as a security verdict. Runs on
/// `tokio::spawn_blocking` because the resolver is blocking.
async fn resolve_all(host: &str) -> Option<Vec<std::net::IpAddr>> {
    let host = host.trim().to_string();
    if host.is_empty() {
        return None;
    }
    tokio::task::spawn_blocking(move || {
        use std::net::ToSocketAddrs;
        match (host.as_str(), 0u16).to_socket_addrs() {
            Ok(iter) => {
                let addrs: Vec<std::net::IpAddr> = iter.map(|a| a.ip()).collect();
                (!addrs.is_empty()).then_some(addrs)
            }
            Err(_) => None,
        }
    })
    .await
    .ok()
    .flatten()
}

/// Markers that identify a specific vendor's anti-bot interstitial.
///
/// These used to be substring tests run against a `to_ascii_lowercase()` copy
/// of the whole body — a full extra allocation and pass over up to 5 MB per
/// fetch, five times over. `(?i)` literals compile once and use the regex
/// crate's SIMD literal search, which short-circuits at the first hit and never
/// copies the haystack.
const CLOUDFLARE_MARKERS: &[&str] = &[
    "cf-browser-verification",
    "cf_chl_opt",
    "__cf_chl_",
    "cdn-cgi/challenge-platform",
    "checking your browser",
    "enable javascript and cookies",
    "cf-mitigated",
];

const PERIMETERX_MARKERS: &[&str] = &[
    "px-captcha",
    "please verify you are human",
    "perimeterx",
    "px-cdn",
];

/// How many bytes of a response are scanned for challenge markers.
///
/// A challenge page announces itself in its `<title>`, its first inline
/// script, and a marker comment — all within the first few KB. Sampling the
/// head instead of the whole body keeps the anti-bot check off the critical
/// path for the 5 MB pages where it can never fire anyway.
const ANTIBOT_SCAN_BYTES: usize = 16 * 1024;

fn marker_regexes(
    cell: &'static std::sync::OnceLock<Vec<regex::Regex>>,
    markers: &[&str],
) -> &'static [regex::Regex] {
    cell.get_or_init(|| {
        markers
            .iter()
            .filter_map(|marker| match regex::Regex::new(&format!("(?i){marker}")) {
                Ok(re) => Some(re),
                Err(err) => {
                    crate::logging::warn(&format!(
                        "webfetch: failed to compile anti-bot marker `{marker}`: {err}"
                    ));
                    None
                }
            })
            .collect()
    })
}

fn cloudflare_markers() -> &'static [regex::Regex] {
    static CELL: std::sync::OnceLock<Vec<regex::Regex>> = std::sync::OnceLock::new();
    marker_regexes(&CELL, CLOUDFLARE_MARKERS)
}

fn perimeterx_markers() -> &'static [regex::Regex] {
    static CELL: std::sync::OnceLock<Vec<regex::Regex>> = std::sync::OnceLock::new();
    marker_regexes(&CELL, PERIMETERX_MARKERS)
}

/// Normalize a model-supplied target into an absolute `http(s)` URL.
///
/// The old check was `starts_with("http://") || starts_with("https://")` on the
/// raw string, which rejected three spellings that are perfectly valid and
/// that models emit routinely:
/// * surrounding whitespace — `" https://x.com "` is not a URL the transport
///   can use, but the intent is unambiguous;
/// * an upper- or mixed-case scheme — `HTTP://` and `Https://` are the same
///   scheme per RFC 3986, and several providers echo the scheme back verbatim;
/// * a scheme-less host — `example.com/api` and `example.com:8443/health` are
///   what a model writes when it is transcribing a target it just read off a
///   page, and failing those wasted a turn on a trivially repairable call.
fn normalize_target_url(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow::anyhow!(
            "URL must not be empty. Send {{\"url\": \"https://example.com\"}}."
        ));
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Ok(trimmed.to_string());
    }
    if lower.starts_with("//") {
        return Ok(format!("https:{trimmed}"));
    }
    // Scheme-less authority, which is a host (and optional port), then a path.
    // Requiring a dot *or* a port with a purely numeric port is what keeps
    // `example.com:8443/x` working while `data:text/html,x` — whose colon is
    // followed by non-numeric text — is recognized as a bogus scheme below.
    let authority = trimmed.split(['/', '?', '#']).next().unwrap_or(trimmed);
    let is_host = match authority.rsplit_once(':') {
        Some((host, port)) => {
            host.contains('.') && !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())
        }
        None => authority.contains('.'),
    };
    if is_host {
        return Ok(format!("https://{trimmed}"));
    }

    // Not a host. If the text still opens with something scheme-shaped, say so
    // precisely rather than letting it fall through to the generic complaint.
    if let Some(idx) = trimmed.find(':') {
        let scheme = &lower[..idx];
        let looks_like_scheme = !scheme.is_empty()
            && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
        if looks_like_scheme {
            return Err(anyhow::anyhow!(
                "URL scheme `{scheme}:` is not supported. Use http:// or https://."
            ));
        }
    }
    Err(anyhow::anyhow!(
        "URL must start with http:// or https:// (got `{trimmed}`). \
         Send {{\"url\": \"https://example.com\"}}."
    ))
}

/// Refuse a target that points at loopback, RFC 1918 space, link-local, or a
/// cloud metadata endpoint — as a literal *or* as whatever the name resolves to.
async fn guard_target(raw_url: &str) -> Result<String> {
    let parsed = url::Url::parse(raw_url).map_err(|e| {
        anyhow::anyhow!(
            "could not parse URL `{raw_url}`: {e}. Send a full URL including the scheme."
        )
    })?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(anyhow::anyhow!(
            "Blocked: URL scheme `{scheme}://` is not allowed. Use http:// or https://."
        ));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("Blocked: URL `{raw_url}` has no host."))?
        .to_ascii_lowercase();

    // Block localhost variants and cloud metadata endpoints by name.
    if host == "localhost"
        || host == "0.0.0.0"
        || host == "::1"
        || host == "[::1]"
        || host.ends_with(".local")
        || host.ends_with(".internal")
    {
        return Err(anyhow::anyhow!(
            "Blocked: URL targets localhost/internal host ({host}). Refusing SSRF request."
        ));
    }
    if host == "169.254.169.254"
        || host == "100.100.100.200"
        || host == "fd00:ec2::254"
        || host == "metadata.google.internal"
        || host == "metadata.goog"
    {
        return Err(anyhow::anyhow!(
            "Blocked: URL targets cloud metadata endpoint ({host}). Refusing SSRF request."
        ));
    }
    if is_private_ip(&host) {
        return Err(anyhow::anyhow!(
            "Blocked: URL targets private network address ({host}). Refusing SSRF request."
        ));
    }

    // Resolve the name and check every address it maps to. The literal check
    // above cannot see `internal.corp.example` or `127.0.0.1.nip.io`, both of
    // which resolve to loopback and both of which the resolver connects to
    // without complaint. Checking the *string* only was the gap that made the
    // guard decorative.
    if let Some(addrs) = resolve_all(&host).await {
        for addr in &addrs {
            if is_non_public_addr(*addr) {
                return Err(anyhow::anyhow!(
                    "Blocked: `{host}` resolves to {addr}, an internal/private address. \
                     Refusing SSRF request."
                ));
            }
        }
    }
    Ok(raw_url.to_string())
}

/// Whether a status code carries a `Location` we should chase.
fn is_redirect_status(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::MOVED_PERMANENTLY
            | reqwest::StatusCode::FOUND
            | reqwest::StatusCode::SEE_OTHER
            | reqwest::StatusCode::TEMPORARY_REDIRECT
            | reqwest::StatusCode::PERMANENT_REDIRECT
    )
}

/// Whether a redirect forces the follow-up request to become a bodyless GET.
///
/// 303 always does. 301/302 do for `POST`, matching every browser and HTTP
/// library, so a form POST that redirects to a confirmation page does not get
/// replayed as a POST to the confirmation URL. 307/308 preserve the method by
/// specification and are left alone.
fn redirect_downgrades_to_get(status: reqwest::StatusCode, method: &str) -> bool {
    if status == reqwest::StatusCode::SEE_OTHER {
        return true;
    }
    if matches!(
        status,
        reqwest::StatusCode::MOVED_PERMANENTLY | reqwest::StatusCode::FOUND
    ) {
        return method == "POST";
    }
    false
}

/// Returns `true` when `body` looks like an anti-bot challenge page rather
/// than real content.
///
/// Two of the previous triggers were broad enough to misfire on real content,
/// which cost a wasted request per false positive on every article page:
/// * `"just a moment"` appears in ordinary prose ("just a moment, and the page
///   loaded"), so it is no longer a trigger on its own.
/// * `"ray id"` is a generic phrase and is now only honoured alongside a
///   Cloudflare marker, or a `cf-ray` / `cf-mitigated` response header.
///
/// A genuine denial/captcha page is still caught by the minimal-content
/// heuristic, which requires both a small body and very little visible text —
/// the profile of an interstitial and not of an article.
fn detect_anti_bot_page(body: &str) -> Option<&'static str> {
    let head = match body.get(..ANTIBOT_SCAN_BYTES) {
        Some(head) if body.len() > ANTIBOT_SCAN_BYTES => head,
        _ => body,
    };
    // A cut mid-UTF-8 sequence would make `is_match` bail on invalid UTF-8;
    // the markers are ASCII, so backing off to the last char boundary is safe.
    let head = match head.char_indices().last() {
        Some((idx, c)) if c.len_utf8() > 1 => &head[..idx],
        _ => head,
    };

    if cloudflare_markers()
        .iter()
        .chain(perimeterx_markers().iter())
        .any(|re| re.is_match(head))
    {
        return Some("bot-protection challenge");
    }

    // Generic "access denied" or captcha pages with minimal real content.
    let low = head.to_ascii_lowercase();
    if (low.contains("access denied") || low.contains("captcha"))
        && body.len() < 50_000
        && body.split_whitespace().count() < 200
    {
        return Some("generic captcha/denial page");
    }
    None
}

/// Header-only challenge signal, so an interstitial is recognised before the
/// body is downloaded rather than after.
fn detect_anti_bot_headers(headers: &reqwest::header::HeaderMap) -> Option<&'static str> {
    let server_is_cloudflare = headers
        .get(reqwest::header::SERVER)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().contains("cloudflare"));
    if headers.contains_key("cf-mitigated") {
        return Some("Cloudflare challenge");
    }
    if headers.contains_key("cf-chl-bypass")
        || (server_is_cloudflare
            && headers.contains_key("cf-ray")
            && headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.starts_with("text/html"))
            && headers
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.parse::<u64>().is_ok_and(|n| n < 50_000)))
    {
        return Some("Cloudflare challenge");
    }
    if headers.get("px-request-id").is_some() {
        return Some("PerimeterX challenge");
    }
    None
}

pub struct WebFetchTool {
    client: reqwest::Client,
}

/// Client for user-supplied fetches.
///
/// Redirects are deliberately **not** followed by the transport. The SSRF guard
/// in [`guard_target`] validates the *initial* URL only, and reqwest's default
/// policy (`limit(10)`) would carry a `302 Location:
/// http://169.254.169.254/latest/meta-data/` straight past it — turning every
/// guarded fetch into an open proxy to the cloud metadata service, and a
/// browser-as-a-service endpoint for anything listening on loopback. Redirects
/// are followed by hand in `execute`, re-validating every hop.
fn fetch_client() -> reqwest::Client {
    use std::sync::OnceLock;
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .user_agent(crate::alphacode_provider_core::ALPHACODE_USER_AGENT)
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(20))
                .tcp_keepalive(Some(Duration::from_secs(30)))
                .http2_keep_alive_interval(Some(Duration::from_secs(25)))
                .http2_keep_alive_timeout(Duration::from_secs(20))
                .http2_keep_alive_while_idle(true)
                .pool_idle_timeout(Duration::from_secs(120))
                .pool_max_idle_per_host(4)
                .build()
                .unwrap_or_else(|err| {
                    crate::logging::warn(&format!("webfetch: failed to build HTTP client: {err}"));
                    reqwest::Client::builder()
                        .redirect(reqwest::redirect::Policy::none())
                        .build()
                        .unwrap_or_else(|_| reqwest::Client::new())
                })
        })
        .clone()
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            client: fetch_client(),
        }
    }
}

/// Everything except the URL, which is recovered separately by
/// [`super::coerce_url_arg`] before this is built.
#[derive(Default)]
struct WebFetchInput {
    format: Option<String>,
    timeout: Option<u64>,
    method: Option<String>,
    headers: Option<std::collections::HashMap<String, String>>,
    body: Option<String>,
}

impl WebFetchInput {
    /// Read the optional fields out of the call payload without ever failing.
    ///
    /// This replaced `serde_json::from_value::<WebFetchInput>`, which rejected
    /// the entire call over one mistyped optional field — `"timeout": "30"`
    /// instead of `30`, or a header value sent as a number. Both are
    /// repairable typos, and the fetch they were blocking would have succeeded.
    /// The URL is still mandatory, and is checked separately.
    fn from_value(input: &Value) -> Self {
        let map = input.as_object();
        let get = |key: &str| map.and_then(|m| m.get(key));
        // Accept `"30"` as well as `30`: providers that coerce tool arguments
        // to strings are common enough that rejecting them was costing turns.
        let timeout = get("timeout").and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
        });
        Self {
            format: get("format")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from),
            timeout,
            method: get("method")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_ascii_uppercase),
            // Header values are stringified rather than dropped: a model that
            // sends `{"x-port": 8080}` means the same thing as
            // `{"x-port": "8080"}`, and silently discarding the header is a
            // worse failure than sending it.
            headers: get("headers").and_then(Value::as_object).map(|headers| {
                headers
                    .iter()
                    .map(|(key, value)| {
                        let value = match value {
                            Value::String(text) => text.clone(),
                            other => other.to_string(),
                        };
                        (key.clone(), value)
                    })
                    .collect()
            }),
            body: get("body").and_then(Value::as_str).map(String::from),
        }
    }
}

/// What one URL produced: either a page to render, or a hop to follow.
enum Attempt {
    Done(FetchedPage),
    Redirect {
        status: reqwest::StatusCode,
        location: String,
    },
}

struct FetchedPage {
    url: String,
    body: String,
    content_type: String,
    resp_headers: HashMap<String, String>,
    /// Bytes discarded because the body exceeded [`MAX_SIZE`].
    byte_truncated: bool,
    /// Whether a fallback browser User-Agent was needed to get here.
    fallback_ua: bool,
    /// How many redirects were followed to reach this page.
    hops: usize,
}

/// Methods the schema advertises, plus `HEAD` for callers that only want
/// response metadata.
const ALLOWED_METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD"];

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
                    "enum": ALLOWED_METHODS,
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
        // Recover the target before anything else. The previous strict
        // `from_value::<WebFetchInput>` reported a bare serde "missing field
        // `url`" for every shape a model actually emits — an alias key
        // (`uri`, `target`, `href`), a bare URL string where an object was
        // expected, or a payload truncated by the provider's streaming
        // decoder. Each of those is a typo, not a user error, and each one
        // burned a whole turn.
        let requested = super::coerce_url_arg(&input, "webfetch")?;
        let params = WebFetchInput::from_value(&input);
        let start_url = guard_target(&normalize_target_url(&requested)?).await?;

        // `0` means "unset", not "expire immediately": `Some(0).min(MAX)`
        // yields a zero Duration and every request fails at once.
        let timeout = match params.timeout {
            Some(0) | None => DEFAULT_TIMEOUT,
            Some(n) => n.min(MAX_TIMEOUT),
        };
        let format = params.format.as_deref().unwrap_or("markdown");
        let method = match params.method.as_deref().unwrap_or("GET") {
            m if ALLOWED_METHODS.contains(&m) => m.to_string(),
            other => {
                return Err(anyhow::anyhow!(
                    "unsupported HTTP method `{other}`. Use one of: {}.",
                    ALLOWED_METHODS.join(", ")
                ));
            }
        };
        let mut method = method;
        // A 301/302/303 downgrades the follow-up to a bodyless GET; tracked so
        // the redirect loop can apply it.
        let mut drop_body = false;

        let mut current_url = start_url.clone();
        // Counted here rather than inside `attempt`, which knows nothing of the hops
        // that preceded it. The count is reported to the model, so it has to be the
        // real number: it was hard-coded to 0, so every redirected fetch reported
        // "followed 0 redirect(s)" while having followed several.
        let mut hops_followed = 0usize;
        for hop in 0..=MAX_REDIRECTS {
            match self
                .attempt(
                    &current_url,
                    &params,
                    &method,
                    drop_body,
                    timeout,
                    hops_followed,
                )
                .await?
            {
                Attempt::Done(page) => {
                    return Ok(ToolOutput::new(render_page(&page, format, &start_url)));
                }
                Attempt::Redirect { status, location } => {
                    if hop == MAX_REDIRECTS {
                        return Err(anyhow::anyhow!(
                            "Too many redirects (>{MAX_REDIRECTS}) starting at {start_url}; the last \
                             hop pointed at {location}. Fetch the final URL directly."
                        ));
                    }
                    let next = url::Url::parse(&current_url)
                        .ok()
                        .and_then(|base| base.join(&location).ok())
                        .map(|u| u.to_string())
                        .unwrap_or(location.clone());
                    // Re-run the full guard on every hop. This is the check the
                    // transport's own redirect handling used to skip.
                    current_url = guard_target(&next).await?;
                    hops_followed += 1;
                    if redirect_downgrades_to_get(status, &method) {
                        method = "GET".to_string();
                        drop_body = true;
                    }
                }
            }
        }

        // Unreachable in practice: every iteration either returns or errors, and
        // the last iteration refuses to follow another hop. Spelled as an error
        // rather than `unreachable!()` so a mistake here cannot take the whole
        // session down over a fetch.
        Err(anyhow::anyhow!(
            "Too many redirects (>={MAX_REDIRECTS}) starting at {start_url}"
        ))
    }
}

/// One request against one URL, walking the User-Agent ladder until a response
/// is accepted or the ladder is exhausted.
impl WebFetchTool {
    #[allow(clippy::too_many_arguments)]
    async fn attempt(
        &self,
        url: &str,
        params: &WebFetchInput,
        method: &str,
        drop_body: bool,
        timeout: u64,
        hops: usize,
    ) -> Result<Attempt> {
        let mut last_err: Option<anyhow::Error> = None;

        for (attempt, &ua) in USER_AGENTS.iter().enumerate() {
            let custom_user_agent = params.headers.as_ref().and_then(|headers| {
                headers
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
                    .map(|(_, value)| value.clone())
            });
            let user_agent = custom_user_agent
                .unwrap_or_else(|| crate::alphacode_provider_core::with_alphacode_brand(ua));
            let request = self
                .build_request(url, params, method, drop_body, &user_agent, timeout)
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
            if is_redirect_status(status)
                && let Some(location) = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string)
            {
                return Ok(Attempt::Redirect { status, location });
            }

            if !status.is_success() {
                // Smart 404: don't waste UA retries on a missing page, and tell
                // the agent to discover the URL instead of guessing variants.
                if status == reqwest::StatusCode::NOT_FOUND {
                    return Err(anyhow::anyhow!(
                        "HTTP error: 404 Not Found for {url}. URL does not exist — do NOT retry with \
                         different User-Agents or guess similar deep URLs (e.g. /uniswap/ vs \
                         /uniswap-v3/). Use websearch to discover the correct URL, try the site \
                         index, or check trailing-slash variant once."
                    ));
                }
                last_err = Some(anyhow::anyhow!("HTTP error: {} for {url}", status));
                continue;
            }

            let headers = response.headers().clone();
            let content_type = headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();

            // Capture key headers before consuming the response.
            let resp_headers: HashMap<String, String> = headers
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

            // Check declared content length before spending the bandwidth.
            if let Some(len) = response.content_length()
                && len as usize > MAX_SIZE
            {
                return Err(anyhow::anyhow!(
                    "Response too large: {} bytes (max {} bytes)",
                    len,
                    MAX_SIZE
                ));
            }

            // An interstitial can be recognised from its headers alone, before
            // the body is transferred.
            let header_reason = detect_anti_bot_headers(&headers);
            let mut body_bytes = Vec::with_capacity(64 * 1024);
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
            let challenge = header_reason.or_else(|| detect_anti_bot_page(&body));
            if let Some(reason) = challenge
                && attempt + 1 < USER_AGENTS.len()
            {
                crate::logging::info(&format!(
                    "webfetch: {reason} detected for {url}, retrying with fallback User-Agent (attempt {}/{})",
                    attempt + 2,
                    USER_AGENTS.len(),
                ));
                last_err = Some(anyhow::anyhow!(
                    "anti-bot challenge ({reason}), retrying with different browser fingerprint"
                ));
                continue;
            }

            return Ok(Attempt::Done(FetchedPage {
                url: url.to_string(),
                body,
                content_type,
                resp_headers,
                byte_truncated: truncated,
                fallback_ua: attempt > 0,
                hops,
            }));
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("webfetch: all attempts failed")))
    }

    fn build_request(
        &self,
        url: &str,
        params: &WebFetchInput,
        method: &str,
        drop_body: bool,
        user_agent: &str,
        timeout: u64,
    ) -> Result<reqwest::Request> {
        let method_ = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|_| anyhow::anyhow!("unsupported HTTP method `{method}`"))?;
        let bodyless = drop_body || matches!(method_, reqwest::Method::GET | reqwest::Method::HEAD);
        let mut builder = self
            .client
            .request(method_, url)
            .header(reqwest::header::USER_AGENT, user_agent)
            .timeout(Duration::from_secs(timeout));

        // `send` would otherwise set `Content-Length: 0` on bodyless requests.
        if bodyless {
            builder = builder.header(reqwest::header::CONTENT_LENGTH, 0);
        }

        if let Some(headers) = &params.headers {
            for (key, value) in headers {
                if key.eq_ignore_ascii_case("user-agent") {
                    continue;
                }
                // A malformed header must not abort the fetch; the transport
                // rejects it and the request proceeds without it.
                if let (Ok(name), Ok(header_value)) = (
                    reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                    reqwest::header::HeaderValue::from_str(value),
                ) {
                    builder = builder.header(name, header_value);
                } else {
                    crate::logging::debug(&format!(
                        "webfetch: ignoring malformed header `{key}` for {url}"
                    ));
                }
            }
        }

        if !bodyless && let Some(body) = &params.body {
            // Auto-detect JSON if body starts with { or [
            let content_type =
                if body.trim_start().starts_with('{') || body.trim_start().starts_with('[') {
                    "application/json"
                } else {
                    "application/x-www-form-urlencoded"
                };
            // Allow user-specified Content-Type to override. Looked up
            // case-insensitively: HTTP header names are case-insensitive, and
            // models send `Content-Type` at least as often as `content-type`.
            let user_ct = params.headers.as_ref().and_then(|h| {
                h.iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                    .map(|(_, v)| v.as_str())
            });
            builder = builder
                .header(
                    reqwest::header::CONTENT_TYPE,
                    user_ct.unwrap_or(content_type),
                )
                .body(body.clone());
        }

        Ok(builder.build()?)
    }
}

/// Format a completed fetch for the model.
fn render_page(page: &FetchedPage, format: &str, start_url: &str) -> String {
    let output = match format {
        "html" => page.body.clone(),
        "text" => html_to_text(&page.body),
        // `markdown` and any unrecognised value both render HTML as markdown;
        // non-HTML payloads (JSON, CSV, source) pass through untouched.
        _ if page.content_type.contains("text/html") => html_to_markdown(&page.body),
        _ => page.body.clone(),
    };

    let full_len = output.len();
    let (output, output_truncated) = truncate_output(output);

    let mut header = String::with_capacity(160);
    header.push_str(&format!(
        "Fetched {} ({:.1}KB, {} lines)\n",
        page.url,
        full_len as f64 / 1024.0,
        output.lines().count(),
    ));
    if page.url != start_url {
        // Redirection is security-relevant, so say it happened rather than
        // silently reporting the final URL as if it were the requested one.
        header.push_str(&format!(
            "(followed {} redirect(s) from {start_url})\n",
            page.hops
        ));
    }
    if page.fallback_ua {
        header.push_str("(retrieved with fallback User-Agent after anti-bot challenge)\n");
    }
    if page.byte_truncated {
        header.push_str(&format!(
            "(body exceeded the {} byte limit and was cut short)\n",
            MAX_SIZE
        ));
    }
    for name in ["set-cookie", "location", "content-security-policy"] {
        if let Some(value) = page.resp_headers.get(name) {
            header.push_str(name);
            header.push_str(": ");
            header.push_str(value);
            header.push('\n');
        }
    }

    if output_truncated {
        let saved_k = (full_len - output.len()) / 4 / 1000;
        header.push('\n');
        header.push_str(&format!(
            "[Truncated: ~{saved_k}k tokens saved — fetch a more specific URL or anchor for the rest]"
        ));
    }

    format!("{header}\n\n{output}")
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

/// Apply `re` to `input`, consuming the buffer without a redundant copy.
///
/// Every stage of HTML conversion used to read
/// `text = re.replace_all(&text, "").to_string()`. `replace_all` already
/// returns an owned `String` whenever it changed anything, so the trailing
/// `.to_string()` cloned the entire buffer again — and the pipeline runs 15
/// (text) to 25 (markdown) regex passes over a body that reaches 5 MB. That
/// was ~100 MB of pointless allocation and memcpy per fetch.
///
/// The `is_match` pre-check is the second half of the fix: for a pattern that
/// does not occur (`<canvas>`, `<dialog>`, `&quot;` — the common case on any
/// given page) the buffer is handed straight back, so neither the match nor
/// the copy happens.
fn replace_owned(input: String, re: &regex::Regex, replacement: &str) -> String {
    if !re.is_match(&input) {
        return input;
    }
    match re.replace_all(&input, replacement) {
        std::borrow::Cow::Owned(out) => out,
        std::borrow::Cow::Borrowed(_) => unreachable!("is_match passed, so replace_all must own"),
    }
}

/// As [`replace_owned`], for replacements driven by a closure over captures.
fn replace_owned_with(
    input: String,
    re: &regex::Regex,
    replacement: impl FnMut(&regex::Captures<'_>) -> String,
) -> String {
    if !re.is_match(&input) {
        return input;
    }
    match re.replace_all(&input, replacement) {
        std::borrow::Cow::Owned(out) => out,
        std::borrow::Cow::Borrowed(_) => unreachable!("is_match passed, so replace_all must own"),
    }
}

/// `String::replace` for single-call-site patterns, skipping the allocation
/// when the needle is absent.
fn replace_str_owned(input: String, needle: &str, replacement: &str) -> String {
    if !input.contains(needle) {
        return input;
    }
    input.replace(needle, replacement)
}

/// Decode the five named entities plus numeric references. Unbounded `&amp;`
/// handling needs care: `&amp;lt;` decodes to `&lt;`, not `<`, so this runs
/// last among the literal replacements and cannot re-introduce markup.
fn decode_entities(mut text: String) -> String {
    text = replace_str_owned(text, "&nbsp;", " ");
    text = replace_str_owned(text, "&lt;", "<");
    text = replace_str_owned(text, "&gt;", ">");
    text = replace_str_owned(text, "&quot;", "\"");
    text = replace_str_owned(text, "&#39;", "'");
    // `&amp;` last: decoding it first would let `&amp;lt;` collapse into `<`.
    replace_str_owned(text, "&amp;", "&")
}

pub(crate) fn html_to_text(html: &str) -> String {
    let (Some(script), Some(style), Some(tag), Some(whitespace)) = (
        html_regex::script(),
        html_regex::style(),
        html_regex::tag(),
        html_regex::whitespace(),
    ) else {
        return html.trim().to_string();
    };

    // A payload with no markup at all needs none of the tag passes below.
    if !html.contains('<') {
        return collapse_whitespace(html, whitespace);
    }

    let mut text = html.to_string();
    text = replace_owned(text, script, "");
    text = replace_owned(text, style, "");
    if let Some(comment) = html_regex::comment() {
        text = replace_owned(text, comment, "");
    }
    for re in html_regex::chrome() {
        text = replace_owned(text, re, "");
    }

    text = replace_str_owned(text, "<br>", "\n");
    text = replace_str_owned(text, "<br/>", "\n");
    text = replace_str_owned(text, "<br />", "\n");
    text = replace_str_owned(text, "</p>", "\n\n");
    text = replace_str_owned(text, "</div>", "\n");
    text = replace_str_owned(text, "</li>", "\n");
    text = replace_str_owned(text, "</tr>", "\n");

    text = replace_owned(text, tag, "");

    text = decode_entities(text);

    collapse_whitespace(&text, whitespace).trim().to_string()
}

/// Collapse runs of blank lines to one, but only *outside* fenced code blocks.
///
/// The previous unconditional `replace_all` also rewrote blank lines inside
/// ``` fences, which is where blank lines carry meaning: a stack trace, a
/// diff, or a Python function's two blank lines all get silently reflowed, and
/// the model then reasons about code that is not what the file says. For a
/// security tool this is the worst kind of error — a wrong payload looks right.
fn collapse_whitespace(text: &str, whitespace: &regex::Regex) -> String {
    if !text.contains("```") {
        return apply_regex(text, whitespace, "\n\n");
    }
    let mut out = String::with_capacity(text.len());
    let mut in_fence = false;
    for line in text.split_inclusive('\n') {
        // A fence is a line that opens or closes with ``` (optionally
        // indented, optionally language-tagged). Toggling per line keeps
        // ``` inside a code sample from ending the block early only in the
        // exact case where it is itself a fence delimiter, which is the correct
        // reading of CommonMark for this purpose.
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            out.push_str(line);
            continue;
        }
        if in_fence {
            out.push_str(line);
        } else {
            out.push_str(&apply_regex(line, whitespace, "\n\n"));
        }
    }
    out
}

fn apply_regex(text: &str, re: &regex::Regex, replacement: &str) -> String {
    match re.replace_all(text, replacement) {
        std::borrow::Cow::Owned(out) => out,
        std::borrow::Cow::Borrowed(out) => out.to_string(),
    }
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

    // Plain-text payloads (JSON, source code, CSV) never contain markup, so
    // every tag pass below is provably a no-op. Skipping them also avoids
    // mangling code that happens to contain `<foo>` in a string literal.
    if !html.contains('<') {
        return decode_entities(html.to_string()).trim().to_string();
    }

    // Fenced code is extracted *before* the inline-element passes, because
    // `<strong>`, `<em>`, `<code>` and `<li>` are all replaced wholesale
    // downstream — a `**` inside a stack trace or a diff would otherwise be
    // rewritten as markdown emphasis, and an `_var_` in a Python payload would
    // lose characters. Fences are then re-inserted at the end, after the
    // entity decode, so their contents are never entity-expanded either.
    let (code_blocks, mut md) = extract_code_blocks(html);
    md = replace_owned(md, script, "");
    md = replace_owned(md, style, "");
    if let Some(comment) = html_regex::comment() {
        md = replace_owned(md, comment, "");
    }
    for re in html_regex::chrome() {
        md = replace_owned(md, re, "");
    }

    if let (Some(h_open), Some(h_close)) = (html_regex::h_open(), html_regex::h_close()) {
        for i in 0..6 {
            md = replace_owned(md, &h_open[i], &format!("\n{} ", "#".repeat(i + 1)));
            md = replace_owned(md, &h_close[i], "\n");
        }
    }

    md = replace_owned_with(md, link, |caps: &regex::Captures<'_>| {
        render_link(
            caps.get(1).map_or("", |m| m.as_str()),
            caps.get(2).map_or("", |m| m.as_str()),
        )
    });
    md = replace_owned(md, strong, "**$1**");
    md = replace_owned(md, em, "*$1*");
    // `<pre>` blocks that are already parked are dropped from the working
    // text; the remainder still needs the inline `<code>` treatment.
    md = replace_owned(md, code, "`$1`");
    md = replace_owned(md, pre_code, "\n```\n$1\n```\n");
    md = replace_owned(md, li, "\n- ");

    md = replace_str_owned(md, "<br>", "\n");
    md = replace_str_owned(md, "<br/>", "\n");
    md = replace_str_owned(md, "<br />", "\n");
    md = replace_str_owned(md, "</p>", "\n\n");

    md = replace_owned(md, tag, "");

    md = decode_entities(md);

    if let Some(empty_bullets) = html_regex::empty_bullets() {
        md = replace_owned(md, empty_bullets, "");
    }

    // Put the verbatim code back, and only then collapse blank lines: the
    // collapse now skips fence interiors, so a stack trace keeps its spacing.
    md = restore_code_blocks(&code_blocks, &md);
    collapse_whitespace(&md, whitespace).trim().to_string()
}

/// Placeholder that survives the inline-element passes untouched.
///
/// Chosen from characters that no tag-stripping or emphasis rule rewrites, and
/// that cannot appear in a URL or attribute value left in the working text.
const CODE_PLACEHOLDER: &str = "\u{0}alphacode-code-\u{0}";

/// Lift every `<pre>` block out of the document, leaving a placeholder.
///
/// Returns the extracted bodies alongside the working text so the caller can
/// re-insert them verbatim.
fn extract_code_blocks(html: &str) -> (Vec<String>, String) {
    let Some(pre) = html_regex::pre_code() else {
        return (Vec::new(), html.to_string());
    };
    let mut blocks = Vec::new();
    let stripped = pre.replace_all(html, |caps: &regex::Captures<'_>| {
        // Entity-decode the body once, here, so a code sample that shows
        // `&lt;div&gt;` is rendered as the markup it documents. The result is
        // never re-processed, so it cannot be re-expanded.
        let body = caps.get(1).map_or("", |m| m.as_str());
        blocks.push(decode_entities(body.to_string()));
        CODE_PLACEHOLDER.to_string()
    });
    (blocks, stripped.into_owned())
}

/// Re-insert the parked `<pre>` bodies as fenced blocks.
fn restore_code_blocks(blocks: &[String], md: &str) -> String {
    if blocks.is_empty() || !md.contains(CODE_PLACEHOLDER) {
        return md.to_string();
    }
    let mut out = md.to_string();
    for block in blocks {
        let Some(pos) = out.find(CODE_PLACEHOLDER) else {
            break;
        };
        let replacement = format!("\n```\n{block}\n```\n");
        out.replace_range(pos..pos + CODE_PLACEHOLDER.len(), &replacement);
    }
    out
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
    fn normalizes_accepted_url_spellings() {
        // Whitespace, a mixed-case scheme, and a scheme-less host are all
        // shapes models emit and all resolve to the same absolute URL.
        assert_eq!(
            normalize_target_url("  https://example.com/a  ").unwrap(),
            "https://example.com/a"
        );
        assert_eq!(
            normalize_target_url("HTTP://Example.com/a").unwrap(),
            "HTTP://Example.com/a"
        );
        assert_eq!(
            normalize_target_url("example.com/api").unwrap(),
            "https://example.com/api"
        );
        assert_eq!(
            normalize_target_url("example.com:8443/health").unwrap(),
            "https://example.com:8443/health"
        );
        assert_eq!(
            normalize_target_url("//example.com/x").unwrap(),
            "https://example.com/x"
        );
    }

    /// A redirected fetch must report both URLs and the *real* hop count.
    /// The count was hard-coded to 0 in `Attempt::Done`, so every redirected
    /// fetch told the model "followed 0 redirect(s)" — which reads as "this
    /// went straight there" and hides exactly the hop the model needs to see.
    #[test]
    fn render_page_reports_the_hop_count_it_was_given() {
        let page = |url: &str, hops: usize| FetchedPage {
            url: url.to_string(),
            body: "hello".to_string(),
            content_type: "text/plain".to_string(),
            resp_headers: HashMap::new(),
            byte_truncated: false,
            fallback_ua: false,
            hops,
        };
        let start = "https://example.com/";

        let direct = render_page(&page(start, 0), "text", start);
        assert!(
            !direct.contains("redirect"),
            "an unredirected fetch must not claim it followed redirects: {direct}"
        );

        let redirected = render_page(&page("https://www.example.org/", 2), "text", start);
        assert!(
            redirected.contains("followed 2 redirect(s) from https://example.com/"),
            "hop count not reported: {redirected}"
        );
        assert!(
            redirected.contains("https://www.example.org/"),
            "final URL not reported: {redirected}"
        );
    }

    #[test]
    fn rejects_non_http_schemes_and_bare_words() {
        for bad in [
            "file:///etc/passwd",
            "ftp://x.com",
            "gopher://x.com",
            "data:text/html,x",
        ] {
            let err = normalize_target_url(bad).unwrap_err().to_string();
            assert!(
                err.contains("not supported") || err.contains("must start with"),
                "unexpected error for {bad}: {err}"
            );
        }
        // A bare word with no dot or port is not a host, so it must not be
        // silently promoted to a web target.
        assert!(normalize_target_url("not a url").is_err());
        assert!(normalize_target_url("   ").is_err());
        // A colon followed by a non-numeric port is a malformed host, and must
        // not be read as a URL to send.
        assert!(normalize_target_url("example.com:notaport/x").is_err());
    }

    /// The classification above has to be right in both directions: a genuine
    /// host:port must survive, and a non-http scheme must not be mistaken for
    /// one. Both were wrong at some point during this fix.
    #[test]
    fn host_port_and_scheme_are_told_apart_correctly() {
        // Kept working: a dot-qualified host with a numeric port.
        for good in [
            "example.com:8443",
            "example.com:8443/health",
            "example.com:8443/a?b=c",
            "sub.example.co.uk:443/x",
        ] {
            assert_eq!(
                normalize_target_url(good).unwrap(),
                format!("https://{good}"),
                "{good} should have been accepted"
            );
        }
        // Rejected: schemes whose payload merely looks like `host:text`.
        for bad in [
            "javascript:alert(1)",
            "data:text/html,<script>x</script>",
            "vbscript:msgbox(1)",
        ] {
            assert!(
                normalize_target_url(bad).is_err(),
                "{bad} should have been rejected"
            );
        }
    }

    #[test]
    fn ssrf_guard_rejects_internal_targets() {
        // `guard_target` is async; these all fail before the DNS lookup, so a
        // single-threaded runtime is enough.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        for bad in [
            "http://localhost/admin",
            "http://127.0.0.1:8080/",
            "http://127.1/",
            "http://0x7f.0.0.1/",
            "http://[::1]/",
            "http://[::ffff:127.0.0.1]/",
            "http://10.0.0.5/",
            "http://192.168.1.1/",
            "http://172.16.0.1/",
            "http://169.254.169.254/latest/meta-data/",
            "http://metadata.google.internal/",
            "http://100.100.100.200/",
            "http://foo.internal/",
            "http://2130706433/",
        ] {
            let err = rt.block_on(guard_target(bad)).unwrap_err().to_string();
            assert!(
                err.starts_with("Blocked:"),
                "expected a Blocked verdict for {bad}, got: {err}"
            );
        }
    }

    #[test]
    fn redirect_status_and_downgrade_rules() {
        assert!(is_redirect_status(reqwest::StatusCode::FOUND));
        assert!(is_redirect_status(reqwest::StatusCode::PERMANENT_REDIRECT));
        assert!(!is_redirect_status(reqwest::StatusCode::OK));

        // 303 always downgrades; 301/302 only for POST; 307/308 never do.
        assert!(redirect_downgrades_to_get(
            reqwest::StatusCode::SEE_OTHER,
            "POST"
        ));
        assert!(redirect_downgrades_to_get(
            reqwest::StatusCode::SEE_OTHER,
            "GET"
        ));
        assert!(redirect_downgrades_to_get(
            reqwest::StatusCode::FOUND,
            "POST"
        ));
        assert!(!redirect_downgrades_to_get(
            reqwest::StatusCode::FOUND,
            "GET"
        ));
        assert!(!redirect_downgrades_to_get(
            reqwest::StatusCode::TEMPORARY_REDIRECT,
            "POST"
        ));
    }

    #[test]
    fn anti_bot_detection_ignores_ordinary_prose() {
        // The old marker list treated "just a moment" and "ray id" as proof of
        // a challenge, so any article mentioning either paid a wasted request
        // per fetch. They are ordinary English.
        for prose in [
            "<html><body><h1>Wait just a moment</h1><p>An honest article about latency.</p></body></html>",
            "<html><body><p>Find the ray id in your render settings.</p></body></html>",
        ] {
            assert_eq!(
                detect_anti_bot_page(prose),
                None,
                "false positive on ordinary prose: {prose}"
            );
        }
    }

    #[test]
    fn anti_bot_detection_catches_real_challenges() {
        for challenge in [
            "<html><head><title>Just a moment...</title><script src='/cdn-cgi/challenge-platform/h/b/orchestrate'></script></head></html>",
            "<html><body><div id='cf-browser-verification'>Verifying</div></body></html>",
            "<html><body>Please verify you are human</body></html>",
        ] {
            assert!(
                detect_anti_bot_page(challenge).is_some(),
                "missed a real challenge page: {challenge}"
            );
        }
        // A small denial page still trips the minimal-content heuristic.
        assert!(detect_anti_bot_page("<html><body>Access denied</body></html>").is_some());
    }

    #[test]
    fn anti_bot_scan_handles_multibyte_and_large_bodies() {
        // The scan window can land mid-codepoint; that must not panic and must
        // not stop a marker placed after the window from being ignored.
        let mut body = "é".repeat(ANTIBOT_SCAN_BYTES);
        assert_eq!(detect_anti_bot_page(&body), None);
        body.push_str("<div id='cf-browser-verification'>x</div>");
        // The marker is past the 16 KB window, so it is not scanned.
        assert_eq!(detect_anti_bot_page(&body), None);
    }

    #[test]
    fn entity_decoding_is_single_pass() {
        // `&amp;lt;` is the escaped form of `&lt;`. Decoding `&amp;` first
        // turned it into a real `<` and re-introduced markup.
        let decoded = decode_entities("&amp;lt;script&amp;gt;".to_string());
        assert_eq!(decoded, "&lt;script&gt;");
        assert!(!decoded.contains('<'));
    }

    #[test]
    fn code_blocks_keep_their_exact_contents() {
        // The inline-element passes used to rewrite code: `**` in a stack trace
        // became markdown emphasis, and a Python `a ** b` lost characters. A
        // wrong payload that still looks plausible is the worst failure mode
        // for a security tool.
        let html = "<p>Trace:</p><pre><code>  a ** b\n\n\n  &lt;div&gt; escaped\n</code></pre>";
        let md = html_to_markdown(html);
        assert!(
            md.contains("a ** b"),
            "emphasis characters were rewritten: {md}"
        );
        assert!(
            md.contains("<div> escaped"),
            "entities inside a code block were not decoded: {md}"
        );
    }

    #[test]
    fn blank_lines_inside_code_blocks_survive() {
        // A stack trace separated by blank lines must not be reflowed into a
        // single block; the model reads these to pick a payload.
        let html = "<pre><code>line1\n\n\nline2\n\n\n\nline3</code></pre>";
        let md = html_to_markdown(html);
        assert!(
            md.contains("line1\n\n\nline2"),
            "blank line collapsed: {md:?}"
        );
        assert!(
            md.contains("line2\n\n\n\nline3"),
            "blank lines collapsed: {md:?}"
        );
    }

    #[test]
    fn blank_lines_outside_code_blocks_still_collapse() {
        let html = "<p>one</p><div></div><div></div><div></div><p>two</p>";
        let md = html_to_markdown(html);
        assert!(!md.contains("\n\n\n"), "excess blank lines kept: {md:?}");
        assert!(md.contains("one") && md.contains("two"));
    }

    #[test]
    fn multiple_code_blocks_are_all_restored() {
        let html = "<pre><code>first</code></pre><p>between</p><pre><code>second</code></pre>";
        let md = html_to_markdown(html);
        assert!(md.contains("first"), "first block lost: {md}");
        assert!(md.contains("between"), "prose lost: {md}");
        assert!(md.contains("second"), "second block lost: {md}");
        assert!(!md.contains(CODE_PLACEHOLDER), "placeholder leaked: {md}");
    }

    #[test]
    fn plain_text_payloads_skip_the_tag_passes() {
        // JSON, source and CSV contain no markup, so every tag pass is a
        // no-op — and running them anyway is what turned a `<foo>` inside a
        // JSON string literal into a stripped tag.
        let json = r#"{"query": "select * from t where a < b", "n": 3}"#;
        assert_eq!(html_to_markdown(json), json);
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

    /// Regression: the SSRF guard only understood four-part dotted-decimal
    /// literals and returned "public" for everything else, so all of these
    /// — which the OS resolver connects to without complaint — reached
    /// internal services and the cloud metadata endpoint.
    #[test]
    fn ssrf_guard_catches_alternate_ip_spellings() {
        for internal in [
            "127.0.0.1",
            "127.1",              // short form
            "127.0.1",            // 3-part
            "0x7f.0.0.1",         // hex octets
            "0x7f.1",             // hex + short
            "2130706433",         // whole 32-bit value
            "0177.0.0.1",         // octal octets
            "[::1]",              // bracketed IPv6 loopback
            "::1",                // bare IPv6 loopback
            "[::ffff:127.0.0.1]", // IPv4-mapped IPv6
            "0.0.0.0",            // this-network
            "169.254.169.254",    // AWS/GCP metadata
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "100.64.0.1", // CGNAT
            "255.255.255.255",
        ] {
            assert!(is_private_ip(internal), "SSRF guard missed `{internal}`");
        }
    }

    #[test]
    fn ssrf_guard_does_not_block_real_public_hosts() {
        // A guard that blocks everything is as useless as one that blocks
        // nothing — it just trains the caller to route around it.
        for public in [
            "1.1.1.1",
            "8.8.8.8",
            "93.184.216.34",
            "172.32.0.1", // just outside 172.16/12
            "172.15.255.255",
            "11.0.0.1",
        ] {
            assert!(
                !is_private_ip(public),
                "false positive on public address `{public}`"
            );
        }
        // Real hostnames are not literals, so the literal check must not fire
        // on them; the resolve-and-check pass handles those.
        for name in ["example.com", "api.github.com"] {
            assert!(!is_private_ip(name), "false positive on `{name}`");
        }
    }

    #[test]
    fn legacy_ip_parsing_matches_the_right_addresses() {
        assert_eq!(
            parse_ip_literal("127.1"),
            Some("127.0.0.1".parse().unwrap())
        );
        assert_eq!(
            parse_ip_literal("2130706433"),
            Some("127.0.0.1".parse().unwrap())
        );
        assert_eq!(
            parse_ip_literal("0x7f.0.0.1"),
            Some("127.0.0.1".parse().unwrap())
        );
        assert_eq!(parse_ip_literal("example.com"), None);
    }
}
