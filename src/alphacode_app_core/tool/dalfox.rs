use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

pub struct DalfoxTool;

#[derive(Deserialize)]
struct DalfoxInput {
    /// Target URL
    url: String,
    /// Test parameter
    #[serde(default)]
    param: Option<String>,
    /// Test all parameters
    #[serde(default)]
    all_params: bool,
    /// Blind XSS
    #[serde(default)]
    blind: Option<String>,
    /// Payload
    #[serde(default)]
    payload: Option<String>,
    /// User agent
    #[serde(default)]
    user_agent: Option<String>,
    /// Cookie
    #[serde(default)]
    cookie: Option<String>,
    /// Header
    #[serde(default)]
    header: Vec<String>,
    /// Data (POST)
    #[serde(default)]
    data: Option<String>,
    /// Method
    #[serde(default)]
    method: Option<String>,
    /// Timeout
    #[serde(default)]
    timeout: Option<u64>,
    /// Delay
    #[serde(default)]
    delay: Option<u64>,
    /// Retries
    #[serde(default)]
    retries: Option<u64>,
    /// Concurrency
    #[serde(default)]
    concurrency: Option<usize>,
    /// Output format
    #[serde(default)]
    output_format: Option<String>,
    /// Silent
    #[serde(default)]
    silent: bool,
    /// JSON output
    #[serde(default)]
    json: bool,
    /// Only discovery
    #[serde(default)]
    only_discovery: bool,
    /// Mining
    #[serde(default)]
    mining: bool,
    /// Mining dom
    #[serde(default)]
    mining_dom: bool,
    /// Blind xss callback
    #[serde(default)]
    blind_xss_callback: Option<String>,
    /// Follow redirects
    #[serde(default)]
    follow_redirects: bool,
    /// Proxy
    #[serde(default)]
    proxy: Option<String>,
    /// Ignore return
    #[serde(default)]
    ignore_return: Option<String>,
    /// Custom payload file
    #[serde(default)]
    custom_payload_file: Option<String>,
    /// Custom blind xss file
    #[serde(default)]
    custom_blind_xss_file: Option<String>,
    /// Wordlist
    #[serde(default)]
    wordlist: Option<String>,
    /// Fuzzing
    #[serde(default)]
    fuzzing: bool,
    /// Parameter file
    #[serde(default)]
    param_file: Option<String>,
    /// Header file
    #[serde(default)]
    header_file: Option<String>,
    /// Cookie file
    #[serde(default)]
    cookie_file: Option<String>,
    /// Data file
    #[serde(default)]
    data_file: Option<String>,
    /// Payload file
    #[serde(default)]
    payload_file: Option<String>,
    /// Blind xss file
    #[serde(default)]
    blind_xss_file: Option<String>,
    /// Output file
    #[serde(default)]
    output_file: Option<String>,
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
    /// Found action blind xss
    #[serde(default)]
    found_action_blind_xss: Option<String>,
    /// Found action blind xss callback
    #[serde(default)]
    found_action_blind_xss_callback: Option<String>,
    /// Found action custom payload file
    #[serde(default)]
    found_action_custom_payload_file: Option<String>,
    /// Found action custom blind xss file
    #[serde(default)]
    found_action_custom_blind_xss_file: Option<String>,
    /// Found action wordlist
    #[serde(default)]
    found_action_wordlist: Option<String>,
    /// Found action fuzzing
    #[serde(default)]
    found_action_fuzzing: bool,
    /// Found action param file
    #[serde(default)]
    found_action_param_file: Option<String>,
    /// Found action header file
    #[serde(default)]
    found_action_header_file: Option<String>,
    /// Found action cookie file
    #[serde(default)]
    found_action_cookie_file: Option<String>,
    /// Found action data file
    #[serde(default)]
    found_action_data_file: Option<String>,
    /// Found action payload file
    #[serde(default)]
    found_action_payload_file: Option<String>,
    /// Found action blind xss file
    #[serde(default)]
    found_action_blind_xss_file: Option<String>,
    /// Found action output file
    #[serde(default)]
    found_action_output_file: Option<String>,
}

#[async_trait]
impl Tool for DalfoxTool {
    fn name(&self) -> &str {
        "dalfox"
    }

    fn description(&self) -> &str {
        "XSS scanner and payload generator. Use to detect and exploit XSS vulnerabilities in authorized web applications. If Dalfox or Go is missing, Alphacode attempts to install them on first use; wait for the tool result before choosing another approach, and treat setup failures as a scan that did not run."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "intent": super::intent_schema_property(),
                "url": {
                    "type": "string",
                    "description": "Target URL with parameter (e.g., 'https://example.com/search?q=test')."
                },
                "param": {
                    "type": "string",
                    "description": "Parameter to test."
                },
                "all_params": {
                    "type": "boolean",
                    "description": "Test all parameters. Default: false."
                },
                "blind": {
                    "type": "string",
                    "description": "Blind XSS callback URL."
                },
                "payload": {
                    "type": "string",
                    "description": "Custom payload."
                },
                "data": {
                    "type": "string",
                    "description": "POST data."
                },
                "method": {
                    "type": "string",
                    "description": "HTTP method."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds."
                },
                "concurrency": {
                    "type": "integer",
                    "description": "Concurrent requests."
                },
                "only_discovery": {
                    "type": "boolean",
                    "description": "Only discover parameters. Default: false."
                },
                "mining": {
                    "type": "boolean",
                    "description": "Mine parameters. Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: DalfoxInput = normalize_dalfox_input(&input)?;
        let args = build_args(&params)?;

        let output = super::recon_common::run_bounded(
            "dalfox",
            &args,
            super::recon_common::DEFAULT_TOOL_TIMEOUT,
        )
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. {}", super::recon_common::install_hint("dalfox"))
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
            return Err(anyhow::anyhow!("dalfox exited with error: {detail}"));
        }

        let (lines, total, truncated) = super::recon_common::parse_lines(&output.stdout);

        let mut result = format!("dalfox found {} results:\n\n", total);
        for line in &lines {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str(&super::recon_common::truncation_notice(lines.len(), total));

        let mut metadata = HashMap::new();
        metadata.insert("url".to_string(), json!(params.url));
        metadata.insert("count".to_string(), json!(lines.len()));
        metadata.insert("total_found".to_string(), json!(total));
        metadata.insert("truncated".to_string(), json!(truncated));

        Ok(ToolOutput::new(result)
            .with_title(format!("dalfox: {total} results"))
            .with_metadata(json!(metadata)))
    }
}

impl DalfoxTool {
    pub fn new() -> Self {
        Self
    }
}

fn normalize_dalfox_input(input: &Value) -> Result<DalfoxInput> {
    if let Ok(params) = serde_json::from_value::<DalfoxInput>(input.clone()) {
        return Ok(params);
    }
    // Aliases, bare-URL strings and truncated payloads all resolve through the shared
    // coercion ladder; a hand-rolled key list missed every other shape.
    let url = super::coerce_url_arg(input, "dalfox")?;
    // Remaining options are optional. A payload that was a bare URL string has no
    // object to read them from, so an empty map stands in for the defaults.
    let obj = input.as_object().cloned().unwrap_or_default();
    Ok(DalfoxInput {
        url: url.to_string(),
        param: obj.get("param").and_then(|v| v.as_str()).map(String::from),
        all_params: obj
            .get("all_params")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        blind: obj.get("blind").and_then(|v| v.as_str()).map(String::from),
        payload: obj
            .get("payload")
            .and_then(|v| v.as_str())
            .map(String::from),
        user_agent: obj
            .get("user_agent")
            .and_then(|v| v.as_str())
            .map(String::from),
        cookie: obj.get("cookie").and_then(|v| v.as_str()).map(String::from),
        header: obj
            .get("header")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        data: obj.get("data").and_then(|v| v.as_str()).map(String::from),
        method: obj.get("method").and_then(|v| v.as_str()).map(String::from),
        timeout: obj.get("timeout").and_then(|v| v.as_u64()),
        delay: obj.get("delay").and_then(|v| v.as_u64()),
        retries: obj.get("retries").and_then(|v| v.as_u64()),
        concurrency: obj
            .get("concurrency")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize),
        output_format: obj
            .get("output_format")
            .and_then(|v| v.as_str())
            .map(String::from),
        silent: obj.get("silent").and_then(|v| v.as_bool()).unwrap_or(false),
        json: obj.get("json").and_then(|v| v.as_bool()).unwrap_or(false),
        only_discovery: obj
            .get("only_discovery")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        mining: obj.get("mining").and_then(|v| v.as_bool()).unwrap_or(false),
        mining_dom: obj
            .get("mining_dom")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        blind_xss_callback: obj
            .get("blind_xss_callback")
            .and_then(|v| v.as_str())
            .map(String::from),
        follow_redirects: obj
            .get("follow_redirects")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        proxy: obj.get("proxy").and_then(|v| v.as_str()).map(String::from),
        ignore_return: obj
            .get("ignore_return")
            .and_then(|v| v.as_str())
            .map(String::from),
        custom_payload_file: obj
            .get("custom_payload_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        custom_blind_xss_file: obj
            .get("custom_blind_xss_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        wordlist: obj
            .get("wordlist")
            .and_then(|v| v.as_str())
            .map(String::from),
        fuzzing: obj
            .get("fuzzing")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        param_file: obj
            .get("param_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        header_file: obj
            .get("header_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        cookie_file: obj
            .get("cookie_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        data_file: obj
            .get("data_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        payload_file: obj
            .get("payload_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        blind_xss_file: obj
            .get("blind_xss_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        output_file: obj
            .get("output_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action: obj
            .get("found_action")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_shell: obj
            .get("found_action_shell")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_url: obj
            .get("found_action_url")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_file: obj
            .get("found_action_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_data: obj
            .get("found_action_data")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_header: obj
            .get("found_action_header")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_cookie: obj
            .get("found_action_cookie")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_method: obj
            .get("found_action_method")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_param: obj
            .get("found_action_param")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_payload: obj
            .get("found_action_payload")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_blind_xss: obj
            .get("found_action_blind_xss")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_blind_xss_callback: obj
            .get("found_action_blind_xss_callback")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_custom_payload_file: obj
            .get("found_action_custom_payload_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_custom_blind_xss_file: obj
            .get("found_action_custom_blind_xss_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_wordlist: obj
            .get("found_action_wordlist")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_fuzzing: obj
            .get("found_action_fuzzing")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        found_action_param_file: obj
            .get("found_action_param_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_header_file: obj
            .get("found_action_header_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_cookie_file: obj
            .get("found_action_cookie_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_data_file: obj
            .get("found_action_data_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_payload_file: obj
            .get("found_action_payload_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_blind_xss_file: obj
            .get("found_action_blind_xss_file")
            .and_then(|v| v.as_str())
            .map(String::from),
        found_action_output_file: obj
            .get("found_action_output_file")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

fn build_args(params: &DalfoxInput) -> Result<Vec<String>> {
    let mut args = Vec::new();

    let url = super::recon_common::validate_target(&params.url)?;
    args.push("url".to_string());
    args.push(url);

    if let Some(ref param) = params.param {
        args.push("--param".to_string());
        args.push(param.clone());
    }
    if params.all_params {
        args.push("--all-params".to_string());
    }
    if let Some(ref blind) = params.blind {
        args.push("--blind".to_string());
        args.push(blind.clone());
    }
    if let Some(ref payload) = params.payload {
        args.push("--payload".to_string());
        args.push(payload.clone());
    }
    args.push("--user-agent".to_string());
    args.push(
        params
            .user_agent
            .as_deref()
            .unwrap_or(crate::alphacode_provider_core::ALPHACODE_USER_AGENT)
            .to_string(),
    );
    if let Some(ref cookie) = params.cookie {
        args.push("--cookie".to_string());
        args.push(cookie.clone());
    }
    for header in &params.header {
        args.push("--header".to_string());
        args.push(header.clone());
    }
    if let Some(ref data) = params.data {
        args.push("--data".to_string());
        args.push(data.clone());
    }
    if let Some(ref method) = params.method {
        args.push("--method".to_string());
        args.push(method.clone());
    }
    if let Some(timeout) = params.timeout {
        args.push("--timeout".to_string());
        args.push(timeout.to_string());
    }
    if let Some(delay) = params.delay {
        args.push("--delay".to_string());
        args.push(delay.to_string());
    }
    if let Some(retries) = params.retries {
        args.push("--retries".to_string());
        args.push(retries.to_string());
    }
    if let Some(concurrency) = params.concurrency {
        args.push("--concurrency".to_string());
        args.push(concurrency.to_string());
    }
    if let Some(ref format) = params.output_format {
        args.push("--output-format".to_string());
        args.push(format.clone());
    }
    if params.silent {
        args.push("--silent".to_string());
    }
    if params.json {
        args.push("--json".to_string());
    }
    if params.only_discovery {
        args.push("--only-discovery".to_string());
    }
    if params.mining {
        args.push("--mining".to_string());
    }
    if params.mining_dom {
        args.push("--mining-dom".to_string());
    }
    if let Some(ref callback) = params.blind_xss_callback {
        args.push("--blind-xss-callback".to_string());
        args.push(callback.clone());
    }
    if params.follow_redirects {
        args.push("--follow-redirects".to_string());
    }
    if let Some(ref proxy) = params.proxy {
        args.push("--proxy".to_string());
        args.push(proxy.clone());
    }
    if let Some(ref ignore) = params.ignore_return {
        args.push("--ignore-return".to_string());
        args.push(ignore.clone());
    }
    if let Some(ref file) = params.custom_payload_file {
        args.push("--custom-payload-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.custom_blind_xss_file {
        args.push("--custom-blind-xss-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.wordlist {
        args.push("--wordlist".to_string());
        args.push(file.clone());
    }
    if params.fuzzing {
        args.push("--fuzzing".to_string());
    }
    if let Some(ref file) = params.param_file {
        args.push("--param-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.header_file {
        args.push("--header-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.cookie_file {
        args.push("--cookie-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.data_file {
        args.push("--data-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.payload_file {
        args.push("--payload-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.blind_xss_file {
        args.push("--blind-xss-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.output_file {
        args.push("--output-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref action) = params.found_action {
        args.push("--found-action".to_string());
        args.push(action.clone());
    }
    if let Some(ref shell) = params.found_action_shell {
        args.push("--found-action-shell".to_string());
        args.push(shell.clone());
    }
    if let Some(ref url) = params.found_action_url {
        args.push("--found-action-url".to_string());
        args.push(url.clone());
    }
    if let Some(ref file) = params.found_action_file {
        args.push("--found-action-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref data) = params.found_action_data {
        args.push("--found-action-data".to_string());
        args.push(data.clone());
    }
    if let Some(ref header) = params.found_action_header {
        args.push("--found-action-header".to_string());
        args.push(header.clone());
    }
    if let Some(ref cookie) = params.found_action_cookie {
        args.push("--found-action-cookie".to_string());
        args.push(cookie.clone());
    }
    if let Some(ref method) = params.found_action_method {
        args.push("--found-action-method".to_string());
        args.push(method.clone());
    }
    if let Some(ref param) = params.found_action_param {
        args.push("--found-action-param".to_string());
        args.push(param.clone());
    }
    if let Some(ref payload) = params.found_action_payload {
        args.push("--found-action-payload".to_string());
        args.push(payload.clone());
    }
    if let Some(ref blind) = params.found_action_blind_xss {
        args.push("--found-action-blind-xss".to_string());
        args.push(blind.clone());
    }
    if let Some(ref callback) = params.found_action_blind_xss_callback {
        args.push("--found-action-blind-xss-callback".to_string());
        args.push(callback.clone());
    }
    if let Some(ref file) = params.found_action_custom_payload_file {
        args.push("--found-action-custom-payload-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.found_action_custom_blind_xss_file {
        args.push("--found-action-custom-blind-xss-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.found_action_wordlist {
        args.push("--found-action-wordlist".to_string());
        args.push(file.clone());
    }
    if params.found_action_fuzzing {
        args.push("--found-action-fuzzing".to_string());
    }
    if let Some(ref file) = params.found_action_param_file {
        args.push("--found-action-param-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.found_action_header_file {
        args.push("--found-action-header-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.found_action_cookie_file {
        args.push("--found-action-cookie-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.found_action_data_file {
        args.push("--found-action-data-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.found_action_payload_file {
        args.push("--found-action-payload-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.found_action_blind_xss_file {
        args.push("--found-action-blind-xss-file".to_string());
        args.push(file.clone());
    }
    if let Some(ref file) = params.found_action_output_file {
        args.push("--found-action-output-file".to_string());
        args.push(file.clone());
    }

    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_accepts_aliases() {
        for payload in [
            serde_json::json!({"url": "https://example.com/search?q=test"}),
            serde_json::json!({"target": "https://example.com/search?q=test"}),
        ] {
            let params = normalize_dalfox_input(&payload).expect("normalize");
            assert_eq!(params.url, "https://example.com/search?q=test");
        }
    }
}
