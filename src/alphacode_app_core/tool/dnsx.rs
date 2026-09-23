use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const DEFAULT_THREADS: usize = 100;
const DEFAULT_TIMEOUT: u64 = 5;

pub struct DnsxTool;

#[derive(Deserialize)]
struct DnsxInput {
    #[serde(default)]
    list: Option<String>,
    #[serde(default)]
    targets: Vec<String>,
    #[serde(default)]
    a: bool,
    #[serde(default)]
    aaaa: bool,
    #[serde(default)]
    cname: bool,
    #[serde(default)]
    mx: bool,
    #[serde(default)]
    ns: bool,
    #[serde(default)]
    ptr: bool,
    #[serde(default)]
    soa: bool,
    #[serde(default)]
    txt: bool,
    #[serde(default)]
    srv: bool,
    #[serde(default)]
    resolvers: Option<String>,
    #[serde(default)]
    threads: Option<usize>,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    retry: Option<u32>,
    #[serde(default)]
    resp: bool,
    #[serde(default)]
    json_output: bool,
}

#[async_trait]
impl Tool for DnsxTool {
    fn name(&self) -> &str {
        "dnsx"
    }

    fn description(&self) -> &str {
        "DNS resolution tool. Resolves domains and extracts DNS records including A, AAAA, CNAME, MX, NS, TXT, and more."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": super::intent_schema_property(),
                "targets": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "List of domains to resolve."
                },
                "list": {
                    "type": "string",
                    "description": "Path to file containing domains (one per line)."
                },
                "a": {
                    "type": "boolean",
                    "description": "Resolve A records. Default: false."
                },
                "aaaa": {
                    "type": "boolean",
                    "description": "Resolve AAAA records. Default: false."
                },
                "cname": {
                    "type": "boolean",
                    "description": "Resolve CNAME records. Default: false."
                },
                "mx": {
                    "type": "boolean",
                    "description": "Resolve MX records. Default: false."
                },
                "ns": {
                    "type": "boolean",
                    "description": "Resolve NS records. Default: false."
                },
                "ptr": {
                    "type": "boolean",
                    "description": "Resolve PTR records. Default: false."
                },
                "txt": {
                    "type": "boolean",
                    "description": "Resolve TXT records. Default: false."
                },
                "resolvers": {
                    "type": "string",
                    "description": "Custom DNS resolver list file."
                },
                "threads": {
                    "type": "integer",
                    "description": "Number of concurrent threads. Default: 100."
                },
                "timeout": {
                    "type": "integer",
                    "description": "DNS query timeout in seconds. Default: 5."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: DnsxInput = serde_json::from_value(input)?;
        let args = build_args(&params);

        let output = tokio::process::Command::new("dnsx")
            .args(&args)
            .output()
            .await
            .with_context(|| {
                "dnsx not found. Install it: go install github.com/projectdiscovery/dnsx/cmd/dnsx@latest"
                    .to_string()
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("dnsx exited with error: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let lines: Vec<String> = stdout
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .collect();

        let mut result = format!("dnsx resolved {} domains:\n\n", lines.len());

        for line in &lines {
            result.push_str(line);
            result.push('\n');
        }

        let mut metadata = HashMap::new();
        metadata.insert("count".to_string(), json!(lines.len()));
        metadata.insert("a".to_string(), json!(params.a));
        metadata.insert("aaaa".to_string(), json!(params.aaaa));
        metadata.insert("cname".to_string(), json!(params.cname));
        metadata.insert("txt".to_string(), json!(params.txt));
        metadata.insert(
            "threads".to_string(),
            json!(params.threads.unwrap_or(DEFAULT_THREADS)),
        );

        Ok(ToolOutput::new(result)
            .with_title(format!("dnsx: {} domains resolved", lines.len()))
            .with_metadata(json!(metadata)))
    }
}

impl DnsxTool {
    pub fn new() -> Self {
        Self
    }
}

fn build_args(params: &DnsxInput) -> Vec<String> {
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

    if params.a {
        args.push("-a".to_string());
    }
    if params.aaaa {
        args.push("-aaaa".to_string());
    }
    if params.cname {
        args.push("-cname".to_string());
    }
    if params.mx {
        args.push("-mx".to_string());
    }
    if params.ns {
        args.push("-ns".to_string());
    }
    if params.ptr {
        args.push("-ptr".to_string());
    }
    if params.soa {
        args.push("-soa".to_string());
    }
    if params.txt {
        args.push("-txt".to_string());
    }
    if params.srv {
        args.push("-srv".to_string());
    }

    if let Some(ref resolvers) = params.resolvers {
        args.push("-r".to_string());
        args.push(resolvers.clone());
    }

    let threads = params.threads.unwrap_or(DEFAULT_THREADS);
    args.push("-threads".to_string());
    args.push(threads.to_string());

    let timeout = params.timeout.unwrap_or(DEFAULT_TIMEOUT);
    args.push("-timeout".to_string());
    args.push(timeout.to_string());

    if let Some(retry) = params.retry {
        args.push("-retry".to_string());
        args.push(retry.to_string());
    }

    if params.resp {
        args.push("-resp".to_string());
    }
    if params.json_output {
        args.push("-json".to_string());
    }

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_args_with_list() {
        let input = DnsxInput {
            list: Some("subs.txt".to_string()),
            targets: vec![],
            a: true,
            aaaa: false,
            cname: false,
            mx: false,
            ns: false,
            ptr: false,
            soa: false,
            txt: false,
            srv: false,
            resolvers: None,
            threads: None,
            timeout: None,
            retry: None,
            resp: false,
            json_output: false,
        };
        let args = build_args(&input);
        assert!(args.contains(&"-l".to_string()));
        assert!(args.contains(&"subs.txt".to_string()));
        assert!(args.contains(&"-a".to_string()));
    }
}
