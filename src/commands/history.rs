use crate::errors::AppError;
use crate::i18n::erros;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use rusqlite::params;
use rusqlite::OptionalExtension;
use serde::Serialize;

#[derive(clap::Args)]
pub struct HistoryArgs {
    /// Memory name whose version history will be returned. Includes soft-deleted memories
    /// so that `restore --version <V>` workflow remains discoverable after `forget`.
    #[arg(long)]
    pub name: String,
    #[arg(long, default_value = "global")]
    pub namespace: Option<String>,
    #[arg(long, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct HistoryVersion {
    version: i64,
    name: String,
    #[serde(rename = "type")]
    memory_type: String,
    description: String,
    body: String,
    metadata: String,
    change_reason: String,
    changed_by: Option<String>,
    created_at: i64,
    created_at_iso: String,
}

#[derive(Serialize)]
struct HistoryResponse {
    name: String,
    namespace: String,
    /// True quando a memória está atualmente soft-deleted (forgotten).
    /// Permite ao usuário descobrir a versão para `restore` mesmo após `forget`.
    deleted: bool,
    versions: Vec<HistoryVersion>,
    /// Tempo total de execução em milissegundos desde início do handler até serialização.
    elapsed_ms: u64,
}

pub fn run(args: HistoryArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    if !paths.db.exists() {
        return Err(AppError::NotFound(erros::banco_nao_encontrado(
            &paths.db.display().to_string(),
        )));
    }
    let conn = open_ro(&paths.db)?;

    // v1.0.22 P0: query direta SEM filtro deleted_at — history DEVE retornar versões
    // de memórias forgotten para que o usuário descubra a versão em `restore`.
    // O find_by_name antigo filtrava deleted_at IS NULL e gerava dead-end no workflow forget+restore.
    let row: Option<(i64, Option<i64>)> = conn
        .query_row(
            "SELECT id, deleted_at FROM memories WHERE namespace = ?1 AND name = ?2",
            params![namespace, args.name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (memory_id, deleted_at) = row
        .ok_or_else(|| AppError::NotFound(erros::memoria_nao_encontrada(&args.name, &namespace)))?;
    let deleted = deleted_at.is_some();

    let mut stmt = conn.prepare(
        "SELECT version, name, type, description, body, metadata,
                change_reason, changed_by, created_at
         FROM memory_versions
         WHERE memory_id = ?1
         ORDER BY version ASC",
    )?;

    let versions = stmt
        .query_map(params![memory_id], |r| {
            let created_at: i64 = r.get(8)?;
            let created_at_iso = crate::tz::epoch_para_iso(created_at);
            Ok(HistoryVersion {
                version: r.get(0)?,
                name: r.get(1)?,
                memory_type: r.get(2)?,
                description: r.get(3)?,
                body: r.get(4)?,
                metadata: r.get(5)?,
                change_reason: r.get(6)?,
                changed_by: r.get(7)?,
                created_at,
                created_at_iso,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    output::emit_json(&HistoryResponse {
        name: args.name,
        namespace,
        deleted,
        versions,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod testes {
    #[test]
    fn epoch_zero_gera_iso_valido() {
        // epoch_para_iso usa chrono-tz com offset explícito (+00:00 para UTC)
        let iso = crate::tz::epoch_para_iso(0);
        assert!(iso.starts_with("1970-01-01T00:00:00"), "obtido: {iso}");
        assert!(iso.contains("00:00"), "deve conter offset, obtido: {iso}");
    }

    #[test]
    fn epoch_tipico_gera_iso_rfc3339() {
        let iso = crate::tz::epoch_para_iso(1_745_000_000);
        assert!(!iso.is_empty(), "created_at_iso não deve ser vazio");
        assert!(iso.contains('T'), "created_at_iso deve conter separador T");
        // Com UTC o offset é +00:00; verifica formato geral sem depender do fuso global
        assert!(
            iso.contains('+') || iso.contains('-'),
            "deve conter sinal de offset, obtido: {iso}"
        );
    }

    #[test]
    fn epoch_invalido_retorna_fallback() {
        let iso = crate::tz::epoch_para_iso(i64::MIN);
        assert!(
            !iso.is_empty(),
            "epoch inválido deve retornar fallback não-vazio"
        );
    }
}
