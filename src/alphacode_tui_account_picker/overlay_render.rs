use super::*;
use crate::alphacode_tui_style::{Role, role_color};

pub(super) fn hotkey(text: &'static str) -> Span<'static> {
    Span::styled(
        text,
        Style::default()
            .fg(role_color(Role::AiText))
            .bg(role_color(Role::CodeBg)),
    )
}

pub(super) fn provider_header_line(
    provider_label: &str,
    account_count: usize,
    secondary_count: usize,
    provider_id: &str,
) -> Line<'static> {
    let summary = if account_count > 0 {
        format!(
            "  -  {}  -  {} other",
            account_count_summary(account_count),
            secondary_count
        )
    } else {
        format!(
            "  -  {} control{}",
            secondary_count,
            if secondary_count == 1 { "" } else { "s" }
        )
    };
    Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(provider_label.to_string(), provider_style(provider_id)),
        Span::styled(summary, Style::default().fg(muted_dark())),
    ])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ActionSection {
    Switch,
    Add,
    Login,
    Overview,
    Setting,
    Remove,
    Other,
}

pub(super) fn action_section(item: &AccountPickerItem) -> ActionSection {
    match &item.command {
        AccountPickerCommand::OpenAccountCenter { .. } => ActionSection::Overview,
        AccountPickerCommand::OpenAddReplaceFlow { .. } => ActionSection::Add,
        AccountPickerCommand::Switch { .. } => ActionSection::Switch,
        AccountPickerCommand::Login { .. } => ActionSection::Login,
        AccountPickerCommand::Remove { .. } => ActionSection::Remove,
        AccountPickerCommand::PromptNew { .. } => ActionSection::Add,
        AccountPickerCommand::PromptValue { .. } => ActionSection::Setting,
        AccountPickerCommand::SubmitInput(input) if input.contains(" switch ") => {
            ActionSection::Switch
        }
        AccountPickerCommand::SubmitInput(input) if input.contains(" remove ") => {
            ActionSection::Remove
        }
        AccountPickerCommand::SubmitInput(input) if input.ends_with(" settings") => {
            ActionSection::Overview
        }
        AccountPickerCommand::SubmitInput(input) if input.ends_with(" login") => {
            ActionSection::Login
        }
        AccountPickerCommand::SubmitInput(input) if input.contains(" add") => ActionSection::Add,
        AccountPickerCommand::SubmitInput(_) => ActionSection::Other,
    }
}

pub(super) fn account_is_active(item: &AccountPickerItem) -> bool {
    item.subtitle
        .split(['·', '-'])
        .any(|part| part.trim().eq_ignore_ascii_case("active"))
}

fn extract_account_label(title: &str) -> Option<String> {
    let prefixes = ["Switch account `", "Re-login account `", "Remove account `"];
    for prefix in prefixes {
        if let Some(rest) = title.strip_prefix(prefix)
            && let Some(label) = rest.strip_suffix('`')
        {
            return Some(label.to_string());
        }
    }
    None
}

pub(super) fn compact_item_title(item: &AccountPickerItem) -> String {
    match action_section(item) {
        ActionSection::Switch => {
            extract_account_label(&item.title).unwrap_or_else(|| item.title.clone())
        }
        ActionSection::Add => item.title.clone(),
        ActionSection::Login => extract_account_label(&item.title)
            .map(|label| format!("Refresh {label}"))
            .unwrap_or_else(|| "Login / refresh".to_string()),
        ActionSection::Overview => "Provider settings".to_string(),
        ActionSection::Remove => extract_account_label(&item.title)
            .map(|label| format!("Remove {label}"))
            .unwrap_or_else(|| item.title.clone()),
        ActionSection::Setting | ActionSection::Other => item.title.clone(),
    }
}

pub(super) fn action_icon(item: &AccountPickerItem) -> (&'static str, Color) {
    match action_section(item) {
        ActionSection::Switch => (
            if account_is_active(item) { "*" } else { "o" },
            if account_is_active(item) {
                role_color(Role::Success)
            } else {
                role_color(Role::MutedText)
            },
        ),
        ActionSection::Add => ("+", role_color(Role::Accent)),
        ActionSection::Login => ("R", role_color(Role::Accent)),
        ActionSection::Overview => ("S", role_color(Role::Accent)),
        ActionSection::Setting => (".", role_color(Role::Accent)),
        ActionSection::Remove => ("x", role_color(Role::Error)),
        ActionSection::Other => ("-", role_color(Role::MutedText)),
    }
}

pub(super) fn account_count_summary(count: usize) -> String {
    format!(
        "{} saved account{}",
        count,
        if count == 1 { "" } else { "s" }
    )
}

pub(super) fn action_kind_label(command: &AccountPickerCommand) -> &'static str {
    crate::alphacode_tui_account_picker::action_kind_label(command)
}

pub(super) fn action_kind_badge(command: &AccountPickerCommand) -> (&'static str, Color) {
    match action_kind_label(command) {
        "overview" => ("overview", role_color(Role::Info)),
        "login" => ("login", role_color(Role::Success)),
        "setting" => ("setting", role_color(Role::Accent)),
        "danger" => ("remove", role_color(Role::Error)),
        "account" => ("account", role_color(Role::Accent)),
        _ => ("action", role_color(Role::MutedText)),
    }
}

pub(super) fn action_kind_help(command: &AccountPickerCommand) -> &'static str {
    match command {
        AccountPickerCommand::OpenAccountCenter { .. } => {
            "Returns to the main account center with all provider and saved-auth actions."
        }
        AccountPickerCommand::OpenAddReplaceFlow { .. } => {
            "Opens a focused chooser where you pick whether to add a new Claude/OpenAI account or replace an existing saved one."
        }
        AccountPickerCommand::SubmitInput(input) if input.ends_with(" settings") => {
            "Opens a detailed text summary for this provider, including the exact commands you can run manually."
        }
        AccountPickerCommand::SubmitInput(input) if input.contains(" remove ") => {
            "Removes saved credentials for the selected account. Use this when an account is stale or should no longer be available in alphacode."
        }
        AccountPickerCommand::SubmitInput(input) if input.contains(" login") => {
            "Starts or refreshes authentication for this provider so it becomes usable again."
        }
        AccountPickerCommand::SubmitInput(input) if input.contains(" add") => {
            "Starts the flow for adding the next numbered account, so you can keep multiple identities side by side."
        }
        AccountPickerCommand::SubmitInput(input) if input.contains(" switch ") => {
            "Makes this account active so future requests use it immediately."
        }
        AccountPickerCommand::PromptValue { .. } => {
            "Prompts for a new value, then saves the matching provider or global setting."
        }
        AccountPickerCommand::Switch { .. } => {
            "Switches the active saved account for this provider."
        }
        AccountPickerCommand::Login { .. } => {
            "Refreshes the selected account by starting the provider login flow again."
        }
        AccountPickerCommand::Remove { .. } => {
            "Deletes the saved account credentials from local storage."
        }
        AccountPickerCommand::PromptNew { .. } => {
            "Starts login for the next numbered account immediately."
        }
        AccountPickerCommand::SubmitInput(_) => {
            "Runs the selected account-management command immediately."
        }
    }
}

pub(super) fn command_preview(command: &AccountPickerCommand) -> String {
    match command {
        AccountPickerCommand::SubmitInput(input) => input.clone(),
        AccountPickerCommand::OpenAccountCenter { provider_filter } => match provider_filter {
            Some(provider_id) => format!("Open /account {}", provider_id),
            None => "Open /account".to_string(),
        },
        AccountPickerCommand::OpenAddReplaceFlow { provider_filter } => match provider_filter {
            Some(provider_id) => format!("Open add/replace flow for {}", provider_id),
            None => "Open add/replace flow".to_string(),
        },
        AccountPickerCommand::PromptValue {
            command_prefix,
            empty_value,
            ..
        } => match empty_value {
            Some(value) => format!("{} <value>  (special: {} )", command_prefix, value),
            None => format!("{} <value>", command_prefix),
        },
        AccountPickerCommand::Switch { provider, label } => match provider {
            AccountProviderKind::Anthropic => format!("/account switch {}", label),
            AccountProviderKind::OpenAi => format!("/account openai switch {}", label),
        },
        AccountPickerCommand::Login { provider, label } => match provider {
            AccountProviderKind::Anthropic => format!("/account claude add {}", label),
            AccountProviderKind::OpenAi => format!("/account openai add {}", label),
        },
        AccountPickerCommand::Remove { provider, label } => match provider {
            AccountProviderKind::Anthropic => format!("/account claude remove {}", label),
            AccountProviderKind::OpenAi => format!("/account openai remove {}", label),
        },
        AccountPickerCommand::PromptNew { provider } => match provider {
            AccountProviderKind::Anthropic => "/account claude add".to_string(),
            AccountProviderKind::OpenAi => "/account openai add".to_string(),
        },
    }
}

pub(super) fn metric_span(label: &'static str, value: usize, color: Color) -> Span<'static> {
    Span::styled(
        format!("{} {}", label, value),
        Style::default().fg(color).bold(),
    )
}

pub(super) fn provider_style(_provider_id: &str) -> Style {
    Style::default().fg(role_color(Role::Accent)).bold()
}

pub(super) fn truncate_with_ellipsis(input: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let display_width = unicode_width::UnicodeWidthStr::width(input);
    if display_width <= width {
        return input.to_string();
    }
    if width <= 3 {
        return ".".repeat(width);
    }
    let mut out = String::new();
    let mut current_width = 0;
    for ch in input.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_width + ch_width > width - 3 {
            break;
        }
        out.push(ch);
        current_width += ch_width;
    }
    out.push_str("...");
    out
}

pub(super) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let percent_x = percent_x.min(100);
    let percent_y = percent_y.min(100);
    let popup = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup[1])[1]
}
