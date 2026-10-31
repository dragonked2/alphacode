//! Tests for the OrcaRouter credential seam.
//!
//! Everything here uses fake keys and a loopback fake token endpoint; no test
//! ever contacts the real OrcaRouter service.

use super::*;
use crate::alphacode_base::auth::test_sandbox::AuthTestSandbox;

/// Restores `ORCA_*` process env after a test (the sandbox only tracks its own
/// list of variables).
struct OrcaEnvGuard {
    saved: Vec<(&'static str, Option<String>)>,
}

const ORCA_ENV_VARS: &[&str] = &[
    "ORCA_AUTH_BASE_URL",
    "ORCA_BASE_URL",
    "ORCA_API_BASE_URL",
    API_KEY_ENV,
];

impl OrcaEnvGuard {
    fn new() -> Self {
        let saved = ORCA_ENV_VARS
            .iter()
            .map(|name| {
                let value = std::env::var(name).ok();
                crate::alphacode_core::env::remove_var(name);
                (*name, value)
            })
            .collect();
        Self { saved }
    }
}

impl Drop for OrcaEnvGuard {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            match value {
                Some(value) => crate::alphacode_core::env::set_var(name, value),
                None => crate::alphacode_core::env::remove_var(name),
            }
        }
    }
}

#[test]
fn attempt_is_fresh_and_s256_challenge_matches_verifier() {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use sha2::{Digest, Sha256};

    let (first, _) = begin_attempt();
    let (second, _) = begin_attempt();

    assert_ne!(first.verifier, second.verifier, "verifier must be fresh");
    assert_ne!(first.state, second.state, "state must be fresh");
    assert_ne!(first.challenge, second.challenge);

    let mut hasher = Sha256::new();
    hasher.update(first.verifier.as_bytes());
    let expected = URL_SAFE_NO_PAD.encode(hasher.finalize());
    assert_eq!(first.challenge, expected);
    // base64url, no padding.
    assert!(!first.challenge.contains('='));
    assert!(!first.challenge.contains('+'));
    assert!(!first.challenge.contains('/'));
}

#[test]
fn attempt_debug_never_reveals_the_verifier() {
    let (attempt, _) = begin_attempt();
    let rendered = format!("{:?}", attempt);
    assert!(!rendered.contains(&attempt.verifier));
    assert!(rendered.contains("[redacted]"));
}

#[test]
fn authorize_url_carries_only_the_challenge_and_state() {
    let (attempt, _) = begin_attempt();
    let url = authorize_url(
        "https://www.orcarouter.ai",
        "http://127.0.0.1:51733/cb",
        &attempt,
    );
    assert!(url.starts_with("https://www.orcarouter.ai/auth?"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains(&format!("code_challenge={}", attempt.challenge)));
    assert!(url.contains(&format!("state={}", attempt.state)));
    assert!(url.contains("callback_url=http%3A%2F%2F127.0.0.1%3A51733%2Fcb"));
    assert!(url.contains("app_name=Alphacode"));
    assert!(url.contains("scope=api"));
    assert!(
        !url.contains(&attempt.verifier),
        "the verifier must never appear in a URL, not even the authorize URL"
    );
}

#[test]
fn exchange_uses_the_auth_origin_not_the_inference_origin() {
    assert_eq!(
        exchange_url("https://www.orcarouter.ai"),
        "https://www.orcarouter.ai/api/v1/auth/keys"
    );
    // The documented most-common mistake: inference host + auth path is a 404.
    assert_ne!(
        exchange_url("https://www.orcarouter.ai"),
        "https://api.orcarouter.ai/v1/auth/keys"
    );
    assert_eq!(DEFAULT_API_BASE, "https://api.orcarouter.ai/v1");
}

#[test]
fn origins_are_configurable_independently() {
    let _sandbox = AuthTestSandbox::new().expect("sandbox");
    let _env = OrcaEnvGuard::new();

    assert_eq!(auth_base(), DEFAULT_AUTH_BASE);
    assert_eq!(api_base(), DEFAULT_API_BASE);

    crate::alphacode_core::env::set_var("ORCA_AUTH_BASE_URL", "https://auth.example.test/");
    crate::alphacode_core::env::set_var("ORCA_API_BASE_URL", "https://relay.example.test/v1");
    assert_eq!(
        crate::provider_catalog::resolve_openai_compatible_profile(
            crate::provider_catalog::ORCAROUTER_PROFILE
        )
        .api_base,
        "https://relay.example.test/v1"
    );
    assert_eq!(auth_base(), "https://auth.example.test");
    assert_eq!(api_base(), "https://relay.example.test/v1");

    // Shared self-hosted fallback applies to auth only; the inference base is
    // never derived from it.
    crate::alphacode_core::env::remove_var("ORCA_AUTH_BASE_URL");
    crate::alphacode_core::env::set_var("ORCA_BASE_URL", "http://localhost:8080");
    assert_eq!(auth_base(), "http://localhost:8080");
    assert_eq!(api_base(), "https://relay.example.test/v1");

    // Plain HTTP is a loopback-only exception; an invalid override is skipped
    // rather than accepted, and only then does the shared fallback apply.
    crate::alphacode_core::env::set_var("ORCA_AUTH_BASE_URL", "http://evil.example.test");
    assert_eq!(auth_base(), "http://localhost:8080");
    crate::alphacode_core::env::remove_var("ORCA_BASE_URL");
    assert_eq!(auth_base(), DEFAULT_AUTH_BASE);
}

#[test]
fn parse_callback_input_accepts_url_query_and_bare_code() {
    assert_eq!(
        parse_callback_input("http://127.0.0.1:5000/cb?code=abc123&state=xyz"),
        ("abc123".to_string(), Some("xyz".to_string()))
    );
    assert_eq!(
        parse_callback_input("code=abc123&state=xyz"),
        ("abc123".to_string(), Some("xyz".to_string()))
    );
    assert_eq!(
        parse_callback_input("  abc123  "),
        ("abc123".to_string(), None)
    );
}

#[test]
fn key_helpers_never_expose_the_secret() {
    let key = "sk-orca-0123456789abcdef";
    let redacted = redact_key(key);
    assert!(redacted.starts_with("sk-orca-"));
    assert!(!redacted.contains("0123456789abcdef"));
    assert_eq!(redact_key("short"), "*****");
    assert_eq!(key_prefix_warning(key), None);
    assert!(key_prefix_warning("sk-something-else").is_some());
}

#[test]
fn save_and_clear_round_trip_through_the_env_file() {
    let _sandbox = AuthTestSandbox::new().expect("sandbox");
    let _env = OrcaEnvGuard::new();

    save_key("sk-orca-test-key-123456").expect("save key");
    assert_eq!(
        load_key().as_deref(),
        Some("sk-orca-test-key-123456"),
        "the key must come back from the project's existing credential store"
    );

    let path = _sandbox.env_file_path(ENV_FILE);
    assert!(path.exists(), "the key must live in {}", ENV_FILE);
    let raw = std::fs::read_to_string(&path).expect("read env file");
    assert!(raw.contains(&format!("{API_KEY_ENV}=")));

    clear_key().expect("clear key");
    assert_eq!(load_key(), None);
}

#[test]
fn reauth_is_bound_to_the_rejected_key_generation() {
    let _sandbox = AuthTestSandbox::new().expect("sandbox");
    let _env = OrcaEnvGuard::new();

    save_key("sk-orca-old-generation").expect("save old key");
    assert!(!needs_reauth());

    mark_needs_reauth("sk-orca-old-generation");
    assert!(needs_reauth(), "the rejected generation is flagged");

    // Re-authorizing replaces the credential; the old failure must not leak
    // onto the new key.
    save_key("sk-orca-new-generation").expect("save new key");
    assert!(
        !needs_reauth(),
        "a late 401 from the old key must not mark the new generation"
    );

    mark_needs_reauth("sk-orca-old-generation");
    assert!(
        !needs_reauth(),
        "a stale rejection record never matches a newer key"
    );
}

#[test]
fn unauthorized_is_recorded_only_for_the_orcarouter_endpoint() {
    let _sandbox = AuthTestSandbox::new().expect("sandbox");
    let _env = OrcaEnvGuard::new();

    save_key("sk-orca-generation-one").expect("save key");
    assert!(!needs_reauth());

    // A 401 from some other gateway must never touch OrcaRouter's state.
    note_unauthorized("https://api.openai.com/v1", "sk-orca-generation-one");
    assert!(!needs_reauth());

    note_unauthorized("https://api.orcarouter.ai/v1", "sk-orca-generation-one");
    assert!(needs_reauth());
    assert!(reauth_notice().is_some());

    save_key("sk-orca-generation-two").expect("save key");
    assert!(!needs_reauth());
    assert!(reauth_notice().is_none());
    note_unauthorized("https://api.orcarouter.ai/v1", "sk-orca-generation-one");
    assert!(
        !needs_reauth(),
        "a late 401 must not reject the replacement key"
    );

    assert!(is_orcarouter_api_base("https://api.orcarouter.ai/v1"));
    assert!(is_orcarouter_api_base("https://api.orcarouter.ai/v1/"));
    assert!(!is_orcarouter_api_base("https://api.example.test/v1"));
    assert!(!is_orcarouter_api_base("https://api.orcarouter.ai:444/v1"));
    assert!(!is_orcarouter_api_base("not a url"));
}

/// Minimal loopback HTTP server used as the fake token endpoint.
fn spawn_fake_token_server(
    status_line: &'static str,
    body: &'static str,
) -> (std::net::SocketAddr, std::sync::mpsc::Receiver<String>) {
    use std::io::{Read, Write};
    use std::sync::mpsc;

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind fake server");
    let addr = listener.local_addr().expect("addr");
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buffer = [0u8; 8192];
            let read = stream.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let _ = tx.send(request);
            let response = format!(
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status_line,
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    (addr, rx)
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

#[test]
fn exchange_posts_verifier_and_persisted_key_is_usable() {
    let (addr, request_rx) = spawn_fake_token_server(
        "200 OK",
        r#"{"key":"sk-orca-issued-key-abcdef","user_id":"u1","scope":"api"}"#,
    );
    let (attempt, _) = begin_attempt();
    let auth_base = format!("http://127.0.0.1:{}", addr.port());

    let issued = block_on(exchange_code(&auth_base, &attempt, "one-time-code")).expect("exchange");
    assert_eq!(issued.key, "sk-orca-issued-key-abcdef");
    assert_eq!(issued.scope.as_deref(), Some("api"));

    let request = request_rx.recv().expect("captured request");
    assert!(request.starts_with("POST /api/v1/auth/keys "));
    assert!(request.contains(&format!("\"code_verifier\":\"{}\"", attempt.verifier)));
    assert!(request.contains("\"code_challenge_method\":\"S256\""));
    assert!(request.contains("\"code\":\"one-time-code\""));
    // The verifier travels in the body only, never in the request target.
    assert!(
        !request
            .lines()
            .next()
            .unwrap_or_default()
            .contains(&attempt.verifier)
    );
}

#[test]
fn exchange_error_never_echoes_server_supplied_secrets() {
    let (addr, _rx) = spawn_fake_token_server(
        "400 Bad Request",
        r#"{"error":"authorization failed", "echo":"sk-orca-sensitive-code"}"#,
    );
    let (attempt, _) = begin_attempt();
    let error = block_on(exchange_code(
        &format!("http://127.0.0.1:{}", addr.port()),
        &attempt,
        "code",
    ))
    .expect_err("exchange must fail");
    assert!(!error.to_string().contains("sk-orca-sensitive-code"));
}

#[test]
fn exchange_error_is_actionable_and_hides_the_verifier() {
    let (addr, _rx) = spawn_fake_token_server(
        "400 Bad Request",
        r#"{"error":"invalid_grant","error_description":"code expired"}"#,
    );
    let (attempt, _) = begin_attempt();
    let auth_base = format!("http://127.0.0.1:{}", addr.port());

    let error = block_on(exchange_code(&auth_base, &attempt, "stale-code"))
        .expect_err("exchange must fail");
    let message = error.to_string();
    assert!(message.contains("400"));
    assert!(!message.contains("invalid_grant"));
    assert!(
        !message.contains(&attempt.verifier),
        "verifier leaked into the error: {message}"
    );
}
