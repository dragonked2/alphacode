//! `alphacode doctor` — one command that answers "why isn't this working?".
//!
//! AlphaCode had three separate diagnostics, each reachable only by remembering
//! which one you wanted: `auth doctor` (credentials), `provider-doctor`
//! (end-to-end model checkpoints), and `bugbounty doctor` (recon toolchain).
//! None of them looked at the things that break a session on a new machine —
//! terminal capability, terminal width, leftover processes holding the console,
//! emoji/styling configuration. The most common first-run failures were
//! therefore invisible from the CLI.
//!
//! `doctor` is the top-level entry point. It is read-only: it probes and
//! reports, and points at the deeper command when a section needs one.
//!
//! Output is rendered through
//! [`alphacode_core::output_enhance`](crate::alphacode_core::output_enhance),
//! so it respects `NO_COLOR`, non-TTY output, and terminal width — a diagnostic
//! that emits escape garbage into a bug report is worse than no diagnostic.

use anyhow::Result;

use crate::alphacode_core::console;
use crate::alphacode_core::output_enhance as enhance;

/// One section of the report. `ok` is `None` for informational rows that are
/// not pass/fail (versions, paths).
struct Check {
    label: &'static str,
    ok: Option<bool>,
    detail: String,
}

impl Check {
    fn pass(label: &'static str, detail: impl Into<String>) -> Self {
        Self {
            label,
            ok: Some(true),
            detail: detail.into(),
        }
    }

    fn fail(label: &'static str, detail: impl Into<String>) -> Self {
        Self {
            label,
            ok: Some(false),
            detail: detail.into(),
        }
    }

    fn info(label: &'static str, detail: impl Into<String>) -> Self {
        Self {
            label,
            ok: None,
            detail: detail.into(),
        }
    }
}

pub fn run_doctor(json: bool) -> Result<()> {
    let checks = collect_checks();
    let tools = crate::alphacode_app_core::bugbounty_doctor::probe();
    let failed = checks.iter().filter(|c| c.ok == Some(false)).count();

    if json {
        render_json(&checks, &tools, failed)?;
    } else {
        render_human(&checks, &tools, failed);
    }

    // A health check that always exits 0 cannot gate a pipeline, and gating a
    // pipeline is the single most common reason to run `doctor` from a
    // script. Exit non-zero when a check failed so `alphacode doctor` is
    // usable as a CI precondition.
    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn collect_checks() -> Vec<Check> {
    let mut checks = Vec::new();

    // ── Build ────────────────────────────────────────────────────────────
    checks.push(Check::pass(
        "version",
        format!("alphacode {}", crate::alphacode_build_meta::version()),
    ));
    checks.push(Check::info(
        "binary",
        std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "<unknown>".to_string()),
    ));

    // ── Terminal ─────────────────────────────────────────────────────────
    // Width and styling are the two settings that produce the most confusing
    // "it looks broken" reports, and neither is visible from the TUI.
    let width = console::terminal_width();
    checks.push(if width < console::MIN_USABLE_WIDTH + 20 {
        Check::fail(
            "terminal width",
            format!("{width} columns — very narrow; panels and tables will collapse"),
        )
    } else {
        Check::pass("terminal width", format!("{width} columns"))
    });
    checks.push(Check::info(
        "styling",
        if console::ansi_enabled() {
            "color enabled".to_string()
        } else {
            "color disabled (not a TTY, NO_COLOR, quiet mode, or TERM=dumb)".to_string()
        },
    ));
    checks.push(Check::info(
        "unicode",
        if console::unicode_supported() {
            "box-drawing supported".to_string()
        } else {
            "ASCII fallback (set ALPHACODE_ASCII=0 to force Unicode)".to_string()
        },
    ));

    // ── Configuration ────────────────────────────────────────────────────
    let config = crate::config::config();
    checks.push(Check::info(
        "provider",
        if config.provider.default_provider.is_some() {
            "default provider set".to_string()
        } else {
            "using automatic provider selection".to_string()
        },
    ));
    checks.push(Check::info(
        "default model",
        match config.provider.default_model.as_deref() {
            Some(model) => public_model_label(model),
            None => "<automatic>".to_string(),
        },
    ));
    checks.push(Check::info(
        "emoji",
        if crate::output_style::emoji_enabled() {
            "enabled".to_string()
        } else {
            "disabled (display.emoji = false)".to_string()
        },
    ));

    // ── Auto-update ──────────────────────────────────────────────────────
    // Auto-update runs on a background thread with no TUI affordance, so the
    // first question anyone has when it does not fire is "did it even run?".
    // Answer it from the same gates the updater uses, plus the on-disk
    // throttle state, rather than asking them to grep a log file.
    checks.push(Check::pass(
        "build type",
        if crate::update::is_release_build() {
            "release build (updates come from GitHub releases)".to_string()
        } else {
            "dev/source build (updates come from the local checkout)".to_string()
        },
    ));

    let update_metadata =
        crate::alphacode_app_core::update::UpdateMetadata::load().unwrap_or_default();
    checks.push(Check::info(
        "last update check",
        match update_metadata.last_check.elapsed() {
            Ok(elapsed) if update_metadata.should_check() => {
                format!("{} ago (check is due now)", describe_age(elapsed.as_secs()))
            }
            Ok(elapsed) => format!(
                "{} ago (next automatic check in {})",
                describe_age(elapsed.as_secs()),
                describe_age(30 * 60 - elapsed.as_secs().min(30 * 60))
            ),
            Err(_) => "never".to_string(),
        },
    ));

    match crate::update::auto_update_skip_reason_now(!update_metadata.should_check()) {
        Some(reason) => checks.push(Check::fail(
            "auto-update",
            format!("not running — {reason}"),
        )),
        None => {
            let installs = default_auto_update_flag();
            checks.push(Check::pass(
                "auto-update",
                if installs {
                    "enabled; installs automatically when a release is found"
                } else {
                    "enabled; notifies only (--auto-update is off)"
                },
            ));
        }
    }

    // ── Leftover processes ───────────────────────────────────────────────
    // A previous update/reload can leave an alphacode process attached to this
    // console. It keeps the terminal in raw mode, which reads to the user as
    // "alphacode closed but my shell is broken".
    let live = crate::storage::session_presence();
    let self_pid = std::process::id();
    let others: Vec<_> = live.iter().filter(|p| p.pid != self_pid).collect();
    checks.push(if others.is_empty() {
        Check::pass("leftover processes", "none")
    } else {
        Check::fail(
            "leftover processes",
            format!(
                "{} other alphacode session(s) still running (pid {}). If the terminal \
                 seems stuck, close those first — they may hold raw mode.",
                others.len(),
                others
                    .iter()
                    .map(|p| p.pid.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
    });

    checks
}

/// Render the leading status marker for a check row.
///
/// Deliberately not built from [`enhance::status_line`]: that helper emits
/// `indent + glyph + gap + message`, so using it with an empty message and
/// trimming would strip exactly the padding that aligns the labels below.
fn marker(check: &Check) -> String {
    let (token, label) = match check.ok {
        Some(true) => (enhance::PaletteToken::Success, enhance::PREFIX_OK),
        Some(false) => (enhance::PaletteToken::Warn, enhance::PREFIX_WARN),
        None => return " ".repeat(enhance::STATUS_INDENT + enhance::STATUS_GLYPH_WIDTH + 1),
    };
    let (r, g, b) = enhance::pick_for_terminal_bg(token);
    format!(
        "{}{}{} ",
        " ".repeat(enhance::STATUS_INDENT),
        enhance::rgb_escape(r, g, b, true),
        enhance::pad_end_to(&enhance::status_glyph(label), enhance::STATUS_GLYPH_WIDTH),
    )
}

/// Compact human duration for the update-check age, e.g. `12s`, `4m`, `3h`.
fn describe_age(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        3600..=86399 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86400),
    }
}

/// The `--auto-update` default, read from clap rather than hardcoded here so
/// this row cannot drift away from what the argument parser actually does.
fn default_auto_update_flag() -> bool {
    use clap::Parser;
    crate::cli::args::Args::parse_from(["alphacode"]).auto_update
}

/// The model label to show, never the raw routing id.
///
/// Routing aliases (the free gateway resolves them server-side) must not reach
/// a diagnostic verbatim. A diagnostic that prints one re-introduces exactly
/// the internal detail the rest of the UI maps away, so resolve through the
/// same display-name layer the header uses.
fn public_model_label(model: &str) -> String {
    if let Some(name) = crate::alphacode_provider_metadata::internal_model_display_name(model) {
        return name.to_string();
    }
    if let Some(name) =
        crate::alphacode_app_core::agent::free_pool_rotation::free_pool_display_name(model)
    {
        return name.to_string();
    }
    model.to_string()
}

fn render_human(
    checks: &[Check],
    tools: &[crate::alphacode_app_core::bugbounty_doctor::ToolReport],
    failed: usize,
) {
    println!(
        "{}",
        crate::cli::output::CliOutput::section("Alphacode doctor")
    );
    println!();

    // Column width is measured in display cells, not bytes: a label with a
    // wide glyph would otherwise push its detail one column to the right and
    // break the alignment of every row below it.
    let label_width = checks
        .iter()
        .map(|c| enhance::display_width(c.label))
        .max()
        .unwrap_or(10);
    for check in checks {
        println!(
            "{}{}  {}",
            marker(check),
            enhance::pad_end_to(check.label, label_width),
            check.detail
        );
    }

    // Summary first, then the deeper pointers: the summary is what a human
    // (or a CI log) reads; the pointers are what they read next.
    println!();
    if failed == 0 {
        println!(
            "{}",
            crate::cli::output::CliOutput::success("All checks passed")
        );
    } else {
        println!(
            "{}",
            crate::cli::output::CliOutput::error(&format!(
                "{failed} check{} need{} attention",
                if failed == 1 { "" } else { "s" },
                if failed == 1 { "s" } else { "" },
            ))
        );
    }

    println!();
    println!(
        "{}",
        crate::alphacode_app_core::bugbounty_doctor::render_markdown(tools)
    );
    println!();
    println!("Deeper diagnostics:");
    println!("  alphacode auth doctor      credentials and provider auth");
    println!("  alphacode provider-doctor  end-to-end model checkpoints");
    println!("  alphacode bugbounty doctor recon toolchain only");
}

fn render_json(
    checks: &[Check],
    tools: &[crate::alphacode_app_core::bugbounty_doctor::ToolReport],
    failed: usize,
) -> Result<()> {
    use serde_json::json;

    let rows = checks
        .iter()
        .map(|c| {
            json!({
                "label": c.label,
                "status": match c.ok {
                    Some(true) => "ok",
                    Some(false) => "warn",
                    None => "info",
                },
                "detail": c.detail,
            })
        })
        .collect::<Vec<_>>();

    let tool_rows = tools
        .iter()
        .map(|t| {
            json!({
                "binary": t.binary,
                "purpose": t.purpose,
                "present": matches!(t.status, crate::alphacode_app_core::bugbounty_doctor::ToolStatus::Present { .. }),
                "install_hint": t.install_hint,
            })
        })
        .collect::<Vec<_>>();

    let payload = json!({
        "version": crate::alphacode_build_meta::version(),
        "ok": failed == 0,
        "failed_checks": failed,
        "checks": rows,
        "recon_tools": tool_rows,
    });
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}
