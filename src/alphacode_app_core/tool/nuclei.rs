use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const DEFAULT_THREADS: usize = 50;
const DEFAULT_RATE_LIMIT: u64 = 150;

pub struct NucleiTool;

#[derive(Deserialize)]
struct NucleiInput {
    /// Target URL(s) or file path
    #[serde(default)]
    target: Vec<String>,
    #[serde(default)]
    list: Option<String>,
    /// Template IDs or tags to run
    #[serde(default)]
    tags: Option<String>,
    #[serde(default)]
    templates: Option<String>,
    /// Severity filter (critical, high, medium, low, info)
    #[serde(default)]
    severity: Option<String>,
    /// Exclude templates
    #[serde(default)]
    exclude: Option<String>,
    /// Follow redirects
    #[serde(default)]
    follow_redirects: bool,
    /// Max time per request
    #[serde(default)]
    #[allow(dead_code)]
    timeout: Option<u64>,
    /// Rate limit per second
    #[serde(default)]
    rate_limit: Option<u64>,
    /// Threads
    #[serde(default)]
    threads: Option<usize>,
    /// JSON output
    #[serde(default)]
    json: bool,
    /// Silent mode
    #[serde(default)]
    silent: bool,
    /// Extra headers
    #[serde(default)]
    headers: Vec<String>,
}

#[async_trait]
impl Tool for NucleiTool {
    fn name(&self) -> &str {
        "nuclei"
    }

    fn description(&self) -> &str {
        "Template-driven vulnerability scanner. Use AFTER subdomain enumeration and port scanning to find known vulnerabilities on live targets. Chains well after httpx."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": super::intent_schema_property(),
                "target": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Target URLs to scan."
                },
                "list": {
                    "type": "string",
                    "description": "Path to file with target URLs (one per line)."
                },
                "tags": {
                    "type": "string",
                    "description": "Template tags to run (e.g., 'cve,exposure,misconfig')."
                },
                "severity": {
                    "type": "string",
                    "description": "Filter by severity (critical,high,medium,low,info)."
                },
                "follow_redirects": {
                    "type": "boolean",
                    "description": "Follow HTTP redirects. Default: false."
                },
                "rate_limit": {
                    "type": "integer",
                    "description": "Max requests per second. Default: 150."
                },
                "threads": {
                    "type": "integer",
                    "description": "Concurrent threads. Default: 50."
                },
                "silent": {
                    "type": "boolean",
                    "description": "Silent mode (only findings). Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: NucleiInput = normalize_nuclei_input(&input)?;
        if params.target.is_empty() && params.list.is_none() {
            return Err(anyhow::anyhow!(
                "nuclei needs targets: provide `target` (array of URLs) or `list` \
                 (path to a file with one target per line), e.g., \
                 {{\"target\": [\"https://example.com\"], \"severity\": \"critical,high\"}}"
            ));
        }
        let args = build_args(&params)?;

        let output = super::recon_common::run_bounded(
            "nuclei",
            &args,
            super::recon_common::DEFAULT_TOOL_TIMEOUT,
        )
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. {}", super::recon_common::install_hint("nuclei"))
            } else {
                anyhow::anyhow!("{e}")
            }
        })?;

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if !output.status.success() {
            let detail = if stderr.is_empty() {
                if stdout.is_empty() {
                    "no output (binary exited non-zero with empty stderr; \
                     check that the targets are reachable and the nuclei binary is \
                     ProjectDiscovery's nuclei)"
                        .to_string()
                } else {
                    crate::alphacode_core::util::truncate_str(&stdout, 500).to_string()
                }
            } else {
                crate::alphacode_core::util::truncate_str(&stderr, 500).to_string()
            };
            return Err(anyhow::anyhow!("nuclei exited with error: {detail}"));
        }

        let (lines, total, truncated) = super::recon_common::parse_lines(&output.stdout);

        let mut result = format!("nuclei found {} findings:\n\n", total);
        for line in &lines {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str(&super::recon_common::truncation_notice(lines.len(), total));

        let mut metadata = HashMap::new();
        metadata.insert("count".to_string(), json!(lines.len()));
        metadata.insert("total_findings".to_string(), json!(total));
        metadata.insert("truncated".to_string(), json!(truncated));
        if let Some(ref sev) = params.severity {
            metadata.insert("severity_filter".to_string(), json!(sev));
        }

        Ok(ToolOutput::new(result)
            .with_title(format!("nuclei: {total} findings"))
            .with_metadata(json!(metadata)))
    }
}

impl NucleiTool {
    pub fn new() -> Self {
        Self
    }
}

fn normalize_nuclei_input(input: &Value) -> Result<NucleiInput> {
    let mut params: NucleiInput = serde_json::from_value(input.clone()).unwrap_or(NucleiInput {
        target: Vec::new(),
        list: None,
        tags: None,
        templates: None,
        severity: None,
        exclude: None,
        follow_redirects: false,
        timeout: None,
        rate_limit: None,
        threads: None,
        json: false,
        silent: false,
        headers: Vec::new(),
    });
    if !params.target.is_empty() {
        return Ok(params);
    }
    let obj = match input.as_object() {
        Some(obj) => obj,
        None => return Ok(params),
    };
    let mut extra: Vec<String> = Vec::new();
    for key in ["target", "targets", "url", "urls", "u"] {
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
    params.target.extend(extra);
    Ok(params)
}

fn build_args(params: &NucleiInput) -> Result<Vec<String>> {
    let mut args = Vec::new();

    if let Some(ref list) = params.list {
        args.push("-l".to_string());
        args.push(super::recon_common::validate_file_arg(list, "list")?);
    } else if !params.target.is_empty() {
        for target in &params.target {
            args.push("-u".to_string());
            args.push(super::recon_common::validate_target(target)?);
        }
    }

    if let Some(ref tags) = params.tags {
        args.push("-t".to_string());
        args.push(tags.clone());
    }
    if let Some(ref templates) = params.templates {
        args.push("-t".to_string());
        args.push(templates.clone());
    }
    if let Some(ref severity) = params.severity {
        args.push("-severity".to_string());
        args.push(severity.clone());
    }
    if let Some(ref exclude) = params.exclude {
        args.push("-exclude".to_string());
        args.push(exclude.clone());
    }
    if params.follow_redirects {
        args.push("-follow-redirects".to_string());
    }
    if params.silent {
        args.push("-silent".to_string());
    }
    if params.json {
        args.push("-json".to_string());
    }

    let rate_limit = params.rate_limit.unwrap_or(DEFAULT_RATE_LIMIT);
    args.push("-rl".to_string());
    args.push(rate_limit.to_string());

    let threads = params.threads.unwrap_or(DEFAULT_THREADS);
    args.push("-c".to_string());
    args.push(threads.to_string());

    super::recon_common::append_default_user_agent_header(&mut args, &params.headers);
    for header in &params.headers {
        args.push("-H".to_string());
        args.push(header.clone());
    }

    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_accepts_aliases() {
        for payload in [
            serde_json::json!({"target": "https://example.com"}),
            serde_json::json!({"targets": "https://example.com"}),
        ] {
            let params = normalize_nuclei_input(&payload).expect("normalize");
            assert!(!params.target.is_empty());
        }
    }

    #[test]
    fn test_build_args_basic() {
        let input = NucleiInput {
            target: vec!["https://example.com".to_string()],
            list: None,
            tags: None,
            templates: None,
            severity: None,
            exclude: None,
            follow_redirects: false,
            timeout: None,
            rate_limit: None,
            threads: None,
            json: false,
            silent: false,
            headers: Vec::new(),
        };
        let args = build_args(&input).expect("args");
        assert!(args.contains(&"-u".to_string()));
        assert!(args.contains(&"https://example.com".to_string()));
    }
}
