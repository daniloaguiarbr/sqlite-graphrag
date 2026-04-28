//! Handler for the `forget` CLI subcommand.

use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::memories;
use serde::Serialize;

#[derive(clap::Args)]
pub struct ForgetArgs {
    /// Memory name as a positional argument. Alternative to `--name`.
    #[arg(value_name = "NAME", conflicts_with = "name")]
    pub name_positional: Option<String>,
    /// Memory name to soft-delete. The row is preserved with `deleted_at` set, recoverable via `restore`.
    /// Use `purge` to permanently remove soft-deleted memories.
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long, default_value = "global")]
    pub namespace: Option<String>,
    #[arg(long, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct ForgetResponse {
    forgotten: bool,
    name: String,
    namespace: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: ForgetArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    // Resolve name from positional or --name flag; both are optional, at least one is required.
    let name = args.name_positional.or(args.name).ok_or_else(|| {
        AppError::Validation("name required: pass as positional argument or via --name".to_string())
    })?;
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    if !paths.db.exists() {
        return Err(AppError::NotFound(errors_msg::database_not_found(
            &paths.db.display().to_string(),
        )));
    }

    let conn = open_rw(&paths.db)?;

    let maybe_row = memories::read_by_name(&conn, &namespace, &name)?;
    let forgotten = memories::soft_delete(&conn, &namespace, &name)?;

    if !forgotten {
        return Err(AppError::NotFound(errors_msg::memory_not_found(
            &name, &namespace,
        )));
    }

    if let Some(row) = maybe_row {
        // FTS5 external-content: manual `DELETE FROM fts_memories WHERE rowid=?`
        // corrompe o índice. A limpeza correta acontece via trigger `trg_fts_ad`
        // quando `purge` remove fisicamente a linha de `memories`. Entre soft-delete
        // e purge, as queries FTS filtram `m.deleted_at IS NULL` no JOIN.
        if let Err(e) = memories::delete_vec(&conn, row.id) {
            tracing::warn!(memory_id = row.id, error = %e, "vec cleanup failed — orphan vector left");
        }
    }

    output::emit_json(&ForgetResponse {
        forgotten: true,
        name,
        namespace,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forget_response_serializa_campos_basicos() {
        let resp = ForgetResponse {
            forgotten: true,
            name: "minha-memoria".to_string(),
            namespace: "global".to_string(),
            elapsed_ms: 5,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["forgotten"], true);
        assert_eq!(json["name"], "minha-memoria");
        assert_eq!(json["namespace"], "global");
        assert!(json["elapsed_ms"].is_number());
    }

    #[test]
    fn forget_response_forgotten_true_indica_sucesso() {
        let resp = ForgetResponse {
            forgotten: true,
            name: "teste".to_string(),
            namespace: "ns".to_string(),
            elapsed_ms: 1,
        };
        assert!(
            resp.forgotten,
            "forgotten deve ser true quando soft-delete bem-sucedido"
        );
    }

    #[test]
    fn forget_resposta_com_namespace_correto() {
        let resp = ForgetResponse {
            forgotten: true,
            name: "abc".to_string(),
            namespace: "meu-projeto".to_string(),
            elapsed_ms: 0,
        };
        assert_eq!(
            resp.namespace, "meu-projeto",
            "namespace deve ser preservado na resposta"
        );
    }

    #[test]
    fn forget_elapsed_ms_zero_e_valido() {
        let resp = ForgetResponse {
            forgotten: true,
            name: "qualquer".to_string(),
            namespace: "global".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["elapsed_ms"], 0u64);
    }
}
