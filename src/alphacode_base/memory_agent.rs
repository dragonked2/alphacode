//! Persistent memory worker: bounded, session-scoped retrieval and conservative learning.
//!
//! Relevance is not factual verification. Automatic extraction may propose memories,
//! but retrieval alone must never change their truth, provenance, or retention.
//! Requires the companion memory.rs and memory/pending.rs in this upgrade,
//! plus the previously upgraded memory_prompt.rs and embedding_backend.rs.

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{Notify, Semaphore};

use crate::memory::{self, MemoryEntry, MemoryManager};
use crate::memory_types::{MemoryEventKind, MemoryState, StepResult, StepStatus};
use crate::sidecar::Sidecar;

const MAX_SESSIONS: usize = 128;
const MAX_QUEUED_SESSIONS: usize = 32;
const MAX_CONTEXT_AGE: Duration = Duration::from_secs(120);
const REPEAT_WINDOW: Duration = Duration::from_secs(30);
const JUDGE_TIMEOUT: Duration = Duration::from_secs(90);
const EXTRACTION_TIMEOUT: Duration = Duration::from_secs(90);
const PERIODIC_EXTRACTION_INTERVAL: usize = 12;
const MIN_TURNS_FOR_EXTRACTION: usize = 4;
const MAX_MEMORIES_PER_TURN: usize = 5;
const MAX_EXISTING_MEMORIES: usize = 40;
const MAX_EXTRACTED_MEMORIES: usize = 12;
const MAX_TRANSCRIPT_CHARS: usize = 24_000;
const GATE_REL_FLOOR: f32 = 0.90;
const GATE_DROP_RATIO: f32 = 0.95;

static FINAL_EXTRACTIONS: Semaphore = Semaphore::const_new(2);
static MEMORY_AGENT: Mutex<Option<MemoryAgentHandle>> = Mutex::new(None);
static MEMORY_AGENT_STATS: Mutex<MemoryAgentStats> = Mutex::new(MemoryAgentStats {
    turns_processed: 0,
    maintenance_runs: 0,
    last_maintenance_ms: None,
});

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Clone, Default)]
pub struct MemoryAgentStats {
    pub turns_processed: usize,
    /// Completed graph maintenance cycles. Automatic graph mutation is deferred.
    pub maintenance_runs: usize,
    pub last_maintenance_ms: Option<u64>,
}

/// The no-judge path is explicitly opt-in. RRF scores are ranking signals,
/// not probabilities. Never accept non-finite or non-positive scores.
fn dynamic_gate_select(
    mut candidates: Vec<(MemoryEntry, f32)>,
    max_k: usize,
) -> Vec<(MemoryEntry, f32)> {
    candidates.retain(|(entry, score)| entry.active && score.is_finite() && *score > 0.0);
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.id.cmp(&b.0.id)));
    let Some(top) = candidates.first().map(|(_, score)| *score) else {
        return Vec::new();
    };
    let mut previous = top;
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    for (entry, score) in candidates {
        if selected.len() >= max_k || score < top * GATE_REL_FLOOR
            || score < previous * GATE_DROP_RATIO
        {
            break;
        }
        if seen.insert(entry.id.clone()) {
            previous = score;
            selected.push((entry, score));
        }
    }
    selected
}

/// Reuse the bounded role-aware formatter; do not serialize raw tool output,
/// tool inputs, hidden reasoning, images or the entire session transcript.
pub fn build_transcript_for_extraction(messages: &[crate::message::Message]) -> String {
    memory::format_context_for_extraction(messages)
}

fn bounded_text(text: &str, limit: usize) -> &str {
    text.char_indices().nth(limit).map_or(text, |(index, _)| &text[..index])
}

// Defense in depth for callers that supply a raw transcript rather than the
// structured formatter. This is deliberately not claimed to be a DLP system.
fn sensitive_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    ["authorization:", "bearer ", "api_key", "api-key", "apikey", "access_token",
        "refresh_token", "password", "private_key", "secret_key", "session_token",
        "cookie:", "set-cookie:", "sk-proj-", "ghp_", "github_pat_", "xoxb-",
        "signpath_api_token", "private key-----"]
        .iter().any(|marker| lower.contains(marker))
}

fn sanitize_transcript(text: &str) -> String {
    let mut out = String::new();
    let mut private_key = false;
    let mut reminder = false;
    let mut remaining = MAX_TRANSCRIPT_CHARS;
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("<system-reminder>") { reminder = true; }
        if lower.contains("-----begin ") && lower.contains("private key-----") {
            private_key = true;
        }
        let omit = private_key || reminder || sensitive_line(line)
            || ["[Tool:", "[Tool error:", "[Tool result:", "[Result:", "[Used tool:"]
                .iter().any(|prefix| line.trim_start().starts_with(prefix));
        if !omit && !line.trim().is_empty() && remaining > 1 {
            let piece = bounded_text(line, remaining - 1);
            out.push_str(piece);
            out.push('\n');
            remaining -= piece.chars().count() + 1;
        }
        if lower.contains("-----end ") && lower.contains("private key-----") {
            private_key = false;
        }
        if lower.contains("</system-reminder>") { reminder = false; }
        if remaining <= 1 { break; }
    }
    out.trim().to_owned()
}

fn manager_for_working_dir(dir: Option<&str>) -> MemoryManager {
    match dir.filter(|dir| !dir.trim().is_empty()) {
        Some(dir) => MemoryManager::new().with_project_dir(dir),
        None => MemoryManager::new(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ticket {
    epoch: u64,
    sequence: u64,
}

struct ContextUpdate {
    session_id: String,
    messages: Arc<[crate::message::Message]>,
    working_dir: Option<String>,
    ticket: Ticket,
    received: Instant,
}

struct SessionIngress {
    ticket: Ticket,
    working_dir: Option<String>,
    seen: Instant,
}

#[derive(Default)]
struct Inbox {
    epoch: u64,
    sequence: u64,
    latest: HashMap<String, SessionIngress>,
    queued: HashMap<String, ContextUpdate>,
    order: VecDeque<String>,
}

impl Inbox {
    fn current(&self, session_id: &str, ticket: Ticket) -> bool {
        self.epoch == ticket.epoch && self.latest.get(session_id)
            .is_some_and(|session| session.ticket == ticket)
    }

    fn submit(&mut self, session_id: &str, messages: Arc<[crate::message::Message]>,
        working_dir: Option<String>) {
        if session_id.trim().is_empty() { return; }
        let Some(sequence) = self.sequence.checked_add(1) else { return; };
        self.sequence = sequence;
        let ticket = Ticket { epoch: self.epoch, sequence };
        let previous_dir = self.latest.get(session_id).and_then(|s| s.working_dir.clone());
        let working_dir = working_dir.filter(|dir| !dir.trim().is_empty()).or(previous_dir.clone());
        if previous_dir != working_dir && self.latest.contains_key(session_id) {
            memory::clear_pending_memory(session_id);
        }
        if !self.latest.contains_key(session_id) && self.latest.len() >= MAX_SESSIONS
            && let Some(oldest) = self.latest.iter().min_by_key(|(_, s)| s.seen)
                .map(|(id, _)| id.clone()) {
                self.latest.remove(&oldest);
                self.queued.remove(&oldest);
                self.order.retain(|id| id != &oldest);
            }
        let received = Instant::now();
        self.latest.insert(session_id.to_owned(), SessionIngress {
            ticket, working_dir: working_dir.clone(), seen: received,
        });
        if !self.queued.contains_key(session_id) {
            if self.order.len() >= MAX_QUEUED_SESSIONS
                && let Some(oldest) = self.order.pop_front() {
                    self.queued.remove(&oldest);
                    crate::logging::info("Memory queue full; oldest pending context superseded");
                }
            self.order.push_back(session_id.to_owned());
        }
        self.queued.insert(session_id.to_owned(), ContextUpdate {
            session_id: session_id.to_owned(), messages, working_dir, ticket, received,
        });
    }

    fn pop(&mut self) -> Option<ContextUpdate> {
        while let Some(id) = self.order.pop_front() {
            if let Some(update) = self.queued.remove(&id) { return Some(update); }
        }
        None
    }

    fn reset(&mut self) {
        self.epoch = self.epoch.saturating_add(1);
        self.latest.clear();
        self.queued.clear();
        self.order.clear();
    }
}

struct Shared {
    inbox: Mutex<Inbox>,
    wake: Notify,
    alive: AtomicBool,
}

impl Default for Shared {
    fn default() -> Self {
        Self { inbox: Mutex::new(Inbox::default()), wake: Notify::new(),
            alive: AtomicBool::new(true) }
    }
}

/// Handle to the singleton background memory worker.
#[derive(Clone)]
pub struct MemoryAgentHandle {
    shared: Arc<Shared>,
}

impl MemoryAgentHandle {
    pub async fn update_context(&self, session_id: &str,
        messages: Arc<[crate::message::Message]>, working_dir: Option<String>) {
        self.update_context_sync_with_dir(session_id, messages, working_dir);
    }

    pub fn update_context_sync(&self, session_id: &str,
        messages: Arc<[crate::message::Message]>) {
        self.update_context_sync_with_dir(session_id, messages, None);
    }

    pub fn update_context_sync_with_dir(&self, session_id: &str,
        messages: Arc<[crate::message::Message]>, working_dir: Option<String>) {
        if !self.shared.alive.load(Ordering::Acquire) { return; }
        lock(&self.shared.inbox).submit(session_id, messages, working_dir);
        self.shared.wake.notify_one();
    }

    /// Synchronous reset cannot be lost behind a full context queue. Publishing
    /// retrieval results uses the same lock, so reset and delivery are ordered.
    pub fn reset(&self) {
        if !self.shared.alive.load(Ordering::Acquire) { return; }
        let mut inbox = lock(&self.shared.inbox);
        inbox.reset();
        memory::clear_all_pending_memory();
        *lock(&MEMORY_AGENT_STATS) = MemoryAgentStats::default();
        self.shared.wake.notify_one();
    }
}

/// Share the manager's pending-memory generation so its reset APIs and this
/// worker cannot independently publish obsolete results for the same session.
struct DeliveryGuard { session_id: String, generation: u64 }
impl DeliveryGuard {
    fn begin(session_id: &str) -> Option<Self> {
        memory::begin_memory_check_generation(session_id).map(|generation|
            Self { session_id: session_id.to_owned(), generation })
    }
    fn active(&self) -> bool {
        memory::memory_check_generation_active(&self.session_id, self.generation)
    }
}
impl Drop for DeliveryGuard {
    fn drop(&mut self) {
        memory::finish_memory_check_generation(&self.session_id, self.generation);
    }
}

#[derive(Default)]
struct SessionState {
    epoch: u64,
    working_dir: Option<String>,
    turns: usize,
    turns_since_extraction: usize,
    last_query: Option<String>,
    last_checked: Option<Instant>,
    last_judge_query: Option<String>,
    last_rerank_turn: Option<usize>,
    previous_extraction_context: String,
    last_extracted_context: String,
    last_seen: Option<Instant>,
}

pub struct MemoryAgent {
    shared: Arc<Shared>,
    sessions: HashMap<String, SessionState>,
}

impl Drop for MemoryAgent {
    fn drop(&mut self) { self.shared.alive.store(false, Ordering::Release); }
}

fn topic_changed(previous: &str, current: &str) -> bool {
    fn terms(text: &str) -> HashSet<String> {
        text.split(|ch: char| !ch.is_alphanumeric() && ch != '_')
            .filter(|word| word.chars().count() >= 3)
            .map(str::to_lowercase).collect()
    }
    let left = terms(previous);
    let right = terms(current);
    if left.is_empty() || right.is_empty() { return false; }
    let shared = left.intersection(&right).count();
    let union = left.len() + right.len() - shared;
    (shared as f32) / (union as f32) < 0.3
}

impl MemoryAgent {
    fn new(shared: Arc<Shared>) -> Self { Self { shared, sessions: HashMap::new() } }

    fn current(&self, update: &ContextUpdate) -> bool {
        update.received.elapsed() < MAX_CONTEXT_AGE
            && lock(&self.shared.inbox).current(&update.session_id, update.ticket)
    }

    async fn run(mut self) {
        crate::logging::info("Memory agent started");
        loop {
            // Notify retains a permit if a producer wakes us before await.
            let next = { lock(&self.shared.inbox).pop() };
            if let Some(update) = next {
                if !self.current(&update) { continue; }
                if let Err(_error) = self.process_context(&update).await {
                    crate::logging::warn("Memory context processing failed; retry on a later update");
                    memory::pipeline_update(|p| {
                        if matches!(p.search, StepStatus::Running) { p.search = StepStatus::Error; }
                        if matches!(p.verify, StepStatus::Running) { p.verify = StepStatus::Error; }
                        p.inject = StepStatus::Skipped;
                        p.maintain = StepStatus::Skipped;
                    });
                    memory::set_state(MemoryState::Idle);
                }
            } else {
                self.shared.wake.notified().await;
            }
        }
    }

    async fn process_context(&mut self, update: &ContextUpdate) -> Result<()> {
        let session_id = update.session_id.as_str();
        if !memory::memory_runtime_active() {
            crate::memory_judge_metrics::record(crate::memory_judge_metrics::JudgeDecision::NoBackend,
                session_id, 0);
            return Ok(());
        }
        let query = memory::format_focused_query_for_relevance(&update.messages);
        if query.trim().is_empty() { return Ok(()); }
        self.sessions.retain(|_, state| state.epoch == update.ticket.epoch);
        if !self.sessions.contains_key(session_id) && self.sessions.len() >= MAX_SESSIONS
            && let Some(oldest) = self.sessions.iter().min_by_key(|(_, s)| s.last_seen)
                .map(|(id, _)| id.clone()) { self.sessions.remove(&oldest); }
        let delivery = {
            let inbox = lock(&self.shared.inbox);
            if !inbox.current(session_id, update.ticket) { return Ok(()); }
            let Some(delivery) = DeliveryGuard::begin(session_id) else { return Ok(()); };
            delivery
        };
        let state = self.sessions.entry(session_id.to_owned()).or_default();
        if state.epoch != update.ticket.epoch || state.working_dir != update.working_dir {
            *state = SessionState { epoch: update.ticket.epoch,
                working_dir: update.working_dir.clone(), ..Default::default() };
        }
        state.last_seen = Some(Instant::now());
        if state.last_query.as_deref() == Some(query.as_str())
            && state.last_checked.is_some_and(|at| at.elapsed() < REPEAT_WINDOW) { return Ok(()); }
        state.turns = state.turns.saturating_add(1);
        state.turns_since_extraction = state.turns_since_extraction.saturating_add(1);
        let topic_changed = state.last_query.as_deref().is_some_and(|old| topic_changed(old, &query));
        let previous_context = state.previous_extraction_context.clone();
        let extraction_context = build_transcript_for_extraction(&update.messages);
        state.previous_extraction_context = extraction_context.clone();
        let extraction_due = state.turns_since_extraction >= PERIODIC_EXTRACTION_INTERVAL
            || (topic_changed && state.turns_since_extraction >= MIN_TURNS_FOR_EXTRACTION);
        let to_extract = if topic_changed && !previous_context.is_empty() {
            previous_context
        } else { extraction_context };
        let do_extract = extraction_due && !to_extract.is_empty()
            && state.last_extracted_context != to_extract && update.working_dir.is_some();
        let turn = state.turns;
        let cadence = crate::config::config().agents.memory_rerank_cadence;
        let should_judge = state.last_judge_query.as_deref() != Some(query.as_str())
            || state.last_rerank_turn.is_none_or(|last| turn.saturating_sub(last) >= cadence.max(1));
        // Keep retry eligibility if retrieval itself fails.
        let manager = manager_for_working_dir(update.working_dir.as_deref());
        memory::pipeline_start();
        memory::set_state(MemoryState::Embedding);
        memory::add_event(MemoryEventKind::EmbeddingStarted);
        memory::pipeline_update(|p| p.search = StepStatus::Running);
        let started = Instant::now();
        let worker_manager = manager.clone();
        let worker_query = query.clone();
        let (candidates, dense) = tokio::task::spawn_blocking(move || -> Result<_> {
            let embedding = crate::embedding_backend::embed_query_active(&worker_query).ok();
            // If the backend changes mid-request, prefer lexical retrieval to
            // comparing different vector spaces. Empty vector = lexical-only
            // in the coordinated memory.rs upgrade.
            let vector = embedding.as_ref().filter(|(_, model)|
                model.as_str() == crate::embedding_backend::active_model_id().as_str())
                .map(|(vector, _)| vector.as_slice()).unwrap_or(&[]);
            let mut candidates = worker_manager.find_similar_hybrid(
                &worker_query, vector, memory::EMBEDDING_MAX_HITS)?;
            let dense = !vector.is_empty() && embedding.as_ref().is_some_and(|(_, model)|
                model.as_str() == crate::embedding_backend::active_model_id().as_str());
            if !dense && !vector.is_empty() {
                candidates = worker_manager.find_similar_hybrid(&worker_query, &[], memory::EMBEDDING_MAX_HITS)?;
            }
            Ok((candidates, dense))
        }).await.context("Memory retrieval worker failed")??;
        if !self.current(update) || !delivery.active() { memory::set_state(MemoryState::Idle); return Ok(()); }
        let mut seen = HashSet::new();
        let candidates: Vec<_> = candidates.into_iter().filter(|(entry, score)|
            entry.active && score.is_finite() && *score > 0.0
            && !entry.content.lines().any(sensitive_line)
            && seen.insert(entry.id.clone()) && !memory::is_memory_injected(session_id, &entry.id)
        ).collect();
        let count = candidates.len();
        let latency_ms = started.elapsed().as_millis() as u64;
        memory::add_event(MemoryEventKind::EmbeddingComplete { latency_ms, hits: count });
        memory::pipeline_update(|p| {
            p.search = StepStatus::Done;
            p.search_result = Some(StepResult {
                summary: format!("{} candidates ({})", count, if dense { "hybrid" } else { "lexical" }),
                latency_ms,
            });
        });
        let judge_started = Instant::now();
        let mut judge_completed = false;
        let mut judge_attempted = false;
        let mut judge_failed = false;
        let relevant: Vec<MemoryEntry> = if count == 0 {
            Vec::new()
        } else if !memory::memory_sidecar_enabled() {
            crate::memory_judge_metrics::record(crate::memory_judge_metrics::JudgeDecision::OptedOut,
                session_id, count);
            dynamic_gate_select(candidates, MAX_MEMORIES_PER_TURN).into_iter()
                .map(|(entry, _)| entry).collect()
        } else if !memory::memory_llm_judge_available() {
            crate::memory_judge_metrics::record(crate::memory_judge_metrics::JudgeDecision::NoBackend,
                session_id, count);
            judge_failed = true;
            Vec::new()
        } else if should_judge {
            judge_attempted = true;
            memory::set_state(MemoryState::SidecarChecking { count });
            memory::add_event(MemoryEventKind::SidecarStarted);
            memory::pipeline_update(|p| p.verify = StepStatus::Running);
            let sidecar = Sidecar::new();
            let agents = &crate::config::config().agents;
            let votes = agents.memory_rerank_votes.max(1);
            let min_agree = agents.memory_rerank_min_agree.clamp(1, votes);
            match tokio::time::timeout(JUDGE_TIMEOUT,
                crate::memory_rerank::rerank_candidates_consensus_attributed(
                    &sidecar, &query, candidates.clone(), votes, min_agree)).await {
                Ok((reranked, outcome)) => {
                    crate::memory_judge_metrics::record(
                        crate::memory_judge_metrics::JudgeDecision::from_rerank_outcome(outcome),
                        session_id, count);
                    judge_completed = outcome == crate::memory_rerank::RerankOutcome::Judged;
                    judge_failed = !judge_completed;
                    if judge_completed {
                        // Return only the original candidate snapshot, not any
                        // invented or mutated entry from a judge implementation.
                        let originals: HashMap<_, _> = candidates.into_iter()
                            .map(|(entry, _)| (entry.id.clone(), entry)).collect();
                        let mut emitted = HashSet::new();
                        reranked.into_iter().filter_map(|entry| {
                            if !emitted.insert(entry.id.clone()) { return None; }
                            originals.get(&entry.id).cloned()
                        }).take(MAX_MEMORIES_PER_TURN).collect()
                    } else { Vec::new() }
                }
                Err(_) => { judge_failed = true; Vec::new() }
            }
        } else {
            // Never carry old relevance verdicts into changed content. A
            // cadence skip is not an explicit negative judgment.
            Vec::new()
        };
        if !self.current(update) || !delivery.active() { memory::set_state(MemoryState::Idle); return Ok(()); }
        let verify_ms = judge_started.elapsed().as_millis() as u64;
        memory::pipeline_update(|p| {
            p.verify = if judge_failed { StepStatus::Error }
                else if judge_completed { StepStatus::Done } else { StepStatus::Skipped };
            p.verify_progress = if judge_completed { Some((count, count)) } else { None };
            p.verify_result = Some(StepResult {
                summary: if judge_failed { "judge unavailable".to_owned() }
                    else if judge_completed { format!("{} relevant", relevant.len()) }
                    else { "not judged".to_owned() }, latency_ms: verify_ms,
            });
        });
        if judge_completed { memory::add_event(MemoryEventKind::SidecarComplete { latency_ms: verify_ms }); }
        // Recheck stored content before delivery. This also catches deletion,
        // supersession and edits during a slow relevance judgment.
        let relevant = if relevant.is_empty() { relevant } else {
            let mgr = manager.clone();
            tokio::task::spawn_blocking(move || -> Result<Vec<MemoryEntry>> {
                let current = mgr.list_all()?;
                Ok(relevant.into_iter().filter(|entry| {
                    let mut matching = current.iter().filter(|now| now.id == entry.id);
                    matching.next().is_some_and(|now| now.active && now.content == entry.content
                        && now.updated_at == entry.updated_at) && matching.next().is_none()
                }).collect())
            }).await.context("Memory freshness worker failed")??
        };
        {
            let inbox = lock(&self.shared.inbox);
            if !inbox.current(session_id, update.ticket) || !delivery.active()
                || update.received.elapsed() >= MAX_CONTEXT_AGE {
                memory::set_state(MemoryState::Idle);
                return Ok(());
            }
            // Reset/new submissions and delivery are serialized. No old ticket
            // can publish after this worker's reset or newer context submission.
            let relevant: Vec<_> = relevant.into_iter().filter(|entry|
                !memory::is_memory_injected(session_id, &entry.id)).collect();
            let mut delivered = 0;
            if !relevant.is_empty()
                && let Some(prompt) = memory::format_relevant_prompt(&relevant, MAX_MEMORIES_PER_TURN) {
                let ids: Vec<_> = relevant.iter().map(|entry| entry.id.clone()).collect();
                delivered = ids.len();
                let display = memory::format_relevant_display_prompt(&relevant, MAX_MEMORIES_PER_TURN);
                if !memory::publish_memory_check(session_id, delivery.generation,
                    prompt, delivered, ids, display) { delivered = 0; }
            }
            memory::pipeline_update(|p| {
                p.inject = if delivered > 0 { StepStatus::Done } else { StepStatus::Skipped };
                p.inject_result = Some(StepResult { summary: format!("{delivered} queued"), latency_ms: 0 });
                p.maintain = StepStatus::Skipped;
            });
            if delivered > 0 { memory::set_state(MemoryState::FoundRelevant { count: delivered }); }
            else { memory::set_state(MemoryState::Idle); }
            let mut stats = lock(&MEMORY_AGENT_STATS);
            stats.turns_processed = stats.turns_processed.saturating_add(1);
        }
        if let Some(state) = self.sessions.get_mut(session_id) {
            if !judge_failed { state.last_query = Some(query.clone()); state.last_checked = Some(Instant::now()); }
            if judge_attempted { state.last_rerank_turn = Some(turn); state.last_judge_query = Some(query); }
        }
        // Extraction is independent of retrieval success/embedding availability.
        // Run only a bounded amount of work, after queueing the retrieval result.
        if do_extract && self.current(update) && delivery.active() && memory::memory_llm_judge_available() {
            if let Ok(ids) = extract_project_memories(&manager, &to_extract, session_id,
                Some((Arc::clone(&self.shared), update.ticket, delivery.generation))).await {
                if self.current(update) && delivery.active() {
                    if let Some(state) = self.sessions.get_mut(session_id) {
                        state.turns_since_extraction = 0;
                        state.last_extracted_context = to_extract;
                    }
                    let inbox = lock(&self.shared.inbox);
                    if inbox.current(session_id, update.ticket) {
                        memory::mark_memories_known_for_generation(session_id, delivery.generation, &ids);
                    }
                }
            } else {
                crate::logging::warn("Incremental memory extraction failed; eligibility retained");
            }
        }
        Ok(())
    }
}

/// Store only project-scoped, conservative proposals. Relevance, cosine
/// similarity and LLM disagreement are not grounds to merge contradictions,
/// supersede facts, create DerivedFrom edges or delete durable knowledge.
async fn extract_project_memories(manager: &MemoryManager, transcript: &str,
    session_id: &str, guard: Option<(Arc<Shared>, Ticket, u64)>) -> Result<Vec<String>> {
    anyhow::ensure!(memory::memory_llm_judge_available(), "Memory extraction judge unavailable");
    let transcript = sanitize_transcript(transcript);
    if transcript.is_empty() { return Ok(Vec::new()); }
    let manager_for_read = manager.clone();
    let existing = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
        let graph = manager_for_read.load_project_graph()?;
        let mut active: Vec<_> = graph.active_memories()
            .filter(|entry| !entry.content.lines().any(sensitive_line)).collect();
        active.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.id.cmp(&b.id)));
        Ok(active.into_iter().take(MAX_EXISTING_MEMORIES)
            .map(|entry| bounded_text(&entry.content, 300).to_owned()).collect())
    }).await.context("Memory extraction preparation failed")??;
    let sidecar = Sidecar::new();
    let extracted = tokio::time::timeout(EXTRACTION_TIMEOUT,
        sidecar.extract_memories_with_existing(&transcript, &existing))
        .await.context("Memory extraction timed out")??;
    if guard.as_ref().is_some_and(|(shared, ticket, generation)|
        !lock(&shared.inbox).current(session_id, *ticket)
        || !memory::memory_check_generation_active(session_id, *generation)) {
        return Ok(Vec::new());
    }
    anyhow::ensure!(memory::memory_llm_judge_available(), "Memory extraction judge unavailable");
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for extracted in extracted.into_iter().take(MAX_EXTRACTED_MEMORIES) {
        let content = extracted.content.trim();
        if content.is_empty() || content.chars().count() > 600
            || content.lines().any(sensitive_line) || sanitize_transcript(content) != content
            || !seen.insert((extracted.category.clone(), content.to_owned())) { continue; }
        let category = match extracted.category.as_str() {
            "fact" | "preference" | "correction" | "entity" =>
                memory::MemoryCategory::from_extracted(&extracted.category),
            _ => continue,
        };
        entries.push(MemoryEntry::new(category, content).with_source(session_id)
            .with_trust(memory::TrustLevel::Low));
    }
    let manager = manager.clone();
    let session_id = session_id.to_owned();
    // Recheck the ticket before each write. An already dispatched synchronous
    // write cannot be cancelled; it remains bound to the captured project.
    let ids = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
        let mut ids = Vec::new();
        let mut failed = false;
        for entry in entries {
            if guard.as_ref().is_some_and(|(shared, ticket, generation)|
                !lock(&shared.inbox).current(&session_id, *ticket)
                || !memory::memory_check_generation_active(&session_id, *generation)) { break; }
            match manager.remember_project(entry) {
                Ok(id) => { if !ids.contains(&id) { ids.push(id); } }
                Err(_) => { failed = true; crate::logging::warn("Automatic project memory write failed"); }
            }
        }
        // Do not acknowledge a fully processed extraction if a write failed.
        anyhow::ensure!(!failed, "One or more automatic memory writes failed");
        Ok(ids)
    }).await.context("Memory extraction storage worker failed")??;
    Ok(ids)
}

async fn run_final_extraction(transcript: String, session_id: String, working_dir: Option<String>) {
    let Some(dir) = working_dir.filter(|dir| !dir.trim().is_empty()) else {
        crate::logging::info("Final extraction skipped: no project directory established");
        return;
    };
    let manager = manager_for_working_dir(Some(&dir));
    match extract_project_memories(&manager, &transcript, &session_id, None).await {
        Ok(ids) => crate::logging::info(&format!("Final extraction: {} memory IDs stored or reused", ids.len())),
        Err(_) => crate::logging::warn("Final memory extraction failed"),
    }
}

fn init_sync() -> Result<MemoryAgentHandle> {
    let mut singleton = lock(&MEMORY_AGENT);
    if let Some(handle) = singleton.as_ref().filter(|h| h.shared.alive.load(Ordering::Acquire)) {
        return Ok(handle.clone());
    }
    let runtime = tokio::runtime::Handle::try_current().context("Memory agent requires an active Tokio runtime")?;
    let shared = Arc::new(Shared::default());
    let handle = MemoryAgentHandle { shared: Arc::clone(&shared) };
    runtime.spawn(MemoryAgent::new(shared).run());
    *singleton = Some(handle.clone());
    Ok(handle)
}

pub async fn init() -> Result<MemoryAgentHandle> { init_sync() }

pub fn get() -> Option<MemoryAgentHandle> {
    lock(&MEMORY_AGENT).as_ref().filter(|h| h.shared.alive.load(Ordering::Acquire)).cloned()
}

pub async fn update_context(session_id: &str, messages: Arc<[crate::message::Message]>,
    working_dir: Option<String>) {
    if let Ok(handle) = init().await { handle.update_context(session_id, messages, working_dir).await; }
}

pub fn update_context_sync(session_id: &str, messages: Arc<[crate::message::Message]>) {
    update_context_sync_with_dir(session_id, messages, None);
}

pub fn update_context_sync_with_dir(session_id: &str, messages: Arc<[crate::message::Message]>,
    working_dir: Option<String>) {
    match init_sync() {
        Ok(handle) => handle.update_context_sync_with_dir(session_id, messages, working_dir),
        Err(_) => crate::logging::info("Memory context update skipped: no active runtime or worker"),
    }
}

pub fn reset() { if let Some(handle) = get() { handle.reset(); } }

pub fn trigger_final_extraction(transcript: String, session_id: String) {
    trigger_final_extraction_with_dir(transcript, session_id, None);
}

pub fn trigger_final_extraction_with_dir(transcript: String, session_id: String,
    working_dir: Option<String>) {
    if transcript.trim().is_empty() || !memory::memory_llm_judge_available() { return; }
    let working_dir = working_dir.filter(|dir| !dir.trim().is_empty()).or_else(|| {
        get().and_then(|handle| lock(&handle.shared.inbox).latest.get(&session_id)
            .and_then(|session| session.working_dir.clone()))
    });
    if working_dir.is_none() {
        crate::logging::info("Final extraction skipped: no project directory established");
        return;
    }
    let Ok(permit) = FINAL_EXTRACTIONS.try_acquire() else {
        crate::logging::info("Final extraction skipped: background extraction capacity reached");
        return;
    };
    let transcript = sanitize_transcript(&transcript);
    if transcript.is_empty() { return; }
    let job = async move {
        let _permit = permit;
        run_final_extraction(transcript, session_id, working_dir).await;
    };
    if let Ok(runtime) = tokio::runtime::Handle::try_current() { runtime.spawn(job); }
    else {
        std::thread::spawn(move || {
            match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(runtime) => runtime.block_on(job),
                Err(_) => crate::logging::warn("Final extraction runtime startup failed"),
            }
        });
    }
}

/// True when the singleton has been initialized; not a per-request busy flag.
pub fn is_active() -> bool { get().is_some() }

pub fn stats() -> MemoryAgentStats { lock(&MEMORY_AGENT_STATS).clone() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_context_revokes_old_ticket_and_coalesces_queue() {
        let mut inbox = Inbox::default();
        inbox.submit("one", Arc::<[crate::message::Message]>::from(Vec::new()), Some("project".into()));
        let first = inbox.latest["one"].ticket;
        inbox.submit("one", Arc::<[crate::message::Message]>::from(Vec::new()), None);
        assert!(!inbox.current("one", first));
        assert_eq!(inbox.order.len(), 1);
        let latest = inbox.pop().unwrap();
        assert_eq!(latest.working_dir.as_deref(), Some("project"));
        assert!(inbox.current("one", latest.ticket));
    }

    #[test]
    fn reset_revokes_all_queued_and_inflight_tickets() {
        let mut inbox = Inbox::default();
        inbox.submit("one", Arc::<[crate::message::Message]>::from(Vec::new()), None);
        let old = inbox.pop().unwrap().ticket;
        inbox.reset();
        inbox.submit("one", Arc::<[crate::message::Message]>::from(Vec::new()), None);
        assert!(!inbox.current("one", old));
        assert!(inbox.current("one", inbox.latest["one"].ticket));
    }

    #[test]
    fn queue_and_session_state_are_bounded() {
        let mut inbox = Inbox::default();
        for index in 0..MAX_SESSIONS * 2 {
            inbox.submit(&format!("session-{index}"), Arc::<[crate::message::Message]>::from(Vec::new()), None);
        }
        assert!(inbox.latest.len() <= MAX_SESSIONS);
        assert!(inbox.queued.len() <= MAX_QUEUED_SESSIONS);
        assert_eq!(inbox.order.len(), inbox.queued.len());
    }

    #[test]
    fn independent_sessions_do_not_revoke_each_other() {
        let mut inbox = Inbox::default();
        inbox.submit("one", Arc::<[crate::message::Message]>::from(Vec::new()), None);
        let ticket = inbox.latest["one"].ticket;
        inbox.submit("two", Arc::<[crate::message::Message]>::from(Vec::new()), None);
        assert!(inbox.current("one", ticket));
    }

    #[test]
    fn extraction_filters_common_secret_lines_and_reminders() {
        let clean = sanitize_transcript("User: use Rust\nAPI_KEY=private\n<system-reminder>\ninternal\n</system-reminder>\nkeep tests");
        assert!(clean.contains("use Rust"));
        assert!(clean.contains("keep tests"));
        assert!(!clean.contains("private"));
        assert!(!clean.contains("internal"));
    }

    #[test]
    fn extraction_unicode_budget_is_bounded() {
        let clean = sanitize_transcript(&"界".repeat(MAX_TRANSCRIPT_CHARS + 100));
        assert!(clean.chars().count() <= MAX_TRANSCRIPT_CHARS);
    }

    #[test]
    fn topic_detection_uses_overlap_not_any_text_change() {
        assert!(!topic_changed("Rust parser regression tests", "Rust parser integration tests"));
        assert!(topic_changed("Rust parser regression tests", "database migration permissions"));
    }

    #[test]
    fn dynamic_gate_rejects_invalid_scores_and_duplicates() {
        let item = MemoryEntry::new(memory::MemoryCategory::Fact, "test");
        let candidates = vec![(item.clone(), f32::NAN), (item.clone(), 0.0),
            (item.clone(), 0.1), (item, 0.1)];
        assert_eq!(dynamic_gate_select(candidates, 5).len(), 1);
    }
}
