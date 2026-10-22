// Differential test for the incremental streaming markdown renderer.
//
// The incremental renderer reuses the rendered form of a stable prefix and
// re-renders only the volatile tail. That is only sound if the spliced
// output is *identical* to a full render of the same text. This test drives
// a range of markdown shapes through the streaming path and asserts that
// every intermediate frame equals the corresponding full render.
//
// If a future change to the checkpoint logic or to the block-separator
// rules breaks the equivalence, this test fails at the frame that diverges.

/// Render the text into a comparable string: one entry per line, spans
/// concatenated.
fn lines_to_text(lines: &[ratatui::text::Line<'static>]) -> String {
    lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The corpus: markdown shapes that exercise the block kinds the checkpoint
/// logic recognises, plus shapes that deliberately break block continuity.
fn corpus() -> Vec<String> {
    vec![
        // Plain paragraphs.
        "First paragraph.\n\nSecond paragraph.\n".to_string(),
        // Headings (a checkpoint kind) followed by body text.
        "# Heading one\n\nBody after heading one.\n\n## Heading two\n\nBody two.\n".to_string(),
        // Lists (continuity-sensitive: a list item must not be split).
        "- alpha\n- beta\n- gamma\n\nAfter the list.\n".to_string(),
        // Nested list followed by a paragraph.
        "- outer one\n  - inner one\n  - inner two\n- outer two\n\nParagraph.\n".to_string(),
        // Fenced code block (a checkpoint kind, and a block that must not be
        // split across the prefix boundary).
        "Intro.\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\nAfter code.\n".to_string(),
        // An *unclosed* fence: the checkpoint must not advance past it.
        "Intro.\n\n```rust\nfn main() {\n".to_string(),
        // Blockquote.
        "> quoted line\n> another quoted line\n\nAfter quote.\n".to_string(),
        // Thematic break.
        "Above.\n\n---\n\nBelow.\n".to_string(),
        // Display math (a checkpoint kind).
        "Text before.\n\n$$\nE = mc^2\n$$\n\nText after.\n".to_string(),
        // Unclosed display math: checkpoint must not advance past it.
        "Text before.\n\n$$\nE = mc^2\n".to_string(),
        // Table.
        "| a | b |\n| --- | --- |\n| 1 | 2 |\n\nAfter table.\n".to_string(),
        // Inline formatting mixed with prose.
        "Some **bold** and *italic* and `code` and [link](https://example.com).\n\nMore.\n"
            .to_string(),
        // Mermaid block: exercises the deferred-placeholder path.
        "Intro.\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\nAfter diagram.\n".to_string(),
        // Everything at once, which is the realistic streaming shape.
        concat!(
            "# Title\n\n",
            "Intro paragraph with **bold**.\n\n",
            "## Section\n\n",
            "- one\n- two\n\n",
            "```python\nprint('x')\n```\n\n",
            "> quote\n\n",
            "| h | h2 |\n| --- | --- |\n| a | b |\n\n",
            "Closing paragraph.\n"
        )
        .to_string(),
    ]
}

/// Assert that every intermediate streaming frame of `text` equals the full
/// render of the same prefix.
fn assert_incremental_matches_full(text: &str, width: usize) {
    let full = crate::alphacode_tui_markdown::render_markdown_with_width(text, Some(width));
    let expected = lines_to_text(&full);

    let mut renderer = crate::alphacode_tui_markdown::IncrementalMarkdownRenderer::new(Some(width));
    let mut offset = 0usize;
    // Stream in small chunks so the tail is usually incomplete.
    for chunk in text.chars().collect::<Vec<char>>().chunks(7) {
        offset += chunk.len();
        let prefix: String = text.chars().take(offset).collect();
        let lines = renderer.update_shared(&prefix);
        let actual = lines_to_text(&lines);
        let want = lines_to_text(&crate::alphacode_tui_markdown::render_markdown_with_width(
            &prefix, Some(width),
        ));
        assert_eq!(
            actual, want,
            "incremental render diverged from full render at offset {offset} of:\n{text}"
        );
    }
    // Final assertion on the whole document.
    let lines = renderer.update_shared(text);
    assert_eq!(
        lines_to_text(&lines),
        expected,
        "final incremental render diverged from full render for:\n{text}"
    );
}

#[test]
fn incremental_streaming_matches_full_render_for_every_frame() {
    for text in corpus() {
        for width in [40usize, 80, 120] {
            assert_incremental_matches_full(&text, width);
        }
    }
}

#[test]
fn incremental_streaming_handles_rewritten_text() {
    // A response that is *rewritten* (not appended) must fall back to a full
    // render and still match it.
    let mut renderer = crate::alphacode_tui_markdown::IncrementalMarkdownRenderer::new(Some(80));
    let first = "# One\n\nBody one.\n\n## Two\n\nBody two.\n";
    renderer.update_shared(first);
    let second = "# One\n\nBody one changed.\n\n## Two\n\nBody two.\n";
    let lines = renderer.update_shared(second);
    let want = lines_to_text(&crate::alphacode_tui_markdown::render_markdown_with_width(
        second, Some(80),
    ));
    assert_eq!(lines_to_text(&lines), want, "rewritten text must match a full render");
}

#[test]
fn incremental_streaming_handles_width_change() {
    let mut renderer = crate::alphacode_tui_markdown::IncrementalMarkdownRenderer::new(Some(80));
    let text = "# Title\n\nSome body text that will wrap differently at another width.\n";
    renderer.update_shared(text);
    renderer.set_width(Some(40));
    let lines = renderer.update_shared(text);
    let want = lines_to_text(&crate::alphacode_tui_markdown::render_markdown_with_width(
        text, Some(40),
    ));
    assert_eq!(lines_to_text(&lines), want, "width change must match a full render");
}