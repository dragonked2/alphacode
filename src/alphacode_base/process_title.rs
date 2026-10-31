const LINUX_PROCESS_TITLE_LIMIT: usize = 15;
#[cfg(target_os = "linux")]
const KILLALL_PROCESS_NAME: &str = "alphacode";

pub fn compact_process_title(prefix: &str, name: Option<&str>) -> String {
    let mut title = prefix.to_string();
    if let Some(name) = name.filter(|name| !name.is_empty()) {
        let remaining = LINUX_PROCESS_TITLE_LIMIT.saturating_sub(title.len());
        if remaining > 0 {
            title.push_str(&name.chars().take(remaining).collect::<String>());
        }
    }
    title
}

pub fn session_name(session_id: &str) -> String {
    crate::id::extract_session_name(session_id)
        .map(|name| name.to_string())
        .unwrap_or_else(|| session_id.to_string())
}

fn normalized_display_title(title: &str) -> Option<String> {
    let normalized = title.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

fn capitalize_ascii_label(label: &str) -> String {
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}…", truncated)
    } else {
        truncated
    }
}

pub fn terminal_session_label(session_name: &str, display_title: Option<&str>) -> String {
    let fallback = capitalize_ascii_label(session_name);
    let Some(title) = display_title.and_then(normalized_display_title) else {
        return fallback;
    };
    if title.eq_ignore_ascii_case(session_name) || title.eq_ignore_ascii_case(&fallback) {
        return fallback;
    }
    format!("{} ({})", truncate_chars(&title, 48), session_name)
}

/// Resolve the human-authored title used by terminal windows and `/resume`.
/// Explicit renames win over todo/goal-derived titles, which win over the
/// generated session title.
pub fn terminal_display_title_for_id(session_id: &str) -> Option<String> {
    crate::session::Session::load_startup_stub(session_id)
        .ok()
        .and_then(|session| {
            session
                .custom_title
                .filter(|title| !title.trim().is_empty())
                .or_else(|| crate::todo::load_session_title(session_id))
                .or(session.title)
        })
}

/// Build the terminal window title.
///
/// The window title is the one piece of chrome that leaves the terminal (tab
/// bars, window switchers, the menubar label), so it leads with the branded
/// product name and follows with the session label. The icon stays in front to
/// keep the connection/session kind scannable. Labels are normalised and
/// truncated so a long generated title cannot push the brand out of view.
pub fn terminal_window_title(
    icon: &str,
    display_title: Option<&str>,
    fallback_label: Option<&str>,
    is_selfdev: bool,
) -> String {
    let label = display_title
        .or(fallback_label)
        .and_then(normalized_display_title)
        .map(|title| truncate_chars(&title, 48));
    let branded = crate::alphacode_brand::window_title(label.as_deref(), is_selfdev);
    let title = if icon.is_empty() {
        branded
    } else {
        format!("{icon} {branded}")
    };
    crate::output_style::terminal_text(&title).into_owned()
}

/// Fallback window label for a session that has no human-authored title.
///
/// Deliberately **excludes** the product name. [`terminal_window_title`] already
/// leads with the brand, and the call sites used to prepend their own lowercase
/// copy, which produced titles like `AlphaCode · alphacode Fox` — the name
/// twice, the second time misspelled. A remote server still contributes its
/// name so the window says which server the session came from; the default
/// server is named after the product, so repeating it would be redundant.
///
/// Kept here rather than in each caller because the resume-window path, the
/// live TUI title sync and the spawn helpers each carried their own version of
/// this expression and had already drifted apart.
pub fn terminal_fallback_label(server_name: Option<&str>, session_label: &str) -> String {
    match server_name
        .map(str::trim)
        .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case(crate::alphacode_brand::SLUG))
    {
        Some(server) => format!("{}/{}", server.to_lowercase(), session_label),
        None => session_label.to_string(),
    }
}

/// Menu-entry title for one session: icon plus label, deliberately **without**
/// the product name.
///
/// A menubar lists sessions *under the app's own name*, so repeating
/// `AlphaCode ·` on every row spends the row's width on information the menu
/// already shows. The standalone window title keeps the brand; the per-session
/// rows inside a menu do not.
pub fn terminal_session_menu_title(
    icon: &str,
    display_title: Option<&str>,
    is_selfdev: bool,
) -> String {
    let label = display_title
        .and_then(normalized_display_title)
        .map(|title| truncate_chars(&title, 48));
    let suffix = if is_selfdev { " [self-dev]" } else { "" };
    let body = match label {
        Some(label) => format!("{label}{suffix}"),
        None => suffix.trim_start().to_string(),
    };
    let title = match (icon.is_empty(), body.is_empty()) {
        (true, true) => String::new(),
        (true, false) => body,
        (false, true) => icon.to_string(),
        (false, false) => format!("{icon} {body}"),
    };
    crate::output_style::terminal_text(&title).into_owned()
}

pub fn terminal_session_label_for_id(session_id: &str) -> String {
    let session_name = session_name(session_id);
    let display_title = terminal_display_title_for_id(session_id);
    match display_title.as_deref() {
        Some(title) => terminal_session_label(&session_name, Some(title)),
        None => session_name,
    }
}

pub fn set_title(title: impl AsRef<str>) {
    proctitle::set_title(title.as_ref());
    set_killall_process_name();
}

fn set_killall_process_name() {
    #[cfg(target_os = "linux")]
    unsafe {
        let mut name = [0u8; 16];
        let bytes = KILLALL_PROCESS_NAME.as_bytes();
        let len = bytes.len().min(name.len().saturating_sub(1));
        name[..len].copy_from_slice(&bytes[..len]);
        let _ = libc::prctl(libc::PR_SET_NAME, name.as_ptr(), 0, 0, 0);
    }
}

pub fn set_server_title(server_name: &str) {
    set_title(compact_process_title("alphacode:s:", Some(server_name)));
}

pub fn set_client_generic_title(is_selfdev: bool) {
    let prefix = if is_selfdev {
        "alphacode:selfdev"
    } else {
        "alphacode:client"
    };
    set_title(compact_process_title(prefix, None));
}

pub fn set_client_session_title(session_id: &str, is_selfdev: bool) {
    set_client_display_title(&session_name(session_id), is_selfdev);
}

pub fn set_client_display_title(session_name: &str, is_selfdev: bool) {
    let prefix = if is_selfdev {
        "alphacode:d:"
    } else {
        "alphacode:c:"
    };
    set_title(compact_process_title(prefix, Some(session_name)));
}

pub fn set_client_remote_display_title(server_name: &str, session_name: &str, is_selfdev: bool) {
    if server_name.is_empty() || server_name.eq_ignore_ascii_case("alphacode") {
        set_client_display_title(session_name, is_selfdev);
        return;
    }
    let prefix = if is_selfdev {
        "alphacode:d:"
    } else {
        "alphacode:c:"
    };
    set_title(format!("{prefix}{server_name}/{session_name}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::lock_test_env;

    #[test]
    fn terminal_session_label_includes_custom_title_and_short_name() {
        assert_eq!(
            terminal_session_label("fox", Some("Release planning")),
            "Release planning (fox)"
        );
        assert_eq!(terminal_session_label("fox", Some("Fox")), "Fox");
        assert_eq!(terminal_session_label("fox", None), "Fox");
    }

    #[test]
    fn terminal_window_title_leads_with_the_branded_product_name() {
        assert_eq!(
            terminal_window_title(
                "🐙",
                Some("resume window title"),
                Some("alphacode Octopus"),
                false
            ),
            "🐙 AlphaCode \u{00b7} resume window title"
        );
        // With no explicit title the fallback label still rides behind the
        // brand, so the window never loses its identity.
        assert_eq!(
            terminal_window_title("🐙", None, Some("Octopus"), false),
            "🐙 AlphaCode \u{00b7} Octopus"
        );
        assert_eq!(
            terminal_window_title("🐙", None, None, false),
            "🐙 AlphaCode"
        );
        assert_eq!(
            terminal_window_title(
                "🐙",
                Some("resume window title"),
                Some("alphacode Octopus"),
                true
            ),
            "🐙 AlphaCode \u{00b7} resume window title [self-dev]"
        );
        // Window chrome must never fall back to the lowercase slug.
        assert!(!terminal_window_title("", Some("x"), None, false).contains("alphacode"));
    }

    /// The fallback label must not repeat the product name.
    ///
    /// Every caller used to build this string itself as `"alphacode {label}"`,
    /// which `terminal_window_title` then prefixed with the brand — so a real
    /// window read `AlphaCode · alphacode Fox`: the name twice, once in the
    /// wrong case. The helper now returns a bare label for the default server.
    #[test]
    fn fallback_label_never_repeats_the_product_name() {
        assert_eq!(terminal_fallback_label(None, "Fox"), "Fox");
        // The default server is named after the product; repeating it would be
        // the exact duplication this helper exists to prevent.
        assert_eq!(terminal_fallback_label(Some("alphacode"), "Fox"), "Fox");
        assert_eq!(terminal_fallback_label(Some("AlphaCode"), "Fox"), "Fox");
        assert_eq!(terminal_fallback_label(Some("  "), "Fox"), "Fox");
        // Whitespace around a real server name must not leak into the title.
        assert_eq!(
            terminal_fallback_label(Some("  Octopus "), "Fox"),
            "octopus/Fox"
        );
    }

    /// End to end: the composed window title names the product once, and never
    /// in the lowercase slug spelling.
    #[test]
    fn window_title_names_the_product_once_and_in_branded_case() {
        let label = terminal_fallback_label(Some("alphacode"), "Fox");
        let title = terminal_window_title("", None, Some(&label), false);
        assert_eq!(title, "AlphaCode \u{00b7} Fox");
        assert_eq!(title.matches("lphaCode").count(), 1, "brand appears once");
        assert!(
            !title.contains("alphacode"),
            "window chrome must not use the slug: {title:?}"
        );
    }

    #[test]
    fn terminal_session_label_for_id_reads_custom_title_from_session() {
        let _guard = lock_test_env();
        let previous_home = std::env::var_os("ALPHACODE_HOME");
        let temp = tempfile::tempdir().expect("temp dir");
        crate::alphacode_core::env::set_var("ALPHACODE_HOME", temp.path());

        let mut session = crate::session::Session::create_with_id(
            "session_fox_123".to_string(),
            None,
            Some("Generated title".to_string()),
        );
        session.rename_title(Some("Release planning".to_string()));
        session.save().expect("save session");

        assert_eq!(
            terminal_session_label_for_id("session_fox_123"),
            "Release planning (fox)"
        );

        if let Some(previous_home) = previous_home {
            crate::alphacode_core::env::set_var("ALPHACODE_HOME", previous_home);
        } else {
            crate::alphacode_core::env::remove_var("ALPHACODE_HOME");
        }
    }

    #[test]
    fn terminal_session_label_for_id_prefers_todo_title_over_generated_title() {
        let _guard = lock_test_env();
        let previous_home = std::env::var_os("ALPHACODE_HOME");
        let temp = tempfile::tempdir().expect("temp dir");
        crate::alphacode_core::env::set_var("ALPHACODE_HOME", temp.path());

        let session_id = "session_fox_456";
        let mut session = crate::session::Session::create_with_id(
            session_id.to_string(),
            None,
            Some("Generated title".to_string()),
        );
        session.save().expect("save session");
        crate::todo::save_todos(
            session_id,
            &[crate::todo::TodoItem {
                content: "Synchronize terminal window names".to_string(),
                status: "in_progress".to_string(),
                priority: "high".to_string(),
                id: "window-title".to_string(),
                group: Some("resume title sync".to_string()),
                confidence: Some(90),
                completion_confidence: None,
                confidence_history: Vec::new(),
                blocked_by: Vec::new(),
                assigned_to: None,
            }],
        )
        .expect("save todos");

        assert_eq!(
            terminal_session_label_for_id(session_id),
            "resume title sync (fox)"
        );

        if let Some(previous_home) = previous_home {
            crate::alphacode_core::env::set_var("ALPHACODE_HOME", previous_home);
        } else {
            crate::alphacode_core::env::remove_var("ALPHACODE_HOME");
        }
    }
}
