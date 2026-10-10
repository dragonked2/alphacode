pub mod chrome;
pub mod layout;
pub mod memory_tiles;
pub mod swarm_gallery;
pub mod swarm_tiles;

use ratatui::prelude::{Color, Line, Span, Style};
use unicode_segmentation::UnicodeSegmentation;

/// Border character sets for rendered boxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoxCharset {
    /// Rounded corners with light horizontal/vertical lines.
    #[default]
    Rounded,
    /// Double lines with square corners.
    Double,
    /// Thick lines with square corners.
    Thick,
}

impl BoxCharset {
    pub const fn corners(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            BoxCharset::Rounded => ("╭", "╮", "╰", "╯"),
            BoxCharset::Double => ("╔", "╗", "╚", "╝"),
            BoxCharset::Thick => ("┏", "┓", "┗", "┛"),
        }
    }

    pub const fn horizontal(&self) -> &'static str {
        match self {
            BoxCharset::Rounded => "─",
            BoxCharset::Double => "═",
            BoxCharset::Thick => "━",
        }
    }

    pub const fn vertical(&self) -> &'static str {
        match self {
            BoxCharset::Rounded => "│",
            BoxCharset::Double => "║",
            BoxCharset::Thick => "┃",
        }
    }
}

/// Render a box with configurable border style, title style, and content background.
///
/// This is the full-featured builder; [`render_rounded_box`] delegates here with
/// default title and content styles for backward compatibility.
pub fn render_styled_box(
    title: &str,
    content: Vec<Line<'static>>,
    max_width: usize,
    border_style: Style,
    title_style: Style,
    content_bg: Option<Color>,
    charset: BoxCharset,
) -> Vec<Line<'static>> {
    if content.is_empty() || max_width < 6 {
        return Vec::new();
    }

    let max_content_width = content
        .iter()
        .map(|line| line.width())
        .max()
        .unwrap_or(0)
        .min(max_width.saturating_sub(4));

    let truncated_title = truncate_line_with_ellipsis_to_width(
        &Line::from(Span::raw(format!(" {} ", title))),
        max_width.saturating_sub(2).max(1),
    );
    let title_text = line_plain_text(&truncated_title);
    let title_len = truncated_title.width();
    let box_content_width = max_content_width.max(title_len.saturating_sub(2));

    if box_content_width < 6 {
        return Vec::new();
    }

    let box_width = box_content_width + 4;
    let border_chars = box_width.saturating_sub(title_len + 2);
    let left_border = charset.horizontal().repeat(border_chars / 2);
    let right_border = charset.horizontal().repeat(border_chars - border_chars / 2);

    let (tl, tr, bl, br) = charset.corners();
    let v = charset.vertical();
    let h = charset.horizontal();

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(format!("{tl}{}", left_border), border_style),
        Span::styled(title_text.clone(), title_style),
        Span::styled(format!("{}{}", right_border, tr), border_style),
    ]));

    for line in content {
        let truncated = truncate_line_to_width(&line, box_content_width);
        let padding = box_content_width.saturating_sub(truncated.width());
        let mut spans: Vec<Span<'static>> = Vec::new();
        let border_with_bg = if let Some(bg) = content_bg {
            border_style.bg(bg)
        } else {
            border_style
        };
        spans.push(Span::styled(format!("{} ", v), border_with_bg));
        spans.extend(truncated.spans);
        if padding > 0 {
            spans.push(Span::styled(" ".repeat(padding), border_with_bg));
        }
        spans.push(Span::styled(format!(" {}", v), border_with_bg));
        lines.push(Line::from(spans));
    }

    let bottom_border = h.repeat(box_width.saturating_sub(2));
    lines.push(Line::from(Span::styled(
        format!("{bl}{}{br}", bottom_border),
        border_style,
    )));

    lines
}

/// Render a box with rounded Unicode corners (the historical default).
pub fn render_rounded_box(
    title: &str,
    content: Vec<Line<'static>>,
    max_width: usize,
    border_style: Style,
) -> Vec<Line<'static>> {
    render_styled_box(
        title,
        content,
        max_width,
        border_style,
        border_style,
        None,
        BoxCharset::Rounded,
    )
}

/// Render a box with double-line Unicode borders for premium/important content
/// (e.g. error dialogs, release notes, update notifications).
pub fn render_double_rounded_box(
    title: &str,
    content: Vec<Line<'static>>,
    max_width: usize,
    border_style: Style,
    title_style: Style,
) -> Vec<Line<'static>> {
    render_styled_box(
        title,
        content,
        max_width,
        border_style,
        title_style,
        None,
        BoxCharset::Double,
    )
}

/// Render a box with thick Unicode borders for high-contrast emphasis.
pub fn render_thick_box(
    title: &str,
    content: Vec<Line<'static>>,
    max_width: usize,
    border_style: Style,
    title_style: Style,
) -> Vec<Line<'static>> {
    render_styled_box(
        title,
        content,
        max_width,
        border_style,
        title_style,
        None,
        BoxCharset::Thick,
    )
}

pub fn truncate_line_to_width(line: &Line<'static>, width: usize) -> Line<'static> {
    let (spans, _) = truncate_spans_to_width(&line.spans, width);
    let mut truncated = Line::from(spans);
    truncated.alignment = line.alignment;
    truncated
}

pub fn truncate_line_with_ellipsis_to_width(line: &Line<'static>, width: usize) -> Line<'static> {
    if width == 0 {
        let mut empty = Line::from("");
        empty.alignment = line.alignment;
        return empty;
    }
    if line.width() <= width {
        return line.clone();
    }
    if width == 1 {
        let mut ellipsis = Line::from(Span::raw("…"));
        ellipsis.alignment = line.alignment;
        return ellipsis;
    }

    let (mut spans, ellipsis_style) = truncate_spans_to_width(&line.spans, width.saturating_sub(1));
    spans.push(Span::styled("…", ellipsis_style.unwrap_or_default()));
    let mut truncated = Line::from(spans);
    truncated.alignment = line.alignment;
    truncated
}

/// Clip styled spans without splitting a user-perceived Unicode character.
/// All line truncation in the TUI goes through this helper so output, badges,
/// and chrome agree on terminal-cell widths and style boundaries.
fn truncate_spans_to_width(
    spans: &[Span<'static>],
    max_width: usize,
) -> (Vec<Span<'static>>, Option<Style>) {
    let mut remaining = max_width;
    let mut kept = Vec::new();
    let mut last_style = None;

    for span in spans {
        if remaining == 0 {
            break;
        }

        let text = span.content.as_ref();
        let span_width = unicode_width::UnicodeWidthStr::width(text);
        if span_width <= remaining {
            if !text.is_empty() {
                last_style = Some(span.style);
            }
            kept.push(span.clone());
            remaining = remaining.saturating_sub(span_width);
            continue;
        }

        let mut clipped = String::new();
        let mut used = 0usize;
        for grapheme in text.graphemes(true) {
            let grapheme_width = unicode_width::UnicodeWidthStr::width(grapheme);
            if used.saturating_add(grapheme_width) > remaining {
                break;
            }
            clipped.push_str(grapheme);
            used = used.saturating_add(grapheme_width);
        }
        if !clipped.is_empty() {
            kept.push(Span::styled(clipped, span.style));
            last_style = Some(span.style);
        }
        break;
    }

    (kept, last_style)
}

pub fn truncate_line_preserving_suffix_to_width(
    prefix: &Line<'static>,
    suffix: &Line<'static>,
    width: usize,
) -> Line<'static> {
    if width == 0 {
        let mut empty = Line::from("");
        empty.alignment = prefix.alignment;
        return empty;
    }

    if suffix.width() == 0 {
        return truncate_line_with_ellipsis_to_width(prefix, width);
    }

    let mut combined_spans = prefix.spans.clone();
    combined_spans.extend(suffix.spans.clone());
    let mut combined = Line::from(combined_spans);
    combined.alignment = prefix.alignment;
    if combined.width() <= width {
        return combined;
    }

    let suffix_width = suffix.width();
    if suffix_width >= width {
        let mut truncated = truncate_line_with_ellipsis_to_width(suffix, width);
        truncated.alignment = prefix.alignment;
        return truncated;
    }

    let prefix_budget = width.saturating_sub(suffix_width);
    let mut prefix_part = truncate_line_with_ellipsis_to_width(prefix, prefix_budget);
    prefix_part.spans.extend(suffix.spans.clone());
    prefix_part.alignment = prefix.alignment;
    prefix_part
}

pub fn line_plain_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
}

#[cfg(test)]
mod truncation_tests {
    use super::*;
    use ratatui::prelude::Alignment;

    #[test]
    fn truncation_keeps_zwj_emoji_graphemes_intact() {
        let line = Line::from("A👩‍💻XY");

        let clipped = truncate_line_to_width(&line, 3);
        assert_eq!(line_plain_text(&clipped), "A👩‍💻");
        assert_eq!(clipped.width(), 3);

        let ellipsized = truncate_line_with_ellipsis_to_width(&line, 4);
        assert_eq!(line_plain_text(&ellipsized), "A👩‍💻…");
        assert_eq!(ellipsized.width(), 4);
    }

    #[test]
    fn truncation_keeps_combining_marks_with_their_base_character() {
        let line = Line::from("e\u{301}xy");

        let clipped = truncate_line_to_width(&line, 1);
        assert_eq!(line_plain_text(&clipped), "e\u{301}");
        assert_eq!(clipped.width(), 1);
    }

    #[test]
    fn truncation_preserves_alignment_and_ellipsis_style() {
        let style = Style::default().fg(Color::LightBlue);
        let mut line = Line::from(vec![Span::styled("abcdef", style)]);
        line.alignment = Some(Alignment::Right);

        let clipped = truncate_line_to_width(&line, 3);
        assert_eq!(line_plain_text(&clipped), "abc");
        assert_eq!(clipped.alignment, Some(Alignment::Right));
        assert_eq!(clipped.spans[0].style, style);

        let ellipsized = truncate_line_with_ellipsis_to_width(&line, 4);
        assert_eq!(line_plain_text(&ellipsized), "abc…");
        assert_eq!(ellipsized.alignment, Some(Alignment::Right));
        assert_eq!(ellipsized.spans.last().unwrap().style, style);
    }
}
