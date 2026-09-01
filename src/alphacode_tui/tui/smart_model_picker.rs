use crate::alphacode_tui::tui::color_support::rgb;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, BorderType, List, ListItem};
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

    /// Toggle favorite status for a model
    pub fn toggle_favorite(&self, model: &str) -> bool {
        let mut favorites = self
            .favorites
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(pos) = favorites.iter().position(|m| m == model) {
            favorites.remove(pos);
            false
        } else {
            if favorites.len() >= self.max_favorites {
                favorites.remove(0);
            }
            favorites.push(model.to_string());
            true
        }
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

    /// Calculate score for a model based on various factors
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
        }

        // Model name heuristic (prefer shorter, simpler names)
        let name_len = model.len() as f64;
        score += 100.0 / (name_len / 10.0);

        score
    }

    /// Get model statistics
    pub fn get_stats(&self, model: &str) -> Option<ModelUsageStats> {
        self
            .stats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()).get(model).cloned()
    }

    /// Get recent models
    pub fn get_recent(&self) -> Vec<String> {
        self
            .recent_models
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    /// Get favorite models
    pub fn get_favorites(&self) -> Vec<String> {
        self
            .favorites
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
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
    let sorted_models = picker.get_sorted_models(models.to_vec());
    
    // Filter models based on search query
    let filtered_models: Vec<&String> = if search_query.is_empty() {
        sorted_models.iter().collect()
    } else {
        sorted_models
            .iter()
            .filter(|model| model.to_lowercase().contains(&search_query.to_lowercase()))
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
                    Style::default().fg(rgb(255, 215, 0)).add_modifier(Modifier::BOLD),
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
            
            // Model name
            let name_style = if is_selected {
                Style::default()
                    .fg(rgb(100, 255, 180))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(rgb(220, 220, 220))
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
            
            // Context window indicator
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
                spans.push(Span::styled(
                    format!(" [{}]", ctx_display),
                    Style::default().fg(rgb(100, 150, 200)),
                ));
            }
            
            // Provider indicator
            if let Some(stats) = &stats
                && !stats.provider.is_empty()
            {
                spans.push(Span::styled(
                    format!(" ({})", stats.provider),
                    Style::default().fg(rgb(120, 120, 120)),
                ));
            }
            
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Model Picker ")
                .title_bottom(Line::from(Span::styled(
                    " f: favorite | /: search | Enter: select ",
                    Style::default().fg(rgb(100, 100, 100)),
                )))
                .border_style(Style::default().fg(rgb(100, 100, 100))),
        )
        .highlight_style(Style::default().bg(rgb(40, 44, 52)).add_modifier(Modifier::BOLD));

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
        assert!(picker.get_favorites().contains(&"claude-3-opus".to_string()));
        assert!(picker.toggle_favorite("claude-3-opus"));
        assert!(!picker.get_favorites().contains(&"claude-3-opus".to_string()));
    }

    #[test]
    fn test_sorting_by_usage() {
        let picker = SmartModelPicker::new();
        picker.record_usage("model-b", "provider", 500);
        picker.record_usage("model-a", "provider", 1000);
        picker.record_usage("model-b", "provider", 500);
        
        let models = vec!["model-a".to_string(), "model-b".to_string(), "model-c".to_string()];
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
}
