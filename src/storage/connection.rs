use crate::errors::AppError;
use crate::pragmas::apply_connection_pragmas;
use rusqlite::Connection;
use sqlite_vec::sqlite3_vec_init;
use std::path::Path;

/// Register sqlite-vec GLOBALLY before any connection is opened.
/// Must be called once at program start.
pub fn register_vec_extension() {
    #[allow(clippy::missing_transmute_annotations)]
    unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite3_vec_init as *const (),
        )));
    }
}

pub fn open_rw(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    apply_connection_pragmas(&conn)?;
    apply_secure_permissions(path);
    Ok(conn)
}

pub fn ensure_schema(conn: &mut Connection) -> Result<(), AppError> {
    crate::migrations::runner()
        .run(conn)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("migration failed: {e}")))?;
    conn.execute_batch(&format!(
        "PRAGMA user_version = {};",
        crate::constants::SCHEMA_USER_VERSION
    ))?;
    Ok(())
}

/// Aplica permissões 600 (owner read/write only) ao arquivo SQLite e aos arquivos WAL/SHM
/// associados no Unix para evitar vazamento de memórias privadas em diretórios compartilhados
/// (ex: /tmp multi-user, Dropbox, NFS). No-op em Windows. Falhas silenciosas para não
/// bloquear operação quando o processo não é o dono do arquivo (ex: montagem read-only).
#[allow(unused_variables)]
fn apply_secure_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let candidates = [
            path.to_path_buf(),
            path.with_extension(format!(
                "{}-wal",
                path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("sqlite")
            )),
            path.with_extension(format!(
                "{}-shm",
                path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("sqlite")
            )),
        ];
        for file in candidates.iter() {
            if file.exists() {
                if let Ok(meta) = std::fs::metadata(file) {
                    let mut perms = meta.permissions();
                    perms.set_mode(0o600);
                    let _ = std::fs::set_permissions(file, perms);
                }
            }
        }
    }
}

pub fn open_ro(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    Ok(conn)
}
