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
use crate::alphacode_app_core::memory_manager::{MemoryManager, ProjectState, atomic_write};

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
    #[serde(default)]
    version: u32,
    #[serde(default)]
    state: Option<ProjectState>,
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
        let manager = Self {
            memory,
            checkpoints_dir,
        };
        if let Ok(Some(state)) = manager.memory.load_state_snapshot() {
            manager.reconcile_index(state)?;
        }
        Ok(manager)
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
        let state = self
            .memory
            .load_state()
            .context("loading state for checkpoint")?;
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
            version: 1,
            state: Some(state),
        };

        let path = self.checkpoint_path(&checkpoint.id);
        let json = serde_json::to_vec_pretty(&meta)?;
        atomic_write(&path, &json)
            .with_context(|| format!("writing checkpoint {}", path.display()))?;

        // The checkpoint file is the commit record. The index is rebuilt from
        // committed generations on startup if this secondary write fails.
        if let Err(error) = self.memory.update_state(|state| {
            if !state.checkpoints.contains(&checkpoint.id) {
                state.checkpoints.push(checkpoint.id.clone());
                state.statistics.checkpoints_created =
                    state.statistics.checkpoints_created.saturating_add(1);
            }
        }) {
            crate::alphacode_base::logging::warn(&format!(
                "Checkpoint {} committed, but its index update failed: {error:#}",
                checkpoint.id
            ));
        }

        Ok(checkpoint)
    }

    /// Restore a checkpoint's project state (utility — does not revert files).
    ///
    /// This updates the in-memory project state.  File restoration uses
    /// the stored git commit hash.
    pub fn restore(&self, checkpoint_id: &str) -> Result<Checkpoint> {
        let path = self.checked_checkpoint_path(checkpoint_id)?;
        let meta = Self::read_generation(&path)?;
        let mut state = match self.memory.load_state_snapshot() {
            Ok(Some(state)) => state,
            Ok(None) => meta.state.clone().unwrap_or_default(),
            Err(error) => meta.state.clone().with_context(|| {
                format!("Project state is unreadable and the checkpoint has no state snapshot: {error:#}")
            })?,
        };
        state.active_phase = Some(meta.checkpoint.phase.clone());
        state.updated_at = Utc::now();
        self.refresh_index(&mut state)?;
        self.memory.save_state(&state)?;

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
        let path = self.checked_checkpoint_path(checkpoint_id)?;
        match fs::remove_file(&path) {
            Ok(()) => {
                self.memory
                    .update_state(|state| state.checkpoints.retain(|id| id != checkpoint_id))?;
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    /// List valid, committed checkpoint generations.
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
            if let Ok(meta) = Self::read_generation(&path) {
                result.insert(meta.checkpoint.id.clone(), meta);
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

    fn read_generation(path: &std::path::Path) -> Result<CheckpointMeta> {
        let bytes =
            fs::read(path).with_context(|| format!("reading checkpoint {}", path.display()))?;
        let meta: CheckpointMeta = serde_json::from_slice(&bytes).context("parsing checkpoint")?;
        anyhow::ensure!(
            path.file_stem().and_then(|id| id.to_str()) == Some(&meta.checkpoint.id),
            "Checkpoint ID does not match its filename"
        );
        anyhow::ensure!(
            meta.version == 0 || (meta.version == 1 && meta.state.is_some()),
            "Unsupported or incomplete checkpoint generation"
        );
        Ok(meta)
    }

    fn reconcile_index(&self, mut state: ProjectState) -> Result<()> {
        if self.refresh_index(&mut state)? {
            state.updated_at = Utc::now();
            self.memory.save_state(&state)?;
        }
        Ok(())
    }

    fn refresh_index(&self, state: &mut ProjectState) -> Result<bool> {
        let mut generations: Vec<_> = self
            .list()?
            .into_values()
            .map(|meta| meta.checkpoint)
            .collect();
        generations.sort_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));
        let ids: Vec<_> = generations
            .into_iter()
            .map(|checkpoint| checkpoint.id)
            .collect();
        if ids == state.checkpoints {
            return Ok(false);
        }
        let previous: std::collections::HashSet<_> = state.checkpoints.iter().collect();
        let recovered = ids.iter().filter(|id| !previous.contains(id)).count();
        state.statistics.checkpoints_created = state
            .statistics
            .checkpoints_created
            .saturating_add(u32::try_from(recovered).unwrap_or(u32::MAX));
        state.checkpoints = ids;
        Ok(true)
    }

    fn checked_checkpoint_path(&self, id: &str) -> Result<PathBuf> {
        anyhow::ensure!(
            !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "Invalid checkpoint ID"
        );
        Ok(self.checkpoint_path(id))
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
    crate::alphacode_base::platform::helper_command("git")
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
    fn checkpoint_rejects_corrupt_state() {
        let (dir, mgr) = make_mgr();
        fs::write(dir.path().join("STATE.json"), "{invalid").unwrap();
        assert!(
            mgr.create("phase", "summary", CheckpointTrigger::TimeInterval)
                .is_err()
        );
        assert!(mgr.list().unwrap().is_empty());
    }

    #[test]
    fn recovers_committed_generation_after_index_write_is_interrupted() {
        let (dir, mgr) = make_mgr();
        let mut state = mgr.memory.load_state().unwrap();
        state.objective = "Preserve the original objective".into();
        mgr.memory.save_state(&state).unwrap();
        let checkpoint = mgr
            .create("phase", "summary", CheckpointTrigger::TimeInterval)
            .unwrap();

        // Simulate a crash after committing the generation but before replacing STATE.json.
        mgr.memory.save_state(&state).unwrap();
        // Simulate an unrelated damaged generation and an interrupted temporary write.
        fs::write(mgr.checkpoint_path("deadbeef"), b"{").unwrap();
        fs::write(mgr.checkpoints_dir.join(".tmp-checkpoint"), b"{").unwrap();
        let reopened = CheckpointManager::new(MemoryManager::new(dir.path()).unwrap()).unwrap();
        let indexed = reopened.memory.load_state().unwrap();
        assert_eq!(indexed.checkpoints, std::slice::from_ref(&checkpoint.id));
        assert_eq!(indexed.statistics.checkpoints_created, 1);
        assert_eq!(reopened.latest().unwrap().unwrap().id, checkpoint.id);

        fs::write(dir.path().join("STATE.json"), b"{").unwrap();
        reopened.restore(&checkpoint.id).unwrap();
        let recovered = reopened.memory.load_state().unwrap();
        assert_eq!(recovered.objective, state.objective);
        assert_eq!(recovered.checkpoints, [checkpoint.id]);
    }

    #[test]
    fn restores_objective_after_state_file_is_missing_on_restart() {
        let (dir, mgr) = make_mgr();
        mgr.memory
            .update_state(|state| state.objective = "Recover the exact objective".into())
            .unwrap();
        let checkpoint = mgr
            .create("phase", "summary", CheckpointTrigger::TimeInterval)
            .unwrap();
        fs::remove_file(dir.path().join("STATE.json")).unwrap();

        let reopened = CheckpointManager::new(MemoryManager::new(dir.path()).unwrap()).unwrap();
        reopened.restore(&checkpoint.id).unwrap();
        let state = reopened.memory.load_state().unwrap();
        assert_eq!(state.objective, "Recover the exact objective");
        assert_eq!(state.checkpoints, [checkpoint.id]);
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
