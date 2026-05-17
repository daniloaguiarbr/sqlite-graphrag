//! Handler for the `export` CLI subcommand.

use crate::cli::MemoryType;
use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Export all memories as NDJSON\n  \
    sqlite-graphrag export\n\n  \
    # Export only decision memories from a namespace\n  \
    sqlite-graphrag export --type decision --namespace my-project\n\n  \
    # Export including soft-deleted memories\n  \
    sqlite-graphrag export --include-deleted\n\n  \
    # Pipe to file for backup\n  \
    sqlite-graphrag export > backup.ndjson")]
pub struct ExportArgs {
    /// Namespace (env: SQLITE_GRAPHRAG_NAMESPACE, default: global).
    #[arg(
        long,
        help = "Namespace (env: SQLITE_GRAPHRAG_NAMESPACE, default: global)"
    )]
    pub namespace: Option<String>,
    /// Filter by memory type.
    #[arg(long, value_enum)]
    pub r#type: Option<MemoryType>,
    /// Include soft-deleted memories in the export.
    #[arg(long, default_value_t = false)]
    pub include_deleted: bool,
    /// Maximum number of memories to export (default: 100000).
    #[arg(long, default_value_t = 100_000)]
    pub limit: usize,
    /// Offset for pagination.
    #[arg(long, default_value_t = 0)]
    pub offset: usize,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    /// Path to graphrag.sqlite (overrides SQLITE_GRAPHRAG_DB_PATH and default CWD).
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct ExportMemoryLine {
    name: String,
    r#type: String,
    memory_type: String,
    description: String,
    body: String,
    namespace: String,
    created_at_iso: String,
    updated_at_iso: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    deleted_at_iso: Option<String>,
}

#[derive(Serialize)]
struct ExportSummary {
    summary: bool,
    exported: usize,
    namespace: String,
    elapsed_ms: u64,
}

/// Exports memories as NDJSON (one JSON line per memory, followed by a summary line).
pub fn run(args: ExportArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    crate::storage::connection::ensure_db_ready(&paths)?;
    let conn = open_ro(&paths.db)?;

    let deleted_filter = if args.include_deleted {
        ""
    } else {
        "AND m.deleted_at IS NULL"
    };

    let limit_i64 = args.limit as i64;
    let offset_i64 = args.offset as i64;
    let type_str: Option<String> = args.r#type.map(|t| t.as_str().to_string());

    let rows = fetch_rows(
        &conn,
        &namespace,
        &type_str,
        deleted_filter,
        limit_i64,
        offset_i64,
    )?;

    let exported = rows.len();
    for line in &rows {
        output::emit_json_compact(line)?;
    }

    output::emit_json_compact(&ExportSummary {
        summary: true,
        exported,
        namespace: namespace.clone(),
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

fn fetch_rows(
    conn: &rusqlite::Connection,
    namespace: &str,
    type_str: &Option<String>,
    deleted_filter: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<ExportMemoryLine>, AppError> {
    let rows = if let Some(t) = type_str {
        let sql = format!(
            "SELECT m.name, m.type, m.description, m.body, m.namespace, \
                    m.created_at, m.updated_at, m.deleted_at \
             FROM memories m \
             WHERE m.namespace = ?1 {deleted_filter} AND m.type = ?2 \
             ORDER BY m.name \
             LIMIT ?3 OFFSET ?4"
        );
        let mut stmt = conn.prepare(&sql)?;
        let result = stmt
            .query_map(rusqlite::params![namespace, t, limit, offset], map_row)?
            .collect::<Result<Vec<_>, _>>()?;
        result
    } else {
        let sql = format!(
            "SELECT m.name, m.type, m.description, m.body, m.namespace, \
                    m.created_at, m.updated_at, m.deleted_at \
             FROM memories m \
             WHERE m.namespace = ?1 {deleted_filter} \
             ORDER BY m.name \
             LIMIT ?2 OFFSET ?3"
        );
        let mut stmt = conn.prepare(&sql)?;
        let result = stmt
            .query_map(rusqlite::params![namespace, limit, offset], map_row)?
            .collect::<Result<Vec<_>, _>>()?;
        result
    };
    Ok(rows)
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExportMemoryLine> {
    let memory_type_val: String = row.get(1)?;
    Ok(ExportMemoryLine {
        name: row.get(0)?,
        r#type: memory_type_val.clone(),
        memory_type: memory_type_val,
        description: row.get(2)?,
        body: row.get(3)?,
        namespace: row.get(4)?,
        created_at_iso: crate::tz::epoch_to_iso(row.get::<_, i64>(5)?),
        updated_at_iso: crate::tz::epoch_to_iso(row.get::<_, i64>(6)?),
        deleted_at_iso: row.get::<_, Option<i64>>(7)?.map(crate::tz::epoch_to_iso),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_line_emits_both_type_and_memory_type() {
        let line = ExportMemoryLine {
            name: "test".to_string(),
            r#type: "document".to_string(),
            memory_type: "document".to_string(),
            description: "desc".to_string(),
            body: "body".to_string(),
            namespace: "global".to_string(),
            created_at_iso: "2025-01-01T00:00:00Z".to_string(),
            updated_at_iso: "2025-01-01T00:00:00Z".to_string(),
            deleted_at_iso: None,
        };
        let json = serde_json::to_value(&line).unwrap();
        assert_eq!(json["type"], "document");
        assert_eq!(json["memory_type"], "document");
    }
}
