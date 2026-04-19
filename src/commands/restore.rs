use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::{memories, versions};
use rusqlite::params;
use serde::Serialize;

#[derive(clap::Args)]
pub struct RestoreArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub version: i64,
    #[arg(long, default_value = "global")]
    pub namespace: Option<String>,
    /// Optimistic locking: rejeitar se updated_at atual não bater (exit 3).
    #[arg(long, value_name = "EPOCH")]
    pub expected_updated_at: Option<i64>,
    /// Formato da saída.
    #[arg(long, value_enum, default_value_t = crate::output::OutputFormat::Json)]
    pub format: crate::output::OutputFormat,
    #[arg(long, env = "NEUROGRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct RestoreResponse {
    memory_id: i64,
    name: String,
    version: i64,
    restored_from: i64,
}

pub fn run(args: RestoreArgs) -> Result<(), AppError> {
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    let mut conn = open_rw(&paths.db)?;

    let (memory_id, current_updated_at, _) = memories::find_by_name(&conn, &namespace, &args.name)?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "memory '{}' not found in namespace '{}'",
                args.name, namespace
            ))
        })?;

    if let Some(expected) = args.expected_updated_at {
        if expected != current_updated_at {
            return Err(AppError::Conflict(format!(
                "optimistic lock conflict: expected updated_at={expected}, but current is {current_updated_at}"
            )));
        }
    }

    let version_row: (String, String, String, String, String) = {
        let mut stmt = conn.prepare(
            "SELECT name, type, description, body, metadata
             FROM memory_versions
             WHERE memory_id = ?1 AND version = ?2",
        )?;

        stmt.query_row(params![memory_id, args.version], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .map_err(|_| {
            AppError::NotFound(format!(
                "version {} not found for memory '{}'",
                args.version, args.name
            ))
        })?
    };

    let (old_name, old_type, old_description, old_body, old_metadata) = version_row;

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    let affected = if let Some(ts) = args.expected_updated_at {
        tx.execute(
            "UPDATE memories SET name=?2, type=?3, description=?4, body=?5, body_hash=?6
             WHERE id=?1 AND updated_at=?7 AND deleted_at IS NULL",
            rusqlite::params![
                memory_id,
                old_name,
                old_type,
                old_description,
                old_body,
                blake3::hash(old_body.as_bytes()).to_hex().to_string(),
                ts
            ],
        )?
    } else {
        tx.execute(
            "UPDATE memories SET name=?2, type=?3, description=?4, body=?5, body_hash=?6
             WHERE id=?1 AND deleted_at IS NULL",
            rusqlite::params![
                memory_id,
                old_name,
                old_type,
                old_description,
                old_body,
                blake3::hash(old_body.as_bytes()).to_hex().to_string()
            ],
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
        &old_name,
        &old_type,
        &old_description,
        &old_body,
        &old_metadata,
        None,
        "restore",
    )?;

    tx.commit()?;

    output::emit_json(&RestoreResponse {
        memory_id,
        name: old_name,
        version: next_v,
        restored_from: args.version,
    })?;

    Ok(())
}

#[cfg(test)]
mod testes {
    use crate::errors::AppError;

    #[test]
    fn optimistic_lock_conflict_retorna_exit_3() {
        let err = AppError::Conflict(
            "optimistic lock conflict: expected updated_at=50, but current is 99".to_string(),
        );
        assert_eq!(err.exit_code(), 3);
        assert!(err.to_string().contains("conflict"));
    }
}
