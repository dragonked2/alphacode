//! Persistent memory event log for post-session analysis.
//!
//! Writes structured JSONL (one JSON object per line) to:
//!   `<AlphaCode home>/logs/memory-events-YYYY-MM-DD.jsonl`
//!
//! Every memory pipeline event - embedding search, sidecar verification,
//! injection, extraction, maintenance, tool actions - is captured with
//! wall-clock timestamps, session ID, and metadata only. Raw memory content,
//! prompts, tool arguments, and provider error bodies are intentionally omitted.
//!
//! Logs are retained for 14 days when cleanup runs; each daily file is capped
//! at 16 MiB on a best-effort basis. Excess records are dropped.

use crate::memory_types::MemoryEventKind;
use chrono::Local;
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static MEMORY_LOGGER: Mutex<Option<MemoryLogger>> = Mutex::new(None);
const MAX_DAILY_LOG_BYTES: u64 = 16 * 1024 * 1024;
static LOG_FAILURE_REPORTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn report_log_failure() {
    if !LOG_FAILURE_REPORTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("AlphaCode: memory event logging unavailable or daily limit reached");
    }
}

fn regular_or_absent(path: &std::path::Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(meta) => meta.file_type().is_file(),
        Err(err) => err.kind() == std::io::ErrorKind::NotFound,
    }
}

fn safe_label(value: &serde_json::Value, allowed: &[&str]) -> String {
    let Some(text) = value.as_str() else {
        return "other".into();
    };
    let normalized = text.to_ascii_lowercase();
    if allowed.contains(&normalized.as_str()) {
        normalized
    } else {
        "other".into()
    }
}

fn safe_session(value: &str) -> Option<String> {
    if !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        Some(value.to_owned())
    } else {
        None
    }
}

struct MemoryLogger {
    file: File,
    current_date: String,
    path: PathBuf,
    written: u64,
}

impl MemoryLogger {
    fn open(date: &str) -> Option<Self> {
        let dir = log_dir()?;
        fs::create_dir_all(&dir).ok()?;
        if !fs::symlink_metadata(&dir).ok()?.file_type().is_dir() {
            return None;
        }
        let path = dir.join(format!("memory-events-{}.jsonl", date));
        if !regular_or_absent(&path) {
            return None;
        }
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path).ok()?;
        let metadata = file.metadata().ok()?;
        if !metadata.is_file() {
            return None;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .ok()?;
        }
        Some(Self {
            file,
            current_date: date.to_owned(),
            path,
            written: metadata.len(),
        })
    }

    fn write_entry(&mut self, entry: &LogEntry) -> bool {
        let Ok(mut json) = serde_json::to_vec(entry) else {
            return false;
        };
        json.push(b'\n');
        // Refresh size to account for other appenders (not a cross-process lock).
        let Ok(meta) = self.file.metadata() else {
            return false;
        };
        self.written = meta.len();
        if self.written.saturating_add(json.len() as u64) > MAX_DAILY_LOG_BYTES {
            return false;
        }
        if self.file.write_all(&json).is_err() {
            return false;
        }
        self.written = self.written.saturating_add(json.len() as u64);
        true
    }
}

fn log_dir() -> Option<PathBuf> {
    crate::storage::alphacode_dir()
        .ok()
        .map(|root| root.join("logs"))
}

#[derive(Serialize)]
struct LogEntry {
    timestamp: String,
    session_id: Option<String>,
    event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<serde_json::Value>,
}

fn current_session_id() -> Option<String> {
    crate::logging::current_session()
}

fn write_log(event: &str, detail: Option<serde_json::Value>) {
    let now = Local::now();
    let date = now.format("%Y-%m-%d").to_string();

    let session_id = detail
        .as_ref()
        .and_then(|d| d.get("target_session"))
        .and_then(serde_json::Value::as_str)
        .and_then(safe_session)
        .or_else(|| current_session_id().as_deref().and_then(safe_session));
    let entry = LogEntry {
        timestamp: now.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string(),
        session_id,
        event: event.to_owned(),
        detail,
    };
    // Date check and write must share one lock to avoid midnight races.
    let Ok(mut guard) = MEMORY_LOGGER.lock() else {
        report_log_failure();
        return;
    };
    if guard.as_ref().is_none_or(|logger| {
        logger.current_date != date || !regular_or_absent(&logger.path) || !logger.path.is_file()
    }) {
        *guard = MemoryLogger::open(&date);
    }
    if !guard
        .as_mut()
        .is_some_and(|logger| logger.write_entry(&entry))
    {
        report_log_failure();
    }
}

/// Log a memory event from the in-memory event system.
pub fn log_event(kind: &MemoryEventKind) {
    let (event, detail) = event_metadata(kind);
    write_log(event, detail);
}

fn event_metadata(kind: &MemoryEventKind) -> (&'static str, Option<serde_json::Value>) {
    match kind {
        MemoryEventKind::EmbeddingStarted => ("embedding_started", None),

        MemoryEventKind::EmbeddingComplete { latency_ms, hits } => (
            "embedding_complete",
            Some(serde_json::json!({
                "latency_ms": latency_ms,
                "hits": hits,
            })),
        ),

        MemoryEventKind::SidecarStarted => ("sidecar_started", None),

        MemoryEventKind::SidecarRelevant { memory_preview } => (
            "sidecar_relevant",
            Some(serde_json::json!({
                "memory_preview_chars": memory_preview.chars().count(),
            })),
        ),

        MemoryEventKind::SidecarNotRelevant => ("sidecar_not_relevant", None),

        MemoryEventKind::SidecarComplete { latency_ms } => (
            "sidecar_complete",
            Some(serde_json::json!({
                "latency_ms": latency_ms,
            })),
        ),

        MemoryEventKind::MemorySurfaced { memory_preview } => (
            "memory_surfaced",
            Some(serde_json::json!({
                "memory_preview_chars": memory_preview.chars().count(),
            })),
        ),

        MemoryEventKind::MemoryInjected {
            count,
            prompt_chars,
            age_ms,
            preview,
            items,
        } => (
            "memory_injected",
            Some(serde_json::json!({
                "count": count,
                "prompt_chars": prompt_chars,
                "age_ms": age_ms,
                "preview_chars": preview.chars().count(),
                "item_count": items.len(),
            })),
        ),

        MemoryEventKind::MaintenanceStarted { verified, rejected } => (
            "maintenance_started",
            Some(serde_json::json!({
                "verified": verified,
                "rejected": rejected,
            })),
        ),

        MemoryEventKind::MaintenanceLinked { links } => (
            "maintenance_linked",
            Some(serde_json::json!({ "links": links })),
        ),

        MemoryEventKind::MaintenanceConfidence { boosted, decayed } => (
            "maintenance_confidence",
            Some(serde_json::json!({
                "boosted": boosted,
                "decayed": decayed,
            })),
        ),

        MemoryEventKind::MaintenanceCluster { clusters, members } => (
            "maintenance_cluster",
            Some(serde_json::json!({
                "clusters": clusters,
                "members": members,
            })),
        ),

        MemoryEventKind::MaintenanceTagInferred { tag, applied } => (
            "maintenance_tag_inferred",
            Some(serde_json::json!({
                "tag_chars": tag.chars().count(),
                "applied": applied,
            })),
        ),

        MemoryEventKind::MaintenanceGap { candidates } => (
            "maintenance_gap",
            Some(serde_json::json!({ "candidates": candidates })),
        ),

        MemoryEventKind::MaintenanceComplete { latency_ms } => (
            "maintenance_complete",
            Some(serde_json::json!({ "latency_ms": latency_ms })),
        ),

        MemoryEventKind::ExtractionStarted { reason } => (
            "extraction_started",
            Some(serde_json::json!({ "reason_chars": reason.chars().count() })),
        ),

        MemoryEventKind::ExtractionComplete { count } => (
            "extraction_complete",
            Some(serde_json::json!({ "count": count })),
        ),

        MemoryEventKind::Error { message } => (
            "error",
            Some(serde_json::json!({ "message_chars": message.chars().count() })),
        ),

        MemoryEventKind::ToolRemembered {
            content,
            scope,
            category,
        } => (
            "tool_remembered",
            Some(serde_json::json!({
                "content_chars": content.chars().count(),
                "scope": safe_label(&serde_json::to_value(scope).unwrap_or_default(), &["project", "global", "all"]),
                "category": safe_label(&serde_json::to_value(category).unwrap_or_default(), &["fact", "preference", "correction", "procedure", "skill", "note", "goal"]),
            })),
        ),

        MemoryEventKind::ToolRecalled { query, count } => (
            "tool_recalled",
            Some(serde_json::json!({
                "query_chars": query.chars().count(),
                "count": count,
            })),
        ),

        MemoryEventKind::ToolForgot { id } => (
            "tool_forgot",
            Some(serde_json::json!({ "id_chars": id.chars().count() })),
        ),

        MemoryEventKind::ToolTagged { id, tags } => (
            "tool_tagged",
            Some(serde_json::json!({
                "id_chars": id.chars().count(),
                "tag_count": tags.len(),
            })),
        ),

        MemoryEventKind::ToolLinked { from, to } => (
            "tool_linked",
            Some(serde_json::json!({
                "from_id_chars": from.chars().count(),
                "to_id_chars": to.chars().count(),
            })),
        ),

        MemoryEventKind::ToolListed { count } => {
            ("tool_listed", Some(serde_json::json!({ "count": count })))
        }
    }
}

/// Log when a pending memory is prepared (before it's actually injected).
pub fn log_pending_prepared(session_id: &str, prompt: &str, count: usize, memory_ids: &[String]) {
    write_log(
        "pending_prepared",
        Some(serde_json::json!({
            "target_session": safe_session(session_id),
            "count": count,
            "prompt_chars": prompt.chars().count(),

            "memory_id_count": memory_ids.len(),
        })),
    );
}

/// Log when memories are marked as injected (dedup tracking).
pub fn log_marked_injected(session_id: &str, ids: &[String]) {
    if ids.is_empty() {
        return;
    }
    write_log(
        "marked_injected",
        Some(serde_json::json!({
            "target_session": safe_session(session_id),
            "memory_id_count": ids.len(),
        })),
    );
}

/// Log when memories are marked as already known to a session without an
/// injection (e.g. extracted from that session's own transcript).
pub fn log_marked_known(session_id: &str, ids: &[String], reason: &str) {
    if ids.is_empty() {
        return;
    }
    write_log(
        "marked_known",
        Some(serde_json::json!({
            "target_session": safe_session(session_id),
            "memory_id_count": ids.len(),
            "reason_chars": reason.chars().count(),
        })),
    );
}

/// Log when a pending memory is consumed (actually injected into context).
pub fn log_pending_consumed(session_id: &str, count: usize, age_ms: u64, prompt_chars: usize) {
    write_log(
        "pending_consumed",
        Some(serde_json::json!({
            "target_session": safe_session(session_id),
            "count": count,
            "age_ms": age_ms,
            "prompt_chars": prompt_chars,
        })),
    );
}

/// Log when a pending memory is discarded (stale, duplicate, etc.)
pub fn log_pending_discarded(session_id: &str, reason: &str) {
    write_log(
        "pending_discarded",
        Some(serde_json::json!({
            "target_session": safe_session(session_id),
            "reason_chars": reason.chars().count(),
        })),
    );
}

/// Log topic change detection (which triggers extraction).
pub fn log_topic_change(session_id: &str, old_topic: &str, new_topic: &str) {
    write_log(
        "topic_change",
        Some(serde_json::json!({
            "target_session": safe_session(session_id),
            "old_topic_chars": old_topic.chars().count(),
            "new_topic_chars": new_topic.chars().count(),
        })),
    );
}

/// Log final extraction trigger (session end).
pub fn log_final_extraction(session_id: &str, transcript_chars: usize) {
    write_log(
        "final_extraction_started",
        Some(serde_json::json!({
            "target_session": safe_session(session_id),
            "transcript_chars": transcript_chars,
        })),
    );
}

/// Log embedding candidate filtering results.
pub fn log_candidate_filter(
    session_id: &str,
    total_candidates: usize,
    after_dedup: usize,
    context_preview: &str,
) {
    write_log(
        "candidate_filter",
        Some(serde_json::json!({
            "target_session": safe_session(session_id),
            "total_candidates": total_candidates,
            "after_dedup": after_dedup,
            "context_chars": context_preview.chars().count(),
        })),
    );
}

fn safe_decision(decision: &str) -> &str {
    // Keep the known metrics labels (both snake_case and enum Debug spelling).
    match decision {
        "judged" | "Judged" | "all_judges_failed" | "AllJudgesFailed" | "unparseable"
        | "Unparseable" | "transport_error" | "TransportError" | "cadence_carry"
        | "CadenceCarry" | "opted_out" | "OptedOut" | "no_backend" | "NoBackend"
        | "no_candidates" | "NoCandidates" => decision,
        _ => "other",
    }
}

/// Log the terminal judge decision for a memory surfacing turn. This is the
/// per-session attribution record backing the no-LLM conversion metric (see
/// `memory_judge_metrics`).
pub fn log_judge_decision(
    session_id: &str,
    decision: &str,
    no_llm: bool,
    degradation: bool,
    candidate_count: usize,
) {
    write_log(
        "judge_decision",
        Some(serde_json::json!({
            "target_session": safe_session(session_id),
            "decision": safe_decision(decision),
            "no_llm": no_llm,
            "degradation": degradation,
            "candidate_count": candidate_count,
        })),
    );
}

/// Remove `memory-events-*.jsonl` files older than the documented 14-day
/// retention window. The general log rotation deliberately leaves these files
/// alone (they are analysis data, not debug logs), so this is the only place
/// that prunes them.
pub fn cleanup_old_memory_logs() {
    if let Some(dir) = log_dir() {
        cleanup_old_memory_logs_in(&dir, Local::now());
    }
}

/// Core of [`cleanup_old_memory_logs`], parameterized on the directory and
/// "now" for unit testing without touching the real log directory.
fn cleanup_old_memory_logs_in(dir: &std::path::Path, now: chrono::DateTime<Local>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let cutoff = now - chrono::Duration::days(14);
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !(name.starts_with("memory-events-") && name.ends_with(".jsonl")) {
            continue;
        }
        if let Ok(metadata) = fs::symlink_metadata(entry.path())
            && metadata.file_type().is_file()
            && let Ok(modified) = metadata.modified()
        {
            let modified: chrono::DateTime<Local> = modified.into();
            if modified < cutoff {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    #[test]
    fn error_event_does_not_persist_raw_body() {
        let kind = MemoryEventKind::Error {
            message: "test-secret-value".into(),
        };
        let (name, detail) = event_metadata(&kind);
        assert_eq!(name, "error");
        let detail = detail.unwrap();
        assert_eq!(detail["message_chars"], 17);
        assert!(!detail.to_string().contains("test-secret-value"));
    }

    #[test]
    fn untrusted_metadata_is_bounded() {
        assert_eq!(safe_session("session-123").as_deref(), Some("session-123"));
        assert!(safe_session("secret\ntext").is_none());
        assert_eq!(safe_decision("raw provider response"), "other");
    }

    #[test]
    fn memory_log_cleanup_respects_14_day_window() {
        let dir = std::env::temp_dir().join(format!(
            "alphacode-mem-cleanup-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).expect("create temp dir");

        let write = |name: &str, age_days: u64| {
            let path = dir.join(name);
            let mut f = File::create(&path).expect("create");
            f.write_all(b"{}\n").ok();
            if age_days > 0 {
                let mtime = SystemTime::now() - Duration::from_secs(age_days * 24 * 60 * 60);
                f.set_modified(mtime).expect("set mtime");
            }
            path
        };

        // 20 days old: past the 14-day window -> deleted.
        let old = write("memory-events-2000-01-01.jsonl", 20);
        // 10 days old: within the window -> kept.
        let recent = write("memory-events-2000-02-01.jsonl", 10);
        // Old, but not a memory-events file -> must be left alone here.
        let other = write("alphacode-2000-01-01.log", 20);

        cleanup_old_memory_logs_in(&dir, Local::now());

        assert!(!old.exists(), "20-day-old memory log should be deleted");
        assert!(recent.exists(), "10-day-old memory log must survive");
        assert!(
            other.exists(),
            "non-memory files are out of scope for this cleanup"
        );

        fs::remove_dir_all(&dir).ok();
    }
}
