//! Shared HTTP primitives for the OpenRouter chat and embeddings clients.
//!
//! [`crate::chat_api::OpenRouterChatClient`] and
//! [`crate::embedding_api::OpenRouterClient`] talk to the same OpenAI-compatible
//! REST surface and run the exact same retry/backoff policy. This module
//! centralizes the pieces that were duplicated verbatim between the two
//! (GAP-SG-74): the structured provider-error envelope, the
//! exponential-backoff-with-jitter sleep helper, and — per the reauditor
//! addendum — the retry-verdict classifiers both clients use to attach a
//! typed [`crate::retry::AttemptOutcome`] to every error AT THE ORIGIN (the
//! exact HTTP status or structured provider code), instead of letting a
//! downstream consumer (the enrich queue) infer it from a formatted message
//! substring, which `rules_rust_tratamento_de_erros.md` and
//! `rules_rust_retry_com_backoff.md` both forbid ("NUNCA usar string matching
//! em mensagens de erro").

use std::time::Duration;

use crate::retry::AttemptOutcome;

/// Maximum number of attempts `execute_with_retry` makes per request before
/// giving up. Shared by the chat and embeddings clients so both retry
/// policies stay in lockstep by construction rather than by convention.
pub(crate) const MAX_RETRIES: u32 = 4;

/// Structured OpenRouter error object carried under the `error` key of an
/// otherwise-2xx response body (e.g. a token/context-length overflow). `code`
/// is a `serde_json::Value` because the provider sends it as either a JSON
/// number or string; `message` defaults to empty so a malformed error object
/// never masks the underlying cause.
#[derive(serde::Deserialize)]
pub(crate) struct ApiError {
    #[serde(default)]
    pub(crate) code: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) message: String,
}

impl ApiError {
    /// Renders `code` as a plain string without JSON quoting, falling back to
    /// `unknown` when the provider omitted it.
    pub(crate) fn code_string(&self) -> String {
        match &self.code {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
            None => "unknown".to_string(),
        }
    }
}

/// Exponential backoff with up to 500ms of jitter, shared by both clients'
/// `execute_with_retry` loops. `attempt` is the zero-based retry index.
pub(crate) async fn backoff(attempt: u32) {
    let base_ms = 1000u64 * 2u64.pow(attempt);
    let jitter = fastrand::u64(0..500);
    let sleep_ms = base_ms + jitter;
    tracing::debug!(attempt, sleep_ms, "exponential backoff");
    tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
}

/// Classifies an HTTP status from the OpenRouter chat/embeddings endpoints
/// into a retry verdict, computed AT THE ORIGIN where the exact status is
/// known — never by matching a formatted error message downstream.
///
/// `400`/`401`/`403`/`404` are permanent client errors (bad request, bad
/// key, forbidden, bad model); `408`/`425`/`429`/`5xx` are transient. Any
/// other status observed here (redirects, unexpected `2xx`/`3xx`/other
/// `4xx`) defaults to [`AttemptOutcome::HardFailure`]: an unrecognised shape
/// from this endpoint is closer to a permanent protocol violation than a
/// transient hiccup, and the caller can widen this match from real
/// dead-letter evidence.
pub(crate) fn status_retry_class(status: reqwest::StatusCode) -> AttemptOutcome {
    match status.as_u16() {
        400 | 401 | 403 | 404 => AttemptOutcome::HardFailure,
        408 | 425 | 429 => AttemptOutcome::Transient,
        _ if status.is_server_error() => AttemptOutcome::Transient,
        _ => AttemptOutcome::HardFailure,
    }
}

/// Classifies a structured OpenRouter provider error (the `error` object
/// carried inside an otherwise-2xx body) by its `code`, never by its
/// `message` — mapping an external code to an internal variant is the
/// pattern `rules_rust_retry_com_backoff.md` explicitly allows ("MAPEAR
/// códigos de erro externos para variantes internas").
///
/// A numeric code in `429` or `500..=599` is transient (rate limit / server
/// overload surfaced inside a 200 body); known transient string codes are
/// also mapped. Everything else (e.g. `context_length_exceeded`,
/// `invalid_request_error`, a refusal) is permanent.
pub(crate) fn provider_error_retry_class(api_err: &ApiError) -> AttemptOutcome {
    let code = api_err.code_string();
    if let Ok(numeric) = code.parse::<u16>() {
        return if numeric == 429 || (500..=599).contains(&numeric) {
            AttemptOutcome::Transient
        } else {
            AttemptOutcome::HardFailure
        };
    }
    match code.as_str() {
        "rate_limit_exceeded" | "rate_limited" | "server_error" | "service_unavailable" => {
            AttemptOutcome::Transient
        }
        _ => AttemptOutcome::HardFailure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_retry_class_maps_client_errors_to_hard_failure() {
        assert_eq!(
            status_retry_class(reqwest::StatusCode::UNAUTHORIZED),
            AttemptOutcome::HardFailure
        );
        assert_eq!(
            status_retry_class(reqwest::StatusCode::BAD_REQUEST),
            AttemptOutcome::HardFailure
        );
        assert_eq!(
            status_retry_class(reqwest::StatusCode::NOT_FOUND),
            AttemptOutcome::HardFailure
        );
    }

    #[test]
    fn status_retry_class_maps_rate_limit_and_server_errors_to_transient() {
        assert_eq!(
            status_retry_class(reqwest::StatusCode::TOO_MANY_REQUESTS),
            AttemptOutcome::Transient
        );
        assert_eq!(
            status_retry_class(reqwest::StatusCode::SERVICE_UNAVAILABLE),
            AttemptOutcome::Transient
        );
        assert_eq!(
            status_retry_class(reqwest::StatusCode::BAD_GATEWAY),
            AttemptOutcome::Transient
        );
    }

    #[test]
    fn status_retry_class_treats_403_as_hard_failure() {
        assert_eq!(
            status_retry_class(reqwest::StatusCode::FORBIDDEN),
            AttemptOutcome::HardFailure
        );
    }

    #[test]
    fn status_retry_class_treats_408_and_425_as_transient() {
        assert_eq!(
            status_retry_class(reqwest::StatusCode::REQUEST_TIMEOUT),
            AttemptOutcome::Transient
        );
        assert_eq!(
            status_retry_class(reqwest::StatusCode::from_u16(425).expect("425 is a valid status")),
            AttemptOutcome::Transient
        );
    }

    #[test]
    fn status_retry_class_defaults_unrecognised_status_to_hard_failure() {
        assert_eq!(
            status_retry_class(reqwest::StatusCode::IM_A_TEAPOT),
            AttemptOutcome::HardFailure
        );
    }

    fn api_error(code: serde_json::Value) -> ApiError {
        serde_json::from_value(serde_json::json!({ "code": code, "message": "x" }))
            .expect("valid ApiError fixture")
    }

    #[test]
    fn provider_error_retry_class_treats_numeric_429_and_5xx_as_transient() {
        assert_eq!(
            provider_error_retry_class(&api_error(serde_json::json!(429))),
            AttemptOutcome::Transient
        );
        assert_eq!(
            provider_error_retry_class(&api_error(serde_json::json!(503))),
            AttemptOutcome::Transient
        );
    }

    #[test]
    fn provider_error_retry_class_treats_numeric_400_as_hard_failure() {
        assert_eq!(
            provider_error_retry_class(&api_error(serde_json::json!(400))),
            AttemptOutcome::HardFailure
        );
    }

    #[test]
    fn provider_error_retry_class_treats_known_transient_codes_as_transient() {
        assert_eq!(
            provider_error_retry_class(&api_error(serde_json::json!("rate_limited"))),
            AttemptOutcome::Transient
        );
        assert_eq!(
            provider_error_retry_class(&api_error(serde_json::json!("server_error"))),
            AttemptOutcome::Transient
        );
    }

    #[test]
    fn provider_error_retry_class_treats_context_length_exceeded_as_hard_failure() {
        assert_eq!(
            provider_error_retry_class(&api_error(serde_json::json!("context_length_exceeded"))),
            AttemptOutcome::HardFailure
        );
    }
}
