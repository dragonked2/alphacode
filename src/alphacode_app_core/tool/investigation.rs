use super::{Registry, Tool, ToolContext, ToolOutput};
use crate::alphacode_app_core::engagement_runtime::{
    self as runtime, EngagementRuntime, PlannedCall,
};
use crate::alphacode_security_core::investigation::{
    ExperimentAttempt, ExperimentPlan, InvestigationEvidence, InvestigationLead,
    InvestigationStatus, NegativeCause, ReassessmentDisposition,
};
use crate::alphacode_security_core::{DedupCandidate, ProofObject};
use anyhow::{Result, ensure};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::Digest;
use std::sync::{Arc, Weak};
use tokio::sync::RwLock;

pub struct InvestigationTool {
    tools: Weak<RwLock<std::collections::HashMap<String, Arc<dyn Tool>>>>,
    skills: Weak<RwLock<crate::skill::SkillRegistry>>,
    compaction: Weak<RwLock<crate::compaction::CompactionManager>>,
}

#[derive(Deserialize)]
struct Priority {
    information_gain: u32,
    impact_potential: u32,
    cost: u32,
    risk: u32,
}

fn dispatch_readiness(state: &EngagementRuntime, lead: &InvestigationLead) -> Result<()> {
    ensure!(
        state.plans.contains_key(&lead.id),
        "lead needs an executable plan"
    );
    ensure!(
        !lead.research.reassessment_pending(),
        "resolve pending reassessment before another dispatch"
    );
    ensure!(
        !state.in_flight.contains_key(&lead.id),
        "prior experiment has an indeterminate outcome"
    );
    if let Some(experiment) = &lead.research.plan {
        let suffix = format!(":{}:experiment", lead.id);
        let dispatched = state
            .calls
            .keys()
            .filter(|id| id.ends_with(&suffix))
            .count();
        ensure!(
            dispatched < usize::from(experiment.attempt_budget),
            "structured experiment dispatch budget exhausted; reassess or propose a distinct experiment"
        );
        ensure!(
            lead.research
                .reassessments
                .last()
                .is_none_or(|review| review.disposition == ReassessmentDisposition::Continue),
            "reassessment chose a different frontier or stop condition"
        );
    }
    Ok(())
}

#[derive(Deserialize)]
struct Input {
    action: String,
    #[serde(default)]
    in_scope: Vec<String>,
    #[serde(default)]
    out_of_scope: Vec<String>,
    #[serde(default)]
    objective: String,
    #[serde(default)]
    lead_id: String,
    #[serde(default)]
    asset: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    claim: String,
    #[serde(default)]
    expected: String,
    #[serde(default)]
    steps: Vec<String>,
    #[serde(default)]
    tool: String,
    #[serde(default)]
    input: Value,
    #[serde(default)]
    evidence_id: String,
    #[serde(default)]
    supports: bool,
    #[serde(default)]
    reason: String,
    status: Option<InvestigationStatus>,
    proof: Option<ProofObject>,
    root: Option<DedupCandidate>,
    experiment: Option<ExperimentPlan>,
    attempt: Option<ExperimentAttempt>,
    #[serde(default)]
    reviewed_attempts: std::collections::BTreeSet<String>,
    cause: Option<NegativeCause>,
    disposition: Option<ReassessmentDisposition>,
    max_cost: Option<u32>,
    max_risk: Option<u32>,
    priority: Option<Priority>,
}

impl InvestigationTool {
    pub fn for_registry(registry: &Registry) -> Self {
        Self {
            tools: Arc::downgrade(&registry.tools),
            skills: Arc::downgrade(&registry.skills),
            compaction: Arc::downgrade(&registry.compaction),
        }
    }

    fn registry(&self) -> Result<Registry> {
        Ok(Registry {
            tools: self
                .tools
                .upgrade()
                .ok_or_else(|| anyhow::anyhow!("registry closed"))?,
            skills: self
                .skills
                .upgrade()
                .ok_or_else(|| anyhow::anyhow!("skill registry closed"))?,
            compaction: self
                .compaction
                .upgrade()
                .ok_or_else(|| anyhow::anyhow!("context manager closed"))?,
        })
    }
}

#[async_trait]
impl Tool for InvestigationTool {
    fn name(&self) -> &str {
        "investigation"
    }
    fn description(&self) -> &str {
        "Shared security engagement state. Start/status, propose hypotheses, plan controlled experiments with attempt ceilings, select next, execute through normal scope/permission/budget gates, attach actual evidence, record attempts, reassess stalled work, check proof, group roots, retest and export. No action grants authorization."
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","required":["action"],"properties":{
            "intent":super::intent_schema_property(),
            "action":{"type":"string","enum":["start","status","propose","plan","prioritize","next","execute","attach","record_attempt","reassess","resolve_reassessment","transition","verify","group_root","retest","export"]},
            "objective":{"type":"string"},"lead_id":{"type":"string"},"asset":{"type":"string"},
            "in_scope":{"type":"array","items":{"type":"string"},"description":"Assets to record for this engagement. This never installs or widens scope: it is accepted only when the session already has an operator-installed engagement scope, and the installed policy alone decides what is authorized."},
            "out_of_scope":{"type":"array","items":{"type":"string"}},
            "category":{"type":"string"},"claim":{"type":"string"},"expected":{"type":"string"},
            "steps":{"type":"array","items":{"type":"string"}},"tool":{"type":"string"},"input":{"type":"object"},
            "evidence_id":{"type":"string","description":"Actual tool-call receipt ID, returned by execute."},
            "supports":{"type":"boolean"},"reason":{"type":"string"},
            "status":{"type":"string","enum":["hypothesized","investigating","blocked","rejected","inconclusive","paused","supported"]},
            "proof":{"type":"object","description":"ProofObject referencing exchanges captured by this engagement; assertions alone cannot pass."},
            "root":{"type":"object","description":"DedupCandidate; finding_id must be a proof-checked lead ID."},
            "max_cost":{"type":"integer","minimum":0,"description":"Optional next-selection utility-cost ceiling; does not alter authorization or engagement budgets."},
            "max_risk":{"type":"integer","minimum":0,"description":"Optional next-selection risk ceiling."},
            "priority":{"type":"object","description":"Action=prioritize: relative planning estimates only, never evidence confidence or authorization.","required":["information_gain","impact_potential","cost","risk"],"properties":{
                "information_gain":{"type":"integer","minimum":0,"maximum":10000},"impact_potential":{"type":"integer","minimum":0,"maximum":10000},
                "cost":{"type":"integer","minimum":0,"maximum":10000},"risk":{"type":"integer","minimum":0,"maximum":10000}
            }},
            "experiment":{"type":"object","description":"Optional structured plan on action=plan. Records controls and caps actual planned dispatches before execution; keep expected identical to expected_secure_behavior.","required":["expected_secure_behavior","positive_control","negative_control","success_oracle","reopen_condition","request","attempt_budget"],"properties":{
                "expected_secure_behavior":{"type":"string"},"positive_control":{"type":"string"},"negative_control":{"type":"string"},"success_oracle":{"type":"string"},"reopen_condition":{"type":"string"},
                "attempt_budget":{"type":"integer","minimum":1,"maximum":32},
                "request":{"type":"object","required":["method","workflow_state","dimension"],"properties":{
                    "method":{"type":"string"},"actor_label":{"type":["string","null"]},"object_owner_label":{"type":["string","null"]},"workflow_state":{"type":"string"},
                    "dimension":{"type":"string","enum":["identity","ownership","tenant","method","api_version","workflow_state","content_type","cache"]}
                }}
            }},
            "attempt":{"type":"object","description":"Action=record_attempt: interpretation of a completed experiment, not independent proof. Evidence IDs must already be attached from real completed receipts.","required":["id","request_fingerprint","outcome","observation","evidence_ids","at"],"properties":{
                "id":{"type":"string"},"request_fingerprint":{"type":"string","description":"SHA-256 of the redacted request capture; does not authenticate supplied interpretations."},
                "outcome":{"type":"string","enum":["supports","contradicts","inconclusive","blocked","paused"]},"observation":{"type":"string"},
                "evidence_ids":{"type":"array","items":{"type":"string"}},"negative_cause":{"type":["string","null"],"enum":["expected_behavior","control_not_reached","authentication_unavailable","rate_limited","tool_failure","cache_or_routing","other",null]},"at":{"type":"string"}
            }},
            "reviewed_attempts":{"type":"array","items":{"type":"string"}},
            "cause":{"type":"string","enum":["expected_behavior","control_not_reached","authentication_unavailable","rate_limited","tool_failure","cache_or_routing","other"]},
            "disposition":{"type":"string","enum":["continue","new_experiment","pivot","pause","block","close"]}
        }})
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        ensure!(
            input.to_string().len() <= 2 * 1024 * 1024,
            "investigation input too large"
        );
        let params: Input = serde_json::from_value(input)?;
        if params.action == "start" {
            ensure!(
                !params.objective.trim().is_empty(),
                "start requires objective"
            );
            runtime::ensure_started(&ctx.session_id, &params.objective)?;
            if !params.in_scope.is_empty() || !params.out_of_scope.is_empty() {
                // `in_scope` is model-supplied tool input. Installing a session
                // policy from it let the model authorize its own engagement:
                // `investigation start` with `in_scope: ["0.0.0.0/0"]` turned a
                // session with no authorization into a fail-closed gate that
                // permitted every address, and later prompts then reported an
                // installed scope to the model as fact.
                //
                // Scope is installed only by paths that carry the operator's own
                // instruction (`bugbounty_executor`, which derives it from the
                // target the user passed on the command line) or by restoring a
                // previously authorized engagement. Tool arguments may record
                // what the operator authorized, but can neither create nor
                // widen a policy, and `out_of_scope` is deliberately *not*
                // applied as a narrowing filter here: silently subtracting
                // rules would let the model shrink what the operator granted
                // and then report the result as the engagement's real boundary.
                // An authorization decision never originates in model output.
                let installed = super::scope::session_scope_policy(&ctx.session_id);
                ensure!(
                    installed.is_some(),
                    "no engagement scope is installed for this session, so `in_scope` cannot \
                     grant one. Scope comes from the operator (for example an explicit \
                     `alphacode bugbounty` target) or from a saved engagement. Continue with \
                     offline analysis, or ask the operator to start an authorized engagement."
                );
                // Recording the operator-visible asset list is still useful, and
                // is checked against the installed policy below rather than
                // trusted as policy.
                runtime::update(&ctx.session_id, |state| {
                    state.research.scope = params.in_scope.clone();
                    Ok(())
                })?;
            }
        }
        let policy = super::scope::session_scope_policy(&ctx.session_id).unwrap_or_default();
        let response = match params.action.as_str() {
            "start" | "status" | "export" => runtime::read(&ctx.session_id)?
                .ok_or_else(|| anyhow::anyhow!("start an investigation first"))?
                .summary(),
            "propose" => runtime::update(&ctx.session_id, |state| {
                ensure!(
                    state.research.investigation.leads().len() < 1000,
                    "lead limit reached"
                );
                ensure!(
                    matches!(
                        crate::alphacode_security_core::scope_gate::decide(&policy, &params.asset),
                        crate::alphacode_security_core::ScopeDecision::Allowed { .. }
                    ),
                    "asset requires an explicit installed scope rule"
                );
                state
                    .research
                    .investigation
                    .discover(&params.asset, "hypothesis");
                let id = state
                    .research
                    .investigation
                    .propose(&params.asset, &params.category, &params.claim)
                    .map_err(anyhow::Error::msg)?;
                Ok(json!({"lead_id":id,"status":"discovered"}))
            })?,
            "plan" => runtime::update(&ctx.session_id, |state| {
                if state
                    .research
                    .investigation
                    .leads()
                    .get(&params.lead_id)
                    .is_some_and(|lead| lead.research.plan.is_some())
                {
                    let suffix = format!(":{}:experiment", params.lead_id);
                    ensure!(
                        !state.calls.keys().any(|id| id.ends_with(&suffix)),
                        "a dispatched structured experiment is immutable; propose a distinct lead for a changed experiment"
                    );
                }
                ensure!(
                    !matches!(
                        params.tool.as_str(),
                        "investigation" | "batch" | "bg" | "swarm"
                    ) && !params.tool.is_empty(),
                    "plan needs one direct tool, not recursive/background orchestration"
                );
                if let Some(experiment) = &params.experiment {
                    ensure!(
                        params.expected == experiment.expected_secure_behavior,
                        "structured plan and expected secure behavior must agree"
                    );
                    state
                        .research
                        .investigation
                        .set_experiment_plan(&params.lead_id, experiment.clone())
                        .map_err(anyhow::Error::msg)?;
                }
                state
                    .research
                    .investigation
                    .plan(
                        &params.lead_id,
                        params.expected.clone(),
                        params.steps.clone(),
                        None,
                        Vec::new(),
                    )
                    .map_err(anyhow::Error::msg)?;
                if state.research.investigation.leads()[&params.lead_id].status
                    != InvestigationStatus::Hypothesized
                {
                    state
                        .research
                        .investigation
                        .transition(
                            &params.lead_id,
                            InvestigationStatus::Hypothesized,
                            "experiment planned",
                            &policy,
                        )
                        .map_err(anyhow::Error::msg)?;
                }
                state.plans.insert(
                    params.lead_id.clone(),
                    PlannedCall {
                        tool: params.tool.clone(),
                        input: params.input.clone(),
                    },
                );
                Ok(json!({"lead_id":params.lead_id,"status":"hypothesized"}))
            })?,
            "prioritize" => runtime::update(&ctx.session_id, |state| {
                let p = params
                    .priority
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("prioritize requires priority estimates"))?;
                ensure!(
                    [p.information_gain, p.impact_potential, p.cost, p.risk]
                        .iter()
                        .all(|value| *value <= 10_000),
                    "planning estimates exceed 10000 units"
                );
                state
                    .research
                    .investigation
                    .prioritize(
                        &params.lead_id,
                        p.information_gain,
                        p.impact_potential,
                        p.cost,
                        p.risk,
                    )
                    .map_err(anyhow::Error::msg)?;
                Ok(
                    json!({"lead_id":params.lead_id,"priority":"planning estimates updated; evidence unchanged"}),
                )
            })?,
            "next" => runtime::update(&ctx.session_id, |state| {
                let lead = state.research.investigation.next_matching(
                    &policy,
                    params.max_cost.unwrap_or(u32::MAX),
                    params.max_risk.unwrap_or(u32::MAX),
                    |lead| dispatch_readiness(state, lead).is_ok(),
                );
                let planning_lead = state.research.investigation.next_matching(
                    &policy,
                    params.max_cost.unwrap_or(u32::MAX),
                    params.max_risk.unwrap_or(u32::MAX),
                    |lead| !state.plans.contains_key(&lead.id),
                );
                Ok(
                    json!({"lead":lead,"planned_call":lead.and_then(|lead| state.plans.get(&lead.id)),
                        "planning_lead":planning_lead,"budget_exhausted":state.ledger.exhausted(),
                        "interpretation":"lead is dispatch-ready under planning limits; planning_lead needs a plan. A null lead does not establish completed coverage. Inspect status for reassessment, blocked, indeterminate or spent work."}),
                )
            })?,
            "execute" => {
                let call_suffix = format!(":{}:experiment", params.lead_id);
                let call_id = format!("{}{call_suffix}", ctx.tool_call_id);
                let plan = runtime::update(&ctx.session_id, |state| {
                    ensure!(
                        !state.in_flight.contains_key(&params.lead_id),
                        "prior experiment has an indeterminate outcome; reconcile it before another execution"
                    );
                    let lead = state
                        .research
                        .investigation
                        .leads()
                        .get(&params.lead_id)
                        .ok_or_else(|| anyhow::anyhow!("unknown lead"))?;
                    dispatch_readiness(state, lead)?;
                    ensure!(
                        matches!(
                            crate::alphacode_security_core::scope_gate::decide(
                                &policy,
                                &lead.asset
                            ),
                            crate::alphacode_security_core::ScopeDecision::Allowed { .. }
                        ),
                        "lead is outside the current installed scope"
                    );
                    let plan = state
                        .plans
                        .get(&params.lead_id)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("lead has no planned call"))?;
                    if state.research.investigation.leads()[&params.lead_id].status
                        != InvestigationStatus::Investigating
                    {
                        state
                            .research
                            .investigation
                            .transition(
                                &params.lead_id,
                                InvestigationStatus::Investigating,
                                "planned experiment dispatched through registry",
                                &policy,
                            )
                            .map_err(anyhow::Error::msg)?;
                    }
                    state
                        .in_flight
                        .insert(params.lead_id.clone(), call_id.clone());
                    Ok(plan)
                })?;
                let result = Box::pin(self.registry()?.execute(
                    &plan.tool,
                    plan.input,
                    ctx.for_subcall(call_id.clone()),
                ))
                .await;
                runtime::update(&ctx.session_id, |state| {
                    if state
                        .calls
                        .get(&call_id)
                        .is_none_or(|receipt| receipt.finished)
                    {
                        state.in_flight.remove(&params.lead_id);
                    }
                    Ok(())
                })?;
                match result {
                    Ok(output) => {
                        json!({"lead_id":params.lead_id,"evidence_id":call_id,"observation":output.output,"disposition":"uninterpreted; attach evidence only after assessing the controls"})
                    }
                    Err(error) => {
                        runtime::update(&ctx.session_id, |state| {
                            state.research.investigation.transition(&params.lead_id,InvestigationStatus::Blocked,"planned tool failed; inspect receipt and transcript before retry",&policy).map_err(anyhow::Error::msg)
                        })?;
                        return Err(error);
                    }
                }
            }
            "attach" => runtime::update(&ctx.session_id, |state| {
                let receipt = state
                    .calls
                    .get(&params.evidence_id)
                    .ok_or_else(|| anyhow::anyhow!("unknown tool receipt"))?;
                ensure!(
                    receipt.finished && !receipt.failed,
                    "failed or indeterminate work cannot support or disprove a finding"
                );
                let artifact_path =
                    runtime::path(&ctx.session_id)?
                        .with_extension("")
                        .join(format!(
                            "{:x}.receipt.json",
                            sha2::Sha256::digest(params.evidence_id.as_bytes())
                        ));
                crate::storage::write_json_fast(&artifact_path, receipt)?;
                let artifact_bytes = std::fs::read(&artifact_path)?;
                state
                    .research
                    .investigation
                    .record_evidence(InvestigationEvidence {
                        id: params.evidence_id.clone(),
                        source: receipt.tool.clone(),
                        artifact: artifact_path.display().to_string(),
                        sha256: format!("{:x}", sha2::Sha256::digest(&artifact_bytes)),
                        collected_at: receipt.completed_at.clone().ok_or_else(|| {
                            anyhow::anyhow!("receipt has no completion timestamp")
                        })?,
                        redacted: true,
                    })
                    .map_err(anyhow::Error::msg)?;
                state
                    .research
                    .investigation
                    .attach_evidence(&params.lead_id, &params.evidence_id, params.supports)
                    .map_err(anyhow::Error::msg)?;
                Ok(json!({"attached":params.evidence_id,"lead_id":params.lead_id}))
            })?,
            "record_attempt" => runtime::update(&ctx.session_id, |state| {
                let attempt = params
                    .attempt
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("record_attempt requires attempt"))?;
                for id in &attempt.evidence_ids {
                    let receipt = state
                        .calls
                        .get(id)
                        .ok_or_else(|| anyhow::anyhow!("attempt needs an actual tool receipt"))?;
                    ensure!(
                        receipt.finished && !receipt.failed,
                        "failed or indeterminate work cannot be evidential support"
                    );
                }
                state
                    .research
                    .investigation
                    .record_experiment_attempt(&params.lead_id, attempt)
                    .map_err(anyhow::Error::msg)?;
                Ok(
                    json!({"lead_id":params.lead_id,"attempt":"interpretation recorded; no finding promotion"}),
                )
            })?,
            "reassess" => runtime::update(&ctx.session_id, |state| {
                state
                    .research
                    .investigation
                    .request_reassessment(
                        &params.lead_id,
                        params.reviewed_attempts.clone(),
                        params
                            .cause
                            .ok_or_else(|| anyhow::anyhow!("reassess requires cause"))?,
                        params.reason.clone(),
                    )
                    .map_err(anyhow::Error::msg)?;
                Ok(
                    json!({"lead_id":params.lead_id,"reassessment":"pending; further dispatch paused"}),
                )
            })?,
            "resolve_reassessment" => runtime::update(&ctx.session_id, |state| {
                state
                    .research
                    .investigation
                    .resolve_reassessment(
                        &params.lead_id,
                        params.disposition.ok_or_else(|| {
                            anyhow::anyhow!("resolve_reassessment requires disposition")
                        })?,
                        params.reason.clone(),
                    )
                    .map_err(anyhow::Error::msg)?;
                Ok(json!({"lead_id":params.lead_id,"disposition":params.disposition}))
            })?,
            "transition" => runtime::update(&ctx.session_id, |state| {
                state
                    .research
                    .investigation
                    .transition(
                        &params.lead_id,
                        params
                            .status
                            .ok_or_else(|| anyhow::anyhow!("transition requires status"))?,
                        &params.reason,
                        &policy,
                    )
                    .map_err(anyhow::Error::msg)?;
                Ok(json!({"lead_id":params.lead_id,"status":params.status}))
            })?,
            "verify" => runtime::update(&ctx.session_id, |state| {
                let proof = params
                    .proof
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("verify requires proof"))?;
                state
                    .research
                    .investigation
                    .validate(&params.lead_id, proof, &state.exchanges, &policy)
                    .map_err(anyhow::Error::msg)?;
                state.proofs.insert(params.lead_id.clone(), proof.clone());
                Ok(
                    json!({"lead_id":params.lead_id,"proof_gate":"passed","impact":"requires independent review"}),
                )
            })?,
            "group_root" => runtime::update(&ctx.session_id, |state| {
                state.record_root(
                    params
                        .root
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("group_root requires root"))?,
                )?;
                Ok(json!({"groups":state.dedup.groups()}))
            })?,
            "retest" => runtime::update(&ctx.session_id, |state| {
                ensure!(
                    state.proofs.contains_key(&params.lead_id),
                    "retest requires a prior proof-checked experiment"
                );
                let receipt = state
                    .calls
                    .get(&params.evidence_id)
                    .ok_or_else(|| anyhow::anyhow!("retest requires fresh tool evidence"))?;
                ensure!(
                    receipt.finished && !receipt.failed && !params.reason.trim().is_empty(),
                    "retest requires a completed observation and deployment/control interpretation"
                );
                ensure!(
                    !state.research.investigation.leads()[&params.lead_id]
                        .supporting
                        .contains(&params.evidence_id),
                    "original proof evidence is not a fix retest"
                );
                state.retests.insert(
                    params.lead_id.clone(),
                    format!(
                        "Observed, not independently verified: {} ({})",
                        params.reason, params.evidence_id
                    ),
                );
                Ok(
                    json!({"lead_id":params.lead_id,"retest":"new observation recorded; no automatic fixed verdict"}),
                )
            })?,
            _ => anyhow::bail!("unknown investigation action"),
        };
        Ok(ToolOutput::new(serde_json::to_string(&response)?))
    }
}
