//! No-op extraction backend (v1.0.75 — G21 auxiliary)
//!
//! Returns empty output for pipelines that want to skip extraction entirely.

use super::{
    BackendHealth, BackendKind, ExtractionBackend, ExtractionHints, ExtractionOutput,
};
use crate::errors::AppError;
use async_trait::async_trait;

pub struct NoneBackend;

impl NoneBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoneBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExtractionBackend for NoneBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::None
    }

    fn model_name(&self) -> String {
        "none".to_string()
    }

    async fn extract(
        &self,
        _content: &str,
        _hints: &ExtractionHints,
    ) -> Result<ExtractionOutput, AppError> {
        Ok(ExtractionOutput {
            backend: self.kind().as_str().to_string(),
            elapsed_ms: 0,
            ..Default::default()
        })
    }

    async fn health(&self) -> Result<BackendHealth, AppError> {
        Ok(BackendHealth {
            kind: self.kind(),
            healthy: true,
            model_name: "none".to_string(),
            message: "no-op backend always healthy".to_string(),
        })
    }
}
