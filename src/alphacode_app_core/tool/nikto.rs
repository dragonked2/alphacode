use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

pub struct NiktoTool;

#[derive(Deserialize)]
struct NiktoInput {
    /// Target URL
    url: String,
    /// Port
    #[serde(default)]
    port: Option<u16>,
    /// SSL
    #[serde(default)]
    ssl: bool,
    /// Tuning: 1-9 (1=interesting, 2=all, etc.)
    #[serde(default)]
    tuning: Option<String>,
    /// Timeout
    #[serde(default)]
    timeout: Option<u64>,
    /// User agent
    #[serde(default)]
    user_agent: Option<String>,
    /// Cookie
    #[serde(default)]
    cookie: Option<String>,
    /// Proxy
    #[serde(default)]
    proxy: Option<String>,
    /// Output format
    #[serde(default)]
    output_format: Option<String>,
    /// Evasion technique
    #[serde(default)]
    evasion: Option<String>,
    /// Follow redirects
    #[serde(default)]
    follow_redirects: bool,
    /// Max time
    #[serde(default)]
    max_time: Option<u64>,
}

#[async_trait]
impl Tool for NiktoTool {
    fn name(&self) -> &str {
        "nikto"
    }

    fn description(&self) -> &str {
        "Web server scanner. Use to find known vulnerabilities, misconfigurations, and dangerous files on web servers."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "intent": super::intent_schema_property(),
                "url": {
                    "type": "string",
                    "description": "Target URL (e.g., 'https://example.com')."
                },
                "port": {
                    "type": "integer",
                    "description": "Port to scan."
                },
                "ssl": {
                    "type": "boolean",
                    "description": "Use SSL. Default: false."
                },
                "tuning": {
                    "type": "string",
                    "description": "Scan tuning (1-9)."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds."
                },
                "follow_redirects": {
                    "type": "boolean",
                    "description": "Follow redirects. Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: NiktoInput = normalize_nikto_input(&input)?;
        let args = build_args(&params)?;

        let output = super::recon_common::run_bounded(
            "nikto",
            &args,
            super::recon_common::DEFAULT_TOOL_TIMEOUT,
        )
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. {}", super::recon_common::install_hint("nikto"))
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
            return Err(anyhow::anyhow!("nikto exited with error: {detail}"));
        }

        let (lines, total, truncated) = super::recon_common::parse_lines(&output.stdout);

        let mut result = format!("nikto found {} results:\n\n", total);
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
            .with_title(format!("nikto: {total} results"))
            .with_metadata(json!(metadata)))
    }
}

impl NiktoTool {
    pub fn new() -> Self {
        Self
    }
}

fn normalize_nikto_input(input: &Value) -> Result<NiktoInput> {
    if let Ok(params) = serde_json::from_value::<NiktoInput>(input.clone()) {
        return Ok(params);
    }
    // Aliases, bare-URL strings and truncated payloads all resolve through the shared
    // coercion ladder; a hand-rolled key list missed every other shape.
    let url = super::coerce_url_arg(input, "nikto")?;
    // Remaining options are optional. A payload that was a bare URL string has no
    // object to read them from, so an empty map stands in for the defaults.
    let obj = input.as_object().cloned().unwrap_or_default();
    Ok(NiktoInput {
        url: url.to_string(),
        port: obj.get("port").and_then(|v| v.as_u64()).map(|n| n as u16),
        ssl: obj.get("ssl").and_then(|v| v.as_bool()).unwrap_or(false),
        tuning: obj.get("tuning").and_then(|v| v.as_str()).map(String::from),
        timeout: obj.get("timeout").and_then(|v| v.as_u64()),
        user_agent: obj
            .get("user_agent")
            .and_then(|v| v.as_str())
            .map(String::from),
        cookie: obj.get("cookie").and_then(|v| v.as_str()).map(String::from),
        proxy: obj.get("proxy").and_then(|v| v.as_str()).map(String::from),
        output_format: obj
            .get("output_format")
            .and_then(|v| v.as_str())
            .map(String::from),
        evasion: obj
            .get("evasion")
            .and_then(|v| v.as_str())
            .map(String::from),
        follow_redirects: obj
            .get("follow_redirects")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        max_time: obj.get("max_time").and_then(|v| v.as_u64()),
    })
}

fn build_args(params: &NiktoInput) -> Result<Vec<String>> {
    let mut args = Vec::new();

    let url = super::recon_common::validate_target(&params.url)?;
    args.push("-host".to_string());
    args.push(url);

    if let Some(port) = params.port {
        args.push("-port".to_string());
        args.push(port.to_string());
    }
    if params.ssl {
        args.push("-ssl".to_string());
    }
    if let Some(ref tuning) = params.tuning {
        args.push("-Tuning".to_string());
        args.push(tuning.clone());
    }
    if let Some(timeout) = params.timeout {
        args.push("-timeout".to_string());
        args.push(timeout.to_string());
    }
    args.push("-useragent".to_string());
    args.push(
        params
            .user_agent
            .as_deref()
            .unwrap_or(crate::alphacode_provider_core::ALPHACODE_USER_AGENT)
            .to_string(),
    );
    if let Some(ref cookie) = params.cookie {
        args.push("-cookie".to_string());
        args.push(cookie.clone());
    }
    if let Some(ref proxy) = params.proxy {
        args.push("-proxy".to_string());
        args.push(proxy.clone());
    }
    if let Some(ref format) = params.output_format {
        args.push("-Format".to_string());
        args.push(format.clone());
    }
    if let Some(ref evasion) = params.evasion {
        args.push("-evasion".to_string());
        args.push(evasion.clone());
    }
    if params.follow_redirects {
        args.push("-followredirects".to_string());
    }
    if let Some(max_time) = params.max_time {
        args.push("-maxtime".to_string());
        args.push(max_time.to_string());
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
            let params = normalize_nikto_input(&payload).expect("normalize");
            assert_eq!(params.url, "https://example.com");
        }
    }
}
