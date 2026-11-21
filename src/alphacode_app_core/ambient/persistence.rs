use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashSet;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use super::paths::{lock_path, state_path};
use super::{
    AmbientCycleResult, AmbientState, AmbientStatus, CycleStatus, ScheduleTarget, ScheduledItem,
};
use crate::alphacode_app_core::storage;

// ---------------------------------------------------------------------------
// AmbientState persistence
// ---------------------------------------------------------------------------

impl AmbientState {
    pub fn load() -> Result<Self> {
        let path = state_path()?;
        if path.exists() {
            storage::read_json(&path)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        storage::write_json(&state_path()?, self)
    }

    pub fn record_cycle(&mut self, result: &AmbientCycleResult) {
        self.last_run = Some(result.ended_at);
        self.last_summary = Some(result.summary.clone());
        self.last_compactions = Some(result.compactions);
        self.last_memories_modified = Some(result.memories_modified);
        self.total_cycles += 1;

        match result.status {
            CycleStatus::Complete => {
                if let Some(ref req) = result.next_schedule {
                    let next = req.wake_at.unwrap_or_else(|| {
                        Utc::now()
                            + chrono::Duration::minutes(req.wake_in_minutes.unwrap_or(30) as i64)
                    });
                    self.status = AmbientStatus::Scheduled { next_wake: next };
                } else {
                    self.status = AmbientStatus::Idle;
                }
            }
            CycleStatus::Interrupted | CycleStatus::Incomplete => {
                self.status = AmbientStatus::Idle;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ScheduledQueue
// ---------------------------------------------------------------------------

pub struct ScheduledQueue {
    items: Vec<ScheduledItem>,
    path: PathBuf,
}

fn open_queue_lock(path: &Path) -> Result<File> {
    if let Some(parent) = path.parent() {
        storage::ensure_dir(parent)?;
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("Could not open schedule lock {}", path.display()))
}

impl ScheduledQueue {
    pub fn load(path: PathBuf) -> Result<Self> {
        let items = Self::read_items(&path)?;
        Ok(Self { items, path })
    }

    fn read_items(path: &Path) -> Result<Vec<ScheduledItem>> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("Invalid schedule queue {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error)
                .with_context(|| format!("Could not read schedule queue {}", path.display())),
        }
    }

    /// Reload after locking so a stale snapshot cannot overwrite another
    /// process's changes. Publish the updated state before returning a claim.
    fn update<T>(
        &mut self,
        change: impl FnOnce(&mut Vec<ScheduledItem>) -> (T, bool),
    ) -> Result<T> {
        let lock = open_queue_lock(&self.path.with_extension("lock"))?;
        lock.lock().context("Could not lock schedule queue")?;
        let mut items = Self::read_items(&self.path)?;
        let (result, changed) = change(&mut items);
        if changed {
            storage::write_json(&self.path, &items)?;
        }
        self.items = items;
        Ok(result)
    }

    pub fn push(&mut self, item: ScheduledItem) -> Result<()> {
        self.update(|items| {
            items.push(item);
            ((), true)
        })
    }

    pub fn remove_by_id(&mut self, id: &str) -> Result<Option<ScheduledItem>> {
        self.remove_matching(|item| item.id == id)
    }

    pub(crate) fn remove_session_schedule(&mut self, id: &str) -> Result<Option<ScheduledItem>> {
        self.remove_matching(|item| item.id == id && item.target != ScheduleTarget::Workspace)
    }

    fn remove_matching(
        &mut self,
        matches: impl Fn(&ScheduledItem) -> bool,
    ) -> Result<Option<ScheduledItem>> {
        self.update(|items| match items.iter().position(matches) {
            Some(index) => (Some(items.remove(index)), true),
            None => (None, false),
        })
    }

    pub fn set_paused(&mut self, id: &str, paused: bool) -> Result<Option<ScheduledItem>> {
        self.update(|items| {
            let Some(item) = items.iter_mut().find(|item| item.id == id) else {
                return (None, false);
            };
            let changed = item.paused != paused;
            item.paused = paused;
            (Some(item.clone()), changed)
        })
    }

    pub fn pop_ready(&mut self) -> Result<Vec<ScheduledItem>> {
        self.take_ready_matching(|_| true, usize::MAX)
    }

    pub fn take_ready_direct_items(&mut self) -> Result<Vec<ScheduledItem>> {
        self.take_ready_matching(|item| item.target.is_direct_delivery(), usize::MAX)
    }

    pub(crate) fn claim_next_direct_item(
        &mut self,
        include: impl Fn(&ScheduledItem) -> bool,
    ) -> Result<Option<(File, ScheduledItem)>> {
        let Some(lock) = self.try_lock_delivery()? else {
            return Ok(None);
        };
        let mut claimed = self.take_ready_matching(include, 1)?;
        Ok(claimed.pop().map(|item| (lock, item)))
    }

    /// Serialize direct delivery across daemons without blocking schedule edits.
    /// The OS releases this handle if a daemon exits during delivery.
    fn try_lock_delivery(&self) -> Result<Option<File>> {
        let file = open_queue_lock(&self.path.with_extension("delivery.lock"))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(file)),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => {
                Err(error).context("Could not lock schedule delivery")
            }
        }
    }

    fn take_ready_matching(
        &mut self,
        include: impl Fn(&ScheduledItem) -> bool,
        limit: usize,
    ) -> Result<Vec<ScheduledItem>> {
        self.update(|items| {
            let now = Utc::now();
            let mut due: Vec<_> = items
                .iter()
                .enumerate()
                .filter(|(_, item)| !item.paused && item.scheduled_for <= now && include(item))
                .collect();
            due.sort_by(|(_, a), (_, b)| {
                b.priority
                    .cmp(&a.priority)
                    .then_with(|| a.scheduled_for.cmp(&b.scheduled_for))
            });
            due.truncate(limit);
            let claimed_indices: HashSet<_> = due.iter().map(|(index, _)| *index).collect();
            let claimed: Vec<_> = due.into_iter().map(|(_, item)| item.clone()).collect();

            let mut index = 0;
            items.retain_mut(|item| {
                let claimed = claimed_indices.contains(&index);
                index += 1;
                if !claimed {
                    return true;
                }
                let Some(interval) = item.repeat_every_minutes else {
                    return false;
                };
                let interval_seconds = i64::from(interval.get()) * 60;
                let elapsed_seconds = (now - item.scheduled_for).num_seconds();
                // Keep the original cadence and coalesce missed occurrences.
                let delta = chrono::TimeDelta::seconds(
                    (elapsed_seconds / interval_seconds + 1) * interval_seconds,
                );
                if let Some(next) = item.scheduled_for.checked_add_signed(delta) {
                    item.scheduled_for = next;
                } else {
                    item.paused = true;
                }
                true
            });

            let changed = !claimed.is_empty();
            (claimed, changed)
        })
    }

    pub fn peek_next(&self) -> Option<&ScheduledItem> {
        self.items
            .iter()
            .filter(|item| !item.paused)
            .min_by_key(|item| item.scheduled_for)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn items(&self) -> &[ScheduledItem] {
        &self.items
    }
}

// ---------------------------------------------------------------------------
// AmbientLock  (single-instance guard)
// ---------------------------------------------------------------------------

pub struct AmbientLock {
    pub(crate) lock_path: PathBuf,
}

impl AmbientLock {
    /// Try to acquire the ambient lock.
    /// Returns `Ok(Some(lock))` if acquired, `Ok(None)` if another instance
    /// already holds it, or `Err` on I/O failure.
    pub fn try_acquire() -> Result<Option<Self>> {
        let path = lock_path()?;

        // Check existing lock
        if path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&path)
                && let Ok(pid) = contents.trim().parse::<u32>()
                && is_pid_alive(pid)
            {
                return Ok(None); // Another instance is running
            }
            let _ = std::fs::remove_file(&path);
        }

        // Write our PID
        let pid = std::process::id();
        if let Some(parent) = path.parent() {
            storage::ensure_dir(parent)?;
        }
        std::fs::write(&path, pid.to_string())?;

        Ok(Some(Self { lock_path: path }))
    }

    pub fn release(self) -> Result<()> {
        let _ = std::fs::remove_file(&self.lock_path);
        // Drop runs, but we already cleaned up
        std::mem::forget(self);
        Ok(())
    }
}

impl Drop for AmbientLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

fn is_pid_alive(pid: u32) -> bool {
    crate::platform::is_process_running(pid)
}
