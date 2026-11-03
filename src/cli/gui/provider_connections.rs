//! Desktop authentication reuses the CLI in an isolated, cancellable child.
//! Credentials travel over stdin, never command-line arguments or GUI settings.
use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde::Serialize;
use serde_json::{Value, json};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::provider_catalog::{
    LoginProviderAuthKind, LoginProviderDescriptor, LoginProviderTarget,
};

#[derive(Clone, Default, Serialize)]
pub(super) struct ConnectionStatus {
    pub provider: String,
    pub phase: String,
    pub detail: String,
}

#[derive(Default)]
pub(super) struct ProviderConnections {
    status: Arc<Mutex<ConnectionStatus>>,
    input: Option<mpsc::Sender<String>>,
    cancel: Option<mpsc::Sender<()>>,
}

fn provider_for_id(id: &str) -> Option<LoginProviderDescriptor> {
    crate::provider_catalog::cli_login_providers()
        .into_iter()
        .find(|provider| provider.id == id && provider.target != LoginProviderTarget::AutoImport)
}

fn supports_key(provider: LoginProviderDescriptor) -> bool {
    matches!(
        provider.auth_kind,
        LoginProviderAuthKind::ApiKey | LoginProviderAuthKind::Hybrid
    ) || matches!(
        provider.target,
        LoginProviderTarget::OpenAi | LoginProviderTarget::Claude | LoginProviderTarget::Gemini
    )
}

fn supports_browser(provider: LoginProviderDescriptor) -> bool {
    matches!(
        provider.auth_kind,
        LoginProviderAuthKind::OAuth | LoginProviderAuthKind::DeviceCode
    )
}

fn login_arguments(
    provider: LoginProviderDescriptor,
    method: &str,
    base: Option<&str>,
) -> Result<Vec<String>> {
    use super::super::provider_init::{ProviderChoice, choice_for_login_provider};
    let choice = match (provider.target, method) {
        (LoginProviderTarget::OpenAi, "api_key") => ProviderChoice::OpenaiApi,
        (LoginProviderTarget::Claude, "api_key") => ProviderChoice::AnthropicApi,
        (_, "api_key") if supports_key(provider) => {
            choice_for_login_provider(provider).context("This provider has no CLI login")?
        }
        (_, "oauth") if supports_browser(provider) => {
            choice_for_login_provider(provider).context("This provider has no browser login")?
        }
        (_, "local") if provider.auth_kind == LoginProviderAuthKind::Local => {
            choice_for_login_provider(provider).context("This endpoint has no CLI setup")?
        }
        _ => bail!("This sign-in method is not supported by the selected provider"),
    };
    let name = choice
        .to_possible_value()
        .context("Invalid provider")?
        .get_name()
        .to_owned();
    let mut args = vec![
        "--no-update".into(),
        "--no-selfdev".into(),
        "login".into(),
        name,
    ];
    if let Some(base) = base.filter(|base| !base.trim().is_empty()) {
        if !matches!(provider.target, LoginProviderTarget::OpenAiCompatible(_)) {
            bail!("A custom endpoint is only supported by compatible providers");
        }
        let url = url::Url::parse(base.trim()).context("Enter a valid endpoint URL")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            bail!("Use an HTTP(S) endpoint without embedded credentials");
        }
        args.extend(["--api-base".into(), url.to_string()]);
    }
    Ok(args)
}

impl ProviderConnections {
    pub fn status(&self) -> ConnectionStatus {
        self.status
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn providers(&self) -> Value {
        let status = crate::alphacode_base::auth::AuthStatus::check();
        let providers: Vec<_> = crate::provider_catalog::cli_login_providers().into_iter()
            .filter(|provider| provider.target != LoginProviderTarget::AutoImport)
            .filter(|provider| super::super::provider_init::choice_for_login_provider(*provider).is_some())
            .map(|provider| {
                let assessment = status.assessment_for_provider(provider);
                json!({"id": provider.id, "name": provider.display_name,
                    "configured": assessment.is_configured(), "detail": assessment.health_summary(),
                    "oauth": supports_browser(provider), "api_key": supports_key(provider),
                    "local": provider.auth_kind == LoginProviderAuthKind::Local,
                    "endpoint": matches!(provider.target, LoginProviderTarget::OpenAiCompatible(_))})
            }).collect();
        json!({"ok": true, "providers": providers, "connection": self.status()})
    }

    pub fn start(&mut self, id: &str, method: &str, key: String, base: Option<&str>) -> Result<()> {
        if self.status().phase == "connecting" {
            bail!("Finish or cancel the current sign-in first");
        }
        let provider = provider_for_id(id).context("Unknown provider")?;
        let args = login_arguments(provider, method, base)?;
        if key.len() > 16_000 || key.contains(['\r', '\n']) {
            bail!("Enter a single-line API key");
        }
        if method == "api_key" && key.trim().is_empty() {
            bail!("Enter an API key");
        }
        let mut command = tokio::process::Command::new(std::env::current_exe()?);
        command
            .args(args)
            .env("ALPHACODE_GUI_BROWSER_LOGIN", "1")
            .env("ALPHACODE_GUI_LOGIN_METHOD", method)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command
            .spawn()
            .context("Could not start provider sign-in")?;
        let mut stdin = child
            .stdin
            .take()
            .context("Sign-in input pipe is unavailable")?;
        let secrets = Arc::new(Mutex::new(vec![key.clone()]));
        let stdout = child
            .stdout
            .take()
            .context("Sign-in output pipe is unavailable")?;
        let stderr = child
            .stderr
            .take()
            .context("Sign-in output pipe is unavailable")?;
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = ConnectionStatus {
            provider: id.into(),
            phase: "connecting".into(),
            detail: if method == "oauth" {
                "Complete sign-in in your browser. Device codes appear here."
            } else {
                "Connecting using the CLI credential store…"
            }
            .into(),
        };
        let out = tokio::spawn(read_progress(
            stdout,
            self.status.clone(),
            secrets.clone(),
            method != "api_key",
        ));
        let err = tokio::spawn(read_progress(
            stderr,
            self.status.clone(),
            secrets.clone(),
            method != "api_key",
        ));
        let (input, mut inputs) = mpsc::channel::<String>(4);
        let (cancel, mut cancels) = mpsc::channel::<()>(1);
        self.input = Some(input);
        self.cancel = Some(cancel);
        let status = self.status.clone();
        let initial_input = (method != "oauth").then_some(key);
        tokio::spawn(async move {
            if let Some(key) = initial_input {
                let _ = stdin.write_all(format!("{key}\n").as_bytes()).await;
                let _ = stdin.flush().await;
            }
            let timeout = tokio::time::sleep(std::time::Duration::from_secs(600));
            tokio::pin!(timeout);
            let (phase, detail) = loop {
                tokio::select! {
                    result = child.wait() => break match result {
                        Ok(exit) if exit.success() => ("completed", "Sign-in finished. Review the refreshed provider status below."),
                        _ => ("failed", "Sign-in did not complete. Review the provider message and try again."),
                    },
                    Some(value) = inputs.recv() => {
                        secrets.lock().unwrap_or_else(|e| e.into_inner()).push(value.clone());
                        if stdin.write_all(format!("{value}\n").as_bytes()).await.is_err() { break ("failed", "The sign-in input pipe closed."); }
                        let _ = stdin.flush().await;
                    },
                    _ = cancels.recv() => { let _ = child.kill().await; break ("cancelled", "Sign-in cancelled."); },
                    _ = &mut timeout => { let _ = child.kill().await; break ("failed", "Sign-in timed out. Start again when ready."); },
                }
            };
            let _ = out.await;
            let _ = err.await;
            crate::alphacode_base::auth::AuthStatus::invalidate_cached_status();
            let mut status = status.lock().unwrap_or_else(|e| e.into_inner());
            status.phase = phase.into();
            if phase == "completed" || phase == "cancelled" {
                status.detail = detail.into();
            } else {
                status.detail = format!("{detail}\n{}", status.detail);
            }
        });
        Ok(())
    }

    pub async fn send_input(&self, value: &str) -> Result<()> {
        if value.len() > 16_000 || value.contains(['\r', '\n']) {
            bail!("Enter one verification code or answer");
        }
        if self.status().phase != "connecting" {
            bail!("No sign-in is waiting for input");
        }
        self.input
            .as_ref()
            .context("No sign-in is active")?
            .send(value.into())
            .await
            .context("Sign-in has finished")
    }

    pub async fn cancel(&self) {
        if let Some(cancel) = &self.cancel {
            let _ = cancel.send(()).await;
        }
    }
}

async fn read_progress(
    mut reader: impl AsyncRead + Unpin,
    state: Arc<Mutex<ConnectionStatus>>,
    secrets: Arc<Mutex<Vec<String>>>,
    show_output: bool,
) {
    let mut buffer = [0; 1024];
    while let Ok(size) = reader.read(&mut buffer).await {
        if size == 0 {
            break;
        }
        // API-key setup output is never forwarded: even partial echoed keys
        // must not appear in UI status. OAuth/device prompts need their URLs/codes.
        if !show_output {
            continue;
        }
        let text =
            crate::message::strip_ansi_escape_sequences(&String::from_utf8_lossy(&buffer[..size]));
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        let mut detail = format!("{}{}", state.detail, text);
        for secret in secrets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|s| !s.is_empty())
        {
            detail = detail.replace(secret, "[redacted]");
        }
        state.detail = crate::util::truncate_str(&detail, 8000).to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_sign_in_rejects_unknown_providers_and_unsupported_methods() {
        assert!(provider_for_id("unknown;command").is_none());
        for provider in crate::provider_catalog::cli_login_providers() {
            assert!(login_arguments(provider, "unknown", None).is_err());
        }
    }
    #[test]
    fn api_keys_use_the_cli_key_provider_without_a_secret_argument() {
        let provider = provider_for_id("openai").expect("OpenAI login exists");
        let args = login_arguments(provider, "api_key", None).unwrap();
        assert_eq!(args, ["--no-update", "--no-selfdev", "login", "openai-api"]);
        assert!(!args.iter().any(|arg| arg.contains("api-key")));
    }
}
