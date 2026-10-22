//! Shared composer token helpers.
//!
//! The composer multiplexes three grammars inside one buffer: slash commands
//! (`/model gpt-5`), filesystem paths (`/usr/bin/env`) and ordinary prose.
//! Everything that needs to answer "which grammar is the user typing?" asks
//! here, so the palette, the composer chrome and the key handlers can never
//! disagree about it.

/// The slash token the composer is currently typing, if any.
///
/// Returns the first whitespace-delimited word of the trimmed input when that
/// word starts with `/`. `Some("/")` means the user typed the command
/// introducer but no name yet.
pub fn slash_token(input: &str) -> Option<&str> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with('/') {
        return None;
    }
    Some(trimmed.split_whitespace().next().unwrap_or("/"))
}

/// Whether a slash token is really a filesystem path rather than a command.
///
/// Slash commands are a single bare word (`/model`, `/help`). A token with a
/// second `/` inside it (`/usr/bin/env`, `/c/Users/me`) is a path. Offering
/// command completions for a path is noise at best, and it used to be
/// actively hostile: `Esc` on an open palette cleared the *whole draft*, so a
/// user who typed an absolute path lost it to an undismissable popup.
pub fn is_path_like_token(token: &str) -> bool {
    token.starts_with('/') && token[1..].contains('/')
}

/// Whether the composer is currently typing something that should be treated
/// as a filesystem path rather than a slash command.
pub fn is_typing_path(input: &str) -> bool {
    slash_token(input).is_some_and(is_path_like_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_token_reads_first_word() {
        assert_eq!(slash_token("/model"), Some("/model"));
        assert_eq!(slash_token("  /model gpt-5"), Some("/model"));
        assert_eq!(slash_token("/"), Some("/"));
        assert_eq!(slash_token("hello"), None);
        assert_eq!(slash_token(""), None);
    }

    #[test]
    fn path_like_tokens_are_detected() {
        assert!(is_path_like_token("/usr/bin/env"));
        assert!(is_path_like_token("/c/Users/me"));
        assert!(!is_path_like_token("/model"));
        assert!(!is_path_like_token("/"));
    }

    #[test]
    fn typing_path_matches_slash_token_rules() {
        assert!(is_typing_path("/usr/bin/env"));
        assert!(!is_typing_path("/model"));
        assert!(!is_typing_path("plain text"));
    }
}
