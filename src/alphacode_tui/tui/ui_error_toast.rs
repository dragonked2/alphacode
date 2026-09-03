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
//! - Width auto-fits the message, capped at 60 cells. Two-line layout:
//!   icon + message on the first line, optional hint on the second.
//!
//! # Public API
//!
//! ```ignore
//! use crate::alphacode_tui::tui::error_toast;
//! error_toast::push_error("OAuth flow was cancelled by the user.");
//! error_toast::push_warning("Rate-limited by upstream; retrying in 30s.");
//! error_toast::push_info("New session started.");
//! error_toast::clear();   // dismiss all toasts immediately
//! ```

use std::sync::Mutex;
use std::time::{Duration, Instant};

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::alphacode_tui_style::icons::Icon;
use crate::alphacode_tui_style::palette::{Role, role_color};

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
    pub created_at: Instant,
    pub ttl: Duration,
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

/// Maximum toast width in cells. Anything longer wraps within this width
/// rather than exceeding the chat area.
pub const MAX_TOAST_WIDTH: u16 = 60;

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
    let mut guard = TOASTS.lock().expect("error_toast mutex poisoned");
    guard.push(Toast {
        severity,
        message: message.into(),
        hint,
        created_at: Instant::now(),
        ttl,
    });
    // Keep only the most-recent `MAX_VISIBLE_TOASTS` to bound the screen
    // real estate. The oldest in the queue is evicted first.
    let len = guard.len();
    if len > MAX_VISIBLE_TOASTS {
        let drop = len - MAX_VISIBLE_TOASTS;
        guard.drain(0..drop);
    }
}

fn push(severity: Severity, message: impl Into<String>, hint: Option<String>) {
    push_with_ttl(severity, message, hint, DEFAULT_TTL);
}

/// Clear all currently-visible toasts. Used by the `Esc` dismissal hook.
pub fn clear() {
    TOASTS.lock().expect("error_toast mutex poisoned").clear();
}

/// Dismiss a single toast by index (used when the user clicks an "x" on
/// a specific toast). Out-of-range indices are ignored.
pub fn dismiss(index: usize) {
    let mut guard = TOASTS.lock().expect("error_toast mutex poisoned");
    if index < guard.len() {
        guard.remove(index);
    }
}

/// Snapshot of the current toast queue, after pruning expired entries.
/// Returns owned `Toast`s so the caller can render without holding the
/// mutex during ratatui calls.
pub fn snapshot() -> Vec<Toast> {
    let now = Instant::now();
    let mut guard = TOASTS.lock().expect("error_toast mutex poisoned");
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
/// 1-cell margin on each side. Each toast is one rounded `Block` whose
/// width auto-fits its message (capped at `MAX_TOAST_WIDTH`) and whose
/// height is `1 + hint_lines + 1 (border top + bottom)`. Toasts stack
/// vertically with a 1-cell gap between them.
///
/// This is called once per render pass after the main frame has been
/// drawn, so the toasts visually float over the transcript.
pub fn draw(frame: &mut ratatui::Frame, area: Rect) {
    let toasts = snapshot();
    if toasts.is_empty() {
        return;
    }

    let mut y = area.y + area.height.saturating_sub(1);
    for toast in toasts.iter().rev() {
        // Compute the toast size from its content (auto-fit, capped).
        let msg_width = toast.message.chars().count().min(MAX_TOAST_WIDTH as usize) as u16;
        let hint_lines = toast.hint.as_ref().map(|h| h.lines().count()).unwrap_or(0);
        // 1 line for message + hint_lines + 1 padding row inside + 2 for borders.
        let height = (1 + hint_lines + 1 + 2) as u16;
        if y < area.y + height {
            break; // off-screen above; older toasts stay queued for next frame.
        }
        let width = (msg_width + 4).max(20).min(area.width.saturating_sub(2));
        let x = area.x + area.width.saturating_sub(width + 1);
        y -= height + 1; // 1-cell gap between toasts

        let toast_area = Rect {
            x,
            y,
            width,
            height,
        };

        // Clear the area under the toast so the chat transcript does not
        // bleed through.
        frame.render_widget(Clear, toast_area);

        let border_color = role_color(toast.severity.role());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                format!(" {} {} ", toast.severity.icon(), toast.severity.label()),
                Style::default()
                    .fg(border_color)
                    .add_modifier(Modifier::BOLD),
            ));

        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from(Span::styled(
            toast.message.clone(),
            Style::default(),
        )));
        if let Some(hint) = &toast.hint {
            lines.push(Line::from(Span::styled(
                hint.clone(),
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
