//! Invariants for the declarative provider catalog.
//!
//! The catalog is the single source of truth for login/profile metadata across
//! the CLI, TUI, auth status, and routing surfaces. These tests pin the
//! structural invariants those surfaces assume (unique ids, resolvable
//! aliases, well-formed endpoints and key slots) so a malformed entry fails
//! here rather than at runtime on a user's machine.

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
            normalize_api_base(profile.api_base).is_some(),
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

/// The three models AgentRouter serves must be offered before any live
/// `/v1/models` fetch, since the endpoint 401s without a key and the picker
/// would otherwise be empty on a fresh login.
#[test]
fn agentrouter_static_models_cover_the_served_catalog() {
    let models = openai_compatible_profile_static_models(AGENTROUTER_PROFILE);
    assert_eq!(
        models,
        vec!["claude-opus-5", "gpt-5.6-sol", "claude-opus-4-8"]
    );
}

/// The default model must be one the gateway actually serves.
#[test]
fn agentrouter_default_model_is_a_served_model() {
    let default_model = AGENTROUTER_PROFILE
        .default_model
        .expect("agentrouter declares a default model");
    assert_eq!(default_model, "claude-opus-5");
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
