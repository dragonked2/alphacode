use serde::{Deserialize, Serialize};

use super::context::SecurityContext;
use super::finding::{Finding, Severity};
use super::skill_router::{SkillRoute, SkillRouter};

/// Actions the operator can take.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OperatorAction {
    /// Start a new engagement.
    StartEngagement { target: String, mode: String },
    /// Run reconnaissance on the target.
    RunRecon { techniques: Vec<String> },
    /// Analyze recon results and generate hypotheses.
    AnalyzeAndHypothesize,
    /// Dispatch a hypothesis to a specialist agent.
    DispatchHypothesis {
        hypothesis_id: String,
        agent_type: String,
    },
    /// Validate a candidate finding.
    ValidateFinding { finding_id: String },
    /// Check for chains between findings.
    AnalyzeChains,
    /// Generate final report.
    GenerateReport,
    /// Get current engagement summary.
    GetSummary,
}

/// The result of an operator decision.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperatorDecision {
    pub action: String,
    pub details: String,
    pub next_actions: Vec<String>,
    pub skills_to_load: Vec<String>,
    pub agents_to_spawn: Vec<AgentAssignment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentAssignment {
    pub agent_type: String,
    pub task: String,
    pub skills: Vec<String>,
    pub priority: u32,
}

/// The Security Operator coordinates the entire engagement.
///
/// It does not perform tasks directly. It:
/// 1. Understands the objective
/// 2. Inspects scope
/// 3. Creates a plan
/// 4. Decomposes the problem
/// 5. Selects specialized agents
/// 6. Assigns relevant skills
/// 7. Merges observations
/// 8. Identifies gaps
/// 9. Triggers validation
/// 10. Constructs chains when justified
/// 11. Generates evidence
/// 12. Produces the final result
pub struct SecurityOperator;

impl SecurityOperator {
    /// Process an operator action and return a decision.
    pub fn decide(context: &SecurityContext, action: OperatorAction) -> OperatorDecision {
        match action {
            OperatorAction::StartEngagement { target, mode } => {
                Self::handle_start_engagement(context, &target, &mode)
            }
            OperatorAction::RunRecon { techniques } => Self::handle_run_recon(context, &techniques),
            OperatorAction::AnalyzeAndHypothesize => Self::handle_analyze(context),
            OperatorAction::DispatchHypothesis {
                hypothesis_id,
                agent_type,
            } => Self::handle_dispatch(context, &hypothesis_id, &agent_type),
            OperatorAction::ValidateFinding { finding_id } => {
                Self::handle_validate(context, &finding_id)
            }
            OperatorAction::AnalyzeChains => Self::handle_chain_analysis(context),
            OperatorAction::GenerateReport => Self::handle_report(context),
            OperatorAction::GetSummary => Self::handle_summary(context),
        }
    }

    fn handle_start_engagement(
        _context: &SecurityContext,
        target: &str,
        mode: &str,
    ) -> OperatorDecision {
        let scope = super::scope::LiveScope::new(
            match mode {
                "bug_bounty" => super::scope::EngagementMode::BugBounty,
                "pentest" => super::scope::EngagementMode::Pentest,
                "ctf" => super::scope::EngagementMode::Ctf,
                "security_lab" => super::scope::EngagementMode::SecurityLab,
                "research" => super::scope::EngagementMode::Research,
                "audit" => super::scope::EngagementMode::Audit,
                _ => super::scope::EngagementMode::BugBounty,
            },
            target.to_string(),
        );
        let route = SkillRouter::route(&scope, None);

        OperatorDecision {
            action: "start_engagement".to_string(),
            details: format!("Engagement started for target: {target}"),
            next_actions: vec![
                "Run passive recon".to_string(),
                "Enumerate subdomains".to_string(),
                "Fingerprint technology".to_string(),
            ],
            skills_to_load: route.skills,
            agents_to_spawn: vec![
                AgentAssignment {
                    agent_type: "recon".to_string(),
                    task: format!("Passive reconnaissance on {target}"),
                    skills: vec!["passive-recon".to_string()],
                    priority: 1,
                },
                AgentAssignment {
                    agent_type: "recon".to_string(),
                    task: format!("Technology fingerprinting on {target}"),
                    skills: vec!["technology-fingerprinting".to_string()],
                    priority: 2,
                },
            ],
        }
    }

    fn handle_run_recon(context: &SecurityContext, techniques: &[String]) -> OperatorDecision {
        let route = SkillRouter::route(&context.scope, None);

        let mut next_actions = Vec::new();
        if context.scope.discovered_subdomains.is_empty() {
            next_actions.push("Enumerate subdomains".to_string());
        }
        if context.scope.discovered_hosts.is_empty() {
            next_actions.push("Resolve and scan hosts".to_string());
        }
        if context.scope.discovered_endpoints.is_empty() {
            next_actions.push("Discover endpoints via crawling/fuzzing".to_string());
        }
        if context.scope.technologies.is_empty() {
            next_actions.push("Fingerprint technology stack".to_string());
        }
        if next_actions.is_empty() {
            next_actions.push("Analyze findings and generate hypotheses".to_string());
        }

        OperatorDecision {
            action: "run_recon".to_string(),
            details: format!("Recon techniques requested: {:?}", techniques),
            next_actions,
            skills_to_load: route.skills.clone(),
            agents_to_spawn: vec![AgentAssignment {
                agent_type: "recon".to_string(),
                task: format!("Execute recon techniques: {:?}", techniques),
                skills: route.skills,
                priority: 1,
            }],
        }
    }

    fn handle_analyze(context: &SecurityContext) -> OperatorDecision {
        let surface = context.scope.surface_summary();
        let mut hypotheses = Vec::new();
        let mut agents = Vec::new();

        // Generate hypotheses based on discovered surface
        if !context.scope.discovered_endpoints.is_empty() {
            hypotheses.push("Test all endpoints for IDOR/BOLA".to_string());
            agents.push(AgentAssignment {
                agent_type: "analysis".to_string(),
                task: "Authorization boundary testing".to_string(),
                skills: vec!["idor".to_string(), "bfla".to_string()],
                priority: 1,
            });
        }

        let has_auth = context
            .scope
            .discovered_endpoints
            .iter()
            .any(|e| e.requires_auth);
        if has_auth {
            hypotheses.push("Test authentication bypass".to_string());
            agents.push(AgentAssignment {
                agent_type: "analysis".to_string(),
                task: "Authentication bypass testing".to_string(),
                skills: vec!["authentication-analysis".to_string()],
                priority: 2,
            });
        }

        if surface.technologies > 0 {
            hypotheses.push("Test tech-specific vulnerabilities".to_string());
            let mut tech_skills: Vec<String> = SkillRouter::route(&context.scope, None).skills;
            tech_skills.sort();
            tech_skills.dedup();
            if tech_skills.is_empty() {
                tech_skills.push("technology-fingerprinting".to_string());
            }
            agents.push(AgentAssignment {
                agent_type: "analysis".to_string(),
                task: "Technology-specific vulnerability analysis".to_string(),
                skills: tech_skills,
                priority: 3,
            });
        }

        let mut skills_to_load: Vec<String> =
            agents.iter().flat_map(|a| a.skills.clone()).collect();
        skills_to_load.sort();
        skills_to_load.dedup();

        OperatorDecision {
            action: "analyze".to_string(),
            details: format!(
                "Analysis of {} endpoints, {} hosts, {} technologies",
                surface.endpoints, surface.hosts, surface.technologies
            ),
            next_actions: hypotheses,
            skills_to_load,
            agents_to_spawn: agents,
        }
    }

    fn handle_dispatch(
        context: &SecurityContext,
        hypothesis_id: &str,
        agent_type: &str,
    ) -> OperatorDecision {
        let hypothesis = context.hypotheses.iter().find(|h| h.id == hypothesis_id);
        let route = match hypothesis {
            Some(h) => SkillRouter::route(&context.scope, Some(h)),
            None => SkillRoute {
                skills: vec![],
                rationale: "Hypothesis not found".to_string(),
                confidence: 0.0,
            },
        };

        OperatorDecision {
            action: "dispatch".to_string(),
            details: format!(
                "Dispatching hypothesis {} to {} agent",
                hypothesis_id, agent_type
            ),
            next_actions: vec!["Execute assigned skills".to_string()],
            skills_to_load: route.skills.clone(),
            agents_to_spawn: vec![AgentAssignment {
                agent_type: agent_type.to_string(),
                task: format!("Investigate hypothesis {hypothesis_id}"),
                skills: route.skills,
                priority: 1,
            }],
        }
    }

    fn handle_validate(context: &SecurityContext, finding_id: &str) -> OperatorDecision {
        let finding = context.findings.iter().find(|f| f.id == finding_id);
        let status = finding
            .map(|f| f.stage.as_str().to_string())
            .unwrap_or_else(|| "not found".to_string());

        OperatorDecision {
            action: "validate".to_string(),
            details: format!("Validating finding {finding_id} (current stage: {status})"),
            next_actions: vec![
                "Run validation gates".to_string(),
                "Adversarial review".to_string(),
            ],
            skills_to_load: vec!["validation".to_string()],
            agents_to_spawn: vec![AgentAssignment {
                agent_type: "validation".to_string(),
                task: format!("Validate finding {finding_id}"),
                skills: vec!["validation".to_string()],
                priority: 1,
            }],
        }
    }

    fn handle_chain_analysis(context: &SecurityContext) -> OperatorDecision {
        let chain_candidates: Vec<&Finding> = context
            .confirmed_findings()
            .into_iter()
            .filter(|f| f.severity >= Severity::Medium)
            .collect();

        OperatorDecision {
            action: "chain_analysis".to_string(),
            details: format!(
                "Analyzing {} confirmed findings for potential chains",
                chain_candidates.len()
            ),
            next_actions: if chain_candidates.len() >= 2 {
                vec!["Cross-reference findings for exploit chains".to_string()]
            } else {
                vec!["Need more confirmed findings for chain analysis".to_string()]
            },
            skills_to_load: vec!["chain-analysis".to_string()],
            agents_to_spawn: vec![AgentAssignment {
                agent_type: "chain_analyst".to_string(),
                task: "Analyze confirmed findings for exploit chains".to_string(),
                skills: vec!["chain-analysis".to_string()],
                priority: 1,
            }],
        }
    }

    fn handle_report(context: &SecurityContext) -> OperatorDecision {
        let summary = context.summary();
        OperatorDecision {
            action: "report".to_string(),
            details: format!(
                "Generating report: {} confirmed findings, {} chains",
                summary.confirmed_findings, summary.validated_chains
            ),
            next_actions: vec![
                "Compile findings into report".to_string(),
                "Generate reproduction steps".to_string(),
                "Add remediation guidance".to_string(),
            ],
            skills_to_load: vec!["vulnerability-report".to_string()],
            agents_to_spawn: vec![AgentAssignment {
                agent_type: "reporter".to_string(),
                task: "Generate professional vulnerability report".to_string(),
                skills: vec!["vulnerability-report".to_string()],
                priority: 1,
            }],
        }
    }

    fn handle_summary(context: &SecurityContext) -> OperatorDecision {
        let summary = context.summary();
        OperatorDecision {
            action: "summary".to_string(),
            details: serde_json::to_string_pretty(&summary).unwrap_or_default(),
            next_actions: vec![],
            skills_to_load: vec![],
            agents_to_spawn: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> SecurityContext {
        let scope = super::super::scope::LiveScope::new(
            super::super::scope::EngagementMode::BugBounty,
            "test-target".into(),
        );
        SecurityContext::new("eng-1".into(), "Test".into(), scope)
    }

    #[test]
    fn operator_start_engagement() {
        let ctx = test_context();
        let decision = SecurityOperator::decide(
            &ctx,
            OperatorAction::StartEngagement {
                target: "example.com".into(),
                mode: "bug_bounty".into(),
            },
        );
        assert_eq!(decision.action, "start_engagement");
        assert!(!decision.agents_to_spawn.is_empty());
    }

    #[test]
    fn operator_analyze_empty_surface() {
        let ctx = test_context();
        let decision = SecurityOperator::decide(&ctx, OperatorAction::AnalyzeAndHypothesize);
        assert_eq!(decision.action, "analyze");
        // No endpoints discovered yet, so no hypotheses
        assert!(decision.next_actions.is_empty());
    }

    #[test]
    fn operator_analyze_with_endpoints() {
        let mut ctx = test_context();
        ctx.scope
            .add_endpoint(super::super::scope::DiscoveredEndpoint {
                url: "https://target/api".into(),
                method: "GET".into(),
                path: "/api".into(),
                parameters: vec![],
                status_code: Some(200),
                content_type: None,
                requires_auth: true,
                discovered_by: "recon".into(),
                noise_level: super::super::noise::NoiseLevel::Moderate,
            });
        let decision = SecurityOperator::decide(&ctx, OperatorAction::AnalyzeAndHypothesize);
        assert!(!decision.next_actions.is_empty());
        assert!(!decision.agents_to_spawn.is_empty());
    }

    #[test]
    fn operator_chain_analysis_needs_findings() {
        let ctx = test_context();
        let decision = SecurityOperator::decide(&ctx, OperatorAction::AnalyzeChains);
        assert!(
            decision
                .next_actions
                .iter()
                .any(|a| a.contains("Need more"))
        );
    }
}
