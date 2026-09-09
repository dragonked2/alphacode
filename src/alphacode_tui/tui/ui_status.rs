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
    let model = if model.contains('/') {
        let stem = model.rsplit('/').next().unwrap_or(model);
        stem.strip_suffix(".gguf")
            .unwrap_or(stem)
            .strip_suffix(".bin")
            .unwrap_or(stem)
    } else {
        model
    };
    if model.contains('/') {
        // Slashed ids (e.g. qwen/qwen3.8-max-free): show a compact form
        // like `qwen3.8max` instead of the full path.
        return model
            .split('/')
            .next_back()
            .unwrap_or(model)
            .replace('-', "");
    }
    if model.contains("opus") {
        if model.contains("4-5") || model.contains("4.5") {
            return "claude4.5opus".to_string();
        }
        return "claudeopus".to_string();
    }
    if model.contains("sonnet") {
        if model.contains("3-5") || model.contains("3.5") {
            return "claude3.5sonnet".to_string();
        }
        return "claudesonnet".to_string();
    }
    if model.contains("haiku") {
        return "claudehaiku".to_string();
    }
    if model.starts_with("gpt-5") {
        return model.replace("gpt-", "gpt").replace("-", "");
    }
    if model.starts_with("gpt-4") {
        return model.replace("gpt-", "").replace("-", "");
    }
    if model.starts_with("gpt-3") {
        return "gpt3.5".to_string();
    }
    model.split('-').take(3).collect::<Vec<_>>().join("")
}

pub fn format_status_for_debug(app: &dyn TuiState) -> String {
    // Build a structured summary line that complements the human-readable
    // status produced below: model + detected task + connection + tokens.
    // Empty when the trait object does not expose those fields (legacy tests).
    let summary = build_status_summary(
        Some(&app.provider_model()),
        None,
        app.total_session_tokens(),
        app.connection_type().is_some(),
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
        ProcessingStatus::Sending => "Working/thinking...".to_string(),
        ProcessingStatus::Connecting(ref phase) => format!("{}...", phase),
        ProcessingStatus::Thinking(_start) => {
            let elapsed = app.elapsed().map(|d| d.as_secs_f32()).unwrap_or(0.0);
            format!("Thinking... ({:.1}s)", elapsed)
        }
        ProcessingStatus::Streaming => {
            let (input, output) = app.streaming_tokens();
            format!("Streaming (↑{} ↓{})", input, output)
        }
        ProcessingStatus::WaitingForNetwork { ref listener } => {
            format!("Waiting for network to retry ({})", listener)
        }
        ProcessingStatus::RunningTool(ref name) => {
            if name == "batch"
                && let Some(progress) = app.batch_progress()
            {
                let completed = progress.completed;
                let total = progress.total;
                let mut status = format!("Running batch: {}/{} done", completed, total);
                if let Some(running) =
                    tools_ui::summarize_batch_running_tools_compact(&progress.running)
                {
                    status.push_str(&format!(", running: {}", running));
                }
                if let Some(last) = progress.last_completed.filter(|_| completed < total) {
                    status.push_str(&format!(", last done: {}", last));
                }
                return status;
            }
            format!("Running tool: {}", name)
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
pub fn build_status_summary(
    model: Option<&str>,
    task_hint: Option<&str>,
    tokens: Option<(u64, u64)>,
    connected: bool,
) -> String {
    use crate::alphacode_provider_core::selection::TaskKind;

    let mut parts: Vec<String> = Vec::new();

    // Model (compact).
    if let Some(m) = model {
        parts.push(format!("model={}", shorten_model_name(m)));
    } else {
        parts.push("model=auto".to_string());
    }

    // Task classification.
    if let Some(hint) = task_hint
        && !hint.trim().is_empty()
    {
        let kind = TaskKind::classify(hint);
        parts.push(format!("task={}", kind.as_str()));
    }

    // Connection state.
    parts.push(if connected {
        "conn=ok".to_string()
    } else {
        "conn=offline".to_string()
    });

    // Session tokens.
    if let Some((input, output)) = tokens {
        parts.push(format!("tokens={}k/{}k", input / 1000, output / 1000));
    }

    parts.join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(s.contains("conn=ok"));
        assert!(s.contains("tokens=12k/4k"));
    }

    #[test]
    fn build_status_summary_omits_task_when_empty() {
        let s = build_status_summary(Some("gpt-5"), None, None, false);
        assert!(!s.contains("task="));
        assert!(s.contains("conn=offline"));
    }
}
