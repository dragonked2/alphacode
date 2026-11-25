//! Bounded, metadata-validated graph snapshots.
use crate::memory_graph::MemoryGraph;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

const GRAPH_CACHE_CAPACITY: usize = 24;
// Metadata can be preserved or have coarse resolution. Periodically re-read.
const MAX_SNAPSHOT_AGE: Duration = Duration::from_secs(30);

#[derive(Clone, PartialEq, Eq)]
struct FileStamp {
    modified: SystemTime,
    len: u64,
    created: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    ctime: (i64, i64),
}

fn stamp(path: &Path) -> Option<FileStamp> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_file() {
        return None;
    }
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Some(FileStamp {
        modified: meta.modified().ok()?,
        len: meta.len(),
        created: meta.created().ok(),
        #[cfg(unix)]
        device: meta.dev(),
        #[cfg(unix)]
        inode: meta.ino(),
        #[cfg(unix)]
        ctime: (meta.ctime(), meta.ctime_nsec()),
    })
}

struct GraphCacheEntry {
    graph: Arc<MemoryGraph>,
    stamp: FileStamp,
    inserted: Instant,
    used: Instant,
}

static GRAPH_CACHE: OnceLock<Mutex<BTreeMap<PathBuf, GraphCacheEntry>>> = OnceLock::new();

fn graph_cache() -> &'static Mutex<BTreeMap<PathBuf, GraphCacheEntry>> {
    GRAPH_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(super) fn cached_graph_arc(path: &Path) -> Option<Arc<MemoryGraph>> {
    let current = stamp(path);
    let mut cache = graph_cache().lock().ok()?;
    let valid = cache.get(path).is_some_and(|entry| {
        current.as_ref() == Some(&entry.stamp) && entry.inserted.elapsed() < MAX_SNAPSHOT_AGE
    });
    if !valid {
        cache.remove(path);
        return None;
    }
    let entry = cache.get_mut(path)?;
    entry.used = Instant::now();
    Some(Arc::clone(&entry.graph))
}

#[allow(dead_code)]
pub(super) fn cached_graph(path: &Path) -> Option<MemoryGraph> {
    cached_graph_arc(path).map(|shared| (*shared).clone())
}

#[allow(dead_code)]
pub(super) fn cache_graph(path: PathBuf, graph: &MemoryGraph) {
    let Some(current) = stamp(&path) else {
        if let Ok(mut cache) = graph_cache().lock() {
            cache.remove(&path);
        }
        return;
    };
    // Clone outside the global lock.
    let graph = Arc::new(graph.clone());
    if stamp(&path).as_ref() != Some(&current) {
        return;
    }
    if let Ok(mut cache) = graph_cache().lock() {
        if !cache.contains_key(&path)
            && cache.len() >= GRAPH_CACHE_CAPACITY
            && let Some(oldest) = cache
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(path, _)| path.clone())
        {
            cache.remove(&oldest);
        }
        let now = Instant::now();
        cache.insert(
            path,
            GraphCacheEntry {
                graph,
                stamp: current,
                inserted: now,
                used: now,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_or_changed_file_cannot_hit_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.json");
        let graph = MemoryGraph::new();
        cache_graph(path.clone(), &graph);
        assert!(cached_graph_arc(&path).is_none());
        std::fs::write(&path, b"one").unwrap();
        cache_graph(path.clone(), &graph);
        assert!(cached_graph_arc(&path).is_some());
        std::fs::write(&path, b"changed length").unwrap();
        assert!(cached_graph_arc(&path).is_none());
        cache_graph(path.clone(), &graph);
        std::fs::remove_file(&path).unwrap();
        assert!(cached_graph_arc(&path).is_none());
    }
}
