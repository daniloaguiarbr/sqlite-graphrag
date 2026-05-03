//! Handler for the `health` CLI subcommand.

use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use serde::Serialize;
use std::fs;
use std::time::Instant;

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
    /// Alias of `memories` for the documented contract in AGENT_PROTOCOL.md.
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
    vec_entities_ok: bool,
    vec_chunks_ok: bool,
    fts_ok: bool,
    model_ok: bool,
    counts: HealthCounts,
    db_path: String,
    db_size_bytes: u64,
    /// MAX(version) from refinery_schema_history — number of the last applied migration.
    /// Distinct from PRAGMA schema_version (SQLite DDL counter) and PRAGMA user_version
    /// (canonical SCHEMA_USER_VERSION from __debug_schema).
    schema_version: u32,
    /// List of entities referenced by memories but absent from the entities table.
    /// Empty in a healthy DB. Per the contract documented in AGENT_PROTOCOL.md.
    missing_entities: Vec<String>,
    /// WAL file size in MB (0.0 if WAL does not exist or journal_mode != wal).
    wal_size_mb: f64,
    /// Modo de journaling do SQLite (wal, delete, truncate, persist, memory, off).
    journal_mode: String,
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

pub fn run(args: HealthArgs) -> Result<(), AppError> {
    let start = Instant::now();
    let _ = args.json; // --json is a no-op because output is already JSON by default
    let _ = args.format; // --format is a no-op; JSON is always emitted on stdout
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let conn = open_ro(&paths.db)?;

    let integrity: String = conn.query_row("PRAGMA integrity_check;", [], |r| r.get(0))?;
    let integrity_ok = integrity == "ok";

    if !integrity_ok {
        let db_size_bytes = fs::metadata(&paths.db).map(|m| m.len()).unwrap_or(0);
        output::emit_json(&HealthResponse {
            status: "degraded".to_string(),
            integrity: integrity.clone(),
            integrity_ok: false,
            schema_ok: false,
            vec_memories_ok: false,
            vec_entities_ok: false,
            vec_chunks_ok: false,
            fts_ok: false,
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
            missing_entities: vec![],
            wal_size_mb: 0.0,
            journal_mode: "unknown".to_string(),
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
    let vec_memories_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM vec_memories", [], |r| r.get(0))?;

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
    let vec_memories_ok = table_exists(&conn, "vec_memories");
    let vec_entities_ok = table_exists(&conn, "vec_entities");
    let vec_chunks_ok = table_exists(&conn, "vec_chunks");
    let fts_ok = table_exists(&conn, "fts_memories");

    // Detects orphan entities referenced by memories but absent from the entities table.
    let mut missing_entities: Vec<String> = Vec::new();
    let mut stmt = conn.prepare(
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

    // Checks whether the ONNX model is present in the cache
    let model_dir = paths.models.join("models--intfloat--multilingual-e5-small");
    let model_ok = model_dir.exists();

    // Builds the checks array for detailed diagnostics
    let mut checks: Vec<HealthCheck> = Vec::new();

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
            Some("vec_memories table missing from sqlite_master".to_string())
        },
    });

    checks.push(HealthCheck {
        name: "vec_entities".to_string(),
        ok: vec_entities_ok,
        detail: if vec_entities_ok {
            None
        } else {
            Some("vec_entities table missing from sqlite_master".to_string())
        },
    });

    checks.push(HealthCheck {
        name: "vec_chunks".to_string(),
        ok: vec_chunks_ok,
        detail: if vec_chunks_ok {
            None
        } else {
            Some("vec_chunks table missing from sqlite_master".to_string())
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
        name: "model_onnx".to_string(),
        ok: model_ok,
        detail: if model_ok {
            None
        } else {
            Some(format!(
                "model missing at {}; run 'sqlite-graphrag models download'",
                model_dir.display()
            ))
        },
    });

    let response = HealthResponse {
        status: status.to_string(),
        integrity,
        integrity_ok,
        schema_ok,
        vec_memories_ok,
        vec_entities_ok,
        vec_chunks_ok,
        fts_ok,
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
        missing_entities,
        wal_size_mb,
        journal_mode,
        checks,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };

    output::emit_json(&response)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_check_serializes_all_new_fields() {
        let resposta = HealthResponse {
            status: "ok".to_string(),
            integrity: "ok".to_string(),
            integrity_ok: true,
            schema_ok: true,
            vec_memories_ok: true,
            vec_entities_ok: true,
            vec_chunks_ok: true,
            fts_ok: true,
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
            elapsed_ms: 0,
            missing_entities: vec![],
            wal_size_mb: 0.0,
            journal_mode: "wal".to_string(),
            checks: vec![
                HealthCheck {
                    name: "integrity".to_string(),
                    ok: true,
                    detail: None,
                },
                HealthCheck {
                    name: "model_onnx".to_string(),
                    ok: false,
                    detail: Some("modelo ausente".to_string()),
                },
            ],
        };

        let json = serde_json::to_value(&resposta).unwrap();
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
        assert_eq!(model_check["detail"], "modelo ausente");
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
            "campo detail deve ser omitido quando None"
        );
    }

    #[test]
    fn health_check_with_detail_serializes_field() {
        let check = HealthCheck {
            name: "fts_memories".to_string(),
            ok: false,
            detail: Some("tabela fts_memories ausente".to_string()),
        };
        let json = serde_json::to_value(&check).unwrap();
        assert_eq!(json["detail"], "tabela fts_memories ausente");
    }
}
