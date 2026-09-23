//! Checkpoint Manager — creates and restores project checkpoints.
//!
//! Checkpoints capture the full project state at a point in time so that
//! after a crash or error the system can roll back to a known good state
//! and continue exactly where it stopped.  Never restart the entire
//! project; restore the latest checkpoint and continue execution.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use super::Checkpoint;
use crate::alphacode_app_core::memory_manager::{MemoryManager, ProjectState};

/// Triggers for creating a checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointTrigger {
    PhaseCompleted,
    ImportantDiscovery,
    LargeFileModified,
    TimeInterval,
    BeforeRiskyOperation,
}

/// Checkpoint metadata stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMeta {
    pub checkpoint: Checkpoint,
    pub trigger: String,
}

/// CheckpointManager owns the checkpoints directory and provides create,
/// restore, rollback, and list operations.
pub struct CheckpointManager {
    /// Memory manager (provides root and state).
    memory: MemoryManager,
    /// Checkpoints directory.
    checkpoints_dir: PathBuf,
}

impl CheckpointManager {
    /// Create a new CheckpointManager from a MemoryManager.
    pub fn new(memory: MemoryManager) -> Result<Self> {
        let checkpoints_dir = memory.root().join("checkpoints");
        fs::create_dir_all(&checkpoints_dir)
            .with_context(|| format!("creating checkpoints dir {}", checkpoints_dir.display()))?;
        Ok(Self {
            memory,
            checkpoints_dir,
        })
    }

    /// Create a new checkpoint and persist it.
    ///
    /// Stores the current project state, open tasks, completed tasks,
    /// agent status, and the git commit hash (if available) so the system
    /// can roll back to this exact point.
    pub fn create(
        &self,
        phase: &str,
        summary: &str,
        trigger: CheckpointTrigger,
    ) -> Result<Checkpoint> {
        let state = self.memory.load_state().unwrap_or_default();
        let checkpoint = Checkpoint {
            id: super::new_id(),
            phase: phase.to_string(),
            summary: summary.to_string(),
            open_tasks: state.open_tasks(),
            completed_tasks: state.completed_tasks(),
            agent_status: state.active_agents.clone(),
            git_commit: read_git_commit(),
            progress_summary: Some(summary.to_string()),
            created_at: Utc::now(),
        };

        let meta = CheckpointMeta {
            checkpoint: checkpoint.clone(),
            trigger: trigger_label(trigger).to_string(),
        };

        let path = self.checkpoint_path(&checkpoint.id);
        let json = serde_json::to_string_pretty(&meta)?;
        fs::write(&path, json).with_context(|| format!("writing checkpoint {}", path.display()))?;

        // Update state's checkpoint list.
        self.memory.update_state(|s| {
            s.checkpoints.push(checkpoint.id.clone());
            s.statistics.checkpoints_created += 1;
        })?;

        Ok(checkpoint)
    }

    /// Restore a checkpoint's project state (utility — does not revert files).
    ///
    /// This updates the in-memory project state.  File restoration uses
    /// the stored git commit hash.
    pub fn restore(&self, checkpoint_id: &str) -> Result<Checkpoint> {
        let path = self.checkpoint_path(checkpoint_id);
        let data = fs::read_to_string(&path)
            .with_context(|| format!("reading checkpoint {}", path.display()))?;
        let meta: CheckpointMeta = serde_json::from_str(&data).context("parsing checkpoint")?;

        self.memory.update_state(|s| {
            s.active_phase = Some(meta.checkpoint.phase.clone());
        })?;

        Ok(meta.checkpoint)
    }

    /// Roll back the project to a previous checkpoint.
    ///
    /// In a full system this would:
    ///  1. Restore the git tree to the stored commit.
    ///  2. Restore the project state to the checkpoint's snapshot.
    ///  3. Remove all checkpoints after the target.
    ///
    /// Returns the rollback instruction string (git command to apply).
    pub fn rollback(&self, checkpoint_id: &str) -> Result<(Checkpoint, String)> {
        let checkpoint = self.restore(checkpoint_id)?;
        let rollback_cmd = checkpoint
            .git_commit
            .as_ref()
            .map(|c| format!("git checkout {c}"))
            .unwrap_or_default();

        // Remove all checkpoints after this one (by timestamp).
        let all = self.list()?;
        for (id, meta) in &all {
            if meta.checkpoint.created_at > checkpoint.created_at {
                let _ = self.delete(id);
            }
        }

        Ok((checkpoint, rollback_cmd))
    }

    /// Delete a checkpoint file.
    pub fn delete(&self, checkpoint_id: &str) -> Result<bool> {
        let path = self.checkpoint_path(checkpoint_id);
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(&path)?;
        Ok(true)
    }

    /// List all checkpoints, ordered oldest first.
    pub fn list(&self) -> Result<HashMap<String, CheckpointMeta>> {
        let mut result = HashMap::new();
        if !self.checkpoints_dir.exists() {
            return Ok(result);
        }
        for entry in fs::read_dir(&self.checkpoints_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            match fs::read_to_string(&path) {
                Ok(data) => {
                    if let Ok(meta) = serde_json::from_str::<CheckpointMeta>(&data) {
                        let id = meta.checkpoint.id.clone();
                        result.insert(id, meta);
                    }
                }
                Err(_) => continue,
            }
        }
        Ok(result)
    }

    /// Get the latest checkpoint.
    pub fn latest(&self) -> Result<Option<Checkpoint>> {
        let all = self.list()?;
        Ok(all
            .into_values()
            .map(|m| m.checkpoint)
            .max_by_key(|c| c.created_at))
    }

    /// Get the best checkpoint to restore after a crash.
    ///
    /// Looks for the most recent checkpoint that has a git commit hash
    /// (which allows actual file rollback).  Falls back to the latest
    /// checkpoint if none has a commit.
    pub fn best_for_recovery(&self) -> Result<Option<Checkpoint>> {
        let all = self.list()?;
        if all.is_empty() {
            return Ok(None);
        }
        let mut checkpoints: Vec<Checkpoint> = all.into_values().map(|m| m.checkpoint).collect();
        checkpoints.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Prefer one with a git commit for full rollback.
        let with_commit = checkpoints.iter().find(|c| c.git_commit.is_some()).cloned();
        Ok(with_commit.or_else(|| checkpoints.into_iter().next()))
    }

    fn checkpoint_path(&self, id: &str) -> PathBuf {
        self.checkpoints_dir.join(format!("{id}.json"))
    }
}

/// Extension trait to extract aggregate tasks from ProjectState.
trait ProjectStateExt {
    fn open_tasks(&self) -> Vec<String>;
    fn completed_tasks(&self) -> Vec<String>;
}

impl ProjectStateExt for ProjectState {
    /// Placeholder: in a full system, open tasks live in TODO.md / the task graph.
    fn open_tasks(&self) -> Vec<String> {
        Vec::new()
    }

    /// Placeholder: completed phases double as completed tasks.
    fn completed_tasks(&self) -> Vec<String> {
        self.completed_phases.clone()
    }
}

fn trigger_label(t: CheckpointTrigger) -> &'static str {
    match t {
        CheckpointTrigger::PhaseCompleted => "phase_completed",
        CheckpointTrigger::ImportantDiscovery => "important_discovery",
        CheckpointTrigger::LargeFileModified => "large_file_modified",
        CheckpointTrigger::TimeInterval => "time_interval",
        CheckpointTrigger::BeforeRiskyOperation => "before_risky_operation",
    }
}

fn read_git_commit() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_mgr() -> (TempDir, CheckpointManager) {
        let dir = TempDir::new().unwrap();
        let memory = MemoryManager::new(dir.path()).unwrap();
        let mgr = CheckpointManager::new(memory).unwrap();
        (dir, mgr)
    }

    #[test]
    fn test_create_and_restore() {
        let (_dir, mgr) = make_mgr();
        let cp = mgr
            .create(
                "Phase 1",
                "Architecture complete",
                CheckpointTrigger::PhaseCompleted,
            )
            .unwrap();
        assert_eq!(cp.phase, "Phase 1");

        let restored = mgr.restore(&cp.id).unwrap();
        assert_eq!(restored.id, cp.id);
        assert_eq!(restored.phase, "Phase 1");
    }

    #[test]
    fn test_list() {
        let (_dir, mgr) = make_mgr();
        mgr.create("Phase 1", "Done", CheckpointTrigger::PhaseCompleted)
            .unwrap();
        mgr.create("Phase 2", "Done", CheckpointTrigger::TimeInterval)
            .unwrap();
        let all = mgr.list().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_latest() {
        let (_dir, mgr) = make_mgr();
        mgr.create("Phase 1", "First", CheckpointTrigger::PhaseCompleted)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        mgr.create("Phase 2", "Second", CheckpointTrigger::TimeInterval)
            .unwrap();
        let latest = mgr.latest().unwrap().unwrap();
        assert_eq!(latest.phase, "Phase 2");
    }

    #[test]
    fn test_best_for_recovery() {
        let (_dir, mgr) = make_mgr();
        mgr.create("Phase 1", "No commit", CheckpointTrigger::PhaseCompleted)
            .unwrap();
        let best = mgr.best_for_recovery().unwrap();
        assert!(best.is_some());
        // Without a git repo, we still get the latest checkpoint.
        assert_eq!(best.unwrap().phase, "Phase 1");
    }

    #[test]
    fn test_rollback_deletes_future_checkpoints() {
        let (_dir, mgr) = make_mgr();
        let cp1 = mgr
            .create("Phase 1", "Old", CheckpointTrigger::PhaseCompleted)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let _cp2 = mgr
            .create("Phase 2", "New", CheckpointTrigger::PhaseCompleted)
            .unwrap();
        assert_eq!(mgr.list().unwrap().len(), 2);

        let _ = mgr.rollback(&cp1.id).unwrap();
        assert_eq!(mgr.list().unwrap().len(), 1);
    }

    #[test]
    fn test_delete() {
        let (_dir, mgr) = make_mgr();
        let cp = mgr
            .create("Phase 1", "Done", CheckpointTrigger::PhaseCompleted)
            .unwrap();
        assert!(mgr.delete(&cp.id).unwrap());
        assert!(!mgr.delete(&cp.id).unwrap());
    }
}
