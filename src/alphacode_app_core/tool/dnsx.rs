use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
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
        let args = build_args(&params)?;

        let output = super::recon_common::run_bounded(
            "dnsx",
            &args,
            super::recon_common::DEFAULT_TOOL_TIMEOUT,
        )
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. {}", super::recon_common::install_hint("dnsx"))
            } else {
                anyhow::anyhow!("{e}")
            }
        })?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "{}",
                super::recon_common::describe_failure("dnsx", &output)
            ));
        }

        let (lines, total, truncated) = super::recon_common::parse_lines(&output.stdout);

        let mut result = format!("dnsx resolved {} domains:\n\n", total);

        for line in &lines {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str(&super::recon_common::truncation_notice(lines.len(), total));

        // dnsx reports per-source resolution problems on stderr while still
        // exiting 0. Discarding stderr on success made a total failure look
        // like a clean "resolved 0 domains" result.
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            result.push_str(&format!(
                "\n[dnsx diagnostics: {}]",
                crate::alphacode_core::util::truncate_str(stderr.trim(), 500)
            ));
        }

        let mut metadata = HashMap::new();
        metadata.insert("count".to_string(), json!(lines.len()));
        metadata.insert("total_found".to_string(), json!(total));
        metadata.insert("truncated".to_string(), json!(truncated));
        metadata.insert("a".to_string(), json!(params.a));
        metadata.insert("aaaa".to_string(), json!(params.aaaa));
        metadata.insert("cname".to_string(), json!(params.cname));
        metadata.insert("txt".to_string(), json!(params.txt));
        metadata.insert(
            "threads".to_string(),
            json!(params.threads.unwrap_or(DEFAULT_THREADS)),
        );

        Ok(ToolOutput::new(result)
            .with_title(format!("dnsx: {total} domains resolved"))
            .with_metadata(json!(metadata)))
    }
}

impl DnsxTool {
    pub fn new() -> Self {
        Self
    }
}

fn build_args(params: &DnsxInput) -> Result<Vec<String>> {
    let mut args = Vec::new();

    if let Some(ref list) = params.list {
        args.push("-l".to_string());
        args.push(super::recon_common::validate_file_arg(list, "list")?);
    } else if !params.targets.is_empty() {
        // -d starts wordlist-based brute forcing and requires -w. Ordinary
        // explicit targets belong to -l, which also accepts comma-separated hosts.
        let mut joined = String::new();
        for (i, target) in params.targets.iter().enumerate() {
            let t = super::recon_common::validate_target(target)?;
            if i > 0 {
                joined.push(',');
            }
            joined.push_str(&t);
        }
        args.push("-l".to_string());
        args.push(joined);
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

    Ok(args)
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
        let args = build_args(&input).expect("build_args");
        assert!(args.contains(&"-l".to_string()));
        assert!(args.contains(&"subs.txt".to_string()));
        assert!(args.contains(&"-a".to_string()));
    }

    /// -d requires a brute-force wordlist; explicit hosts use -l instead.
    #[test]
    fn targets_use_list_flag_without_requiring_a_wordlist() {
        let input = DnsxInput {
            list: None,
            targets: vec!["example.com".to_string(), "test.example.com".to_string()],
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
        let args = build_args(&input).expect("build_args");
        assert!(
            !args.contains(&"-target".to_string()),
            "-target is not a dnsx flag: {args:?}"
        );
        assert!(
            !args.contains(&"-d".to_string()),
            "-d requires -w: {args:?}"
        );
        let d = args.iter().position(|a| a == "-l").expect("-l present");
        assert_eq!(args[d + 1], "example.com,test.example.com");
    }

    #[test]
    fn flag_like_target_is_rejected() {
        let base = DnsxInput {
            list: None,
            targets: vec!["-json".to_string()],
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
        assert!(build_args(&base).is_err());
    }
}
