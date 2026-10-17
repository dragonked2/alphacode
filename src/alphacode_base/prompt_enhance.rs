//! Prompt Autopilot: deterministic assessment and briefing of user input.
//!
//! Users who are not professional engineers often submit very short or vague
//! requests ("fix this", "make it work", "it doesn't work"). Historically
//! those requests produced shallow or wrong results because the model had to
//! guess the intent. This module classifies the request and, when it is
//! underspecified, produces a compact "task briefing" that is injected into
//! the turn's dynamic system context (never into the user's stored text), so
//! the model restate the concrete requirement, infers missing details from the
//! codebase, lists assumptions, and verifies its work before reporting.
//!
//! Everything here is pure and deterministic: no I/O, no model calls, no
//! latency. A false negative ("prompt was vague but we did not notice") only
//! reproduces the previous behavior; a false positive costs a few dozen extra
//! tokens, so the heuristics intentionally bias toward specificity signals.

/// Coarse classification of what the user is asking for. Used to tailor the
/// briefing ("verify with tests" for bug fixes, "check the diff" for
/// refactors, and so on).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    BugFix,
    Feature,
    Refactor,
    Performance,
    Test,
    Docs,
    Explain,
    Question,
    Run,
    Security,
    Other,
}

impl TaskKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::BugFix => "bug fix",
            Self::Feature => "feature / change",
            Self::Refactor => "refactor",
            Self::Performance => "performance",
            Self::Test => "test",
            Self::Docs => "documentation",
            Self::Explain => "explanation",
            Self::Question => "question",
            Self::Run => "run / build / setup",
            Self::Security => "security review",
            Self::Other => "general task",
        }
    }

    /// One extra verification hint tailored to the task type, appended to the
    /// briefing so an inexperienced user gets "done" defined for them.
    fn verification_hint(self) -> &'static str {
        match self {
            Self::BugFix => {
                "Reproduce the bug (or point to the failing signal) first; after the fix, \
                 re-run the same check to prove it passes."
            }
            Self::Test => {
                "Run the tests you added/changed and report the pass/fail output; never claim \
                 a test passes without running it."
            }
            Self::Performance => {
                "Measure before and after (timing or benchmark output) and report the numbers; \
                 do not claim a speedup without evidence."
            }
            Self::Refactor => {
                "Keep behavior identical: run the existing build/tests before and after and \
                 show that they still pass."
            }
            Self::Security => {
                "Stay inside the authorized scope, distinguish confirmed findings from \
                 hypotheses, and include reproducible evidence for each finding."
            }
            Self::Run => "Report the exact command(s) run and their real output.",
            _ => "Verify the result (build, test, or manual check) before reporting it as done.",
        }
    }
}

/// Result of assessing a user prompt.
#[derive(Debug, Clone)]
pub struct Assessment {
    pub task_kind: TaskKind,
    /// True when the prompt is too underspecified to act on directly and the
    /// briefing should be attached.
    pub vague: bool,
    /// Human-readable reasons the prompt was classified as vague (empty when
    /// the prompt is specific enough).
    pub reasons: Vec<&'static str>,
}

/// Classify a user prompt and decide whether it needs a briefing.
pub fn assess(text: &str) -> Assessment {
    let trimmed = text.trim();
    let lower = trimmed.to_ascii_lowercase();
    let words = trimmed.split_whitespace().count();
    let task_kind = classify_task_kind(&lower);

    let mut reasons: Vec<&'static str> = Vec::new();

    if is_specific(&lower, words) {
        return Assessment {
            task_kind,
            vague: false,
            reasons,
        };
    }

    if words <= 2 {
        reasons.push("only a few words long");
    } else if words <= 4 {
        reasons.push("very short request");
    }
    if has_generic_intent(&lower) {
        reasons.push("generic intent with no concrete details");
    }
    if !mentions_error_detail(&lower) && matches!(task_kind, TaskKind::BugFix) {
        reasons.push("asks for a fix without an error message, log, or failing case");
    }
    if reasons.is_empty() {
        // Specificity heuristics did not fire but the prompt is still on the
        // short side with no supporting detail; treat as mildly vague.
        if words < 6 {
            reasons.push("no concrete details (files, errors, or acceptance criteria)");
        } else {
            return Assessment {
                task_kind,
                vague: false,
                reasons,
            };
        }
    }

    Assessment {
        task_kind,
        vague: true,
        reasons,
    }
}

/// Build the enhanced user message for an underspecified prompt, or `None`
/// when the prompt is already specific enough (the common case) so callers
/// can leave the user's text untouched.
///
/// The returned text keeps the user's original words verbatim and appends a
/// clearly-delimited briefing; nothing is rewritten or removed.
pub fn enhance(text: &str, working_dir: Option<&str>) -> Option<String> {
    let assessment = assess(text);
    if !assessment.vague {
        return None;
    }
    Some(enhanced_message(text.trim_end(), &assessment, working_dir))
}

/// Render only the briefing section, for injection into the turn's dynamic
/// system context when the user's stored text must stay verbatim (the
/// Prompt Autopilot path used by the agent on every provider request).
pub fn briefing_for_assessment(assessment: &Assessment) -> String {
    let mut out = String::with_capacity(640);
    out.push_str("# Task Briefing (auto)\n\n");
    out.push_str(&format!(
        "The user's latest request was auto-classified as \"{}\" and is underspecified ({}).\n\n",
        assessment.task_kind.label(),
        assessment.reasons.join("; ")
    ));
    out.push_str(
        "Treat the request as a goal, not a literal script:\n\
         1. Locate the relevant code/context and restate the concrete requirement in 1-2 sentences.\n\
         2. If details are missing, infer the most reasonable interpretation from the codebase, \
         make reasonable assumptions, and proceed aggressively. Only ask if the blocker is absolute and cannot be inferred.\n\
         3. Define what \"done\" means, make the smallest complete change, and do not stop halfway.\n\
         4. Verify before reporting. ",
    );
    out.push_str(assessment.task_kind.verification_hint());
    out
}

/// Render the enhanced message. Public so tests (and future UI previews) can
/// build the same text without going through [`enhance`].
pub fn enhanced_message(
    original: &str,
    assessment: &Assessment,
    working_dir: Option<&str>,
) -> String {
    let mut out = String::with_capacity(original.len() + 700);
    out.push_str(original);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n<auto-enhanced by Alphacode>\n");
    out.push_str(&format!(
        "Task type (auto-detected): {}\n",
        assessment.task_kind.label()
    ));
    if !assessment.reasons.is_empty() {
        out.push_str(&format!(
            "Note: this request was brief ({}).\n",
            assessment.reasons.join("; ")
        ));
    }
    out.push_str(
        "Treat the request above as a goal, not a literal script:\n\
         1. Locate the relevant code/context first and restate the concrete requirement in 1-2 sentences.\n\
         2. If details are missing, infer the most reasonable interpretation from the codebase, \
         briefly list your assumptions, and proceed. Ask the user only when an ambiguity is truly blocking.\n\
         3. Define what \"done\" means (an explicit acceptance check), make the smallest complete change, \
         and do not stop halfway.\n\
         4. Verify before reporting. ",
    );
    out.push_str(assessment.task_kind.verification_hint());
    out.push('\n');
    if let Some(dir) = working_dir.map(str::trim).filter(|d| !d.is_empty()) {
        out.push_str(&format!("Working directory: {dir}\n"));
    }
    out.push_str("</auto-enhanced>\n");
    out
}

fn classify_task_kind(lower: &str) -> TaskKind {
    // Order matters: the most specific families first so "write tests for the
    // crash" classifies as Test, not BugFix.
    if contains_any(
        lower,
        &[
            "test failing",
            "tests failing",
            "tests are failing",
            "test is failing",
            "failing test",
            "failing tests",
            "unit test",
            "integration test",
            "add tests",
            "write tests",
            "test coverage",
            "make a test",
            "test this",
        ],
    ) {
        return TaskKind::Test;
    }
    if contains_any(
        lower,
        &[
            "security",
            "vulnerab",
            "cve-",
            "xss",
            "csrf",
            "injection",
            "recon",
            "bug bounty",
            "exploit",
            "pentest",
            "渗透",
        ],
    ) {
        return TaskKind::Security;
    }
    if contains_any(
        lower,
        &[
            "slow",
            "speed",
            "performance",
            "optimize",
            "optimise",
            "speed up",
            "speedup",
            "latency",
            "memory leak",
            "benchmark",
        ],
    ) {
        return TaskKind::Performance;
    }
    if contains_any(
        lower,
        &[
            "refactor",
            "clean up",
            "cleanup",
            "restructure",
            "rename",
            "simplify",
            "reorganize",
            "reorganise",
            "extract into",
            "move into",
        ],
    ) {
        return TaskKind::Refactor;
    }
    if contains_any(
        lower,
        &[
            "doesn't work",
            "does not work",
            "not working",
            "broken",
            "crash",
            "panic",
            "bug",
            "error",
            "exception",
            "traceback",
            "regression",
            "fails",
            "failing",
            "fix",
            "debug",
            "wrong result",
            "infinite loop",
            "stack overflow",
            "null pointer",
            "segfault",
        ],
    ) {
        return TaskKind::BugFix;
    }
    if contains_any(
        lower,
        &[
            "document",
            "readme",
            "changelog",
            "docstring",
            "comment the",
            "write docs",
        ],
    ) {
        return TaskKind::Docs;
    }
    if contains_any(
        lower,
        &[
            "explain",
            "why does",
            "how does",
            "what does",
            "walk me through",
            "summarize",
            "summarise",
            "what is",
        ],
    ) {
        return TaskKind::Explain;
    }
    if contains_any(
        lower,
        &[
            "run ",
            "start the",
            "launch",
            "install",
            "set up",
            "setup",
            "build the",
            "compile",
            "serve",
        ],
    ) {
        return TaskKind::Run;
    }
    if contains_any(
        lower,
        &[
            "add ",
            "implement",
            "create",
            "new feature",
            "build me",
            "generate",
            "support for",
            "make me",
        ],
    ) {
        return TaskKind::Feature;
    }
    if lower.ends_with('?') || lower.starts_with("what ") || lower.starts_with("which ") {
        return TaskKind::Question;
    }
    TaskKind::Other
}

fn contains_any(lower: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| lower.contains(needle))
}

/// Specificity signals: hard evidence that the user gave (or pointed at)
/// concrete material, which means the prompt does not need a briefing.
fn is_specific(lower: &str, words: usize) -> bool {
    // Code fences mean real content was pasted.
    if lower.contains("```") {
        return true;
    }
    // File-path- or code-shaped tokens.
    if has_file_reference(lower) {
        return true;
    }
    // A pasted error/log excerpt is already a concrete failing case.
    if mentions_error_detail(lower) && words >= 8 {
        return true;
    }
    // Long enough to carry its own details.
    if words >= 14 {
        return true;
    }
    // Explicit acceptance-criteria language implies the user thought through
    // the outcome.
    contains_any(
        lower,
        &[
            "should ",
            "expect",
            "make sure",
            "without breaking",
            "keep the",
            "preserve",
            "so that",
            "in order to",
        ],
    ) && words >= 6
}

fn has_file_reference(lower: &str) -> bool {
    const EXTENSIONS: &[&str] = &[
        ".rs", ".py", ".js", ".jsx", ".ts", ".tsx", ".go", ".java", ".kt", ".c", ".h", ".cpp",
        ".hpp", ".cs", ".rb", ".php", ".sh", ".ps1", ".toml", ".yaml", ".yml", ".json", ".md",
        ".html", ".css", ".sql", ".swift", ".zig",
    ];
    for word in lower.split_whitespace() {
        if EXTENSIONS.iter().any(|ext| word.ends_with(ext)) {
            return true;
        }
    }
    // Path separators with alphanumerics on both sides ("src/app", "a\\b").
    lower.split_whitespace().any(|word| {
        let bytes = word.as_bytes();
        bytes.len() > 2
            && bytes.iter().any(|b| *b == b'/' || *b == b'\\')
            && bytes.iter().any(|b| b.is_ascii_alphanumeric())
    })
}

fn mentions_error_detail(lower: &str) -> bool {
    contains_any(
        lower,
        &[
            "error:",
            "exception",
            "traceback",
            "panic",
            "fatal",
            "eacces",
            "enoent",
            "eaddrinuse",
            "undefined is not",
            "cannot find",
            "expected ",
            "at line ",
            "line ",
            "status code",
            "http/",
        ],
    )
}

/// Openers/phrases that signal "I am pointing at something without saying
/// what" — the classic underspecified request.
fn has_generic_intent(lower: &str) -> bool {
    let t = lower.trim();
    const EXACT: &[&str] = &[
        "fix",
        "fix it",
        "fix this",
        "help",
        "help me",
        "improve",
        "make it work",
        "make it better",
        "make this work",
        "it doesn't work",
        "it does not work",
        "not working",
        "something is wrong",
        "something broken",
        "do it",
        "go on",
        "continue",
        "keep going",
        "check",
        "check this",
        "look into",
        "asap",
        "it fails",
        "broken?",
        "why?",
        "how?",
        "what next",
        "then what",
        "optimize",
        "clean up",
    ];
    if EXACT.contains(&t) {
        return true;
    }
    // Trailing ellipsis or "etc" hedge: the user left the requirement open.
    if t.ends_with("...") || t.ends_with('…') || t.ends_with(" etc") || t.ends_with(" etc.") {
        return true;
    }
    // "fix the X" with a one-word X, e.g. "fix bug", "fix error".
    if let Some(rest) = t.strip_prefix("fix ")
        && rest.split_whitespace().count() <= 1
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specific_prompts_are_left_untouched() {
        for prompt in [
            "Fix the off-by-one error in src/alphacode_base/id.rs:42 — `next_id` returns the previous counter value, so ids collide after the first call.",
            "Add a `--json` flag to the doctor command that prints the report as one JSON object instead of the table. Preserve the existing table output when the flag is absent.",
            "```\nerror[E0308]: mismatched types\n  --> src/main.rs:10\n```",
            "The tokenizer crashes with a traceback when the input has an unmatched surrogate; make sure unicode paths keep working without breaking the ascii fast path.",
        ] {
            let assessment = assess(prompt);
            assert!(
                !assessment.vague,
                "prompt should be considered specific: {prompt:?} (reasons: {:?})",
                assessment.reasons
            );
            assert!(enhance(prompt, None).is_none());
        }
    }

    #[test]
    fn vague_prompts_are_enhanced() {
        for prompt in [
            "fix",
            "fix this",
            "it doesn't work",
            "make it better",
            "improve speed",
            "something is broken help",
        ] {
            let assessment = assess(prompt);
            assert!(assessment.vague, "prompt should be vague: {prompt:?}");
            let enhanced = enhance(prompt, Some("/tmp/project")).expect("should enhance");
            assert!(enhanced.starts_with(prompt));
            assert!(enhanced.contains("<auto-enhanced by Alphacode>"));
            assert!(enhanced.contains("Task type (auto-detected):"));
            assert!(enhanced.contains("Working directory: /tmp/project"));
        }
    }

    #[test]
    fn bug_fix_without_error_detail_is_flagged() {
        let assessment = assess("fix the login bug in the app");
        assert_eq!(assessment.task_kind, TaskKind::BugFix);
        assert!(assessment.vague);
    }

    #[test]
    fn task_kinds_are_classified() {
        assert_eq!(
            assess("the tests are failing after my change").task_kind,
            TaskKind::Test
        );
        assert_eq!(
            assess("scan this endpoint for injection vulnerabilities").task_kind,
            TaskKind::Security
        );
        assert_eq!(
            assess("the parser is slow, optimize it").task_kind,
            TaskKind::Performance
        );
        assert_eq!(
            assess("refactor the config loader into its own module").task_kind,
            TaskKind::Refactor
        );
        assert_eq!(
            assess("explain how the retry loop works").task_kind,
            TaskKind::Explain
        );
        assert_eq!(
            assess("add a dark mode toggle to the settings page").task_kind,
            TaskKind::Feature
        );
    }

    #[test]
    fn enhancement_never_rewrites_the_original_text() {
        let original = "fix it";
        let enhanced = enhance(original, None).expect("should enhance");
        assert!(enhanced.starts_with(original));
        assert!(enhanced.contains("Treat the request above as a goal"));
    }

    #[test]
    fn empty_and_whitespace_prompts_are_vague_not_specific() {
        let assessment = assess("   ");
        assert!(assessment.vague);
    }

    #[test]
    fn briefing_stands_alone_without_the_original_text() {
        let assessment = assess("fix it");
        let briefing = briefing_for_assessment(&assessment);
        assert!(briefing.starts_with("# Task Briefing (auto)"));
        assert!(briefing.contains("auto-classified"));
        assert!(briefing.contains("Verify before reporting"));
    }
}
