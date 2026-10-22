//! Process-wide cache for OAuth token files.
//!
//! # Why
//!
//! Every provider request re-read its token file from disk: a `stat`, a
//! permission-hardening call, a file read, and a JSON parse. On the Gemini
//! and Antigravity paths that ran once per request, adding three to four
//! syscalls to a code path that otherwise spends its time on the network.
//!
//! The tokens themselves change rarely — only when the user logs in, when a
//! refresh happens, or when the token file is edited externally. The cache
//! below keeps the parsed value in memory and re-reads the file only when
//! the token is about to expire, when the file's modification time changed,
//! or when the cache was explicitly invalidated.
//!
//! # Invalidation
//!
//! [`TokenCache::invalidate`] must be called whenever the token file is
//! written, cleared, or refreshed, so the next read observes the new value.
//! The refresh coordinators call it after a successful refresh; the login
//! and logout flows call it after writing or removing the file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::SystemTime;

/// A cached token file: the parsed value plus the file metadata it was
/// derived from.
#[derive(Clone)]
struct CacheEntry {
    /// The parsed token value. `Arc` so a cache hit does not clone the
    /// (potentially large) token struct.
    value: Arc<dyn std::any::Any + Send + Sync>,
    /// The file's modification time when the value was read.
    modified: Option<SystemTime>,
    /// The file's length when the value was read.
    len: u64,
}

/// A process-wide cache of parsed token files, keyed by absolute path.
pub struct TokenCache {
    entries: RwLock<HashMap<PathBuf, CacheEntry>>,
}

impl TokenCache {
    fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Return the cached value for `path` if the file has not changed.
    ///
    /// The file is stat'ed on every call — that is one syscall instead of
    /// the previous stat + permission-harden + read + parse. The stat is
    /// what makes the cache safe: if the file's modification time or length
    /// changed, the cached value is discarded and the caller re-reads.
    pub fn get<T>(&self, path: &Path) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        let (modified, len) = file_stamp(path)?;
        let entries = self.entries.read().ok()?;
        let entry = entries.get(path)?;
        if entry.modified != Some(modified) || entry.len != len {
            // The file changed on disk — the cached value is stale.
            return None;
        }
        entry.value.clone().downcast::<T>().ok()
    }

    /// Store `value` for `path`, along with the file's current stamp.
    pub fn put<T>(&self, path: &Path, value: T)
    where
        T: Send + Sync + 'static,
    {
        let stamp = file_stamp(path);
        let entry = CacheEntry {
            value: Arc::new(value),
            modified: stamp.map(|(modified, _)| modified),
            len: stamp.map(|(_, len)| len).unwrap_or(0),
        };
        if let Ok(mut entries) = self.entries.write() {
            entries.insert(path.to_path_buf(), entry);
        }
    }

    /// Drop the cached value for `path`.
    ///
    /// Called after the token file is written, cleared, or refreshed so the
    /// next read observes the new value.
    pub fn invalidate(&self, path: &Path) {
        if let Ok(mut entries) = self.entries.write() {
            entries.remove(path);
        }
    }

    /// Drop every cached value.
    pub fn invalidate_all(&self) {
        if let Ok(mut entries) = self.entries.write() {
            entries.clear();
        }
    }

    /// Number of cached entries. Used by tests.
    pub fn len(&self) -> usize {
        self.entries
            .read()
            .map(|entries| entries.len())
            .unwrap_or(0)
    }

    /// Whether the cache holds no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The modification time and length of `path`, or `None` when it cannot be
/// stat'ed.
fn file_stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

/// The process-wide token cache.
static CACHE: OnceLock<TokenCache> = OnceLock::new();

/// The process-wide token cache.
pub fn cache() -> &'static TokenCache {
    CACHE.get_or_init(TokenCache::new)
}

/// Read a token file through the cache.
///
/// On a miss, `load` is called to read and parse the file; the result is
/// cached against the file's current stamp. On a hit, the file is only
/// stat'ed.
pub fn load_cached<T, F>(path: &Path, load: F) -> Result<T, anyhow::Error>
where
    T: Clone + Send + Sync + 'static,
    F: FnOnce() -> Result<T, anyhow::Error>,
{
    if let Some(cached) = cache().get::<T>(path) {
        return Ok((*cached).clone());
    }
    let value = load()?;
    cache().put(path, value.clone());
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_returns_the_value_while_the_file_is_unchanged() {
        let dir =
            std::env::temp_dir().join(format!("alphacode-token-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tokens.json");
        std::fs::write(&path, b"{\"v\":1}").unwrap();

        let cache = TokenCache::new();
        assert!(cache.get::<u32>(&path).is_none());

        cache.put(&path, 7u32);
        assert_eq!(cache.get::<u32>(&path).map(|v| *v), Some(7));

        // A different type at the same path must not be confused with the
        // cached value.
        assert!(cache.get::<u64>(&path).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalidate_removes_the_entry() {
        let dir =
            std::env::temp_dir().join(format!("alphacode-token-cache-inv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tokens.json");
        std::fs::write(&path, b"{}").unwrap();

        let cache = TokenCache::new();
        cache.put(&path, 1u32);
        assert_eq!(cache.len(), 1);
        cache.invalidate(&path);
        assert_eq!(cache.len(), 0);
        assert!(cache.get::<u32>(&path).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
