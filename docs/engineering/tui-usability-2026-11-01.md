# TUI and console usability — 2026-11-01

This pass improves everyday composer, notification, resize and console-output behavior using the existing Ratatui architecture and dependencies.

## Interface changes

- The existing composer bottom border now carries complete, width-fitted shortcuts: Enter adapts to send, queue, immediate continuation, command selection, or shell execution; Alt+Enter teaches multiline input; F1 opens help. Active turns expose Esc to stop. Smaller viewports drop secondary shortcuts instead of clipping key names.
- The top border names Message, Command, Local shell or Server shell when there is no work ribbon. Running tasks retain their existing ribbon and explicitly reserve space for the brand badge.
- Multiline drafts show their line count, or the visible wrapped-row range when scrolled. These indicators use existing border cells, preserving transcript space and input/copy geometry.
- Task titles now sanitize transient text, measure terminal cells, and truncate on grapheme boundaries. CJK and joined emoji no longer overwrite the brand badge.
- Repeated identical notifications merge with a visible occurrence count. Their timeout refreshes without restarting the slide animation or closing expanded diagnostics. Different messages, severities, hints and recovery actions remain separate.
- Notification messages, hints and recovery text strip terminal escape/control sequences while retaining readable paragraphs.
- Tiny terminals enter the resize state before opening overlays. Collapsed composer interiors publish no cursor; normal cursor coordinates stay within the editor interior.
- Help documents Alt+Enter and the universal trailing-backslash newline fallback.

## Editing correctness

Typing or pasting over a selection now takes an undo snapshot before removing the selected text, so a single undo restores the complete original draft. The direct selection-replacement helper now replaces the selected byte range instead of inserting before it. Regression cases cover Unicode drafts and replacing one selected word.

## Console output

CLI boxes use a single width budget for title, corners, body padding and bottom border. Long titles truncate within the frame, blank lines remain visible as spacing, and narrow terminals retain the unboxed fallback. The strengthened tests check every row, including title and corner rows, rather than only body rows.

## Review and limits

The render regression matrix covers 12×5, 40×10, 80×24, 120×32 and 200×45 cells across idle, long Unicode draft, streaming, shell and command states. It checks both the isolated composer and the complete page. Optional `ALPHACODE_UI_REVIEW_DIR` exports actual TestBackend cells and styles as JSON for visual inspection; ordinary test runs write no review files.

Headless rendering verifies geometry and text order, not terminal-emulator shaping, font coverage, screen-reader behavior, remote-network recovery or a live provider. Arabic remains in logical input order; its visual shaping depends on the terminal. These changes do not claim exhaustive UX coverage across all terminals.

## Verification results

- `cargo fmt --all -- --check`: passed. Included test files were also checked directly with Rustfmt because Cargo formatting does not traverse `include!` sources.
- `cargo check --workspace`: passed.
- `cargo check --workspace --tests`: passed.
- `cargo clippy --lib -- -D warnings`: passed.
- Full library suite: **6,342 passed, 0 failed, 12 ignored**, 216.65 seconds of test execution. This includes the 12 CLI-output tests; the executable wrapper itself contains none of those tests.
- Render matrix: 50 actual TestBackend frame captures. Selected 40-, 80-, 120- and 200-column views were visually inspected; 12×5 and a collapsed composer interior were covered by geometry checks.

Tests ran with the VS 2022 MSVC environment, the external temporary Cargo target directory required by AGENTS.md, `TERM=xterm-256color`, and no `NO_COLOR` setting in the test process. No live provider or real terminal emulator was exercised.

The verification pass also corrected an existing batch-render assertion that searched the entire page for subcall indices. A changelog entry containing `#20` falsely matched its ban on `#2`; the assertion now inspects the batch section only. An initial new undo test incorrectly expected the caret position from before selection rather than immediately before the replacement edit; its expectation was corrected without changing the editor behavior.
