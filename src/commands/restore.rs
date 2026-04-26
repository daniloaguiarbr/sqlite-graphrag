use crate::errors::AppError;
use crate::i18n::erros;
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
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub version: i64,
    #[arg(long, default_value = "global")]
    pub namespace: Option<String>,
    /// Optimistic locking: rejeitar se updated_at atual não bater (exit 3).
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
    #[arg(long, help = "No-op; JSON is always emitted on stdout")]
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
    /// Tempo total de execução em milissegundos desde início do handler até serialização.
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
        .ok_or_else(|| AppError::NotFound(erros::memoria_nao_encontrada(&args.name, &namespace)))?;

    if let Some(expected) = args.expected_updated_at {
        if expected != current_updated_at {
            return Err(AppError::Conflict(erros::conflito_optimistic_lock(
                expected,
                current_updated_at,
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
        .map_err(|_| AppError::NotFound(erros::versao_nao_encontrada(args.version, &args.name)))?
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
        return Err(AppError::Conflict(erros::conflito_processo_concorrente()));
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
        restored_from: args.version,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
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
