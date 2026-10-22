use super::Agent;
use crate::alphacode_app_core::logging;
use crate::alphacode_app_core::message::{Message, Role, ToolDefinition};

/// Free-function for probe-burst detection: `true` when the tool call looks
/// like an outbound network probe (HTTP request or a recon/attack binary).
/// Input text is the serialized tool input (bash commands appear there).
fn tool_call_is_network_probe(name: &str, input_text: &str) -> bool {
    const PROBE_BINARIES: &[&str] = &[
        "curl ",
        "wget ",
        "httpx",
        "ffuf",
        "gau",
        "waybackurls",
        "katana",
        "subfinder",
        "nuclei",
        "sqlmap",
        "wfuzz",
        "feroxbuster",
        "gobuster",
        "nikto",
        "dalfox",
        "hakrawler",
        "gospider",
        "arjun",
        "dirsearch",
        "jaeles",
        "amass",
        "assetfinder",
        "dnsx",
        "httprobe",
        "mapcidr",
    ];
    const WEB_TOOLS: &[&str] = &["webfetch"];
    if WEB_TOOLS.contains(&name) {
        return true;
    }
    if name != "bash" {
        return false;
    }
    let lower = input_text.to_ascii_lowercase();
    lower.contains("http://")
        || lower.contains("https://")
        || PROBE_BINARIES.iter().any(|binary| lower.contains(binary))
}

/// Free-function probe-burst reminder. Returns the reminder text when the
/// recent tool-call history looks like an unattributed probe grind: a run of
/// ≥`PROBE_BURST_THRESHOLD` consecutive network probes with no
/// hypothesis-tracking artifact (`todo`, `memory`, `session_search`, or
/// `plan`) anywhere in the scanned window, and no non-probe tool call after
/// the run started that would indicate interleaved analysis.
fn probe_burst_reminder(tool_calls: &[(String, String)]) -> Option<String> {
    const PROBE_BURST_THRESHOLD: usize = 10;
    const TRACKING_TOOLS: &[&str] = &["todo", "memory", "session_search", "plan", "skill_manage"];

    // The run we measure is the suffix of consecutive probe calls. A tracking
    // tool anywhere in the scanned window means hypotheses are being
    // maintained, so the fourth consecutive blind probe is still costly but
    // already accounted for; do not nag.
    let tracking_present = tool_calls
        .iter()
        .any(|(name, _)| TRACKING_TOOLS.contains(&name.as_str()));
    if tracking_present {
        return None;
    }

    let mut run = 0usize;
    for (name, input) in tool_calls.iter().rev() {
        if tool_call_is_network_probe(name, input) {
            run += 1;
        } else {
            break;
        }
    }
    if run < PROBE_BURST_THRESHOLD {
        return None;
    }
    let reminder = format!(
        "\n\n# Probe Grind Detected\n\n\
        {} consecutive probing tool calls with no hypothesis ledger update \
        (todo / memory) anywhere in this window. If this session is security \
        research, this is the unattributed probe grind that turns hours into \
        nothing:\n\n\
        1. Name the hypothesis each probe targets (an id like `H3: If IDOR exists on /api/orders/<foreign-id>, a foreign id returns 200`). Drop probes with no open hypothesis.\n\
        2. Record the discriminating observation each probe produced; three identical results retire the hypothesis instead of feeding it a fourth variant.\n\
        3. If the live hypotheses are all dead, stop: log the kill decision and move to the next highest-value attack surface. Repointing the same payloads is not progress.\n\n\
        If this is not security research, batch the repeated calls into one scripted command with bounded output and open a todo list to track the work.",
        run,
    );
    Some(reminder)
}

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
        let prefix_tokens = system_tokens.saturating_add(tool_tokens);
        // The default estimate is necessarily a guess because neither the
        // compaction manager nor the provider API owns the assembled tool
        // schemas. Once this request's prefix exists, give the measured size
        // back to compaction so small-window models reserve the space they
        // actually need for their system prompt and tools.
        if let Ok(mut manager) = self.registry.compaction().try_write() {
            manager.set_prompt_overhead_tokens(prefix_tokens);
        }
        let tier_label = match tier {
            crate::prompt::PromptTier::Minimal => "minimal",
            crate::prompt::PromptTier::Compact => "compact",
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

    /// Detect verification loops: N consecutive read-only tool calls with no
    /// state-changing result. When detected, inject a system reminder that
    /// breaks the loop by re-deriving the completion predicate.
    fn detect_verification_loop(&self, split: &mut crate::prompt::SplitSystemPrompt) {
        const LOOP_THRESHOLD: usize = 8;

        // Read-only tools that don't change external state
        const READ_ONLY_TOOLS: &[&str] = &[
            "webfetch",
            "scrapling",
            "read",
            "grep",
            "glob",
            "alphagrep",
            "session_search",
            "conversation_search",
            "skill_manage",
            "memory",
            "todo",
            "browser",
        ];

        // State-changing tools that break the loop
        const STATE_CHANGING_TOOLS: &[&str] =
            &["bash", "write", "edit", "multiedit", "apply_patch", "patch"];

        let mut consecutive_read_only = 0usize;
        let mut last_tool_calls = Vec::new();

        // Scan recent messages (last 20) for tool call patterns
        let msg_count = self.session.messages.len();
        let start = msg_count.saturating_sub(20);

        for stored in &self.session.messages[start..] {
            for block in &stored.content {
                if let crate::alphacode_app_core::message::ContentBlock::ToolUse { name, .. } =
                    block
                {
                    last_tool_calls.push(name.clone());
                }
            }
        }

        // Walk backwards through tool calls
        for tool_name in last_tool_calls.iter().rev() {
            if STATE_CHANGING_TOOLS.contains(&tool_name.as_str()) {
                consecutive_read_only = 0;
                break;
            } else if READ_ONLY_TOOLS.contains(&tool_name.as_str()) {
                consecutive_read_only += 1;
            }
        }

        if consecutive_read_only >= LOOP_THRESHOLD {
            let reminder = format!(
                "\n\n# Verification Loop Detected\n\n\
                You have made {} consecutive read-only tool calls without a state-changing result. \
                This pattern indicates you are verifying against derived artifacts instead of the authoritative signal.\n\n\
                **Stop and re-derive the completion predicate from first principles:**\n\
                1. What is the exact authoritative signal? (not grep output, not substring match)\n\
                2. Have you observed it directly in a tool result?\n\
                3. If not, what specific state change would produce it?\n\
                4. Use a state-changing tool (bash, write, edit) to advance toward the goal, or report the blocker.",
                consecutive_read_only
            );

            if !split.dynamic_part.is_empty() {
                split.dynamic_part.push_str("\n\n");
            }
            split.dynamic_part.push_str(&reminder);
        }
    }

    /// Detect probe bursts: a long run of consecutive network-probing tool
    /// calls with no hypothesis-tracking artifact (`todo`, `memory`) anywhere
    /// in the window. During security research this is the signature of the
    /// unattributed probe grind that burns whole sessions on blind requests;
    /// during ordinary scripted work it usually means the calls should have
    /// been batched. Injection is advisory (a system reminder), not an
    /// interruption.
    fn detect_probe_burst(&self, split: &mut crate::prompt::SplitSystemPrompt) {
        const SCAN_WINDOW: usize = 24;

        let mut tool_calls: Vec<(String, String)> = Vec::new();
        let msg_count = self.session.messages.len();
        let start = msg_count.saturating_sub(20);
        'outer: for stored in &self.session.messages[start..] {
            for block in &stored.content {
                if let crate::alphacode_app_core::message::ContentBlock::ToolUse {
                    name,
                    input,
                    ..
                } = block
                {
                    if tool_calls.len() >= SCAN_WINDOW {
                        break 'outer;
                    }
                    let input_text = serde_json::to_string(input).unwrap_or_default();
                    tool_calls.push((name.clone(), input_text));
                }
            }
        }

        if let Some(reminder) = probe_burst_reminder(&tool_calls)
            && !reminder.trim().is_empty()
        {
            if !split.dynamic_part.is_empty() {
                split.dynamic_part.push_str("\n\n");
            }
            split.dynamic_part.push_str(&reminder);
        }
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

        // One snapshot, used for both consumers. `current_skills_snapshot`
        // deep-clones the whole skill registry and re-scans the project
        // overlay directories, so calling it twice per prompt build cost two
        // full registry clones plus two disk scans for the same data.
        let skills = self.current_skills_snapshot();
        let skill_prompt = self
            .active_skill
            .as_ref()
            .and_then(|name| skills.get(name).map(|skill| skill.get_prompt().to_string()));

        let available_skills: Vec<crate::prompt::SkillInfo> = skills
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
        self.detect_verification_loop(&mut split);
        self.detect_probe_burst(&mut split);
        self.append_prompt_autopilot_briefing(&mut split, resolved_tier);
        crate::prompt::append_swarm_effort_directive(
            &mut split,
            self.provider.reasoning_effort().as_deref(),
        );

        (split, resolved_tier)
    }

    /// Prompt Autopilot: when the fresh user request is underspecified (short,
    /// vague, no files/errors/acceptance criteria), append a compact task
    /// briefing to the dynamic system context so the model expands the request
    /// itself — restates the concrete requirement, infers missing details from
    /// the codebase, defines "done", and verifies before reporting. The user's
    /// stored text is never modified, so transcripts, prompt history, and
    /// replay stay verbatim.
    fn append_prompt_autopilot_briefing(
        &self,
        split: &mut crate::prompt::SplitSystemPrompt,
        tier: crate::prompt::PromptTier,
    ) {
        // Trivial greeting turns never need a briefing, and the compact tier
        // serves small-context models that cannot spare the tokens.
        if tier != crate::prompt::PromptTier::Standard {
            return;
        }
        if !crate::config::config().features.prompt_autopilot {
            return;
        }
        let Some(latest) = self.latest_fresh_user_text() else {
            return;
        };
        let assessment = crate::prompt_enhance::assess(&latest);
        if !assessment.vague {
            return;
        }
        logging::info(&format!(
            "Prompt autopilot: underspecified request detected (kind={}, reasons={:?})",
            assessment.task_kind.label(),
            assessment.reasons
        ));
        if !split.dynamic_part.is_empty() {
            split.dynamic_part.push_str("\n\n");
        }
        split
            .dynamic_part
            .push_str(&crate::prompt_enhance::briefing_for_assessment(&assessment));
    }

    /// Text of the newest user message when it is the *current* turn request
    /// (the last stored message is a user text message). Returns `None` once
    /// the turn moves past the user's own words — assistant replies, tool
    /// results, injected reminders, and auto-pokes — so the briefing never
    /// fires on continuations or automation messages.
    fn latest_fresh_user_text(&self) -> Option<String> {
        let last = self.session.messages.last()?;
        if last.role != Role::User {
            return None;
        }
        let mut out = String::new();
        for block in &last.content {
            if let crate::alphacode_app_core::message::ContentBlock::Text { text, .. } = block {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(text);
            }
        }
        let trimmed = out.trim();
        if trimmed.is_empty() {
            return None;
        }
        // Skip injected automation messages: dynamic system reminders, soft
        // interrupts, auto-pokes, and previously attached briefings.
        if trimmed.starts_with("<system-reminder>")
            || trimmed.starts_with('[')
            || trimmed.contains("<auto-enhanced by Alphacode>")
            || crate::todo::is_auto_poke_message(trimmed)
        {
            return None;
        }
        Some(trimmed.to_string())
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
            Some(_) if self.uses_compact_local_context() => crate::prompt::PromptTier::Compact,
            _ => crate::prompt::PromptTier::Standard,
        }
    }

    /// Returns the textual content of the most recent user message, if any.
    /// Walks backwards through the stored messages to find the last `User`
    /// role and concatenates any text blocks.
    pub(super) fn latest_user_text(&self) -> Option<String> {
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

#[cfg(test)]
mod probe_burst_tests {
    use super::probe_burst_reminder;
    use super::tool_call_is_network_probe;

    fn bash(command: &str) -> (String, String) {
        (
            "bash".to_string(),
            serde_json::json!({ "command": command }).to_string(),
        )
    }

    #[test]
    fn probe_like_bash_commands_classify() {
        assert!(tool_call_is_network_probe(
            "bash",
            &serde_json::json!({ "command": "curl -s https://site/api/v1/orders/1" }).to_string(),
        ));
        assert!(tool_call_is_network_probe(
            "bash",
            &serde_json::json!({ "command": "httpx -l targets.txt -sc" }).to_string(),
        ));
        assert!(tool_call_is_network_probe(
            "webfetch",
            &serde_json::json!({ "url": "https://example.com/a" }).to_string(),
        ));
        assert!(!tool_call_is_network_probe(
            "bash",
            &serde_json::json!({ "command": "cargo test --lib todo" }).to_string(),
        ));
        assert!(!tool_call_is_network_probe(
            "read",
            &serde_json::json!({ "path": "src/main.rs" }).to_string(),
        ));
    }

    #[test]
    fn a_long_untracked_probe_run_trips_the_reminder() {
        let calls: Vec<(String, String)> = (0..12)
            .map(|n| bash(&format!("curl -s https://site/api/page/{n}")))
            .collect();
        assert!(probe_burst_reminder(&calls).is_some());
    }

    #[test]
    fn a_todo_checkpoint_anywhere_in_the_window_disarms_it() {
        let mut calls: Vec<(String, String)> = (0..12)
            .map(|n| bash(&format!("curl -s https://site/api/page/{n}")))
            .collect();
        calls.push((
            "todo".to_string(),
            serde_json::json!({ "todos": [] }).to_string(),
        ));
        assert!(probe_burst_reminder(&calls).is_none());
    }

    #[test]
    fn a_recent_analysis_call_breaks_the_consecutive_run() {
        let mut calls: Vec<(String, String)> = (0..12)
            .map(|n| bash(&format!("curl -s https://site/api/page/{n}")))
            .collect();
        calls.push(bash("sort payloads | uniq > candidates.txt"));
        assert!(probe_burst_reminder(&calls).is_none());
    }

    #[test]
    fn a_short_run_stays_silent() {
        let calls: Vec<(String, String)> = (0..4)
            .map(|n| bash(&format!("curl -s https://site/api/page/{n}")))
            .collect();
        assert!(probe_burst_reminder(&calls).is_none());
    }
}
