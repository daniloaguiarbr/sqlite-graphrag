//! Embedding-based extraction backend (v1.0.75 — G21 legacy path)
//!
//! The legacy fastembed pipeline behind the `embedding-legacy` feature was
//! REMOVED in v1.0.79 (originally scheduled for v1.1.0). This backend is a
//! permanent stub kept only so `--extraction-backend embedding` keeps
//! parsing and returns a clear migration error instead of an opaque one.

use super::{BackendHealth, BackendKind, ExtractionBackend, ExtractionHints, ExtractionOutput};
use crate::errors::AppError;
use async_trait::async_trait;

/// Embedding-based extraction backend (permanent stub since v1.0.79).
pub struct EmbeddingBackend {
    model_name: String,
}

impl EmbeddingBackend {
    pub fn new() -> Self {
        Self {
            model_name: crate::constants::SQLITE_GRAPHRAG_VERSION.to_string(),
        }
    }

    pub fn with_model(model_name: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
        }
    }
}

impl Default for EmbeddingBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExtractionBackend for EmbeddingBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Embedding
    }

    fn model_name(&self) -> String {
        self.model_name.clone()
    }

    async fn extract(
        &self,
        _content: &str,
        _hints: &ExtractionHints,
    ) -> Result<ExtractionOutput, AppError> {
        Err(AppError::Validation(format!(
            "the legacy embedding extraction backend was removed in v1.0.79 \
             (the CLI is LLM-only); use --extraction-backend llm instead. \
             Model requested: {}",
            self.model_name
        )))
    }

    async fn health(&self) -> Result<BackendHealth, AppError> {
        Ok(BackendHealth {
            kind: self.kind(),
            healthy: false,
            model_name: self.model_name.clone(),
            message: "legacy embedding backend removed in v1.0.79; use the llm backend".to_string(),
        })
    }
}
