use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const DEFAULT_TIMEOUT: u64 = 30;
const DEFAULT_THREADS: usize = 50;

pub struct HttpxTool;

#[derive(Deserialize)]
struct HttpxInput {
    #[serde(default)]
    list: Option<String>,
    #[serde(default)]
    targets: Vec<String>,
    #[serde(default)]
    status_codes: Option<String>,
    #[serde(default)]
    title: bool,
    #[serde(default)]
    tech_detect: bool,
    #[serde(default)]
    web_server: bool,
    #[serde(default)]
    content_type: bool,
    #[serde(default)]
    response_size: bool,
    #[serde(default)]
    method: bool,
    #[serde(default)]
    tls: bool,
    #[serde(default)]
    cdn: bool,
    #[serde(default)]
    chains: bool,
    #[serde(default)]
    threads: Option<usize>,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    follow_redirects: bool,
    #[serde(default)]
    no_color: bool,
}

#[async_trait]
impl Tool for HttpxTool {
    fn name(&self) -> &str {
        "httpx"
    }

    fn description(&self) -> &str {
        "HTTP probing and fingerprinting tool. Checks which hosts are live, their status codes, technologies, TLS info, and more."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": super::intent_schema_property(),
                "targets": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "List of target URLs or hosts to probe."
                },
                "list": {
                    "type": "string",
                    "description": "Path to a file containing targets (one per line)."
                },
                "status_codes": {
                    "type": "string",
                    "description": "Filter by specific status codes (e.g., '200,301,403')."
                },
                "title": {
                    "type": "boolean",
                    "description": "Extract page title. Default: false."
                },
                "tech_detect": {
                    "type": "boolean",
                    "description": "Detect technology stack. Default: false."
                },
                "web_server": {
                    "type": "boolean",
                    "description": "Extract web server name. Default: false."
                },
                "content_type": {
                    "type": "boolean",
                    "description": "Extract content type. Default: false."
                },
                "response_size": {
                    "type": "boolean",
                    "description": "Extract response size. Default: false."
                },
                "method": {
                    "type": "boolean",
                    "description": "Extract HTTP method. Default: false."
                },
                "tls": {
                    "type": "boolean",
                    "description": "Extract TLS certificate info. Default: false."
                },
                "cdn": {
                    "type": "boolean",
                    "description": "Detect CDN provider. Default: false."
                },
                "follow_redirects": {
                    "type": "boolean",
                    "description": "Follow redirects. Default: false."
                },
                "threads": {
                    "type": "integer",
                    "description": "Number of concurrent threads. Default: 50."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Request timeout in seconds. Default: 30."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: HttpxInput = serde_json::from_value(input)?;
        let args = build_args(&params);

        let output = tokio::process::Command::new("httpx")
            .args(&args)
            .output()
            .await
            .with_context(|| {
                "httpx not found. Install it: go install github.com/projectdiscovery/httpx/cmd/httpx@latest"
                    .to_string()
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("httpx exited with error: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let lines: Vec<String> = stdout
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .collect();

        let mut result = format!("httpx found {} live hosts:\n\n", lines.len());

        for line in &lines {
            result.push_str(line);
            result.push('\n');
        }

        let mut metadata = HashMap::new();
        metadata.insert("count".to_string(), json!(lines.len()));
        metadata.insert("tech_detect".to_string(), json!(params.tech_detect));
        metadata.insert("title".to_string(), json!(params.title));
        metadata.insert("tls".to_string(), json!(params.tls));
        metadata.insert("cdn".to_string(), json!(params.cdn));

        Ok(ToolOutput::new(result)
            .with_title(format!("httpx: {} hosts probed", lines.len()))
            .with_metadata(json!(metadata)))
    }
}

impl HttpxTool {
    pub fn new() -> Self {
        Self
    }
}

fn build_args(params: &HttpxInput) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(ref list) = params.list {
        args.push("-l".to_string());
        args.push(list.clone());
    } else if !params.targets.is_empty() {
        for target in &params.targets {
            args.push("-target".to_string());
            args.push(target.clone());
        }
    }

    if let Some(ref codes) = params.status_codes {
        args.push("-status-code".to_string());
        args.push(codes.clone());
    }

    if params.title {
        args.push("-title".to_string());
    }
    if params.tech_detect {
        args.push("-tech-detect".to_string());
    }
    if params.web_server {
        args.push("-web-server".to_string());
    }
    if params.content_type {
        args.push("-content-type".to_string());
    }
    if params.response_size {
        args.push("-response-size".to_string());
    }
    if params.method {
        args.push("-method".to_string());
    }
    if params.tls {
        args.push("-tls".to_string());
    }
    if params.cdn {
        args.push("-cdn".to_string());
    }
    if params.chains {
        args.push("-chains".to_string());
    }
    if params.follow_redirects {
        args.push("-follow-redirects".to_string());
    }
    if params.no_color {
        args.push("-no-color".to_string());
    }

    let threads = params.threads.unwrap_or(DEFAULT_THREADS);
    args.push("-threads".to_string());
    args.push(threads.to_string());

    let timeout = params.timeout.unwrap_or(DEFAULT_TIMEOUT);
    args.push("-timeout".to_string());
    args.push(timeout.to_string());

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_args_with_list() {
        let input = HttpxInput {
            list: Some("subs.txt".to_string()),
            targets: vec![],
            status_codes: None,
            title: true,
            tech_detect: false,
            web_server: false,
            content_type: false,
            response_size: false,
            method: false,
            tls: false,
            cdn: false,
            chains: false,
            threads: None,
            timeout: None,
            follow_redirects: false,
            no_color: false,
        };
        let args = build_args(&input);
        assert!(args.contains(&"-l".to_string()));
        assert!(args.contains(&"subs.txt".to_string()));
        assert!(args.contains(&"-title".to_string()));
    }
}
