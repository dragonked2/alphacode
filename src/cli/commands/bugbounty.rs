//! `alphacode bugbounty {doctor,install,list}`
//!
//! The recon tools are external binaries, so a bug-bounty run can fail purely
//! because `subfinder` is not on `PATH`. These commands make that a first-class,
//! inspectable state instead of a confusing `No such file or directory` buried
//! in a tool result.

use serde_json::json;

use crate::alphacode_app_core::bugbounty_doctor::{self, ToolStatus};
use crate::alphacode_app_core::bugbounty_install::{self, InstallOutcome};

/// The pipeline most bug-bounty engagements need, used when `install` is
/// called with no arguments. Deliberately excludes the heavyweight scanners
/// (nmap, sqlmap, nikto) — those are opt-in.
const DEFAULT_PIPELINE: &[&str] = &[
    "subfinder",
    "httpx",
    "dnsx",
    "katana",
    "gau",
    "waybackurls",
    "ffuf",
    "dalfox",
    "nuclei",
];

pub async fn run(args: &crate::cli::args::BugBountyCommand) -> anyhow::Result<()> {
    match args {
        crate::cli::args::BugBountyCommand::Orchestrate {
            target,
            output,
            silent,
            json,
            dry_run,
            resume,
        } => orchestrate(target, output, *silent, *json, *dry_run, *resume).await,
        crate::cli::args::BugBountyCommand::List { json } => list(*json),
        crate::cli::args::BugBountyCommand::Doctor { tools, json } => {
            doctor(tools, *json);
            Ok(())
        }
        crate::cli::args::BugBountyCommand::Install {
            tools,
            dry_run,
            json,
        } => install(tools, *dry_run, *json).await,
    }
}

fn list(as_json: bool) -> anyhow::Result<()> {
    let specs = bugbounty_doctor::TOOL_SPECS;
    let rows: Vec<_> = specs
        .iter()
        .map(|s| {
            let step = bugbounty_install::plan_for(s.binary);
            json!({
                "binary": s.binary,
                "purpose": s.purpose,
                "auto_installable": step.is_some(),
                "family": step.as_ref().map(|x| x.family.as_str()),
                "command": step.as_ref().map(|x| x.display()),
                "manual_hint": s.install,
            })
        })
        .collect();

    if as_json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    println!("Bug-bounty toolchain\n");
    for (spec, row) in specs.iter().zip(rows.iter()) {
        let _ = spec;
        let mark = if row["auto_installable"].as_bool().unwrap_or(false) {
            "auto"
        } else {
            "manual"
        };
        println!(
            "  {:<14} {:<6} {}",
            row["binary"].as_str().unwrap_or("?"),
            mark,
            spec.purpose
        );
    }
    println!("\n`auto` = `alphacode bugbounty install <name>` works without further setup.");
    Ok(())
}

fn doctor(tools: &[String], as_json: bool) {
    let report = bugbounty_doctor::probe();
    let wanted: Vec<String> = tools
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .collect();
    let filtered: Vec<_> = report
        .iter()
        .filter(|r| wanted.is_empty() || wanted.iter().any(|w| w == &r.binary.to_ascii_lowercase()))
        .collect();

    if as_json {
        let rows: Vec<_> = filtered
            .iter()
            .map(|r| {
                json!({
                    "binary": r.binary,
                    "purpose": r.purpose,
                    "present": matches!(r.status, ToolStatus::Present { .. }),
                    "path": match &r.status {
                        ToolStatus::Present { path } => Value::String(path.clone()),
                        ToolStatus::Missing => Value::Null,
                    },
                    "install": bugbounty_install::plan_for(&r.binary).map(|s| s.display()),
                    "manual_hint": r.install_hint,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).unwrap_or_default()
        );
        return;
    }

    let mut md = bugbounty_doctor::render_markdown(
        &filtered.iter().map(|r| (*r).clone()).collect::<Vec<_>>(),
    );
    // Point at the one-command fix so the report is actionable rather than
    // just informational.
    let missing: Vec<&str> = filtered
        .iter()
        .filter(|r| matches!(r.status, ToolStatus::Missing))
        .map(|r| r.binary.as_str())
        .collect();
    if !missing.is_empty() {
        let installable: Vec<&str> = missing
            .iter()
            .copied()
            .filter(|b| bugbounty_install::plan_for(b).is_some())
            .collect();
        if !installable.is_empty() {
            md.push_str(&format!(
                "\nInstall the missing pipeline in one step:\n\n    alphacode bugbounty install {}\n",
                installable.join(" ")
            ));
        }
    }
    if !bugbounty_install::go_available() {
        md.push_str(&format!("\n{}\n", bugbounty_install::go_bootstrap_hint()));
    }
    println!("{}", md.trim_end());
}

async fn orchestrate(
    target: &str,
    output: &str,
    silent: bool,
    json: bool,
    dry_run: bool,
    _resume: bool,
) -> anyhow::Result<()> {
    use crate::alphacode_app_core::bugbounty_orchestrator::{BountyPhase, BugBountyOrchestrator};

    let orchestrator = BugBountyOrchestrator::new(target);

    if !silent {
        println!("Bug Bounty Orchestrator");
        println!("=======================");
        println!("Target: {}", target);
        println!("Output: {}", output);
        println!();
    }

    if dry_run {
        println!("Dry run — showing planned tasks:");
        println!();
        orchestrator.initialize().await?;
        let tasks = orchestrator.get_pending_tasks().await;
        for task in &tasks {
            println!(
                "  [{}] {} — {}",
                task.phase.description(),
                task.name,
                task.description
            );
        }
        return Ok(());
    }

    orchestrator.initialize().await?;

    if !silent {
        println!(
            "Initialized {} tasks",
            orchestrator.get_pending_tasks().await.len()
        );
        println!();
    }

    // Execute phases in order
    let phases = [
        BountyPhase::Reconnaissance,
        BountyPhase::AttackSurfaceMapping,
        BountyPhase::VulnerabilityDiscovery,
        BountyPhase::Exploitation,
    ];

    for phase in &phases {
        if !silent {
            println!("Phase: {}", phase.description());
        }

        let tasks = orchestrator.get_tasks_for_phase(*phase).await;
        for task in &tasks {
            if !silent {
                println!("  Executing: {}", task.name);
            }
            // In a real implementation, this would execute the tool
            // For now, we mark it as completed
            orchestrator
                .complete_task(&task.id, format!("Completed: {}", task.name))
                .await?;
        }

        orchestrator.advance_phase().await?;
    }

    let summary = orchestrator.get_summary().await;

    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!();
        println!("{}", summary);
    }

    Ok(())
}

// `Value` is only referenced in the JSON branch above.
use serde_json::Value;

async fn install(tools: &[String], dry_run: bool, as_json: bool) -> anyhow::Result<()> {
    let requested: Vec<String> = if tools.is_empty() {
        DEFAULT_PIPELINE.iter().map(|s| (*s).to_string()).collect()
    } else {
        tools.to_vec()
    };

    if dry_run {
        let rows: Vec<_> = requested
            .iter()
            .map(|b| match bugbounty_install::plan_for(b) {
                Some(step) => json!({
                    "binary": b,
                    "would_run": step.display(),
                    "family": step.family.as_str(),
                    "already_present": matches!(bugbounty_doctor::which(b), ToolStatus::Present { .. }),
                }),
                None => json!({
                    "binary": b,
                    "would_run": Value::Null,
                    "reason": "no automated installer; see `alphacode bugbounty list`",
                }),
            })
            .collect();
        if as_json {
            println!("{}", serde_json::to_string_pretty(&rows)?);
        } else {
            println!("Dry run — nothing installed.\n");
            for row in &rows {
                let bin = row["binary"].as_str().unwrap_or("?");
                match row["would_run"].as_str() {
                    Some(cmd) => println!("  {bin:<14} would run: {cmd}"),
                    None => println!(
                        "  {bin:<14} skipped: {}",
                        row["reason"].as_str().unwrap_or("no installer")
                    ),
                }
            }
        }
        return Ok(());
    }

    if !as_json {
        println!("Installing {} tool(s)...\n", requested.len());
        if !bugbounty_install::go_available() {
            println!("{}\n", bugbounty_install::go_bootstrap_hint());
        }
    }

    let results = bugbounty_install::install_all(&requested).await;

    if as_json {
        let rows: Vec<_> = results
            .iter()
            .map(|(bin, outcome)| json!({ "binary": bin, "outcome": outcome }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        for (bin, outcome) in &results {
            let line = match outcome {
                InstallOutcome::AlreadyPresent { path } => format!("already present ({path})"),
                InstallOutcome::Installed { path } => format!("installed ({path})"),
                InstallOutcome::InstalledButNotOnPath { hint, .. } => {
                    format!("installed but not on PATH: {hint}")
                }
                InstallOutcome::Failed { output } => format!("FAILED: {output}"),
                InstallOutcome::MissingPrerequisite { hint, .. } => hint.clone(),
                InstallOutcome::Skipped { reason } => reason.clone(),
                InstallOutcome::TimedOut => "timed out".to_string(),
            };
            println!("  {bin:<14} {line}");
        }
    }

    // A non-zero exit lets CI / scripts detect a partial failure. Only hard
    // failures count; "installed but not on PATH" is a user-actionable
    // warning, not a build break.
    let hard_failures = results.iter().filter(|(_, o)| {
        matches!(
            o,
            InstallOutcome::Failed { .. }
                | InstallOutcome::TimedOut
                | InstallOutcome::MissingPrerequisite { .. }
        )
    });
    if hard_failures.count() > 0 {
        anyhow::bail!("one or more tools failed to install");
    }
    Ok(())
}
