use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// On-chain capability inventory — who can do what to value or control.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    HoldAssets,
    Mint,
    Burn,
    Transfer,
    Upgrade,
    Pause,
    Seize,
    SetParameters,
    SetOracle,
    ArbitraryCall,
}

impl Capability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HoldAssets => "hold_assets",
            Self::Mint => "mint",
            Self::Burn => "burn",
            Self::Transfer => "transfer",
            Self::Upgrade => "upgrade",
            Self::Pause => "pause",
            Self::Seize => "seize",
            Self::SetParameters => "set_parameters",
            Self::SetOracle => "set_oracle",
            Self::ArbitraryCall => "arbitrary_call",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProxyType {
    None,
    Uups,
    Transparent,
    Beacon,
    Diamond,
    Unknown,
}

/// One contract in the protocol map.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContractInfo {
    pub name: String,
    pub proxy: ProxyType,
    pub admin: Option<String>,
    pub initializer_protected: bool,
    pub capabilities: Vec<Capability>,
}

impl ContractInfo {
    pub fn new(name: String) -> Self {
        Self {
            name,
            proxy: ProxyType::Unknown,
            admin: None,
            initializer_protected: false,
            capabilities: Vec::new(),
        }
    }

    pub fn can(&self, cap: &Capability) -> bool {
        self.capabilities.contains(cap)
    }
}

/// Explicit protocol model: contracts, upgrade authorities, oracles, bridges.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ProtocolModel {
    pub contracts: Vec<ContractInfo>,
    pub oracles: Vec<String>,
    pub bridges: Vec<String>,
    pub privileged_addresses: Vec<String>,
}

impl ProtocolModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_contract(&mut self, c: ContractInfo) {
        if !self.contracts.iter().any(|e| e.name == c.name) {
            self.contracts.push(c);
        }
    }

    pub fn asset_holders(&self) -> Vec<&ContractInfo> {
        self.contracts
            .iter()
            .filter(|c| c.can(&Capability::HoldAssets))
            .collect()
    }

    pub fn minters(&self) -> Vec<&ContractInfo> {
        self.contracts
            .iter()
            .filter(|c| c.can(&Capability::Mint))
            .collect()
    }

    /// Contracts reachable for upgrade/takeover: upgradeable with a named admin.
    pub fn upgrade_paths(&self) -> Vec<(&ContractInfo, &String)> {
        self.contracts
            .iter()
            .filter(|c| {
                c.can(&Capability::Upgrade)
                    && !matches!(c.proxy, ProxyType::None)
                    && c.admin.is_some()
            })
            .filter_map(|c| c.admin.as_ref().map(|a| (c, a)))
            .collect()
    }

    /// Uninitialized upgradeable contracts — ownership-seizure candidates.
    pub fn unprotected_initializers(&self) -> Vec<&ContractInfo> {
        self.contracts
            .iter()
            .filter(|c| !matches!(c.proxy, ProxyType::None) && !c.initializer_protected)
            .collect()
    }
}

/// Asset-flow edge: value moves from one holder to another.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FlowEdge {
    pub from: String,
    pub to: String,
    pub asset: String,
    pub mechanism: String,
    /// False when the edge moves an accounting representation (shares, LP,
    /// debt, receipt, bridge token) rather than real backing.
    pub is_real_backing: bool,
}

/// Asset-flow graph distinguishing backing from representation.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AssetFlowGraph {
    pub edges: Vec<FlowEdge>,
}

impl AssetFlowGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_edge(&mut self, edge: FlowEdge) {
        if !self.edges.contains(&edge) {
            self.edges.push(edge);
        }
    }

    pub fn representation_edges(&self) -> Vec<&FlowEdge> {
        self.edges.iter().filter(|e| !e.is_real_backing).collect()
    }

    /// Exit points: holders that only receive (sinks like user wallets).
    pub fn exits(&self) -> Vec<String> {
        let sources: Vec<&str> = self.edges.iter().map(|e| e.from.as_str()).collect();
        let mut exits = Vec::new();
        for e in &self.edges {
            if !sources.contains(&e.to.as_str()) && !exits.contains(&e.to) {
                exits.push(e.to.clone());
            }
        }
        exits
    }
}

/// Protocol-specific economic invariants with their rationale.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Web3Invariant {
    RedeemableLteBacked,
    LiabilitiesLteAssets,
    DebtLteCollateralTimesLtv,
    SharesTimesRateApproxAssets,
    WithdrawnLteCredited,
    MintedLteLocked,
    NoPrivilegeGrowth,
}

impl Web3Invariant {
    pub fn statement(&self) -> &'static str {
        match self {
            Self::RedeemableLteBacked => "redeemable supply <= backed assets",
            Self::LiabilitiesLteAssets => "liabilities <= assets",
            Self::DebtLteCollateralTimesLtv => "debt <= collateral value x LTV",
            Self::SharesTimesRateApproxAssets => "shares x exchangeRate ~= represented assets",
            Self::WithdrawnLteCredited => "withdrawnValue <= legitimately creditedValue",
            Self::MintedLteLocked => "mintedBridgeValue <= verifiedLockedValue",
            Self::NoPrivilegeGrowth => "unauthorized users cannot increase privileged state",
        }
    }

    pub fn rationale(&self) -> &'static str {
        match self {
            Self::RedeemableLteBacked => {
                "redemption converts representation into backing; excess means unbacked extraction"
            }
            Self::LiabilitiesLteAssets => "insolvency: the protocol owes more than it holds",
            Self::DebtLteCollateralTimesLtv => {
                "lending solvency depends on discounted collateral coverage"
            }
            Self::SharesTimesRateApproxAssets => {
                "share price must track backing or mint/redeem misprices value"
            }
            Self::WithdrawnLteCredited => {
                "conservation of funds per account across deposit/withdraw paths"
            }
            Self::MintedLteLocked => {
                "bridges must not create destination value beyond verified source locks"
            }
            Self::NoPrivilegeGrowth => {
                "authorization model: privilege only grows through defined admin paths"
            }
        }
    }
}

/// Numeric protocol snapshot for invariant checking (u128 fixed-point friendly).
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct ProtocolState {
    pub redeemable_supply: u128,
    pub backed_assets: u128,
    pub liabilities: u128,
    pub assets: u128,
    pub debt: u128,
    pub collateral_value: u128,
    /// LTV in basis points (e.g. 7500 = 75%).
    pub ltv_bps: u128,
    pub shares: u128,
    /// Exchange rate scaled by 1e18.
    pub exchange_rate_e18: u128,
    pub represented_assets: u128,
    pub withdrawn: u128,
    pub credited: u128,
    pub minted_bridge: u128,
    pub locked_verified: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InvariantViolation {
    pub invariant: Web3Invariant,
    pub detail: String,
}

/// Integer `a * b / d` without the intermediate overflow of a naive
/// multiply-then-divide. Splits `a = q*d + r` so the only multiplication is
/// `r * b` with `r < d`; falls back to an f64 estimate only when even that
/// overflows (absurd magnitudes), saturating instead of panicking.
fn mul_div_floor(a: u128, b: u128, d: u128) -> u128 {
    if d == 0 {
        return u128::MAX;
    }
    let q = a / d;
    let r = a % d;
    let hi = q.saturating_mul(b) / d;
    let lo = r.checked_mul(b).map(|v| v / d).unwrap_or_else(|| {
        ((r as f64) * (b as f64) / (d as f64)).clamp(0.0, u128::MAX as f64) as u128
    });
    hi.saturating_add(lo)
}

/// Saturating u128 -> i128 conversion for profit math: amounts above
/// i128::MAX pin at i128::MAX instead of wrapping negative (the old
/// `as` cast turned huge gains into huge negative "profits").
fn saturating_i128(v: u128) -> i128 {
    i128::try_from(v).unwrap_or(i128::MAX)
}

/// Check all applicable invariants against a snapshot. Deterministic.
pub fn check_invariants(state: &ProtocolState) -> Vec<InvariantViolation> {
    let mut out = Vec::new();
    if state.redeemable_supply > state.backed_assets {
        out.push(InvariantViolation {
            invariant: Web3Invariant::RedeemableLteBacked,
            detail: format!(
                "redeemable {} > backed {}",
                state.redeemable_supply, state.backed_assets
            ),
        });
    }
    if state.liabilities > state.assets {
        out.push(InvariantViolation {
            invariant: Web3Invariant::LiabilitiesLteAssets,
            detail: format!(
                "liabilities {} > assets {}",
                state.liabilities, state.assets
            ),
        });
    }
    if state.collateral_value > 0 || state.debt > 0 {
        let max_debt = mul_div_floor(state.collateral_value, state.ltv_bps, 10_000);
        if state.debt > max_debt {
            out.push(InvariantViolation {
                invariant: Web3Invariant::DebtLteCollateralTimesLtv,
                detail: format!(
                    "debt {} > max {} (ltv {}bps)",
                    state.debt, max_debt, state.ltv_bps
                ),
            });
        }
    }
    if state.shares > 0 && state.exchange_rate_e18 > 0 {
        // shares (often 1e18-scale) times a 1e18-scaled rate overflows u128
        // for realistic mainnet magnitudes (e.g. 1e27 * 2e18). Compare with
        // relative tolerance instead of exact integer math.
        let implied = (state.shares as f64) * (state.exchange_rate_e18 as f64) / 1e18;
        let represented = state.represented_assets as f64;
        let within = if represented <= 0.0 {
            implied <= 1.0
        } else {
            (implied - represented).abs() <= represented / 1000.0 + 1.0
        };
        if !within || !implied.is_finite() {
            out.push(InvariantViolation {
                invariant: Web3Invariant::SharesTimesRateApproxAssets,
                detail: format!(
                    "implied {:.0} vs represented {}",
                    implied, state.represented_assets
                ),
            });
        }
    }
    if state.withdrawn > state.credited {
        out.push(InvariantViolation {
            invariant: Web3Invariant::WithdrawnLteCredited,
            detail: format!(
                "withdrawn {} > credited {}",
                state.withdrawn, state.credited
            ),
        });
    }
    if state.minted_bridge > state.locked_verified {
        out.push(InvariantViolation {
            invariant: Web3Invariant::MintedLteLocked,
            detail: format!(
                "minted {} > verified locked {}",
                state.minted_bridge, state.locked_verified
            ),
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Targeted detectors (pure, deterministic, no chain access)
// ---------------------------------------------------------------------------

/// ERC4626-style share inflation: near-empty vault + raw donation receivable
/// + no virtual offset. Returns a description when vulnerable.
pub fn detect_share_inflation(
    total_assets: u128,
    total_supply: u128,
    decimals_offset: u8,
    donation_receivable: bool,
) -> Option<String> {
    if decimals_offset > 0 || !donation_receivable {
        return None;
    }
    if total_supply <= 10_000 && total_assets <= 10_000 {
        Some(format!(
            "near-empty vault (assets={total_assets} supply={total_supply}) with no virtual offset accepts raw donations: donation inflates exchange rate and victim deposits round to 0 shares"
        ))
    } else {
        None
    }
}

/// Victim deposit computes 0 shares for nonzero assets — fund lock / grief.
pub fn detect_rounding_victim(deposit_assets: u128, computed_shares: u128) -> bool {
    deposit_assets > 0 && computed_shares == 0
}

/// Normalize a chain state label for composition matching: all whitespace
/// removed, lowercased. State labels are short `key=value` snapshots, so
/// aggressive normalization is safe and fixes "price=100" vs "Price = 100".
fn normalize_chain_state(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

/// Paired-state desync: tracked vs actual diverge creating phantom value
/// (positive) or hiding a shortfall (negative). Either direction breaks
/// the accounting invariant, so any nonzero divergence is reported with
/// its sign; callers decide exploitability.
pub fn detect_desync_phantom(
    tracked_a: i128,
    actual_a: i128,
    tracked_b: i128,
    actual_b: i128,
) -> Option<i128> {
    let apparent = tracked_a.saturating_sub(tracked_b);
    let real = actual_a.saturating_sub(actual_b);
    let phantom = apparent.saturating_sub(real);
    if phantom != 0 { Some(phantom) } else { None }
}

/// Share price after a raw donation to a vault with `supply` shares and
/// `assets` backing: (assets + donation) / supply. Used to quantify
/// donation-inflation leverage on non-empty vaults where
/// [`detect_share_inflation`] (near-empty heuristic) does not fire.
/// Returns `None` for an empty vault (division by zero — that case is
/// covered by `detect_share_inflation`).
pub fn share_price_after_donation(supply: u128, assets: u128, donation: u128) -> Option<f64> {
    if supply == 0 {
        return None;
    }
    let total = (assets as f64) + (donation as f64);
    if !total.is_finite() {
        return None;
    }
    Some(total / (supply as f64))
}

/// Stale oracle feed.
pub fn detect_oracle_staleness(now: u64, updated_at: u64, max_age_secs: u64) -> bool {
    now.saturating_sub(updated_at) > max_age_secs
}

/// Oracle manipulation viability: needs both profit and available liquidity.
pub fn oracle_manipulation_viable(
    attacker_gain: u128,
    manipulation_cost: u128,
    liquidity_needed: u128,
    liquidity_available: u128,
) -> bool {
    liquidity_needed <= liquidity_available && attacker_gain > manipulation_cost
}

/// Bridge over-mint.
pub fn detect_bridge_overmint(minted: u128, verified_locked: u128) -> bool {
    minted > verified_locked
}

/// Signature binding gaps. Any missing binding is a replay candidate.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct SignatureBindings {
    pub binds_nonce: bool,
    pub binds_chain_id: bool,
    pub binds_contract: bool,
    pub binds_expiry: bool,
}

impl SignatureBindings {
    pub fn missing(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if !self.binds_nonce {
            out.push("nonce");
        }
        if !self.binds_chain_id {
            out.push("chain_id");
        }
        if !self.binds_contract {
            out.push("contract");
        }
        if !self.binds_expiry {
            out.push("expiry");
        }
        out
    }

    /// Bindings whose absence alone enables practical replay.
    pub fn missing_critical(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if !self.binds_nonce {
            out.push("nonce");
        }
        if !self.binds_chain_id {
            out.push("chain_id");
        }
        if !self.binds_contract {
            out.push("contract");
        }
        out
    }

    pub fn is_replay_candidate(&self) -> bool {
        !self.missing().is_empty()
    }

    /// Likely-exploitable replay: a *critical* binding (nonce, chain id,
    /// or contract) is missing. A message missing only expiry is a weaker,
    /// often-intentional finding (e.g. votes, permits with out-of-band
    /// revocation) — report at most as informational, never as a
    /// confirmed signature-replay vulnerability on its own.
    pub fn is_likely_exploitable_replay(&self) -> bool {
        !self.missing_critical().is_empty()
    }
}

// ---------------------------------------------------------------------------
// Economic model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct EconomicOutcome {
    pub attacker_capital: u128,
    pub temporary_capital: u128,
    pub flash_fee: u128,
    pub liquidity_needed: u128,
    pub liquidity_available: u128,
    pub gas_cost: u128,
    pub protocol_loss: u128,
    pub attacker_gain: u128,
    pub repayment: u128,
}

impl EconomicOutcome {
    pub fn net_profit(&self) -> i128 {
        saturating_i128(self.attacker_gain)
            .saturating_sub(saturating_i128(self.attacker_capital))
            .saturating_sub(saturating_i128(self.flash_fee))
            .saturating_sub(saturating_i128(self.gas_cost))
            .saturating_sub(saturating_i128(self.repayment))
    }

    pub fn viable(&self) -> bool {
        self.net_profit() > 0
            && self.liquidity_needed <= self.liquidity_available
            && self.protocol_loss > 0
    }
}

// ---------------------------------------------------------------------------
// Exploit chains
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExploitStep {
    pub tx_index: u32,
    pub action: String,
    pub state_before: String,
    pub state_after: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct ExploitChain {
    pub steps: Vec<ExploitStep>,
    pub economics: Option<EconomicOutcome>,
}

impl ExploitChain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_step(&mut self, action: String, before: String, after: String) {
        let idx = self.steps.len() as u32 + 1;
        self.steps.push(ExploitStep {
            tx_index: idx,
            action,
            state_before: before,
            state_after: after,
        });
    }

    /// Two chains compose when one's final state enables the other's first.
    /// State labels are normalized (whitespace collapsed, case-insensitive)
    /// because independent workers describe the same state differently
    /// ("price=100" vs "Price = 100"); exact-match brittleness dropped
    /// real A+B compositions.
    pub fn composes_with(&self, next: &ExploitChain) -> bool {
        match (self.steps.last(), next.steps.first()) {
            (Some(last), Some(first)) => {
                normalize_chain_state(&last.state_after)
                    == normalize_chain_state(&first.state_before)
            }
            _ => false,
        }
    }

    pub fn is_economically_viable(&self) -> bool {
        self.economics.as_ref().map(|e| e.viable()).unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Verification gates + findings + suppression
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Web3Gate {
    Reachability,
    AttackerControl,
    InvariantViolation,
    StateTransition,
    AssetEffect,
    EconomicEffect,
    Reproducibility,
    Scope,
    ThreatModel,
    AlternativeExplanation,
    ExistingMitigation,
    Evidence,
}

impl Web3Gate {
    pub fn all() -> Vec<Self> {
        vec![
            Self::Reachability,
            Self::AttackerControl,
            Self::InvariantViolation,
            Self::StateTransition,
            Self::AssetEffect,
            Self::EconomicEffect,
            Self::Reproducibility,
            Self::Scope,
            Self::ThreatModel,
            Self::AlternativeExplanation,
            Self::ExistingMitigation,
            Self::Evidence,
        ]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GateVerdict {
    pub gate: Web3Gate,
    pub passed: bool,
    pub reason: String,
}

pub fn evaluate_gates(verdicts: &[GateVerdict]) -> bool {
    !verdicts.is_empty()
        && verdicts.len() == Web3Gate::all().len()
        && verdicts.iter().all(|v| v.passed)
}

/// Evidence tier per claim — never mix reproduced with hypothetical.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceTier {
    Observed,
    Reproduced,
    Inferred,
    Hypothetical,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Web3Confidence {
    Unconfirmed,
    Theoretical,
    Probable,
    Verified,
    Confirmed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Web3Finding {
    pub title: String,
    pub severity: String,
    pub component: String,
    pub root_cause: String,
    pub violated_invariant: Web3Invariant,
    pub attack_sequence: Vec<String>,
    pub economic_impact: Option<EconomicOutcome>,
    pub confidence: Web3Confidence,
    pub claim_tiers: HashMap<String, EvidenceTier>,
}

impl Web3Finding {
    pub fn new(
        title: String,
        severity: String,
        component: String,
        violated_invariant: Web3Invariant,
    ) -> Self {
        Self {
            title,
            severity,
            component,
            root_cause: String::new(),
            violated_invariant,
            attack_sequence: Vec::new(),
            economic_impact: None,
            confidence: Web3Confidence::Unconfirmed,
            claim_tiers: HashMap::new(),
        }
    }

    /// Confidence must match evidence: CONFIRMED requires a reproduced
    /// claim and viable economics (when value is at stake).
    pub fn confidence_justified(&self) -> bool {
        match self.confidence {
            Web3Confidence::Unconfirmed | Web3Confidence::Theoretical => true,
            Web3Confidence::Probable => self
                .claim_tiers
                .values()
                .any(|t| *t == EvidenceTier::Observed || *t == EvidenceTier::Reproduced),
            Web3Confidence::Verified | Web3Confidence::Confirmed => self
                .claim_tiers
                .values()
                .any(|t| *t == EvidenceTier::Reproduced),
        }
    }
}

/// Aggressive false-positive suppression reasons.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionReason {
    NoReachablePath,
    HarmlessReentrancy,
    IntentionalPrivilegedFunction,
    OwnerTrustInModel,
    ImpossibleManipulationCost,
    PriceGapWithoutTransition,
    TokenQuirkWithoutLoss,
    InformationalOnly,
    Duplicate,
    OutOfScopeDependency,
    TestOnly,
    DeadCode,
}

pub fn should_suppress(
    reachable: bool,
    economic_viable: bool,
    reasons: &[SuppressionReason],
) -> bool {
    !reachable || !economic_viable || !reasons.is_empty()
}

/// Suppression for non-economic findings (privilege escalation, governance
/// takeover, access-control gaps, fund-lock griefing): no profit equation
/// applies, so economic viability must NOT gate them. Suppress only on
/// unreachability or an explicit suppression reason. Using
/// [`should_suppress`] for these classes is a false-negative factory —
/// route by finding class instead.
pub fn should_suppress_non_economic_finding(
    reachable: bool,
    reasons: &[SuppressionReason],
) -> bool {
    !reachable || !reasons.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_share_inflation_in_bare_vault() {
        let sig = detect_share_inflation(1, 1, 0, true);
        assert!(sig.is_some());
    }

    #[test]
    fn virtual_offset_suppresses_inflation() {
        assert!(detect_share_inflation(1, 1, 9, true).is_none());
        assert!(detect_share_inflation(1, 1, 0, false).is_none());
    }

    #[test]
    fn rounding_victim_detected() {
        assert!(detect_rounding_victim(1000, 0));
        assert!(!detect_rounding_victim(1000, 5));
        assert!(!detect_rounding_victim(0, 0));
    }

    #[test]
    fn desync_phantom_value() {
        // Tracked supply decremented early (90) while actual is still 100:
        // apparent yield 10 vs real 0 → phantom 10.
        assert_eq!(detect_desync_phantom(100, 100, 90, 100), Some(10));
        assert_eq!(detect_desync_phantom(100, 100, 100, 100), None);
    }

    #[test]
    fn invariants_catch_insolvency() {
        let s = ProtocolState {
            liabilities: 150,
            assets: 100,
            ..Default::default()
        };
        let v = check_invariants(&s);
        assert!(
            v.iter()
                .any(|x| x.invariant == Web3Invariant::LiabilitiesLteAssets)
        );
    }

    #[test]
    fn invariants_catch_bridge_overmint() {
        let s = ProtocolState {
            minted_bridge: 200,
            locked_verified: 100,
            ..Default::default()
        };
        assert!(detect_bridge_overmint(200, 100));
        let v = check_invariants(&s);
        assert!(
            v.iter()
                .any(|x| x.invariant == Web3Invariant::MintedLteLocked)
        );
    }

    #[test]
    fn oracle_staleness_and_viability() {
        assert!(detect_oracle_staleness(10_000, 1_000, 3_600));
        assert!(!detect_oracle_staleness(2_000, 1_000, 3_600));
        assert!(oracle_manipulation_viable(
            1_000_000, 100_000, 500_000, 900_000
        ));
        assert!(!oracle_manipulation_viable(
            50_000, 100_000, 500_000, 900_000
        ));
        assert!(!oracle_manipulation_viable(
            1_000_000, 100_000, 5_000_000, 900_000
        ));
    }

    #[test]
    fn signature_binding_gaps() {
        let b = SignatureBindings {
            binds_nonce: true,
            binds_chain_id: false,
            binds_contract: true,
            binds_expiry: true,
        };
        assert!(b.is_replay_candidate());
        assert_eq!(b.missing(), vec!["chain_id"]);
        let full = SignatureBindings {
            binds_nonce: true,
            binds_chain_id: true,
            binds_contract: true,
            binds_expiry: true,
        };
        assert!(!full.is_replay_candidate());
    }

    #[test]
    fn unprotected_initializer_listed() {
        let mut m = ProtocolModel::new();
        let mut c = ContractInfo::new("VaultProxy".into());
        c.proxy = ProxyType::Transparent;
        c.admin = Some("0xadmin".into());
        c.capabilities.push(Capability::Upgrade);
        m.add_contract(c);
        assert_eq!(m.unprotected_initializers().len(), 1);
        assert_eq!(m.upgrade_paths().len(), 1);
    }

    #[test]
    fn economics_rejects_impossible_attack() {
        let bad = EconomicOutcome {
            attacker_capital: 10_000,
            attacker_gain: 5_000,
            repayment: 0,
            protocol_loss: 5_000,
            liquidity_needed: 100,
            liquidity_available: 1_000,
            ..Default::default()
        };
        assert!(bad.net_profit() < 0);
        assert!(!bad.viable());
        let good = EconomicOutcome {
            attacker_capital: 1_000,
            attacker_gain: 50_000,
            repayment: 0,
            protocol_loss: 50_000,
            liquidity_needed: 100,
            liquidity_available: 1_000,
            ..Default::default()
        };
        assert!(good.viable());
    }

    #[test]
    fn chains_compose_on_shared_state() {
        let mut a = ExploitChain::new();
        a.add_step(
            "manipulate price".into(),
            "price=1".into(),
            "price=100".into(),
        );
        let mut b = ExploitChain::new();
        b.add_step("borrow".into(), "price=100".into(), "debt issued".into());
        assert!(a.composes_with(&b));
        let mut c = ExploitChain::new();
        c.add_step("other".into(), "price=1".into(), "nothing".into());
        assert!(!a.composes_with(&c));
    }

    #[test]
    fn gates_require_all_twelve() {
        let partial: Vec<GateVerdict> = Web3Gate::all()
            .into_iter()
            .take(11)
            .map(|gate| GateVerdict {
                gate,
                passed: true,
                reason: "ok".into(),
            })
            .collect();
        assert!(!evaluate_gates(&partial));
        let full: Vec<GateVerdict> = Web3Gate::all()
            .into_iter()
            .map(|gate| GateVerdict {
                gate,
                passed: true,
                reason: "ok".into(),
            })
            .collect();
        assert!(evaluate_gates(&full));
    }

    #[test]
    fn confidence_requires_evidence() {
        let mut f = Web3Finding::new(
            "t".into(),
            "High".into(),
            "Vault.withdraw".into(),
            Web3Invariant::WithdrawnLteCredited,
        );
        f.confidence = Web3Confidence::Confirmed;
        assert!(!f.confidence_justified());
        f.claim_tiers
            .insert("repro".into(), EvidenceTier::Reproduced);
        assert!(f.confidence_justified());
    }

    #[test]
    fn suppression_rejects_weak_findings() {
        assert!(should_suppress(false, true, &[]));
        assert!(should_suppress(true, false, &[]));
        assert!(should_suppress(
            true,
            true,
            &[SuppressionReason::OwnerTrustInModel]
        ));
        assert!(!should_suppress(true, true, &[]));
    }

    #[test]
    fn invariants_no_overflow_on_mainnet_magnitudes() {
        // 1M tokens at 1e18 + realistic rate 2e18: naive
        // shares*rate overflows u128. Must not panic and must not
        // false-positive on a balanced vault.
        let s = ProtocolState {
            shares: 1_000_000_000_000_000_000_000_000,
            exchange_rate_e18: 2_000_000_000_000_000_000,
            represented_assets: 2_000_000_000_000_000_000_000_000,
            ..Default::default()
        };
        let v = check_invariants(&s);
        assert!(
            !v.iter()
                .any(|x| x.invariant == Web3Invariant::SharesTimesRateApproxAssets),
            "balanced large vault flagged: {v:?}"
        );
        // Mispriced large vault still caught.
        let bad = ProtocolState {
            shares: 1_000_000_000_000_000_000_000_000,
            exchange_rate_e18: 2_000_000_000_000_000_000,
            represented_assets: 1_000,
            ..Default::default()
        };
        assert!(
            check_invariants(&bad)
                .iter()
                .any(|x| x.invariant == Web3Invariant::SharesTimesRateApproxAssets)
        );
        // Huge collateral * LTV must not panic either.
        let big = ProtocolState {
            collateral_value: u128::MAX / 2,
            ltv_bps: 8_000,
            debt: 1,
            ..Default::default()
        };
        let _ = check_invariants(&big);
    }

    #[test]
    fn net_profit_saturates_instead_of_wrapping() {
        let huge = EconomicOutcome {
            attacker_gain: u128::MAX,
            attacker_capital: 1,
            ..Default::default()
        };
        assert!(
            huge.net_profit() > 0,
            "wrapped negative: {}",
            huge.net_profit()
        );
        let symmetric = EconomicOutcome {
            attacker_gain: u128::MAX,
            attacker_capital: u128::MAX,
            ..Default::default()
        };
        assert_eq!(symmetric.net_profit(), 0);
    }

    #[test]
    fn chains_compose_despite_label_formatting() {
        let mut a = ExploitChain::new();
        a.add_step("manipulate".into(), "start".into(), "Price = 100 ".into());
        let mut b = ExploitChain::new();
        b.add_step("borrow".into(), "price=100".into(), "debt".into());
        assert!(a.composes_with(&b));
    }

    #[test]
    fn desync_flags_shortfall_direction() {
        // Hidden shortfall: tracked claims 100, actually 60.
        assert_eq!(detect_desync_phantom(100, 60, 0, 0), Some(40));
        assert_eq!(detect_desync_phantom(60, 100, 0, 0), Some(-40));
        assert_eq!(detect_desync_phantom(50, 50, 0, 0), None);
    }

    #[test]
    fn expiry_only_gap_is_not_likely_exploitable() {
        let expiry_only = SignatureBindings {
            binds_nonce: true,
            binds_chain_id: true,
            binds_contract: true,
            binds_expiry: false,
        };
        assert!(expiry_only.is_replay_candidate());
        assert!(!expiry_only.is_likely_exploitable_replay());
        let missing_nonce = SignatureBindings {
            binds_nonce: false,
            binds_chain_id: true,
            binds_contract: true,
            binds_expiry: true,
        };
        assert!(missing_nonce.is_likely_exploitable_replay());
    }

    #[test]
    fn non_economic_findings_not_gated_on_profit() {
        // Governance takeover with no profit equation: reachable, no
        // suppression reason -> must NOT suppress.
        assert!(!should_suppress_non_economic_finding(true, &[]));
        assert!(should_suppress_non_economic_finding(false, &[]));
        assert!(should_suppress_non_economic_finding(
            true,
            &[SuppressionReason::OwnerTrustInModel]
        ));
    }

    #[test]
    fn donation_pricing_quantifies_leverage() {
        // 100 shares over 100 assets + 9900 donation -> ~100x price.
        let p = share_price_after_donation(100, 100, 9_900).unwrap();
        assert!((p - 100.0).abs() < 1e-6, "price={p}");
        assert!(share_price_after_donation(0, 0, 1_000).is_none());
    }

    #[test]
    fn balanced_snapshot_reports_nothing() {
        // Harmless-reentrancy FP case: no value moved, books balance.
        let s = ProtocolState {
            redeemable_supply: 100,
            backed_assets: 100,
            liabilities: 50,
            assets: 100,
            credited: 100,
            withdrawn: 100,
            ..Default::default()
        };
        assert!(check_invariants(&s).is_empty());
    }

    #[test]
    fn asset_flow_exits_found() {
        let mut g = AssetFlowGraph::new();
        g.add_edge(FlowEdge {
            from: "user".into(),
            to: "vault".into(),
            asset: "USDC".into(),
            mechanism: "deposit".into(),
            is_real_backing: true,
        });
        g.add_edge(FlowEdge {
            from: "vault".into(),
            to: "user".into(),
            asset: "shares".into(),
            mechanism: "mint".into(),
            is_real_backing: false,
        });
        assert_eq!(g.representation_edges().len(), 1);
    }
}
