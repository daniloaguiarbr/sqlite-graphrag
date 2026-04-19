use crate::errors::AppError;
use crate::i18n::erros;
use crate::output::{self, OutputFormat};
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::entities;
use serde::Serialize;

#[derive(clap::Args)]
pub struct CleanupOrphansArgs {
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub yes: bool,
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "NEUROGRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct CleanupResponse {
    orphan_count: usize,
    deleted: usize,
    dry_run: bool,
    namespace: Option<String>,
    /// Tempo total de execução em milissegundos desde início do handler até serialização.
    elapsed_ms: u64,
}

pub fn run(args: CleanupOrphansArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if !paths.db.exists() {
        return Err(AppError::NotFound(erros::banco_nao_encontrado(
            &paths.db.display().to_string(),
        )));
    }

    let mut conn = open_rw(&paths.db)?;

    let orphan_ids = entities::find_orphan_entity_ids(&conn, args.namespace.as_deref())?;
    let orphan_count = orphan_ids.len();

    let deleted = if args.dry_run {
        0
    } else {
        if orphan_count > 0 && !args.yes {
            output::emit_progress(&format!(
                "removing {orphan_count} orphan entities (use --yes to skip this notice)"
            ));
        }
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let removed = entities::delete_entities_by_ids(&tx, &orphan_ids)?;
        tx.commit()?;
        removed
    };

    let response = CleanupResponse {
        orphan_count,
        deleted,
        dry_run: args.dry_run,
        namespace: args.namespace.clone(),
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    };

    match args.format {
        OutputFormat::Json => output::emit_json(&response)?,
        OutputFormat::Text | OutputFormat::Markdown => {
            let ns = response.namespace.as_deref().unwrap_or("<all>");
            output::emit_text(&format!(
                "orphans: {} found, {} deleted (dry_run={}) [{}]",
                response.orphan_count, response.deleted, response.dry_run, ns
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cleanup_response_serializa_dry_run_true() {
        let resp = CleanupResponse {
            orphan_count: 5,
            deleted: 0,
            dry_run: true,
            namespace: Some("global".to_string()),
            elapsed_ms: 12,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["orphan_count"], 5);
        assert_eq!(json["deleted"], 0);
        assert_eq!(json["dry_run"], true);
        assert_eq!(json["namespace"], "global");
        assert!(json["elapsed_ms"].is_number());
    }

    #[test]
    fn cleanup_response_deleted_zero_quando_dry_run() {
        let resp = CleanupResponse {
            orphan_count: 10,
            deleted: 0,
            dry_run: true,
            namespace: None,
            elapsed_ms: 5,
        };
        assert_eq!(resp.deleted, 0, "dry_run deve manter deleted em 0");
        assert_eq!(resp.orphan_count, 10);
    }

    #[test]
    fn cleanup_response_namespace_none_serializa_null() {
        let resp = CleanupResponse {
            orphan_count: 0,
            deleted: 0,
            dry_run: false,
            namespace: None,
            elapsed_ms: 1,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert!(
            json["namespace"].is_null(),
            "namespace None deve serializar como null"
        );
    }

    #[test]
    fn cleanup_response_deleted_igual_orphan_count_quando_executado() {
        let resp = CleanupResponse {
            orphan_count: 3,
            deleted: 3,
            dry_run: false,
            namespace: Some("projeto".to_string()),
            elapsed_ms: 20,
        };
        assert_eq!(
            resp.deleted, resp.orphan_count,
            "ao executar sem dry_run, deleted deve igualar orphan_count"
        );
    }
}
