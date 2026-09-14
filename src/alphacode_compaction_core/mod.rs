use crate::alphacode_message_types::{ContentBlock, Message, Role};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

/// Default token budget (200k tokens - matches Claude's actual context limit)
pub const DEFAULT_TOKEN_BUDGET: usize = 200_000;

/// Trigger compaction at this percentage of budget
pub const COMPACTION_THRESHOLD: f32 = 0.85;

/// If context is above this threshold when compaction starts, do a synchronous
/// hard-compact (drop old messages) so the API call doesn't fail.
pub const CRITICAL_THRESHOLD: f32 = 0.95;

/// Minimum threshold for manual compaction (can compact at any time above this)
pub const MANUAL_COMPACT_MIN_THRESHOLD: f32 = 0.10;

/// Keep this many recent turns verbatim (not summarized). Increased from 15
/// to 20 to preserve more context during compaction, reducing information loss
/// and improving accuracy on multi-step tasks.
pub const RECENT_TURNS_TO_KEEP: usize = 20;

/// Absolute minimum turns to keep during emergency compaction
pub const MIN_TURNS_TO_KEEP: usize = 2;

/// Max chars for a single tool result during emergency truncation. Increased
/// from 8000 to 10000 to preserve more tool output context during compaction.
pub const EMERGENCY_TOOL_RESULT_MAX_CHARS: usize = 10_000;

/// Max chars to keep for an inline image payload during emergency recovery.
/// Images are usually base64 screenshots; at hard-threshold time the useful
/// state should be represented by nearby tool text/summary, not by replaying a
/// huge raw image in the recent tail.
pub const EMERGENCY_IMAGE_MAX_CHARS: usize = 1024;

/// Approximate maximum request body size (in base64 characters) we aim to keep
/// the transcript under when recovering from a provider "request too large" /
/// 413 payload error. Anthropic rejects requests whose serialized body exceeds
/// roughly 32 MB; this distinct failure mode is driven almost entirely by inline
/// base64 images, which the normal token-budget accounting deliberately
/// undercounts (see `IMAGE_TOKEN_COST`). We target a conservative budget well
/// under the hard provider cap so a single retry reliably fits.
pub const PAYLOAD_IMAGE_CHAR_BUDGET: usize = 12 * 1024 * 1024;

/// Approximate chars per token for estimation
pub const CHARS_PER_TOKEN: usize = 4;

/// Approximate token cost charged for a single inline image.
///
/// Image content blocks carry base64-encoded payloads that are often hundreds
/// of kilobytes. Counting that raw base64 length as message text (len / 4)
/// massively overestimates the real context cost: providers tokenize images by
/// resolution, not by transport-encoded byte length, and a typical screenshot
/// costs on the order of ~1-2k tokens regardless of base64 size. Using the raw
/// length caused the token estimate to balloon far above the real
/// provider-observed input, spuriously tripping the compaction threshold and
/// driving repeated back-to-back ("triple") compactions that could not bring
/// the estimate down because the images stayed in the recent kept turns.
///
/// We charge a flat, slightly conservative per-image token budget instead.
pub const IMAGE_TOKEN_COST: usize = 1_600;

/// Fixed token overhead for system prompt + tool definitions.
/// These are not counted in message content but do count toward the context limit.
/// Estimated conservatively: ~8k tokens for system prompt + ~10k for 50+ tools.
pub const SYSTEM_OVERHEAD_TOKENS: usize = 18_000;

/// Rolling window size for token history (proactive/semantic modes)
pub const TOKEN_HISTORY_WINDOW: usize = 20;

/// Maximum characters to embed per message (first N chars capture semantic content)
pub const EMBED_MAX_CHARS_PER_MSG: usize = 512;

/// Rolling window of per-turn embeddings used for topic-shift detection
pub const EMBEDDING_HISTORY_WINDOW: usize = 10;

/// Per-manager semantic embedding cache capacity.
pub const SEMANTIC_EMBED_CACHE_CAPACITY: usize = 256;

/// Cosine similarity threshold for detecting topic shifts (lower = more sensitive)
pub const TOPIC_SHIFT_THRESHOLD: f32 = 0.3;

/// Minimum messages between topic-shift detections (prevents noise)
pub const TOPIC_SHIFT_MIN_MESSAGES: usize = 3;

/// Importance score thresholds for message retention
pub const IMPORTANCE_CRITICAL: f32 = 0.8;
pub const IMPORTANCE_HIGH: f32 = 0.6;
pub const IMPORTANCE_MEDIUM: f32 = 0.4;
pub const IMPORTANCE_LOW: f32 = 0.2;

/// Maximum ratio of important messages to keep during compaction
pub const IMPORTANT_MESSAGE_KEEP_RATIO: f32 = 0.3;

/// Adaptive threshold adjustment factor based on task complexity
pub const ADAPTIVE_THRESHOLD_STEP: f32 = 0.05;

/// Maximum adaptive threshold upper bound
pub const ADAPTIVE_THRESHOLD_MAX: f32 = 0.95;

/// Minimum adaptive threshold lower bound
pub const ADAPTIVE_THRESHOLD_MIN: f32 = 0.70;

/// Rolling window for tracking compaction quality metrics
pub const QUALITY_METRICS_WINDOW: usize = 10;

pub const SUMMARY_PROMPT: &str = r#"Summarize our conversation so you can continue this work later. Write in natural language with these sections:

- **Context:** What we're working on and why (1-2 sentences)
- **What we did:** Key actions taken, files changed, problems solved (use file paths only, no content)
- **Current state:** What works, what's broken, what's next
- **User preferences:** Specific requirements or decisions they made
- **Key decisions:** Important architectural or design choices
- **Files modified:** Bullet list of all files created/edited/deleted (just paths)
- **Lessons learned:** Important insights, gotchas, or patterns discovered
- **Active tasks:** Unfinished work that needs to continue

Rules:
- Be concise: aim for 200-400 words total. Every word should earn its place.
- Preserve user-stated constraints/requirements VERBATIM — these are highest priority.
- Do NOT include code snippets, error messages, or verbose output. Reference file paths instead.
- Do NOT include tool call details or intermediate reasoning — focus on outcomes.
- If a file path was referenced multiple times, mention it once with what changed.
- Focus on information needed to CONTINUE the work, not to relive it.
- Preserve ALL decision rationale — explain WHY choices were made, not just what was chosen.
- Keep any debugging insights or workarounds that might be needed again."#;

/// Enhanced summary prompt for topic-aware compaction with multiple topics
pub const TOPIC_AWARE_SUMMARY_PROMPT: &str = r#"Summarize our conversation, organized by distinct topics we discussed. For each topic section, include:

**Topic: [Name]**
- What we were trying to accomplish
- Key decisions and reasoning
- Files touched and changes made
- Current status

Then add:
- **Cross-cutting concerns:** Decisions or patterns that apply across topics
- **User preferences:** Requirements that span multiple topics
- **Unfinished work:** Tasks that need continuation across topics

Rules:
- Group related messages by topic, not chronologically
- Preserve ALL user-stated constraints VERBATIM
- Keep decision rationale (WHY, not just what)
- Be concise: aim for 300-500 words total
- Reference file paths, not content
- Focus on what's needed to CONTINUE the work"#;

/// A completed summary covering turns up to a certain point
#[derive(Debug, Clone)]
pub struct Summary {
    pub text: String,
    pub openai_encrypted_content: Option<String>,
    pub covers_up_to_turn: usize,
    pub original_turn_count: usize,
}

/// Event emitted when compaction is applied
#[derive(Debug, Clone)]
pub struct CompactionEvent {
    pub trigger: String,
    pub pre_tokens: Option<u64>,
    pub post_tokens: Option<u64>,
    pub tokens_saved: Option<u64>,
    pub duration_ms: Option<u64>,
    pub messages_dropped: Option<usize>,
    pub messages_compacted: Option<usize>,
    pub summary_chars: Option<usize>,
    pub active_messages: Option<usize>,
}

/// What happened when ensure_context_fits was called
#[derive(Debug, Clone, PartialEq)]
pub enum CompactionAction {
    /// Nothing needed, context is fine.
    None,
    /// Background summarization started.
    BackgroundStarted { trigger: String },
    /// Emergency hard compact performed. Contains number of messages dropped.
    HardCompacted(usize),
}

/// Stats about compaction state
#[derive(Debug, Clone)]
pub struct CompactionStats {
    pub total_turns: usize,
    pub active_messages: usize,
    pub has_summary: bool,
    pub is_compacting: bool,
    pub token_estimate: usize,
    pub effective_tokens: usize,
    pub observed_input_tokens: Option<u64>,
    pub context_usage: f32,
}

/// Detected topic shift in conversation flow
#[derive(Debug, Clone)]
pub struct TopicShift {
    /// Index where the topic shift occurred
    pub message_index: usize,
    /// Similarity score before the shift (higher = more similar to previous)
    pub pre_shift_similarity: f32,
    /// Similarity score after the shift (lower = more different from previous)
    pub post_shift_similarity: f32,
    /// Magnitude of the shift (difference between pre and post)
    pub shift_magnitude: f32,
    /// Whether this shift is significant enough to warrant separate summarization
    pub is_significant: bool,
}

/// Importance score for a message
#[derive(Debug, Clone)]
pub struct ImportanceScore {
    /// Overall importance score (0.0 - 1.0)
    pub score: f32,
    /// Whether this message contains user decisions
    pub has_decisions: bool,
    /// Whether this message contains errors or debugging
    pub has_errors: bool,
    /// Whether this message contains tool results
    pub has_tool_results: bool,
    /// Whether this message is a user preference/constraint
    pub has_preferences: bool,
    /// Reason for the score
    pub reason: String,
}

/// Quality metrics for compaction
#[derive(Debug, Clone)]
pub struct CompactionQuality {
    /// Estimated information loss ratio (0.0 - 1.0, lower is better)
    pub information_loss: f32,
    /// Ratio of important content preserved
    pub important_content_preserved: f32,
    /// Number of topic shifts detected
    pub topic_shifts_detected: usize,
    /// Average importance score of kept messages
    pub avg_importance_kept: f32,
    /// Average importance score of dropped messages
    pub avg_importance_dropped: f32,
    /// Whether the summary quality is acceptable
    pub summary_quality_acceptable: bool,
    /// Recommended action based on quality analysis
    pub recommended_action: QualityAction,
}

/// Recommended action based on quality analysis
#[derive(Debug, Clone, PartialEq)]
pub enum QualityAction {
    /// Compaction quality is acceptable
    Acceptable,
    /// Keep more messages due to high importance content
    KeepMoreMessages,
    /// Use topic-aware summarization due to topic shifts
    UseTopicAwareSummarization,
    /// Skip compaction due to critical content
    SkipCompaction,
}

/// Adaptive compaction thresholds based on context usage patterns
#[derive(Debug, Clone)]
pub struct AdaptiveThresholds {
    /// Current compaction threshold (adjusted based on usage patterns)
    pub compaction_threshold: f32,
    /// Current critical threshold
    pub critical_threshold: f32,
    /// Recent context usage history
    pub usage_history: Vec<f32>,
    /// Whether we're in a complex task (more conservative compaction)
    pub is_complex_task: bool,
    /// Task complexity score (0.0 - 1.0)
    pub task_complexity: f32,
    /// Number of compactions in current session
    pub compaction_count: usize,
}

impl AdaptiveThresholds {
    /// Create new adaptive thresholds with defaults
    pub fn new() -> Self {
        Self {
            compaction_threshold: COMPACTION_THRESHOLD,
            critical_threshold: CRITICAL_THRESHOLD,
            usage_history: Vec::new(),
            is_complex_task: false,
            task_complexity: 0.5,
            compaction_count: 0,
        }
    }

    /// Update thresholds based on context usage pattern
    pub fn update(&mut self, context_usage: f32) {
        self.usage_history.push(context_usage);
        if self.usage_history.len() > TOKEN_HISTORY_WINDOW {
            self.usage_history.remove(0);
        }

        // Detect rapid context growth (indicates complex task)
        if self.usage_history.len() >= 3 {
            let recent = &self.usage_history[self.usage_history.len() - 3..];
            let growth_rate = recent[2] - recent[0];
            self.is_complex_task = growth_rate > 0.2;
            self.task_complexity = (growth_rate * 2.0).min(1.0);
        }

        // Adjust thresholds based on task complexity
        let adjustment = if self.is_complex_task {
            // Be more conservative with complex tasks
            -ADAPTIVE_THRESHOLD_STEP * self.task_complexity
        } else {
            // Normal tasks can be more aggressive
            ADAPTIVE_THRESHOLD_STEP * 0.5
        };

        self.compaction_threshold = (COMPACTION_THRESHOLD + adjustment)
            .clamp(ADAPTIVE_THRESHOLD_MIN, ADAPTIVE_THRESHOLD_MAX);

        self.critical_threshold =
            (self.compaction_threshold + 0.10).clamp(ADAPTIVE_THRESHOLD_MIN, 0.98);
    }

    /// Record a compaction event
    pub fn record_compaction(&mut self) {
        self.compaction_count += 1;
    }

    /// Get recommended recent turns to keep based on task complexity
    pub fn recommended_recent_turns(&self) -> usize {
        if self.is_complex_task {
            // Keep more turns for complex tasks
            (RECENT_TURNS_TO_KEEP as f32 * 1.5) as usize
        } else {
            RECENT_TURNS_TO_KEEP
        }
    }
}

impl Default for AdaptiveThresholds {
    fn default() -> Self {
        Self::new()
    }
}

/// Topic detector for identifying conversation topic shifts
#[derive(Debug, Clone)]
pub struct TopicDetector {
    /// Rolling window of message semantic hashes
    pub recent_hashes: Vec<u64>,
    /// Detected topic shifts
    pub topic_shifts: Vec<TopicShift>,
    /// Current topic index
    pub current_topic: usize,
}

impl TopicDetector {
    pub fn new() -> Self {
        Self {
            recent_hashes: Vec::new(),
            topic_shifts: Vec::new(),
            current_topic: 0,
        }
    }

    /// Add a new message hash and detect topic shifts
    pub fn add_message(&mut self, semantic_hash: u64) -> bool {
        self.recent_hashes.push(semantic_hash);
        if self.recent_hashes.len() > EMBEDDING_HISTORY_WINDOW {
            self.recent_hashes.remove(0);
        }

        // Need at least a few messages to detect shifts
        if self.recent_hashes.len() < TOPIC_SHIFT_MIN_MESSAGES {
            return false;
        }

        // Calculate similarity between recent and older messages
        let recent_avg = self.average_recent_hash();
        let older_avg = self.average_older_hash();

        // Simple hash-based similarity (not perfect but lightweight)
        let similarity = hash_similarity(recent_avg, older_avg);

        // Detect significant shift
        if similarity < TOPIC_SHIFT_THRESHOLD {
            let shift = TopicShift {
                message_index: self.recent_hashes.len(),
                pre_shift_similarity: similarity,
                post_shift_similarity: similarity,
                shift_magnitude: 1.0 - similarity,
                is_significant: true,
            };
            self.topic_shifts.push(shift);
            self.current_topic += 1;
            return true;
        }

        false
    }

    /// Get all detected significant topic shifts
    pub fn get_topic_shifts(&self) -> &[TopicShift] {
        &self.topic_shifts
    }

    /// Check if there are significant topic shifts
    pub fn has_topic_shifts(&self) -> bool {
        self.topic_shifts.iter().any(|s| s.is_significant)
    }

    /// Get the number of distinct topics detected
    pub fn topic_count(&self) -> usize {
        self.current_topic + 1
    }

    fn average_recent_hash(&self) -> u64 {
        if self.recent_hashes.is_empty() {
            return 0;
        }
        let recent = &self.recent_hashes[self.recent_hashes.len().saturating_sub(3)..];
        recent.iter().sum::<u64>() / recent.len() as u64
    }

    fn average_older_hash(&self) -> u64 {
        if self.recent_hashes.len() < TOPIC_SHIFT_MIN_MESSAGES {
            return 0;
        }
        let older = &self.recent_hashes[..self.recent_hashes.len() - 3];
        if older.is_empty() {
            return 0;
        }
        older.iter().sum::<u64>() / older.len() as u64
    }
}

impl Default for TopicDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Importance scorer for messages
#[derive(Debug, Clone)]
pub struct ImportanceScorer;

impl ImportanceScorer {
    /// Score a message's importance (0.0 - 1.0)
    pub fn score_message(msg: &Message) -> ImportanceScore {
        let mut score: f32 = 0.0;
        let mut has_decisions = false;
        let mut has_errors = false;
        let mut has_tool_results = false;
        let mut has_preferences = false;
        let mut reasons = Vec::new();

        for block in &msg.content {
            match block {
                ContentBlock::Text { text, .. } => {
                    let lower = text.to_lowercase();

                    // User messages are generally more important
                    if msg.role == Role::User {
                        score += 0.1;
                    }

                    // Check for user preferences and constraints
                    if lower.contains("must ")
                        || lower.contains("should ")
                        || lower.contains("need to ")
                        || lower.contains("required")
                        || lower.contains("constraint")
                        || lower.contains("preference")
                    {
                        score += 0.3;
                        has_preferences = true;
                        reasons.push("user preference/constraint".to_string());
                    }

                    // Check for decision markers
                    if lower.contains("decided ")
                        || lower.contains("choosing ")
                        || lower.contains("going with ")
                        || lower.contains("let's ")
                        || lower.contains("i'll ")
                        || lower.contains("we need to ")
                    {
                        score += 0.25;
                        has_decisions = true;
                        reasons.push("decision".to_string());
                    }

                    // Check for debugging/error context
                    if lower.contains("error")
                        || lower.contains("bug")
                        || lower.contains("fix")
                        || lower.contains("workaround")
                        || lower.contains("hack")
                    {
                        score += 0.2;
                        has_errors = true;
                        reasons.push("error/debugging context".to_string());
                    }
                }
                ContentBlock::ToolUse { name, input, .. } => {
                    // File operations are important
                    if name == "write" || name == "edit" || name == "multiedit" {
                        score += 0.15;
                        if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
                            reasons.push(format!("file modification: {}", path));
                        }
                    }
                    // Search/explore operations are less important
                    if name == "grep" || name == "glob" || name == "read" {
                        score += 0.05;
                    }
                }
                ContentBlock::ToolResult {
                    content, is_error, ..
                } => {
                    has_tool_results = true;
                    // Error results are important
                    if is_error.unwrap_or(false) {
                        score += 0.2;
                        has_errors = true;
                        reasons.push("error result".to_string());
                    } else {
                        score += 0.1;
                    }
                    // Short results are often more important than long ones
                    if content.len() < 200 {
                        score += 0.05;
                    }
                }
                _ => {}
            }
        }

        ImportanceScore {
            score: score.min(1.0),
            has_decisions,
            has_errors,
            has_tool_results,
            has_preferences,
            reason: if reasons.is_empty() {
                "no significant indicators".to_string()
            } else {
                reasons.join("; ")
            },
        }
    }

    /// Score all messages and return sorted by importance
    pub fn score_all_messages(messages: &[Message]) -> Vec<(usize, ImportanceScore)> {
        messages
            .iter()
            .enumerate()
            .map(|(idx, msg)| (idx, Self::score_message(msg)))
            .collect()
    }

    /// Get indices of messages that should be kept based on importance
    pub fn get_important_indices(messages: &[Message], keep_ratio: f32) -> HashSet<usize> {
        let scored = Self::score_all_messages(messages);
        let mut sorted = scored;
        sorted.sort_by(|a, b| b.1.score.partial_cmp(&a.1.score).unwrap());

        let keep_count = (messages.len() as f32 * keep_ratio) as usize;
        let mut indices = HashSet::new();

        for (idx, _) in sorted.iter().take(keep_count) {
            indices.insert(*idx);
        }

        indices
    }
}

/// Compaction quality analyzer
#[derive(Debug, Clone)]
pub struct QualityAnalyzer;

impl QualityAnalyzer {
    /// Analyze the quality of a proposed compaction
    pub fn analyze_quality(
        messages: &[Message],
        kept_indices: &HashSet<usize>,
        topic_shifts: &[TopicShift],
    ) -> CompactionQuality {
        let total = messages.len() as f32;
        let kept = kept_indices.len() as f32;
        let dropped = total - kept;

        // Calculate importance scores
        let all_scores: Vec<f32> = messages
            .iter()
            .map(|msg| ImportanceScorer::score_message(msg).score)
            .collect();

        let kept_avg = if kept_indices.is_empty() {
            0.0
        } else {
            let kept_sum: f32 = kept_indices.iter().map(|&idx| all_scores[idx]).sum();
            kept_sum / kept_indices.len() as f32
        };

        let dropped_avg = if dropped == 0.0 {
            0.0
        } else {
            let dropped_sum: f32 = all_scores
                .iter()
                .enumerate()
                .filter(|(idx, _)| !kept_indices.contains(idx))
                .map(|(_, &score)| score)
                .sum();
            dropped_sum / dropped
        };

        // Estimate information loss
        let information_loss = 1.0 - (kept_avg / total.max(1.0));

        // Important content preservation
        let high_importance_count =
            all_scores.iter().filter(|&&s| s >= IMPORTANCE_HIGH).count() as f32;
        let high_importance_kept = kept_indices
            .iter()
            .filter(|&&idx| all_scores[idx] >= IMPORTANCE_HIGH)
            .count() as f32;

        let important_content_preserved = if high_importance_count == 0.0 {
            1.0
        } else {
            high_importance_kept / high_importance_count
        };

        // Determine recommended action
        let recommended_action = if important_content_preserved < 0.5 {
            QualityAction::KeepMoreMessages
        } else if topic_shifts.iter().any(|s| s.is_significant) {
            QualityAction::UseTopicAwareSummarization
        } else if dropped_avg > IMPORTANCE_MEDIUM && important_content_preserved < 0.7 {
            QualityAction::KeepMoreMessages
        } else {
            QualityAction::Acceptable
        };

        CompactionQuality {
            information_loss,
            important_content_preserved,
            topic_shifts_detected: topic_shifts.iter().filter(|s| s.is_significant).count(),
            avg_importance_kept: kept_avg,
            avg_importance_dropped: dropped_avg,
            summary_quality_acceptable: important_content_preserved >= 0.7,
            recommended_action,
        }
    }

    /// Check if compaction should be skipped based on quality analysis
    pub fn should_skip_compaction(quality: &CompactionQuality) -> bool {
        quality.recommended_action == QualityAction::SkipCompaction
            || quality.important_content_preserved < 0.3
    }
}

/// Calculate similarity between two hashes (0.0 - 1.0, 1.0 = identical)
fn hash_similarity(a: u64, b: u64) -> f32 {
    let xor = a ^ b;
    let bits = xor.count_ones() as f32;
    1.0 - (bits / 64.0)
}

pub fn compacted_summary_text_block(summary: &str) -> String {
    format!("## Previous Conversation Summary\n\n{}\n\n---\n\n", summary)
}

pub fn build_compaction_prompt(
    messages: &[Message],
    existing_summary: Option<&Summary>,
    max_prompt_chars: usize,
) -> String {
    build_compaction_prompt_with_context(messages, existing_summary, max_prompt_chars, None, None)
}

/// Build compaction prompt with topic-aware summarization support
pub fn build_compaction_prompt_with_context(
    messages: &[Message],
    existing_summary: Option<&Summary>,
    max_prompt_chars: usize,
    topic_shifts: Option<&[TopicShift]>,
    adaptive_thresholds: Option<&AdaptiveThresholds>,
) -> String {
    let use_topic_aware = topic_shifts
        .map(|shifts| shifts.iter().any(|s| s.is_significant))
        .unwrap_or(false);

    let prompt_template = if use_topic_aware {
        TOPIC_AWARE_SUMMARY_PROMPT
    } else {
        SUMMARY_PROMPT
    };

    let mut conversation_text = build_compaction_conversation_text(messages, existing_summary);

    // Add topic markers if topic-aware summarization is enabled
    if let Some(shifts) = topic_shifts
        && use_topic_aware
    {
        conversation_text.push_str("\n\n## Topic Boundaries Detected\n\n");
        for (i, shift) in shifts.iter().enumerate() {
            if shift.is_significant {
                conversation_text.push_str(&format!(
                    "- Topic {} starts at message {}\n",
                    i + 1,
                    shift.message_index
                ));
            }
        }
        conversation_text.push('\n');
    }

    // Add adaptive threshold info for the model
    if let Some(thresholds) = adaptive_thresholds
        && thresholds.is_complex_task
    {
        conversation_text.push_str(&format!(
            "\n\n## Task Complexity: {:.0}% (complex task - preserve more detail)\n\n",
            thresholds.task_complexity * 100.0
        ));
    }

    let overhead = prompt_template.len() + 50;
    if conversation_text.len() + overhead > max_prompt_chars && max_prompt_chars > overhead {
        let budget = max_prompt_chars - overhead;
        conversation_text = truncate_str_boundary(&conversation_text, budget).to_string();
        conversation_text
            .push_str("\n\n... [earlier conversation truncated to fit context window]\n");
    }
    format!("{}\n\n---\n\n{}", conversation_text, prompt_template)
}

pub fn build_compaction_conversation_text(
    messages: &[Message],
    existing_summary: Option<&Summary>,
) -> String {
    let mut conversation_text = String::new();
    if let Some(summary) = existing_summary {
        conversation_text.push_str("## Previous Summary\n\n");
        conversation_text.push_str(&summary.text);
        conversation_text.push_str("\n\n## New Conversation\n\n");
    }

    for msg in messages {
        let role_str = match msg.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
        };
        conversation_text.push_str(&format!("**{}:**\n", role_str));
        for block in &msg.content {
            match block {
                ContentBlock::Text { text, .. } => {
                    conversation_text.push_str(text);
                    conversation_text.push('\n');
                }
                ContentBlock::ToolUse { name, input, .. } => {
                    // For file operations, extract the path for better context
                    let summary = if name == "write" || name == "edit" || name == "read" {
                        if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
                            format!("[Tool: {} -> {}]", name, path)
                        } else {
                            format!("[Tool: {} - {}]", name, input)
                        }
                    } else {
                        format!("[Tool: {} - {}]", name, input)
                    };
                    conversation_text.push_str(&summary);
                    conversation_text.push('\n');
                }
                ContentBlock::ToolResult { content, .. } => {
                    // Keep more context for tool results — they often contain
                    // the actual state of the system after an action
                    let truncated = if content.len() > 800 {
                        format!(
                            "{}... (truncated from {} chars)",
                            truncate_str_boundary(content, 800),
                            content.len()
                        )
                    } else {
                        content.clone()
                    };
                    conversation_text.push_str(&format!("[Result: {}]\n", truncated));
                }
                ContentBlock::Reasoning { .. }
                | ContentBlock::ReasoningTrace { .. }
                | ContentBlock::AnthropicThinking { .. }
                | ContentBlock::OpenAIReasoning { .. } => {}
                ContentBlock::Image { .. } => conversation_text.push_str("[Image]\n"),
                ContentBlock::OpenAICompaction { .. } => {
                    conversation_text.push_str("[OpenAI native compaction]\n")
                }
            }
        }
        conversation_text.push('\n');
    }
    conversation_text
}

pub fn truncate_str_boundary(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

pub fn mean_embedding(embeddings: &[&Vec<f32>], dim: usize) -> Vec<f32> {
    let mut mean = vec![0f32; dim];
    for emb in embeddings {
        for (i, v) in emb.iter().enumerate() {
            if i < dim {
                mean[i] += v;
            }
        }
    }
    let n = embeddings.len().max(1) as f32;
    for v in &mut mean {
        *v /= n;
    }
    let norm: f32 = mean.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut mean {
            *v /= norm;
        }
    }
    mean
}

/// Find a safe compaction cutoff that does not leave kept tool results without
/// their corresponding tool calls.
pub fn safe_compaction_cutoff(messages: &[Message], initial_cutoff: usize) -> usize {
    let mut cutoff = initial_cutoff.min(messages.len());

    // Track tool call/result ids in the kept portion.
    let mut available_tool_ids = HashSet::new();
    let mut missing_tool_ids = HashSet::new();

    for msg in &messages[cutoff..] {
        for block in &msg.content {
            match block {
                ContentBlock::ToolUse { id, .. } => {
                    available_tool_ids.insert(id.clone());
                    missing_tool_ids.remove(id);
                }
                ContentBlock::ToolResult { tool_use_id, .. }
                    if !available_tool_ids.contains(tool_use_id) =>
                {
                    missing_tool_ids.insert(tool_use_id.clone());
                }
                _ => {}
            }
        }
    }

    if missing_tool_ids.is_empty() {
        return cutoff;
    }

    // Walk backward once, progressively growing the kept suffix until every
    // kept tool result has its matching tool use in the same suffix.
    for (idx, msg) in messages[..cutoff].iter().enumerate().rev() {
        for block in &msg.content {
            match block {
                ContentBlock::ToolUse { id, .. } => {
                    available_tool_ids.insert(id.clone());
                    missing_tool_ids.remove(id);
                }
                ContentBlock::ToolResult { tool_use_id, .. }
                    if !available_tool_ids.contains(tool_use_id) =>
                {
                    missing_tool_ids.insert(tool_use_id.clone());
                }
                _ => {}
            }
        }
        if missing_tool_ids.is_empty() {
            cutoff = idx;
            return cutoff;
        }
    }

    // If we couldn't find every matching tool call, don't compact at all.
    0
}

pub fn message_char_count(msg: &Message) -> usize {
    content_char_count(&msg.content)
}

pub fn content_char_count(content: &[ContentBlock]) -> usize {
    content
        .iter()
        .map(|block| match block {
            ContentBlock::Text { text, .. } => text.len(),
            ContentBlock::Reasoning { text } => text.len(),
            ContentBlock::ReasoningTrace { text } => text.len(),
            ContentBlock::AnthropicThinking {
                thinking,
                signature,
            } => thinking.len() + signature.len(),
            ContentBlock::OpenAIReasoning {
                id,
                summary,
                encrypted_content,
                status,
            } => {
                id.len()
                    + summary.iter().map(String::len).sum::<usize>()
                    + encrypted_content.as_ref().map(String::len).unwrap_or(0)
                    + status.as_ref().map(String::len).unwrap_or(0)
            }
            ContentBlock::ToolUse { input, .. } => input.to_string().len() + 50,
            ContentBlock::ToolResult { content, .. } => content.len() + 20,
            // Charge a flat token cost for images instead of the raw base64
            // payload length. See IMAGE_TOKEN_COST: counting base64 length here
            // overestimates context by ~100x and triggers spurious repeated
            // compactions.
            ContentBlock::Image { .. } => IMAGE_TOKEN_COST * CHARS_PER_TOKEN,
            ContentBlock::OpenAICompaction { encrypted_content } => encrypted_content.len(),
        })
        .sum()
}

pub fn summary_payload_char_count(summary: &Summary) -> usize {
    summary
        .openai_encrypted_content
        .as_ref()
        .map(|value| value.len())
        .unwrap_or_else(|| summary.text.len())
}

pub fn estimate_compaction_tokens(
    summary: Option<&Summary>,
    active_message_chars: usize,
    token_budget: usize,
) -> usize {
    let summary_chars = summary.map(summary_payload_char_count).unwrap_or(0);
    estimate_compaction_tokens_from_chars(summary_chars + active_message_chars, token_budget)
}

/// Best-effort context size (tokens) from a provider usage report.
///
/// Providers disagree on what `input_tokens` means:
/// - **Split accounting** (Anthropic-style): `input_tokens` is only the
///   *uncached* remainder; cache reads/writes are separate counters, so the
///   real context size is `input + cache_read + cache_creation`.
/// - **Subset accounting** (OpenAI-style): `input_tokens` (`prompt_tokens`)
///   already includes cached tokens; `cached_tokens` is a subset and must NOT
///   be added again.
///
/// This is the single source of truth for that heuristic. Both the sidebar
/// context figure and the compaction manager's observed-token feed must use it
/// so the two never disagree (issue #441). When in doubt, avoid over-counting
/// unless there is strong evidence of split accounting.
pub fn effective_context_tokens_from_usage(
    provider_name: &str,
    input_tokens: u64,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
) -> u64 {
    if input_tokens == 0 {
        return 0;
    }
    let cache_read = cache_read_input_tokens.unwrap_or(0);
    let cache_creation = cache_creation_input_tokens.unwrap_or(0);
    let provider_name = provider_name.to_lowercase();

    let split_cache_accounting = provider_name.contains("anthropic")
        || provider_name.contains("claude")
        || cache_creation > 0
        || cache_read > input_tokens;

    if split_cache_accounting {
        input_tokens
            .saturating_add(cache_read)
            .saturating_add(cache_creation)
    } else {
        input_tokens
    }
}

pub fn estimate_compaction_tokens_from_chars(total_chars: usize, token_budget: usize) -> usize {
    let msg_tokens = total_chars / CHARS_PER_TOKEN;
    // Add overhead for system prompt + tool definitions, which are not in the
    // message list but do count toward the context limit. Scale the overhead to
    // the budget so tests with tiny budgets aren't affected.
    let overhead = if token_budget >= DEFAULT_TOKEN_BUDGET / 2 {
        SYSTEM_OVERHEAD_TOKENS
    } else {
        0
    };
    msg_tokens + overhead
}

pub fn semantic_goal_text(messages: &[Message]) -> String {
    let mut text = String::new();
    for msg in messages {
        for block in &msg.content {
            match block {
                ContentBlock::Text {
                    text: block_text, ..
                } => push_semantic_excerpt(&mut text, block_text, 200),
                ContentBlock::ToolResult { content, .. } => {
                    push_semantic_excerpt(&mut text, content, 100)
                }
                _ => {}
            }
        }
    }
    text
}

pub fn semantic_message_text(msg: &Message) -> String {
    let mut text = String::new();
    for block in &msg.content {
        if let ContentBlock::Text {
            text: block_text, ..
        } = block
        {
            push_semantic_excerpt(&mut text, block_text, EMBED_MAX_CHARS_PER_MSG);
        }
    }
    text
}

pub fn push_semantic_excerpt(target: &mut String, source: &str, max_chars: usize) {
    if source.is_empty() {
        return;
    }
    if !target.is_empty() {
        target.push(' ');
    }
    target.extend(source.chars().take(max_chars));
}

pub fn semantic_cache_key(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

pub fn build_emergency_summary_text(
    existing_summary: Option<&str>,
    dropped_count: usize,
    pre_tokens: u64,
    token_budget: usize,
    dropped_messages: &[Message],
) -> String {
    let mut summary_parts: Vec<String> = Vec::new();

    if let Some(existing) = existing_summary
        && !existing.is_empty()
    {
        summary_parts.push(existing.to_string());
    }

    summary_parts.push(format!(
        "**[Emergency compaction]** {} messages were dropped (~{}k -> {}k tokens). Recent work above.\nDropped artifacts:",
        dropped_count,
        pre_tokens / 1000,
        token_budget / 1000,
    ));

    let mut file_mentions = Vec::new();
    let mut tool_names = HashSet::new();
    let mut user_goals = Vec::new();
    let mut key_decisions = Vec::new();
    let mut lessons_learned = Vec::new();
    for msg in dropped_messages {
        collect_emergency_summary_hints(msg, &mut tool_names, &mut file_mentions);
        collect_user_goals(msg, &mut user_goals);
        collect_key_decisions(msg, &mut key_decisions);
        collect_lessons_learned(msg, &mut lessons_learned);
    }

    if !user_goals.is_empty() {
        user_goals.truncate(5);
        summary_parts.push(format!(
            "User goals in dropped context: {}",
            user_goals.join("; ")
        ));
    }

    if !key_decisions.is_empty() {
        key_decisions.truncate(5);
        summary_parts.push(format!("Key decisions made: {}", key_decisions.join("; ")));
    }

    if !lessons_learned.is_empty() {
        lessons_learned.truncate(3);
        summary_parts.push(format!("Lessons learned: {}", lessons_learned.join("; ")));
    }

    if !tool_names.is_empty() {
        let mut tools: Vec<_> = tool_names.into_iter().collect();
        tools.sort();
        summary_parts.push(format!("Tools used: {}", tools.join(", ")));
    }

    file_mentions.sort();
    file_mentions.dedup();
    if !file_mentions.is_empty() {
        file_mentions.truncate(30);
        // Compact file list: group by directory to save tokens
        let compact = compact_file_list(&file_mentions);
        summary_parts.push(format!("Files: {}", compact));
    }

    summary_parts.join("\n")
}

fn collect_emergency_summary_hints(
    msg: &Message,
    tool_names: &mut HashSet<String>,
    file_mentions: &mut Vec<String>,
) {
    for block in &msg.content {
        match block {
            ContentBlock::ToolUse { name, input, .. } => {
                tool_names.insert(name.clone());
                // Extract file paths from tool inputs for better context
                if let Some(path) = input.get("file_path").and_then(|v| v.as_str())
                    && !file_mentions.contains(&path.to_string())
                {
                    file_mentions.push(path.to_string());
                }
                if let Some(path) = input.get("path").and_then(|v| v.as_str())
                    && !file_mentions.contains(&path.to_string())
                {
                    file_mentions.push(path.to_string());
                }
            }
            ContentBlock::ToolResult { content, .. } => {
                extract_file_mentions(content, file_mentions);
            }
            ContentBlock::Text { text, .. } => {
                extract_file_mentions(text, file_mentions);
            }
            _ => {}
        }
    }
}

/// Extract user goals from user messages in dropped context.
fn collect_user_goals(msg: &Message, goals: &mut Vec<String>) {
    if msg.role != Role::User {
        return;
    }
    for block in &msg.content {
        if let ContentBlock::Text { text, .. } = block {
            let trimmed = text.trim();
            // Extract the first sentence or first 200 chars as a goal summary
            if trimmed.len() > 10 {
                let goal = trimmed.split(['.', '\n']).next().unwrap_or(trimmed).trim();
                if goal.len() > 10 && goal.len() < 200 {
                    goals.push(goal.to_string());
                }
            }
        }
    }
}

/// Extract key decisions from assistant messages (edit/write tool calls).
fn collect_key_decisions(msg: &Message, decisions: &mut Vec<String>) {
    if msg.role != Role::Assistant {
        return;
    }
    for block in &msg.content {
        match block {
            ContentBlock::ToolUse { name, input, .. } => {
                let tool_name = name.as_str();
                if (tool_name == "write" || tool_name == "edit" || tool_name == "multiedit")
                    && let Some(path) = input.get("file_path").and_then(|v| v.as_str())
                {
                    decisions.push(format!("{} {}", tool_name, path));
                }
            }
            ContentBlock::Text { text, .. } => {
                // Look for decision markers in assistant text
                let lower = text.to_lowercase();
                if lower.contains("i'll ")
                    || lower.contains("let me ")
                    || lower.contains("going to ")
                {
                    let snippet = text
                        .split('\n')
                        .find(|line| {
                            let l = line.to_lowercase();
                            l.contains("i'll ") || l.contains("let me ") || l.contains("going to ")
                        })
                        .unwrap_or(text)
                        .trim();
                    if snippet.len() > 10 && snippet.len() < 150 {
                        decisions.push(snippet.to_string());
                    }
                }
            }
            _ => {}
        }
    }
}

/// Collect lessons learned from messages (workarounds, insights, gotchas)
fn collect_lessons_learned(msg: &Message, lessons: &mut Vec<String>) {
    for block in &msg.content {
        if let ContentBlock::Text { text, .. } = block {
            let lower = text.to_lowercase();

            // Look for workarounds and insights
            if lower.contains("workaround")
                || lower.contains("lesson learned")
                || lower.contains("gotcha")
                || lower.contains("note: ")
                || lower.contains("important: ")
                || lower.contains("warning: ")
            {
                // Extract the relevant sentence
                for line in text.lines() {
                    let l = line.to_lowercase();
                    if l.contains("workaround")
                        || l.contains("lesson")
                        || l.contains("gotcha")
                        || l.contains("note:")
                        || l.contains("important:")
                        || l.contains("warning:")
                    {
                        let trimmed = line.trim().to_string();
                        if trimmed.len() > 10 && trimmed.len() < 200 {
                            lessons.push(trimmed);
                        }
                    }
                }
            }
        }
    }
}

/// Compact a list of file paths by grouping files in the same directory.
/// E.g. ["src/foo.rs", "src/bar.rs", "tests/baz.rs"] becomes
/// "src/{foo,bar}.rs, tests/baz.rs" — saving tokens on long lists.
fn compact_file_list(files: &[String]) -> String {
    use std::collections::BTreeMap;
    let mut by_dir: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in files {
        let path = std::path::Path::new(f);
        let dir = path
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| f.clone());
        by_dir.entry(dir).or_default().push(name);
    }
    let mut parts = Vec::new();
    for (dir, names) in &by_dir {
        if names.len() == 1 {
            if dir.is_empty() {
                parts.push(names[0].clone());
            } else {
                parts.push(format!("{}/{}", dir, names[0]));
            }
        } else {
            // Group: src/{foo.rs, bar.rs, baz.rs}
            let joined = names.join(", ");
            if dir.is_empty() {
                parts.push(format!("{{{}}}", joined));
            } else {
                parts.push(format!("{}/{{{}}}", dir, joined));
            }
        }
    }
    parts.join(", ")
}

pub fn extract_file_mentions(text: &str, file_mentions: &mut Vec<String>) {
    for word in text.split_whitespace() {
        if looks_like_file_reference(word) {
            let cleaned = clean_file_reference(word);
            if !cleaned.is_empty() {
                file_mentions.push(cleaned.to_string());
            }
        }
    }
}

pub fn looks_like_file_reference(word: &str) -> bool {
    (word.contains('/') || word.contains('.'))
        && word.len() > 3
        && word.len() < 120
        && !word.starts_with("http")
        && (word.contains(".rs")
            || word.contains(".ts")
            || word.contains(".py")
            || word.contains(".toml")
            || word.contains(".json")
            || word.starts_with("src/")
            || word.starts_with("./"))
}

pub fn clean_file_reference(word: &str) -> &str {
    word.trim_matches(|c: char| {
        !c.is_alphanumeric() && c != '/' && c != '.' && c != '_' && c != '-'
    })
}

pub fn emergency_truncate_tool_results(messages: &mut [Message], max_chars: usize) -> usize {
    let mut truncated = 0;

    for msg in messages.iter_mut() {
        for block in msg.content.iter_mut() {
            match block {
                ContentBlock::ToolResult { content, .. } if content.len() > max_chars => {
                    *content = emergency_truncated_tool_result(content, max_chars);
                    truncated += 1;
                }
                _ => {}
            }
        }
    }

    truncated
}

pub fn emergency_truncate_large_payloads(
    messages: &mut [Message],
    max_tool_result_chars: usize,
    max_image_chars: usize,
) -> usize {
    let mut truncated = 0;

    for msg in messages.iter_mut() {
        for block in msg.content.iter_mut() {
            match block {
                ContentBlock::ToolResult { content, .. }
                    if content.len() > max_tool_result_chars =>
                {
                    *content = emergency_truncated_tool_result(content, max_tool_result_chars);
                    truncated += 1;
                }
                ContentBlock::Image { media_type, data } if data.len() > max_image_chars => {
                    let original_len = data.len();
                    let media_type = media_type.clone();
                    *block = ContentBlock::Text {
                        text: format!(
                            "[Image omitted during emergency context recovery: media_type={media_type}, original_base64_chars={original_len}. Rely on adjacent browser/tool text, screenshots saved to disk, or re-open/re-screenshot if visual details are needed.]"
                        ),
                        cache_control: None,
                    };
                    truncated += 1;
                }
                _ => {}
            }
        }
    }

    truncated
}

/// Whether a provider error indicates the *serialized request body* was too
/// large (HTTP 413), as distinct from exceeding the model's token context
/// window. Anthropic surfaces this as `request_too_large` / "Request exceeds the
/// maximum size" / "413 Payload Too Large"; OpenAI and gateways use similar
/// wording. This failure mode is dominated by inline base64 images, which the
/// token-budget accounting deliberately undercounts, so it needs a dedicated
/// byte-size recovery rather than ordinary context compaction.
pub fn is_request_payload_too_large_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("request_too_large")
        || lower.contains("request too large")
        || lower.contains("payload too large")
        || lower.contains("request entity too large")
        || lower.contains("request exceeds the maximum size")
        || lower.contains("exceeds the maximum size")
        || contains_independent_status_code(&lower, "413")
}

/// Whether `haystack` contains `code` as a standalone status code rather than as
/// a fragment of a longer number (so "413" matches but "4130"/"version 4131"
/// does not). Mirrors the failover classifier's guard.
fn contains_independent_status_code(haystack: &str, code: &str) -> bool {
    let bytes = haystack.as_bytes();
    haystack.match_indices(code).any(|(start, _)| {
        let before_ok = start == 0 || !bytes[start - 1].is_ascii_digit();
        let end = start + code.len();
        let after_ok = end == bytes.len() || !bytes[end].is_ascii_digit();
        before_ok && after_ok
    })
}

/// Strip oversized inline images from `messages`, oldest-first, until the total
/// remaining base64 image payload fits within `target_total_chars`.
///
/// Unlike [`emergency_truncate_large_payloads`] (which is driven by the token
/// budget and replaces *every* image past a tiny per-image cap), this is the
/// byte-size recovery path for HTTP 413 "request too large" errors: it preserves
/// as many of the most recent images as the request size budget allows and only
/// drops the older ones. Each stripped image is replaced with a text marker so
/// the model still knows an image existed and where to recover it from.
///
/// Returns the number of images that were replaced with text markers.
pub fn emergency_strip_large_images(messages: &mut [Message], target_total_chars: usize) -> usize {
    let mut contents: Vec<&mut Vec<ContentBlock>> =
        messages.iter_mut().map(|m| &mut m.content).collect();
    strip_large_images_in_contents(&mut contents, target_total_chars)
}

/// Core of [`emergency_strip_large_images`], operating directly on a slice of
/// content-block vectors so it can be reused for both provider `Message`s and
/// the session's stored-message representation (which share `ContentBlock`).
pub fn strip_large_images_in_contents(
    contents: &mut [&mut Vec<ContentBlock>],
    target_total_chars: usize,
) -> usize {
    // Collect (content_index, block_index, payload_len) for every inline image,
    // in transcript order (oldest first).
    let mut images: Vec<(usize, usize, usize)> = Vec::new();
    let mut total: usize = 0;
    for (ci, content) in contents.iter().enumerate() {
        for (bi, block) in content.iter().enumerate() {
            if let ContentBlock::Image { data, .. } = block {
                images.push((ci, bi, data.len()));
                total = total.saturating_add(data.len());
            }
        }
    }

    if total <= target_total_chars {
        return 0;
    }

    let mut stripped = 0;
    // Drop oldest images first until we're under budget (always keep trying even
    // if a single huge recent image alone exceeds the budget — better to ship a
    // request the provider might still trim than to give up entirely).
    for (ci, bi, payload_len) in images {
        if total <= target_total_chars {
            break;
        }
        let block = &mut contents[ci][bi];
        if let ContentBlock::Image { media_type, data } = block {
            let original_len = data.len();
            let media_type = media_type.clone();
            *block = ContentBlock::Text {
                text: format!(
                    "[Image omitted during request-size recovery: media_type={media_type}, original_base64_chars={original_len}. The request body exceeded the provider size limit; older images were dropped. Rely on adjacent browser/tool text, screenshots saved to disk, or re-open/re-screenshot if visual details are needed.]"
                ),
                cache_control: None,
            };
            total = total.saturating_sub(payload_len);
            stripped += 1;
        }
    }

    stripped
}

/// Truncate a tool result for emergency context recovery.
///
/// Preserves the head (most relevant for understanding what happened)
/// and tail (often contains final status/errors), with a clear marker
/// indicating what was dropped. The split ratio favors the head because
/// tool outputs typically have their most important content at the start
/// (command output, error messages) and end (final status, results).
pub fn emergency_truncated_tool_result(content: &str, max_chars: usize) -> String {
    let original_len = content.len();
    if original_len <= max_chars {
        return content.to_string();
    }
    // 60% head, 30% tail — head is more important for context
    let keep_head = (max_chars * 6) / 10;
    let keep_tail = (max_chars * 3) / 10;
    let head = truncate_str_boundary(content, keep_head);
    let tail = tail_str_boundary(content, keep_tail);
    let truncated_len = original_len.saturating_sub(head.len() + tail.len());
    format!(
        "{}\n\n... [{} chars truncated for context recovery] ...\n\n{}",
        head, truncated_len, tail,
    )
}

pub fn tail_str_boundary(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut start = value.len().saturating_sub(max_bytes);
    while start < value.len() && !value.is_char_boundary(start) {
        start += 1;
    }
    &value[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_context_split_accounting_adds_cache_counters() {
        // Anthropic-style: input is the uncached remainder.
        assert_eq!(
            effective_context_tokens_from_usage("anthropic", 10_000, Some(300_000), Some(5_000)),
            315_000
        );
        assert_eq!(
            effective_context_tokens_from_usage("Claude", 10_000, Some(300_000), None),
            310_000
        );
    }

    #[test]
    fn effective_context_subset_accounting_does_not_double_count() {
        // OpenAI-style: prompt_tokens already includes cached tokens.
        assert_eq!(
            effective_context_tokens_from_usage("openai", 400_000, Some(390_000), None),
            400_000
        );
        // No cache info at all: pass through.
        assert_eq!(
            effective_context_tokens_from_usage("opencode-go", 396_000, None, None),
            396_000
        );
    }

    #[test]
    fn effective_context_infers_split_accounting_from_counter_shape() {
        // cache_read > input implies input can't already contain it.
        assert_eq!(
            effective_context_tokens_from_usage("unknown", 10_000, Some(500_000), None),
            510_000
        );
        // Any cache_creation implies split accounting.
        assert_eq!(
            effective_context_tokens_from_usage("unknown", 10_000, Some(2_000), Some(1_000)),
            13_000
        );
    }

    #[test]
    fn effective_context_zero_input_reports_zero() {
        assert_eq!(
            effective_context_tokens_from_usage("anthropic", 0, Some(300_000), Some(5_000)),
            0
        );
    }

    #[test]
    fn builds_compaction_prompt_with_summary_and_truncated_tool_result() {
        let summary = Summary {
            text: "prior work".to_string(),
            openai_encrypted_content: None,
            covers_up_to_turn: 1,
            original_turn_count: 1,
        };
        let message = Message::user("hello");
        let prompt = build_compaction_prompt(&[message], Some(&summary), 10_000);
        assert!(prompt.contains("## Previous Summary"));
        assert!(prompt.contains("prior work"));
        assert!(prompt.contains("**User:**"));
        assert!(prompt.contains(SUMMARY_PROMPT));
    }

    #[test]
    fn truncates_on_utf8_boundary() {
        assert_eq!(truncate_str_boundary("éabc", 1), "");
        assert_eq!(truncate_str_boundary("éabc", 2), "é");
    }

    #[test]
    fn mean_embedding_is_normalized() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let mean = mean_embedding(&[&a, &b], 2);
        let norm = (mean[0] * mean[0] + mean[1] * mean[1]).sqrt();
        assert!((norm - 1.0).abs() < 0.0001);
    }

    #[test]
    fn safe_cutoff_keeps_tool_use_with_tool_result() {
        let tool_use = Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "call_1".to_string(),
                name: "read".to_string(),
                input: serde_json::json!({"file":"src/lib.rs"}),
                thought_signature: None,
            }],
            timestamp: None,
            tool_duration_ms: None,
        };
        let tool_result = Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "call_1".to_string(),
                content: "ok".to_string(),
                is_error: None,
            }],
            timestamp: None,
            tool_duration_ms: None,
        };
        let messages = vec![
            Message::user("old"),
            tool_use,
            tool_result,
            Message::user("new"),
        ];

        assert_eq!(safe_compaction_cutoff(&messages, 2), 1);
    }

    #[test]
    fn estimates_tokens_with_large_budget_overhead() {
        let summary = Summary {
            text: "abcd".repeat(100),
            openai_encrypted_content: None,
            covers_up_to_turn: 1,
            original_turn_count: 1,
        };

        assert_eq!(estimate_compaction_tokens(Some(&summary), 0, 1000), 100);
        assert_eq!(
            estimate_compaction_tokens(Some(&summary), 0, DEFAULT_TOKEN_BUDGET),
            100 + SYSTEM_OVERHEAD_TOKENS
        );
    }

    #[test]
    fn image_token_cost_is_bounded_not_base64_length() {
        // Regression: a large base64 image payload must not be counted as ~len/4
        // tokens. Doing so inflated the estimate ~100x and caused repeated
        // back-to-back compactions.
        let huge_base64 = "A".repeat(1_400_000);
        let mut image_msg = Message::user("");
        image_msg.content = vec![ContentBlock::Image {
            media_type: "image/png".to_string(),
            data: huge_base64.clone(),
        }];

        let chars = message_char_count(&image_msg);
        // Flat per-image cost in char-equivalents, far below the raw payload.
        assert_eq!(chars, IMAGE_TOKEN_COST * CHARS_PER_TOKEN);
        assert!(
            chars < huge_base64.len() / 10,
            "image should not be charged anywhere near its base64 length"
        );

        // The token estimate for four such images stays small.
        let tokens = estimate_compaction_tokens_from_chars(chars * 4, DEFAULT_TOKEN_BUDGET);
        assert!(
            tokens < SYSTEM_OVERHEAD_TOKENS + 4 * IMAGE_TOKEN_COST + 10,
            "four images should cost ~{} tokens, got {}",
            SYSTEM_OVERHEAD_TOKENS + 4 * IMAGE_TOKEN_COST,
            tokens
        );
    }

    #[test]
    fn builds_semantic_text_from_relevant_content() {
        let message = Message {
            role: Role::User,
            content: vec![
                ContentBlock::Text {
                    text: "hello world".to_string(),
                    cache_control: None,
                },
                ContentBlock::ToolResult {
                    tool_use_id: "call_1".to_string(),
                    content: "tool output".to_string(),
                    is_error: None,
                },
            ],
            timestamp: None,
            tool_duration_ms: None,
        };

        assert_eq!(semantic_message_text(&message), "hello world");
        assert_eq!(semantic_goal_text(&[message]), "hello world tool output");
        assert_eq!(semantic_cache_key("stable"), semantic_cache_key("stable"));
    }

    #[test]
    fn builds_emergency_summary_with_tools_and_files() {
        let messages = vec![
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".to_string(),
                    name: "read".to_string(),
                    input: serde_json::json!({"file":"src/lib.rs"}),
                    thought_signature: None,
                }],
                timestamp: None,
                tool_duration_ms: None,
            },
            Message::user("Edited src/compaction.rs and Cargo.toml, ignored https://example.com"),
        ];

        let summary =
            build_emergency_summary_text(Some("previous"), 2, 201_000, 200_000, &messages);
        assert!(summary.contains("previous"));
        assert!(summary.contains("2 messages were dropped"));
        assert!(summary.contains("Tools used: read"));
        assert!(summary.contains("Files:"));
        assert!(summary.contains("Cargo.toml"));
        assert!(summary.contains("src/compaction.rs"));
        assert!(!summary.contains("https://example.com"));
    }

    #[test]
    fn emergency_truncation_is_utf8_safe() {
        let original = format!("{}middle{}", "é".repeat(20), "尾".repeat(20));
        let truncated = emergency_truncated_tool_result(&original, 25);
        assert!(truncated.contains("chars truncated for context recovery"));
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn emergency_truncation_replaces_large_images_with_text_marker() {
        let mut messages = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Image {
                media_type: "image/png".to_string(),
                data: "a".repeat(2048),
            }],
            timestamp: None,
            tool_duration_ms: None,
        }];

        let truncated = emergency_truncate_large_payloads(&mut messages, 4000, 1024);
        assert_eq!(truncated, 1);
        match &messages[0].content[0] {
            ContentBlock::Text { text, .. } => {
                assert!(text.contains("Image omitted during emergency context recovery"));
                assert!(text.contains("original_base64_chars=2048"));
            }
            other => panic!("expected image to be replaced with text marker, got {other:?}"),
        }
    }

    #[test]
    fn detects_request_payload_too_large_errors() {
        assert!(is_request_payload_too_large_error(
            "Anthropic API error (413 Payload Too Large): {\"error\":{\"type\":\"request_too_large\",\"message\":\"Request exceeds the maximum size\"}}"
        ));
        assert!(is_request_payload_too_large_error(
            "413 Request Entity Too Large"
        ));
        assert!(is_request_payload_too_large_error("request too large"));
        // Not a payload error — should not match.
        assert!(!is_request_payload_too_large_error(
            "rate limit exceeded, retry after 20s"
        ));
        // Embedded digits must not trip the standalone 413 check.
        assert!(!is_request_payload_too_large_error(
            "model version 4130 is unavailable"
        ));
    }

    fn image_msg(data_len: usize) -> Message {
        Message {
            role: Role::User,
            content: vec![ContentBlock::Image {
                media_type: "image/png".to_string(),
                data: "a".repeat(data_len),
            }],
            timestamp: None,
            tool_duration_ms: None,
        }
    }

    #[test]
    fn strip_large_images_drops_oldest_until_under_budget() {
        // Four 1000-char images = 4000 total; budget 2500 should drop the two
        // oldest (leaving 2000 <= 2500), keeping the two most recent.
        let mut messages = vec![
            image_msg(1000),
            image_msg(1000),
            image_msg(1000),
            image_msg(1000),
        ];

        let stripped = emergency_strip_large_images(&mut messages, 2500);
        assert_eq!(stripped, 2);
        // Oldest two replaced with text markers.
        assert!(matches!(messages[0].content[0], ContentBlock::Text { .. }));
        assert!(matches!(messages[1].content[0], ContentBlock::Text { .. }));
        // Most recent two preserved as images.
        assert!(matches!(messages[2].content[0], ContentBlock::Image { .. }));
        assert!(matches!(messages[3].content[0], ContentBlock::Image { .. }));
        if let ContentBlock::Text { text, .. } = &messages[0].content[0] {
            assert!(text.contains("Image omitted during request-size recovery"));
            assert!(text.contains("original_base64_chars=1000"));
        }
    }

    #[test]
    fn strip_large_images_noop_when_under_budget() {
        let mut messages = vec![image_msg(500), image_msg(500)];
        let stripped = emergency_strip_large_images(&mut messages, 4000);
        assert_eq!(stripped, 0);
        assert!(matches!(messages[0].content[0], ContentBlock::Image { .. }));
        assert!(matches!(messages[1].content[0], ContentBlock::Image { .. }));
    }

    #[test]
    fn strip_large_images_strips_all_when_single_image_exceeds_budget() {
        // Even a lone over-budget image is stripped (better than re-sending an
        // oversized request that the provider will reject again).
        let mut messages = vec![image_msg(8000)];
        let stripped = emergency_strip_large_images(&mut messages, 2000);
        assert_eq!(stripped, 1);
        assert!(matches!(messages[0].content[0], ContentBlock::Text { .. }));
    }

    #[test]
    fn compact_file_list_groups_by_directory() {
        let files = vec![
            "src/foo.rs".to_string(),
            "src/bar.rs".to_string(),
            "tests/baz.rs".to_string(),
        ];
        let compact = compact_file_list(&files);
        // Should group src/ files together
        assert!(compact.contains("src/{"), "expected grouping: {compact}");
        assert!(compact.contains("foo.rs"));
        assert!(compact.contains("bar.rs"));
        assert!(compact.contains("tests/baz.rs"));
    }

    #[test]
    fn compact_file_list_single_file_no_grouping() {
        let files = vec!["src/only.rs".to_string()];
        let compact = compact_file_list(&files);
        assert_eq!(compact, "src/only.rs");
    }

    #[test]
    fn compact_file_list_empty() {
        let compact = compact_file_list(&[]);
        assert!(compact.is_empty());
    }

    // ========== New tests for enhanced compaction features ==========

    #[test]
    fn adaptive_thresholds_adjust_for_complex_tasks() {
        let mut thresholds = AdaptiveThresholds::new();
        assert_eq!(thresholds.compaction_threshold, COMPACTION_THRESHOLD);

        // Simulate rapid context growth (complex task)
        thresholds.update(0.3);
        thresholds.update(0.5);
        thresholds.update(0.7);

        assert!(thresholds.is_complex_task);
        assert!(thresholds.task_complexity > 0.5);
        assert!(thresholds.compaction_threshold < COMPACTION_THRESHOLD);
    }

    #[test]
    fn adaptive_thresholds_stay_within_bounds() {
        let mut thresholds = AdaptiveThresholds::new();

        // Extreme growth
        thresholds.update(0.1);
        thresholds.update(0.5);
        thresholds.update(0.95);

        assert!(thresholds.compaction_threshold >= ADAPTIVE_THRESHOLD_MIN);
        assert!(thresholds.compaction_threshold <= ADAPTIVE_THRESHOLD_MAX);
    }

    #[test]
    fn adaptive_thresholds_recommended_turns_for_complex_tasks() {
        let mut thresholds = AdaptiveThresholds::new();
        thresholds.is_complex_task = true;
        thresholds.task_complexity = 0.8;

        assert!(thresholds.recommended_recent_turns() > RECENT_TURNS_TO_KEEP);
    }

    #[test]
    fn topic_detector_detects_shifts() {
        let mut detector = TopicDetector::new();

        // Add similar messages (same topic)
        for _ in 0..5 {
            assert!(!detector.add_message(0xABCD1234));
        }

        // Add very different messages (topic shift)
        assert!(detector.add_message(0x00000001));
        assert!(detector.has_topic_shifts());
    }

    #[test]
    fn topic_detector_needs_minimum_messages() {
        let mut detector = TopicDetector::new();

        // Not enough messages to detect shifts
        assert!(!detector.add_message(0x1234));
        assert!(!detector.add_message(0x5678));
        assert!(!detector.has_topic_shifts());
    }

    #[test]
    fn importance_scorer_identifies_user_preferences() {
        let msg = Message::user("You must use TypeScript and the error is critical");
        let score = ImportanceScorer::score_message(&msg);

        assert!(score.has_preferences);
        assert!(score.has_errors);
        assert!(score.score > IMPORTANCE_MEDIUM);
    }

    #[test]
    fn importance_scorer_identifies_decisions() {
        let mut msg = Message::user("");
        msg.content = vec![ContentBlock::ToolUse {
            id: "call_1".to_string(),
            name: "write".to_string(),
            input: serde_json::json!({"file_path": "src/main.rs"}),
            thought_signature: None,
        }];
        let score = ImportanceScorer::score_message(&msg);

        assert!(score.score > 0.0);
        assert!(score.reason.contains("file modification"));
    }

    #[test]
    fn importance_scorer_ranks_messages() {
        let messages = vec![
            Message::user("normal message"),
            Message::user("This is required and must work correctly"),
            Message::user("another normal message"),
        ];

        let indices = ImportanceScorer::get_important_indices(&messages, 0.5);
        // Should include at least one message
        assert!(!indices.is_empty());
    }

    #[test]
    fn quality_analyzer_evaluates_compaction() {
        let messages = vec![
            Message::user("normal"),
            Message::user("This must be preserved exactly"),
            Message::user("another normal"),
            Message::user("more normal content"),
        ];

        let mut kept = HashSet::new();
        kept.insert(1); // Keep the important message

        let quality = QualityAnalyzer::analyze_quality(&messages, &kept, &[]);

        assert!(quality.important_content_preserved >= 0.5);
        assert!(quality.avg_importance_kept > quality.avg_importance_dropped);
    }

    #[test]
    fn quality_analyzer_recommends_keep_more_when_important_content_dropped() {
        let messages = vec![
            Message::user("This is required and critical"),
            Message::user("normal message"),
            Message::user("normal message 2"),
        ];

        let mut kept = HashSet::new();
        kept.insert(1); // Keep only normal message

        let quality = QualityAnalyzer::analyze_quality(&messages, &kept, &[]);

        assert_eq!(quality.recommended_action, QualityAction::KeepMoreMessages);
    }

    #[test]
    fn quality_analyzer_recommends_topic_aware_when_shifts_detected() {
        let messages = vec![Message::user("normal")];
        let kept = HashSet::from([0]);

        let shifts = vec![TopicShift {
            message_index: 5,
            pre_shift_similarity: 0.8,
            post_shift_similarity: 0.2,
            shift_magnitude: 0.6,
            is_significant: true,
        }];

        let quality = QualityAnalyzer::analyze_quality(&messages, &kept, &shifts);

        assert_eq!(
            quality.recommended_action,
            QualityAction::UseTopicAwareSummarization
        );
    }

    #[test]
    fn hash_similarity_produces_valid_range() {
        let sim = hash_similarity(0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF);
        assert!((sim - 1.0).abs() < 0.001);

        let sim = hash_similarity(0xFFFFFFFFFFFFFFFF, 0x0000000000000000);
        assert!((sim - 0.0).abs() < 0.001);
    }

    #[test]
    fn collect_lessons_learned_finds_workarounds() {
        let msg = Message::user("Important: we found a workaround for the race condition");
        let mut lessons = Vec::new();

        collect_lessons_learned(&msg, &mut lessons);

        assert!(!lessons.is_empty());
        assert!(lessons[0].contains("workaround"));
    }

    #[test]
    fn build_compaction_prompt_with_topic_shifts() {
        let messages = vec![Message::user("test")];
        let shifts = vec![TopicShift {
            message_index: 5,
            pre_shift_similarity: 0.8,
            post_shift_similarity: 0.2,
            shift_magnitude: 0.6,
            is_significant: true,
        }];

        let prompt =
            build_compaction_prompt_with_context(&messages, None, 10_000, Some(&shifts), None);

        assert!(prompt.contains("Topic Boundaries Detected"));
        assert!(prompt.contains(TOPIC_AWARE_SUMMARY_PROMPT));
    }

    #[test]
    fn build_compaction_prompt_with_adaptive_thresholds() {
        let messages = vec![Message::user("test")];
        let mut thresholds = AdaptiveThresholds::new();
        thresholds.is_complex_task = true;
        thresholds.task_complexity = 0.8;

        let prompt =
            build_compaction_prompt_with_context(&messages, None, 10_000, None, Some(&thresholds));

        assert!(prompt.contains("Task Complexity"));
    }
}
