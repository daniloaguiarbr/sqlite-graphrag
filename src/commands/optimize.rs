use crate::errors::AppError;
use crate::i18n::erros;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use serde::Serialize;

#[derive(clap::Args)]
pub struct OptimizeArgs {
    #[arg(long, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct OptimizeResponse {
    db_path: String,
    status: String,
    /// Tempo total de execução em milissegundos desde início do handler até serialização.
    elapsed_ms: u64,
}

pub fn run(args: OptimizeArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if !paths.db.exists() {
        return Err(AppError::NotFound(erros::banco_nao_encontrado(
            &paths.db.display().to_string(),
        )));
    }

    let conn = open_rw(&paths.db)?;
    conn.execute_batch("PRAGMA optimize;")?;

    output::emit_json(&OptimizeResponse {
        db_path: paths.db.display().to_string(),
        status: "ok".to_string(),
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    fn optimize_response_serializa_campos_obrigatorios() {
        let resp = OptimizeResponse {
            db_path: "/tmp/graphrag.sqlite".to_string(),
            status: "ok".to_string(),
            elapsed_ms: 5,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["db_path"], "/tmp/graphrag.sqlite");
        assert_eq!(json["elapsed_ms"], 5);
    }

    #[test]
    #[serial]
    fn optimize_retorna_not_found_quando_db_ausente() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("inexistente.sqlite");
        std::env::set_var("SQLITE_GRAPHRAG_DB_PATH", db_path.to_str().unwrap());
        std::env::set_var("LOG_LEVEL", "error");

        let args = OptimizeArgs {
            json: false,
            db: Some(db_path.to_string_lossy().to_string()),
        };
        let resultado = run(args);
        assert!(resultado.is_err(), "deve falhar quando db não existe");
        match resultado.unwrap_err() {
            AppError::NotFound(_) => {}
            outro => unreachable!("esperava NotFound, obteve: {outro:?}"),
        }
        std::env::remove_var("SQLITE_GRAPHRAG_DB_PATH");
        std::env::remove_var("LOG_LEVEL");
    }

    #[test]
    fn optimize_response_status_ok_fixo() {
        let resp = OptimizeResponse {
            db_path: "/qualquer/caminho".to_string(),
            status: "ok".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["status"], "ok", "status deve ser sempre 'ok'");
    }
}
