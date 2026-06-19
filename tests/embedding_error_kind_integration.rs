//! GAP-004 (v1.0.88) integration tests for the `EmbeddingErrorKind`
//! typed classifier.
//!
//! These tests exercise the public API of `embedder::classify_embedding_error`
//! to confirm that the new typed-discriminator dispatch produces the same
//! `FallbackReason` shape as the legacy `msg.contains(...)` chain.
//!
//! The 5 unit tests for the bare `EmbeddingErrorKind::classify` are in
//! `src/embedder.rs` (the `#[cfg(test)] mod tests` block). The integration
//! tests here verify the public wiring through `classify_embedding_error`.

use sqlite_graphrag::embedder::classify_embedding_error;
use sqlite_graphrag::errors::AppError;

/// Helper: build an `AppError::Embedding(msg)` cheaply.
fn embed_err(msg: &str) -> AppError {
    AppError::Embedding(msg.to_string())
}

/// GAP-004: a message containing "OAuth" maps to OAuthQuota (with
/// "claude" backend hint, since the message names claude explicitly).
#[test]
fn classify_oauth_message_routes_to_oauth_quota() {
    let r = classify_embedding_error(embed_err("OAuth token expired for claude"));
    match r {
        sqlite_graphrag::embedder::FallbackReason::OAuthQuota { backend } => {
            assert_eq!(backend, "claude");
        }
        other => panic!("expected OAuthQuota, got {other:?}"),
    }
}

/// GAP-004: a message containing only "quota" (no "OAuth") still maps
/// to OAuthQuota, preserving v1.0.85 behavior.
#[test]
fn classify_quota_message_routes_to_oauth_quota() {
    let r = classify_embedding_error(embed_err("usage quota exhausted on backend"));
    match r {
        sqlite_graphrag::embedder::FallbackReason::OAuthQuota { backend } => {
            // No codex/claude substring, so backend falls back to "unknown".
            assert_eq!(backend, "unknown");
        }
        other => panic!("expected OAuthQuota, got {other:?}"),
    }
}

/// GAP-004: a slot-sema error message maps to SlotExhausted. The
/// EmbeddingErrorKind classifier must catch this BEFORE the Quota arm
/// so the more specific "LLM never tried" path wins.
#[test]
fn classify_slot_exhausted_message_routes_to_slot_exhausted() {
    let r = classify_embedding_error(embed_err(
        "slot exhausted: failed to acquire LLM slot after backoff",
    ));
    assert_eq!(r, sqlite_graphrag::embedder::FallbackReason::SlotExhausted,);
}

/// GAP-004: a message containing both "dim" and "zero" maps to DimZero.
#[test]
fn classify_zero_dimension_message_routes_to_dim_zero() {
    let r = classify_embedding_error(embed_err("embedding returned dim=zero"));
    assert_eq!(r, sqlite_graphrag::embedder::FallbackReason::DimZero);
}

/// GAP-004: a message that matches NO marker falls back to
/// EmbeddingFailed (or Cancelled for the explicit "cancelled" case).
/// This guards the unknown-fallback arm of `classify_embedding_error`.
#[test]
fn classify_unknown_message_routes_to_embedding_failed() {
    let r = classify_embedding_error(embed_err("unrelated subprocess error"));
    match r {
        sqlite_graphrag::embedder::FallbackReason::EmbeddingFailed(msg) => {
            assert_eq!(msg, "unrelated subprocess error");
        }
        other => panic!("expected EmbeddingFailed, got {other:?}"),
    }
}
