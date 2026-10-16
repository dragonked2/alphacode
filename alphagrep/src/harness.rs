//! Reader for the session harness context.
//!
//! The alphacode agent tool writes a small JSON document describing what the
//! session has already read and searched, then deletes it after the call. Trace
//! uses it to prefer files and symbols the agent has already touched, and
//! outline uses it to mention that part of a file is already in context.
//! Everything here is best-effort: an unreadable or malformed document simply
//! means no session boosts.

use crate::fsutil;
use serde::Deserialize;

/// One region the session has already seen.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct KnownRegion {
    /// Path as recorded by the agent tool.
    #[serde(default)]
    pub path: String,
    /// First line of the region.
    #[serde(default)]
    pub start_line: usize,
    /// Last line of the region.
    #[serde(default)]
    pub end_line: usize,
    /// How much body content the region carries.
    #[serde(default)]
    pub body_confidence: f32,
    /// How likely the content is still current.
    #[serde(default)]
    pub current_version_confidence: f32,
    /// How likely the region can be pruned from context.
    #[serde(default)]
    pub prune_confidence: f32,
    /// How the region was learned (full region, snippet, ...).
    #[serde(default)]
    pub source_strength: String,
    /// Free-form provenance tags.
    #[serde(default)]
    pub reasons: Vec<String>,
}

/// One file the session has already seen.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct KnownFile {
    /// Path as recorded by the agent tool.
    #[serde(default)]
    pub path: String,
    /// Confidence that the structure of the file is known.
    #[serde(default)]
    pub structure_confidence: f32,
    /// Confidence that the body is known.
    #[serde(default)]
    pub body_confidence: f32,
    /// Confidence that the knowledge is still current.
    #[serde(default)]
    pub current_version_confidence: f32,
    /// Confidence that the entry can be pruned.
    #[serde(default)]
    pub prune_confidence: f32,
    /// How the file was learned.
    #[serde(default)]
    pub source_strength: String,
    /// Free-form provenance tags.
    #[serde(default)]
    pub reasons: Vec<String>,
}

/// One symbol the session has already seen.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct KnownSymbol {
    /// Path as recorded by the agent tool.
    #[serde(default)]
    pub path: String,
    /// Symbol name.
    #[serde(default)]
    pub symbol: String,
    /// Declaration kind when known.
    #[serde(default)]
    pub kind: Option<String>,
    /// Confidence that the structure is known.
    #[serde(default)]
    pub structure_confidence: f32,
    /// Confidence that the body is known.
    #[serde(default)]
    pub body_confidence: f32,
    /// Confidence that the knowledge is still current.
    #[serde(default)]
    pub current_version_confidence: f32,
    /// Confidence that the entry can be pruned.
    #[serde(default)]
    pub prune_confidence: f32,
    /// How the symbol was learned.
    #[serde(default)]
    pub source_strength: String,
    /// Free-form provenance tags.
    #[serde(default)]
    pub reasons: Vec<String>,
}

/// The whole harness document.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct HarnessContext {
    /// Document version written by the producer.
    #[serde(default)]
    pub version: u32,
    /// Regions already seen.
    #[serde(default)]
    pub known_regions: Vec<KnownRegion>,
    /// Files already seen.
    #[serde(default)]
    pub known_files: Vec<KnownFile>,
    /// Symbols already seen.
    #[serde(default)]
    pub known_symbols: Vec<KnownSymbol>,
    /// Paths the session is currently working on.
    #[serde(default)]
    pub focus_files: Vec<String>,
}

impl HarnessContext {
    /// Load the context document, returning `None` on any problem.
    pub fn load(path: Option<&str>) -> Option<Self> {
        let path = path?;
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Ranking boost for a path the session is actively working on.
    pub fn focus_boost(&self, relative: &str) -> f32 {
        if self
            .focus_files
            .iter()
            .any(|focus| path_matches(focus, relative))
        {
            12.0
        } else {
            0.0
        }
    }

    /// Ranking boost for a file the session has already seen.
    pub fn known_file_boost(&self, relative: &str) -> f32 {
        match self
            .known_files
            .iter()
            .find(|known| path_matches(&known.path, relative))
        {
            Some(known) => 4.0 + 6.0 * known.structure_confidence.clamp(0.0, 1.0),
            None => 0.0,
        }
    }

    /// Ranking boost for a symbol the session has already seen.
    pub fn symbol_boost(&self, relative: &str, label: &str) -> f32 {
        let lowered = label.to_ascii_lowercase();
        match self.known_symbols.iter().find(|known| {
            path_matches(&known.path, relative) && known.symbol.to_ascii_lowercase() == lowered
        }) {
            Some(known) => 6.0 + 6.0 * known.structure_confidence.clamp(0.0, 1.0),
            None => 0.0,
        }
    }

    /// Regions the session has already seen in one file.
    pub fn known_regions_in(&self, relative: &str) -> Vec<&KnownRegion> {
        self.known_regions
            .iter()
            .filter(|region| path_matches(&region.path, relative))
            .collect()
    }

    /// Whether the session already saw a region covering these lines.
    pub fn region_seen(&self, relative: &str, start_line: usize, end_line: usize) -> bool {
        self.known_regions_in(relative)
            .iter()
            .any(|region| region.start_line <= start_line && region.end_line >= end_line)
    }
}

/// Compare a harness path with an alphagrep path, tolerating separator and
/// root differences such as `src\lib.rs` versus `lib.rs`.
fn path_matches(left: &str, right: &str) -> bool {
    if fsutil::same_path(left, right) {
        return true;
    }
    let name_of = |value: &str| {
        value
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(value)
            .to_ascii_lowercase()
    };
    let left_has_directory = left.contains('/') || left.contains('\\');
    let right_has_directory = right.contains('/') || right.contains('\\');
    name_of(left) == name_of(right) && (!left_has_directory || !right_has_directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_written_context_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("context.json");
        fs::write(
            &path,
            r#"{
                "version": 1,
                "known_files": [
                    {"path": "src\\auth.rs", "structure_confidence": 0.5, "body_confidence": 0.2,
                     "current_version_confidence": 0.8, "prune_confidence": 0.1,
                     "source_strength": "snippet", "reasons": ["read_tool_exposure"]}
                ],
                "known_symbols": [
                    {"path": "src/auth.rs", "symbol": "render_status", "kind": "fn",
                     "structure_confidence": 0.9, "body_confidence": 0.1, "current_version_confidence": 0.8,
                     "prune_confidence": 0.5, "source_strength": "outline_only", "reasons": []}
                ],
                "focus_files": ["src/auth.rs"],
                "future_field": true
            }"#,
        )
        .expect("write");

        let context = HarnessContext::load(Some(&path.display().to_string())).expect("context");
        assert_eq!(context.version, 1);
        assert_eq!(context.focus_boost("src/auth.rs"), 12.0);
        assert!(context.known_file_boost("auth.rs") > 4.0);
        assert!(context.symbol_boost("src/auth.rs", "render_status") > 6.0);
        assert_eq!(context.known_regions_in("src/auth.rs").len(), 0);
    }

    #[test]
    fn missing_or_broken_documents_yield_no_context() {
        assert!(HarnessContext::load(None).is_none());
        assert!(HarnessContext::load(Some("does-not-exist.json")).is_none());
    }
}
