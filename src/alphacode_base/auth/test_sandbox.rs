use std::path::{Path, PathBuf};
use std::sync::MutexGuard;

use crate::provider_catalog::{OpenAiCompatibleProfile, openai_compatible_profiles};

pub struct AuthTestSandbox {
    _lock: MutexGuard<'static, ()>,
    temp: tempfile::TempDir,
    saved_env: Vec<(String, Option<String>)>,
}

impl AuthTestSandbox {
    pub fn new() -> anyhow::Result<Self> {
        let lock = crate::storage::lock_test_env();
        let temp = tempfile::Builder::new()
            .prefix("alphacode-auth-lifecycle-")
            .tempdir()?;
        let saved_env = tracked_env_vars()
            .into_iter()
            .map(|key| {
                let value = std::env::var(&key).ok();
                (key, value)
            })
            .collect::<Vec<_>>();

        for (key, _) in &saved_env {
            crate::alphacode_core::env::remove_var(key);
        }

        std::fs::create_dir_all(temp.path().join("config").join("alphacode"))?;
        std::fs::create_dir_all(temp.path().join("external"))?;
        crate::alphacode_core::env::set_var("ALPHACODE_HOME", temp.path());
        crate::provider_catalog::force_apply_openai_compatible_profile_env(None);
        reset_global_auth_state();

        Ok(Self {
            _lock: lock,
            temp,
            saved_env,
        })
    }

    pub fn root(&self) -> &Path {
        self.temp.path()
    }

    pub fn config_dir(&self) -> PathBuf {
        self.root().join("config").join("alphacode")
    }

    pub fn external_dir(&self) -> PathBuf {
        self.root().join("external")
    }

    pub fn env_file_path(&self, file_name: &str) -> PathBuf {
        self.config_dir().join(file_name)
    }

    pub fn write_env_file(
        &self,
        file_name: &str,
        env_key: &str,
        value: &str,
    ) -> anyhow::Result<PathBuf> {
        let path = self.env_file_path(file_name);
        std::fs::create_dir_all(self.config_dir())?;
        std::fs::write(&path, format!("{}={}\n", env_key, value))?;
        crate::alphacode_core::fs::set_permissions_owner_only(&path)?;
        reset_global_auth_state();
        Ok(path)
    }

    pub fn write_openai_compatible_api_key(
        &self,
        profile: OpenAiCompatibleProfile,
        value: &str,
    ) -> anyhow::Result<PathBuf> {
        let resolved = crate::provider_catalog::resolve_openai_compatible_profile(profile);
        self.write_env_file(&resolved.env_file, &resolved.api_key_env, value)
    }
}

impl Drop for AuthTestSandbox {
    fn drop(&mut self) {
        for (key, value) in self.saved_env.drain(..) {
            if let Some(value) = value {
                crate::alphacode_core::env::set_var(&key, value);
            } else {
                crate::alphacode_core::env::remove_var(&key);
            }
        }
        reset_global_auth_state();
    }
}

fn reset_global_auth_state() {
    crate::alphacode_base::auth::AuthStatus::invalidate_cache();
    crate::provider::clear_all_provider_unavailability_for_account();
    crate::provider::clear_all_model_unavailability_for_account();
}

fn tracked_env_vars() -> Vec<String> {
    let mut keys = [
        "ALPHACODE_HOME",
        "XDG_CONFIG_HOME",
        "ALPHACODE_OPENROUTER_API_BASE",
        "ALPHACODE_OPENROUTER_API_KEY_NAME",
        "ALPHACODE_OPENROUTER_ENV_FILE",
        "ALPHACODE_OPENROUTER_CACHE_NAMESPACE",
        "ALPHACODE_OPENROUTER_PROVIDER_FEATURES",
        "ALPHACODE_OPENROUTER_TRANSPORT_STATE",
        "ALPHACODE_OPENROUTER_ALLOW_NO_AUTH",
        "ALPHACODE_OPENROUTER_PROVIDER",
        "ALPHACODE_OPENROUTER_NO_FALLBACK",
        "ALPHACODE_OPENROUTER_MODEL",
        "ALPHACODE_OPENROUTER_MODEL_CATALOG",
        "ALPHACODE_OPENROUTER_STATIC_MODELS",
        "ALPHACODE_OPENROUTER_AUTH_HEADER",
        "ALPHACODE_OPENROUTER_AUTH_HEADER_NAME",
        "ALPHACODE_OPENROUTER_DYNAMIC_BEARER_PROVIDER",
        "ALPHACODE_OPENAI_COMPAT_API_BASE",
        "ALPHACODE_OPENAI_COMPAT_API_KEY_NAME",
        "ALPHACODE_OPENAI_COMPAT_ENV_FILE",
        "ALPHACODE_OPENAI_COMPAT_SETUP_URL",
        "ALPHACODE_OPENAI_COMPAT_DEFAULT_MODEL",
        "ALPHACODE_OPENAI_COMPAT_LOCAL_ENABLED",
        "ALPHACODE_NAMED_PROVIDER_PROFILE",
        "ALPHACODE_PROVIDER_PROFILE_ACTIVE",
        "ALPHACODE_PROVIDER_PROFILE_NAME",
        "ALPHACODE_RUNTIME_PROVIDER",
        "ALPHACODE_ACTIVE_PROVIDER",
        "ALPHACODE_INITIAL_PROVIDER_EXPLICIT",
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
        "ANTHROPIC_API_KEY",
        "AZURE_OPENAI_ENDPOINT",
        "AZURE_OPENAI_MODEL",
        "AZURE_OPENAI_API_KEY",
        "AZURE_OPENAI_USE_ENTRA",
        "GOOGLE_API_KEY",
        "GEMINI_API_KEY",
        "CURSOR_API_KEY",
        "BEDROCK_API_KEY",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect::<std::collections::HashSet<_>>();

    for profile in openai_compatible_profiles() {
        keys.insert(profile.api_key_env.to_string());
    }

    let mut keys = keys.into_iter().collect::<Vec<_>>();
    keys.sort();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_isolates_alphacode_home_and_config_dir() {
        let sandbox = AuthTestSandbox::new().expect("sandbox");

        assert_eq!(
            std::env::var("ALPHACODE_HOME").ok().as_deref(),
            Some(sandbox.root().to_str().unwrap())
        );
        assert_eq!(
            crate::storage::app_config_dir().unwrap(),
            sandbox.config_dir()
        );
        assert!(sandbox.config_dir().starts_with(sandbox.root()));
        assert!(sandbox.external_dir().starts_with(sandbox.root()));
        assert!(sandbox.external_dir().exists());
    }

    #[test]
    fn sandbox_openai_compatible_key_file_is_read_from_temp_config() {
        let sandbox = AuthTestSandbox::new().expect("sandbox");
        sandbox
            .write_openai_compatible_api_key(
                crate::provider_catalog::CEREBRAS_PROFILE,
                "test-cerebras-key",
            )
            .expect("write key");

        assert_eq!(
            crate::provider_catalog::load_api_key_from_env_or_config(
                "CEREBRAS_API_KEY",
                "cerebras.env",
            )
            .as_deref(),
            Some("test-cerebras-key")
        );
        assert!(sandbox.env_file_path("cerebras.env").exists());
    }
}
