use crate::alphacode_provider_core::selection::TaskKind;
use crate::alphacode_tui::tui::color_support::rgb;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Model usage statistics for intelligent sorting
#[derive(Clone, Debug, Default)]
pub struct ModelUsageStats {
    /// Timestamp of last usage (seconds since epoch)
    pub last_used: u64,
    /// Total number of times this model was used
    pub usage_count: u32,
    /// Average response time in milliseconds
    pub avg_response_ms: u64,
    /// Whether this model is marked as favorite
    pub is_favorite: bool,
    /// Provider name for this model
    pub provider: String,
    /// Context window size in tokens
    pub context_window: Option<u64>,
    /// Whether the model supports tools
    pub supports_tools: bool,
    /// Model capability tier (e.g., "premium", "standard", "fast")
    pub tier: ModelTier,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModelTier {
    Premium,
    #[default]
    Standard,
    Fast,
    Free,
}

impl ModelTier {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Premium => "💎",
            Self::Standard => "⚡",
            Self::Fast => "🚀",
            Self::Free => "🆓",
        }
    }
}

/// Smart model picker with recent usage tracking and intelligent sorting
pub struct SmartModelPicker {
    /// Model usage statistics
    stats: Arc<Mutex<HashMap<String, ModelUsageStats>>>,
    /// Recently used models (in order of use)
    recent_models: Arc<Mutex<Vec<String>>>,
    /// Favorite models
    favorites: Arc<Mutex<Vec<String>>>,
    /// Maximum number of recent models to track
    max_recent: usize,
    /// Maximum number of favorites
    max_favorites: usize,
}

impl SmartModelPicker {
    pub fn new() -> Self {
        Self {
            stats: Arc::new(Mutex::new(HashMap::new())),
            recent_models: Arc::new(Mutex::new(Vec::new())),
            favorites: Arc::new(Mutex::new(Vec::new())),
            max_recent: 10,
            max_favorites: 5,
        }
    }

    /// Record model usage
    pub fn record_usage(&self, model: &str, provider: &str, response_time_ms: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut stats = self
            .stats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = stats.entry(model.to_string()).or_default();
        entry.last_used = now;
        entry.usage_count += 1;
        entry.provider = provider.to_string();

        // Update rolling average response time
        if entry.avg_response_ms == 0 {
            entry.avg_response_ms = response_time_ms;
        } else {
            entry.avg_response_ms = (entry.avg_response_ms + response_time_ms) / 2;
        }

        // Update recent models list
        let mut recent = self
            .recent_models
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        recent.retain(|m| m != model);
        recent.insert(0, model.to_string());
        if recent.len() > self.max_recent {
            recent.truncate(self.max_recent);
        }
    }

    /// Toggle favorite status for a model.
    ///
    /// Returns `true` when the model was *removed* from favorites and
    /// `false` when it was *added*. This matches the semantics callers
    /// expect from a "toggle" verb (true = "I just toggled it OFF").
    pub fn toggle_favorite(&self, model: &str) -> bool {
        let mut favorites = self
            .favorites
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(pos) = favorites.iter().position(|m| m == model) {
            favorites.remove(pos);
            true
        } else {
            if favorites.len() >= self.max_favorites {
                favorites.remove(0);
            }
            favorites.push(model.to_string());
            false
        }
    }

    /// Get sorted models tuned for a specific task kind.
    ///
    /// Combines the existing usage/favorite/recency ranking with the
    /// task-aware model-name + context-window scoring from
    /// []. Pure function: no model calls, no
    /// network I/O.
    pub fn get_sorted_models_for_task(&self, models: Vec<String>, task: TaskKind) -> Vec<String> {
        let stats = self
            .stats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let recent = self
            .recent_models
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let favorites = self
            .favorites
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let mut scored: Vec<(String, f64)> = models
            .into_iter()
            .map(|model| {
                let base = self.calculate_score(&model, &stats, &recent, &favorites);
                let ctx = stats.get(&model).and_then(|s| s.context_window);
                let task_bonus = task.score_model(&model, ctx) as f64;
                (model, base + task_bonus)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().map(|(model, _)| model).collect()
    }

    /// Auto-classify the user's task from free-form text and return the
    /// sorted list. Convenience wrapper.
    pub fn get_sorted_models_for_text(&self, models: Vec<String>, user_text: &str) -> Vec<String> {
        self.get_sorted_models_for_task(models, TaskKind::classify(user_text))
    }

    /// Get sorted models based on usage patterns
    pub fn get_sorted_models(&self, models: Vec<String>) -> Vec<String> {
        let stats = self
            .stats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let recent = self
            .recent_models
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let favorites = self
            .favorites
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let mut scored_models: Vec<(String, f64)> = models
            .into_iter()
            .map(|model| {
                let score = self.calculate_score(&model, &stats, &recent, &favorites);
                (model, score)
            })
            .collect();

        // Sort by score (higher is better)
        scored_models.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored_models.into_iter().map(|(model, _)| model).collect()
    }

    /// Calculate score for a model based on various factors.
    /// Provider-aware: logged-in providers get a significant boost.
    fn calculate_score(
        &self,
        model: &str,
        stats: &HashMap<String, ModelUsageStats>,
        recent: &[String],
        favorites: &[String],
    ) -> f64 {
        let mut score = 0.0;

        // Favorite bonus (highest priority)
        if favorites.iter().any(|m| m == model) {
            score += 1000.0;
        }

        // Recency bonus (exponential decay)
        if let Some(pos) = recent.iter().position(|m| m == model) {
            score += 500.0 / (pos as f64 + 1.0);
        }

        // Usage count bonus (logarithmic)
        if let Some(stats) = stats.get(model) {
            score += (stats.usage_count as f64).log2() * 100.0;

            // Response time bonus (faster is better)
            if stats.avg_response_ms > 0 {
                score += 100.0 / (stats.avg_response_ms as f64 / 1000.0);
            }

            // Provider-aware bonus: boost models from well-known providers
            let provider_lower = stats.provider.to_lowercase();
            if provider_lower.contains("anthropic") || provider_lower.contains("claude") {
                score += 150.0; // Premium provider bonus
            } else if provider_lower.contains("openai") {
                score += 140.0;
            } else if provider_lower.contains("gmi") {
                score += 130.0; // GMI Cloud (free tier)
            } else if provider_lower.contains("openrouter") {
                score += 120.0;
            } else if provider_lower.contains("gemini") {
                score += 135.0;
            }
        }

        // Model tier bonus
        if let Some(stats) = stats.get(model) {
            match stats.tier {
                ModelTier::Premium => score += 200.0,
                ModelTier::Standard => score += 100.0,
                ModelTier::Fast => score += 150.0, // Fast models are often preferred
                ModelTier::Free => score += 80.0,
            }
        }

        // Model name heuristic (prefer shorter, simpler names)
        let name_len = model.len() as f64;
        score += 100.0 / (name_len / 10.0);

        score
    }

    /// Get model statistics
    pub fn get_stats(&self, model: &str) -> Option<ModelUsageStats> {
        self.stats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(model)
            .cloned()
    }

    /// Get recent models
    pub fn get_recent(&self) -> Vec<String> {
        self.recent_models
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Get favorite models
    pub fn get_favorites(&self) -> Vec<String> {
        self.favorites
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Update model metadata
    pub fn update_metadata(
        &self,
        model: &str,
        context_window: Option<u64>,
        supports_tools: bool,
        tier: ModelTier,
    ) {
        let mut stats = self
            .stats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = stats.entry(model.to_string()).or_default();
        entry.context_window = context_window;
        entry.supports_tools = supports_tools;
        entry.tier = tier;
    }
}

impl Default for SmartModelPicker {
    fn default() -> Self {
        Self::new()
    }
}

/// Render model picker with smart sorting and metadata
pub fn render_smart_model_picker(
    models: &[String],
    picker: &SmartModelPicker,
    selected: Option<usize>,
    search_query: &str,
    area: Rect,
    frame: &mut Frame,
) {
    render_smart_model_picker_with_task(models, picker, selected, search_query, None, area, frame)
}

/// Like [], but takes an explicit optional task
/// hint so the picker can rank models by task-fit. Pass
/// (any free-form prompt fragment) to enable task-aware ranking; pass
///  to fall back to usage-based ranking.
pub fn render_smart_model_picker_with_task(
    models: &[String],
    picker: &SmartModelPicker,
    selected: Option<usize>,
    search_query: &str,
    task_hint: Option<&str>,
    area: Rect,
    frame: &mut Frame,
) {
    let sorted_models = match task_hint {
        Some(text) if !text.trim().is_empty() => {
            picker.get_sorted_models_for_text(models.to_vec(), text)
        }
        _ => picker.get_sorted_models(models.to_vec()),
    };

    // Filter models based on search query.
    // Strategy: case-insensitive substring match first (covers the common
    // "claude-opus" / "gpt-5" case), then fall back to fuzzy match for
    // typos so "haik" finds "claude-haiku-3-5".
    let filtered_models: Vec<&String> = if search_query.is_empty() {
        sorted_models.iter().collect()
    } else {
        let q = search_query.to_ascii_lowercase();
        sorted_models
            .iter()
            .filter(|model| {
                let m = model.to_ascii_lowercase();
                if m.contains(&q) {
                    return true;
                }
                // Subsequence match: every char in the query appears in the
                // model name in order. Cheap and catches "haik" -> "haiku".
                let mut hay = m.chars();
                for needle_ch in q.chars() {
                    if !hay.by_ref().any(|c| c == needle_ch) {
                        return false;
                    }
                }
                true
            })
            .collect()
    };

    let items: Vec<ListItem> = filtered_models
        .iter()
        .enumerate()
        .map(|(idx, model)| {
            let is_selected = selected == Some(idx);
            let stats = picker.get_stats(model);
            let is_favorite = picker.get_favorites().contains(model);

            let mut spans = vec![];

            // Favorite indicator
            if is_favorite {
                spans.push(Span::styled(
                    "★ ",
                    Style::default()
                        .fg(rgb(255, 215, 0))
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled("  ", Style::default()));
            }

            // Model tier
            if let Some(stats) = &stats {
                spans.push(Span::styled(
                    format!("{} ", stats.tier.display_name()),
                    Style::default().fg(rgb(180, 180, 180)),
                ));
            } else {
                spans.push(Span::styled("  ", Style::default()));
            }

            // Gradient-colored model name
            let gradient = crate::alphacode_tui::tui::brand_ux::BrandTheme::gradient();
            let name_style = if is_selected {
                Style::default()
                    .fg(gradient[5]) // teal for selected
                    .add_modifier(Modifier::BOLD)
            } else if is_favorite {
                Style::default()
                    .fg(gradient[12]) // rose for favorites
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(rgb(220, 225, 240)) // brighter default
            };
            spans.push(Span::styled(model.to_string(), name_style));

            // Usage count indicator
            if let Some(stats) = &stats
                && stats.usage_count > 0
            {
                spans.push(Span::styled(
                    format!(" ({})", stats.usage_count),
                    Style::default().fg(rgb(120, 120, 120)),
                ));
            }

            // Context window indicator with gradient color
            if let Some(stats) = &stats
                && let Some(ctx) = stats.context_window
            {
                let ctx_display = if ctx >= 1_000_000 {
                    format!("{}M", ctx / 1_000_000)
                } else if ctx >= 1_000 {
                    format!("{}k", ctx / 1_000)
                } else {
                    ctx.to_string()
                };
                // Color based on context size: larger = more capable
                let ctx_color = if ctx >= 200_000 {
                    rgb(100, 220, 160) // green for large context
                } else if ctx >= 100_000 {
                    rgb(120, 200, 220) // cyan for medium context
                } else {
                    rgb(180, 160, 200) // purple for small context
                };
                spans.push(Span::styled(
                    format!(" [{}]", ctx_display),
                    Style::default().fg(ctx_color),
                ));
            }

            // Provider-aware gradient-colored indicator
            if let Some(stats) = &stats
                && !stats.provider.is_empty()
            {
                let provider_lower = stats.provider.to_lowercase();
                let provider_color =
                    if provider_lower.contains("anthropic") || provider_lower.contains("claude") {
                        rgb(118, 166, 255) // blue for Anthropic
                    } else if provider_lower.contains("openai") {
                        rgb(134, 233, 180) // green for OpenAI
                    } else if provider_lower.contains("gmi") {
                        rgb(130, 224, 215) // teal for GMI Cloud
                    } else if provider_lower.contains("openrouter") {
                        rgb(200, 140, 255) // purple for OpenRouter
                    } else if provider_lower.contains("gemini") {
                        rgb(255, 195, 88) // amber for Gemini
                    } else {
                        rgb(140, 150, 170) // default dim
                    };
                spans.push(Span::styled(
                    format!(" ({})", stats.provider),
                    Style::default().fg(provider_color),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    // Gradient-colored border and highlight for the model picker
    let gradient = crate::alphacode_tui::tui::brand_ux::BrandTheme::gradient();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(Span::styled(
                    match task_hint {
                        Some(text) if !text.trim().is_empty() => {
                            let kind = TaskKind::classify(text);
                            format!(" Model Picker [task: {}] ", kind.as_str())
                        }
                        _ => " Model Picker ".to_string(),
                    },
                    Style::default()
                        .fg(gradient[4])
                        .add_modifier(Modifier::BOLD),
                ))
                .title_bottom(Line::from(Span::styled(
                    " f: favorite | /: search | Enter: select ",
                    Style::default().fg(gradient[3]).add_modifier(Modifier::DIM),
                )))
                .border_style(Style::default().fg(gradient[4])),
        )
        .highlight_style(
            Style::default()
                .bg(rgb(35, 40, 55))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(list, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smart_model_picker_creation() {
        let picker = SmartModelPicker::new();
        assert!(picker.get_recent().is_empty());
        assert!(picker.get_favorites().is_empty());
    }

    #[test]
    fn test_record_usage() {
        let picker = SmartModelPicker::new();
        picker.record_usage("claude-3-opus", "anthropic", 1000);

        let stats = picker.get_stats("claude-3-opus").unwrap();
        assert_eq!(stats.usage_count, 1);
        assert_eq!(stats.provider, "anthropic");
    }

    #[test]
    fn test_toggle_favorite() {
        let picker = SmartModelPicker::new();
        assert!(!picker.toggle_favorite("claude-3-opus"));
        assert!(
            picker
                .get_favorites()
                .contains(&"claude-3-opus".to_string())
        );
        assert!(picker.toggle_favorite("claude-3-opus"));
        assert!(
            !picker
                .get_favorites()
                .contains(&"claude-3-opus".to_string())
        );
    }

    #[test]
    fn test_sorting_by_usage() {
        let picker = SmartModelPicker::new();
        picker.record_usage("model-b", "provider", 500);
        picker.record_usage("model-a", "provider", 1000);
        picker.record_usage("model-b", "provider", 500);

        let models = vec![
            "model-a".to_string(),
            "model-b".to_string(),
            "model-c".to_string(),
        ];
        let sorted = picker.get_sorted_models(models);

        // model-b should be first (most used)
        assert_eq!(sorted[0], "model-b");
    }

    #[test]
    fn test_max_recent_limit() {
        let picker = SmartModelPicker::new();
        for i in 0..15 {
            picker.record_usage(&format!("model-{}", i), "provider", 1000);
        }

        let recent = picker.get_recent();
        assert_eq!(recent.len(), 10); // max_recent is 10
    }

    #[test]
    fn test_task_aware_sorting_promotes_reasoning_models() {
        let picker = SmartModelPicker::new();
        let models = vec![
            "claude-haiku-3-5".to_string(),
            "claude-opus-4-5".to_string(),
            "gpt-4o-mini".to_string(),
            "gpt-5".to_string(),
        ];

        // Security task should rank opus (reasoning) above haiku (fast).
        let sorted = picker.get_sorted_models_for_task(models.clone(), TaskKind::SecurityAnalysis);
        let pos_opus = sorted.iter().position(|m| m == "claude-opus-4-5").unwrap();
        let pos_haiku = sorted.iter().position(|m| m == "claude-haiku-3-5").unwrap();
        assert!(
            pos_opus < pos_haiku,
            "opus should outrank haiku for security: {sorted:?}"
        );

        // Simple-question task should rank haiku (fast) above opus.
        let sorted_simple = picker.get_sorted_models_for_task(models, TaskKind::SimpleQuestion);
        let pos_opus2 = sorted_simple
            .iter()
            .position(|m| m == "claude-opus-4-5")
            .unwrap();
        let pos_haiku2 = sorted_simple
            .iter()
            .position(|m| m == "claude-haiku-3-5")
            .unwrap();
        assert!(
            pos_haiku2 < pos_opus2,
            "haiku should outrank opus for simple Q&A: {sorted_simple:?}"
        );
    }

    #[test]
    fn test_text_classification_routes_correctly() {
        let picker = SmartModelPicker::new();
        let models = vec![
            "claude-opus-4-5".to_string(),
            "claude-haiku-3-5".to_string(),
        ];
        let sorted =
            picker.get_sorted_models_for_text(models, "audit this code for XSS vulnerabilities");
        assert_eq!(sorted[0], "claude-opus-4-5");
    }
}
