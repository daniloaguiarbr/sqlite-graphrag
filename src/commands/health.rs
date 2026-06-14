//! Handler for the `health` CLI subcommand.

use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use serde::Serialize;
use std::fs;
use std::time::Instant;

const MEMORY_EMBEDDING_TABLES: &[&str] = &["memory_embeddings", "vec_memories"];
const ENTITY_EMBEDDING_TABLES: &[&str] = &["entity_embeddings", "vec_entities"];
const CHUNK_EMBEDDING_TABLES: &[&str] = &["chunk_embeddings", "vec_chunks"];

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Check database health (connectivity, integrity, vector index)\n  \
    sqlite-graphrag health\n\n  \
    # Check health of a database at a custom path\n  \
    sqlite-graphrag health --db /path/to/graphrag.sqlite\n\n  \
    # Use SQLITE_GRAPHRAG_DB_PATH env var\n  \
    SQLITE_GRAPHRAG_DB_PATH=/data/graphrag.sqlite sqlite-graphrag health")]
pub struct HealthArgs {
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// Explicit JSON flag. Accepted as a no-op because output is already JSON by default.
    #[arg(long, default_value_t = false)]
    pub json: bool,
    /// Output format: `json` or `text`. JSON is always emitted on stdout regardless of the value.
    #[arg(long, value_parser = ["json", "text"], hide = true)]
    pub format: Option<String>,
}

#[derive(Serialize)]
struct HealthCounts {
    memories: i64,
    /// Alias of `memories` for the documented contract in SKILL.md.
    memories_total: i64,
    entities: i64,
    relationships: i64,
    vec_memories: i64,
}

#[derive(Serialize)]
struct HealthCheck {
    name: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    integrity: String,
    integrity_ok: bool,
    schema_ok: bool,
    vec_memories_ok: bool,
    vec_memories_missing: i64,
    vec_memories_orphaned: i64,
    vec_entities_ok: bool,
    vec_chunks_ok: bool,
    fts_ok: bool,
    /// Whether a live FTS5 MATCH query against fts_memories succeeded.
    fts_query_ok: bool,
    model_ok: bool,
    counts: HealthCounts,
    db_path: String,
    db_size_bytes: u64,
    /// MAX(version) from refinery_schema_history — number of the last applied migration.
    /// Distinct from PRAGMA schema_version (SQLite DDL counter) and PRAGMA user_version
    /// (canonical SCHEMA_USER_VERSION from __debug_schema).
    schema_version: u32,
    /// List of entities referenced by memories but absent from the entities table.
    /// Empty in a healthy DB. Per the contract documented in SKILL.md.
    missing_entities: Vec<String>,
    /// WAL file size in MB (0.0 if WAL does not exist or journal_mode != wal).
    wal_size_mb: f64,
    /// SQLite journaling mode (wal, delete, truncate, persist, memory, off).
    journal_mode: String,
    /// SQLite version string, e.g. `"3.46.0"`.
    sqlite_version: String,
    /// Fraction of relationships that use the `mentions` relation type (0.0–1.0).
    /// Omitted when there are no relationships in the database.
    #[serde(skip_serializing_if = "Option::is_none")]
    mentions_ratio: Option<f64>,
    /// Human-readable warning when `mentions` relationships dominate the graph (ratio > 0.5).
    /// Omitted when the ratio is within acceptable bounds or there are no relationships.
    #[serde(skip_serializing_if = "Option::is_none")]
    mentions_warning: Option<String>,
    /// The relation type with the highest edge count in the namespace.
    /// Omitted when there are no relationships in the database.
    #[serde(skip_serializing_if = "Option::is_none")]
    top_relation: Option<String>,
    /// Fraction of all edges occupied by `top_relation` (0.0–1.0).
    /// Omitted when there are no relationships in the database.
    #[serde(skip_serializing_if = "Option::is_none")]
    top_relation_ratio: Option<f64>,
    /// Fraction of relationships that use the `applies_to` relation type (0.0–1.0).
    /// Omitted when there are no relationships or when `applies_to` is absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    applies_to_ratio: Option<f64>,
    /// Human-readable warning when a single relation type occupies more than 40 % of edges.
    /// Omitted when concentration is within acceptable bounds or there are no relationships.
    #[serde(skip_serializing_if = "Option::is_none")]
    relation_concentration_warning: Option<String>,
    /// Number of entities whose name differs from its normalized kebab-case form.
    #[serde(skip_serializing_if = "Option::is_none")]
    non_normalized_count: Option<i64>,
    /// Warning when non-normalized entities are detected.
    #[serde(skip_serializing_if = "Option::is_none")]
    normalization_warning: Option<String>,
    /// Number of entities with degree exceeding the super-hub threshold (default 50).
    #[serde(skip_serializing_if = "Option::is_none")]
    super_hub_count: Option<i64>,
    /// Warning listing top super-hub entity names.
    #[serde(skip_serializing_if = "Option::is_none")]
    super_hub_warning: Option<String>,
    /// Name of the entity with the highest connection count in the namespace.
    /// Omitted when there are no entities in the database.
    #[serde(skip_serializing_if = "Option::is_none")]
    top_hub_entity: Option<String>,
    /// Number of connections (degree) of `top_hub_entity`.
    /// Omitted when there are no entities in the database.
    #[serde(skip_serializing_if = "Option::is_none")]
    top_hub_degree: Option<i64>,
    /// Human-readable warning when `top_hub_entity` exceeds 50 connections.
    /// Omitted when degree is within acceptable bounds or there are no entities.
    #[serde(skip_serializing_if = "Option::is_none")]
    hub_warning: Option<String>,
    checks: Vec<HealthCheck>,
    elapsed_ms: u64,
}

/// Checks whether a table (including virtual ones) exists in sqlite_master.
fn table_exists(conn: &rusqlite::Connection, table_name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table', 'shadow') AND name = ?1",
        rusqlite::params![table_name],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

fn first_existing_table<'a>(
    conn: &rusqlite::Connection,
    candidates: &'a [&'a str],
) -> Option<&'a str> {
    candidates
        .iter()
        .copied()
        .find(|name| table_exists(conn, name))
}

fn count_rows(conn: &rusqlite::Connection, table_name: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table_name}"), [], |r| {
        r.get(0)
    })
    .unwrap_or(0)
}

fn memory_embedding_health(conn: &rusqlite::Connection) -> (bool, i64, i64, i64) {
    let Some(table_name) = first_existing_table(conn, MEMORY_EMBEDDING_TABLES) else {
        return (false, 0, 0, 0);
    };

    let total = count_rows(conn, table_name);
    let missing = conn
        .query_row(
            &format!(
                "SELECT COUNT(*)
                 FROM memories m
                 LEFT JOIN {table_name} me ON me.memory_id = m.id
                 WHERE me.memory_id IS NULL AND m.deleted_at IS NULL"
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let orphaned = conn
        .query_row(
            &format!(
                "SELECT COUNT(*)
                 FROM {table_name} me
                 LEFT JOIN memories m ON m.id = me.memory_id
                 WHERE m.id IS NULL OR m.deleted_at IS NOT NULL"
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    (true, total, missing, orphaned)
}

pub fn run(args: HealthArgs) -> Result<(), AppError> {
    let start = Instant::now();
    let _ = args.json; // --json is a no-op because output is already JSON by default
    let _ = args.format; // --format is a no-op; JSON is always emitted on stdout
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let conn = open_ro(&paths.db)?;

    let integrity: String = conn.query_row("PRAGMA integrity_check;", [], |r| r.get(0))?;
    let integrity_ok = integrity == "ok";
    tracing::info!(target: "health", integrity_ok = %integrity_ok, "PRAGMA integrity_check complete");

    if !integrity_ok {
        let db_size_bytes = fs::metadata(&paths.db).map(|m| m.len()).unwrap_or(0);
        output::emit_json(&HealthResponse {
            status: "degraded".to_string(),
            integrity: integrity.clone(),
            integrity_ok: false,
            schema_ok: false,
            vec_memories_ok: false,
            vec_memories_missing: 0,
            vec_memories_orphaned: 0,
            vec_entities_ok: false,
            vec_chunks_ok: false,
            fts_ok: false,
            fts_query_ok: false,
            model_ok: false,
            counts: HealthCounts {
                memories: 0,
                memories_total: 0,
                entities: 0,
                relationships: 0,
                vec_memories: 0,
            },
            db_path: paths.db.display().to_string(),
            db_size_bytes,
            schema_version: 0,
            sqlite_version: "unknown".to_string(),
            missing_entities: vec![],
            wal_size_mb: 0.0,
            journal_mode: "unknown".to_string(),
            mentions_ratio: None,
            mentions_warning: None,
            top_relation: None,
            top_relation_ratio: None,
            applies_to_ratio: None,
            relation_concentration_warning: None,
            non_normalized_count: None,
            normalization_warning: None,
            super_hub_count: None,
            super_hub_warning: None,
            top_hub_entity: None,
            top_hub_degree: None,
            hub_warning: None,
            checks: vec![HealthCheck {
                name: "integrity".to_string(),
                ok: false,
                detail: Some(integrity),
            }],
            elapsed_ms: start.elapsed().as_millis() as u64,
        })?;
        return Err(AppError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT),
            Some("integrity check failed".to_string()),
        )));
    }

    let memories_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    let entities_count: i64 = conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))?;
    let relationships_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM relationships", [], |r| r.get(0))?;
    let (vec_memories_ok, vec_memories_count, vec_memories_missing, vec_memories_orphaned) =
        memory_embedding_health(&conn);

    let mentions_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM relationships WHERE relation = 'mentions'",
        [],
        |r| r.get(0),
    )?;
    let (mentions_ratio, mentions_warning) = if relationships_count > 0 {
        let ratio = mentions_count as f64 / relationships_count as f64;
        let warning = if ratio > 0.5 {
            Some(format!(
                "mentions relationships dominate graph at {:.1}% ({}/{} total); consider running prune-relations --relation mentions --dry-run",
                ratio * 100.0,
                mentions_count,
                relationships_count
            ))
        } else {
            None
        };
        (Some(ratio), warning)
    } else {
        (None, None)
    };

    // Relation concentration: find the most frequent relation type and check threshold.
    let (top_relation, top_relation_ratio, applies_to_ratio, relation_concentration_warning) =
        if relationships_count > 0 {
            // Identify the relation with the highest edge count.
            let (top_rel, top_count): (String, i64) = conn
                .query_row(
                    "SELECT relation, COUNT(*) AS cnt
                     FROM relationships
                     GROUP BY relation
                     ORDER BY cnt DESC
                     LIMIT 1",
                    [],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
                )
                .unwrap_or_else(|_| ("unknown".to_string(), 0));

            let top_ratio = top_count as f64 / relationships_count as f64;

            // Compute applies_to ratio separately (may be 0 if absent).
            let applies_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM relationships WHERE relation = 'applies_to'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let at_ratio = if applies_count > 0 {
                Some(applies_count as f64 / relationships_count as f64)
            } else {
                None
            };

            let concentration_warning = if top_ratio > 0.40 {
                Some(format!(
                    "relation '{}' dominates graph at {:.1}% ({}/{} total); consider running prune-relations --relation {} --dry-run",
                    top_rel,
                    top_ratio * 100.0,
                    top_count,
                    relationships_count,
                    top_rel,
                ))
            } else {
                None
            };

            (
                Some(top_rel),
                Some(top_ratio),
                at_ratio,
                concentration_warning,
            )
        } else {
            (None, None, None, None)
        };

    let status = "ok";

    let schema_version: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM refinery_schema_history",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as u32;

    let schema_ok = schema_version > 0;

    // Checks vector tables via sqlite_master
    let vec_entities_ok = first_existing_table(&conn, ENTITY_EMBEDDING_TABLES).is_some();
    let vec_chunks_ok = first_existing_table(&conn, CHUNK_EMBEDDING_TABLES).is_some();

    tracing::info!(target: "health", vec_memories_ok = %vec_memories_ok, vec_entities_ok = %vec_entities_ok, vec_missing = vec_memories_missing, vec_orphaned = vec_memories_orphaned, "vector table checks complete");
    let fts_ok = table_exists(&conn, "fts_memories");

    // Verifies that FTS5 can execute a MATCH query (catches index corruption distinct from table absence).
    let fts_query_ok = if fts_ok {
        conn.query_row(
            "SELECT COUNT(*) FROM fts_memories WHERE fts_memories MATCH 'a' LIMIT 1",
            [],
            |r| r.get::<_, i64>(0),
        )
        .is_ok()
    } else {
        false
    };

    tracing::info!(target: "health", fts_ok = %fts_ok, fts_query_ok = %fts_query_ok, "FTS5 checks complete");

    // Captures the SQLite runtime version for observability.
    let sqlite_version: String = conn
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))
        .unwrap_or_else(|_| "unknown".to_string());

    // Detects orphan entities referenced by memories but absent from the entities table.
    let mut missing_entities: Vec<String> = Vec::with_capacity(4);
    let mut stmt = conn.prepare_cached(
        "SELECT DISTINCT me.entity_id
         FROM memory_entities me
         LEFT JOIN entities e ON e.id = me.entity_id
         WHERE e.id IS NULL",
    )?;
    let orphans: Vec<i64> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in orphans {
        missing_entities.push(format!("entity_id={id}"));
    }

    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
        .unwrap_or_else(|_| "unknown".to_string());

    let wal_size_mb = fs::metadata(format!("{}-wal", paths.db.display()))
        .map(|m| m.len() as f64 / 1024.0 / 1024.0)
        .unwrap_or(0.0);

    // Database file size in bytes
    let db_size_bytes = fs::metadata(&paths.db).map(|m| m.len()).unwrap_or(0);

    // G46: the ONNX model cache no longer exists in the LLM-only build
    // (v1.0.76+). model_ok now reports whether an LLM CLI (claude or codex)
    // is reachable on PATH — the real prerequisite for embedding generation.
    let model_ok = crate::commands::ingest_claude::find_claude_binary(None).is_ok()
        || crate::commands::ingest_codex::find_codex_binary(None).is_ok();
    tracing::info!(target: "health", model_ok = %model_ok, "LLM CLI availability check complete");

    // Builds the checks array for detailed diagnostics
    let mut checks: Vec<HealthCheck> = Vec::with_capacity(8);

    // At this point integrity_ok is always true (corrupt DB returned early above).
    checks.push(HealthCheck {
        name: "integrity".to_string(),
        ok: true,
        detail: None,
    });

    checks.push(HealthCheck {
        name: "schema_version".to_string(),
        ok: schema_ok,
        detail: if schema_ok {
            None
        } else {
            Some(format!("schema_version={schema_version} (expected >0)"))
        },
    });

    checks.push(HealthCheck {
        name: "vec_memories".to_string(),
        ok: vec_memories_ok,
        detail: if vec_memories_ok {
            None
        } else {
            Some("memory_embeddings/vec_memories table missing from sqlite_master".to_string())
        },
    });

    checks.push(HealthCheck {
        name: "vec_entities".to_string(),
        ok: vec_entities_ok,
        detail: if vec_entities_ok {
            None
        } else {
            Some("entity_embeddings/vec_entities table missing from sqlite_master".to_string())
        },
    });

    checks.push(HealthCheck {
        name: "vec_chunks".to_string(),
        ok: vec_chunks_ok,
        detail: if vec_chunks_ok {
            None
        } else {
            Some("chunk_embeddings/vec_chunks table missing from sqlite_master".to_string())
        },
    });

    checks.push(HealthCheck {
        name: "fts_memories".to_string(),
        ok: fts_ok,
        detail: if fts_ok {
            None
        } else {
            Some("fts_memories table missing from sqlite_master".to_string())
        },
    });

    checks.push(HealthCheck {
        name: "fts_query".to_string(),
        ok: fts_query_ok,
        detail: if fts_query_ok {
            None
        } else {
            Some("FTS5 MATCH query failed — run 'sqlite-graphrag fts rebuild'".to_string())
        },
    });

    checks.push(HealthCheck {
        name: "llm_cli".to_string(),
        ok: model_ok,
        detail: if model_ok {
            None
        } else {
            Some(
                "no LLM CLI found on PATH; install 'claude' (Claude Code) or 'codex' \
                 (Codex CLI) — required for embedding generation since v1.0.76"
                    .to_string(),
            )
        },
    });

    // G24: detect non-normalized entity names
    let (non_normalized_count, normalization_warning) = {
        let mut stmt = conn.prepare_cached("SELECT name FROM entities")?;
        let names: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        let count = names
            .iter()
            .filter(|n| crate::parsers::normalize_entity_name(n) != **n)
            .count() as i64;
        let warning = if count > 0 {
            Some(format!(
                "run 'normalize-entities --yes' to fix {count} non-normalized entities"
            ))
        } else {
            None
        };
        (Some(count), warning)
    };

    // G25: detect super-hub entities (degree > 50)
    let (super_hub_count, super_hub_warning) = {
        let mut stmt = conn.prepare_cached(
            "SELECT e.name, COUNT(r.id) as deg FROM entities e \
             LEFT JOIN relationships r ON e.id = r.source_id OR e.id = r.target_id \
             GROUP BY e.id HAVING deg > 50 ORDER BY deg DESC LIMIT 5",
        )?;
        let hubs: Vec<(String, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        let count = hubs.len() as i64;
        let warning = if count > 0 {
            let names: Vec<String> = hubs
                .iter()
                .map(|(n, d)| format!("{n} (degree {d})"))
                .collect();
            Some(format!("super-hubs detected: {}", names.join(", ")))
        } else {
            None
        };
        (Some(count), warning)
    };

    // G25 (extended): identify the single highest-degree entity for programmatic use.
    let (top_hub_entity, top_hub_degree, hub_warning) = {
        let result: Option<(String, i64)> = conn
            .query_row(
                "SELECT e.name, COUNT(r.id) AS degree
                 FROM entities e
                 LEFT JOIN relationships r ON e.id = r.source_id OR e.id = r.target_id
                 GROUP BY e.id
                 ORDER BY degree DESC
                 LIMIT 1",
                [],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            )
            .ok();
        match result {
            Some((name, degree)) => {
                let warning = if degree > 50 {
                    Some(format!(
                        "entity '{name}' has {degree} connections; consider splitting or using --max-neighbors-per-hop"
                    ))
                } else {
                    None
                };
                (Some(name), Some(degree), warning)
            }
            None => (None, None, None),
        }
    };

    let response = HealthResponse {
        status: status.to_string(),
        integrity,
        integrity_ok,
        schema_ok,
        vec_memories_ok,
        vec_memories_missing,
        vec_memories_orphaned,
        vec_entities_ok,
        vec_chunks_ok,
        fts_ok,
        fts_query_ok,
        model_ok,
        counts: HealthCounts {
            memories: memories_count,
            memories_total: memories_count,
            entities: entities_count,
            relationships: relationships_count,
            vec_memories: vec_memories_count,
        },
        db_path: paths.db.display().to_string(),
        db_size_bytes,
        schema_version,
        sqlite_version,
        missing_entities,
        wal_size_mb,
        journal_mode,
        mentions_ratio,
        mentions_warning,
        top_relation,
        top_relation_ratio,
        applies_to_ratio,
        relation_concentration_warning,
        non_normalized_count,
        normalization_warning,
        super_hub_count,
        super_hub_warning,
        top_hub_entity,
        top_hub_degree,
        hub_warning,
        checks,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };

    output::emit_json(&response)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn open_health_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE memories (
                id INTEGER PRIMARY KEY,
                deleted_at INTEGER
            );
            CREATE TABLE memory_embeddings (
                memory_id INTEGER PRIMARY KEY,
                namespace TEXT NOT NULL,
                embedding BLOB NOT NULL,
                source TEXT NOT NULL,
                model TEXT NOT NULL,
                dim INTEGER NOT NULL DEFAULT 384,
                created_at TEXT NOT NULL DEFAULT '0'
            );
            CREATE TABLE vec_memories (
                memory_id INTEGER PRIMARY KEY,
                embedding BLOB NOT NULL,
                created_at INTEGER NOT NULL DEFAULT 0
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn memory_embedding_health_prefers_memory_embeddings_and_counts_soft_deleted_as_orphaned() {
        let conn = open_health_test_db();
        conn.execute("INSERT INTO memories (id, deleted_at) VALUES (1, NULL)", [])
            .unwrap();
        conn.execute("INSERT INTO memories (id, deleted_at) VALUES (2, NULL)", [])
            .unwrap();
        conn.execute("INSERT INTO memories (id, deleted_at) VALUES (3, 123)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO memory_embeddings(memory_id, namespace, embedding, source, model, dim, created_at)
             VALUES (1, 'global', X'00', 'llm', 'm', 384, '1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memory_embeddings(memory_id, namespace, embedding, source, model, dim, created_at)
             VALUES (3, 'global', X'00', 'llm', 'm', 384, '2')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memory_embeddings(memory_id, namespace, embedding, source, model, dim, created_at)
             VALUES (99, 'global', X'00', 'llm', 'm', 384, '3')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO vec_memories(memory_id, embedding, created_at) VALUES (777, X'00', 0)",
            [],
        )
        .unwrap();

        let (ok, total, missing, orphaned) = memory_embedding_health(&conn);
        assert!(ok);
        assert_eq!(total, 3);
        assert_eq!(missing, 1);
        assert_eq!(orphaned, 2);
    }

    #[test]
    fn first_existing_table_falls_back_to_legacy_vec_name() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE vec_memories (
                memory_id INTEGER PRIMARY KEY,
                embedding BLOB NOT NULL,
                created_at INTEGER NOT NULL DEFAULT 0
            );",
        )
        .unwrap();

        let resolved = first_existing_table(&conn, MEMORY_EMBEDDING_TABLES);
        assert_eq!(resolved, Some("vec_memories"));
    }

    #[test]
    fn health_check_serializes_all_new_fields() {
        let response = HealthResponse {
            status: "ok".to_string(),
            integrity: "ok".to_string(),
            integrity_ok: true,
            schema_ok: true,
            vec_memories_ok: true,
            vec_memories_missing: 0,
            vec_memories_orphaned: 0,
            vec_entities_ok: true,
            vec_chunks_ok: true,
            fts_ok: true,
            fts_query_ok: true,
            model_ok: false,
            counts: HealthCounts {
                memories: 5,
                memories_total: 5,
                entities: 3,
                relationships: 2,
                vec_memories: 5,
            },
            db_path: "/tmp/test.sqlite".to_string(),
            db_size_bytes: 4096,
            schema_version: 6,
            sqlite_version: "3.46.0".to_string(),
            elapsed_ms: 0,
            missing_entities: vec![],
            wal_size_mb: 0.0,
            journal_mode: "wal".to_string(),
            mentions_ratio: None,
            mentions_warning: None,
            top_relation: None,
            top_relation_ratio: None,
            applies_to_ratio: None,
            relation_concentration_warning: None,
            non_normalized_count: None,
            normalization_warning: None,
            super_hub_count: None,
            super_hub_warning: None,
            top_hub_entity: None,
            top_hub_degree: None,
            hub_warning: None,
            checks: vec![
                HealthCheck {
                    name: "integrity".to_string(),
                    ok: true,
                    detail: None,
                },
                HealthCheck {
                    name: "model_onnx".to_string(),
                    ok: false,
                    detail: Some("model missing".to_string()),
                },
            ],
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["integrity_ok"], true);
        assert_eq!(json["schema_ok"], true);
        assert_eq!(json["vec_memories_ok"], true);
        assert_eq!(json["vec_entities_ok"], true);
        assert_eq!(json["vec_chunks_ok"], true);
        assert_eq!(json["fts_ok"], true);
        assert_eq!(json["model_ok"], false);
        assert_eq!(json["db_size_bytes"], 4096u64);
        assert!(json["checks"].is_array());
        assert_eq!(json["checks"].as_array().unwrap().len(), 2);

        // Verifies that detail is absent when ok=true (skip_serializing_if)
        let integrity_check = &json["checks"][0];
        assert_eq!(integrity_check["name"], "integrity");
        assert_eq!(integrity_check["ok"], true);
        assert!(integrity_check.get("detail").is_none());

        // Verifies that detail is present when ok=false
        let model_check = &json["checks"][1];
        assert_eq!(model_check["name"], "model_onnx");
        assert_eq!(model_check["ok"], false);
        assert_eq!(model_check["detail"], "model missing");
    }

    #[test]
    fn health_check_without_detail_omits_field() {
        let check = HealthCheck {
            name: "vec_memories".to_string(),
            ok: true,
            detail: None,
        };
        let json = serde_json::to_value(&check).unwrap();
        assert!(
            json.get("detail").is_none(),
            "detail field must be omitted when None"
        );
    }

    #[test]
    fn health_check_with_detail_serializes_field() {
        let check = HealthCheck {
            name: "fts_memories".to_string(),
            ok: false,
            detail: Some("fts_memories table missing from sqlite_master".to_string()),
        };
        let json = serde_json::to_value(&check).unwrap();
        assert_eq!(
            json["detail"],
            "fts_memories table missing from sqlite_master"
        );
    }

    #[test]
    fn health_response_fts_query_ok_and_sqlite_version_serialize() {
        // Verifies that fts_query_ok and sqlite_version appear in the serialized JSON
        // with the expected keys and values.
        let response = HealthResponse {
            status: "ok".to_string(),
            integrity: "ok".to_string(),
            integrity_ok: true,
            schema_ok: true,
            vec_memories_ok: true,
            vec_memories_missing: 0,
            vec_memories_orphaned: 0,
            vec_entities_ok: true,
            vec_chunks_ok: true,
            fts_ok: true,
            fts_query_ok: true,
            model_ok: true,
            counts: HealthCounts {
                memories: 0,
                memories_total: 0,
                entities: 0,
                relationships: 0,
                vec_memories: 0,
            },
            db_path: "/tmp/test.sqlite".to_string(),
            db_size_bytes: 0,
            schema_version: 1,
            sqlite_version: "3.45.1".to_string(),
            elapsed_ms: 0,
            missing_entities: vec![],
            wal_size_mb: 0.0,
            journal_mode: "wal".to_string(),
            mentions_ratio: None,
            mentions_warning: None,
            top_relation: None,
            top_relation_ratio: None,
            applies_to_ratio: None,
            relation_concentration_warning: None,
            non_normalized_count: None,
            normalization_warning: None,
            super_hub_count: None,
            super_hub_warning: None,
            top_hub_entity: None,
            top_hub_degree: None,
            hub_warning: None,
            checks: vec![],
        };

        let json = serde_json::to_value(&response).unwrap();

        // fts_query_ok must appear at the top level
        assert_eq!(
            json["fts_query_ok"], true,
            "fts_query_ok must be present and true in serialized JSON"
        );

        // sqlite_version must appear at the top level with the exact string
        assert_eq!(
            json["sqlite_version"], "3.45.1",
            "sqlite_version must be present and match the provided string"
        );

        // Verify fts_query_ok=false path includes the expected detail message
        let check_fail = HealthCheck {
            name: "fts_query".to_string(),
            ok: false,
            detail: Some("FTS5 MATCH query failed — run 'sqlite-graphrag fts rebuild'".to_string()),
        };
        let check_json = serde_json::to_value(&check_fail).unwrap();
        assert_eq!(check_json["name"], "fts_query");
        assert_eq!(check_json["ok"], false);
        assert_eq!(
            check_json["detail"],
            "FTS5 MATCH query failed — run 'sqlite-graphrag fts rebuild'"
        );
    }

    fn make_full_response(
        top_relation: Option<String>,
        top_relation_ratio: Option<f64>,
        applies_to_ratio: Option<f64>,
        relation_concentration_warning: Option<String>,
    ) -> HealthResponse {
        HealthResponse {
            status: "ok".to_string(),
            integrity: "ok".to_string(),
            integrity_ok: true,
            schema_ok: true,
            vec_memories_ok: true,
            vec_memories_missing: 0,
            vec_memories_orphaned: 0,
            vec_entities_ok: true,
            vec_chunks_ok: true,
            fts_ok: true,
            fts_query_ok: true,
            model_ok: true,
            counts: HealthCounts {
                memories: 10,
                memories_total: 10,
                entities: 5,
                relationships: 20,
                vec_memories: 10,
            },
            db_path: "/tmp/test.sqlite".to_string(),
            db_size_bytes: 8192,
            schema_version: 3,
            sqlite_version: "3.46.0".to_string(),
            elapsed_ms: 1,
            missing_entities: vec![],
            wal_size_mb: 0.0,
            journal_mode: "wal".to_string(),
            mentions_ratio: None,
            mentions_warning: None,
            top_relation,
            top_relation_ratio,
            applies_to_ratio,
            relation_concentration_warning,
            non_normalized_count: None,
            normalization_warning: None,
            super_hub_count: None,
            super_hub_warning: None,
            top_hub_entity: None,
            top_hub_degree: None,
            hub_warning: None,
            checks: vec![],
        }
    }

    #[test]
    fn health_concentration_fields_omitted_when_no_relationships() {
        // Represents a DB with zero relationships.
        let resp = make_full_response(None, None, None, None);
        let json = serde_json::to_value(&resp).unwrap();
        assert!(
            json.get("top_relation").is_none(),
            "top_relation must be omitted when None"
        );
        assert!(
            json.get("top_relation_ratio").is_none(),
            "top_relation_ratio must be omitted when None"
        );
        assert!(
            json.get("applies_to_ratio").is_none(),
            "applies_to_ratio must be omitted when None"
        );
        assert!(
            json.get("relation_concentration_warning").is_none(),
            "relation_concentration_warning must be omitted when None"
        );
    }

    #[test]
    fn health_concentration_fields_present_with_data() {
        let resp = make_full_response(
            Some("mentions".to_string()),
            Some(0.60),
            Some(0.10),
            Some("relation 'mentions' dominates graph at 60.0%".to_string()),
        );
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["top_relation"], "mentions");
        assert!((json["top_relation_ratio"].as_f64().unwrap() - 0.60).abs() < 1e-9);
        assert!((json["applies_to_ratio"].as_f64().unwrap() - 0.10).abs() < 1e-9);
        assert!(json["relation_concentration_warning"]
            .as_str()
            .unwrap()
            .contains("60.0%"));
    }

    #[test]
    fn health_concentration_warning_absent_when_ratio_below_threshold() {
        // top_relation_ratio of 0.39 is below the 0.40 threshold — no warning.
        let resp = make_full_response(Some("uses".to_string()), Some(0.39), None, None);
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["top_relation"], "uses");
        assert!(
            json.get("relation_concentration_warning").is_none(),
            "warning must be absent when ratio <= 0.40"
        );
    }

    #[test]
    fn health_concentration_warning_present_at_threshold() {
        // Exactly at 0.41 (above 0.40) — warning must appear.
        let resp = make_full_response(
            Some("depends_on".to_string()),
            Some(0.41),
            None,
            Some("relation 'depends_on' dominates graph at 41.0%".to_string()),
        );
        let json = serde_json::to_value(&resp).unwrap();
        assert!(
            json["relation_concentration_warning"].is_string(),
            "warning must be present when top_relation_ratio > 0.40"
        );
    }

    #[test]
    fn health_applies_to_ratio_omitted_when_none() {
        // applies_to_ratio is None when there are no applies_to edges.
        let resp = make_full_response(Some("related".to_string()), Some(0.30), None, None);
        let json = serde_json::to_value(&resp).unwrap();
        assert!(
            json.get("applies_to_ratio").is_none(),
            "applies_to_ratio must be omitted when None"
        );
    }
}
