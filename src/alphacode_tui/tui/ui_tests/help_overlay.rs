//! Regression tests for the F1 help overlay contents.
//!
//! The command sections of `draw_help_overlay` are hand-curated for ordering
//! and only backstopped by the "More commands" sweep for *missing* commands.
//! Nothing stopped the curated list from showing the same command syntax
//! twice (both `/catchup [next]` and `/back` were listed twice at one point),
//! which costs scroll space and makes the reference look untrustworthy. These
//! tests render the real overlay through a `TestBackend` and assert each
//! command syntax appears once.

use super::*;
use crate::alphacode_tui::tui::ui::overlays;

/// Render the help overlay tall enough that nothing scrolls off, and return
/// one string per content row with the block borders stripped.
fn rendered_help_rows() -> Vec<String> {
    let state = TestState::default();
    // 400 rows comfortably exceeds the current overlay length; bump it if the
    // help grows (the assertions below scan every rendered row).
    let backend = ratatui::backend::TestBackend::new(160, 400);
    let mut terminal = ratatui::Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| overlays::draw_help_overlay(frame, frame.area(), 0, &state))
        .expect("draw help overlay");

    let buffer = terminal.backend().buffer();
    let mut rows = Vec::new();
    for y in 0..buffer.area.height {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        // Strip the left/right border glyphs so row starts are content starts.
        let chars: Vec<char> = row.chars().collect();
        let inner: String = chars
            .get(1..chars.len().saturating_sub(1))
            .into_iter()
            .flatten()
            .collect();
        rows.push(inner);
    }
    rows
}

/// The command text of every command row: the part after the row indent up to
/// the two-space gap before the description, e.g. `/catchup [next|list]`.
///
/// Key-binding rows never start with `/`, so this only picks up commands.
fn rendered_command_entries(rows: &[String]) -> Vec<String> {
    rows.iter()
        .filter_map(|row| {
            let trimmed = row.trim_start();
            if !trimmed.starts_with('/') {
                return None;
            }
            Some(trimmed.split("  ").next().unwrap_or(trimmed).to_string())
        })
        .collect()
}

#[test]
fn help_overlay_lists_each_command_syntax_once() {
    let _lock = viewport_snapshot_test_lock();
    let rows = rendered_help_rows();
    let entries = rendered_command_entries(&rows);
    assert!(
        entries.len() > 30,
        "expected the overlay to render many command rows, got {}",
        entries.len()
    );

    // Multiple distinct rows for one command are fine (`/model` and
    // `/model <name>`); an exact repeat of the same syntax is not.
    let mut counts = std::collections::HashMap::new();
    for entry in &entries {
        *counts.entry(entry.clone()).or_insert(0usize) += 1;
    }
    let mut duplicates: Vec<(&String, &usize)> =
        counts.iter().filter(|(_, count)| **count > 1).collect();
    duplicates.sort();
    assert!(
        duplicates.is_empty(),
        "help overlay repeats command syntax: {duplicates:?}"
    );
}

#[test]
fn help_overlay_shows_catchup_and_back_once_with_current_syntax() {
    let _lock = viewport_snapshot_test_lock();
    let rows = rendered_help_rows();
    let entries = rendered_command_entries(&rows);

    let catchup: Vec<&String> = entries
        .iter()
        .filter(|entry| entry.starts_with("/catchup"))
        .collect();
    assert_eq!(
        catchup.len(),
        1,
        "expected exactly one /catchup row, got {catchup:?}"
    );
    assert_eq!(
        catchup.first().map(|entry| entry.as_str()),
        Some("/catchup [next|list]"),
        "stale /catchup syntax in help row"
    );

    let back: Vec<&String> = entries
        .iter()
        .filter(|entry| entry.starts_with("/back"))
        .collect();
    assert_eq!(
        back.len(),
        1,
        "expected exactly one /back row, got {back:?}"
    );
    assert_eq!(back.first().map(|entry| entry.as_str()), Some("/back"));
}
