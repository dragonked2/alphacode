//! On-disk state shared by every alphacode process that checks for updates.
//!
//! Update checks are cheap but rate limited, and several alphacode processes can run
//! on one machine at once, so the cadence and backoff state live in a single
//! metadata file rather than per-process memory.

use super::UPDATE_CHECK_INTERVAL;
use crate::alphacode_app_core::storage;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMetadata {
    pub last_check: SystemTime,
    pub installed_version: Option<String>,
    pub installed_from: Option<String>,
    #[serde(default)]
    pub last_release_update_secs: Option<f64>,
    #[serde(default)]
    pub last_source_update_secs: Option<f64>,
    /// When set, automatic update checks are suppressed until this time
    /// because GitHub reported the API rate limit was exhausted. Shared via
    /// the metadata file so every alphacode process on the machine backs off, not
    /// just the one that saw the 403.
    #[serde(default)]
    pub rate_limited_until: Option<SystemTime>,
}

impl Default for UpdateMetadata {
    fn default() -> Self {
        Self {
            last_check: SystemTime::UNIX_EPOCH,
            installed_version: None,
            installed_from: None,
            last_release_update_secs: None,
            last_source_update_secs: None,
            rate_limited_until: None,
        }
    }
}

impl UpdateMetadata {
    pub fn load() -> Result<Self> {
        let path = metadata_path()?;
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = metadata_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    pub fn should_check(&self) -> bool {
        match self.last_check.elapsed() {
            Ok(elapsed) => elapsed > UPDATE_CHECK_INTERVAL,
            Err(_) => true,
        }
    }

    /// Rate-limit backoff is no longer enforced; always allow checks.
    pub fn rate_limited_backoff_remaining(&self) -> Option<Duration> {
        None
    }
}

pub(super) fn metadata_path() -> Result<PathBuf> {
    Ok(storage::alphacode_dir()?.join("update_metadata.json"))
}

pub(super) fn record_release_update_duration(duration: Duration) {
    if let Ok(mut metadata) = UpdateMetadata::load() {
        metadata.last_release_update_secs = Some(duration.as_secs_f64());
        let _ = metadata.save();
    }
}

pub(super) fn record_source_update_duration(duration: Duration) {
    if let Ok(mut metadata) = UpdateMetadata::load() {
        metadata.last_source_update_secs = Some(duration.as_secs_f64());
        let _ = metadata.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respects_check_interval() {
        let metadata = UpdateMetadata {
            last_check: SystemTime::now(),
            ..Default::default()
        };
        assert!(!metadata.should_check());
    }
}
