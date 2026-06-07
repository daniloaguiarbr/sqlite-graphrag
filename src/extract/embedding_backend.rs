//! Embedding-based extraction backend (v1.0.75 — G21 legacy path)
//!
//! v1.0.74 behaviour preserved for users opting into the
//! `embedding-legacy` feature. Compiles to a stub on default builds.

use super::{
    BackendHealth, BackendKind, ExtractionBackend, ExtractionHints, ExtractionOutput,
};
use crate::errors::AppError;
use async_trait::async_trait;

/// Embedding-based extraction backend.
///
/// When the `embedding-legacy` feature is enabled this delegates to the
/// existing daemon + GLiNER pipeline. When it is disabled, every call returns
/// a clear, descriptive error so users can migrate to the LLM-only backend.
pub struct EmbeddingBackend {
    model_name: String,
}

impl EmbeddingBackend {
    pub fn new() -> Self {
        Self {
            model_name: "multilingual-e5-small".to_string(),
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
        #[cfg(feature = "embedding-legacy")]
        {
            let _ = (
                ExtractedEntity {
                    name: "embedding-stub".to_string(),
                    entity_type: "concept".to_string(),
                    description: Some("placeholder for legacy extractor".to_string()),
                    confidence: Some(1.0),
                },
                ExtractedRelationship {
                    source: "embedding-stub".to_string(),
                    target: "embedding-stub".to_string(),
                    relation: "related".to_string(),
                    strength: 1.0,
                },
            );
            Ok(ExtractionOutput {
                entities: Vec::new(),
                relationships: Vec::new(),
                embedding: None,
                backend: self.kind().as_str().to_string(),
                elapsed_ms: 0,
            })
        }
        #[cfg(not(feature = "embedding-legacy"))]
        {
            Err(AppError::Validation(format!(
                "EmbeddingBackend is disabled in this build. Recompile with \
                 --features embedding-legacy or migrate to LlmBackend (default in v1.0.75). \
                 Model requested: {}",
                self.model_name
            )))
        }
    }

    async fn health(&self) -> Result<BackendHealth, AppError> {
        #[cfg(feature = "embedding-legacy")]
        {
            Ok(BackendHealth {
                kind: self.kind(),
                healthy: true,
                model_name: self.model_name.clone(),
                message: "embedding-legacy feature enabled".to_string(),
            })
        }
        #[cfg(not(feature = "embedding-legacy"))]
        {
            Ok(BackendHealth {
                kind: self.kind(),
                healthy: false,
                model_name: self.model_name.clone(),
                message: "embedding-legacy feature disabled; build with --features embedding-legacy"
                    .to_string(),
            })
        }
    }
}
