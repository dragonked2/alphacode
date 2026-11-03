pub(super) fn line_display_width(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(text)
}

pub(super) fn display_col_to_byte_offset(text: &str, display_col: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;

    let mut width = 0usize;
    for (idx, grapheme) in text.grapheme_indices(true) {
        if width >= display_col {
            return idx;
        }
        let next_width = width.saturating_add(unicode_width::UnicodeWidthStr::width(grapheme));
        if next_width > display_col {
            return idx;
        }
        width = next_width;
    }
    text.len()
}

pub(super) fn clamp_display_col(text: &str, display_col: usize) -> usize {
    display_col.min(line_display_width(text))
}

pub(super) fn display_col_slice(text: &str, start_col: usize, end_col: usize) -> &str {
    let (start_col, end_col) = (start_col.min(end_col), start_col.max(end_col));
    let start_byte = display_col_to_byte_offset(text, start_col);
    let end_byte = display_col_to_byte_offset(text, end_col);
    &text[start_byte..end_byte]
}

#[cfg(test)]
mod tests {
    use super::{
        clamp_display_col, display_col_slice, display_col_to_byte_offset, line_display_width,
    };

    #[test]
    fn reversed_selection_ranges_are_safe() {
        assert_eq!(display_col_slice("a🙂bc", 4, 1), "🙂b");
        assert_eq!(display_col_slice("abc", 99, 0), "abc");
    }

    #[test]
    fn line_display_width_counts_wide_chars() {
        assert_eq!(line_display_width("abc"), 3);
        assert_eq!(line_display_width("a🙂b"), 4);
    }

    #[test]
    fn display_col_to_byte_offset_stops_before_partial_wide_char() {
        let text = "a🙂b";

        assert_eq!(display_col_to_byte_offset(text, 0), 0);
        assert_eq!(display_col_to_byte_offset(text, 1), 1);
        assert_eq!(display_col_to_byte_offset(text, 2), 1);
        assert_eq!(display_col_to_byte_offset(text, 3), "a🙂".len());
        assert_eq!(display_col_to_byte_offset(text, 99), text.len());
    }

    #[test]
    fn clamp_display_col_caps_at_line_display_width() {
        assert_eq!(clamp_display_col("a🙂b", 99), 4);
        assert_eq!(clamp_display_col("a🙂b", 2), 2);
    }

    #[test]
    fn display_col_slice_respects_wide_char_boundaries() {
        let text = "a🙂bc";

        assert_eq!(display_col_slice(text, 0, 1), "a");
        assert_eq!(display_col_slice(text, 1, 3), "🙂");
        assert_eq!(display_col_slice(text, 2, 4), "🙂b");
        assert_eq!(display_col_slice(text, 3, 99), "bc");
    }

    #[test]
    fn display_col_slice_preserves_grapheme_clusters() {
        let text = "e\u{301}👩\u{200d}💻x";
        assert_eq!(display_col_slice(text, 0, 1), "e\u{301}");
        assert_eq!(display_col_slice(text, 1, 3), "👩\u{200d}💻");
        assert_eq!(display_col_slice(text, 2, 4), "👩\u{200d}💻x");
    }
}
