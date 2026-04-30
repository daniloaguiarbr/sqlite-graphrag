//! Handler for the `forget` CLI subcommand.

use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::memories;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Soft-delete a memory by name (positional form)\n  \
    sqlite-graphrag forget onboarding\n\n  \
    # Soft-delete using the named flag form\n  \
    sqlite-graphrag forget --name onboarding\n\n  \
    # Soft-delete from a specific namespace\n  \
    sqlite-graphrag forget onboarding --namespace my-project")]
pub struct ForgetArgs {
    /// Memory name as a positional argument. Alternative to `--name`.
    #[arg(
        value_name = "NAME",
        conflicts_with = "name",
        help = "Memory name to soft-delete; alternative to --name"
    )]
    pub name_positional: Option<String>,
    /// Memory name to soft-delete. The row is preserved with `deleted_at` set, recoverable via `restore`.
    /// Use `purge` to permanently remove soft-deleted memories.
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long, default_value = "global")]
    pub namespace: Option<String>,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct ForgetResponse {
    /// Outcome of the forget operation: `soft_deleted`, `already_deleted`, or `not_found`.
    action: String,
    /// True only when this invocation actively transitioned the memory from live to soft-deleted.
    forgotten: bool,
    name: String,
    namespace: String,
    /// Unix epoch seconds when the memory was soft-deleted; `None` when `action="not_found"`.
    deleted_at: Option<i64>,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: ForgetArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    // Resolve name from positional or --name flag; both are optional, at least one is required.
    let name = args.name_positional.or(args.name).ok_or_else(|| {
        AppError::Validation("name required: pass as positional argument or via --name".to_string())
    })?;
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    crate::storage::connection::ensure_db_ready(&paths)?;

    let conn = open_rw(&paths.db)?;

    // Probe state without filtering on `deleted_at` so we can distinguish
    // `not_found` (no row) from `already_deleted` (row with deleted_at set)
    // from the live case (deleted_at IS NULL) handled by `soft_delete`.
    let probe: Option<(i64, Option<i64>)> = conn
        .query_row(
            "SELECT id, deleted_at FROM memories WHERE namespace = ?1 AND name = ?2",
            params![namespace, name],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)),
        )
        .optional()?;

    let (action, forgotten, deleted_at, memory_id) = match probe {
        None => ("not_found", false, None, None),
        Some((id, Some(existing))) => ("already_deleted", false, Some(existing), Some(id)),
        Some((id, None)) => {
            let ok = memories::soft_delete(&conn, &namespace, &name)?;
            if !ok {
                // Race: row was concurrently soft-deleted between probe and update.
                // Re-read to get the current `deleted_at`.
                let current: Option<i64> = conn
                    .query_row(
                        "SELECT deleted_at FROM memories WHERE id = ?1",
                        params![id],
                        |r| r.get::<_, Option<i64>>(0),
                    )
                    .optional()?
                    .flatten();
                ("already_deleted", false, current, Some(id))
            } else {
                let ts: Option<i64> = conn
                    .query_row(
                        "SELECT deleted_at FROM memories WHERE id = ?1",
                        params![id],
                        |r| r.get::<_, Option<i64>>(0),
                    )
                    .optional()?
                    .flatten();
                ("soft_deleted", true, ts, Some(id))
            }
        }
    };

    if forgotten {
        if let Some(id) = memory_id {
            // FTS5 external-content: manual `DELETE FROM fts_memories WHERE rowid=?`
            // corrupts the index. The correct cleanup happens via the `trg_fts_ad` trigger
            // when `purge` physically removes the row from `memories`. Between soft-delete
            // and purge, FTS queries filter `m.deleted_at IS NULL` in the JOIN.
            if let Err(e) = memories::delete_vec(&conn, id) {
                tracing::warn!(memory_id = id, error = %e, "vec cleanup failed — orphan vector left");
            }
        }
    }

    let response = ForgetResponse {
        action: action.to_string(),
        forgotten,
        name: name.clone(),
        namespace: namespace.clone(),
        deleted_at,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };
    output::emit_json(&response)?;

    if action == "not_found" {
        return Err(AppError::NotFound(errors_msg::memory_not_found(
            &name, &namespace,
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forget_response_serializes_basic_fields() {
        let resp = ForgetResponse {
            action: "soft_deleted".to_string(),
            forgotten: true,
            name: "my-memory".to_string(),
            namespace: "global".to_string(),
            deleted_at: Some(1_700_000_000),
            elapsed_ms: 5,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "soft_deleted");
        assert_eq!(json["forgotten"], true);
        assert_eq!(json["name"], "my-memory");
        assert_eq!(json["namespace"], "global");
        assert_eq!(json["deleted_at"], 1_700_000_000i64);
        assert!(json["elapsed_ms"].is_number());
    }

    #[test]
    fn forget_response_action_soft_deleted_implies_forgotten_true() {
        let resp = ForgetResponse {
            action: "soft_deleted".to_string(),
            forgotten: true,
            name: "test".to_string(),
            namespace: "ns".to_string(),
            deleted_at: Some(42),
            elapsed_ms: 1,
        };
        assert_eq!(resp.action, "soft_deleted");
        assert!(resp.forgotten);
        assert_eq!(resp.deleted_at, Some(42));
    }

    #[test]
    fn forget_response_already_deleted_preserves_timestamp() {
        let resp = ForgetResponse {
            action: "already_deleted".to_string(),
            forgotten: false,
            name: "abc".to_string(),
            namespace: "my-project".to_string(),
            deleted_at: Some(1_650_000_000),
            elapsed_ms: 2,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "already_deleted");
        assert_eq!(json["forgotten"], false);
        assert_eq!(json["deleted_at"], 1_650_000_000i64);
    }

    #[test]
    fn forget_response_not_found_emits_deleted_at_null() {
        let resp = ForgetResponse {
            action: "not_found".to_string(),
            forgotten: false,
            name: "phantom".to_string(),
            namespace: "global".to_string(),
            deleted_at: None,
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "not_found");
        assert_eq!(json["forgotten"], false);
        assert!(json["deleted_at"].is_null());
        assert_eq!(json["elapsed_ms"], 0u64);
    }
}
