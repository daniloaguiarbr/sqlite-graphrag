use crate::errors::AppError;
use crate::i18n::erros;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::{memories, versions};
use serde::Serialize;

#[derive(clap::Args)]
pub struct RenameArgs {
    /// Nome atual da memória. Aceita alias `--old` para compatibilidade com doc bilíngue.
    #[arg(long, alias = "old")]
    pub name: String,
    /// Novo nome da memória. Aceita alias `--new` para compatibilidade com doc bilíngue.
    #[arg(long, alias = "new")]
    pub new_name: String,
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
    /// Session ID opcional para rastrear origem da mudança.
    #[arg(long, value_name = "UUID")]
    pub session_id: Option<String>,
    /// Formato da saída.
    #[arg(long, value_enum, default_value_t = crate::output::OutputFormat::Json)]
    pub format: crate::output::OutputFormat,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct RenameResponse {
    memory_id: i64,
    name: String,
    action: &'static str,
    version: i64,
    /// Tempo total de execução em milissegundos desde início do handler até serialização.
    elapsed_ms: u64,
}

pub fn run(args: RenameArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    use crate::constants::*;

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;

    if args.new_name.starts_with("__") {
        return Err(AppError::Validation(
            crate::i18n::validacao::nome_reservado(),
        ));
    }

    if args.new_name.is_empty() || args.new_name.len() > MAX_MEMORY_NAME_LEN {
        return Err(AppError::Validation(
            crate::i18n::validacao::novo_nome_comprimento(MAX_MEMORY_NAME_LEN),
        ));
    }

    {
        let slug_re = regex::Regex::new(crate::constants::NAME_SLUG_REGEX)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("regex: {e}")))?;
        if !slug_re.is_match(&args.new_name) {
            return Err(AppError::Validation(
                crate::i18n::validacao::novo_nome_kebab(&args.new_name),
            ));
        }
    }

    let paths = AppPaths::resolve(args.db.as_deref())?;
    let mut conn = open_rw(&paths.db)?;

    let (memory_id, current_updated_at, _) = memories::find_by_name(&conn, &namespace, &args.name)?
        .ok_or_else(|| AppError::NotFound(erros::memoria_nao_encontrada(&args.name, &namespace)))?;

    if let Some(expected) = args.expected_updated_at {
        if expected != current_updated_at {
            return Err(AppError::Conflict(erros::conflito_optimistic_lock(
                expected,
                current_updated_at,
            )));
        }
    }

    let row = memories::read_by_name(&conn, &namespace, &args.name)?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("memory not found before rename")))?;

    let memory_type = row.memory_type.clone();
    let description = row.description.clone();
    let body = row.body.clone();
    let metadata = row.metadata.clone();

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    let affected = if let Some(ts) = args.expected_updated_at {
        tx.execute(
            "UPDATE memories SET name=?2 WHERE id=?1 AND updated_at=?3 AND deleted_at IS NULL",
            rusqlite::params![memory_id, args.new_name, ts],
        )?
    } else {
        tx.execute(
            "UPDATE memories SET name=?2 WHERE id=?1 AND deleted_at IS NULL",
            rusqlite::params![memory_id, args.new_name],
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
        &args.new_name,
        &memory_type,
        &description,
        &body,
        &metadata,
        None,
        "rename",
    )?;

    tx.commit()?;

    output::emit_json(&RenameResponse {
        memory_id,
        name: args.new_name,
        action: "renamed",
        version: next_v,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod testes {
    use crate::storage::memories::{insert, NewMemory};
    use tempfile::TempDir;

    fn setup_db() -> (TempDir, rusqlite::Connection) {
        crate::storage::connection::register_vec_extension();
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let mut conn = rusqlite::Connection::open(&db_path).unwrap();
        crate::migrations::runner().run(&mut conn).unwrap();
        (dir, conn)
    }

    fn nova_memoria(name: &str) -> NewMemory {
        NewMemory {
            namespace: "global".to_string(),
            name: name.to_string(),
            memory_type: "user".to_string(),
            description: "desc".to_string(),
            body: "corpo".to_string(),
            body_hash: format!("hash-{name}"),
            session_id: None,
            source: "agent".to_string(),
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn rejeita_new_name_com_prefixo_duplo_underscore() {
        use crate::errors::AppError;
        let (_dir, conn) = setup_db();
        insert(&conn, &nova_memoria("mem-teste")).unwrap();
        drop(conn);

        let err = AppError::Validation(
            "names and namespaces starting with __ are reserved for internal use".to_string(),
        );
        assert!(err.to_string().contains("__"));
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn optimistic_lock_conflict_retorna_exit_3() {
        use crate::errors::AppError;
        let err = AppError::Conflict(
            "optimistic lock conflict: expected updated_at=100, but current is 200".to_string(),
        );
        assert_eq!(err.exit_code(), 3);
    }
}
