//! HTTP client for the OpenRouter embeddings API.
//!
//! Sends embedding requests to the OpenAI-compatible endpoint at
//! `openrouter.ai/api/v1/embeddings` and returns dense `Vec<f32>`
//! vectors. Handles retry with exponential backoff + jitter for
//! transient failures (429, 5xx) and immediate abort for permanent
//! errors (401, 400).

use crate::errors::AppError;
use crate::retry::AttemptOutcome;
use secrecy::{ExposeSecret, SecretBox};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const OPENROUTER_EMBEDDINGS_URL: &str = "https://openrouter.ai/api/v1/embeddings";
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;
const MAX_BATCH_SIZE: usize = 32;

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: EmbeddingInput<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<usize>,
    encoding_format: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_type: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum EmbeddingInput<'a> {
    Single(&'a str),
    Batch(Vec<&'a str>),
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

/// Envelope that captures BOTH shapes the OpenRouter embeddings endpoint can
/// return: the success payload (`data`) and the structured error object
/// (`error`). OpenRouter sometimes returns the error object inside an HTTP 200
/// body (e.g. token/context-length overflow); a direct parse to
/// [`EmbeddingResponse`] would fail with a misleading missing-field error,
/// masking the real cause. Both fields are optional so the branch is decided
/// by inspection, not by a parse failure.
#[derive(Deserialize)]
struct EmbeddingEnvelope {
    #[serde(default)]
    data: Option<Vec<EmbeddingData>>,
    #[serde(default)]
    error: Option<ApiError>,
}

// ApiError and code_string() moved to `crate::openrouter_http` (GAP-SG-74):
// this client and `crate::chat_api::OpenRouterChatClient` decode the exact
// same structured error envelope, so the type is shared instead of
// duplicated.
use crate::openrouter_http::ApiError;

/// [`OpenRouterClient::embed_single`] / [`OpenRouterClient::embed_batch`]
/// failure (reauditor addendum, mirrors [`crate::chat_api::ChatError`]).
///
/// `retry_class` is the retry verdict computed AT THE ORIGIN (the exact HTTP
/// status, or the provider's structured error `code`) via the same
/// [`crate::openrouter_http::status_retry_class`] /
/// [`crate::openrouter_http::provider_error_retry_class`] classifiers
/// [`crate::chat_api::OpenRouterChatClient`] uses (GAP-SG-74 DRY) — never
/// inferred downstream from `source.to_string()`. The enrich `re-embed`
/// consumer reads this field directly instead of pattern-matching the
/// formatted message.
#[derive(Debug)]
pub struct EmbedError {
    /// Underlying cause, preserved via `source()` rather than restated.
    pub source: AppError,
    /// Typed retry verdict computed where the failure originated (HTTP
    /// status / provider code), not by matching `source`'s message.
    pub retry_class: AttemptOutcome,
}

impl EmbedError {
    fn new(source: AppError, retry_class: AttemptOutcome) -> Self {
        Self {
            source,
            retry_class,
        }
    }
}

impl std::fmt::Display for EmbedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.source, f)
    }
}

impl std::error::Error for EmbedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Converts a bare `AppError` into an `EmbedError` with `retry_class:
/// HardFailure`. Used by the `?` operator on call sites that predate the
/// origin-typed classification (the GAP-SG-02 oversized-input guard, the
/// dimension-mismatch guard in [`OpenRouterClient::truncate_embedding`], and
/// the batch-size-mismatch check) — all of those are genuine permanent
/// client/config errors, never transient. Every `EmbedError` constructed
/// inside `execute_with_retry` uses [`EmbedError::new`] explicitly with a
/// retry verdict computed at the exact HTTP status / provider code instead.
impl From<AppError> for EmbedError {
    fn from(source: AppError) -> Self {
        Self::new(source, AttemptOutcome::HardFailure)
    }
}

/// Unwraps `EmbedError` back down to its `source`, discarding `retry_class`.
/// Lets the many pre-existing `?`-based callers of [`OpenRouterClient::embed_single`]
/// / [`OpenRouterClient::embed_batch`] (in [`crate::embedder`]) keep compiling
/// unchanged; callers that need the typed retry verdict (the enrich
/// `re-embed` path) should match on `EmbedError` directly instead of relying
/// on this conversion.
impl From<EmbedError> for AppError {
    fn from(err: EmbedError) -> Self {
        err.source
    }
}

pub struct OpenRouterClient {
    client: reqwest::Client,
    api_key: SecretBox<String>,
    model: String,
    dim: usize,
    supports_mrl: bool,
    default_input_type: Option<&'static str>,
    /// Endpoint each request is POSTed to. Always
    /// [`OPENROUTER_EMBEDDINGS_URL`] in production; only the test-only
    /// [`Self::new_with_url`] constructor repoints it at a local mock server
    /// (mirrors `crate::chat_api::OpenRouterChatClient`).
    base_url: String,
}

fn model_supports_mrl(model: &str) -> bool {
    model.contains("qwen3-embedding")
        || model.contains("text-embedding-3")
        || model.contains("gemini-embedding")
        || model.contains("llama-nemotron-embed")
        || model.contains("bge-m3")
}

fn model_default_input_type(model: &str) -> Option<&'static str> {
    if model.contains("llama-nemotron-embed") {
        Some("passage")
    } else if model.contains("mistral-embed") {
        None
    } else {
        Some("search_document")
    }
}

impl OpenRouterClient {
    pub fn new(api_key: SecretBox<String>, model: String, dim: usize) -> Result<Self, AppError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .connect_timeout(Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS))
            .user_agent("sqlite-graphrag/1.1.00")
            .build()
            .map_err(|e| AppError::Embedding(format!("failed to build HTTP client: {e}")))?;

        let supports_mrl = model_supports_mrl(&model);
        let default_input_type = model_default_input_type(&model);

        Ok(Self {
            client,
            api_key,
            model,
            dim,
            supports_mrl,
            default_input_type,
            base_url: OPENROUTER_EMBEDDINGS_URL.to_string(),
        })
    }

    /// Test-only constructor that POSTs to an arbitrary `base_url` (such as a
    /// `wiremock::MockServer`) instead of the public OpenRouter endpoint.
    /// Behaviour is otherwise identical to [`Self::new`].
    #[cfg(test)]
    fn new_with_url(
        api_key: SecretBox<String>,
        model: String,
        dim: usize,
        base_url: String,
    ) -> Result<Self, AppError> {
        let mut client = Self::new(api_key, model, dim)?;
        client.base_url = base_url;
        Ok(client)
    }

    pub fn default_input_type(&self) -> Option<&'static str> {
        self.default_input_type
    }

    pub async fn embed_single(
        &self,
        text: &str,
        input_type: Option<&str>,
    ) -> Result<Vec<f32>, EmbedError> {
        // GAP-SG-02: reject an input that would overflow the model's token
        // window BEFORE the HTTP request, surfacing a clear Validation error
        // instead of a provider context-length rejection paid for round-trip.
        crate::memory_guard::check_embedding_input_size(text)?;

        let request = EmbeddingRequest {
            model: &self.model,
            input: EmbeddingInput::Single(text),
            dimensions: if self.supports_mrl {
                Some(self.dim)
            } else {
                None
            },
            encoding_format: "float",
            input_type,
        };

        let response = self.execute_with_retry(&request).await?;

        let embedding = response
            .data
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Embedding("empty response from OpenRouter".into()))?
            .embedding;

        Ok(self.truncate_embedding(embedding)?)
    }

    pub async fn embed_batch(
        &self,
        texts: &[&str],
        input_type: Option<&str>,
    ) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // GAP-SG-02: validate every input before any HTTP request so an
        // oversized member of the batch fails fast as Validation rather than a
        // provider context-length rejection mid-batch.
        for text in texts {
            crate::memory_guard::check_embedding_input_size(text)?;
        }

        let mut all = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(MAX_BATCH_SIZE) {
            let request = EmbeddingRequest {
                model: &self.model,
                input: EmbeddingInput::Batch(chunk.to_vec()),
                dimensions: if self.supports_mrl {
                    Some(self.dim)
                } else {
                    None
                },
                encoding_format: "float",
                input_type,
            };

            let response = self.execute_with_retry(&request).await?;

            if response.data.len() != chunk.len() {
                return Err(AppError::Embedding(format!(
                    "expected {} embeddings, got {}",
                    chunk.len(),
                    response.data.len()
                ))
                .into());
            }

            let mut sorted = response.data;
            sorted.sort_by_key(|d| d.index);

            for d in sorted {
                all.push(self.truncate_embedding(d.embedding)?);
            }
        }

        Ok(all)
    }

    fn truncate_embedding(&self, embedding: Vec<f32>) -> Result<Vec<f32>, AppError> {
        if embedding.len() < self.dim {
            return Err(AppError::Embedding(format!(
                "embedding dimension {} < requested {}",
                embedding.len(),
                self.dim
            )));
        }
        if embedding.len() == self.dim {
            Ok(embedding)
        } else {
            Ok(embedding[..self.dim].to_vec())
        }
    }

    /// Runs the request/retry loop, classifying every failure into an
    /// [`EmbedError`] with `retry_class` set AT THE ORIGIN (the exact HTTP
    /// status, or the provider's structured error code) via the same
    /// classifiers [`crate::chat_api::OpenRouterChatClient`] uses
    /// (GAP-SG-74 DRY) — never inferred downstream from a formatted message
    /// (reauditor addendum).
    async fn execute_with_retry(
        &self,
        request: &EmbeddingRequest<'_>,
    ) -> Result<EmbeddingResponse, EmbedError> {
        let mut last_err: Option<EmbedError> = None;

        for attempt in 0..crate::openrouter_http::MAX_RETRIES {
            let result = self
                .client
                .post(&self.base_url)
                .header(
                    "Authorization",
                    format!("Bearer {}", self.api_key.expose_secret()),
                )
                .json(request)
                .send()
                .await;

            let resp = match result {
                Ok(r) => r,
                Err(e) if e.is_timeout() => {
                    return Err(EmbedError::new(
                        AppError::Embedding("OpenRouter request timed out".into()),
                        AttemptOutcome::Transient,
                    ));
                }
                Err(e) => {
                    last_err = Some(EmbedError::new(
                        AppError::Embedding(format!("HTTP request failed: {e}")),
                        AttemptOutcome::Transient,
                    ));
                    crate::openrouter_http::backoff(attempt).await;
                    continue;
                }
            };

            let status = resp.status();

            if status.is_success() {
                let body = resp.text().await.map_err(|e| {
                    EmbedError::new(
                        AppError::Embedding(format!("failed to read response body: {e}")),
                        AttemptOutcome::Transient,
                    )
                })?;
                match serde_json::from_str::<EmbeddingEnvelope>(&body) {
                    Ok(env) => {
                        // A structured error object inside a 2xx body is
                        // classified by its own `code` (GAP-SG-01 surfaces
                        // the real code/message instead of masking it as a
                        // parse failure).
                        if let Some(api_err) = env.error {
                            let retry_class =
                                crate::openrouter_http::provider_error_retry_class(&api_err);
                            return Err(EmbedError::new(
                                AppError::ProviderError {
                                    code: api_err.code_string(),
                                    message: api_err.message,
                                },
                                retry_class,
                            ));
                        }
                        match env.data {
                            Some(data) => return Ok(EmbeddingResponse { data }),
                            None => {
                                tracing::warn!(
                                    attempt,
                                    body_len = body.len(),
                                    "HTTP 200 with neither data nor error (retrying)"
                                );
                                last_err = Some(EmbedError::new(
                                    AppError::Embedding(
                                        "OpenRouter 200 response had neither data nor error".into(),
                                    ),
                                    AttemptOutcome::Transient,
                                ));
                                crate::openrouter_http::backoff(attempt).await;
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            attempt,
                            body_len = body.len(),
                            "HTTP 200 but JSON unparseable (retrying): {e}"
                        );
                        last_err = Some(EmbedError::new(
                            AppError::Embedding(format!("failed to parse embedding response: {e}")),
                            AttemptOutcome::Transient,
                        ));
                        crate::openrouter_http::backoff(attempt).await;
                        continue;
                    }
                }
            }

            if status.as_u16() == 401 {
                return Err(EmbedError::new(
                    AppError::Embedding("invalid OpenRouter API key (HTTP 401)".into()),
                    AttemptOutcome::HardFailure,
                ));
            }

            if status.as_u16() == 400 || status.as_u16() == 404 {
                let body = resp.text().await.unwrap_or_default();
                return Err(EmbedError::new(
                    AppError::Embedding(format!("OpenRouter returned {status}: {body}")),
                    AttemptOutcome::HardFailure,
                ));
            }

            if status.as_u16() == 429 {
                let retry_after = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(2);
                tracing::warn!(
                    attempt,
                    retry_after_secs = retry_after,
                    "OpenRouter rate limited, waiting"
                );
                // GAP-SG-56: surface the Retry-After delay to the caller. If
                // every attempt is rate limited, the loop exits with this
                // RateLimited error (retryable) carrying the server-advised
                // wait, instead of a generic max-retries-exceeded message.
                last_err = Some(EmbedError::new(
                    AppError::RateLimited {
                        detail: format!("OpenRouter HTTP 429 (retry-after {retry_after}s)"),
                    },
                    AttemptOutcome::Transient,
                ));
                tokio::time::sleep(Duration::from_secs(retry_after)).await;
                continue;
            }

            if status.is_server_error() {
                tracing::warn!(attempt, status = %status, "OpenRouter server error, retrying");
                last_err = Some(EmbedError::new(
                    AppError::Embedding(format!("OpenRouter server error: {status}")),
                    AttemptOutcome::Transient,
                ));
                crate::openrouter_http::backoff(attempt).await;
                continue;
            }

            let body = resp.text().await.unwrap_or_default();
            return Err(EmbedError::new(
                AppError::Embedding(format!("unexpected HTTP {status}: {body}")),
                crate::openrouter_http::status_retry_class(status),
            ));
        }

        // Reauditor addendum: exhausting every retry against a transient
        // condition (429/5xx/timeout/network) is ITSELF transient — it is
        // exactly the case the queue's re-embed `--max-attempts` backoff
        // covers, and must never be reclassified as a permanent failure.
        Err(last_err.unwrap_or_else(|| {
            EmbedError::new(
                AppError::Embedding("max retries exceeded for OpenRouter request".into()),
                AttemptOutcome::Transient,
            )
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supports_mrl_detection() {
        assert!(model_supports_mrl("qwen/qwen3-embedding-8b"));
        assert!(model_supports_mrl("qwen/qwen3-embedding-4b"));
        assert!(model_supports_mrl("openai/text-embedding-3-small"));
        assert!(model_supports_mrl("openai/text-embedding-3-large"));
        assert!(model_supports_mrl("google/gemini-embedding-001"));
        assert!(model_supports_mrl("google/gemini-embedding-2"));
        assert!(model_supports_mrl(
            "nvidia/llama-nemotron-embed-vl-1b-v2:free"
        ));
        assert!(model_supports_mrl("baai/bge-m3"));

        assert!(!model_supports_mrl("perplexity/pplx-embed-v1-0.6b"));
        assert!(!model_supports_mrl("mistralai/mistral-embed-2312"));
        assert!(!model_supports_mrl("some-random-model"));
    }

    #[test]
    fn test_model_default_input_type() {
        assert_eq!(
            model_default_input_type("nvidia/llama-nemotron-embed-vl-1b-v2:free"),
            Some("passage")
        );
        assert_eq!(
            model_default_input_type("mistralai/mistral-embed-2312"),
            None
        );
        assert_eq!(
            model_default_input_type("qwen/qwen3-embedding-8b"),
            Some("search_document")
        );
        assert_eq!(
            model_default_input_type("openai/text-embedding-3-small"),
            Some("search_document")
        );
        assert_eq!(
            model_default_input_type("baai/bge-m3"),
            Some("search_document")
        );
    }

    #[test]
    fn test_truncate_embedding() {
        let api_key = SecretBox::new(Box::new("test-key".to_string()));
        let client = OpenRouterClient::new(api_key, "test-model".into(), 3).unwrap();

        let full = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let truncated = client.truncate_embedding(full).unwrap();
        assert_eq!(truncated, vec![1.0, 2.0, 3.0]);

        let exact = vec![1.0, 2.0, 3.0];
        let kept = client.truncate_embedding(exact).unwrap();
        assert_eq!(kept, vec![1.0, 2.0, 3.0]);

        let short = vec![1.0, 2.0];
        let err = client.truncate_embedding(short);
        assert!(err.is_err());
    }

    #[test]
    fn embedding_envelope_surfaces_provider_error_not_missing_field() {
        // GAP-SG-01: a 200 body carrying an OpenRouter error object must yield
        // the REAL message, not the misleading missing-field parse failure.
        let body = r#"{"error":{"code":400,"message":"context length exceeded"}}"#;

        // Precondition: the legacy optimistic parse masked the cause. Match
        // instead of unwrap_err so EmbeddingResponse need not derive Debug.
        let legacy_err = match serde_json::from_str::<EmbeddingResponse>(body) {
            Ok(_) => panic!("legacy parse should have failed on an error body"),
            Err(e) => e.to_string(),
        };
        assert!(
            legacy_err.contains("missing field"),
            "precondition: legacy parse masks the cause as a missing field: {legacy_err}"
        );

        // The envelope captures the structured error instead.
        let env: EmbeddingEnvelope =
            serde_json::from_str(body).expect("envelope parses an error body");
        assert!(env.data.is_none());
        let api_err = env.error.expect("error object captured");
        assert_eq!(api_err.message, "context length exceeded");
        assert_eq!(api_err.code_string(), "400");
    }

    #[test]
    fn embedding_envelope_parses_success_body() {
        let body = r#"{"data":[{"embedding":[1.0,2.0,3.0],"index":0}]}"#;
        let env: EmbeddingEnvelope =
            serde_json::from_str(body).expect("envelope parses a success body");
        assert!(env.error.is_none());
        let data = env.data.expect("data present");
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].embedding, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn api_error_code_string_handles_number_string_and_missing() {
        let num: ApiError = serde_json::from_str(r#"{"code":429,"message":"slow down"}"#).unwrap();
        assert_eq!(num.code_string(), "429");

        let s: ApiError =
            serde_json::from_str(r#"{"code":"rate_limited","message":"slow down"}"#).unwrap();
        assert_eq!(s.code_string(), "rate_limited");

        let missing: ApiError = serde_json::from_str(r#"{"message":"oops"}"#).unwrap();
        assert_eq!(missing.code_string(), "unknown");
    }

    #[tokio::test]
    async fn embed_single_rejects_oversized_input_before_request() {
        // GAP-SG-02: an input above EMBEDDING_REQUEST_MAX_TOKENS must fail as
        // Validation WITHOUT any network call. The fake key/URL would error
        // distinctly (Embedding) if the guard let the request through.
        let api_key = SecretBox::new(Box::new("test-key".to_string()));
        let client = OpenRouterClient::new(api_key, "qwen/qwen3-embedding-8b".into(), 384).unwrap();
        let big = "word ".repeat(crate::constants::EMBEDDING_REQUEST_MAX_TOKENS + 5_000);
        match client.embed_single(&big, None).await {
            Err(EmbedError {
                source: AppError::Validation(msg),
                retry_class,
            }) => {
                assert!(msg.contains("tokens"));
                assert_eq!(
                    retry_class,
                    AttemptOutcome::HardFailure,
                    "an oversized input is a permanent client error"
                );
            }
            other => unreachable!("expected Validation before request, got: {other:?}"),
        }
    }

    async fn client_for(server: &wiremock::MockServer, model: &str) -> OpenRouterClient {
        OpenRouterClient::new_with_url(
            SecretBox::new(Box::new("test-key".to_string())),
            model.to_string(),
            384,
            format!("{}/embeddings", server.uri()),
        )
        .expect("test client builds")
    }

    #[tokio::test]
    async fn embed_single_401_is_hard_failure() {
        // Reauditor addendum: classification happens at the HTTP status, not
        // by matching the error message downstream.
        use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = client_for(&server, "qwen/qwen3-embedding-8b").await;
        let err = client
            .embed_single("hello", None)
            .await
            .expect_err("401 is an error");
        assert_eq!(err.retry_class, AttemptOutcome::HardFailure);
    }

    #[tokio::test]
    async fn embed_single_exhausted_5xx_is_transient() {
        // Reauditor addendum: exhausting every retry against a persistent
        // 5xx is TRANSIENT — the caller's --max-attempts is what eventually
        // dead-letters it, never a HardFailure from this layer.
        use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let client = client_for(&server, "qwen/qwen3-embedding-8b").await;
        let err = client
            .embed_single("hello", None)
            .await
            .expect_err("persistent 5xx exhausts retries");
        assert_eq!(err.retry_class, AttemptOutcome::Transient);
    }

    #[tokio::test]
    async fn embed_single_provider_error_code_classifies_by_code_not_message() {
        // Reauditor addendum: a 200 body carrying a structured provider error
        // is classified by its `code`, reusing the exact same classifier
        // `chat_api` uses (GAP-SG-74 DRY).
        use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "error": { "code": "context_length_exceeded", "message": "too many tokens" }
            })))
            .mount(&server)
            .await;

        let client = client_for(&server, "qwen/qwen3-embedding-8b").await;
        let err = client
            .embed_single("hello", None)
            .await
            .expect_err("provider error must surface");
        assert_eq!(err.retry_class, AttemptOutcome::HardFailure);
    }
}
