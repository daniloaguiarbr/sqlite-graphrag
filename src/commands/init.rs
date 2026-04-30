//! Handler for the `init` CLI subcommand.

use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::pragmas::{apply_init_pragmas, ensure_wal_mode};
use crate::storage::connection::open_rw;
use serde::Serialize;

/// Embedding model choices exposed through `--model`.
///
/// Currently only `multilingual-e5-small` is supported. Additional variants
/// will be added here as new models are integrated; the `value_enum` derive
/// ensures the CLI rejects unknown strings at parse time rather than at runtime.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum EmbeddingModelChoice {
    #[value(name = "multilingual-e5-small")]
    MultilingualE5Small,
}

#[derive(clap::Args)]
pub struct InitArgs {
    /// Path to graphrag.sqlite. Defaults to `./graphrag.sqlite` in the current directory.
    /// Resolution precedence (highest to lowest): `--db` flag > `SQLITE_GRAPHRAG_DB_PATH` env >
    /// `SQLITE_GRAPHRAG_HOME` env (used as base directory) > cwd.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// Embedding model identifier. Currently only `multilingual-e5-small` is supported.
    /// Reserved for future multi-model support; safe to omit.
    #[arg(long, value_enum)]
    pub model: Option<EmbeddingModelChoice>,
    /// Force re-initialization, overwriting any existing schema metadata.
    /// Use only when the schema is corrupted; loses configuration but preserves data.
    #[arg(long)]
    pub force: bool,
    /// Initial namespace to resolve. Aligned with bilingual docs that mention `init --namespace`.
    /// When provided, overrides `SQLITE_GRAPHRAG_NAMESPACE`; otherwise resolves via env or fallback `global`.
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
}

#[derive(Serialize)]
struct InitResponse {
    db_path: String,
    /// Latest applied migration number from `refinery_schema_history`.
    /// Emitted as a JSON number for cross-command consistency with `health` and `stats` (since v1.0.35).
    schema_version: u32,
    model: String,
    dim: usize,
    /// Active namespace resolved during initialisation, aligned with the bilingual docs.
    namespace: String,
    status: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: InitArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;
    paths.ensure_dirs()?;

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;

    let mut conn = open_rw(&paths.db)?;

    apply_init_pragmas(&conn)?;

    crate::migrations::runner()
        .run(&mut conn)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("migration failed: {e}")))?;

    conn.execute_batch(&format!(
        "PRAGMA user_version = {};",
        crate::constants::SCHEMA_USER_VERSION
    ))?;

    // Defensive re-assertion: refinery may revert journal_mode during migrations.
    ensure_wal_mode(&conn)?;

    let schema_version = latest_schema_version(&conn)?;

    conn.execute(
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('schema_version', ?1)",
        rusqlite::params![schema_version],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('model', 'multilingual-e5-small')",
        [],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('dim', '384')",
        [],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('created_at', CAST(unixepoch() AS TEXT))",
        [],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('sqlite-graphrag_version', ?1)",
        rusqlite::params![crate::constants::SQLITE_GRAPHRAG_VERSION],
    )?;
    // Persist the resolved namespace so downstream tools can inspect it without re-resolving.
    conn.execute(
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('namespace_initial', ?1)",
        rusqlite::params![namespace],
    )?;

    output::emit_progress_i18n(
        "Initializing embedding model (may download on first run)...",
        crate::i18n::validation::runtime_pt::initializing_embedding_model(),
    );

    let test_emb = crate::daemon::embed_passage_or_local(&paths.models, "smoke test")?;

    output::emit_json(&InitResponse {
        db_path: paths.db.display().to_string(),
        schema_version,
        model: "multilingual-e5-small".to_string(),
        dim: test_emb.len(),
        namespace,
        status: "ok".to_string(),
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

fn latest_schema_version(conn: &rusqlite::Connection) -> Result<u32, AppError> {
    match conn.query_row(
        "SELECT version FROM refinery_schema_history ORDER BY version DESC LIMIT 1",
        [],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(version) => Ok(version.max(0) as u32),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
        Err(err) => Err(AppError::Database(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_response_serializes_all_fields() {
        let resp = InitResponse {
            db_path: "/tmp/test.sqlite".to_string(),
            schema_version: 6,
            model: "multilingual-e5-small".to_string(),
            dim: 384,
            namespace: "global".to_string(),
            status: "ok".to_string(),
            elapsed_ms: 100,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["db_path"], "/tmp/test.sqlite");
        assert_eq!(json["schema_version"], 6);
        assert_eq!(json["model"], "multilingual-e5-small");
        assert_eq!(json["dim"], 384usize);
        assert_eq!(json["namespace"], "global");
        assert_eq!(json["status"], "ok");
        assert!(json["elapsed_ms"].is_number());
    }

    #[test]
    fn latest_schema_version_returns_zero_for_empty_db() {
        let conn = rusqlite::Connection::open_in_memory().expect("failed to open in-memory db");
        conn.execute_batch("CREATE TABLE refinery_schema_history (version INTEGER NOT NULL);")
            .expect("failed to create table");

        let version = latest_schema_version(&conn).expect("latest_schema_version failed");
        assert_eq!(version, 0u32, "empty db must return schema_version 0");
    }

    #[test]
    fn latest_schema_version_returns_max_version() {
        let conn = rusqlite::Connection::open_in_memory().expect("failed to open in-memory db");
        conn.execute_batch(
            "CREATE TABLE refinery_schema_history (version INTEGER NOT NULL);
             INSERT INTO refinery_schema_history VALUES (1);
             INSERT INTO refinery_schema_history VALUES (3);
             INSERT INTO refinery_schema_history VALUES (2);",
        )
        .expect("failed to populate table");

        let version = latest_schema_version(&conn).expect("latest_schema_version failed");
        assert_eq!(version, 3u32, "must return the highest version present");
    }

    #[test]
    fn init_response_dim_aligned_with_constant() {
        assert_eq!(
            crate::constants::EMBEDDING_DIM,
            384,
            "dim must be aligned with EMBEDDING_DIM=384"
        );
    }

    #[test]
    fn init_response_namespace_aligned_with_schema() {
        // Verify namespace field survives round-trip serialization with correct value.
        let resp = InitResponse {
            db_path: "/tmp/x.sqlite".to_string(),
            schema_version: 6,
            model: "multilingual-e5-small".to_string(),
            dim: 384,
            namespace: "my-project".to_string(),
            status: "ok".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["namespace"], "my-project");
    }
}
