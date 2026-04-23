//! Semáforo de contagem via lock files para limitar invocações paralelas do CLI.
//!
//! `acquire_cli_slot` tenta adquirir um dos `N` slots disponíveis abrindo o arquivo
//! `cli-slot-{N}.lock` no diretório de cache do SO e obtendo um `flock` exclusivo.
//! O [`std::fs::File`] retornado DEVE ser mantido vivo durante toda a execução de
//! `main`; descartá-lo libera o slot automaticamente para a próxima invocação.
//!
//! Quando `wait_seconds` é `Some(n) > 0`, a função faz polling a cada
//! [`crate::constants::CLI_LOCK_POLL_INTERVAL_MS`] milissegundos até o deadline. Quando é `None`
//! ou `Some(0)`, uma única tentativa é feita e `Err(AppError::AllSlotsFull)` é
//! retornado imediatamente se todos os slots estiverem ocupados.

use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use directories::ProjectDirs;
use fs4::fs_std::FileExt;

use crate::constants::{CLI_LOCK_POLL_INTERVAL_MS, MAX_CONCURRENT_CLI_INSTANCES};
use crate::errors::AppError;

/// Retorna o caminho do arquivo de lock para o slot indicado.
///
/// Honra `SQLITE_GRAPHRAG_CACHE_DIR` quando definida (útil para testes, containers
/// e caches em NFS), caindo para o diretório de cache padrão do SO via
/// `directories::ProjectDirs`. O slot deve ser 1-based.
fn slot_path(slot: usize) -> Result<PathBuf, AppError> {
    let cache = if let Some(override_dir) = std::env::var_os("SQLITE_GRAPHRAG_CACHE_DIR") {
        PathBuf::from(override_dir)
    } else {
        let dirs = ProjectDirs::from("", "", "sqlite-graphrag").ok_or_else(|| {
            AppError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "não foi possível determinar o diretório de cache para os lock files do sqlite-graphrag",
            ))
        })?;
        dirs.cache_dir().to_path_buf()
    };
    std::fs::create_dir_all(&cache)?;
    Ok(cache.join(format!("cli-slot-{slot}.lock")))
}

/// Tenta abrir e travar exclusivamente o arquivo de lock do slot indicado.
///
/// Retorna `Ok(file)` se o slot estiver livre, ou `Err(io::Error)` se estiver
/// ocupado por outra instância (sem bloquear).
fn try_acquire_slot(slot: usize) -> Result<File, AppError> {
    let path = slot_path(slot)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    file.try_lock_exclusive().map_err(AppError::Io)?;
    Ok(file)
}

/// Adquire um slot de concorrência no semáforo de `max_concurrency` posições.
///
/// Itera os slots `1..=max_concurrency` tentando `try_lock_exclusive` em cada
/// arquivo `cli-slot-N.lock`. Quando encontra um slot livre, retorna
/// `(File, slot_number)`. Se todos os slots estiverem ocupados:
///
/// - Se `wait_seconds` for `None` ou `Some(0)`, retorna imediatamente com
///   `AppError::AllSlotsFull { max, waited_secs: 0 }`.
/// - Se `wait_seconds` for `Some(n) > 0`, entra em loop de polling a cada
///   [`crate::constants::CLI_LOCK_POLL_INTERVAL_MS`] ms até o deadline expirar, retornando
///   `AppError::AllSlotsFull { max, waited_secs: n }` se nenhum slot abrir.
///
/// O `File` retornado DEVE ser mantido vivo até o processo encerrar; descartá-lo
/// libera o slot automaticamente via `flock` implícito no fechamento.
pub fn acquire_cli_slot(
    max_concurrency: usize,
    wait_seconds: Option<u64>,
) -> Result<(File, usize), AppError> {
    let max = max_concurrency.clamp(1, MAX_CONCURRENT_CLI_INSTANCES);
    let wait_secs = wait_seconds.unwrap_or(0);

    // Tentativa inicial sem espera.
    if let Some((file, slot)) = try_any_slot(max)? {
        return Ok((file, slot));
    }

    if wait_secs == 0 {
        return Err(AppError::AllSlotsFull {
            max,
            waited_secs: 0,
        });
    }

    // Loop de polling até o deadline.
    let deadline = Instant::now() + Duration::from_secs(wait_secs);
    loop {
        thread::sleep(Duration::from_millis(CLI_LOCK_POLL_INTERVAL_MS));
        if let Some((file, slot)) = try_any_slot(max)? {
            return Ok((file, slot));
        }
        if Instant::now() >= deadline {
            return Err(AppError::AllSlotsFull {
                max,
                waited_secs: wait_secs,
            });
        }
    }
}

/// Tenta adquirir qualquer slot livre em `1..=max`, retornando o primeiro disponível.
///
/// Retorna `Ok(Some((file, slot)))` se um slot foi obtido, `Ok(None)` se todos
/// estão ocupados (`EWOULDBLOCK`). Propaga erros de I/O distintos de "lock contended".
fn try_any_slot(max: usize) -> Result<Option<(File, usize)>, AppError> {
    for slot in 1..=max {
        match try_acquire_slot(slot) {
            Ok(file) => return Ok(Some((file, slot))),
            Err(AppError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}
