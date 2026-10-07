use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

pub struct HakrawlerTool;

#[derive(Deserialize)]
struct HakrawlerInput {
    /// Target URL(s)
    url: Vec<String>,
    /// Depth
    #[serde(default)]
    depth: Option<usize>,
    /// Threads
    #[serde(default)]
    threads: Option<usize>,
    /// Timeout
    #[serde(default)]
    timeout: Option<u64>,
    /// Follow redirects
    #[serde(default)]
    follow_redirects: bool,
    /// JSON output
    #[serde(default)]
    json: bool,
    /// Silent
    #[serde(default)]
    silent: bool,
    /// Scope
    #[serde(default)]
    scope: Option<String>,
    /// Exclude
    #[serde(default)]
    exclude: Vec<String>,
    /// Include
    #[serde(default)]
    include: Vec<String>,
    /// Headers
    #[serde(default)]
    headers: Vec<String>,
    /// Cookies
    #[serde(default)]
    cookies: Option<String>,
    /// User agent
    #[serde(default)]
    user_agent: Option<String>,
    /// Proxy
    #[serde(default)]
    proxy: Option<String>,
    /// Wayback
    #[serde(default)]
    wayback: bool,
    /// Wayback machine
    #[serde(default)]
    wayback_machine: bool,
    /// JSLuice
    #[serde(default)]
    jsluice: bool,
    /// Robots.txt
    #[serde(default)]
    robots: bool,
    /// Sitemap.xml
    #[serde(default)]
    sitemap: bool,
    /// Favicon
    #[serde(default)]
    favicon: bool,
    /// Security.txt
    #[serde(default)]
    security_txt: bool,
}

#[async_trait]
impl Tool for HakrawlerTool {
    fn name(&self) -> &str {
        "hakrawler"
    }

    fn description(&self) -> &str {
        "Web crawler with JavaScript parsing. Use to crawl web applications and extract URLs, forms, and endpoints."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "intent": super::intent_schema_property(),
                "url": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Target URLs to crawl."
                },
                "depth": {
                    "type": "integer",
                    "description": "Crawl depth. Default: 2."
                },
                "threads": {
                    "type": "integer",
                    "description": "Concurrent threads. Default: 10."
                },
                "follow_redirects": {
                    "type": "boolean",
                    "description": "Follow redirects. Default: false."
                },
                "wayback": {
                    "type": "boolean",
                    "description": "Include Wayback Machine URLs. Default: false."
                },
                "jsluice": {
                    "type": "boolean",
                    "description": "Parse JavaScript files. Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: HakrawlerInput = normalize_hakrawler_input(&input)?;
        if params.url.is_empty() {
            return Err(anyhow::anyhow!(
                "hakrawler needs targets: provide `url` (array of URLs)"
            ));
        }
        let args = build_args(&params)?;

        let output = super::recon_common::run_bounded(
            "hakrawler",
            &args,
            super::recon_common::DEFAULT_TOOL_TIMEOUT,
        )
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. {}", super::recon_common::install_hint("hakrawler"))
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
            return Err(anyhow::anyhow!("hakrawler exited with error: {detail}"));
        }

        let (lines, total, truncated) = super::recon_common::parse_lines(&output.stdout);

        let mut result = format!("hakrawler found {} URLs:\n\n", total);
        for line in &lines {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str(&super::recon_common::truncation_notice(lines.len(), total));

        let mut metadata = HashMap::new();
        metadata.insert("count".to_string(), json!(lines.len()));
        metadata.insert("total_found".to_string(), json!(total));
        metadata.insert("truncated".to_string(), json!(truncated));

        Ok(ToolOutput::new(result)
            .with_title(format!("hakrawler: {total} URLs"))
            .with_metadata(json!(metadata)))
    }
}

impl HakrawlerTool {
    pub fn new() -> Self {
        Self
    }
}

fn normalize_hakrawler_input(input: &Value) -> Result<HakrawlerInput> {
    let mut params: HakrawlerInput =
        serde_json::from_value(input.clone()).unwrap_or(HakrawlerInput {
            url: Vec::new(),
            depth: None,
            threads: None,
            timeout: None,
            follow_redirects: false,
            json: false,
            silent: false,
            scope: None,
            exclude: Vec::new(),
            include: Vec::new(),
            headers: Vec::new(),
            cookies: None,
            user_agent: None,
            proxy: None,
            wayback: false,
            wayback_machine: false,
            jsluice: false,
            robots: false,
            sitemap: false,
            favicon: false,
            security_txt: false,
        });
    if !params.url.is_empty() {
        return Ok(params);
    }
    let obj = match input.as_object() {
        Some(obj) => obj,
        None => return Ok(params),
    };
    let mut extra: Vec<String> = Vec::new();
    for key in ["url", "urls", "target", "targets", "u"] {
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
    params.url.extend(extra);
    Ok(params)
}

fn build_args(params: &HakrawlerInput) -> Result<Vec<String>> {
    let mut args = Vec::new();

    for url in &params.url {
        args.push("-u".to_string());
        args.push(super::recon_common::validate_target(url)?);
    }

    if let Some(depth) = params.depth {
        args.push("-d".to_string());
        args.push(depth.to_string());
    }
    if let Some(threads) = params.threads {
        args.push("-t".to_string());
        args.push(threads.to_string());
    }
    if let Some(timeout) = params.timeout {
        args.push("-timeout".to_string());
        args.push(timeout.to_string());
    }
    if params.follow_redirects {
        args.push("-r".to_string());
    }
    if params.json {
        args.push("-j".to_string());
    }
    if params.silent {
        args.push("-s".to_string());
    }
    if let Some(ref scope) = params.scope {
        args.push("-scope".to_string());
        args.push(scope.clone());
    }
    for exclude in &params.exclude {
        args.push("-exclude".to_string());
        args.push(exclude.clone());
    }
    for include in &params.include {
        args.push("-include".to_string());
        args.push(include.clone());
    }
    for header in &params.headers {
        args.push("-h".to_string());
        args.push(header.clone());
    }
    if let Some(ref cookies) = params.cookies {
        args.push("-c".to_string());
        args.push(cookies.clone());
    }
    args.push("-ua".to_string());
    args.push(
        params
            .user_agent
            .as_deref()
            .unwrap_or(crate::alphacode_provider_core::ALPHACODE_USER_AGENT)
            .to_string(),
    );
    if let Some(ref proxy) = params.proxy {
        args.push("-p".to_string());
        args.push(proxy.clone());
    }
    if params.wayback {
        args.push("-wayback".to_string());
    }
    if params.wayback_machine {
        args.push("-wayback-machine".to_string());
    }
    if params.jsluice {
        args.push("-jsluice".to_string());
    }
    if params.robots {
        args.push("-robots".to_string());
    }
    if params.sitemap {
        args.push("-sitemap".to_string());
    }
    if params.favicon {
        args.push("-favicon".to_string());
    }
    if params.security_txt {
        args.push("-securitytxt".to_string());
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
            let params = normalize_hakrawler_input(&payload).expect("normalize");
            assert!(!params.url.is_empty());
        }
    }
}
