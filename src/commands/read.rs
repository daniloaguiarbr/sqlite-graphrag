//! Handler for the `read` CLI subcommand.

use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use crate::storage::memories;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Read a memory by name (positional)\n  \
    sqlite-graphrag read onboarding\n\n  \
    # Read using the named flag form\n  \
    sqlite-graphrag read --name onboarding\n\n  \
    # Read by memory ID (integer emitted in JSON output of most commands)\n  \
    sqlite-graphrag read --id 42 --json\n\n  \
    # Read from a specific namespace\n  \
    sqlite-graphrag read onboarding --namespace my-project")]
pub struct ReadArgs {
    /// Memory name as a positional argument. Alternative to `--name`.
    #[arg(
        value_name = "NAME",
        conflicts_with = "name",
        help = "Memory name (kebab-case slug); alternative to --name"
    )]
    pub name_positional: Option<String>,
    /// Memory name to read. Returns NotFound (exit 4) if missing or soft-deleted.
    #[arg(long)]
    pub name: Option<String>,
    /// Memory ID (integer) for direct lookup. Conflicts with --name and positional NAME.
    #[arg(
        long,
        conflicts_with_all = ["name", "name_positional"],
        help = "Memory ID (integer) for direct lookup"
    )]
    pub id: Option<i64>,
    #[arg(
        long,
        help = "Namespace (env: SQLITE_GRAPHRAG_NAMESPACE, default: global)"
    )]
    pub namespace: Option<String>,
    /// Include linked entities and relationships in the response.
    #[arg(
        long,
        help = "Include graph context (entities + relationships) in response"
    )]
    pub with_graph: bool,
    /// Output format: `json` (default, full envelope) or `raw` (the pure memory
    /// body to stdout, no JSON wrapper). GAP-SG-50: `raw` lets the body be piped
    /// without a `jaq -r '.body'` round-trip.
    #[arg(
        long,
        value_enum,
        default_value_t = ReadFormat::Json,
        help = "Output format: json (default) or raw (pure body to stdout)"
    )]
    pub format: ReadFormat,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

/// GAP-SG-50: output format for `read`. `Raw` emits the pure body; `Json`
/// emits the full structured envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Default)]
#[value(rename_all = "lowercase")]
pub enum ReadFormat {
    #[default]
    Json,
    Raw,
}

#[derive(Serialize)]
struct ReadResponse {
    /// Canonical storage field. Preserved for compatibility with v2.0.0 clients.
    id: i64,
    /// Semantic alias of `id` for the contract documented in SKILL.md.
    memory_id: i64,
    namespace: String,
    name: String,
    /// Semantic alias of `memory_type` for the documented contract.
    #[serde(rename = "type")]
    type_alias: String,
    memory_type: String,
    description: String,
    body: String,
    body_hash: String,
    session_id: Option<String>,
    source: String,
    metadata: serde_json::Value,
    /// Most recent memory version, useful for optimistic control via `--expected-updated-at`.
    version: i64,
    created_at: i64,
    /// RFC 3339 UTC timestamp parallel to `created_at` for ISO 8601 parsers.
    created_at_iso: String,
    updated_at: i64,
    /// RFC 3339 UTC timestamp parallel to `updated_at` for ISO 8601 parsers.
    updated_at_iso: String,
    /// Linked entities (opt-in via --with-graph).
    #[serde(skip_serializing_if = "Option::is_none")]
    entities: Option<Vec<ReadEntityBinding>>,
    /// Relationships from linked entities (opt-in via --with-graph).
    #[serde(skip_serializing_if = "Option::is_none")]
    relationships: Option<Vec<ReadRelationshipBinding>>,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct ReadEntityBinding {
    entity_id: i64,
    name: String,
    entity_type: String,
}

#[derive(Serialize)]
struct ReadRelationshipBinding {
    from: String,
    to: String,
    relation: String,
    weight: f64,
}

fn epoch_to_iso(epoch: i64) -> String {
    crate::tz::epoch_to_iso(epoch)
}

pub fn run(args: ReadArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    crate::storage::connection::ensure_db_ready(&paths)?;
    let conn = open_ro(&paths.db)?;

    let row_opt = if let Some(id) = args.id {
        let r = memories::read_full(&conn, id)?;
        if let Some(ref row) = r {
            if row.namespace != namespace {
                return Err(AppError::NotFound(format!(
                    "memory id {id} exists but belongs to namespace '{}', not '{namespace}'",
                    row.namespace
                )));
            }
        }
        if r.is_none() {
            // G55 S2: surface the requested id structurally so the message
            // never drops it for the legacy `unknown` literal.
            return Err(AppError::MemoryNotFoundById { id });
        }
        r
    } else {
        let name = args
            .name_positional
            .clone()
            .or(args.name.clone())
            .ok_or_else(|| {
                AppError::Validation(
                "name or --id required: pass name as positional argument, via --name, or use --id"
                    .to_string(),
            )
            })?;
        memories::read_by_name(&conn, &namespace, &name)?
    };

    match row_opt {
        Some(row) => {
            // GAP-SG-50: `--format raw` emits the pure body and returns early,
            // before building the JSON envelope. The body is written verbatim so
            // it can be redirected to a file or piped without parsing.
            if args.format == ReadFormat::Raw {
                output::emit_raw(row.body.as_bytes());
                return Ok(());
            }
            // Resolve current version via memory_versions table (highest version for this memory_id).
            let version: i64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(version), 1) FROM memory_versions WHERE memory_id=?1",
                    rusqlite::params![row.id],
                    |r| r.get(0),
                )
                .unwrap_or(1);

            // G22: optional graph context
            let (entities, relationships) = if args.with_graph {
                let mut ent_stmt = conn.prepare_cached(
                    "SELECT e.id, e.name, e.type FROM memory_entities me \
                     JOIN entities e ON e.id = me.entity_id \
                     WHERE me.memory_id = ?1",
                )?;
                let ents: Vec<ReadEntityBinding> = ent_stmt
                    .query_map(rusqlite::params![row.id], |r| {
                        Ok(ReadEntityBinding {
                            entity_id: r.get(0)?,
                            name: r.get(1)?,
                            entity_type: r.get(2)?,
                        })
                    })?
                    .filter_map(|r| r.ok())
                    .collect();
                drop(ent_stmt);

                let entity_ids: Vec<i64> = ents.iter().map(|e| e.entity_id).collect();
                let rels: Vec<ReadRelationshipBinding> = if !entity_ids.is_empty() {
                    let placeholders: String = entity_ids
                        .iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    let sql = format!(
                        "SELECT e1.name, e2.name, r.relation, r.weight \
                         FROM relationships r \
                         JOIN entities e1 ON e1.id = r.source_id \
                         JOIN entities e2 ON e2.id = r.target_id \
                         WHERE r.source_id IN ({placeholders}) OR r.target_id IN ({placeholders})"
                    );
                    let mut rel_stmt = conn.prepare(&sql)?;
                    let result: Vec<ReadRelationshipBinding> = rel_stmt
                        .query_map([], |r| {
                            Ok(ReadRelationshipBinding {
                                from: r.get(0)?,
                                to: r.get(1)?,
                                relation: r.get(2)?,
                                weight: r.get(3)?,
                            })
                        })?
                        .filter_map(|r| r.ok())
                        .collect();
                    drop(rel_stmt);
                    result
                } else {
                    vec![]
                };
                (Some(ents), Some(rels))
            } else {
                (None, None)
            };

            let response = ReadResponse {
                id: row.id,
                memory_id: row.id,
                namespace: row.namespace,
                name: row.name,
                type_alias: row.memory_type.clone(),
                memory_type: row.memory_type,
                description: row.description,
                body: row.body,
                body_hash: row.body_hash,
                session_id: row.session_id,
                source: row.source,
                metadata: serde_json::from_str::<serde_json::Value>(&row.metadata)
                    .unwrap_or(serde_json::Value::Null),
                version,
                created_at: row.created_at,
                created_at_iso: epoch_to_iso(row.created_at),
                updated_at: row.updated_at,
                updated_at_iso: epoch_to_iso(row.updated_at),
                entities,
                relationships,
                elapsed_ms: start.elapsed().as_millis() as u64,
            };
            output::emit_json(&response)?;
        }
        None => {
            // G55 S2: when the lookup target is a name, use the structural
            // `MemoryNotFound { name, namespace }` variant so the message is
            // guaranteed to carry the requested identifier. The legacy
            // `NotFound(String)` path is only reached via the `--id` branch
            // (which now emits `MemoryNotFoundById` structurally a few lines
            // above) or when a future caller needs ad-hoc messages.
            if let Some(name) = args.name_positional.as_deref().or(args.name.as_deref()) {
                return Err(AppError::MemoryNotFound {
                    name: name.to_string(),
                    namespace: namespace.clone(),
                });
            }
            // Fallback: id lookup that did not match (defensive — the
            // MemoryNotFoundById branch above already returned in the
            // normal id-miss path).
            if let Some(id) = args.id {
                return Err(AppError::MemoryNotFoundById { id });
            }
            // Unreachable: the `else` branch above already validated that
            // one of name/id is set. Keep a defensive message for future
            // refactors that may restructure the lookup arms.
            return Err(AppError::Validation(
                "internal: read reached NotFound without name or id".to_string(),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // GAP-SG-50: `read --format raw` must parse to ReadFormat::Raw; default is Json.
    #[test]
    fn read_format_flag_parses_raw_and_defaults_json() {
        use crate::cli::{Cli, Commands};
        use clap::Parser;

        let raw = Cli::try_parse_from(["sqlite-graphrag", "read", "my-mem", "--format", "raw"])
            .expect("parse raw");
        match raw.command {
            Some(Commands::Read(a)) => assert_eq!(a.format, ReadFormat::Raw),
            other => panic!("expected read, got {other:?}"),
        }

        let dflt = Cli::try_parse_from(["sqlite-graphrag", "read", "my-mem"]).expect("parse");
        match dflt.command {
            Some(Commands::Read(a)) => assert_eq!(a.format, ReadFormat::Json),
            other => panic!("expected read, got {other:?}"),
        }
    }

    #[test]
    fn epoch_to_iso_converts_zero_to_unix_epoch() {
        // v1.0.68 (test fix): parse the ISO back into a DateTime<FixedOffset>
        // and compare with chrono::DateTime::UNIX_EPOCH so the assertion is
        // timezone-agnostic.  The previous `starts_with("1970-01-01T00:00:00")`
        // assertion leaked the global SQLITE_GRAPHRAG_DISPLAY_TZ from sibling
        // tests in the same process and failed on hosts where the default
        // timezone is non-UTC.
        let result = epoch_to_iso(0);
        let parsed = chrono::DateTime::parse_from_rfc3339(&result)
            .unwrap_or_else(|e| panic!("epoch_to_iso(0) returned non-RFC3339 `{result}`: {e}"));
        assert_eq!(
            parsed.timestamp(),
            chrono::DateTime::UNIX_EPOCH.timestamp(),
            "epoch 0 must map to the Unix epoch instant, got: {result}"
        );
    }

    #[test]
    fn epoch_to_iso_converts_known_timestamp() {
        // v1.0.68 (test fix): 1_705_320_000 = 2024-01-15T12:00:00Z, not
        // 2024-01-15T00:00:00Z (the previous test asserted the wrong instant).
        // The fix uses parse + timestamp compare to be timezone-agnostic and
        // to catch wrong-epoch regressions regardless of host TZ.
        let result = epoch_to_iso(1_705_320_000);
        let parsed = chrono::DateTime::parse_from_rfc3339(&result).unwrap_or_else(|e| {
            panic!("epoch_to_iso(1705320000) returned non-RFC3339 `{result}`: {e}")
        });
        let expected = chrono::DateTime::parse_from_rfc3339("2024-01-15T12:00:00+00:00")
            .expect("static RFC3339 is valid");
        assert_eq!(
            parsed.timestamp(),
            expected.timestamp(),
            "timestamp 1705320000 must map to 2024-01-15T12:00:00Z, got: {result}"
        );
    }

    #[test]
    fn epoch_to_iso_returns_fallback_for_invalid_negative_epoch() {
        let result = epoch_to_iso(i64::MIN);
        assert!(
            !result.is_empty(),
            "must return a non-empty string even for invalid epoch"
        );
    }

    #[test]
    fn read_response_serializes_id_and_memory_id_aliases() {
        let resp = ReadResponse {
            id: 42,
            memory_id: 42,
            namespace: "global".to_string(),
            name: "my-mem".to_string(),
            type_alias: "fact".to_string(),
            memory_type: "fact".to_string(),
            description: "desc".to_string(),
            body: "body".to_string(),
            body_hash: "abc123".to_string(),
            session_id: None,
            source: "agent".to_string(),
            metadata: serde_json::json!({}),
            version: 1,
            created_at: 1_705_320_000,
            created_at_iso: "2024-01-15T12:00:00Z".to_string(),
            updated_at: 1_705_320_000,
            updated_at_iso: "2024-01-15T12:00:00Z".to_string(),
            entities: None,
            relationships: None,
            elapsed_ms: 5,
        };

        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["id"], 42);
        assert_eq!(json["memory_id"], 42);
        assert_eq!(json["type"], "fact");
        assert_eq!(json["memory_type"], "fact");
        assert_eq!(json["elapsed_ms"], 5u64);
        assert!(
            json["session_id"].is_null(),
            "session_id None must serialize as null"
        );
        // metadata must serialize as a JSON object, not as an escaped string
        assert!(
            json["metadata"].is_object(),
            "metadata must be a JSON object"
        );
    }

    #[test]
    fn read_response_session_id_some_serializes_string() {
        let resp = ReadResponse {
            id: 1,
            memory_id: 1,
            namespace: "global".to_string(),
            name: "mem".to_string(),
            type_alias: "skill".to_string(),
            memory_type: "skill".to_string(),
            description: "d".to_string(),
            body: "b".to_string(),
            body_hash: "h".to_string(),
            session_id: Some("sess-123".to_string()),
            source: "agent".to_string(),
            metadata: serde_json::json!({}),
            version: 2,
            created_at: 0,
            created_at_iso: "1970-01-01T00:00:00Z".to_string(),
            updated_at: 0,
            updated_at_iso: "1970-01-01T00:00:00Z".to_string(),
            entities: None,
            relationships: None,
            elapsed_ms: 0,
        };

        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["session_id"], "sess-123");
    }

    #[test]
    fn read_response_elapsed_ms_is_present() {
        let resp = ReadResponse {
            id: 7,
            memory_id: 7,
            namespace: "ns".to_string(),
            name: "n".to_string(),
            type_alias: "procedure".to_string(),
            memory_type: "procedure".to_string(),
            description: "d".to_string(),
            body: "b".to_string(),
            body_hash: "h".to_string(),
            session_id: None,
            source: "agent".to_string(),
            metadata: serde_json::json!({}),
            version: 3,
            created_at: 1000,
            created_at_iso: "1970-01-01T00:16:40Z".to_string(),
            updated_at: 2000,
            updated_at_iso: "1970-01-01T00:33:20Z".to_string(),
            entities: None,
            relationships: None,
            elapsed_ms: 123,
        };

        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["elapsed_ms"], 123u64);
        assert!(json["created_at_iso"].is_string());
        assert!(json["updated_at_iso"].is_string());
    }

    #[test]
    fn read_response_metadata_object_not_escaped_string() {
        // P2-A: metadata must serialize as a JSON object, not as an escaped string.
        let resp = ReadResponse {
            id: 3,
            memory_id: 3,
            namespace: "ns".to_string(),
            name: "meta-test".to_string(),
            type_alias: "fact".to_string(),
            memory_type: "fact".to_string(),
            description: "d".to_string(),
            body: "b".to_string(),
            body_hash: "h".to_string(),
            session_id: None,
            source: "agent".to_string(),
            metadata: serde_json::json!({"key": "value", "number": 42}),
            version: 1,
            created_at: 0,
            created_at_iso: "1970-01-01T00:00:00Z".to_string(),
            updated_at: 0,
            updated_at_iso: "1970-01-01T00:00:00Z".to_string(),
            entities: None,
            relationships: None,
            elapsed_ms: 1,
        };

        let json = serde_json::to_value(&resp).expect("serialization failed");
        // Must be object, not a JSON string containing escaped JSON.
        assert!(json["metadata"].is_object());
        assert_eq!(json["metadata"]["key"], "value");
        assert_eq!(json["metadata"]["number"], 42);
    }

    #[test]
    fn read_response_metadata_fallback_to_null_for_invalid_json() {
        // P2-A: fallback when metadata is an invalid string.
        let raw = "invalid-json{{{";
        let parsed =
            serde_json::from_str::<serde_json::Value>(raw).unwrap_or(serde_json::Value::Null);
        assert!(parsed.is_null());
    }

    // G55 S2 (v1.0.80): the structural `MemoryNotFound` variant must include
    // the requested name and namespace in the message — never the legacy
    // `unknown` literal that masked which lookup target failed.
    #[test]
    fn memory_not_found_structural_includes_name_and_namespace() {
        let err = AppError::MemoryNotFound {
            name: "atomwrite-projeto-contexto".to_string(),
            namespace: "global".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("atomwrite-projeto-contexto"), "got: {msg}");
        assert!(msg.contains("global"), "got: {msg}");
        assert!(
            !msg.contains("unknown"),
            "must not contain 'unknown': {msg}"
        );
        assert_eq!(err.exit_code(), 4);
        assert!(err.is_permanent());
    }

    #[test]
    fn memory_not_found_by_id_structural_includes_id() {
        let err = AppError::MemoryNotFoundById { id: 42 };
        let msg = err.to_string();
        assert!(msg.contains("42"), "got: {msg}");
        assert!(msg.contains("id=42"), "got: {msg}");
        assert_eq!(err.exit_code(), 4);
    }

    #[test]
    fn memory_not_found_pt_br_drops_english_fragments() {
        // The pt-BR translation must not contain leftover English fragments
        // like "not found" — that was the original G55 bug.
        use crate::i18n::Language;
        let err = AppError::MemoryNotFound {
            name: "mem-fantasma".to_string(),
            namespace: "global".to_string(),
        };
        let pt = err.localized_message_for(Language::Portuguese);
        assert!(!pt.contains("not found"), "pt-BR fragment leaked: {pt}");
        assert!(pt.contains("mem-fantasma"), "name missing in pt: {pt}");
        assert!(pt.contains("global"), "namespace missing in pt: {pt}");
    }
}
