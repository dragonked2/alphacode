//! Bounded reconnaissance with explicit status evidence and a missing-page control.
use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::time::Duration;
use url::Url;

pub struct CtfWebReconTool;

#[derive(Deserialize)]
struct Input {
    url: String,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    headers: HashMap<String, String>,
}

fn targets(input: &Input) -> Result<Vec<Url>> {
    let base = Url::parse(&input.url)?;
    if !matches!(base.scheme(), "http" | "https")
        || base.host_str().is_none()
        || !base.username().is_empty()
        || base.password().is_some()
    {
        bail!("Use an authorized HTTP(S) URL without embedded credentials");
    }
    if input.paths.len() > 24 {
        bail!("At most 24 evidence-selected paths per recon pass");
    }
    let mut urls = vec![
        base.clone(),
        base.join(&format!("/__alphacode_missing_{}", uuid::Uuid::new_v4()))?,
    ];
    for path in ["/robots.txt", "/sitemap.xml", "/.well-known/security.txt"]
        .into_iter()
        .chain(input.paths.iter().map(String::as_str))
    {
        let url = base.join(path)?;
        if url.origin() != base.origin() || !url.username().is_empty() || url.password().is_some() {
            bail!("Recon paths must stay on the authorized origin");
        }
        urls.push(url);
    }
    let mut seen = HashSet::new();
    urls.retain(|url| seen.insert(url.as_str().to_string()));
    Ok(urls)
}

fn classify(status: u16) -> &'static str {
    match status {
        200..=299 => "success",
        300..=399 => "redirect_not_followed",
        401 => "authentication_required",
        403 => "forbidden_existence_unconfirmed",
        404 => "not_found",
        429 => "rate_limited",
        500..=599 => "server_error_not_proof_of_exploit",
        _ => "other",
    }
}

async fn inspect(client: &reqwest::Client, url: Url) -> Result<Value> {
    let mut response = client.get(url.clone()).send().await?;
    let status = response.status().as_u16();
    let mut headers = serde_json::Map::new();
    for name in [
        "server",
        "x-powered-by",
        "content-type",
        "location",
        "allow",
        "www-authenticate",
        "retry-after",
    ] {
        if let Some(value) = response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
        {
            headers.insert(name.into(), json!(value));
        }
    }
    let mut sample = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = response.chunk().await? {
        let remaining = (64 * 1024usize).saturating_sub(sample.len());
        if chunk.len() > remaining {
            truncated = true;
        }
        sample.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if sample.len() >= 64 * 1024 {
            truncated = true;
            break;
        }
    }
    let mut hash = DefaultHasher::new();
    sample.hash(&mut hash);
    Ok(
        json!({"url":url.as_str(), "status":status, "classification":classify(status), "headers":headers,
        "sample_bytes":sample.len(), "sample_hash":format!("{:016x}",hash.finish()), "truncated":truncated,
        "untrusted_body_preview":String::from_utf8_lossy(&sample).chars().take(512).collect::<String>()}),
    )
}

#[async_trait]
impl Tool for CtfWebReconTool {
    fn name(&self) -> &str {
        "ctf_web_recon"
    }
    fn description(&self) -> &str {
        "Bounded authorized web-CTF recon: fingerprints headers, compares a random missing-page control, and maps selected same-origin paths. Keeps 401/403/404/500 separate. No password guessing, redirects, exploit battery, or automatic solve claims. Read challenge hints first."
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{"intent":super::intent_schema_property(),"url":{"type":"string"},"headers":{"type":"object","additionalProperties":{"type":"string"},"description":"Optional authorized session headers, such as Cookie or Authorization. Host overrides are not supported. Header values are not included in the report."},"paths":{"type":"array","maxItems":24,"items":{"type":"string"},"description":"Paths selected from challenge hints or observed application evidence; same origin only."}},"required":["url"]})
    }
    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let input: Input = serde_json::from_value(input)?;
        let targets = targets(&input)?;
        let mut headers = reqwest::header::HeaderMap::new();
        for (name, value) in &input.headers {
            let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())?;
            if name == reqwest::header::HOST {
                bail!("Host overrides can widen scope; use the authorized URL's hostname");
            }
            headers.insert(name, reqwest::header::HeaderValue::from_str(value)?);
        }
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(6))
            .user_agent("AlphaCode-CTF-Recon/1.0")
            .build()?;
        let total_requests = targets.len();
        let mut rows = Vec::new();
        let max_rps = super::scope::session_scope_policy(&ctx.session_id)
            .map(|policy| policy.max_rate_rps)
            .filter(|rate| *rate > 0)
            .unwrap_or(10)
            .min(10);
        let pacing = Duration::from_secs_f64(1.0 / f64::from(max_rps));
        let report = tokio::time::timeout(Duration::from_secs(60), async {
            for url in targets {
                if super::scope::enforce_call(&ctx, self.name(), &json!({"url":url.as_str()})).is_err() {
                    rows.push(json!({"url":url.as_str(),"classification":"scope_denied","request_sent":false}));
                    continue;
                }
                let row = match inspect(&client, url.clone()).await {
                    Ok(row) => row,
                    Err(_) => json!({"url":url.as_str(),"classification":"transport_error","error":"Request failed or timed out; inspect transport separately"}),
                };
                let rate_limited = row["status"] == 429;
                rows.push(row);
                if rate_limited { break; }
                tokio::time::sleep(pacing).await;
            }
        }).await;
        Ok(ToolOutput::new(serde_json::to_string_pretty(
            &json!({"requests":rows,
                "timed_out":report.is_err(),
                "remaining_requests":total_requests.saturating_sub(rows.len()),
                "interpretation":"The second request is a missing-page control. Compare status, sampled body hash and size for wildcard routing. A 403 does not prove route existence. Headers are claims, not conclusive stack identification. No challenge solve is implied."}),
        )?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_cross_origin_paths_and_embedded_credentials() {
        for (url, paths) in [
            ("https://example.test/", vec!["https://other.test/".into()]),
            ("https://user:secret@example.test/", vec![]),
        ] {
            assert!(
                targets(&Input {
                    url: url.into(),
                    paths,
                    headers: HashMap::new(),
                })
                .is_err()
            );
        }
    }
    #[test]
    fn status_classes_preserve_important_negatives() {
        assert_ne!(classify(403), classify(404));
        assert_ne!(classify(401), classify(403));
        assert!(classify(403).contains("unconfirmed"));
        assert!(classify(500).contains("not_proof"));
    }

    #[tokio::test]
    async fn local_fixture_preserves_headers_and_does_not_follow_redirects() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 2048];
            // The fixture only needs the client to have sent *something*; the
            // byte count is irrelevant, so bind it to `_` to make the ignored
            // read amount explicit (clippy::unused_io_amount).
            let _ = stream.read(&mut request).await.unwrap();
            stream.write_all(b"HTTP/1.1 302 Found\r\nX-Powered-By: Express\r\nLocation: https://outside.test/\r\nContent-Length: 4\r\nConnection: close\r\n\r\nbody").await.unwrap();
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let row = inspect(&client, Url::parse(&format!("http://{address}/")).unwrap())
            .await
            .unwrap();
        assert_eq!(row["status"], 302);
        assert_eq!(row["headers"]["x-powered-by"], "Express");
        assert_eq!(row["sample_bytes"], 4);
        server.await.unwrap();
    }
}
