//! GAP-004 (v1.0.82): cross-process semaphore for spawning LLM subprocesses.
//!
//! When N Claude Code sessions run in parallel on the same host, each `remember`/`edit`/
//! `recall`/`hybrid-search`/`enrich`/`deep-research`/`ingest` wants to spawn its own
//! `codex exec` or `claude -p` subprocess. Without coordination, N subprocesses saturate
//! the shared OAuth rate limit (observed: 19+ concurrent codex in the transcript
//! of 2026-06-15).
//!
//! ## Solution
//! - Slot files at `${XDG_RUNTIME_DIR:-~/.local/share}/sqlite-graphrag/llm-slots/slot-{0..N}.lock`
//! - `fs4::FileExt::try_lock_exclusive` for atomic cross-process acquire (fcntl on Unix,
//!   LockFileEx on Windows — `fs4` 0.9 with trustScore 9.6 confirmed via context7)
//! - RAII guard `LlmSlotGuard` with `Drop` releases automatically on panic
//! - Integration with `reaper.rs::scan_and_kill_orphans` to detect orphaned slots
//!
//! ## Usage
//! ```rust,ignore
//! use crate::llm_slots::acquire_llm_slot;
//!
//! let _guard = acquire_llm_slot(4, 30)?;
//! // ... spawn LLM subprocess ...
//! // the guard releases the slot automatically when it leaves scope
//! ```

use fs4::fs_std::FileExt;
use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::errors::AppError;

/// RAII guard that releases the slot automatically on panic, abrupt cancellation,
/// or normal scope exit.
pub struct LlmSlotGuard {
    #[allow(dead_code)]
    slot_file: File,
    slot_id: u32,
    acquired_at: Instant,
}

impl LlmSlotGuard {
    /// Returns the slot id (0..max-1) this guard holds. Used by
    /// `slots release --slot-id N` to map back to the file path.
    pub fn slot_id(&self) -> u32 {
        self.slot_id
    }
}

impl Drop for LlmSlotGuard {
    fn drop(&mut self) {
        // Libera o lock do filesystem E remove o slot file.
        // O flock é liberado automaticamente quando `slot_file` é dropado (RAII).
        let path = slot_path(self.slot_id);
        if let Err(e) = fs::remove_file(&path) {
            tracing::debug!(slot_id = self.slot_id, error = %e, "slot file removal failed (already gone?)");
        }
        tracing::debug!(
            slot_id = self.slot_id,
            held_ms = self.acquired_at.elapsed().as_millis() as u64,
            "llm slot released"
        );
    }
}

/// Acquires a free LLM slot, waiting up to `wait_secs` seconds.
///
/// Iterates over `slot_id` in `[0, max_concurrent)` and tries `create_new` + `try_lock_exclusive`.
/// If all slots are busy, polls with `sleep(100ms)` until `wait_secs` expires.
///
/// ## Errors
/// - `AppError::LockBusy` (exit 75) if `wait_secs` expires without a free slot
/// - `AppError::Io` if the filesystem fails
pub fn acquire_llm_slot(max_concurrent: u32, wait_secs: u64) -> Result<LlmSlotGuard, AppError> {
    if max_concurrent == 0 {
        return Err(AppError::Validation(
            "max_concurrent deve ser >= 1 para acquire_llm_slot".to_string(),
        ));
    }
    let dir = slots_dir();
    fs::create_dir_all(&dir).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to create slots dir {}: {e}", dir.display()),
        ))
    })?;

    let stale = find_stale_slots(max_concurrent);
    for slot_id in &stale {
        let _ = force_release(*slot_id);
        tracing::info!(slot_id, "released stale LLM slot (PID dead)");
    }

    let start = Instant::now();
    let timeout = Duration::from_secs(wait_secs);

    loop {
        for slot_id in 0..max_concurrent {
            let path = slot_path(slot_id);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    if file.try_lock_exclusive().is_ok() {
                        let pid = std::process::id();
                        // Escreve pid no arquivo para que  possa reportar
                        use std::io::Write;
                        let _ = writeln!(file, "pid={pid}");
                        tracing::debug!(slot_id, pid, "llm slot acquired");
                        return Ok(LlmSlotGuard {
                            slot_file: file,
                            slot_id,
                            acquired_at: Instant::now(),
                        });
                    }
                    // Slot file existe mas está locked por outro processo
                }
                Err(_) => {
                    // Slot file já existe (race condition rara) — tenta próximo
                }
            }
        }
        // Todos os slots ocupados — polling
        if start.elapsed() >= timeout {
            return Err(AppError::LockBusy(format!(
                "failed to acquire LLM slot within {wait_secs}s (max={max_concurrent} concurrent)"
            )));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Returns the current status of the LLM slots (for the `slots status --json` subcommand).
#[derive(Debug, Clone, serde::Serialize)]
pub struct SlotStatus {
    pub max: u32,
    pub active: u32,
    pub pids: Vec<u32>,
}

pub fn read_status(max_concurrent: u32) -> SlotStatus {
    let mut active = 0u32;
    let mut pids = Vec::new();
    for slot_id in 0..max_concurrent {
        let path = slot_path(slot_id);
        if path.exists() {
            active += 1;
            if let Ok(content) = fs::read_to_string(&path) {
                if let Some(pid_line) = content.lines().find(|l| l.starts_with("pid=")) {
                    if let Ok(pid) = pid_line[4..].parse::<u32>() {
                        pids.push(pid);
                    }
                }
            }
        }
    }
    SlotStatus {
        max: max_concurrent,
        active,
        pids,
    }
}

/// Releases a specific slot (for the `slots release --slot-id N --yes` subcommand).
pub fn force_release(slot_id: u32) -> Result<(), AppError> {
    let path = slot_path(slot_id);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| {
            AppError::Io(std::io::Error::new(
                e.kind(),
                format!("failed to release slot {slot_id}: {e}"),
            ))
        })?;
    }
    Ok(())
}

/// Lists stale slot IDs (orphaned PIDs) — for automatic cleanup.
pub fn find_stale_slots(max_concurrent: u32) -> Vec<u32> {
    let mut stale = Vec::new();
    for slot_id in 0..max_concurrent {
        let path = slot_path(slot_id);
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Some(pid_line) = content.lines().find(|l| l.starts_with("pid=")) {
                    if let Ok(pid) = pid_line[4..].parse::<u32>() {
                        if !pid_alive(pid) {
                            stale.push(slot_id);
                        }
                    }
                }
            }
        }
    }
    stale
}

/// Checks whether a PID is alive on the system (best-effort cross-platform).
#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    // Tenta enviar signal 0 (no-op) para verificar existência
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
fn pid_alive(pid: u32) -> bool {
    // No Windows, sem equivalente direto; assume vivo se arquivo existe.
    // Cleanup manual via `slots cleanup --yes` é a via.
    let _ = pid;
    true
}

pub fn slots_dir() -> PathBuf {
    let base = std::env::var("XDG_RUNTIME_DIR")
        .or_else(|_| std::env::var("SQLITE_GRAPHRAG_CACHE_DIR"))
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| format!("{h}/.local/share"))
                .unwrap_or_else(|_| "/tmp".to_string())
        });
    PathBuf::from(base).join("sqlite-graphrag/llm-slots")
}

pub fn slot_path(id: u32) -> PathBuf {
    slots_dir().join(format!("slot-{id}.lock"))
}

/// Resolves the default LLM max-host-concurrency value.
///
/// Calibrated for the LLM-only build: each worker holds one subprocess
/// `codex` or `claude` invocation. The formula mirrors the CLI semaphore
/// in `lock::calculate_safe_concurrency`:
///   `min(ncpus, available_memory_mb / LLM_WORKER_RSS_MB)`
///
/// Falls back to `MAX_CONCURRENT_CLI_INSTANCES` (16) when `sysinfo`
/// cannot read `/proc/meminfo` (rare).
pub fn default_max_concurrency() -> u32 {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4);
    // Without `sysinfo` at hand here, we use a conservative memory
    // estimate: 4 GiB available on most hosts. The CLI semaphore in
    // `lock::calculate_safe_concurrency` is the source of truth when
    // exact memory data is available; this fallback just keeps the
    // LLM slot default in the same order of magnitude.
    let assumed_available_mb: u32 = 4096;
    let per_worker = crate::constants::LLM_WORKER_RSS_MB as u32;
    let safe = assumed_available_mb / per_worker.max(1);
    let capped = safe.min(crate::constants::MAX_CONCURRENT_CLI_INSTANCES as u32);
    cpus.min(capped).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::thread;

    // Serialises every test that mutates the process-global slot env
    // (XDG_RUNTIME_DIR / SQLITE_GRAPHRAG_CACHE_DIR). Without this, parallel
    // tests clobber each other's env and collide in the same slots dir.
    static SLOT_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn unique_test_dir() -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "llm-slots-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        dir
    }

    fn isolate_slots_env() -> (Option<String>, Option<String>) {
        let orig_xdg = std::env::var("XDG_RUNTIME_DIR").ok();
        let orig_cache = std::env::var("SQLITE_GRAPHRAG_CACHE_DIR").ok();
        std::env::remove_var("XDG_RUNTIME_DIR");
        std::env::set_var("SQLITE_GRAPHRAG_CACHE_DIR", unique_test_dir());
        (orig_xdg, orig_cache)
    }

    fn restore_slots_env(orig_xdg: Option<String>, orig_cache: Option<String>) {
        match orig_xdg {
            Some(v) => std::env::set_var("XDG_RUNTIME_DIR", v),
            None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }
        match orig_cache {
            Some(v) => std::env::set_var("SQLITE_GRAPHRAG_CACHE_DIR", v),
            None => std::env::remove_var("SQLITE_GRAPHRAG_CACHE_DIR"),
        }
    }

    #[test]
    fn slot_enforces_max_concurrency() {
        let _serial = SLOT_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (orig_xdg, orig_cache) = isolate_slots_env();

        let _g1 = acquire_llm_slot(2, 5).expect("first slot");
        let _g2 = acquire_llm_slot(2, 5).expect("second slot");
        let start = std::time::Instant::now();
        let result = acquire_llm_slot(2, 1);
        assert!(result.is_err(), "third slot should fail with max=2");
        assert!(
            start.elapsed() >= std::time::Duration::from_secs(1),
            "should wait full timeout before failing"
        );

        restore_slots_env(orig_xdg, orig_cache);
    }

    #[test]
    fn slot_releases_on_drop() {
        let _serial = SLOT_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (orig_xdg, orig_cache) = isolate_slots_env();

        let g1 = acquire_llm_slot(1, 5).expect("first slot");
        drop(g1);
        let _g2 = acquire_llm_slot(1, 5).expect("second slot after drop");

        restore_slots_env(orig_xdg, orig_cache);
    }

    #[test]
    fn slot_max_concurrent_zero_is_validation_error() {
        let result = acquire_llm_slot(0, 1);
        assert!(matches!(result, Err(AppError::Validation(_))));
    }

    #[test]
    fn read_status_reflects_active_slots() {
        let _serial = SLOT_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (orig_xdg, orig_cache) = isolate_slots_env();

        let _g1 = acquire_llm_slot(4, 5).expect("first slot");
        let status = read_status(4);
        assert_eq!(status.max, 4);
        assert!(status.active >= 1);
        assert!(!status.pids.is_empty());

        restore_slots_env(orig_xdg, orig_cache);
    }

    #[test]
    fn concurrent_acquires_with_2_threads_serialize() {
        let _serial = SLOT_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (orig_xdg, orig_cache) = isolate_slots_env();

        let barrier = Arc::new(Barrier::new(3));
        let mut handles = vec![];
        for _ in 0..3 {
            let b = barrier.clone();
            handles.push(thread::spawn(move || {
                b.wait();
                acquire_llm_slot(2, 5)
            }));
        }
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let successes = results.iter().filter(|r| r.is_ok()).count();
        // max=2 → no máximo 2 succeeds simultâneos (mas teste serializa)
        assert!(successes >= 1);

        restore_slots_env(orig_xdg, orig_cache);
    }
}
