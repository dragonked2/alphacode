//! Headless wire between the bug-bounty orchestrator and the first-party
//! recon/scanner tools.
//!
//! `BugBountyOrchestrator::execute_phase_tasks` accepts an executor closure,
//! but the individual tool modules (`tool::subfinder`, `tool::httpx`, ...) are
//! private to this `tool` module, so the CLI executor could only have reached
//! them by shelling the binaries itself — which would have duplicated every
//! flag schema, output cap, timeout, and input validation this module already
//! contains. This bridge exposes exactly the orchestrator's tool set through
//! the [`Tool::execute`] contract so the executor can drive the *same* code
//! the agent turn drives.
//!
//! Every tool listed here must also appear in
//! [`super::bugbounty_doctor::TOOL_SPECS`] so `alphacode bugbounty
//! doctor/install` covers it; a tool present here but absent there would
//! produce runs that always fail with "binary not found".

use anyhow::{Result, bail};
use serde_json::Value;

use super::{Tool, ToolContext, ToolOutput};

/// Tools the orchestrator's task graph can drive.
///
/// Kept in one place so callers get a precise error naming the supported set
/// when a task references something unregistered.
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

/// Execute one first-party tool call on behalf of an orchestrator task.
///
/// `name` is the orchestrator's `BountyTask.tool` value; `input` is the tool's
/// own JSON schema input (hydrated by the executor). Construction of the tool
/// struct is trivial (plain fieldless structs), so there is no registry to
/// spin up and no provider dependency — this runs the external binary via the
/// tool's own bounded-execution path.
pub async fn run_task_tool(name: &str, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
    // Spawn failures already carry an actionable shape: missing Go tools
    // auto-install once and retry inside `recon_common`, and whatever still
    // fails surfaces "Retry with `alphacode bugbounty install <tool>`" in the
    // error text, so the bridge passes the result through unchanged.
    match name {
        "subfinder" => {
            super::subfinder::SubfinderTool::new()
                .execute(input, ctx)
                .await
        }
        "amass" => super::amass::AmassTool::new().execute(input, ctx).await,
        "assetfinder" => {
            super::assetfinder::AssetfinderTool::new()
                .execute(input, ctx)
                .await
        }
        "dnsx" => super::dnsx::DnsxTool::new().execute(input, ctx).await,
        "httpx" => super::httpx::HttpxTool::new().execute(input, ctx).await,
        "httprobe" => {
            super::httprobe::HttprobeTool::new()
                .execute(input, ctx)
                .await
        }
        "katana" => super::katana::KatanaTool::new().execute(input, ctx).await,
        "gau" => super::gau::GauTool::new().execute(input, ctx).await,
        "waybackurls" => {
            super::waybackurls::WaybackurlsTool::new()
                .execute(input, ctx)
                .await
        }
        "feroxbuster" => {
            super::feroxbuster::FeroxbusterTool::new()
                .execute(input, ctx)
                .await
        }
        "gospider" => {
            super::gospider::GospiderTool::new()
                .execute(input, ctx)
                .await
        }
        "nuclei" => super::nuclei::NucleiTool::new().execute(input, ctx).await,
        "nikto" => super::nikto::NiktoTool::new().execute(input, ctx).await,
        "dalfox" => super::dalfox::DalfoxTool::new().execute(input, ctx).await,
        "corsy" => super::corsy::CorsyTool::new().execute(input, ctx).await,
        "crlfuzz" => super::crlfuzz::CrlfuzzTool::new().execute(input, ctx).await,
        "sqlmap" => super::sqlmap::SqlmapTool::new().execute(input, ctx).await,
        "gobuster" => {
            super::gobuster::GobusterTool::new()
                .execute(input, ctx)
                .await
        }
        "ffuf" => super::ffuf::FfufTool::new().execute(input, ctx).await,
        _ => bail!(
            "tool '{name}' is not wired for orchestrator tasks; supported: {}",
            SUPPORTED_TASK_TOOLS.join(", ")
        ),
    }
}
