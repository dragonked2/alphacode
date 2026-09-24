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
        "Passive subdomain enumeration using subfinder. Use ONLY when task is organization/domain-scope enumeration (user gave a base domain/org and asks to discover assets). Do NOT use for single-service work where user gave exactly one URL to test directly — there is nothing to enumerate."
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
        let params: SubfinderInput = normalize_subfinder_input(&input)?;
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
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // Prefer stderr, fall back to stdout: some builds report the
            // failure on stdout, and an empty message leaves the model with
            // nothing to adapt to (it then repeats the call until refused).
            let detail = if !stderr.is_empty() {
                crate::alphacode_core::util::truncate_str(&stderr, 500).to_string()
            } else if !stdout.is_empty() {
                crate::alphacode_core::util::truncate_str(&stdout, 500).to_string()
            } else {
                format!(
                    "no output for domain '{}' (exit non-zero with empty stderr; \
                     the target may have no passive sources, the network may block \
                     source APIs, or provider keys may be missing — try `\"all\": true` \
                     or verify the domain resolves)",
                    params.domain
                )
            };
            return Err(anyhow::anyhow!("subfinder exited with error: {detail}"));
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

/// Accept the key spellings models actually send (`target`, `host`, `d`)
/// in addition to the canonical `domain`, so a wrong key name becomes a
/// working call instead of a `missing field` failure loop.
fn normalize_subfinder_input(input: &Value) -> Result<SubfinderInput> {
    if let Ok(params) = serde_json::from_value::<SubfinderInput>(input.clone()) {
        return Ok(params);
    }
    let obj = input.as_object().ok_or_else(|| {
        anyhow::anyhow!(
            "subfinder expects a JSON object with `domain`, e.g. \
             {{\"domain\": \"example.com\"}}"
        )
    })?;
    let domain = ["domain", "target", "host", "domain_name", "d"]
        .iter()
        .find_map(|k| obj.get(*k).and_then(|v| v.as_str()))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            let mut keys: Vec<&String> = obj.keys().collect();
            keys.sort();
            let keys = keys
                .iter()
                .map(|k| format!("`{k}`"))
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::anyhow!(
                "missing field `domain`. Received keys: {keys}. \
                 Provide the base domain as `domain`, e.g. \
                 {{\"domain\": \"example.com\"}}"
            )
        })?;
    Ok(SubfinderInput {
        domain: domain.to_string(),
        all: obj.get("all").and_then(|v| v.as_bool()).unwrap_or(false),
        threads: obj
            .get("threads")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize),
        verbose: obj
            .get("verbose")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    })
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
    fn test_normalize_accepts_domain_aliases() {
        for payload in [
            serde_json::json!({"target": "example.com"}),
            serde_json::json!({"host": "example.com"}),
        ] {
            let params = normalize_subfinder_input(&payload).expect("normalize");
            assert_eq!(params.domain, "example.com");
        }
        assert!(normalize_subfinder_input(&serde_json::json!({})).is_err());
    }

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
