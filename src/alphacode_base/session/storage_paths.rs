use anyhow::Result;
use std::path::{Path, PathBuf};

use super::ProviderMessagesCacheMode;
use crate::storage;

pub(crate) fn session_path_in_dir(base: &std::path::Path, session_id: &str) -> PathBuf {
    base.join("sessions").join(format!("{}.json", session_id))
}

pub(super) use crate::process_memory::estimate_json_bytes;

pub(super) fn file_len_or_zero(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

/// Human-readable label for the provider-message cache mode.
pub(super) fn provider_messages_cache_mode_label(mode: ProviderMessagesCacheMode) -> &'static str {
    match mode {
        ProviderMessagesCacheMode::Clean => "clean",
        ProviderMessagesCacheMode::Append => "append",
        ProviderMessagesCacheMode::Full => "full",
        ProviderMessagesCacheMode::Bodies => "bodies",
    }
}

pub fn session_path(session_id: &str) -> Result<PathBuf> {
    let base = storage::alphacode_dir()?;
    Ok(session_path_in_dir(&base, session_id))
}

pub fn session_journal_path_from_snapshot(path: &Path) -> PathBuf {
    let mut name = path
        .file_stem()
        .map(|stem| stem.to_os_string())
        .unwrap_or_default();
    name.push(".journal.jsonl");
    path.with_file_name(name)
}

pub fn session_journal_path(session_id: &str) -> Result<PathBuf> {
    Ok(session_journal_path_from_snapshot(&session_path(
        session_id,
    )?))
}

pub fn session_exists(session_id: &str) -> bool {
    session_path(session_id)
        .map(|path| path.exists())
        .unwrap_or(false)
}
