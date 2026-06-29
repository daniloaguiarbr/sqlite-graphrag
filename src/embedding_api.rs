//! HTTP client for the OpenRouter embeddings API.
//!
//! Sends embedding requests to the OpenAI-compatible endpoint at
//! `openrouter.ai/api/v1/embeddings` and returns dense `Vec<f32>`
//! vectors. Handles retry with exponential backoff + jitter for
//! transient failures (429, 5xx) and immediate abort for permanent
//! errors (401, 400).

use crate::errors::AppError;
use secrecy::{ExposeSecret, SecretBox};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const OPENROUTER_EMBEDDINGS_URL: &str = "https://openrouter.ai/api/v1/embeddings";
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;
const MAX_BATCH_SIZE: usize = 32;
const MAX_RETRIES: u32 = 4;

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

/// Structured OpenRouter error object. `code` is a `serde_json::Value` because
/// the provider sends it as either a JSON number or string depending on the
/// failure; `message` defaults to empty so a malformed error object never
/// re-introduces the missing-field masking.
#[derive(Deserialize)]
struct ApiError {
    #[serde(default)]
    code: Option<serde_json::Value>,
    #[serde(default)]
    message: String,
}

impl ApiError {
    /// Renders `code` as a plain string without JSON quoting, falling back to
    /// `unknown` when the provider omitted it.
    fn code_string(&self) -> String {
        match &self.code {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
            None => "unknown".to_string(),
        }
    }
}

pub struct OpenRouterClient {
    client: reqwest::Client,
    api_key: SecretBox<String>,
    model: String,
    dim: usize,
    supports_mrl: bool,
    default_input_type: Option<&'static str>,
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
            .user_agent("sqlite-graphrag/1.0.96")
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
        })
    }

    pub fn default_input_type(&self) -> Option<&'static str> {
        self.default_input_type
    }

    pub async fn embed_single(
        &self,
        text: &str,
        input_type: Option<&str>,
    ) -> Result<Vec<f32>, AppError> {
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

        self.truncate_embedding(embedding)
    }

    pub async fn embed_batch(
        &self,
        texts: &[&str],
        input_type: Option<&str>,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        if texts.is_empty() {
            return Ok(Vec::new());
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
                )));
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

    async fn execute_with_retry(
        &self,
        request: &EmbeddingRequest<'_>,
    ) -> Result<EmbeddingResponse, AppError> {
        let mut last_err = None;

        for attempt in 0..MAX_RETRIES {
            let result = self
                .client
                .post(OPENROUTER_EMBEDDINGS_URL)
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
                    return Err(AppError::Embedding("OpenRouter request timed out".into()));
                }
                Err(e) => {
                    last_err = Some(AppError::Embedding(format!("HTTP request failed: {e}")));
                    Self::backoff(attempt).await;
                    continue;
                }
            };

            let status = resp.status();

            if status.is_success() {
                let body = resp.text().await.map_err(|e| {
                    AppError::Embedding(format!("failed to read response body: {e}"))
                })?;
                match serde_json::from_str::<EmbeddingEnvelope>(&body) {
                    Ok(env) => {
                        // A structured error object inside a 2xx body is a
                        // PERMANENT provider rejection (e.g. context-length
                        // overflow). Surface the REAL code/message instead of
                        // masking it as a parse failure, and do not retry.
                        if let Some(api_err) = env.error {
                            return Err(AppError::ProviderError {
                                code: api_err.code_string(),
                                message: api_err.message,
                            });
                        }
                        match env.data {
                            Some(data) => return Ok(EmbeddingResponse { data }),
                            None => {
                                tracing::warn!(
                                    attempt,
                                    body_len = body.len(),
                                    "HTTP 200 with neither data nor error (retrying)"
                                );
                                last_err = Some(AppError::Embedding(
                                    "OpenRouter 200 response had neither data nor error".into(),
                                ));
                                Self::backoff(attempt).await;
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
                        last_err = Some(AppError::Embedding(format!(
                            "failed to parse embedding response: {e}"
                        )));
                        Self::backoff(attempt).await;
                        continue;
                    }
                }
            }

            if status.as_u16() == 401 {
                return Err(AppError::Embedding(
                    "invalid OpenRouter API key (HTTP 401)".into(),
                ));
            }

            if status.as_u16() == 400 || status.as_u16() == 404 {
                let body = resp.text().await.unwrap_or_default();
                return Err(AppError::Embedding(format!(
                    "OpenRouter returned {status}: {body}"
                )));
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
                last_err = Some(AppError::RateLimited {
                    detail: format!("OpenRouter HTTP 429 (retry-after {retry_after}s)"),
                });
                tokio::time::sleep(Duration::from_secs(retry_after)).await;
                continue;
            }

            if status.is_server_error() {
                tracing::warn!(attempt, status = %status, "OpenRouter server error, retrying");
                last_err = Some(AppError::Embedding(format!(
                    "OpenRouter server error: {status}"
                )));
                Self::backoff(attempt).await;
                continue;
            }

            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Embedding(format!(
                "unexpected HTTP {status}: {body}"
            )));
        }

        Err(last_err.unwrap_or_else(|| {
            AppError::Embedding("max retries exceeded for OpenRouter request".into())
        }))
    }

    async fn backoff(attempt: u32) {
        let base_ms = 1000u64 * 2u64.pow(attempt);
        let jitter = fastrand::u64(0..500);
        let sleep_ms = base_ms + jitter;
        tracing::debug!(attempt, sleep_ms, "exponential backoff");
        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
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
}
