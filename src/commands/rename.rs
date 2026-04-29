//! Handler for the `rename` CLI subcommand.

use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output;
use crate::output::JsonOutputFormat;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::{memories, versions};
use serde::Serialize;

#[derive(clap::Args)]
pub struct RenameArgs {
    /// Current memory name as a positional argument. Alternative to `--name` / `--old`.
    #[arg(value_name = "NAME", conflicts_with = "name")]
    pub name_positional: Option<String>,
    /// Current memory name. Also accepts the alias `--old`.
    #[arg(long, alias = "old")]
    pub name: Option<String>,
    /// New memory name. Also accepts the alias `--new`.
    #[arg(long, alias = "new")]
    pub new_name: String,
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
    /// Optional session ID used to trace the origin of the change.
    #[arg(long, value_name = "UUID")]
    pub session_id: Option<String>,
    /// Output format.
    #[arg(long, value_enum, default_value_t = JsonOutputFormat::Json)]
    pub format: JsonOutputFormat,
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
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: RenameArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let _ = args.format;
    use crate::constants::*;

    // Resolve current name from positional or --name/--old flag.
    let name = args.name_positional.or(args.name).ok_or_else(|| {
        AppError::Validation("name required: pass as positional argument or via --name".to_string())
    })?;
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;

    // v1.0.20: trim_matches('-') também remove hífens trailing/leading.
    let normalized_new_name = {
        let lower = args.new_name.to_lowercase().replace(['_', ' '], "-");
        let trimmed = lower.trim_matches('-').to_string();
        if trimmed != args.new_name {
            tracing::warn!(
                original = %args.new_name,
                normalized = %trimmed,
                "new_name auto-normalized to kebab-case"
            );
        }
        trimmed
    };

    if normalized_new_name.starts_with("__") {
        return Err(AppError::Validation(
            crate::i18n::validation::reserved_name(),
        ));
    }

    if normalized_new_name.is_empty() || normalized_new_name.len() > MAX_MEMORY_NAME_LEN {
        return Err(AppError::Validation(
            crate::i18n::validation::new_name_length(MAX_MEMORY_NAME_LEN),
        ));
    }

    {
        let slug_re = regex::Regex::new(crate::constants::NAME_SLUG_REGEX)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("regex: {e}")))?;
        if !slug_re.is_match(&normalized_new_name) {
            return Err(AppError::Validation(
                crate::i18n::validation::new_name_kebab(&normalized_new_name),
            ));
        }
    }

    let paths = AppPaths::resolve(args.db.as_deref())?;
    if !paths.db.exists() {
        return Err(AppError::NotFound(errors_msg::database_not_found(
            &paths.db.display().to_string(),
        )));
    }
    let mut conn = open_rw(&paths.db)?;

    let (memory_id, current_updated_at, _) = memories::find_by_name(&conn, &namespace, &name)?
        .ok_or_else(|| AppError::NotFound(errors_msg::memory_not_found(&name, &namespace)))?;

    if let Some(expected) = args.expected_updated_at {
        if expected != current_updated_at {
            return Err(AppError::Conflict(errors_msg::optimistic_lock_conflict(
                expected,
                current_updated_at,
            )));
        }
    }

    let row = memories::read_by_name(&conn, &namespace, &name)?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("memory not found before rename")))?;

    let memory_type = row.memory_type.clone();
    let description = row.description.clone();
    let body = row.body.clone();
    let metadata = row.metadata.clone();

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    let affected = if let Some(ts) = args.expected_updated_at {
        tx.execute(
            "UPDATE memories SET name=?2 WHERE id=?1 AND updated_at=?3 AND deleted_at IS NULL",
            rusqlite::params![memory_id, normalized_new_name, ts],
        )?
    } else {
        tx.execute(
            "UPDATE memories SET name=?2 WHERE id=?1 AND deleted_at IS NULL",
            rusqlite::params![memory_id, normalized_new_name],
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
        &normalized_new_name,
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
        name: normalized_new_name,
        action: "renamed",
        version: next_v,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
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
