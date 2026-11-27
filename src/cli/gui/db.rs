//! Local SQLite persistence for the AlphaCode desktop/web GUI (`~/.alphacode/alphacode.db`).
//!
//! Stores workspaces, chat metadata (synced with `~/.alphacode/sessions/<id>.json`),
//! and GUI preferences/app state in WAL mode with a safe busy timeout.

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::{
    GuiPersistentState, is_program_install_dir, is_user_home_dir, normalize_existing_dir,
    user_home_dir_or_install,
};

const BUSY_TIMEOUT_MS: u64 = 5000;
const MAX_CHAT_PREVIEW_CHARS: usize = 90;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub(super) struct WorkspaceRow {
    pub path: String,
    pub name: String,
    pub pinned: bool,
    pub collapsed: bool,
    pub added_at: i64,
    pub last_opened_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ChatRow {
    pub id: String,
    pub workspace_path: Option<String>,
    pub title: Option<String>,
    pub custom_title: Option<String>,
    pub preview: String,
    pub pinned: bool,
    pub saved: bool,
    pub save_label: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) struct SyncStats {
    pub synced: usize,
    pub removed: usize,
    pub skipped: usize,
}

pub(super) fn db_path() -> Result<PathBuf> {
    Ok(crate::storage::alphacode_dir()?.join("alphacode.db"))
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn workspace_display_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(path)
        .to_string()
}

pub(super) fn normalize_workspace_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(existing) = normalize_existing_dir(trimmed) {
        return Some(trim_trailing_workspace_sep(&existing));
    }
    #[cfg(windows)]
    let normalized_input = trimmed
        .strip_prefix(r"\\?\")
        .unwrap_or(trimmed)
        .replace('/', "\\");
    #[cfg(not(windows))]
    let normalized_input = trimmed.to_string();

    let lex = super::lexically_normalize_path(Path::new(&normalized_input));
    let out = lex.to_string_lossy().into_owned();
    #[cfg(windows)]
    let out = out.strip_prefix(r"\\?\").unwrap_or(&out).to_string();
    let out = trim_trailing_workspace_sep(&out);
    if out.is_empty() { None } else { Some(out) }
}

fn trim_trailing_workspace_sep(path: &str) -> String {
    let mut s = path.trim().to_string();
    while s.ends_with('/') || s.ends_with('\\') {
        #[cfg(windows)]
        {
            if s.len() <= 3 && s.as_bytes().get(1) == Some(&b':') {
                break;
            }
        }
        if s.len() <= 1 {
            break;
        }
        s.pop();
    }
    s
}

fn workspace_path_key(raw: &str) -> Option<String> {
    let norm = normalize_workspace_path(raw)?;
    #[cfg(windows)]
    {
        Some(norm.to_lowercase())
    }
    #[cfg(not(windows))]
    {
        Some(norm)
    }
}

pub(super) fn workspace_paths_equal(a: &str, b: &str) -> bool {
    matches!((workspace_path_key(a), workspace_path_key(b)), (Some(ka), Some(kb)) if ka == kb)
}

fn extract_session_preview(session: &crate::session::Session) -> String {
    for msg in session.messages.iter().rev() {
        if msg.display_role.is_some() {
            continue;
        }
        for block in &msg.content {
            if let crate::message::ContentBlock::Text { text, .. } = block {
                let trimmed = text.trim();
                if trimmed.is_empty()
                    || trimmed.starts_with("<system-reminder>")
                    || trimmed.starts_with("<session-context>")
                {
                    continue;
                }
                let collapsed = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
                return crate::util::truncate_str(&collapsed, MAX_CHAT_PREVIEW_CHARS).to_string();
            }
        }
    }
    String::new()
}

fn chat_row_from_session(session: &crate::session::Session) -> ChatRow {
    ChatRow {
        id: session.id.clone(),
        workspace_path: session
            .working_dir
            .as_deref()
            .and_then(normalize_workspace_path),
        title: session.title.clone(),
        custom_title: session.custom_title.clone(),
        preview: extract_session_preview(session),
        pinned: false,
        saved: session.saved,
        save_label: session.save_label.clone(),
        created_at: session.created_at.timestamp(),
        updated_at: session.updated_at.timestamp(),
        message_count: session.visible_conversation_message_count() as i64,
    }
}

pub(super) struct GuiDb {
    conn: Connection,
}

impl GuiDb {
    pub fn open_default() -> Result<Self> {
        let path = db_path()?;
        Self::open(&path)
    }

    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create directory for SQLite db: {}",
                    parent.display()
                )
            })?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open SQLite db at {}", path.display()))?;
        conn.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;

        let db = Self { conn };
        db.ensure_schema()?;
        db.migrate_legacy_gui_state_once(path.parent())?;
        if db.get_app_state("theme")?.is_none() {
            db.set_app_state("theme", "dark")?;
        }
        Ok(db)
    }

    fn ensure_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS workspaces (
                path TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                pinned INTEGER NOT NULL DEFAULT 0,
                collapsed INTEGER NOT NULL DEFAULT 0,
                added_at INTEGER NOT NULL,
                last_opened_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS chats (
                id TEXT PRIMARY KEY,
                workspace_path TEXT,
                title TEXT,
                custom_title TEXT,
                preview TEXT NOT NULL DEFAULT '',
                pinned INTEGER NOT NULL DEFAULT 0,
                saved INTEGER NOT NULL DEFAULT 0,
                save_label TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                message_count INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_chats_updated_at ON chats(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_chats_workspace_path ON chats(workspace_path);
            CREATE TABLE IF NOT EXISTS app_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;
        Ok(())
    }

    fn migrate_legacy_gui_state_once(&self, base_dir: Option<&Path>) -> Result<()> {
        if self.get_app_state("migrated_gui_state_json")?.as_deref() == Some("1") {
            return Ok(());
        }

        if let Some(dir) = base_dir {
            let legacy_path = dir.join("gui_state.json");
            if legacy_path.is_file()
                && let Ok(raw) = std::fs::read_to_string(&legacy_path)
                && let Ok(legacy) = serde_json::from_str::<GuiPersistentState>(&raw)
            {
                let pinned_set = &legacy.pinned_workspaces;
                let collapsed_set = &legacy.collapsed_workspaces;
                for ws_raw in &legacy.workspaces {
                    let Some(clean) = normalize_existing_dir(ws_raw) else {
                        continue;
                    };
                    if user_home_dir_or_install(Path::new(&clean)) {
                        continue;
                    }
                    let pinned = pinned_set.iter().any(|p| workspace_paths_equal(p, &clean));
                    let collapsed = collapsed_set
                        .iter()
                        .any(|c| workspace_paths_equal(c, &clean));
                    let now = now_unix_secs();
                    self.conn.execute(
                        "INSERT INTO workspaces (path, name, pinned, collapsed, added_at, last_opened_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                         ON CONFLICT(path) DO UPDATE SET
                             pinned = MAX(workspaces.pinned, excluded.pinned),
                             collapsed = MAX(workspaces.collapsed, excluded.collapsed)",
                        params![
                            clean,
                            workspace_display_name(&clean),
                            i64::from(pinned),
                            i64::from(collapsed),
                            now
                        ],
                    )?;
                }

                for chat_id in &legacy.pinned_chats {
                    let id = chat_id.trim();
                    if id.is_empty() {
                        continue;
                    }
                    let now = now_unix_secs();
                    self.conn.execute(
                        "INSERT INTO chats (id, pinned, created_at, updated_at)
                         VALUES (?1, 1, ?2, ?2)
                         ON CONFLICT(id) DO UPDATE SET pinned = 1",
                        params![id, now],
                    )?;
                }

                if let Some(last) = legacy
                    .last_working_dir
                    .as_deref()
                    .and_then(normalize_existing_dir)
                    && !user_home_dir_or_install(Path::new(&last))
                {
                    self.set_app_state("last_working_dir", &last)?;
                    if legacy.active_workspace.is_none() {
                        self.set_app_state("active_workspace", &last)?;
                    }
                }
                if let Some(active) = legacy
                    .active_workspace
                    .as_deref()
                    .and_then(normalize_existing_dir)
                    && !user_home_dir_or_install(Path::new(&active))
                {
                    self.set_app_state("active_workspace", &active)?;
                }
                if let Some(theme) = legacy
                    .theme
                    .as_deref()
                    .filter(|t| matches!(*t, "dark" | "light"))
                {
                    self.set_app_state("theme", theme)?;
                }
                if let Some(lang) = legacy.lang.as_deref().filter(|l| matches!(*l, "en" | "ar")) {
                    self.set_app_state("lang", lang)?;
                }
                if let Some(font_size) = legacy.font_size.filter(|f| (10..=24).contains(f)) {
                    self.set_app_state("font_size", &font_size.to_string())?;
                }
                if let Some(sidebar_open) = legacy.sidebar_open {
                    self.set_app_state(
                        "sidebar_open",
                        if sidebar_open { "true" } else { "false" },
                    )?;
                }
                if let Some(last_sid) = legacy
                    .last_session_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    self.set_app_state("last_session_id", last_sid)?;
                }
            }
        }

        self.set_app_state("migrated_gui_state_json", "1")?;
        Ok(())
    }

    // ── App state key/value ───────────────────────────────────────────────────

    pub fn get_app_state(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM app_state WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn set_app_state(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO app_state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn delete_app_state(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM app_state WHERE key = ?1", params![key])?;
        Ok(())
    }

    // ── Workspaces CRUD ───────────────────────────────────────────────────────

    fn find_stored_workspace_key(&self, path: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT path FROM workspaces")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            let stored = row?;
            if workspace_paths_equal(&stored, path) {
                return Ok(Some(stored));
            }
        }
        Ok(None)
    }

    pub fn list_workspaces(&self) -> Result<Vec<WorkspaceRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, name, pinned, collapsed, added_at, last_opened_at
             FROM workspaces
             ORDER BY pinned DESC, last_opened_at DESC, added_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(WorkspaceRow {
                path: row.get(0)?,
                name: row.get(1)?,
                pinned: row.get::<_, i64>(2)? != 0,
                collapsed: row.get::<_, i64>(3)? != 0,
                added_at: row.get(4)?,
                last_opened_at: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn upsert_workspace(
        &self,
        path: &str,
        name: Option<&str>,
        touch_opened: bool,
    ) -> Result<()> {
        let key = self
            .find_stored_workspace_key(path)?
            .unwrap_or_else(|| path.trim().to_string());
        let display_name = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| workspace_display_name(&key));
        let now = now_unix_secs();
        if touch_opened {
            self.conn.execute(
                "INSERT INTO workspaces (path, name, pinned, collapsed, added_at, last_opened_at)
                 VALUES (?1, ?2, 0, 0, ?3, ?3)
                 ON CONFLICT(path) DO UPDATE SET
                     name = excluded.name,
                     last_opened_at = excluded.last_opened_at",
                params![key, display_name, now],
            )?;
        } else {
            self.conn.execute(
                "INSERT INTO workspaces (path, name, pinned, collapsed, added_at, last_opened_at)
                 VALUES (?1, ?2, 0, 0, ?3, ?3)
                 ON CONFLICT(path) DO UPDATE SET
                     name = excluded.name",
                params![key, display_name, now],
            )?;
        }
        Ok(())
    }

    pub fn set_workspace_pinned(&self, path: &str, pinned: Option<bool>) -> Result<bool> {
        let key = match self.find_stored_workspace_key(path)? {
            Some(existing) => existing,
            None => {
                self.upsert_workspace(path, None, false)?;
                path.trim().to_string()
            }
        };
        let current: i64 = self.conn.query_row(
            "SELECT pinned FROM workspaces WHERE path = ?1",
            params![&key],
            |row| row.get(0),
        )?;
        let next = pinned.unwrap_or(current == 0);
        self.conn.execute(
            "UPDATE workspaces SET pinned = ?1 WHERE path = ?2",
            params![i64::from(next), &key],
        )?;
        Ok(next)
    }

    pub fn set_workspace_collapsed(&self, path: &str, collapsed: Option<bool>) -> Result<bool> {
        let key = match self.find_stored_workspace_key(path)? {
            Some(existing) => existing,
            None => {
                self.upsert_workspace(path, None, false)?;
                path.trim().to_string()
            }
        };
        let current: i64 = self.conn.query_row(
            "SELECT collapsed FROM workspaces WHERE path = ?1",
            params![&key],
            |row| row.get(0),
        )?;
        let next = collapsed.unwrap_or(current == 0);
        self.conn.execute(
            "UPDATE workspaces SET collapsed = ?1 WHERE path = ?2",
            params![i64::from(next), &key],
        )?;
        Ok(next)
    }

    pub fn remove_workspace(&self, path: &str) -> Result<bool> {
        let Some(key) = self.find_stored_workspace_key(path)? else {
            return Ok(false);
        };
        let deleted = self
            .conn
            .execute("DELETE FROM workspaces WHERE path = ?1", params![&key])?;
        if let Some(active) = self.get_app_state("active_workspace")?
            && workspace_paths_equal(&active, &key)
        {
            self.delete_app_state("active_workspace")?;
        }
        if let Some(last) = self.get_app_state("last_working_dir")?
            && workspace_paths_equal(&last, &key)
        {
            self.delete_app_state("last_working_dir")?;
        }
        Ok(deleted > 0)
    }

    // ── Chats CRUD & sync ─────────────────────────────────────────────────────

    #[cfg(test)]
    pub fn upsert_chat(&self, chat: &ChatRow) -> Result<()> {
        let normalized_ws = chat
            .workspace_path
            .as_deref()
            .and_then(normalize_workspace_path);
        self.conn.execute(
            "INSERT INTO chats (
                id, workspace_path, title, custom_title, preview,
                pinned, saved, save_label, created_at, updated_at, message_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(id) DO UPDATE SET
                workspace_path = COALESCE(excluded.workspace_path, chats.workspace_path),
                title = COALESCE(excluded.title, chats.title),
                custom_title = COALESCE(excluded.custom_title, chats.custom_title),
                preview = CASE WHEN excluded.preview != '' THEN excluded.preview ELSE chats.preview END,
                pinned = excluded.pinned,
                saved = excluded.saved,
                save_label = excluded.save_label,
                created_at = MIN(chats.created_at, excluded.created_at),
                updated_at = MAX(chats.updated_at, excluded.updated_at),
                message_count = MAX(chats.message_count, excluded.message_count)",
            params![
                chat.id,
                normalized_ws,
                chat.title,
                chat.custom_title,
                chat.preview,
                i64::from(chat.pinned),
                i64::from(chat.saved),
                chat.save_label,
                chat.created_at,
                chat.updated_at,
                chat.message_count,
            ],
        )?;
        Ok(())
    }

    pub fn upsert_chat_from_session(&self, session: &crate::session::Session) -> Result<()> {
        let row = chat_row_from_session(session);
        self.conn.execute(
            "INSERT INTO chats (
                id, workspace_path, title, custom_title, preview,
                pinned, saved, save_label, created_at, updated_at, message_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(id) DO UPDATE SET
                workspace_path = excluded.workspace_path,
                title = excluded.title,
                custom_title = excluded.custom_title,
                preview = excluded.preview,
                saved = excluded.saved,
                save_label = excluded.save_label,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                message_count = excluded.message_count",
            params![
                row.id,
                row.workspace_path,
                row.title,
                row.custom_title,
                row.preview,
                i64::from(row.saved),
                row.save_label,
                row.created_at,
                row.updated_at,
                row.message_count,
            ],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn get_chat(&self, id: &str) -> Result<Option<ChatRow>> {
        self.conn
            .query_row(
                "SELECT id, workspace_path, title, custom_title, preview, pinned,
                        saved, save_label, created_at, updated_at, message_count
                 FROM chats WHERE id = ?1",
                params![id],
                |row| {
                    Ok(ChatRow {
                        id: row.get(0)?,
                        workspace_path: row.get(1)?,
                        title: row.get(2)?,
                        custom_title: row.get(3)?,
                        preview: row.get(4)?,
                        pinned: row.get::<_, i64>(5)? != 0,
                        saved: row.get::<_, i64>(6)? != 0,
                        save_label: row.get(7)?,
                        created_at: row.get(8)?,
                        updated_at: row.get(9)?,
                        message_count: row.get(10)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    #[cfg(test)]
    pub fn list_chats(&self, workspace_path: Option<&str>, limit: usize) -> Result<Vec<ChatRow>> {
        let mut out = Vec::new();
        let cap = if limit == 0 { 1000 } else { limit };
        let mut stmt = self.conn.prepare(
            "SELECT id, workspace_path, title, custom_title, preview, pinned,
                    saved, save_label, created_at, updated_at, message_count
             FROM chats
             ORDER BY pinned DESC, updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ChatRow {
                id: row.get(0)?,
                workspace_path: row.get(1)?,
                title: row.get(2)?,
                custom_title: row.get(3)?,
                preview: row.get(4)?,
                pinned: row.get::<_, i64>(5)? != 0,
                saved: row.get::<_, i64>(6)? != 0,
                save_label: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                message_count: row.get(10)?,
            })
        })?;
        for r in rows {
            let row = r?;
            if let Some(ws) = workspace_path {
                let matches = row
                    .workspace_path
                    .as_deref()
                    .is_some_and(|p| workspace_paths_equal(p, ws));
                if !matches {
                    continue;
                }
            }
            out.push(row);
            if out.len() >= cap {
                break;
            }
        }
        Ok(out)
    }

    pub fn set_chat_title(
        &self,
        id: &str,
        title: Option<&str>,
        custom_title: Option<&str>,
    ) -> Result<()> {
        let id = id.trim();
        if id.is_empty() {
            return Ok(());
        }
        let now = now_unix_secs();
        self.conn.execute(
            "INSERT INTO chats (id, title, custom_title, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(id) DO UPDATE SET
                 title = COALESCE(excluded.title, chats.title),
                 custom_title = excluded.custom_title,
                 updated_at = MAX(chats.updated_at, excluded.updated_at)",
            params![id, title, custom_title, now],
        )?;
        Ok(())
    }

    pub fn set_chat_saved(&self, id: &str, saved: bool, save_label: Option<&str>) -> Result<()> {
        let id = id.trim();
        if id.is_empty() {
            return Ok(());
        }
        let now = now_unix_secs();
        let label = if saved { save_label } else { None };
        self.conn.execute(
            "INSERT INTO chats (id, saved, save_label, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(id) DO UPDATE SET
                 saved = excluded.saved,
                 save_label = excluded.save_label,
                 updated_at = MAX(chats.updated_at, excluded.updated_at)",
            params![id, i64::from(saved), label, now],
        )?;
        Ok(())
    }

    pub fn delete_chat(&self, id: &str) -> Result<bool> {
        let id = id.trim();
        let deleted = self
            .conn
            .execute("DELETE FROM chats WHERE id = ?1", params![id])?;
        if self.get_app_state("last_session_id")?.as_deref() == Some(id) {
            self.delete_app_state("last_session_id")?;
        }
        Ok(deleted > 0)
    }

    /// Synchronize the `chats` table with `~/.alphacode/sessions/<id>.json`.
    pub fn sync_sessions_index(&self, sessions_dir: &Path) -> Result<SyncStats> {
        let mut stats = SyncStats::default();
        if !sessions_dir.is_dir() {
            return Ok(stats);
        }

        let mut seen_ids = HashSet::new();
        let mut parsed_rows = Vec::new();

        for entry in std::fs::read_dir(sessions_dir)? {
            let Ok(entry) = entry else {
                continue;
            };
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if stem.is_empty()
                || !stem
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
            {
                continue;
            }
            seen_ids.insert(stem.to_string());
            match crate::session::Session::load_from_path(&path) {
                Ok(session) => {
                    seen_ids.insert(session.id.clone());
                    parsed_rows.push(chat_row_from_session(&session));
                }
                Err(_) => {
                    stats.skipped += 1;
                }
            }
        }

        let tx = self.conn.unchecked_transaction()?;
        for row in parsed_rows {
            tx.execute(
                "INSERT INTO chats (
                    id, workspace_path, title, custom_title, preview,
                    pinned, saved, save_label, created_at, updated_at, message_count
                ) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, ?9, ?10)
                ON CONFLICT(id) DO UPDATE SET
                    workspace_path = excluded.workspace_path,
                    title = excluded.title,
                    custom_title = excluded.custom_title,
                    preview = excluded.preview,
                    saved = excluded.saved,
                    save_label = excluded.save_label,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at,
                    message_count = excluded.message_count",
                params![
                    row.id,
                    row.workspace_path,
                    row.title,
                    row.custom_title,
                    row.preview,
                    i64::from(row.saved),
                    row.save_label,
                    row.created_at,
                    row.updated_at,
                    row.message_count,
                ],
            )?;
            stats.synced += 1;
        }

        let existing_ids: Vec<String> = {
            let mut stmt = tx.prepare("SELECT id FROM chats")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        for id in existing_ids {
            if !seen_ids.contains(&id) {
                tx.execute("DELETE FROM chats WHERE id = ?1", params![id])?;
                stats.removed += 1;
            }
        }
        tx.commit()?;
        Ok(stats)
    }

    pub fn sync_single_session(&self, session_id: &str) -> Result<bool> {
        let path = crate::session::session_path(session_id)?;
        if !path.is_file() {
            return Ok(false);
        }
        let session = crate::session::Session::load_from_path(&path)?;
        self.upsert_chat_from_session(&session)?;
        Ok(true)
    }

    // ── Bridge GuiPersistentState projection ──────────────────────────────────

    pub fn load_gui_state(&self) -> Result<GuiPersistentState> {
        let workspaces_rows = self.list_workspaces()?;
        let mut workspaces = Vec::new();
        let mut pinned_workspaces = Vec::new();
        let mut collapsed_workspaces = Vec::new();

        for row in workspaces_rows {
            if !Path::new(&row.path).is_dir() || is_program_install_dir(Path::new(&row.path)) {
                continue;
            }
            if row.pinned {
                pinned_workspaces.push(row.path.clone());
            }
            if row.collapsed {
                collapsed_workspaces.push(row.path.clone());
            }
            workspaces.push(row.path);
        }

        let pinned_chats = {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM chats WHERE pinned = 1 ORDER BY updated_at DESC")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };

        let last_working_dir = self
            .get_app_state("last_working_dir")?
            .and_then(|s| normalize_existing_dir(&s))
            .filter(|p| !is_program_install_dir(Path::new(p)));
        let active_workspace = self
            .get_app_state("active_workspace")?
            .and_then(|s| normalize_existing_dir(&s))
            .filter(|p| !is_program_install_dir(Path::new(p)));
        let last_session_id = self
            .get_app_state("last_session_id")?
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let theme = self
            .get_app_state("theme")?
            .filter(|t| matches!(t.as_str(), "dark" | "light"))
            .or_else(|| Some("dark".to_string()));
        let lang = self
            .get_app_state("lang")?
            .filter(|l| matches!(l.as_str(), "en" | "ar"));
        let font_size = self
            .get_app_state("font_size")?
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|f| (10..=24).contains(f));
        let sidebar_open = self
            .get_app_state("sidebar_open")?
            .and_then(|v| match v.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            });

        Ok(GuiPersistentState {
            last_working_dir,
            active_workspace,
            workspaces,
            pinned_workspaces,
            collapsed_workspaces,
            pinned_chats,
            last_session_id,
            theme,
            lang,
            font_size,
            sidebar_open,
        })
    }

    pub fn save_gui_state(&self, state: &GuiPersistentState) -> Result<()> {
        let existing_ws = self.list_workspaces()?;
        let now = now_unix_secs();

        let tx = self.conn.unchecked_transaction()?;
        // Remove workspaces no longer in state.workspaces
        for row in &existing_ws {
            if !state
                .workspaces
                .iter()
                .any(|w| workspace_paths_equal(w, &row.path))
            {
                tx.execute("DELETE FROM workspaces WHERE path = ?1", params![&row.path])?;
            }
        }
        // Upsert workspaces in state.workspaces
        for (idx, ws) in state.workspaces.iter().enumerate() {
            let trimmed = ws.trim();
            if trimmed.is_empty() {
                continue;
            }
            let pinned = state
                .pinned_workspaces
                .iter()
                .any(|p| workspace_paths_equal(p, trimmed));
            let collapsed = state
                .collapsed_workspaces
                .iter()
                .any(|c| workspace_paths_equal(c, trimmed));
            let existing_key = existing_ws
                .iter()
                .find(|r| workspace_paths_equal(&r.path, trimmed))
                .map(|r| r.path.clone())
                .unwrap_or_else(|| trimmed.to_string());
            let opened_ts = now - idx as i64;
            tx.execute(
                "INSERT INTO workspaces (path, name, pinned, collapsed, added_at, last_opened_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(path) DO UPDATE SET
                     name = excluded.name,
                     pinned = excluded.pinned,
                     collapsed = excluded.collapsed",
                params![
                    existing_key,
                    workspace_display_name(trimmed),
                    i64::from(pinned),
                    i64::from(collapsed),
                    now,
                    opened_ts
                ],
            )?;
        }

        // Sync pinned_chats
        tx.execute("UPDATE chats SET pinned = 0 WHERE pinned != 0", [])?;
        for cid in &state.pinned_chats {
            let cid = cid.trim();
            if cid.is_empty() {
                continue;
            }
            tx.execute(
                "INSERT INTO chats (id, pinned, created_at, updated_at)
                 VALUES (?1, 1, ?2, ?2)
                 ON CONFLICT(id) DO UPDATE SET pinned = 1",
                params![cid, now],
            )?;
        }
        tx.commit()?;

        // Sync app_state keys
        let sync_opt_str = |key: &str, val: Option<&str>| -> Result<()> {
            match val.map(str::trim).filter(|s| !s.is_empty()) {
                Some(v) => self.set_app_state(key, v),
                None => self.delete_app_state(key),
            }
        };
        sync_opt_str("last_working_dir", state.last_working_dir.as_deref())?;
        sync_opt_str("active_workspace", state.active_workspace.as_deref())?;
        sync_opt_str("last_session_id", state.last_session_id.as_deref())?;
        self.set_app_state("theme", state.theme.as_deref().unwrap_or("dark"))?;
        sync_opt_str("lang", state.lang.as_deref())?;
        match state.font_size {
            Some(f) => self.set_app_state("font_size", &f.to_string())?,
            None => self.delete_app_state("font_size")?,
        }
        match state.sidebar_open {
            Some(b) => self.set_app_state("sidebar_open", if b { "true" } else { "false" })?,
            None => self.delete_app_state("sidebar_open")?,
        }

        // Also avoid leaving stale user-home entries if a caller passed none
        let _ = is_user_home_dir;
        Ok(())
    }
}

pub(super) fn sync_session_renamed(
    session_id: &str,
    display_title: &str,
    custom_title: Option<&str>,
) {
    if let Ok(db) = GuiDb::open_default() {
        let _ = db.sync_single_session(session_id);
        let _ = db.set_chat_title(session_id, Some(display_title), custom_title);
    }
}

pub(super) fn sync_session_by_id(session_id: &str) {
    if let Ok(db) = GuiDb::open_default() {
        let _ = db.sync_single_session(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ContentBlock, Role};

    #[test]
    fn sqlite_schema_wal_and_workspace_chat_crud() {
        let temp = tempfile::tempdir().unwrap();
        let db_file = temp.path().join("alphacode.db");
        let db = GuiDb::open(&db_file).unwrap();

        // Default theme is dark
        assert_eq!(db.get_app_state("theme").unwrap().as_deref(), Some("dark"));
        let mode: String = db
            .conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode.to_ascii_lowercase(), "wal");

        // Workspaces CRUD
        let ws1 = temp.path().join("alpha");
        let ws2 = temp.path().join("beta");
        std::fs::create_dir_all(&ws1).unwrap();
        std::fs::create_dir_all(&ws2).unwrap();
        let ws1_str = ws1.to_string_lossy().into_owned();
        let ws2_str = ws2.to_string_lossy().into_owned();

        db.upsert_workspace(&ws1_str, None, true).unwrap();
        db.upsert_workspace(&ws2_str, Some("Beta App"), true)
            .unwrap();
        assert!(db.set_workspace_pinned(&ws1_str, Some(true)).unwrap());
        assert!(db.set_workspace_collapsed(&ws2_str, Some(true)).unwrap());

        let rows = db.list_workspaces().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].path, ws1_str);
        assert!(rows[0].pinned);
        assert!(!rows[0].collapsed);
        assert_eq!(rows[1].name, "Beta App");
        assert!(rows[1].collapsed);

        // Toggle pin/collapse
        assert!(!db.set_workspace_pinned(&ws1_str, None).unwrap());
        assert!(!db.set_workspace_collapsed(&ws2_str, None).unwrap());
        assert!(db.remove_workspace(&ws2_str).unwrap());
        assert_eq!(db.list_workspaces().unwrap().len(), 1);

        // Chats CRUD
        let chat = ChatRow {
            id: "sess_abc".into(),
            workspace_path: Some(ws1_str.clone()),
            title: Some("Initial Title".into()),
            custom_title: None,
            preview: "hello world".into(),
            pinned: true,
            saved: false,
            save_label: None,
            created_at: 100,
            updated_at: 200,
            message_count: 2,
        };
        db.upsert_chat(&chat).unwrap();
        db.set_chat_title("sess_abc", None, Some("Custom Name"))
            .unwrap();
        db.set_chat_saved("sess_abc", true, Some("important"))
            .unwrap();
        let loaded = db.get_chat("sess_abc").unwrap().unwrap();
        assert_eq!(loaded.custom_title.as_deref(), Some("Custom Name"));
        assert!(loaded.saved);
        assert_eq!(loaded.save_label.as_deref(), Some("important"));
        assert!(loaded.pinned);

        let by_ws = db.list_chats(Some(&ws1_str), 50).unwrap();
        assert_eq!(by_ws.len(), 1);
        assert!(db.delete_chat("sess_abc").unwrap());
        assert!(db.get_chat("sess_abc").unwrap().is_none());
    }

    #[test]
    fn migrates_legacy_gui_state_once_and_syncs_sessions_json() {
        let _lock = crate::storage::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let prev = std::env::var_os("ALPHACODE_HOME");
        crate::alphacode_core::env::set_var("ALPHACODE_HOME", temp.path());

        let ws_valid = temp.path().join("repo_one");
        std::fs::create_dir_all(&ws_valid).unwrap();
        let ws_valid_canon = normalize_existing_dir(&ws_valid.to_string_lossy()).unwrap();

        let home_dir = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(|p| PathBuf::from(p).to_string_lossy().into_owned())
            .unwrap_or_default();

        // Write legacy gui_state.json containing both a valid project and USERPROFILE
        let legacy_json = serde_json::json!({
            "last_working_dir": ws_valid_canon,
            "workspaces": [home_dir, ws_valid_canon],
            "pinned_workspaces": [ws_valid_canon],
            "collapsed_workspaces": [],
            "pinned_chats": ["sess_sync_1"],
            "theme": "light",
            "lang": "ar",
            "font_size": 15,
            "sidebar_open": false,
        });
        std::fs::write(
            temp.path().join("gui_state.json"),
            serde_json::to_string_pretty(&legacy_json).unwrap(),
        )
        .unwrap();

        // Create a real session file under sessions/<id>.json
        let mut sess = crate::session::Session::create_with_id(
            "sess_sync_1".to_string(),
            None,
            Some("Auto Title".to_string()),
        );
        sess.working_dir = Some(ws_valid_canon.clone());
        sess.add_message(
            Role::User,
            vec![ContentBlock::Text {
                text: "Fix database sync".into(),
                cache_control: None,
            }],
        );
        sess.save().unwrap();

        let db = GuiDb::open(&temp.path().join("alphacode.db")).unwrap();
        let state = db.load_gui_state().unwrap();
        assert_eq!(state.theme.as_deref(), Some("light"));
        assert_eq!(state.lang.as_deref(), Some("ar"));
        assert_eq!(state.font_size, Some(15));
        assert_eq!(state.sidebar_open, Some(false));
        assert_eq!(state.workspaces, vec![ws_valid_canon.clone()]);
        assert_eq!(state.pinned_workspaces, vec![ws_valid_canon.clone()]);
        assert!(
            !state
                .workspaces
                .iter()
                .any(|w| is_user_home_dir(Path::new(w)))
        );

        // Sync sessions/<id>.json -> chats table (preserving pinned = 1 from migration)
        let sessions_dir = temp.path().join("sessions");
        let stats = db.sync_sessions_index(&sessions_dir).unwrap();
        assert_eq!(stats.synced, 1);
        let chat = db.get_chat("sess_sync_1").unwrap().unwrap();
        assert_eq!(chat.title.as_deref(), Some("Auto Title"));
        assert_eq!(
            chat.workspace_path.as_deref(),
            Some(ws_valid_canon.as_str())
        );
        assert!(chat.pinned);
        assert_eq!(chat.preview, "Fix database sync");
        assert_eq!(chat.message_count, 1);

        // Removing <id>.json removes it from chats on next sync
        std::fs::remove_file(crate::session::session_path("sess_sync_1").unwrap()).unwrap();
        let stats2 = db.sync_sessions_index(&sessions_dir).unwrap();
        assert_eq!(stats2.removed, 1);
        assert!(db.get_chat("sess_sync_1").unwrap().is_none());

        if let Some(prev) = prev {
            crate::alphacode_core::env::set_var("ALPHACODE_HOME", prev);
        } else {
            crate::alphacode_core::env::remove_var("ALPHACODE_HOME");
        }
    }

    #[test]
    fn workspace_path_normalization_isolates_chats_by_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let db = GuiDb::open(&temp.path().join("alphacode.db")).unwrap();

        let ws1 = temp.path().join("proj_alpha");
        let ws2 = temp.path().join("proj_beta");
        std::fs::create_dir_all(&ws1).unwrap();
        std::fs::create_dir_all(&ws2).unwrap();

        let ws1_forward = format!("{}/", ws1.to_string_lossy().replace('\\', "/"));
        let ws1_norm = normalize_workspace_path(&ws1.to_string_lossy()).unwrap();
        let ws2_norm = normalize_workspace_path(&ws2.to_string_lossy()).unwrap();

        db.upsert_chat(&ChatRow {
            id: "chat_a1".into(),
            workspace_path: Some(ws1_forward.clone()),
            title: Some("Alpha Chat".into()),
            custom_title: None,
            preview: "alpha".into(),
            pinned: false,
            saved: false,
            save_label: None,
            created_at: 10,
            updated_at: 20,
            message_count: 1,
        })
        .unwrap();
        db.upsert_chat(&ChatRow {
            id: "chat_b1".into(),
            workspace_path: Some(ws2_norm.clone()),
            title: Some("Beta Chat".into()),
            custom_title: None,
            preview: "beta".into(),
            pinned: false,
            saved: false,
            save_label: None,
            created_at: 15,
            updated_at: 25,
            message_count: 1,
        })
        .unwrap();
        db.upsert_chat(&ChatRow {
            id: "chat_none".into(),
            workspace_path: Some("   ".into()),
            title: Some("Unassigned".into()),
            custom_title: None,
            preview: "none".into(),
            pinned: false,
            saved: false,
            save_label: None,
            created_at: 12,
            updated_at: 22,
            message_count: 1,
        })
        .unwrap();

        assert_eq!(
            db.get_chat("chat_a1")
                .unwrap()
                .unwrap()
                .workspace_path
                .as_deref(),
            Some(ws1_norm.as_str())
        );
        assert!(
            db.get_chat("chat_none")
                .unwrap()
                .unwrap()
                .workspace_path
                .is_none()
        );

        let alpha_chats = db.list_chats(Some(&ws1_forward), 50).unwrap();
        assert_eq!(alpha_chats.len(), 1);
        assert_eq!(alpha_chats[0].id, "chat_a1");

        let beta_chats = db.list_chats(Some(&ws2_norm), 50).unwrap();
        assert_eq!(beta_chats.len(), 1);
        assert_eq!(beta_chats[0].id, "chat_b1");
    }
}
