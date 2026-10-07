use super::info_widget::{InfoWidgetData, is_traceworthy_memory_event};

pub(crate) const MAX_TODO_LINES: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InfoPageKind {
    CompactOnly,
    TodosExpanded,
    MemoryExpanded,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct InfoPage {
    pub kind: InfoPageKind,
    pub height: u16,
}

pub(crate) struct PageLayout {
    pub pages: Vec<InfoPage>,
    pub max_page_height: u16,
    pub show_dots: bool,
}

pub(crate) fn compute_page_layout(
    data: &InfoWidgetData,
    _inner_width: usize,
    inner_height: u16,
) -> PageLayout {
    let compact_height = compact_overview_height(data);
    if compact_height == 0 {
        return PageLayout {
            pages: Vec::new(),
            max_page_height: 0,
            show_dots: false,
        };
    }

    // Always keep the compact Session card useful in a small margin pocket.
    // The renderer prioritizes the model and combined context/usage/memory row,
    // then clips lower-priority details when the pocket is short.
    if compact_height > inner_height {
        return PageLayout {
            pages: vec![InfoPage {
                kind: InfoPageKind::CompactOnly,
                height: inner_height,
            }],
            max_page_height: inner_height,
            show_dots: false,
        };
    }

    let mut candidates: Vec<InfoPage> = Vec::new();
    let todos_compact = compact_todos_height(data);

    let todos_expanded = expanded_todos_height(data);
    if todos_expanded > 0 {
        candidates.push(InfoPage {
            kind: InfoPageKind::TodosExpanded,
            height: compact_height - todos_compact + todos_expanded,
        });
    }

    let memory_expanded = expanded_memory_height(data);
    if memory_expanded > 0 {
        let telemetry_compact = compact_telemetry_height(data, true);
        let telemetry_without_memory = compact_telemetry_height(data, false);
        candidates.push(InfoPage {
            kind: InfoPageKind::MemoryExpanded,
            height: compact_height - telemetry_compact + telemetry_without_memory + memory_expanded,
        });
    }

    let mut pages: Vec<InfoPage> = candidates
        .into_iter()
        .filter(|page| page.height <= inner_height)
        .collect();

    if pages.is_empty() {
        if compact_height <= inner_height {
            pages.push(InfoPage {
                kind: InfoPageKind::CompactOnly,
                height: compact_height,
            });
        } else {
            return PageLayout {
                pages,
                max_page_height: 0,
                show_dots: false,
            };
        }
    }

    let mut show_dots = false;
    if pages.len() > 1 {
        let filtered: Vec<InfoPage> = pages
            .iter()
            .copied()
            .filter(|page| page.height < inner_height)
            .collect();
        if filtered.len() > 1 {
            pages = filtered;
            show_dots = true;
        } else if filtered.len() == 1 {
            pages = filtered;
        }
    }

    let max_page_height = pages
        .iter()
        .map(|page| page.height + u16::from(show_dots))
        .max()
        .unwrap_or(0);

    PageLayout {
        pages,
        max_page_height,
        show_dots,
    }
}

fn compact_queue_height(data: &InfoWidgetData) -> u16 {
    u16::from(data.queue_mode.is_some())
}

fn compact_todos_height(data: &InfoWidgetData) -> u16 {
    if data.todos.is_empty() { 0 } else { 2 }
}

fn compact_model_height(data: &InfoWidgetData) -> u16 {
    u16::from(data.model.is_some())
}

fn compact_telemetry_height(data: &InfoWidgetData, include_memory: bool) -> u16 {
    let has_context = data.context_info_stale
        || data
            .context_info
            .as_ref()
            .is_some_and(|info| info.total_chars > 0)
        || data.observed_context_tokens.is_some();
    let has_usage = data.usage_info.as_ref().is_some_and(|info| info.available);
    let has_memory = include_memory
        && data
            .memory_info
            .as_ref()
            .is_some_and(|info| info.should_render());
    u16::from(has_context || has_usage || has_memory)
}

fn compact_background_height(data: &InfoWidgetData) -> u16 {
    if let Some(info) = &data.background_info
        && info.running_count > 0
    {
        let task_lines = info.running_tasks.len().min(3) as u16;
        let overflow_line = u16::from(info.running_tasks.len() > 3);
        return 1 + task_lines + overflow_line;
    }
    0
}

fn compact_kv_cache_height(data: &InfoWidgetData) -> u16 {
    if data.cache_hit_info.is_some() { 1 } else { 0 }
}

fn compact_compaction_height(data: &InfoWidgetData) -> u16 {
    if data.compaction_info.is_some() { 2 } else { 0 }
}

fn compact_git_height(data: &InfoWidgetData) -> u16 {
    if let Some(info) = &data.git_info
        && info.is_interesting()
    {
        return 1;
    }
    0
}

fn compact_overview_height(data: &InfoWidgetData) -> u16 {
    compact_model_height(data)
        + compact_queue_height(data)
        + compact_telemetry_height(data, true)
        + compact_todos_height(data)
        + compact_background_height(data)
        + compact_kv_cache_height(data)
        + compact_compaction_height(data)
        + compact_git_height(data)
}

fn expanded_todos_height(data: &InfoWidgetData) -> u16 {
    if data.todos.is_empty() {
        return 0;
    }

    let available_lines = MAX_TODO_LINES.saturating_sub(1);
    let todo_lines = data.todos.len().min(available_lines);
    let mut height = 1 + u16::try_from(todo_lines).unwrap_or(u16::MAX);
    if data.todos.len() > available_lines {
        height += 1;
    }
    height
}

fn expanded_memory_height(data: &InfoWidgetData) -> u16 {
    if let Some(info) = &data.memory_info
        && info.should_render()
    {
        let mut height = 1u16;
        if info.should_show_activity() {
            height += 1 + 4;
            if let Some(activity) = &info.activity
                && activity
                    .recent_events
                    .iter()
                    .any(is_traceworthy_memory_event)
            {
                height += 1;
            }
        }
        return height;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::{InfoPageKind, compute_page_layout};
    use crate::alphacode_tui::prompt::ContextInfo;
    use crate::alphacode_tui::todo::TodoItem;
    use crate::alphacode_tui::tui::info_widget::{
        InfoWidgetData, MemoryInfo, WidgetKind, is_overview_mergeable,
    };
    use std::collections::HashMap;

    #[test]
    fn compute_page_layout_falls_back_to_compact_page() {
        let data = InfoWidgetData {
            model: Some("gpt-test".to_string()),
            queue_mode: Some(true),
            ..Default::default()
        };

        let layout = compute_page_layout(&data, 40, 8);

        assert_eq!(layout.pages.len(), 1);
        assert_eq!(layout.pages[0].kind, InfoPageKind::CompactOnly);
        assert!(!layout.show_dots);
    }

    #[test]
    fn compute_page_layout_keeps_multiple_expanded_pages_when_height_allows() {
        let data = InfoWidgetData {
            todos: vec![TodoItem {
                group: None,
                content: "ship refactor".to_string(),
                status: "pending".to_string(),
                priority: "high".to_string(),
                id: "todo-1".to_string(),
                blocked_by: Vec::new(),
                assigned_to: None,
                confidence: None,
                completion_confidence: None,
                confidence_history: Vec::new(),
            }],
            memory_info: Some(MemoryInfo {
                total_count: 3,
                project_count: 2,
                global_count: 1,
                by_category: HashMap::from([("fact".to_string(), 3usize)]),
                sidecar_model: Some("openai · gpt-5.3-codex-spark".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let layout = compute_page_layout(&data, 40, 8);

        assert!(layout.pages.len() >= 2);
        assert!(layout.show_dots);
        assert!(
            layout
                .pages
                .iter()
                .any(|page| page.kind == InfoPageKind::TodosExpanded)
        );
        assert!(
            layout
                .pages
                .iter()
                .any(|page| page.kind == InfoPageKind::MemoryExpanded)
        );
    }

    #[test]
    fn session_overview_combines_model_context_and_memory() {
        let data = InfoWidgetData {
            model: Some("gpt-test".to_string()),
            context_info: Some(ContextInfo {
                total_chars: 1_200,
                ..Default::default()
            }),
            memory_info: Some(MemoryInfo {
                total_count: 3,
                project_count: 2,
                global_count: 1,
                ..Default::default()
            }),
            ..Default::default()
        };

        assert!(data.has_data_for(WidgetKind::Overview));
        assert!(is_overview_mergeable(WidgetKind::ContextUsage));
        assert!(is_overview_mergeable(WidgetKind::MemoryActivity));
        assert!(is_overview_mergeable(WidgetKind::UsageLimits));
        let layout = compute_page_layout(&data, 40, 5);
        assert!(layout.max_page_height >= 2);
    }

    #[test]
    fn compact_overview_survives_a_short_margin_pocket() {
        let data = InfoWidgetData {
            model: Some("gpt-test".to_string()),
            context_info: Some(ContextInfo {
                total_chars: 1_200,
                ..Default::default()
            }),
            queue_mode: Some(true),
            memory_info: Some(MemoryInfo {
                total_count: 3,
                project_count: 2,
                global_count: 1,
                ..Default::default()
            }),
            ..Default::default()
        };

        let layout = compute_page_layout(&data, 40, 2);

        assert_eq!(layout.pages.len(), 1);
        assert_eq!(layout.pages[0].kind, InfoPageKind::CompactOnly);
        assert_eq!(layout.max_page_height, 2);
    }
}
