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
                let mut start = 0;
                while let Some(pos) = out[start..].find(&format!("{key}=")) {
                    let abs = start + pos + key.len() + 1;
                    let end = out[abs..]
                        .find(['&', '#', ' '])
                        .map(|i| abs + i)
                        .unwrap_or(out.len());
                    out.replace_range(abs..end, "[REDACTED]");
                    start = abs + "[REDACTED]".len();
                }
            }
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
                _ => {}
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
}
