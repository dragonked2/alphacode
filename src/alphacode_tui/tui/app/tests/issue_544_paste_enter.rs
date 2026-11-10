// Issue #544: stray Enter after a bracketed paste must not submit.
#[test]
fn normal_questions_submit_without_becoming_pasted_content() {
    for question in [
        "How are you?",
        "Can you explain why this ordinary question should submit normally even when it is longer than sixty four bytes?",
    ] {
        for processing in [false, true] {
            let mut app = create_test_app();
            app.is_processing = processing;
            app.status = if processing {
                ProcessingStatus::Streaming
            } else {
                ProcessingStatus::Idle
            };
            input::paste_guard_expire_trailing_window_for_test();
            for c in question.chars() {
                app.handle_key(
                    KeyCode::Char(c),
                    if c == '?' {
                        KeyModifiers::SHIFT
                    } else {
                        KeyModifiers::NONE
                    },
                )
                .unwrap();
            }
            assert_eq!(app.input, question);
            app.handle_key(KeyCode::Enter, KeyModifiers::NONE).unwrap();
            assert!(app.input.is_empty());
            assert!(app.pasted_contents.is_empty());
            if processing {
                assert_eq!(app.interleave_message.as_deref(), Some(question));
            }
        }
    }
}

#[test]
fn control_v_terminal_delivery_collapses_like_alt_v() {
    use crate::alphacode_tui::bus::{
        ClipboardPasteCompleted, ClipboardPasteContent, ClipboardPasteKind,
    };

    for terminal_delivered in [false, true] {
        let mut app = create_test_app();
        let before = "Explain:  please";
        let cursor = "Explain: ".len();
        let text = "first\nsecond 👩‍💻";
        app.set_input_for_test(before);
        app.cursor_pos = cursor;
        let session_id = app.active_client_session_id().unwrap().to_string();
        app.clipboard_paste_draft = Some((session_id.clone(), before.to_string(), cursor));
        if terminal_delivered {
            app.input.insert_str(cursor, text);
            app.cursor_pos += text.len();
        }
        assert!(
            app.handle_clipboard_paste_completed(ClipboardPasteCompleted {
                session_id,
                kind: ClipboardPasteKind::Smart,
                content: ClipboardPasteContent::Text(text.to_string()),
            })
        );
        assert_eq!(app.input, "Explain: [pasted content] please");
        assert_eq!(app.pasted_contents, vec![text.to_string()]);
        assert_eq!(
            app.expand_paste_placeholders(&app.input.clone()),
            "Explain: first\nsecond 👩‍💻 please"
        );
    }
}

#[test]
fn pasting_matching_text_does_not_silently_discard_the_paste() {
    let mut app = create_test_app();
    app.set_input_for_test("hello");
    app.cursor_pos = app.input.len();
    app.handle_paste("hello".to_string());
    assert_eq!(app.input, "hello[pasted content]");
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "hellohello"
    );
}

#[test]
fn control_v_does_not_duplicate_a_terminal_paste_token() {
    use crate::alphacode_tui::bus::{
        ClipboardPasteCompleted, ClipboardPasteContent, ClipboardPasteKind,
    };
    let mut app = create_test_app();
    let session_id = app.active_client_session_id().unwrap().to_string();
    app.clipboard_paste_draft = Some((session_id.clone(), String::new(), 0));
    app.handle_paste("hello\nworld".to_string());
    app.handle_clipboard_paste_completed(ClipboardPasteCompleted {
        session_id,
        kind: ClipboardPasteKind::Smart,
        content: ClipboardPasteContent::Text("hello\nworld".to_string()),
    });
    assert_eq!(app.input, "[pasted content]");
    assert_eq!(app.pasted_contents.len(), 1);
}

#[test]
fn bare_enter_immediately_after_paste_does_not_submit() {
    // Windows Terminal / conhost sends a separate bare Enter key event after
    // a bracketed paste ending with \n; it must not submit the chat (#544).
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = create_test_app();
    crate::alphacode_tui::tui::app::input::handle_paste(&mut app, "hello world\n".to_string());
    assert_eq!(app.input, "[pasted content]");

    app.handle_key_press_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(
        !app.is_processing,
        "Enter right after paste must not submit"
    );
    assert!(
        !app.input.is_empty(),
        "input should be preserved after paste"
    );

    // A later, human-timed Enter still submits.  Expire only the timestamp so
    // this also catches a regression where `Event::Paste` leaves the
    // in-flight flag set forever.
    crate::alphacode_tui::tui::app::input::paste_guard_expire_trailing_window_for_test();
    app.handle_key_press_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(app.input.is_empty(), "later Enter should submit normally");
}

#[test]
fn typing_after_paste_allows_immediate_submit() {
    let mut app = create_test_app();
    app.handle_paste("line\n".repeat(5_000));
    app.handle_key(KeyCode::Char('x'), KeyModifiers::NONE)
        .unwrap();
    app.handle_key(KeyCode::Enter, KeyModifiers::NONE).unwrap();
    assert!(
        app.input.is_empty(),
        "typing after a paste must disarm the trailing Enter guard"
    );
}

// Keymap-independent multi-line input: a trailing backslash makes Enter insert
// a newline instead of submitting.
//
// Motivation: both newline chords (Shift+Enter, Option/Alt+Enter) require the
// terminal to disambiguate modified Enter via the kitty keyboard protocol.
// macOS Terminal.app and default-profile iTerm2 send a bare CR instead, so
// those users previously had no way to type a multi-line prompt.

#[test]
fn trailing_backslash_enter_inserts_newline_instead_of_submitting() {
    let mut app = create_test_app();
    app.set_input_for_test("first\\");
    app.cursor_pos = app.input.len();

    app.handle_key(KeyCode::Enter, KeyModifiers::empty())
        .expect("enter should be handled");

    assert_eq!(app.input, "first\n");
    assert_eq!(app.cursor_pos, app.input.len());
    assert!(
        app.display_messages().is_empty(),
        "the draft must not be submitted: {:?}",
        app.display_messages()
    );
}

#[test]
fn plain_enter_still_submits_without_a_trailing_backslash() {
    let mut app = create_test_app();
    app.set_input_for_test("first");
    app.cursor_pos = app.input.len();

    app.handle_key(KeyCode::Enter, KeyModifiers::empty())
        .expect("enter should be handled");

    assert!(app.input.is_empty(), "input should be consumed by submit");
}

#[test]
fn escaped_double_backslash_is_literal_text_and_still_submits() {
    let mut app = create_test_app();
    app.set_input_for_test("path\\\\");
    app.cursor_pos = app.input.len();

    app.handle_key(KeyCode::Enter, KeyModifiers::empty())
        .expect("enter should be handled");

    assert!(
        app.input.is_empty(),
        "an escaped backslash is literal, so Enter submits: {:?}",
        app.input
    );
}

#[test]
fn backslash_continuation_only_applies_at_the_end_of_the_draft() {
    let mut app = create_test_app();
    app.set_input_for_test("a\\b");
    app.cursor_pos = 2; // just after the backslash, not at end

    app.handle_key(KeyCode::Enter, KeyModifiers::empty())
        .expect("enter should be handled");

    assert!(
        app.input.is_empty(),
        "mid-draft backslash must not become a continuation: {:?}",
        app.input
    );
}

#[test]
fn repeated_continuations_build_a_multi_line_draft() {
    let mut app = create_test_app();
    for line in ["one\\", "two\\"] {
        for c in line.chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::empty())
                .expect("char should be handled");
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::empty())
            .expect("enter should be handled");
    }
    for c in "three".chars() {
        app.handle_key(KeyCode::Char(c), KeyModifiers::empty())
            .expect("char should be handled");
    }

    assert_eq!(app.input, "one\ntwo\nthree");
}

#[test]
fn bracketed_paste_is_one_draft_in_local_remote_and_streaming_modes() {
    use crate::alphacode_tui::tui::terminal_events::ImmediateEvents;
    use crossterm::event::{Event, KeyEvent};
    use futures::StreamExt;
    let text = "x\n".to_owned() + &"next line 世界 👩‍💻\n".repeat(200);
    for remote_mode in [false, true] {
        for processing in [false, true] {
            let mut app = create_test_app();
            app.is_remote = remote_mode;
            app.is_processing = processing;
            app.status = if processing {
                ProcessingStatus::Streaming
            } else {
                ProcessingStatus::Idle
            };
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let _entered = runtime.enter();
            let mut connection = crate::alphacode_tui::tui::backend::RemoteConnection::dummy();
            runtime.block_on(async {
                let events = vec![Ok::<_, std::io::Error>(Event::Paste(text.clone()))];
                let mut source = ImmediateEvents::from_source(futures::stream::iter(events));
                while let Some(event) = source.next().await {
                    match event.unwrap() {
                        Event::Paste(text) => app.handle_paste(text),
                        Event::Key(key) if remote_mode => {
                            app.handle_remote_key(key.code, key.modifiers, &mut connection)
                                .await
                                .unwrap();
                        }
                        Event::Key(key) => {
                            app.handle_key_press_event(key).unwrap();
                        }
                        _ => panic!("unexpected input event"),
                    }
                }
                assert_eq!(app.input, "[pasted content]");
                assert_eq!(app.pasted_contents, vec![text.clone()]);
                assert_eq!(app.expand_paste_placeholders(&app.input.clone()), text);
                assert!(app.interleave_message.is_none());
                assert!(app.pending_soft_interrupt_requests.is_empty());
                if remote_mode {
                    app.handle_remote_key(KeyCode::Enter, KeyModifiers::NONE, &mut connection)
                        .await
                        .unwrap();
                } else {
                    app.handle_key_press_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
                        .unwrap();
                }
                assert_eq!(
                    app.input, "[pasted content]",
                    "trailing paste Enter submitted the draft"
                );
            });
        }
    }
}

#[test]
fn control_v_clipboard_and_native_chunks_reconcile_in_either_order() {
    use crate::alphacode_tui::bus::{
        ClipboardPasteCompleted, ClipboardPasteContent, ClipboardPasteKind,
    };
    let chunks = ["first\n", "second 世界\n", "third\n"];
    let text = chunks.concat();
    for native_before in 0..=chunks.len() {
        let mut app = create_test_app();
        let id = app.active_client_session_id().unwrap().to_string();
        app.clipboard_paste_draft = Some((id.clone(), String::new(), 0));
        app.clipboard_paste_native_echo_expected = true;
        for chunk in &chunks[..native_before] {
            app.handle_paste((*chunk).into());
        }
        assert!(
            app.handle_clipboard_paste_completed(ClipboardPasteCompleted {
                session_id: id,
                kind: ClipboardPasteKind::Smart,
                content: ClipboardPasteContent::Text(text.clone()),
            })
        );
        for chunk in &chunks[native_before..] {
            app.handle_paste((*chunk).into());
        }
        assert_eq!(
            app.input, "[pasted content]",
            "delivery order {native_before}"
        );
        assert_eq!(app.pasted_contents, vec![text.clone()]);
        // A later intentional paste of identical text must still be accepted.
        app.handle_paste(text.clone());
        assert_eq!(app.pasted_contents.len(), 2);
    }
}

#[test]
fn pasted_placeholder_literals_are_never_expanded_recursively() {
    let mut app = create_test_app();
    let draft = "before [pasted content] between [pasted content] after";
    app.pasted_contents.insert(
        "before ".len(),
        "first [pasted content] literal world".into(),
    );
    app.pasted_contents.insert(
        "before [pasted content] between ".len(),
        "second [pasted content] literal 👩‍💻".into(),
    );
    assert_eq!(
        app.expand_paste_placeholders(draft),
        "before first [pasted content] literal world between second [pasted content] literal 👩‍💻 after"
    );
}

#[test]
fn regression_paste_insertion_order_follows_caret_not_payload_order() {
    let mut app = create_test_app();
    app.handle_paste("second block".into());
    app.cursor_pos = 0;
    app.handle_paste("first block".into());
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "first blocksecond block"
    );
    input::insert_input_text(&mut app, " between ");
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "first block between second block"
    );
}

#[test]
fn regression_deleting_each_paste_restores_only_the_surviving_payload() {
    for delete_first in [false, true] {
        let mut app = create_test_app();
        app.handle_paste("first block".into());
        app.handle_paste("second block".into());
        let start = if delete_first {
            0
        } else {
            input::PASTE_PLACEHOLDER.len()
        };
        app.input_selection_anchor = Some(start);
        app.cursor_pos = start + input::PASTE_PLACEHOLDER.len();
        assert!(app.delete_input_selection());
        assert_eq!(app.pasted_contents.len(), 1);
        let remaining = if delete_first {
            "second block"
        } else {
            "first block"
        };
        assert_eq!(app.expand_paste_placeholders(&app.input.clone()), remaining);
        app.undo_input_change();
        assert_eq!(
            app.expand_paste_placeholders(&app.input.clone()),
            "first blocksecond block"
        );
    }
}

#[test]
fn regression_replacing_a_paste_is_atomic_and_undo_restores_its_payload() {
    let mut app = create_test_app();
    app.handle_paste("original block".into());
    app.select_all_input();
    app.handle_paste("replacement block".into());
    assert_eq!(app.pasted_contents.len(), 1);
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "replacement block"
    );
    app.undo_input_change();
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "original block"
    );
    app.undo_input_change();
    assert!(app.input.is_empty());
    assert!(app.pasted_contents.is_empty());
}

#[test]
fn regression_literal_paste_tokens_do_not_steal_payloads() {
    let mut app = create_test_app();
    app.handle_paste("actual payload".into());
    input::insert_input_text(&mut app, " literal [pasted content]");
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "actual payload literal [pasted content]"
    );
    app.cursor_pos = 0;
    input::insert_input_text(&mut app, "[pasted content] literal ");
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "[pasted content] literal actual payload literal [pasted content]"
    );
}

#[test]
fn regression_paste_payloads_follow_stashed_drafts_and_not_other_prompts() {
    let mut app = create_test_app();
    app.handle_paste("stashed payload".into());
    app.toggle_input_stash();
    assert!(app.input.is_empty());
    assert!(app.pasted_contents.is_empty());
    app.handle_paste("current payload".into());
    app.toggle_input_stash();
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "stashed payload"
    );
    app.toggle_input_stash();
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "current payload"
    );
}

#[test]
fn regression_editing_inside_a_token_cannot_rebind_a_surviving_paste() {
    let mut app = create_test_app();
    app.handle_paste("first block".into());
    app.handle_paste("second block".into());
    app.cursor_pos = 1;
    input::insert_input_text(&mut app, "x");
    assert_eq!(app.pasted_contents.len(), 1);
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "[xpasted content]second block"
    );
    app.undo_input_change();
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "first blocksecond block"
    );
}

#[test]
fn regression_clear_and_cut_drop_live_payloads_but_undo_restores_them() {
    for cut in [false, true] {
        let mut app = create_test_app();
        app.handle_paste("original payload".into());
        if cut {
            assert!(input::cut_input_line_to_clipboard_with(&mut app, |text| {
                assert_eq!(text, "original payload");
                true
            }));
        } else {
            input::clear_input_for_escape(&mut app);
        }
        assert!(app.input.is_empty());
        assert!(app.pasted_contents.is_empty());
        app.undo_input_change();
        assert_eq!(
            app.expand_paste_placeholders(&app.input.clone()),
            "original payload"
        );
        input::clear_input_for_escape(&mut app);
        input::insert_input_text(&mut app, input::PASTE_PLACEHOLDER);
        assert_eq!(
            app.expand_paste_placeholders(&app.input.clone()),
            input::PASTE_PLACEHOLDER
        );
    }
}

#[test]
fn regression_history_replacement_detaches_pastes_and_cancel_restores_them() {
    let mut app = create_test_app();
    app.persisted_prompt_history = Some(vec!["historical prompt".into()]);
    app.handle_paste("draft payload".into());
    app.open_prompt_history_search();
    app.handle_prompt_history_search_key(KeyCode::Char('h'), KeyModifiers::NONE);
    assert_eq!(app.input, "historical prompt");
    assert!(app.pasted_contents.is_empty());
    app.handle_prompt_history_search_key(KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "draft payload"
    );

    assert!(input::handle_prompt_history_navigation(
        &mut app,
        KeyCode::Up,
        KeyModifiers::CONTROL
    ));
    assert_eq!(app.input, "historical prompt");
    assert!(app.pasted_contents.is_empty());
    app.undo_input_change();
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "draft payload"
    );
}

#[test]
fn regression_deferred_remote_input_restores_its_paste_payload() {
    let mut app = create_test_app();
    app.handle_paste("deferred payload".into());
    let prepared = input::take_prepared_input(&mut app);
    assert!(app.input.is_empty());
    assert!(app.pasted_contents.is_empty());
    remote::restore_prepared_remote_input(&mut app, prepared);
    assert_eq!(app.input, input::PASTE_PLACEHOLDER);
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "deferred payload"
    );
}

#[test]
fn regression_failed_turn_restores_payload_without_overwriting_a_new_draft() {
    for has_new_draft in [false, true] {
        let mut app = create_test_app();
        app.handle_paste("failed turn payload".into());
        app.submit_input();
        assert!(app.input.is_empty());
        if has_new_draft {
            app.handle_paste("new draft payload".into());
        }
        app.restore_failed_input_to_box();
        assert_eq!(app.input, input::PASTE_PLACEHOLDER);
        assert_eq!(
            app.expand_paste_placeholders(&app.input.clone()),
            if has_new_draft {
                "new draft payload"
            } else {
                "failed turn payload"
            }
        );
        assert!(app.last_submitted_input.is_none());
        assert!(app.last_submitted_pastes.is_empty());
    }
}

#[test]
fn regression_dictation_append_preserves_pastes_and_replace_is_undoable() {
    let mut app = create_test_app();
    app.handle_paste("pasted payload".into());
    remote::apply_transcript_event(
        &mut app,
        " appended".into(),
        crate::protocol::TranscriptMode::Append,
    );
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "pasted payload appended"
    );
    remote::apply_transcript_event(
        &mut app,
        "replacement".into(),
        crate::protocol::TranscriptMode::Replace,
    );
    assert!(app.pasted_contents.is_empty());
    app.undo_input_change();
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "pasted payload appended"
    );
}

#[test]
fn regression_copy_and_cut_selection_expand_only_complete_owned_tokens() {
    let mut app = create_test_app();
    input::insert_input_text(&mut app, "literal [pasted content] ");
    app.handle_paste("clipboard payload".into());
    app.select_all_input();
    assert!(app.cut_input_selection_to_clipboard_with(|text| {
        assert_eq!(text, "literal [pasted content] clipboard payload");
        true
    }));
    assert!(app.pasted_contents.is_empty());
    app.undo_input_change();
    app.input_selection_anchor = Some(app.input.len() - input::PASTE_PLACEHOLDER.len() + 1);
    app.cursor_pos = app.input.len();
    assert_eq!(
        app.selected_input_text().as_deref(),
        Some("pasted content]")
    );
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "literal [pasted content] clipboard payload"
    );
}

#[test]
fn regression_path_completion_rebases_later_paste_tokens() {
    let mut app = create_test_app();
    input::insert_input_text(&mut app, "please review @Cargo ");
    app.handle_paste("review payload".into());
    app.cursor_pos = "please review @Cargo".len();
    let suggestions = app.command_suggestions();
    app.command_suggestion_selected = suggestions
        .iter()
        .position(|(label, _)| label.ends_with("Cargo.toml"))
        .expect("Cargo.toml path suggestion");
    assert!(app.accept_selected_command_suggestion());
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "please review @Cargo.toml review payload"
    );
    app.undo_input_change();
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "please review @Cargo review payload"
    );
}

#[test]
fn regression_fallback_resend_distinguishes_drafts_with_identical_tokens() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    for new_draft in [false, true] {
        let mut app = create_test_app();
        app.is_remote = true;
        let mut remote = crate::alphacode_tui::tui::backend::RemoteConnection::dummy();
        remote.mark_history_loaded();
        app.handle_paste("failed payload".into());
        let prepared = input::take_prepared_input(&mut app);
        app.pending_fallback_resend = Some(crate::alphacode_tui::tui::app::FallbackResendPayload {
            content: prepared.expanded.clone(),
            images: Vec::new(),
            is_system: false,
            auto_retry: false,
            system_reminder: None,
            raw_input: Some(prepared.raw_input.clone()),
            pastes: prepared.pastes.clone(),
        });
        if new_draft {
            app.handle_paste("new draft payload".into());
        } else {
            remote::restore_prepared_remote_input(&mut app, prepared);
        }
        rt.block_on(remote::process_remote_followups(&mut app, &mut remote));
        assert!(app.pending_fallback_resend.is_none());
        assert_eq!(
            app.last_submitted_pastes
                .expand(app.last_submitted_input.as_deref().unwrap()),
            "failed payload"
        );
        assert_eq!(
            app.expand_paste_placeholders(&app.input.clone()),
            if new_draft { "new draft payload" } else { "" }
        );
    }
}

#[test]
fn regression_failed_fallback_switch_restores_its_owned_paste() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let mut app = create_test_app();
    app.is_remote = true;
    let mut remote = crate::alphacode_tui::tui::backend::RemoteConnection::dummy();
    remote.mark_history_loaded();
    app.handle_paste("fallback payload".into());
    let prepared = input::take_prepared_input(&mut app);
    app.pending_fallback_resend = Some(crate::alphacode_tui::tui::app::FallbackResendPayload {
        content: prepared.expanded,
        images: Vec::new(),
        is_system: false,
        auto_retry: false,
        system_reminder: None,
        raw_input: Some(prepared.raw_input),
        pastes: prepared.pastes,
    });
    app.handle_server_event(
        crate::protocol::ServerEvent::ModelChanged {
            id: 0,
            model: "test-model".into(),
            provider_name: None,
            error: Some("switch failed".into()),
        },
        &mut remote,
    );
    assert!(app.pending_fallback_resend.is_none());
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "fallback payload"
    );
}
