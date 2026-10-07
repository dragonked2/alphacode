use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const DEFAULT_TIMEOUT: u64 = 30;
const DEFAULT_THREADS: usize = 50;

pub struct HttpxTool;

#[derive(Deserialize)]
struct HttpxInput {
    #[serde(default)]
    list: Option<String>,
    #[serde(default)]
    targets: Vec<String>,
    #[serde(default)]
    status_codes: Option<String>,
    #[serde(default)]
    title: bool,
    #[serde(default)]
    tech_detect: bool,
    #[serde(default)]
    web_server: bool,
    #[serde(default)]
    content_type: bool,
    #[serde(default)]
    response_size: bool,
    #[serde(default)]
    method: bool,
    #[serde(default)]
    tls: bool,
    #[serde(default)]
    cdn: bool,
    #[serde(default)]
    chains: bool,
    #[serde(default)]
    threads: Option<usize>,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    follow_redirects: bool,
    #[serde(default)]
    no_color: bool,
}

#[async_trait]
impl Tool for HttpxTool {
    fn name(&self) -> &str {
        "httpx"
    }

    fn description(&self) -> &str {
        "HTTP probing and fingerprinting tool. Checks which hosts are live, their status codes, technologies, TLS info, and more."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": super::intent_schema_property(),
                "targets": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "List of target URLs or hosts to probe."
                },
                "list": {
                    "type": "string",
                    "description": "Path to a file containing targets (one per line)."
                },
                "status_codes": {
                    "type": "string",
                    "description": "Filter by specific status codes (e.g., '200,301,403')."
                },
                "title": {
                    "type": "boolean",
                    "description": "Extract page title. Default: false."
                },
                "tech_detect": {
                    "type": "boolean",
                    "description": "Detect technology stack. Default: false."
                },
                "web_server": {
                    "type": "boolean",
                    "description": "Extract web server name. Default: false."
                },
                "content_type": {
                    "type": "boolean",
                    "description": "Extract content type. Default: false."
                },
                "response_size": {
                    "type": "boolean",
                    "description": "Extract response size. Default: false."
                },
                "method": {
                    "type": "boolean",
                    "description": "Extract HTTP method. Default: false."
                },
                "tls": {
                    "type": "boolean",
                    "description": "Extract TLS certificate info. Default: false."
                },
                "cdn": {
                    "type": "boolean",
                    "description": "Detect CDN provider. Default: false."
                },
                "follow_redirects": {
                    "type": "boolean",
                    "description": "Follow redirects. Default: false."
                },
                "threads": {
                    "type": "integer",
                    "description": "Number of concurrent threads. Default: 50."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Request timeout in seconds. Default: 30."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: HttpxInput = normalize_httpx_input(&input)?;
        if params.targets.is_empty() && params.list.is_none() {
            return Err(anyhow::anyhow!(
                "httpx needs targets: provide `targets` (array of URLs/hosts) or `list` \
                 (path to a file with one target per line), e.g. \
                 {{\"targets\": [\"https://example.com\"], \"title\": true, \"tech_detect\": true}}"
            ));
        }
        let args = build_args(&params)?;

        // `run_bounded` resolves the Go-installed build, which matters here:
        // `httpx` collides with the Python HTTP client's CLI of the same name.
        let output = super::recon_common::run_bounded(
            "httpx",
            &args,
            super::recon_common::DEFAULT_TOOL_TIMEOUT,
        )
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. {}", super::recon_common::install_hint("httpx"))
            } else {
                anyhow::anyhow!("{e}")
            }
        })?;

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // The Python `httpx` CLI (a different tool with the same name) prints
        // `Usage: httpx [OPTIONS] URL`. When it shadows ProjectDiscovery's
        // httpx on PATH, every probe fails with a usage error — detect it and
        // tell the user how to fix PATH instead of a cryptic usage line.
        //
        // Checked on *both* the success and failure paths: the Python httpx is
        // a perfectly valid CLI that exits 0 for `httpx https://target`, so
        // gating this on a non-zero exit let its output flow into the results
        // as though it were a real scan. The match is anchored on the stable
        // fragment rather than the full sentence, which Click has varied
        // (double space, `Try 'httpx -h' for help`) across releases.
        let python_httpx = format!("{stdout}\n{stderr}");
        if python_httpx.contains("[OPTIONS] URL") {
            return Err(anyhow::anyhow!(
                "httpx failed: the `httpx` on PATH is the Python HTTP client, not \
                 ProjectDiscovery's httpx, and no ProjectDiscovery build was found in \
                 {}. Install the right binary with \
                 `go install github.com/projectdiscovery/httpx/cmd/httpx@latest`, then \
                 either make sure it precedes the Python one on PATH (`where httpx` on \
                 Windows, `which -a httpx` elsewhere) or point GOBIN at the directory \
                 holding it.",
                crate::alphacode_app_core::bugbounty_install::go_bin_dir()
                    .map(|d| d.display().to_string())
                    .unwrap_or_else(|| "the Go bin directory".to_string())
            ));
        }

        if !output.status.success() {
            let detail = if stderr.is_empty() {
                if stdout.is_empty() {
                    "no output (binary exited non-zero with empty stderr; \
                     check that the targets are reachable and the httpx binary is \
                     ProjectDiscovery's httpx)"
                        .to_string()
                } else {
                    crate::alphacode_core::util::truncate_str(&stdout, 500).to_string()
                }
            } else {
                crate::alphacode_core::util::truncate_str(&stderr, 500).to_string()
            };
            return Err(anyhow::anyhow!("httpx exited with error: {detail}"));
        }

        let (lines, total, truncated) = super::recon_common::parse_lines(&output.stdout);

        let mut result = format!("httpx found {total} live hosts:\n\n");

        for line in &lines {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str(&super::recon_common::truncation_notice(lines.len(), total));

        let mut metadata = HashMap::new();
        metadata.insert("count".to_string(), json!(lines.len()));
        metadata.insert("total_found".to_string(), json!(total));
        metadata.insert("truncated".to_string(), json!(truncated));
        metadata.insert("tech_detect".to_string(), json!(params.tech_detect));
        metadata.insert("title".to_string(), json!(params.title));
        metadata.insert("tls".to_string(), json!(params.tls));
        metadata.insert("cdn".to_string(), json!(params.cdn));

        Ok(ToolOutput::new(result)
            .with_title(format!("httpx: {total} hosts probed"))
            .with_metadata(json!(metadata)))
    }
}

impl HttpxTool {
    pub fn new() -> Self {
        Self
    }
}

/// Accept the key spellings models actually send: `target`/`url`/`host`
/// (singular string or array) in addition to the canonical `targets` array.
fn normalize_httpx_input(input: &Value) -> Result<HttpxInput> {
    let mut params: HttpxInput = serde_json::from_value(input.clone()).unwrap_or(HttpxInput {
        list: None,
        targets: Vec::new(),
        status_codes: None,
        title: false,
        tech_detect: false,
        web_server: false,
        content_type: false,
        response_size: false,
        method: false,
        tls: false,
        cdn: false,
        chains: false,
        threads: None,
        timeout: None,
        follow_redirects: false,
        no_color: false,
    });
    if !params.targets.is_empty() {
        return Ok(params);
    }
    let obj = match input.as_object() {
        Some(obj) => obj,
        None => return Ok(params),
    };
    let mut extra: Vec<String> = Vec::new();
    for key in ["targets", "target", "url", "urls", "host", "hosts", "u"] {
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
    params.targets.extend(extra);
    Ok(params)
}

fn build_args(params: &HttpxInput) -> Result<Vec<String>> {
    let mut args = Vec::new();
    args.push("-H".to_string());
    args.push(format!(
        "User-Agent: {}",
        crate::alphacode_provider_core::ALPHACODE_USER_AGENT
    ));

    if let Some(ref list) = params.list {
        args.push("-l".to_string());
        args.push(super::recon_common::validate_file_arg(list, "list")?);
    } else if !params.targets.is_empty() {
        // `-u` is the long-standing per-target flag across httpx releases.
        for target in &params.targets {
            args.push("-u".to_string());
            args.push(super::recon_common::validate_target(target)?);
        }
    }

    if let Some(ref codes) = params.status_codes {
        // `-mc` / `-match-code`, NOT `-status-code`. httpx registers
        // `-sc, -status-code` as a *boolean probe* ("display response
        // status-code"), so it consumes no value: the old
        // `-status-code 200,404` made Go's flag package treat `200,404` as a
        // positional target, so httpx probed a host literally named
        // `200,404` while applying no filter at all. The matcher is the flag
        // that actually filters.
        args.push("-match-code".to_string());
        args.push(codes.clone());
    }

    if params.title {
        args.push("-title".to_string());
    }
    if params.tech_detect {
        args.push("-tech-detect".to_string());
    }
    if params.web_server {
        args.push("-web-server".to_string());
    }
    if params.content_type {
        args.push("-content-type".to_string());
    }
    if params.response_size {
        // httpx has no `-response-size`; the size probe is `-cl` /
        // `-content-length`. (`-rsts`/`-rstr` cap the *saved* response, which
        // is a different thing.)
        args.push("-content-length".to_string());
    }
    if params.method {
        args.push("-method".to_string());
    }
    if params.tls {
        // There is no bare `-tls`; the certificate-grabbing flag is
        // `-tls-grab`.
        args.push("-tls-grab".to_string());
    }
    if params.cdn {
        args.push("-cdn".to_string());
    }
    if params.chains {
        // No `-chains`; the redirect-chain flag is `-include-chain`.
        args.push("-include-chain".to_string());
    }
    if params.follow_redirects {
        args.push("-follow-redirects".to_string());
    }
    if params.no_color {
        args.push("-no-color".to_string());
    }

    let threads = params.threads.unwrap_or(DEFAULT_THREADS);
    args.push("-threads".to_string());
    args.push(threads.to_string());

    let timeout = params.timeout.unwrap_or(DEFAULT_TIMEOUT);
    args.push("-timeout".to_string());
    args.push(timeout.to_string());

    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_accepts_singular_aliases() {
        for payload in [
            serde_json::json!({"target": "https://example.com"}),
            serde_json::json!({"url": "https://example.com"}),
            serde_json::json!({"targets": "https://example.com"}),
            serde_json::json!({"hosts": ["a.com", "b.com"]}),
        ] {
            let params = normalize_httpx_input(&payload).expect("normalize");
            assert!(
                !params.targets.is_empty(),
                "expected targets from {payload}"
            );
        }
        let empty = normalize_httpx_input(&serde_json::json!({})).expect("normalize");
        assert!(empty.targets.is_empty());
    }

    #[test]
    fn test_build_args_with_list() {
        let input = HttpxInput {
            list: Some("subs.txt".to_string()),
            targets: vec![],
            status_codes: None,
            title: true,
            tech_detect: false,
            web_server: false,
            content_type: false,
            response_size: false,
            method: false,
            tls: false,
            cdn: false,
            chains: false,
            threads: None,
            timeout: None,
            follow_redirects: false,
            no_color: false,
        };
        let args = build_args(&input).expect("build_args");
        assert!(args.contains(&"-l".to_string()));
        assert!(args.contains(&"subs.txt".to_string()));
        assert!(args.contains(&"-title".to_string()));
    }
}
