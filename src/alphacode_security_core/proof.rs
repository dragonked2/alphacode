//! Structured proof for security findings.
//!
//! A security finding is a claim about how a live system behaves. That claim
//! is worth nothing on its own — "there is SQLi in `/search`" is a guess until
//! something that actually happened is attached to it. This module is the
//! evidence gate that turns a guess into a checkable proof object.
//!
//! A proof requires four things, and verification is **fail-closed**: if any
//! one of them is absent or inconclusive, the finding is *not* verified and the
//! verdict names exactly what is missing.
//!
//! 1. **The request actually sent** — method, URL, and (where relevant) body.
//! 2. **The response actually received** — status, headers, body.
//! 3. **Reproduction steps** a third party can repeat without the author.
//! 4. **Observed vs expected** — and the two must actually differ. A finding
//!    whose observed behaviour matches its expected behaviour is not a finding.
//!
//! On top of that a proof may carry a **baseline differential**: the same
//! request against a comparison point (typically unauthenticated, or against a
//! sibling path), so the claim is anchored to a difference rather than to a
//! single response that merely looks unusual. See [`BaselineComparison`].
//!
//! Finally, the observed response must be *informative*: a bare `200` is the
//! least interesting result recon produces and is not evidence of a problem.
//! See [`StatusClass`].

use serde::{Deserialize, Serialize};

/// Largest response body retained in a proof, in characters.
///
/// A proof only needs enough of the body to show what the reader is being
/// pointed at. Capping keeps a single 5 MB response from being copied into
/// every downstream report and checkpoint.
const MAX_BODY_CHARS: usize = 8_192;

/// A single observed HTTP exchange, reconstructed from real tool output.
///
/// This is the ground truth for "a request was sent and this is what came
/// back". It is only ever built by [`HttpExchange::parse_http_transcript`],
/// which reads an actual tool result, so a `HttpExchange` in hand means the
/// exchange really happened.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HttpExchange {
    pub method: String,
    pub url: String,
    pub request_body: Option<String>,
    pub status: u16,
    pub response_headers: Vec<(String, String)>,
    pub response_body: Option<String>,
    /// Tool that produced this exchange, for provenance in the report.
    pub source_tool: String,
}

impl HttpExchange {
    /// Rebuild an exchange from an HTTP tool's input and output.
    ///
    /// `input` supplies the request side (method, url, body) because the
    /// transcript's own header block only echoes the *final* URL of a redirect
    /// chain. `output` is the tool's textual response transcript.
    ///
    /// Returns `None` when the output does not carry an HTTP status line, so a
    /// failed or non-HTTP tool result never fabricates an exchange.
    pub fn parse_http_transcript(
        source_tool: &str,
        input: &serde_json::Value,
        output: &str,
    ) -> Option<Self> {
        let mut status = None;
        let mut url = None;
        let mut headers: Vec<(String, String)> = Vec::new();
        let mut body_lines: Vec<&str> = Vec::new();
        let mut in_body = false;

        for raw in output.lines() {
            let line = raw.trim_end_matches('\r');
            if in_body {
                body_lines.push(line);
                continue;
            }
            if line.trim().is_empty() {
                // Blank line before any status is not a transcript we
                // understand; after one it is the header/body separator.
                if status.is_some() {
                    in_body = true;
                }
                continue;
            }
            if status.is_none() {
                if let Some(rest) = line.strip_prefix("HTTP ") {
                    let code = rest.split_whitespace().next()?;
                    let code: u16 = code.parse().ok()?;
                    // Guard against `HTTP 2000` or a banner that merely starts
                    // with "HTTP".
                    if !(100..600).contains(&code) {
                        return None;
                    }
                    status = Some(code);
                }
                continue;
            }
            if let Some(value) = line.strip_prefix("URL: ") {
                url = Some(value.trim().to_string());
                continue;
            }
            // `Cookies set:` and `Redirect chain:` are prose emitted by the
            // tool, not headers, and `CSRF token:` is a footer line.
            if line.starts_with("Cookies set:")
                || line.starts_with("Redirect chain:")
                || line.starts_with("CSRF token:")
            {
                continue;
            }
            if let Some((name, value)) = line.split_once(": ")
                && !name.contains(' ')
            {
                headers.push((name.to_ascii_lowercase(), value.trim().to_string()));
            }
        }

        let status = status?;
        let url = url.or_else(|| input.get("url").and_then(|v| v.as_str()).map(String::from))?;

        let body = body_lines.join("\n");
        let request_body = input
            .get("body")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(|b| b.chars().take(MAX_BODY_CHARS).collect());

        Some(Self {
            method: input
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("GET")
                .trim()
                .to_ascii_uppercase(),
            url,
            request_body,
            status,
            response_headers: headers,
            response_body: if body.trim().is_empty() {
                None
            } else {
                Some(body.chars().take(MAX_BODY_CHARS).collect())
            },
            source_tool: source_tool.to_string(),
        })
    }

    /// Identity of the exchange for cross-checking against observed activity:
    /// same method, same URL, same status.
    pub fn matches_observed(&self, other: &Self) -> bool {
        self.method.eq_ignore_ascii_case(&other.method)
            && self.url == other.url
            && self.status == other.status
    }

    /// Character length of the observed body, for differential comparison.
    pub fn body_len(&self) -> usize {
        self.response_body.as_deref().map(str::len).unwrap_or(0)
    }
}

/// How interesting an observed status code actually is.
///
/// The original verifier treated `2xx` as verified and everything else as
/// unverified, which is exactly backwards for recon: a bare `200` says the host
/// is up and nothing more, while `403`, `500` and `302` on a sensitive path are
/// frequently the finding itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusClass {
    /// 2xx. On any path this is the least informative result recon produces.
    RoutineSuccess,
    /// 2xx on a path that should not be publicly readable.
    SensitiveSuccess,
    /// 401/403 — access control engaged. Informative mainly in contrast with a
    /// sibling path that returns 404 (an existence oracle).
    AccessDenied,
    /// 3xx — informative when it leaves the origin or carries a token.
    Redirect,
    /// 5xx — server error; informative when the body discloses internals.
    ServerError,
    /// 429 and friends.
    Throttled,
    /// Anything outside the ranges above.
    Other,
}

impl StatusClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RoutineSuccess => "routine_success",
            Self::SensitiveSuccess => "sensitive_success",
            Self::AccessDenied => "access_denied",
            Self::Redirect => "redirect",
            Self::ServerError => "server_error",
            Self::Throttled => "throttled",
            Self::Other => "other",
        }
    }

    /// Whether this status is worth reporting at all.
    ///
    /// `RoutineSuccess` is never informative: that variant *means* "2xx on a
    /// path where nothing sensitive is exposed", so it is excluded regardless
    /// of context. Path sensitivity is resolved once, by
    /// [`StatusClass::classify`], which promotes a sensitive-path success to
    /// [`StatusClass::SensitiveSuccess`] before any caller sees it — so there
    /// is nothing for a caller to pass in here.
    pub fn is_informative(&self) -> bool {
        !matches!(self, Self::RoutineSuccess)
    }

    /// Classify a status code observed on `path`.
    pub fn classify(status: u16, path: &str) -> Self {
        let sensitive = is_sensitive_path(path);
        match status {
            200..=299 => {
                if sensitive {
                    Self::SensitiveSuccess
                } else {
                    Self::RoutineSuccess
                }
            }
            401 | 403 => Self::AccessDenied,
            300..=399 => Self::Redirect,
            429 => Self::Throttled,
            500..=599 => Self::ServerError,
            _ => Self::Other,
        }
    }
}

/// Path fragments that mark a route as one that should not simply serve `200`
/// to anyone. Used to decide whether a success response is itself noteworthy.
const SENSITIVE_PATH_MARKERS: &[&str] = &[
    "admin",
    "actuator",
    "api-key",
    "apikey",
    "auth",
    "backup",
    "config",
    "credential",
    "dashboard",
    "debug",
    "dump",
    "env",
    "graphql",
    "internal",
    "manager",
    "metrics",
    "phpmyadmin",
    "private",
    "secret",
    "swagger",
    "token",
    "trace",
    "upload",
    "user",
    "wp-admin",
];

/// Does this path look like one that should not be publicly readable?
pub fn is_sensitive_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    SENSITIVE_PATH_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

/// How the treatment exchange differed from the baseline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedDifference {
    /// `treatment.status - baseline.status`, when both are known.
    pub status_delta: Option<i32>,
    pub status_changed: bool,
    pub body_len_delta: Option<i64>,
    /// The body grew or shrank by at least this many characters.
    pub body_len_materially_changed: bool,
}

/// A baseline exchange paired with the exchange under test.
///
/// A single response can look alarming and be entirely normal for the
/// application. Anchoring the claim to a comparison point — the same request
/// unauthenticated, or against a sibling path that should behave identically —
/// is what separates a finding from a pattern match.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BaselineComparison {
    /// What the comparison point was, e.g. "unauthenticated", "sibling path".
    pub label: String,
    /// The control exchange.
    pub baseline: HttpExchange,
    /// The exchange under test.
    pub treatment: HttpExchange,
    pub difference: ObservedDifference,
}

impl BaselineComparison {
    pub fn new(label: impl Into<String>, baseline: HttpExchange, treatment: HttpExchange) -> Self {
        let status_delta = i32::from(treatment.status) - i32::from(baseline.status);
        let body_len_delta = treatment.body_len() as i64 - baseline.body_len() as i64;
        let difference = ObservedDifference {
            status_delta: Some(status_delta),
            status_changed: treatment.status != baseline.status,
            body_len_delta: Some(body_len_delta),
            body_len_materially_changed: body_len_delta.unsigned_abs() >= 64,
        };
        Self {
            label: label.into(),
            baseline,
            treatment,
            difference,
        }
    }

    /// Whether the difference is large enough to be a finding rather than
    /// noise.
    ///
    /// A status change always counts. A body change has to clear both an
    /// absolute and a relative floor, so a page that grows by 40 bytes between
    /// two renders is not reported as a data-exposure finding.
    pub fn is_material(&self) -> bool {
        if self.difference.status_changed {
            return true;
        }
        if !self.difference.body_len_materially_changed {
            return false;
        }
        let baseline_len = (self.baseline.body_len() as u64).max(1);
        let delta = self.difference.body_len_delta.unwrap_or(0).unsigned_abs();
        delta * 4 >= baseline_len
    }
}

/// A way for a proof to fail. Every variant names something the reader can go
/// and supply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofDefect {
    /// No observed exchange is attached — nothing was actually sent.
    MissingTestExchange,
    /// The exchange was never seen in the session's tool history.
    ExchangeNotObserved,
    /// No reproduction steps, so a third party cannot repeat the test.
    MissingReproductionSteps,
    /// No expected behaviour to compare against.
    MissingExpectedBehavior,
    /// No observed behaviour recorded.
    MissingObservedBehavior,
    /// Observed behaviour matches expected behaviour: not a finding.
    BehaviorMatchesExpectation,
    /// No baseline comparison, so the claim rests on a single response.
    MissingBaseline,
    /// The baseline and treatment are indistinguishable.
    BaselineIndistinguishable,
    /// The observed status is not informative on its own (e.g. a bare 200).
    UninformativeStatus,
}

/// The outcome of checking a proof. `verified` is only ever true when
/// `defects` is empty.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProofVerdict {
    pub verified: bool,
    pub defects: Vec<ProofDefect>,
    /// What the proof actually established, for the report.
    pub observations: Vec<String>,
}

impl ProofVerdict {
    fn fail(defects: Vec<ProofDefect>, observations: Vec<String>) -> Self {
        Self {
            verified: defects.is_empty(),
            defects,
            observations,
        }
    }
}

impl ProofDefect {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MissingTestExchange => "no observed request/response was attached",
            Self::ExchangeNotObserved => {
                "the attached request/response was not observed in this session"
            }
            Self::MissingReproductionSteps => "no reproduction steps were provided",
            Self::MissingExpectedBehavior => "no expected behaviour was stated",
            Self::MissingObservedBehavior => "no observed behaviour was stated",
            Self::BehaviorMatchesExpectation => "observed behaviour matches expected behaviour",
            Self::MissingBaseline => "no baseline/control comparison was provided",
            Self::BaselineIndistinguishable => {
                "the baseline and treatment responses are indistinguishable"
            }
            Self::UninformativeStatus => {
                "the observed status is a routine success and is not informative on its own"
            }
        }
    }
}

/// One reproduction step a third party can repeat.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProofStep {
    /// Literal request to issue, e.g. `GET /search?q=' OR 1=1--`.
    pub action: String,
    /// What to look for in the response.
    pub expected_observation: String,
    #[serde(default)]
    pub tool: Option<String>,
}

/// The structured proof attached to a security finding.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProofObject {
    pub finding_id: String,
    /// The exchange that demonstrates the issue.
    pub test_exchange: Option<HttpExchange>,
    #[serde(default)]
    pub reproduction: Vec<ProofStep>,
    /// How the application is supposed to behave.
    pub expected_behavior: String,
    /// How it actually behaved.
    pub observed_behavior: String,
    /// The comparison point, when one was captured.
    #[serde(default)]
    pub baseline: Option<BaselineComparison>,
}

impl ProofObject {
    pub fn new(finding_id: impl Into<String>) -> Self {
        Self {
            finding_id: finding_id.into(),
            test_exchange: None,
            reproduction: Vec::new(),
            expected_behavior: String::new(),
            observed_behavior: String::new(),
            baseline: None,
        }
    }

    /// Check the proof on its own terms, without cross-checking against the
    /// session's observed activity.
    ///
    /// This is the weaker check and is for callers that genuinely have no
    /// transcript (e.g. validating a finding loaded from a report). It is
    /// **not** sufficient for reporting: a self-consistent proof that nobody
    /// observed is still an unbacked claim.
    pub fn verify(&self) -> ProofVerdict {
        self.evaluate(None)
    }

    /// Check the proof against the exchanges actually observed in the session.
    ///
    /// This is the check that matters, and it is fail-closed. The attached
    /// exchange must appear in `observed`; otherwise the proof is describing a
    /// request that was never made, which is the failure mode this whole gate
    /// exists to stop. An **empty** history fails for the same reason — with
    /// nothing observed, nothing can be confirmed.
    pub fn verify_against(&self, observed: &[HttpExchange]) -> ProofVerdict {
        self.evaluate(Some(observed))
    }

    fn evaluate(&self, observed: Option<&[HttpExchange]>) -> ProofVerdict {
        let mut defects = Vec::new();
        let mut observations = Vec::new();

        let Some(exchange) = self.test_exchange.as_ref() else {
            defects.push(ProofDefect::MissingTestExchange);
            return ProofVerdict::fail(defects, observations);
        };

        match observed {
            Some(history) => {
                if !history.iter().any(|o| o.matches_observed(exchange)) {
                    defects.push(ProofDefect::ExchangeNotObserved);
                }
            }
            None => observations.push(
                "no session history available — the request could not be cross-checked".to_string(),
            ),
        }

        observations.push(format!(
            "{} {} returned {}",
            exchange.method, exchange.url, exchange.status
        ));

        let status_class = StatusClass::classify(exchange.status, &exchange.url);
        if !status_class.is_informative() {
            defects.push(ProofDefect::UninformativeStatus);
        } else {
            observations.push(format!("status classified as {}", status_class.as_str()));
        }

        if self.reproduction.iter().all(|s| s.action.trim().is_empty()) {
            defects.push(ProofDefect::MissingReproductionSteps);
        }
        if self.expected_behavior.trim().is_empty() {
            defects.push(ProofDefect::MissingExpectedBehavior);
        }
        if self.observed_behavior.trim().is_empty() {
            defects.push(ProofDefect::MissingObservedBehavior);
        }

        // Observed vs expected must actually differ. Normalised before
        // comparison so that whitespace or casing cannot disguise a match.
        let expected_key = normalise(&self.expected_behavior);
        let observed_key = normalise(&self.observed_behavior);
        if !expected_key.is_empty() && expected_key == observed_key {
            defects.push(ProofDefect::BehaviorMatchesExpectation);
        }

        match self.baseline.as_ref() {
            None => defects.push(ProofDefect::MissingBaseline),
            Some(baseline) if !baseline.is_material() => {
                defects.push(ProofDefect::BaselineIndistinguishable);
                observations.push(format!(
                    "baseline '{}' did not differ materially from the treatment",
                    baseline.label
                ));
            }
            Some(baseline) => observations.push(format!(
                "baseline '{}' differs (status_changed={}, body_len_delta={:?})",
                baseline.label,
                baseline.difference.status_changed,
                baseline.difference.body_len_delta
            )),
        }

        ProofVerdict::fail(defects, observations)
    }
}

fn normalise(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

/// Render a verdict as report-ready lines. Unverified proofs name their
/// defects rather than quietly passing.
pub fn describe_verdict(subject: &str, verdict: &ProofVerdict) -> String {
    let mut out = String::new();
    if verdict.verified {
        out.push_str(&format!("VERIFIED: {subject}\n"));
    } else {
        out.push_str(&format!("UNVERIFIED: {subject}\n"));
    }
    for observation in &verdict.observations {
        out.push_str(&format!("  observed: {observation}\n"));
    }
    for defect in &verdict.defects {
        out.push_str(&format!("  missing: {}\n", defect.as_str()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exchange(method: &str, url: &str, status: u16, body: &str) -> HttpExchange {
        HttpExchange {
            method: method.to_string(),
            url: url.to_string(),
            request_body: None,
            status,
            response_headers: Vec::new(),
            response_body: Some(body.to_string()),
            source_tool: "httpflow".to_string(),
        }
    }

    /// Mirrors the shape `httpflow` actually emits: a status line, a URL line,
    /// zero or more header lines, a blank line, then the body.
    fn transcript(status_line: &str, url: &str, extra: &str, body: &str) -> String {
        let mut out = format!("{status_line}\nURL: {url}\n");
        if !extra.is_empty() {
            out.push_str(extra);
            out.push('\n');
        }
        out.push('\n');
        out.push_str(body);
        out.push('\n');
        out
    }

    // --- transcript parsing ------------------------------------------------

    #[test]
    fn parses_an_exchange_from_a_real_transcript() {
        let raw = transcript(
            "HTTP 403 Forbidden",
            "https://example.com/admin",
            "content-type: text/html\nset-cookie: session=abc",
            "<html>forbidden</html>",
        );
        let input = serde_json::json!({
            "method": "get",
            "url": "https://example.com/admin",
        });
        let parsed = HttpExchange::parse_http_transcript("httpflow", &input, &raw)
            .expect("a status line and URL should parse");

        assert_eq!(parsed.method, "GET");
        assert_eq!(parsed.url, "https://example.com/admin");
        assert_eq!(parsed.status, 403);
        assert_eq!(
            parsed.response_body.as_deref(),
            Some("<html>forbidden</html>")
        );
        assert!(
            parsed
                .response_headers
                .contains(&("content-type".to_string(), "text/html".to_string())),
            "content-type header should survive: {:?}",
            parsed.response_headers
        );
        assert!(
            parsed
                .response_headers
                .iter()
                .all(|(n, _)| n != "cookies set"),
            "tool prose must not be parsed as a header: {:?}",
            parsed.response_headers
        );
    }

    #[test]
    fn a_non_http_tool_result_yields_no_exchange() {
        let input = serde_json::json!({"url": "https://example.com"});
        // A failed tool call must never fabricate an observed exchange.
        assert!(
            HttpExchange::parse_http_transcript("httpflow", &input, "error: timed out").is_none()
        );
        assert!(HttpExchange::parse_http_transcript("httpflow", &input, "").is_none());
        // A banner that merely starts with HTTP must not be read as a status.
        assert!(
            HttpExchange::parse_http_transcript("httpflow", &input, "HTTPS 2000 OK\n").is_none()
        );
    }

    #[test]
    fn request_body_is_carried_over_from_the_tool_input() {
        let raw = transcript(
            "HTTP 500 Internal Server Error",
            "https://example.com/x",
            "",
            "err",
        );
        let input = serde_json::json!({
            "method": "POST",
            "url": "https://example.com/x",
            "body": "{\"q\":\"' OR 1=1--\"}",
        });
        let parsed =
            HttpExchange::parse_http_transcript("httpflow", &input, &raw).expect("should parse");
        assert_eq!(parsed.method, "POST");
        assert_eq!(
            parsed.request_body.as_deref(),
            Some("{\"q\":\"' OR 1=1--\"}")
        );
    }

    // --- status semantics --------------------------------------------------

    #[test]
    fn a_bare_200_is_not_informative_but_a_sensitive_200_is() {
        assert_eq!(
            StatusClass::classify(200, "https://example.com/about"),
            StatusClass::RoutineSuccess
        );
        // RoutineSuccess is never informative — the variant *means* the path
        // exposed nothing sensitive, so there is nothing to report.
        assert!(!StatusClass::RoutineSuccess.is_informative());

        // A 2xx on a sensitive path is classified differently, not re-asked.
        assert_eq!(
            StatusClass::classify(200, "https://example.com/admin/config"),
            StatusClass::SensitiveSuccess
        );
        assert!(StatusClass::SensitiveSuccess.is_informative());
    }

    #[test]
    fn interesting_statuses_are_informative_on_any_path() {
        for status in [301, 302, 401, 403, 500, 502, 429] {
            let class = StatusClass::classify(status, "https://example.com/about");
            assert!(
                class.is_informative(),
                "status {status} classified as {} should be informative",
                class.as_str()
            );
        }
        assert_eq!(StatusClass::classify(403, "/x"), StatusClass::AccessDenied);
        assert_eq!(StatusClass::classify(500, "/x"), StatusClass::ServerError);
        assert_eq!(StatusClass::classify(302, "/x"), StatusClass::Redirect);
    }

    // --- baseline differential --------------------------------------------

    #[test]
    fn a_status_change_is_always_material() {
        // 404 vs 403 is the classic existence oracle.
        let baseline = exchange("GET", "https://example.com/nope", 404, "not found");
        let treatment = exchange("GET", "https://example.com/admin", 403, "forbidden");
        let comparison = BaselineComparison::new("sibling path", baseline, treatment);
        assert!(comparison.is_material());
        assert!(comparison.difference.status_changed);
        assert_eq!(comparison.difference.status_delta, Some(-1));
    }

    #[test]
    fn identical_responses_are_not_material() {
        let baseline = exchange(
            "GET",
            "https://example.com/a",
            200,
            "<html>same page</html>",
        );
        let treatment = exchange(
            "GET",
            "https://example.com/b",
            200,
            "<html>same page</html>",
        );
        let comparison = BaselineComparison::new("sibling", baseline, treatment);
        assert!(
            !comparison.is_material(),
            "{comparison:?} should be immaterial"
        );
    }

    #[test]
    fn a_trivial_body_jitter_is_not_material() {
        let baseline = exchange("GET", "https://example.com/a", 200, &"x".repeat(1_000));
        // 40 bytes longer on a 1000-byte page is rendering noise, not exposure.
        let treatment = exchange("GET", "https://example.com/a", 200, &"x".repeat(1_040));
        let comparison = BaselineComparison::new("same path", baseline, treatment);
        assert!(
            !comparison.is_material(),
            "a 4% body change should not count as exposure"
        );
    }

    #[test]
    fn a_large_body_growth_is_material() {
        let baseline = exchange("GET", "https://example.com/a", 200, &"x".repeat(100));
        let treatment = exchange("GET", "https://example.com/a", 200, &"x".repeat(2_000));
        let comparison = BaselineComparison::new("unauthenticated", baseline, treatment);
        assert!(comparison.is_material());
    }

    // --- the gate ----------------------------------------------------------

    fn complete_proof() -> ProofObject {
        let baseline = exchange("GET", "https://example.com/api/search", 403, "forbidden");
        let treatment = exchange(
            "GET",
            "https://example.com/api/search?q=%27%20OR%201=1--",
            500,
            &"SQL syntax error near 'OR'".repeat(20),
        );
        ProofObject {
            finding_id: "f1".into(),
            test_exchange: Some(treatment.clone()),
            reproduction: vec![ProofStep {
                action: "GET /api/search?q=%27%20OR%201=1--".into(),
                expected_observation: "a SQL error, or a result set".into(),
                tool: Some("httpflow".into()),
            }],
            expected_behavior: "the search rejects the malformed query with 400".into(),
            observed_behavior: "the server returned 500 with a SQL syntax error".into(),
            baseline: Some(BaselineComparison::new(
                "unauthenticated",
                baseline,
                treatment,
            )),
        }
    }

    #[test]
    fn a_complete_proof_verifies() {
        let proof = complete_proof();
        let verdict = proof.verify();
        assert!(verdict.verified, "defects: {:?}", verdict.defects);
        assert!(verdict.defects.is_empty());
        assert!(
            verdict
                .observations
                .iter()
                .any(|o| o.contains("returned 500"))
        );
    }

    /// One (label, expected defect, mutation) triple for the fail-closed table.
    type DefectCase = (&'static str, ProofDefect, fn(&mut ProofObject));

    /// The gate has to fail closed. Every one of these removals must break
    /// verification.
    #[test]
    fn removing_any_requirement_breaks_verification() {
        fn drop_exchange(p: &mut ProofObject) {
            p.test_exchange = None;
        }
        fn drop_reproduction(p: &mut ProofObject) {
            p.reproduction.clear();
        }
        fn drop_expected(p: &mut ProofObject) {
            p.expected_behavior = String::new();
        }
        fn drop_observed(p: &mut ProofObject) {
            p.observed_behavior = String::new();
        }
        fn drop_baseline(p: &mut ProofObject) {
            p.baseline = None;
        }

        let cases: Vec<DefectCase> = vec![
            (
                "no exchange",
                ProofDefect::MissingTestExchange,
                drop_exchange,
            ),
            (
                "no reproduction",
                ProofDefect::MissingReproductionSteps,
                drop_reproduction,
            ),
            (
                "no expected",
                ProofDefect::MissingExpectedBehavior,
                drop_expected,
            ),
            (
                "no observed",
                ProofDefect::MissingObservedBehavior,
                drop_observed,
            ),
            ("no baseline", ProofDefect::MissingBaseline, drop_baseline),
        ];

        for (label, expected, mutate) in cases {
            let mut proof = complete_proof();
            mutate(&mut proof);
            let verdict = proof.verify();
            assert!(
                !verdict.verified,
                "a proof with {label} must not verify, got {verdict:?}"
            );
            assert!(
                verdict.defects.contains(&expected),
                "a proof with {label} should report {expected:?}, got {:?}",
                verdict.defects
            );
        }
    }

    /// The claim "SQLi in /search" is the exact false positive this gate
    /// exists to kill: nothing was sent, so nothing can be verified.
    #[test]
    fn an_empty_proof_never_verifies() {
        let verdict = ProofObject::new("f1").verify();
        assert!(!verdict.verified);
        assert!(verdict.defects.contains(&ProofDefect::MissingTestExchange));
    }

    #[test]
    fn observed_matching_expected_is_not_a_finding() {
        let mut proof = complete_proof();
        proof.observed_behavior =
            "   The Search Rejects   the malformed query with 400 ".to_string();
        proof.expected_behavior = "the search rejects the malformed query with 400".to_string();
        let verdict = proof.verify();
        assert!(
            !verdict.verified,
            "behaviour identical to expectation is not a finding: {verdict:?}"
        );
        assert!(
            verdict
                .defects
                .contains(&ProofDefect::BehaviorMatchesExpectation)
        );
    }

    #[test]
    fn a_routine_200_on_an_ordinary_path_does_not_verify() {
        let baseline = exchange("GET", "https://example.com/about", 200, "short");
        let treatment = exchange("GET", "https://example.com/nope", 200, &"x".repeat(500));
        let mut proof = complete_proof();
        proof.test_exchange = Some(treatment.clone());
        proof.baseline = Some(BaselineComparison::new("sibling", baseline, treatment));

        let verdict = proof.verify();
        assert!(!verdict.verified, "{verdict:?}");
        assert!(verdict.defects.contains(&ProofDefect::UninformativeStatus));
    }

    /// A proof describing a request that was never made must not pass just
    /// because it is internally consistent.
    #[test]
    fn a_proof_for_an_unobserved_request_is_rejected() {
        let proof = complete_proof();
        let observed = vec![exchange(
            "GET",
            "https://example.com/elsewhere",
            500,
            "other",
        )];
        let verdict = proof.verify_against(&observed);
        assert!(!verdict.verified);
        assert!(verdict.defects.contains(&ProofDefect::ExchangeNotObserved));
    }

    /// The cross-check is fail-closed: an *empty* history is not "skip the
    /// check", it is "no evidence the request was sent".
    #[test]
    fn an_empty_history_cannot_confirm_a_request() {
        let proof = complete_proof();
        let verdict = proof.verify_against(&[]);
        assert!(
            !verdict.verified,
            "with nothing observed, nothing can be confirmed"
        );
        assert!(verdict.defects.contains(&ProofDefect::ExchangeNotObserved));
    }

    #[test]
    fn a_proof_matches_the_observed_history_it_cites() {
        let proof = complete_proof();
        let observed = vec![proof.test_exchange.clone().expect("proof has an exchange")];
        let verdict = proof.verify_against(&observed);
        assert!(verdict.verified, "defects: {:?}", verdict.defects);
    }

    #[test]
    fn sensitive_path_detection() {
        assert!(is_sensitive_path("/admin/users"));
        assert!(is_sensitive_path("/actuator/env"));
        assert!(is_sensitive_path("/.env"));
        assert!(is_sensitive_path("/graphql"));
        assert!(!is_sensitive_path("/about/team"));
        assert!(!is_sensitive_path("/blog/post-1"));
    }

    #[test]
    fn an_unverified_verdict_names_its_defects() {
        let verdict = ProofObject::new("f1").verify();
        let text = describe_verdict("SQLi in /search", &verdict);
        assert!(text.starts_with("UNVERIFIED: SQLi in /search"));
        assert!(text.contains("missing: no observed request/response"));
    }
}
