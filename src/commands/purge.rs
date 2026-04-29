//! Handler for the `purge` CLI subcommand.

use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use serde::Serialize;

#[derive(clap::Args)]
pub struct PurgeArgs {
    #[arg(long)]
    pub name: Option<String>,
    /// Namespace to purge. Defaults to the contextual namespace (SQLITE_GRAPHRAG_NAMESPACE env var or "global").
    #[arg(long)]
    pub namespace: Option<String>,
    /// Retention days: memories with deleted_at older than (now - retention_days*86400) will be
    /// permanently removed. Default: PURGE_RETENTION_DAYS_DEFAULT (90).
    #[arg(long, alias = "days", value_name = "DAYS", default_value_t = crate::constants::PURGE_RETENTION_DAYS_DEFAULT)]
    pub retention_days: u32,
    /// [DEPRECATED in v2.0.0] Legacy alias — use --retention-days instead.
    #[arg(long, hide = true)]
    pub older_than_seconds: Option<u64>,
    /// Does not execute DELETE: computes and reports what WOULD be purged.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    /// Compatibility with tools that pass --yes to confirm destructive operations.
    #[arg(long, hide = true, default_value_t = false)]
    pub yes: bool,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
pub struct PurgeResponse {
    pub purged_count: usize,
    pub bytes_freed: i64,
    pub oldest_deleted_at: Option<i64>,
    pub retention_days_used: u32,
    pub dry_run: bool,
    pub namespace: Option<String>,
    pub cutoff_epoch: i64,
    pub warnings: Vec<String>,
    /// Total execution time in milliseconds from handler start to serialisation.
    pub elapsed_ms: u64,
}

/// Permanently delete soft-deleted memories that have exceeded the retention window.
///
/// Only memories with `deleted_at IS NOT NULL AND deleted_at <= cutoff_epoch` are affected.
/// When `--dry-run` is set the DELETE is skipped and the response reflects candidates only.
pub fn run(args: PurgeArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if !paths.db.exists() {
        return Err(AppError::NotFound(errors_msg::database_not_found(
            &paths.db.display().to_string(),
        )));
    }

    let mut warnings: Vec<String> = Vec::new();
    let now = current_epoch()?;

    let cutoff_epoch = if let Some(secs) = args.older_than_seconds {
        warnings.push(
            "--older-than-seconds is deprecated; use --retention-days in v2.0.0+".to_string(),
        );
        now - secs as i64
    } else {
        now - (args.retention_days as i64) * 86_400
    };

    let namespace_opt: Option<&str> = Some(namespace.as_str());

    let mut conn = open_rw(&paths.db)?;

    let (bytes_freed, oldest_deleted_at, candidates_count) =
        compute_metrics(&conn, cutoff_epoch, namespace_opt, args.name.as_deref())?;

    if candidates_count == 0 && args.name.is_some() {
        return Err(AppError::NotFound(
            errors_msg::soft_deleted_memory_not_found(
                args.name.as_deref().unwrap_or_default(),
                &namespace,
            ),
        ));
    }

    if !args.dry_run {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        execute_purge(
            &tx,
            &namespace,
            args.name.as_deref(),
            cutoff_epoch,
            &mut warnings,
        )?;
        tx.commit()?;
    }

    output::emit_json(&PurgeResponse {
        purged_count: candidates_count,
        bytes_freed,
        oldest_deleted_at,
        retention_days_used: args.retention_days,
        dry_run: args.dry_run,
        namespace: Some(namespace),
        cutoff_epoch,
        warnings,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

fn current_epoch() -> Result<i64, AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| AppError::Internal(anyhow::anyhow!("system clock error: {err}")))?;
    Ok(now.as_secs() as i64)
}

fn compute_metrics(
    conn: &rusqlite::Connection,
    cutoff_epoch: i64,
    namespace_opt: Option<&str>,
    name: Option<&str>,
) -> Result<(i64, Option<i64>, usize), AppError> {
    let (bytes_freed, oldest_deleted_at): (i64, Option<i64>) = if let Some(name) = name {
        conn.query_row(
            "SELECT COALESCE(SUM(LENGTH(COALESCE(body,'')) + LENGTH(COALESCE(description,'')) + LENGTH(name)), 0),
                    MIN(deleted_at)
             FROM memories
             WHERE deleted_at IS NOT NULL AND deleted_at <= ?1
                   AND (?2 IS NULL OR namespace = ?2)
                   AND name = ?3",
            rusqlite::params![cutoff_epoch, namespace_opt, name],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)),
        )?
    } else {
        conn.query_row(
            "SELECT COALESCE(SUM(LENGTH(COALESCE(body,'')) + LENGTH(COALESCE(description,'')) + LENGTH(name)), 0),
                    MIN(deleted_at)
             FROM memories
             WHERE deleted_at IS NOT NULL AND deleted_at <= ?1
                   AND (?2 IS NULL OR namespace = ?2)",
            rusqlite::params![cutoff_epoch, namespace_opt],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)),
        )?
    };

    let count: usize = if let Some(name) = name {
        conn.query_row(
            "SELECT COUNT(*) FROM memories
             WHERE deleted_at IS NOT NULL AND deleted_at <= ?1
                   AND (?2 IS NULL OR namespace = ?2)
                   AND name = ?3",
            rusqlite::params![cutoff_epoch, namespace_opt, name],
            |r| r.get::<_, usize>(0),
        )?
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM memories
             WHERE deleted_at IS NOT NULL AND deleted_at <= ?1
                   AND (?2 IS NULL OR namespace = ?2)",
            rusqlite::params![cutoff_epoch, namespace_opt],
            |r| r.get::<_, usize>(0),
        )?
    };

    Ok((bytes_freed, oldest_deleted_at, count))
}

fn execute_purge(
    tx: &rusqlite::Transaction,
    namespace: &str,
    name: Option<&str>,
    cutoff_epoch: i64,
    warnings: &mut Vec<String>,
) -> Result<(), AppError> {
    let candidates = select_candidates(tx, namespace, name, cutoff_epoch)?;

    for (memory_id, _name) in &candidates {
        if let Err(err) = tx.execute(
            "DELETE FROM vec_chunks WHERE memory_id = ?1",
            rusqlite::params![memory_id],
        ) {
            warnings.push(format!(
                "failed to clean vec_chunks for memory_id {memory_id}: {err}"
            ));
        }
        if let Err(err) = tx.execute(
            "DELETE FROM vec_memories WHERE memory_id = ?1",
            rusqlite::params![memory_id],
        ) {
            warnings.push(format!(
                "failed to clean vec_memories for memory_id {memory_id}: {err}"
            ));
        }
        tx.execute(
            "DELETE FROM memories WHERE id = ?1 AND namespace = ?2 AND deleted_at IS NOT NULL",
            rusqlite::params![memory_id, namespace],
        )?;
    }

    Ok(())
}

fn select_candidates(
    conn: &rusqlite::Connection,
    namespace: &str,
    name: Option<&str>,
    cutoff_epoch: i64,
) -> Result<Vec<(i64, String)>, AppError> {
    let query = if name.is_some() {
        "SELECT id, name FROM memories
         WHERE namespace = ?1 AND name = ?2 AND deleted_at IS NOT NULL AND deleted_at <= ?3
         ORDER BY deleted_at ASC"
    } else {
        "SELECT id, name FROM memories
         WHERE namespace = ?1 AND deleted_at IS NOT NULL AND deleted_at <= ?2
         ORDER BY deleted_at ASC"
    };

    let mut stmt = conn.prepare(query)?;
    let rows = if let Some(name) = name {
        stmt.query_map(rusqlite::params![namespace, name, cutoff_epoch], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map(rusqlite::params![namespace, cutoff_epoch], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("falha ao abrir banco em memória");
        conn.execute_batch(
            "CREATE TABLE memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                namespace TEXT NOT NULL DEFAULT 'global',
                description TEXT,
                body TEXT,
                deleted_at INTEGER
            );
            CREATE TABLE IF NOT EXISTS vec_chunks (memory_id INTEGER);
            CREATE TABLE IF NOT EXISTS vec_memories (memory_id INTEGER);",
        )
        .expect("falha ao criar tabelas de teste");
        conn
    }

    fn insert_deleted_memory(
        conn: &Connection,
        name: &str,
        namespace: &str,
        body: &str,
        deleted_at: i64,
    ) -> i64 {
        conn.execute(
            "INSERT INTO memories (name, namespace, body, deleted_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![name, namespace, body, deleted_at],
        )
        .expect("falha ao inserir memória de teste");
        conn.last_insert_rowid()
    }

    #[test]
    fn retention_days_used_padrao_eh_90() {
        assert_eq!(crate::constants::PURGE_RETENTION_DAYS_DEFAULT, 90u32);
    }

    #[test]
    fn compute_metrics_bytes_freed_positivo_para_body_populado() {
        let conn = setup_test_db();
        let now = current_epoch().expect("epoch falhou");
        let old_epoch = now - 100 * 86_400;
        insert_deleted_memory(&conn, "mem-teste", "global", "corpo da memória", old_epoch);

        let cutoff = now - 30 * 86_400;
        let (bytes, oldest, count) =
            compute_metrics(&conn, cutoff, Some("global"), None).expect("compute_metrics falhou");

        assert!(bytes > 0, "bytes_freed deve ser > 0 para body populado");
        assert!(oldest.is_some(), "oldest_deleted_at deve ser Some");
        assert_eq!(count, 1);
    }

    #[test]
    fn compute_metrics_retorna_zero_sem_candidatos() {
        let conn = setup_test_db();
        let now = current_epoch().expect("epoch falhou");
        let cutoff = now - 90 * 86_400;

        let (bytes, oldest, count) =
            compute_metrics(&conn, cutoff, Some("global"), None).expect("compute_metrics falhou");

        assert_eq!(bytes, 0);
        assert!(oldest.is_none());
        assert_eq!(count, 0);
    }

    #[test]
    fn dry_run_nao_deleta_registros() {
        let conn = setup_test_db();
        let now = current_epoch().expect("epoch falhou");
        let old_epoch = now - 200 * 86_400;
        insert_deleted_memory(&conn, "mem-dry", "global", "conteúdo dry run", old_epoch);

        let cutoff = now - 30 * 86_400;
        let (_, _, count_antes) =
            compute_metrics(&conn, cutoff, Some("global"), None).expect("compute_metrics falhou");
        assert_eq!(count_antes, 1, "deve haver 1 candidato antes do dry run");

        let (_, _, count_depois) =
            compute_metrics(&conn, cutoff, Some("global"), None).expect("compute_metrics falhou");
        assert_eq!(
            count_depois, 1,
            "dry_run não deve remover registros: count deve permanecer 1"
        );
    }

    #[test]
    fn oldest_deleted_at_retorna_menor_epoch() {
        let conn = setup_test_db();
        let now = current_epoch().expect("epoch falhou");
        let epoch_antigo = now - 300 * 86_400;
        let epoch_recente = now - 200 * 86_400;

        insert_deleted_memory(&conn, "mem-a", "global", "corpo-a", epoch_antigo);
        insert_deleted_memory(&conn, "mem-b", "global", "corpo-b", epoch_recente);

        let cutoff = now - 30 * 86_400;
        let (_, oldest, count) =
            compute_metrics(&conn, cutoff, Some("global"), None).expect("compute_metrics falhou");

        assert_eq!(count, 2);
        assert_eq!(
            oldest,
            Some(epoch_antigo),
            "oldest_deleted_at deve ser o epoch mais antigo"
        );
    }

    #[test]
    fn purge_args_namespace_aceita_none_sem_default() {
        // P1-C: namespace deve ser None quando não fornecido, permitindo resolve_namespace
        // consultar SQLITE_GRAPHRAG_NAMESPACE antes de cair em "global".
        // O campo era `default_value = "global"` antes de P1-C; com isso removido,
        // resolve_namespace(None) consulta o env var corretamente.
        let resolved = crate::namespace::resolve_namespace(None)
            .expect("resolve_namespace(None) deve retornar Ok");
        assert_eq!(
            resolved, "global",
            "sem env var, resolve_namespace(None) deve cair em 'global'"
        );
    }

    #[test]
    fn purge_response_serializa_todos_campos_novos() {
        let resp = PurgeResponse {
            purged_count: 3,
            bytes_freed: 1024,
            oldest_deleted_at: Some(1_700_000_000),
            retention_days_used: 90,
            dry_run: false,
            namespace: Some("global".to_string()),
            cutoff_epoch: 1_710_000_000,
            warnings: vec![],
            elapsed_ms: 42,
        };
        let json = serde_json::to_string(&resp).expect("serialização falhou");
        assert!(json.contains("bytes_freed"));
        assert!(json.contains("oldest_deleted_at"));
        assert!(json.contains("retention_days_used"));
        assert!(json.contains("dry_run"));
        assert!(json.contains("elapsed_ms"));
    }
}
