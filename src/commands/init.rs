//! Handler for the `init` CLI subcommand.

use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::pragmas::apply_init_pragmas;
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
    schema_version: String,
    model: String,
    dim: usize,
    /// Active namespace resolved during initialisation, aligned with the bilingual docs.
    namespace: String,
    status: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: InitArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
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
        "Inicializando modelo de embedding (pode baixar na primeira execução)...",
    );

    let test_emb = crate::daemon::embed_passage_or_local(&paths.models, "smoke test")?;

    output::emit_json(&InitResponse {
        db_path: paths.db.display().to_string(),
        schema_version,
        model: "multilingual-e5-small".to_string(),
        dim: test_emb.len(),
        namespace,
        status: "ok".to_string(),
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

fn latest_schema_version(conn: &rusqlite::Connection) -> Result<String, AppError> {
    match conn.query_row(
        "SELECT version FROM refinery_schema_history ORDER BY version DESC LIMIT 1",
        [],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(version) => Ok(version.to_string()),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok("0".to_string()),
        Err(err) => Err(AppError::Database(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_response_serializa_todos_campos() {
        let resp = InitResponse {
            db_path: "/tmp/test.sqlite".to_string(),
            schema_version: "6".to_string(),
            model: "multilingual-e5-small".to_string(),
            dim: 384,
            namespace: "global".to_string(),
            status: "ok".to_string(),
            elapsed_ms: 100,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["db_path"], "/tmp/test.sqlite");
        assert_eq!(json["schema_version"], "6");
        assert_eq!(json["model"], "multilingual-e5-small");
        assert_eq!(json["dim"], 384usize);
        assert_eq!(json["namespace"], "global");
        assert_eq!(json["status"], "ok");
        assert!(json["elapsed_ms"].is_number());
    }

    #[test]
    fn latest_schema_version_retorna_zero_para_banco_vazio() {
        let conn = rusqlite::Connection::open_in_memory().expect("falha ao abrir banco em memória");
        conn.execute_batch("CREATE TABLE refinery_schema_history (version INTEGER NOT NULL);")
            .expect("falha ao criar tabela");

        let versao = latest_schema_version(&conn).expect("latest_schema_version falhou");
        assert_eq!(versao, "0", "banco vazio deve retornar schema_version '0'");
    }

    #[test]
    fn latest_schema_version_retorna_versao_maxima() {
        let conn = rusqlite::Connection::open_in_memory().expect("falha ao abrir banco em memória");
        conn.execute_batch(
            "CREATE TABLE refinery_schema_history (version INTEGER NOT NULL);
             INSERT INTO refinery_schema_history VALUES (1);
             INSERT INTO refinery_schema_history VALUES (3);
             INSERT INTO refinery_schema_history VALUES (2);",
        )
        .expect("falha ao popular tabela");

        let versao = latest_schema_version(&conn).expect("latest_schema_version falhou");
        assert_eq!(versao, "3", "deve retornar a maior versão presente");
    }

    #[test]
    fn init_response_dim_alinhado_com_constante() {
        assert_eq!(
            crate::constants::EMBEDDING_DIM,
            384,
            "dim deve estar alinhado com EMBEDDING_DIM=384"
        );
    }

    #[test]
    fn init_response_namespace_alinhado_com_schema() {
        // Verify namespace field survives round-trip serialization with correct value.
        let resp = InitResponse {
            db_path: "/tmp/x.sqlite".to_string(),
            schema_version: "6".to_string(),
            model: "multilingual-e5-small".to_string(),
            dim: 384,
            namespace: "meu-projeto".to_string(),
            status: "ok".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["namespace"], "meu-projeto");
    }
}
