//! Tests for the G21 ExtractionBackend trait implementations.

use sqlite_graphrag::extract::{
    backend_from_kind, default_backend, BackendKind, CompositeBackend, EmbeddingBackend,
    ExtractionBackend, ExtractionHints, LlmBackend, LlmExtractorConfig, NoneBackend,
};
use std::sync::Arc;

#[tokio::test]
async fn llm_backend_kind_and_model() {
    let backend = LlmBackend::with_default_codex();
    assert_eq!(backend.kind(), BackendKind::Llm);
    assert!(backend.model_name().contains("codex"));
}

#[tokio::test]
async fn llm_backend_health_is_ok() {
    let backend = LlmBackend::with_default_codex();
    let health = backend.health().await.expect("health");
    assert!(health.healthy);
    assert_eq!(health.kind, BackendKind::Llm);
}

#[tokio::test]
async fn llm_backend_extracts_basic_entities() {
    let backend = LlmBackend::with_default_codex();
    let hints = ExtractionHints::default();
    let output = backend
        .extract("rust tokio sqlite graphrag is a memory tool", &hints)
        .await
        .expect("extract");
    assert_eq!(output.backend, "llm");
    assert!(!output.entities.is_empty());
    assert!(output.entities.iter().any(|e| e.name == "rust"));
}

#[tokio::test]
async fn llm_backend_skips_short_input() {
    let backend = LlmBackend::with_default_codex();
    let hints = ExtractionHints::default();
    let output = backend.extract("", &hints).await.expect("extract");
    assert!(output.entities.is_empty());
    assert!(output.relationships.is_empty());
}

#[tokio::test]
async fn llm_backend_clamps_relations_on_skip() {
    let backend = LlmBackend::with_default_codex();
    let mut hints = ExtractionHints::default();
    hints.skip_relations = true;
    let output = backend
        .extract("rust tokio sqlite graphrag memory tool", &hints)
        .await
        .expect("extract");
    assert!(output.relationships.is_empty());
    assert!(!output.entities.is_empty());
}

#[tokio::test]
async fn embedding_backend_health_includes_model_name() {
    let backend = EmbeddingBackend::new();
    let health = backend.health().await.expect("health");
    assert_eq!(health.kind, BackendKind::Embedding);
    assert_eq!(health.model_name, "multilingual-e5-small");
}

#[tokio::test]
async fn none_backend_returns_empty() {
    let backend = NoneBackend::new();
    let hints = ExtractionHints::default();
    let output = backend.extract("anything", &hints).await.expect("extract");
    assert_eq!(output.backend, "none");
    assert!(output.entities.is_empty());
    assert!(output.relationships.is_empty());
    assert!(output.embedding.is_none());
}

#[tokio::test]
async fn composite_backend_merges_outputs() {
    let llm: Arc<dyn sqlite_graphrag::extract::ExtractionBackend> =
        Arc::new(LlmBackend::with_default_codex());
    let none: Arc<dyn sqlite_graphrag::extract::ExtractionBackend> =
        Arc::new(NoneBackend::new());
    let composite = CompositeBackend::new(vec![llm, none]);
    let hints = ExtractionHints::default();
    let output = composite
        .extract("rust tokio sqlite graphrag memory", &hints)
        .await
        .expect("extract");
    assert_eq!(output.backend, "composite");
    assert!(!output.entities.is_empty());
}

#[tokio::test]
async fn default_backend_factory_returns_llm() {
    let backend = default_backend();
    assert_eq!(backend.kind(), BackendKind::Llm);
}

#[tokio::test]
async fn backend_from_kind_dispatch() {
    for kind in [
        BackendKind::Llm,
        BackendKind::Embedding,
        BackendKind::None,
        BackendKind::Composite,
    ] {
        let backend = backend_from_kind(kind);
        assert_eq!(backend.kind(), kind);
    }
}

#[tokio::test]
async fn backend_kind_parse() {
    assert_eq!(BackendKind::parse("llm"), Some(BackendKind::Llm));
    assert_eq!(BackendKind::parse("LLM"), Some(BackendKind::Llm));
    assert_eq!(BackendKind::parse("embedding"), Some(BackendKind::Embedding));
    assert_eq!(BackendKind::parse("none"), Some(BackendKind::None));
    assert_eq!(BackendKind::parse("both"), Some(BackendKind::Composite));
    assert_eq!(BackendKind::parse("composite"), Some(BackendKind::Composite));
    assert_eq!(BackendKind::parse("bogus"), None);
}

#[tokio::test]
async fn llm_backend_with_claude_config() {
    let config = LlmExtractorConfig {
        backend: "claude".to_string(),
        model: Some("claude-sonnet-4-6".to_string()),
        timeout_secs: Some(120),
    };
    let backend = LlmBackend::new(config);
    assert!(backend.model_name().contains("claude"));
}
