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
}
