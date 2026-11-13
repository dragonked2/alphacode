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

#[test]
fn narrow_help_wraps_descriptions_instead_of_clipping_them() {
    let _lock = viewport_snapshot_test_lock();
    let state = TestState::default();
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(34, 500)).expect("test terminal");
    terminal
        .draw(|frame| overlays::draw_help_overlay(frame, frame.area(), 0, &state))
        .expect("draw help overlay");
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..buffer.area.height)
        .map(|y| {
            (1..buffer.area.width - 1)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim()
                .to_string()
        })
        .collect();
    let flattened = rows.join(" ");
    assert!(flattened.contains("Show live-test evidence for the current provider/model"));
}

fn help_review_buffer(
    width: u16,
    height: u16,
    state: &TestState,
    scroll: usize,
) -> ratatui::buffer::Buffer {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| overlays::draw_help_overlay(frame, frame.area(), scroll, state))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn help_review_text(buffer: &ratatui::buffer::Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Optional visual-QA artifacts use the exact cells exercised by assertions.
fn save_help_review_buffer(name: &str, buffer: &ratatui::buffer::Buffer) {
    let Some(directory) = std::env::var_os("ALPHACODE_TUI_REVIEW_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let mut cells = Vec::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            cells.push(serde_json::json!({"x": x, "y": y, "text": cell.symbol(), "fg": format!("{:?}", cell.fg), "bg": format!("{:?}", cell.bg), "bold": cell.modifier.contains(ratatui::style::Modifier::BOLD)}));
        }
    }
    std::fs::write(directory.join(format!("{name}.json")), serde_json::to_vec(&serde_json::json!({"width": buffer.area.width, "height": buffer.area.height, "cells": cells})).unwrap()).unwrap();
}

#[test]
fn help_search_filters_by_all_words_and_keeps_search_visible_while_scrolling() {
    let _lock = viewport_snapshot_test_lock();
    let state = TestState {
        help_query: "MoDeL switch".into(),
        help_search_editing: true,
        ..Default::default()
    };
    let buffer = help_review_buffer(96, 28, &state, 0);
    let text = help_review_text(&buffer);
    assert!(text.contains("/model <name>"), "{text}");
    assert!(
        !text.contains("/quit"),
        "unrelated commands must be filtered out"
    );
    assert!(!text.contains("Toggle queue mode"));
    assert!(text.contains("Search: MoDeL switch"));
    assert!(text.contains("Esc clear"));
    save_help_review_buffer("help-search", &buffer);
    let scrolled = help_review_buffer(
        34,
        10,
        &TestState {
            help_query: "model".into(),
            ..Default::default()
        },
        usize::MAX,
    );
    assert!(help_review_text(&scrolled).contains("Search: model"));
}

#[test]
fn help_has_a_useful_empty_state_and_a_small_screen_escape_route() {
    let _lock = viewport_snapshot_test_lock();
    let state = TestState {
        help_query: "no-such-command-xyz".into(),
        help_search_editing: true,
        ..Default::default()
    };
    let buffer = help_review_buffer(34, 14, &state, 0);
    let text = help_review_text(&buffer);
    assert!(text.contains("No matches."), "{text}");
    assert!(text.contains("0 matches"));
    assert!(text.contains("Esc clear"));
    save_help_review_buffer("help-no-results", &buffer);
    for (width, height) in [(1, 1), (8, 3), (20, 6), (34, 14), (80, 24)] {
        let buffer = help_review_buffer(width, height, &state, usize::MAX);
        if width >= 8 {
            assert!(
                help_review_text(&buffer).contains("Esc"),
                "{width}x{height}"
            );
        }
    }
}

#[test]
fn help_start_view_teaches_real_actions_and_omits_retired_send_modes() {
    let _lock = viewport_snapshot_test_lock();
    let buffer = help_review_buffer(96, 28, &TestState::default(), 0);
    let text = help_review_text(&buffer);
    assert!(text.contains("Start here"));
    assert!(text.contains("Choose a model: /model"));
    assert!(text.contains("Shift+Enter adds a line"));
    assert!(text.contains("Ctrl+F search"));
    save_help_review_buffer("help-start", &buffer);
    let rows = rendered_help_rows().join("\n");
    assert!(!rows.contains("Toggle queue mode"));
    assert!(!rows.contains("opposite send mode"));
    assert!(rows.contains("Move to start of input"));
}

#[test]
fn help_query_caret_stays_in_bounds_with_wide_and_combining_graphemes() {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;
    let query = "model e\u{301} 中文 👩‍💻 final";
    let positions = query
        .grapheme_indices(true)
        .map(|(index, _)| index)
        .chain(std::iter::once(query.len()))
        .collect::<Vec<_>>();
    for width in 0..=100 {
        for &cursor in &positions {
            let (text, caret) = overlays::help_search_display(query, cursor, width);
            assert!(text.width() <= width, "{text:?} exceeds {width}");
            assert!(width == 0 || caret < width);
        }
    }
}

#[test]
fn narrow_busy_composer_keeps_the_stop_action_visible_in_the_buffer() {
    let _lock = viewport_snapshot_test_lock();
    let state = TestState {
        input: "Please use the existing module".into(),
        status: ProcessingStatus::Streaming,
        ..Default::default()
    };
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(34, 6)).unwrap();
    terminal
        .draw(|frame| {
            let mut capture = None;
            super::input_ui::draw_input(frame, &state, frame.area(), 1, &mut capture);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(
        help_review_text(buffer).contains("Esc stops"),
        "{}",
        help_review_text(buffer)
    );
    save_help_review_buffer("composer-busy-narrow", buffer);
}
