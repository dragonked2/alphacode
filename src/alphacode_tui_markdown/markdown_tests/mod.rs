use super::*;

fn line_to_string(line: &Line<'_>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

fn leading_spaces(text: &str) -> usize {
    text.chars().take_while(|c| *c == ' ').count()
}

fn render_markdown_with_mode(text: &str, mode: MarkdownSpacingMode) -> Vec<Line<'static>> {
    with_markdown_spacing_mode_override(Some(mode), || render_markdown(text))
}

fn render_markdown_with_width_and_mode(
    text: &str,
    width: usize,
    mode: MarkdownSpacingMode,
) -> Vec<Line<'static>> {
    with_markdown_spacing_mode_override(Some(mode), || {
        render_markdown_with_width(text, Some(width))
    })
}

fn lines_to_string(lines: &[Line<'_>]) -> String {
    lines
        .iter()
        .map(line_to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[path = "cases.rs"]
mod cases;
