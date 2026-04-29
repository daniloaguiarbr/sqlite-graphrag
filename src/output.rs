//! Single point of terminal I/O for the CLI (stdout JSON, stderr human).
//!
//! All user-visible output must go through this module; direct `println!` in
//! other modules is forbidden.

use crate::errors::AppError;
use serde::Serialize;

#[derive(Debug, Clone, Copy, clap::ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Json,
    Text,
    Markdown,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum, Default)]
pub enum JsonOutputFormat {
    #[default]
    Json,
}

pub fn emit_json<T: Serialize>(value: &T) -> Result<(), AppError> {
    let json = serde_json::to_string_pretty(value)?;
    println!("{json}");
    Ok(())
}

pub fn emit_json_compact<T: Serialize>(value: &T) -> Result<(), AppError> {
    let json = serde_json::to_string(value)?;
    println!("{json}");
    Ok(())
}

pub fn emit_text(msg: &str) {
    println!("{msg}");
}

pub fn emit_progress(msg: &str) {
    tracing::info!(message = msg);
}

/// Emits a bilingual progress message honouring `--lang` or `SQLITE_GRAPHRAG_LANG`.
/// Usage: `output::emit_progress_i18n("Computing embedding...", "Calculando embedding...")`.
pub fn emit_progress_i18n(en: &str, pt: &str) {
    use crate::i18n::{current, Language};
    match current() {
        Language::English => tracing::info!(message = en),
        Language::Portuguese => tracing::info!(message = pt),
    }
}

/// Emits a localised error message to stderr with the `Error:`/`Erro:` prefix.
///
/// Centralises human-readable error output following Pattern 5 (`output.rs` is the
/// SOLE I/O point of the CLI). Does not log via `tracing` — call `tracing::error!`
/// explicitly before this function when structured observability is desired.
pub fn emit_error(localized_msg: &str) {
    eprintln!("{}: {}", crate::i18n::error_prefix(), localized_msg);
}

/// Emits a bilingual error to stderr honouring `--lang` or `SQLITE_GRAPHRAG_LANG`.
/// Usage: `output::emit_error_i18n("invariant violated", "invariante violado")`.
pub fn emit_error_i18n(en: &str, pt: &str) {
    use crate::i18n::{current, Language};
    let msg = match current() {
        Language::English => en,
        Language::Portuguese => pt,
    };
    emit_error(msg);
}

/// JSON payload emitted by the `remember` subcommand.
///
/// All fields are required by the JSON contract (see `docs/schemas/remember.schema.json`).
/// `operation` is an alias of `action` for compatibility with clients using the old field name.
///
/// # Examples
///
/// ```
/// use sqlite_graphrag::output::RememberResponse;
///
/// let resp = RememberResponse {
///     memory_id: 1,
///     name: "nota-inicial".into(),
///     namespace: "global".into(),
///     action: "created".into(),
///     operation: "created".into(),
///     version: 1,
///     entities_persisted: 0,
///     relationships_persisted: 0,
///     relationships_truncated: false,
///     chunks_created: 1,
///     chunks_persisted: 0,
///     urls_persisted: 0,
///     extraction_method: None,
///     merged_into_memory_id: None,
///     warnings: vec![],
///     created_at: 1_700_000_000,
///     created_at_iso: "2023-11-14T22:13:20Z".into(),
///     elapsed_ms: 42,
/// };
///
/// let json = serde_json::to_string(&resp).unwrap();
/// assert!(json.contains("\"memory_id\":1"));
/// assert!(json.contains("\"elapsed_ms\":42"));
/// assert!(json.contains("\"merged_into_memory_id\":null"));
/// assert!(json.contains("\"urls_persisted\":0"));
/// assert!(json.contains("\"relationships_truncated\":false"));
/// ```
#[derive(Serialize)]
pub struct RememberResponse {
    pub memory_id: i64,
    pub name: String,
    pub namespace: String,
    pub action: String,
    /// Semantic alias of `action` for compatibility with the contract documented in SKILL.md and AGENT_PROTOCOL.md.
    pub operation: String,
    pub version: i64,
    pub entities_persisted: usize,
    pub relationships_persisted: usize,
    /// True when the relationship builder hit the cap before covering all entity pairs.
    /// Callers can use this to decide whether to increase GRAPHRAG_MAX_RELATIONSHIPS_PER_MEMORY.
    pub relationships_truncated: bool,
    /// Total chunks produced by the hierarchical splitter for this body.
    ///
    /// For single-chunk bodies this equals 1 even though no row is added to
    /// the `memory_chunks` table — the memory row itself acts as the chunk.
    /// Use `chunks_persisted` to know how many rows were actually written.
    pub chunks_created: usize,
    /// Number of rows actually inserted into the `memory_chunks` table.
    ///
    /// Equals zero for single-chunk bodies (the memory row is the chunk) and
    /// equals `chunks_created` for multi-chunk bodies. Added in v1.0.23 to
    /// disambiguate from `chunks_created` and reflect database state precisely.
    pub chunks_persisted: usize,
    /// Number of unique URLs inserted into `memory_urls` for this memory.
    /// Added in v1.0.24 — split URLs out of the entity graph (P0-2 fix).
    #[serde(default)]
    pub urls_persisted: usize,
    /// Extraction method used: "bert+regex" or "regex-only". None when skip-extraction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_method: Option<String>,
    pub merged_into_memory_id: Option<i64>,
    pub warnings: Vec<String>,
    /// Timestamp Unix epoch seconds.
    pub created_at: i64,
    /// RFC 3339 UTC timestamp string parallel to `created_at` for ISO 8601 parsers.
    pub created_at_iso: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    pub elapsed_ms: u64,
}

/// Individual item returned by the `recall` query.
///
/// The `memory_type` field is serialised as `"type"` in JSON to maintain
/// compatibility with external clients — the Rust name uses `memory_type`
/// to avoid conflict with the reserved keyword.
///
/// # Examples
///
/// ```
/// use sqlite_graphrag::output::RecallItem;
///
/// let item = RecallItem {
///     memory_id: 7,
///     name: "nota-rust".into(),
///     namespace: "global".into(),
///     memory_type: "user".into(),
///     description: "aprendizado de Rust".into(),
///     snippet: "ownership e borrowing".into(),
///     distance: 0.12,
///     source: "direct".into(),
///     graph_depth: None,
/// };
///
/// let json = serde_json::to_string(&item).unwrap();
/// // Rust field `memory_type` appears as `"type"` in JSON.
/// assert!(json.contains("\"type\":\"user\""));
/// assert!(!json.contains("memory_type"));
/// assert!(json.contains("\"distance\":0.12"));
/// ```
#[derive(Serialize, Clone)]
pub struct RecallItem {
    pub memory_id: i64,
    pub name: String,
    pub namespace: String,
    #[serde(rename = "type")]
    pub memory_type: String,
    pub description: String,
    pub snippet: String,
    pub distance: f32,
    pub source: String,
    /// Number of graph hops between this match and the seed memories.
    ///
    /// Set to `None` for direct vector matches (where `distance` is meaningful)
    /// and to `Some(N)` for traversal results, with `N=0` when the depth could
    /// not be tracked precisely. Added in v1.0.23 to disambiguate graph results
    /// from the `distance: 0.0` placeholder previously used for graph entries.
    /// Field is omitted from JSON output when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_depth: Option<u32>,
}

#[derive(Serialize)]
pub struct RecallResponse {
    pub query: String,
    pub k: usize,
    pub direct_matches: Vec<RecallItem>,
    pub graph_matches: Vec<RecallItem>,
    /// Aggregated alias of `direct_matches` + `graph_matches` for the contract documented in SKILL.md.
    pub results: Vec<RecallItem>,
    /// Total execution time in milliseconds from handler start to serialisation.
    pub elapsed_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Dummy {
        val: u32,
    }

    // Tipo não-serializável para forçar erro de serialização JSON
    struct NotSerializable;
    impl Serialize for NotSerializable {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom(
                "falha intencional de serialização",
            ))
        }
    }

    #[test]
    fn emit_json_retorna_ok_para_valor_valido() {
        let v = Dummy { val: 42 };
        assert!(emit_json(&v).is_ok());
    }

    #[test]
    fn emit_json_retorna_erro_para_valor_nao_serializavel() {
        let v = NotSerializable;
        assert!(emit_json(&v).is_err());
    }

    #[test]
    fn emit_json_compact_retorna_ok_para_valor_valido() {
        let v = Dummy { val: 7 };
        assert!(emit_json_compact(&v).is_ok());
    }

    #[test]
    fn emit_json_compact_retorna_erro_para_valor_nao_serializavel() {
        let v = NotSerializable;
        assert!(emit_json_compact(&v).is_err());
    }

    #[test]
    fn emit_text_nao_entra_em_panico() {
        emit_text("mensagem de teste");
    }

    #[test]
    fn emit_progress_nao_entra_em_panico() {
        emit_progress("progresso de teste");
    }

    #[test]
    fn remember_response_serializa_corretamente() {
        let r = RememberResponse {
            memory_id: 1,
            name: "teste".to_string(),
            namespace: "ns".to_string(),
            action: "created".to_string(),
            operation: "created".to_string(),
            version: 1,
            entities_persisted: 2,
            relationships_persisted: 3,
            relationships_truncated: false,
            chunks_created: 4,
            chunks_persisted: 4,
            urls_persisted: 2,
            extraction_method: None,
            merged_into_memory_id: None,
            warnings: vec!["aviso".to_string()],
            created_at: 1776569715,
            created_at_iso: "2026-04-19T03:34:15Z".to_string(),
            elapsed_ms: 123,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("memory_id"));
        assert!(json.contains("aviso"));
        assert!(json.contains("\"namespace\""));
        assert!(json.contains("\"merged_into_memory_id\""));
        assert!(json.contains("\"operation\""));
        assert!(json.contains("\"created_at\""));
        assert!(json.contains("\"created_at_iso\""));
        assert!(json.contains("\"elapsed_ms\""));
        assert!(json.contains("\"urls_persisted\""));
        assert!(json.contains("\"relationships_truncated\":false"));
    }

    #[test]
    fn recall_item_serializa_campo_type_renomeado() {
        let item = RecallItem {
            memory_id: 10,
            name: "entidade".to_string(),
            namespace: "ns".to_string(),
            memory_type: "entity".to_string(),
            description: "desc".to_string(),
            snippet: "trecho".to_string(),
            distance: 0.5,
            source: "db".to_string(),
            graph_depth: None,
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"type\""));
        assert!(!json.contains("memory_type"));
        // Field is omitted from JSON when None.
        assert!(!json.contains("graph_depth"));
    }

    #[test]
    fn recall_response_serializa_com_listas() {
        let resp = RecallResponse {
            query: "busca".to_string(),
            k: 10,
            direct_matches: vec![],
            graph_matches: vec![],
            results: vec![],
            elapsed_ms: 42,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("direct_matches"));
        assert!(json.contains("graph_matches"));
        assert!(json.contains("\"k\":"));
        assert!(json.contains("\"results\""));
        assert!(json.contains("\"elapsed_ms\""));
    }

    #[test]
    fn output_format_default_eh_json() {
        let fmt = OutputFormat::default();
        assert!(matches!(fmt, OutputFormat::Json));
    }

    #[test]
    fn output_format_variantes_existem() {
        let _text = OutputFormat::Text;
        let _md = OutputFormat::Markdown;
        let _json = OutputFormat::Json;
    }

    #[test]
    fn recall_item_clone_produces_equal_value() {
        let item = RecallItem {
            memory_id: 99,
            name: "clone".to_string(),
            namespace: "ns".to_string(),
            memory_type: "relation".to_string(),
            description: "d".to_string(),
            snippet: "s".to_string(),
            distance: 0.1,
            source: "src".to_string(),
            graph_depth: Some(2),
        };
        let cloned = item.clone();
        assert_eq!(cloned.memory_id, item.memory_id);
        assert_eq!(cloned.name, item.name);
        assert_eq!(cloned.graph_depth, Some(2));
    }
}
