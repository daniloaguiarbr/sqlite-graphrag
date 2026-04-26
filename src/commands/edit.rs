use crate::errors::AppError;
use crate::i18n::erros;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::{memories, versions};
use serde::Serialize;
use std::io::Read as _;

#[derive(clap::Args)]
pub struct EditArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long, conflicts_with_all = ["body_file", "body_stdin"])]
    pub body: Option<String>,
    #[arg(long, conflicts_with_all = ["body", "body_stdin"])]
    pub body_file: Option<std::path::PathBuf>,
    #[arg(long, conflicts_with_all = ["body", "body_file"])]
    pub body_stdin: bool,
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
    #[arg(long, help = "No-op; JSON is always emitted on stdout")]
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
    /// Tempo total de execução em milissegundos desde início do handler até serialização.
    elapsed_ms: u64,
}

pub fn run(args: EditArgs) -> Result<(), AppError> {
    use crate::constants::*;

    let inicio = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;

    let paths = AppPaths::resolve(args.db.as_deref())?;
    let mut conn = open_rw(&paths.db)?;

    let (memory_id, current_updated_at, _current_version) =
        memories::find_by_name(&conn, &namespace, &args.name)?.ok_or_else(|| {
            AppError::NotFound(erros::memoria_nao_encontrada(&args.name, &namespace))
        })?;

    if let Some(expected) = args.expected_updated_at {
        if expected != current_updated_at {
            return Err(AppError::Conflict(erros::conflito_optimistic_lock(
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
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(AppError::Io)?;
            buf
        };
        if b.len() > MAX_MEMORY_BODY_LEN {
            return Err(AppError::LimitExceeded(
                crate::i18n::validacao::body_excede(MAX_MEMORY_BODY_LEN),
            ));
        }
        raw_body = Some(b);
    }

    if let Some(ref desc) = args.description {
        if desc.len() > MAX_MEMORY_DESCRIPTION_LEN {
            return Err(AppError::Validation(
                crate::i18n::validacao::descricao_excede(MAX_MEMORY_DESCRIPTION_LEN),
            ));
        }
    }

    let row = memories::read_by_name(&conn, &namespace, &args.name)?
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
        &args.name,
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
        name: args.name,
        action: "updated".to_string(),
        version: next_v,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn edit_response_serializa_todos_campos() {
        let resp = EditResponse {
            memory_id: 42,
            name: "minha-memoria".to_string(),
            action: "updated".to_string(),
            version: 3,
            elapsed_ms: 7,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["memory_id"], 42i64);
        assert_eq!(json["name"], "minha-memoria");
        assert_eq!(json["action"], "updated");
        assert_eq!(json["version"], 3i64);
        assert!(json["elapsed_ms"].is_number());
    }

    #[test]
    fn edit_response_action_contem_updated() {
        let resp = EditResponse {
            memory_id: 1,
            name: "n".to_string(),
            action: "updated".to_string(),
            version: 1,
            elapsed_ms: 0,
        };
        assert_eq!(
            resp.action, "updated",
            "action deve ser 'updated' para edições bem-sucedidas"
        );
    }

    #[test]
    fn edit_body_excede_limite_retorna_erro() {
        let limite = crate::constants::MAX_MEMORY_BODY_LEN;
        let corpo_grande: String = "a".repeat(limite + 1);
        assert!(
            corpo_grande.len() > limite,
            "corpo acima do limite deve ter tamanho > MAX_MEMORY_BODY_LEN"
        );
    }

    #[test]
    fn edit_description_excede_limite_retorna_erro() {
        let limite = crate::constants::MAX_MEMORY_DESCRIPTION_LEN;
        let desc_grande: String = "d".repeat(limite + 1);
        assert!(
            desc_grande.len() > limite,
            "descrição acima do limite deve ter tamanho > MAX_MEMORY_DESCRIPTION_LEN"
        );
    }
}
