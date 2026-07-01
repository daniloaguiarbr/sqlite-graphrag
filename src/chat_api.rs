//! HTTP client for the OpenRouter chat-completions API.
//!
//! Sends structured-output chat requests to the OpenAI-compatible endpoint
//! at `openrouter.ai/api/v1/chat/completions` and returns the parsed JSON
//! object the model produced under a strict `json_schema` `response_format`.
//!
//! This mirrors [`crate::embedding_api`] for the embeddings endpoint: same
//! retry/backoff policy (immediate abort on 401/400/404, `retry-after` on
//! 429, exponential backoff + jitter on 5xx) and the same minimal headers
//! (only `Authorization: Bearer`, no `HTTP-Referer`/`X-Title`). The shared
//! error envelope and backoff helper live in [`crate::openrouter_http`]
//! (GAP-SG-74).
//!
//! v1.0.95 (ADR-0054): adds an OpenRouter REST transport for the `enrich`
//! JUDGE so structured extraction no longer requires a locally installed
//! `claude` / `codex` / `opencode` CLI subprocess.
//!
//! v1.1.00 (GAP-SG-70/72-chat): the OpenAI-compatible contract surfaces
//! `choices[].finish_reason` and `usage.{prompt_tokens,completion_tokens}`.
//! `finish_reason == "length"` means the response was truncated because
//! `max_tokens` was too small — not a malformed generation. [`Self::complete`]
//! now detects this BEFORE attempting JSON repair, grows `max_tokens` and
//! re-issues the request (bounded by
//! [`crate::constants::ENRICH_MAX_LENGTH_RETRIES`]), and always reports the
//! diagnostics (`finish_reason`, token counts) to the caller via
//! [`ChatCompletion`] on success or [`ChatError`] on failure.

use crate::errors::AppError;
use crate::retry::AttemptOutcome;
use secrecy::{ExposeSecret, SecretBox};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const OPENROUTER_CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
// GAP-SG-17: raised from 300 to 600 — the per-request fallback budget when a
// caller passes `0`. Dense bodies near the model's ~32K-token context ceiling
// regularly need more than five minutes to generate.
const DEFAULT_TIMEOUT_SECS: u64 = 600;
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

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
    error: Option<crate::openrouter_http::ApiError>,
}

#[derive(Deserialize)]
struct Choice {
    message: RespMessage,
    /// Why the model stopped generating: `"stop"` on a normal completion,
    /// `"length"` when `max_tokens` cut the response short (GAP-SG-70/72-chat).
    /// Absent from providers that omit it, hence `#[serde(default)]`.
    #[serde(default)]
    finish_reason: Option<String>,
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
    /// Prompt token count reported by OpenRouter (GAP-SG-72-chat). Diagnostic
    /// only — never used to gate control flow, so a missing value stays `None`.
    #[serde(default)]
    prompt_tokens: Option<u32>,
    /// Completion token count reported by OpenRouter (GAP-SG-72-chat), used
    /// alongside `finish_reason` to explain a truncated response.
    #[serde(default)]
    completion_tokens: Option<u32>,
}

/// Successful [`OpenRouterChatClient::complete`] result (GAP-SG-72-chat).
///
/// `finish_reason`, `prompt_tokens` and `completion_tokens` are the raw
/// diagnostics OpenRouter attached to the response that ultimately succeeded
/// (after any `max_tokens` growth retries — see [`Self::value`] and the
/// module docs). They are `None` only when the provider omitted them.
#[derive(Debug)]
pub struct ChatCompletion {
    /// Model output parsed as JSON (guaranteed to be a JSON object).
    pub value: serde_json::Value,
    /// Cost in USD read from `usage.cost`, or `0.0` when the provider omitted it.
    pub cost_usd: f64,
    /// `choices[0].finish_reason` from the response that produced `value`.
    pub finish_reason: Option<String>,
    /// `usage.prompt_tokens` from the response that produced `value`.
    pub prompt_tokens: Option<u32>,
    /// `usage.completion_tokens` from the response that produced `value`.
    pub completion_tokens: Option<u32>,
}

/// [`OpenRouterChatClient::complete`] failure (GAP-SG-72-chat / GAP-SG-72
/// reauditor addendum).
///
/// Wraps the underlying [`AppError`] with whatever truncation diagnostics were
/// available at the point of failure. `finish_reason`/token fields are `None`
/// when the failure happened before a response was parsed (network error, a
/// permanent 4xx, or exhausted retries) — only failures that occur AFTER a
/// `ChatResponse` was successfully decoded (JSON-repair or shape-guard
/// failures) carry them.
///
/// `retry_class` is the retry verdict computed AT THE ORIGIN (the exact HTTP
/// status, or the provider's structured error `code`), never inferred
/// downstream from `source.to_string()`. The enrich queue consumes this field
/// directly instead of pattern-matching the formatted message.
#[derive(Debug)]
pub struct ChatError {
    /// Underlying cause, preserved via `source()` rather than restated.
    pub source: AppError,
    /// `choices[0].finish_reason` from the response that led to this error,
    /// when one was decoded.
    pub finish_reason: Option<String>,
    /// `usage.prompt_tokens` from the response that led to this error, when
    /// one was decoded.
    pub prompt_tokens: Option<u32>,
    /// `usage.completion_tokens` from the response that led to this error,
    /// when one was decoded.
    pub completion_tokens: Option<u32>,
    /// Typed retry verdict computed where the failure originated (HTTP
    /// status / provider code), not by matching `source`'s message.
    pub retry_class: AttemptOutcome,
}

impl ChatError {
    /// Wraps `source` with no diagnostics attached (used when no
    /// `ChatResponse` was decoded before the failure) and the `retry_class`
    /// computed by the caller at the exact HTTP status / provider code.
    fn new(source: AppError, retry_class: AttemptOutcome) -> Self {
        Self {
            source,
            finish_reason: None,
            prompt_tokens: None,
            completion_tokens: None,
            retry_class,
        }
    }

    /// Wraps `source` with the diagnostics captured from a decoded
    /// `ChatResponse` that nonetheless failed downstream (repair or
    /// shape-guard), plus its `retry_class`.
    fn with_diagnostics(
        source: AppError,
        finish_reason: Option<String>,
        prompt_tokens: Option<u32>,
        completion_tokens: Option<u32>,
        retry_class: AttemptOutcome,
    ) -> Self {
        Self {
            source,
            finish_reason,
            prompt_tokens,
            completion_tokens,
            retry_class,
        }
    }
}

impl std::fmt::Display for ChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.source, f)
    }
}

impl std::error::Error for ChatError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
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
            .user_agent("sqlite-graphrag/1.1.00")
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

    /// Runs a single structured-output completion, transparently growing
    /// `max_tokens` and re-issuing the request when the model truncates its
    /// output (GAP-SG-70).
    ///
    /// `schema_str` is the JSON Schema (as a string) the model must honour
    /// under `strict: true`. When `input_text` is empty only the system
    /// message is sent. `max_tokens` seeds the first attempt; `None` lets the
    /// provider apply its own default.
    ///
    /// Returns [`ChatCompletion`] on success or [`ChatError`] on failure; both
    /// carry `finish_reason`/token diagnostics when a response was decoded.
    ///
    /// # Errors
    ///
    /// Returns [`ChatError`] when: the schema is invalid JSON; the HTTP
    /// request fails or exhausts retries; the provider returns a permanent
    /// error (401/400/404, or a structured `error` object in a 2xx body); the
    /// response carries no usable content; the content cannot be parsed as
    /// JSON even after repair; the parsed JSON is not an object; or the
    /// response is truncated (`finish_reason: "length"`) after
    /// [`crate::constants::ENRICH_MAX_LENGTH_RETRIES`] `max_tokens` growth
    /// attempts are exhausted.
    pub async fn complete(
        &self,
        system_prompt: &str,
        input_text: &str,
        schema_str: &str,
        max_tokens: Option<u32>,
    ) -> Result<ChatCompletion, ChatError> {
        // A malformed schema is a permanent caller/config error — classified
        // explicitly (no blanket `From<AppError>` conversion exists for this
        // type; every `ChatError` states its `retry_class` at construction).
        let schema: serde_json::Value = serde_json::from_str(schema_str).map_err(|e| {
            ChatError::new(
                AppError::Validation(format!("invalid JSON schema for OpenRouter request: {e}")),
                AttemptOutcome::HardFailure,
            )
        })?;

        let mut current_max_tokens = max_tokens;

        for length_attempt in 0..=crate::constants::ENRICH_MAX_LENGTH_RETRIES {
            let response = self
                .complete_one_attempt(&schema, system_prompt, input_text, current_max_tokens)
                .await?;

            let finish_reason = response
                .choices
                .first()
                .and_then(|c| c.finish_reason.clone());
            let prompt_tokens = response.usage.as_ref().and_then(|u| u.prompt_tokens);
            let completion_tokens = response.usage.as_ref().and_then(|u| u.completion_tokens);

            let truncated = finish_reason.as_deref() == Some("length");
            let retries_left = length_attempt < crate::constants::ENRICH_MAX_LENGTH_RETRIES;

            if truncated && retries_left {
                let next_max_tokens = grow_max_tokens(current_max_tokens);
                tracing::warn!(
                    model = %self.model,
                    attempt = length_attempt,
                    previous_max_tokens = ?current_max_tokens,
                    next_max_tokens,
                    "OpenRouter completion truncated (finish_reason=length); \
                     retrying with a larger max_tokens budget"
                );
                current_max_tokens = Some(next_max_tokens);
                continue;
            }

            if truncated {
                tracing::warn!(
                    model = %self.model,
                    max_length_retries = crate::constants::ENRICH_MAX_LENGTH_RETRIES,
                    max_tokens = ?current_max_tokens,
                    "OpenRouter completion still truncated after exhausting \
                     max_tokens growth"
                );
            }

            return self.finish_completion(
                response,
                finish_reason,
                prompt_tokens,
                completion_tokens,
            );
        }

        unreachable!("loop always returns within ENRICH_MAX_LENGTH_RETRIES + 1 iterations")
    }

    /// Runs one HTTP attempt (including the mandatory-reasoning fallback) and
    /// returns the decoded [`ChatResponse`] without inspecting `finish_reason`
    /// or extracting content — that happens in [`Self::complete`] so the
    /// `max_tokens` growth loop can re-issue the request first.
    async fn complete_one_attempt(
        &self,
        schema: &serde_json::Value,
        system_prompt: &str,
        input_text: &str,
        max_tokens: Option<u32>,
    ) -> Result<ChatResponse, ChatError> {
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
        match self.execute_with_retry(&primary).await {
            Ok(r) => Ok(r),
            Err(first_err) => {
                if reasoning_disable_rejected(&first_err) {
                    tracing::warn!(
                        model = %self.model,
                        "model rejected reasoning.enabled=false (mandatory); \
                         retrying once with reasoning omitted"
                    );
                    let fallback = self.build_request(
                        schema.clone(),
                        system_prompt,
                        input_text,
                        max_tokens,
                        None,
                    );
                    match self.execute_with_retry(&fallback).await {
                        Ok(r) => Ok(r),
                        Err(_) => Err(first_err),
                    }
                } else {
                    Err(first_err)
                }
            }
        }
    }

    /// Extracts content, repairs/parses it as JSON, and enforces the
    /// object-shape guard, attaching `finish_reason`/token diagnostics to any
    /// failure.
    ///
    /// Every failure branch below (missing content, JSON-repair failure,
    /// non-object shape) classifies as `AttemptOutcome::Transient`. This is a
    /// deliberate, acknowledged tension with `rules_rust_retry_com_backoff.md`
    /// ("NUNCA retentar erros de parsing ou deserialização" / "NUNCA retentar
    /// erros de deserialização"): those rules target DETERMINISTIC parse
    /// errors, where retrying the identical input reproduces the identical
    /// failure. Here the "input" is `deepseek-v4-flash:nitro` sampling
    /// variance — the SAME prompt can legitimately produce well-formed JSON
    /// on the next generation (see GAP-SG-10). So this is a typed, bounded
    /// hiccup, not a retry-forever loophole: it is capped by `--max-attempts`
    /// (GAP-SG-09/GAP-SG-21) and dead-letters once attempts are exhausted.
    fn finish_completion(
        &self,
        response: ChatResponse,
        finish_reason: Option<String>,
        prompt_tokens: Option<u32>,
        completion_tokens: Option<u32>,
    ) -> Result<ChatCompletion, ChatError> {
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
            })
            .map_err(|e| {
                ChatError::with_diagnostics(
                    e,
                    finish_reason.clone(),
                    prompt_tokens,
                    completion_tokens,
                    AttemptOutcome::Transient,
                )
            })?;

        // GAP-SG-10: deepseek-v4-flash:nitro and similar models do not honour
        // `json_schema` strict mode reliably — they wrap output in markdown
        // fences, add trailing commas, or omit quotes around keys. Try a strict
        // parse first (zero cost for well-formed JSON), then fall back to the
        // repair pass (a Rust port of `json_repair`) before giving up.
        let value = crate::json_repair::repair_to_value(&content).map_err(|e| {
            ChatError::with_diagnostics(
                AppError::Validation(format!(
                    "model '{}' returned content that could not be parsed even after \
                     JSON repair: {e}",
                    self.model
                )),
                finish_reason.clone(),
                prompt_tokens,
                completion_tokens,
                AttemptOutcome::Transient,
            )
        })?;

        // GAP-SG-10: `llm_json` coerces aggressively — free text becomes a JSON
        // string, empty input becomes `{}`, a lone delimiter becomes `null`. The
        // enrich JUDGE contract is ALWAYS a JSON object, so a non-object result
        // here is a malformed/refused generation, NOT a usable value. Reject it
        // (the enrich classifier reclassifies this as a transient model hiccup,
        // GAP-SG-09) instead of letting a coerced scalar masquerade as a
        // valid-but-empty result downstream.
        if !value.is_object() {
            return Err(ChatError::with_diagnostics(
                AppError::Validation(format!(
                    "model '{}' returned non-object JSON after repair (got {}); \
                     likely a refusal or malformed structured output",
                    self.model,
                    json_shape_name(&value)
                )),
                finish_reason,
                prompt_tokens,
                completion_tokens,
                AttemptOutcome::Transient,
            ));
        }

        let cost = response.usage.and_then(|u| u.cost).unwrap_or(0.0);

        Ok(ChatCompletion {
            value,
            cost_usd: cost,
            finish_reason,
            prompt_tokens,
            completion_tokens,
        })
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

    /// Runs the request/retry loop, classifying every failure into a
    /// [`ChatError`] with `retry_class` set AT THE ORIGIN (the exact HTTP
    /// status, or the provider's structured error code) — never inferred
    /// downstream from a formatted message (reauditor addendum to
    /// GAP-SG-72-chat).
    async fn execute_with_retry(
        &self,
        request: &ChatRequest<'_>,
    ) -> Result<ChatResponse, ChatError> {
        let mut last_err: Option<ChatError> = None;

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
                    return Err(ChatError::new(
                        AppError::Validation("OpenRouter chat request timed out".into()),
                        AttemptOutcome::Transient,
                    ));
                }
                Err(e) => {
                    last_err = Some(ChatError::new(
                        AppError::Validation(format!("HTTP request failed: {e}")),
                        AttemptOutcome::Transient,
                    ));
                    crate::openrouter_http::backoff(attempt).await;
                    continue;
                }
            };

            let status = resp.status();

            if status.is_success() {
                let body = resp.text().await.map_err(|e| {
                    ChatError::new(
                        AppError::Validation(format!("failed to read response body: {e}")),
                        AttemptOutcome::Transient,
                    )
                })?;
                match serde_json::from_str::<ChatResponse>(&body) {
                    Ok(parsed) => {
                        // A structured error object inside a 2xx body is
                        // classified by its own `code` (GAP-SG-03 surfaces
                        // the real code/message instead of letting empty
                        // choices masquerade as no-structured-content).
                        if let Some(api_err) = parsed.error {
                            let retry_class =
                                crate::openrouter_http::provider_error_retry_class(&api_err);
                            return Err(ChatError::new(
                                AppError::ProviderError {
                                    code: api_err.code_string(),
                                    message: api_err.message,
                                },
                                retry_class,
                            ));
                        }
                        return Ok(parsed);
                    }
                    Err(e) => {
                        tracing::warn!(
                            attempt,
                            body_len = body.len(),
                            "HTTP 200 but parse failed (retrying): {e}"
                        );
                        last_err = Some(ChatError::new(
                            AppError::Validation(format!("failed to parse chat response: {e}")),
                            AttemptOutcome::Transient,
                        ));
                        crate::openrouter_http::backoff(attempt).await;
                        continue;
                    }
                }
            }

            if status.as_u16() == 401 {
                return Err(ChatError::new(
                    AppError::Validation("invalid OpenRouter API key (HTTP 401)".into()),
                    AttemptOutcome::HardFailure,
                ));
            }

            if status.as_u16() == 400 || status.as_u16() == 404 {
                let body = resp.text().await.unwrap_or_default();
                return Err(ChatError::new(
                    AppError::Validation(format!(
                        "OpenRouter returned {status} for model '{}': {body}",
                        self.model
                    )),
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
                last_err = Some(ChatError::new(
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
                last_err = Some(ChatError::new(
                    AppError::Validation(format!("OpenRouter server error: {status}")),
                    AttemptOutcome::Transient,
                ));
                crate::openrouter_http::backoff(attempt).await;
                continue;
            }

            let body = resp.text().await.unwrap_or_default();
            return Err(ChatError::new(
                AppError::Validation(format!("unexpected HTTP {status}: {body}")),
                crate::openrouter_http::status_retry_class(status),
            ));
        }

        // GAP-SG-72-chat addendum: exhausting every retry against a
        // transient condition (429/5xx/timeout/network) is ITSELF transient
        // — it is exactly the case the queue's `--max-attempts` backoff
        // covers, and must never be reclassified as a permanent failure.
        Err(last_err.unwrap_or_else(|| {
            ChatError::new(
                AppError::Validation("max retries exceeded for OpenRouter chat request".into()),
                AttemptOutcome::Transient,
            )
        }))
    }
}

/// Grows `current` for the next `max_tokens` retry after a truncated
/// (`finish_reason: "length"`) response (GAP-SG-70/71). When `current` is
/// `None` the caller left the provider default in place, so growth starts
/// from [`crate::constants::ENRICH_INITIAL_MAX_TOKENS`] instead of an unknown
/// base. The result is always capped at
/// [`crate::constants::ENRICH_MAX_TOKENS_CEILING`].
fn grow_max_tokens(current: Option<u32>) -> u32 {
    let base = current.unwrap_or(crate::constants::ENRICH_INITIAL_MAX_TOKENS);
    base.saturating_mul(crate::constants::ENRICH_MAX_TOKENS_GROWTH_FACTOR)
        .min(crate::constants::ENRICH_MAX_TOKENS_CEILING)
}

/// True when an error from `execute_with_retry` indicates the model rejected
/// `reasoning.enabled=false` because reasoning is mandatory: an HTTP 400 whose
/// body mentions "reasoning" (case-insensitive). Triggers the one-shot retry
/// with the `reasoning` field omitted.
///
/// This IS a legitimate, narrowly-scoped substring check on the underlying
/// `AppError`'s message — not a retry-classification decision (that lives in
/// `ChatError.retry_class`, computed at the origin). It only decides whether
/// to attempt the mandatory-reasoning fallback shape, an orthogonal concern.
fn reasoning_disable_rejected(err: &ChatError) -> bool {
    let msg = err.source.to_string().to_lowercase();
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
    /// under structured outputs), optionally attaching `usage.cost` and a
    /// `finish_reason` (defaults to `"stop"`).
    fn success_body(content: &str, cost: Option<f64>) -> serde_json::Value {
        success_body_with_finish(content, cost, "stop")
    }

    /// Same as [`success_body`] but with an explicit `finish_reason`, used by
    /// the GAP-SG-70 truncation tests.
    fn success_body_with_finish(
        content: &str,
        cost: Option<f64>,
        finish_reason: &str,
    ) -> serde_json::Value {
        let mut body = json!({
            "choices": [{ "message": { "content": content }, "finish_reason": finish_reason }]
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

    #[test]
    fn grow_max_tokens_uses_initial_default_when_current_is_none() {
        assert_eq!(
            grow_max_tokens(None),
            crate::constants::ENRICH_INITIAL_MAX_TOKENS
                * crate::constants::ENRICH_MAX_TOKENS_GROWTH_FACTOR
        );
    }

    #[test]
    fn grow_max_tokens_caps_at_ceiling() {
        assert_eq!(
            grow_max_tokens(Some(crate::constants::ENRICH_MAX_TOKENS_CEILING)),
            crate::constants::ENRICH_MAX_TOKENS_CEILING
        );
        assert_eq!(
            grow_max_tokens(Some(u32::MAX)),
            crate::constants::ENRICH_MAX_TOKENS_CEILING
        );
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
        let completion = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect("completion succeeds");

        assert_eq!(
            completion.value,
            json!({"entities": [], "relationships": []})
        );
        assert!((completion.cost_usd - 0.0023).abs() < f64::EPSILON);
        assert_eq!(completion.finish_reason.as_deref(), Some("stop"));
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
        let completion = client
            .complete("system", "", TEST_SCHEMA, Some(4096))
            .await
            .expect("completion succeeds");
        assert_eq!(completion.cost_usd, 0.0);
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
        let completion = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect("retried completion succeeds");
        assert_eq!(completion.value, json!({"ok": true}));
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
        let completion = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect("retried completion succeeds");
        assert_eq!(completion.value, json!({"ok": 1}));
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
        assert_eq!(err.retry_class, AttemptOutcome::HardFailure);
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
        assert_eq!(err.retry_class, AttemptOutcome::HardFailure);
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
        assert_eq!(err.finish_reason, None);
        assert_eq!(
            err.retry_class,
            AttemptOutcome::Transient,
            "no-content is a model hiccup, not a permanent rejection"
        );
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
        let completion = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect("fenced object is repaired");
        assert_eq!(
            completion.value,
            json!({"entities": ["rust"], "relationships": []})
        );
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
        assert_eq!(
            err.retry_class,
            AttemptOutcome::HardFailure,
            "a malformed schema is a permanent caller error"
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
        let completion = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect("fallback completion succeeds");
        assert_eq!(
            completion.value,
            json!({"entities": [], "relationships": []})
        );

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
        assert_eq!(
            err.retry_class,
            AttemptOutcome::HardFailure,
            "code 400 (context length exceeded) is a permanent provider rejection"
        );
    }

    #[tokio::test]
    async fn complete_classifies_provider_error_429_code_as_transient() {
        // Reauditor addendum: the provider error's structured `code` — never
        // its message — decides the retry verdict. A 429 code inside an
        // otherwise-2xx body is exactly as transient as an HTTP-level 429.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "error": { "code": 429, "message": "rate limited" }
            })))
            .mount(&server)
            .await;

        let client = client_for(&server, "deepseek/deepseek-v4-flash").await;
        let err = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect_err("provider rate-limit error must surface");
        assert_eq!(err.retry_class, AttemptOutcome::Transient);
    }

    #[tokio::test]
    async fn complete_classifies_exhausted_5xx_retries_as_transient() {
        // GAP-SG-72-chat addendum: exhausting every retry against a
        // persistent 5xx is a TRANSIENT outcome (the queue's --max-attempts
        // is what eventually dead-letters it), never a HardFailure.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let client = client_for(&server, "openai/gpt-oss-120b").await;
        let err = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect_err("persistent 5xx exhausts retries");
        assert_eq!(err.retry_class, AttemptOutcome::Transient);
    }

    #[tokio::test]
    async fn complete_regrows_max_tokens_and_retries_on_length_truncation() {
        // GAP-SG-70: the first response is truncated (finish_reason="length")
        // with content that is NOT valid JSON on its own (proving the retry
        // happens BEFORE json_repair, not as a repair fallback). The second
        // response (after max_tokens grows) is well-formed and finishes
        // normally.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(success_body_with_finish(
                    r#"{"entities":["trunc"#,
                    Some(0.001),
                    "length",
                )),
            )
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(success_body_with_finish(
                    r#"{"entities":["rust"],"relationships":[]}"#,
                    Some(0.002),
                    "stop",
                )),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(&server, "deepseek/deepseek-v4-flash:nitro").await;
        let completion = client
            .complete("system", "input", TEST_SCHEMA, Some(64))
            .await
            .expect("second attempt with grown max_tokens succeeds");

        assert_eq!(
            completion.value,
            json!({"entities": ["rust"], "relationships": []})
        );
        assert_eq!(completion.finish_reason.as_deref(), Some("stop"));

        let requests = server
            .received_requests()
            .await
            .expect("request recording is enabled");
        assert_eq!(requests.len(), 2, "expected exactly one regrowth retry");
        let first: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("first request body is JSON");
        let second: serde_json::Value =
            serde_json::from_slice(&requests[1].body).expect("second request body is JSON");
        assert_eq!(first["max_tokens"], json!(64));
        assert_eq!(
            second["max_tokens"],
            json!(64 * crate::constants::ENRICH_MAX_TOKENS_GROWTH_FACTOR),
            "max_tokens must grow by ENRICH_MAX_TOKENS_GROWTH_FACTOR before the retry"
        );
    }

    #[tokio::test]
    async fn complete_captures_finish_reason_and_tokens_on_success() {
        // GAP-SG-72-chat: a normal (non-truncated) completion still reports
        // finish_reason and both token counts to the caller.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{
                    "message": { "content": r#"{"ok":true}"# },
                    "finish_reason": "stop"
                }],
                "usage": { "cost": 0.001, "prompt_tokens": 120, "completion_tokens": 30 }
            })))
            .mount(&server)
            .await;

        let client = client_for(&server, "z-ai/glm-5.2").await;
        let completion = client
            .complete("system", "input", TEST_SCHEMA, None)
            .await
            .expect("completion succeeds");

        assert_eq!(completion.finish_reason.as_deref(), Some("stop"));
        assert_eq!(completion.prompt_tokens, Some(120));
        assert_eq!(completion.completion_tokens, Some(30));
    }

    #[tokio::test]
    async fn complete_gives_up_after_exhausting_length_retries() {
        // GAP-SG-70: every attempt (primary + ENRICH_MAX_LENGTH_RETRIES
        // regrowth retries) reports finish_reason="length" with unparsable
        // content, so complete() must give up and return a ChatError instead
        // of retrying forever.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(success_body_with_finish(
                    r#"[1, 2, 3"#,
                    Some(0.0),
                    "length",
                )),
            )
            .mount(&server)
            .await;

        let client = client_for(&server, "deepseek/deepseek-v4-flash:nitro").await;
        let err = client
            .complete("system", "input", TEST_SCHEMA, Some(64))
            .await
            .expect_err("exhausted length retries must fail");
        assert_eq!(err.finish_reason.as_deref(), Some("length"));
        assert_eq!(
            err.retry_class,
            AttemptOutcome::Transient,
            "a repeatedly truncated response is a bounded-retry hiccup, not permanent"
        );

        let requests = server
            .received_requests()
            .await
            .expect("request recording is enabled");
        assert_eq!(
            requests.len() as u32,
            crate::constants::ENRICH_MAX_LENGTH_RETRIES + 1,
            "expected the primary attempt plus every regrowth retry"
        );
    }
}
