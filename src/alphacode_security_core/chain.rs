use serde::{Deserialize, Serialize};

use super::finding::{Confidence, Severity};

/// An attack chain: two or more findings that together create meaningful impact.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttackChain {
    pub id: String,
    pub name: String,
    pub description: String,
    pub links: Vec<ChainLink>,
    pub combined_impact: String,
    pub confidence: Confidence,
    pub severity: Severity,
    pub status: ChainStatus,
    pub evidence: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainLink {
    pub finding_id: String,
    pub role: String,
    pub order: u32,
    pub required_state: Option<String>,
    pub dependency_satisfied: bool,
    #[serde(default)]
    pub severity: Severity,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainStatus {
    Proposed,
    PartiallyValidated,
    FullyValidated,
    Refuted,
}

impl ChainStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::PartiallyValidated => "partially_validated",
            Self::FullyValidated => "fully_validated",
            Self::Refuted => "refuted",
        }
    }
}

impl AttackChain {
    pub fn new(id: String, name: String, description: String) -> Self {
        Self {
            id,
            name,
            description,
            links: Vec::new(),
            combined_impact: String::new(),
            confidence: Confidence::Low,
            severity: Severity::Info,
            status: ChainStatus::Proposed,
            evidence: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn add_link(&mut self, link: ChainLink) {
        self.links.push(link);
        self.links.sort_by_key(|l| l.order);
    }

    pub fn all_dependencies_satisfied(&self) -> bool {
        self.links.iter().all(|l| l.dependency_satisfied)
    }

    pub fn validate_chain(&mut self) {
        if self.links.len() < 2 {
            self.status = ChainStatus::Refuted;
            let msg = "Chain requires at least 2 linked findings".to_string();
            if !self.notes.contains(&msg) {
                self.notes.push(msg);
            }
            return;
        }

        if self.all_dependencies_satisfied() {
            self.status = ChainStatus::FullyValidated;
        } else {
            self.status = ChainStatus::PartiallyValidated;
        }
    }

    pub fn calculate_combined_severity(&mut self) {
        if self.links.is_empty() {
            self.severity = Severity::Info;
            return;
        }

        let max_severity = self
            .links
            .iter()
            .map(|l| l.severity.clone())
            .max_by_key(|s| s.numeric_value())
            .unwrap_or(Severity::Info);

        // Chains escalate severity one step because combined impact exceeds parts.
        self.severity = match max_severity {
            Severity::Info => Severity::Low,
            Severity::Low => Severity::Medium,
            Severity::Medium => Severity::High,
            Severity::High | Severity::Critical => Severity::Critical,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(id: &str, order: u32, severity: Severity, satisfied: bool) -> ChainLink {
        ChainLink {
            finding_id: id.to_string(),
            role: format!("step{order}"),
            order,
            required_state: None,
            dependency_satisfied: satisfied,
            severity,
        }
    }

    #[test]
    fn chain_requires_two_links() {
        let mut chain = AttackChain::new(
            "c1".to_string(),
            "Auth bypass + data exfil".to_string(),
            "Bypass auth then access admin data".to_string(),
        );
        chain.add_link(link("f1", 1, Severity::Medium, true));
        chain.validate_chain();
        assert_eq!(chain.status, ChainStatus::Refuted);
        // Re-validating must not duplicate the note.
        chain.validate_chain();
        assert_eq!(
            chain
                .notes
                .iter()
                .filter(|n| n.contains("at least 2"))
                .count(),
            1
        );
    }

    #[test]
    fn chain_validated_with_two_satisfied_links() {
        let mut chain = AttackChain::new("c1".to_string(), "Chain".to_string(), "desc".to_string());
        chain.add_link(link("f1", 1, Severity::Medium, true));
        let mut second = link("f2", 2, Severity::High, true);
        second.required_state = Some("f1 completed".to_string());
        chain.add_link(second);
        chain.validate_chain();
        assert_eq!(chain.status, ChainStatus::FullyValidated);
    }

    #[test]
    fn chain_partially_validated() {
        let mut chain = AttackChain::new("c1".to_string(), "Chain".to_string(), "desc".to_string());
        chain.add_link(link("f1", 1, Severity::Medium, true));
        let mut second = link("f2", 2, Severity::Medium, false);
        second.required_state = Some("f1 completed".to_string());
        chain.add_link(second);
        chain.validate_chain();
        assert_eq!(chain.status, ChainStatus::PartiallyValidated);
    }

    #[test]
    fn chain_severity_escalation() {
        let mut chain = AttackChain::new("c1".to_string(), "Chain".to_string(), "desc".to_string());
        chain.add_link(link("f1", 1, Severity::Medium, true));
        chain.add_link(link("f2", 2, Severity::Medium, true));
        chain.calculate_combined_severity();
        // Medium + Medium escalates to High.
        assert_eq!(chain.severity, Severity::High);

        let mut low = AttackChain::new("c2".to_string(), "low".to_string(), "d".to_string());
        low.add_link(link("f1", 1, Severity::Low, true));
        low.add_link(link("f2", 2, Severity::Info, true));
        low.calculate_combined_severity();
        assert_eq!(low.severity, Severity::Medium);
    }
}
