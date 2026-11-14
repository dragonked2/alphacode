//! Opt-in, local provider HTTP dispatch accounting. Never records request data.
use std::future::Future;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

tokio::task_local! {
    static CONTEXT: (String, String);
    static SUPPRESSED: bool;
}

pub(super) async fn suppress<T>(future: impl Future<Output = T>) -> T {
    SUPPRESSED.scope(true, future).await
}

/// Attribute directly awaited work to one turn. Background tasks stay separate
/// unless explicitly spawned through `spawn`.
pub async fn scope<T>(session: String, future: impl Future<Output = T>) -> T {
    CONTEXT
        .scope((session, uuid::Uuid::new_v4().to_string()), future)
        .await
}

/// Provider stream workers must retain their caller's turn attribution.
pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let context = CONTEXT.try_with(Clone::clone).ok();
    tokio::spawn(async move {
        match context {
            Some(context) => CONTEXT.scope(context, future).await,
            None => future.await,
        }
    })
}

#[derive(Debug, serde::Serialize)]
struct Dispatch {
    unix_ms: u128,
    pid: u32,
    session: Option<String>,
    prompt: Option<String>,
    method: String,
    origin: String,
    kind: &'static str,
}

fn dispatch(request: &reqwest::Request) -> Dispatch {
    dispatch_parts(request.url(), request.method())
}

fn dispatch_parts(url: &reqwest::Url, method: &reqwest::Method) -> Dispatch {
    let context = CONTEXT.try_with(Clone::clone).ok();
    let path = url.path();
    let kind = if path.ends_with("/endpoints") {
        "endpoints"
    } else if path.ends_with("/models")
        || path.contains("/models/") && method == reqwest::Method::GET
    {
        "models"
    } else if path.contains("usage") || path.contains("credits") || path.contains("billing") {
        "usage"
    } else if path.contains("token") || path.contains("oauth") || path.contains("login") {
        "auth"
    } else if method == reqwest::Method::POST {
        "inference_or_other_post"
    } else {
        "other"
    };
    Dispatch {
        unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        pid: std::process::id(),
        session: context.as_ref().map(|value| value.0.clone()),
        prompt: context.map(|value| value.1),
        method: method.to_string(),
        origin: url.origin().ascii_serialization(),
        kind,
    }
}

/// Called after admission, immediately before execute. Counts dispatch attempts,
/// including transport failures; redirects/internal HTTP retries are invisible.
pub(super) fn record(request: &reqwest::Request) {
    write_dispatch(|| Some(dispatch(request)));
}

/// Account for protocol transports that do not use reqwest. Only the origin
/// survives parsing; paths, queries, headers, and payloads are never stored.
pub fn record_operation(url: &str, method: &str, kind: &'static str) {
    write_dispatch(|| {
        let url = reqwest::Url::parse(url).ok()?;
        let mut event = dispatch_parts(&url, &reqwest::Method::GET);
        event.method = method.to_owned();
        event.kind = kind;
        Some(event)
    });
}

fn write_dispatch(event: impl FnOnce() -> Option<Dispatch>) {
    if SUPPRESSED.try_with(|value| *value).unwrap_or(false) {
        return;
    }
    static FILE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
    let file = FILE.get_or_init(|| {
        let directory = std::env::var_os("ALPHACODE_PROVIDER_AUDIT_DIR")?;
        if directory.is_empty() {
            return None;
        }
        let directory = std::path::PathBuf::from(directory);
        if let Err(error) = std::fs::create_dir_all(&directory) {
            eprintln!("Provider audit disabled: cannot create output directory ({error})");
            return None;
        }
        let name = format!(
            "requests-{}-{}.jsonl",
            std::process::id(),
            uuid::Uuid::new_v4()
        );
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(directory.join(name))
        {
            Ok(file) => Some(Mutex::new(file)),
            Err(error) => {
                eprintln!("Provider audit disabled: cannot create output file ({error})");
                None
            }
        }
    });
    let Some(file) = file else {
        return;
    };
    if let Ok(mut file) = file.lock()
        && let Some(event) = event()
        && let Ok(mut line) = serde_json::to_vec(&event)
    {
        line.push(b'\n');
        if let Err(error) = file.write_all(&line) {
            static WARNED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!("Provider audit incomplete: output write failed ({error})");
            }
        }
    }
}

/// Same semantics as RequestBuilder::send, with dispatch accounting.
pub trait AuditedSend {
    fn audited_send(self) -> impl Future<Output = reqwest::Result<reqwest::Response>> + Send;
}

impl AuditedSend for reqwest::RequestBuilder {
    async fn audited_send(self) -> reqwest::Result<reqwest::Response> {
        let (client, request) = self.build_split();
        let request = request?;
        record(&request);
        client.execute(request).await
    }
}

pub trait BlockingAuditedSend {
    fn audited_send(self) -> reqwest::Result<reqwest::blocking::Response>;
}

impl BlockingAuditedSend for reqwest::blocking::RequestBuilder {
    fn audited_send(self) -> reqwest::Result<reqwest::blocking::Response> {
        // Blocking RequestBuilder has no build_split; try_clone retains its
        // original client and request semantics while allowing metadata access.
        if let Some(builder) = self.try_clone()
            && let Ok(request) = builder.build()
        {
            record_operation(
                request.url().as_str(),
                request.method().as_str(),
                "client_metadata",
            );
        }
        self.send()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn attribution_survives_stream_worker_and_excludes_background() {
        let request = reqwest::Client::new()
            .get("https://example.com/v1/models?key=secret")
            .header("Authorization", "secret")
            .build()
            .unwrap();
        assert!(dispatch(&request).prompt.is_none());
        let (first, second) = scope("fixture-session".into(), async {
            let first = dispatch(&request);
            let second = spawn(async move { dispatch(&request) }).await.unwrap();
            (first, second)
        })
        .await;
        assert_eq!(first.session.as_deref(), Some("fixture-session"));
        assert!(first.prompt.is_some());
        assert_eq!(first.prompt, second.prompt);
        assert_eq!(first.kind, "models");
        let json = serde_json::to_string(&first).unwrap();
        assert!(!json.contains("secret"));
        assert!(!json.contains("/v1"));
    }
}
