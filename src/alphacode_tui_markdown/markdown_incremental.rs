use super::*;

pub struct IncrementalMarkdownRenderer {
    /// Previously rendered lines, shared so streaming hits avoid a deep
    /// `Vec<Line>` clone per token batch.
    ///
    /// This is the single owner of the rendered lines. It used to be mirrored
    /// into a second `Vec<Line>` field for the sake of `last_lines()`, but that
    /// mirror was deep-cloned on *every* streaming frame (the one place where
    /// the line count grows largest) and `last_lines()` had no callers, so the
    /// mirror was pure duplicated memory plus duplicated work. `last_lines()`
    /// now borrows straight from this `Arc`.
    rendered_shared: std::sync::Arc<Vec<Line<'static>>>,
    /// Text that was rendered (for comparison)
    rendered_text: String,
    /// Position of last safe checkpoint (after complete block)
    last_checkpoint: usize,
    /// Number of lines at last checkpoint
    lines_at_checkpoint: usize,
    /// Whether a blank separator should be preserved at the checkpoint boundary
    checkpoint_needs_separator: bool,
    /// Whether the cached lines contain a deferred-mermaid pending
    /// placeholder; when true the identical-text fast path must re-render
    /// once the deferred render epoch advances so the completed diagram
    /// replaces the placeholder.
    rendered_mermaid_pending: bool,
    /// Deferred-render epoch observed just before the cached lines were
    /// rendered.
    rendered_mermaid_epoch: u64,
    /// Width constraint
    max_width: Option<usize>,
    /// Rendered lines for the **stable prefix** — the text up to and
    /// including `prefix_checkpoint` — and the byte length of that prefix.
    ///
    /// This is what turns the streaming render from O(n²) into O(n): the
    /// renderer re-parses only the *volatile tail* (the text after the last
    /// complete block) on each token batch and concatenates it onto the
    /// already-rendered prefix, instead of re-parsing the whole accumulated
    /// response every time.
    ///
    /// `prefix_lines` is `None` when no usable prefix exists yet, which
    /// forces a full render.
    prefix_lines: Option<std::sync::Arc<Vec<Line<'static>>>>,
    /// Byte offset in the source text that `prefix_lines` was rendered from.
    prefix_checkpoint: usize,
    /// Source text of the prefix that `prefix_lines` was rendered from.
    ///
    /// Kept so the prefix can be re-validated cheaply (a byte-for-byte
    /// comparison of the immutable prefix is O(prefix) but with no
    /// allocation and no parsing) and so a text that was *rewritten* rather
    /// than appended is detected and forces a full re-render.
    prefix_text: String,
    /// Whether the block that ends at `prefix_checkpoint` is followed by a
    /// blank separator line.
    ///
    /// This belongs to the *cached prefix's* boundary, not to the newest
    /// checkpoint: the tail is spliced onto the cached prefix, so the
    /// separator between the two is the one the cached prefix's own block
    /// calls for.
    prefix_needs_separator: bool,
}

impl IncrementalMarkdownRenderer {
    pub fn new(max_width: Option<usize>) -> Self {
        Self {
            rendered_shared: std::sync::Arc::new(Vec::new()),
            rendered_text: String::new(),
            last_checkpoint: 0,
            lines_at_checkpoint: 0,
            checkpoint_needs_separator: false,
            rendered_mermaid_pending: false,
            rendered_mermaid_epoch: 0,
            max_width,
            prefix_lines: None,
            prefix_checkpoint: 0,
            prefix_text: String::new(),
            prefix_needs_separator: false,
        }
    }

    /// Update with new text, returns rendered lines
    ///
    /// This method efficiently handles streaming by:
    /// 1. Detecting if text was only appended (common case)
    /// 2. Finding safe re-render points (after complete blocks)
    /// 3. Only re-rendering from the last safe point
    pub fn update(&mut self, full_text: &str) -> Vec<Line<'static>> {
        self.update_shared(full_text).as_ref().clone()
    }

    /// Zero-copy variant: shares the cached `Vec<Line>` via `Arc`.
    /// Streaming render loops should prefer this to avoid cloning every
    /// `Span`/`String` on each token batch.
    pub fn update_shared(&mut self, full_text: &str) -> std::sync::Arc<Vec<Line<'static>>> {
        with_streaming_render_context(|| self.update_internal_shared(full_text))
    }

    /// Borrow the last rendered lines without cloning.
    pub fn last_lines(&self) -> &[Line<'static>] {
        &self.rendered_shared
    }

    pub fn debug_memory_profile(&self) -> serde_json::Value {
        let rendered_lines_estimate_bytes = estimate_lines_bytes(&self.rendered_shared);
        let rendered_text_bytes = self.rendered_text.capacity();
        serde_json::json!({
            "rendered_lines_count": self.rendered_shared.len(),
            "rendered_lines_estimate_bytes": rendered_lines_estimate_bytes,
            "rendered_text_bytes": rendered_text_bytes,
            "last_checkpoint": self.last_checkpoint,
            "lines_at_checkpoint": self.lines_at_checkpoint,
            "total_estimate_bytes": rendered_lines_estimate_bytes + rendered_text_bytes,
        })
    }

    #[allow(dead_code)]
    fn update_internal(&mut self, full_text: &str) -> Vec<Line<'static>> {
        self.update_internal_shared(full_text).as_ref().clone()
    }

    fn update_internal_shared(&mut self, full_text: &str) -> std::sync::Arc<Vec<Line<'static>>> {
        // Fast path: text unchanged. Not taken while a deferred mermaid
        // placeholder is baked into the cached lines and the deferred render
        // epoch has advanced: the background render finished, so re-render to
        // pick up the completed diagram.
        if full_text == self.rendered_text
            && !(self.rendered_mermaid_pending
                && mermaid::deferred_render_epoch() != self.rendered_mermaid_epoch)
        {
            return std::sync::Arc::clone(&self.rendered_shared);
        }

        // Incremental path: re-render only the volatile tail.
        //
        // The previous implementation re-parsed the *entire* accumulated
        // response on every token batch. For a response of length n streamed
        // in k batches that is O(n*k) parse work — quadratic in the response
        // length, and it grows the longer the answer gets. On a long answer
        // this showed up as a visibly stuttering stream.
        //
        // Instead we keep the *rendered* form of the stable prefix (the text
        // up to the last complete markdown block) and re-render only what
        // came after it. Because the prefix ends at a block boundary, its
        // rendered lines cannot change when more text is appended, so
        // re-rendering the tail and concatenating produces exactly the same
        // output as a full render — at a fraction of the cost.
        //
        // Correctness guards:
        //  - the prefix is only reused when the *source* prefix is unchanged
        //    byte-for-byte, so a rewritten (not appended) response falls
        //    back to a full render;
        //  - the prefix must end on a block boundary that the renderer
        //    recognises, so a block cannot be split across the boundary;
        //  - the deferred-mermaid epoch is re-checked, because a background
        //    diagram render can complete while the prefix is cached.
        let mermaid_epoch_before = mermaid::deferred_render_epoch();

        let checkpoint = self.find_last_complete_block_checkpoint(full_text);
        let checkpoint_offset = checkpoint.map(|cp| cp.offset).unwrap_or(0);

        // Can we reuse the cached prefix?
        //
        // Only when the document renders the same on its own as it does
        // inside the full text. That holds for prose and headings, and fails
        // for the constructs whose rendered form depends on the rest of the
        // document (see [`supports_incremental_splice`]).
        let can_reuse_prefix = supports_incremental_splice(full_text)
            && match (&self.prefix_lines, self.prefix_checkpoint) {
                (Some(_), 0) => false,
                (Some(_), cp) => {
                    cp <= checkpoint_offset
                        && full_text
                            .as_bytes()
                            .starts_with(self.prefix_text.as_bytes())
                }
                (None, _) => false,
            };

        let (lines, new_prefix_lines, new_prefix_checkpoint, new_prefix_text, new_prefix_separator) =
            if can_reuse_prefix {
                let prefix = self
                    .prefix_lines
                    .as_ref()
                    .expect("prefix_lines present when can_reuse_prefix");
                let prefix_len = self.prefix_checkpoint;
                // Re-render only the tail after the cached prefix.
                let tail = &full_text[prefix_len..];
                let tail_lines = render_markdown_with_width(tail, self.max_width);

                // Splice the tail onto the cached prefix. The tail render starts
                // on a block boundary, so it is a standalone document.
                let mut combined: Vec<Line<'static>> =
                    Vec::with_capacity(prefix.len() + tail_lines.len());
                combined.extend(prefix.iter().cloned());
                // The prefix render strips trailing blank separator lines, so
                // restore the separator that the full renderer inserts between the
                // last prefix block and the first tail block when the checkpoint
                // requires it.
                //
                // The flag belongs to the *cached prefix's* boundary, not to the
                // newest checkpoint: the cached prefix ends at
                // `prefix_checkpoint`, and that is the boundary the tail is
                // spliced onto.
                if self.prefix_needs_separator {
                    let last_blank = combined
                        .last()
                        .map(|line| {
                            line.spans.is_empty()
                                || line.spans.iter().all(|span| span.content.is_empty())
                        })
                        .unwrap_or(true);
                    let tail_first_blank = tail_lines
                        .first()
                        .map(|line| {
                            line.spans.is_empty()
                                || line.spans.iter().all(|span| span.content.is_empty())
                        })
                        .unwrap_or(true);
                    if !last_blank && !tail_first_blank {
                        combined.push(Line::default());
                    }
                }
                combined.extend(tail_lines);

                // The new stable prefix is everything up to the newest
                // checkpoint, and its rendered form is the prefix we already
                // have plus the tail's rendering up to that point. Rather than
                // re-render the prefix, we only advance the cached prefix when
                // the *entire* tail has been consumed into the checkpoint, which
                // keeps the cached prefix aligned with a block boundary.
                let consumed = checkpoint_offset.saturating_sub(prefix_len);
                let mut new_prefix_lines = std::sync::Arc::clone(prefix);
                let mut new_prefix_checkpoint = prefix_len;
                let mut new_prefix_text = self.prefix_text.clone();
                let mut new_prefix_separator = self.prefix_needs_separator;
                if consumed == full_text.len() - prefix_len && checkpoint_offset > prefix_len {
                    // The checkpoint covers the whole tail: re-render the
                    // checkpoint prefix once so the next batch has a bigger
                    // stable base. This is a single O(checkpoint) render that
                    // replaces many O(checkpoint) renders.
                    let new_prefix = trim_trailing_blank_lines(render_markdown_with_width(
                        &full_text[..checkpoint_offset],
                        self.max_width,
                    ));
                    new_prefix_lines = std::sync::Arc::new(new_prefix);
                    new_prefix_checkpoint = checkpoint_offset;
                    new_prefix_text = full_text[..checkpoint_offset].to_string();
                    new_prefix_separator = checkpoint.map(|cp| cp.needs_separator).unwrap_or(false);
                } else {
                    new_prefix_text.truncate(new_prefix_text.len().min(prefix_len));
                }
                (
                    combined,
                    Some(new_prefix_lines),
                    new_prefix_checkpoint,
                    new_prefix_text,
                    new_prefix_separator,
                )
            } else {
                // Full render: no usable cached prefix.
                let lines = render_markdown_with_width(full_text, self.max_width);
                // Cache the prefix for the next batch. The prefix render is a
                // second pass over the prefix; it is amortised across every
                // subsequent batch, so it pays for itself immediately.
                let mut new_prefix_lines = None;
                let mut new_prefix_checkpoint = 0;
                let mut new_prefix_text = String::new();
                let mut new_prefix_separator = false;
                if checkpoint_offset > 0 && supports_incremental_splice(full_text) {
                    let prefix = trim_trailing_blank_lines(render_markdown_with_width(
                        &full_text[..checkpoint_offset],
                        self.max_width,
                    ));
                    new_prefix_checkpoint = checkpoint_offset;
                    new_prefix_text = full_text[..checkpoint_offset].to_string();
                    new_prefix_lines = Some(std::sync::Arc::new(prefix));
                    new_prefix_separator = checkpoint.map(|cp| cp.needs_separator).unwrap_or(false);
                }
                (
                    lines,
                    new_prefix_lines,
                    new_prefix_checkpoint,
                    new_prefix_text,
                    new_prefix_separator,
                )
            };

        self.rendered_mermaid_pending = lines.iter().any(line_is_mermaid_pending_placeholder);
        self.rendered_mermaid_epoch = mermaid_epoch_before;

        // Reuse the text buffer capacity across streaming deltas.
        self.rendered_text.clear();
        self.rendered_text.push_str(full_text);
        // The freshly rendered lines are handed straight to the `Arc`. There is
        // deliberately no second `Vec` mirror: the renderer is the only owner,
        // so this is a move, and the previous contents of `rendered_shared` are
        // dropped here rather than being deep-copied on every frame.
        self.rendered_shared = std::sync::Arc::new(lines);

        self.prefix_lines = new_prefix_lines;
        self.prefix_checkpoint = new_prefix_checkpoint;
        self.prefix_text = new_prefix_text;
        self.prefix_needs_separator = new_prefix_separator;

        // Find checkpoint for next incremental update
        self.refresh_checkpoint(full_text, true);

        std::sync::Arc::clone(&self.rendered_shared)
    }

    /// Find the last complete block in text
    #[cfg(test)]
    pub(crate) fn find_last_complete_block(&self, text: &str) -> Option<usize> {
        self.find_last_complete_block_checkpoint(text)
            .map(|checkpoint| checkpoint.offset)
    }

    fn find_last_complete_block_checkpoint(&self, text: &str) -> Option<CompleteBlockCheckpoint> {
        let mut checkpoint = None;
        let mut line_start = 0usize;
        let mut fence_state: Option<(char, usize)> = None;
        let mut display_math_open = false;
        let mut last_nonblank_kind: Option<MarkdownBlockKind> = None;
        let spacing_mode = effective_markdown_spacing_mode();

        while line_start <= text.len() {
            let relative_end = text[line_start..].find('\n');
            let (line_end, line_ends_with_newline) = match relative_end {
                Some(end) => (line_start + end, true),
                None => (text.len(), false),
            };
            let line = &text[line_start..line_end];
            let line_end_including_newline = if line_ends_with_newline {
                line_end + 1
            } else {
                line_end
            };

            match fence_state {
                Some((fence_char, fence_len)) => {
                    if is_closing_fence(line, fence_char, fence_len) {
                        fence_state = None;
                        last_nonblank_kind = Some(MarkdownBlockKind::CodeBlock);
                        checkpoint = Some(CompleteBlockCheckpoint {
                            offset: line_end_including_newline,
                            needs_separator: spacing_separates_after(
                                MarkdownBlockKind::CodeBlock,
                                spacing_mode,
                            ),
                        });
                    }
                }
                None => {
                    if display_math_open {
                        let dd_count = count_unescaped_double_dollar(line);
                        if dd_count % 2 == 1 {
                            display_math_open = false;
                            last_nonblank_kind = Some(MarkdownBlockKind::DisplayMath);
                            checkpoint = Some(CompleteBlockCheckpoint {
                                offset: line_end_including_newline,
                                needs_separator: spacing_separates_after(
                                    MarkdownBlockKind::DisplayMath,
                                    spacing_mode,
                                ),
                            });
                        }
                    } else if let Some((fence_char, fence_len)) = parse_opening_fence(line) {
                        fence_state = Some((fence_char, fence_len));
                    } else {
                        let dd_count = count_unescaped_double_dollar(line);
                        if dd_count > 0 {
                            if dd_count % 2 == 1 {
                                display_math_open = true;
                            } else {
                                last_nonblank_kind = Some(MarkdownBlockKind::DisplayMath);
                                checkpoint = Some(CompleteBlockCheckpoint {
                                    offset: line_end_including_newline,
                                    needs_separator: spacing_separates_after(
                                        MarkdownBlockKind::DisplayMath,
                                        spacing_mode,
                                    ),
                                });
                            }
                        } else if line_ends_with_newline && is_heading_line(line.trim_start()) {
                            last_nonblank_kind = Some(MarkdownBlockKind::Heading);
                            checkpoint = Some(CompleteBlockCheckpoint {
                                offset: line_end_including_newline,
                                needs_separator: spacing_separates_after(
                                    MarkdownBlockKind::Heading,
                                    spacing_mode,
                                ),
                            });
                        } else if line.trim().is_empty() {
                            checkpoint = Some(CompleteBlockCheckpoint {
                                offset: line_end_including_newline,
                                needs_separator: last_nonblank_kind
                                    .map(|kind| spacing_separates_after(kind, spacing_mode))
                                    .unwrap_or(false),
                            });
                        } else {
                            last_nonblank_kind = Some(infer_markdown_line_kind(line));
                        }
                    }
                }
            }

            if !line_ends_with_newline {
                break;
            }
            line_start = line_end + 1;
        }

        checkpoint
    }

    /// Refresh checkpoint metadata from the latest rendered text.
    ///
    /// `force = true` recomputes prefix line counts even when checkpoint byte position is unchanged.
    fn refresh_checkpoint(&mut self, full_text: &str, force: bool) {
        let checkpoint = self.find_last_complete_block_checkpoint(full_text);
        let new_checkpoint = checkpoint.map(|cp| cp.offset).unwrap_or(0);
        let new_checkpoint_needs_separator =
            checkpoint.map(|cp| cp.needs_separator).unwrap_or(false);
        if !force
            && new_checkpoint == self.last_checkpoint
            && new_checkpoint_needs_separator == self.checkpoint_needs_separator
        {
            return;
        }

        self.last_checkpoint = new_checkpoint;
        self.checkpoint_needs_separator = new_checkpoint_needs_separator;
        if new_checkpoint == 0 {
            self.lines_at_checkpoint = 0;
        } else {
            // PERF: previously this did a second full markdown render of the
            // prefix just to count lines for debug telemetry → 2x parse cost
            // per streaming delta (O(n²) on long answers). The checkpoint line
            // count is only consumed by debug_memory_profile, so approximate
            // with a cheap newline scan instead of a full render.
            // SAFETY: byte index is always on a line boundary (produced by
            // find_last_complete_block_checkpoint), so slicing is safe.
            let prefix = full_text.get(..new_checkpoint).unwrap_or("");
            self.lines_at_checkpoint = prefix.matches('\n').count() + 1;
        }
    }

    /// Reset the renderer state
    pub fn reset(&mut self) {
        self.rendered_shared = std::sync::Arc::new(Vec::new());
        self.rendered_text.clear();
        self.last_checkpoint = 0;
        self.lines_at_checkpoint = 0;
        self.checkpoint_needs_separator = false;
        self.prefix_needs_separator = false;
        self.rendered_mermaid_pending = false;
        self.rendered_mermaid_epoch = crate::alphacode_tui_mermaid::deferred_render_epoch();
    }

    /// Update width constraint, resets if changed
    pub fn set_width(&mut self, max_width: Option<usize>) {
        if self.max_width != max_width {
            self.max_width = max_width;
            self.reset();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompleteBlockCheckpoint {
    offset: usize,
    needs_separator: bool,
}

/// Drop the trailing blank lines of a rendered block.
/// Drop the trailing blank lines of a rendered block.
///
/// A prefix render is a *standalone* document, so it decides its own block
/// separators: rendering `"# Heading\n\n"` on its own leaves a trailing blank
/// line, while the same text as the head of a longer document does not (a
/// heading sits tight against the paragraph that follows it).
///
/// The cached prefix therefore must not carry that separator. Keeping it would
/// make the spliced output differ from a full render of the same text by one
/// blank line — the incremental stream would show a paragraph pushed down by a
/// line that the full render does not have. The separator is restored by the
/// splice instead, from the checkpoint's own `needs_separator` flag.
fn trim_trailing_blank_lines(mut lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    while lines.last().is_some_and(|line| {
        line.spans.is_empty() || line.spans.iter().all(|span| span.content.is_empty())
    }) {
        lines.pop();
    }
    lines
}

/// Whether the text can be rendered incrementally by splicing a cached prefix
/// onto a freshly rendered tail.
///
/// The renderer is not a pure function of a block of text:
///
/// - it applies *document-wide* post-processing — structured blocks
///   (blockquotes, tables, code fences) are centered inside the width, and the
///   padding depends on how wide the widest line of the whole render is;
/// - it carries *block state* across the document — list continuity, fence
///   depth, blockquote depth, and the indentation of a continued list.
///
/// A prefix rendered on its own can therefore differ from the same text
/// rendered as the head of the full document: a blockquote in the prefix gets
/// centered against the prefix's own width instead of the document's, and a
/// list that starts in the prefix and continues in the tail loses its
/// indentation. Splicing such a prefix produces output that does not match a
/// full render of the same text, which shows up as misaligned or duplicated
/// content while the response streams.
///
/// Prose, headings, and inline formatting have no such context dependence, so
/// the common case keeps the incremental path. Everything else falls back to a
/// full render, which is always correct.
fn supports_incremental_splice(text: &str) -> bool {
    !text.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with('>')
            || trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("+ ")
            || trimmed.starts_with("```")
            || trimmed.starts_with("~~~")
            || trimmed.starts_with("$$")
            || trimmed.starts_with('|')
            || is_thematic_break_line(trimmed)
            || is_ordered_list_marker(trimmed)
    })
}

/// Whether the line starts an ordered-list item (`1.`, `12)`, ...).
fn is_ordered_list_marker(line: &str) -> bool {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && line[digits..].starts_with(['.', ')'])
}

pub(crate) fn is_heading_line(line: &str) -> bool {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    hashes > 0 && hashes <= 6 && line.chars().nth(hashes) == Some(' ')
}

pub(crate) fn is_thematic_break_line(line: &str) -> bool {
    let trimmed = line.trim();
    let mut marker: Option<char> = None;
    let mut count = 0usize;

    for ch in trimmed.chars() {
        if ch == ' ' || ch == '\t' {
            continue;
        }
        match marker {
            None if matches!(ch, '-' | '*' | '_') => {
                marker = Some(ch);
                count = 1;
            }
            Some(existing) if ch == existing => count += 1,
            _ => return false,
        }
    }

    count >= 3
}

pub(crate) fn looks_like_ordered_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    let digit_count = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    digit_count > 0
        && matches!(trimmed.chars().nth(digit_count), Some('.' | ')'))
        && matches!(trimmed.chars().nth(digit_count + 1), Some(' ' | '\t'))
}

pub(crate) fn infer_markdown_line_kind(line: &str) -> MarkdownBlockKind {
    let trimmed = line.trim_start();
    if is_heading_line(trimmed) {
        MarkdownBlockKind::Heading
    } else if is_thematic_break_line(trimmed) {
        MarkdownBlockKind::Rule
    } else if trimmed.starts_with('>') {
        MarkdownBlockKind::BlockQuote
    } else if trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("+ ")
        || looks_like_ordered_list_item(trimmed)
    {
        MarkdownBlockKind::List
    } else if trimmed.starts_with('<') {
        MarkdownBlockKind::HtmlBlock
    } else {
        MarkdownBlockKind::Paragraph
    }
}
