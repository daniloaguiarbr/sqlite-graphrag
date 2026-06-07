//! Extraction backend abstraction (v1.0.75 — G21 solution)
//!
//! Provides `ExtractionBackend` trait with concrete implementations for
//! LLM-only (default in v1.0.75), Embedding (legacy), None (no extraction),
//! and Composite (orchestrates multiple backends in parallel).
//!
//! The trait enables backend-agnostic ingest/enrich/remember pipelines.

use crate::errors::AppError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Hint configuration forwarded to the extraction backend.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionHints {
    /// Memory name to be remembered (kebab-case)
    pub memory_name: Option<String>,
    /// Memory type to be remembered
    pub memory_type: Option<String>,
    /// Existing entity names to avoid duplicates
    pub existing_entities: Vec<String>,
    /// Whether to skip relation extraction
    pub skip_relations: bool,
    /// Backend-specific seed for determinism
    pub seed: Option<u64>,
}

/// Entity extracted from content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: String,
    pub description: Option<String>,
    pub confidence: Option<f32>,
}

/// Relationship extracted from content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedRelationship {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub strength: f32,
}

/// Output of extraction backend.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionOutput {
    pub entities: Vec<ExtractedEntity>,
    pub relationships: Vec<ExtractedRelationship>,
    /// Optional embedding vector (only populated by EmbeddingBackend)
    pub embedding: Option<Vec<f32>>,
    /// Backend that produced this output
    pub backend: String,
    /// Latency in milliseconds
    pub elapsed_ms: u64,
}

/// Backend kind enumeration used for selection and telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendKind {
    Llm,
    Embedding,
    None,
    Composite,
}

impl BackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BackendKind::Llm => "llm",
            BackendKind::Embedding => "embedding",
            BackendKind::None => "none",
            BackendKind::Composite => "composite",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "llm" => Some(BackendKind::Llm),
            "embedding" => Some(BackendKind::Embedding),
            "none" => Some(BackendKind::None),
            "both" | "composite" => Some(BackendKind::Composite),
            _ => None,
        }
    }
}

/// Trait abstraction for any extraction backend (LLM, Embedding, None, Composite).
///
/// G21 HIGH solution: the trait allows the rest of the codebase to remain
/// agnostic of the underlying extraction mechanism. New backends can be added
/// without touching call sites.
#[async_trait]
pub trait ExtractionBackend: Send + Sync {
    /// Identify this backend (used in metrics, logs and ExtractionOutput)
    fn kind(&self) -> BackendKind;

    /// Identify the underlying model/CLI being used (e.g. "codex-0.137.0")
    fn model_name(&self) -> String;

    /// Extract entities and relationships from `content`.
    ///
    /// `hints` provides optional context (memory name, type, etc.).
    /// Returns `ExtractionOutput` with entities, relationships, and optional embedding.
    async fn extract(
        &self,
        content: &str,
        hints: &ExtractionHints,
    ) -> Result<ExtractionOutput, AppError>;

    /// Health check: whether this backend is ready to operate.
    async fn health(&self) -> Result<BackendHealth, AppError>;
}

/// Health status of a backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendHealth {
    pub kind: BackendKind,
    pub healthy: bool,
    pub model_name: String,
    pub message: String,
}

/// Type alias for shared backend references.
pub type SharedBackend = Arc<dyn ExtractionBackend>;

pub mod composite_backend;
pub mod embedding_backend;
pub mod llm_backend;
pub mod llm_embedding;
pub mod none_backend;

pub use composite_backend::{backend_from_kind, default_backend, CompositeBackend};
pub use embedding_backend::EmbeddingBackend;
pub use llm_backend::{LlmBackend, LlmExtractorConfig};
pub use llm_embedding::{EmbeddingFlavour, LlmEmbedding, EMBEDDING_DIM as LLM_EMBEDDING_DIM};
pub use none_backend::NoneBackend;
