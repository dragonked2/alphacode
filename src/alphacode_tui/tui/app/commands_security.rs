use super::{App, DisplayMessage};
use crate::alphacode_security_core::investigation::InvestigationStatus;
use crate::engagement_runtime::EngagementRuntime;

fn command_rest<'a>(line: &'a str, command: &str) -> Option<&'a str> {
    if line == command {
        return Some("");
    }
    let rest = line.strip_prefix(command)?;
    let first = rest.chars().next()?;
    first.is_whitespace().then(|| &rest[first.len_utf8()..])
}

fn status_label(status: InvestigationStatus) -> &'static str {
    match status {
        InvestigationStatus::Discovered => "discovered",
        InvestigationStatus::Hypothesized => "hypothesized",
        InvestigationStatus::Investigating => "investigating",
        InvestigationStatus::Validated => "validated experiments",
        InvestigationStatus::Rejected => "rejected under tested conditions",
        InvestigationStatus::Blocked => "blocked",
        InvestigationStatus::Supported => "supported; not yet validated",
        InvestigationStatus::Inconclusive => "inconclusive",
        InvestigationStatus::Paused => "paused",
    }
}

fn display_code(value: &str) -> String {
    let safe: String = value
        .chars()
        .filter(|character| !character.is_control())
        .take(240)
        .map(|character| if character == '`' { '\'' } else { character })
        .collect();
    format!("`{safe}`")
}

fn budget(used: u64, limit: u64) -> String {
    if limit == 0 {
        format!("{used} used; no engagement-ledger cap configured")
    } else {
        format!("{used} / {limit}")
    }
}

fn format_engagement_dashboard(state: &EngagementRuntime) -> String {
    let investigation = &state.research.investigation;
    let leads = investigation.leads();
    let count = |status| leads.values().filter(|lead| lead.status == status).count();
    let mut output = String::from("# Security engagement status\n\n");

    match &state.policy {
        Some(policy) if policy.in_scope.is_empty() => {
            output.push_str("- **Scope:** no explicit allowlist is attached to this checkpoint; target actions still require an explicit matching authorization rule.\n");
        }
        Some(policy) => {
            output.push_str(&format!(
                "- **Scope:** {} explicit allow rule(s), {} exclusion(s); policy rate cap: {}; concurrency cap: {}.\n",
                policy.in_scope.len(),
                policy.out_of_scope.len(),
                if policy.max_rate_rps == 0 {
                    "not configured".to_string()
                } else {
                    format!("{} requests/second", policy.max_rate_rps)
                },
                if policy.max_concurrent == 0 {
                    "not configured".to_string()
                } else {
                    policy.max_concurrent.to_string()
                }
            ));
        }
        None => output.push_str(
            "- **Scope:** no persisted scope policy is attached to this checkpoint; authorization is not inferred.\n",
        ),
    }

    output.push_str(&format!(
        "- **Inventory:** {} discovered asset(s), {} lead(s), {} asset(s) without a validated investigation.\n",
        investigation.assets().len(),
        leads.len(),
        investigation.coverage_gaps().len()
    ));
    output.push_str("\n## Hypothesis states\n");
    for status in [
        InvestigationStatus::Discovered,
        InvestigationStatus::Hypothesized,
        InvestigationStatus::Investigating,
        InvestigationStatus::Supported,
        InvestigationStatus::Validated,
        InvestigationStatus::Rejected,
        InvestigationStatus::Blocked,
        InvestigationStatus::Inconclusive,
        InvestigationStatus::Paused,
    ] {
        output.push_str(&format!("- {}: {}\n", status_label(status), count(status)));
    }

    let unfinished = state
        .calls
        .values()
        .filter(|receipt| !receipt.finished)
        .count();
    let failed = state
        .calls
        .values()
        .filter(|receipt| receipt.failed)
        .count();
    output.push_str(&format!(
        "\n## Activity and limits\n- **Tool receipts:** {} total, {} failed, {} unfinished; {} experiment(s) marked in flight.\n- **Observed HTTP exchanges:** {}. **Proof-checked experiments:** {} (not confirmed findings). **Root-cause groups:** {}. **Retest observations:** {}.\n- **Budget ledger:** tool calls {}; network requests {}; tokens {}.\n",
        state.calls.len(),
        failed,
        unfinished,
        state.in_flight.len(),
        state.exchanges.len(),
        state.proofs.len(),
        state.dedup.groups().len(),
        state.retests.len(),
        budget(
            u64::from(state.ledger.tool_calls),
            u64::from(state.ledger.tool_call_budget)
        ),
        budget(
            u64::from(state.ledger.network_requests),
            u64::from(state.ledger.network_budget)
        ),
        budget(state.ledger.tokens_used, state.ledger.tokens_budget),
    ));

    let blocked = count(InvestigationStatus::Blocked);
    let inconclusive = count(InvestigationStatus::Inconclusive);
    let paused = count(InvestigationStatus::Paused);
    output.push_str(&format!(
        "- **Unresolved work:** {blocked} blocked, {inconclusive} inconclusive, {paused} paused, {unfinished} unfinished receipt(s). Reconcile these before retrying shared-state operations.\n"
    ));

    output.push_str("\n## Current frontier\n");
    if let Some(policy) = &state.policy {
        if let Some(lead) = investigation.next(policy, u32::MAX, u32::MAX) {
            let plan = if state.plans.contains_key(&lead.id) {
                "an executable call is planned; scope and runtime gates still apply"
            } else {
                "no executable call is planned yet"
            };
            output.push_str(&format!(
                "- **Highest-ranked in-scope lead (planning guidance, not authorization):** {} — {}; {}.\n- **Asset:** {}\n",
                display_code(&lead.id),
                status_label(lead.status),
                plan,
                display_code(&lead.asset)
            ));
        } else {
            output.push_str("- No open lead currently matches the persisted scope and selection limits. This does not establish complete coverage or a clean target.\n");
        }
    } else {
        output.push_str("- No lead is shown because this checkpoint has no persisted scope policy. The empty selection is not a coverage verdict.\n");
    }
    output.push_str(
        "\nValidated experiments are not automatically confirmed findings. Counts and empty queues do not imply complete coverage; review the investigation status and retained evidence before drawing conclusions.\n",
    );
    output
}

pub(super) fn handle_engagement_command(app: &mut App, trimmed: &str) -> bool {
    let Some(rest) = command_rest(trimmed, "/engagement") else {
        return false;
    };
    let argument = rest.trim();
    if !argument.is_empty() && !argument.eq_ignore_ascii_case("status") {
        app.push_display_message(DisplayMessage::system(
            "Usage: `/engagement [status]` — show the read-only security-engagement checkpoint.",
        ));
        return true;
    }

    let report = if app.is_remote {
        "# Security engagement status\n\nThis session is connected to a remote server, so its private engagement checkpoint is not available in this local TUI process. Ask the connected agent to run the `investigation` tool with action `status`.\n".to_string()
    } else {
        match crate::engagement_runtime::read(&app.session.id) {
            Ok(Some(state)) => format_engagement_dashboard(&state),
            Ok(None) => "# Security engagement status\n\nNo tracked security-engagement checkpoint exists for this conversation yet. This does not establish that no assessment is active elsewhere. Start or load the intended engagement before using this dashboard.\n".to_string(),
            Err(error) => format!(
                "# Security engagement status\n\nCould not read the persisted checkpoint: {}\n\nThe checkpoint was not changed. Reconcile its state before treating coverage or finding status as current.\n",
                display_code(&error.to_string())
            ),
        }
    };
    app.push_display_message(DisplayMessage::system(report));
    app.set_status_notice("Security engagement status");
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_security_core::{
        Deduplicator, ResearchSession, ResourceLedger, ScopePolicy,
    };

    fn runtime(policy: Option<ScopePolicy>) -> EngagementRuntime {
        EngagementRuntime {
            version: 1,
            policy,
            research: ResearchSession::new("test-session", "authorized fixture review"),
            ledger: ResourceLedger::with_budgets(0, 0, 10, 20, 0, 0),
            calls: Default::default(),
            exchanges: Vec::new(),
            plans: Default::default(),
            in_flight: Default::default(),
            proofs: Default::default(),
            dedup: Deduplicator::new(),
            root_candidates: Vec::new(),
            retests: Default::default(),
        }
    }

    #[test]
    fn dashboard_shows_scope_budgets_gaps_and_a_non_authoritative_next_lead() {
        let policy = ScopePolicy::from_lists(
            &["https://lab.example/api".into()],
            &["https://lab.example/admin".into()],
            true,
            &[],
            2,
            1,
        );
        let mut state = runtime(Some(policy));
        state
            .research
            .investigation
            .discover(
                "https://lab.example/api/objects?token=fixture-secret",
                "fixture",
            )
            .unwrap();
        state
            .research
            .investigation
            .propose(
                "https://lab.example/api/objects?token=fixture-secret",
                "authorization",
                "cross-account object access",
            )
            .unwrap();
        state.ledger.tool_calls = 2;
        state.ledger.network_requests = 3;

        let report = format_engagement_dashboard(&state);
        assert!(report.contains("1 explicit allow rule(s), 1 exclusion(s)"));
        assert!(report.contains("tool calls 2 / 10"));
        assert!(report.contains("network requests 3 / 20"));
        assert!(report.contains("1 asset(s) without a validated investigation"));
        assert!(report.contains("no executable call is planned yet"));
        assert!(report.contains("planning guidance, not authorization"));
        assert!(report.contains("do not imply complete coverage"));
        assert!(!report.contains("fixture-secret"));
    }

    #[test]
    fn dashboard_does_not_infer_scope_or_coverage_from_an_empty_selection() {
        let state = runtime(None);
        let report = format_engagement_dashboard(&state);
        assert!(report.contains("no persisted scope policy"));
        assert!(report.contains("empty selection is not a coverage verdict"));
    }

    #[test]
    fn command_parser_rejects_prefix_collisions_and_accepts_status() {
        assert_eq!(command_rest("/engagement", "/engagement"), Some(""));
        assert_eq!(
            command_rest("/engagement status", "/engagement"),
            Some("status")
        );
        assert_eq!(command_rest("/engagements", "/engagement"), None);
    }
}
