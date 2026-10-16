use super::{Token, split_segments, tokenize};

#[test]
fn simple_command_tokenized() {
    let tokens = tokenize("ls -la /tmp");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].text, "ls");
    assert!(!tokens[0].is_flag());
    assert_eq!(tokens[1].text, "-la");
    assert!(tokens[1].is_flag());
    assert_eq!(tokens[2].text, "/tmp");
}

/// `receives_pipe` is a per-segment property assigned by [`split_segments`]
/// (the API the assessor uses): the command after `|` consumes the previous
/// command's output as operands, which this parser cannot see (#604 review).
/// [`tokenize`] alone is word-level and leaves the flag unset.
#[test]
fn pipe_sets_receives_pipe_on_the_segment_after_the_pipe() {
    let segments = split_segments("cat file.txt | xargs rm");
    let last_segment = segments.last().expect("at least one segment");
    let last = last_segment.last().expect("segment has tokens");
    assert!(last.receives_pipe);

    // The first segment (the pipe producer) never receives a pipe.
    let first_segment = &segments[0];
    assert!(first_segment.iter().all(|t| !t.receives_pipe));
}

/// The `>` operator is consumed while tokenizing; the *next* word is marked
/// as the truncating redirect target so the assessor can flag clobbers.
#[test]
fn redirect_target_marked() {
    let tokens = tokenize("echo hello > /tmp/out.txt");
    let target = tokens
        .iter()
        .find(|t| t.is_truncating_redirect_target)
        .expect("redirect target token");
    assert_eq!(target.text, "/tmp/out.txt");
    assert!(!target.is_operator);
}

#[test]
fn empty_string_yields_no_tokens() {
    assert!(tokenize("").is_empty());
}

#[test]
fn basename_strips_directory() {
    let token = Token::word("/usr/bin/rm");
    assert_eq!(token.basename(), "rm");
}

#[test]
fn recursive_flag_detection() {
    assert!(Token::word("-rf").is_recursive_flag());
    assert!(Token::word("-r").is_recursive_flag());
    assert!(Token::word("--recursive").is_recursive_flag());
    assert!(!Token::word("-la").is_recursive_flag());
    assert!(!Token::word("file").is_recursive_flag());
}

#[test]
fn is_flag_requires_dash() {
    assert!(Token::word("-l").is_flag());
    assert!(!Token::word("file").is_flag());
    assert!(!Token::word("/etc").is_flag());
    assert!(!Token::word("/c").is_flag());
    assert!(Token::word("/s").is_flag_for("rd"));
    assert!(Token::word("/C").is_flag_for("cmd.exe"));
    assert!(!Token::word("/c").is_flag_for("rm"));
    assert!(Token::word("/s").is_recursive_flag_for("rd"));
    assert!(!Token::word("/s").is_recursive_flag_for("rm"));
}

/// `2>&1` duplicates an already-open descriptor: it writes to no file, and the
/// `&` belongs to the redirect rather than separating commands. Before this
/// was handled, `cmd 2>&1 | head` produced a bare `1` token flagged as a
/// truncating redirect target — see
/// `fd_redirects_are_not_denied_from_a_protected_working_directory` in
/// `false_positive_tests.rs` for the denial it caused.
#[test]
fn fd_duplication_is_not_a_separator_or_a_target() {
    let tokens = tokenize("curl -s http://x 2>&1 | head -5");
    assert!(
        tokens.iter().all(|t| t.text != "1" && t.text != "&"),
        "`2>&1` must not leave an fd token or an `&` operator behind: {tokens:?}"
    );
    assert!(
        !tokens.iter().any(|t| t.is_truncating_redirect_target),
        "`2>&1` truncates no file: {tokens:?}"
    );
    let segments = split_segments("curl -s http://x 2>&1 | head -5");
    assert_eq!(
        segments.len(),
        2,
        "`2>&1` must not split the pipe into extra segments: {segments:?}"
    );
}

/// `>&2` and `2>&-` name a destination descriptor, not a path.
#[test]
fn fd_duplication_consumes_the_destination() {
    let tokens = tokenize("echo hi >&2");
    let texts: Vec<&str> = tokens.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(texts, ["echo", "hi"], "{tokens:?}");

    let tokens = tokenize("exec 2>&-");
    let texts: Vec<&str> = tokens.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(texts, ["exec"], "{tokens:?}");
}

/// The digits directly before `>` name the descriptor being redirected, not
/// an operand: `readlink -f x 2>/dev/null` must not leave a `2` word behind.
#[test]
fn redirect_fd_number_is_not_an_operand() {
    let tokens = tokenize("readlink -f x 2>/dev/null");
    assert!(
        tokens.iter().all(|t| t.text != "2"),
        "the fd number before `>` is not a word: {tokens:?}"
    );
    let target = tokens
        .iter()
        .find(|t| t.is_truncating_redirect_target)
        .expect("redirect target token");
    assert_eq!(target.text, "/dev/null");
}

/// A genuine `&` (background / command separator) must still split segments.
#[test]
fn plain_ampersand_still_separates_commands() {
    let segments = split_segments("echo a & rm -rf x");
    assert_eq!(segments.len(), 2, "{segments:?}");
    let tokens = tokenize("echo a & echo b");
    assert!(
        tokens.iter().any(|t| t.is_operator && t.text == "&"),
        "{tokens:?}"
    );
}
