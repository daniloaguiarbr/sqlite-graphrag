//! Handler for the `edit` CLI subcommand.

use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::{memories, versions};
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Edit body inline\n  \
    sqlite-graphrag edit onboarding --body \"updated content\"\n\n  \
    # Edit body from a file\n  \
    sqlite-graphrag edit onboarding --body-file ./updated.md\n\n  \
    # Edit body from stdin (pipe)\n  \
    cat updated.md | sqlite-graphrag edit onboarding --body-stdin\n\n  \
    # Update only the description\n  \
    sqlite-graphrag edit onboarding --description \"new short description\"")]
pub struct EditArgs {
    /// Memory name as a positional argument. Alternative to `--name`.
    #[arg(
        value_name = "NAME",
        conflicts_with = "name",
        help = "Memory name to edit; alternative to --name"
    )]
    pub name_positional: Option<String>,
    /// Memory name to edit. Soft-deleted memories are not editable; use `restore` first.
    #[arg(long)]
    pub name: Option<String>,
    /// New inline body content. Mutually exclusive with --body-file and --body-stdin.
    #[arg(long, conflicts_with_all = ["body_file", "body_stdin"])]
    pub body: Option<String>,
    /// Read new body from a file. Mutually exclusive with --body and --body-stdin.
    #[arg(long, conflicts_with_all = ["body", "body_stdin"])]
    pub body_file: Option<std::path::PathBuf>,
    /// Read new body from stdin until EOF. Mutually exclusive with --body and --body-file.
    #[arg(long, conflicts_with_all = ["body", "body_file"])]
    pub body_stdin: bool,
    /// New description (≤500 chars) replacing the existing one.
    #[arg(long)]
    pub description: Option<String>,
    #[arg(
        long,
        value_name = "EPOCH_OR_RFC3339",
        value_parser = crate::parsers::parse_expected_updated_at,
        long_help = "Optimistic lock: reject if updated_at does not match. \
Accepts Unix epoch (e.g. 1700000000) or RFC 3339 (e.g. 2026-04-19T12:00:00Z)."
    )]
    pub expected_updated_at: Option<i64>,
    #[arg(long, default_value = "global")]
    pub namespace: Option<String>,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct EditResponse {
    memory_id: i64,
    name: String,
    action: String,
    version: i64,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: EditArgs) -> Result<(), AppError> {
    use crate::constants::*;

    let inicio = std::time::Instant::now();
    // Resolve name from positional or --name flag; both are optional, at least one is required.
    let name = args.name_positional.or(args.name).ok_or_else(|| {
        AppError::Validation("name required: pass as positional argument or via --name".to_string())
    })?;
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;

    let paths = AppPaths::resolve(args.db.as_deref())?;
    crate::storage::connection::ensure_db_ready(&paths)?;
    let mut conn = open_rw(&paths.db)?;

    let (memory_id, current_updated_at, _current_version) =
        memories::find_by_name(&conn, &namespace, &name)?
            .ok_or_else(|| AppError::NotFound(errors_msg::memory_not_found(&name, &namespace)))?;

    if let Some(expected) = args.expected_updated_at {
        if expected != current_updated_at {
            return Err(AppError::Conflict(errors_msg::optimistic_lock_conflict(
                expected,
                current_updated_at,
            )));
        }
    }

    let mut raw_body: Option<String> = None;
    if args.body.is_some() || args.body_file.is_some() || args.body_stdin {
        let b = if let Some(b) = args.body {
            b
        } else if let Some(path) = &args.body_file {
            std::fs::read_to_string(path).map_err(AppError::Io)?
        } else {
            crate::stdin_helper::read_stdin_with_timeout(60)?
        };
        if b.len() > MAX_MEMORY_BODY_LEN {
            return Err(AppError::LimitExceeded(
                crate::i18n::validation::body_exceeds(MAX_MEMORY_BODY_LEN),
            ));
        }
        raw_body = Some(b);
    }

    if let Some(ref desc) = args.description {
        if desc.len() > MAX_MEMORY_DESCRIPTION_LEN {
            return Err(AppError::Validation(
                crate::i18n::validation::description_exceeds(MAX_MEMORY_DESCRIPTION_LEN),
            ));
        }
    }

    let row = memories::read_by_name(&conn, &namespace, &name)?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("memory row not found after check")))?;

    let new_body = raw_body.unwrap_or(row.body.clone());
    let new_description = args.description.unwrap_or(row.description.clone());
    let new_hash = blake3::hash(new_body.as_bytes()).to_hex().to_string();
    let memory_type = row.memory_type.clone();
    let metadata = row.metadata.clone();

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    let affected = if let Some(ts) = args.expected_updated_at {
        tx.execute(
            "UPDATE memories SET description=?2, body=?3, body_hash=?4
             WHERE id=?1 AND updated_at=?5 AND deleted_at IS NULL",
            rusqlite::params![memory_id, new_description, new_body, new_hash, ts],
        )?
    } else {
        tx.execute(
            "UPDATE memories SET description=?2, body=?3, body_hash=?4
             WHERE id=?1 AND deleted_at IS NULL",
            rusqlite::params![memory_id, new_description, new_body, new_hash],
        )?
    };

    if affected == 0 {
        return Err(AppError::Conflict(
            "optimistic lock conflict: memory was modified by another process".to_string(),
        ));
    }

    let next_v = versions::next_version(&tx, memory_id)?;

    versions::insert_version(
        &tx,
        memory_id,
        next_v,
        &name,
        &memory_type,
        &new_description,
        &new_body,
        &metadata,
        None,
        "edit",
    )?;

    tx.commit()?;

    output::emit_json(&EditResponse {
        memory_id,
        name,
        action: "updated".to_string(),
        version: next_v,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_response_serializes_all_fields() {
        let resp = EditResponse {
            memory_id: 42,
            name: "my-memory".to_string(),
            action: "updated".to_string(),
            version: 3,
            elapsed_ms: 7,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["memory_id"], 42i64);
        assert_eq!(json["name"], "my-memory");
        assert_eq!(json["action"], "updated");
        assert_eq!(json["version"], 3i64);
        assert!(json["elapsed_ms"].is_number());
    }

    #[test]
    fn edit_response_action_contains_updated() {
        let resp = EditResponse {
            memory_id: 1,
            name: "n".to_string(),
            action: "updated".to_string(),
            version: 1,
            elapsed_ms: 0,
        };
        assert_eq!(
            resp.action, "updated",
            "action must be 'updated' for successful edits"
        );
    }

    #[test]
    fn edit_body_exceeds_limit_returns_error() {
        let limit = crate::constants::MAX_MEMORY_BODY_LEN;
        let large_body: String = "a".repeat(limit + 1);
        assert!(
            large_body.len() > limit,
            "body above limit must have length > MAX_MEMORY_BODY_LEN"
        );
    }

    #[test]
    fn edit_description_exceeds_limit_returns_error() {
        let limit = crate::constants::MAX_MEMORY_DESCRIPTION_LEN;
        let large_desc: String = "d".repeat(limit + 1);
        assert!(
            large_desc.len() > limit,
            "description above limit must have length > MAX_MEMORY_DESCRIPTION_LEN"
        );
    }
}
