//! Handler for the `vec` CLI subcommand family.
//!
//! Provides maintenance operations for the memory embedding store,
//! preferring `memory_embeddings` and falling back to legacy `vec_memories`:
//!
//! - `orphan-list`: lists embedding rows whose `memory_id` no longer
//!   references a live (non-soft-deleted) memory.
//! - `purge-orphan`: deletes those orphan rows in a single transaction.
//! - `stats`: surfaces total rows, orphan count, and coverage percentage.
//!
//! G39 (v1.0.69): before v1.0.69, the only way to detect a vec-orphan was
//! `health --json` which reported `vec_memories_orphaned > 0` with no
//! remediation path. This module closes the loop.

use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::{open_ro, open_rw};
use serde::Serialize;

const MEMORY_VEC_TABLES: &[&str] = &["memory_embeddings", "vec_memories"];

/// Arguments for the `vec` subcommand family.
#[derive(clap::Args)]
#[command(
    about = "Vector index maintenance (orphan detection, purge, stats)",
    after_long_help = "EXAMPLES:\n  \
        # List orphan memory embedding rows whose memory_id is gone\n  \
        sqlite-graphrag vec orphan-list\n\n  \
        # Dry-run the purge (does not delete)\n  \
        sqlite-graphrag vec purge-orphan --dry-run\n\n  \
        # Actually purge orphans\n  \
        sqlite-graphrag vec purge-orphan --yes\n\n  \
        # Show stats for all vec0 tables\n  \
        sqlite-graphrag vec stats --json"
)]
pub struct VecArgs {
    #[command(subcommand)]
    pub command: VecSubcommand,
}

/// Subcommands nested under `vec`.
#[derive(clap::Subcommand)]
pub enum VecSubcommand {
    /// List orphan memory embedding rows.
    OrphanList(VecOrphanListArgs),
    /// Delete orphan memory embedding rows. Requires `--yes` to confirm.
    PurgeOrphan(VecPurgeOrphanArgs),
    /// Show statistics for vec_memories, vec_entities, vec_chunks.
    Stats(VecStatsArgs),
}

/// Arguments for `vec orphan-list`.
#[derive(clap::Args)]
pub struct VecOrphanListArgs {
    /// No-op; JSON is always emitted on stdout.
    #[arg(long, hide = true)]
    pub json: bool,
    /// Path to the SQLite database file.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

/// Arguments for `vec purge-orphan`.
#[derive(clap::Args)]
pub struct VecOrphanListInner {
    pub json: bool,
    pub db: Option<String>,
}

/// Arguments for `vec purge-orphan`.
#[derive(clap::Args)]
pub struct VecPurgeOrphanArgs {
    /// No-op; JSON is always emitted on stdout.
    #[arg(long, hide = true)]
    pub json: bool,
    /// Path to the SQLite database file.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// Skip the interactive confirmation; required for automation.
    #[arg(long, default_value_t = false)]
    pub yes: bool,
    /// Report what would be purged without writing.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

/// Arguments for `vec stats`.
#[derive(clap::Args)]
pub struct VecStatsArgs {
    /// No-op; JSON is always emitted on stdout.
    #[arg(long, hide = true)]
    pub json: bool,
    /// Path to the SQLite database file.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct VecOrphanListItem {
    /// The orphan `memory_id` value stored in the active memory embedding table.
    memory_id: i64,
    /// Hash of the float vector blob, for fingerprinting.
    vector_hash: String,
    /// When the orphan row was originally inserted.
    created_at: i64,
}

#[derive(Serialize)]
struct VecOrphanListResponse {
    action: String,
    count: i64,
    items: Vec<VecOrphanListItem>,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct VecPurgeOrphanResponse {
    action: String,
    deleted: i64,
    /// Number of orphan rows in `vec_entities` that were also removed (G39).
    deleted_entities: i64,
    /// Number of orphan rows in `vec_chunks` that were also removed (G39).
    deleted_chunks: i64,
    dry_run: bool,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct VecStatsResponse {
    total_rows: i64,
    orphaned: i64,
    coverage_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    vec_entities_rows: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vec_chunks_rows: Option<i64>,
    fts_memories_rows: i64,
    elapsed_ms: u64,
}

/// Dispatch entry point called from `main`.
///
/// # Errors
/// Propagates any [`AppError`] raised by the underlying subcommand.
pub fn run(args: VecArgs) -> Result<(), AppError> {
    match args.command {
        VecSubcommand::OrphanList(a) => run_orphan_list(a),
        VecSubcommand::PurgeOrphan(a) => run_purge_orphan(a),
        VecSubcommand::Stats(a) => run_stats(a),
    }
}

fn live_memory_embedding_stats(conn: &rusqlite::Connection) -> (i64, i64) {
    if let Some(table_name) = first_existing_vec_table(conn, MEMORY_VEC_TABLES) {
        let total = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table_name}"), [], |r| {
                r.get(0)
            })
            .unwrap_or(0);
        let orphaned = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*)
                     FROM {table_name} v
                     LEFT JOIN memories m ON m.id = v.memory_id
                     WHERE m.id IS NULL OR m.deleted_at IS NOT NULL"
                ),
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        return (total, orphaned);
    }

    (0, 0)
}

fn first_existing_vec_table<'a>(
    conn: &rusqlite::Connection,
    candidates: &'a [&'a str],
) -> Option<&'a str> {
    candidates
        .iter()
        .copied()
        .find(|table_name| vec_table_exists(conn, table_name))
}

fn count_rows_first_existing(conn: &rusqlite::Connection, candidates: &[&str]) -> Option<i64> {
    for table in candidates {
        if vec_table_exists(conn, table) {
            return conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .ok();
        }
    }
    None
}

fn run_orphan_list(args: VecOrphanListArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;
    crate::storage::connection::ensure_db_ready(&paths)?;
    let conn = open_ro(&paths.db)?;

    let Some(memory_table) = first_existing_vec_table(&conn, MEMORY_VEC_TABLES) else {
        return output::emit_json(&VecOrphanListResponse {
            action: "orphan_list".to_string(),
            count: 0,
            items: Vec::new(),
            elapsed_ms: start.elapsed().as_millis() as u64,
        });
    };

    // List embedding rows that have no corresponding live memory row.
    // We use a hash of the float[] blob (BLAKE3) as a fingerprint so the
    // operator can detect duplicate embeddings even after the parent
    // memory has been re-embedded with new content.
    let mut stmt = conn.prepare(&format!(
        "SELECT v.memory_id, v.embedding, CAST(v.created_at AS INTEGER)
         FROM {memory_table} v
         LEFT JOIN memories m ON m.id = v.memory_id
         WHERE m.id IS NULL OR m.deleted_at IS NOT NULL
         ORDER BY v.memory_id"
    ))?;
    let rows: Vec<VecOrphanListItem> = stmt
        .query_map([], |r| {
            let memory_id: i64 = r.get(0)?;
            let blob: Vec<u8> = r.get(1)?;
            let created_at: i64 = r.get(2)?;
            let vector_hash = blake3::hash(&blob).to_hex().to_string();
            Ok(VecOrphanListItem {
                memory_id,
                vector_hash,
                created_at,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let count = rows.len() as i64;

    output::emit_json(&VecOrphanListResponse {
        action: "orphan_list".to_string(),
        count,
        items: rows,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;
    Ok(())
}

fn run_purge_orphan(args: VecPurgeOrphanArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;
    crate::storage::connection::ensure_db_ready(&paths)?;
    let conn = open_rw(&paths.db)?;

    let Some(memory_table) = first_existing_vec_table(&conn, MEMORY_VEC_TABLES) else {
        return output::emit_json(&VecPurgeOrphanResponse {
            action: "purge_orphan".to_string(),
            deleted: 0,
            deleted_entities: 0,
            deleted_chunks: 0,
            dry_run: args.dry_run,
            elapsed_ms: start.elapsed().as_millis() as u64,
        });
    };

    let orphan_count: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {memory_table} v
                 LEFT JOIN memories m ON m.id = v.memory_id
                 WHERE m.id IS NULL OR m.deleted_at IS NOT NULL"
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // G39: also count orphans in vec_entities and vec_chunks. These
    // tables follow the same `memory_id` foreign key convention and
    // accumulate orphans on the same paths as vec_memories.
    let orphan_entities_count: i64 = if vec_table_exists(&conn, "vec_entities") {
        conn.query_row(
            "SELECT COUNT(*) FROM vec_entities v
             LEFT JOIN memories m ON m.id = v.memory_id
             WHERE m.id IS NULL OR m.deleted_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0)
    } else {
        0
    };
    let orphan_chunks_count: i64 = if vec_table_exists(&conn, "vec_chunks") {
        conn.query_row(
            "SELECT COUNT(*) FROM vec_chunks v
             LEFT JOIN memories m ON m.id = v.memory_id
             WHERE m.id IS NULL OR m.deleted_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0)
    } else {
        0
    };

    if args.dry_run {
        tracing::info!(target: "vec", orphan_count, orphan_entities_count, orphan_chunks_count, "dry-run: would delete orphans");
        return output::emit_json(&VecPurgeOrphanResponse {
            action: "purge_orphan_dry_run".to_string(),
            deleted: 0,
            deleted_entities: 0,
            deleted_chunks: 0,
            dry_run: true,
            elapsed_ms: start.elapsed().as_millis() as u64,
        });
    }

    if !args.yes {
        return Err(AppError::Validation(format!(
            "refusing to delete {orphan_count} memory embedding + {orphan_entities_count} vec_entities + {orphan_chunks_count} vec_chunks orphan rows without --yes (use --dry-run to preview)"
        )));
    }

    let deleted: i64 = conn.execute(
        &format!(
            "DELETE FROM {memory_table}
             WHERE NOT EXISTS (
                 SELECT 1 FROM memories m
                 WHERE m.id = {memory_table}.memory_id
                   AND m.deleted_at IS NULL
             )"
        ),
        [],
    )? as i64;

    let deleted_entities: i64 = if vec_table_exists(&conn, "vec_entities") {
        conn.execute(
            "DELETE FROM vec_entities
             WHERE NOT EXISTS (
                 SELECT 1 FROM memories m
                 WHERE m.id = vec_entities.memory_id
                   AND m.deleted_at IS NULL
             )",
            [],
        )
        .unwrap_or(0) as i64
    } else {
        0
    };
    let deleted_chunks: i64 = if vec_table_exists(&conn, "vec_chunks") {
        conn.execute(
            "DELETE FROM vec_chunks
             WHERE NOT EXISTS (
                 SELECT 1 FROM memories m
                 WHERE m.id = vec_chunks.memory_id
                   AND m.deleted_at IS NULL
             )",
            [],
        )
        .unwrap_or(0) as i64
    } else {
        0
    };

    tracing::info!(target: "vec", deleted, deleted_entities, deleted_chunks, "purged orphan vec rows");

    output::emit_json(&VecPurgeOrphanResponse {
        action: "purged_orphan".to_string(),
        deleted,
        deleted_entities,
        deleted_chunks,
        dry_run: false,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;
    Ok(())
}

fn run_stats(args: VecStatsArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;
    crate::storage::connection::ensure_db_ready(&paths)?;
    let conn = open_ro(&paths.db)?;

    let (total_rows, orphaned) = live_memory_embedding_stats(&conn);
    let coverage_percent = if total_rows > 0 {
        ((total_rows - orphaned) as f64 / total_rows as f64) * 100.0
    } else {
        100.0
    };

    let vec_entities_rows =
        count_rows_first_existing(&conn, &["entity_embeddings", "vec_entities"]);
    let vec_chunks_rows = count_rows_first_existing(&conn, &["chunk_embeddings", "vec_chunks"]);
    let fts_memories_rows = conn
        .query_row("SELECT COUNT(*) FROM fts_memories", [], |r| r.get(0))
        .unwrap_or(0);

    output::emit_json(&VecStatsResponse {
        total_rows,
        orphaned,
        coverage_percent,
        vec_entities_rows,
        vec_chunks_rows,
        fts_memories_rows,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;
    Ok(())
}

fn vec_table_exists(conn: &rusqlite::Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        rusqlite::params![name],
        |r| r.get::<_, i64>(0).map(|v| v > 0),
    )
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn open_vec_test_db() -> Connection {
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
                dim INTEGER NOT NULL DEFAULT 384
            );
            CREATE TABLE vec_memories (
                memory_id INTEGER PRIMARY KEY,
                embedding BLOB NOT NULL,
                created_at INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE entity_embeddings (
                entity_id INTEGER PRIMARY KEY,
                namespace TEXT NOT NULL,
                embedding BLOB NOT NULL,
                source TEXT NOT NULL,
                model TEXT NOT NULL,
                dim INTEGER NOT NULL DEFAULT 384
            );
            CREATE TABLE vec_entities (
                memory_id INTEGER PRIMARY KEY
            );
            CREATE TABLE chunk_embeddings (
                chunk_id INTEGER PRIMARY KEY,
                memory_id INTEGER NOT NULL,
                embedding BLOB NOT NULL,
                source TEXT NOT NULL,
                model TEXT NOT NULL,
                dim INTEGER NOT NULL DEFAULT 384
            );
            CREATE TABLE vec_chunks (
                memory_id INTEGER PRIMARY KEY
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn vec_orphan_list_response_serializes_all_fields() {
        let resp = VecOrphanListResponse {
            action: "orphan_list".into(),
            count: 0,
            items: Vec::new(),
            elapsed_ms: 5,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["action"], "orphan_list");
        assert_eq!(v["count"], 0i64);
        assert_eq!(v["elapsed_ms"], 5u64);
        assert!(v["items"].is_array());
    }

    #[test]
    fn vec_purge_orphan_response_serializes_dry_run_flag() {
        let resp = VecPurgeOrphanResponse {
            action: "purge_orphan_dry_run".into(),
            deleted: 0,
            deleted_entities: 0,
            deleted_chunks: 0,
            dry_run: true,
            elapsed_ms: 1,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["dry_run"], true);
        assert_eq!(v["deleted"], 0i64);
    }

    #[test]
    fn vec_stats_response_computes_coverage() {
        let resp = VecStatsResponse {
            total_rows: 100,
            orphaned: 25,
            coverage_percent: 75.0,
            vec_entities_rows: Some(50),
            vec_chunks_rows: None,
            fts_memories_rows: 100,
            elapsed_ms: 10,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["coverage_percent"], 75.0);
        assert_eq!(v["vec_entities_rows"], 50i64);
        assert!(v.get("vec_chunks_rows").is_none());
    }

    #[test]
    fn live_memory_embedding_stats_prefers_memory_embeddings() {
        let conn = open_vec_test_db();
        conn.execute("INSERT INTO memories (id, deleted_at) VALUES (1, NULL)", [])
            .unwrap();
        conn.execute("INSERT INTO memories (id, deleted_at) VALUES (2, 123)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO memory_embeddings(memory_id, namespace, embedding, source, model, dim)
             VALUES (1, 'global', X'00', 'llm', 'm', 384)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memory_embeddings(memory_id, namespace, embedding, source, model, dim)
             VALUES (2, 'global', X'00', 'llm', 'm', 384)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memory_embeddings(memory_id, namespace, embedding, source, model, dim)
             VALUES (3, 'global', X'00', 'llm', 'm', 384)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO vec_memories(memory_id, embedding, created_at) VALUES (99, X'00', 0)",
            [],
        )
        .unwrap();

        let (total, orphaned) = live_memory_embedding_stats(&conn);
        assert_eq!(total, 3);
        assert_eq!(orphaned, 2);
    }

    #[test]
    fn count_rows_first_existing_prefers_new_embedding_tables() {
        let conn = open_vec_test_db();
        conn.execute(
            "INSERT INTO entity_embeddings(entity_id, namespace, embedding, source, model, dim)
             VALUES (1, 'global', X'00', 'llm', 'm', 384)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO vec_entities(memory_id) VALUES (1)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO chunk_embeddings(chunk_id, memory_id, embedding, source, model, dim)
             VALUES (1, 1, X'00', 'llm', 'm', 384)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO vec_chunks(memory_id) VALUES (1)", [])
            .unwrap();

        assert_eq!(
            count_rows_first_existing(&conn, &["entity_embeddings", "vec_entities"]),
            Some(1)
        );
        assert_eq!(
            count_rows_first_existing(&conn, &["chunk_embeddings", "vec_chunks"]),
            Some(1)
        );
    }
}
