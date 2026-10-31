//! Live OrcaRouter checks that run the *real* provider code path.
//!
//! These are `#[ignore]`d so an ordinary `cargo test --lib` never spends
//! balance. Run them explicitly with a real key in the environment:
//!
//! ```text
//! ORCAROUTER_API_KEY=... cargo test --lib orcarouter_live -- --ignored --nocapture
//! ```
//!
//! The key is only ever held in memory: it is written to a sandboxed
//! `ALPHACODE_HOME` env file so the production credential loader and the
//! production provider constructor are the ones under test, and it is never
//! printed or asserted against.

use super::*;
use crate::alphacode_base::auth::test_sandbox::AuthTestSandbox;
use crate::alphacode_provider_core::Provider;

/// Build a provider the way the CLI does, against a sandboxed home that holds
/// the real key. The sandbox is returned alongside the provider because it owns
/// the test-env lock and `ALPHACODE_HOME`; dropping it early would let another
/// test redirect the credential store.
fn live_provider() -> Option<(AuthTestSandbox, OpenRouterProvider)> {
    let key = std::env::var(crate::alphacode_provider_metadata::ORCAROUTER_PROFILE.api_key_env)
        .ok()
        .filter(|key| !key.trim().is_empty())?;

    let sandbox = AuthTestSandbox::new().expect("live test sandbox");
    sandbox
        .write_openai_compatible_api_key(
            crate::alphacode_provider_metadata::ORCAROUTER_PROFILE,
            &key,
        )
        .expect("store the live key in the sandboxed credential file");
    drop(key);

    // The provider is constructed exactly as the CLI constructs it, so a key
    // that cannot be loaded here would fail for real users too.
    let provider = OpenRouterProvider::new_openai_compatible_profile_runtime(
        crate::alphacode_provider_metadata::ORCAROUTER_PROFILE,
    )
    .expect("build the OrcaRouter provider from the sandboxed key");
    Some((sandbox, provider))
}

/// Live model catalog through the provider's own discovery path.
#[ignore = "requires a real ORCAROUTER_API_KEY; spends no balance but hits the live catalog"]
#[tokio::test(flavor = "current_thread")]
async fn orcarouter_live_catalog_through_the_provider() {
    let Some((_sandbox, provider)) = live_provider() else {
        eprintln!("ORCAROUTER_API_KEY not set; skipping live catalog check");
        return;
    };

    let models = tokio::time::timeout(
        std::time::Duration::from_secs(90),
        provider.refresh_models(),
    )
    .await
    .expect("live catalog request timed out")
    .expect("live catalog request failed");

    assert!(
        !models.is_empty(),
        "live OrcaRouter catalog returned no rows through the provider path"
    );
    // Every live row keeps its vendor/model namespace.
    assert!(
        models.iter().filter(|m| m.id.contains('/')).count() >= models.len() / 2,
        "live catalog rows should keep their vendor/model namespace"
    );

    let chat_rows = models
        .iter()
        .filter(|m| {
            crate::alphacode_base::provider_catalog::openai_compatible_profile_model_supports_chat(
                crate::alphacode_provider_metadata::ORCAROUTER_PROFILE.id,
                &m.id,
            )
        })
        .count();
    assert!(
        chat_rows > 0,
        "live catalog must expose at least one chat-capable model"
    );

    // The catalog is authoritative: the chat gate must reject rows that the
    // live catalog advertises only as embeddings/image/rerank.
    let advertised_non_chat = models.iter().find(|m| {
        !m.supported_endpoint_types.is_empty()
            && m.supported_endpoint_types.iter().all(|endpoint| {
                matches!(
                    endpoint.to_ascii_lowercase().as_str(),
                    "embeddings"
                        | "embedding"
                        | "image-generation"
                        | "image_generation"
                        | "rerank"
                        | "reranking"
                )
            })
    });
    if let Some(model) = advertised_non_chat {
        assert!(
            !crate::alphacode_base::provider_catalog::openai_compatible_profile_model_supports_chat(
                crate::alphacode_provider_metadata::ORCAROUTER_PROFILE.id,
                &model.id,
            ),
            "a model advertised only as a non-chat endpoint must not pass the chat gate: {}",
            model.id
        );
    }

    // Multimodal support must follow the live catalog's declared modalities.
    if let Some(model) = models.iter().find(|m| !m.input_modalities.is_empty()) {
        let expected = model
            .input_modalities
            .iter()
            .any(|modality| modality.eq_ignore_ascii_case("image"));
        assert_eq!(
            crate::alphacode_base::provider_catalog::openai_compatible_profile_model_supports_image_input(
                crate::alphacode_provider_metadata::ORCAROUTER_PROFILE.id,
                &model.id,
            ),
            Some(expected),
            "image-input support must mirror the live catalog for {}",
            model.id
        );
    }
}

/// Live inference through the provider object the CLI/TUI construct.
#[ignore = "requires a real ORCAROUTER_API_KEY and spends a small amount of balance"]
#[tokio::test(flavor = "current_thread")]
async fn orcarouter_live_inference_through_the_provider() {
    let Some((_sandbox, provider)) = live_provider() else {
        eprintln!("ORCAROUTER_API_KEY not set; skipping live inference check");
        return;
    };

    let model = std::env::var("ORCAROUTER_LIVE_MODEL")
        .unwrap_or_else(|_| "deepseek/deepseek-v4-pro".to_string());
    provider.set_model(&model).expect("live model is accepted");
    assert_eq!(provider.model(), model);

    let reply = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        provider.complete_simple("Reply with exactly: ORCA_OK", "You are a test harness."),
    )
    .await
    .expect("live completion timed out")
    .expect("live completion failed");

    assert!(
        reply.contains("ORCA_OK"),
        "live completion did not echo the expected marker: {reply:?}"
    );
    assert!(
        crate::alphacode_base::auth::orcarouter::reauth_notice().is_none(),
        "a successful request must not leave OrcaRouter marked as needing reauth"
    );
}
