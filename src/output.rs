//! Single point of terminal I/O for the CLI (stdout JSON, stderr human).
//!
//! All user-visible output must go through this module; direct `println!` in
//! other modules is forbidden.

use crate::errors::AppError;
use serde::Serialize;

/// Output format variants accepted by `--format` CLI flags.
#[derive(Debug, Clone, Copy, clap::ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Json,
    Text,
    Markdown,
}

/// Restricted JSON-only format for commands that always emit JSON.
#[derive(Debug, Clone, Copy, clap::ValueEnum, Default)]
pub enum JsonOutputFormat {
    #[default]
    Json,
}

/// Serializes `value` as pretty-printed JSON and writes it to stdout with a trailing newline.
///
/// Flushes stdout after writing. A `BrokenPipe` error is silenced so that
/// piping to consumers that close early (e.g. `head`) does not surface an error.
///
/// # Errors
/// Returns `Err` when serialization fails or when a non-`BrokenPipe` I/O error occurs.
#[inline]
pub fn emit_json<T: Serialize>(value: &T) -> Result<(), AppError> {
    let json = serde_json::to_string_pretty(value)?;
    let mut out = std::io::stdout().lock();
    if let Err(e) = std::io::Write::write_all(&mut out, json.as_bytes())
        .and_then(|()| std::io::Write::write_all(&mut out, b"\n"))
        .and_then(|()| std::io::Write::flush(&mut out))
    {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            return Ok(());
        }
        return Err(AppError::Io(e));
    }
    Ok(())
}

/// Serializes `value` as compact (single-line) JSON and writes it to stdout with a trailing newline.
///
/// Flushes stdout after writing. A `BrokenPipe` error is silenced.
///
/// # Errors
/// Returns `Err` when serialization fails or when a non-`BrokenPipe` I/O error occurs.
#[inline]
pub fn emit_json_compact<T: Serialize>(value: &T) -> Result<(), AppError> {
    let json = serde_json::to_string(value)?;
    let mut out = std::io::stdout().lock();
    if let Err(e) = std::io::Write::write_all(&mut out, json.as_bytes())
        .and_then(|()| std::io::Write::write_all(&mut out, b"\n"))
        .and_then(|()| std::io::Write::flush(&mut out))
    {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            return Ok(());
        }
        return Err(AppError::Io(e));
    }
    Ok(())
}

/// Writes compact JSON to stdout, silently ignoring serialization and I/O errors.
/// Designed for NDJSON streaming where partial output is acceptable.
#[inline]
pub fn emit_json_line<T: Serialize>(value: &T) {
    if let Ok(json) = serde_json::to_string(value) {
        let mut out = std::io::stdout().lock();
        let _ = std::io::Write::write_all(&mut out, json.as_bytes());
        let _ = std::io::Write::write_all(&mut out, b"\n");
        let _ = std::io::Write::flush(&mut out);
    }
}

/// Writes `msg` followed by a newline to stdout and flushes.
///
/// A `BrokenPipe` error is silenced gracefully.
#[inline]
pub fn emit_text(msg: &str) {
    let mut out = std::io::stdout().lock();
    let _ = std::io::Write::write_all(&mut out, msg.as_bytes())
        .and_then(|()| std::io::Write::write_all(&mut out, b"\n"))
        .and_then(|()| std::io::Write::flush(&mut out));
}

/// Logs `msg` as a structured `tracing::info!` event (does not write to stdout).
/// v1.0.89: suppressed when stderr is not a terminal (pipe) to avoid
/// polluting JSON pipelines when the user redirects stderr with `2>&1`.
#[inline]
pub fn emit_progress(msg: &str) {
    if std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        tracing::info!(target: "output", message = msg);
    }
}

/// Emits a bilingual progress message honouring `--lang` or `SQLITE_GRAPHRAG_LANG`.
/// v1.0.89: suppressed when stderr is not a terminal (pipe).
pub fn emit_progress_i18n(en: &str, pt: &str) {
    if !std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        return;
    }
    use crate::i18n::{current, Language};
    match current() {
        Language::English => tracing::info!(target: "output", message = en),
        Language::Portuguese => tracing::info!(target: "output", message = pt),
    }
}

/// Emits a JSON error envelope to stdout for machine consumers.
///
/// Ensures the stdout JSON contract is honoured even on error paths:
/// `{"error": true, "code": <exit_code>, "message": "<localized_msg>"}`.
/// A `BrokenPipe` error is silenced so piping to early-closing consumers
/// does not surface a secondary error.
#[cold]
#[inline(never)]
pub fn emit_error_json(code: i32, message: &str) {
    #[derive(serde::Serialize)]
    struct ErrorEnvelope<'a> {
        error: bool,
        code: i32,
        message: &'a str,
    }
    let envelope = ErrorEnvelope {
        error: true,
        code,
        message,
    };
    if emit_json(&envelope).is_err() {
        use std::io::Write;
        let escaped = message.replace('\\', "\\\\").replace('"', "\\\"");
        let _ = writeln!(
            std::io::stdout().lock(),
            r#"{{"error":true,"code":{code},"message":"{escaped}"}}"#
        );
    }
}

/// Emits a localised error message to stderr via the `tracing` subscriber.
///
/// ADR-0047 / BUG-12 v1.0.88: prior implementation also called `eprintln!`
/// which produced a SECOND stderr line (Error:/Erro: prefix) for the same
/// error, on top of the structured `tracing::error!` line. Operators and
/// log parsers observed duplicated stderr lines.
///
/// The tracing subscriber is configured for stderr at `main.rs:115`, so a
/// single `tracing::error!` call already produces the human-readable line.
/// Callers that want a plain stderr line without tracing (e.g. one-shot
/// scripts) should use `eprintln!` directly instead of this helper.
///
/// Centralises human-readable error output following Pattern 5 (`output.rs` is
/// the SOLE I/O point of the CLI).
#[cold]
#[inline(never)]
pub fn emit_error(localized_msg: &str) {
    tracing::error!(target: "output", message = localized_msg);
}

/// Emits a bilingual error to stderr honouring `--lang` or `SQLITE_GRAPHRAG_LANG`.
/// Usage: `output::emit_error_i18n("invariant violated", "invariante violado")`.
#[cold]
#[inline(never)]
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
///     name_was_normalized: false,
///     original_name: None,
///     backend_invoked: None,
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
    /// Semantic alias of `action` for compatibility with the contract documented in SKILL.md.
    pub operation: String,
    pub version: i64,
    pub entities_persisted: usize,
    pub relationships_persisted: usize,
    /// True when the relationship builder hit the cap before covering all entity pairs.
    /// Callers can use this to decide whether to increase GRAPHRAG_MAX_RELATIONSHIPS_PER_MEMORY.
    pub relationships_truncated: bool,
    /// Total number of chunks the body was split into BEFORE dedup.
    ///
    /// For single-chunk bodies this equals 1 even though no row is added to
    /// the `memory_chunks` table — the memory row itself acts as the chunk.
    /// Use `chunks_persisted` to know how many rows were actually written.
    pub chunks_created: usize,
    /// Number of chunks actually written to chunks/embeddings tables. Always <= chunks_created.
    ///
    /// Equal when no chunk had identical normalized text already in DB; less when dedup skipped
    /// some. Equals zero for single-chunk bodies (the memory row is the chunk) and equals
    /// `chunks_created` for multi-chunk bodies. Added in v1.0.23 to disambiguate from
    /// `chunks_created` and reflect database state precisely.
    pub chunks_persisted: usize,
    /// Number of unique URLs inserted into `memory_urls` for this memory.
    /// Added in v1.0.24 — split URLs out of the entity graph (P0-2 fix).
    #[serde(default)]
    pub urls_persisted: usize,
    /// Extraction method used: "gliner-{variant}+regex" or "regex-only". None when NER is not enabled.
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
    /// True when the user-supplied `--name` differed from the persisted slug
    /// (i.e. kebab-case normalization changed the value). Added in v1.0.32 so
    /// callers can detect normalization without parsing stderr WARN logs.
    #[serde(default)]
    pub name_was_normalized: bool,
    /// Original user-supplied `--name` value before normalization.
    /// Present only when `name_was_normalized == true`; omitted otherwise to
    /// keep the common (already-kebab) payload small.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_name: Option<String>,
    /// v1.0.84 (ADR-0042): discriminador do backend LLM que efetivamente
    /// executou o embedding da passagem. `"claude" | "codex" | "none"`.
    /// Absent on the wire when `None` (kept for happy-path envelope cleanliness).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_invoked: Option<&'static str>,
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
///     score: 0.88,
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
    /// Cosine similarity in `[0.0, 1.0]` derived as `1.0 - distance` and clamped
    /// to that interval. Always populated to satisfy the documented contract
    /// (M-A5 in v1.0.40); higher means more similar. For graph hits the value
    /// reflects the hop-derived distance proxy and should be interpreted
    /// alongside `graph_depth` rather than as a true cosine score.
    pub score: f32,
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

impl RecallItem {
    /// Computes the similarity score from a vector distance, clamped to
    /// `[0.0, 1.0]`. Cosine distance returned by sqlite-vec lives in `[0, 2]`
    /// in theory but the embedder produces unit-norm vectors so the practical
    /// range is `[0, 1]`. Centralized so every constructor keeps the contract.
    #[inline]
    pub fn score_from_distance(distance: f32) -> f32 {
        let raw = 1.0 - distance;
        if raw.is_nan() {
            0.0
        } else {
            raw.clamp(0.0, 1.0)
        }
    }
}

/// Full response envelope returned by the `recall` subcommand.
///
/// Contains both direct vector matches and graph-traversal matches, plus the
/// aggregated `results` list that merges both for callers that do not need
/// to distinguish the source.
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
    /// G58 (v1.0.80): `true` when the live query embedding failed and the
    /// handler fell back to FTS5 BM25 + LIKE prefix. Symmetric to
    /// `fts_degraded` in `hybrid-search`. Absent on the wire when false.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub vec_degraded: bool,
    /// G58 (v1.0.80): human-readable description of the embedding failure
    /// that triggered the fallback. Absent on the wire when `vec_degraded`
    /// is false or the failure had no message.
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub vec_error: Option<String>,
    /// G58 (v1.0.80): advisory warning echoed for callers that branch on
    /// top-level status. Distinguishes a FTS5-only fallback from a clean
    /// hybrid response so downstream pipelines can lower their confidence.
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub warning: Option<String>,
    /// v1.0.84 (ADR-0042): discriminador do backend LLM que efetivamente
    /// executou o embedding live. `"claude" | "codex" | "none"`. Absent
    /// on the wire when `None` (kept for happy-path envelope cleanliness).
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub backend_invoked: Option<&'static str>,
    /// v1.0.84 (ADR-0042): reason code discriminador de degradação
    /// (`"embedding_failed" | "cancelled" | "timeout"`). Absent when
    /// `vec_degraded` is false.
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub vec_degraded_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Dummy {
        val: u32,
    }

    // Non-serializable type to force a JSON serialization error
    struct NotSerializable;
    impl Serialize for NotSerializable {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom(
                "intentional serialization failure",
            ))
        }
    }

    #[test]
    fn emit_json_returns_ok_for_valid_value() {
        let v = Dummy { val: 42 };
        assert!(emit_json(&v).is_ok());
    }

    #[test]
    fn emit_json_returns_err_for_non_serializable_value() {
        let v = NotSerializable;
        assert!(emit_json(&v).is_err());
    }

    #[test]
    fn emit_json_compact_returns_ok_for_valid_value() {
        let v = Dummy { val: 7 };
        assert!(emit_json_compact(&v).is_ok());
    }

    #[test]
    fn emit_json_compact_returns_err_for_non_serializable_value() {
        let v = NotSerializable;
        assert!(emit_json_compact(&v).is_err());
    }

    #[test]
    fn emit_text_does_not_panic() {
        emit_text("mensagem de teste");
    }

    #[test]
    fn emit_progress_does_not_panic() {
        emit_progress("progresso de teste");
    }

    #[test]
    fn remember_response_serializes_correctly() {
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
            name_was_normalized: false,
            original_name: None,
            backend_invoked: None,
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
    fn recall_item_serializes_renamed_type_field() {
        let item = RecallItem {
            memory_id: 10,
            name: "entidade".to_string(),
            namespace: "ns".to_string(),
            memory_type: "entity".to_string(),
            description: "desc".to_string(),
            snippet: "trecho".to_string(),
            distance: 0.5,
            score: RecallItem::score_from_distance(0.5),
            source: "db".to_string(),
            graph_depth: None,
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"type\""));
        assert!(!json.contains("memory_type"));
        // Field is omitted from JSON when None.
        assert!(!json.contains("graph_depth"));
        assert!(json.contains("\"score\":0.5"));
    }

    #[test]
    fn recall_response_serializes_with_lists() {
        let resp = RecallResponse {
            query: "busca".to_string(),
            k: 10,
            direct_matches: vec![],
            graph_matches: vec![],
            results: vec![],
            elapsed_ms: 42,
            vec_degraded: false,
            vec_error: None,
            warning: None,
            backend_invoked: None,
            vec_degraded_reason: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("direct_matches"));
        assert!(json.contains("graph_matches"));
        assert!(json.contains("\"k\":"));
        assert!(json.contains("\"results\""));
        assert!(json.contains("\"elapsed_ms\""));
        // G58: clean response must NOT carry the degradation fields.
        assert!(!json.contains("vec_degraded"));
        assert!(!json.contains("vec_error"));
        assert!(!json.contains("warning"));
    }

    #[test]
    fn recall_response_serializes_vec_degraded_when_fallback_fired() {
        let resp = RecallResponse {
            query: "busca".to_string(),
            k: 10,
            direct_matches: vec![],
            graph_matches: vec![],
            results: vec![],
            elapsed_ms: 42,
            vec_degraded: true,
            vec_error: Some("embedding cancelled by external signal".to_string()),
            warning: Some("live query embedding unavailable; results are FTS5 BM25 only (semantic relevance reduced)".to_string()),
            backend_invoked: None,
            vec_degraded_reason: Some("embedding cancelled by external signal".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"vec_degraded\":true"));
        assert!(json.contains("\"vec_error\":\"embedding cancelled by external signal\""));
        assert!(json.contains("\"warning\":\"live query embedding unavailable"));
    }

    #[test]
    fn error_envelope_serializes_correctly() {
        #[derive(serde::Serialize)]
        struct ErrorEnvelope<'a> {
            error: bool,
            code: i32,
            message: &'a str,
        }
        let envelope = ErrorEnvelope {
            error: true,
            code: 10,
            message: "database disk image is malformed",
        };
        let json = serde_json::to_value(&envelope).unwrap();
        assert_eq!(json["error"], true);
        assert_eq!(json["code"], 10);
        assert_eq!(json["message"], "database disk image is malformed");
    }

    #[test]
    fn output_format_default_is_json() {
        let fmt = OutputFormat::default();
        assert!(matches!(fmt, OutputFormat::Json));
    }

    #[test]
    fn output_format_variants_exist() {
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
            score: RecallItem::score_from_distance(0.1),
            source: "src".to_string(),
            graph_depth: Some(2),
        };
        let cloned = item.clone();
        assert_eq!(cloned.memory_id, item.memory_id);
        assert_eq!(cloned.name, item.name);
        assert_eq!(cloned.graph_depth, Some(2));
    }
}
