//! Handler for the `history` CLI subcommand.

use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use rusqlite::params;
use rusqlite::OptionalExtension;
use serde::Serialize;

#[derive(clap::Args)]
pub struct HistoryArgs {
    /// Memory name as a positional argument. Alternative to `--name`.
    #[arg(value_name = "NAME", conflicts_with = "name")]
    pub name_positional: Option<String>,
    /// Memory name whose version history will be returned. Includes soft-deleted memories
    /// so that `restore --version <V>` workflow remains discoverable after `forget`.
    #[arg(long)]
    pub name: Option<String>,
    /// Namespace to query history from. Defaults to "global".
    #[arg(long, default_value = "global", help = "Namespace to query")]
    pub namespace: Option<String>,
    /// Omit body content from each version to reduce response size.
    #[arg(
        long,
        default_value_t = false,
        help = "Omit body content from response"
    )]
    pub no_body: bool,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    /// Path to graphrag.sqlite (overrides SQLITE_GRAPHRAG_DB_PATH and default CWD).
    #[arg(
        long,
        env = "SQLITE_GRAPHRAG_DB_PATH",
        help = "Path to graphrag.sqlite"
    )]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct HistoryVersion {
    version: i64,
    name: String,
    #[serde(rename = "type")]
    memory_type: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    metadata: serde_json::Value,
    change_reason: String,
    changed_by: Option<String>,
    created_at: i64,
    created_at_iso: String,
}

#[derive(Serialize)]
struct HistoryResponse {
    name: String,
    namespace: String,
    /// True when the memory is currently soft-deleted (forgotten).
    /// Allows the user to discover the version for `restore` even after `forget`.
    deleted: bool,
    versions: Vec<HistoryVersion>,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: HistoryArgs) -> Result<(), AppError> {
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
    let conn = open_ro(&paths.db)?;

    // v1.0.22 P0: query direta SEM filtro deleted_at — history DEVE retornar versões
    // de memórias forgotten para que o usuário descubra a versão em `restore`.
    // O find_by_name antigo filtrava deleted_at IS NULL e gerava dead-end no workflow forget+restore.
    let row: Option<(i64, Option<i64>)> = conn
        .query_row(
            "SELECT id, deleted_at FROM memories WHERE namespace = ?1 AND name = ?2",
            params![namespace, name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (memory_id, deleted_at) =
        row.ok_or_else(|| AppError::NotFound(errors_msg::memory_not_found(&name, &namespace)))?;
    let deleted = deleted_at.is_some();

    let mut stmt = conn.prepare(
        "SELECT version, name, type, description, body, metadata,
                change_reason, changed_by, created_at
         FROM memory_versions
         WHERE memory_id = ?1
         ORDER BY version ASC",
    )?;

    let no_body = args.no_body;
    let versions = stmt
        .query_map(params![memory_id], |r| {
            let created_at: i64 = r.get(8)?;
            let created_at_iso = crate::tz::epoch_to_iso(created_at);
            let body_str: String = r.get(4)?;
            let metadata_str: String = r.get(5)?;
            let metadata_value: serde_json::Value = serde_json::from_str(&metadata_str)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            Ok(HistoryVersion {
                version: r.get(0)?,
                name: r.get(1)?,
                memory_type: r.get(2)?,
                description: r.get(3)?,
                body: if no_body { None } else { Some(body_str) },
                metadata: metadata_value,
                change_reason: r.get(6)?,
                changed_by: r.get(7)?,
                created_at,
                created_at_iso,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    output::emit_json(&HistoryResponse {
        name,
        namespace,
        deleted,
        versions,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn epoch_zero_yields_valid_iso() {
        // epoch_to_iso uses chrono-tz with explicit offset (+00:00 for UTC)
        let iso = crate::tz::epoch_to_iso(0);
        assert!(iso.starts_with("1970-01-01T00:00:00"), "obtido: {iso}");
        assert!(iso.contains("00:00"), "deve conter offset, obtido: {iso}");
    }

    #[test]
    fn typical_epoch_yields_iso_rfc3339() {
        let iso = crate::tz::epoch_to_iso(1_745_000_000);
        assert!(!iso.is_empty(), "created_at_iso não deve ser vazio");
        assert!(iso.contains('T'), "created_at_iso deve conter separador T");
        // Com UTC o offset é +00:00; verifica formato geral sem depender do fuso global
        assert!(
            iso.contains('+') || iso.contains('-'),
            "deve conter sinal de offset, obtido: {iso}"
        );
    }

    #[test]
    fn invalid_epoch_returns_fallback() {
        let iso = crate::tz::epoch_to_iso(i64::MIN);
        assert!(
            !iso.is_empty(),
            "epoch inválido deve retornar fallback não-vazio"
        );
    }
}
