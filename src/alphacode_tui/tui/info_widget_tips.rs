use super::*;

const TIP_CYCLE_SECONDS: u64 = 15;
const STATUS_TIP_PERIOD_SECONDS: u64 = 90;
const STATUS_TIP_OFFSET_SECONDS: u64 = 28;
const STATUS_TIP_SHOW_SECONDS: u64 = 12;

struct Tip {
    text: String,
}

fn all_tips(mermaid: bool) -> Vec<Tip> {
    let mut tips = vec![
        "Ctrl+J / Ctrl+K to jump between user prompts (Cmd+J / Cmd+K on macOS terminals that forward Command)",
        "Ctrl+Shift+J / Ctrl+Shift+K to scroll the chat down and up one line",
        "Ctrl+G to bookmark your scroll position - press again to teleport back",
        "Swarms form automatically when multiple sessions share a repo - they coordinate plans, share context, and track file conflicts",
        "Memories are stored in a graph with semantic embeddings - recall finds related facts even if you use different words",
        "Ambient mode runs background cycles while you're away - maintaining memories, compacting context, and doing proactive work",
        "Ambient cycles can email you a summary and you can reply with directives for the next run",
        "Alt+B moves a long-running tool to the background - the agent continues and can check on it later with the `bg` tool",
        "Most terminals can be configured to copy text on highlight - no Ctrl+C needed. Check your terminal's settings for 'copy on select'",
        "Alt+G (or /diff) cycles diff mode: Off, Inline, Pinned, File. Shift+Tab cycles favorited models. Pinned shows all diffs in a side pane. File shows the full file with changes highlighted, synced to your scroll position",
        "/usage opens a live usage overlay - compare every provider's headroom and reset times in one view",
        "Ctrl+R searches your prompt history; ↑↓ selects, ↵ inserts, Esc cancels",
        "Type `!` before a command to run it as a shell command right from the composer",
        "The ⌁ line under the input tracks your session tokens, cost, and compactions at a glance",
        "Context usage turns amber near 50% and red near 80% - keep an eye on the meter on the right",
        "Sessions auto-compact as context fills up, and /clear always starts a fresh window",
        "The info dock lists todos, memory activity, and usage limits whenever the right margin has room",
        "Skills are reusable instruction packs: /skills shows what's installed and how to find more",
        "Session facts on the right show provider, model, git branch, and live context usage",
        "/set model <name> is a quick way to switch models, e.g. /set model qwen/qwen3.8-max-free",
        "TokenRouter offers free model access - use `alphacode --provider tokenrouter` to get started",
    ];
    if mermaid {
        tips.insert(3, "```mermaid code blocks render as diagrams");
    }
    // Mac keyboards label this modifier ⌥, not Alt, so rewrite hints there.
    let alt = crate::alphacode_tui_core::keybind::alt_label();
    tips.into_iter()
        .map(|text| Tip {
            text: text.replace("Alt+", &format!("{alt}+")),
        })
        .collect()
}

static TIP_STATE: Mutex<Option<(usize, Instant)>> = Mutex::new(None);
static TIPS: std::sync::OnceLock<Vec<Tip>> = std::sync::OnceLock::new();
static TIPS_WITH_MERMAID: std::sync::OnceLock<Vec<Tip>> = std::sync::OnceLock::new();

fn current_tip(_max_width: usize) -> Tip {
    let tips = if crate::config::config().features.mermaid {
        TIPS_WITH_MERMAID.get_or_init(|| all_tips(true))
    } else {
        TIPS.get_or_init(|| all_tips(false))
    };
    let mut guard = TIP_STATE.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    let (idx, last) = guard.get_or_insert_with(|| (0, now));

    let should_advance = now.duration_since(*last).as_secs() >= TIP_CYCLE_SECONDS;
    if should_advance {
        *idx = (*idx + 1) % tips.len();
        *last = now;
    }

    let i = *idx % tips.len();
    drop(guard);
    Tip {
        text: tips[i].text.clone(),
    }
}

pub(crate) fn occasional_status_tip(max_width: usize, elapsed_secs: u64) -> Option<String> {
    if max_width < 16 {
        return None;
    }

    let cycle_pos = elapsed_secs % STATUS_TIP_PERIOD_SECONDS;
    let show_until = STATUS_TIP_OFFSET_SECONDS + STATUS_TIP_SHOW_SECONDS;
    if cycle_pos < STATUS_TIP_OFFSET_SECONDS || cycle_pos >= show_until {
        return None;
    }

    let prefix = "💡 ";
    let available = max_width.saturating_sub(UnicodeWidthStr::width(prefix));
    if available < 12 {
        return None;
    }

    let tip = current_tip(available);
    Some(format!(
        "{}{}",
        prefix,
        truncate_smart(&tip.text, available)
    ))
}

fn wrap_tip_text(text: &str, width: usize) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;

    if width == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut remaining = text.trim_start();
    while !remaining.is_empty() {
        let mut end = 0;
        let mut used = 0;
        let mut last_space = None;
        for (index, grapheme) in remaining.grapheme_indices(true) {
            let cells = UnicodeWidthStr::width(grapheme);
            if used + cells > width {
                break;
            }
            used += cells;
            end = index + grapheme.len();
            if grapheme.chars().all(char::is_whitespace) {
                last_space = Some(index);
            }
        }
        if end == remaining.len() {
            lines.push(remaining.trim_end().to_string());
            break;
        }
        if end == 0 {
            // A wide grapheme cannot fit in a one-cell column. Consume it so
            // narrow terminals cannot trap the renderer in an infinite loop.
            let grapheme = remaining.graphemes(true).next().unwrap();
            lines.push("�".to_string());
            remaining = remaining[grapheme.len()..].trim_start();
            continue;
        }
        let split = last_space.filter(|&index| index > 0).unwrap_or(end);
        lines.push(remaining[..split].trim_end().to_string());
        remaining = remaining[split..].trim_start();
    }
    lines
}

pub(super) fn tips_widget_height(inner_width: usize) -> u16 {
    let effective_w = inner_width.saturating_sub(2);
    let tip = current_tip(effective_w);
    let lines = wrap_tip_text(&tip.text, effective_w);
    1 + lines.len() as u16
}

pub(super) fn render_tips_widget(inner: Rect) -> Vec<Line<'static>> {
    let w = inner.width.saturating_sub(2) as usize;
    let tip = current_tip(w);
    let wrapped = wrap_tip_text(&tip.text, w);

    let gradient = crate::alphacode_tui::tui::brand_ux::BrandTheme::gradient();
    let mut lines: Vec<Line<'static>> = Vec::new();
    // Gradient-accented tip header with sparkle
    lines.push(Line::from(vec![
        Span::styled("\u{2728} ", Style::default().fg(gradient[7])),
        Span::styled(
            "Tip",
            Style::default()
                .fg(gradient[4])
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", Style::default()),
        Span::styled("\u{2502}", Style::default().fg(gradient[3])),
    ]));

    for line_text in wrapped {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(line_text, Style::default().fg(rgb(175, 180, 200))),
        ]));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tips_wrap_on_terminal_cells_and_make_progress() {
        assert_eq!(wrap_tip_text("ab 界 cd", 4), vec!["ab", "界", "cd"]);
        assert_eq!(wrap_tip_text("界a", 1), vec!["�", "a"]);
        assert_eq!(wrap_tip_text("e\u{301}x", 1), vec!["e\u{301}", "x"]);
        assert!(wrap_tip_text("anything", 0).is_empty());
    }
}
