//! Handler for the `restore` CLI subcommand.

use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output;
use crate::output::JsonOutputFormat;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::memories;
use crate::storage::versions;
use rusqlite::params;
use rusqlite::OptionalExtension;
use serde::Serialize;

#[derive(clap::Args)]
pub struct RestoreArgs {
    /// Memory name to restore (must exist, including soft-deleted/forgotten).
    #[arg(long)]
    pub name: String,
    /// Version to restore. When omitted, defaults to the latest non-`restore` version
    /// from `memory_versions`. This makes the forget+restore workflow work without
    /// requiring the user to discover the version first.
    #[arg(long)]
    pub version: Option<i64>,
    #[arg(long, default_value = "global")]
    pub namespace: Option<String>,
    /// Optimistic locking: reject if the current updated_at does not match (exit 3).
    #[arg(
        long,
        value_name = "EPOCH_OR_RFC3339",
        value_parser = crate::parsers::parse_expected_updated_at,
        long_help = "Optimistic lock: reject if updated_at does not match. \
Accepts Unix epoch (e.g. 1700000000) or RFC 3339 (e.g. 2026-04-19T12:00:00Z)."
    )]
    pub expected_updated_at: Option<i64>,
    /// Output format.
    #[arg(long, value_enum, default_value_t = JsonOutputFormat::Json)]
    pub format: JsonOutputFormat,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct RestoreResponse {
    memory_id: i64,
    name: String,
    version: i64,
    restored_from: i64,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: RestoreArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let _ = args.format;
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    let mut conn = open_rw(&paths.db)?;

    // PRD linha 1118: buscar SEM filtro deleted_at — restore deve funcionar em memórias soft-deletadas
    let result: Option<(i64, i64)> = conn
        .query_row(
            "SELECT id, updated_at FROM memories WHERE namespace = ?1 AND name = ?2",
            params![namespace, args.name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (memory_id, current_updated_at) = result
        .ok_or_else(|| AppError::NotFound(errors_msg::memory_not_found(&args.name, &namespace)))?;

    if let Some(expected) = args.expected_updated_at {
        if expected != current_updated_at {
            return Err(AppError::Conflict(errors_msg::optimistic_lock_conflict(
                expected,
                current_updated_at,
            )));
        }
    }

    // v1.0.22 P0: resolve `--version` opcional. Quando ausente, usa a maior versão
    // cujo `change_reason` não seja 'restore' (recupera o estado real, não meta-restore).
    // Permite o workflow forget+restore funcionar sem ler memory_versions manualmente.
    let target_version: i64 = match args.version {
        Some(v) => v,
        None => {
            let last: Option<i64> = conn
                .query_row(
                    "SELECT MAX(version) FROM memory_versions
                     WHERE memory_id = ?1 AND change_reason != 'restore'",
                    params![memory_id],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            let v = last.ok_or_else(|| {
                AppError::NotFound(errors_msg::memory_not_found(&args.name, &namespace))
            })?;
            tracing::info!(
                "restore --version omitted; using latest non-restore version: {}",
                v
            );
            v
        }
    };

    let version_row: (String, String, String, String, String) = {
        let mut stmt = conn.prepare(
            "SELECT name, type, description, body, metadata
             FROM memory_versions
             WHERE memory_id = ?1 AND version = ?2",
        )?;

        stmt.query_row(params![memory_id, target_version], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .map_err(|_| {
            AppError::NotFound(errors_msg::version_not_found(target_version, &args.name))
        })?
    };

    let (old_name, old_type, old_description, old_body, old_metadata) = version_row;

    // v1.0.21 P1-D: re-embed body restaurado para manter `vec_memories` sincronizado
    // com `memories`. Sem isso, queries semânticas usavam o vetor da versão pós-forget,
    // causando recall inconsistente (vec_memories=2 vs memories=3 após forget+restore).
    output::emit_progress_i18n(
        "Re-computing embedding for restored memory...",
        "Recalculando embedding da memória restaurada...",
    );
    let embedding = crate::daemon::embed_passage_or_local(&paths.models, &old_body)?;
    let snippet: String = old_body.chars().take(300).collect();

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    // deleted_at = NULL reativa memórias soft-deletadas; sem filtro deleted_at no WHERE
    let affected = if let Some(ts) = args.expected_updated_at {
        tx.execute(
            "UPDATE memories SET name=?2, type=?3, description=?4, body=?5, body_hash=?6, deleted_at=NULL
             WHERE id=?1 AND updated_at=?7",
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
            "UPDATE memories SET name=?2, type=?3, description=?4, body=?5, body_hash=?6, deleted_at=NULL
             WHERE id=?1",
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
        return Err(AppError::Conflict(errors_msg::concurrent_process_conflict()));
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

    // v1.0.21 P1-D: ressincronizar vec_memories com o body restaurado.
    memories::upsert_vec(
        &tx, memory_id, &namespace, &old_type, &embedding, &old_name, &snippet,
    )?;

    tx.commit()?;

    output::emit_json(&RestoreResponse {
        memory_id,
        name: old_name,
        version: next_v,
        restored_from: target_version,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::errors::AppError;

    #[test]
    fn optimistic_lock_conflict_returns_exit_3() {
        let err = AppError::Conflict(
            "optimistic lock conflict: expected updated_at=50, but current is 99".to_string(),
        );
        assert_eq!(err.exit_code(), 3);
        assert!(err.to_string().contains("conflict"));
    }
}
