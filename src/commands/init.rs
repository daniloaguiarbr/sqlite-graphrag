use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::pragmas::apply_init_pragmas;
use crate::storage::connection::open_rw;
use serde::Serialize;

#[derive(clap::Args)]
pub struct InitArgs {
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub force: bool,
    /// Namespace inicial a resolver. Alinhado à documentação bilíngue que prevê `init --namespace`.
    /// Se fornecido, sobrepõe `SQLITE_GRAPHRAG_NAMESPACE`; caso contrário, resolve via env
    /// ou fallback `global`.
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
}

#[derive(Serialize)]
struct InitResponse {
    db_path: String,
    schema_version: String,
    model: String,
    dim: usize,
    /// Namespace ativo resolvido durante a inicialização, alinhado à doc bilíngue.
    namespace: String,
    status: String,
    /// Tempo total de execução em milissegundos desde início do handler até serialização.
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
mod testes {
    use super::*;

    #[test]
    fn init_response_serializa_todos_campos() {
        let resp = InitResponse {
            db_path: "/tmp/test.sqlite".to_string(),
            schema_version: "5".to_string(),
            model: "multilingual-e5-small".to_string(),
            dim: 384,
            namespace: "global".to_string(),
            status: "ok".to_string(),
            elapsed_ms: 100,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["db_path"], "/tmp/test.sqlite");
        assert_eq!(json["schema_version"], "5");
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
}
