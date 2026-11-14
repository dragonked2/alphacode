//! Scanner tasks execute through the same registry gates as ordinary agent calls.
use super::{Registry, Tool, ToolContext, ToolOutput};
use anyhow::{Result, bail};
use serde_json::Value;
use std::sync::Arc;

pub const SUPPORTED_TASK_TOOLS: &[&str] = &[
    "subfinder",
    "amass",
    "assetfinder",
    "dnsx",
    "httpx",
    "httprobe",
    "katana",
    "gau",
    "waybackurls",
    "feroxbuster",
    "gospider",
    "nuclei",
    "nikto",
    "dalfox",
    "corsy",
    "crlfuzz",
    "sqlmap",
    "gobuster",
    "ffuf",
];

pub async fn run_task_tool(name: &str, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
    let tool: Arc<dyn Tool> = match name {
        "subfinder" => Arc::new(super::subfinder::SubfinderTool::new()),
        "amass" => Arc::new(super::amass::AmassTool::new()),
        "assetfinder" => Arc::new(super::assetfinder::AssetfinderTool::new()),
        "dnsx" => Arc::new(super::dnsx::DnsxTool::new()),
        "httpx" => Arc::new(super::httpx::HttpxTool::new()),
        "httprobe" => Arc::new(super::httprobe::HttprobeTool::new()),
        "katana" => Arc::new(super::katana::KatanaTool::new()),
        "gau" => Arc::new(super::gau::GauTool::new()),
        "waybackurls" => Arc::new(super::waybackurls::WaybackurlsTool::new()),
        "feroxbuster" => Arc::new(super::feroxbuster::FeroxbusterTool::new()),
        "gospider" => Arc::new(super::gospider::GospiderTool::new()),
        "nuclei" => Arc::new(super::nuclei::NucleiTool::new()),
        "nikto" => Arc::new(super::nikto::NiktoTool::new()),
        "dalfox" => Arc::new(super::dalfox::DalfoxTool::new()),
        "corsy" => Arc::new(super::corsy::CorsyTool::new()),
        "crlfuzz" => Arc::new(super::crlfuzz::CrlfuzzTool::new()),
        "sqlmap" => Arc::new(super::sqlmap::SqlmapTool::new()),
        "gobuster" => Arc::new(super::gobuster::GobusterTool::new()),
        "ffuf" => Arc::new(super::ffuf::FfufTool::new()),
        _ => bail!(
            "tool '{name}' is not wired for orchestrator tasks; supported: {}",
            SUPPORTED_TASK_TOOLS.join(", ")
        ),
    };
    let registry = Registry::empty();
    registry.register(name.into(), tool).await;
    registry.execute(name, input, ctx).await
}
