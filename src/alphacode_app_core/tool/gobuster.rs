use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const DEFAULT_THREADS: usize = 50;
#[allow(dead_code)]
const DEFAULT_WORDLIST: &str = "/usr/share/wordlists/dirb/common.txt";

pub struct GobusterTool;

#[derive(Deserialize)]
struct GobusterInput {
    /// Target URL
    url: String,
    /// Mode: dir, dns, vhost, fuzz
    #[serde(default = "default_mode")]
    mode: String,
    /// Wordlist path
    #[serde(default)]
    wordlist: Option<String>,
    /// Threads
    #[serde(default)]
    threads: Option<usize>,
    /// Status codes to match
    #[serde(default)]
    status_codes: Option<String>,
    /// Exclude status codes
    #[serde(default)]
    exclude_status: Option<String>,
    /// Follow redirects
    #[serde(default)]
    follow_redirects: bool,
    /// Timeout seconds
    #[serde(default)]
    #[allow(dead_code)]
    timeout: Option<u64>,
    /// Extra headers
    #[serde(default)]
    headers: Vec<String>,
    /// Cookies
    #[serde(default)]
    cookies: Option<String>,
    /// User agent
    #[serde(default)]
    user_agent: Option<String>,
    /// Wildcard detection
    #[serde(default)]
    wildcard: bool,
}

fn default_mode() -> String {
    "dir".to_string()
}

#[async_trait]
impl Tool for GobusterTool {
    fn name(&self) -> &str {
        "gobuster"
    }

    fn description(&self) -> &str {
        "Directory, DNS, and VHost brute-forcer. Use AFTER port scanning to discover hidden content on web servers."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "intent": super::intent_schema_property(),
                "url": {
                    "type": "string",
                    "description": "Target URL (e.g., 'https://example.com')."
                },
                "mode": {
                    "type": "string",
                    "enum": ["dir", "dns", "vhost", "fuzz"],
                    "description": "Scan mode. Default: 'dir'."
                },
                "wordlist": {
                    "type": "string",
                    "description": "Path to wordlist file."
                },
                "status_codes": {
                    "type": "string",
                    "description": "Status codes to match (e.g., '200,204,301,302,307,401,403')."
                },
                "threads": {
                    "type": "integer",
                    "description": "Concurrent threads. Default: 50."
                },
                "follow_redirects": {
                    "type": "boolean",
                    "description": "Follow redirects. Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: GobusterInput = normalize_gobuster_input(&input)?;
        let args = build_args(&params)?;

        let output = super::recon_common::run_bounded(
            "gobuster",
            &args,
            super::recon_common::DEFAULT_TOOL_TIMEOUT,
        )
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. {}", super::recon_common::install_hint("gobuster"))
            } else {
                anyhow::anyhow!("{e}")
            }
        })?;

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if !output.status.success() {
            let detail = if stderr.is_empty() {
                if stdout.is_empty() {
                    "no output (binary exited non-zero with empty stderr)".to_string()
                } else {
                    crate::alphacode_core::util::truncate_str(&stdout, 500).to_string()
                }
            } else {
                crate::alphacode_core::util::truncate_str(&stderr, 500).to_string()
            };
            return Err(anyhow::anyhow!("gobuster exited with error: {detail}"));
        }

        let (lines, total, truncated) = super::recon_common::parse_lines(&output.stdout);

        let mut result = format!("gobuster found {} results:\n\n", total);
        for line in &lines {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str(&super::recon_common::truncation_notice(lines.len(), total));

        let mut metadata = HashMap::new();
        metadata.insert("url".to_string(), json!(params.url));
        metadata.insert("mode".to_string(), json!(params.mode));
        metadata.insert("count".to_string(), json!(lines.len()));
        metadata.insert("total_found".to_string(), json!(total));
        metadata.insert("truncated".to_string(), json!(truncated));

        Ok(ToolOutput::new(result)
            .with_title(format!("gobuster: {total} results"))
            .with_metadata(json!(metadata)))
    }
}

impl GobusterTool {
    pub fn new() -> Self {
        Self
    }
}

fn normalize_gobuster_input(input: &Value) -> Result<GobusterInput> {
    if let Ok(params) = serde_json::from_value::<GobusterInput>(input.clone()) {
        return Ok(params);
    }
    // Aliases, bare-URL strings and truncated payloads all resolve through the shared
    // coercion ladder; a hand-rolled key list missed every other shape.
    let url = super::coerce_url_arg(input, "gobuster")?;
    // Remaining options are optional. A payload that was a bare URL string has no
    // object to read them from, so an empty map stands in for the defaults.
    let obj = input.as_object().cloned().unwrap_or_default();
    Ok(GobusterInput {
        url: url.to_string(),
        mode: obj
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("dir")
            .to_string(),
        wordlist: obj
            .get("wordlist")
            .and_then(|v| v.as_str())
            .map(String::from),
        threads: obj
            .get("threads")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize),
        status_codes: obj
            .get("status_codes")
            .and_then(|v| v.as_str())
            .map(String::from),
        exclude_status: obj
            .get("exclude_status")
            .and_then(|v| v.as_str())
            .map(String::from),
        follow_redirects: obj
            .get("follow_redirects")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        timeout: obj.get("timeout").and_then(|v| v.as_u64()),
        headers: obj
            .get("headers")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        cookies: obj
            .get("cookies")
            .and_then(|v| v.as_str())
            .map(String::from),
        user_agent: obj
            .get("user_agent")
            .and_then(|v| v.as_str())
            .map(String::from),
        wildcard: obj
            .get("wildcard")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    })
}

fn build_args(params: &GobusterInput) -> Result<Vec<String>> {
    let mut args = Vec::new();
    args.push(params.mode.clone());

    let url = super::recon_common::validate_target(&params.url)?;
    args.push("-u".to_string());
    args.push(url);

    if let Some(ref wl) = params.wordlist {
        args.push("-w".to_string());
        args.push(super::recon_common::validate_file_arg(wl, "wordlist")?);
    }

    let threads = params.threads.unwrap_or(DEFAULT_THREADS);
    args.push("-t".to_string());
    args.push(threads.to_string());

    if let Some(ref codes) = params.status_codes {
        args.push("-s".to_string());
        args.push(codes.clone());
    }
    if let Some(ref codes) = params.exclude_status {
        args.push("-b".to_string());
        args.push(codes.clone());
    }
    if params.follow_redirects {
        args.push("-r".to_string());
    }
    if params.wildcard {
        args.push("--wildcard".to_string());
    }

    for header in &params.headers {
        args.push("-H".to_string());
        args.push(header.clone());
    }
    if let Some(ref cookies) = params.cookies {
        args.push("-c".to_string());
        args.push(cookies.clone());
    }
    args.push("-a".to_string());
    args.push(
        params
            .user_agent
            .as_deref()
            .unwrap_or(crate::alphacode_provider_core::ALPHACODE_USER_AGENT)
            .to_string(),
    );

    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_accepts_aliases() {
        for payload in [
            serde_json::json!({"url": "https://example.com"}),
            serde_json::json!({"target": "https://example.com"}),
        ] {
            let params = normalize_gobuster_input(&payload).expect("normalize");
            assert_eq!(params.url, "https://example.com");
        }
    }
}
