use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

pub struct CorsyTool;

#[derive(Deserialize)]
struct CorsyInput {
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
    /// Headers
    #[serde(default)]
    headers: Vec<String>,
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
}

#[async_trait]
impl Tool for CorsyTool {
    fn name(&self) -> &str {
        "corsy"
    }

    fn description(&self) -> &str {
        "CORS misconfiguration scanner. Use to find CORS vulnerabilities in web applications."
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
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: CorsyInput = normalize_corsy_input(&input)?;
        if params.url.is_empty() && params.input.is_none() {
            return Err(anyhow::anyhow!(
                "corsy needs targets: provide `url` (array of URLs) or `input` (file path)"
            ));
        }
        let args = build_args(&params)?;

        let output = super::recon_common::run_bounded(
            "corsy",
            &args,
            super::recon_common::DEFAULT_TOOL_TIMEOUT,
        )
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. {}", super::recon_common::install_hint("corsy"))
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
            return Err(anyhow::anyhow!("corsy exited with error: {detail}"));
        }

        let (lines, total, truncated) = super::recon_common::parse_lines(&output.stdout);

        let mut result = format!("corsy found {} results:\n\n", total);
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
            .with_title(format!("corsy: {total} results"))
            .with_metadata(json!(metadata)))
    }
}

impl CorsyTool {
    pub fn new() -> Self {
        Self
    }
}

fn normalize_corsy_input(input: &Value) -> Result<CorsyInput> {
    let mut params: CorsyInput = serde_json::from_value(input.clone()).unwrap_or(CorsyInput {
        url: Vec::new(),
        input: None,
        threads: None,
        timeout: None,
        headers: Vec::new(),
        proxy: None,
        output: None,
        json: false,
        silent: false,
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

fn build_args(params: &CorsyInput) -> Result<Vec<String>> {
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
    super::recon_common::append_default_user_agent_header(&mut args, &params.headers);
    for header in &params.headers {
        if !header.contains(':') {
            return Err(anyhow::anyhow!(
                "invalid header `{header}`: expected the form `Name: value`"
            ));
        }
        args.push("-H".to_string());
        args.push(header.clone());
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
            let params = normalize_corsy_input(&payload).expect("normalize");
            assert!(!params.url.is_empty());
        }
    }

    #[test]
    fn build_args_brands_requests_without_overriding_a_custom_user_agent() {
        let mut params = normalize_corsy_input(&serde_json::json!({
            "url": ["https://example.com"]
        }))
        .expect("normalize");
        let args = build_args(&params).expect("args");
        let default_user_agent = format!(
            "User-Agent: {}",
            crate::alphacode_provider_core::ALPHACODE_USER_AGENT
        );
        assert!(
            args.windows(2)
                .any(|pair| { pair[0] == "-H" && pair[1] == default_user_agent })
        );

        params.headers.push("User-Agent: scanner-test".to_string());
        let args = build_args(&params).expect("custom args");
        let user_agents: Vec<_> = args
            .windows(2)
            .filter(|pair| {
                pair[0] == "-H" && pair[1].to_ascii_lowercase().starts_with("user-agent:")
            })
            .collect();
        assert_eq!(user_agents.len(), 1);
        assert_eq!(user_agents[0][1], "User-Agent: scanner-test");
    }
}
