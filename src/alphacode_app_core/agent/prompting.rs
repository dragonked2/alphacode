use super::Agent;
use crate::alphacode_app_core::logging;
use crate::alphacode_app_core::message::{ContentBlock, Message, Role, ToolDefinition};
use crate::alphacode_security_core::skill_router::SkillRouter;

fn security_request(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "bug bounty",
        "bugbounty",
        "hackerone",
        "bugcrowd",
        "intigriti",
        "security audit",
        "security review",
        "security assessment",
        "vulnerability",
        "vulnerabilities",
        "pentest",
        "penetration test",
    ]
    .iter()
    .any(|term| lower.contains(term))
}

pub(super) fn adaptive_tool_query(task: &str, observed_node_stack: bool) -> String {
    let lower = task.to_ascii_lowercase();
    let ctf = super::challenge_context::is_ctf_request(task);
    let web = observed_node_stack
        || SkillRouter::is_nodejs_stack(task)
        || ["http://", "https://", "web app", "api", "endpoint"]
            .iter()
            .any(|term| lower.contains(term));
    if web && ctf {
        format!("{task}\nctf_web_recon httpflow")
    } else if web && security_request(task) {
        format!("{task}\nhttpflow")
    } else {
        task.to_owned()
    }
}

fn node_project(path: &std::path::Path) -> bool {
    use std::io::Read as _;

    const LIMIT: u64 = 64 * 1024;
    let Ok(file) = std::fs::File::open(path.join("package.json")) else {
        return false;
    };
    if !file
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= LIMIT)
    {
        return false;
    }
    let mut bytes = Vec::new();
    if file.take(LIMIT + 1).read_to_end(&mut bytes).is_err() {
        return false;
    }
    let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    ["dependencies", "devDependencies", "optionalDependencies"]
        .iter()
        .filter_map(|key| manifest.get(key).and_then(serde_json::Value::as_object))
        .any(|dependencies| {
            ["next", "express", "fastify", "koa", "@nestjs/core"]
                .iter()
                .any(|package| dependencies.contains_key(*package))
        })
}

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
    if crate::tool::scope::is_network_tool(name) {
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
/// ≥`PROBE_BURST_THRESHOLD` consecutive network probes after the most recent
/// analysis or checkpoint call. An older checkpoint does not exempt a new run.
fn probe_burst_reminder(tool_calls: &[(String, String)]) -> Option<String> {
    const PROBE_BURST_THRESHOLD: usize = 10;
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
        since the last checkpoint. If this session is security \
        research, this is the unattributed probe grind that turns hours into \
        nothing:\n\n\
        1. Name the hypothesis each probe targets (an id like `H3: If IDOR exists on /api/orders/<foreign-id>, a foreign id returns 200`). Drop probes with no open hypothesis.\n\
        2. Record the discriminating observation each probe produced; equivalent results require a control and reachability check; pause inconclusive paths rather than declaring them refuted.\n\
        3. If the live hypotheses are all dead, stop: log the kill decision and move to the next highest-value attack surface. Repointing the same payloads is not progress.\n\n\
        If this is not security research, batch the repeated calls into one scripted command with bounded output and open a todo list to track the work.",
        run,
    );
    Some(reminder)
}

/// Treat headers as routing hints. They never confer target authorization
/// or establish that a framework or vulnerability is present.
pub(crate) fn observed_node_stack(
    messages: &[crate::alphacode_session_types::StoredMessage],
) -> bool {
    let mut calls = std::collections::HashMap::new();
    for message in &messages[messages.len().saturating_sub(24)..] {
        for block in &message.content {
            match block {
                ContentBlock::ToolUse {
                    id, name, input, ..
                } if matches!(name.as_str(), "httpflow" | "ctf_web_recon") => {
                    calls.insert(id.as_str(), (name.as_str(), input));
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } if !is_error.unwrap_or(false) && content.len() <= 64 * 1024 => {
                    match calls.remove(tool_use_id.as_str()) {
                        Some(("httpflow", input)) => {
                            use crate::alphacode_security_core::proof::HttpExchange;
                            if HttpExchange::parse_http_transcript("httpflow", input, content)
                                .is_some_and(|exchange| {
                                    exchange.response_headers.iter().any(|(key, value)| {
                                        key.eq_ignore_ascii_case("x-powered-by")
                                            && SkillRouter::is_nodejs_stack(value)
                                    })
                                })
                            {
                                return true;
                            }
                        }
                        Some(("ctf_web_recon", _)) => {
                            if let Ok(report) = serde_json::from_str::<serde_json::Value>(content)
                                && report["requests"].as_array().is_some_and(|rows| {
                                    rows.iter().any(|row| {
                                        row["headers"]["x-powered-by"]
                                            .as_str()
                                            .is_some_and(SkillRouter::is_nodejs_stack)
                                    })
                                })
                            {
                                return true;
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    false
}

/// Resolve guidance from the current task, engagement, and stack evidence.
pub(crate) fn routed_skill_context(
    skills: &crate::skill::SkillRegistry,
    session: &crate::session::Session,
    active_skill: Option<&str>,
    observed_node_stack: bool,
) -> (String, String) {
    let task = latest_task_text(&session.messages).unwrap_or_default();
    let selected = active_skill
        .map(str::to_owned)
        .or_else(|| skills.find_auto_invoke_match(&task));
    let mut selected_skills = Vec::new();
    if let Some(name) = selected.as_deref() {
        selected_skills.push(name);
    }

    let ctf_requested = active_skill.is_some_and(|name| matches!(name, "ctf" | "ctf-workflow"))
        || super::challenge_context::is_ctf_request(&task);
    let research_dir = crate::tool::research_ledger_relative_dir(&session.id);
    let ledger_dir = session
        .working_dir
        .as_deref()
        .map(|directory| std::path::Path::new(directory).join(&research_dir));
    let ledger = ledger_dir
        .as_deref()
        .and_then(super::challenge_context::ledger_context);
    let hunt_ledger = ledger_dir
        .as_deref()
        .and_then(super::challenge_context::hunt_ledger_context);
    let ctf = ctf_requested || ledger.is_some();
    let security = ctf
        || hunt_ledger.is_some()
        || security_request(&task)
        || selected
            .as_deref()
            .is_some_and(|name| matches!(name, "bugbounty" | "vuln-hunter" | "nodejs-security"))
        || crate::tool::scope::session_scope_policy(&session.id)
            .is_some_and(|policy| !policy.in_scope.is_empty());
    if security {
        let workflow = if ctf { "ctf-workflow" } else { "bugbounty" };
        if !selected_skills.contains(&workflow) {
            selected_skills.push(workflow);
        }
        if !selected_skills.contains(&"nodejs-security")
            && (observed_node_stack
                || SkillRouter::is_nodejs_stack(&task)
                || session
                    .working_dir
                    .as_deref()
                    .is_some_and(|path| node_project(std::path::Path::new(path))))
        {
            selected_skills.push("nodejs-security");
        }
    }
    let mut skill_prompt = selected_skills
        .into_iter()
        .filter_map(|name| skills.get(name))
        .map(|skill| skill.get_prompt())
        .collect::<Vec<_>>()
        .join("\n\n");

    // Bundled workflows refer to the legacy project-wide filenames. Bind
    // those references to this session without rewriting users' files.
    if security {
        skill_prompt = crate::tool::bind_research_ledger_paths(&skill_prompt, &session.id);
        skill_prompt.push_str(
            "\n\nChoose skills, references, and tools from the task, scope, and observed +             evidence. Consult the relevant class reference via skill_manage when needed. +             Use the available tool inventory to select the smallest useful experiment +             and adapt after each result. Proceed with authorized independent work when +             a prerequisite is missing. A technology hint cannot extend scope.",
        );
    }

    let evidence = [ledger, hunt_ledger]
        .into_iter()
        .flatten()
        .collect::<String>();
    (skill_prompt, evidence)
}

pub(super) fn latest_task_text(
    messages: &[crate::alphacode_session_types::StoredMessage],
) -> Option<String> {
    messages
        .iter()
        .rev()
        .filter(|message| message.role == Role::User)
        .find_map(|message| {
            let task = message
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text, .. } => {
                        let text = text.trim();
                        (!text.is_empty()
                            && !text.starts_with("<system-reminder>")
                            && !crate::todo::is_auto_poke_message(text))
                        .then_some(text)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            let continuation = task.trim_end_matches(['.', '!']).to_ascii_lowercase();
            (!task.is_empty() && !matches!(continuation.as_str(), "continue" | "keep going"))
                .then_some(task)
        })
}

impl Agent {
    /// Log how many tokens the request prefix costs this turn by prompt tier.
    pub(super) fn log_prompt_prefix_accounting(
        &self,
        split: &crate::prompt::SplitSystemPrompt,
        tools: &[ToolDefinition],
        tier: crate::prompt::PromptTier,
        messages: &[Message],
        capability_brief: &str,
    ) {
        let system_tokens = split
            .estimated_tokens()
            .saturating_add(crate::util::estimate_tokens(capability_brief));
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
        // These are byte counts, not provider token counts. They help separate
        // a large conversation or tool output from the fixed prompt without
        // logging user content, image data, or credentials.
        let mut text_bytes = 0usize;
        let mut result_bytes = 0usize;
        let mut images = 0usize;
        for block in messages.iter().flat_map(|message| &message.content) {
            match block {
                ContentBlock::Text { text, .. } => {
                    text_bytes = text_bytes.saturating_add(text.len())
                }
                ContentBlock::ToolResult { content, .. } => {
                    result_bytes = result_bytes.saturating_add(content.len())
                }
                ContentBlock::Image { .. } => images += 1,
                _ => {}
            }
        }
        logging::info(&format!(
            "Prompt history accounting: messages={} text_bytes={} tool_result_bytes={} images={} (excludes tool inputs, reasoning and injected memory)",
            messages.len(),
            text_bytes,
            result_bytes,
            images
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
    /// calls since the last analysis or hypothesis checkpoint. During security
    /// research this is the signature of the
    /// unattributed probe grind that burns whole sessions on blind requests;
    /// during ordinary scripted work it usually means the calls should have
    /// been batched. Injection is advisory (a system reminder), not an
    /// interruption.
    fn detect_probe_burst(&self, split: &mut crate::prompt::SplitSystemPrompt) {
        const SCAN_WINDOW: usize = 24;

        let mut tool_calls: Vec<(String, String)> = Vec::new();
        let msg_count = self.session.messages.len();
        let start = msg_count.saturating_sub(20);
        'outer: for stored in self.session.messages[start..].iter().rev() {
            for block in stored.content.iter().rev() {
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

        tool_calls.reverse();
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
        observed_node_stack: bool,
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
        let (skill_prompt, evidence) = routed_skill_context(
            &skills,
            &self.session,
            self.active_skill.as_deref(),
            observed_node_stack,
        );

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
            (!skill_prompt.is_empty()).then_some(skill_prompt.as_str()),
            &available_skills,
            self.session.is_canary,
            memory_prompt,
            working_dir.as_deref(),
            crate::prompt::PromptCapabilities::current(),
        );

        self.append_current_turn_system_reminder(&mut split);
        split.dynamic_part.push_str(&evidence);
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
            Some(_) if self.uses_compact_context() => crate::prompt::PromptTier::Compact,
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

    #[test]
    fn an_old_checkpoint_does_not_exempt_a_new_probe_grind() {
        let mut calls = vec![("todo".to_string(), "{}".to_string())];
        calls.extend((0..12).map(|n| bash(&format!("curl https://site/{n}"))));
        assert!(probe_burst_reminder(&calls).is_some());
    }
}

#[cfg(test)]
mod automatic_routing_tests {
    use super::*;

    #[test]
    fn observed_stack_is_read_from_a_matching_http_tool_result() {
        let mut session = crate::session::Session::create(None, None);
        let result = ContentBlock::ToolResult {
            tool_use_id: "call".into(),
            content: "HTTP 200 OK\nURL: https://fixture.test/\nX-Powered-By: Express\n\nfixture"
                .into(),
            is_error: Some(false),
        };
        session.add_message(Role::User, vec![result.clone()]);
        assert!(!observed_node_stack(&session.messages));
        session.add_message(
            Role::Assistant,
            vec![ContentBlock::ToolUse {
                id: "call".into(),
                name: "httpflow".into(),
                input: serde_json::json!({"url": "https://fixture.test/"}),
                thought_signature: None,
            }],
        );
        session.add_message(Role::User, vec![result]);
        assert!(observed_node_stack(&session.messages));
        assert!(
            adaptive_tool_query("Check this API for vulnerabilities", true).contains("httpflow")
        );
        assert!(!adaptive_tool_query("Solve this reversing CTF", false).contains("ctf_web_recon"));
    }
}
