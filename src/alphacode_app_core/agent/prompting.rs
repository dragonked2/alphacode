use super::Agent;
use crate::alphacode_app_core::logging;
use crate::alphacode_app_core::message::{Message, Role, ToolDefinition};

impl Agent {
    /// Log how many tokens the prefix is costing the API this turn.
    /// `tier` is included so cache misses on a minimal-tier turn can be
    /// distinguished from cache misses on the full system prompt (different
    /// blast radius).
    pub(super) fn log_prompt_prefix_accounting(
        &self,
        split: &crate::prompt::SplitSystemPrompt,
        tools: &[ToolDefinition],
        tier: crate::prompt::PromptTier,
    ) {
        let system_tokens = split.estimated_tokens();
        let tool_tokens = ToolDefinition::aggregate_prompt_token_estimate(tools);
        let prefix_tokens = system_tokens + tool_tokens;
        let tier_label = match tier {
            crate::prompt::PromptTier::Minimal => "minimal",
            crate::prompt::PromptTier::Standard => "standard",
        };
        logging::info(&format!(
            "Prompt prefix estimate: tier={} total={} tokens (system={} tools={} tools_count={})",
            tier_label,
            prefix_tokens,
            system_tokens,
            tool_tokens,
            tools.len()
        ));
    }

    pub(super) fn build_memory_prompt_nonblocking_shared(
        &self,
        messages: std::sync::Arc<[Message]>,
        _memory_event_tx: Option<crate::memory::MemoryEventSink>,
    ) -> Option<crate::memory::PendingMemory> {
        if !self.memory_enabled {
            return None;
        }

        let session_id = &self.session.id;

        let fresh_user_turn = crate::message::ends_with_fresh_user_turn(&messages);
        let pending = if fresh_user_turn {
            crate::memory::take_pending_memory(session_id)
        } else {
            None
        };

        // Use the persistent memory-agent pipeline as the single source of truth.
        // Running both this and the legacy MemoryManager background retrieval path
        // can prepare overlapping pending prompts for the same turn, which makes
        // memory injection feel overly aggressive.
        // Relevance results are consumed only at the start of a fresh user turn.
        // Enqueuing again after every tool result runs the local embedding model
        // for each provider continuation without creating an additional injection
        // opportunity. One update per user turn keeps memory current while avoiding
        // redundant 512-token inference during tool-heavy agent loops.
        if fresh_user_turn {
            crate::memory_agent::update_context_sync_with_dir(
                session_id,
                messages,
                self.session.working_dir.clone(),
            );
        }

        pending
    }

    fn append_current_turn_system_reminder(&self, split: &mut crate::prompt::SplitSystemPrompt) {
        let Some(reminder) = self
            .current_turn_system_reminder
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            return;
        };

        if !split.dynamic_part.is_empty() {
            split.dynamic_part.push_str("\n\n");
        }
        split.dynamic_part.push_str("# System Reminder\n\n");
        split.dynamic_part.push_str(reminder);
    }

    /// Build split system prompt for better caching.
    /// Returns static (cacheable) and dynamic (not cached) parts separately,
    /// along with the resolved prompt tier.
    ///
    /// The optional `tier` argument lets callers pick a smaller prompt when
    /// they already know the turn does not need full guidance — for example
    /// a trivial greeting. When the caller is unsure, pass `None` and the
    /// tier is inferred from the most recent user message.
    pub(super) fn build_system_prompt_split(
        &self,
        memory_prompt: Option<&str>,
        tier: Option<crate::prompt::PromptTier>,
    ) -> (crate::prompt::SplitSystemPrompt, crate::prompt::PromptTier) {
        if let Some(ref override_prompt) = self.system_prompt_override {
            return (
                crate::prompt::SplitSystemPrompt {
                    static_part: override_prompt.clone(),
                    dynamic_part: String::new(),
                },
                crate::prompt::PromptTier::Standard,
            );
        }

        let skills = self.current_skills_snapshot();
        let skill_prompt = self
            .active_skill
            .as_ref()
            .and_then(|name| skills.get(name).map(|skill| skill.get_prompt().to_string()));

        let available_skills: Vec<crate::prompt::SkillInfo> = self
            .current_skills_snapshot()
            .list()
            .iter()
            .map(|skill| crate::prompt::SkillInfo {
                name: skill.name.clone(),
                description: skill.description.clone(),
            })
            .collect();

        let working_dir = self
            .session
            .working_dir
            .as_ref()
            .map(std::path::PathBuf::from);

        let resolved_tier = tier.unwrap_or_else(|| self.infer_prompt_tier_from_history());

        let (mut split, _context_info) = crate::prompt::build_system_prompt_split_with_tier(
            resolved_tier,
            skill_prompt.as_deref(),
            &available_skills,
            self.session.is_canary,
            memory_prompt,
            working_dir.as_deref(),
            crate::prompt::PromptCapabilities::current(),
        );

        self.append_current_turn_system_reminder(&mut split);
        crate::prompt::append_swarm_effort_directive(
            &mut split,
            self.provider.reasoning_effort().as_deref(),
        );

        (split, resolved_tier)
    }

    /// Inspect the most recent user message to decide whether the upcoming
    /// turn can run on the minimal prompt tier. Falls back to standard tier
    /// if anything is ambiguous. The decision is per-turn and does not
    /// persist, so a follow-up real request naturally steps back up to the
    /// standard tier.
    fn infer_prompt_tier_from_history(&self) -> crate::prompt::PromptTier {
        match self.latest_user_text() {
            Some(text) if crate::prompt::looks_like_trivial_chat(&text) => {
                crate::prompt::PromptTier::Minimal
            }
            _ => crate::prompt::PromptTier::Standard,
        }
    }

    /// Returns the textual content of the most recent user message, if any.
    /// Walks backwards through the stored messages to find the last `User`
    /// role and concatenates any text blocks.
    fn latest_user_text(&self) -> Option<String> {
        for stored in self.session.messages.iter().rev() {
            if stored.role == Role::User {
                let mut out = String::new();
                for block in &stored.content {
                    if let crate::alphacode_app_core::message::ContentBlock::Text { text, .. } =
                        block
                    {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(text);
                    }
                }
                if !out.is_empty() {
                    return Some(out);
                }
            }
        }
        None
    }
}
