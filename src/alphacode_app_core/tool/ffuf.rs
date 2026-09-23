use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const DEFAULT_TIMEOUT: u64 = 40;
const DEFAULT_THREADS: usize = 40;

pub struct FfufTool;

#[derive(Deserialize)]
struct FfufInput {
    url: String,
    #[serde(default)]
    wordlist: Option<String>,
    #[serde(default)]
    extensions: Option<String>,
    #[serde(default)]
    threads: Option<usize>,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    rate_limit: Option<u64>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    headers: Option<Vec<String>>,
    #[serde(default)]
    no_color: bool,
}

#[async_trait]
impl Tool for FfufTool {
    fn name(&self) -> &str {
        "ffuf"
    }

    fn description(&self) -> &str {
        "Fast web fuzzer for directory and parameter discovery. Brute-forces paths, parameters, and virtual hosts against web servers."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "intent": super::intent_schema_property(),
                "url": {
                    "type": "string",
                    "description": "Target URL with FUZZ placeholder (e.g., 'https://target/FUZZ')."
                },
                "wordlist": {
                    "type": "string",
                    "description": "Path to wordlist file. Default: common.txt."
                },
                "extensions": {
                    "type": "string",
                    "description": "File extensions to try (e.g., 'php,html,bak')."
                },
                "threads": {
                    "type": "integer",
                    "description": "Number of concurrent threads. Default: 40."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Request timeout in seconds. Default: 40."
                },
                "rate_limit": {
                    "type": "integer",
                    "description": "Requests per second limit. 0 = no limit. Default: 0."
                },
                "method": {
                    "type": "string",
                    "description": "HTTP method (GET, POST, etc.). Default: GET."
                },
                "data": {
                    "type": "string",
                    "description": "POST body data."
                },
                "headers": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Custom headers (e.g., ['Authorization: Bearer token'])."
                },
                "no_color": {
                    "type": "boolean",
                    "description": "Suppress color codes. Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: FfufInput = serde_json::from_value(input)?;
        let args = build_args(&params);

        let output = tokio::process::Command::new("ffuf")
            .args(&args)
            .output()
            .await
            .with_context(|| {
                "ffuf not found. Install it: go install github.com/ffuf/ffuf/v2@latest".to_string()
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("ffuf exited with error: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();

        let mut metadata = HashMap::new();
        metadata.insert("url".to_string(), json!(params.url));
        metadata.insert(
            "thread_count".to_string(),
            json!(params.threads.unwrap_or(DEFAULT_THREADS)),
        );

        Ok(ToolOutput::new(stdout)
            .with_title(format!("ffuf: fuzzing {}", params.url))
            .with_metadata(json!(metadata)))
    }
}

impl FfufTool {
    pub fn new() -> Self {
        Self
    }
}

fn build_args(params: &FfufInput) -> Vec<String> {
    let mut args = Vec::new();
    args.push("-u".to_string());
    args.push(params.url.clone());

    if let Some(ref wordlist) = params.wordlist {
        args.push("-w".to_string());
        args.push(wordlist.clone());
    } else {
        args.push("-w".to_string());
        args.push("common.txt".to_string());
    }

    if let Some(ref ext) = params.extensions {
        args.push("-e".to_string());
        args.push(ext.clone());
    }

    let threads = params.threads.unwrap_or(DEFAULT_THREADS);
    args.push("-t".to_string());
    args.push(threads.to_string());

    let timeout = params.timeout.unwrap_or(DEFAULT_TIMEOUT);
    args.push("-timeout".to_string());
    args.push(timeout.to_string());

    if let Some(rate) = params.rate_limit
        && rate > 0
    {
        args.push("-rate".to_string());
        args.push(rate.to_string());
    }

    if let Some(ref method) = params.method {
        args.push("-X".to_string());
        args.push(method.clone());
    }

    if let Some(ref data) = params.data {
        args.push("-d".to_string());
        args.push(data.clone());
    }

    if let Some(ref headers) = params.headers {
        for header in headers {
            args.push("-H".to_string());
            args.push(header.clone());
        }
    }

    if params.no_color {
        args.push("-nc".to_string());
    }

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_args_basic() {
        let input = FfufInput {
            url: "https://example.com/FUZZ".to_string(),
            wordlist: None,
            extensions: None,
            threads: None,
            timeout: None,
            rate_limit: None,
            method: None,
            data: None,
            headers: None,
            no_color: false,
        };
        let args = build_args(&input);
        assert!(args.contains(&"-u".to_string()));
        assert!(args.contains(&"https://example.com/FUZZ".to_string()));
        assert!(args.contains(&"-w".to_string()));
        assert!(args.contains(&"common.txt".to_string()));
    }
}
