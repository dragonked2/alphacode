use crate::alphacode_render_core::{ArabicShapingMode, with_arabic_shaping_mode};

#[test]
fn wraps_arabic_line_with_proper_shaping_and_bidi() {
    with_arabic_shaping_mode(ArabicShapingMode::PresentationForms, || {
        let line = Line::from("مرحبا بك في الفحص");
        let wrapped = wrap_line(line, 40);
        assert_eq!(wrapped.len(), 1);
        let rendered_text = line_plain_text(&wrapped[0]);

        // Shaped characters should be present (Presentation Forms-B)
        assert!(rendered_text.contains('\u{FEE3}')); // Meem initial
        assert!(rendered_text.contains('\u{FE8E}')); // Alif final
    });
}

#[test]
fn modern_terminal_preserves_standard_arabic_unicode() {
    // High-Priority Fix 1: Windows Terminal (DirectWrite) natively shapes standard Arabic Unicode.
    // In StandardUnicode mode, characters must remain in U+0600..U+06FF without converting
    // to Presentation Forms-B (which monospace fonts render as isolated fixed-width cells with gaps).
    with_arabic_shaping_mode(ArabicShapingMode::StandardUnicode, || {
        let line = Line::from("مرحبا بك في الفحص");
        let wrapped = wrap_line(line, 40);
        assert_eq!(wrapped.len(), 1);
        let rendered_text = line_plain_text(&wrapped[0]);

        // Standard Arabic characters must be preserved
        assert!(rendered_text.contains("مرحبا"));
        assert!(rendered_text.contains("الفحص"));

        // Presentation Forms-B glyphs must NOT be present (prevents 'ك ل م ن ي' cell-gap detachment)
        assert!(!rendered_text.contains('\u{FEE3}')); // No Meem initial form
        assert!(!rendered_text.contains('\u{FE8E}')); // No Alif final form
    });
}

#[test]
fn modern_terminal_normalizes_pasted_presentation_forms() {
    // If a user or data stream provides compatibility Presentation Forms-B,
    // they must be normalized back to standard Arabic in modern terminal mode.
    with_arabic_shaping_mode(ArabicShapingMode::StandardUnicode, || {
        let line = Line::from("\u{FEE3}\u{FEAE}\u{FEA3}\u{FE92}\u{FE8E}"); // ﻣﺮﺣﺒﺎ in Forms-B
        let wrapped = wrap_line(line, 40);
        assert_eq!(wrapped.len(), 1);
        let rendered_text = line_plain_text(&wrapped[0]);

        assert_eq!(rendered_text, "مرحبا");
        assert!(!rendered_text.contains('\u{FEE3}'));
    });
}

#[test]
fn mixed_english_arabic_question_dominant_direction_preserves_question_mark() {
    // High-Priority Fix 2: Mixed English-Arabic question must NOT flip '?' to the start of the line.
    // When English is the dominant script, paragraph base direction must be LTR, keeping '?' attached to the question.
    let line_str = "Alphacode, can you explain? أنا أتساءل";

    // 1. In Modern Terminal (StandardUnicode) mode:
    with_arabic_shaping_mode(ArabicShapingMode::StandardUnicode, || {
        let line = Line::from(line_str);
        let wrapped = wrap_line(line, 60);
        assert_eq!(wrapped.len(), 1);
        let rendered = line_plain_text(&wrapped[0]);

        assert!(
            rendered.contains("Alphacode, can you explain?"),
            "Question with '?' must stay intact: got '{}'",
            rendered
        );
        assert!(
            !rendered.starts_with('?'),
            "Question mark must NOT flip to the beginning of the line: got '{}'",
            rendered
        );
        assert!(rendered.contains("أنا أتساءل"));
    });

    // 2. In Legacy (PresentationForms) mode:
    with_arabic_shaping_mode(ArabicShapingMode::PresentationForms, || {
        let line = Line::from(line_str);
        let wrapped = wrap_line(line, 60);
        assert_eq!(wrapped.len(), 1);
        let rendered = line_plain_text(&wrapped[0]);

        assert!(
            rendered.contains("Alphacode, can you explain?"),
            "Question with '?' must stay intact in presentation mode: got '{}'",
            rendered
        );
        assert!(
            !rendered.starts_with('?'),
            "Question mark must NOT flip to the beginning in presentation mode: got '{}'",
            rendered
        );
    });
}

#[test]
fn mixed_arabic_english_question_keeps_arabic_question_mark() {
    // Line with dominant Arabic script: paragraph base direction must be RTL.
    let line_str = "هل يمكنك شرح كود Alphacode في هذا الملف؟";

    with_arabic_shaping_mode(ArabicShapingMode::StandardUnicode, || {
        let line = Line::from(line_str);
        let wrapped = wrap_line(line, 60);
        assert_eq!(wrapped.len(), 1);
        let rendered = line_plain_text(&wrapped[0]);

        assert!(rendered.contains("هل يمكنك شرح كود"));
        assert!(rendered.contains("Alphacode"));
        assert!(rendered.contains('؟'));
    });

    with_arabic_shaping_mode(ArabicShapingMode::PresentationForms, || {
        let line = Line::from(line_str);
        let wrapped = wrap_line(line, 60);
        assert_eq!(wrapped.len(), 1);
        let rendered = line_plain_text(&wrapped[0]);

        assert!(rendered.contains("Alphacode"));
        assert!(rendered.contains('؟'));
    });
}

#[test]
fn wraps_mixed_arabic_and_english() {
    let line = Line::from("الملف src/main.rs تم فحصه");
    let wrapped = wrap_line(line, 50);
    assert_eq!(wrapped.len(), 1);
    let rendered_text = line_plain_text(&wrapped[0]);

    // English path should remain intact and LTR
    assert!(rendered_text.contains("src/main.rs"));
}

#[test]
fn wraps_multiline_arabic_paragraph() {
    let long_arabic = "هذا نص تجريبي طويل جدا باللغة العربية لاختبار كيفية تقسيم الأسطر والتفاف الكلمات في الواجهة التفاعلية";
    let line = Line::from(long_arabic);
    let wrapped = wrap_line(line, 30);

    // Should wrap into multiple lines
    assert!(wrapped.len() > 1);
    for row in &wrapped {
        let text = line_plain_text(row);
        assert!(!text.is_empty());
    }
}

#[test]
fn preserves_styles_across_arabic_wrapping() {
    let line = Line::from(vec![
        Span::styled("تنبيه: ", Style::default().fg(Color::Red)),
        Span::styled("العملية تمت بنجاح", Style::default().fg(Color::Green)),
    ]);
    let wrapped = wrap_line(line, 40);
    assert_eq!(wrapped.len(), 1);
    assert!(
        wrapped[0]
            .spans
            .iter()
            .any(|s| s.style.fg == Some(Color::Red))
    );
    assert!(
        wrapped[0]
            .spans
            .iter()
            .any(|s| s.style.fg == Some(Color::Green))
    );
}

#[test]
fn handles_lam_alif_ligatures_and_variants() {
    with_arabic_shaping_mode(ArabicShapingMode::PresentationForms, || {
        // Isolated Lam-Alif
        let line_la = Line::from("لا إله إلا الله");
        let wrapped_la = wrap_line(line_la, 40);
        let text_la = line_plain_text(&wrapped_la[0]);
        assert!(text_la.contains('\u{FEFB}') || text_la.contains('\u{FEFC}'));

        // Final / connected Lam-Alif in "سلام"
        let line_salam = Line::from("سلام عليكم");
        let wrapped_salam = wrap_line(line_salam, 30);
        let text_salam = line_plain_text(&wrapped_salam[0]);
        assert!(text_salam.contains('\u{FEFC}')); // Final Lam-Alif

        // Lam-Alif with Madda in "الآن"
        let line_alan = Line::from("الآن يتم البدء");
        let wrapped_alan = wrap_line(line_alan, 30);
        let text_alan = line_plain_text(&wrapped_alan[0]);
        assert!(text_alan.contains('\u{FEF6}') || text_alan.contains('\u{FEF5}'));

        // Lam-Alif with Hamza above in "لأنه"
        let line_li_annahu = Line::from("لأنه يعمل جيدا");
        let wrapped_ha = wrap_line(line_li_annahu, 30);
        let text_ha = line_plain_text(&wrapped_ha[0]);
        assert!(text_ha.contains('\u{FEF7}') || text_ha.contains('\u{FEF8}'));

        // Lam-Alif with Hamza below in "للإصدار"
        let line_isdar = Line::from("للإصدار الجديد");
        let wrapped_hb = wrap_line(line_isdar, 30);
        let text_hb = line_plain_text(&wrapped_hb[0]);
        assert!(text_hb.contains('\u{FEF9}') || text_hb.contains('\u{FEFA}'));
    });
}

#[test]
fn handles_lam_alif_standard_unicode() {
    with_arabic_shaping_mode(ArabicShapingMode::StandardUnicode, || {
        let line = Line::from("لا إله إلا الله");
        let wrapped = wrap_line(line, 40);
        let text = line_plain_text(&wrapped[0]);
        // In standard Unicode mode, base Lam (\u{0644}) and Alif (\u{0627}) remain intact
        assert!(text.contains("لا"));
        assert!(!text.contains('\u{FEFB}'));
    });
}

#[test]
fn handles_diacritics_and_tashkeel_cleanly() {
    with_arabic_shaping_mode(ArabicShapingMode::PresentationForms, || {
        // "مَرْحَبًا" with tashkeel (Sukun, Fathatan)
        let line = Line::from("مَرْحَبًا بك في ألفاكود");
        let wrapped = wrap_line(line, 40);
        assert_eq!(wrapped.len(), 1);
        let rendered = line_plain_text(&wrapped[0]);

        // Tashkeel must be present in the output
        assert!(rendered.contains('\u{0652}')); // Sukun
        assert!(rendered.contains('\u{064B}')); // Fathatan

        // And base shaped glyphs must be present
        assert!(rendered.contains('\u{FEE3}')); // Meem initial
        assert!(rendered.contains('\u{FE8E}')); // Alif final
    });
}

#[test]
fn handles_numbers_in_arabic_context() {
    // European numbers should remain left-to-right (2026, 3.14)
    let line = Line::from("الإصدار 3.14 صدر في عام 2026 بنجاح");
    let wrapped = wrap_line(line, 50);
    assert_eq!(wrapped.len(), 1);
    let rendered = line_plain_text(&wrapped[0]);

    assert!(rendered.contains("3.14"));
    assert!(rendered.contains("2026"));

    // Arabic-Indic digits should also stay left-to-right (٢٠٢٦)
    let line_indic = Line::from("في عام ٢٠٢٦ م");
    let wrapped_indic = wrap_line(line_indic, 30);
    assert_eq!(wrapped_indic.len(), 1);
    let rendered_indic = line_plain_text(&wrapped_indic[0]);
    assert!(rendered_indic.contains("٢٠٢٦"));
}

#[test]
fn handles_brackets_and_punctuation_in_bidi() {
    // Bracket mirroring in Arabic RTL text
    let line = Line::from("(تنبيه مهم)");
    let wrapped = wrap_line(line, 30);
    let rendered = line_plain_text(&wrapped[0]);
    // The closing parenthesis should mirror to match visual orientation
    assert!(rendered.starts_with('(') || rendered.ends_with(')'));

    // English identifier inside brackets in Arabic sentence
    let line_code = Line::from("استخدم الدالة (process_event) للبدء");
    let wrapped_code = wrap_line(line_code, 50);
    let rendered_code = line_plain_text(&wrapped_code[0]);
    // English identifier inside parentheses must stay intact
    assert!(rendered_code.contains("process_event"));

    // Mixed punctuation: colon, comma, exclamation
    let line_punct = Line::from("النتيجة: تم الفحص، لا توجد أخطاء!");
    let wrapped_punct = wrap_line(line_punct, 50);
    let rendered_punct = line_plain_text(&wrapped_punct[0]);
    assert!(rendered_punct.contains(':') || rendered_punct.contains('!'));
}

#[test]
fn wraps_multiline_mixed_arabic_english_paragraph() {
    let mixed_long = "هذا المشروع يستخدم لغة البرمجة Rust ومكتبات متعددة مثل tokio و ratatui لتوفير واجهة تفاعلية CLI/TUI تدعم اللغة العربية والإنجليزية معا بشكل ممتاز.";
    let line = Line::from(mixed_long);
    let wrapped = wrap_line(line, 40);

    // Should wrap into multiple lines cleanly
    assert!(wrapped.len() >= 3);

    // Verify all English tokens remain completely intact across wrapped rows
    let full_rendered: String = wrapped
        .iter()
        .map(line_plain_text)
        .collect::<Vec<_>>()
        .join(" ");
    assert!(full_rendered.contains("Rust"));
    assert!(full_rendered.contains("tokio"));
    assert!(full_rendered.contains("ratatui"));
    assert!(full_rendered.contains("CLI/TUI"));

    // Every wrapped line must have valid content
    for row in &wrapped {
        assert!(!line_plain_text(row).trim().is_empty());
    }
}

#[test]
fn english_sentence_with_embedded_arabic_preserves_ltr() {
    let line = Line::from("The status of الملف src/main.rs is ready for review.");
    let wrapped = wrap_line(line, 60);
    assert_eq!(wrapped.len(), 1);
    let rendered = line_plain_text(&wrapped[0]);

    assert!(rendered.contains("The status of"));
    assert!(rendered.contains("src/main.rs"));
    assert!(rendered.contains("ready for review."));
}

#[test]
fn cross_platform_terminal_detection_covers_windows_macos_linux_and_legacy() {
    use crate::alphacode_render_core::bidi::{TerminalShapingEnv, detect_native_shaping_from};

    // Windows modern
    assert!(detect_native_shaping_from(&TerminalShapingEnv {
        wt_session: Some("wt-1"),
        ..Default::default()
    }));
    assert!(detect_native_shaping_from(&TerminalShapingEnv {
        conemu_pid: Some("100"),
        ..Default::default()
    }));
    // Raw conhost.exe legacy fallback
    assert!(!detect_native_shaping_from(&TerminalShapingEnv::default()));

    // macOS modern
    for prog in [
        "iTerm.app",
        "Apple_Terminal",
        "WarpTerminal",
        "Ghostty",
        "WezTerm",
    ] {
        assert!(detect_native_shaping_from(&TerminalShapingEnv {
            term_program: Some(prog),
            ..Default::default()
        }));
    }

    // Linux modern + SSH + tmux
    assert!(detect_native_shaping_from(&TerminalShapingEnv {
        vte_version: Some("7400"),
        ..Default::default()
    }));
    assert!(detect_native_shaping_from(&TerminalShapingEnv {
        konsole_version: Some("230800"),
        ..Default::default()
    }));
    for term in ["xterm-kitty", "foot", "alacritty", "xterm-ghostty"] {
        assert!(detect_native_shaping_from(&TerminalShapingEnv {
            term: Some(term),
            ..Default::default()
        }));
    }
    assert!(detect_native_shaping_from(&TerminalShapingEnv {
        term: Some("tmux-256color"),
        colorterm: Some("truecolor"),
        ..Default::default()
    }));

    // Dumb / legacy terminals
    for term in ["linux", "vt100", "dumb"] {
        assert!(!detect_native_shaping_from(&TerminalShapingEnv {
            term: Some(term),
            ..Default::default()
        }));
    }
}

#[test]
fn renders_arabic_markdown_tables_lists_blockquotes_and_headings_cleanly() {
    let md = "\
# عنوان رئيسي

- العنصر الأول مع `foo()`
- العنصر الثاني في `/etc/hosts`

1. الخطوة الأولى
2. الخطوة الثانية

> اقتباس عربي مهم

| الملف | الحالة |
| --- | --- |
| سلام | جاهز |
| `src/bidi.rs` | مكتمل |
";

    for mode in [
        ArabicShapingMode::StandardUnicode,
        ArabicShapingMode::PresentationForms,
    ] {
        with_arabic_shaping_mode(mode, || {
            let legacy_lines = wrap_lines(render_markdown_with_width(md, Some(60)), 60);
            let core_lines = render_markdown_via_core_wrapped(md, 60);

            for (label, rendered_lines) in [("legacy", legacy_lines), ("core", core_lines)] {
                let text_lines: Vec<String> = rendered_lines.iter().map(line_plain_text).collect();
                let joined = text_lines.join("\n");

                // Inline code and paths stay uncorrupted
                assert!(
                    joined.contains("foo()"),
                    "{label} ({mode:?}) corrupted foo(): {joined}"
                );
                assert!(
                    joined.contains("/etc/hosts"),
                    "{label} ({mode:?}) corrupted /etc/hosts: {joined}"
                );
                assert!(
                    joined.contains("src/bidi.rs"),
                    "{label} ({mode:?}) corrupted src/bidi.rs: {joined}"
                );

                // Blockquote gutter and list markers stay left-pinned (allowing optional centering pad)
                assert!(
                    text_lines.iter().any(|l| l.trim_start().starts_with("│ ")),
                    "{label} ({mode:?}) lost blockquote left gutter: {joined}"
                );
                assert!(
                    text_lines.iter().any(|l| l.trim_start().starts_with("• ")),
                    "{label} ({mode:?}) lost bullet prefix: {joined}"
                );
                assert!(
                    text_lines.iter().any(|l| l.trim_start().starts_with("1. ")),
                    "{label} ({mode:?}) lost ordered prefix: {joined}"
                );

                // Table column borders (`│` / `┼`) align vertically across all rows
                let table_sep_col = text_lines
                    .iter()
                    .find(|l| l.contains("─┼─"))
                    .and_then(|l| l.chars().position(|c| c == '┼'))
                    .expect("table separator row");
                for row in text_lines
                    .iter()
                    .filter(|l| !l.trim_start().starts_with("│ ") && l.contains(" │ "))
                {
                    let bar_col = row.chars().position(|c| c == '│').expect("table bar");
                    assert_eq!(
                        bar_col, table_sep_col,
                        "{label} ({mode:?}) misaligned table border in {row:?} vs {table_sep_col}"
                    );
                }
            }
        });
    }
}

#[test]
fn handles_lam_alif_with_tashkeel_and_urls_in_markdown_wrapping() {
    for mode in [
        ArabicShapingMode::StandardUnicode,
        ArabicShapingMode::PresentationForms,
    ] {
        with_arabic_shaping_mode(mode, || {
            let line = Line::from("لَا تفتح https://example.com/docs/ar/ الآن في عام ٢٠٢٦");
            let wrapped = wrap_line(line, 60);
            let rendered = line_plain_text(&wrapped[0]);
            assert!(
                rendered.contains("https://example.com/docs/ar/"),
                "{rendered:?}"
            );
            assert!(rendered.contains("٢٠٢٦"), "{rendered:?}");
            if mode == ArabicShapingMode::PresentationForms {
                assert!(
                    rendered.contains('\u{FEFB}'),
                    "expected Lam-Alif in {rendered:?}"
                );
                assert!(
                    rendered.contains('\u{064E}'),
                    "expected Fatha in {rendered:?}"
                );
            } else {
                assert!(
                    rendered.contains("لَا"),
                    "expected standard Arabic in {rendered:?}"
                );
            }
        });
    }
}

#[test]
fn preserves_user_prompt_prefix_and_unmatched_parens_around_ltr_tokens() {
    with_arabic_shaping_mode(ArabicShapingMode::PresentationForms, || {
        // User prompt prefix `1› ` must stay pinned on the left rather than mirroring to `‹1` on the right.
        let prompt = Line::from(vec![
            Span::styled("1", Style::default().fg(Color::Red)),
            Span::styled("› ", Style::default().fg(Color::Yellow)),
            Span::raw("أهلاً يا أحمد عامل إيه؟"),
        ]);
        let wrapped_prompt = wrap_line(prompt, 50);
        assert_eq!(wrapped_prompt.len(), 1);
        let prompt_rendered = line_plain_text(&wrapped_prompt[0]);
        assert!(
            prompt_rendered.starts_with("1› "),
            "expected prompt prefix pinned on left: got {prompt_rendered:?}"
        );
        assert!(
            !prompt_rendered.contains("‹1"),
            "prompt prefix must not mirror to ‹1: got {prompt_rendered:?}"
        );

        // Unmatched closing parenthesis after a slash token inside Arabic parens must not trap `)`.
        let mixed = Line::from("شغال معك على كود نفسه (نسخة الـ TUI/CLI). جاهز لأي حاجة");
        let wrapped_mixed = wrap_line(mixed, 80);
        assert_eq!(wrapped_mixed.len(), 1);
        let mixed_rendered = line_plain_text(&wrapped_mixed[0]);
        assert!(
            mixed_rendered.contains("TUI/CLI"),
            "expected intact LTR token: got {mixed_rendered:?}"
        );
        let open_idx = mixed_rendered.find('(').expect("expected '('");
        let close_idx = mixed_rendered.find(')').expect("expected ')'");
        assert!(
            open_idx < close_idx,
            "parentheses around '(نسخة الـ TUI/CLI)' must balance visually: got {mixed_rendered:?}"
        );

        // Multi-line wrapped paragraph must keep RTL run ordering on every continuation row.
        let paragraph = Line::from(
            "أنا AlphaCode، شغال معاك على كود الـ alphacode نفسه (نسخة الـ TUI/CLI). جاهز لأي حاجة — تعديل كود، بحث خطأ (bug)، مراجعة، بناء و reload، أو أي تحقيق أمني/reverse engineering انت محتاجه.",
        )
        .alignment(Alignment::Center);
        let wrapped_para = wrap_line(paragraph, 96);
        assert!(wrapped_para.len() >= 2, "expected wrapped lines");
        let row2 = line_plain_text(&wrapped_para[1]);
        assert!(
            row2.contains("reload") && row2.contains("reverse engineering"),
            "row2={row2:?}"
        );
        let reload_pos = row2.find("reload").unwrap();
        let rev_pos = row2.find("reverse engineering").unwrap();
        assert!(
            rev_pos < reload_pos,
            "RTL continuation line must put later logical LTR run (`reverse engineering`) to the LEFT of earlier run (`reload`): got {row2:?}"
        );
    });
}
