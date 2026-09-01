//! Coverage for the CLI `ProviderChoice` <-> login-provider catalog wiring.
//!
//! `ProviderChoice` is a hand-maintained clap enum that must stay in lockstep
//! with `PROVIDER_CHOICE_LOGIN_PROVIDERS`; a variant added without its mapping
//! entry silently resolves to `None` at runtime (no compile error), so the
//! round-trip is asserted here instead.

// `ClaudeSubprocess` is deprecated but still enumerated by `value_variants()`,
// so exhaustiveness checks below must be able to name it.
#![allow(deprecated)]

use super::{
    ProviderChoice, choice_for_login_provider, login_provider_choice_mappings,
    login_provider_for_choice, profile_for_choice,
};
use clap::ValueEnum;

/// Every `ProviderChoice` except the deprecated `ClaudeSubprocess` shim and the
/// synthetic `Auto` selector must map to a catalog login provider.
#[test]
fn every_choice_maps_to_a_login_provider() {
    for choice in ProviderChoice::value_variants() {
        if matches!(
            choice,
            ProviderChoice::Auto | ProviderChoice::ClaudeSubprocess
        ) {
            continue;
        }
        assert!(
            login_provider_for_choice(choice).is_some(),
            "ProviderChoice::{:?} has no PROVIDER_CHOICE_LOGIN_PROVIDERS entry",
            choice
        );
    }
}

#[test]
fn choice_mappings_have_no_duplicate_providers() {
    let mut seen = std::collections::HashSet::new();
    for (choice, provider) in login_provider_choice_mappings() {
        if matches!(choice, ProviderChoice::ClaudeSubprocess) {
            continue;
        }
        assert!(
            seen.insert(provider.id),
            "duplicate login provider id in choice mappings: {}",
            provider.id
        );
    }
}

#[test]
fn dragonmeta_choice_round_trips_through_the_catalog() {
    let provider = login_provider_for_choice(&ProviderChoice::Dragonmeta)
        .expect("dragonmeta choice must map to a login provider");
    assert_eq!(provider.id, "dragonmeta");
    assert_eq!(
        choice_for_login_provider(provider),
        Some(ProviderChoice::Dragonmeta)
    );
    assert_eq!(ProviderChoice::Dragonmeta.as_arg_value(), "dragonmeta");
    let profile = profile_for_choice(&ProviderChoice::Dragonmeta)
        .expect("dragonmeta must map to an OpenAI-compatible profile");
    assert_eq!(profile.id, "dragonmeta");
    // Aliases resolve too.
    for alias in ["dragon", "dragon-meta", "dmeta"] {
        assert_eq!(
            ProviderChoice::from_str(alias, true).ok(),
            Some(ProviderChoice::Dragonmeta),
            "alias {alias} should parse to ProviderChoice::Dragonmeta"
        );
    }
}

#[test]
fn agentrouter_choice_round_trips_through_the_catalog() {
    let provider = login_provider_for_choice(&ProviderChoice::Agentrouter)
        .expect("agentrouter choice must map to a login provider");
    assert_eq!(provider.id, "agentrouter");
    assert_eq!(
        choice_for_login_provider(provider),
        Some(ProviderChoice::Agentrouter)
    );
    assert_eq!(ProviderChoice::Agentrouter.as_arg_value(), "agentrouter");
}

/// AgentRouter is an OpenAI-compatible gateway, so `--provider agentrouter`
/// must resolve to a profile carrying the documented base URL and key slot.
#[test]
fn agentrouter_choice_resolves_to_its_openai_compatible_profile() {
    let profile = profile_for_choice(&ProviderChoice::Agentrouter)
        .expect("agentrouter must resolve to an OpenAI-compatible profile");
    assert_eq!(profile.id, "agentrouter");
    assert_eq!(profile.api_base, "https://agentrouter.org/v1");
    assert_eq!(profile.api_key_env, "AGENTROUTER_API_KEY");
    assert_eq!(profile.env_file, "agentrouter.env");
    assert!(
        profile.requires_api_key,
        "agentrouter gates /v1 on a bearer key"
    );
}

/// The documented CLI aliases must all parse to the same variant.
#[test]
fn agentrouter_cli_aliases_parse() {
    for alias in [
        "agentrouter",
        "agent-router",
        "agentrouter-org",
        "agentrouter-api",
    ] {
        assert_eq!(
            ProviderChoice::from_str(alias, true).ok(),
            Some(ProviderChoice::Agentrouter),
            "alias {} should parse to ProviderChoice::Agentrouter",
            alias
        );
    }
}
