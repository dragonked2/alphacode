//! Logging infrastructure for alphacode
//!
//! Logs to ~/.alphacode/logs/ with automatic rotation
//!
//! Supports thread-local context for server, session, provider, and model info.

pub mod watchdog;

use chrono::Local;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Maximum number of live per-task logging contexts retained.
///
/// Every `async` task that logs creates a task id, and a task id is never
/// reused. Without a bound, the map grows by one entry per task that ever
/// logs, for the process lifetime (audit finding M25).
const MAX_TASK_CONTEXTS: usize = 256;

/// Age after which a rate-limit entry is considered stale and prunable.
///
/// Comfortably longer than any `min_interval` the callers use, so a live
/// throttle is never dropped (audit finding M29).
const RATE_LIMIT_PRUNE_AFTER: Duration = Duration::from_secs(3600);

/// Maximum number of retained rate-limit entries. A second backstop behind
/// the time-based prune, for the pathological case of many distinct keys
/// inside one prune window.
const MAX_RATE_LIMIT_ENTRIES: usize = 1024;

/// A task's logging context plus the sequence number used for LRU eviction.
struct TaskContextEntry {
    context: LogContext,
    /// Monotonic counter stamped on last use; the smallest value is evicted
    /// when the map is at capacity.
    last_used: u64,
}

/// Everything the logging path needs, behind **one** lock.
///
/// Previously the log path took three independent global mutexes — `LOGGER`
/// (H7), `TASK_LOG_CONTEXTS` (M25), and `RATE_LIMITS` (M29) — none held
/// together, so a rate-limited log line serialized across all three. Merging
/// them into a single state means a log line takes **one** lock.
struct LogState {
    /// The active logger, if logging was initialized.
    logger: Option<Logger>,
    /// Per-task logging contexts, keyed by `tokio::task::Id`.
    ///
    /// The key is a `TaskId` (which is `Copy + Hash + Eq`) rather than the
    /// rendered `String`, so the log path no longer allocates a `String` per
    /// call just to look up its own context.
    task_contexts: HashMap<tokio::task::Id, TaskContextEntry>,
    /// Rate-limit state, keyed by the caller's rate key.
    rate_limits: HashMap<String, RateLimitState>,
}

impl LogState {
    fn new() -> Self {
        Self {
            logger: None,
            task_contexts: HashMap::new(),
            rate_limits: HashMap::new(),
        }
    }

    /// Evict the least-recently-used task context if the map is at capacity.
    fn evict_task_contexts_if_full(&mut self) {
        while self.task_contexts.len() >= MAX_TASK_CONTEXTS {
            let Some(oldest) = self
                .task_contexts
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(id, _)| *id)
            else {
                break;
            };
            self.task_contexts.remove(&oldest);
        }
    }

    /// Drop rate-limit entries that are stale, and enforce the hard cap.
    fn prune_rate_limits(&mut self, now: Instant) {
        self.rate_limits.retain(|_, state| {
            now.saturating_duration_since(state.last_emit) < RATE_LIMIT_PRUNE_AFTER
        });
        while self.rate_limits.len() > MAX_RATE_LIMIT_ENTRIES {
            let Some(oldest) = self
                .rate_limits
                .iter()
                .min_by_key(|(_, state)| state.last_emit)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.rate_limits.remove(&oldest);
        }
    }
}

static LOG_STATE: std::sync::LazyLock<Mutex<LogState>> =
    std::sync::LazyLock::new(|| Mutex::new(LogState::new()));
static CLEANUP_STARTED: AtomicBool = AtomicBool::new(false);

/// Sequence counter for task-context LRU ordering.
static TASK_CONTEXT_CLOCK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Lock the log state, tolerating a poisoned mutex.
///
/// A panic inside a log call must not wedge the whole process, so the
/// poisoned value is recovered rather than propagated.
fn lock_log_state() -> std::sync::MutexGuard<'static, LogState> {
    match LOG_STATE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Whether verbose diagnostics are enabled for this process.
///
/// Presence is sufficient; even a non-Unicode value should enable tracing.
#[inline]
pub fn trace_enabled() -> bool {
    std::env::var_os("ALPHACODE_TRACE").is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

impl LogLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Debug => "DEBUG",
        }
    }

    fn is_enabled(self) -> bool {
        !matches!(self, Self::Debug) || trace_enabled()
    }
}

#[derive(Clone, Copy, Debug)]
struct RateLimitState {
    last_emit: Instant,
    suppressed: u64,
}

/// Thread-local logging context
#[derive(Default, Clone)]
pub struct LogContext {
    pub server: Option<String>,
    pub session: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

thread_local! {
    static LOG_CONTEXT: RefCell<LogContext> = RefCell::new(LogContext::default());
}

/// Update just the session in the current context
pub fn set_session(session: &str) {
    if with_task_context_mut(|ctx| {
        ctx.session = Some(session.to_string());
    }) {
        return;
    }

    LOG_CONTEXT.with(|c| {
        c.borrow_mut().session = Some(session.to_string());
    });
}

/// Update just the server in the current context
pub fn set_server(server: &str) {
    if with_task_context_mut(|ctx| {
        ctx.server = Some(server.to_string());
    }) {
        return;
    }

    LOG_CONTEXT.with(|c| {
        c.borrow_mut().server = Some(server.to_string());
    });
}

/// Update provider and model in the current context
pub fn set_provider_info(provider: &str, model: &str) {
    if with_task_context_mut(|ctx| {
        ctx.provider = Some(provider.to_string());
        ctx.model = Some(model.to_string());
    }) {
        return;
    }

    LOG_CONTEXT.with(|c| {
        let mut ctx = c.borrow_mut();
        ctx.provider = Some(provider.to_string());
        ctx.model = Some(model.to_string());
    });
}

/// Render the current context as a log-line prefix.
///
/// This reads the task-context map, so it takes the shared log-state lock.
/// It must be called *before* the logger is locked, because both now live
/// behind the same mutex.
fn log_prefix() -> String {
    if let Some(task_ctx) = task_context_snapshot() {
        return context_prefix_for(&task_ctx);
    }

    LOG_CONTEXT.with(|c| context_prefix_for(&c.borrow()))
}

/// The current task's id, if the caller is running inside a Tokio task.
///
/// `tokio::task::Id` is `Copy + Hash + Eq`, so it is used directly as the
/// map key — the previous code rendered it to a `String` on every log line
/// purely to look up its own context (audit finding M25).
fn current_task_id() -> Option<tokio::task::Id> {
    tokio::task::try_id()
}

fn with_task_context_mut(update: impl FnOnce(&mut LogContext)) -> bool {
    let Some(task_id) = current_task_id() else {
        return false;
    };

    let mut state = lock_log_state();
    state.evict_task_contexts_if_full();
    let clock = TASK_CONTEXT_CLOCK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let entry = state
        .task_contexts
        .entry(task_id)
        .or_insert_with(|| TaskContextEntry {
            context: LogContext::default(),
            last_used: clock,
        });
    entry.last_used = clock;
    update(&mut entry.context);
    true
}

fn task_context_snapshot() -> Option<LogContext> {
    let task_id = current_task_id()?;
    let mut state = lock_log_state();
    let clock = TASK_CONTEXT_CLOCK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let entry = state.task_contexts.get_mut(&task_id)?;
    entry.last_used = clock;
    Some(entry.context.clone())
}

/// Drop the current task's logging context.
///
/// Call this when a spawned task finishes so its context does not linger
/// until the LRU bound evicts it (audit finding M25).
pub fn clear_task_context() {
    let Some(task_id) = current_task_id() else {
        return;
    };
    lock_log_state().task_contexts.remove(&task_id);
}

/// Snapshot the current logging context for diagnostics that need stable,
/// session-scoped in-memory keys in addition to the rendered log prefix.
pub fn current_context_snapshot() -> LogContext {
    task_context_snapshot().unwrap_or_else(|| LOG_CONTEXT.with(|c| c.borrow().clone()))
}

fn context_prefix_for(ctx: &LogContext) -> String {
    let mut parts = Vec::new();

    if let Some(ref server) = ctx.server {
        parts.push(format!("srv:{}", server));
    }
    if let Some(ref session) = ctx.session {
        // Truncate on a character boundary. Session names can contain Unicode,
        // so slicing the first 20 bytes can panic in the logging path.
        let short = session.chars().take(20).collect::<String>();
        parts.push(format!("ses:{short}"));
    }
    if let Some(ref provider) = ctx.provider {
        parts.push(format!("prv:{}", provider));
    }
    if let Some(ref model) = ctx.model {
        // Just use first part of model name
        let short = model.split('-').next().unwrap_or(model);
        parts.push(format!("mod:{}", short));
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!("[{}] ", parts.join("|"))
    }
}

/// The buffered log writer.
///
/// `std::fs::File` is **unbuffered**: every `write_all` is a `write(2)`
/// syscall, and the old code issued one per log line while holding the
/// global logger mutex. On a mechanical or network-backed disk that is
/// 0.1-5ms per line, and it serializes every logging thread in the process
/// for the duration of the disk write.
///
/// A `BufWriter` turns that into a memcpy into an in-process buffer, with
/// the syscall amortized across many lines. The buffer is flushed on a
/// background timer (see [`spawn_flush_thread`]) and explicitly on shutdown.
pub struct Logger {
    file: BufWriter<File>,
    /// Timestamp of the last flush, for the periodic-flush check.
    last_flush: Instant,
}

/// How often buffered log lines are written to disk.
const FLUSH_INTERVAL: Duration = Duration::from_millis(250);

/// How much log data is buffered before an automatic flush is forced.
const FLUSH_BUFFER_BYTES: usize = 64 * 1024;

fn log_dir() -> Option<PathBuf> {
    crate::alphacode_storage::logs_dir().ok()
}

impl Logger {
    fn new() -> Option<Self> {
        let log_dir = log_dir()?;
        crate::alphacode_storage::ensure_dir(&log_dir).ok()?;

        // Use date-based log file
        let date = Local::now().format("%Y-%m-%d");
        let path = log_dir.join(format!("alphacode-{}.log", date));

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()?;

        Some(Self {
            file: BufWriter::with_capacity(FLUSH_BUFFER_BYTES, file),
            last_flush: Instant::now(),
        })
    }

    /// Write one log line using an already-rendered context prefix.
    ///
    /// The prefix is rendered by the caller (`log_prefix()`) *before* the
    /// shared log-state lock is taken. Rendering it here would re-enter the
    /// lock that owns the task-context map and deadlock, because the logger
    /// and the context map now share a single mutex (audit finding M25).
    fn write_with_prefix(&mut self, level: &str, message: &str, prefix: &str) {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let line = format!("[{}] [{}] {}{}\n", timestamp, level, prefix, message);
        if let Err(err) = self.file.write_all(line.as_bytes()) {
            eprintln!("alphacode logger write failed: {err}");
            return;
        }
        // Flush when the interval elapsed. The check is a cheap `Instant`
        // read, and it keeps the log's on-disk latency bounded to
        // `FLUSH_INTERVAL` rather than growing without bound.
        if self.last_flush.elapsed() >= FLUSH_INTERVAL {
            self.flush();
        }
    }

    /// Write buffered lines to disk.
    fn flush(&mut self) {
        if let Err(err) = self.file.flush() {
            eprintln!("alphacode logger flush failed: {err}");
            return;
        }
        self.last_flush = Instant::now();
    }
}

impl Drop for Logger {
    fn drop(&mut self) {
        // Best-effort final flush so the tail of the log is not lost when the
        // process exits without an explicit shutdown.
        let _ = self.file.flush();
    }
}

/// Initialize the logger (call once at startup)
pub fn init() {
    {
        let mut state = lock_log_state();
        if state.logger.is_none() {
            state.logger = Logger::new();
        }
    }

    // Prune stale daily log files once per process, off the startup path so
    // disk I/O never blocks launch. cleanup_old_logs is scoped to our own
    // `alphacode-*.log` files, so it is safe to run unconditionally.
    // Detect silent hangs in long sessions: without this a freeze leaves the
    // log simply stopping, with no phase or thread-state evidence.
    watchdog::start();

    if !CLEANUP_STARTED.swap(true, Ordering::SeqCst) {
        std::thread::Builder::new()
            .name("alphacode-log-cleanup".to_string())
            .spawn(cleanup_old_logs)
            .ok();
    }

    // Keep the log buffer from going stale during quiet periods.
    spawn_flush_thread();
}

/// Flush any buffered log lines to disk.
///
/// Call this before a crash-sensitive path, before an explicit shutdown, or
/// whenever the caller needs the log file to be up to date on disk.
pub fn flush() {
    let mut state = lock_log_state();
    if let Some(logger) = state.logger.as_mut() {
        logger.flush();
    }
}

/// Background thread that flushes the log buffer on a fixed interval.
///
/// The buffer is also flushed inline whenever a write happens more than
/// `FLUSH_INTERVAL` after the last flush, so this thread is a *bound* on
/// staleness for quiet periods — not the only flush path. It exists so a
/// single quiet second cannot leave the tail of the log unwritten.
fn spawn_flush_thread() {
    static FLUSH_THREAD_STARTED: AtomicBool = AtomicBool::new(false);
    if !FLUSH_THREAD_STARTED.swap(true, Ordering::SeqCst) {
        std::thread::Builder::new()
            .name("alphacode-log-flush".to_string())
            .spawn(|| {
                loop {
                    std::thread::sleep(FLUSH_INTERVAL);
                    flush();
                }
            })
            .ok();
    }
}

/// Log an info message
pub fn info(message: &str) {
    let prefix = log_prefix();
    let mut state = lock_log_state();
    if let Some(logger) = state.logger.as_mut() {
        logger.write_with_prefix("INFO", message, &prefix);
    }
}

/// Minimum interval between two emissions of the same throttled log key.
const THROTTLE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// Last-emission timestamps for [`info_throttled`], keyed by a static label.
/// Bounded implicitly by the number of distinct call-site keys (small, static).
static THROTTLE_STATE: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<&'static str, std::time::Instant>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Log `message` at INFO level, but at most once per [`THROTTLE_INTERVAL`]
/// per `key`. Cheap no-op (a single `Instant::now` + map probe) when the
/// interval has not elapsed. Use this in hot loops (per-tool-call lifecycle
/// lines, per-iteration turn summaries) where the content repeats and the
/// wall-clock timing it carries is still served by a non-throttled line
/// elsewhere.
///
/// The first call for a key always logs, so one-off keys pay only map-insert
/// cost. The message argument is not formatted by the caller until this
/// returns true — gate the `format!` on the returned bool.
pub fn info_throttled(key: &'static str, message: &str) -> bool {
    if should_emit_throttled(key) {
        info(message);
        true
    } else {
        false
    }
}

/// Lazy variant of [`info_throttled`] for hot paths whose message formatting
/// is more expensive than the throttle lookup itself.
pub fn info_throttled_with(key: &'static str, message: impl FnOnce() -> String) -> bool {
    if should_emit_throttled(key) {
        info(&message());
        true
    } else {
        false
    }
}

fn should_emit_throttled(key: &'static str) -> bool {
    let now = std::time::Instant::now();
    let mut state = match THROTTLE_STATE.lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    match state.get(key) {
        Some(last) if now.saturating_duration_since(*last) < THROTTLE_INTERVAL => false,
        _ => {
            state.insert(key, now);
            true
        }
    }
}

/// Log an error message
pub fn error(message: &str) {
    let prefix = log_prefix();
    let mut state = lock_log_state();
    if let Some(logger) = state.logger.as_mut() {
        logger.write_with_prefix("ERROR", message, &prefix);
    }
}

/// Log a warning message
pub fn warn(message: &str) {
    let prefix = log_prefix();
    let mut state = lock_log_state();
    if let Some(logger) = state.logger.as_mut() {
        logger.write_with_prefix("WARN", message, &prefix);
    }
}

/// Truncate a value for inclusion in a log line, appending an ellipsis marker
/// with the original length when cut. Char-boundary safe. Use this for
/// payload-bearing values (event debug dumps, request bodies) so a routine
/// warning cannot flood the log with a multi-kilobyte line.
pub fn truncate_for_log(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let truncated: String = value.chars().take(max_chars).collect();
    format!("{}… [{} chars total]", truncated, value.chars().count())
}

/// Log a debug message (only if ALPHACODE_TRACE is set)
pub fn debug(message: &str) {
    if trace_enabled() {
        let prefix = log_prefix();
        let mut state = lock_log_state();
        if let Some(logger) = state.logger.as_mut() {
            logger.write_with_prefix("DEBUG", message, &prefix);
        }
    }
}

pub fn event_info<K, V, I>(event_name: &str, fields: I)
where
    K: AsRef<str>,
    V: ToString,
    I: IntoIterator<Item = (K, V)>,
{
    event(LogLevel::Info, event_name, fields);
}

pub fn event_warn<K, V, I>(event_name: &str, fields: I)
where
    K: AsRef<str>,
    V: ToString,
    I: IntoIterator<Item = (K, V)>,
{
    event(LogLevel::Warn, event_name, fields);
}

pub fn event_error<K, V, I>(event_name: &str, fields: I)
where
    K: AsRef<str>,
    V: ToString,
    I: IntoIterator<Item = (K, V)>,
{
    event(LogLevel::Error, event_name, fields);
}

pub fn event_debug<K, V, I>(event_name: &str, fields: I)
where
    K: AsRef<str>,
    V: ToString,
    I: IntoIterator<Item = (K, V)>,
{
    event(LogLevel::Debug, event_name, fields);
}

pub fn event<K, V, I>(level: LogLevel, event: &str, fields: I)
where
    K: AsRef<str>,
    V: ToString,
    I: IntoIterator<Item = (K, V)>,
{
    if !level.is_enabled() {
        return;
    }
    write_level(level, &format_structured_event(event, fields));
}

pub fn event_rate_limited<K, V, I>(
    level: LogLevel,
    rate_key: &str,
    min_interval: Duration,
    event: &str,
    fields: I,
) where
    K: AsRef<str>,
    V: ToString,
    I: IntoIterator<Item = (K, V)>,
{
    if !level.is_enabled() {
        return;
    }

    let now = Instant::now();
    let mut suppressed = 0;
    let should_emit = {
        // The rate-limit state now lives in the shared log state, so a
        // rate-limited log line takes **one** global lock instead of three
        // (`LOGGER` + `TASK_LOG_CONTEXTS` + `RATE_LIMITS`).
        let mut state = lock_log_state();
        // Prune stale entries before inserting, so a never-reused key (an MCP
        // server name, for example) cannot grow the map without bound
        // (audit finding M29).
        state.prune_rate_limits(now);
        match state.rate_limits.get_mut(rate_key) {
            Some(entry) if now.duration_since(entry.last_emit) < min_interval => {
                entry.suppressed = entry.suppressed.saturating_add(1);
                false
            }
            Some(entry) => {
                suppressed = entry.suppressed;
                entry.suppressed = 0;
                entry.last_emit = now;
                true
            }
            None => {
                state.rate_limits.insert(
                    rate_key.to_string(),
                    RateLimitState {
                        last_emit: now,
                        suppressed: 0,
                    },
                );
                true
            }
        }
    };

    if !should_emit {
        return;
    }

    let mut fields: Vec<(String, String)> = fields
        .into_iter()
        .map(|(key, value)| (key.as_ref().to_string(), value.to_string()))
        .collect();
    if suppressed > 0 {
        fields.push(("suppressed".to_string(), suppressed.to_string()));
    }
    write_level(level, &format_structured_event(event, fields));
}

fn write_level(level: LogLevel, message: &str) {
    let prefix = log_prefix();
    let mut state = lock_log_state();
    if let Some(logger) = state.logger.as_mut() {
        logger.write_with_prefix(level.as_str(), message, &prefix);
    }
}

fn format_structured_event<K, V, I>(event: &str, fields: I) -> String
where
    K: AsRef<str>,
    V: ToString,
    I: IntoIterator<Item = (K, V)>,
{
    let event = sanitize_log_value(event);
    let mut ordered = BTreeMap::new();
    for (key, value) in fields {
        let raw_key = key.as_ref();
        let key = sanitize_log_key(raw_key);
        let value = redact_auth_field(raw_key, &value.to_string());
        ordered.insert(key, value);
    }

    if structured_json_enabled() {
        let mut object = serde_json::Map::new();
        object.insert("event".to_string(), serde_json::Value::String(event));
        for (key, value) in ordered {
            object.insert(key, serde_json::Value::String(value));
        }
        return format!("EVENT_JSON {}", serde_json::Value::Object(object));
    }

    let mut parts = Vec::with_capacity(ordered.len() + 1);
    parts.push(format!("event={}", format_log_field_value(&event)));
    parts.extend(
        ordered
            .into_iter()
            .map(|(key, value)| format!("{}={}", key, format_log_field_value(&value))),
    );
    format!("EVENT {}", parts.join(" "))
}

fn structured_json_enabled() -> bool {
    matches!(
        std::env::var("ALPHACODE_LOG_JSON").as_deref(),
        Ok("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

fn sanitize_log_key(key: &str) -> String {
    let sanitized: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "field".to_string()
    } else {
        truncate(&sanitized, 80)
    }
}

fn format_log_field_value(value: &str) -> String {
    if value.is_empty()
        || value
            .chars()
            .any(|c| c.is_whitespace() || c == '=' || c == '"' || c == '\'')
    {
        serde_json::to_string(value).unwrap_or_else(|_| "\"<unserializable>\"".to_string())
    } else {
        value.to_string()
    }
}

/// Log a structured auth event with conservative redaction.
///
/// Callers should pass only non-secret metadata. This function still redacts any
/// field whose key looks credential-like so accidental tokens/keys do not land in
/// logs.
pub fn auth_event(event: &str, provider: &str, fields: &[(&str, &str)]) {
    let mut parts = vec![
        format!("event={}", sanitize_log_value(event)),
        format!("provider={}", sanitize_log_value(provider)),
    ];
    for (key, value) in fields {
        parts.push(format!(
            "{}={}",
            sanitize_log_value(key),
            redact_auth_field(key, value)
        ));
    }
    let msg = format!("AUTH {}", parts.join(" "));
    let prefix = log_prefix();
    let mut state = lock_log_state();
    if let Some(logger) = state.logger.as_mut() {
        logger.write_with_prefix("AUTH", &msg, &prefix);
    }
}

/// Log a tool call
pub fn tool_call(name: &str, input: &str, output: &str) {
    let msg = format!(
        "TOOL[{}] input={} output={}",
        name,
        truncate(input, 200),
        truncate(output, 500)
    );
    let prefix = log_prefix();
    let mut state = lock_log_state();
    if let Some(logger) = state.logger.as_mut() {
        logger.write_with_prefix("TOOL", &msg, &prefix);
    }
}

/// Log a crash/panic for auto-debug
pub fn crash(error: &str, context: &str) {
    let msg = format!("CRASH: {} | Context: {}", error, context);
    let prefix = log_prefix();
    let mut state = lock_log_state();
    if let Some(logger) = state.logger.as_mut() {
        logger.write_with_prefix("CRASH", &msg, &prefix);
    }
}

/// Get the session ID from the current logging context (thread-local or task-local).
pub fn current_session() -> Option<String> {
    if let Some(ctx) = task_context_snapshot() {
        return ctx.session;
    }
    LOG_CONTEXT.with(|c| c.borrow().session.clone())
}

/// Get path to today's log file
pub fn log_path() -> Option<PathBuf> {
    let log_dir = log_dir()?;
    let date = Local::now().format("%Y-%m-%d");
    Some(log_dir.join(format!("alphacode-{}.log", date)))
}

/// Remove daily `alphacode-*.log` / `alphacode-desktop-*.log` files older than 7 days.
///
/// Scoped deliberately to the date-stamped log files this logger produces. The
/// log directory also holds non-log data (e.g. `memory/`, `memory-events-*.jsonl`)
/// that must NOT be touched here, so we never blanket-delete by mtime.
pub fn cleanup_old_logs() {
    if let Some(log_dir) = log_dir() {
        cleanup_old_logs_in(&log_dir, Local::now());
    }
}

/// Core of [`cleanup_old_logs`], parameterized on the directory and "now" so it
/// can be unit-tested without touching the real log directory or process env.
fn cleanup_old_logs_in(log_dir: &std::path::Path, now: chrono::DateTime<Local>) {
    let Ok(entries) = fs::read_dir(log_dir) else {
        return;
    };
    let cutoff = now - chrono::Duration::days(7);
    for entry in entries.flatten() {
        // Only consider our own date-stamped log files.
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let is_alphacode_log = (name.starts_with("alphacode-")
            || name.starts_with("alphacode-desktop-"))
            && name.ends_with(".log");
        if !is_alphacode_log {
            continue;
        }

        if let Ok(metadata) = entry.metadata()
            && metadata.is_file()
            && let Ok(modified) = metadata.modified()
        {
            let modified: chrono::DateTime<Local> = modified.into();
            if modified < cutoff
                && let Err(err) = fs::remove_file(entry.path())
            {
                eprintln!("alphacode logger cleanup failed: {err}");
            }
        }
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!(
            "{}...",
            crate::alphacode_core::util::truncate_str(s, max_len)
        )
    } else {
        s.to_string()
    }
}

fn redact_auth_field(key: &str, value: &str) -> String {
    let key = key.to_ascii_lowercase();
    if key.contains("token")
        || key.contains("secret")
        || key.contains("key")
        || key.contains("credential")
        || key.contains("callback")
        || key.contains("authorization")
        || key.contains("auth_code")
        || key.contains("oauth_code")
        || key.contains("code_verifier")
        || key.contains("code_challenge")
    {
        return "<redacted>".to_string();
    }
    sanitize_log_value(value)
}

fn sanitize_log_value(value: &str) -> String {
    let value = value.replace(['\n', '\r', '\t'], " ");
    let value = redact_url_queries(&value);
    truncate(&value, 160)
}

fn redact_url_queries(value: &str) -> String {
    value
        .split(' ')
        .map(|word| {
            if (word.starts_with("http://") || word.starts_with("https://")) && word.contains('?') {
                let (base, _) = word.split_once('?').unwrap_or((word, ""));
                format!("{}?<redacted>", base)
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_log_redacts_secret_like_fields() {
        assert_eq!(redact_auth_field("api_key", "sk-secret"), "<redacted>");
        assert_eq!(
            redact_auth_field("callback_url", "https://example.com/?code=secret"),
            "<redacted>"
        );
    }

    #[test]
    fn auth_log_sanitizes_urls_and_control_characters() {
        assert_eq!(
            sanitize_log_value("failed\nhttps://login.example.com/cb?code=secret&state=abc"),
            "failed https://login.example.com/cb?<redacted>"
        );
    }

    #[test]
    fn structured_event_orders_and_redacts_fields() {
        let line = format_structured_event(
            "server_request",
            vec![
                ("z", "last"),
                ("api_key", "sk-secret"),
                ("a field", "hello world"),
            ],
        );
        assert_eq!(
            line,
            "EVENT event=server_request a_field=\"hello world\" api_key=<redacted> z=last"
        );
    }

    #[test]
    fn structured_event_redacts_url_queries() {
        let line = format_structured_event(
            "callback",
            vec![("url", "https://example.test/cb?code=secret&state=abc")],
        );
        assert_eq!(
            line,
            "EVENT event=callback url=https://example.test/cb?<redacted>"
        );
    }

    #[test]
    fn structured_event_keeps_non_secret_code_fields() {
        let line = format_structured_event("tool_done", vec![("exit_code", "127")]);
        assert_eq!(line, "EVENT event=tool_done exit_code=127");
    }

    #[test]
    fn diagnostic_field_names_are_not_redacted() {
        // The model-picker / login diagnostics intentionally avoid field names
        // that collide with the secret-redaction heuristic (which masks any
        // field whose name contains key/token/secret/credential/...). If any of
        // these regress, the uploaded logs we ask users for would show
        // `<redacted>` instead of the boolean/count/env-var-name we need.
        for name in [
            "env_var",
            "input_len",
            "optional",
            "anthropic_api",
            "openai_api",
            "azure_api",
            "copilot_cred",
            "session_provider",
            "routes_in",
            "by_provider",
            "requested_model",
            "route_provider",
        ] {
            assert_eq!(
                redact_auth_field(name, "value"),
                "value",
                "diagnostic field `{name}` should not be redacted",
            );
        }
    }

    #[test]
    fn context_prefix_truncates_unicode_session_safely() {
        let ctx = LogContext {
            session: Some("é".repeat(21)),
            ..Default::default()
        };
        let prefix = context_prefix_for(&ctx);
        assert_eq!(prefix, format!("[ses:{}] ", "é".repeat(20)));
    }

    #[test]
    fn lazy_throttle_does_not_format_suppressed_messages() {
        let formatted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let first_counter = std::sync::Arc::clone(&formatted);
        assert!(info_throttled_with(
            "test_lazy_throttle_does_not_format",
            move || {
                first_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                "first".to_string()
            }
        ));
        let second_counter = std::sync::Arc::clone(&formatted);
        assert!(!info_throttled_with(
            "test_lazy_throttle_does_not_format",
            move || {
                second_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                "second".to_string()
            }
        ));
        assert_eq!(formatted.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn cleanup_removes_only_old_alphacode_logs() {
        use std::time::{Duration, SystemTime};

        let dir = std::env::temp_dir().join(format!(
            "alphacode-log-cleanup-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).expect("create temp log dir");

        let old_mtime = SystemTime::now() - Duration::from_secs(60 * 60 * 24 * 30); // 30 days

        let write = |name: &str, age_old: bool| {
            let path = dir.join(name);
            let mut f = File::create(&path).expect("create file");
            f.write_all(b"x").ok();
            if age_old {
                f.set_modified(old_mtime).expect("set mtime");
            }
            path
        };

        // Old log files that SHOULD be deleted.
        let old_log = write("alphacode-2000-01-01.log", true);
        let old_desktop = write("alphacode-desktop-2000-01-01.log", true);
        // Recent log file that SHOULD survive.
        let new_log = write("alphacode-2099-01-01.log", false);
        // Non-log data that SHOULD survive even though it is old.
        let old_memory = write("memory-events-2000-01-01.jsonl", true);
        let old_other = write("notes-2000-01-01.txt", true);
        // A subdirectory (e.g. `memory/`) must never be removed.
        let subdir = dir.join("memory");
        fs::create_dir_all(&subdir).expect("create subdir");

        cleanup_old_logs_in(&dir, Local::now());

        assert!(!old_log.exists(), "old alphacode log should be deleted");
        assert!(!old_desktop.exists(), "old desktop log should be deleted");
        assert!(new_log.exists(), "recent alphacode log must survive");
        assert!(old_memory.exists(), "memory-events jsonl must survive");
        assert!(old_other.exists(), "unrelated files must survive");
        assert!(subdir.is_dir(), "subdirectories must survive");

        fs::remove_dir_all(&dir).ok();
    }
}
