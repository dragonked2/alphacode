use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const DEFAULT_LIMIT: usize = 1000;

pub struct WaybackurlsTool;

#[derive(Deserialize)]
struct WaybackurlsInput {
    domain: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    no_color: bool,
}

#[async_trait]
impl Tool for WaybackurlsTool {
    fn name(&self) -> &str {
        "waybackurls"
    }

    fn description(&self) -> &str {
        "Fetch all known URLs for a domain from the Wayback Machine. Great for discovering historical endpoints, hidden parameters, and old versions of files."
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
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of URLs to return. Default: 1000."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Request timeout in seconds. Default: 60."
                },
                "no_color": {
                    "type": "boolean",
                    "description": "Suppress color codes in output. Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: WaybackurlsInput = serde_json::from_value(input)?;
        let args = build_args(&params);

        let output = tokio::process::Command::new("waybackurls")
            .args(&args)
            .output()
            .await
            .with_context(|| {
                "waybackurls not found. Install it: go install github.com/tomnomnom/waybackurls@latest"
                    .to_string()
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("waybackurls exited with error: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let urls: Vec<String> = stdout
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .collect();

        let mut result = format!(
            "waybackurls found {} URLs for {}:\n\n",
            urls.len(),
            params.domain
        );

        for url in &urls {
            result.push_str(url);
            result.push('\n');
        }

        let mut metadata = HashMap::new();
        metadata.insert("domain".to_string(), json!(params.domain));
        metadata.insert("count".to_string(), json!(urls.len()));
        metadata.insert(
            "limit".to_string(),
            json!(params.limit.unwrap_or(DEFAULT_LIMIT)),
        );

        Ok(ToolOutput::new(result)
            .with_title(format!("waybackurls: {} URLs found", urls.len()))
            .with_metadata(json!(metadata)))
    }
}

impl WaybackurlsTool {
    pub fn new() -> Self {
        Self
    }
}

fn build_args(params: &WaybackurlsInput) -> Vec<String> {
    let mut args = Vec::new();
    args.push(params.domain.clone());

    if let Some(limit) = params.limit {
        args.push("-limit".to_string());
        args.push(limit.to_string());
    }

    if params.no_color {
        args.push("-no-color".to_string());
    }

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_args_basic() {
        let input = WaybackurlsInput {
            domain: "example.com".to_string(),
            limit: None,
            no_color: false,
        };
        let args = build_args(&input);
        assert!(args.contains(&"example.com".to_string()));
    }
}
