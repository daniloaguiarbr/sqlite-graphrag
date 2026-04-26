use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use rusqlite::OptionalExtension;
use serde::Serialize;

#[derive(clap::Args)]
pub struct MigrateArgs {
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// Explicit JSON flag. Accepted as a no-op because output is already JSON by default.
    #[arg(long, default_value_t = false)]
    pub json: bool,
    /// Show already applied migrations without applying new ones.
    #[arg(long, default_value_t = false)]
    pub status: bool,
}

#[derive(Serialize)]
struct MigrateResponse {
    db_path: String,
    schema_version: String,
    status: String,
    /// Tempo total de execução em milissegundos desde início do handler até serialização.
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct MigrateStatusResponse {
    db_path: String,
    applied_migrations: Vec<MigrationEntry>,
    schema_version: String,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct MigrationEntry {
    version: i64,
    name: String,
    applied_on: Option<String>,
}

pub fn run(args: MigrateArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let _ = args.json; // --json é no-op pois output já é JSON por default
    let paths = AppPaths::resolve(args.db.as_deref())?;
    paths.ensure_dirs()?;

    let mut conn = open_rw(&paths.db)?;

    if args.status {
        let schema_version = latest_schema_version(&conn).unwrap_or_else(|_| "0".to_string());
        let applied = list_applied_migrations(&conn)?;
        output::emit_json(&MigrateStatusResponse {
            db_path: paths.db.display().to_string(),
            applied_migrations: applied,
            schema_version,
            elapsed_ms: inicio.elapsed().as_millis() as u64,
        })?;
        return Ok(());
    }

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

    output::emit_json(&MigrateResponse {
        db_path: paths.db.display().to_string(),
        schema_version,
        status: "ok".to_string(),
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

fn list_applied_migrations(conn: &rusqlite::Connection) -> Result<Vec<MigrationEntry>, AppError> {
    let table_exists: Option<String> = conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='refinery_schema_history'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if table_exists.is_none() {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(
        "SELECT version, name, applied_on FROM refinery_schema_history ORDER BY version ASC",
    )?;
    let entries = stmt
        .query_map([], |r| {
            Ok(MigrationEntry {
                version: r.get(0)?,
                name: r.get(1)?,
                applied_on: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
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
    use rusqlite::Connection;

    fn cria_db_sem_historico() -> Connection {
        Connection::open_in_memory().expect("falha ao abrir banco em memória")
    }

    fn cria_db_com_historico(versao: i64) -> Connection {
        let conn = Connection::open_in_memory().expect("falha ao abrir banco em memória");
        conn.execute_batch(
            "CREATE TABLE refinery_schema_history (
                version INTEGER NOT NULL,
                name TEXT,
                applied_on TEXT,
                checksum TEXT
            );",
        )
        .expect("falha ao criar tabela de histórico");
        conn.execute(
            "INSERT INTO refinery_schema_history (version, name) VALUES (?1, 'V001__init')",
            rusqlite::params![versao],
        )
        .expect("falha ao inserir versão");
        conn
    }

    #[test]
    fn latest_schema_version_retorna_erro_sem_tabela() {
        let conn = cria_db_sem_historico();
        // Sem tabela refinery_schema_history, SQLite retorna Unknown (código 1) → AppError::Database
        let resultado = latest_schema_version(&conn);
        assert!(
            resultado.is_err(),
            "deve retornar Err quando tabela não existe"
        );
    }

    #[test]
    fn latest_schema_version_retorna_versao_maxima() {
        let conn = cria_db_com_historico(6);
        let version = latest_schema_version(&conn).unwrap();
        assert_eq!(version, "6");
    }

    #[test]
    fn migrate_response_serializa_campos_obrigatorios() {
        let resp = MigrateResponse {
            db_path: "/tmp/test.sqlite".to_string(),
            schema_version: "6".to_string(),
            status: "ok".to_string(),
            elapsed_ms: 12,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["schema_version"], "6");
        assert_eq!(json["db_path"], "/tmp/test.sqlite");
        assert_eq!(json["elapsed_ms"], 12);
    }

    #[test]
    fn latest_schema_version_retorna_zero_quando_tabela_vazia() {
        let conn = Connection::open_in_memory().expect("banco em memória");
        conn.execute_batch(
            "CREATE TABLE refinery_schema_history (
                version INTEGER NOT NULL,
                name TEXT
            );",
        )
        .expect("criação da tabela");
        // Tabela existe mas está vazia → QueryReturnedNoRows → "0"
        let version = latest_schema_version(&conn).unwrap();
        assert_eq!(version, "0");
    }
}
