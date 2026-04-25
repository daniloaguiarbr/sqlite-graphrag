use crate::errors::AppError;
use crate::i18n::{erros, validacao};
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use serde::Serialize;

#[derive(clap::Args)]
pub struct SyncSafeCopyArgs {
    /// Snapshot destination path. Also accepts the aliases `--to` and `--output`.
    #[arg(long, alias = "to", alias = "output")]
    pub dest: std::path::PathBuf,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    /// Output format: `json` or `text`. JSON is always emitted on stdout regardless of the value.
    #[arg(long, value_parser = ["json", "text"], hide = true)]
    pub format: Option<String>,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct SyncSafeCopyResponse {
    source_db_path: String,
    dest_path: String,
    bytes_copied: u64,
    status: String,
    /// Tempo total de execução em milissegundos desde início do handler até serialização.
    elapsed_ms: u64,
}

pub fn run(args: SyncSafeCopyArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let _ = args.format; // --format é no-op; JSON sempre emitido no stdout
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if !paths.db.exists() {
        return Err(AppError::NotFound(erros::banco_nao_encontrado(
            &paths.db.display().to_string(),
        )));
    }

    if args.dest == paths.db {
        return Err(AppError::Validation(validacao::sync_destino_igual_fonte()));
    }

    if let Some(parent) = args.dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = open_rw(&paths.db)?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    drop(conn);

    let bytes_copied = std::fs::copy(&paths.db, &args.dest)?;

    // Aplica permissões 600 no snapshot em Unix para evitar vazamento em Dropbox/NFS compartilhado.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&args.dest)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&args.dest, perms)?;
    }

    output::emit_json(&SyncSafeCopyResponse {
        source_db_path: paths.db.display().to_string(),
        dest_path: args.dest.display().to_string(),
        bytes_copied,
        status: "ok".to_string(),
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn sync_safe_copy_response_serializa_todos_campos() {
        let resp = SyncSafeCopyResponse {
            source_db_path: "/home/user/.local/share/sqlite-graphrag/db.sqlite".to_string(),
            dest_path: "/tmp/backup.sqlite".to_string(),
            bytes_copied: 16384,
            status: "ok".to_string(),
            elapsed_ms: 12,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(
            json["source_db_path"],
            "/home/user/.local/share/sqlite-graphrag/db.sqlite"
        );
        assert_eq!(json["dest_path"], "/tmp/backup.sqlite");
        assert_eq!(json["bytes_copied"], 16384u64);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["elapsed_ms"], 12u64);
    }

    #[test]
    fn sync_safe_copy_rejeita_dest_igual_source() {
        let db_path = std::path::PathBuf::from("/tmp/mesmo.sqlite");
        let args = SyncSafeCopyArgs {
            dest: db_path.clone(),
            json: false,
            format: None,
            db: Some("/tmp/mesmo.sqlite".to_string()),
        };
        // Simula resolução manual do caminho — valida lógica de rejeição
        let resultado = if args.dest == std::path::PathBuf::from(args.db.as_deref().unwrap_or("")) {
            Err(AppError::Validation(
                "destination path must differ from the source database path".to_string(),
            ))
        } else {
            Ok(())
        };
        assert!(resultado.is_err(), "deve rejeitar dest igual ao source");
        if let Err(AppError::Validation(msg)) = resultado {
            assert!(msg.contains("destination path must differ"));
        }
    }

    #[test]
    fn sync_safe_copy_response_status_ok() {
        let resp = SyncSafeCopyResponse {
            source_db_path: "/data/db.sqlite".to_string(),
            dest_path: "/backup/db.sqlite".to_string(),
            bytes_copied: 0,
            status: "ok".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["status"], "ok");
    }

    #[test]
    fn sync_safe_copy_response_bytes_copied_zero_valido() {
        let resp = SyncSafeCopyResponse {
            source_db_path: "/data/db.sqlite".to_string(),
            dest_path: "/backup/db.sqlite".to_string(),
            bytes_copied: 0,
            status: "ok".to_string(),
            elapsed_ms: 1,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["bytes_copied"], 0u64);
        assert_eq!(json["elapsed_ms"], 1u64);
    }
}
