use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

pub struct GauTool;

#[derive(Deserialize)]
struct GauInput {
    domain: String,
    #[serde(default)]
    threads: Option<usize>,
    #[serde(default)]
    no_color: bool,
    #[serde(default)]
    o: Option<bool>,
    #[serde(default)]
    p: Option<bool>,
    #[serde(default)]
    q: Option<bool>,
    #[serde(default)]
    s: Option<bool>,
    #[serde(default)]
    t: Option<u64>,
}

#[async_trait]
impl Tool for GauTool {
    fn name(&self) -> &str {
        "gau"
    }

    fn description(&self) -> &str {
        "Get All URLs (gau) fetches known URLs for a domain from multiple sources including AlienVault OTX, CommonCrawl, URLScan, and Wayback Machine."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["domain"],
            "properties": {
                "intent": super::intent_schema_property(),
                "domain": {
                    "type": "string",
                    "description": "Target domain to fetch URLs for."
                },
                "threads": {
                    "type": "integer",
                    "description": "Number of concurrent threads. Default: 1."
                },
                "no_color": {
                    "type": "boolean",
                    "description": "Suppress color codes. Default: false."
                },
                "o": {
                    "type": "boolean",
                    "description": "Only show unique hosts. Default: false."
                },
                "p": {
                    "type": "boolean",
                    "description": "Show only paths. Default: false."
                },
                "q": {
                    "type": "boolean",
                    "description": "Show only query strings. Default: false."
                },
                "s": {
                    "type": "boolean",
                    "description": "Match only on subdomains. Default: false."
                },
                "t": {
                    "type": "integer",
                    "description": "Time limit in seconds. Default: 30."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: GauInput = serde_json::from_value(input)?;
        let args = build_args(&params);

        let output = tokio::process::Command::new("gau")
            .args(&args)
            .output()
            .await
            .with_context(|| {
                "gau not found. Install it: go install github.com/lc/gau/v2/cmd/gau@latest"
                    .to_string()
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("gau exited with error: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let urls: Vec<String> = stdout
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .collect();

        let mut result = format!("gau found {} URLs for {}:\n\n", urls.len(), params.domain);

        for url in &urls {
            result.push_str(url);
            result.push('\n');
        }

        let mut metadata = HashMap::new();
        metadata.insert("domain".to_string(), json!(params.domain));
        metadata.insert("count".to_string(), json!(urls.len()));
        metadata.insert("threads".to_string(), json!(params.threads.unwrap_or(1)));

        Ok(ToolOutput::new(result)
            .with_title(format!("gau: {} URLs found", urls.len()))
            .with_metadata(json!(metadata)))
    }
}

impl GauTool {
    pub fn new() -> Self {
        Self
    }
}

fn build_args(params: &GauInput) -> Vec<String> {
    let mut args = Vec::new();
    args.push(params.domain.clone());

    if let Some(threads) = params.threads {
        args.push("-threads".to_string());
        args.push(threads.to_string());
    }

    if params.no_color {
        args.push("-nc".to_string());
    }
    if params.o.unwrap_or(false) {
        args.push("-o".to_string());
    }
    if params.p.unwrap_or(false) {
        args.push("-p".to_string());
    }
    if params.q.unwrap_or(false) {
        args.push("-q".to_string());
    }
    if params.s.unwrap_or(false) {
        args.push("-s".to_string());
    }
    if let Some(t) = params.t {
        args.push("-t".to_string());
        args.push(t.to_string());
    }

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_args_basic() {
        let input = GauInput {
            domain: "example.com".to_string(),
            threads: None,
            no_color: false,
            o: None,
            p: None,
            q: None,
            s: None,
            t: None,
        };
        let args = build_args(&input);
        assert!(args.contains(&"example.com".to_string()));
    }
}
