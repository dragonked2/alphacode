use super::SwarmMember;
use super::client_lifecycle::TurnEventRelay;
use crate::alphacode_app_core::agent::Agent;
use crate::alphacode_provider_core::Provider;
use crate::protocol::ServerEvent;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, mpsc};

const AUTO_TITLE_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_AUTO_TITLE_CHARS: usize = 60;
const MAX_FALLBACK_TITLE_CHARS: usize = 48;

const TITLE_SYSTEM_PROMPT: &str = "\
Generate a concise, descriptive 3 to 5 word title for this conversation in the exact same language as the user's message (Arabic if Arabic, English if English). \
Output ONLY the title text — no quotes, no markdown, no prefixes, and no trailing punctuation.";

static IN_FLIGHT_AUTO_TITLES: OnceLock<StdMutex<HashSet<String>>> = OnceLock::new();

fn in_flight_set() -> &'static StdMutex<HashSet<String>> {
    IN_FLIGHT_AUTO_TITLES.get_or_init(|| StdMutex::new(HashSet::new()))
}

struct AutoTitleGuard {
    session_id: String,
}

impl AutoTitleGuard {
    fn try_acquire(session_id: &str) -> Option<Self> {
        let mut set = in_flight_set().lock().ok()?;
        if !set.insert(session_id.to_string()) {
            return None;
        }
        Some(Self {
            session_id: session_id.to_string(),
        })
    }
}

impl Drop for AutoTitleGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = in_flight_set().lock() {
            set.remove(&self.session_id);
        }
    }
}

pub(crate) struct AutoTitleInput {
    pub(crate) provider: Arc<dyn Provider>,
    pub(crate) user_snippet: String,
    pub(crate) assistant_snippet: String,
}

pub(crate) fn is_greeting_or_command_for_auto_title(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('/')
        || trimmed.starts_with("<system-reminder>")
        || trimmed.starts_with("<session-context>")
    {
        return true;
    }
    let clean = trimmed
        .trim_matches(|ch: char| ch.is_ascii_punctuation() || matches!(ch, '؟' | '،' | '!' | '.'))
        .to_lowercase();
    matches!(
        clean.as_str(),
        "hi" | "hello"
            | "hey"
            | "yo"
            | "test"
            | "ping"
            | "اهلا"
            | "أهلا"
            | "أهلاً"
            | "هلا"
            | "مرحبا"
            | "مرحباً"
            | "السلام عليكم"
            | "سلام"
            | "ازيك"
            | "ازيكم"
            | "عامل ايه"
            | "هاي"
            | "صباح الخير"
            | "مساء الخير"
    )
}

fn truncate_snippet(raw: &str, max_chars: usize) -> String {
    let compact = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        return compact;
    }
    let prefix: String = compact.chars().take(max_chars).collect();
    if let Some((head, _)) = prefix.rsplit_once(' ')
        && head.chars().count() >= max_chars / 2
    {
        return format!("{head}…");
    }
    format!("{prefix}…")
}

fn strip_tag_block(mut text: String, open_tag: &str, close_tag: &str) -> String {
    loop {
        let lower = text.to_ascii_lowercase();
        let Some(start) = lower.find(open_tag) else {
            break;
        };
        let after_open = start + open_tag.len();
        if let Some(rel_end) = lower[after_open..].find(close_tag) {
            let end = after_open + rel_end + close_tag.len();
            text.replace_range(start..end, " ");
        } else {
            text.truncate(start);
            break;
        }
    }
    text
}

fn strip_thinking_blocks(raw: &str) -> String {
    let text = strip_tag_block(raw.to_string(), "<thinking>", "</thinking>");
    strip_tag_block(text, "<think>", "</think>")
}

pub(crate) fn clean_ai_chat_title(raw: &str) -> Option<String> {
    let mut text = strip_thinking_blocks(raw).trim().to_string();
    if let Some(start) = text.find("<chat_title>")
        && let Some(end) = text[start + "<chat_title>".len()..].find("</chat_title>")
    {
        text = text[start + "<chat_title>".len()..start + "<chat_title>".len() + end]
            .trim()
            .to_string();
    }

    let first_line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("```"))?;

    let mut cleaned = first_line.trim().to_string();
    for prefix in [
        "Title:",
        "title:",
        "Chat Title:",
        "العنوان:",
        "عنوان المحادثة:",
        "عنوان الشات:",
    ] {
        if let Some(rest) = cleaned.strip_prefix(prefix) {
            cleaned = rest.trim().to_string();
            break;
        }
    }

    cleaned = cleaned
        .trim_start_matches(['#', '*', '-', '•'])
        .trim()
        .trim_matches(['"', '\'', '`', '“', '”', '«', '»'])
        .trim_end_matches(['.', ':', ';', '!', '?', '؟', ',', '،'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if cleaned.chars().count() < 2 {
        return None;
    }

    let lower = cleaned.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "new chat" | "new conversation" | "alphacode" | "alphacode chat" | "untitled" | "title"
    ) {
        return None;
    }

    if cleaned.chars().count() > MAX_AUTO_TITLE_CHARS {
        let slice: String = cleaned.chars().take(MAX_AUTO_TITLE_CHARS).collect();
        cleaned = if let Some((head, _)) = slice.rsplit_once(' ')
            && head.chars().count() >= 16
        {
            head.to_string()
        } else {
            slice
        };
    }

    Some(cleaned)
}

pub(crate) fn prepare_auto_title_input(
    agent: &Agent,
    user_content: &str,
    assistant_reply: Option<&str>,
) -> Option<AutoTitleInput> {
    if !agent.should_auto_generate_title() {
        return None;
    }
    if is_greeting_or_command_for_auto_title(user_content) {
        return None;
    }
    let reply_trimmed = assistant_reply.map(str::trim).filter(|s| !s.is_empty())?;
    let user_snippet = truncate_snippet(user_content, 500);
    let assistant_snippet = truncate_snippet(reply_trimmed, 500);
    if user_snippet.is_empty() || assistant_snippet.is_empty() {
        return None;
    }
    Some(AutoTitleInput {
        provider: agent.provider_handle(),
        user_snippet,
        assistant_snippet,
    })
}

pub(crate) fn fallback_title_from_prompt(raw: &str) -> Option<String> {
    if is_greeting_or_command_for_auto_title(raw) {
        return None;
    }
    let first_line = raw.lines().map(str::trim).find(|line| {
        !line.is_empty()
            && !line.starts_with("```")
            && !line.starts_with("<system-reminder>")
            && !line.starts_with("<session-context>")
    })?;
    let mut text = first_line.split_whitespace().collect::<Vec<_>>().join(" ");

    const ARABIC_PREFIXES: &[&str] = &[
        "من فضلك ",
        "لو سمحت ",
        "ممكن تساعدني في ",
        "ممكن تساعدني ",
        "ممكن تشرحلي ",
        "ممكن تشرح لي ",
        "ممكن تعمل ",
        "ممكن ",
        "عاوزك ",
        "عايزك ",
        "عاوز اعرف ",
        "عايز اعرف ",
        "عاوز ",
        "عايز ",
        "اريد ",
        "أريد ",
        "محتاج ",
        "ابغى ",
        "أبغى ",
    ];
    const ENGLISH_PREFIXES: &[&str] = &[
        "please help me with ",
        "please help me ",
        "please can you ",
        "please could you ",
        "please ",
        "help me with ",
        "help me ",
        "can you please ",
        "can you help me ",
        "can you ",
        "could you please ",
        "could you ",
        "would you ",
        "i want you to ",
        "i want to ",
        "i need you to ",
        "i need to ",
    ];

    for _ in 0..2 {
        let mut stripped = false;
        for prefix in ARABIC_PREFIXES {
            if let Some(rest) = text.strip_prefix(prefix) {
                let rest_trimmed = rest.trim();
                if !rest_trimmed.is_empty() {
                    text = rest_trimmed.to_string();
                    stripped = true;
                    break;
                }
            }
        }
        if stripped {
            continue;
        }
        let lower = text.to_ascii_lowercase();
        for prefix in ENGLISH_PREFIXES {
            if lower.starts_with(prefix) && text.is_char_boundary(prefix.len()) {
                let rest_trimmed = text[prefix.len()..].trim();
                if !rest_trimmed.is_empty() {
                    text = rest_trimmed.to_string();
                    stripped = true;
                    break;
                }
            }
        }
        if !stripped {
            break;
        }
    }

    let cleaned = text
        .trim_start_matches(['#', '*', '-', '•'])
        .trim()
        .trim_matches(['"', '\'', '`', '“', '”', '«', '»'])
        .trim_end_matches(['?', '؟', '!', '.', ',', '،', ':', ';'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if cleaned.chars().count() < 2 {
        return None;
    }

    if cleaned.chars().count() <= MAX_FALLBACK_TITLE_CHARS {
        return Some(cleaned);
    }

    let slice: String = cleaned.chars().take(MAX_FALLBACK_TITLE_CHARS - 1).collect();
    if let Some((head, _)) = slice.rsplit_once(' ')
        && head.chars().count() >= 16
    {
        return Some(format!("{head}…"));
    }
    Some(format!("{slice}…"))
}

pub(crate) fn spawn_background_auto_title(
    agent_arc: Arc<Mutex<Agent>>,
    session_id: String,
    members: Arc<RwLock<HashMap<String, SwarmMember>>>,
    fallback_tx: mpsc::UnboundedSender<ServerEvent>,
    input: AutoTitleInput,
) {
    tokio::spawn(async move {
        let Some(_inflight) = AutoTitleGuard::try_acquire(&session_id) else {
            return;
        };

        let prompt = format!(
            "User: {}\nAssistant: {}",
            input.user_snippet, input.assistant_snippet
        );
        let raw_title = match tokio::time::timeout(
            AUTO_TITLE_TIMEOUT,
            input.provider.complete_simple(&prompt, TITLE_SYSTEM_PROMPT),
        )
        .await
        {
            Ok(Ok(text)) => Some(text),
            Ok(Err(error)) => {
                crate::logging::warn(&format!(
                    "Background auto-title model failed for session {session_id}, using fallback: {error:#}"
                ));
                None
            }
            Err(_) => {
                crate::logging::warn(&format!(
                    "Background auto-title timed out for session {session_id}, using fallback"
                ));
                None
            }
        };

        let Some(final_title) = raw_title
            .as_deref()
            .and_then(clean_ai_chat_title)
            .or_else(|| fallback_title_from_prompt(&input.user_snippet))
        else {
            return;
        };

        let display_title = {
            let mut agent = agent_arc.lock().await;
            if !agent.should_auto_generate_title() {
                return;
            }
            match agent.set_generated_session_title(Some(final_title)) {
                Ok(display) => display,
                Err(error) => {
                    crate::logging::warn(&format!(
                        "Failed to save background auto-title for session {session_id}: {error:#}"
                    ));
                    return;
                }
            }
        };

        crate::session_list_cache::invalidate();
        let event = ServerEvent::SessionRenamed {
            session_id: session_id.clone(),
            title: None,
            display_title,
        };
        TurnEventRelay::new(&members, &session_id, Some(&fallback_tx))
            .send(event)
            .await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_filter_skips_greetings_and_commands_but_keeps_real_prompts() {
        assert!(is_greeting_or_command_for_auto_title("اهلا"));
        assert!(is_greeting_or_command_for_auto_title("السلام عليكم!"));
        assert!(is_greeting_or_command_for_auto_title("hi"));
        assert!(is_greeting_or_command_for_auto_title("/model gemini"));
        assert!(is_greeting_or_command_for_auto_title(
            "<system-reminder>x</system-reminder>"
        ));
        assert!(!is_greeting_or_command_for_auto_title(
            "عاوز اصلح مشكلة الكوكيز في Auth"
        ));
        assert!(!is_greeting_or_command_for_auto_title(
            "Fix OAuth callback timeout"
        ));
    }

    #[test]
    fn clean_ai_chat_title_strips_noise_quotes_and_rejects_placeholders() {
        assert_eq!(
            clean_ai_chat_title("  \"بناء خادم FastAPI للمصادقة.\" \nExtra line").as_deref(),
            Some("بناء خادم FastAPI للمصادقة")
        );
        assert_eq!(
            clean_ai_chat_title("Title: <chat_title>OAuth Callback Timeout Fix</chat_title>")
                .as_deref(),
            Some("OAuth Callback Timeout Fix")
        );
        assert!(clean_ai_chat_title("New Chat").is_none());
        assert!(clean_ai_chat_title("x").is_none());
    }

    #[test]
    fn clean_ai_chat_title_strips_think_and_thinking_blocks() {
        assert_eq!(
            clean_ai_chat_title(
                "<think>\nLet me analyze what the user asked...\n</think>\nFix SQLite Workspace Sync"
            )
            .as_deref(),
            Some("Fix SQLite Workspace Sync")
        );
        assert_eq!(
            clean_ai_chat_title(
                "<THINKING>Reasoning step 1\nStep 2</THINKING>\n<chat_title>إصلاح مزامنة العناوين</chat_title>"
            )
            .as_deref(),
            Some("إصلاح مزامنة العناوين")
        );
        // Only thinking block -> returns None so caller falls back cleanly
        assert!(clean_ai_chat_title("<think>Unclosed reasoning without title").is_none());
        assert!(clean_ai_chat_title("<thinking>Only reasoning</thinking>").is_none());
    }

    #[test]
    fn fallback_title_from_prompt_strips_prefixes_punctuation_and_caps_at_48_chars() {
        assert_eq!(
            fallback_title_from_prompt("ممكن تصلح مشكلة تسجيل الدخول في API؟").as_deref(),
            Some("تصلح مشكلة تسجيل الدخول في API")
        );
        assert_eq!(
            fallback_title_from_prompt("عاوز اعمل صفحة اعدادات جديدة!").as_deref(),
            Some("اعمل صفحة اعدادات جديدة")
        );
        assert_eq!(
            fallback_title_from_prompt("Please fix the WebSocket reconnect timeout bug...")
                .as_deref(),
            Some("fix the WebSocket reconnect timeout bug")
        );
        assert!(fallback_title_from_prompt("اهلا").is_none());

        let long_prompt = "Please implement a comprehensive distributed caching layer with automatic invalidation and metrics";
        let capped = fallback_title_from_prompt(long_prompt).unwrap();
        assert!(
            capped.chars().count() <= 48,
            "got {} chars: {capped}",
            capped.chars().count()
        );
        assert!(capped.ends_with('…'));
    }
}
