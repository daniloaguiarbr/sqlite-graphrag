//! HTTP client for the OpenRouter chat-completions API.
//!
//! Sends structured-output chat requests to the OpenAI-compatible endpoint
//! at `openrouter.ai/api/v1/chat/completions` and returns the parsed JSON
//! object the model produced under a strict `json_schema` `response_format`.
//!
//! This mirrors [`crate::embedding_api`] for the embeddings endpoint: same
//! retry/backoff policy (immediate abort on 401/400/404, `retry-after` on
//! 429, exponential backoff + jitter on 5xx) and the same minimal headers
//! (only `Authorization: Bearer`, no `HTTP-Referer`/`X-Title`).
//!
//! v1.0.95 (ADR-0054): adds an OpenRouter REST transport for the `enrich`
//! JUDGE so structured extraction no longer requires a locally installed
//! `claude` / `codex` / `opencode` CLI subprocess.

use crate::errors::AppError;
use secrecy::{ExposeSecret, SecretBox};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const OPENROUTER_CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
// GAP-SG-17: raised from 300 to 600 — the per-request fallback budget when a
// caller passes `0`. Dense bodies near the model's ~32K-token context ceiling
// regularly need more than five minutes to generate.
const DEFAULT_TIMEOUT_SECS: u64 = 600;
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;
const MAX_RETRIES: u32 = 4;

/// Fixed `json_schema` name sent in the `response_format`. OpenRouter only
/// requires a short identifier; the actual contract is carried by `schema`.
const SCHEMA_NAME: &str = "enrich_output";

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    response_format: ResponseFormat,
    provider: ProviderPrefs,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<ReasoningPrefs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: String,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: &'static str,
    json_schema: JsonSchemaSpec,
}

#[derive(Serialize)]
struct JsonSchemaSpec {
    name: &'static str,
    strict: bool,
    schema: serde_json::Value,
}

#[derive(Serialize)]
struct ProviderPrefs {
    require_parameters: bool,
}

#[derive(Serialize)]
struct ReasoningPrefs {
    enabled: bool,
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
    /// Structured provider error. OpenRouter may return this inside an HTTP 200
    /// body (e.g. token/context-length overflow); without it the response would
    /// parse into empty `choices` and surface the misleading "no structured
    /// content" error instead of the real cause (GAP-SG-03).
    #[serde(default)]
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct Choice {
    message: RespMessage,
}

#[derive(Deserialize)]
struct RespMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    cost: Option<f64>,
}

/// Structured OpenRouter error object carried under the `error` key. `code` is
/// a `serde_json::Value` because the provider sends it as either a JSON number
/// or string; `message` defaults to empty so a malformed error object never
/// masks the cause.
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

/// Process-wide OpenRouter chat client. Holds the model name so that callers
/// only thread the per-item prompt/schema/input through [`Self::complete`].
pub struct OpenRouterChatClient {
    client: reqwest::Client,
    api_key: SecretBox<String>,
    model: String,
    /// Endpoint each request is POSTed to. Always [`OPENROUTER_CHAT_URL`] in
    /// production; only the test-only [`Self::new_with_url`] constructor
    /// repoints it at a local mock server.
    base_url: String,
}

impl OpenRouterChatClient {
    /// Builds a chat client bound to `model`, applying `timeout_secs` as the
    /// total per-request budget (wired from `--openrouter-timeout`). A value of
    /// `0` falls back to `DEFAULT_TIMEOUT_SECS` so a missing or zero flag never
    /// degrades into reqwest`'s immediate-timeout behaviour.
    pub fn new(
        api_key: SecretBox<String>,
        model: String,
        timeout_secs: u64,
    ) -> Result<Self, AppError> {
        let timeout_secs = if timeout_secs == 0 {
            DEFAULT_TIMEOUT_SECS
        } else {
            timeout_secs
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .connect_timeout(Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS))
            .user_agent("sqlite-graphrag/1.0.95")
            .build()
            .map_err(|e| AppError::Validation(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            client,
            api_key,
            model,
            base_url: OPENROUTER_CHAT_URL.to_string(),
        })
    }

    /// Test-only constructor that POSTs to an arbitrary `base_url` (such as a
    /// `wiremock::MockServer`) instead of the public OpenRouter endpoint.
    /// Behaviour is otherwise identical to [`Self::new`].
    #[cfg(test)]
    pub fn new_with_url(
        api_key: SecretBox<String>,
        model: String,
        base_url: String,
        timeout_secs: u64,
    ) -> Result<Self, AppError> {
        let mut client = Self::new(api_key, model, timeout_secs)?;
        client.base_url = base_url;
        Ok(client)
    }

    /// Returns the model bound to this client.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Runs a single structured-output completion.
    ///
    /// `schema_str` is the JSON Schema (as a string) the model must honour
    /// under `strict: true`. When `input_text` is empty only the system
    /// message is sent. Returns `(value, cost_usd, is_oauth)` where `value`
    /// is the model output parsed as JSON, `cost_usd` is read from
    /// `usage.cost` (or `0.0` when absent), and `is_oauth` is always `false`
    /// because OpenRouter uses an API key, not OAuth.
    pub async fn complete(
        &self,
        system_prompt: &str,
        input_text: &str,
        schema_str: &str,
        max_tokens: Option<u32>,
    ) -> Result<(serde_json::Value, f64, bool), AppError> {
        let schema: serde_json::Value = serde_json::from_str(schema_str).map_err(|e| {
            AppError::Validation(format!("invalid JSON schema for OpenRouter request: {e}"))
        })?;

        // First attempt sends reasoning.enabled=false (token savings on the
        // ~9 models that allow disabling). The ~4 reasoning-mandatory models
        // (e.g. minimax-m2.7, gpt-oss-120b) reject it with HTTP 400 mentioning
        // "reasoning"; on that specific failure we retry ONCE with the
        // reasoning field omitted so the model uses its mandatory default. Any
        // other error, or a second failure, propagates the original error.
        let primary = self.build_request(
            schema.clone(),
            system_prompt,
            input_text,
            max_tokens,
            Some(ReasoningPrefs { enabled: false }),
        );
        let response = match self.execute_with_retry(&primary).await {
            Ok(r) => r,
            Err(first_err) => {
                if reasoning_disable_rejected(&first_err) {
                    tracing::warn!(
                        model = %self.model,
                        "model rejected reasoning.enabled=false (mandatory); \
                         retrying once with reasoning omitted"
                    );
                    let fallback =
                        self.build_request(schema, system_prompt, input_text, max_tokens, None);
                    match self.execute_with_retry(&fallback).await {
                        Ok(r) => r,
                        Err(_) => return Err(first_err),
                    }
                } else {
                    return Err(first_err);
                }
            }
        };

        let content = response
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .filter(|c| !c.trim().is_empty())
            .ok_or_else(|| {
                AppError::Validation(format!(
                    "model '{}' returned no structured content (incompatible with \
                     structured outputs, or refused the request)",
                    self.model
                ))
            })?;

        // GAP-SG-10: deepseek-v4-flash:nitro and similar models do not honour
        // `json_schema` strict mode reliably — they wrap output in markdown
        // fences, add trailing commas, or omit quotes around keys. Try a strict
        // parse first (zero cost for well-formed JSON), then fall back to the
        // repair pass (a Rust port of `json_repair`) before giving up.
        let value = crate::json_repair::repair_to_value(&content).map_err(|e| {
            AppError::Validation(format!(
                "model '{}' returned content that could not be parsed even after \
                 JSON repair: {e}",
                self.model
            ))
        })?;

        // GAP-SG-10: `llm_json` coerces aggressively — free text becomes a JSON
        // string, empty input becomes `{}`, a lone delimiter becomes `null`. The
        // enrich JUDGE contract is ALWAYS a JSON object, so a non-object result
        // here is a malformed/refused generation, NOT a usable value. Reject it
        // (the enrich classifier reclassifies this as a transient model hiccup,
        // GAP-SG-09) instead of letting a coerced scalar masquerade as a
        // valid-but-empty result downstream.
        if !value.is_object() {
            return Err(AppError::Validation(format!(
                "model '{}' returned non-object JSON after repair (got {}); \
                 likely a refusal or malformed structured output",
                self.model,
                json_shape_name(&value)
            )));
        }

        let cost = response.usage.and_then(|u| u.cost).unwrap_or(0.0);

        Ok((value, cost, false))
    }

    /// Builds a `ChatRequest` for one attempt. `reasoning` is `Some` on the
    /// primary attempt (`enabled:false`) and `None` on the mandatory-reasoning
    /// fallback, where the field is omitted entirely.
    fn build_request<'a>(
        &'a self,
        schema: serde_json::Value,
        system_prompt: &str,
        input_text: &str,
        max_tokens: Option<u32>,
        reasoning: Option<ReasoningPrefs>,
    ) -> ChatRequest<'a> {
        let mut messages = Vec::with_capacity(2);
        messages.push(ChatMessage {
            role: "system",
            content: system_prompt.to_string(),
        });
        if !input_text.is_empty() {
            messages.push(ChatMessage {
                role: "user",
                content: input_text.to_string(),
            });
        }
        ChatRequest {
            model: &self.model,
            messages,
            response_format: ResponseFormat {
                format_type: "json_schema",
                json_schema: JsonSchemaSpec {
                    name: SCHEMA_NAME,
                    strict: true,
                    schema,
                },
            },
            provider: ProviderPrefs {
                require_parameters: true,
            },
            reasoning,
            max_tokens,
        }
    }

    async fn execute_with_retry(
        &self,
        request: &ChatRequest<'_>,
    ) -> Result<ChatResponse, AppError> {
        let mut last_err = None;

        for attempt in 0..MAX_RETRIES {
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
                    return Err(AppError::Validation(
                        "OpenRouter chat request timed out".into(),
                    ));
                }
                Err(e) => {
                    last_err = Some(AppError::Validation(format!("HTTP request failed: {e}")));
                    Self::backoff(attempt).await;
                    continue;
                }
            };

            let status = resp.status();

            if status.is_success() {
                let body = resp.text().await.map_err(|e| {
                    AppError::Validation(format!("failed to read response body: {e}"))
                })?;
                match serde_json::from_str::<ChatResponse>(&body) {
                    Ok(parsed) => {
                        // A structured error object inside a 2xx body is a
                        // PERMANENT provider rejection (e.g. context-length
                        // overflow). Surface the REAL code/message instead of
                        // letting empty choices masquerade as no-structured-
                        // content, and do not retry.
                        if let Some(api_err) = parsed.error {
                            return Err(AppError::ProviderError {
                                code: api_err.code_string(),
                                message: api_err.message,
                            });
                        }
                        return Ok(parsed);
                    }
                    Err(e) => {
                        tracing::warn!(
                            attempt,
                            body_len = body.len(),
                            "HTTP 200 but parse failed (retrying): {e}"
                        );
                        last_err = Some(AppError::Validation(format!(
                            "failed to parse chat response: {e}"
                        )));
                        Self::backoff(attempt).await;
                        continue;
                    }
                }
            }

            if status.as_u16() == 401 {
                return Err(AppError::Validation(
                    "invalid OpenRouter API key (HTTP 401)".into(),
                ));
            }

            if status.as_u16() == 400 || status.as_u16() == 404 {
                let body = resp.text().await.unwrap_or_default();
                return Err(AppError::Validation(format!(
                    "OpenRouter returned {status} for model '{}': {body}",
                    self.model
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
                last_err = Some(AppError::Validation(format!(
                    "OpenRouter server error: {status}"
                )));
                Self::backoff(attempt).await;
                continue;
            }

            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Validation(format!(
                "unexpected HTTP {status}: {body}"
            )));
        }

        Err(last_err.unwrap_or_else(|| {
            AppError::Validation("max retries exceeded for OpenRouter chat request".into())
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

/// True when an error from `execute_with_retry` indicates the model rejected
/// `reasoning.enabled=false` because reasoning is mandatory: an HTTP 400 whose
/// body mentions "reasoning" (case-insensitive). Triggers the one-shot retry
/// with the `reasoning` field omitted.
fn reasoning_disable_rejected(err: &AppError) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("400") && msg.contains("reasoning")
}

/// Names the JSON shape of `value` for diagnostics (GAP-SG-10). Used when the
/// repaired model output is not the object the enrich JUDGE contract requires.
fn json_shape_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_SCHEMA: &str = r#"{"type":"object"}"#;

    fn key() -> SecretBox<String> {
        SecretBox::new(Box::new("test-key".to_string()))
    }

    /// Builds a chat-completions success body whose single choice carries the
    /// model output as a JSON *string* (the double-encoding the real API uses
    /// under structured outputs), optionally attaching `usage.cost`.
    fn success_body(content: &str, cost: Option<f64>) -> serde_json::Value {
        let mut body = json!({
            "choices": [{ "message": { "content": content } }]
        });
        if let Some(c) = cost {
            body["usage"] = json!({ "cost": c });
        }
        body
    }

    async fn client_for(server: &MockServer, model: &str) -> OpenRouterChatClient {
        OpenRouterChatClient::new_with_url(
            key(),
            model.to_string(),
            format!("{}/chat/completions", server.uri()),
            30,
        )
        .expect("test client builds")
    }

    #[test]
    fn new_builds_client_and_binds_model() {
        let client = OpenRouterChatClient::new(key(), "z-ai/glm-5.2".to_string(), 30)
            .expect("client builds");
        assert_eq!(client.model(), "z-ai/glm-5.2");
    }

    #[test]
    fn new_defaults_base_url_to_public_endpoint() {
        let client = OpenRouterChatClient::new(key(), "z-ai/glm-5.2".to_string(), 30)
            .expect("client builds");
        assert_eq!(client.base_url, OPENROUTER_CHAT_URL);
    }

    #[test]
    fn request_serializes_with_strict_schema_and_disabled_reasoning() {
        let request = ChatRequest {
            model: "deepseek/deepseek-v4-flash",
            messages: vec![ChatMessage {
                role: "system",
                content: "extract".to_string(),
            }],
            response_format: ResponseFormat {
                format_type: "json_schema",
                json_schema: JsonSchemaSpec {
                    name: SCHEMA_NAME,
                    strict: true,
                    schema: serde_json::json!({"type": "object"}),
                },
            },
            provider: ProviderPrefs {
                require_parameters: true,
            },
            reasoning: Some(ReasoningPrefs { enabled: false }),
            max_tokens: None,
        };
        let json = serde_json::to_value(&request).expect("serializes");
        assert_eq!(json["response_format"]["type"], "json_schema");
        assert_eq!(json["response_format"]["json_schema"]["strict"], true);
        assert_eq!(json["provider"]["require_parameters"], true);
        assert_eq!(json["reasoning"]["enabled"], false);
        // max_tokens omitted when None
        assert!(json.get("max_tokens").is_none());
    }

    #[tokio::test]
    async fn complete_sends_wellformed_request_and_parses_content() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(body_partial_json(json!({
                "model": "deepseek/deepseek-v4-flash",
                "response_format": {
                    "type": "json_schema",
                    "json_schema": { "name": "enrich_output", "strict": true }
                },
                "provider": { "require_parameters": true },
                "reasoning": { "enabled": false }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_body(
                r#"{"entities":[],"relationships":[]}"#,
                Some(0.0023),
            )))
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(&server, "deepseek/deepseek-v4-flash").await;
        let (value, cost, is_oauth) = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect("completion succeeds");

        assert_eq!(value, json!({"entities": [], "relationships": []}));
        assert!((cost - 0.0023).abs() < f64::EPSILON);
        assert!(!is_oauth);
    }

    #[tokio::test]
    async fn complete_defaults_cost_to_zero_when_usage_absent() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(success_body(r#"{"entities":[]}"#, None)),
            )
            .mount(&server)
            .await;

        let client = client_for(&server, "z-ai/glm-5.2").await;
        let (_, cost, _) = client
            .complete("system", "", TEST_SCHEMA, Some(4096))
            .await
            .expect("completion succeeds");
        assert_eq!(cost, 0.0);
    }

    #[tokio::test]
    async fn complete_retries_on_429_honouring_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "1"))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(success_body(r#"{"ok":true}"#, Some(0.0))),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(&server, "minimax/minimax-m3").await;
        let (value, _, _) = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect("retried completion succeeds");
        assert_eq!(value, json!({"ok": true}));
    }

    #[tokio::test]
    async fn complete_retries_on_5xx_with_backoff() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(success_body(r#"{"ok":1}"#, Some(0.0))),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(&server, "openai/gpt-oss-120b").await;
        let (value, _, _) = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect("retried completion succeeds");
        assert_eq!(value, json!({"ok": 1}));
    }

    #[tokio::test]
    async fn complete_401_is_permanent_without_retry() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(&server, "z-ai/glm-5.2").await;
        let err = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect_err("401 is an error");
        assert!(err.to_string().contains("401"), "got: {err}");
    }

    #[tokio::test]
    async fn complete_400_returns_body_and_model_without_retry() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_string("schema not supported"))
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(&server, "xiaomi/mimo-v2.5").await;
        let err = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect_err("400 is an error");
        let msg = err.to_string();
        assert!(msg.contains("400"), "got: {msg}");
        assert!(msg.contains("xiaomi/mimo-v2.5"), "got: {msg}");
        assert!(msg.contains("schema not supported"), "got: {msg}");
    }

    #[tokio::test]
    async fn complete_empty_choices_errors_citing_model() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "choices": [] })))
            .mount(&server)
            .await;

        let client = client_for(&server, "minimax/minimax-m2.7").await;
        let err = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect_err("empty choices is an error");
        let msg = err.to_string();
        assert!(msg.contains("minimax/minimax-m2.7"), "got: {msg}");
        assert!(msg.contains("no structured content"), "got: {msg}");
    }

    #[tokio::test]
    async fn complete_empty_content_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_body("   ", Some(0.0))))
            .mount(&server)
            .await;

        let client = client_for(&server, "z-ai/glm-5.2:nitro").await;
        let err = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect_err("blank content is an error");
        assert!(
            err.to_string().contains("no structured content"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn complete_non_json_content_errors_as_incompatible() {
        // GAP-SG-10: free text is coerced by the repair pass into a JSON string
        // (not an object), so it is rejected by the shape guard rather than the
        // strict-parse error. The message names the offending shape + model.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(success_body("this is not json", Some(0.0))),
            )
            .mount(&server)
            .await;

        let client = client_for(&server, "google/gemini-3.1-flash-lite").await;
        let err = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect_err("non-json content is an error");
        let msg = err.to_string();
        assert!(msg.contains("non-object JSON after repair"), "got: {msg}");
        assert!(msg.contains("google/gemini-3.1-flash-lite"), "got: {msg}");
    }

    #[tokio::test]
    async fn complete_repairs_markdown_fenced_object() {
        // GAP-SG-10: a model that wraps a valid object in a ```json fence (a
        // common deepseek-v4-flash:nitro defect) is repaired and parsed instead
        // of being rejected as non-JSON.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_body(
                "```json\n{\"entities\":[\"rust\"],\"relationships\":[]}\n```",
                Some(0.0),
            )))
            .mount(&server)
            .await;

        let client = client_for(&server, "deepseek/deepseek-v4-flash").await;
        let (value, _, _) = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect("fenced object is repaired");
        assert_eq!(value, json!({"entities": ["rust"], "relationships": []}));
    }

    #[tokio::test]
    async fn complete_rejects_invalid_schema_before_network() {
        // No mock mounted: an unreachable URL proves we never hit the network.
        let client = OpenRouterChatClient::new_with_url(
            key(),
            "z-ai/glm-5.2".to_string(),
            "http://127.0.0.1:1/chat/completions".to_string(),
            30,
        )
        .expect("client builds");
        let err = client
            .complete("system", "input", "{not valid json", None)
            .await
            .expect_err("invalid schema is rejected");
        assert!(
            err.to_string().contains("invalid JSON schema"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn complete_retries_with_reasoning_omitted_when_mandatory() {
        let server = MockServer::start().await;
        // Primary attempt (reasoning.enabled=false) is rejected with a 400 whose
        // body mentions "reasoning" — the mandatory-reasoning signal that drives
        // the one-shot fallback.
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(400).set_body_string(
                    "reasoning is mandatory for this model and cannot be disabled",
                ),
            )
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        // Fallback attempt (reasoning field omitted) succeeds.
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_body(
                r#"{"entities":[],"relationships":[]}"#,
                Some(0.0),
            )))
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(&server, "minimax/minimax-m2.7").await;
        let (value, _, _) = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect("fallback completion succeeds");
        assert_eq!(value, json!({"entities": [], "relationships": []}));

        // Exactly two requests were sent: the FIRST carries reasoning.enabled=false,
        // the SECOND (fallback) OMITS the reasoning field entirely.
        let requests = server
            .received_requests()
            .await
            .expect("request recording is enabled");
        assert_eq!(requests.len(), 2, "expected primary + fallback requests");
        let first: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("first request body is JSON");
        let second: serde_json::Value =
            serde_json::from_slice(&requests[1].body).expect("second request body is JSON");
        assert_eq!(
            first["reasoning"]["enabled"],
            json!(false),
            "primary request must send reasoning.enabled=false"
        );
        assert!(
            second.get("reasoning").is_none(),
            "fallback request must omit the reasoning field, got: {second}"
        );
    }

    #[tokio::test]
    async fn complete_honours_configured_timeout() {
        // A 1s client timeout against a server that delays 2s proves the
        // --openrouter-timeout value is wired into the reqwest builder instead
        // of the fixed 300s default (regression: the flag was silently ignored).
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(std::time::Duration::from_secs(2))
                    .set_body_json(success_body(r#"{"ok":1}"#, Some(0.0))),
            )
            .mount(&server)
            .await;

        let client = OpenRouterChatClient::new_with_url(
            key(),
            "z-ai/glm-5.2".to_string(),
            format!("{}/chat/completions", server.uri()),
            1,
        )
        .expect("client builds");
        let err = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect_err("request exceeds the 1s timeout");
        assert!(err.to_string().contains("timed out"), "got: {err}");
    }

    #[tokio::test]
    async fn complete_surfaces_provider_error_in_200_body() {
        // GAP-SG-03: an HTTP 200 whose body is a structured OpenRouter error
        // (token/context-length overflow) must surface the REAL message, not
        // the misleading no-structured-content from empty choices.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "error": { "code": 400, "message": "context length exceeded" }
            })))
            .mount(&server)
            .await;

        let client = client_for(&server, "deepseek/deepseek-v4-flash").await;
        let err = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect_err("provider error must surface");
        let msg = err.to_string();
        assert!(msg.contains("context length exceeded"), "got: {msg}");
        assert!(
            !msg.contains("no structured content"),
            "must not mask as empty choices: {msg}"
        );
        assert!(
            !msg.contains("missing field"),
            "must not mask as a missing field: {msg}"
        );
    }
}
