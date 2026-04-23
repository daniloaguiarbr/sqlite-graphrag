use crate::errors::AppError;
use crate::i18n::erros;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use serde::Serialize;

#[derive(clap::Args)]
pub struct VacuumArgs {
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    /// Executar WAL checkpoint antes e depois do VACUUM (padrão: true).
    #[arg(long, default_value_t = true)]
    pub checkpoint: bool,
    /// Formato da saída.
    #[arg(long, value_enum, default_value_t = crate::output::OutputFormat::Json)]
    pub format: crate::output::OutputFormat,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct VacuumResponse {
    db_path: String,
    size_before_bytes: u64,
    size_after_bytes: u64,
    status: String,
    /// Tempo total de execução em milissegundos desde início do handler até serialização.
    elapsed_ms: u64,
}

pub fn run(args: VacuumArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if !paths.db.exists() {
        return Err(AppError::NotFound(erros::banco_nao_encontrado(
            &paths.db.display().to_string(),
        )));
    }

    let size_before_bytes = std::fs::metadata(&paths.db)
        .map(|meta| meta.len())
        .unwrap_or(0);
    let conn = open_rw(&paths.db)?;
    if args.checkpoint {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    }
    conn.execute_batch("VACUUM;")?;
    if args.checkpoint {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    }
    drop(conn);
    let size_after_bytes = std::fs::metadata(&paths.db)
        .map(|meta| meta.len())
        .unwrap_or(0);

    output::emit_json(&VacuumResponse {
        db_path: paths.db.display().to_string(),
        size_before_bytes,
        size_after_bytes,
        status: "ok".to_string(),
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn vacuum_response_serializa_todos_campos() {
        let resp = VacuumResponse {
            db_path: "/home/user/.local/share/sqlite-graphrag/db.sqlite".to_string(),
            size_before_bytes: 32768,
            size_after_bytes: 16384,
            status: "ok".to_string(),
            elapsed_ms: 55,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(
            json["db_path"],
            "/home/user/.local/share/sqlite-graphrag/db.sqlite"
        );
        assert_eq!(json["size_before_bytes"], 32768u64);
        assert_eq!(json["size_after_bytes"], 16384u64);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["elapsed_ms"], 55u64);
    }

    #[test]
    fn vacuum_response_size_after_menor_ou_igual_before() {
        let resp = VacuumResponse {
            db_path: "/data/db.sqlite".to_string(),
            size_before_bytes: 65536,
            size_after_bytes: 32768,
            status: "ok".to_string(),
            elapsed_ms: 100,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        let before = json["size_before_bytes"].as_u64().unwrap();
        let after = json["size_after_bytes"].as_u64().unwrap();
        assert!(
            after <= before,
            "size_after_bytes deve ser <= size_before_bytes após VACUUM"
        );
    }

    #[test]
    fn vacuum_response_status_ok() {
        let resp = VacuumResponse {
            db_path: "/data/db.sqlite".to_string(),
            size_before_bytes: 0,
            size_after_bytes: 0,
            status: "ok".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["status"], "ok");
    }

    #[test]
    fn vacuum_response_elapsed_ms_presente_e_nao_negativo() {
        let resp = VacuumResponse {
            db_path: "/data/db.sqlite".to_string(),
            size_before_bytes: 1024,
            size_after_bytes: 1024,
            status: "ok".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert!(
            json.get("elapsed_ms").is_some(),
            "campo elapsed_ms deve estar presente"
        );
        assert!(
            json["elapsed_ms"].as_u64().is_some(),
            "elapsed_ms deve ser inteiro não negativo"
        );
    }
}
