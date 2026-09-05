// The `swarm` tool's `json!` parameter schema is large; the default macro
// recursion limit (128) is exceeded once more properties are added.
#![allow(
    unknown_lints,
    clippy::collapsible_match,
    clippy::manual_checked_ops,
    clippy::unnecessary_sort_by,
    clippy::useless_conversion
)]

//! Application core for alphacode (upper layer).
//!
//! This crate holds the server/tool/agent layer and its presentation-adjacent
//! leaves. The foundational layer it builds on (provider, auth, config, session,
//! message, memory, telemetry, ...) lives in the `alphacode-base` crate and is
//! re-exported here via `pub use crate::alphacode_base::*`, so every existing
//! `crate::<module>` path (e.g. `crate::config`, `crate::provider`) keeps
//! resolving unchanged across this crate and the root `alphacode` crate, which in
//! turn re-exports this crate via `pub use crate::alphacode_app_core::*`.

// Foundational layer: re-export every `alphacode-base` module so `crate::<module>`
// paths resolve here exactly as they did before the split.
pub use crate::alphacode_base::*;

// Upper layer (server / tool / agent and supporting leaves).
pub mod agent;
pub mod ambient;
pub mod ambient_runner;
pub mod ambient_scheduler;
pub mod autonomous;
pub mod bounded;
pub mod bugbounty_doctor;
pub mod build;
pub mod catchup;
pub mod channel;
pub mod dedup;
pub mod external_auth;
pub mod failures;
pub mod health;
pub mod memory_manager;
pub mod metrics;
pub mod mission;
pub mod network_retry;
pub mod notifications;
pub mod overnight;
pub mod perf;
pub mod replay;
pub mod restart_snapshot;
pub mod server;
pub mod server_spawn;
pub mod session_effort;
pub mod session_launch;
pub mod session_rebuild;
pub mod session_watchdog;
pub mod setup_hints;
pub mod smart_stream;
pub mod ssh_remote;
pub mod startup_profile;
pub mod system_diagnostics;
pub mod plugin;
pub mod tool;
pub mod turn_cancel_registry;
pub mod update;

use std::sync::Mutex;

static CURRENT_SESSION_ID: Mutex<Option<String>> = Mutex::new(None);

pub fn set_current_session(session_id: &str) {
    if let Ok(mut guard) = CURRENT_SESSION_ID.lock() {
        *guard = Some(session_id.to_string());
    }
}

pub fn get_current_session() -> Option<String> {
    CURRENT_SESSION_ID.lock().ok()?.clone()
}
