use serde::{Deserialize, Serialize};

/// A reusable knowledge entry extracted from live analysis.
///
/// Everything here is discovered at runtime from the target.
/// No vulnerability patterns, payloads, or techniques are pre-populated.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub id: String,
    pub title: String,
    pub vulnerability_class: String,
    pub root_cause: String,
    pub attack_preconditions: Vec<String>,
    pub request_pattern: Option<String>,
    pub response_pattern: Option<String>,
    pub technology: Option<String>,
    pub bypass_techniques: Vec<String>,
    pub edge_cases: Vec<String>,
    pub validation_method: String,
    pub impact: String,
    pub defensive_lesson: String,
    pub tags: Vec<String>,
    pub source: Option<String>,
    pub confidence: f64,
}

impl KnowledgeEntry {
    pub fn new(id: String, title: String, vuln_class: String, root_cause: String) -> Self {
        Self {
            id,
            title,
            vulnerability_class: vuln_class,
            root_cause,
            attack_preconditions: Vec::new(),
            request_pattern: None,
            response_pattern: None,
            technology: None,
            bypass_techniques: Vec::new(),
            edge_cases: Vec::new(),
            validation_method: String::new(),
            impact: String::new(),
            defensive_lesson: String::new(),
            tags: Vec::new(),
            source: None,
            confidence: 0.5,
        }
    }

    pub fn is_relevant(&self, tech: &str, vuln_class: &str) -> bool {
        let tech_match = self
            .technology
            .as_ref()
            .map(|t| t.eq_ignore_ascii_case(tech))
            .unwrap_or(true);
        let class_match = self.vulnerability_class.eq_ignore_ascii_case(vuln_class);
        tech_match && class_match
    }
}

/// A knowledge store populated entirely from live analysis.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct KnowledgeStore {
    pub entries: Vec<KnowledgeEntry>,
}

impl KnowledgeStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, entry: KnowledgeEntry) {
        if !self.entries.iter().any(|e| e.id == entry.id) {
            self.entries.push(entry);
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() < before
    }

    /// Lookup by class, optionally filtered by technology.
    ///
    /// `technology=None` matches everything. `Some(t)` matches entries for
    /// technology `t` PLUS generic entries with no technology set (wildcards).
    pub fn lookup(&self, vuln_class: &str, technology: Option<&str>) -> Vec<&KnowledgeEntry> {
        self.entries
            .iter()
            .filter(|e| {
                let class_match = e.vulnerability_class.eq_ignore_ascii_case(vuln_class);
                let tech_match = technology
                    .map(|t| {
                        e.technology
                            .as_ref()
                            .map(|et| et.eq_ignore_ascii_case(t))
                            .unwrap_or(true)
                    })
                    .unwrap_or(true);
                class_match && tech_match
            })
            .collect()
    }

    /// Strict technology filter: excludes generic entries with no technology set.
    /// Use [`lookup`](Self::lookup) when generic entries should be included.
    pub fn by_technology(&self, tech: &str) -> Vec<&KnowledgeEntry> {
        self.entries
            .iter()
            .filter(|e| {
                e.technology
                    .as_ref()
                    .map(|t| t.eq_ignore_ascii_case(tech))
                    .unwrap_or(false)
            })
            .collect()
    }

    pub fn by_vuln_class(&self, class: &str) -> Vec<&KnowledgeEntry> {
        self.entries
            .iter()
            .filter(|e| e.vulnerability_class.eq_ignore_ascii_case(class))
            .collect()
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }

    pub fn merge(&mut self, other: KnowledgeStore) {
        for entry in other.entries {
            self.add(entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knowledge_store_add_and_lookup() {
        let mut store = KnowledgeStore::new();
        let entry = KnowledgeEntry::new(
            "k1".into(),
            "Test finding".into(),
            "xss".into(),
            "unescaped output".into(),
        );
        store.add(entry);
        assert_eq!(store.count(), 1);

        let results = store.lookup("xss", None);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn knowledge_store_deduplicates() {
        let mut store = KnowledgeStore::new();
        store.add(KnowledgeEntry::new(
            "k1".into(),
            "A".into(),
            "xss".into(),
            "rc".into(),
        ));
        store.add(KnowledgeEntry::new(
            "k1".into(),
            "B".into(),
            "sqli".into(),
            "rc".into(),
        ));
        assert_eq!(store.count(), 1);
    }

    #[test]
    fn knowledge_store_remove() {
        let mut store = KnowledgeStore::new();
        store.add(KnowledgeEntry::new(
            "k1".into(),
            "A".into(),
            "xss".into(),
            "rc".into(),
        ));
        assert!(store.remove("k1"));
        assert_eq!(store.count(), 0);
        assert!(!store.remove("k1"));
    }

    #[test]
    fn knowledge_store_merge() {
        let mut a = KnowledgeStore::new();
        a.add(KnowledgeEntry::new(
            "k1".into(),
            "A".into(),
            "xss".into(),
            "rc".into(),
        ));
        let mut b = KnowledgeStore::new();
        b.add(KnowledgeEntry::new(
            "k2".into(),
            "B".into(),
            "sqli".into(),
            "rc".into(),
        ));
        b.add(KnowledgeEntry::new(
            "k1".into(),
            "C".into(),
            "xss".into(),
            "rc2".into(),
        ));
        a.merge(b);
        assert_eq!(a.count(), 2);
    }

    #[test]
    fn knowledge_by_technology_filters() {
        let mut store = KnowledgeStore::new();
        store.add(KnowledgeEntry {
            technology: Some("react".into()),
            ..KnowledgeEntry::new("k1".into(), "A".into(), "xss".into(), "rc".into())
        });
        store.add(KnowledgeEntry {
            technology: None,
            ..KnowledgeEntry::new("k2".into(), "B".into(), "xss".into(), "rc".into())
        });
        let react = store.by_technology("react");
        assert_eq!(react.len(), 1);
        let all = store.by_vuln_class("xss");
        assert_eq!(all.len(), 2);
    }
}
