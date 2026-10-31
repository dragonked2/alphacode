use super::*;

#[cfg(test)]
/// Extract semantic version for UI display/grouping.
pub(super) fn semver() -> &'static str {
    static SEMVER: OnceLock<String> = OnceLock::new();
    SEMVER.get_or_init(|| format!("v{}", crate::alphacode_build_meta::semver()))
}

#[cfg(test)]
pub(crate) fn calculate_input_lines(input: &str, line_width: usize) -> usize {
    use unicode_width::UnicodeWidthChar;

    if line_width == 0 {
        return 1;
    }
    if input.is_empty() {
        return 1;
    }

    let mut total_lines = 0;
    for line in input.split("\n") {
        if line.is_empty() {
            total_lines += 1;
        } else {
            let display_width: usize = line.chars().map(|c| c.width().unwrap_or(0)).sum();
            total_lines += display_width.div_ceil(line_width);
        }
    }
    total_lines.max(1)
}

pub(super) fn shorten_model_name(model: &str) -> String {
    if let Some(name) = crate::alphacode_provider_metadata::internal_model_display_name(model) {
        return name.to_string();
    }
    // The status line already fits or drops segments to suit the viewport.
    // Keep the actual model identity rather than guessing a family/version.
    let stem = model.rsplit(['/', '\\']).next().unwrap_or(model);
    stem.strip_suffix(".gguf")
        .or_else(|| stem.strip_suffix(".bin"))
        .unwrap_or(stem)
        .to_string()
}

pub fn format_status_for_debug(app: &dyn TuiState) -> String {
    let summary = build_status_summary_with_current_context(
        Some(&app.provider_model()),
        None,
        app.total_session_tokens(),
        app.connection_type().is_some(),
        app.context_limit(),
        app.current_context_tokens(),
    );
    let body: String = match app.status() {
        ProcessingStatus::Idle => {
            if let Some(notice) = app.status_notice() {
                format!("Idle (notice: {})", notice)
            } else if let Some((input, output)) = app.total_session_tokens() {
                format!(
                    "Idle (session: {}k in, {}k out)",
                    input / 1000,
                    output / 1000
                )
            } else if let Some(tip) =
                info_widget::occasional_status_tip(120, app.animation_elapsed() as u64)
            {
                format!("Idle ({})", tip)
            } else {
                "Idle".to_string()
            }
        }
        ProcessingStatus::Sending => "\u{25cf} Working/thinking...".to_string(),
        ProcessingStatus::Connecting(ref phase) => format!("\u{25cb} {}...", phase),
        ProcessingStatus::Thinking(_start) => {
            let elapsed = app.elapsed().map(|d| d.as_secs_f32()).unwrap_or(0.0);
            format!("\u{25cf} Thinking... ({:.1}s)", elapsed)
        }
        ProcessingStatus::Streaming => {
            let (input, output) = app.streaming_tokens();
            format!("\u{25b6} Streaming (\u{2191}{} \u{2193}{})", input, output)
        }
        ProcessingStatus::WaitingForNetwork { ref listener } => {
            format!("\u{25cc} Waiting for network to retry ({})", listener)
        }
        ProcessingStatus::RunningTool(ref name) => {
            if name == "batch"
                && let Some(progress) = app.batch_progress()
            {
                let completed = progress.completed;
                let total = progress.total;
                let mut status = format!("\u{25cf} Batch: {}/{} done", completed, total);
                if let Some(running) =
                    tools_ui::summarize_batch_running_tools_compact(&progress.running)
                {
                    status.push_str(&format!(", running: {}", running));
                }
                if let Some(last) = progress.last_completed.filter(|_| completed < total) {
                    status.push_str(&format!(", last: {}", last));
                }
                return status;
            }
            format!("\u{25cf} Tool: {}", name)
        }
    };
    if summary.is_empty() {
        body
    } else {
        format!("{summary} | {body}")
    }
}

/// Build a one-line status summary suitable for the header strip.
///
/// Combines the current model (shortened), the running task hint classified
/// via [`TaskKind`], the connection state, and any session token totals into
/// a single `key=value` line. Pure function: no I/O, no model calls.
// Legacy form for callers that only have cumulative session totals.
#[allow(dead_code)]
pub fn build_status_summary(
    model: Option<&str>,
    task_hint: Option<&str>,
    tokens: Option<(u64, u64)>,
    connected: bool,
) -> String {
    build_status_summary_with_context(model, task_hint, tokens, connected, None)
}

/// Format a context-usage percentage for the header strip. When no limit is
/// known the current context token count is shown instead of a percentage.
/// Returns e.g. `ctx=42%` or `ctx=118k`.
fn format_context_fragment(tokens: Option<u64>, context_limit: Option<usize>) -> Option<String> {
    let input = tokens?;
    if input == 0 {
        return None;
    }
    match context_limit {
        Some(limit) if limit > 0 => {
            let pct = ((input as f64 / limit as f64) * 100.0).round() as u64;
            Some(format!("ctx={}%", pct.min(999)))
        }
        _ if input < 1000 => Some(format!("ctx={input}")),
        _ => Some(format!("ctx={}k", input / 1000)),
    }
}

/// Build a status summary when only cumulative session totals are available.
/// Session totals cannot be used to infer current context usage.
pub fn build_status_summary_with_context(
    model: Option<&str>,
    task_hint: Option<&str>,
    tokens: Option<(u64, u64)>,
    connected: bool,
    context_limit: Option<usize>,
) -> String {
    build_status_summary_with_current_context(
        model,
        task_hint,
        tokens,
        connected,
        context_limit,
        None,
    )
}

fn build_status_summary_with_current_context(
    model: Option<&str>,
    task_hint: Option<&str>,
    tokens: Option<(u64, u64)>,
    connected: bool,
    context_limit: Option<usize>,
    context_tokens: Option<u64>,
) -> String {
    use crate::alphacode_provider_core::selection::TaskKind;

    let mut parts: Vec<String> = Vec::new();

    // Provider status indicator — show connected/disconnected state with
    // provider name so the header reads at a glance.
    let conn_icon = if connected { "\u{25cf}" } else { "\u{25cb}" };
    parts.push(format!(
        "{} {}",
        conn_icon,
        if connected { "connected" } else { "offline" }
    ));

    // Model (compact).
    if let Some(m) = model {
        parts.push(format!("model={}", shorten_model_name(m)));
    } else {
        parts.push("model=auto".to_string());
    }

    // Context usage — placed right after the model so it is always visible in
    // the header, even while streaming.
    if let Some(fragment) = format_context_fragment(context_tokens, context_limit) {
        parts.push(fragment);
    }

    // Task classification.
    if let Some(hint) = task_hint
        && !hint.trim().is_empty()
    {
        let kind = TaskKind::classify(hint);
        parts.push(format!("task={}", kind.as_str()));
    }

    // Session tokens.
    if let Some((input, output)) = tokens {
        parts.push(format!("tokens={}k/{}k", input / 1000, output / 1000));
    }

    // A middot reads as one calm strip; the pipe used to cut the header into
    // hard-edged cells that competed with the transcript.
    parts.join(" \u{00b7} ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_model_label_preserves_identity_and_local_weight_stems() {
        assert_eq!(shorten_model_name("qwen/qwen3:8b"), "qwen3:8b");
        assert_ne!(shorten_model_name("my-sonnet-4-copy"), "claudesonnet");
        assert_eq!(shorten_model_name("weights/custom.gguf"), "custom");
        assert_eq!(shorten_model_name("gpt-4o"), "gpt-4o");
        assert_eq!(shorten_model_name("claude-sonnet-4-5"), "claude-sonnet-4-5");
        let alias = crate::alphacode_provider_metadata::route_cloak::free_route_default_model();
        assert_eq!(
            shorten_model_name(&alias),
            crate::alphacode_provider_metadata::internal_model_display_name(&alias).unwrap()
        );
    }

    #[test]
    fn build_status_summary_includes_model_and_task() {
        let s = build_status_summary(
            Some("claude-opus-4-5"),
            Some("audit for XSS vulnerabilities"),
            Some((12_000, 4_500)),
            true,
        );
        assert!(s.contains("model="));
        assert!(s.contains("task=security"));
        assert!(s.contains("● connected"));
        assert!(s.contains("tokens=12k/4k"));
    }

    #[test]
    fn build_status_summary_with_context_shows_percent() {
        let s = build_status_summary_with_current_context(
            Some("claude-opus-4-5"),
            None,
            Some((84_000, 4_500)),
            true,
            Some(200_000),
            Some(84_000),
        );
        assert!(s.contains("ctx=42%"), "got: {}", s);
    }

    #[test]
    fn build_status_summary_with_context_falls_back_to_tokens() {
        let s = build_status_summary_with_current_context(
            Some("gpt-5"),
            None,
            Some((118_000, 500)),
            false,
            None,
            Some(118_000),
        );
        assert!(s.contains("ctx=118k"), "got: {}", s);
    }

    #[test]
    fn build_status_summary_with_context_omits_when_empty() {
        let s = build_status_summary_with_context(Some("gpt-5"), None, None, true, Some(200_000));
        assert!(!s.contains("ctx="), "got: {}", s);
    }

    #[test]
    fn cumulative_tokens_are_not_reported_as_context_usage() {
        let s = build_status_summary_with_context(
            Some("gpt-5"),
            None,
            Some((800_000, 10_000)),
            true,
            Some(200_000),
        );
        assert!(s.contains("tokens=800k/10k"), "got: {s}");
        assert!(!s.contains("ctx="), "got: {s}");
        let s = build_status_summary_with_current_context(
            Some("gpt-5"),
            None,
            Some((800_000, 10_000)),
            true,
            Some(200_000),
            Some(20_000),
        );
        assert!(s.contains("ctx=10%"), "got: {s}");
    }

    #[test]
    fn build_status_summary_omits_task_when_empty() {
        let s = build_status_summary(Some("gpt-5"), None, None, false);
        assert!(!s.contains("task="));
        assert!(s.contains("○ offline"));
    }
}
