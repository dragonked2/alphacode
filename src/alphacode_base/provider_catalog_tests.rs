//! Invariants for the declarative provider catalog.
//!
//! The catalog is the single source of truth for login/profile metadata across
//! the CLI, TUI, auth status, and routing surfaces. These tests pin the
//! structural invariants those surfaces assume (unique ids, resolvable
//! aliases, well-formed endpoints and key slots) so a malformed entry fails
//! here rather than at runtime on a user's machine.

use crate::alphacode_provider_metadata::OpenAiCompatibleProfile;
use crate::provider_catalog::{
    AGENTROUTER_PROFILE, LoginProviderTarget, is_safe_env_file_name, is_safe_env_key_name,
    login_providers, normalize_api_base, openai_compatible_profile_by_id,
    openai_compatible_profile_static_models, openai_compatible_profiles, resolve_login_provider,
    resolve_openai_compatible_profile,
};
use std::collections::HashSet;

#[test]
fn openai_compatible_profile_ids_are_unique() {
    let mut seen = HashSet::new();
    for profile in openai_compatible_profiles() {
        assert!(
            seen.insert(profile.id),
            "duplicate OpenAI-compatible profile id: {}",
            profile.id
        );
    }
}

/// Every profile must carry a usable endpoint and a safe env key/file slot, or
/// credential loading and catalog caching silently misbehave.
#[test]
fn openai_compatible_profiles_are_well_formed() {
    for profile in openai_compatible_profiles() {
        assert_eq!(
            profile.id,
            profile.id.to_ascii_lowercase(),
            "profile id must be lowercase for id lookups: {}",
            profile.id
        );
        assert!(
            normalize_api_base(
                &crate::alphacode_provider_metadata::route_cloak::api_base_for(profile)
            )
            .is_some(),
            "profile {} has an unusable api_base: {}",
            profile.id,
            profile.api_base
        );
        assert!(
            is_safe_env_key_name(profile.api_key_env),
            "profile {} has an unsafe api_key_env: {}",
            profile.id,
            profile.api_key_env
        );
        assert!(
            is_safe_env_file_name(profile.env_file),
            "profile {} has an unsafe env_file: {}",
            profile.id,
            profile.env_file
        );
        assert!(
            !profile.display_name.trim().is_empty(),
            "profile {} has an empty display_name",
            profile.id
        );
    }
}

#[test]
fn login_provider_ids_and_aliases_are_unique_and_resolvable() {
    let mut keys = HashSet::new();
    for provider in login_providers() {
        assert!(
            keys.insert(provider.id),
            "duplicate login provider id: {}",
            provider.id
        );
        assert_eq!(
            resolve_login_provider(provider.id).map(|resolved| resolved.id),
            Some(provider.id),
            "login provider {} does not resolve by its own id",
            provider.id
        );
        for alias in provider.aliases.iter().copied() {
            assert!(
                keys.insert(alias),
                "alias {} collides with another provider id/alias",
                alias
            );
            assert_eq!(
                resolve_login_provider(alias).map(|resolved| resolved.id),
                Some(provider.id),
                "alias {} does not resolve to provider {}",
                alias,
                provider.id
            );
        }
    }
}

/// A login provider targeting an OpenAI-compatible profile must point at a
/// profile that is actually registered in `OPENAI_COMPAT_PROFILES`, otherwise
/// catalog refresh and model-picker lookups by id miss it.
#[test]
fn openai_compatible_login_targets_are_registered_profiles() {
    for provider in login_providers() {
        if let LoginProviderTarget::OpenAiCompatible(profile) = provider.target {
            assert!(
                openai_compatible_profile_by_id(profile.id).is_some(),
                "login provider {} targets unregistered profile {}",
                provider.id,
                profile.id
            );
        }
    }
}

#[test]
fn agentrouter_profile_is_registered_and_resolves() {
    let profile = openai_compatible_profile_by_id("agentrouter")
        .expect("agentrouter profile must be registered");
    assert_eq!(profile, AGENTROUTER_PROFILE);

    let resolved = resolve_openai_compatible_profile(profile);
    assert_eq!(resolved.id, "agentrouter");
    assert_eq!(resolved.display_name, "AgentRouter");
    assert_eq!(resolved.api_base, "https://agentrouter.org/v1");
    assert_eq!(resolved.api_key_env, "AGENTROUTER_API_KEY");
    assert!(resolved.requires_api_key);
}

/// The models AgentRouter serves via its OpenAI-compatible endpoint must be
/// offered before any live `/v1/models` fetch, since the endpoint 401s
/// without a key and the picker would otherwise be empty on a fresh login.
///
/// The list mirrors https://agentrouter.org/docs/opencode.html. AgentRouter
/// rotates its catalog, so a retired id (e.g. the old `glm-5.2` / `gpt-5.5`)
/// must not stay in the list: an unserved model is rejected with 503 "no
/// available channel", which looks like a provider outage rather than a
/// stale picker entry.
#[test]
fn agentrouter_static_models_cover_the_served_catalog() {
    let models = openai_compatible_profile_static_models(AGENTROUTER_PROFILE);
    assert_eq!(
        models,
        vec!["gpt-5.6-sol", "gpt-6-astra", "glm-5.3", "deepseek-v4-flash"]
    );
}

/// The Anthropic-compatible AgentRouter endpoint serves a different set.
#[test]
fn agentrouter_anthropic_static_models_cover_the_served_catalog() {
    let profile = crate::provider_catalog::LoginProviderTarget::AgentRouterAnthropic;
    let _ = profile;
    let models = openai_compatible_profile_static_models(OpenAiCompatibleProfile {
        id: "agentrouter-anthropic",
        ..AGENTROUTER_PROFILE
    });
    assert_eq!(
        models,
        vec![
            "claude-opus-5",
            "claude-opus-4-8",
            "glm-5.3",
            "deepseek-v4-flash"
        ]
    );
}

/// The default model must be one the gateway actually serves.
#[test]
fn agentrouter_default_model_is_a_served_model() {
    let default_model = AGENTROUTER_PROFILE
        .default_model
        .expect("agentrouter declares a default model");
    assert_eq!(default_model, "gpt-5.6-sol");
    assert!(
        openai_compatible_profile_static_models(AGENTROUTER_PROFILE)
            .iter()
            .any(|model| model == default_model),
        "default model {} is not in the static model list",
        default_model
    );
}

#[test]
fn agentrouter_login_provider_is_discoverable_on_every_surface() {
    use crate::provider_catalog::LoginProviderSurface;

    let provider =
        resolve_login_provider("agentrouter").expect("agentrouter login provider must resolve");
    assert_eq!(provider.display_name, "AgentRouter");
    assert!(matches!(
        provider.target,
        LoginProviderTarget::OpenAiCompatible(profile) if profile.id == "agentrouter"
    ));

    for surface in [
        LoginProviderSurface::CliLogin,
        LoginProviderSurface::TuiLogin,
        LoginProviderSurface::ServerBootstrap,
        LoginProviderSurface::AutoInit,
        LoginProviderSurface::AuthStatus,
    ] {
        assert!(
            provider.order.for_surface(surface).is_some(),
            "agentrouter should be listed on {:?}",
            surface
        );
    }
}

#[test]
fn agentrouter_anthropic_login_provider_is_registered() {
    use crate::provider_catalog::LoginProviderSurface;

    let provider = resolve_login_provider("agentrouter-anthropic")
        .expect("agentrouter-anthropic login provider must resolve");
    assert_eq!(provider.display_name, "AgentRouter (Anthropic)");
    assert!(matches!(
        provider.target,
        LoginProviderTarget::AgentRouterAnthropic
    ));

    for surface in [
        LoginProviderSurface::CliLogin,
        LoginProviderSurface::TuiLogin,
        LoginProviderSurface::ServerBootstrap,
        LoginProviderSurface::AutoInit,
        LoginProviderSurface::AuthStatus,
    ] {
        assert!(
            provider.order.for_surface(surface).is_some(),
            "agentrouter-anthropic should be listed on {:?}",
            surface
        );
    }
}

// ---------------------------------------------------------------------------
// OrcaRouter
// ---------------------------------------------------------------------------

/// A temp `ALPHACODE_HOME` so the live-catalog lookups below read a cache this
/// test wrote instead of whatever the developer machine has.
struct OrcaCatalogHome {
    _dir: tempfile::TempDir,
    _lock: crate::storage::TestEnvGuard,
    previous: Option<std::ffi::OsString>,
}

impl OrcaCatalogHome {
    fn new() -> Self {
        // One `ALPHACODE_HOME` at a time: these tests redirect it, and a
        // concurrent redirect would point the cache lookup at another test's
        // directory.
        let lock = crate::storage::lock_test_env();
        let dir = tempfile::tempdir().expect("temp home");
        let previous = std::env::var_os("ALPHACODE_HOME");
        crate::alphacode_core::env::set_var("ALPHACODE_HOME", dir.path());
        Self {
            _dir: dir,
            _lock: lock,
            previous,
        }
    }
}

impl Drop for OrcaCatalogHome {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => crate::alphacode_core::env::set_var("ALPHACODE_HOME", value),
            None => crate::alphacode_core::env::remove_var("ALPHACODE_HOME"),
        }
    }
}

fn orcarouter_profile() -> OpenAiCompatibleProfile {
    openai_compatible_profile_by_id("orcarouter").expect("orcarouter profile must be registered")
}

#[test]
fn orcarouter_profile_targets_the_inference_origin_and_key_slot() {
    let profile = orcarouter_profile();
    let resolved = resolve_openai_compatible_profile(profile);
    assert_eq!(resolved.api_base, "https://api.orcarouter.ai/v1");
    assert_eq!(resolved.env_file, "orcarouter.env");
    assert!(resolved.requires_api_key);
    // The auth origin is a different host than the inference origin; the
    // exchange endpoint must never be derived from the API base.
    assert_ne!(
        crate::alphacode_base::auth::orcarouter::auth_base(),
        resolved.api_base
    );
}

#[test]
fn orcarouter_seed_models_are_offered_before_the_live_catalog_lands() {
    let models = openai_compatible_profile_static_models(orcarouter_profile());
    assert_eq!(
        models,
        vec![
            "orcarouter/auto",
            "openai/gpt-5.5",
            "anthropic/claude-opus-4.8",
            "google/gemini-3.5-flash",
            "deepseek/deepseek-v4-pro",
        ]
    );
}

#[test]
fn orcarouter_default_model_is_in_the_seed_list() {
    let profile = orcarouter_profile();
    let default_model = profile.default_model.expect("declares a default model");
    assert!(
        openai_compatible_profile_static_models(profile)
            .iter()
            .any(|model| model == default_model),
        "default model {} is not in the seed list",
        default_model
    );
}

#[test]
fn orcarouter_login_providers_are_registered_and_discoverable() {
    use crate::provider_catalog::LoginProviderSurface;

    let api = resolve_login_provider("orcarouter").expect("orcarouter login provider must resolve");
    assert!(matches!(
        api.target,
        LoginProviderTarget::OpenAiCompatible(profile) if profile.id == "orcarouter"
    ));
    let oauth = resolve_login_provider("orcarouter-oauth")
        .expect("orcarouter-oauth login provider must resolve");
    assert!(matches!(oauth.target, LoginProviderTarget::OrcaRouterOAuth));

    for provider in [api, oauth] {
        for surface in [
            LoginProviderSurface::CliLogin,
            LoginProviderSurface::TuiLogin,
            LoginProviderSurface::ServerBootstrap,
            LoginProviderSurface::AutoInit,
            LoginProviderSurface::AuthStatus,
        ] {
            assert!(
                provider.order.for_surface(surface).is_some(),
                "{} should be listed on {:?}",
                provider.id,
                surface
            );
        }
    }
}

#[test]
fn orcarouter_both_logins_boot_one_inference_profile() {
    let api = resolve_login_provider("orcarouter").expect("api provider");
    let oauth = resolve_login_provider("orcarouter-oauth").expect("oauth provider");
    // Different ids, different auth kinds, same inference slot.
    assert_ne!(api.id, oauth.id);
    assert_ne!(api.auth_kind, oauth.auth_kind);
    let resolved = resolve_openai_compatible_profile(orcarouter_profile());
    assert_eq!(
        crate::provider::MultiProvider::config_default_provider_for_login_provider(oauth),
        Some(resolved.id.as_str())
    );
}

/// Embedding-only and image-generation rows must never be offered as chat
/// models; a model that advertises a chat endpoint, or advertises nothing at
/// all, stays available.
#[test]
fn orcarouter_endpoint_capabilities_gate_chat_models() {
    use crate::alphacode_provider_openrouter::ModelInfo;
    use crate::provider_catalog::openai_compatible_profile_model_supports_chat;

    let _home = OrcaCatalogHome::new();
    let model =
        |id: &str, endpoints: &[&str], capabilities: &[&str], modalities: &[&str]| ModelInfo {
            id: id.to_string(),
            name: id.to_string(),
            context_length: Some(128_000),
            pricing: Default::default(),
            created: Some(1_700_000_000),
            supported_endpoint_types: endpoints.iter().map(|v| v.to_string()).collect(),
            endpoint_capabilities: capabilities.iter().map(|v| v.to_string()).collect(),
            input_modalities: modalities.iter().map(|v| v.to_string()).collect(),
        };
    crate::alphacode_provider_openrouter::save_disk_cache_with_source_for_namespace(
        "orcarouter",
        &[
            model(
                "openai/gpt-5.5",
                &["openai"],
                &["chat_completions"],
                &["text", "image"],
            ),
            model(
                "anthropic/claude-opus-4.8",
                &["anthropic"],
                &[],
                &["text", "image", "file"],
            ),
            model("embed/text-3", &["embeddings"], &["embedding"], &["text"]),
            model(
                "img/flux",
                &["image-generation"],
                &["image_generation"],
                &["text"],
            ),
            model("mystery/model", &[], &[], &[]),
        ],
        Some("https://api.orcarouter.ai/v1"),
    );

    for chat_model in [
        "openai/gpt-5.5",
        "anthropic/claude-opus-4.8",
        "mystery/model",
    ] {
        assert!(
            openai_compatible_profile_model_supports_chat("orcarouter", chat_model),
            "{chat_model} advertises chat (or nothing) and must stay selectable"
        );
    }
    for non_chat_model in ["embed/text-3", "img/flux"] {
        assert!(
            !openai_compatible_profile_model_supports_chat("orcarouter", non_chat_model),
            "{non_chat_model} is not a chat model and must not be offered"
        );
    }

    // Other profiles keep their permissive behavior.
    assert!(openai_compatible_profile_model_supports_chat(
        "groq",
        "embed/text-3"
    ));
}

#[test]
fn orcarouter_multimodal_support_follows_the_catalog() {
    use crate::alphacode_provider_openrouter::ModelInfo;

    let _home = OrcaCatalogHome::new();
    let model = |id: &str, modalities: &[&str]| ModelInfo {
        id: id.to_string(),
        name: id.to_string(),
        context_length: None,
        pricing: Default::default(),
        created: None,
        supported_endpoint_types: vec!["openai".to_string()],
        endpoint_capabilities: Vec::new(),
        input_modalities: modalities.iter().map(|v| v.to_string()).collect(),
    };
    crate::alphacode_provider_openrouter::save_disk_cache_with_source_for_namespace(
        "orcarouter",
        &[
            model("vision/model", &["text", "image"]),
            model("text/model", &["text"]),
            model("unknown/model", &[]),
        ],
        Some("https://api.orcarouter.ai/v1"),
    );

    use crate::provider_catalog::openai_compatible_profile_model_supports_image_input;
    assert_eq!(
        openai_compatible_profile_model_supports_image_input("orcarouter", "vision/model"),
        Some(true)
    );
    assert_eq!(
        openai_compatible_profile_model_supports_image_input("orcarouter", "text/model"),
        Some(false)
    );
    // Unadvertised: the caller keeps its own default rather than guessing.
    assert_eq!(
        openai_compatible_profile_model_supports_image_input("orcarouter", "unknown/model"),
        None
    );
    assert_eq!(
        openai_compatible_profile_model_supports_image_input("groq", "vision/model"),
        None
    );
}

/// LM Studio reports `created: null` for every model it serves. Those rows
/// must still count as *models*: an undated row is valid, and discarding
/// undated rows emptied the catalog and left the login with no default.
#[test]
fn undated_local_models_still_yield_a_default() {
    use crate::alphacode_provider_openrouter::ModelInfo;

    let _home = OrcaCatalogHome::new();
    let previous = std::env::var_os("ALPHACODE_OPENAI_COMPAT_API_BASE");
    crate::alphacode_core::env::set_var(
        "ALPHACODE_OPENAI_COMPAT_API_BASE",
        "http://localhost:1234/v1",
    );

    let model = |id: &str| ModelInfo {
        id: id.to_string(),
        name: String::new(),
        context_length: None,
        pricing: Default::default(),
        // Exactly what LM Studio sends.
        created: None,
        supported_endpoint_types: Vec::new(),
        endpoint_capabilities: Vec::new(),
        input_modalities: Vec::new(),
    };
    crate::alphacode_provider_openrouter::save_disk_cache_with_source_for_namespace(
        "openai-compatible",
        &[
            model("qwen3.8-9b-heretic-uncensored"),
            model("text-embedding-nomic-embed-text-v1.5"),
        ],
        Some("http://localhost:1234/v1"),
    );

    assert_eq!(
        crate::provider_catalog::newest_released_model_for_openai_compatible_profile(
            "openai-compatible"
        )
        .as_deref(),
        Some("qwen3.8-9b-heretic-uncensored"),
        "an undated chat model is still a usable default; the embedding row is filtered"
    );

    let resolved =
        resolve_openai_compatible_profile(crate::provider_catalog::OPENAI_COMPAT_PROFILE);
    assert_eq!(
        resolved.default_model.as_deref(),
        Some("qwen3.8-9b-heretic-uncensored")
    );

    match previous {
        Some(value) => {
            crate::alphacode_core::env::set_var("ALPHACODE_OPENAI_COMPAT_API_BASE", value)
        }
        None => crate::alphacode_core::env::remove_var("ALPHACODE_OPENAI_COMPAT_API_BASE"),
    }
}

/// A `[providers.<name>]`-free live catalog written under the generic
/// `openai-compatible` namespace but produced by a *different* endpoint than
/// the resolved one must not be trusted as evidence about that endpoint.
#[test]
fn borrowed_generic_default_is_replaced_by_what_the_endpoint_advertises() {
    use crate::alphacode_provider_openrouter::ModelInfo;

    let _home = OrcaCatalogHome::new();
    let previous = std::env::var_os("ALPHACODE_OPENAI_COMPAT_API_BASE");
    crate::alphacode_core::env::set_var(
        "ALPHACODE_OPENAI_COMPAT_API_BASE",
        "http://localhost:1234/v1",
    );

    let model = |id: &str| ModelInfo {
        id: id.to_string(),
        name: String::new(),
        context_length: None,
        pricing: Default::default(),
        created: None,
        supported_endpoint_types: Vec::new(),
        endpoint_capabilities: Vec::new(),
        input_modalities: Vec::new(),
    };
    // What LM Studio reports: the loaded GGUF plus an embedding model, and
    // crucially no model named `alpha`.
    crate::alphacode_provider_openrouter::save_disk_cache_with_source_for_namespace(
        "openai-compatible",
        &[
            model("qwen3.8-9b-heretic-uncensored"),
            model("text-embedding-nomic-embed-text-v1.5"),
        ],
        Some("http://127.0.0.1:1234/v1"),
    );

    crate::alphacode_provider_openrouter::save_disk_cache_with_source_for_namespace(
        "lmstudio",
        &[model("unrelated-model")],
        Some("http://127.0.0.1:1235/v1"),
    );

    let resolved =
        resolve_openai_compatible_profile(crate::provider_catalog::OPENAI_COMPAT_PROFILE);
    assert_eq!(
        resolved.default_model.as_deref(),
        Some("qwen3.8-9b-heretic-uncensored"),
        "the session must pin a model this endpoint actually serves, not the \
         Dragonmeta `alpha` default"
    );

    assert_eq!(resolved.display_name, "LM Studio");
    assert_eq!(
        crate::provider_catalog::openai_compatible_profile_id_for_api_base("http://127.0.0.1:1234"),
        Some("lmstudio")
    );
    let native = resolve_openai_compatible_profile(crate::provider_catalog::LMSTUDIO_PROFILE);
    assert_eq!(
        native.default_model.as_deref(),
        Some("qwen3.8-9b-heretic-uncensored")
    );

    match previous {
        Some(value) => {
            crate::alphacode_core::env::set_var("ALPHACODE_OPENAI_COMPAT_API_BASE", value)
        }
        None => crate::alphacode_core::env::remove_var("ALPHACODE_OPENAI_COMPAT_API_BASE"),
    }
}

/// The mirror case: when the endpoint itself lists the declared default, the
/// declared default wins. Dragonmeta serves `alpha`, so a catalog containing
/// `alpha` must not be second-guessed by the "pick the strongest tier" rule.
#[test]
fn declared_default_survives_a_catalog_that_actually_serves_it() {
    use crate::alphacode_provider_openrouter::ModelInfo;

    let _home = OrcaCatalogHome::new();
    let previous = std::env::var_os("ALPHACODE_OPENAI_COMPAT_API_BASE");
    crate::alphacode_core::env::set_var(
        "ALPHACODE_OPENAI_COMPAT_API_BASE",
        "http://127.0.0.1:8080/v1",
    );

    let model = |id: &str| ModelInfo {
        id: id.to_string(),
        name: String::new(),
        context_length: None,
        pricing: Default::default(),
        created: None,
        supported_endpoint_types: Vec::new(),
        endpoint_capabilities: Vec::new(),
        input_modalities: Vec::new(),
    };
    crate::alphacode_provider_openrouter::save_disk_cache_with_source_for_namespace(
        "openai-compatible",
        &[model("alpha"), model("some-other-local-model")],
        Some("http://127.0.0.1:8080/v1"),
    );

    let resolved =
        resolve_openai_compatible_profile(crate::provider_catalog::OPENAI_COMPAT_PROFILE);
    assert_eq!(
        resolved.default_model.as_deref(),
        Some("alpha"),
        "a catalog that lists the declared default confirms it"
    );

    match previous {
        Some(value) => {
            crate::alphacode_core::env::set_var("ALPHACODE_OPENAI_COMPAT_API_BASE", value)
        }
        None => crate::alphacode_core::env::remove_var("ALPHACODE_OPENAI_COMPAT_API_BASE"),
    }
}

/// With no catalog at all (cold install, server not started), the declared
/// default must survive: there is no evidence it is wrong, and blanking the
/// header would be worse than showing the bundled expectation.
#[test]
fn declared_default_survives_a_missing_catalog() {
    let _home = OrcaCatalogHome::new();
    let resolved =
        resolve_openai_compatible_profile(crate::provider_catalog::OPENAI_COMPAT_PROFILE);
    assert_eq!(resolved.default_model.as_deref(), Some("alpha"));
}
