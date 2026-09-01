//! Web-safety heuristics: catch the FDJ-04 anti-pattern before the request fires.
//!
//! # Why this exists
//!
//! During a real security audit, a model was about to be used to
//! "audit deeper" with a user's pasted session cookie jar. The
//! request would have been:
//!
//!     curl -H "Cookie: access_token=<real-token>" https://api.target.com/...
//!
//! and the response would have been 200 with the user's own data —
//! which a triager would correctly close as N/A, not a vulnerability.
//! The session was about to be leaked into the chat transcript as a
//! side effect.
//!
//! # What this catches
//!
//! Before any `webfetch` / `web_request` / `curl` runs, scan the URL + header
//! block for:
//!
//! - Bearer tokens (e.g. `Bearer eyJ...`, `Authorization: Bearer ...`)
//! - OAuth `access_token` / `id_token` / `refresh_token` cookies
//! - Session cookies (Cookie: session=..., PHPSESSID=..., JSESSIONID=...,
//!   sp=..., _session=..., etc.)
//! - AWS / Stripe / SendGrid API keys
//!
//! If any are present, refuse with a `Confirm`-level prompt that asks the
//! model to justify, mirroring the existing `bash_destructive_gate` policy
//! seam. The model is told:
//!
//! 1. What the detected token looks like (length-prefixed, never the full
//!    value) so it can confirm.
//! 2. That pasting a real session into a tool call is a reportable rule
//!    violation on most programs and will burn the user's account if the
//!    transcript is leaked.
//! 3. That the correct IDOR/auth-bypass PoC is the user's own session +
//!    a forged customerId, not the victim's session.
//!
//! # What this does NOT do
//!
//! - It does not block tokens outright. The model may legitimately need to
//!   send an Authorization header to its own services. The gate returns
//!   `Confirm` (reflection prompt), not `Deny`.
//! - It does not log or store the token. We hash-and-prefix for the
//!   diagnostic message and never write the secret to disk.
//!
//! # Companion to `bash_destructive_gate`
//!
//! Same shape, different surface. `bash_destructive_gate` blocks
//! `rm -rf /`; this one blocks `curl -H 'Cookie: session=...'`. Both are
//! deterministic, pre-execution, no-network, no-model-call.

use sha2::{Digest, Sha256};

/// The kinds of secrets this gate looks for. Used in the diagnostic
/// message so the model knows what was found without revealing the value.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    /// `Authorization: Bearer ...` or `Authorization: Token ...`
    BearerToken,
    /// Cookie containing `access_token=`, `id_token=`, `refresh_token=`,
    /// `PHPSESSID=`, `JSESSIONID=`, `sp=`, `_session=`, `__Secure-...`
    SessionCookie,
    /// AWS access key (`AKIA[0-9A-Z]{16}`) or secret
    AwsKey,
    /// Stripe live or test key
    StripeKey,
    /// `sk_live_...`, `pk_live_...`, `rk_live_...`
    /// (test keys are also flagged because they signal a real account)
    /// Generic API key (`api_key=...`, `apikey=...`, `x-api-key: ...`)
    GenericApiKey,
}

impl SecretKind {
    pub fn as_label(self) -> &'static str {
        match self {
            SecretKind::BearerToken => "Bearer token",
            SecretKind::SessionCookie => "session cookie",
            SecretKind::AwsKey => "AWS access key",
            SecretKind::StripeKey => "Stripe key",
            SecretKind::GenericApiKey => "API key",
        }
    }
}

/// A detection result. `preview` is the first 6 chars + `…` so the model
/// can recognize it. The full value is never stored.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SecretHit {
    pub kind: SecretKind,
    pub preview: String,
    /// Where in the input the hit was found. Used for the diagnostic, not
    /// for indexing the secret.
    pub location: &'static str,
}

/// The verdict. `Clean` means the input is safe to fire. `Confirm` means
/// the model must justify, mirroring `bash_destructive_gate`'s
/// `Reflect` outcome. We never return `Deny` here because the use case
/// is sometimes legitimate; we just want the model to slow down.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum WebSafetyVerdict {
    Clean,
    #[allow(dead_code)]
    Confirm {
        prompt: String,
        hits: Vec<SecretHit>,
    },
}

impl WebSafetyVerdict {
    #[allow(dead_code)]
    pub fn runs_immediately(&self) -> bool {
        matches!(self, WebSafetyVerdict::Clean)
    }
}

const MAX_INPUT_LEN: usize = 16 * 1024;

/// Scan a URL + accompanying header blob for pasted secrets. The
/// header blob can be free-form (whatever the user pasted alongside
/// the URL), or the contents of a `-H`, `-b`, `-F` curl flag.
pub fn scan_for_pasted_secrets(url: &str, headers_blob: &str) -> WebSafetyVerdict {
    if url.len() + headers_blob.len() > MAX_INPUT_LEN {
        // Too large to scan safely. Err on the side of asking.
        return WebSafetyVerdict::Confirm {
            prompt: "URL+headers blob exceeds 16KB; cannot pre-scan for pasted \
                     secrets. Re-issue with a smaller input or split into \
                     multiple calls.".to_string(),
            hits: Vec::new(),
        };
    }

    let mut hits: Vec<SecretHit> = Vec::new();

    // 1. Bearer token in headers
    if let Some(hit) = detect_bearer(headers_blob) {
        hits.push(hit);
    }
    if let Some(hit) = detect_bearer(url) {
        hits.push(hit);
    }

    // 2. Session cookies
    if let Some(hit) = detect_session_cookie(headers_blob) {
        hits.push(hit);
    }
    if let Some(hit) = detect_session_cookie(url) {
        hits.push(hit);
    }

    // 3. AWS keys
    if let Some(hit) = detect_aws_key(headers_blob) {
        hits.push(hit);
    }
    if let Some(hit) = detect_aws_key(url) {
        hits.push(hit);
    }

    // 4. Stripe keys
    if let Some(hit) = detect_stripe_key(headers_blob) {
        hits.push(hit);
    }
    if let Some(hit) = detect_stripe_key(url) {
        hits.push(hit);
    }

    // 5. Generic API keys
    if let Some(hit) = detect_generic_api_key(headers_blob) {
        hits.push(hit);
    }
    if let Some(hit) = detect_generic_api_key(url) {
        hits.push(hit);
    }

    if hits.is_empty() {
        return WebSafetyVerdict::Clean;
    }

    let prompt = build_prompt(&hits);
    WebSafetyVerdict::Confirm { prompt, hits }
}

fn detect_bearer(blob: &str) -> Option<SecretHit> {
    // `Authorization: Bearer <token>` or `Authorization: Token <token>`
    let lower = blob.to_ascii_lowercase();
    if let Some(idx) = lower.find("authorization:") {
        let rest = &blob[idx + "authorization:".len()..];
        let trimmed = rest.trim_start();
        let kind = if trimmed.to_ascii_lowercase().starts_with("bearer ")
            || trimmed.to_ascii_lowercase().starts_with("token ")
        {
            SecretKind::BearerToken
        } else {
            return None;
        };
        let token = trimmed.split_whitespace().nth(1)?;
        return Some(SecretHit {
            kind,
            preview: preview(token),
            location: "header",
        });
    }
    None
}

fn detect_session_cookie(blob: &str) -> Option<SecretHit> {
    // Look for `Cookie:` header containing known session-cookie names.
    let lower = blob.to_ascii_lowercase();
    let cookie_idx = lower.find("cookie:")?;
    let after = &blob[cookie_idx + "cookie:".len()..];
    // Take until next newline or end
    let cookie_line = after.lines().next().unwrap_or(after);
    let names = [
        "access_token=", "id_token=", "refresh_token=",
        "phpsessid=", "jsessionid=", "asp.net_sessionid=",
        "sp=", "_session=", "session=", "__secure-session=",
        "sid=", "auth=", "token=", "lt=", "rt=",
        // Spring / OAuth common
        "remember-me=", "sess=",
    ];
    for name in names {
        if let Some(pos) = cookie_line.to_ascii_lowercase().find(name) {
            // Find the value end (next ; or end-of-string)
            let after_name = &cookie_line[pos + name.len()..];
            let value_end = after_name.find(';').unwrap_or(after_name.len());
            let value = &after_name[..value_end];
            if value.len() >= 8 {
                return Some(SecretHit {
                    kind: SecretKind::SessionCookie,
                    preview: preview(value),
                    location: "header",
                });
            }
        }
    }
    None
}

fn detect_aws_key(blob: &str) -> Option<SecretHit> {
    // AWS access key: AKIA[0-9A-Z]{16}
    let bytes = blob.as_bytes();
    let needle = b"AKIA";
    let mut i = 0;
    while i + 20 <= bytes.len() {
        if &bytes[i..i + 4] == needle
            && bytes[i + 4..i + 20].iter().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            let key = std::str::from_utf8(&bytes[i..i + 20]).ok()?;
            return Some(SecretHit {
                kind: SecretKind::AwsKey,
                preview: preview(key),
                location: "input",
            });
        }
        i += 1;
    }
    None
}

fn detect_stripe_key(blob: &str) -> Option<SecretHit> {
    for prefix in &["sk_live_", "pk_live_", "rk_live_", "sk_test_", "pk_test_"] {
        if let Some(idx) = blob.find(prefix) {
            // Stripe keys are 32+ chars after the prefix
            let after = &blob[idx + prefix.len()..];
            let value: String = after.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            if value.len() >= 24 {
                return Some(SecretHit {
                    kind: SecretKind::StripeKey,
                    preview: preview(&format!("{}{}", prefix, value)),
                    location: "input",
                });
            }
        }
    }
    None
}

fn detect_generic_api_key(blob: &str) -> Option<SecretHit> {
    let patterns = [
        "api_key=", "apikey=", "x-api-key:", "api-key:",
        "x-auth-token:", "auth-token:", "x-token:",
        "access-key:", "secret-key:",
    ];
    let lower = blob.to_ascii_lowercase();
    for pat in patterns {
        if let Some(idx) = lower.find(pat) {
            let after = &blob[idx + pat.len()..].trim_start();
            let value: String = after.chars().take_while(|c| !c.is_whitespace() && *c != '"' && *c != '\'').collect();
            if value.len() >= 12 {
                return Some(SecretHit {
                    kind: SecretKind::GenericApiKey,
                    preview: preview(&value),
                    location: "header",
                });
            }
        }
    }
    None
}

fn preview(s: &str) -> String {
    if s.len() <= 6 {
        return s.to_string();
    }
    let head: String = s.chars().take(6).collect();
    format!("{head}…({} chars)", s.len())
}

fn build_prompt(hits: &[SecretHit]) -> String {
    let mut s = String::from(
        "Detected a secret that looks like a real session or API key in the \
         URL or headers of this request. Before I send it, please confirm:\n\n\
         1. This is YOUR OWN session or service-account key, not a victim's.\n\
         2. The receiving service is in the authorized test scope.\n\
         3. The key is not a session cookie from another user's browser.\n\n\
         If this is an auth-replay attempt: STOP. Replay of another user's \
         session against a session-required endpoint is not a vulnerability. \
         The correct PoC is your own session with a forged \
         customerId/accountId.\n\n\
         Detected: ",
    );
    for (i, hit) in hits.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&format!("{} `{}`", hit.kind.as_label(), hit.preview));
    }
    s.push('.');
    s
}

/// Hash a secret for logging without revealing it. Used by the bash /
/// webfetch tools to record a fingerprint of what was sent, never the
/// secret itself.
#[allow(dead_code)]
pub fn secret_fingerprint(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let out = hasher.finalize();
    let hex: String = out.iter().take(6).map(|b| format!("{:02x}", b)).collect();
    format!("sha256:{}…", hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_input_passes() {
        let v = scan_for_pasted_secrets(
            "https://api.example.com/users/1",
            "User-Agent: my-agent",
        );
        assert!(v.runs_immediately());
    }

    #[test]
    fn bearer_token_in_header_triggers_confirm() {
        let v = scan_for_pasted_secrets(
            "https://api.example.com/me",
            "Authorization: Bearer eyJabc123def456ghi789jkl012mno345pqr",
        );
        match v {
            WebSafetyVerdict::Confirm { hits, prompt } => {
                assert_eq!(hits.len(), 1);
                assert_eq!(hits[0].kind, SecretKind::BearerToken);
                assert!(prompt.contains("Detected"));
                assert!(prompt.contains("Bearer token"));
            }
            _ => panic!("expected Confirm, got Clean"),
        }
    }

    #[test]
    fn session_cookie_triggers_confirm() {
        // The exact FDJ-04 pattern: a real-looking access_token cookie.
        let v = scan_for_pasted_secrets(
            "https://www.unibet.com.au/wallitt/mainbalance",
            "Cookie: access_token=Q8hygLZcTbKZi_SRg_orNA:cdnGB6f1QRKaDY4xJAgJIg",
        );
        match v {
            WebSafetyVerdict::Confirm { hits, .. } => {
                assert_eq!(hits[0].kind, SecretKind::SessionCookie);
            }
            _ => panic!("expected Confirm, got Clean"),
        }
    }

    #[test]
    fn aws_key_in_url_triggers_confirm() {
        let v = scan_for_pasted_secrets(
            "https://s3.amazonaws.com/?key=AKIAIOSFODNN7EXAMPLE",
            "",
        );
        match v {
            WebSafetyVerdict::Confirm { hits, .. } => {
                assert_eq!(hits[0].kind, SecretKind::AwsKey);
            }
            _ => panic!("expected Confirm, got Clean"),
        }
    }

    #[test]
    fn stripe_live_key_triggers_confirm() {
        let v = scan_for_pasted_secrets(
            "https://api.stripe.com/v1/charges",
            "Authorization: Bearer sk_live_4eC39HqLyjWDarjtT1zdp7dc",
        );
        match v {
            WebSafetyVerdict::Confirm { hits, .. } => {
                assert!(hits.iter().any(|h| h.kind == SecretKind::StripeKey));
            }
            _ => panic!("expected Confirm, got Clean"),
        }
    }

    #[test]
    fn secret_fingerprint_is_stable() {
        let a = secret_fingerprint("hello");
        let b = secret_fingerprint("hello");
        assert_eq!(a, b);
        let c = secret_fingerprint("world");
        assert_ne!(a, c);
    }

    #[test]
    fn preview_truncates_long_secrets() {
        let p = preview("eyJabc123def456ghi789jkl012mno345pqr");
        assert!(p.contains('…'));
        assert!(!p.contains("pqr")); // full value not in preview
    }

    #[test]
    fn no_false_positive_on_short_session_values() {
        // A 4-char `sp=ABCD` should not trigger; sessions are >= 8 chars
        let v = scan_for_pasted_secrets(
            "https://example.com/",
            "Cookie: sp=ABCD",
        );
        assert!(v.runs_immediately());
    }
}
