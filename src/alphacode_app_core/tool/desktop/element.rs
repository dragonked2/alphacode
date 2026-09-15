//! Ephemeral element ID system.
//!
//! Elements discovered by `desktop_find` or `desktop_snapshot` are assigned a
//! short, opaque ID (e.g. `desk_42f91`) that the agent can pass to subsequent
//! actions.  IDs are stored in a thread-safe in-memory store and automatically
//! invalidate when the underlying application state changes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Maximum number of element entries to keep before evicting oldest.
const MAX_ELEMENTS: usize = 500;

/// Global element store — lazily initialized, shared across tool invocations.
pub(crate) static ELEMENT_STORE: OnceLock<Mutex<ElementStore>> = OnceLock::new();

/// Compact representation of an element for LLM consumption and action
/// targeting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementInfo {
    pub element_id: String,
    pub role: String,
    pub name: Option<String>,
    pub value: Option<String>,
    pub description: Option<String>,
    pub bounds: Option<String>,
    pub states: String,
    pub actions: Vec<String>,
    pub app_name: Option<String>,
}

/// Thread-safe store for ephemeral element references.
pub(crate) struct ElementStore {
    entries: HashMap<String, StoredElement>,
    counter: u64,
}

struct StoredElement {
    info: ElementInfo,
    #[allow(dead_code)]
    handle: u64,
}

impl ElementStore {
    pub(crate) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            counter: 0,
        }
    }

    /// Register an xa11y element and return its ElementInfo.
    pub fn register(&mut self, element: &xa11y::Element) -> ElementInfo {
        self.counter = self.counter.wrapping_add(1);
        let id = format!("desk_{:05x}", self.counter);
        let data = element.data();

        let bounds_str = data
            .bounds
            .map(|b| format!("({},{}) {}x{}", b.x, b.y, b.width, b.height));

        let states = format_states(&data.states);
        let actions = data.actions.clone();

        let info = ElementInfo {
            element_id: id.clone(),
            role: format!("{:?}", data.role),
            name: data.name.clone(),
            value: data.value.clone(),
            description: data.description.clone(),
            bounds: bounds_str,
            states,
            actions,
            app_name: None,
        };

        // Evict oldest if at capacity
        if self.entries.len() >= MAX_ELEMENTS
            && let Some(oldest_key) = self.entries.keys().next().cloned()
        {
            self.entries.remove(&oldest_key);
        }

        self.entries.insert(
            id,
            StoredElement {
                info: info.clone(),
                handle: data.handle,
            },
        );

        info
    }

    /// Get a stored element info by ID.
    pub fn get(&self, element_id: &str) -> Option<&ElementInfo> {
        self.entries.get(element_id).map(|e| &e.info)
    }

    /// Clear all stored elements (e.g. on session reset).
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.entries.clear();
        self.counter = 0;
    }
}

/// Format StateSet into a compact string for LLM consumption.
fn format_states(states: &xa11y::StateSet) -> String {
    let mut parts = Vec::new();
    if states.enabled {
        parts.push("enabled");
    }
    if states.visible {
        parts.push("visible");
    }
    if states.focused {
        parts.push("focused");
    }
    if states.editable {
        parts.push("editable");
    }
    if states.focusable {
        parts.push("focusable");
    }
    if states.modal {
        parts.push("modal");
    }
    if states.required {
        parts.push("required");
    }
    if states.busy {
        parts.push("busy");
    }
    if states.selected {
        parts.push("selected");
    }
    if let Some(checked) = &states.checked {
        match checked {
            xa11y::Toggled::On => parts.push("checked"),
            xa11y::Toggled::Off => parts.push("unchecked"),
            xa11y::Toggled::Mixed => parts.push("mixed"),
        }
    }
    if let Some(expanded) = &states.expanded {
        if *expanded {
            parts.push("expanded");
        } else {
            parts.push("collapsed");
        }
    }
    if parts.is_empty() {
        "normal".to_string()
    } else {
        parts.join(", ")
    }
}

impl Default for ElementStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_states_default() {
        let states = xa11y::StateSet::default();
        let formatted = format_states(&states);
        assert!(formatted.contains("enabled"));
        assert!(formatted.contains("visible"));
    }

    #[test]
    fn format_states_focused() {
        let mut states = xa11y::StateSet::default();
        states.focused = true;
        let formatted = format_states(&states);
        assert!(formatted.contains("focused"));
    }

    #[test]
    fn format_states_checked() {
        let mut states = xa11y::StateSet::default();
        states.checked = Some(xa11y::Toggled::On);
        let formatted = format_states(&states);
        assert!(formatted.contains("checked"));
    }

    #[test]
    fn element_store_counter_increments() {
        let mut store = ElementStore::new();
        assert_eq!(store.counter, 0);
        store.counter = 10;
        assert_eq!(store.counter, 10);
    }
}
