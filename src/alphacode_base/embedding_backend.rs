//! Pluggable embedding backends for memory retrieval.
//!
//! Memory dense-retrieval embeds two kinds of text: stored memories (passages)
//! and the current query. Historically alphacode had exactly one embedder, the
//! bundled local all-MiniLM-L6-v2 ONNX model, reached directly via
//! [`crate::embedding`]. This module introduces a small abstraction so the
//! embedder can be swapped (e.g. a stronger local model, or a remote provider
//! like OpenAI when the user has an embeddings-capable key) without the rest of
//! the memory system caring which one is active.
//!
//! Design invariants:
//! - **One vector space per index.** Embeddings from different models are not
//!   comparable. Every backend reports a stable [`EmbeddingBackend::model_id`],
//!   which is stored on each `MemoryEntry` (`embedding_model`). Dense similarity
//!   only compares vectors sharing the active model id; mismatched memories stay
//!   reachable via lexical (BM25) search + RRF fusion, so switching backends
//!   never silently corrupts results.
//! - **Asymmetric query/passage formatting is per-model.** Some models (e5/bge)
//!   require instruction prefixes; others (MiniLM, OpenAI) do not. Each backend
//!   owns its own input formatting via [`EmbeddingBackend::format_query`] /
//!   [`EmbeddingBackend::format_passage`], so callers never hardcode prefixes.
//! - **Local is the default compatibility backend, not necessarily operational.** Remote backends are opt-in and
//!   only selected when an embeddings-capable credential is present.

use anyhow::{Context, Result};

use crate::memory_types::LEGACY_EMBEDDING_MODEL;

/// A source of embedding vectors for memory retrieval.
///
/// Implementations must keep `model_id()` stable for a given vector space: it is
/// persisted alongside each embedding and used to gate cross-model comparisons.
pub trait EmbeddingBackend: Send + Sync {
    /// Stable identifier for the model/vector-space this backend produces, e.g.
    /// `"minilm-l6-v2"` or `"openai:text-embedding-3-small"`. Persisted on
    /// `MemoryEntry::embedding_model`.
    fn model_id(&self) -> &str;

    /// Embedding dimensionality (used for sanity checks and index metadata).
    fn dim(&self) -> usize;

    /// Embed a single text already formatted for this backend's role. Prefer
    /// [`Self::embed_query`] / [`Self::embed_passage`] which apply formatting.
    fn embed_raw(&self, text: &str) -> Result<Vec<f32>>;

    /// Apply this model's query-side formatting (e.g. an `"query: "` prefix).
    /// Default: identity (no prefix), correct for MiniLM and OpenAI.
    fn format_query(&self, text: &str) -> String {
        text.to_string()
    }

    /// Apply this model's passage-side formatting (e.g. a `"passage: "` prefix).
    /// Default: identity.
    fn format_passage(&self, text: &str) -> String {
        text.to_string()
    }

    /// Embed a retrieval query (applies query formatting).
    fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        self.embed_raw(&self.format_query(text))
    }

    /// Embed a stored passage/memory (applies passage formatting).
    fn embed_passage(&self, text: &str) -> Result<Vec<f32>> {
        self.embed_raw(&self.format_passage(text))
    }

    /// Batch-embed many passages. Backends with a remote API override this to
    /// amortize one HTTP round-trip over many inputs; the default loops over
    /// [`Self::embed_passage`] so local backends need no special-casing.
    fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed_passage(t)).collect()
    }
}

/// The former bundled local ONNX embedder (all-MiniLM-L6-v2).
///
/// The ONNX/tokenizer stack has been removed from this build. This backend is
/// retained so the memory index keeps a stable vector-space tag for previously
/// embedded entries, but it no longer produces vectors: `embed_raw` fails and
/// dense retrieval degrades to lexical (BM25) search via RRF fusion, exactly as
/// it does for any unavailable backend. Configure
/// `agents.memory_embedding_backend = "openai"` for working dense retrieval.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalOnnxBackend;

impl EmbeddingBackend for LocalOnnxBackend {
    fn model_id(&self) -> &str {
        // Matches MemoryEntry::effective_embedding_model() for untagged legacy
        // memories, so existing embeddings remain comparable with new ones.
        LEGACY_EMBEDDING_MODEL
    }

    fn dim(&self) -> usize {
        crate::embedding::embedding_dim()
    }

    fn embed_raw(&self, text: &str) -> Result<Vec<f32>> {
        crate::embedding::embed(text)
    }

    // MiniLM is symmetric and prefix-free: default identity formatting is correct.
}

/// A remote OpenAI-compatible embeddings backend (`POST /v1/embeddings`).
///
/// Works against OpenAI proper and any OpenAI-compatible gateway that exposes
/// the same schema (the `base_url` + `Authorization: Bearer` contract). The
/// model id is stored as `"openai:<model>"` so its vectors never get compared
/// against local MiniLM vectors (different vector space): see the module
/// invariants. OpenAI embeddings are L2-normalized by the API, so plain cosine
/// over the returned vectors is correct.
#[derive(Clone)]
pub struct OpenAiEmbeddingBackend {
    /// Stable `"openai:<model>"` identifier persisted on each MemoryEntry.
    model_id: String,
    /// Bare model name sent in the request body (e.g. `text-embedding-3-small`).
    model: String,
    /// API base, no trailing slash, e.g. `https://api.openai.com/v1`.
    base_url: String,
    /// Bearer credential.
    api_key: String,
    /// Reported embedding dimensionality.
    dim: usize,
}

impl std::fmt::Debug for OpenAiEmbeddingBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiEmbeddingBackend")
            .field("model_id", &self.model_id)
            .field("dim", &self.dim)
            .finish_non_exhaustive()
    }
}

/// Default OpenAI embedding model. 3-small is the cost/quality sweet spot
/// (1536-d, ~5x cheaper than ada-002 with higher MTEB scores).
pub const DEFAULT_OPENAI_EMBEDDING_MODEL: &str = "text-embedding-3-small";
const OPENAI_EMBEDDINGS_BASE: &str = "https://api.openai.com/v1";

/// Native dimensionality for the common OpenAI embedding models.
fn default_openai_dim(model: &str) -> usize {
    match model {
        "text-embedding-3-small" => 1536,
        "text-embedding-3-large" => 3072,
        "text-embedding-ada-002" => 1536,
        _ => 1536,
    }
}

impl OpenAiEmbeddingBackend {
    /// Construct a backend for `model`, resolving `base_url`/`dim` to sensible
    /// defaults when not supplied.
    pub fn new(
        model: impl Into<String>,
        api_key: impl Into<String>,
        base_url: Option<String>,
        dim: Option<usize>,
    ) -> Self {
        let model = model.into();
        let dim = dim.unwrap_or_else(|| default_openai_dim(&model));
        let base_url = base_url
            .map(|b| b.trim_end_matches('/').to_string())
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| OPENAI_EMBEDDINGS_BASE.to_string());
        let mut model_id = format!("openai:{model}");
        if base_url != OPENAI_EMBEDDINGS_BASE {
            // Stable non-secret namespace for distinct compatible gateways.
            let fingerprint = base_url.bytes().fold(0xcbf29ce484222325u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
            });
            model_id.push_str(&format!(":endpoint-{fingerprint:016x}"));
        }
        if dim != default_openai_dim(&model) {
            model_id.push_str(&format!(":dim-{dim}"));
        }
        Self {
            model_id,
            model,
            base_url,
            api_key: api_key.into(),
            dim,
        }
    }

    /// POST a batch of inputs to `/embeddings` and return their vectors in order.
    ///
    /// Runs the blocking HTTP call on a dedicated thread so it is safe to call
    /// from a synchronous context (the bench) AND from inside a tokio runtime
    /// worker (the live memory path) without the nested-runtime panic that
    /// `reqwest::blocking` would otherwise trigger.
    fn embed_inputs(&self, inputs: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        anyhow::ensure!(
            self.dim > 0 && self.dim <= 65_536,
            "Invalid embedding dimension"
        );
        anyhow::ensure!(
            inputs.iter().all(|text| !text.trim().is_empty()),
            "Empty embedding input"
        );
        // Limit per-request work; larger batches are split by embed_passages.
        anyhow::ensure!(inputs.len() <= 64, "Embedding batch exceeds limit");
        anyhow::ensure!(
            inputs.iter().all(|text| text.len() <= 128 * 1024),
            "Embedding input exceeds byte limit"
        );
        let base = reqwest::Url::parse(&self.base_url)
            .map_err(|_| anyhow::anyhow!("Invalid embedding endpoint"))?;
        anyhow::ensure!(
            base.scheme() == "https"
                || (base.scheme() == "http"
                    && matches!(base.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))),
            "Embedding endpoint must use HTTPS or local loopback HTTP"
        );
        anyhow::ensure!(
            base.username().is_empty()
                && base.password().is_none()
                && base.query().is_none()
                && base.fragment().is_none(),
            "Invalid embedding endpoint components"
        );
        let url = format!("{}/embeddings", self.base_url);
        let api_key = self.api_key.clone();
        let model = self.model.clone();
        let want = inputs.len();
        let expected_dim = self.dim;

        let vectors = std::thread::scope(|scope| {
            scope
                .spawn(move || -> Result<Vec<Vec<f32>>> {
                    let client = reqwest::blocking::Client::builder()
                        .user_agent(crate::alphacode_provider_core::ALPHACODE_USER_AGENT)
                        .connect_timeout(std::time::Duration::from_secs(10))
                        .redirect(reqwest::redirect::Policy::none())
                        .timeout(std::time::Duration::from_secs(60))
                        .build()?;
                    let mut body = serde_json::json!({
                        "model": &model,
                        "input": inputs,
                    });
                    if model.starts_with("text-embedding-3-")
                        && expected_dim != default_openai_dim(&model)
                    {
                        body["dimensions"] = serde_json::json!(expected_dim);
                    }
                    let resp = client
                        .post(&url)
                        .header("Authorization", format!("Bearer {api_key}"))
                        .header("Content-Type", "application/json")
                        .json(&body)
                        .send()?;
                    let status = resp.status();
                    anyhow::ensure!(
                        status.is_success(),
                        "OpenAI embeddings request failed ({status})"
                    );
                    use std::io::Read;
                    const MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;
                    let mut bytes = Vec::new();
                    resp.take(MAX_RESPONSE_BYTES + 1).read_to_end(&mut bytes)?;
                    anyhow::ensure!(
                        bytes.len() as u64 <= MAX_RESPONSE_BYTES,
                        "Embedding response exceeds byte limit"
                    );
                    let parsed: EmbeddingsResponse =
                        serde_json::from_slice(&bytes).context("Invalid embeddings response")?;
                    validate_response(parsed, want, expected_dim)
                })
                .join()
                .map_err(|_| anyhow::anyhow!("OpenAI embeddings worker thread panicked"))?
        })?;

        Ok(vectors)
    }
}

#[derive(serde::Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingsDatum>,
}

#[derive(serde::Deserialize)]
struct EmbeddingsDatum {
    index: usize,
    embedding: Vec<f32>,
}

fn validate_response(
    mut response: EmbeddingsResponse,
    want: usize,
    dim: usize,
) -> Result<Vec<Vec<f32>>> {
    anyhow::ensure!(
        response.data.len() == want,
        "Unexpected embedding response count"
    );
    response.data.sort_unstable_by_key(|item| item.index);
    for (index, item) in response.data.iter().enumerate() {
        anyhow::ensure!(item.index == index, "Missing or duplicate embedding index");
        anyhow::ensure!(item.embedding.len() == dim, "Embedding dimension mismatch");
        anyhow::ensure!(
            item.embedding.iter().all(|v| v.is_finite())
                && item.embedding.iter().any(|v| *v != 0.0),
            "Invalid embedding vector"
        );
    }
    Ok(response
        .data
        .into_iter()
        .map(|item| item.embedding)
        .collect())
}

impl EmbeddingBackend for OpenAiEmbeddingBackend {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn embed_raw(&self, text: &str) -> Result<Vec<f32>> {
        let mut out = self.embed_inputs(vec![text.to_string()])?;
        out.pop()
            .ok_or_else(|| anyhow::anyhow!("OpenAI embeddings returned no vector"))
    }

    fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut result = Vec::with_capacity(texts.len());
        for batch in texts.chunks(64) {
            let inputs = batch.iter().map(|text| self.format_passage(text)).collect();
            result.extend(self.embed_inputs(inputs)?);
        }
        Ok(result)
    }

    // OpenAI embeddings are symmetric and prefix-free: identity formatting.
}

/// Resolve the active embedding backend.
///
/// Selects the OpenAI/openai-compatible remote backend when the user has
/// opted in (`agents.memory_embedding_backend = "openai"`) AND an embeddings
/// credential is resolvable; otherwise falls back to the compatibility local
/// ONNX backend. Selection is intentionally conservative: a misconfigured or
/// keyless remote setting selects the local compatibility backend.
/// Callers must handle unavailable dense retrieval using lexical search.
pub fn active_backend() -> Box<dyn EmbeddingBackend> {
    if let Some(remote) = openai_backend_from_config() {
        return Box::new(remote);
    }
    Box::new(LocalOnnxBackend)
}

/// Build an [`OpenAiEmbeddingBackend`] from config + resolved credentials, or
/// `None` when remote embeddings are not selected/available.
pub fn openai_backend_from_config() -> Option<OpenAiEmbeddingBackend> {
    let agents = &crate::config::config().agents;
    if !agents
        .memory_embedding_backend
        .eq_ignore_ascii_case("openai")
    {
        return None;
    }
    let api_key =
        crate::provider_catalog::load_api_key_from_env_or_config("OPENAI_API_KEY", "openai.env")?;
    let model = agents
        .memory_embedding_model
        .clone()
        .unwrap_or_else(|| DEFAULT_OPENAI_EMBEDDING_MODEL.to_string());
    let base_url = agents.memory_embedding_base_url.clone();
    let dim = agents.memory_embedding_dim;
    Some(OpenAiEmbeddingBackend::new(model, api_key, base_url, dim))
}

/// The model id (vector-space tag) of the currently active backend.
///
/// Persisted on freshly embedded memories and used to gate cross-model dense
/// comparisons. When remote embeddings are not active this is the local
/// MiniLM tag, matching legacy untagged memories.
pub fn active_model_id() -> String {
    active_backend().model_id().to_string()
}

/// Whether `entry_model` (an entry's `effective_embedding_model()`) shares a
/// vector space with the active backend, so their dense cosine is meaningful.
pub fn model_matches_active(entry_model: &str) -> bool {
    entry_model == active_model_id()
}

/// Embed a retrieval QUERY with the active backend, returning the vector and the
/// backend's model id. The local backend round-trips through the cached
/// `crate::embedding` facade; remote backends call their API directly.
pub fn embed_query_active(text: &str) -> anyhow::Result<(Vec<f32>, String)> {
    let backend = active_backend();
    let vec = backend.embed_query(text)?;
    Ok((vec, backend.model_id().to_string()))
}

/// Embed a stored PASSAGE/memory with the active backend, returning the vector
/// and the backend's model id (to persist on the entry for space-gating).
pub fn embed_passage_active(text: &str) -> anyhow::Result<(Vec<f32>, String)> {
    let backend = active_backend();
    let vec = backend.embed_passage(text)?;
    Ok((vec, backend.model_id().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_backend_model_id_matches_legacy_tag() {
        // Critical for backward compatibility: the local backend's model id must
        // equal the legacy tag so pre-tagging memories stay in the same space.
        assert_eq!(LocalOnnxBackend.model_id(), LEGACY_EMBEDDING_MODEL);
    }

    #[test]
    fn default_formatting_is_identity() {
        let b = LocalOnnxBackend;
        assert_eq!(b.format_query("hello"), "hello");
        assert_eq!(b.format_passage("world"), "world");
    }

    #[test]
    fn openai_backend_namespaces_model_id_and_infers_dim() {
        let b = OpenAiEmbeddingBackend::new("text-embedding-3-small", "sk-x", None, None);
        // model_id must be namespaced so OpenAI vectors never compare against
        // local MiniLM vectors (different vector spaces).
        assert_eq!(b.model_id(), "openai:text-embedding-3-small");
        assert_ne!(b.model_id(), LocalOnnxBackend.model_id());
        assert_eq!(b.dim(), 1536);

        let large = OpenAiEmbeddingBackend::new("text-embedding-3-large", "sk-x", None, None);
        assert_eq!(large.dim(), 3072);

        // Explicit dim override wins (for truncated 3-small/3-large indexes).
        let truncated =
            OpenAiEmbeddingBackend::new("text-embedding-3-large", "sk-x", None, Some(256));
        assert_eq!(truncated.dim(), 256);
    }
    #[test]
    fn embedding_response_rejects_duplicate_indices_and_bad_vectors() {
        let data = |index, embedding| EmbeddingsDatum { index, embedding };
        assert!(
            validate_response(
                EmbeddingsResponse {
                    data: vec![data(0, vec![1.0, 0.0]), data(0, vec![1.0, 0.0])]
                },
                2,
                2
            )
            .is_err()
        );
        assert!(
            validate_response(
                EmbeddingsResponse {
                    data: vec![data(0, vec![0.0, 0.0])]
                },
                1,
                2
            )
            .is_err()
        );
        assert!(
            validate_response(
                EmbeddingsResponse {
                    data: vec![data(0, vec![f32::NAN, 1.0])]
                },
                1,
                2
            )
            .is_err()
        );
    }

    #[test]
    fn debug_does_not_disclose_key_and_spaces_are_distinct() {
        let a =
            OpenAiEmbeddingBackend::new("text-embedding-3-small", "secret-key-test", None, None);
        let b = OpenAiEmbeddingBackend::new("text-embedding-3-small", "key", None, Some(256));
        let c = OpenAiEmbeddingBackend::new(
            "text-embedding-3-small",
            "key",
            Some("https://example.invalid/v1".into()),
            None,
        );
        assert!(!format!("{a:?}").contains("secret-key-test"));
        assert_ne!(a.model_id(), b.model_id());
        assert_ne!(a.model_id(), c.model_id());
    }
}
