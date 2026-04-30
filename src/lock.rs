//! Counting semaphore via lock files to limit parallel CLI invocations.
//!
//! `acquire_cli_slot` tries to acquire one of `N` available slots by opening the file
//! `cli-slot-{N}.lock` in the OS cache directory and obtaining an exclusive `flock`.
//! The returned [`std::fs::File`] MUST be kept alive for the entire duration of `main`;
//! dropping it releases the slot automatically for the next invocation.
//!
//! When `wait_seconds` is `Some(n) > 0`, the function polls every
//! [`crate::constants::CLI_LOCK_POLL_INTERVAL_MS`] milliseconds until the deadline. When it
//! is `None` or `Some(0)`, a single attempt is made and `Err(AppError::AllSlotsFull)` is
//! returned immediately if all slots are occupied.

use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use directories::ProjectDirs;
use fs4::fs_std::FileExt;

use crate::constants::{CLI_LOCK_POLL_INTERVAL_MS, MAX_CONCURRENT_CLI_INSTANCES};
use crate::errors::AppError;

/// Returns the lock file path for the given slot.
///
/// Honours `SQLITE_GRAPHRAG_CACHE_DIR` when set (useful for tests, containers,
/// and NFS caches), falling back to the OS default cache directory via
/// `directories::ProjectDirs`. The slot must be 1-based.
fn slot_path(slot: usize) -> Result<PathBuf, AppError> {
    let cache = if let Some(override_dir) = std::env::var_os("SQLITE_GRAPHRAG_CACHE_DIR") {
        PathBuf::from(override_dir)
    } else {
        let dirs = ProjectDirs::from("", "", "sqlite-graphrag").ok_or_else(|| {
            AppError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "could not determine cache directory for sqlite-graphrag lock files",
            ))
        })?;
        dirs.cache_dir().to_path_buf()
    };
    std::fs::create_dir_all(&cache)?;
    Ok(cache.join(format!("cli-slot-{slot}.lock")))
}

/// Tries to open and exclusively lock the lock file for the given slot.
///
/// Returns `Ok(file)` if the slot is free, or `Err(io::Error)` if it is
/// held by another instance (non-blocking).
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

/// Acquires a concurrency slot from the `max_concurrency`-position semaphore.
///
/// Iterates slots `1..=max_concurrency` attempting `try_lock_exclusive` on each
/// `cli-slot-N.lock` file. When a free slot is found, returns `(File, slot_number)`.
/// If all slots are occupied:
///
/// - If `wait_seconds` is `None` or `Some(0)`, returns immediately with
///   `AppError::AllSlotsFull { max, waited_secs: 0 }`.
/// - If `wait_seconds` is `Some(n) > 0`, enters a polling loop every
///   [`crate::constants::CLI_LOCK_POLL_INTERVAL_MS`] ms until the deadline expires, returning
///   `AppError::AllSlotsFull { max, waited_secs: n }` if no slot opens.
///
/// The returned `File` MUST be kept alive until the process exits; dropping it
/// releases the slot automatically via the implicit `flock` on close.
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

    // Polling loop until the deadline.
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

/// Tries to acquire any free slot in `1..=max`, returning the first available one.
///
/// Returns `Ok(Some((file, slot)))` if a slot was obtained, `Ok(None)` if all are
/// occupied (`EWOULDBLOCK`). Propagates I/O errors other than "lock contended".
fn try_any_slot(max: usize) -> Result<Option<(File, usize)>, AppError> {
    for slot in 1..=max {
        match try_acquire_slot(slot) {
            Ok(file) => return Ok(Some((file, slot))),
            Err(AppError::Io(e)) if is_lock_contended(&e) => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}

fn is_lock_contended(error: &std::io::Error) -> bool {
    if error.kind() == std::io::ErrorKind::WouldBlock {
        return true;
    }

    #[cfg(windows)]
    {
        matches!(error.raw_os_error(), Some(32 | 33))
    }

    #[cfg(not(windows))]
    {
        false
    }
}
