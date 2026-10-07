use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const DEFAULT_THREADS: usize = 10;
const DEFAULT_TIMEOUT: u64 = 30;
const DEFAULT_RETRIES: u64 = 3;

pub struct SqlmapTool;

#[derive(Deserialize)]
struct SqlmapInput {
    /// Target URL (must contain a parameter)
    url: String,
    /// POST data
    #[serde(default)]
    data: Option<String>,
    /// Cookie
    #[serde(default)]
    cookie: Option<String>,
    /// User agent
    #[serde(default)]
    user_agent: Option<String>,
    /// Referer
    #[serde(default)]
    referer: Option<String>,
    /// Database type
    #[serde(default)]
    dbms: Option<String>,
    /// Risk level (1-3)
    #[serde(default)]
    risk: Option<u8>,
    /// Test level (1-5)
    #[serde(default)]
    level: Option<u8>,
    /// Threads
    #[serde(default)]
    threads: Option<usize>,
    /// Timeout
    #[serde(default)]
    timeout: Option<u64>,
    /// Retries
    #[serde(default)]
    retries: Option<u64>,
    /// Batch mode (no interactive)
    #[serde(default)]
    batch: bool,
    /// Dump database
    #[serde(default)]
    dump: bool,
    /// Enumerate databases
    #[serde(default)]
    dbs: bool,
    /// Enumerate tables
    #[serde(default)]
    tables: bool,
    /// Enumerate columns
    #[serde(default)]
    columns: bool,
    /// OS shell
    #[serde(default)]
    os_shell: bool,
    /// OS command
    #[serde(default)]
    os_cmd: Option<String>,
    /// Tamper scripts
    #[serde(default)]
    tamper: Option<String>,
    /// Random agent
    #[serde(default)]
    random_agent: bool,
    /// Proxy
    #[serde(default)]
    proxy: Option<String>,
    /// Headers
    #[serde(default)]
    headers: Vec<String>,
}

#[async_trait]
impl Tool for SqlmapTool {
    fn name(&self) -> &str {
        "sqlmap"
    }

    fn description(&self) -> &str {
        "Automatic SQL injection detection and exploitation. Use when you suspect SQL injection in a URL parameter. Chains well after manual testing confirms a parameter is injectable."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "intent": super::intent_schema_property(),
                "url": {
                    "type": "string",
                    "description": "Target URL with parameter (e.g., 'https://example.com/page?id=1')."
                },
                "data": {
                    "type": "string",
                    "description": "POST data string."
                },
                "cookie": {
                    "type": "string",
                    "description": "Cookie header."
                },
                "dbms": {
                    "type": "string",
                    "description": "Database type (e.g., 'mysql', 'postgresql')."
                },
                "risk": {
                    "type": "integer",
                    "description": "Risk level (1-3). Default: 1."
                },
                "level": {
                    "type": "integer",
                    "description": "Test level (1-5). Default: 1."
                },
                "threads": {
                    "type": "integer",
                    "description": "Concurrent threads. Default: 10."
                },
                "batch": {
                    "type": "boolean",
                    "description": "Batch mode (no interactive). Default: true."
                },
                "dump": {
                    "type": "boolean",
                    "description": "Dump database. Default: false."
                },
                "dbs": {
                    "type": "boolean",
                    "description": "Enumerate databases. Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: SqlmapInput = normalize_sqlmap_input(&input)?;
        let args = build_args(&params)?;

        let output = super::recon_common::run_bounded(
            "sqlmap",
            &args,
            super::recon_common::DEFAULT_TOOL_TIMEOUT,
        )
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. {}", super::recon_common::install_hint("sqlmap"))
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
            return Err(anyhow::anyhow!("sqlmap exited with error: {detail}"));
        }

        let (lines, total, truncated) = super::recon_common::parse_lines(&output.stdout);

        let mut result = format!("sqlmap found {} results:\n\n", total);
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
            .with_title(format!("sqlmap: {total} results"))
            .with_metadata(json!(metadata)))
    }
}

impl SqlmapTool {
    pub fn new() -> Self {
        Self
    }
}

fn normalize_sqlmap_input(input: &Value) -> Result<SqlmapInput> {
    if let Ok(params) = serde_json::from_value::<SqlmapInput>(input.clone()) {
        return Ok(params);
    }
    // Aliases, bare-URL strings and truncated payloads all resolve through the shared
    // coercion ladder; a hand-rolled key list missed every other shape.
    let url = super::coerce_url_arg(input, "sqlmap")?;
    // Remaining options are optional. A payload that was a bare URL string has no
    // object to read them from, so an empty map stands in for the defaults.
    let obj = input.as_object().cloned().unwrap_or_default();
    Ok(SqlmapInput {
        url: url.to_string(),
        data: obj.get("data").and_then(|v| v.as_str()).map(String::from),
        cookie: obj.get("cookie").and_then(|v| v.as_str()).map(String::from),
        user_agent: obj
            .get("user_agent")
            .and_then(|v| v.as_str())
            .map(String::from),
        referer: obj
            .get("referer")
            .and_then(|v| v.as_str())
            .map(String::from),
        dbms: obj.get("dbms").and_then(|v| v.as_str()).map(String::from),
        risk: obj.get("risk").and_then(|v| v.as_u64()).map(|n| n as u8),
        level: obj.get("level").and_then(|v| v.as_u64()).map(|n| n as u8),
        threads: obj
            .get("threads")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize),
        timeout: obj.get("timeout").and_then(|v| v.as_u64()),
        retries: obj.get("retries").and_then(|v| v.as_u64()),
        batch: obj.get("batch").and_then(|v| v.as_bool()).unwrap_or(true),
        dump: obj.get("dump").and_then(|v| v.as_bool()).unwrap_or(false),
        dbs: obj.get("dbs").and_then(|v| v.as_bool()).unwrap_or(false),
        tables: obj.get("tables").and_then(|v| v.as_bool()).unwrap_or(false),
        columns: obj
            .get("columns")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        os_shell: obj
            .get("os_shell")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        os_cmd: obj.get("os_cmd").and_then(|v| v.as_str()).map(String::from),
        tamper: obj.get("tamper").and_then(|v| v.as_str()).map(String::from),
        random_agent: obj
            .get("random_agent")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        proxy: obj.get("proxy").and_then(|v| v.as_str()).map(String::from),
        headers: obj
            .get("headers")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn build_args(params: &SqlmapInput) -> Result<Vec<String>> {
    let mut args = Vec::new();

    let url = super::recon_common::validate_target(&params.url)?;
    args.push("-u".to_string());
    args.push(url);

    if let Some(ref data) = params.data {
        args.push("--data".to_string());
        args.push(data.clone());
    }
    if let Some(ref cookie) = params.cookie {
        args.push("--cookie".to_string());
        args.push(cookie.clone());
    }
    args.push("--user-agent".to_string());
    args.push(
        params
            .user_agent
            .as_deref()
            .unwrap_or(crate::alphacode_provider_core::ALPHACODE_USER_AGENT)
            .to_string(),
    );
    if let Some(ref referer) = params.referer {
        args.push("--referer".to_string());
        args.push(referer.clone());
    }
    if let Some(ref dbms) = params.dbms {
        args.push("--dbms".to_string());
        args.push(dbms.clone());
    }
    if let Some(risk) = params.risk {
        args.push("--risk".to_string());
        args.push(risk.to_string());
    }
    if let Some(level) = params.level {
        args.push("--level".to_string());
        args.push(level.to_string());
    }

    let threads = params.threads.unwrap_or(DEFAULT_THREADS);
    args.push("--threads".to_string());
    args.push(threads.to_string());

    let timeout = params.timeout.unwrap_or(DEFAULT_TIMEOUT);
    args.push("--timeout".to_string());
    args.push(timeout.to_string());

    let retries = params.retries.unwrap_or(DEFAULT_RETRIES);
    args.push("--retries".to_string());
    args.push(retries.to_string());

    if params.batch {
        args.push("--batch".to_string());
    }
    if params.dump {
        args.push("--dump".to_string());
    }
    if params.dbs {
        args.push("--dbs".to_string());
    }
    if params.tables {
        args.push("--tables".to_string());
    }
    if params.columns {
        args.push("--columns".to_string());
    }
    if params.os_shell {
        args.push("--os-shell".to_string());
    }
    if let Some(ref cmd) = params.os_cmd {
        args.push("--os-cmd".to_string());
        args.push(cmd.clone());
    }
    if let Some(ref tamper) = params.tamper {
        args.push("--tamper".to_string());
        args.push(tamper.clone());
    }
    if params.random_agent {
        args.push("--random-agent".to_string());
    }
    if let Some(ref proxy) = params.proxy {
        args.push("--proxy".to_string());
        args.push(proxy.clone());
    }
    for header in &params.headers {
        args.push("--headers".to_string());
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
            serde_json::json!({"url": "https://example.com?id=1"}),
            serde_json::json!({"target": "https://example.com?id=1"}),
        ] {
            let params = normalize_sqlmap_input(&payload).expect("normalize");
            assert_eq!(params.url, "https://example.com?id=1");
        }
    }
}
