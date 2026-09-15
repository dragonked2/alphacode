//! Compact snapshot formatting for LLM consumption.
//!
//! Converts the accessibility tree into a structured, context-efficient
//! representation that the agent can reason over without consuming excessive
//! tokens.

use super::backend::{SnapshotResult, WindowSnapshot};
use super::element::ElementInfo;
use super::security;

/// Format a snapshot result into a compact, readable string.
pub fn format_snapshot(result: &SnapshotResult, depth: usize, interactive_only: bool) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "Application: {} ({} window(s), {} element(s))\n",
        result.application_name,
        result.windows.len(),
        result.element_count
    ));

    if result.windows.is_empty() {
        lines.push("No windows found. The application may not have visible windows.".to_string());
        return lines.join("");
    }

    for window in &result.windows {
        lines.push(format_window(window, depth, interactive_only));
    }

    // Add usage hint
    lines.push(String::new());
    lines.push(
        "Use element_id from above with desktop_click/type/focus to interact. \
         Use desktop_find with role/name to search for specific elements."
            .to_string(),
    );

    lines.join("\n")
}

fn format_window(window: &WindowSnapshot, _depth: usize, _interactive_only: bool) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Window: \"{}\"\n", window.title));

    if window.elements.is_empty() {
        lines.push("  (no elements)\n".to_string());
        return lines.join("");
    }

    // Group elements by role for readability
    let mut role_order: Vec<String> = Vec::new();

    for elem in &window.elements {
        if !role_order.contains(&elem.role) {
            role_order.push(elem.role.clone());
        }
    }

    // Prioritize interactive roles
    let priority_roles = [
        "Button",
        "TextField",
        "TextArea",
        "ComboBox",
        "CheckBox",
        "RadioButton",
        "Link",
        "Tab",
        "MenuItem",
        "Switch",
        "Slider",
    ];

    let mut sorted_roles: Vec<String> = role_order.to_vec();
    sorted_roles.sort_by(|a, b| {
        let a_prio = priority_roles.iter().position(|p| p == a).unwrap_or(999);
        let b_prio = priority_roles.iter().position(|p| p == b).unwrap_or(999);
        a_prio.cmp(&b_prio)
    });

    for role in &sorted_roles {
        let elems: Vec<&ElementInfo> = window.elements.iter().filter(|e| &e.role == role).collect();

        for elem in &elems {
            // Sanitize element names to prevent prompt injection via UI text
            let name = elem
                .name
                .as_deref()
                .map(security::sanitize_for_llm)
                .unwrap_or_default();
            let id = &elem.element_id;

            // Compact format: [role] name (id)
            let line = if name.is_empty() {
                format!("  [{role}] ({id})")
            } else {
                format!("  [{role}] {name} ({id})")
            };

            lines.push(line);

            // Add value if present and different from name
            if let Some(ref value) = elem.value
                && Some(value.as_str()) != elem.name.as_deref()
            {
                lines.push(format!("    value: {}", truncate(value, 80)));
            }

            // Add description if present
            if let Some(ref desc) = elem.description {
                lines.push(format!("    desc: {}", truncate(desc, 80)));
            }
        }
    }

    lines.push(String::new());
    lines.join("\n")
}

/// Truncate a string to max_chars, adding "..." if truncated.
fn truncate(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_app_core::tool::desktop::backend::SnapshotResult;

    #[test]
    fn format_empty_snapshot() {
        let result = SnapshotResult {
            application_name: "TestApp".to_string(),
            windows: vec![],
            element_count: 0,
        };
        let formatted = format_snapshot(&result, 8, false);
        assert!(formatted.contains("TestApp"));
        assert!(formatted.contains("No windows"));
    }

    #[test]
    fn format_snapshot_with_elements() {
        let result = SnapshotResult {
            application_name: "TestApp".to_string(),
            windows: vec![WindowSnapshot {
                title: "Main Window".to_string(),
                elements: vec![
                    ElementInfo {
                        element_id: "desk_00001".to_string(),
                        role: "Button".to_string(),
                        name: Some("Save".to_string()),
                        value: None,
                        description: None,
                        bounds: None,
                        states: "enabled".to_string(),
                        actions: vec!["press".to_string()],
                        app_name: None,
                    },
                    ElementInfo {
                        element_id: "desk_00002".to_string(),
                        role: "TextField".to_string(),
                        name: Some("Search".to_string()),
                        value: None,
                        description: Some("Enter search query".to_string()),
                        bounds: None,
                        states: "enabled, focused".to_string(),
                        actions: vec!["type_text".to_string()],
                        app_name: None,
                    },
                ],
            }],
            element_count: 2,
        };
        let formatted = format_snapshot(&result, 8, false);
        assert!(formatted.contains("Main Window"));
        assert!(formatted.contains("[Button] Save"));
        assert!(formatted.contains("[TextField] Search"));
        assert!(formatted.contains("desk_00001"));
    }

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        let result = truncate("this is a very long string that should be truncated", 20);
        assert!(result.len() <= 23); // 20 + "..."
        assert!(result.ends_with("..."));
    }
}
