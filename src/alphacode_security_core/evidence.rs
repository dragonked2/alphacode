use serde::{Deserialize, Serialize};

/// Structured evidence for a confirmed finding.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub finding_id: String,
    pub evidence_items: Vec<EvidenceItem>,
    pub reproduction_trace: Vec<ReproductionStep>,
    pub metadata: EvidenceMetadata,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvidenceItem {
    pub id: String,
    pub kind: EvidenceKind,
    pub description: String,
    pub data: EvidenceData,
    pub collected_by: Option<String>,
    pub timestamp: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EvidenceKind {
    Request,
    Response,
    Screenshot,
    Diff,
    Payload,
    ErrorOutput,
    ToolOutput,
    Analysis,
    Chain,
}

impl EvidenceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Response => "response",
            Self::Screenshot => "screenshot",
            Self::Diff => "diff",
            Self::Payload => "payload",
            Self::ErrorOutput => "error_output",
            Self::ToolOutput => "tool_output",
            Self::Analysis => "analysis",
            Self::Chain => "chain",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EvidenceData {
    Text(String),
    HttpRequest {
        method: String,
        url: String,
        headers: Vec<(String, String)>,
        body: Option<String>,
    },
    HttpResponse {
        status: u16,
        headers: Vec<(String, String)>,
        body: Option<String>,
    },
    Binary {
        data: Vec<u8>,
        mime_type: String,
    },
    Structured(serde_json::Value),
}

/// A single step in the reproduction trace.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReproductionStep {
    pub step_number: u32,
    pub action: String,
    pub tool_used: Option<String>,
    pub input: Option<String>,
    pub output: Option<String>,
    pub screenshot_path: Option<String>,
    pub timestamp: String,
}

/// Metadata about the evidence collection.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvidenceMetadata {
    pub collected_by: Option<String>,
    pub collection_method: String,
    pub total_steps: u32,
    pub environment: Option<String>,
    pub timestamp: String,
}

impl Evidence {
    pub fn new(finding_id: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            finding_id,
            evidence_items: Vec::new(),
            reproduction_trace: Vec::new(),
            metadata: EvidenceMetadata {
                collected_by: None,
                collection_method: "manual".to_string(),
                total_steps: 0,
                environment: None,
                timestamp: now,
            },
        }
    }

    pub fn add_item(&mut self, item: EvidenceItem) {
        self.evidence_items.push(item);
    }

    pub fn add_step(&mut self, step: ReproductionStep) {
        self.reproduction_trace.push(step);
        self.metadata.total_steps = self.reproduction_trace.len() as u32;
    }

    pub fn has_request_response(&self) -> bool {
        self.evidence_items
            .iter()
            .any(|e| matches!(e.data, EvidenceData::HttpRequest { .. }))
            && self
                .evidence_items
                .iter()
                .any(|e| matches!(e.data, EvidenceData::HttpResponse { .. }))
    }

    /// Pair requests with responses by insertion order.
    ///
    /// Items carry no correlation IDs, so pairing is positional: the nth
    /// request pairs with the nth response. Extra items on either side are
    /// dropped. Callers needing exact correlation should store request and
    /// response as adjacent items.
    pub fn request_response_pairs(&self) -> Vec<(&EvidenceItem, &EvidenceItem)> {
        let requests: Vec<&EvidenceItem> = self
            .evidence_items
            .iter()
            .filter(|e| matches!(e.data, EvidenceData::HttpRequest { .. }))
            .collect();
        let responses: Vec<&EvidenceItem> = self
            .evidence_items
            .iter()
            .filter(|e| matches!(e.data, EvidenceData::HttpResponse { .. }))
            .collect();
        requests.into_iter().zip(responses).collect()
    }

    /// Redact sensitive fields (cookies, auth headers, tokens, URL secrets).
    pub fn redacted(&self) -> Self {
        fn is_sensitive_header(name: &str) -> bool {
            let lower = name.to_lowercase();
            lower.contains("authorization")
                || lower.contains("cookie")
                || lower.contains("set-cookie")
                || lower.contains("token")
                || lower.contains("api-key")
                || lower.contains("secret")
                || lower.contains("password")
                || lower.contains("session")
                || lower == "auth"
        }

        fn redact_url(url: &str) -> String {
            let mut out = url.to_string();
            for key in [
                "token", "api_key", "apikey", "session", "secret", "password", "auth",
            ] {
                // Match case-insensitively. Header names are already matched
                // case-insensitively, and HTTP query keys are case-sensitive in
                // practice, so `?Token=` / `?API_KEY=` were slipping through
                // while the identically-named header was redacted.
                let needle = format!("{key}=");
                let needle_lower = needle.to_ascii_lowercase();
                let mut start = 0usize;
                while start <= out.len() {
                    let hay = &out[start..];
                    let Some(found) = hay.to_ascii_lowercase().find(&needle_lower) else {
                        break;
                    };
                    let abs = start + found + needle.len();
                    let end = out[abs..]
                        .find(['&', '#', ' ', '\'', '"'])
                        .map(|i| abs + i)
                        .unwrap_or(out.len());
                    out.replace_range(abs..end, "[REDACTED]");
                    start = abs + "[REDACTED]".len();
                }
            }
            out
        }

        /// Scrub a raw transcript: header lines, bearer/basic credentials, and
        /// any URL query secrets embedded in request lines.
        fn redact_transcript(text: &str) -> String {
            let mut out = redact_body_text(text);
            // Header lines of the form `Name: value`.
            let mut rebuilt = String::with_capacity(out.len());
            for line in out.split_inclusive('\n') {
                let trimmed = line.trim_end_matches(['\r', '\n']);
                let is_header = trimmed
                    .split_once(':')
                    .is_some_and(|(name, _)| is_sensitive_header(name));
                if is_header {
                    let (name, _) = trimmed.split_once(':').expect("checked above");
                    rebuilt.push_str(name);
                    rebuilt.push_str(": [REDACTED]");
                    if line.ends_with('\n') {
                        rebuilt.push('\n');
                    }
                } else {
                    rebuilt.push_str(&redact_url(trimmed));
                    rebuilt.push_str(&line[trimmed.len()..]);
                }
            }
            out = rebuilt;
            out
        }

        fn redact_body_text(body: &str) -> String {
            let mut out = body.to_string();
            for prefix in ["Bearer ", "Basic "] {
                let mut start = 0;
                while let Some(pos) = out[start..].find(prefix) {
                    let abs = start + pos + prefix.len();
                    let end = out[abs..]
                        .find(['"', '\'', ' ', '\n', '\r', '&'])
                        .map(|i| abs + i)
                        .unwrap_or(out.len());
                    if end > abs {
                        out.replace_range(abs..end, "[REDACTED]");
                        start = abs + "[REDACTED]".len();
                    } else {
                        break;
                    }
                }
            }
            out
        }

        let mut redacted = self.clone();
        for item in &mut redacted.evidence_items {
            match &mut item.data {
                EvidenceData::HttpRequest {
                    url, headers, body, ..
                } => {
                    *url = redact_url(url);
                    for (name, value) in headers.iter_mut() {
                        if is_sensitive_header(name) {
                            *value = "[REDACTED]".to_string();
                        }
                    }
                    if let Some(b) = body {
                        *b = redact_body_text(b);
                    }
                }
                EvidenceData::HttpResponse { headers, body, .. } => {
                    for (name, value) in headers.iter_mut() {
                        if is_sensitive_header(name) {
                            *value = "[REDACTED]".to_string();
                        }
                    }
                    if let Some(b) = body {
                        *b = redact_body_text(b);
                    }
                }
                EvidenceData::Structured(v) => {
                    let s = v.to_string().to_lowercase();
                    if s.contains("token") || s.contains("secret") || s.contains("password") {
                        *v =
                            serde_json::Value::String("[REDACTED STRUCTURED EVIDENCE]".to_string());
                    }
                }
                // `Text` is how raw HTTP transcripts, curl output and tool
                // stdout usually land in an evidence pipeline, and it was left
                // completely untouched. A `Cookie:`/`Authorization:` line in an
                // exported report is a live credential leak, and `redacted()`
                // is the only sanitiser in the pipeline. Raw text gets the
                // header- and bearer-scrubbing pass plus a URL-parameter pass,
                // because a transcript can contain either.
                EvidenceData::Text(t) => {
                    *t = redact_transcript(t);
                }
                // Binary payloads (archives, PDFs, exported HARs) routinely
                // embed credentials. We cannot parse them, so the bytes are
                // dropped outright and only a size marker survives.
                //
                // This arm used to be `*mime_type = mime_type.clone()`, which is
                // a self-assignment: the comment promised the bytes were being
                // removed and the code kept them, so every redacted report still
                // carried the original attachment. Redaction that does not
                // redact is worse than no redaction, because it is trusted.
                EvidenceData::Binary { data, mime_type } => {
                    *mime_type = format!("{mime_type} ({} bytes redacted)", data.len());
                    data.clear();
                    data.shrink_to_fit();
                }
            }
        }
        redacted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_request_response_pairs() {
        let mut evidence = Evidence::new("f1".to_string());
        evidence.add_item(EvidenceItem {
            id: "req1".to_string(),
            kind: EvidenceKind::Request,
            description: "GET /api".to_string(),
            data: EvidenceData::HttpRequest {
                method: "GET".to_string(),
                url: "https://example.com/api".to_string(),
                headers: vec![],
                body: None,
            },
            collected_by: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        evidence.add_item(EvidenceItem {
            id: "res1".to_string(),
            kind: EvidenceKind::Response,
            description: "200 OK".to_string(),
            data: EvidenceData::HttpResponse {
                status: 200,
                headers: vec![],
                body: Some("data".to_string()),
            },
            collected_by: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        assert!(evidence.has_request_response());
        assert_eq!(evidence.request_response_pairs().len(), 1);
    }

    #[test]
    fn evidence_redaction() {
        let mut evidence = Evidence::new("f1".to_string());
        evidence.add_item(EvidenceItem {
            id: "req1".to_string(),
            kind: EvidenceKind::Request,
            description: "GET /api".to_string(),
            data: EvidenceData::HttpRequest {
                method: "GET".to_string(),
                url: "https://example.com/api".to_string(),
                headers: vec![
                    ("Authorization".to_string(), "Bearer secret123".to_string()),
                    ("Content-Type".to_string(), "application/json".to_string()),
                ],
                body: None,
            },
            collected_by: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        let redacted = evidence.redacted();
        if let EvidenceData::HttpRequest { headers, .. } = &redacted.evidence_items[0].data {
            assert_eq!(
                headers
                    .iter()
                    .find(|(n, _)| n == "Authorization")
                    .unwrap()
                    .1,
                "[REDACTED]"
            );
            assert_eq!(
                headers.iter().find(|(n, _)| n == "Content-Type").unwrap().1,
                "application/json"
            );
        }
    }

    /// Regression: `EvidenceData::Text` fell through the redaction match
    /// entirely. Text is how a raw HTTP transcript, a curl dump or tool stdout
    /// normally reaches the evidence pipeline, so this shipped live
    /// credentials into any exported report.
    #[test]
    fn text_evidence_is_redacted() {
        let mut evidence = Evidence::new("f1".to_string());
        evidence.add_item(EvidenceItem {
            id: "raw1".to_string(),
            kind: EvidenceKind::ToolOutput,
            description: "raw request transcript".to_string(),
            data: EvidenceData::Text(
                "GET /v1/me?api_key=AKIAIOSFODNN7EXAMPLE HTTP/1.1\n\
                 Host: example.com\n\
                 Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload.sig\n\
                 Cookie: session=abc123\n\
                 Accept: application/json\n"
                    .to_string(),
            ),
            collected_by: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });

        let redacted = evidence.redacted();
        let EvidenceData::Text(text) = &redacted.evidence_items[0].data else {
            panic!("expected text evidence");
        };
        assert!(
            !text.contains("AKIAIOSFODNN7EXAMPLE"),
            "url api_key leaked: {text}"
        );
        assert!(
            !text.contains("eyJhbGciOiJIUzI1NiJ9"),
            "bearer token leaked: {text}"
        );
        assert!(!text.contains("abc123"), "cookie leaked: {text}");
        // Non-sensitive content must survive so the evidence stays useful.
        assert!(text.contains("example.com"), "host lost: {text}");
        assert!(text.contains("application/json"), "accept lost: {text}");
    }

    /// Header names are matched case-insensitively, so URL query keys must be
    /// too. Previously `?Token=` and `?API_KEY=` were not redacted.
    #[test]
    fn url_query_redaction_is_case_insensitive() {
        let mut evidence = Evidence::new("f1".to_string());
        evidence.add_item(EvidenceItem {
            id: "req1".to_string(),
            kind: EvidenceKind::Request,
            description: "GET".to_string(),
            data: EvidenceData::HttpRequest {
                method: "GET".to_string(),
                url: "https://example.com/a?Token=SECRETVALUE&API_KEY=ALSOSECRET&ok=1".to_string(),
                headers: vec![],
                body: None,
            },
            collected_by: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        let redacted = evidence.redacted();
        let EvidenceData::HttpRequest { url, .. } = &redacted.evidence_items[0].data else {
            panic!("expected request evidence");
        };
        assert!(!url.contains("SECRETVALUE"), "Token= leaked: {url}");
        assert!(!url.contains("ALSOSECRET"), "API_KEY= leaked: {url}");
        assert!(url.contains("ok=1"), "harmless param lost: {url}");
    }

    /// Regression: `redacted()` claimed to strip binary evidence payloads and
    /// did not. The arm was `*mime_type = mime_type.clone()`, a self-assignment,
    /// so every redacted report still carried the original attachment — with
    /// whatever credentials it embedded. A redactor that keeps the secret is
    /// worse than no redactor, because it is trusted.
    #[test]
    fn binary_evidence_bytes_are_actually_dropped() {
        let mut evidence = Evidence::new("f1".to_string());
        let secret = b"-----BEGIN PRIVATE KEY-----\nMIIEvQIBADAN\n".to_vec();
        evidence.add_item(EvidenceItem {
            id: "bin1".to_string(),
            kind: EvidenceKind::Request,
            description: "downloaded export".to_string(),
            data: EvidenceData::Binary {
                data: secret.clone(),
                mime_type: "application/octet-stream".to_string(),
            },
            collected_by: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });

        let redacted = evidence.redacted();
        let EvidenceData::Binary { data, mime_type } = &redacted.evidence_items[0].data else {
            panic!("expected binary evidence");
        };
        assert!(
            data.is_empty(),
            "binary payload survived redaction: {} bytes",
            data.len()
        );
        assert!(
            !String::from_utf8_lossy(data).contains("PRIVATE KEY"),
            "embedded key survived redaction"
        );
        // The size marker keeps the evidence honest about what was withheld.
        assert!(
            mime_type.contains(&secret.len().to_string()),
            "size marker lost: {mime_type}"
        );
        // The original is untouched: redaction is a view, not a mutation.
        let EvidenceData::Binary { data: original, .. } = &evidence.evidence_items[0].data else {
            panic!("expected binary evidence");
        };
        assert_eq!(original, &secret, "redacted() must not mutate its receiver");
    }
}
