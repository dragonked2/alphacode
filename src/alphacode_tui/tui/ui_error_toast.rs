//! Error toast widget — short-lived, role-colored, dismissible.
//!
//! # Why this exists
//!
//! Before this widget, every error in the TUI went through one of two
//! paths:
//!
//! 1. `app.push_display_message(DisplayMessage { role: "error", .. })` —
//!    the error became an inline markdown block, visually identical to a
//!    user message. Users had to scroll up to find it after the next
//!    turn scrolled the transcript.
//! 2. A silent log line via `crate::logging::warn!`. The user never
//!    saw it.
//!
//! Neither path is right for *transient* errors: "OAuth flow cancelled",
//! "rate-limited by upstream", "checksum mismatch, will retry". They
//! deserve a brief, distinct, dismissible surface that doesn't pollute
//! the transcript.
//!
//! # Design
//!
//! - A bounded queue (max 4 visible toasts) so a flood of errors doesn't
//!   fill the screen.
//! - Each toast has an `expires_at: Instant`; rendering skips expired
//!   ones and prunes them lazily.
//! - Three severities: `Error` (red), `Warning` (amber), `Info` (blue),
//!   all drawn from the existing palette roles so they theme correctly.
//! - Per-toast `Esc` dismissal hook is wired in the input layer (not in
//!   this file), so the widget stays render-only.
//! - Long failures collapse to a short preview while complete diagnostics stay
//!   available in the expandable hint.
//! - Toasts in one stack share a right edge and width, so notifications line up
//!   instead of appearing as unrelated floating boxes.
//!
//! # Public API
//!
//! ```ignore
//! use crate::alphacode_tui::tui::ui_error_toast;
//! ui_error_toast::push_error("OAuth flow was cancelled by the user.");
//! ui_error_toast::push_warning("Rate-limited; retrying in 30s.");
//! ui_error_toast::push_info("New session started.");
//! ui_error_toast::clear();   // dismiss all toasts immediately
//! ```

use std::sync::Mutex;
use std::time::{Duration, Instant};

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::alphacode_tui::tui::ui_transitions::{Transition, ease_out_cubic};
use crate::alphacode_tui_style::icons::Icon;
use crate::alphacode_tui_style::palette::{Role, role_color};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Severity tier — drives the color and icon of the toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Something failed and the user should notice.
    Error,
    /// Heads-up; non-fatal but worth a glance.
    Warning,
    /// FYI / status update.
    Info,
}

impl Severity {
    /// Icon shown at the start of the toast line.
    fn icon(self) -> &'static str {
        match self {
            Self::Error => Icon::Error.glyph(),
            Self::Warning => Icon::Warn.glyph(),
            Self::Info => Icon::Info.glyph(),
        }
    }

    /// Label prefix (`Error`, `Warning`, `Info`) used by screen readers
    /// and printed as the toast's "kind" badge after the icon.
    fn label(self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::Warning => "Warning",
            Self::Info => "Info",
        }
    }

    /// Role used for the border + icon color.
    fn role(self) -> Role {
        match self {
            Self::Error => Role::Error,
            Self::Warning => Role::Warning,
            Self::Info => Role::Info,
        }
    }
}

/// One queued toast.
#[derive(Debug, Clone)]
pub struct Toast {
    pub severity: Severity,
    pub message: String,
    pub hint: Option<String>,
    pub recovery: Option<String>,
    pub created_at: Instant,
    pub ttl: Duration,
    /// Whether the toast is expanded to show the full message and hint.
    /// Collapsed toasts show a truncated preview; expanded show everything.
    pub expanded: bool,
}

impl Toast {
    fn expires_at(&self) -> Instant {
        self.created_at + self.ttl
    }

    fn is_expired(&self, now: Instant) -> bool {
        now >= self.expires_at()
    }
}

/// Maximum number of toasts shown at once. Beyond this the oldest are
/// evicted silently — a flood of errors should not push the chat area off
/// the screen.
pub const MAX_VISIBLE_TOASTS: usize = 4;

/// Default time-to-live for a toast.
pub const DEFAULT_TTL: Duration = Duration::from_secs(6);

/// Maximum toast width in cells. Expanded details use a wider cap.
pub const MAX_TOAST_WIDTH: u16 = 60;

/// Maximum toast width when expanded.
pub const MAX_TOAST_WIDTH_EXPANDED: u16 = 100;

/// Collapsed messages stay short; full diagnostics remain available by
/// expanding the toast.
const TOAST_PREVIEW_WIDTH: usize = 48;
const MAX_TOAST_DETAIL_CHARS: usize = 8_000;

/// Process-global toast queue. A `Mutex<Vec<Toast>>` is fine here: the
/// queue is touched once per push (from input handlers), once per render
/// pass (to drain expired entries), and contention is therefore bounded
/// to user-driven actions + 60Hz render. If profiling ever shows this is
/// hot, swap for a `crossbeam` channel — but the current shape keeps the
/// API a one-liner from any call site.
static TOASTS: Mutex<Vec<Toast>> = Mutex::new(Vec::new());

/// Push an error toast with the default TTL.
pub fn push_error(message: impl Into<String>) {
    push(Severity::Error, message, None);
}

/// Push an error toast with an extra hint line (e.g. "Try /login again.").
pub fn push_error_with_hint(message: impl Into<String>, hint: impl Into<String>) {
    push(Severity::Error, message, Some(hint.into()));
}

/// Push an error toast with a recovery suggestion (e.g. "Did you mean /login?").
pub fn push_error_with_recovery(message: impl Into<String>, recovery: impl Into<String>) {
    push_with_recovery(Severity::Error, message, Some(recovery.into()));
}

/// Push a warning toast with a recovery suggestion.
pub fn push_warning_with_recovery(message: impl Into<String>, recovery: impl Into<String>) {
    push_with_recovery(Severity::Warning, message, Some(recovery.into()));
}

/// Push an info toast with a recovery suggestion.
pub fn push_info_with_recovery(message: impl Into<String>, recovery: impl Into<String>) {
    push_with_recovery(Severity::Info, message, Some(recovery.into()));
}

/// Push an error toast specifically for auth failures with a tailored recovery path.
pub fn push_auth_error(provider: &str) {
    let message = format!("{} authentication failed", provider);
    let hint = "Check your credentials or run /login to re-authenticate.";
    let recovery = "/login — Re-authenticate with provider";
    push_with_ttl_and_recovery(
        Severity::Error,
        message,
        Some(hint.to_string()),
        Some(recovery.to_string()),
        None,
        DEFAULT_TTL,
    );
}

/// Push a warning toast for rate limits with recovery guidance.
pub fn push_rate_limit_with_recovery(
    provider: &str,
    attempt: u32,
    max_attempts: u32,
    retry_after_secs: u64,
) {
    let message = format!(
        "{provider}: rate-limited, retrying in {retry_after_secs}s (attempt {attempt}/{max_attempts})"
    );
    let hint = if retry_after_secs >= 5 {
        "This is normal for free-tier providers; the agent will keep trying."
    } else {
        "Retrying now."
    };
    let recovery = "/model — Switch to a different provider if this persists";
    let ttl = Duration::from_secs(retry_after_secs.max(2));
    push_with_ttl_and_recovery(
        Severity::Warning,
        message,
        Some(hint.to_string()),
        Some(recovery.to_string()),
        None,
        ttl,
    );
}

/// Push a warning toast with the default TTL.
pub fn push_warning(message: impl Into<String>) {
    push(Severity::Warning, message, None);
}

/// Push an info toast with the default TTL.
pub fn push_info(message: impl Into<String>) {
    push(Severity::Info, message, None);
}

/// Push a rate-limit toast with the seconds-until-retry so the user can
/// see exactly when the in-flight retry will fire. We size the TTL to
/// match the ETA so the toast naturally disappears at the moment the
/// retry actually happens.
pub fn push_rate_limit(provider: &str, attempt: u32, max_attempts: u32, retry_after_secs: u64) {
    let message = format!(
        "{provider}: rate-limited, retrying in {retry_after_secs}s (attempt {attempt}/{max_attempts})"
    );
    let hint = if retry_after_secs >= 5 {
        "This is normal for free-tier providers; the agent will keep trying."
    } else {
        "Retrying now."
    };
    let ttl = Duration::from_secs(retry_after_secs.max(2));
    push_with_ttl(Severity::Warning, message, Some(hint.to_string()), ttl);
}

/// Push a server-error toast so transient 5xx are visible without
/// polluting the transcript.
pub fn push_server_error(provider: &str, status: u16, retry_after_secs: Option<u64>) {
    let message = match retry_after_secs {
        Some(secs) => format!("{provider}: HTTP {status} (server error), retrying in {secs}s"),
        None => format!("{provider}: HTTP {status} (server error), retrying…"),
    };
    let hint = "The server is having a transient issue; the agent will keep trying.";
    let ttl = retry_after_secs
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(4))
        .max(Duration::from_secs(2));
    push_with_ttl(Severity::Warning, message, Some(hint.to_string()), ttl);
}

/// Push a toast with explicit severity and TTL.
pub fn push_with_ttl(
    severity: Severity,
    message: impl Into<String>,
    hint: Option<String>,
    ttl: Duration,
) {
    push_with_ttl_and_recovery(severity, message, hint, None, None, ttl);
}

/// Push a toast with explicit severity, hint, recovery suggestion, and TTL.
pub fn push_with_ttl_and_recovery(
    severity: Severity,
    message: impl Into<String>,
    hint: Option<String>,
    recovery: Option<String>,
    _expanded: Option<bool>,
    ttl: Duration,
) {
    let raw_message = message.into();
    let (message, details) = compact_toast_message(&raw_message);
    let hint = match (hint, details) {
        (Some(hint), Some(details)) => Some(format!("{details}\n\nSuggestion: {hint}")),
        (None, Some(details)) => Some(details),
        (hint, None) => hint,
    };

    let mut guard = TOASTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.push(Toast {
        severity,
        message,
        hint,
        recovery,
        created_at: Instant::now(),
        ttl,
        expanded: false,
    });
    // Keep only the most-recent `MAX_VISIBLE_TOASTS` to bound the screen
    // real estate. The oldest in the queue is evicted first.
    let len = guard.len();
    if len > MAX_VISIBLE_TOASTS {
        let drop = len - MAX_VISIBLE_TOASTS;
        guard.drain(0..drop);
    }
}

fn compact_toast_message(message: &str) -> (String, Option<String>) {
    let normalized = message.split_whitespace().collect::<Vec<_>>().join(" ");
    let multiple_lines = message
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
        > 1;
    if !multiple_lines && UnicodeWidthStr::width(normalized.as_str()) <= TOAST_PREVIEW_WIDTH {
        return (normalized, None);
    }

    let first_line = message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Something went wrong");
    let first_line = first_line
        .strip_prefix("Error: ")
        .or_else(|| first_line.strip_prefix("error: "))
        .unwrap_or(first_line)
        .trim();
    let preview = truncate_to_width(first_line, TOAST_PREVIEW_WIDTH.saturating_sub(1));
    let preview = if preview == first_line {
        preview
    } else {
        format!("{preview}…")
    };

    let mut details: String = message.chars().take(MAX_TOAST_DETAIL_CHARS).collect();
    if message.chars().count() > MAX_TOAST_DETAIL_CHARS {
        details.push_str("\n… additional diagnostic text was omitted");
    }
    (preview, Some(details))
}

fn truncate_to_width(text: &str, max_width: usize) -> String {
    let mut width = 0usize;
    let mut end = 0usize;
    for (index, character) in text.char_indices() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if width.saturating_add(character_width) > max_width {
            break;
        }
        width += character_width;
        end = index + character.len_utf8();
    }
    text[..end].trim_end().to_string()
}

fn wrapped_line_count(text: &str, width: usize) -> usize {
    let width = width.max(1);
    let mut lines = 0usize;
    for logical_line in text.lines() {
        if logical_line.trim().is_empty() {
            lines += 1;
            continue;
        }

        let mut current_width = 0usize;
        for word in logical_line.split_whitespace() {
            let word_width = UnicodeWidthStr::width(word);
            if current_width > 0 && current_width + 1 + word_width <= width {
                current_width += 1 + word_width;
                continue;
            }
            if current_width > 0 {
                lines += 1;
            }
            lines += word_width / width;
            current_width = word_width % width;
        }
        lines += usize::from(current_width > 0);
    }
    lines.max(1)
}

/// Push a toast with a recovery suggestion.
pub fn push_with_recovery(
    severity: Severity,
    message: impl Into<String>,
    recovery: Option<String>,
) {
    push_with_ttl_and_recovery(severity, message, None, recovery, None, DEFAULT_TTL);
}

fn push(severity: Severity, message: impl Into<String>, hint: Option<String>) {
    push_with_ttl(severity, message, hint, DEFAULT_TTL);
}

/// Clear all currently-visible toasts. Used by the `Esc` dismissal hook.
pub fn clear() {
    TOASTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
}

/// Dismiss a single toast by index (used when the user clicks an "x" on
/// a specific toast). Out-of-range indices are ignored.
pub fn dismiss(index: usize) {
    let mut guard = TOASTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if index < guard.len() {
        guard.remove(index);
    }
}

/// Toggle the expanded state of a toast by index. When expanded, the full
/// message and hint are shown at wider width. Out-of-range indices are
/// ignored. Returns whether the toast was expanded after toggling.
pub fn toggle_expand(index: usize) -> bool {
    let mut guard = TOASTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(toast) = guard.get_mut(index) {
        toast.expanded = !toast.expanded;
        toast.expanded
    } else {
        false
    }
}

/// Expand a specific toast by index. Out-of-range indices are ignored.
pub fn expand(index: usize) {
    let mut guard = TOASTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(toast) = guard.get_mut(index) {
        toast.expanded = true;
    }
}

/// Collapse a specific toast by index. Out-of-range indices are ignored.
pub fn collapse(index: usize) {
    let mut guard = TOASTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(toast) = guard.get_mut(index) {
        toast.expanded = false;
    }
}

/// Snapshot of the current toast queue, after pruning expired entries.
/// Returns owned `Toast`s so the caller can render without holding the
/// mutex during ratatui calls.
pub fn snapshot() -> Vec<Toast> {
    let now = Instant::now();
    let mut guard = TOASTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.retain(|t| !t.is_expired(now));
    guard.clone()
}

/// Number of currently-visible toasts. Used by tests and the `/help`
/// overlay ("3 active toasts" status line).
pub fn count() -> usize {
    snapshot().len()
}

/// Render the toast queue into `area`.
///
/// Layout: anchored to the bottom-right of the supplied area, with a
/// 1-cell margin. Toasts in the stack share a width and right edge; content
/// wraps within the terminal bounds and expanded diagnostics can use a wider
/// card. Toasts stack vertically with a 1-cell gap between them.
///
/// This is called once per render pass after the main frame has been
/// drawn, so the toasts visually float over the transcript.
pub fn draw(frame: &mut ratatui::Frame, area: Rect) {
    let toasts = snapshot();
    if toasts.is_empty() || area.width < 12 || area.height < 4 {
        return;
    }

    let width_cap = if toasts.iter().any(|toast| toast.expanded) {
        MAX_TOAST_WIDTH_EXPANDED
    } else {
        MAX_TOAST_WIDTH
    }
    .min(area.width.saturating_sub(2));
    let widest_message = toasts
        .iter()
        .map(|toast| UnicodeWidthStr::width(toast.message.as_str()))
        .max()
        .unwrap_or(0);
    let width = u16::try_from(widest_message.saturating_add(4))
        .unwrap_or(u16::MAX)
        .max(28)
        .min(width_cap);
    if width < 12 {
        return;
    }

    let inner_width = usize::from(width.saturating_sub(2)).max(1);
    let max_height = area.height.saturating_sub(2).max(3);
    let mut bottom = area.y.saturating_add(area.height).saturating_sub(2);
    for toast in toasts.iter().rev() {
        // Slide-in: young toasts rise from one row below with an ease-out
        // settle, so the stack reads as fluid rather than popping in. The
        // animation is ~180ms and only shifts the spawn row, so it never
        // moves a toast that the user is already reading.
        const SLIDE_IN_MS: f32 = 180.0;
        let mut slide = Transition::new(SLIDE_IN_MS / 1000.0);
        slide.advance(toast.created_at.elapsed().as_secs_f32());
        let slide_rows: u16 = if slide.is_complete() {
            0
        } else {
            // ease_out_cubic: fast start, gentle settle.
            let eased = ease_out_cubic(slide.progress);
            ((1.0_f32 - eased) * 2.0).round() as u16 // 2 → 1 → 0 rows
        };

        // Every toast in a stack shares one width and right edge, so errors read
        // as one tidy notification group instead of unrelated floating boxes.
        let message_lines = wrapped_line_count(&toast.message, inner_width);
        let detail_lines = if toast.expanded {
            toast
                .hint
                .as_deref()
                .map(|hint| wrapped_line_count(hint, inner_width))
                .unwrap_or(0)
                + toast
                    .recovery
                    .as_deref()
                    .map(|recovery| wrapped_line_count(recovery, inner_width))
                    .unwrap_or(0)
        } else if toast.hint.is_some() || toast.recovery.is_some() {
            1
        } else {
            0
        };
        let body_lines = message_lines.saturating_add(detail_lines);
        let height = u16::try_from(body_lines.saturating_add(2))
            .unwrap_or(u16::MAX)
            .min(max_height);
        let minimum_bottom = area.y.saturating_add(height.saturating_sub(1));
        if bottom < minimum_bottom {
            break; // off-screen above; older toasts stay queued for next frame.
        }
        let y = bottom.saturating_add(1).saturating_sub(height);
        let x = area.x + area.width.saturating_sub(width + 1);
        bottom = y.saturating_sub(2); // one blank row between stacked toasts

        let toast_area = Rect {
            x,
            y: y.saturating_add(slide_rows.min(2))
                .min(area.y.saturating_add(area.height.saturating_sub(height))),
            width,
            height,
        };

        // Clear the area under the toast so the chat transcript does not
        // bleed through.
        frame.render_widget(Clear, toast_area);

        let border_color = role_color(toast.severity.role());
        // Show expand/collapse indicator when hint is available.
        let expand_indicator = if toast.hint.is_some() {
            if toast.expanded { " ▾" } else { " ▸" }
        } else {
            ""
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                format!(
                    " {} {} {} ",
                    toast.severity.icon(),
                    toast.severity.label(),
                    expand_indicator
                ),
                Style::default()
                    .fg(border_color)
                    .add_modifier(Modifier::BOLD),
            ));

        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from(Span::styled(
            toast.message.clone(),
            Style::default(),
        )));
        if toast.expanded {
            // Expanded: show full hint.
            if let Some(hint) = &toast.hint {
                for detail_line in hint.lines() {
                    lines.push(Line::from(Span::styled(
                        detail_line.to_string(),
                        Style::default().fg(role_color(Role::Dim)),
                    )));
                }
            }
            // Expanded: show recovery suggestion.
            if let Some(recovery) = &toast.recovery {
                for (index, recovery_line) in recovery.lines().enumerate() {
                    let prefix = if index == 0 { " ↳ " } else { "   " };
                    lines.push(Line::from(Span::styled(
                        format!("{prefix}{recovery_line}"),
                        Style::default()
                            .fg(role_color(Role::Success))
                            .add_modifier(Modifier::ITALIC),
                    )));
                }
            }
        } else if toast.hint.is_some() || toast.recovery.is_some() {
            // Collapsed: show expand indicator.
            let collapse_hint = if toast.hint.is_some() && toast.recovery.is_some() {
                "▸ expand for suggestions"
            } else if toast.hint.is_some() {
                "press Enter to expand"
            } else {
                "▸ expand for suggestions"
            };
            lines.push(Line::from(Span::styled(
                collapse_hint,
                Style::default().fg(role_color(Role::Dim)),
            )));
        }

        let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, toast_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests share the process-global queue. Each test clears at start
    /// and end to avoid cross-test contamination.
    fn reset() {
        clear();
    }

    #[test]
    fn push_and_count() {
        reset();
        assert_eq!(count(), 0);
        push_error("first");
        push_warning("second");
        push_info("third");
        assert_eq!(count(), 3);
        reset();
    }

    #[test]
    fn max_visible_caps_queue() {
        reset();
        for i in 0..10 {
            push_error(format!("toast {i}"));
        }
        // Queue is bounded — only the most recent MAX_VISIBLE_TOASTS survive.
        assert_eq!(count(), MAX_VISIBLE_TOASTS);
        reset();
    }

    #[test]
    fn ttl_expires_toast() {
        reset();
        push_with_ttl(Severity::Error, "instant", None, Duration::from_millis(0));
        // After 0ms TTL, the toast is already expired at snapshot time.
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(count(), 0);
        reset();
    }

    #[test]
    fn clear_removes_all() {
        reset();
        push_error("a");
        push_warning("b");
        clear();
        assert_eq!(count(), 0);
        reset();
    }

    #[test]
    fn severity_icon_role_mapping_is_distinct() {
        // Each severity picks a distinct role. If two ever collapse to
        // the same role, the toast colors will be indistinguishable and
        // this test fails.
        assert_ne!(Severity::Error.role(), Severity::Warning.role());
        assert_ne!(Severity::Warning.role(), Severity::Info.role());
        assert_ne!(Severity::Error.role(), Severity::Info.role());
        // Each severity also exposes a non-empty icon and label.
        for sev in [Severity::Error, Severity::Warning, Severity::Info] {
            assert!(!sev.icon().is_empty());
            assert!(!sev.label().is_empty());
        }
    }

    #[test]
    fn long_errors_collapse_to_a_short_preview_and_keep_details() {
        let raw = format!(
            "Error: provider request failed after retries\nCaused by: {}",
            "upstream diagnostic details ".repeat(30)
        );
        let (preview, details) = compact_toast_message(&raw);

        assert!(UnicodeWidthStr::width(preview.as_str()) <= TOAST_PREVIEW_WIDTH);
        assert!(!preview.contains('\n'));
        let details = details.expect("long error should keep expandable details");
        assert!(details.contains("Caused by:"));
        assert!(details.contains("upstream diagnostic details"));
    }

    #[test]
    fn short_errors_stay_unchanged() {
        let (preview, details) = compact_toast_message("Could not connect");
        assert_eq!(preview, "Could not connect");
        assert!(details.is_none());
    }

    #[test]
    fn dismiss_removes_one() {
        reset();
        push_error("a");
        push_error("b");
        push_error("c");
        dismiss(1); // remove the middle one
        let snap = snapshot();
        assert_eq!(snap.len(), 2);
        // The remaining two should be the original first and third; their
        // messages should be "a" and "c" in that order.
        assert_eq!(snap[0].message, "a");
        assert_eq!(snap[1].message, "c");
        reset();
    }
}
