//! Claim checker: verifies factual claims in the final report against session
//! tool history and filesystem state.
//!
//! "Never claim success without evidence" becomes a runtime check, not a hope.
//! The claim checker parses the final report for factual claims (files changed,
//! tests run, vulns confirmed) and mechanically verifies each one.
//!
//! # Fail-closed
//!
//! This module was originally a coding-task verifier and was being pointed at
//! security findings, where three of its five claim types were stubs that
//! returned `verified: true` unconditionally. "Found SQLi in `/search`" was
//! therefore marked verified whether or not a request had ever been sent.
//!
//! Verification here is **fail-closed**: a claim that cannot be checked
//! against real evidence is *not* verified, and the result says which evidence
//! was missing. There is no path through this file that returns `true` without
//! something having been observed.
//!
//! # Where the evidence comes from
//!
//! [`SessionEvidence`] is built from the session transcript — the
//! `ToolUse`/`ToolResult` pairs the agent actually produced. A claim is
//! verified only against exchanges in that transcript, so the agent cannot
//! assert a result it never observed.

use std::path::Path;

use crate::alphacode_message_types::ContentBlock;
use crate::alphacode_security_core::proof::{
    HttpExchange, ProofObject, ProofVerdict, StatusClass, describe_verdict,
};

/// Tools whose results can contain a real HTTP request/response exchange.
///
/// `httpflow` is Alphacode's own HTTP tool; the others are common recon tools
/// that emit a raw response when used with a manual probe. A tool is listed
/// here only if its output can be parsed back into an exchange — an
/// unparseable result yields no exchange and therefore no verification.
const HTTP_EVIDENCE_TOOLS: &[&str] = &["httpflow", "curl", "wget", "httpx", "burp", "request"];

/// A factual claim extracted from the final report.
#[derive(Debug, Clone)]
pub struct Claim {
    /// The raw claim text from the report.
    pub text: String,
    /// The type of claim.
    pub claim_type: ClaimType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ClaimType {
    /// "file X was modified/created"
    FileModified(String),
    /// "test Y passed"
    TestPassed(String),
    /// "command Z returned exit code 0"
    CommandSucceeded(String),
    /// "HTTP status N was observed"
    HttpStatus(u16),
    /// "vulnerability X was confirmed"
    VulnerabilityConfirmed(String),
    /// Generic claim that can't be mechanically verified.
    Generic,
}

/// Result of checking a single claim.
#[derive(Debug, Clone)]
pub struct ClaimCheckResult {
    pub claim: Claim,
    pub verified: bool,
    pub evidence: String,
    /// Structured proof backing the verdict, for report-quality claims.
    pub proof: Option<ProofVerdict>,
}

impl ClaimCheckResult {
    fn verified(claim: &Claim, evidence: String) -> Self {
        Self {
            claim: claim.clone(),
            verified: true,
            evidence,
            proof: None,
        }
    }

    fn unverified(claim: &Claim, evidence: impl Into<String>) -> Self {
        Self {
            claim: claim.clone(),
            verified: false,
            evidence: evidence.into(),
            proof: None,
        }
    }

    fn with_proof(
        claim: &Claim,
        verified: bool,
        evidence: String,
        proof: Option<ProofVerdict>,
    ) -> Self {
        Self {
            claim: claim.clone(),
            verified,
            evidence,
            proof,
        }
    }
}

/// What the session actually did, as reconstructed from its transcript, plus
/// the proof objects the report itself declares.
///
/// This is the only source of truth a security claim is checked against.
/// Observed exchanges come from the transcript; declared proofs come from the
/// report. A claim verifies only when the two agree.
#[derive(Debug, Clone, Default)]
pub struct SessionEvidence {
    /// Real HTTP exchanges observed, in transcript order.
    pub exchanges: Vec<HttpExchange>,
    /// Tool calls made, as `tool_name` -> number of calls.
    pub tool_calls: Vec<(String, usize)>,
    /// Commands whose output shows a successful exit.
    pub succeeded_commands: Vec<String>,
    /// Tests whose output shows a pass.
    pub passed_tests: Vec<String>,
    /// Structured proofs declared by the report, matched to claims below.
    pub proofs: Vec<ProofObject>,
}

impl SessionEvidence {
    /// Reconstruct observed evidence from the session transcript.
    ///
    /// `ToolUse` blocks carry the arguments, `ToolResult` blocks carry the
    /// output; the two are joined by `tool_use_id`. Anything that cannot be
    /// joined, or whose output does not parse, contributes nothing — an
    /// incomplete history verifies less, never more.
    pub fn from_messages(messages: &[crate::alphacode_session_types::StoredMessage]) -> Self {
        use std::collections::HashMap;

        let mut inputs: HashMap<String, (String, serde_json::Value)> = HashMap::new();
        for message in messages {
            for block in &message.content {
                if let ContentBlock::ToolUse {
                    id, name, input, ..
                } = block
                {
                    inputs.insert(id.clone(), (name.clone(), input.clone()));
                }
            }
        }

        let mut evidence = SessionEvidence::default();
        let mut call_counts: HashMap<String, usize> = HashMap::new();

        for message in messages {
            for block in &message.content {
                let ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } = block
                else {
                    continue;
                };
                let Some((tool_name, input)) = inputs.get(tool_use_id) else {
                    continue;
                };
                *call_counts.entry(tool_name.clone()).or_insert(0) += 1;

                if !is_error.unwrap_or(false)
                    && let Some(cmd) = input.get("command").and_then(|v| v.as_str())
                    && indicates_success(content)
                {
                    evidence.succeeded_commands.push(cmd.to_string());
                }

                for test in passed_tests(content) {
                    evidence.passed_tests.push(test);
                }

                if HTTP_EVIDENCE_TOOLS
                    .iter()
                    .any(|t| tool_name.eq_ignore_ascii_case(t))
                    && let Some(exchange) =
                        HttpExchange::parse_http_transcript(tool_name, input, content)
                {
                    evidence.exchanges.push(exchange);
                }
            }
        }

        evidence.tool_calls = call_counts.into_iter().collect();
        evidence.tool_calls.sort();
        evidence
    }

    /// Was this status code actually observed on this URL?
    ///
    /// Matching on URL as well as status stops "we saw a 403 somewhere" from
    /// standing in for "we saw a 403 *there*".
    pub fn observed_status(&self, url_hint: Option<&str>, status: u16) -> bool {
        self.exchanges.iter().any(|exchange| {
            exchange.status == status
                && url_hint.is_none_or(|hint| {
                    let hint = hint.trim().to_ascii_lowercase();
                    !hint.is_empty() && exchange.url.to_ascii_lowercase().contains(&hint)
                })
        })
    }
}

/// Does this tool output indicate a successful exit?
///
/// Deliberately conservative: only an explicit success marker counts. Empty
/// output from a command is *not* success, because "the command printed
/// nothing" and "the command failed silently" are indistinguishable here.
fn indicates_success(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    let failed = [
        "exit code 1",
        "exit code: 1",
        "exit status 1",
        "error:",
        "command failed",
        "traceback (most recent call last)",
        "no such file or directory",
        "permission denied",
    ];
    if failed.iter().any(|marker| lower.contains(marker)) {
        return false;
    }
    [
        "exit code 0",
        "exit code: 0",
        "exit status 0",
        "process exited with 0",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// Extract test names whose output shows them passing.
fn passed_tests(output: &str) -> Vec<String> {
    let mut passed = Vec::new();
    for line in output.lines() {
        let lower = line.to_ascii_lowercase();
        let ok = lower.contains("test result: ok") || lower.contains("... ok");
        if !ok {
            continue;
        }
        if let Some(start) = line.find("test ") {
            let rest = &line[start + 5..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                .collect();
            if !name.is_empty() {
                passed.push(name);
            }
        }
    }
    passed
}

/// Parse claims from the final report text.
pub fn extract_claims(report: &str) -> Vec<Claim> {
    let mut claims = Vec::new();

    for line in report.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // File modification claims
        if let Some(file_path) = extract_file_claim(line) {
            claims.push(Claim {
                text: line.to_string(),
                claim_type: ClaimType::FileModified(file_path),
            });
            continue;
        }

        // Test pass claims
        if let Some(test_name) = extract_test_claim(line) {
            claims.push(Claim {
                text: line.to_string(),
                claim_type: ClaimType::TestPassed(test_name),
            });
            continue;
        }

        // Command success claims
        if let Some(cmd) = extract_command_claim(line) {
            claims.push(Claim {
                text: line.to_string(),
                claim_type: ClaimType::CommandSucceeded(cmd),
            });
            continue;
        }

        // HTTP status claims
        if let Some(status) = extract_http_status_claim(line) {
            claims.push(Claim {
                text: line.to_string(),
                claim_type: ClaimType::HttpStatus(status),
            });
            continue;
        }

        // Vulnerability claims
        if let Some(vuln) = extract_vuln_claim(line) {
            claims.push(Claim {
                text: line.to_string(),
                claim_type: ClaimType::VulnerabilityConfirmed(vuln),
            });
            continue;
        }
    }

    claims
}

/// Check a claim against the filesystem and the session's observed evidence.
///
/// Fail-closed by construction: every branch either produces a concrete
/// observation or an explicit "unverified" reason. There is no branch that
/// returns `verified: true` as a default.
pub fn check_claim(
    claim: &Claim,
    working_dir: &Path,
    evidence: &SessionEvidence,
) -> ClaimCheckResult {
    match &claim.claim_type {
        ClaimType::FileModified(path) => {
            let full_path = working_dir.join(path);
            if full_path.exists() {
                ClaimCheckResult::verified(claim, format!("File '{}' exists", path))
            } else {
                ClaimCheckResult::unverified(claim, format!("File '{}' does NOT exist", path))
            }
        }

        ClaimType::TestPassed(test_name) => {
            // Previously hardcoded to `verified: true`. A test only counts if
            // its output was actually observed in this session.
            let observed = evidence
                .passed_tests
                .iter()
                .any(|t| t == test_name || t.ends_with(test_name));
            if observed {
                ClaimCheckResult::verified(
                    claim,
                    format!("Test '{}' reported passing in session output", test_name),
                )
            } else {
                ClaimCheckResult::unverified(
                    claim,
                    format!(
                        "Test '{}' has no passing output in the session transcript; \
                         a claim of a passing test is not evidence of one",
                        test_name
                    ),
                )
            }
        }

        ClaimType::CommandSucceeded(cmd) => {
            // Previously hardcoded to `verified: true`.
            let observed = evidence
                .succeeded_commands
                .iter()
                .any(|c| c == cmd || c.contains(cmd.as_str()) || cmd.contains(c.as_str()));
            if observed {
                ClaimCheckResult::verified(
                    claim,
                    format!("Command '{}' reported a successful exit", cmd),
                )
            } else {
                ClaimCheckResult::unverified(
                    claim,
                    format!(
                        "Command '{}' has no successful exit recorded in the session transcript",
                        cmd
                    ),
                )
            }
        }

        ClaimType::HttpStatus(status) => check_http_status_claim(claim, *status, evidence),

        ClaimType::VulnerabilityConfirmed(vuln) => check_vulnerability_claim(claim, vuln, evidence),

        ClaimType::Generic => ClaimCheckResult::unverified(
            claim,
            // Previously `verified: true`. A claim that cannot be mechanically
            // checked is not evidence of anything; counting it as verified is
            // how unbacked assertions reach a report.
            "Generic claim — not mechanically verifiable, so it is not counted as verified",
        ),
    }
}

/// Step 2: HTTP status semantics.
///
/// The old rule was `status >= 200 && status < 300`, which is backwards for
/// recon: it certified the least interesting possible result (a bare 200) and
/// rejected `403`/`500`/`302`, which are frequently the finding itself.
///
/// The replacement distinguishes three things that were previously conflated:
/// *was the status observed* (an evidence question), *is it informative*
/// (a signal question), and *is it a finding* (never — a status alone is
/// never proof of a vulnerability).
fn check_http_status_claim(
    claim: &Claim,
    status: u16,
    evidence: &SessionEvidence,
) -> ClaimCheckResult {
    let url_hint = extract_url(claim.text.as_str());
    // Path sensitivity is already resolved by `classify`: a 2xx on a sensitive
    // path comes back as `SensitiveSuccess`, never as `RoutineSuccess`.
    let class = StatusClass::classify(status, url_hint.as_deref().unwrap_or(""));

    if !evidence.observed_status(url_hint.as_deref(), status) {
        return ClaimCheckResult::unverified(
            claim,
            format!(
                "HTTP {} was never observed in the session transcript{}",
                status,
                url_hint.map(|u| format!(" for {u}")).unwrap_or_default()
            ),
        );
    }

    if !class.is_informative() {
        return ClaimCheckResult::unverified(
            claim,
            format!(
                "HTTP {} was observed, but a bare success is not a finding — \
                 it shows the host is reachable and nothing more (classified {})",
                status,
                class.as_str()
            ),
        );
    }

    // Observed and interesting. That is a real signal, but a status code by
    // itself is still not proof of a vulnerability, so it is reported as
    // informative rather than verified-as-a-finding.
    ClaimCheckResult::with_proof(
        claim,
        true,
        format!(
            "HTTP {} observed in session and classified as informative ({}){}",
            status,
            class.as_str(),
            url_hint.map(|u| format!(" on {u}")).unwrap_or_default()
        ),
        None,
    )
}

/// Step 1: a vulnerability claim verifies only against a structured proof.
///
/// The proof objects declared anywhere in the report are matched to this claim,
/// and the matched proof is then cross-checked against the exchanges the
/// session actually observed. A claim with no matching proof is unverified,
/// with the missing requirements named.
fn check_vulnerability_claim(
    claim: &Claim,
    vuln: &str,
    evidence: &SessionEvidence,
) -> ClaimCheckResult {
    let Some(proof) = match_proof_for_claim(&claim.text, &evidence.proofs) else {
        return ClaimCheckResult::unverified(
            claim,
            format!(
                "Vulnerability '{vuln}' has no structured proof. A claim of a \
                 confirmed vulnerability requires the request sent, the response \
                 received, reproduction steps, observed-vs-expected, and a \
                 baseline comparison."
            ),
        );
    };

    let verdict = proof.verify_against(&evidence.exchanges);
    let mut result = ClaimCheckResult::with_proof(
        claim,
        verdict.verified,
        String::new(),
        Some(verdict.clone()),
    );

    if verdict.verified {
        result.evidence = format!("Vulnerability '{vuln}' verified against observed evidence");
    } else {
        let reasons: Vec<&str> = verdict.defects.iter().map(|d| d.as_str()).collect();
        result.evidence = format!(
            "Vulnerability '{vuln}' is UNVERIFIED — {}",
            reasons.join("; ")
        );
    }
    result
}

/// Associate a proof with the claim that asserts it.
///
/// Claims are extracted one line at a time but a proof block spans several, so
/// the two cannot be matched positionally. Matching is by content instead, in
/// descending order of confidence:
///
/// 1. the claim names the proof's `finding_id`;
/// 2. the claim's URL matches the proof's test exchange URL;
/// 3. the report declares exactly one proof, so there is no ambiguity.
///
/// If none of those hold, the claim has no proof. Returning `None` is what
/// makes the claim unverified — a proof that exists but belongs to a different
/// finding must not be borrowed to justify this one.
fn match_proof_for_claim<'a>(
    claim_text: &str,
    proofs: &'a [ProofObject],
) -> Option<&'a ProofObject> {
    let lower = claim_text.to_ascii_lowercase();

    proofs
        .iter()
        .find(|p| !p.finding_id.is_empty() && lower.contains(&p.finding_id.to_ascii_lowercase()))
        .or_else(|| {
            let claim_url = extract_url(claim_text)?;
            proofs.iter().find(|p| {
                p.test_exchange
                    .as_ref()
                    .is_some_and(|e| e.url.eq_ignore_ascii_case(&claim_url))
            })
        })
        .or_else(|| (proofs.len() == 1).then(|| &proofs[0]))
}

/// Parse every structured proof object out of report text.
///
/// Accepts fenced blocks tagged `proof` whose body is JSON, and `PROOF:` inline
/// blocks. A report with none yields an empty vector — which is the common case,
/// and is exactly the case that must not verify.
pub fn extract_proofs(report: &str) -> Vec<ProofObject> {
    let mut proofs = Vec::new();
    let mut fence_open = false;
    let mut buffer = String::new();

    for line in report.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            let tag = trimmed.trim_start_matches('`').trim().to_ascii_lowercase();
            if !fence_open {
                if tag.starts_with("proof") {
                    fence_open = true;
                    buffer.clear();
                }
                continue;
            }
            // The closing fence carries no language tag.
            if tag.is_empty() {
                if let Ok(proof) = serde_json::from_str::<ProofObject>(buffer.trim()) {
                    proofs.push(proof);
                }
                fence_open = false;
                buffer.clear();
            }
            continue;
        }
        if fence_open {
            buffer.push_str(line);
            buffer.push('\n');
        }
    }

    // Inline `PROOF: {...}` blocks, which may appear more than once.
    for line in report.lines() {
        if let Some(rest) = line.trim().strip_prefix("PROOF:")
            && let Ok(proof) = serde_json::from_str::<ProofObject>(rest.trim())
            && !proofs.iter().any(|p| p == &proof)
        {
            proofs.push(proof);
        }
    }

    proofs
}

/// Pull a URL out of a claim line, for status matching.
fn extract_url(line: &str) -> Option<String> {
    let start = line.find("http://").or_else(|| line.find("https://"))?;
    let tail = &line[start..];
    let end = tail
        .find(|c: char| c.is_whitespace() || c == '`' || c == '"' || c == ')' || c == ',')
        .unwrap_or(tail.len());
    Some(tail[..end].trim_end_matches(['.', ';', ':']).to_string())
}

/// Check all claims in a report against observed session evidence.
///
/// The report's own proof blocks are parsed once here and attached to the
/// evidence context, because a proof spans several lines while a claim is a
/// single one.
pub fn check_all_claims(
    report: &str,
    working_dir: &Path,
    evidence: &SessionEvidence,
) -> Vec<ClaimCheckResult> {
    let mut context = evidence.clone();
    context.proofs = extract_proofs(report);
    extract_claims(report)
        .into_iter()
        .map(|claim| check_claim(&claim, working_dir, &context))
        .collect()
}

/// Generate a verification summary.
pub fn verification_summary(results: &[ClaimCheckResult]) -> String {
    let total = results.len();
    let verified = results.iter().filter(|r| r.verified).count();
    let failed = total - verified;

    if total == 0 {
        return "No factual claims found in report.".to_string();
    }

    let mut summary = format!(
        "Claim verification: {}/{} verified, {} failed\n",
        verified, total, failed
    );

    for result in results {
        if !result.verified {
            summary.push_str(&format!(
                "  UNVERIFIED: {} - {}\n",
                result.claim.text, result.evidence
            ));
        }
    }

    summary
}

/// Render a full verification report, including structured proof detail.
///
/// Unverified vulnerability claims are shown with every missing requirement
/// spelled out, so a reader can tell the difference between "not proven" and
/// "proven false".
pub fn verification_report(results: &[ClaimCheckResult]) -> String {
    let mut out = verification_summary(results);
    let proofs: Vec<&ClaimCheckResult> = results.iter().filter(|r| r.proof.is_some()).collect();
    if !proofs.is_empty() {
        out.push_str("\nStructured proof:\n");
        for result in proofs {
            let verdict = result.proof.as_ref().expect("filtered on Some");
            out.push_str(&describe_verdict(&result.claim.text, verdict));
        }
    }
    out
}

// --- Claim extraction helpers ---

fn extract_file_claim(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    if lower.contains("modified")
        || lower.contains("created")
        || lower.contains("updated")
        || lower.contains("changed")
    {
        // Try to extract a file path
        for word in line.split_whitespace() {
            if word.contains('/') || word.contains('.') {
                let cleaned =
                    word.trim_matches(|c: char| c == '`' || c == '*' || c == '(' || c == ')');
                if cleaned.contains('/') && !cleaned.starts_with("http") {
                    return Some(cleaned.to_string());
                }
            }
        }
    }
    None
}

fn extract_test_claim(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    if lower.contains("test")
        && (lower.contains("pass") || lower.contains("succeed") || lower.contains("ran"))
    {
        // Extract test name from backticks or after "test"
        if let Some(start) = line.find('`')
            && let Some(end) = line[start + 1..].find('`')
        {
            return Some(line[start + 1..start + 1 + end].to_string());
        }
    }
    None
}

fn extract_command_claim(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    if lower.contains("command")
        && (lower.contains("succeed") || lower.contains("exit code 0") || lower.contains("ran"))
        && let Some(start) = line.find('`')
        && let Some(end) = line[start + 1..].find('`')
    {
        return Some(line[start + 1..start + 1 + end].to_string());
    }
    None
}

fn extract_http_status_claim(line: &str) -> Option<u16> {
    let lower = line.to_lowercase();
    if lower.contains("http") && lower.contains("status") {
        for word in line.split_whitespace() {
            if let Ok(status) = word.parse::<u16>()
                && (100..600).contains(&status)
            {
                return Some(status);
            }
        }
    }
    None
}

fn extract_vuln_claim(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    if lower.contains("vulnerability") || lower.contains("vuln") || lower.contains("cve") {
        if let Some(start) = line.find('`')
            && let Some(end) = line[start + 1..].find('`')
        {
            return Some(line[start + 1..start + 1 + end].to_string());
        }
        // Try to extract CVE ID
        for word in line.split_whitespace() {
            let upper = word.to_uppercase();
            if upper.starts_with("CVE-") {
                return Some(word.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_message_types::ContentBlock;
    use crate::alphacode_session_types::StoredMessage;
    use chrono::Utc;

    fn claim(claim_type: ClaimType) -> Claim {
        Claim {
            text: "claim text".into(),
            claim_type,
        }
    }

    /// Build a transcript containing one `httpflow` call with `output`.
    fn transcript(url: &str, method: &str, output: &str) -> Vec<StoredMessage> {
        let input = serde_json::json!({"method": method, "url": url});
        vec![
            StoredMessage {
                id: "m1".into(),
                role: crate::alphacode_message_types::Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "httpflow".into(),
                    input,
                    thought_signature: None,
                }],
                display_role: None,
                timestamp: Some(Utc::now()),
                tool_duration_ms: None,
                token_usage: None,
            },
            StoredMessage {
                id: "m2".into(),
                role: crate::alphacode_message_types::Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: output.into(),
                    is_error: Some(false),
                }],
                display_role: None,
                timestamp: Some(Utc::now()),
                tool_duration_ms: None,
                token_usage: None,
            },
        ]
    }

    fn transcript_of_tool(
        tool: &str,
        input: serde_json::Value,
        output: &str,
    ) -> Vec<StoredMessage> {
        vec![
            StoredMessage {
                id: "m1".into(),
                role: crate::alphacode_message_types::Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: tool.into(),
                    input,
                    thought_signature: None,
                }],
                display_role: None,
                timestamp: Some(Utc::now()),
                tool_duration_ms: None,
                token_usage: None,
            },
            StoredMessage {
                id: "m2".into(),
                role: crate::alphacode_message_types::Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: output.into(),
                    is_error: Some(false),
                }],
                display_role: None,
                timestamp: Some(Utc::now()),
                tool_duration_ms: None,
                token_usage: None,
            },
        ]
    }

    // --- the regression: stubs that used to return true --------------------

    /// The exact false positive this gate exists to kill: "found SQLi" with no
    /// request ever sent.
    ///
    /// Before this change `ClaimType::VulnerabilityConfirmed` returned
    /// `verified: true` unconditionally.
    #[test]
    fn a_vulnerability_claim_without_a_proof_is_not_verified() {
        let result = check_claim(
            &claim(ClaimType::VulnerabilityConfirmed("SQL injection".into())),
            Path::new("."),
            &SessionEvidence::default(),
        );
        assert!(!result.verified, "{}", result.evidence);
        assert!(
            result.evidence.contains("no structured proof"),
            "{}",
            result.evidence
        );
    }

    /// Previously hardcoded `verified: true` because "generic claims can't be
    /// verified". Counting them as verified is how unbacked assertions reach a
    /// report.
    #[test]
    fn a_generic_claim_is_not_verified() {
        let result = check_claim(
            &claim(ClaimType::Generic),
            Path::new("."),
            &SessionEvidence::default(),
        );
        assert!(!result.verified);
        assert!(
            result.evidence.contains("not mechanically verifiable"),
            "{}",
            result.evidence
        );
    }

    /// Previously hardcoded `verified: true`.
    #[test]
    fn a_command_claim_without_observed_success_is_not_verified() {
        let result = check_claim(
            &claim(ClaimType::CommandSucceeded("cargo test".into())),
            Path::new("."),
            &SessionEvidence::default(),
        );
        assert!(!result.verified, "{}", result.evidence);
    }

    /// Previously hardcoded `verified: true`.
    #[test]
    fn a_test_claim_without_observed_passing_output_is_not_verified() {
        let result = check_claim(
            &claim(ClaimType::TestPassed("test_login".into())),
            Path::new("."),
            &SessionEvidence::default(),
        );
        assert!(!result.verified, "{}", result.evidence);
    }

    #[test]
    fn a_command_claim_with_observed_success_verifies() {
        let messages = transcript_of_tool(
            "bash",
            serde_json::json!({"command": "cargo test --lib"}),
            "running 1 test\ntest result: ok. 1 passed\nprocess exited with 0",
        );
        let evidence = SessionEvidence::from_messages(&messages);
        let result = check_claim(
            &claim(ClaimType::CommandSucceeded("cargo test".into())),
            Path::new("."),
            &evidence,
        );
        assert!(result.verified, "{}", result.evidence);
    }

    #[test]
    fn a_failed_command_does_not_verify() {
        let messages = transcript_of_tool(
            "bash",
            serde_json::json!({"command": "cargo test"}),
            "error: could not compile `foo`\n",
        );
        let evidence = SessionEvidence::from_messages(&messages);
        assert!(evidence.succeeded_commands.is_empty());
        let result = check_claim(
            &claim(ClaimType::CommandSucceeded("cargo test".into())),
            Path::new("."),
            &evidence,
        );
        assert!(!result.verified);
    }

    #[test]
    fn a_test_claim_with_passing_output_verifies() {
        let messages = transcript_of_tool(
            "bash",
            serde_json::json!({"command": "cargo test test_login"}),
            "running 1 test\ntest test_login_flow ... ok\ntest result: ok.",
        );
        let evidence = SessionEvidence::from_messages(&messages);
        assert!(evidence.passed_tests.iter().any(|t| t == "test_login_flow"));
        let result = check_claim(
            &claim(ClaimType::TestPassed("test_login_flow".into())),
            Path::new("."),
            &evidence,
        );
        assert!(result.verified, "{}", result.evidence);
    }

    // --- step 2: HTTP status semantics -------------------------------------

    /// The old rule verified exactly this case.
    #[test]
    fn a_bare_200_is_no_longer_verified_as_a_finding() {
        let messages = transcript(
            "https://example.com/about",
            "GET",
            "HTTP 200 OK\nURL: https://example.com/about\n\nhello world",
        );
        let evidence = SessionEvidence::from_messages(&messages);
        let claim = Claim {
            text: "HTTP status 200 observed on https://example.com/about".into(),
            claim_type: ClaimType::HttpStatus(200),
        };
        let result = check_claim(&claim, Path::new("."), &evidence);
        assert!(
            !result.verified,
            "a bare 200 on an ordinary path is not a finding: {}",
            result.evidence
        );
        assert!(
            result.evidence.contains("not a finding"),
            "{}",
            result.evidence
        );
    }

    /// The old rule marked this UNVERIFIED. It is the actual finding.
    #[test]
    fn a_403_on_a_sensitive_path_is_informative() {
        let messages = transcript(
            "https://example.com/admin",
            "GET",
            "HTTP 403 Forbidden\nURL: https://example.com/admin\n\nforbidden",
        );
        let evidence = SessionEvidence::from_messages(&messages);
        let claim = Claim {
            text: "HTTP status 403 observed on https://example.com/admin".into(),
            claim_type: ClaimType::HttpStatus(403),
        };
        let result = check_claim(&claim, Path::new("."), &evidence);
        assert!(result.verified, "{}", result.evidence);
        assert!(
            result.evidence.contains("informative"),
            "{}",
            result.evidence
        );
    }

    #[test]
    fn a_500_is_informative() {
        let messages = transcript(
            "https://example.com/x",
            "GET",
            "HTTP 500 Internal Server Error\nURL: https://example.com/x\n\nboom",
        );
        let evidence = SessionEvidence::from_messages(&messages);
        let claim = Claim {
            text: "HTTP status 500 observed on https://example.com/x".into(),
            claim_type: ClaimType::HttpStatus(500),
        };
        let result = check_claim(&claim, Path::new("."), &evidence);
        assert!(result.verified, "{}", result.evidence);
    }

    /// A status nobody ever observed must not verify, even though 403 is an
    /// interesting code.
    #[test]
    fn a_status_that_was_never_observed_does_not_verify() {
        let result = check_claim(
            &claim(ClaimType::HttpStatus(403)),
            Path::new("."),
            &SessionEvidence::default(),
        );
        assert!(!result.verified);
        assert!(
            result.evidence.contains("never observed"),
            "{}",
            result.evidence
        );
    }

    /// "We saw a 403 somewhere" must not stand in for "we saw a 403 there".
    #[test]
    fn a_status_must_match_the_claimed_url() {
        let messages = transcript(
            "https://example.com/admin",
            "GET",
            "HTTP 403 Forbidden\nURL: https://example.com/admin\n\nforbidden",
        );
        let evidence = SessionEvidence::from_messages(&messages);
        // The 403 was genuinely observed — just not on the claimed host.
        assert_eq!(evidence.exchanges.len(), 1, "{evidence:?}");
        let claim = Claim {
            text: "HTTP status 403 observed on https://other.example.com/secret".into(),
            claim_type: ClaimType::HttpStatus(403),
        };
        let result = check_claim(&claim, Path::new("."), &evidence);
        assert!(!result.verified, "{}", result.evidence);
        assert!(
            result.evidence.contains("never observed"),
            "the claim cites a URL that was never probed: {}",
            result.evidence
        );
    }

    /// A tool that is not an HTTP client cannot manufacture HTTP evidence, no
    /// matter what its output looks like.
    #[test]
    fn a_non_http_tool_cannot_supply_http_evidence() {
        let messages = transcript_of_tool(
            "read_file",
            serde_json::json!({"path": "notes.md"}),
            "HTTP 200 OK\nURL: https://example.com/admin\n\npretending",
        );
        let evidence = SessionEvidence::from_messages(&messages);
        assert!(
            evidence.exchanges.is_empty(),
            "only HTTP tools may produce exchanges: {evidence:?}"
        );
    }

    // --- evidence reconstruction -------------------------------------------

    #[test]
    fn session_evidence_reconstructs_exchanges_from_the_transcript() {
        let messages = transcript(
            "https://example.com/admin",
            "GET",
            "HTTP 403 Forbidden\nURL: https://example.com/admin\ncontent-type: text/html\n\nbody",
        );
        let evidence = SessionEvidence::from_messages(&messages);
        assert_eq!(evidence.exchanges.len(), 1, "{evidence:?}");
        assert_eq!(evidence.exchanges[0].status, 403);
        assert_eq!(evidence.exchanges[0].method, "GET");
        assert_eq!(evidence.tool_calls, vec![("httpflow".to_string(), 1)]);
    }

    #[test]
    fn an_errored_tool_result_yields_no_exchange() {
        let messages = transcript(
            "https://example.com/admin",
            "GET",
            "Bot protection detected (status 403).",
        );
        let evidence = SessionEvidence::from_messages(&messages);
        assert!(
            evidence.exchanges.is_empty(),
            "an unparseable result must not fabricate evidence: {evidence:?}"
        );
    }

    // --- full proof verification -------------------------------------------

    fn proof_json(include_baseline: bool) -> String {
        let baseline = serde_json::json!({
            "method": "GET",
            "url": "https://example.com/api/search",
            "request_body": null,
            "status": 403,
            "response_headers": [],
            "response_body": "forbidden",
            "source_tool": "httpflow"
        });
        let treatment = serde_json::json!({
            "method": "GET",
            "url": "https://example.com/api/search?q=%27%20OR%201=1--",
            "request_body": null,
            "status": 500,
            "response_headers": [],
            "response_body": "SQL syntax error near OR",
            "source_tool": "httpflow"
        });
        let baseline_field = if include_baseline {
            serde_json::json!({
                "label": "unauthenticated",
                "baseline": baseline,
                "treatment": treatment,
                "difference": {
                    "status_delta": 97,
                    "status_changed": true,
                    "body_len_delta": 4,
                    "body_len_materially_changed": false
                }
            })
        } else {
            serde_json::Value::Null
        };
        serde_json::json!({
            "finding_id": "f1",
            "test_exchange": treatment,
            "reproduction": [{
                "action": "GET /api/search?q=%27%20OR%201=1--",
                "expected_observation": "a SQL error"
            }],
            "expected_behavior": "400 bad request",
            "observed_behavior": "500 with a SQL syntax error",
            "baseline": baseline_field
        })
        .to_string()
    }

    #[test]
    fn a_vulnerability_claim_with_a_full_proof_verifies_against_observed_history() {
        let messages = transcript(
            "https://example.com/api/search?q=%27%20OR%201=1--",
            "GET",
            "HTTP 500 Internal Server Error\nURL: https://example.com/api/search?q=%27%20OR%201=1--\n\nSQL syntax error near OR",
        );
        let evidence = SessionEvidence::from_messages(&messages);

        let report = format!(
            "Confirmed vulnerability `SQL injection in /api/search`\n```proof\n{}\n```",
            proof_json(true)
        );
        let results = check_all_claims(&report, Path::new("."), &evidence);
        let vuln = results
            .iter()
            .find(|r| matches!(r.claim.claim_type, ClaimType::VulnerabilityConfirmed(_)))
            .expect("a vulnerability claim should be extracted");
        assert!(vuln.verified, "{}", vuln.evidence);
    }

    /// A complete-looking proof for a request that was never sent.
    #[test]
    fn a_proof_for_an_unobserved_request_does_not_verify() {
        let evidence = SessionEvidence::default();
        let report = format!(
            "Confirmed vulnerability `SQL injection in /api/search`\n```proof\n{}\n```",
            proof_json(true)
        );
        let results = check_all_claims(&report, Path::new("."), &evidence);
        let vuln = results
            .iter()
            .find(|r| matches!(r.claim.claim_type, ClaimType::VulnerabilityConfirmed(_)))
            .expect("claim extracted");
        assert!(!vuln.verified, "{}", vuln.evidence);
        assert!(vuln.evidence.contains("not observed"), "{}", vuln.evidence);
    }

    #[test]
    fn a_proof_without_a_baseline_does_not_verify() {
        let messages = transcript(
            "https://example.com/api/search?q=%27%20OR%201=1--",
            "GET",
            "HTTP 500 Internal Server Error\nURL: https://example.com/api/search?q=%27%20OR%201=1--\n\nSQL syntax error near OR",
        );
        let evidence = SessionEvidence::from_messages(&messages);
        let report = format!(
            "Confirmed vulnerability `SQL injection in /api/search`\n```proof\n{}\n```",
            proof_json(false)
        );
        let results = check_all_claims(&report, Path::new("."), &evidence);
        let vuln = results
            .iter()
            .find(|r| matches!(r.claim.claim_type, ClaimType::VulnerabilityConfirmed(_)))
            .expect("claim extracted");
        assert!(!vuln.verified, "{}", vuln.evidence);
        assert!(vuln.evidence.contains("baseline"), "{}", vuln.evidence);
    }

    #[test]
    fn inline_proof_blocks_are_parsed() {
        let text = format!("PROOF: {}", proof_json(true));
        let proof = extract_proofs(&text)
            .into_iter()
            .next()
            .expect("inline proof should parse");
        assert_eq!(proof.finding_id, "f1");
        assert!(proof.test_exchange.is_some());
        assert!(proof.baseline.is_some());
    }

    #[test]
    fn a_report_with_no_proof_block_yields_no_proof() {
        assert!(extract_proofs("Found SQLi in /search").is_empty());
        assert!(extract_proofs("Found SQLi in /search").is_empty());
    }

    /// Claims are one line each but a proof block spans several, so the two
    /// must be matched by content rather than position.
    #[test]
    fn a_proof_block_is_matched_to_its_claim_across_lines() {
        let report = format!(
            "Confirmed vulnerability `SQL injection in /api/search`\n```proof\n{}\n```",
            proof_json(true)
        );
        let proofs = extract_proofs(&report);
        assert_eq!(proofs.len(), 1, "a fenced proof should be parsed");
        assert!(
            match_proof_for_claim(
                "Confirmed vulnerability `SQL injection in /api/search`",
                &proofs
            )
            .is_some(),
            "a single proof must attach to the claim that asserts it"
        );
    }

    /// A proof that belongs to a different finding must not be borrowed to
    /// justify this claim.
    #[test]
    fn a_proof_belonging_to_another_finding_does_not_transfer() {
        let first = serde_json::json!({
            "finding_id": "aaa",
            "test_exchange": {
                "method": "GET",
                "url": "https://example.com/one",
                "request_body": null,
                "status": 500,
                "response_headers": [],
                "response_body": "boom",
                "source_tool": "httpflow"
            },
            "reproduction": [{"action": "GET /one", "expected_observation": "an error"}],
            "expected_behavior": "400",
            "observed_behavior": "500",
            "baseline": {
                "label": "unauthenticated",
                "baseline": {
                    "method": "GET", "url": "https://example.com/one",
                    "request_body": null, "status": 403, "response_headers": [],
                    "response_body": "no", "source_tool": "httpflow"
                },
                "treatment": {
                    "method": "GET", "url": "https://example.com/one",
                    "request_body": null, "status": 500, "response_headers": [],
                    "response_body": "boom", "source_tool": "httpflow"
                },
                "difference": {
                    "status_delta": 97, "status_changed": true,
                    "body_len_delta": 3, "body_len_materially_changed": false
                }
            }
        })
        .to_string();
        let second = first.replace("aaa", "bbb").replace("/one", "/two");
        let proofs = extract_proofs(&format!(
            "Confirmed vulnerability `first`\n```proof\n{first}\n```\n```proof\n{second}\n```"
        ));
        assert_eq!(proofs.len(), 2);

        // The claim asserts a third finding that neither proof covers.
        assert!(
            match_proof_for_claim("Confirmed vulnerability `CCC`", &proofs).is_none(),
            "a proof for another finding must not justify this claim"
        );
    }

    // --- existing behaviour, preserved -------------------------------------

    #[test]
    fn extract_file_claims() {
        let report = "I modified `src/main.rs` and created `tests/new_test.rs`";
        let claims = extract_claims(report);
        assert!(!claims.is_empty(), "should find at least one file claim");
        assert!(
            claims
                .iter()
                .any(|c| matches!(&c.claim_type, ClaimType::FileModified(_)))
        );
    }

    #[test]
    fn extract_test_claims() {
        let report = "All tests passed: `test_login_flow` and `test_auth` succeeded";
        let claims = extract_claims(report);
        assert!(!claims.is_empty());
    }

    #[test]
    fn check_file_claim_exists() {
        let claim = Claim {
            text: "modified Cargo.toml".into(),
            claim_type: ClaimType::FileModified("Cargo.toml".into()),
        };
        let result = check_claim(&claim, Path::new("."), &SessionEvidence::default());
        assert!(result.verified);
    }

    #[test]
    fn check_file_claim_not_exists() {
        let claim = Claim {
            text: "created nonexistent_file.txt".into(),
            claim_type: ClaimType::FileModified("nonexistent_file.txt".into()),
        };
        let result = check_claim(&claim, Path::new("."), &SessionEvidence::default());
        assert!(!result.verified);
    }

    #[test]
    fn verification_summary_format() {
        let results = vec![
            ClaimCheckResult {
                claim: Claim {
                    text: "test".into(),
                    claim_type: ClaimType::Generic,
                },
                verified: true,
                evidence: "ok".into(),
                proof: None,
            },
            ClaimCheckResult {
                claim: Claim {
                    text: "test2".into(),
                    claim_type: ClaimType::Generic,
                },
                verified: false,
                evidence: "failed".into(),
                proof: None,
            },
        ];
        let summary = verification_summary(&results);
        assert!(summary.contains("1/2 verified"));
        assert!(summary.contains("UNVERIFIED"));
    }

    #[test]
    fn a_report_with_no_claims_says_so() {
        let summary = verification_summary(&check_all_claims(
            "All good.",
            Path::new("."),
            &SessionEvidence::default(),
        ));
        assert!(summary.contains("No factual claims"));
    }

    #[test]
    fn the_verification_report_names_missing_evidence() {
        let messages = transcript(
            "https://example.com/api/search?q=%27%20OR%201=1--",
            "GET",
            "HTTP 500 Internal Server Error\nURL: https://example.com/api/search?q=%27%20OR%201=1--\n\nSQL error",
        );
        let evidence = SessionEvidence::from_messages(&messages);
        let report = format!(
            "Confirmed vulnerability `SQL injection in /api/search`\n```proof\n{}\n```",
            proof_json(false)
        );
        let results = check_all_claims(&report, Path::new("."), &evidence);
        let text = verification_report(&results);
        assert!(text.contains("UNVERIFIED"), "{text}");
        assert!(text.contains("Structured proof:"), "{text}");
        assert!(text.contains("missing:"), "{text}");
    }

    #[test]
    fn an_empty_transcript_verifies_nothing() {
        let results = check_all_claims(
            "Confirmed vulnerability `anything`",
            Path::new("."),
            &SessionEvidence::default(),
        );
        assert!(
            results.iter().all(|r| !r.verified),
            "an empty transcript must not verify anything"
        );
    }
}
