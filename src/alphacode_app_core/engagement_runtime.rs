//! One durable authority for the live agent's security investigations.
//! Tool execution remains behind Registry; this store never grants scope.
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::alphacode_security_core::{
    DedupCandidate, Deduplicator, HttpExchange, ProofObject, ResearchSession, ResourceLedger,
};

static TRANSACTION: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CallReceipt {
    pub tool: String,
    pub finished: bool,
    pub failed: bool,
    pub sha256: Option<String>,
    pub elapsed_ms: u64,
    #[serde(default)]
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlannedCall {
    pub tool: String,
    pub input: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngagementRuntime {
    pub version: u32,
    #[serde(default)]
    pub policy: Option<crate::alphacode_security_core::ScopePolicy>,
    pub research: ResearchSession,
    pub ledger: ResourceLedger,
    pub calls: BTreeMap<String, CallReceipt>,
    pub exchanges: Vec<HttpExchange>,
    pub plans: BTreeMap<String, PlannedCall>,
    #[serde(default)]
    pub in_flight: BTreeMap<String, String>,
    pub proofs: BTreeMap<String, ProofObject>,
    #[serde(skip)]
    pub dedup: Deduplicator,
    #[serde(default)]
    pub root_candidates: Vec<DedupCandidate>,
    /// Retests are new observations, never inferred from a patch or timeout.
    pub retests: BTreeMap<String, String>,
}

pub fn path(session: &str) -> Result<PathBuf> {
    Ok(crate::storage::alphacode_dir()?
        .join("engagements")
        .join(format!("{:x}.json", Sha256::digest(session.as_bytes()))))
}

pub fn read(session: &str) -> Result<Option<EngagementRuntime>> {
    let path = path(session)?;
    if !path.exists() {
        return Ok(None);
    }
    let mut state: EngagementRuntime = crate::storage::read_json(&path)?;
    ensure!(
        state.version == 1 && state.research.session_id == session,
        "engagement checkpoint version or session mismatch"
    );
    state
        .research
        .investigation
        .check_integrity()
        .map_err(anyhow::Error::msg)?;
    for candidate in &state.root_candidates {
        state.dedup.submit(candidate.clone());
    }
    Ok(Some(state))
}

pub fn update<T>(session: &str, f: impl FnOnce(&mut EngagementRuntime) -> Result<T>) -> Result<T> {
    let _guard = TRANSACTION
        .lock()
        .map_err(|_| anyhow::anyhow!("engagement transaction poisoned"))?;
    let mut state = read(session)?
        .ok_or_else(|| anyhow::anyhow!("no engagement; start investigation first"))?;
    let value = f(&mut state)?;
    state
        .research
        .investigation
        .check_integrity()
        .map_err(anyhow::Error::msg)?;
    crate::storage::write_json_fast(&path(session)?, &state)?;
    Ok(value)
}

pub fn ensure_started(session: &str, objective: &str) -> Result<()> {
    let _guard = TRANSACTION
        .lock()
        .map_err(|_| anyhow::anyhow!("engagement transaction poisoned"))?;
    if read(session)?.is_some() {
        return Ok(());
    }
    let state = EngagementRuntime {
        version: 1,
        policy: None,
        research: ResearchSession::new(session, objective),
        ledger: ResourceLedger::default(),
        calls: BTreeMap::new(),
        exchanges: Vec::new(),
        plans: BTreeMap::new(),
        in_flight: BTreeMap::new(),
        proofs: BTreeMap::new(),
        dedup: Deduplicator::new(),
        root_candidates: Vec::new(),
        retests: BTreeMap::new(),
    };
    crate::storage::write_json_fast(&path(session)?, &state)
}

/// Reserve before side effects. An interrupted receipt stays indeterminate on resume.
pub fn reserve(session: &str, call: &str, tool: &str) -> Result<bool> {
    if tool == "investigation" || read(session)?.is_none() {
        return Ok(false);
    }
    update(session, |state| {
        ensure!(
            !state.ledger.exhausted(),
            "engagement budget exhausted; checkpoint and report remaining work"
        );
        ensure!(
            !state.calls.contains_key(call),
            "tool call already recorded or indeterminate; reconcile before replay"
        );
        ensure!(
            state.calls.len() < 10_000,
            "engagement receipt limit reached; checkpoint before continuing"
        );
        state.ledger.tool_calls = state.ledger.tool_calls.saturating_add(1);
        state.calls.insert(
            call.into(),
            CallReceipt {
                tool: tool.into(),
                finished: false,
                failed: false,
                sha256: None,
                elapsed_ms: 0,
                completed_at: None,
            },
        );
        Ok(true)
    })
}

pub fn restore_scope(session: &str) -> Result<()> {
    if crate::tool::scope::session_scope_policy(session).is_none()
        && let Some(policy) = read(session)?.and_then(|state| state.policy)
    {
        crate::tool::scope::set_session_scope(session,Some(policy));
    }
    Ok(())
}

pub fn observe(
    session: &str,
    call: &str,
    tool: &str,
    input: &Value,
    output: &str,
    failed: bool,
    elapsed_ms: u64,
) -> Result<()> {
    update(session, |state| {
        let receipt = state
            .calls
            .get_mut(call)
            .ok_or_else(|| anyhow::anyhow!("missing reserved tool receipt"))?;
        ensure!(
            !receipt.finished && receipt.tool == tool,
            "receipt is already complete or tool mismatches"
        );
        receipt.finished = true;
        receipt.failed = failed;
        receipt.elapsed_ms = elapsed_ms;
        receipt.completed_at = Some(chrono::Utc::now().to_rfc3339());
        receipt.sha256 = Some(format!("{:x}", Sha256::digest(output.as_bytes())));
        if failed {
            state.research.record_tool_failure(
                tool,
                call,
                "tool execution failed; inspect the session transcript",
                chrono::Utc::now(),
            );
        } else if tool == "httpflow" {
            if let Some(exchange) = HttpExchange::parse_http_transcript(tool, input, output) {
                if !state
                    .exchanges
                    .iter()
                    .any(|prior| prior.matches_observed(&exchange))
                {
                    state.research.investigation.discover(&exchange.url, tool);
                    state.exchanges.push(exchange);
                }
            }
        }
        Ok(())
    })
}

impl EngagementRuntime {
    pub fn summary(&self) -> Value {
        json!({"session_id":self.research.session_id,"objective":self.research.objective,
            "assets":self.research.investigation.assets().len(),"leads":self.research.investigation.leads(),
            "tool_calls":self.ledger.tool_calls,"tool_call_budget":self.ledger.tool_call_budget,
            "indeterminate_calls":self.calls.iter().filter(|(_,r)| !r.finished).map(|(id,_)| id).collect::<Vec<_>>(),
            "observed_exchanges":self.exchanges.len(),"proof_checked_experiments":self.proofs.keys().collect::<Vec<_>>(),
            "root_cause_groups":self.dedup.groups(),"retests":self.retests,
            "interpretation":"Proof checks establish recorded differentials; independent policy, boundary and impact review remains required. Counts do not imply complete coverage."})
    }

    pub fn record_root(&mut self, candidate: DedupCandidate) -> Result<()> {
        ensure!(
            self.proofs.contains_key(&candidate.finding_id),
            "root grouping requires a proof-checked investigation ID"
        );
        ensure!(
            self.dedup.disposition_of(&candidate.finding_id).is_none(),
            "root grouping already recorded for this finding"
        );
        self.root_candidates.push(candidate.clone());
        self.dedup.submit(candidate);
        Ok(())
    }
}
