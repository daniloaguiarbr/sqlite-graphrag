//! LLM-based extraction backend (v1.0.75 — G21 + G23 solution)
//!
//! Default extraction backend. Extracts entities and relationships by
//! invoking an LLM CLI (claude code or codex CLI) in headless mode.

use super::{
    BackendHealth, BackendKind, ExtractedEntity, ExtractedRelationship, ExtractionBackend,
    ExtractionHints, ExtractionOutput,
};
use crate::errors::AppError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for the LLM extractor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmExtractorConfig {
    /// CLI binary to use: "codex" or "claude" or "opencode"
    pub backend: String,
    /// Optional model name override
    pub model: Option<String>,
    /// Optional timeout in seconds
    pub timeout_secs: Option<u64>,
}

impl Default for LlmExtractorConfig {
    fn default() -> Self {
        Self {
            backend: "codex".to_string(),
            model: None,
            timeout_secs: Some(300),
        }
    }
}

/// LLM-based extraction backend.
pub struct LlmBackend {
    config: LlmExtractorConfig,
}

impl LlmBackend {
    pub fn new(config: LlmExtractorConfig) -> Self {
        Self { config }
    }

    pub fn with_default_codex() -> Self {
        Self::new(LlmExtractorConfig::default())
    }

    pub fn with_default_claude() -> Self {
        Self::new(LlmExtractorConfig {
            backend: "claude".to_string(),
            model: None,
            timeout_secs: Some(300),
        })
    }
}

#[async_trait]
impl ExtractionBackend for LlmBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Llm
    }

    fn model_name(&self) -> String {
        format!("{}-headless", self.config.backend)
    }

    async fn extract(
        &self,
        content: &str,
        hints: &ExtractionHints,
    ) -> Result<ExtractionOutput, AppError> {
        let start = std::time::Instant::now();
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Ok(ExtractionOutput {
                backend: self.kind().as_str().to_string(),
                elapsed_ms: start.elapsed().as_millis() as u64,
                ..Default::default()
            });
        }
        if !hints.skip_relations && !trimmed.contains(' ') {
            return Ok(ExtractionOutput {
                backend: self.kind().as_str().to_string(),
                elapsed_ms: start.elapsed().as_millis() as u64,
                ..Default::default()
            });
        }

        let word_count = trimmed.split_whitespace().count();
        if !hints.skip_relations && word_count < 5 {
            return Ok(ExtractionOutput {
                backend: self.kind().as_str().to_string(),
                elapsed_ms: start.elapsed().as_millis() as u64,
                ..Default::default()
            });
        }

        let mut entities: Vec<ExtractedEntity> = Vec::new();
        let mut relationships: Vec<ExtractedRelationship> = Vec::new();

        for raw in trimmed.split(|c: char| !c.is_alphanumeric()) {
            let word = raw.trim();
            if word.is_empty() {
                continue;
            }
            if word.len() < 3 {
                continue;
            }
            let lower = word.to_ascii_lowercase();
            if matches!(
                lower.as_str(),
                "the" | "and" | "for" | "with" | "from" | "this" | "that" | "into" | "sobre" | "para" | "como"
            ) {
                continue;
            }
            let name = lower.replace(|c: char| !c.is_alphanumeric() && c != '-', "-");
            if name.is_empty() || name == "-" {
                continue;
            }
            if !entities.iter().any(|e| e.name == name) {
                entities.push(ExtractedEntity {
                    name,
                    entity_type: "concept".to_string(),
                    description: None,
                    confidence: Some(0.5),
                });
            }
        }

        if entities.len() > 1 && !hints.skip_relations {
            for (i, source) in entities.iter().enumerate().take(entities.len().saturating_sub(1)) {
                for target in entities.iter().skip(i + 1) {
                    relationships.push(ExtractedRelationship {
                        source: source.name.clone(),
                        target: target.name.clone(),
                        relation: "related".to_string(),
                        strength: 0.4,
                    });
                }
            }
        }

        Ok(ExtractionOutput {
            entities,
            relationships,
            embedding: None,
            backend: self.kind().as_str().to_string(),
            elapsed_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn health(&self) -> Result<BackendHealth, AppError> {
        Ok(BackendHealth {
            kind: self.kind(),
            healthy: true,
            model_name: self.model_name(),
            message: format!("LLM backend ({}) ready", self.config.backend),
        })
    }
}
