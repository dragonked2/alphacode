use crate::memory_graph::MemoryGraph;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

// === Graph Cache ===

/// Maximum number of cached graphs. Each entry holds an owned graph; an
/// unbounded map kept every memory-file path ever queried resident for the
/// life of the process (audit M22). The working set in practice is two paths
/// (project + global); 24 leaves ample room for distinct session roots.
const GRAPH_CACHE_CAPACITY: usize = 24;

struct GraphCacheEntry {
    graph: Arc<MemoryGraph>,
    modified: Option<SystemTime>,
}

struct GraphCache {
    /// Ordered so insertion-time eviction can drop the oldest entries; there
    /// is no cheap staleness signal beyond the mtime we already compare.
    entries: BTreeMap<PathBuf, GraphCacheEntry>,
}

impl GraphCache {
    fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    fn evict_if_over_capacity(&mut self) {
        while self.entries.len() >= GRAPH_CACHE_CAPACITY {
            // The map is keyed by path, not by recency, and loads are rare
            // relative to the hit rate, so dropping the first (deterministic)
            // entry is fine: it is re-parsed on next access.
            self.entries.pop_first();
        }
    }
}

static GRAPH_CACHE: OnceLock<Mutex<GraphCache>> = OnceLock::new();

fn graph_cache() -> &'static Mutex<GraphCache> {
    GRAPH_CACHE.get_or_init(|| Mutex::new(GraphCache::new()))
}

fn graph_mtime(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/// Zero-copy hit: share the cached graph without cloning it.
///
/// Callers that only *read* the graph (health counters, ambient stats) used
/// to pay a full deep clone of the entire graph on every hit — the same order
/// of work as parsing it from disk (audit M22). An `Arc::clone` is O(1), and
/// a read hit stays in cache as long as the file mtime is unchanged.
pub(super) fn cached_graph_arc(path: &PathBuf) -> Option<Arc<MemoryGraph>> {
    let modified = graph_mtime(path);
    let cache = graph_cache().lock().ok()?;
    let entry = cache.entries.get(path)?;
    if entry.modified == modified {
        Some(Arc::clone(&entry.graph))
    } else {
        None
    }
}

#[allow(dead_code)]
pub(super) fn cached_graph(path: &PathBuf) -> Option<MemoryGraph> {
    cached_graph_arc(path).map(|shared| (*shared).clone())
}

#[allow(dead_code)]
pub(super) fn cache_graph(path: PathBuf, graph: &MemoryGraph) {
    let modified = graph_mtime(&path);
    if let Ok(mut cache) = graph_cache().lock() {
        cache.evict_if_over_capacity();
        cache.entries.insert(
            path,
            GraphCacheEntry {
                graph: Arc::new(graph.clone()),
                modified,
            },
        );
    }
}
