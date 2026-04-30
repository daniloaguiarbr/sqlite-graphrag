//! Handler for the `stats` CLI subcommand.

use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Show database statistics (memory counts, sizes, namespace breakdown)\n  \
    sqlite-graphrag stats\n\n  \
    # Stats for a database at a custom path\n  \
    sqlite-graphrag stats --db /path/to/graphrag.sqlite\n\n  \
    # Use SQLITE_GRAPHRAG_DB_PATH env var\n  \
    SQLITE_GRAPHRAG_DB_PATH=/data/graphrag.sqlite sqlite-graphrag stats")]
pub struct StatsArgs {
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
struct StatsResponse {
    memories: i64,
    /// Alias de `memories` para contrato documentado em SKILL.md e AGENT_PROTOCOL.md.
    memories_total: i64,
    entities: i64,
    /// Alias de `entities` para contrato documentado.
    entities_total: i64,
    relationships: i64,
    /// Alias de `relationships` para contrato documentado.
    relationships_total: i64,
    /// Semantic alias of `relationships` per the contract in AGENT_PROTOCOL.md.
    edges: i64,
    /// Total indexed chunks (one row per chunk in `memory_chunks`).
    chunks_total: i64,
    /// Average length of the body field in active (non-deleted) memories.
    avg_body_len: f64,
    namespaces: Vec<String>,
    db_size_bytes: u64,
    /// Semantic alias of `db_size_bytes` for the documented contract.
    db_bytes: u64,
    schema_version: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: StatsArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let _ = args.json; // --json is a no-op because output is already JSON by default
    let _ = args.format; // --format is a no-op; JSON is always emitted on stdout
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let conn = open_ro(&paths.db)?;

    let memories: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    let entities: i64 = conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))?;
    let relationships: i64 =
        conn.query_row("SELECT COUNT(*) FROM relationships", [], |r| r.get(0))?;

    let mut stmt = conn.prepare(
        "SELECT DISTINCT namespace FROM memories WHERE deleted_at IS NULL ORDER BY namespace",
    )?;
    let namespaces: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let schema_version: String = conn
        .query_row(
            "SELECT MAX(version) FROM refinery_schema_history",
            [],
            |row| row.get::<_, Option<i64>>(0),
        )
        .ok()
        .flatten()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let db_size_bytes = std::fs::metadata(&paths.db).map(|m| m.len()).unwrap_or(0);

    // v1.0.21 P1-C: query uses the (correct) `memory_chunks` table.
    // If the table does not exist (legacy pre-chunking DB), the error is "no such table"
    // and the fallback returns 0. Other errors are logged via tracing for audit.
    let chunks_total: i64 = match conn.query_row("SELECT COUNT(*) FROM memory_chunks", [], |r| {
        r.get::<_, i64>(0)
    }) {
        Ok(n) => n,
        Err(rusqlite::Error::SqliteFailure(_, Some(msg))) if msg.contains("no such table") => 0,
        Err(e) => {
            tracing::warn!("failed to count memory_chunks: {e}");
            0
        }
    };

    let avg_body_len: f64 = conn
        .query_row(
            "SELECT COALESCE(AVG(LENGTH(body)), 0.0) FROM memories WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    output::emit_json(&StatsResponse {
        memories,
        memories_total: memories,
        entities,
        entities_total: entities,
        relationships,
        relationships_total: relationships,
        edges: relationships,
        chunks_total,
        avg_body_len,
        namespaces,
        db_size_bytes,
        db_bytes: db_size_bytes,
        schema_version,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_response_serializes_all_fields() {
        let resp = StatsResponse {
            memories: 10,
            memories_total: 10,
            entities: 5,
            entities_total: 5,
            relationships: 3,
            relationships_total: 3,
            edges: 3,
            chunks_total: 20,
            avg_body_len: 42.5,
            namespaces: vec!["global".to_string(), "project".to_string()],
            db_size_bytes: 8192,
            db_bytes: 8192,
            schema_version: "6".to_string(),
            elapsed_ms: 7,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["memories"], 10);
        assert_eq!(json["memories_total"], 10);
        assert_eq!(json["entities"], 5);
        assert_eq!(json["entities_total"], 5);
        assert_eq!(json["relationships"], 3);
        assert_eq!(json["relationships_total"], 3);
        assert_eq!(json["edges"], 3);
        assert_eq!(json["chunks_total"], 20);
        assert_eq!(json["db_size_bytes"], 8192u64);
        assert_eq!(json["db_bytes"], 8192u64);
        assert_eq!(json["schema_version"], "6");
        assert_eq!(json["elapsed_ms"], 7u64);
    }

    #[test]
    fn stats_response_namespaces_is_string_array() {
        let resp = StatsResponse {
            memories: 0,
            memories_total: 0,
            entities: 0,
            entities_total: 0,
            relationships: 0,
            relationships_total: 0,
            edges: 0,
            chunks_total: 0,
            avg_body_len: 0.0,
            namespaces: vec!["ns1".to_string(), "ns2".to_string(), "ns3".to_string()],
            db_size_bytes: 0,
            db_bytes: 0,
            schema_version: "unknown".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        let arr = json["namespaces"]
            .as_array()
            .expect("namespaces must be array");
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0], "ns1");
        assert_eq!(arr[1], "ns2");
        assert_eq!(arr[2], "ns3");
    }

    #[test]
    fn stats_response_namespaces_empty_serializes_empty_array() {
        let resp = StatsResponse {
            memories: 0,
            memories_total: 0,
            entities: 0,
            entities_total: 0,
            relationships: 0,
            relationships_total: 0,
            edges: 0,
            chunks_total: 0,
            avg_body_len: 0.0,
            namespaces: vec![],
            db_size_bytes: 0,
            db_bytes: 0,
            schema_version: "unknown".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        let arr = json["namespaces"]
            .as_array()
            .expect("namespaces must be array");
        assert!(arr.is_empty(), "empty namespaces must serialize as []");
    }

    #[test]
    fn stats_response_aliases_memories_total_and_memories_equal() {
        let resp = StatsResponse {
            memories: 42,
            memories_total: 42,
            entities: 7,
            entities_total: 7,
            relationships: 2,
            relationships_total: 2,
            edges: 2,
            chunks_total: 0,
            avg_body_len: 0.0,
            namespaces: vec![],
            db_size_bytes: 0,
            db_bytes: 0,
            schema_version: "6".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["memories"], json["memories_total"]);
        assert_eq!(json["entities"], json["entities_total"]);
        assert_eq!(json["relationships"], json["relationships_total"]);
        assert_eq!(json["relationships"], json["edges"]);
        assert_eq!(json["db_size_bytes"], json["db_bytes"]);
    }
}
