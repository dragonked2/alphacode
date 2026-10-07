use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const DEFAULT_THREADS: usize = 50;
#[allow(dead_code)]
const DEFAULT_WORDLIST: &str = "/usr/share/wordlists/dirb/common.txt";

pub struct FeroxbusterTool;

#[derive(Deserialize)]
struct FeroxbusterInput {
    /// Target URL
    url: String,
    /// Wordlist path
    #[serde(default)]
    wordlist: Option<String>,
    /// Threads
    #[serde(default)]
    threads: Option<usize>,
    /// Extensions to scan
    #[serde(default)]
    extensions: Vec<String>,
    /// Status codes to match
    #[serde(default)]
    status_codes: Option<String>,
    /// Exclude status codes
    #[serde(default)]
    exclude_status: Option<String>,
    /// Follow redirects
    #[serde(default)]
    follow_redirects: bool,
    /// Timeout
    #[serde(default)]
    #[allow(dead_code)]
    timeout: Option<u64>,
    /// Silent
    #[serde(default)]
    silent: bool,
    /// JSON output
    #[serde(default)]
    json: bool,
    /// Auto-tune
    #[serde(default)]
    auto_tune: bool,
    /// Auto-bail
    #[serde(default)]
    auto_bail: bool,
    /// Scan recursively
    #[serde(default)]
    recursive: bool,
    /// Depth
    #[serde(default)]
    depth: Option<usize>,
    /// Filter by size
    #[serde(default)]
    filter_size: Option<String>,
    /// Filter by words
    #[serde(default)]
    filter_words: Option<String>,
    /// Filter by lines
    #[serde(default)]
    filter_lines: Option<String>,
    /// Filter by regex
    #[serde(default)]
    filter_regex: Option<String>,
    /// Don't scan
    #[serde(default)]
    dont_scan: Vec<String>,
    /// Extract links
    #[serde(default)]
    extract_links: bool,
}

#[async_trait]
impl Tool for FeroxbusterTool {
    fn name(&self) -> &str {
        "feroxbuster"
    }

    fn description(&self) -> &str {
        "Fast content discovery tool. Use to find hidden directories, files, and endpoints on web servers. More features than gobuster."
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
                "wordlist": {
                    "type": "string",
                    "description": "Path to wordlist file."
                },
                "extensions": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Extensions to scan (e.g., ['php', 'html', 'js'])."
                },
                "threads": {
                    "type": "integer",
                    "description": "Concurrent threads. Default: 50."
                },
                "status_codes": {
                    "type": "string",
                    "description": "Status codes to match (e.g., '200,204,301,302,307,401,403')."
                },
                "follow_redirects": {
                    "type": "boolean",
                    "description": "Follow redirects. Default: false."
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Scan recursively. Default: false."
                },
                "depth": {
                    "type": "integer",
                    "description": "Maximum recursion depth."
                },
                "auto_tune": {
                    "type": "boolean",
                    "description": "Auto-tune based on response. Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: FeroxbusterInput = normalize_feroxbuster_input(&input)?;
        let args = build_args(&params)?;

        let output = super::recon_common::run_bounded(
            "feroxbuster",
            &args,
            super::recon_common::DEFAULT_TOOL_TIMEOUT,
        )
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. {}", super::recon_common::install_hint("feroxbuster"))
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
            return Err(anyhow::anyhow!("feroxbuster exited with error: {detail}"));
        }

        let (lines, total, truncated) = super::recon_common::parse_lines(&output.stdout);

        let mut result = format!("feroxbuster found {} results:\n\n", total);
        for line in &lines {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str(&super::recon_common::truncation_notice(lines.len(), total));

        let mut metadata = HashMap::new();
        metadata.insert("url".to_string(), json!(params.url));
        metadata.insert("count".to_string(), json!(lines.len()));
        metadata.insert("total_found".to_string(), json!(total));
        metadata.insert("truncated".to_string(), json!(truncated));

        Ok(ToolOutput::new(result)
            .with_title(format!("feroxbuster: {total} results"))
            .with_metadata(json!(metadata)))
    }
}

impl FeroxbusterTool {
    pub fn new() -> Self {
        Self
    }
}

fn normalize_feroxbuster_input(input: &Value) -> Result<FeroxbusterInput> {
    if let Ok(params) = serde_json::from_value::<FeroxbusterInput>(input.clone()) {
        return Ok(params);
    }
    // Aliases, bare-URL strings and truncated payloads all resolve through the shared
    // coercion ladder; a hand-rolled key list missed every other shape.
    let url = super::coerce_url_arg(input, "feroxbuster")?;
    // Remaining options are optional. A payload that was a bare URL string has no
    // object to read them from, so an empty map stands in for the defaults.
    let obj = input.as_object().cloned().unwrap_or_default();
    Ok(FeroxbusterInput {
        url: url.to_string(),
        wordlist: obj
            .get("wordlist")
            .and_then(|v| v.as_str())
            .map(String::from),
        threads: obj
            .get("threads")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize),
        extensions: obj
            .get("extensions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
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
        silent: obj.get("silent").and_then(|v| v.as_bool()).unwrap_or(false),
        json: obj.get("json").and_then(|v| v.as_bool()).unwrap_or(false),
        auto_tune: obj
            .get("auto_tune")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        auto_bail: obj
            .get("auto_bail")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        recursive: obj
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        depth: obj
            .get("depth")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize),
        filter_size: obj
            .get("filter_size")
            .and_then(|v| v.as_str())
            .map(String::from),
        filter_words: obj
            .get("filter_words")
            .and_then(|v| v.as_str())
            .map(String::from),
        filter_lines: obj
            .get("filter_lines")
            .and_then(|v| v.as_str())
            .map(String::from),
        filter_regex: obj
            .get("filter_regex")
            .and_then(|v| v.as_str())
            .map(String::from),
        dont_scan: obj
            .get("dont_scan")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        extract_links: obj
            .get("extract_links")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    })
}

fn build_args(params: &FeroxbusterInput) -> Result<Vec<String>> {
    let mut args = Vec::new();
    args.push("-H".to_string());
    args.push(format!(
        "User-Agent: {}",
        crate::alphacode_provider_core::ALPHACODE_USER_AGENT
    ));

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

    if !params.extensions.is_empty() {
        args.push("-x".to_string());
        args.push(params.extensions.join(","));
    }
    if let Some(ref codes) = params.status_codes {
        args.push("-s".to_string());
        args.push(codes.clone());
    }
    if let Some(ref codes) = params.exclude_status {
        args.push("--exclude-status".to_string());
        args.push(codes.clone());
    }
    if params.follow_redirects {
        args.push("-r".to_string());
    }
    if params.recursive {
        args.push("-r".to_string());
    }
    if let Some(depth) = params.depth {
        args.push("-d".to_string());
        args.push(depth.to_string());
    }
    if params.auto_tune {
        args.push("--auto-tune".to_string());
    }
    if params.auto_bail {
        args.push("--auto-bail".to_string());
    }
    if params.silent {
        args.push("-s".to_string());
    }
    if params.json {
        args.push("-j".to_string());
    }
    if let Some(ref size) = params.filter_size {
        args.push("--filter-size".to_string());
        args.push(size.clone());
    }
    if let Some(ref words) = params.filter_words {
        args.push("--filter-word".to_string());
        args.push(words.clone());
    }
    if let Some(ref lines) = params.filter_lines {
        args.push("--filter-line".to_string());
        args.push(lines.clone());
    }
    if let Some(ref regex) = params.filter_regex {
        args.push("--filter-regex".to_string());
        args.push(regex.clone());
    }
    for path in &params.dont_scan {
        args.push("--dont-scan".to_string());
        args.push(path.clone());
    }
    if params.extract_links {
        args.push("-e".to_string());
    }

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
            let params = normalize_feroxbuster_input(&payload).expect("normalize");
            assert_eq!(params.url, "https://example.com");
        }
    }
}
