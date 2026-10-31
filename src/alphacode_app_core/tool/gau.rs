use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

pub struct GauTool;

#[derive(Deserialize)]
struct GauInput {
    domain: String,
    #[serde(default)]
    threads: Option<usize>,
    #[serde(default)]
    no_color: bool,
    #[serde(default)]
    o: Option<bool>,
    #[serde(default)]
    p: Option<bool>,
    #[serde(default)]
    q: Option<bool>,
    #[serde(default)]
    s: Option<bool>,
    #[serde(default)]
    t: Option<u64>,
}

#[async_trait]
impl Tool for GauTool {
    fn name(&self) -> &str {
        "gau"
    }

    fn description(&self) -> &str {
        "Get All URLs (gau) fetches known URLs for a domain from multiple sources including AlienVault OTX, CommonCrawl, URLScan, and Wayback Machine."
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
                "threads": {
                    "type": "integer",
                    "description": "Number of concurrent threads. Default: 1."
                },
                "no_color": {
                    "type": "boolean",
                    "description": "Accepted for compatibility; URL output is uncolored."
                },
                "o": {
                    "type": "boolean",
                    "description": "Only show unique hosts. Default: false."
                },
                "p": {
                    "type": "boolean",
                    "description": "Show only paths. Default: false."
                },
                "q": {
                    "type": "boolean",
                    "description": "Show only query strings. Default: false."
                },
                "s": {
                    "type": "boolean",
                    "description": "Match only on subdomains. Default: false."
                },
                "t": {
                    "type": "integer",
                    "description": "HTTP client timeout in seconds; uses gau's default when omitted."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: GauInput = serde_json::from_value(input)?;
        let args = build_args(&params)?;

        let output = super::recon_common::run_bounded(
            "gau",
            &args,
            super::recon_common::DEFAULT_TOOL_TIMEOUT,
        )
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. {}", super::recon_common::install_hint("gau"))
            } else {
                anyhow::anyhow!("{e}")
            }
        })?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "{}",
                super::recon_common::describe_failure("gau", &output)
            ));
        }

        let filtered = format_gau_output(&output.stdout, &params);
        let (urls, total, truncated) = super::recon_common::parse_lines(filtered.as_bytes());

        let mut result = format!("gau found {total} URLs for {}:\n\n", params.domain);

        for url in &urls {
            result.push_str(url);
            result.push('\n');
        }
        result.push_str(&super::recon_common::truncation_notice(urls.len(), total));

        let mut metadata = HashMap::new();
        metadata.insert("domain".to_string(), json!(params.domain));
        metadata.insert("count".to_string(), json!(urls.len()));
        metadata.insert("total_found".to_string(), json!(total));
        metadata.insert("truncated".to_string(), json!(truncated));
        metadata.insert("threads".to_string(), json!(params.threads.unwrap_or(1)));

        Ok(ToolOutput::new(result)
            .with_title(format!("gau: {total} URLs found"))
            .with_metadata(json!(metadata)))
    }
}

impl GauTool {
    pub fn new() -> Self {
        Self
    }
}

fn build_args(params: &GauInput) -> Result<Vec<String>> {
    let domain = super::recon_common::validate_hostname(&params.domain)?;

    let mut args = Vec::new();

    if let Some(threads) = params.threads {
        args.push("--threads".to_string());
        args.push(threads.to_string());
    }

    if [params.o, params.p, params.q]
        .into_iter()
        .flatten()
        .filter(|enabled| *enabled)
        .count()
        > 1
    {
        anyhow::bail!("gau output modes 'o', 'p', and 'q' are mutually exclusive");
    }
    let _ = params.no_color;
    if params.s.unwrap_or(false) {
        args.push("--subs".to_string());
    }
    if let Some(timeout) = params.t {
        args.push("--timeout".to_string());
        args.push(timeout.to_string());
    }

    // `--` so the positional domain can never be parsed as a flag. A domain of
    // `-t` previously switched gau into time-limit mode and reported a
    // successful scan of nothing.
    args.push("--".to_string());
    args.push(domain);

    Ok(args)
}

/// Apply the legacy output options locally; gau v2 does not provide those
/// switches. Filtering happens before output limits and result counts.
fn format_gau_output(raw: &[u8], params: &GauInput) -> String {
    let text = String::from_utf8_lossy(raw);
    let mut seen = std::collections::HashSet::new();
    let mut result = String::new();
    let suffix = format!(".{}", params.domain.to_ascii_lowercase());
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Ok(url) = url::Url::parse(line) else {
            continue;
        };
        let Some(host) = url.host_str() else { continue };
        if params.s.unwrap_or(false) && !host.to_ascii_lowercase().ends_with(&suffix) {
            continue;
        }
        let value = if params.o.unwrap_or(false) {
            if !seen.insert(host.to_ascii_lowercase()) {
                continue;
            }
            host
        } else if params.p.unwrap_or(false) {
            url.path()
        } else if params.q.unwrap_or(false) {
            let Some(query) = url.query() else { continue };
            query
        } else {
            line
        };
        result.push_str(value);
        result.push('\n');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_args_basic() {
        let input = GauInput {
            domain: "example.com".to_string(),
            threads: None,
            no_color: false,
            o: None,
            p: None,
            q: None,
            s: None,
            t: None,
        };
        let args = build_args(&input).expect("build_args");
        assert!(args.contains(&"example.com".to_string()));
    }

    #[test]
    fn gau_v2_flags_and_legacy_filters_are_supported() {
        let input: GauInput = serde_json::from_value(json!({"domain":"example.com", "threads":2, "no_color":true, "s":true, "t":4, "o":true})).unwrap();
        let args = build_args(&input).unwrap();
        assert!(args.contains(&"--threads".to_string()));
        assert!(args.contains(&"--timeout".to_string()));
        assert!(args.contains(&"--subs".to_string()));
        assert!(
            !args
                .iter()
                .any(|arg| ["-nc", "-o", "-t", "-s"].contains(&arg.as_str()))
        );
        let result = format_gau_output(b"https://example.com/a\nhttps://sub.example.com/a\nhttps://sub.example.com/b\nhttps://other.com/\n", &input);
        assert_eq!(result, "sub.example.com\n");
    }
}
