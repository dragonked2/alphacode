use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Status of a background task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundTaskStatus {
    Running,
    Completed,
    Superseded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTaskProgressKind {
    Determinate,
    Indeterminate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTaskProgressSource {
    Reported,
    ParsedOutput,
    Heuristic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackgroundTaskProgress {
    pub kind: BackgroundTaskProgressKind,
    pub percent: Option<f32>,
    pub message: Option<String>,
    pub current: Option<u64>,
    pub total: Option<u64>,
    pub unit: Option<String>,
    pub eta_seconds: Option<u64>,
    pub updated_at: String,
    pub source: BackgroundTaskProgressSource,
}

impl BackgroundTaskProgress {
    pub fn normalize(mut self) -> Self {
        if let (Some(current), Some(total)) = (self.current, self.total)
            && total > 0
            && self.percent.is_none()
        {
            let computed = (current as f64 / total as f64) * 100.0;
            self.percent = Some(((computed * 100.0).round() / 100.0) as f32);
        }

        self.percent = self
            .percent
            .map(|percent| ((percent.clamp(0.0, 100.0) * 100.0).round()) / 100.0);

        if matches!(self.kind, BackgroundTaskProgressKind::Indeterminate)
            && (self.percent.is_some()
                || matches!((self.current, self.total), (_, Some(total)) if total > 0))
        {
            self.kind = BackgroundTaskProgressKind::Determinate;
        }

        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackgroundTaskProgressEvent {
    pub task_id: String,
    pub tool_name: String,
    pub display_name: Option<String>,
    pub session_id: String,
    pub progress: BackgroundTaskProgress,
}

/// Event sent when a background task completes.
#[derive(Debug, Clone)]
pub struct BackgroundTaskCompleted {
    pub task_id: String,
    pub tool_name: String,
    pub display_name: Option<String>,
    pub session_id: String,
    pub status: BackgroundTaskStatus,
    pub exit_code: Option<i32>,
    pub output_preview: String,
    pub output_file: PathBuf,
    pub duration_secs: f64,
    pub notify: bool,
    pub wake: bool,
}

/// Render a one-line human-readable progress display: an optional progress bar,
/// a textual summary, and the source label (for example `[######----] 50% (reported)`).
pub fn format_progress_display(progress: &BackgroundTaskProgress, width: usize) -> String {
    let summary = format_progress_summary(progress);
    let source = progress_source_label(&progress.source);
    match render_progress_bar(progress, width) {
        Some(bar) => format!("{} {} ({})", bar, summary, source),
        None => format!("{} ({})", summary, source),
    }
}

/// Build the textual portion of a progress display (percentage/counts/message).
pub fn format_progress_summary(progress: &BackgroundTaskProgress) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(percent) = progress.percent {
        parts.push(format!("{:.0}%", percent));
    } else if let (Some(current), Some(total)) = (progress.current, progress.total) {
        let mut counts = format!("{}/{}", current, total);
        if let Some(unit) = progress.unit.as_deref() {
            counts.push(' ');
            counts.push_str(unit);
        }
        parts.push(counts);
    } else if let Some(unit) = progress.unit.as_deref() {
        parts.push(unit.to_string());
    }

    if let Some(message) = progress.message.as_deref() {
        parts.push(message.to_string());
    }

    if parts.is_empty() {
        match progress.kind {
            BackgroundTaskProgressKind::Determinate => "progress reported".to_string(),
            BackgroundTaskProgressKind::Indeterminate => "working".to_string(),
        }
    } else {
        parts.join(" · ")
    }
}

fn progress_source_label(source: &BackgroundTaskProgressSource) -> &'static str {
    match source {
        BackgroundTaskProgressSource::Reported => "reported",
        BackgroundTaskProgressSource::ParsedOutput => "parsed",
        BackgroundTaskProgressSource::Heuristic => "estimated",
    }
}

/// Render a smooth progress bar for a determinate progress value, if known.
/// Uses Unicode block characters for a polished, filled appearance with inline percentage.
pub fn render_progress_bar(progress: &BackgroundTaskProgress, width: usize) -> Option<String> {
    let percent = progress.percent?;
    let width = width.max(6);
    let pct_str = format!("{}%", percent as u8);
    let pct_len = pct_str.len() + 2; // space on each side
    let bar_width = width.saturating_sub(pct_len + 2).max(4); // +2 for brackets
    let filled = ((percent / 100.0) * bar_width as f32).round() as usize;
    let filled = filled.min(bar_width);
    let empty = bar_width.saturating_sub(filled);
    // Use full block for filled, left-right half block for transition, light shade for empty
    let filled_str = "\u{2588}".repeat(filled);
    let transition = if empty > 0 { "\u{2590}" } else { "" };
    let empty_str = "\u{2591}".repeat(empty.saturating_sub(1));
    Some(format!(
        "[{}{}{} {}]",
        filled_str, transition, empty_str, pct_str
    ))
}
