use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const DEFAULT_DEPTH: usize = 3;
const DEFAULT_THREADS: usize = 50;

pub struct KatanaTool;

#[derive(Deserialize)]
struct KatanaInput {
    url: String,
    #[serde(default)]
    depth: Option<usize>,
    #[serde(default)]
    threads: Option<usize>,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    json_output: bool,
    #[serde(default)]
    no_color: bool,
    #[serde(default)]
    no_remote: bool,
    #[serde(default)]
    no_store: bool,
    #[serde(default)]
    f: Option<String>,
    #[serde(default)]
    d: Option<String>,
    #[serde(default)]
    robots: bool,
    #[serde(default)]
    headers: bool,
    #[serde(default)]
    include_body: bool,
    #[serde(default)]
    include_params: bool,
}

#[async_trait]
impl Tool for KatanaTool {
    fn name(&self) -> &str {
        "katana"
    }

    fn description(&self) -> &str {
        "Fast, passive web crawler. Use AFTER direct-test triage (webfetch homepage + headers + fingerprint + visible params) has produced a crawl hypothesis, or for organization-scope mapping. Do NOT use as the first step on single-service work — fetch the homepage directly first."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "intent": super::intent_schema_property(),
                "url": {
                    "type": "string",
                    "description": "Target URL to crawl."
                },
                "depth": {
                    "type": "integer",
                    "description": "Maximum crawl depth. Default: 3."
                },
                "threads": {
                    "type": "integer",
                    "description": "Number of concurrent threads. Default: 50."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Request timeout in seconds. Default: 60."
                },
                "json_output": {
                    "type": "boolean",
                    "description": "Output in JSON format. Default: false."
                },
                "no_remote": {
                    "type": "boolean",
                    "description": "Skip remote content discovery. Default: false."
                },
                "no_store": {
                    "type": "boolean",
                    "description": "Do not store results. Default: false."
                },
                "f": {
                    "type": "string",
                    "description": "Field selector (e.g., 'u', 'd', 'r', 'ru', 'rd', 'ri', 'm', 'rdi', 'f')"
                },
                "d": {
                    "type": "string",
                    "description": "Filter domains to include."
                },
                "robots": {
                    "type": "boolean",
                    "description": "Respect robots.txt. Default: false."
                },
                "headers": {
                    "type": "boolean",
                    "description": "Include response headers in output. Default: false."
                },
                "include_body": {
                    "type": "boolean",
                    "description": "Include response body. Default: false."
                },
                "include_params": {
                    "type": "boolean",
                    "description": "Include URL parameters. Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: KatanaInput = serde_json::from_value(input)?;
        let args = build_args(&params);

        let output = tokio::process::Command::new("katana")
            .args(&args)
            .output()
            .await
            .with_context(|| {
                "katana not found. Install it: go install github.com/projectdiscovery/katana/cmd/katana@latest"
                    .to_string()
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("katana exited with error: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let urls: Vec<String> = stdout
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .collect();

        let mut result = format!("katana found {} URLs from {}:\n\n", urls.len(), params.url);

        for url in &urls {
            result.push_str(url);
            result.push('\n');
        }

        let mut metadata = HashMap::new();
        metadata.insert("url".to_string(), json!(params.url));
        metadata.insert("count".to_string(), json!(urls.len()));
        metadata.insert(
            "depth".to_string(),
            json!(params.depth.unwrap_or(DEFAULT_DEPTH)),
        );
        metadata.insert(
            "thread_count".to_string(),
            json!(params.threads.unwrap_or(DEFAULT_THREADS)),
        );

        Ok(ToolOutput::new(result)
            .with_title(format!("katana: {} URLs crawled", urls.len()))
            .with_metadata(json!(metadata)))
    }
}

impl KatanaTool {
    pub fn new() -> Self {
        Self
    }
}

fn build_args(params: &KatanaInput) -> Vec<String> {
    let mut args = Vec::new();
    args.push("-u".to_string());
    args.push(params.url.clone());

    if let Some(depth) = params.depth {
        args.push("-d".to_string());
        args.push(depth.to_string());
    }

    if let Some(threads) = params.threads {
        args.push("-threads".to_string());
        args.push(threads.to_string());
    }

    if let Some(timeout) = params.timeout {
        args.push("-timeout".to_string());
        args.push(timeout.to_string());
    }

    if params.json_output {
        args.push("-j".to_string());
    }
    if params.no_color {
        args.push("-nc".to_string());
    }
    if params.no_remote {
        args.push("-no-remote".to_string());
    }
    if params.no_store {
        args.push("-no-store".to_string());
    }
    if let Some(ref f) = params.f {
        args.push("-f".to_string());
        args.push(f.clone());
    }
    if let Some(ref d) = params.d {
        args.push("-d".to_string());
        args.push(d.clone());
    }
    if params.robots {
        args.push("-robots".to_string());
    }
    if params.headers {
        args.push("-headers".to_string());
    }
    if params.include_body {
        args.push("-include-body".to_string());
    }
    if params.include_params {
        args.push("-include-params".to_string());
    }

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_args_basic() {
        let input = KatanaInput {
            url: "https://example.com".to_string(),
            depth: None,
            threads: None,
            timeout: None,
            json_output: false,
            no_color: false,
            no_remote: false,
            no_store: false,
            f: None,
            d: None,
            robots: false,
            headers: false,
            include_body: false,
            include_params: false,
        };
        let args = build_args(&input);
        assert!(args.contains(&"-u".to_string()));
        assert!(args.contains(&"https://example.com".to_string()));
    }
}
