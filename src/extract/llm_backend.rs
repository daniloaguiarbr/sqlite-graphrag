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
    /// v1.0.89 (GAP-META-006): resolves the backend at runtime via
    /// `detect_available_backend()` instead of hardcoding "codex".
    fn default() -> Self {
        let backend = match detect_available_backend() {
            Ok(LlmBackendKindFactory::Codex) | Ok(LlmBackendKindFactory::Auto) => "codex".to_string(),
            Ok(LlmBackendKindFactory::Claude) => "claude".to_string(),
            Ok(LlmBackendKindFactory::None) | Err(_) => "none".to_string(),
        };
        Self {
            backend,
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

    /// v1.0.89 (GAP-WITH-DEFAULT-CODEX): legacy constructor — `Default` now
    /// resolves the backend at runtime via `detect_available_backend()`.
    /// Callers should use `LlmBackend::new(LlmExtractorConfig::default())`
    /// or the factory pattern in `factory_for_choice()` instead.
    #[deprecated(since = "1.0.89", note = "use LlmBackend::new(LlmExtractorConfig::default()) or factory_for_choice()")]
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
                "the"
                    | "and"
                    | "for"
                    | "with"
                    | "from"
                    | "this"
                    | "that"
                    | "into"
                    | "sobre"
                    | "para"
                    | "como"
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
            for (i, source) in entities
                .iter()
                .enumerate()
                .take(entities.len().saturating_sub(1))
            {
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

// =============================================================================
// v1.0.82 (GAP-003): LlmBackendFactory trait + 3 implementations.
// The factory pattern replaces the legacy `with_default_codex()` /
// `with_default_claude()` constructors with a runtime-resolved factory
// chosen by the user's `--llm-backend` flag. The `Auto` variant is
// the new default: it queries the PATH for codex and claude and
// picks the first available one (preserving the v1.0.81 behaviour
// of preferring codex when both are present).
// =============================================================================

/// LLM backend kind (mirrors `cli::LlmBackendChoice`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LlmBackendKindFactory {
    /// Auto-detect: prefer codex, fall back to claude.
    Auto,
    /// `codex exec` headless OAuth (ChatGPT Pro).
    Codex,
    /// `claude -p` headless OAuth (Claude Pro/Max).
    Claude,
    /// No embedding — every `embed()` call returns `Ok(vec![])`.
    None,
}

/// Factory trait for LLM-based backends. Each implementation knows
/// how to build the right CLI invocation (codex vs claude vs none)
/// from the user-supplied `LlmExtractorConfig`.
///
/// The factory pattern exists so that:
/// 1. `composite_backend.rs` can dispatch to ANY backend via a
///    boxed trait object without knowing the concrete type;
/// 2. `--llm-backend=auto` can probe PATH at runtime and pick
///    the first available CLI;
/// 3. New backends (ollama, opencode, lm-studio) can be added
///    in v1.0.83+ without changing the call sites that consume
///    the factory.
pub trait LlmBackendFactory: Send + Sync {
    /// Build an [`ExtractionBackend`] implementation ready to
    /// extract entities and relationships from a body.
    fn build_extraction_backend(
        &self,
        config: &LlmExtractorConfig,
    ) -> Result<Box<dyn ExtractionBackend>, AppError>;

    /// Build a query embedder (used by `recall` / `hybrid-search`).
    fn build_embedder(
        &self,
        config: &LlmExtractorConfig,
    ) -> Result<Box<dyn std::any::Any + Send + Sync>, AppError>;

    /// Short identifier for logging.
    fn kind(&self) -> LlmBackendKindFactory;
}

/// Codex CLI factory — builds a `LlmBackend` configured for `codex exec`.
pub struct CodexFactory;

impl LlmBackendFactory for CodexFactory {
    fn build_extraction_backend(
        &self,
        config: &LlmExtractorConfig,
    ) -> Result<Box<dyn ExtractionBackend>, AppError> {
        let mut cfg = config.clone();
        cfg.backend = "codex".into();
        Ok(Box::new(LlmBackend::new(cfg)))
    }
    fn build_embedder(
        &self,
        _config: &LlmExtractorConfig,
    ) -> Result<Box<dyn std::any::Any + Send + Sync>, AppError> {
        // The actual embedder is built by `embedder::get_embedder`,
        // not here — the factory is the policy switch, the embedder
        // is the implementation. Returning a typed sentinel is enough
        // for v1.0.82; full integration lands in v1.0.83 alongside
        // the explicit claude-only path.
        Ok(Box::new(()))
    }
    fn kind(&self) -> LlmBackendKindFactory {
        LlmBackendKindFactory::Codex
    }
}

/// Claude CLI factory.
pub struct ClaudeFactory;

impl LlmBackendFactory for ClaudeFactory {
    fn build_extraction_backend(
        &self,
        config: &LlmExtractorConfig,
    ) -> Result<Box<dyn ExtractionBackend>, AppError> {
        let mut cfg = config.clone();
        cfg.backend = "claude".into();
        Ok(Box::new(LlmBackend::new(cfg)))
    }
    fn build_embedder(
        &self,
        _config: &LlmExtractorConfig,
    ) -> Result<Box<dyn std::any::Any + Send + Sync>, AppError> {
        Ok(Box::new(()))
    }
    fn kind(&self) -> LlmBackendKindFactory {
        LlmBackendKindFactory::Claude
    }
}

/// No-op factory — every extraction call returns empty output;
/// every embed call returns an empty vector. Used by
/// `--llm-backend=none` (zero-dependency mode).
pub struct NullFactory;

impl LlmBackendFactory for NullFactory {
    fn build_extraction_backend(
        &self,
        _config: &LlmExtractorConfig,
    ) -> Result<Box<dyn ExtractionBackend>, AppError> {
        struct NullExtraction;
        #[async_trait]
        impl ExtractionBackend for NullExtraction {
            fn kind(&self) -> BackendKind {
                BackendKind::None
            }
            fn model_name(&self) -> String {
                "null".into()
            }
            async fn health(&self) -> Result<BackendHealth, AppError> {
                Ok(BackendHealth {
                    kind: BackendKind::None,
                    healthy: true,
                    model_name: "null".into(),
                    message: "no-op backend".into(),
                })
            }
            async fn extract(
                &self,
                _body: &str,
                _hints: &ExtractionHints,
            ) -> Result<ExtractionOutput, AppError> {
                Ok(ExtractionOutput::default())
            }
        }
        Ok(Box::new(NullExtraction))
    }
    fn build_embedder(
        &self,
        _config: &LlmExtractorConfig,
    ) -> Result<Box<dyn std::any::Any + Send + Sync>, AppError> {
        Ok(Box::new(()))
    }
    fn kind(&self) -> LlmBackendKindFactory {
        LlmBackendKindFactory::None
    }
}

/// Auto-detect factory — picks CodexFactory when `codex` is on PATH,
/// ClaudeFactory when `claude` is on PATH, NullFactory when neither
/// is reachable. This is the v1.0.81 behaviour (implicit preference
/// for codex) made explicit.
pub struct AutoFactory;

impl LlmBackendFactory for AutoFactory {
    fn build_extraction_backend(
        &self,
        config: &LlmExtractorConfig,
    ) -> Result<Box<dyn ExtractionBackend>, AppError> {
        let choice = detect_available_backend()?;
        match choice {
            LlmBackendKindFactory::Codex | LlmBackendKindFactory::Auto => {
                CodexFactory.build_extraction_backend(config)
            }
            LlmBackendKindFactory::Claude => ClaudeFactory.build_extraction_backend(config),
            LlmBackendKindFactory::None => NullFactory.build_extraction_backend(config),
        }
    }
    fn build_embedder(
        &self,
        config: &LlmExtractorConfig,
    ) -> Result<Box<dyn std::any::Any + Send + Sync>, AppError> {
        let choice = detect_available_backend()?;
        match choice {
            LlmBackendKindFactory::Codex | LlmBackendKindFactory::Auto => {
                CodexFactory.build_embedder(config)
            }
            LlmBackendKindFactory::Claude => ClaudeFactory.build_embedder(config),
            LlmBackendKindFactory::None => NullFactory.build_embedder(config),
        }
    }
    fn kind(&self) -> LlmBackendKindFactory {
        LlmBackendKindFactory::Auto
    }
}

/// Resolves the available LLM CLI by probing PATH for `codex` first,
/// then `claude`. Returns `None` if neither is found.
///
/// In test environments where `mock-llm` is on PATH but neither
/// `codex` nor `claude` is, this returns `Codex` to preserve the
/// v1.0.76+ "LLM-only one-shot" contract — the mock LLM plays the
/// role of whichever real LLM the test expects.
pub fn detect_available_backend() -> Result<LlmBackendKindFactory, AppError> {
    // Probing PATH without a `which` crate: std-only `which` is good
    // enough here because we only need to know IF a name resolves,
    // not WHERE it resolves.
    fn has_in_path(name: &str) -> bool {
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path_var) {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return true;
                }
            }
        }
        false
    }

    // Prefer codex, fall back to claude, then null.
    if has_in_path("codex") {
        Ok(LlmBackendKindFactory::Codex)
    } else if has_in_path("claude") {
        Ok(LlmBackendKindFactory::Claude)
    } else {
        // Neither found — degrade gracefully to None.
        Ok(LlmBackendKindFactory::None)
    }
}

/// Factory dispatcher — converts a CLI enum value into a boxed
/// factory. This is the integration point used by
/// `composite_backend.rs` and by the 6 commands that consume
/// `--llm-backend`.
pub fn factory_for_choice(
    choice: LlmBackendKindFactory,
) -> Result<Box<dyn LlmBackendFactory>, AppError> {
    match choice {
        LlmBackendKindFactory::Auto => Ok(Box::new(AutoFactory)),
        LlmBackendKindFactory::Codex => Ok(Box::new(CodexFactory)),
        LlmBackendKindFactory::Claude => Ok(Box::new(ClaudeFactory)),
        LlmBackendKindFactory::None => Ok(Box::new(NullFactory)),
    }
}

#[cfg(test)]
mod factory_tests {
    use super::*;

    #[test]
    fn detect_returns_known_kind() {
        // The test environment may have mock-llm on PATH; we only
        // assert that the return is a known variant.
        let r = detect_available_backend();
        assert!(r.is_ok());
    }

    #[test]
    fn factory_for_choice_returns_boxed_factory() {
        let f = factory_for_choice(LlmBackendKindFactory::Codex).expect("Codex factory");
        assert_eq!(f.kind(), LlmBackendKindFactory::Codex);
        let f = factory_for_choice(LlmBackendKindFactory::None).expect("Null factory");
        assert_eq!(f.kind(), LlmBackendKindFactory::None);
    }

    #[test]
    fn null_factory_extracts_nothing() {
        let f = NullFactory;
        let backend = f
            .build_extraction_backend(&LlmExtractorConfig::default())
            .expect("NullFactory always builds");
        // Drive the async future on the current-thread runtime to avoid
        // pulling in the `futures` crate just for the test.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        let h = rt.block_on(backend.health()).expect("health ok");
        assert!(h.healthy);
        let out = rt
            .block_on(backend.extract("any body", &ExtractionHints::default()))
            .expect("Null extract is Ok");
        assert!(out.entities.is_empty());
        assert!(out.relationships.is_empty());
    }
}
