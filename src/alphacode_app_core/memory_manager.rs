//! Persistent memory & state manager for the autonomous agent architecture.
//!
//! Owns the on-disk project memory store and the flat persistence files
//! described in the architecture plan: `STATE.json`, `GOALS.md`, `PLAN.md`,
//! `TODO.md`, `SUMMARY.md`, `DECISIONS.md`, `NOTES.md`, `BUGS.md`, `INDEX.json`.
//!
//! Nothing important may exist only inside model context.  This module
//! serialises and deserialises everything so that a model restart, application
//! restart, power outage, GPU crash, Windows reboot, network interruption, or
//! context overflow can all be recovered from without losing project state.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::alphacode_memory_types::MemoryEntry;

// ── persistence types ──────────────────────────────────────────────────────

/// Top-level project state persisted to `STATE.json`.
///
/// Contains everything the Main Brain needs to resume after a crash:
/// current objective, active phase, completed phases, active agents,
/// checkpoints, and project statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectState {
    pub version: u32,
    pub objective: String,
    pub active_phase: Option<String>,
    #[serde(default)]
    pub completed_phases: Vec<String>,
    #[serde(default)]
    pub active_agents: Vec<String>,
    #[serde(default)]
    pub checkpoints: Vec<String>,
    #[serde(default)]
    pub statistics: ProjectStatistics,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for ProjectState {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            version: 1,
            objective: String::new(),
            active_phase: None,
            completed_phases: Vec::new(),
            active_agents: Vec::new(),
            checkpoints: Vec::new(),
            statistics: ProjectStatistics::default(),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectStatistics {
    pub total_turns: u64,
    pub total_tokens_input: u64,
    pub total_tokens_output: u64,
    pub phases_completed: u32,
    pub agents_spawned: u32,
    pub checkpoints_created: u32,
    pub errors_recovered: u32,
}

/// Metadata index persisted to `INDEX.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectIndex {
    #[serde(default)]
    pub files: HashMap<String, FileMeta>,
    #[serde(default)]
    pub symbols: HashMap<String, SymbolMeta>,
    #[serde(default)]
    pub functions: Vec<String>,
    #[serde(default)]
    pub classes: Vec<String>,
    #[serde(default)]
    pub apis: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub call_graph: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub architecture_graph: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileMeta {
    pub path: String,
    pub lines: u64,
    pub bytes: u64,
    pub language: Option<String>,
    pub last_indexed: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolMeta {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line: u32,
}

// ── MemoryManager ────────────────────────────────────────────────────────

/// MemoryManager provides persistent memory management: state files,
/// memory compression, and retrieval. It owns the memory store and
/// ensures nothing important exists only in model context.
#[derive(Clone)]
pub struct MemoryManager {
    /// Root directory for memory files.
    root: PathBuf,
    /// State file path for project state.
    state_path: PathBuf,
    /// Goals file path for high-level objectives.
    goals_path: PathBuf,
    /// Plan file path for master execution plan.
    plan_path: PathBuf,
    /// TODO file path for pending tasks.
    todo_path: PathBuf,
    /// Summary file path for compressed history.
    summary_path: PathBuf,
    /// Decisions file path for architectural decisions.
    decisions_path: PathBuf,
    /// Notes file path for research notes.
    notes_path: PathBuf,
    /// Bugs file path for known issues.
    bugs_path: PathBuf,
    /// Index file path for project metadata.
    index_path: PathBuf,
    /// In-memory memory store (keyed by ID).
    #[allow(dead_code)]
    store: Arc<Mutex<HashMap<String, MemoryEntry>>>,
    /// Counter for generating unique IDs.
    next_id: Arc<Mutex<u64>>,
}

impl MemoryManager {
    /// Create a new MemoryManager with the given root directory.
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let root = root
            .canonicalize()
            .map_err(io::Error::other)?;

        // Define file paths relative to root.
        let state_path = root.join("STATE.json");
        let goals_path = root.join("GOALS.md");
        let plan_path = root.join("PLAN.md");
        let todo_path = root.join("TODO.md");
        let summary_path = root.join("SUMMARY.md");
        let decisions_path = root.join("DECISIONS.md");
        let notes_path = root.join("NOTES.md");
        let bugs_path = root.join("BUGS.md");
        let index_path = root.join("INDEX.json");
        let checkpoints_dir = root.join("checkpoints");
        // Ensure root exists.
        fs::create_dir_all(&root).map_err(io::Error::other)?;

        // Ensure files exist (create if missing).
        let empty_json = b"{}";
        for (p, init) in [
            (&state_path, empty_json.as_slice()),
            (&index_path, empty_json.as_slice()),
        ] {
            if !p.exists() {
                fs::write(p, init).map_err(io::Error::other)?;
            }
        }
        for p in [
            &goals_path,
            &plan_path,
            &todo_path,
            &summary_path,
            &decisions_path,
            &notes_path,
            &bugs_path,
        ] {
            if !p.exists() {
                fs::write(p, b"").map_err(io::Error::other)?;
            }
        }
        fs::create_dir_all(&checkpoints_dir).map_err(io::Error::other)?;

        let store = Arc::new(Mutex::new(HashMap::new()));
        let next_id = Arc::new(Mutex::new(1));

        Ok(Self {
            root,
            state_path,
            goals_path,
            plan_path,
            todo_path,
            summary_path,
            decisions_path,
            notes_path,
            bugs_path,
            index_path,
            store,
            next_id,
        })
    }

    /// Root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Generate a new unique ID for a memory entry.
    fn next_id(&self) -> String {
        let mut num = self
            .next_id
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let id = format!("mem-{:08x}", *num);
        *num += 1;
        id
    }

    /// Store a new memory entry.
    pub fn store(&self, entry: MemoryEntry) -> String {
        let id = self.next_id();
        let mut store = self
            .store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        store.insert(id.clone(), entry);
        id
    }

    /// Retrieve a memory entry by ID.
    pub fn get(&self, id: impl AsRef<str>) -> Option<MemoryEntry> {
        self.store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(id.as_ref())
            .cloned()
    }

    /// Delete a memory entry by ID.
    pub fn delete(&self, id: impl AsRef<str>) -> bool {
        self.store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(id.as_ref())
            .is_some()
    }

    /// List all memory entry IDs.
    pub fn list_ids(&self) -> Vec<String> {
        self.store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .keys()
            .cloned()
            .collect()
    }

    /// Clear all in-memory memory entries.
    pub fn clear(&self) {
        self.store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    // ── STATE.json ──────────────────────────────────────────────────────

    /// Load the project state from `STATE.json`.
    pub fn load_state(&self) -> Result<ProjectState> {
        if !self.state_path.exists() {
            return Ok(ProjectState::default());
        }
        let data = fs::read_to_string(&self.state_path)
            .with_context(|| format!("reading {}", self.state_path.display()))?;
        if data.trim().is_empty() || data.trim() == "{}" {
            return Ok(ProjectState::default());
        }
        serde_json::from_str(&data).with_context(|| "parsing STATE.json")
    }

    /// Save the project state to `STATE.json`.
    pub fn save_state(&self, state: &ProjectState) -> Result<()> {
        let json = serde_json::to_string_pretty(state)?;
        fs::write(&self.state_path, json)
            .with_context(|| format!("writing {}", self.state_path.display()))?;
        Ok(())
    }

    /// Update the project state with a helper closure.
    pub fn update_state<F>(&self, f: F) -> Result<ProjectState>
    where
        F: FnOnce(&mut ProjectState),
    {
        let mut state = self.load_state()?;
        f(&mut state);
        state.updated_at = Utc::now();
        self.save_state(&state)?;
        Ok(state)
    }

    // ── INDEX.json ──────────────────────────────────────────────────────

    /// Load the project index from `INDEX.json`.
    pub fn load_index(&self) -> Result<ProjectIndex> {
        if !self.index_path.exists() {
            return Ok(ProjectIndex::default());
        }
        let data = fs::read_to_string(&self.index_path)
            .with_context(|| format!("reading {}", self.index_path.display()))?;
        if data.trim().is_empty() || data.trim() == "{}" {
            return Ok(ProjectIndex::default());
        }
        serde_json::from_str(&data).with_context(|| "parsing INDEX.json")
    }

    /// Save the project index to `INDEX.json`.
    pub fn save_index(&self, index: &ProjectIndex) -> Result<()> {
        let json = serde_json::to_string_pretty(index)?;
        fs::write(&self.index_path, json)
            .with_context(|| format!("writing {}", self.index_path.display()))?;
        Ok(())
    }

    // ── Markdown files ───────────────────────────────────────────────────

    /// Read a markdown file, returning empty string if missing.
    fn read_md(&self, path: &Path) -> Result<String> {
        if !path.exists() {
            return Ok(String::new());
        }
        Ok(fs::read_to_string(path)?)
    }

    /// Write markdown content to a file.
    fn write_md(&self, path: &Path, content: &str) -> Result<()> {
        let mut file = fs::File::create(path)?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }

    /// Append a line (with trailing newline) to a markdown file.
    fn append_md(&self, path: &Path, line: &str) -> Result<()> {
        let mut file = fs::OpenOptions::new().append(true).create(true).open(path)?;
        file.write_all(format!("{line}\n").as_bytes())?;
        Ok(())
    }

    pub fn load_goals(&self) -> Result<String> {
        self.read_md(&self.goals_path)
    }
    pub fn save_goals(&self, content: &str) -> Result<()> {
        self.write_md(&self.goals_path, content)
    }

    pub fn load_plan(&self) -> Result<String> {
        self.read_md(&self.plan_path)
    }
    pub fn save_plan(&self, content: &str) -> Result<()> {
        self.write_md(&self.plan_path, content)
    }

    pub fn load_todo(&self) -> Result<String> {
        self.read_md(&self.todo_path)
    }
    pub fn save_todo(&self, content: &str) -> Result<()> {
        self.write_md(&self.todo_path, content)
    }

    pub fn load_summary(&self) -> Result<String> {
        self.read_md(&self.summary_path)
    }
    pub fn save_summary(&self, content: &str) -> Result<()> {
        self.write_md(&self.summary_path, content)
    }

    pub fn load_decisions(&self) -> Result<String> {
        self.read_md(&self.decisions_path)
    }
    pub fn append_decision(&self, decision: &str) -> Result<()> {
        self.append_md(&self.decisions_path, decision)
    }

    pub fn load_notes(&self) -> Result<String> {
        self.read_md(&self.notes_path)
    }
    pub fn append_note(&self, note: &str) -> Result<()> {
        self.append_md(&self.notes_path, note)
    }

    pub fn load_bugs(&self) -> Result<String> {
        self.read_md(&self.bugs_path)
    }
    pub fn append_bug(&self, bug: &str) -> Result<()> {
        self.append_md(&self.bugs_path, bug)
    }

    // ── Memory compression ──────────────────────────────────────────────

    /// Compress completed work into a structured summary and append to
    /// `SUMMARY.md`.  This discards unnecessary dialogue and keeps only
    /// facts, decisions, code references, dependencies, lessons, and
    /// important outputs.
    pub fn compress_work(
        &self,
        _phase: &str,
        summary: &str,
        facts: &[String],
        decisions: &[String],
        code_refs: &[String],
        lessons: &[String],
    ) -> Result<()> {
        let mut section = format!("## Work Summary — {}\n\n", Utc::now().to_rfc3339());

        if !summary.is_empty() {
            section.push_str(&format!("**Summary:** {summary}\n\n"));
        }
        if !facts.is_empty() {
            section.push_str("### Facts\n");
            for f in facts {
                section.push_str(&format!("- {f}\n"));
            }
            section.push('\n');
        }
        if !decisions.is_empty() {
            section.push_str("### Decisions\n");
            for d in decisions {
                section.push_str(&format!("- {d}\n"));
            }
            section.push('\n');
        }
        if !code_refs.is_empty() {
            section.push_str("### Code References\n");
            for c in code_refs {
                section.push_str(&format!("- {c}\n"));
            }
            section.push('\n');
        }
        if !lessons.is_empty() {
            section.push_str("### Lessons\n");
            for l in lessons {
                section.push_str(&format!("- {l}\n"));
            }
            section.push('\n');
        }

        self.append_md(&self.summary_path, &section)
    }
}

// Note: `Default` is intentionally not implemented for `MemoryManager`.
// `MemoryManager::new(root)` resolves the path with `canonicalize()` and
// creates the storage directory with `create_dir_all`, both of which can
// fail (read-only filesystem, missing parent, etc.). A blanket
// `Default::default() -> Self { Self::new(".").unwrap() }` would panic on
// any of those failure modes, so we leave construction explicit and let
// callers handle the `Result`.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_memory_types::MemoryCategory;
    use tempfile::TempDir;

    fn make_mgr() -> (TempDir, MemoryManager) {
        let dir = TempDir::new().unwrap();
        let mgr = MemoryManager::new(dir.path()).unwrap();
        (dir, mgr)
    }

    #[test]
    fn test_state_roundtrip() {
        let (_dir, mgr) = make_mgr();
        let mut state = mgr.load_state().unwrap();
        state.objective = "Build a browser".to_string();
        state.active_phase = Some("Architecture".to_string());
        mgr.save_state(&state).unwrap();

        let loaded = mgr.load_state().unwrap();
        assert_eq!(loaded.objective, "Build a browser");
        assert_eq!(loaded.active_phase.as_deref(), Some("Architecture"));
    }

    #[test]
    fn test_state_update_closure() {
        let (_dir, mgr) = make_mgr();
        mgr.update_state(|s| {
            s.objective = "Build an OS".to_string();
            s.active_phase = Some("Phase 1".to_string());
        })
        .unwrap();

        let loaded = mgr.load_state().unwrap();
        assert_eq!(loaded.objective, "Build an OS");
        assert_eq!(loaded.active_phase.as_deref(), Some("Phase 1"));
    }

    #[test]
    fn test_markdown_files() {
        let (_dir, mgr) = make_mgr();
        mgr.save_goals("# Goals\n- Build thing").unwrap();
        assert!(mgr.load_goals().unwrap().contains("Build thing"));

        mgr.save_plan("# Plan\n1. Step one").unwrap();
        assert!(mgr.load_plan().unwrap().contains("Step one"));

        mgr.append_decision("Use Rust for everything").unwrap();
        assert!(mgr.load_decisions().unwrap().contains("Use Rust"));

        mgr.append_note("Investigated approach X").unwrap();
        assert!(mgr.load_notes().unwrap().contains("Investigated"));

        mgr.append_bug("Memory leak in turn_loops").unwrap();
        assert!(mgr.load_bugs().unwrap().contains("Memory leak"));
    }

    #[test]
    fn test_index_roundtrip() {
        let (_dir, mgr) = make_mgr();
        let mut idx = mgr.load_index().unwrap();
        idx.files.insert(
            "src/main.rs".to_string(),
            FileMeta {
                path: "src/main.rs".to_string(),
                lines: 1000,
                bytes: 50000,
                language: Some("rust".to_string()),
                last_indexed: Some(Utc::now()),
            },
        );
        idx.functions.push("main".to_string());
        mgr.save_index(&idx).unwrap();

        let loaded = mgr.load_index().unwrap();
        assert!(loaded.files.contains_key("src/main.rs"));
        assert!(loaded.functions.contains(&"main".to_string()));
    }

    #[test]
    fn test_compress_work() {
        let (_dir, mgr) = make_mgr();
        mgr.compress_work(
            "Phase 1",
            "Completed initial architecture",
            &["Rust workspace detected".to_string()],
            &["Use single crate layout".to_string()],
            &["src/main.rs".to_string()],
            &["Incrementally rebuild index".to_string()],
        )
        .unwrap();

        let summary = mgr.load_summary().unwrap();
        assert!(summary.contains("Completed initial architecture"));
        assert!(summary.contains("Rust workspace detected"));
        assert!(summary.contains("Use single crate layout"));
        assert!(summary.contains("src/main.rs"));
        assert!(summary.contains("Incrementally rebuild index"));
    }

    #[test]
    fn test_store_and_retrieve() {
        let (_dir, mgr) = make_mgr();
        let entry =
            MemoryEntry::new(MemoryCategory::Fact, "Hello, memory!");
        let id = mgr.store(entry);
        assert!(mgr.get(&id).is_some());
    }

    #[test]
    fn test_delete() {
        let (_dir, mgr) = make_mgr();
        let id = mgr.store(MemoryEntry::new(MemoryCategory::Fact, "Delete me"));
        assert!(mgr.delete(&id));
        assert!(mgr.get(&id).is_none());
    }

    #[test]
    fn test_list_ids() {
        let (_dir, mgr) = make_mgr();
        mgr.store(MemoryEntry::new(MemoryCategory::Fact, "A"));
        mgr.store(MemoryEntry::new(MemoryCategory::Fact, "B"));
        let ids: Vec<_> = mgr.list_ids().into_iter().collect();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn test_clear() {
        let (_dir, mgr) = make_mgr();
        mgr.store(MemoryEntry::new(MemoryCategory::Fact, "C"));
        mgr.clear();
        assert!(mgr.list_ids().is_empty());
    }
}
