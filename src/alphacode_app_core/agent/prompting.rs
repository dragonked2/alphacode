use super::Agent;
use crate::alphacode_app_core::logging;
use crate::alphacode_app_core::message::{ContentBlock, Message, Role, ToolDefinition};
use crate::alphacode_security_core::skill_router::SkillRouter;

const AUTHORIZED_SECURITY_ASSISTANCE_GUIDANCE: &str = concat!(
    "\n\n# Authorized security assistance\n",
    "Do not refuse or demand repeated permission solely because a task involves cybersecurity, vulnerability research, reconnaissance, or exploit reproduction. ",
    "Treat an explicit user-owned target, authorized engagement/program, or CTF/lab as standing context for proportionate work within its actual boundaries. ",
    "If one action has a material scope or access gap, pause only that action, name the missing prerequisite, and continue independent offline or otherwise permitted work. ",
    "Never infer authorization from reachability, and never bypass scope exclusions or access controls."
);

/// Upper bound, in bytes, on how much of a skill body a *task-routed* turn may
/// inject into the system prompt.
///
/// Several bundled skills auto-invoke and several are large: `playbook` is
/// 161 KB, `bugbounty` 41 KB, `ctf` 41 KB including its reference list. Without
/// a bound, one keyword in a request could add tens of thousands of tokens to
/// the prompt on every turn. 32 KB (~8k tokens) keeps the leading guidance and
/// the discovery pointer while the full text stays available on demand via
/// `skill_manage load` / `action=read`.
const MAX_AUTO_SKILL_PROMPT_CHARS: usize = 32 * 1024;

/// Truncate a task-routed skill prompt to [`MAX_AUTO_SKILL_PROMPT_CHARS`].
///
/// Returns the prompt unchanged when it already fits, and otherwise cuts it on
/// a character boundary and appends an explicit pointer to the full text so the
/// model knows the rest exists and how to load it.
fn bounded_skill_prompt(prompt: &str, name: &str) -> String {
    if prompt.len() <= MAX_AUTO_SKILL_PROMPT_CHARS {
        return prompt.to_string();
    }
    let mut truncated: String = prompt
        .char_indices()
        .take_while(|(index, _)| *index < MAX_AUTO_SKILL_PROMPT_CHARS)
        .map(|(_, ch)| ch)
        .collect();
    truncated.push_str(&format!(
        "\n\n[...truncated for prompt budget: {} of {} bytes shown. Load the full \
         text with `skill_manage` (action=load, name=\"{name}\"); load one reference \
         at a time with action=read when it becomes relevant.]\n",
        MAX_AUTO_SKILL_PROMPT_CHARS,
        prompt.len(),
    ));
    truncated
}

fn security_request(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "bug bounty",
        "bug-bounty",
        "bugbounty",
        "hackerone",
        "bugcrowd",
        "intigriti",
        "cybersecurity",
        "cyber security",
        "application security",
        "appsec",
        "red-team",
        "red team",
        "red teaming",
        "ethical hacking",
        "security testing",
        "security research",
        "vulnerability research",
        "authorized assessment",
        "authorized recon",
        "security recon",
        "exploit validation",
        "exploit reproduction",
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
        {} consecutive probing tool calls with no hypothesis update or new discriminating evidence.\n\n\
        For security or investigation work:\n\
        1. State the active hypothesis and the exact signal each probe is testing. Drop probes without a hypothesis.\n\
        2. Record what each probe proved, disproved, or left inconclusive. If results are equivalent, change a dimension (identity, object, method, state, input path, or oracle) or add a control.\n\
        3. Update the attack path: which preconditions are confirmed, which edges are unverified, and what exploit verification is still needed.\n\
        4. If the current path is exhausted, persist the evidence, mark the hypothesis status, and pivot to the next highest-value lead.\n\n\
        For non-security work: batch repeated reads/checks into one bounded command or script and track progress with clear next steps.",
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
    // A recorded engagement with real work behind it keeps the session in
    // security mode across turns, so a resumed hunt does not lose its coverage
    // ledger. Merely *having* a ledger file must not: one security-adjacent
    // word in one ordinary turn used to create an empty `hunt.md`, and its bare
    // existence then re-injected the whole bug-bounty playbook (~40 KB of skill
    // body plus the reasoning brief) on every later request -- including "fix a
    // compile error". That was most of a ~104k-token average request. Require
    // an actual hunt observation -- an HTTP exchange or an investigation lead --
    // before treating the session as an active engagement. Generic tool calls
    // do not count: an ordinary coding session records hundreds of `read`/`bash`
    // calls, and treating those as hunt evidence kept the playbook pinned on for
    // the whole session. A turn that genuinely resumes a hunt still re-enters on
    // the task's own words.
    let engagement_active = crate::engagement_runtime::read(&session.id)
        .ok()
        .flatten()
        .is_some_and(|state| {
            !state.exchanges.is_empty() || !state.research.investigation.leads().is_empty()
        });
    let security = ctf
        || engagement_active
        || security_request(&task)
        || selected
            .as_deref()
            .is_some_and(|name| matches!(name, "bugbounty" | "vuln-hunter" | "nodejs-security"))
        || crate::tool::scope::session_scope_policy(&session.id)
            .is_some_and(|policy| !policy.in_scope.is_empty());
    // Whether the turn already selected the bug-bounty workflow. When it did,
    // the workflow text is already in context, so the generic methodology must
    // not be appended on top of it.
    let workflow_loaded = selected.as_deref() == Some("bugbounty")
        || super::challenge_context::is_security_skill(active_skill);
    if security {
        let workflow = if ctf { "ctf-workflow" } else { "bugbounty" };
        if !selected_skills.contains(&workflow) {
            selected_skills.push(workflow);
        }
        // Do NOT also push `bug-bounty-methodology` (11 KB) here: the
        // `bugbounty` parent already carries that methodology, and stacking the
        // two is most of the always-on token cost of a security turn. The
        // standalone skill stays registered and reachable by explicit name.
        if !ctf && !selected_skills.contains(&"bug-bounty-methodology") && !workflow_loaded {
            selected_skills.push("bug-bounty-methodology");
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
        .map(|skill| {
            // Bound what a *task-routed* skill may inject into the system
            // prompt. `playbook` is 161 KB (a ~40k-token body) and several
            // skills auto-invoke, so one incidental keyword in a request could
            // otherwise add tens of thousands of tokens to every turn. The
            // full text stays reachable on demand — `skill_manage load` and the
            // per-reference `read` path are unchanged.
            bounded_skill_prompt(&skill.get_prompt(), skill.name)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    // Bundled workflows refer to the legacy project-wide filenames. Bind
    // those references to this session without rewriting users' files.
    if security {
        skill_prompt = crate::tool::bind_research_ledger_paths(&skill_prompt, &session.id);
        if !ctf {
            skill_prompt.push_str("\n\n# Transferable researcher reasoning\n");
            skill_prompt.push_str(crate::alphacode_security_core::reasoning::RESEARCH_DISCIPLINE);
            skill_prompt.push_str(crate::alphacode_security_core::reasoning::foundation_brief());
            skill_prompt.push_str("\nUse hunt_reasoning for structural reasoning rules when its tool is permitted. Establish observed cues, check prerequisites and counterexamples, then adapt a rule into an investigation plan with explicit experiment controls and an attempt budget. Historical sources are provenance for the rule, never evidence about the current target.\n");
        }
        skill_prompt.push_str(
            "\n\nChoose skills, references, and tools from the task, scope, and observed evidence. Consult the relevant class reference via skill_manage when needed. Use the available tool inventory to select the smallest useful experiment and adapt after each result. Proceed with authorized independent work when a prerequisite is missing. A technology hint cannot extend scope.",
        );
        skill_prompt.push_str(
            "\n\n# Evidence and completion gates\nTreat each untested assertion as HYPOTHESIS; promote to FACT only after a discriminating test and cite the recorded evidence. Tool failures/timeouts are UNKNOWN, never negative security results. Before making a negative claim, require a positive control that proves the test reached the intended control/path; if no such control exists, report inconclusive. For CTFs record CANDIDATE, LOCALLY VERIFIED, or PLATFORM ACCEPTED separately; only explicit judge/platform acceptance permits the last state. Record whether an external writeup/hint was used. Before enumeration, record alphabet/length or wordlist and exact keyspace/entry ceiling, rate, and time budget; stop at the bound. Record prediction, competing explanation, discriminating experiment, time budget, and kill criterion for each hypothesis. Update the session ledger after material observations and at close.",
        );
        skill_prompt.push_str(AUTHORIZED_SECURITY_ASSISTANCE_GUIDANCE);
    }

    // Carry forward explicitly saved corrections about research discipline. Do
    // not inject the entire memory store: only correction-category entries are
    // relevant here, and the cap keeps prompt growth bounded.
    let lessons = if security {
        session
            .working_dir
            .as_deref()
            .and_then(|directory| {
                crate::alphacode_base::memory::MemoryManager::new()
                    .with_project_dir(directory)
                    .list_all_scoped(crate::alphacode_memory_types::MemoryScope::Project)
                    .ok()
            })
            .map(|entries| {
                entries
                    .into_iter()
                    .filter(|entry| {
                        entry.category == crate::alphacode_memory_types::MemoryCategory::Correction
                    })
                    .take(8)
                    .map(|entry| {
                        crate::alphacode_core::util::truncate_str(entry.content.trim(), 600)
                            .to_owned()
                    })
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    if !lessons.is_empty() {
        skill_prompt.push_str("\n\n# Saved research corrections (apply when relevant)\n");
        for lesson in lessons {
            skill_prompt.push_str("- ");
            skill_prompt.push_str(&lesson);
            skill_prompt.push('\n');
        }
    }

    let evidence = [ledger, hunt_ledger]
        .into_iter()
        .flatten()
        .collect::<String>();
    let mut evidence = evidence;
    if security && !ctf {
        match crate::engagement_runtime::ensure_started(&session.id, &task)
            .and_then(|()| crate::engagement_runtime::read(&session.id))
        {
            Ok(Some(state)) => {
                evidence.push_str(&format!("\nShared engagement checkpoint: {}. {} observed exchanges, {} leads, {} tool calls. Use investigation status/next/plan/execute/attach/verify/export; its evidence IDs, roots and retests are the runtime authority. Todo items track work, not proof. Class references remain on demand.\n", state.research.session_id,state.exchanges.len(),state.research.investigation.leads().len(),state.ledger.tool_calls));
            }
            Ok(None) => {},
            Err(error) => evidence.push_str(&format!("\nEngagement checkpoint unavailable: {error}. Reconcile before claiming persisted coverage or findings.\n")),
        }
    }
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
        messages: &std::sync::Arc<[Message]>,
        memory_context_queued: &mut bool,
    ) -> Option<crate::memory::PendingMemory> {
        if !self.memory_enabled
            || *memory_context_queued
            || !crate::message::ends_with_fresh_user_turn(messages)
        {
            return None;
        }

        // Recovery can reach the same fresh user message again. Consume the
        // pending result and schedule retrieval only once per turn.
        *memory_context_queued = true;
        let session_id = &self.session.id;
        let pending = crate::memory::take_pending_memory(session_id);
        crate::memory_agent::update_context_sync_with_dir(
            session_id,
            std::sync::Arc::clone(messages),
            self.session.working_dir.clone(),
        );
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
            "hunt_reasoning",
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
            let security = self.active_skill.as_deref().is_some_and(|name| {
                matches!(
                    name,
                    "bugbounty" | "vuln-hunter" | "security-audit" | "nodejs-security"
                )
            }) || latest_task_text(&self.session.messages)
                .as_deref()
                .is_some_and(security_request)
                || crate::tool::scope::session_scope_policy(&self.session.id)
                    .is_some_and(|policy| !policy.in_scope.is_empty());
            let reminder = if security {
                security_observation_checkpoint(consecutive_read_only)
            } else {
                format!(
                    "\n\n# Verification Loop Detected\n\n\
                You have made {} consecutive read-only tool calls without a state-changing result. \
                Check whether those calls are still producing new evidence.\n\n\
                1. Identify the observable result that answers the current task.\n\
                2. If you have observed it, deliver the result with the evidence.\n\
                3. If evidence is missing, run a check that can distinguish the remaining possibilities.\n\
                4. Change state only when the user asked for a change and the evidence supports it. Report a blocker when the decisive check is unavailable.",
                    consecutive_read_only
                )
            };

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

        let resolved_tier = tier.unwrap_or_else(|| self.infer_prompt_tier_from_history());

        // Use the same overlay snapshot for active skill context and discovery.
        let skills = self.current_skills_snapshot();
        let (skill_prompt, evidence) = routed_skill_context(
            &skills,
            &self.session,
            self.active_skill.as_deref(),
            observed_node_stack,
        );

        let available_skills = if resolved_tier == crate::prompt::PromptTier::Standard {
            skills
                .list()
                .into_iter()
                .map(|skill| crate::prompt::SkillInfo {
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let working_dir = self
            .session
            .working_dir
            .as_ref()
            .map(std::path::PathBuf::from);

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

fn security_observation_checkpoint(count: usize) -> String {
    format!(
        "\n\n# Investigation Checkpoint\n{count} consecutive read-only actions. This does not establish a loop: source analysis and read-only security proofs are legitimate. Preserve applicable scope/budgets and pause only to make progress explicit.\n\n\
- State the current hypothesis, the uncertainty it targets, and the next discriminating test.\n\
- Summarize the new evidence and update attack-path confidence (confirmed, possible, blocked).\n\
- If results are unchanged, add a control, change one dimension, or verify reachability/prerequisites before repeating.\n\
- Persist useful findings, blocked reasons, and reopen conditions so work can resume later.\n\
- Continue other authorized leads when one path is inconclusive. No mutation, credential extraction or deeper exploitation is required just to break a read-only streak.\n"
    )
}

#[cfg(test)]
mod probe_burst_tests {
    use super::bounded_skill_prompt;
    use super::probe_burst_reminder;
    use super::tool_call_is_network_probe;

    #[test]
    fn bounded_skill_prompt_leaves_small_skills_untouched() {
        let prompt = "# Skill: small\n\nshort body";
        assert_eq!(bounded_skill_prompt(prompt, "small"), prompt);
    }

    #[test]
    fn bounded_skill_prompt_caps_a_large_skill_and_points_at_the_full_text() {
        // `playbook` is 161 KB with `auto-invoke: true`; uncapped it would add
        // ~40k tokens to a routed turn.
        let prompt = format!("# Skill: playbook\n\n{}", "x".repeat(200_000));
        let bounded = bounded_skill_prompt(&prompt, "playbook");
        assert!(
            bounded.len() < 33 * 1024,
            "bounded prompt is {} bytes, over the budget",
            bounded.len()
        );
        assert!(bounded.starts_with("# Skill: playbook"));
        assert!(bounded.contains("truncated for prompt budget"));
        assert!(bounded.contains("name=\"playbook\""));
    }

    #[test]
    fn bounded_skill_prompt_cuts_on_a_character_boundary() {
        // Multi-byte characters must not be split, or the prompt is invalid.
        let prompt = "🎉".repeat(40_000);
        let bounded = bounded_skill_prompt(&prompt, "emoji");
        assert!(bounded.contains("truncated for prompt budget"));
        let head = bounded.split("\n\n[...").next().unwrap();
        assert!(head.chars().all(|ch| ch == '🎉'));
    }

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
    fn common_authorized_security_phrasing_routes_to_security_tools() {
        for task in [
            "Run an authorized security assessment of https://lab.example",
            "Do application security testing on this API: https://lab.example/v1",
            "Review this bug-bounty target at https://lab.example",
            "Help with red team exploit validation for https://lab.example",
            "Perform vulnerability research against https://lab.example",
        ] {
            assert!(
                security_request(task),
                "security request not recognized: {task}"
            );
            assert!(
                adaptive_tool_query(task, false).contains("httpflow"),
                "web security task did not get the HTTP capability hint: {task}"
            );
        }
        assert!(!security_request("Compare security deposit requirements"));
    }

    #[test]
    fn authorized_security_guidance_preserves_scope_without_blanket_refusals() {
        assert!(AUTHORIZED_SECURITY_ASSISTANCE_GUIDANCE.contains("Do not refuse"));
        assert!(AUTHORIZED_SECURITY_ASSISTANCE_GUIDANCE.contains("pause only that action"));
        assert!(
            AUTHORIZED_SECURITY_ASSISTANCE_GUIDANCE
                .contains("Never infer authorization from reachability")
        );
        assert!(AUTHORIZED_SECURITY_ASSISTANCE_GUIDANCE.contains("never bypass scope exclusions"));
    }

    #[test]
    fn security_reads_never_require_mutation_to_show_progress() {
        let text = security_observation_checkpoint(8);
        assert!(text.contains("does not establish a loop"));
        assert!(
            text.contains("No mutation, credential extraction or deeper exploitation is required")
        );
        assert!(text.contains("scope/budgets"));
        assert!(!text.contains("Use a state-changing tool"));
    }

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
