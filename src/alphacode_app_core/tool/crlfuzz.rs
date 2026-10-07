use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

pub struct CrlfuzzTool;

#[derive(Deserialize)]
struct CrlfuzzInput {
    /// Target URL(s)
    url: Vec<String>,
    /// Input file
    #[serde(default)]
    input: Option<String>,
    /// Threads
    #[serde(default)]
    threads: Option<usize>,
    /// Timeout
    #[serde(default)]
    timeout: Option<u64>,
    /// Proxy
    #[serde(default)]
    proxy: Option<String>,
    /// Output file
    #[serde(default)]
    output: Option<String>,
    /// JSON output
    #[serde(default)]
    json: bool,
    /// Silent
    #[serde(default)]
    silent: bool,
    /// Follow redirects
    #[serde(default)]
    follow_redirects: bool,
    /// Headers
    #[serde(default)]
    headers: Vec<String>,
    /// Cookies
    #[serde(default)]
    cookies: Option<String>,
    /// User agent
    #[serde(default)]
    user_agent: Option<String>,
    /// Data (POST)
    #[serde(default)]
    data: Option<String>,
    /// Method
    #[serde(default)]
    method: Option<String>,
    /// Wordlist
    #[serde(default)]
    wordlist: Option<String>,
    /// Scan type
    #[serde(default)]
    scan_type: Option<String>,
    /// Payload
    #[serde(default)]
    payload: Option<String>,
    /// Payload file
    #[serde(default)]
    payload_file: Option<String>,
    /// Filter
    #[serde(default)]
    filter: Option<String>,
    /// Match
    #[serde(default)]
    match_pattern: Option<String>,
    /// Status codes
    #[serde(default)]
    status_codes: Option<String>,
    /// Exclude status
    #[serde(default)]
    exclude_status: Option<String>,
    /// Max time
    #[serde(default)]
    max_time: Option<u64>,
    /// Delay
    #[serde(default)]
    delay: Option<u64>,
    /// Retries
    #[serde(default)]
    retries: Option<u64>,
    /// Concurrency
    #[serde(default)]
    concurrency: Option<usize>,
    /// Rate limit
    #[serde(default)]
    rate_limit: Option<u64>,
    /// Burst
    #[serde(default)]
    burst: Option<u64>,
    /// Jitter
    #[serde(default)]
    jitter: Option<u64>,
    /// Seed
    #[serde(default)]
    seed: Option<u64>,
    /// Output format
    #[serde(default)]
    output_format: Option<String>,
    /// Found action
    #[serde(default)]
    found_action: Option<String>,
    /// Found action shell
    #[serde(default)]
    found_action_shell: Option<String>,
    /// Found action url
    #[serde(default)]
    found_action_url: Option<String>,
    /// Found action file
    #[serde(default)]
    found_action_file: Option<String>,
    /// Found action data
    #[serde(default)]
    found_action_data: Option<String>,
    /// Found action header
    #[serde(default)]
    found_action_header: Option<String>,
    /// Found action cookie
    #[serde(default)]
    found_action_cookie: Option<String>,
    /// Found action method
    #[serde(default)]
    found_action_method: Option<String>,
    /// Found action param
    #[serde(default)]
    found_action_param: Option<String>,
    /// Found action payload
    #[serde(default)]
    found_action_payload: Option<String>,
    /// Found action payload file
    #[serde(default)]
    found_action_payload_file: Option<String>,
    /// Found action filter
    #[serde(default)]
    found_action_filter: Option<String>,
    /// Found action match
    #[serde(default)]
    found_action_match: Option<String>,
    /// Found action status codes
    #[serde(default)]
    found_action_status_codes: Option<String>,
    /// Found action exclude status
    #[serde(default)]
    found_action_exclude_status: Option<String>,
    /// Found action max time
    #[serde(default)]
    found_action_max_time: Option<u64>,
    /// Found action delay
    #[serde(default)]
    found_action_delay: Option<u64>,
    /// Found action retries
    #[serde(default)]
    found_action_retries: Option<u64>,
    /// Found action concurrency
    #[serde(default)]
    found_action_concurrency: Option<usize>,
    /// Found action rate limit
    #[serde(default)]
    found_action_rate_limit: Option<u64>,
    /// Found action burst
    #[serde(default)]
    found_action_burst: Option<u64>,
    /// Found action jitter
    #[serde(default)]
    found_action_jitter: Option<u64>,
    /// Found action seed
    #[serde(default)]
    found_action_seed: Option<u64>,
    /// Found action output format
    #[serde(default)]
    found_action_output_format: Option<String>,
}

#[async_trait]
impl Tool for CrlfuzzTool {
    fn name(&self) -> &str {
        "crlfuzz"
    }

    fn description(&self) -> &str {
        "CRLF injection scanner. Use to find CRLF injection vulnerabilities in web applications."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": super::intent_schema_property(),
                "url": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Target URLs to scan."
                },
                "input": {
                    "type": "string",
                    "description": "Input file with URLs (one per line)."
                },
                "threads": {
                    "type": "integer",
                    "description": "Concurrent threads. Default: 10."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds."
                },
                "proxy": {
                    "type": "string",
                    "description": "Proxy URL."
                },
                "follow_redirects": {
                    "type": "boolean",
                    "description": "Follow redirects. Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: CrlfuzzInput = normalize_crlfuzz_input(&input)?;
        if params.url.is_empty() && params.input.is_none() {
            return Err(anyhow::anyhow!(
                "crlfuzz needs targets: provide `url` (array of URLs) or `input` (file path)"
            ));
        }
        let args = build_args(&params)?;

        let output = super::recon_common::run_bounded(
            "crlfuzz",
            &args,
            super::recon_common::DEFAULT_TOOL_TIMEOUT,
        )
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. {}", super::recon_common::install_hint("crlfuzz"))
            } else {
                anyhow::anyhow!("{e}")
            }
        })?;

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if !output.status.success() {
            let detail = if stderr.is_empty() {
                if stdout.is_empty() {
                    "no output (binary exited non-zero with empty stderr)".to_string()
                } else {
                    crate::alphacode_core::util::truncate_str(&stdout, 500).to_string()
                }
            } else {
                crate::alphacode_core::util::truncate_str(&stderr, 500).to_string()
            };
            return Err(anyhow::anyhow!("crlfuzz exited with error: {detail}"));
        }

        let (lines, total, truncated) = super::recon_common::parse_lines(&output.stdout);

        let mut result = format!("crlfuzz found {} results:\n\n", total);
        for line in &lines {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str(&super::recon_common::truncation_notice(lines.len(), total));

        let mut metadata = HashMap::new();
        metadata.insert("count".to_string(), json!(lines.len()));
        metadata.insert("total_found".to_string(), json!(total));
        metadata.insert("truncated".to_string(), json!(truncated));

        Ok(ToolOutput::new(result)
            .with_title(format!("crlfuzz: {total} results"))
            .with_metadata(json!(metadata)))
    }
}

impl CrlfuzzTool {
    pub fn new() -> Self {
        Self
    }
}

fn normalize_crlfuzz_input(input: &Value) -> Result<CrlfuzzInput> {
    let mut params: CrlfuzzInput = serde_json::from_value(input.clone()).unwrap_or(CrlfuzzInput {
        url: Vec::new(),
        input: None,
        threads: None,
        timeout: None,
        proxy: None,
        output: None,
        json: false,
        silent: false,
        follow_redirects: false,
        headers: Vec::new(),
        cookies: None,
        user_agent: None,
        data: None,
        method: None,
        wordlist: None,
        scan_type: None,
        payload: None,
        payload_file: None,
        filter: None,
        match_pattern: None,
        status_codes: None,
        exclude_status: None,
        max_time: None,
        delay: None,
        retries: None,
        concurrency: None,
        rate_limit: None,
        burst: None,
        jitter: None,
        seed: None,
        output_format: None,
        found_action: None,
        found_action_shell: None,
        found_action_url: None,
        found_action_file: None,
        found_action_data: None,
        found_action_header: None,
        found_action_cookie: None,
        found_action_method: None,
        found_action_param: None,
        found_action_payload: None,
        found_action_payload_file: None,
        found_action_filter: None,
        found_action_match: None,
        found_action_status_codes: None,
        found_action_exclude_status: None,
        found_action_max_time: None,
        found_action_delay: None,
        found_action_retries: None,
        found_action_concurrency: None,
        found_action_rate_limit: None,
        found_action_burst: None,
        found_action_jitter: None,
        found_action_seed: None,
        found_action_output_format: None,
    });
    if !params.url.is_empty() {
        return Ok(params);
    }
    let obj = match input.as_object() {
        Some(obj) => obj,
        None => return Ok(params),
    };
    let mut extra: Vec<String> = Vec::new();
    for key in ["url", "urls", "target", "targets", "u"] {
        if let Some(value) = obj.get(key) {
            if let Some(s) = value.as_str() {
                if !s.trim().is_empty() {
                    extra.push(s.trim().to_string());
                }
            } else if let Some(arr) = value.as_array() {
                for item in arr {
                    if let Some(s) = item.as_str()
                        && !s.trim().is_empty()
                    {
                        extra.push(s.trim().to_string());
                    }
                }
            }
        }
    }
    params.url.extend(extra);
    Ok(params)
}

fn build_args(params: &CrlfuzzInput) -> Result<Vec<String>> {
    let mut args = Vec::new();

    if let Some(ref input) = params.input {
        args.push("-i".to_string());
        args.push(input.clone());
    } else if !params.url.is_empty() {
        for url in &params.url {
            args.push("-u".to_string());
            args.push(super::recon_common::validate_target(url)?);
        }
    }

    if let Some(threads) = params.threads {
        args.push("-t".to_string());
        args.push(threads.to_string());
    }
    if let Some(timeout) = params.timeout {
        args.push("-timeout".to_string());
        args.push(timeout.to_string());
    }
    if let Some(ref proxy) = params.proxy {
        args.push("-p".to_string());
        args.push(proxy.clone());
    }
    if let Some(ref output) = params.output {
        args.push("-o".to_string());
        args.push(output.clone());
    }
    if params.json {
        args.push("-j".to_string());
    }
    if params.silent {
        args.push("-s".to_string());
    }
    if params.follow_redirects {
        args.push("-r".to_string());
    }
    for header in &params.headers {
        args.push("-H".to_string());
        args.push(header.clone());
    }
    if let Some(ref cookies) = params.cookies {
        args.push("-c".to_string());
        args.push(cookies.clone());
    }
    args.push("-ua".to_string());
    args.push(
        params
            .user_agent
            .as_deref()
            .unwrap_or(crate::alphacode_provider_core::ALPHACODE_USER_AGENT)
            .to_string(),
    );
    if let Some(ref data) = params.data {
        args.push("-d".to_string());
        args.push(data.clone());
    }
    if let Some(ref method) = params.method {
        args.push("-m".to_string());
        args.push(method.clone());
    }
    if let Some(ref wl) = params.wordlist {
        args.push("-w".to_string());
        args.push(wl.clone());
    }
    if let Some(ref st) = params.scan_type {
        args.push("-st".to_string());
        args.push(st.clone());
    }
    if let Some(ref payload) = params.payload {
        args.push("-payload".to_string());
        args.push(payload.clone());
    }
    if let Some(ref file) = params.payload_file {
        args.push("-payload-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref filter) = params.filter {
        args.push("-filter".to_string());
        args.push(filter.clone());
    }
    if let Some(ref match_p) = params.match_pattern {
        args.push("-match".to_string());
        args.push(match_p.clone());
    }
    if let Some(ref codes) = params.status_codes {
        args.push("-sc".to_string());
        args.push(codes.clone());
    }
    if let Some(ref codes) = params.exclude_status {
        args.push("-esc".to_string());
        args.push(codes.clone());
    }
    if let Some(max_time) = params.max_time {
        args.push("-max-time".to_string());
        args.push(max_time.to_string());
    }
    if let Some(delay) = params.delay {
        args.push("-delay".to_string());
        args.push(delay.to_string());
    }
    if let Some(retries) = params.retries {
        args.push("-retries".to_string());
        args.push(retries.to_string());
    }
    if let Some(concurrency) = params.concurrency {
        args.push("-concurrency".to_string());
        args.push(concurrency.to_string());
    }
    if let Some(rate) = params.rate_limit {
        args.push("-rate".to_string());
        args.push(rate.to_string());
    }
    if let Some(burst) = params.burst {
        args.push("-burst".to_string());
        args.push(burst.to_string());
    }
    if let Some(jitter) = params.jitter {
        args.push("-jitter".to_string());
        args.push(jitter.to_string());
    }
    if let Some(seed) = params.seed {
        args.push("-seed".to_string());
        args.push(seed.to_string());
    }
    if let Some(ref format) = params.output_format {
        args.push("-format".to_string());
        args.push(format.clone());
    }
    if let Some(ref action) = params.found_action {
        args.push("-fa".to_string());
        args.push(action.clone());
    }
    if let Some(ref shell) = params.found_action_shell {
        args.push("-fa-shell".to_string());
        args.push(shell.clone());
    }
    if let Some(ref url) = params.found_action_url {
        args.push("-fa-url".to_string());
        args.push(url.clone());
    }
    if let Some(ref file) = params.found_action_file {
        args.push("-fa-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref data) = params.found_action_data {
        args.push("-fa-data".to_string());
        args.push(data.clone());
    }
    if let Some(ref header) = params.found_action_header {
        args.push("-fa-header".to_string());
        args.push(header.clone());
    }
    if let Some(ref cookie) = params.found_action_cookie {
        args.push("-fa-cookie".to_string());
        args.push(cookie.clone());
    }
    if let Some(ref method) = params.found_action_method {
        args.push("-fa-method".to_string());
        args.push(method.clone());
    }
    if let Some(ref param) = params.found_action_param {
        args.push("-fa-param".to_string());
        args.push(param.clone());
    }
    if let Some(ref payload) = params.found_action_payload {
        args.push("-fa-payload".to_string());
        args.push(payload.clone());
    }
    if let Some(ref file) = params.found_action_payload_file {
        args.push("-fa-payload-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref filter) = params.found_action_filter {
        args.push("-fa-filter".to_string());
        args.push(filter.clone());
    }
    if let Some(ref match_p) = params.found_action_match {
        args.push("-fa-match".to_string());
        args.push(match_p.clone());
    }
    if let Some(ref codes) = params.found_action_status_codes {
        args.push("-fa-sc".to_string());
        args.push(codes.clone());
    }
    if let Some(ref codes) = params.found_action_exclude_status {
        args.push("-fa-esc".to_string());
        args.push(codes.clone());
    }
    if let Some(max_time) = params.found_action_max_time {
        args.push("-fa-max-time".to_string());
        args.push(max_time.to_string());
    }
    if let Some(delay) = params.found_action_delay {
        args.push("-fa-delay".to_string());
        args.push(delay.to_string());
    }
    if let Some(retries) = params.found_action_retries {
        args.push("-fa-retries".to_string());
        args.push(retries.to_string());
    }
    if let Some(concurrency) = params.found_action_concurrency {
        args.push("-fa-concurrency".to_string());
        args.push(concurrency.to_string());
    }
    if let Some(rate) = params.found_action_rate_limit {
        args.push("-fa-rate".to_string());
        args.push(rate.to_string());
    }
    if let Some(burst) = params.found_action_burst {
        args.push("-fa-burst".to_string());
        args.push(burst.to_string());
    }
    if let Some(jitter) = params.found_action_jitter {
        args.push("-fa-jitter".to_string());
        args.push(jitter.to_string());
    }
    if let Some(seed) = params.found_action_seed {
        args.push("-fa-seed".to_string());
        args.push(seed.to_string());
    }
    if let Some(ref format) = params.found_action_output_format {
        args.push("-fa-format".to_string());
        args.push(format.clone());
    }

    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_accepts_aliases() {
        for payload in [
            serde_json::json!({"url": "https://example.com"}),
            serde_json::json!({"target": "https://example.com"}),
        ] {
            let params = normalize_crlfuzz_input(&payload).expect("normalize");
            assert!(!params.url.is_empty());
        }
    }
}
