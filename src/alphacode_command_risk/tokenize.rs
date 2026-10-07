//! Minimal shell tokenizer, tuned for risk analysis rather than execution.
//!
//! This deliberately does not implement POSIX shell. It implements just enough
//! to answer "which words are targets of a destructive command", and it is
//! written to **fail loud rather than quiet**: when it cannot understand
//! something it marks the segment as opaque so the caller escalates.

/// One shell word plus the classification bits the assessor needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    /// True when this segment's stdin comes from a pipe, so its operands are
    /// partly supplied at runtime by the previous command.
    pub receives_pipe: bool,
    /// True for `>` / `>|` redirect destinations, which are truncated on open.
    pub is_truncating_redirect_target: bool,
    /// True for control operators like `&&`, which are never path targets.
    pub is_operator: bool,
    /// True when any part of this word was quoted in the original command.
    ///
    /// Quotes are resolved into `text`, so `grep -rn "sh" src/` and
    /// `grep -rn sh src/` are indistinguishable from `text` alone. That is
    /// correct for path comparison (`rm "$HOME"` must equal `rm $HOME`) but
    /// wrong for deciding whether a word is *executed*: a shell name inside a
    /// quoted argument is data -- a grep pattern, a commit message, an `echo`
    /// body -- and never becomes a program. Recorded separately so both
    /// questions can be answered from one tokenization.
    pub was_quoted: bool,
}

impl Token {
    fn word(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            receives_pipe: false,
            is_truncating_redirect_target: false,
            is_operator: false,
            was_quoted: false,
        }
    }

    /// The command name without its directory, so `/bin/rm` and
    /// `C:\Windows\System32\cmd.exe` both match `rm` / `cmd.exe`.
    ///
    /// Splits on both separators: on Windows a fully-qualified program path
    /// uses `\`, and splitting only on `/` would leave the whole path as the
    /// "name", so `C:\Windows\System32\cmd.exe /c del ...` would never be
    /// recognized as a shell invocation.
    ///
    /// Lowercased. Every list this feeds (`DESTRUCTIVE_COMMANDS`,
    /// `SHELL_COMMANDS`, `WRAPPER_COMMANDS`, `CONDITIONALLY_DESTRUCTIVE`) is
    /// written in lowercase, and the lookup is a plain `contains` on `&str`.
    /// Windows command resolution is case-insensitive, so without this
    /// `DeL /f /s /q C:\Users` and `RM -RF ~` matched nothing and classified as
    /// `Safe`. Downstream path comparison already normalises case separately
    /// (`normalize_for_compare`), so folding it here does not affect operands.
    pub fn basename(&self) -> String {
        self.text
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&self.text)
            .to_ascii_lowercase()
    }

    /// Whether this token is a portable `-x` flag rather than an operand.
    /// Windows slash switches are interpreted by [`Self::is_flag_for`], which
    /// needs the command name to avoid treating POSIX paths like `/etc` or `/c`
    /// as flags.
    pub fn is_flag(&self) -> bool {
        self.text.starts_with('-') && self.text.len() > 1
    }

    /// Whether this token is a flag for `program_name`.
    ///
    /// Only commands whose Windows syntax actually uses `/x` switches accept
    /// them. Context matters: `/c` is a `cmd.exe` option, but a path to the C:
    /// drive in Git Bash and must remain a target to `rm`.
    pub fn is_flag_for(&self, program_name: &str) -> bool {
        if self.is_flag() {
            return true;
        }
        let Some(switch) = self.text.strip_prefix('/') else {
            return false;
        };
        if switch.len() != 1 || !switch.as_bytes()[0].is_ascii_alphabetic() {
            return false;
        }
        let switch = char::from(switch.as_bytes()[0].to_ascii_lowercase());
        match program_name.to_ascii_lowercase().as_str() {
            "cmd" | "cmd.exe" => matches!(
                switch,
                'a' | 'c' | 'd' | 'e' | 'f' | 'k' | 'q' | 's' | 'u' | 'v'
            ),
            "del" | "erase" => matches!(switch, 'a' | 'f' | 'p' | 'q' | 's'),
            "rd" | "rmdir" => matches!(switch, 'q' | 's'),
            "diskpart" => switch == 's',
            "format" => matches!(switch, 'q' | 'u' | 'x'),
            _ => false,
        }
    }

    /// Whether this portable flag requests recursion, including bundles like
    /// `-rf` and `--recursive`.
    pub fn is_recursive_flag(&self) -> bool {
        if !self.is_flag() {
            return false;
        }
        if self.text.starts_with("--") {
            return self.text == "--recursive";
        }
        self.text.contains('r') || self.text.contains('R')
    }

    /// Whether this token requests recursion for a command, including `rd /s`.
    pub fn is_recursive_flag_for(&self, program_name: &str) -> bool {
        self.is_recursive_flag()
            || (self.text.eq_ignore_ascii_case("/s")
                && matches!(
                    program_name.to_ascii_lowercase().as_str(),
                    "del" | "erase" | "rd" | "rmdir"
                ))
    }
}

/// Characters that separate one command from the next.
///
/// `&` belongs here: it is cmd.exe's and POSIX's "run this after that" operator
/// and was missing, so `echo hi & del /f /s /q C:\Users\x` was analysed as a
/// single segment whose program was `echo`. `&&` is a separate token (see the
/// tokenizer) and still matches its own entry.
const SEGMENT_SEPARATORS: &[&str] = &["&&", "||", "&", ";", "|", "\n"];

/// Split a command line into individual command segments, each tokenized.
///
/// `rm -rf a && rm -rf b` yields two segments so both are assessed. Without
/// this, chaining would be a trivial bypass.
pub fn split_segments(command: &str) -> Vec<Vec<Token>> {
    let tokens = tokenize(command);
    let mut segments = Vec::new();
    let mut current = Vec::new();

    let mut next_receives_pipe = false;
    for token in tokens {
        if token.is_operator && SEGMENT_SEPARATORS.contains(&token.text.as_str()) {
            if !current.is_empty() {
                segments.push(std::mem::take(&mut current));
            }
            // The command after `|` consumes the previous one's output as
            // operands, which this parser cannot see (#604 review).
            next_receives_pipe = token.text == "|";
            continue;
        }
        let mut token = token;
        token.receives_pipe = next_receives_pipe;
        current.push(token);
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

/// Tokenize a command line, resolving quotes so `rm "$HOME"` and `rm $HOME`
/// produce the same target text.
///
/// Note the asymmetry with a real shell: we intentionally keep `$VAR` intact
/// rather than expanding it, and the path layer treats an unexpanded variable
/// as unknown-and-therefore-risky.
pub fn tokenize(command: &str) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    let mut current = String::new();
    let mut has_content = false;
    // Set whenever a quote is seen for the word being accumulated; stamped onto
    // the token at flush time.
    let mut quoted = false;
    let mut chars = command.chars().peekable();
    let mut pending_redirect = false;

    macro_rules! flush {
        () => {
            if has_content {
                let mut token = Token::word(std::mem::take(&mut current));
                #[allow(unused_assignments)]
                {
                    if pending_redirect {
                        token.is_truncating_redirect_target = true;
                        pending_redirect = false;
                    }
                    token.was_quoted = quoted;
                    quoted = false;
                    has_content = false;
                }
                tokens.push(token);
            }
        };
    }

    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                has_content = true;
                quoted = true;
                for q in chars.by_ref() {
                    if q == '\'' {
                        break;
                    }
                    current.push(q);
                }
            }
            '"' => {
                has_content = true;
                quoted = true;
                while let Some(q) = chars.next() {
                    if q == '"' {
                        break;
                    }
                    if q == '\\'
                        && let Some(&next) = chars.peek()
                        && matches!(next, '"' | '\\' | '$' | '`')
                    {
                        current.push(next);
                        chars.next();
                        continue;
                    }
                    current.push(q);
                }
            }
            '\\' => {
                // A backslash is an escape character in POSIX shells, but it is
                // the *path separator* on Windows. Treating every `\` as an
                // escape mangled `C:\Windows\System32` into
                // `C:WindowsSystem32`, so the protected-path check never
                // matched a Windows path and `rm -rf C:\Windows` sailed
                // through. Escape only the characters POSIX actually escapes
                // (mirroring the double-quoted branch above); otherwise keep
                // the backslash as part of the path.
                match chars.peek() {
                    Some(&next) if matches!(next, '"' | '\\' | '$' | '`') => {
                        has_content = true;
                        current.push(next);
                        chars.next();
                    }
                    _ => {
                        if has_content || !current.is_empty() {
                            has_content = true;
                        }
                        current.push('\\');
                    }
                }
            }
            ' ' | '\t' => flush!(),
            '\n' | ';' => {
                flush!();
                let mut op = Token::word(c.to_string());
                op.is_operator = true;
                tokens.push(op);
            }
            '&' | '|' => {
                flush!();
                let mut text = c.to_string();
                if chars.peek() == Some(&c) {
                    text.push(c);
                    chars.next();
                }
                let mut op = Token::word(text);
                op.is_operator = true;
                tokens.push(op);
            }
            '>' => {
                flush!();
                // `>>` appends and does not truncate, so it is far less
                // destructive; only a single `>` clobbers.
                if chars.peek() == Some(&'>') {
                    chars.next();
                } else {
                    if chars.peek() == Some(&'|') {
                        chars.next();
                    }
                    pending_redirect = true;
                }
            }
            '<' => flush!(),
            _ => {
                has_content = true;
                current.push(c);
            }
        }
    }
    flush!();

    tokens
}

#[cfg(test)]
#[path = "tokenize_tests.rs"]
mod tokenize_tests;
