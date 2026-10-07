use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

pub struct CariddiTool;

#[derive(Deserialize)]
struct CariddiInput {
    /// Target URL(s)
    url: Vec<String>,
    /// Input file
    #[serde(default)]
    input: Option<String>,
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
    /// Subdomains
    #[serde(default)]
    subdomains: bool,
    /// Other sources
    #[serde(default)]
    other_sources: bool,
    /// Include subdomains
    #[serde(default)]
    include_subdomains: bool,
    /// Crawl
    #[serde(default)]
    crawl: bool,
    /// Scan
    #[serde(default)]
    scan: bool,
    /// Fuzz
    #[serde(default)]
    fuzz: bool,
    /// Extract
    #[serde(default)]
    extract: bool,
    /// Discovery
    #[serde(default)]
    discovery: bool,
    /// Vulnerability
    #[serde(default)]
    vulnerability: bool,
    /// Information
    #[serde(default)]
    information: bool,
    /// Misconfiguration
    #[serde(default)]
    misconfiguration: bool,
    /// Exposure
    #[serde(default)]
    exposure: bool,
    /// CVE
    #[serde(default)]
    cve: bool,
    /// CWE
    #[serde(default)]
    cwe: bool,
    /// OWASP
    #[serde(default)]
    owasp: bool,
    /// HackerOne
    #[serde(default)]
    hackerone: bool,
    /// Bugcrowd
    #[serde(default)]
    bugcrowd: bool,
    /// Intigriti
    #[serde(default)]
    intigriti: bool,
    /// YesWeHack
    #[serde(default)]
    yeswehack: bool,
    /// HackenProof
    #[serde(default)]
    hackenproof: bool,
    /// Synack
    #[serde(default)]
    synack: bool,
    /// Cobalt
    #[serde(default)]
    cobalt: bool,
    /// BugBounty
    #[serde(default)]
    bugbounty: bool,
    /// HackerBug
    #[serde(default)]
    hackerbug: bool,
    /// VDP
    #[serde(default)]
    vdp: bool,
    /// Responsible Disclosure
    #[serde(default)]
    responsible_disclosure: bool,
    /// Security Policy
    #[serde(default)]
    security_policy: bool,
    /// Terms of Service
    #[serde(default)]
    terms_of_service: bool,
    /// Privacy Policy
    #[serde(default)]
    privacy_policy: bool,
    /// Cookie Policy
    #[serde(default)]
    cookie_policy: bool,
    /// GDPR
    #[serde(default)]
    gdpr: bool,
    /// CCPA
    #[serde(default)]
    ccpa: bool,
    /// HIPAA
    #[serde(default)]
    hipaa: bool,
    /// PCI DSS
    #[serde(default)]
    pci_dss: bool,
    /// SOC 2
    #[serde(default)]
    soc2: bool,
    /// ISO 27001
    #[serde(default)]
    iso27001: bool,
    /// NIST
    #[serde(default)]
    nist: bool,
    /// CIS
    #[serde(default)]
    cis: bool,
    /// OWASP Top 10
    #[serde(default)]
    owasp_top10: bool,
    /// OWASP API Top 10
    #[serde(default)]
    owasp_api_top10: bool,
    /// OWASP Mobile Top 10
    #[serde(default)]
    owasp_mobile_top10: bool,
    /// OWASP IoT Top 10
    #[serde(default)]
    owasp_iot_top10: bool,
    /// OWASP LLM Top 10
    #[serde(default)]
    owasp_llm_top10: bool,
}

#[async_trait]
impl Tool for CariddiTool {
    fn name(&self) -> &str {
        "cariddi"
    }

    fn description(&self) -> &str {
        "Web crawler and scanner. Use to crawl web applications, extract URLs, and scan for vulnerabilities."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": super::intent_schema_property(),
                "url": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Target URLs to crawl."
                },
                "input": {
                    "type": "string",
                    "description": "Input file with URLs (one per line)."
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
                "crawl": {
                    "type": "boolean",
                    "description": "Crawl mode. Default: false."
                },
                "scan": {
                    "type": "boolean",
                    "description": "Scan mode. Default: false."
                },
                "vulnerability": {
                    "type": "boolean",
                    "description": "Vulnerability scan. Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: CariddiInput = normalize_cariddi_input(&input)?;
        if params.url.is_empty() && params.input.is_none() {
            return Err(anyhow::anyhow!(
                "cariddi needs targets: provide `url` (array of URLs) or `input` (file path)"
            ));
        }
        let args = build_args(&params)?;

        let output = super::recon_common::run_bounded(
            "cariddi",
            &args,
            super::recon_common::DEFAULT_TOOL_TIMEOUT,
        )
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. {}", super::recon_common::install_hint("cariddi"))
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
            return Err(anyhow::anyhow!("cariddi exited with error: {detail}"));
        }

        let (lines, total, truncated) = super::recon_common::parse_lines(&output.stdout);

        let mut result = format!("cariddi found {} results:\n\n", total);
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
            .with_title(format!("cariddi: {total} results"))
            .with_metadata(json!(metadata)))
    }
}

impl CariddiTool {
    pub fn new() -> Self {
        Self
    }
}

fn normalize_cariddi_input(input: &Value) -> Result<CariddiInput> {
    let mut params: CariddiInput = serde_json::from_value(input.clone()).unwrap_or(CariddiInput {
        url: Vec::new(),
        input: None,
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
        jsluice: false,
        robots: false,
        sitemap: false,
        favicon: false,
        security_txt: false,
        subdomains: false,
        other_sources: false,
        include_subdomains: false,
        crawl: false,
        scan: false,
        fuzz: false,
        extract: false,
        discovery: false,
        vulnerability: false,
        information: false,
        misconfiguration: false,
        exposure: false,
        cve: false,
        cwe: false,
        owasp: false,
        hackerone: false,
        bugcrowd: false,
        intigriti: false,
        yeswehack: false,
        hackenproof: false,
        synack: false,
        cobalt: false,
        bugbounty: false,
        hackerbug: false,
        vdp: false,
        responsible_disclosure: false,
        security_policy: false,
        terms_of_service: false,
        privacy_policy: false,
        cookie_policy: false,
        gdpr: false,
        ccpa: false,
        hipaa: false,
        pci_dss: false,
        soc2: false,
        iso27001: false,
        nist: false,
        cis: false,
        owasp_top10: false,
        owasp_api_top10: false,
        owasp_mobile_top10: false,
        owasp_iot_top10: false,
        owasp_llm_top10: false,
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

fn build_args(params: &CariddiInput) -> Result<Vec<String>> {
    let mut args = Vec::new();

    if let Some(ref input) = params.input {
        args.push("-i".to_string());
        args.push(input.clone());
    } else if !params.url.is_empty() {
        for url in &params.url {
            args.push("-u".to_string());
            args.push(super::recon_common::validate_target(url)?);
        }
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
        args.push("-H".to_string());
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
    if params.subdomains {
        args.push("-subdomains".to_string());
    }
    if params.other_sources {
        args.push("-other-source".to_string());
    }
    if params.include_subdomains {
        args.push("-include-subdomains".to_string());
    }
    if params.crawl {
        args.push("-crawl".to_string());
    }
    if params.scan {
        args.push("-scan".to_string());
    }
    if params.fuzz {
        args.push("-fuzz".to_string());
    }
    if params.extract {
        args.push("-extract".to_string());
    }
    if params.discovery {
        args.push("-discovery".to_string());
    }
    if params.vulnerability {
        args.push("-vulnerability".to_string());
    }
    if params.information {
        args.push("-information".to_string());
    }
    if params.misconfiguration {
        args.push("-misconfiguration".to_string());
    }
    if params.exposure {
        args.push("-exposure".to_string());
    }
    if params.cve {
        args.push("-cve".to_string());
    }
    if params.cwe {
        args.push("-cwe".to_string());
    }
    if params.owasp {
        args.push("-owasp".to_string());
    }
    if params.hackerone {
        args.push("-hackerone".to_string());
    }
    if params.bugcrowd {
        args.push("-bugcrowd".to_string());
    }
    if params.intigriti {
        args.push("-intigriti".to_string());
    }
    if params.yeswehack {
        args.push("-yeswehack".to_string());
    }
    if params.hackenproof {
        args.push("-hackenproof".to_string());
    }
    if params.synack {
        args.push("-synack".to_string());
    }
    if params.cobalt {
        args.push("-cobalt".to_string());
    }
    if params.bugbounty {
        args.push("-bugbounty".to_string());
    }
    if params.hackerbug {
        args.push("-hackerbug".to_string());
    }
    if params.vdp {
        args.push("-vdp".to_string());
    }
    if params.responsible_disclosure {
        args.push("-responsible-disclosure".to_string());
    }
    if params.security_policy {
        args.push("-security-policy".to_string());
    }
    if params.terms_of_service {
        args.push("-terms-of-service".to_string());
    }
    if params.privacy_policy {
        args.push("-privacy-policy".to_string());
    }
    if params.cookie_policy {
        args.push("-cookie-policy".to_string());
    }
    if params.gdpr {
        args.push("-gdpr".to_string());
    }
    if params.ccpa {
        args.push("-ccpa".to_string());
    }
    if params.hipaa {
        args.push("-hipaa".to_string());
    }
    if params.pci_dss {
        args.push("-pci-dss".to_string());
    }
    if params.soc2 {
        args.push("-soc2".to_string());
    }
    if params.iso27001 {
        args.push("-iso27001".to_string());
    }
    if params.nist {
        args.push("-nist".to_string());
    }
    if params.cis {
        args.push("-cis".to_string());
    }
    if params.owasp_top10 {
        args.push("-owasp-top10".to_string());
    }
    if params.owasp_api_top10 {
        args.push("-owasp-api-top10".to_string());
    }
    if params.owasp_mobile_top10 {
        args.push("-owasp-mobile-top10".to_string());
    }
    if params.owasp_iot_top10 {
        args.push("-owasp-iot-top10".to_string());
    }
    if params.owasp_llm_top10 {
        args.push("-owasp-llm-top10".to_string());
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
            let params = normalize_cariddi_input(&payload).expect("normalize");
            assert!(!params.url.is_empty());
        }
    }
}
