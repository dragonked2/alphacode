//! `paste_buffer` — robust multi-line paste detector for non-bracketed
//! terminals.
//!
//! ## The problem
//!
//! Some terminals (older conhost profiles, certain web/SSH bridges,
//! tmux without `set -g set-paste-on`, custom muxers) strip the
//! bracketed-paste framing. The paste then arrives as a stream of
//! ordinary `KeyCode::Char(...)` and `KeyCode::Enter` events.
//!
//! The existing `paste_guard` only suppresses the *trailing* Enter
//! inside a small window after a bracketed-paste event. When the
//! bracketed event never fires, every newline in the paste turns
//! into a submit and the user gets the paste submitted line by line
//! as a flood of queued messages.
//!
//! ## The fix
//!
//! This module maintains *two* complementary detectors:
//!
//! 1. **Line-burst detector** — if the user types Enter quickly
//!    enough that the gap between successive Enters is below
//!    `BURST_GAP` and the input has grown by at least one non-newline
//!    character between Enters, the next Enter does NOT submit —
//!    it's treated as a paste-in-progress.
//!
//! 2. **Rapid-insertion detector** — when text is being inserted
//!    into the input very quickly (gap between consecutive char
//!    insertions < `RAPID_INSERT_GAP` and > `RAPID_INSERT_MIN_BYTES`
//!    bytes arrive within `RAPID_INSERT_WINDOW`), the next Enter is
//!    classified as a paste-newline *even if no prior Enter exists*.
//!    This catches the common case where the *first* Enter of a
//!    pasted block would otherwise fire a submit before any of the
//!    burst signals can engage (the original failure mode: each
//!    line of a large paste was submitted as a separate interleave
//!    message, queueing up a flood of overlapping turns).
//!
//! When either detector concludes that an Enter is part of a paste,
//! the newline is buffered into the input instead of submitting.
//! When the burst ends (no new Enter within `BURST_GAP`), the
//! accumulated content is committed to the input as a single
//! `[Pasted Content +N lines]` placeholder, exactly like the
//! bracketed-paste path. The user can then edit freely above it
//! and press Enter once to send the whole thing.
//!
//! These detectors are conservative: a real submit is never
//! re-classified as a paste.

use std::cell::Cell;
use std::time::{Duration, Instant};

/// Maximum gap between two consecutive Enters to still consider the
/// sequence a single paste. Real human typing has 200-500ms gaps
/// between Enter presses; a paste has 0-50ms. We pick a value
/// comfortably between.
const BURST_GAP: Duration = Duration::from_millis(120);

/// If the burst is longer than this, treat it as a paste no matter
/// what the gap says. A real submit has 1 line; a paste has 2+.
const BURST_MIN_LINES: usize = 2;

/// Maximum gap between two consecutive character insertions to
/// still consider the input a rapid paste stream. Human typing has
/// 80-200ms gaps between keys; a paste has 0-10ms.
const RAPID_INSERT_GAP: Duration = Duration::from_millis(20);

/// Total bytes that must arrive within `RAPID_INSERT_WINDOW` before
/// the rapid-insertion detector engages. Chosen to be well above
/// any plausible single-word burst (~20 bytes) so a fast typist
/// cannot trigger it by accident.
const RAPID_INSERT_MIN_BYTES: usize = 64;

/// Time window over which `RAPID_INSERT_MIN_BYTES` must accumulate
/// to engage the detector.
const RAPID_INSERT_WINDOW: Duration = Duration::from_millis(500);

// Thread-local burst + rapid-insertion detectors. Stays a Cell so
// the key-handling thread can read+write without locks.
thread_local! {
    static BURST_LAST_ENTER: Cell<Option<Instant>> = const { Cell::new(None) };
    static BURST_LINES: Cell<usize> = const { Cell::new(0) };
    static BURST_INPUT_LEN_AT_FIRST_ENTER: Cell<usize> = const { Cell::new(0) };

    // Rapid-insertion detector: when did the most recent char
    // insertion happen, and how many bytes have arrived since the
    // window started.
    static RAPID_LAST_INSERT: Cell<Option<Instant>> = const { Cell::new(None) };
    static RAPID_WINDOW_START: Cell<Option<Instant>> = const { Cell::new(None) };
    static RAPID_BYTES_IN_WINDOW: Cell<usize> = const { Cell::new(0) };
}

/// State for the next Enter handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PasteBurstState {
    /// No burst in progress. Submit normally.
    NotBurst,
    /// We are mid-burst; the current Enter is part of a paste.
    /// Do NOT submit. Buffer the newline into the input.
    InBurst,
    /// A burst just ended on the *previous* Enter. The caller
    /// should now collapse the burst content into a placeholder.
    JustEnded,
}

/// Called from `handle_enter` *before* the normal submit logic.
/// Returns the recommended state.
pub(super) fn check_enter_burst(input_len_at_enter: usize) -> PasteBurstState {
    let now = Instant::now();
    let last = BURST_LAST_ENTER.with(|c| c.get());

    match last {
        None => {
            // First Enter in a while. Start a new burst, candidate.
            BURST_LAST_ENTER.with(|c| c.set(Some(now)));
            BURST_LINES.with(|c| c.set(1));
            BURST_INPUT_LEN_AT_FIRST_ENTER.with(|c| c.set(input_len_at_enter));
            PasteBurstState::NotBurst
        }
        Some(t) if now.duration_since(t) <= BURST_GAP => {
            // Within the burst gap. Increment counter.
            let lines = BURST_LINES.with(|c| c.get() + 1);
            BURST_LINES.with(|c| c.set(lines));
            BURST_LAST_ENTER.with(|c| c.set(Some(now)));
            if lines >= BURST_MIN_LINES {
                PasteBurstState::InBurst
            } else {
                PasteBurstState::NotBurst
            }
        }
        Some(_) => {
            // Gap too long. The previous burst (if any) is over; this
            // is a new Enter, possibly the start of a new burst.
            // We need to report "JustEnded" if there was a real
            // burst that wasn't collapsed.
            let prev_lines = BURST_LINES.with(|c| c.get());
            BURST_LAST_ENTER.with(|c| c.set(Some(now)));
            BURST_LINES.with(|c| c.set(1));
            BURST_INPUT_LEN_AT_FIRST_ENTER.with(|c| c.set(input_len_at_enter));
            if prev_lines >= BURST_MIN_LINES {
                PasteBurstState::JustEnded
            } else {
                PasteBurstState::NotBurst
            }
        }
    }
}

/// Record that `bytes` characters were just inserted into the input.
/// Called from `insert_input_text` on every non-empty insertion so
/// the rapid-insertion detector can decide if the next Enter is a
/// paste-newline.
pub(super) fn note_text_inserted(bytes: usize) {
    if bytes == 0 {
        return;
    }
    let now = Instant::now();

    let last = RAPID_LAST_INSERT.with(|c| c.get());
    let window_start = RAPID_WINDOW_START.with(|c| c.get());
    let bytes_in_window = RAPID_BYTES_IN_WINDOW.with(|c| c.get());

    // If the gap from the previous insertion is too large, the user
    // is typing again rather than pasting — reset the window and
    // start a new short one anchored at this insertion.
    match last {
        None => {
            RAPID_WINDOW_START.with(|c| c.set(Some(now)));
            RAPID_BYTES_IN_WINDOW.with(|c| c.set(bytes));
            RAPID_LAST_INSERT.with(|c| c.set(Some(now)));
            return;
        }
        Some(t) if now.duration_since(t) > RAPID_INSERT_GAP => {
            RAPID_WINDOW_START.with(|c| c.set(Some(now)));
            RAPID_BYTES_IN_WINDOW.with(|c| c.set(bytes));
            RAPID_LAST_INSERT.with(|c| c.set(Some(now)));
            return;
        }
        _ => {}
    }

    // We're inside the rapid-insertion gap. Roll the window forward
    // if the previous window expired, otherwise accumulate.
    let new_window_start = match window_start {
        Some(t) if now.duration_since(t) <= RAPID_INSERT_WINDOW => t,
        _ => now,
    };
    let new_bytes = if Some(new_window_start) != window_start {
        bytes
    } else {
        bytes_in_window + bytes
    };

    RAPID_WINDOW_START.with(|c| c.set(Some(new_window_start)));
    RAPID_BYTES_IN_WINDOW.with(|c| c.set(new_bytes));
    RAPID_LAST_INSERT.with(|c| c.set(Some(now)));
}

/// True if a large amount of text arrived recently at a rate that's
/// consistent with a paste, even if no Enter has been seen yet. The
/// caller should treat the current Enter as a paste-newline.
///
/// This is the signal that catches the *first* Enter of a pasted
/// block, which the line-burst detector cannot (it has no prior
/// Enter to compare against).
pub(super) fn rapid_insertion_active() -> bool {
    let now = Instant::now();
    let last = RAPID_LAST_INSERT.with(|c| c.get());
    let window_start = RAPID_WINDOW_START.with(|c| c.get());
    let bytes = RAPID_BYTES_IN_WINDOW.with(|c| c.get());
    let Some(last) = last else { return false; };
    let Some(window_start) = window_start else { return false; };
    now.duration_since(last) <= RAPID_INSERT_GAP
        && now.duration_since(window_start) <= RAPID_INSERT_WINDOW
        && bytes >= RAPID_INSERT_MIN_BYTES
}

/// Called by the agent to insert a newline into the input *as part
/// of a paste*. Distinct from `insert_input_text("\n")` because we
/// don't want a trailing backslash-continuation to fire (it would
/// convert our paste-newline into a literal `\n` instead of an
/// actual newline).
pub(super) fn insert_paste_newline(input: &mut String, cursor_pos: &mut usize) {
    input.insert(*cursor_pos, '\n');
    *cursor_pos += 1;
}

/// Build the placeholder string for a multi-line paste.
pub(super) fn placeholder_for(line_count: usize) -> String {
    format!(
        "[Pasted Content +{} line{}]",
        line_count,
        if line_count == 1 { "" } else { "s" }
    )
}

/// Reset all burst state. Call this on submit, on Esc, on input
/// clear, or on any user action that should definitively end a
/// burst.
pub(super) fn reset_burst() {
    BURST_LAST_ENTER.with(|c| c.set(None));
    BURST_LINES.with(|c| c.set(0));
    BURST_INPUT_LEN_AT_FIRST_ENTER.with(|c| c.set(0));
    RAPID_LAST_INSERT.with(|c| c.set(None));
    RAPID_WINDOW_START.with(|c| c.set(None));
    RAPID_BYTES_IN_WINDOW.with(|c| c.set(0));
}

/// Returns true if the burst detector has an active burst in
/// progress. Useful for deciding whether to suppress the trailing
/// Enter or the bracketed-paste guardrail.
#[allow(dead_code)]
pub(super) fn is_burst_active() -> bool {
    let last = BURST_LAST_ENTER.with(|c| c.get());
    let lines = BURST_LINES.with(|c| c.get());
    matches!(last, Some(t) if t.elapsed() <= BURST_GAP && lines >= BURST_MIN_LINES)
}

/// The number of newlines the burst detector has counted in the
/// current/just-ended burst. Returns 0 if no burst is in progress.
pub(super) fn burst_line_count() -> usize {
    BURST_LINES.with(|c| c.get())
}

/// Test hook.
#[cfg(test)]
#[allow(dead_code)]
pub(in crate::alphacode_tui::tui::app) fn expire_for_test() {
    reset_burst();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    fn reset() {
        reset_burst();
    }

    #[test]
    fn single_enter_is_not_burst() {
        reset();
        let s = check_enter_burst(0);
        assert_eq!(s, PasteBurstState::NotBurst);
    }

    #[test]
    fn two_rapid_enters_form_burst() {
        reset();
        let _ = check_enter_burst(0);
        let s = check_enter_burst(5);
        assert_eq!(s, PasteBurstState::InBurst);
    }

    #[test]
    fn slow_enters_are_not_burst() {
        reset();
        let _ = check_enter_burst(0);
        sleep(BURST_GAP * 2);
        let s = check_enter_burst(5);
        assert_eq!(s, PasteBurstState::NotBurst);
    }

    #[test]
    fn long_burst_ends_after_gap() {
        reset();
        for i in 0..3 {
            let _ = check_enter_burst(i * 5);
        }
        sleep(BURST_GAP * 2);
        let s = check_enter_burst(20);
        assert_eq!(s, PasteBurstState::JustEnded);
    }

    #[test]
    fn placeholder_counts_lines() {
        assert_eq!(placeholder_for(1), "[Pasted Content +1 line]");
        assert_eq!(placeholder_for(5), "[Pasted Content +5 lines]");
    }

    #[test]
    fn insert_paste_newline_handles_empty_string() {
        let mut s = String::new();
        let mut pos = 0;
        insert_paste_newline(&mut s, &mut pos);
        assert_eq!(s, "\n");
        assert_eq!(pos, 1);
    }

    #[test]
    fn insert_paste_newline_preserves_cjk() {
        let mut s = String::from("中文");
        let mut pos = 3;
        insert_paste_newline(&mut s, &mut pos);
        assert!(s.is_char_boundary(pos));
    }

    #[test]
    fn reset_clears_all_state() {
        let _ = check_enter_burst(0);
        let _ = check_enter_burst(5);
        let _ = check_enter_burst(10);
        assert_eq!(burst_line_count(), 3);
        reset_burst();
        assert_eq!(burst_line_count(), 0);
    }

    #[test]
    fn is_burst_active_reflects_state() {
        reset();
        assert!(!is_burst_active());
        let _ = check_enter_burst(0);
        assert!(!is_burst_active());
        let _ = check_enter_burst(5);
        assert!(is_burst_active());
    }

    #[test]
    fn check_enter_burst_returns_in_burst_for_three_rapid_enters() {
        reset();
        let s1 = check_enter_burst(0);
        let s2 = check_enter_burst(5);
        let s3 = check_enter_burst(10);
        assert_eq!(s1, PasteBurstState::NotBurst);
        assert_eq!(s2, PasteBurstState::InBurst);
        assert_eq!(s3, PasteBurstState::InBurst);
    }

    #[test]
    fn check_enter_burst_returns_just_ended_after_timeout() {
        reset();
        for i in 0..3 {
            let _ = check_enter_burst(i * 5);
        }
        sleep(BURST_GAP * 2);
        let s = check_enter_burst(20);
        assert_eq!(s, PasteBurstState::JustEnded);
        assert_eq!(burst_line_count(), 1);
    }

    // ----- Rapid-insertion detector tests -----

    #[test]
    fn rapid_insertion_engages_after_large_quick_text() {
        reset();
        for _ in 0..10 {
            note_text_inserted(10);
            sleep(Duration::from_millis(5));
        }
        assert!(rapid_insertion_active(), "rapid-insertion should engage");
    }

    #[test]
    fn rapid_insertion_does_not_engage_for_short_typing() {
        reset();
        // 30 bytes total at human typing cadence is well below the
        // 64-byte threshold.
        note_text_inserted(20);
        sleep(Duration::from_millis(5));
        note_text_inserted(10);
        assert!(!rapid_insertion_active());
    }

    #[test]
    fn rapid_insertion_resets_after_long_pause() {
        reset();
        note_text_inserted(80);
        sleep(Duration::from_millis(5));
        assert!(rapid_insertion_active());
        sleep(RAPID_INSERT_GAP * 2);
        assert!(!rapid_insertion_active());
    }

    #[test]
    fn reset_burst_also_resets_rapid_insertion() {
        reset();
        note_text_inserted(80);
        sleep(Duration::from_millis(5));
        assert!(rapid_insertion_active());
        reset_burst();
        assert!(!rapid_insertion_active());
    }
}