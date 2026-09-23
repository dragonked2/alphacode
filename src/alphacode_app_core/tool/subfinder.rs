use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const DEFAULT_THREADS: usize = 20;

pub struct SubfinderTool;

#[derive(Deserialize)]
struct SubfinderInput {
    domain: String,
    #[serde(default)]
    all: bool,
    #[serde(default)]
    threads: Option<usize>,
    #[serde(default)]
    verbose: bool,
}

#[async_trait]
impl Tool for SubfinderTool {
    fn name(&self) -> &str {
        "subfinder"
    }

    fn description(&self) -> &str {
        "Passive subdomain enumeration using subfinder. Discovers subdomains via multiple sources including crt.sh, virustotal, and others."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["domain"],
            "properties": {
                "intent": super::intent_schema_property(),
                "domain": {
                    "type": "string",
                    "description": "Target domain to enumerate subdomains for."
                },
                "all": {
                    "type": "boolean",
                    "description": "Use all sources (slower but more comprehensive). Default: false."
                },
                "threads": {
                    "type": "integer",
                    "description": "Number of concurrent threads. Default: 20."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds per source. Default: 120."
                },
                "silent": {
                    "type": "boolean",
                    "description": "Suppress output except domains. Default: false."
                },
                "verbose": {
                    "type": "boolean",
                    "description": "Show verbose output. Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: SubfinderInput = serde_json::from_value(input)?;
        let args = build_args(&params);

        let output = tokio::process::Command::new("subfinder")
            .args(&args)
            .output()
            .await
            .with_context(|| {
                "subfinder not found or failed to execute. Install it: go install github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest"
                    .to_string()
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("subfinder exited with error: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let domains: Vec<String> = stdout
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .collect();

        let mut result = format!(
            "Subfinder found {} subdomains for {}:\n\n",
            domains.len(),
            params.domain
        );

        for domain in &domains {
            result.push_str(domain);
            result.push('\n');
        }

        let mut metadata = HashMap::new();
        metadata.insert("domain".to_string(), json!(params.domain));
        metadata.insert("count".to_string(), json!(domains.len()));
        metadata.insert("all_sources".to_string(), json!(params.all));

        Ok(ToolOutput::new(result)
            .with_title(format!("subfinder: {} domains found", domains.len()))
            .with_metadata(json!(metadata)))
    }
}

impl SubfinderTool {
    pub fn new() -> Self {
        Self
    }
}

fn build_args(params: &SubfinderInput) -> Vec<String> {
    let mut args = Vec::new();
    args.push("-d".to_string());
    args.push(params.domain.clone());

    if params.all {
        args.push("-all".to_string());
    }

    let threads = params.threads.unwrap_or(DEFAULT_THREADS);
    args.push("-threads".to_string());
    args.push(threads.to_string());
    args.push("-silent".to_string());

    if params.verbose {
        args.push("-v".to_string());
    }

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_args_basic() {
        let input = SubfinderInput {
            domain: "example.com".to_string(),
            all: false,
            threads: None,
            verbose: false,
        };
        let args = build_args(&input);
        assert!(args.contains(&"-d".to_string()));
        assert!(args.contains(&"example.com".to_string()));
    }

    #[test]
    fn test_build_args_all() {
        let input = SubfinderInput {
            domain: "example.com".to_string(),
            all: true,
            threads: None,
            verbose: false,
        };
        let args = build_args(&input);
        assert!(args.contains(&"-all".to_string()));
    }
}
