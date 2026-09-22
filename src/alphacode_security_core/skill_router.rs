use serde::{Deserialize, Serialize};

use super::finding::VulnerabilityClass;
use super::hypothesis::Hypothesis;
use super::scope::LiveScope;

/// A security skill descriptor — what it covers and when to use it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecuritySkillDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
    pub family: SkillFamily,
    pub applicable_tech: Vec<String>,
    pub applicable_vuln_classes: Vec<String>,
    pub required_phase: Vec<String>,
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillFamily {
    Recon,
    Authentication,
    Authorization,
    WebVuln,
    Logic,
    ModernTargets,
    CodeAnalysis,
    Ctf,
    Reporting,
}

impl SkillFamily {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Recon => "recon",
            Self::Authentication => "authentication",
            Self::Authorization => "authorization",
            Self::WebVuln => "web_vuln",
            Self::Logic => "logic",
            Self::ModernTargets => "modern_targets",
            Self::CodeAnalysis => "code_analysis",
            Self::Ctf => "ctf",
            Self::Reporting => "reporting",
        }
    }
}

/// Routing decision: which skills to load for a given context.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkillRoute {
    pub skills: Vec<String>,
    pub rationale: String,
    pub confidence: f64,
}

/// Routes skills based on live target analysis.
pub struct SkillRouter;

impl SkillRouter {
    /// Determine which skills are relevant given the current scope and hypothesis.
    pub fn route(scope: &LiveScope, hypothesis: Option<&Hypothesis>) -> SkillRoute {
        let mut selected = Vec::new();
        let mut rationale_parts = Vec::new();

        // Phase-based routing
        match scope.mode {
            super::scope::EngagementMode::Ctf => {
                selected.push("ctf".to_string());
                rationale_parts.push("CTF mode active".to_string());
            }
            super::scope::EngagementMode::BugBounty => {
                rationale_parts.push("Bug bounty engagement".to_string());
            }
            _ => {}
        }

        // Technology-based routing
        for tech in &scope.technologies {
            let skills = Self::skills_for_technology(&tech.name);
            for skill in skills {
                if !selected.contains(&skill) {
                    selected.push(skill);
                    rationale_parts.push(format!("Technology: {}", tech.name));
                }
            }
        }

        // Hypothesis-based routing
        if let Some(h) = hypothesis {
            let skills = Self::skills_for_vuln_class(&h.vuln_class);
            for skill in skills {
                if !selected.contains(&skill) {
                    selected.push(skill);
                    rationale_parts.push(format!("Hypothesis: {}", h.description));
                }
            }
        }

        // Endpoint-based routing
        let has_auth_endpoints = scope.discovered_endpoints.iter().any(|e| e.requires_auth);
        if has_auth_endpoints && !selected.contains(&"authentication-analysis".to_string()) {
            selected.push("authentication-analysis".to_string());
            rationale_parts.push("Authenticated endpoints discovered".to_string());
        }

        let has_api_endpoints = scope.discovered_endpoints.iter().any(|e| {
            let p = e.path.to_lowercase();
            p.contains("/api")
                || p.contains("/graphql")
                || p.contains("/rest")
                || p.contains("/v1/")
                || p.contains("/v2/")
        });
        if has_api_endpoints && !selected.contains(&"api-discovery".to_string()) {
            selected.push("api-discovery".to_string());
            rationale_parts.push("API endpoints discovered".to_string());
        }

        // Always include recon if surface is small (needs more discovery)
        if scope.attack_surface_size() < 5 && !selected.contains(&"passive-recon".to_string()) {
            selected.push("passive-recon".to_string());
            rationale_parts.push("Small attack surface, more recon needed".to_string());
        }

        SkillRoute {
            skills: selected,
            rationale: rationale_parts.join("; "),
            confidence: if rationale_parts.is_empty() { 0.3 } else { 0.7 },
        }
    }

    /// Map a technology name to applicable skills.
    fn skills_for_technology(tech: &str) -> Vec<String> {
        let mut skills = Vec::new();
        let lower = tech.to_lowercase();
        let tokens: Vec<&str> = lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|t| !t.is_empty())
            .collect();
        let has_token = |t: &str| tokens.contains(&t);

        if lower.contains("graphql") {
            skills.push("graphql".to_string());
        }
        if lower.contains("grpc") {
            skills.push("grpc".to_string());
        }
        if lower.contains("websocket") || has_token("ws") {
            skills.push("websocket".to_string());
        }
        if lower.contains("react")
            || lower.contains("next")
            || lower.contains("vue")
            || lower.contains("angular")
        {
            skills.push("spa-analysis".to_string());
        }
        if lower.contains("node") || lower.contains("express") || lower.contains("fastify") {
            skills.push("javascript-analysis".to_string());
        }
        if lower.contains("oauth") || lower.contains("oidc") {
            skills.push("oauth-analysis".to_string());
        }
        if lower.contains("jwt") {
            skills.push("jwt-analysis".to_string());
        }
        if lower.contains("kubernetes") || lower.contains("k8s") {
            skills.push("kubernetes".to_string());
        }
        if lower.contains("docker") || lower.contains("container") {
            skills.push("containers".to_string());
        }
        if lower.contains("serverless") || lower.contains("lambda") || lower.contains("function") {
            skills.push("serverless".to_string());
        }
        if lower.contains("aws") || lower.contains("azure") || lower.contains("gcp") {
            skills.push("cloud".to_string());
        }

        skills
    }

    /// Map a vulnerability class to applicable skills.
    fn skills_for_vuln_class(vuln_class: &VulnerabilityClass) -> Vec<String> {
        match vuln_class {
            VulnerabilityClass::Xss | VulnerabilityClass::DomXss => vec!["xss".to_string()],
            VulnerabilityClass::SqlInjection => vec!["sqli".to_string()],
            VulnerabilityClass::NoSqlInjection => vec!["nosqli".to_string()],
            VulnerabilityClass::Ssrf => vec!["ssrf".to_string()],
            VulnerabilityClass::IdorBola => vec!["idor".to_string()],
            VulnerabilityClass::CommandInjection => vec!["command-injection".to_string()],
            VulnerabilityClass::Ssti => vec!["ssti".to_string()],
            VulnerabilityClass::Xxe => vec!["xxe".to_string()],
            VulnerabilityClass::Lfi | VulnerabilityClass::PathTraversal => vec!["lfi".to_string()],
            VulnerabilityClass::Csrf => vec!["csrf".to_string()],
            VulnerabilityClass::CorsMisconfiguration => vec!["cors".to_string()],
            VulnerabilityClass::OpenRedirect => vec!["open-redirect".to_string()],
            VulnerabilityClass::AuthenticationBypass => vec!["authentication-analysis".to_string()],
            VulnerabilityClass::MfaBypass => vec!["mfa-analysis".to_string()],
            VulnerabilityClass::Bfla => vec!["bfla".to_string()],
            VulnerabilityClass::RaceCondition => vec!["race-condition".to_string()],
            VulnerabilityClass::BusinessLogicFlaw => vec!["business-logic".to_string()],
            VulnerabilityClass::FileUpload => vec!["file-upload".to_string()],
            VulnerabilityClass::InsecureDeserialization => vec!["deserialization".to_string()],
            VulnerabilityClass::Clickjacking => vec!["clickjacking".to_string()],
            VulnerabilityClass::SessionFixation => vec!["session-analysis".to_string()],
            VulnerabilityClass::WeakPasswordPolicy => vec!["authentication-analysis".to_string()],
            VulnerabilityClass::PrivilegeEscalation => vec!["privilege-escalation".to_string()],
            VulnerabilityClass::CrossTenantAccess => vec!["cross-tenant-access".to_string()],
            VulnerabilityClass::PaymentManipulation => vec!["payment-manipulation".to_string()],
            VulnerabilityClass::WorkflowBypass => vec!["workflow-bypass".to_string()],
            VulnerabilityClass::SensitiveDataExposure
            | VulnerabilityClass::InformationDisclosure => {
                vec!["source-leak-analysis".to_string()]
            }
            VulnerabilityClass::Misconfiguration => vec!["technology-fingerprinting".to_string()],
            VulnerabilityClass::CryptographicWeakness => vec!["crypto-analysis".to_string()],
            VulnerabilityClass::InsecureStorage => vec!["secret-analysis".to_string()],
            VulnerabilityClass::LdapInjection => vec!["command-injection".to_string()],
            VulnerabilityClass::Custom(_) => vec!["source-code-audit".to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_selects_recon_for_small_surface() {
        let scope = LiveScope::new(
            super::super::scope::EngagementMode::BugBounty,
            "test-target".into(),
        );
        let route = SkillRouter::route(&scope, None);
        assert!(route.skills.contains(&"passive-recon".to_string()));
    }

    #[test]
    fn router_selects_ctf_skill_for_ctf_mode() {
        let scope = LiveScope::new(
            super::super::scope::EngagementMode::Ctf,
            "test-target".into(),
        );
        let route = SkillRouter::route(&scope, None);
        assert!(route.skills.contains(&"ctf".to_string()));
    }

    #[test]
    fn router_selects_xss_skill_for_xss_hypothesis() {
        let scope = LiveScope::new(
            super::super::scope::EngagementMode::BugBounty,
            "test-target".into(),
        );
        let h = Hypothesis::new(
            "h1".into(),
            "Reflected XSS".into(),
            VulnerabilityClass::Xss,
            "/search".into(),
            "input reflected".into(),
        );
        let route = SkillRouter::route(&scope, Some(&h));
        assert!(route.skills.contains(&"xss".to_string()));
    }

    #[test]
    fn router_selects_api_for_api_endpoints() {
        let mut scope = LiveScope::new(
            super::super::scope::EngagementMode::BugBounty,
            "test-target".into(),
        );
        scope.add_endpoint(super::super::scope::DiscoveredEndpoint {
            url: "https://test-target/api/users".into(),
            method: "GET".into(),
            path: "/api/users".into(),
            parameters: vec![],
            status_code: Some(200),
            content_type: None,
            requires_auth: true,
            discovered_by: "recon".into(),
            noise_level: super::super::noise::NoiseLevel::Moderate,
        });
        let route = SkillRouter::route(&scope, None);
        assert!(route.skills.contains(&"api-discovery".to_string()));
        assert!(
            route
                .skills
                .contains(&"authentication-analysis".to_string())
        );
    }

    #[test]
    fn skills_for_technology_covers_graphql() {
        let skills = SkillRouter::skills_for_technology("graphql");
        assert!(skills.contains(&"graphql".to_string()));
    }

    #[test]
    fn skills_for_vuln_class_maps_correctly() {
        let skills = SkillRouter::skills_for_vuln_class(&VulnerabilityClass::Ssrf);
        assert!(skills.contains(&"ssrf".to_string()));
    }
}
