//! The streaming-text cache fingerprint.
//!
//! The prepared-frame cache is keyed on the live stream. Hashing the whole
//! buffer on every frame is O(bytes), which for a long response is a
//! measurable slice of the frame budget, so the fingerprint samples three
//! windows instead. It still has to change whenever the stream changes, or
//! the transcript would show a stale tail.

use super::super::hash_text_for_cache;

#[test]
fn empty_text_has_a_stable_fingerprint() {
    assert_eq!(hash_text_for_cache(""), hash_text_for_cache(""));
}

#[test]
fn appending_changes_the_fingerprint() {
    let base = hash_text_for_cache("the quick brown fox");
    let appended = hash_text_for_cache("the quick brown fox jumps");
    assert_ne!(base, appended);
}

#[test]
fn a_long_append_changes_the_fingerprint() {
    // Above the exact-fingerprint limit the fingerprint samples windows, so
    // the guarantee is "an append always registers" — the length and the
    // tail window both move.
    let mut text = "a".repeat(4096);
    let before = hash_text_for_cache(&text);
    text.push_str("tail change");
    assert_ne!(before, hash_text_for_cache(&text));
}

#[test]
fn a_change_anywhere_in_a_short_buffer_changes_the_fingerprint() {
    // Short buffers are fingerprinted exactly, so no edit can hide.
    let mut text = "a".repeat(200);
    let before = hash_text_for_cache(&text);
    text.replace_range(100..110, "DIFFERENT!");
    assert_ne!(before, hash_text_for_cache(&text));
}

#[test]
fn a_same_length_replacement_changes_the_fingerprint() {
    let before = hash_text_for_cache("alpha beta gamma");
    let after = hash_text_for_cache("alpha delta gamma");
    assert_ne!(before, after);
}

#[test]
fn different_texts_get_different_fingerprints() {
    assert_ne!(hash_text_for_cache("hello"), hash_text_for_cache("world"));
    assert_ne!(
        hash_text_for_cache("prefix/hello"),
        hash_text_for_cache("prefix/world")
    );
}
