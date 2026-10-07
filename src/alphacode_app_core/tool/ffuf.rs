use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
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
        "Fast web fuzzer for directory and parameter discovery. Brute-forces paths, parameters, and virtual hosts against authorized web servers. If ffuf or Go is missing, Alphacode attempts to install them on first use; wait for the tool result before choosing another approach, and treat setup failures as a scan that did not run."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url", "wordlist"],
            "properties": {
                "intent": super::intent_schema_property(),
                "url": {
                    "type": "string",
                    "description": "Target URL with a FUZZ placeholder (e.g., 'https://target/FUZZ')."
                },
                "wordlist": {
                    "type": "string",
                    "description": "Required. Path to a wordlist file, one entry per line. ffuf ships no wordlist of its own, so this must point at a real file."
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
        let args = build_args(&params)?;

        let output = super::recon_common::run_bounded(
            "ffuf",
            &args,
            super::recon_common::DEFAULT_TOOL_TIMEOUT,
        )
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. {}", super::recon_common::install_hint("ffuf"))
            } else {
                anyhow::anyhow!("{e}")
            }
        })?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "{}",
                super::recon_common::describe_failure("ffuf", &output)
            ));
        }

        // Parse the results instead of handing raw stdout to the model. ffuf
        // prints a banner and a `\r` progress redraw alongside results, so the
        // previous behaviour fed all of that to the model verbatim and, unlike
        // every sibling tool, reported no `count` — so a caller could not tell
        // an empty scan from a truncated one.
        let (lines, total, truncated) = super::recon_common::parse_lines(&output.stdout);

        let mut result = format!("ffuf found {total} results for {}:\n\n", params.url);
        for line in &lines {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str(&super::recon_common::truncation_notice(lines.len(), total));
        if total == 0 {
            result.push_str(
                "\n[no matches. If the wordlist is small or the filter is strict, this may be a true negative — confirm the target responds at the base URL before concluding there is nothing there.]",
            );
        }

        let mut metadata = HashMap::new();
        metadata.insert("url".to_string(), json!(params.url));
        metadata.insert("count".to_string(), json!(lines.len()));
        metadata.insert("total_found".to_string(), json!(total));
        metadata.insert("truncated".to_string(), json!(truncated));
        metadata.insert(
            "thread_count".to_string(),
            json!(params.threads.unwrap_or(DEFAULT_THREADS)),
        );

        Ok(ToolOutput::new(result)
            .with_title(format!("ffuf: {total} results"))
            .with_metadata(json!(metadata)))
    }
}

impl FfufTool {
    pub fn new() -> Self {
        Self
    }
}

fn build_args(params: &FfufInput) -> Result<Vec<String>> {
    let url = super::recon_common::validate_target(&params.url)?;
    if !url.contains("FUZZ") {
        return Err(anyhow::anyhow!(
            "ffuf needs a `FUZZ` placeholder in the URL, e.g. \
             `https://example.com/FUZZ`. Got `{url}`."
        ));
    }

    let mut args = Vec::new();
    args.push("-u".to_string());
    args.push(url);

    // A wordlist is mandatory; the previous default of the bare relative path
    // `common.txt` does not exist in the agent's working directory, so the
    // *default* call always failed. Fail with an actionable message instead of
    // pointing at a file that was never there.
    match params.wordlist.as_deref() {
        Some(wordlist) => {
            args.push("-w".to_string());
            args.push(super::recon_common::validate_file_arg(
                wordlist, "wordlist",
            )?);
        }
        None => {
            return Err(anyhow::anyhow!(
                "ffuf requires a `wordlist` path. Pass one explicitly, e.g. \
                 {{\"url\": \"https://example.com/FUZZ\", \"wordlist\": \"common.txt\"}}. \
                 ffuf ships no wordlist of its own."
            ));
        }
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

    let headers = params.headers.as_deref().unwrap_or_default();
    super::recon_common::append_default_user_agent_header(&mut args, headers);
    if let Some(ref headers) = params.headers {
        for header in headers {
            args.push("-H".to_string());
            args.push(header.clone());
        }
    }

    if params.no_color {
        args.push("-nc".to_string());
    }

    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_args_basic() {
        let input = FfufInput {
            url: "https://example.com/FUZZ".to_string(),
            wordlist: Some("common.txt".to_string()),
            extensions: None,
            threads: None,
            timeout: None,
            rate_limit: None,
            method: None,
            data: None,
            headers: None,
            no_color: false,
        };
        let args = build_args(&input).expect("build_args");
        assert!(args.contains(&"-u".to_string()));
        assert!(args.contains(&"https://example.com/FUZZ".to_string()));
        assert!(args.contains(&"-w".to_string()));
        assert!(args.contains(&"common.txt".to_string()));
    }

    /// Regression: the old builder defaulted to the bare relative path
    /// `common.txt`, which does not exist in the agent's working directory,
    /// so the *default* ffuf call always failed. The wordlist is now
    /// mandatory and the failure is actionable.
    #[test]
    fn missing_wordlist_is_an_actionable_error() {
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
        let err = build_args(&input).expect_err("must require a wordlist");
        assert!(
            err.to_string().contains("wordlist"),
            "error should name the missing parameter: {err}"
        );
    }

    /// ffuf needs a `FUZZ` placeholder; without it the run is meaningless.
    #[test]
    fn url_without_fuzz_placeholder_is_rejected() {
        let input = FfufInput {
            url: "https://example.com/admin".to_string(),
            wordlist: Some("common.txt".to_string()),
            extensions: None,
            threads: None,
            timeout: None,
            rate_limit: None,
            method: None,
            data: None,
            headers: None,
            no_color: false,
        };
        assert!(build_args(&input).is_err());
    }
}
