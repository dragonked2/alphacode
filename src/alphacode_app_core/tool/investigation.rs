use super::{Registry, Tool, ToolContext, ToolOutput};
use crate::alphacode_app_core::engagement_runtime::{self as runtime, PlannedCall};
use crate::alphacode_security_core::investigation::{InvestigationEvidence, InvestigationStatus};
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
struct Input {
    action: String,
    #[serde(default)] in_scope:Vec<String>,
    #[serde(default)] out_of_scope:Vec<String>,
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
    tool_call_budget: Option<u32>,
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
        "Shared security engagement state. Start/status, propose and plan hypotheses, select next, execute a planned tool through normal scope/permission/budget gates, attach observed evidence, check proof, group roots, record fresh retests and export. No action grants authorization."
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","required":["action"],"properties":{
            "intent":super::intent_schema_property(),
            "action":{"type":"string","enum":["start","status","propose","plan","next","execute","attach","transition","verify","group_root","retest","export"]},
            "objective":{"type":"string"},"lead_id":{"type":"string"},"asset":{"type":"string"},
            "in_scope":{"type":"array","items":{"type":"string"},"description":"Initial boundaries copied from the user's authorization/current program policy. Records scope, never grants permission. An existing boundary cannot be expanded in this engagement."},
            "out_of_scope":{"type":"array","items":{"type":"string"}},
            "category":{"type":"string"},"claim":{"type":"string"},"expected":{"type":"string"},
            "steps":{"type":"array","items":{"type":"string"}},"tool":{"type":"string"},"input":{"type":"object"},
            "evidence_id":{"type":"string","description":"Actual tool-call receipt ID, returned by execute."},
            "supports":{"type":"boolean"},"reason":{"type":"string"},
            "status":{"type":"string","enum":["hypothesized","investigating","blocked","rejected","inconclusive","paused","supported"]},
            "proof":{"type":"object","description":"ProofObject referencing exchanges captured by this engagement; assertions alone cannot pass."},
            "root":{"type":"object","description":"DedupCandidate; finding_id must be a proof-checked lead ID."},
            "tool_call_budget":{"type":"integer","minimum":1,"description":"Optional initial tool-call ceiling. Cannot increase an existing ceiling."}
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
            if !params.in_scope.is_empty() {
                let policy = crate::alphacode_security_core::ScopePolicy::from_lists(&params.in_scope,&params.out_of_scope,true,&[],0,6);
                if let Some(existing) = super::scope::session_scope_policy(&ctx.session_id) {
                    ensure!(serde_json::to_value(&existing)? == serde_json::to_value(&policy)?,"installed scope differs; cannot replace it through investigation");
                }
                runtime::update(&ctx.session_id, |state| {
                    ensure!(state.policy.as_ref().is_none_or(|existing| serde_json::to_value(existing).ok() == serde_json::to_value(&policy).ok()),"engagement scope is already bound; start a distinct engagement for changed authorization");
                    state.policy=Some(policy.clone());
                    state.research.scope=params.in_scope.clone();
                    Ok(())
                })?;
                super::scope::set_session_scope(&ctx.session_id,Some(policy));
            }
            if let Some(limit) = params.tool_call_budget {
                ensure!(limit > 0, "tool_call_budget must be positive");
                runtime::update(&ctx.session_id, |state| {
                    ensure!(
                        state.ledger.tool_call_budget == 0
                            || limit <= state.ledger.tool_call_budget,
                        "cannot expand an existing engagement budget"
                    );
                    state.ledger.tool_call_budget = limit;
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
                ensure!(
                    !matches!(
                        params.tool.as_str(),
                        "investigation" | "batch" | "bg" | "swarm"
                    ) && !params.tool.is_empty(),
                    "plan needs one direct tool, not recursive/background orchestration"
                );
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
            "next" => runtime::update(&ctx.session_id, |state| {
                let lead = state
                    .research
                    .investigation
                    .next(&policy, u32::MAX, u32::MAX);
                Ok(
                    json!({"lead":lead,"planned_call":lead.and_then(|lead| state.plans.get(&lead.id)),"budget_exhausted":state.ledger.exhausted()}),
                )
            })?,
            "execute" => {
                let call_id = format!("{}:experiment", ctx.tool_call_id);
                let plan = runtime::update(&ctx.session_id, |state| {
                    ensure!(!state.ledger.exhausted(), "engagement budget exhausted");
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
