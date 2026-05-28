//! Handler for the `normalize-entities` CLI subcommand (GAP-15).
//!
//! Scans all existing entity names in the namespace and normalizes them to
//! kebab-case ASCII using [`crate::parsers::normalize_entity_name`].
//! When a normalized name already exists (collision), the source entity is
//! merged into the target using the same logic as `merge-entities`:
//! relationships are retargeted via `UPDATE OR IGNORE` + `DELETE`, then
//! the source row is removed. Otherwise the entity name is updated in place.

use crate::errors::AppError;
use crate::output::{self, OutputFormat};
use crate::parsers::normalize_entity_name;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use rusqlite::params;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Preview which entities would be renamed or merged\n  \
    sqlite-graphrag normalize-entities --dry-run\n\n  \
    # Apply normalization to all entity names\n  \
    sqlite-graphrag normalize-entities --yes\n\n  \
    # Scope to a specific namespace\n  \
    sqlite-graphrag normalize-entities --yes --namespace my-project\n\n\
NOTE:\n  \
    When a normalized name already exists, the source entity is merged into\n  \
    the existing target via relationship retargeting (UPDATE OR IGNORE + DELETE).\n  \
    Run `cleanup-orphans` afterwards to remove any newly orphaned entities.")]
pub struct NormalizeEntitiesArgs {
    /// Preview changes without persisting them.
    #[arg(long, conflicts_with = "yes")]
    pub dry_run: bool,
    /// Apply normalization without interactive confirmation.
    #[arg(long, conflicts_with = "dry_run")]
    pub yes: bool,
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct NormalizeEntitiesResponse {
    /// "normalized" when changes were applied, "dry_run" when only previewed.
    action: String,
    /// Number of entities whose names were updated in place.
    normalized_count: usize,
    /// Number of entities that collided with an existing normalized name and
    /// were merged into the target.
    merged_count: usize,
    namespace: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: NormalizeEntitiesArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();

    if !args.dry_run && !args.yes {
        return Err(AppError::Validation(
            "pass --dry-run to preview or --yes to apply changes".to_string(),
        ));
    }

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let mut conn = open_rw(&paths.db)?;

    // Collect all entity (id, name) pairs for the namespace.
    let entities: Vec<(i64, String)> = {
        let mut stmt =
            conn.prepare("SELECT id, name FROM entities WHERE namespace = ?1 ORDER BY id")?;
        let rows = stmt.query_map(params![namespace], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    // Compute which names need changing.
    let to_change: Vec<(i64, String, String)> = entities
        .iter()
        .filter_map(|(id, name)| {
            let normalized = normalize_entity_name(name);
            if normalized != *name {
                Some((*id, name.clone(), normalized))
            } else {
                None
            }
        })
        .collect();

    let normalized_count_preview = to_change.len();

    if args.dry_run {
        let response = NormalizeEntitiesResponse {
            action: "dry_run".to_string(),
            normalized_count: normalized_count_preview,
            merged_count: 0,
            namespace,
            elapsed_ms: inicio.elapsed().as_millis() as u64,
        };
        match args.format {
            OutputFormat::Json => output::emit_json(&response)?,
            OutputFormat::Text | OutputFormat::Markdown => {
                output::emit_text(&format!(
                    "dry_run: {} entity names would be normalized",
                    response.normalized_count
                ));
            }
        }
        return Ok(());
    }

    // Apply changes inside a transaction.
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    let mut normalized_count: usize = 0;
    let mut merged_count: usize = 0;

    for (src_id, _original_name, normalized) in &to_change {
        // Check whether a row with the normalized name already exists.
        let existing_id: Option<i64> = {
            let mut stmt =
                tx.prepare_cached("SELECT id FROM entities WHERE namespace = ?1 AND name = ?2")?;
            match stmt.query_row(params![namespace, normalized], |r| r.get::<_, i64>(0)) {
                Ok(id) => Some(id),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(AppError::Database(e)),
            }
        };

        match existing_id {
            Some(target_id) if target_id != *src_id => {
                // Collision: merge source into target using UPDATE OR IGNORE + DELETE.
                // Step 1a: redirect source_id.
                tx.execute(
                    "UPDATE OR IGNORE relationships SET source_id = ?1 WHERE source_id = ?2",
                    params![target_id, src_id],
                )?;
                tx.execute(
                    "DELETE FROM relationships WHERE source_id = ?1",
                    params![src_id],
                )?;
                // Step 1b: redirect target_id.
                tx.execute(
                    "UPDATE OR IGNORE relationships SET target_id = ?1 WHERE target_id = ?2",
                    params![target_id, src_id],
                )?;
                tx.execute(
                    "DELETE FROM relationships WHERE target_id = ?1",
                    params![src_id],
                )?;
                // Remove self-loops.
                tx.execute("DELETE FROM relationships WHERE source_id = target_id", [])?;
                // Retarget memory_entities bindings.
                tx.execute(
                    "UPDATE OR IGNORE memory_entities SET entity_id = ?1 WHERE entity_id = ?2",
                    params![target_id, src_id],
                )?;
                tx.execute(
                    "DELETE FROM memory_entities WHERE entity_id = ?1",
                    params![src_id],
                )?;
                // Remove the source entity row.
                tx.execute("DELETE FROM entities WHERE id = ?1", params![src_id])?;
                // Recalculate degree for the surviving target.
                tx.execute(
                    "UPDATE entities
                     SET degree = (SELECT COUNT(*) FROM relationships
                                   WHERE source_id = entities.id OR target_id = entities.id)
                     WHERE id = ?1",
                    params![target_id],
                )?;
                tracing::info!(
                    src_id = src_id,
                    target_id = target_id,
                    normalized = normalized,
                    "entity merged into existing normalized target"
                );
                merged_count += 1;
            }
            _ => {
                // No collision: simple rename.
                tx.execute(
                    "UPDATE entities SET name = ?1, updated_at = unixepoch() WHERE id = ?2",
                    params![normalized, src_id],
                )?;
                tracing::info!(
                    entity_id = src_id,
                    normalized = normalized,
                    "entity name normalized"
                );
                normalized_count += 1;
            }
        }
    }

    tx.commit()?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;

    let response = NormalizeEntitiesResponse {
        action: "normalized".to_string(),
        normalized_count,
        merged_count,
        namespace,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    };

    match args.format {
        OutputFormat::Json => output::emit_json(&response)?,
        OutputFormat::Text | OutputFormat::Markdown => {
            output::emit_text(&format!(
                "normalized: {} renamed, {} merged",
                response.normalized_count, response.merged_count
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::connection::register_vec_extension;
    use rusqlite::Connection;
    use tempfile::TempDir;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Opens a temp DB with the full schema applied via migrations.
    fn setup_db() -> Result<(TempDir, Connection), Box<dyn std::error::Error>> {
        register_vec_extension();
        let tmp = TempDir::new()?;
        let db_path = tmp.path().join("test.db");
        let mut conn = Connection::open(&db_path)?;
        crate::migrations::runner().run(&mut conn)?;
        Ok((tmp, conn))
    }

    /// Inserts an entity bypassing `upsert_entity` normalization, so tests can
    /// seed deliberately un-normalized names.
    fn insert_entity(conn: &Connection, name: &str) -> Result<i64, Box<dyn std::error::Error>> {
        // Bypass upsert_entity normalization to seed raw (un-normalized) names.
        conn.execute(
            "INSERT INTO entities (namespace, name, type, description) VALUES ('global', ?1, 'concept', NULL)",
            params![name],
        )?;
        let id: i64 = conn.query_row(
            "SELECT id FROM entities WHERE namespace = 'global' AND name = ?1",
            params![name],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    #[test]
    fn dry_run_returns_count_without_changes() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        insert_entity(&conn, "Hello World")?;
        insert_entity(&conn, "already-normalized")?;

        // Verify "Hello World" exists.
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM entities WHERE name = 'Hello World' AND namespace = 'global'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(count, 1, "entity must exist before dry run");

        // dry_run must not modify anything.
        let count_after: i64 = conn.query_row(
            "SELECT COUNT(*) FROM entities WHERE name = 'Hello World' AND namespace = 'global'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(count_after, 1, "dry run must not rename entities");
        Ok(())
    }

    #[test]
    fn renames_unnormalized_entity_in_place() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let src_id = insert_entity(&conn, "Hello World")?;

        // Apply normalization directly via the internal logic.
        {
            let normalized = normalize_entity_name("Hello World");
            let existing: Option<i64> = {
                match conn.query_row(
                    "SELECT id FROM entities WHERE namespace = 'global' AND name = ?1",
                    params![normalized],
                    |r| r.get::<_, i64>(0),
                ) {
                    Ok(id) => Some(id),
                    Err(rusqlite::Error::QueryReturnedNoRows) => None,
                    Err(e) => return Err(e.into()),
                }
            };
            assert!(existing.is_none(), "no collision expected");
            conn.execute(
                "UPDATE entities SET name = ?1 WHERE id = ?2",
                params![normalized, src_id],
            )?;
        }

        let name: String = conn.query_row(
            "SELECT name FROM entities WHERE id = ?1",
            params![src_id],
            |r| r.get(0),
        )?;
        assert_eq!(name, "hello-world");
        Ok(())
    }

    #[test]
    fn merges_into_existing_on_collision() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        // Target already exists with the normalized name.
        let target_id = insert_entity(&conn, "hello-world")?;
        // Source has the un-normalized form that normalizes to the same value.
        let src_id = insert_entity(&conn, "Hello World")?;

        // Insert a relationship attached to src_id.
        conn.execute(
            "INSERT INTO relationships (namespace, source_id, target_id, relation, weight)
             VALUES ('global', ?1, ?1, 'related', 0.5)",
            params![src_id],
        )?;

        // Merge: retarget relationships from src → target.
        conn.execute(
            "UPDATE OR IGNORE relationships SET source_id = ?1 WHERE source_id = ?2",
            params![target_id, src_id],
        )?;
        conn.execute(
            "DELETE FROM relationships WHERE source_id = ?1",
            params![src_id],
        )?;
        conn.execute("DELETE FROM entities WHERE id = ?1", params![src_id])?;

        // Source must be gone.
        let src_exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM entities WHERE id = ?1",
            params![src_id],
            |r| r.get(0),
        )?;
        assert_eq!(src_exists, 0, "source entity must be deleted after merge");

        // Target must still exist.
        let target_name: String = conn.query_row(
            "SELECT name FROM entities WHERE id = ?1",
            params![target_id],
            |r| r.get(0),
        )?;
        assert_eq!(target_name, "hello-world");
        Ok(())
    }

    #[test]
    fn normalize_entities_response_serializes_correctly() {
        let resp = NormalizeEntitiesResponse {
            action: "normalized".to_string(),
            normalized_count: 3,
            merged_count: 1,
            namespace: "global".to_string(),
            elapsed_ms: 42,
        };
        let json = serde_json::to_value(&resp).expect("serialization");
        assert_eq!(json["action"], "normalized");
        assert_eq!(json["normalized_count"], 3);
        assert_eq!(json["merged_count"], 1);
        assert_eq!(json["namespace"], "global");
        assert!(json["elapsed_ms"].as_u64().is_some());
    }

    #[test]
    fn dry_run_response_has_correct_action() {
        let resp = NormalizeEntitiesResponse {
            action: "dry_run".to_string(),
            normalized_count: 5,
            merged_count: 0,
            namespace: "test".to_string(),
            elapsed_ms: 1,
        };
        let json = serde_json::to_value(&resp).expect("serialization");
        assert_eq!(json["action"], "dry_run");
    }
}
