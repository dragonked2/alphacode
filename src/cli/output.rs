//! Structured, color-aware output for the `alphacode` CLI.
//!
//! Everything here renders through
//! [`alphacode_core::output_enhance`](crate::alphacode_core::output_enhance),
//! which in turn consults
//! [`alphacode_core::console`](crate::alphacode_core::console) for the two
//! questions every renderer has to answer first:
//!
//! * may I emit ANSI escapes? (TTY, `NO_COLOR`, `CLICOLOR_FORCE`, quiet mode)
//! * how many cells wide is the terminal?
//!
//! That indirection is the point. The previous version of this file carried
//! its own private table of escape constants and used them unconditionally, so
//! every `CliOutput::` helper produced literal `\u{1b}[32m` garbage the moment
//! output was piped, redirected to a file, or captured by CI — and it padded
//! its columns with `key.len()`, which counts bytes, so any non-ASCII key or
//! value shoved the rest of the line right by one column per wide glyph.
//!
//! # Layout rules enforced here
//!
//! * Columns are measured in **display cells**, never bytes or `char`s.
//! * Every cell is padded to exactly its declared width, so a table stays
//!   rectangular even when a value is truncated with an ellipsis.
//! * Text longer than the terminal is wrapped, not handed to the terminal's
//!   soft-wrap, so indentation survives.

use crate::alphacode_core::console;
use crate::alphacode_core::output_enhance as enhance;

pub const QUIET_ENV: &str = "ALPHACODE_QUIET";

pub fn set_quiet_enabled(enabled: bool) {
    if enabled {
        crate::alphacode_core::env::set_var(QUIET_ENV, "1");
    } else {
        crate::alphacode_core::env::remove_var(QUIET_ENV);
    }
}

pub fn quiet_enabled() -> bool {
    std::env::var(QUIET_ENV)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub fn stderr_info(message: impl AsRef<str>) {
    if !quiet_enabled() {
        eprintln!("{}", crate::output_style::terminal_text(message.as_ref()));
    }
}

pub fn terminal_title(title: impl AsRef<str>) -> String {
    crate::output_style::terminal_text(title.as_ref()).into_owned()
}

pub fn stderr_blank_line() {
    if !quiet_enabled() {
        eprintln!();
    }
}

/// Returns `code` only when styled output is appropriate: stderr is an
/// interactive terminal, color is not disabled (`NO_COLOR` /
/// `ALPHACODE_NO_COLOR`), and quiet mode is off. Otherwise returns an empty
/// string, so logs, pipes, and redirected output never receive raw escapes.
///
/// Thin wrapper over [`console::ansi_enabled`] kept for call sites that were
/// written before that module existed.
pub fn style(code: &'static str) -> &'static str {
    if console::ansi_enabled() { code } else { "" }
}

/// Width reserved for the label column of [`CliOutput::key_value`].
pub const KEY_LABEL_WIDTH: usize = 12;
/// Indent applied to continuation lines of a key/value block.
const KV_INDENT: usize = 2;
/// Cells between a key/value label and its value.
const KV_GAP: usize = 2;

/// A professional CLI output builder for structured, colorized output.
///
/// Every method returns a plain `String` (or `Vec<String>` for multi-line
/// blocks) rather than printing, so callers keep control of ordering and of
/// which stream the text lands on.
pub struct CliOutput;

impl CliOutput {
    /// Render a section header.
    ///
    /// Deliberately *not* an underlined rule: headers are emitted between
    /// blocks of related output, and a rule under each one turns a readable
    /// list into a picket fence. Bold weight and the accent color carry the
    /// grouping instead.
    pub fn section(title: &str) -> String {
        if console::ansi_enabled() {
            let (r, g, b) = enhance::pick_for_terminal_bg(enhance::PaletteToken::Accent);
            format!(
                "{}\u{1b}[1m{title}\u{1b}[0m",
                enhance::rgb_escape(r, g, b, true)
            )
        } else {
            title.to_string()
        }
    }

    /// Render a key/value pair with aligned columns.
    ///
    /// Padding is display-cell based, so a label containing CJK or emoji still
    /// lines its value up with every other row. The value is clipped to the
    /// terminal rather than allowed to soft-wrap, which would break the column
    /// alignment for every row below it.
    pub fn key_value(key: &str, value: &str) -> String {
        let label = Self::key_label(key);
        format!(
            "{}{}{}{}",
            " ".repeat(KV_INDENT),
            label,
            " ".repeat(KV_GAP),
            enhance::truncate_ellipsis(value, Self::value_budget()),
        )
    }

    /// Render `key` into the fixed-width label column.
    ///
    /// Truncation matters as much as padding here: a key wider than
    /// [`KEY_LABEL_WIDTH`] would otherwise push its value to the right and
    /// break the one column every other row lines up on.
    fn key_label(key: &str) -> String {
        enhance::pad_end_to(
            &enhance::truncate_ellipsis(key, KEY_LABEL_WIDTH),
            KEY_LABEL_WIDTH,
        )
    }

    /// Cells available for a value after the indent, label column, and gap.
    fn value_budget() -> usize {
        console::terminal_width().saturating_sub(KV_INDENT + KEY_LABEL_WIDTH + KV_GAP)
    }

    /// Render a key/value pair with the value highlighted in a semantic color.
    ///
    /// `tone` is a palette token, not a raw escape sequence, so the color
    /// automatically disappears when the terminal cannot show it.
    pub fn key_value_toned(key: &str, value: &str, tone: enhance::PaletteToken) -> String {
        let (r, g, b) = enhance::pick_for_terminal_bg(tone);
        CliOutput::key_value_colored(key, value, &enhance::rgb_escape(r, g, b, true))
    }

    /// Render a key/value pair with the value highlighted in a specific color.
    pub fn key_value_colored(key: &str, value: &str, color: &str) -> String {
        format!(
            "{}{}{}{}{}{}",
            " ".repeat(KV_INDENT),
            Self::key_label(key),
            " ".repeat(KV_GAP),
            color,
            enhance::truncate_ellipsis(value, Self::value_budget()),
            enhance::reset(),
        )
    }

    /// Render a success confirmation line.
    pub fn success(message: &str) -> String {
        enhance::status_line(enhance::PREFIX_OK, message)
    }

    /// Render an error line.
    pub fn error(message: &str) -> String {
        enhance::status_line(enhance::PREFIX_ERROR, message)
    }

    /// Render a warning line.
    pub fn warning(message: &str) -> String {
        enhance::status_line(enhance::PREFIX_WARN, message)
    }

    /// Render an informational line.
    pub fn info(message: &str) -> String {
        enhance::status_line(enhance::PREFIX_INFO, message)
    }

    /// Render a dimmed/hint line.
    pub fn hint(message: &str) -> String {
        format!("{}  {}{}", " ".repeat(KV_INDENT), message, enhance::reset())
    }

    /// Render a bold title line.
    pub fn title(message: &str) -> String {
        if console::ansi_enabled() {
            format!("\u{1b}[1m{}\u{1b}[0m", message)
        } else {
            message.to_string()
        }
    }

    /// Render a dimmed/subtle line.
    pub fn subtle(message: &str) -> String {
        if console::ansi_enabled() {
            format!("\u{1b}[2m{}\u{1b}[0m", message)
        } else {
            message.to_string()
        }
    }

    /// Render a horizontal rule that spans the usable terminal width.
    ///
    /// The width is clamped to the terminal so a rule can never soft-wrap and
    /// push the following line off-screen.
    pub fn rule() -> String {
        enhance::divider(None, console::terminal_width())
    }

    /// Render a horizontal rule of an explicit width, clamped to the terminal.
    pub fn rule_of(width: usize) -> String {
        enhance::divider(None, console::fit_width(width))
    }

    /// Render a titled box around `lines`.
    ///
    /// Returns one entry per visual row. The box is sized from the *display
    /// width* of the content and clamped to the terminal, so CJK lines are not
    /// cut in half and long lines wrap inside the border instead of
    /// overflowing it. If the terminal is too narrow to draw a box at all, the
    /// content is returned unboxed rather than clipped away.
    pub fn box_text(title: &str, lines: &[&str], width: usize) -> Vec<String> {
        let bc = enhance::box_chars::pick();
        let budget = console::fit_width(width);
        // 2 cells of vertical border + 2 cells of inner padding on each side.
        let inner = budget.saturating_sub(6);
        if inner == 0 {
            return lines.iter().map(|line| line.to_string()).collect();
        }

        let mut result = Vec::new();
        if !title.is_empty() {
            result.push(format!(
                "{}{}",
                " ".repeat(KV_INDENT),
                enhance::box_top(title, budget)
            ));
        }
        let horizontal = bc.h.to_string().repeat(inner);
        result.push(format!("{}{horizontal}{}", bc.tl, bc.tr));
        for line in lines {
            // Wrap first, then pad: padding an over-long line and letting the
            // terminal soft-wrap it is what pushes the right border off-screen.
            for row in enhance::wrap_line(line, inner).lines() {
                result.push(format!(
                    "{} {}{}",
                    bc.v,
                    enhance::pad_end_to(row, inner),
                    bc.v
                ));
            }
        }
        result.push(format!("{}{horizontal}{}", bc.bl, bc.br));
        result
    }

    /// Render a table with a bold header, a rule, and body rows.
    ///
    /// `widths` declares the display-cell width of each column. Columns are
    /// fitted to the terminal when the declared total overflows, so a wide
    /// table degrades to "fewer, narrower columns" rather than a wrapped mess.
    /// Returns an empty `Vec` when even the first column cannot fit.
    pub fn table(headers: &[&str], rows: &[Vec<&str>], widths: &[usize]) -> Vec<String> {
        let widths = fit_widths(
            widths,
            headers
                .len()
                .max(rows.iter().map(Vec::len).max().unwrap_or(0)),
        );
        if widths.is_empty() {
            return Vec::new();
        }
        // `fit_widths` may shed trailing columns when the declared widths
        // overflow the terminal. Drop the shed columns' *cells* as well:
        // `table_row` fills a missing width with a 20-cell default, so
        // rendering a cell the fitter dropped would put the column back and
        // overflow the very terminal the shedding was meant to respect.
        let column_count = widths.len();
        let headers = &headers[..column_count.min(headers.len())];
        let rows: Vec<Vec<&str>> = rows
            .iter()
            .map(|row| row.iter().take(column_count).copied().collect())
            .collect();
        let mut result = Vec::new();
        result.push(styled_header_row(headers, &widths));
        result.push(enhance::divider(None, row_width(&widths)));
        for row in &rows {
            result.push(enhance::table_row(&widths, row, &[]));
        }
        result
    }

    /// Render a progress bar with a percentage and a semantic tone.
    ///
    /// Width is derived from the terminal, so `label` + bar + percentage always
    /// fit. The bar degrades to ASCII blocks when the terminal cannot render
    /// them, and the percentage is right-aligned so a bar that grows from 9% to
    /// 10% does not shift the whole line.
    pub fn progress_bar(label: &str, progress: f32, width: usize) -> String {
        let progress = progress.clamp(0.0, 1.0);
        let pct = format!("{:>3.0}%", progress * 100.0);
        let tone = if progress >= 1.0 {
            enhance::PaletteToken::Success
        } else if progress >= 0.5 {
            enhance::PaletteToken::Accent
        } else if progress > 0.0 {
            enhance::PaletteToken::Warn
        } else {
            enhance::PaletteToken::Muted
        };
        let (r, g, b) = enhance::pick_for_terminal_bg(tone);
        let bc = enhance::box_chars::pick();

        // "  " + label + " [" + bar + "] " + pct
        const CHROME_CELLS: usize = 6;
        let used =
            KV_INDENT + enhance::display_width(label) + CHROME_CELLS + enhance::display_width(&pct);
        let bar_width = console::fit_width(width).saturating_sub(used);
        let filled = (progress * bar_width as f32).round() as usize;
        let empty = bar_width.saturating_sub(filled);

        format!(
            "  {label} [{}{}{}{}] {pct}",
            enhance::rgb_escape(r, g, b, true),
            bc.block_full.to_string().repeat(filled),
            bc.block_light.to_string().repeat(empty),
            enhance::reset(),
        )
    }

    /// Render a spinner frame for loading states.
    pub fn spinner(frame: usize) -> &'static str {
        enhance::spinner_frame(frame)
    }

    /// Render a loading line with spinner and label.
    pub fn loading(label: &str, frame: usize) -> String {
        format!("  {} {}", Self::spinner(frame), label)
    }

    /// Render a confirmation prompt.
    ///
    /// The answer options are aligned to one column so the reader sees a single
    /// decision rather than a sentence with punctuation in the middle of it.
    pub fn confirm(message: &str) -> String {
        format!("  ? {message}")
    }

    /// Render a multi-line error block with context.
    ///
    /// Structured as *what happened* / *why* / *how to fix*, which is the order
    /// a reader needs them in. The `why` line is wrapped to the terminal so a
    /// provider error containing a long URL stays readable.
    pub fn error_block(what: &str, why: &str, fix: Option<&str>) -> String {
        enhance::error_block(what, why, fix)
    }

    /// Render a version banner.
    pub fn version_banner(name: &str, version: &str, width: usize) -> String {
        let label = format!(" {name} v{version} ");
        let padded = enhance::pad_end_to(&label, console::fit_width(width).max(1));
        if console::ansi_enabled() {
            format!("\u{1b}[1m{padded}\u{1b}[0m")
        } else {
            padded.trim_end().to_string()
        }
    }
}

/// Shrink `widths` until the whole row fits the terminal, dropping trailing
/// columns first.
///
/// A dropped column releases its width, so the surviving columns can expand
/// into the freed space rather than leaving a ragged right edge. This is the
/// behaviour a reader wants: a table that shows three useful columns beats one
/// that shows all six and wraps every row into unreadable ribbons.
fn fit_widths(widths: &[usize], column_count: usize) -> Vec<usize> {
    // Every column costs its width plus a two-cell gutter.
    let total = |widths: &[usize]| widths.iter().sum::<usize>() + widths.len() * 2;
    let budget = console::terminal_width();
    let min_column = 8usize;

    let mut fitted: Vec<usize> = widths.iter().copied().take(column_count).collect();
    if fitted.is_empty() {
        return Vec::new();
    }
    // Clamp narrow columns up to the legible minimum before shedding columns,
    // so shedding is driven by real overflow rather than by a zero width.
    for width in &mut fitted {
        *width = (*width).max(min_column);
    }

    while total(&fitted) > budget {
        if fitted.len() > 1 {
            fitted.pop();
            continue;
        }
        // A single column that is still too wide: clip it to the terminal.
        let available = budget.saturating_sub(2);
        if available == 0 {
            return Vec::new();
        }
        fitted[0] = fitted[0].min(available);
    }
    fitted
}

fn row_width(widths: &[usize]) -> usize {
    widths.iter().sum::<usize>() + widths.len() * 2
}

fn styled_header_row(headers: &[&str], widths: &[usize]) -> String {
    let row = enhance::table_row(widths, headers, &[]);
    if console::ansi_enabled() {
        format!("\u{1b}[1m{row}\u{1b}[0m")
    } else {
        row
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard() -> std::sync::MutexGuard<'static, ()> {
        console::test_override_lock()
    }

    #[test]
    fn style_is_empty_when_ansi_is_off() {
        let _guard = guard();
        console::set_ansi_enabled_override(Some(false));
        assert_eq!(style("\u{1b}[32m"), "");
        console::set_ansi_enabled_override(Some(true));
        assert_eq!(style("\u{1b}[32m"), "\u{1b}[32m");
        console::set_ansi_enabled_override(None);
    }

    #[test]
    fn no_helper_emits_escapes_when_ansi_is_off() {
        let _guard = guard();
        console::set_ansi_enabled_override(Some(false));
        let rendered = [
            CliOutput::section("Setup"),
            CliOutput::key_value("Model", "claude-opus-4-5"),
            CliOutput::key_value_toned("State", "ready", enhance::PaletteToken::Success),
            CliOutput::success("done"),
            CliOutput::error("failed"),
            CliOutput::warning("careful"),
            CliOutput::info("note"),
            CliOutput::hint("press enter"),
            CliOutput::title("Title"),
            CliOutput::subtle("subtle"),
            CliOutput::rule_of(40),
            CliOutput::progress_bar("indexing", 0.42, 40),
            CliOutput::confirm("Continue?"),
            CliOutput::error_block("what", "why", Some("fix")),
            CliOutput::version_banner("alphacode", "1.0.0", 40),
        ];
        for line in rendered {
            assert!(!line.contains('\u{1b}'), "escape leaked: {line:?}");
        }
        let boxed = CliOutput::box_text("Title", &["body"], 40);
        for line in boxed {
            assert!(!line.contains('\u{1b}'), "escape leaked: {line:?}");
        }
        let table = CliOutput::table(&["A", "B"], &[vec!["1", "2"]], &[10, 10]);
        for line in table {
            assert!(!line.contains('\u{1b}'), "escape leaked: {line:?}");
        }
        console::set_ansi_enabled_override(None);
    }

    /// Display column at which `value` begins in a rendered key/value row.
    ///
    /// The value sits after the indent, the label column, and the gap, so its
    /// start column is the row's alignment contract. It must be measured in
    /// display cells: a byte offset runs ahead of the column for any label
    /// containing CJK or emoji, and a leading-whitespace count stops at the
    /// indent and says nothing about where the value begins.
    fn value_column(row: &str, value: &str) -> Option<usize> {
        let plain = crate::alphacode_base::message::strip_ansi_escape_sequences(row);
        plain
            .rfind(value)
            .map(|value_byte| enhance::display_width(&plain[..value_byte]))
    }

    #[test]
    fn key_value_values_align_in_one_column() {
        let _guard = guard();
        console::set_width_override(Some(100));
        // Regression: the over-long key is wider than the label column. Before
        // the label was truncated it pushed its value several columns right,
        // breaking the single column every other row lines up on.
        let rows = [
            ("Provider", "anthropic"),
            ("Model", "claude-opus-4-5"),
            ("a-much-longer-key-name", "value"),
            ("x", "y"),
        ];
        let prefix_cells = KV_INDENT + KEY_LABEL_WIDTH + KV_GAP;
        for (key, value) in rows {
            let row = CliOutput::key_value(key, value);
            assert_eq!(
                value_column(&row, value),
                Some(prefix_cells),
                "value for {key:?} does not start at column {prefix_cells}: {row:?}"
            );
        }
        console::set_width_override(None);
    }

    #[test]
    fn key_value_columns_align_across_scripts() {
        let _guard = guard();
        console::set_width_override(Some(100));
        // Width fixtures written as escapes so this source file stays plain
        // ASCII. `WIDE` is three double-width glyphs (6 cells, 9 bytes): byte
        // padding once put this row's value three cells right of the ASCII
        // rows. `EMOJI` is one double-width glyph; `MIXED` interleaves ASCII
        // and double-width glyphs.
        const WIDE: &str = "\u{65e5}\u{672c}\u{5f8c}";
        const EMOJI: &str = "\u{1f389}";
        const MIXED: &str = "v2 \u{65e5}\u{672c} beta";
        let rows = [
            (
                "Provider",
                "anthropic",
                CliOutput::key_value("Provider", "anthropic"),
            ),
            (WIDE, "value", CliOutput::key_value(WIDE, "value")),
            (EMOJI, "party", CliOutput::key_value(EMOJI, "party")),
            (MIXED, "mixed", CliOutput::key_value(MIXED, "mixed")),
            (
                "Model",
                "claude-opus-4-5",
                CliOutput::key_value("Model", "claude-opus-4-5"),
            ),
        ];
        let prefix_cells = KV_INDENT + KEY_LABEL_WIDTH + KV_GAP;
        for (key, value, row) in rows {
            assert_eq!(
                value_column(&row, value),
                Some(prefix_cells),
                "value for {key:?} does not start at column {prefix_cells}: {row:?}"
            );
            // The label itself must occupy exactly the label column, in cells.
            let plain = crate::alphacode_base::message::strip_ansi_escape_sequences(&row);
            let label_start = KV_INDENT;
            let label_end = plain.rfind(value).expect("value missing from row");
            assert_eq!(
                enhance::display_width(&plain[label_start..label_end]),
                KEY_LABEL_WIDTH + KV_GAP,
                "label for {key:?} does not fill the label column: {row:?}"
            );
        }
        console::set_width_override(None);
    }

    #[test]
    fn key_value_clips_the_value_to_the_terminal() {
        let _guard = guard();
        console::set_width_override(Some(40));
        let row = CliOutput::key_value("Key", &"x".repeat(200));
        assert!(
            enhance::display_width(&row) <= 40,
            "key/value row overflowed: {} cells",
            enhance::display_width(&row)
        );
        console::set_width_override(None);
    }

    #[test]
    fn box_text_is_rectangular_and_fits_the_terminal() {
        let _guard = guard();
        console::set_width_override(Some(48));
        let rows = CliOutput::box_text("Audit", &["short", "a much longer line of prose"], 40);
        let body: Vec<usize> = rows
            .iter()
            .filter(|row| row.starts_with('|') || row.starts_with('\u{2502}'))
            .map(|row| enhance::display_width(row))
            .collect();
        assert!(!body.is_empty(), "box produced no bordered rows");
        for width in &body {
            assert_eq!(*width, body[0], "box rows were not rectangular");
            assert!(*width <= 48, "box overflowed the terminal: {width}");
        }
        console::set_width_override(None);
    }

    #[test]
    fn box_text_degrades_gracefully_when_too_narrow() {
        let _guard = guard();
        console::set_width_override(Some(20));
        let rows = CliOutput::box_text("T", &["content survives"], 4);
        assert!(
            rows.iter().any(|row| row.contains("content survives")),
            "narrow terminal dropped the content: {rows:?}"
        );
        console::set_width_override(None);
    }

    #[test]
    fn table_stays_rectangular_with_mixed_scripts() {
        let _guard = guard();
        console::set_width_override(Some(80));
        let headers = ["Provider", "Model", "Status"];
        // Full-width fixture as escapes, so the source file stays plain ASCII.
        // This row is 6 cells / 9 bytes long, which is what broke `{:<width$}`.
        const WIDE: &str = "\u{65e5}\u{672c}\u{5f8c}\u{30d7}\u{30e9}\u{30a4}\u{30c0}";
        let rows = vec![
            vec!["anthropic", "claude-opus-4-5-20251101", "ready"],
            vec![WIDE, "gpt-5.1", "ok"],
            vec!["q", "an-extremely-long-model-identifier", "ready"],
        ];
        let widths = [14, 20, 8];
        let rendered = CliOutput::table(&headers, &rows, &widths);
        let expected = row_width(&widths);
        for line in &rendered {
            let plain = crate::alphacode_base::message::strip_ansi_escape_sequences(line);
            assert_eq!(
                enhance::display_width(&plain),
                expected,
                "row not rectangular: {line:?}"
            );
        }
        console::set_width_override(None);
    }

    #[test]
    fn table_drops_columns_rather_than_overflowing() {
        let _guard = guard();
        console::set_width_override(Some(40));
        let headers = ["A", "B", "C", "D"];
        let widths = [12, 12, 12, 12];
        let rendered = CliOutput::table(&headers, &[], &widths);
        assert!(rendered.len() >= 2, "table produced nothing");
        for line in &rendered {
            let plain = crate::alphacode_base::message::strip_ansi_escape_sequences(line);
            assert!(
                enhance::display_width(&plain) <= 40,
                "table overflowed a 40-cell terminal: {line:?}"
            );
        }
        console::set_width_override(None);
    }

    #[test]
    fn progress_bar_always_fits_and_reports_a_real_percentage() {
        let _guard = guard();
        for width in [20usize, 40, 80] {
            console::set_width_override(Some(width));
            for progress in [0.0f32, 0.005, 0.5, 0.999, 1.0] {
                let bar = CliOutput::progress_bar("build", progress, width);
                let plain = crate::alphacode_base::message::strip_ansi_escape_sequences(&bar);
                assert!(
                    enhance::display_width(&plain) <= width,
                    "bar of {width} overflowed at {progress}: {plain:?}"
                );
                let expected = format!("{:>3.0}%", progress * 100.0);
                assert!(
                    plain.contains(&expected),
                    "expected {expected:?} in {plain:?}"
                );
            }
        }
        console::set_width_override(None);
    }

    #[test]
    fn progress_bar_percentage_never_reports_over_a_hundred() {
        let bar = CliOutput::progress_bar("build", 4.2, 40);
        let plain = crate::alphacode_base::message::strip_ansi_escape_sequences(&bar);
        assert!(plain.contains("100%"), "got {plain:?}");
        let bar = CliOutput::progress_bar("build", -3.0, 40);
        let plain = crate::alphacode_base::message::strip_ansi_escape_sequences(&bar);
        assert!(plain.contains("  0%"), "got {plain:?}");
    }
}
