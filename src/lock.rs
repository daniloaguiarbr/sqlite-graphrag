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
//!
//! ## Job-type singleton (G28-B, v1.0.68)
//!
//! Heavy long-running jobs (`enrich`, `ingest --mode claude-code`,
//! `ingest --mode codex`) also acquire a *singleton* lock per `(job_type,
//! namespace)` via `acquire_job_singleton`.  This guarantees at most one
//! heavy job per namespace runs at any time, which was the root cause
//! of the 2026-06-03 process-proliferation incident (4 parallel `enrich`
//! instances × N workers × 10 MCP servers = ~192 spawned processes).
// Workload: I/O-bound (flock polling with exponential backoff sleep)

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use directories::ProjectDirs;
use fs4::fs_std::FileExt;

use crate::constants::{
    CLI_LOCK_POLL_INTERVAL_MS, JOB_SINGLETON_POLL_INTERVAL_MS, LLM_WORKER_RSS_MB,
    MAX_CONCURRENT_CLI_INSTANCES,
};
use crate::errors::AppError;

/// Job-type classification for `acquire_job_singleton`.
///
/// `Light` is intentionally NOT a variant here because lightweight
/// commands (`recall`, `stats`, `read`, `list`) share the existing
/// counting-semaphore in [`acquire_cli_slot`] and do not need a singleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobType {
    /// `enrich` command (LLM-driven entity/relation/body enrichment).
    Enrich,
    /// `ingest --mode claude-code` (LLM-curated ingestion).
    IngestClaudeCode,
    /// `ingest --mode codex` (OpenAI Codex CLI ingestion).
    IngestCodex,
}

impl JobType {
    /// Returns the kebab-case tag used inside the lock file name.
    fn tag(self) -> &'static str {
        match self {
            JobType::Enrich => "enrich",
            JobType::IngestClaudeCode => "ingest-claude-code",
            JobType::IngestCodex => "ingest-codex",
        }
    }
}

/// Returns the lock file path for the given slot.
///
/// Honours `SQLITE_GRAPHRAG_CACHE_DIR` when set (useful for tests, containers,
/// and NFS caches), falling back to the OS default cache directory via
/// `directories::ProjectDirs`. The slot must be 1-based.
fn slot_path(slot: usize) -> Result<PathBuf, AppError> {
    let cache = cache_dir()?;
    std::fs::create_dir_all(&cache)?;
    Ok(cache.join(format!("cli-slot-{slot}.lock")))
}

/// Resolves the lock-file directory honouring `SQLITE_GRAPHRAG_CACHE_DIR`.
fn cache_dir() -> Result<PathBuf, AppError> {
    if let Some(override_dir) = std::env::var_os("SQLITE_GRAPHRAG_CACHE_DIR") {
        Ok(PathBuf::from(override_dir))
    } else {
        let dirs = ProjectDirs::from("", "", "sqlite-graphrag").ok_or_else(|| {
            AppError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "could not determine cache directory for sqlite-graphrag lock files",
            ))
        })?;
        Ok(dirs.cache_dir().to_path_buf())
    }
}

/// Computes a short, filesystem-safe hash of the database path so two distinct
/// databases (e.g. `/tmp/a.sqlite` and `/tmp/b.sqlite`) get distinct lock
/// files in the shared cache directory. First 12 hex chars of BLAKE3 are
/// sufficient for collision avoidance across the local filesystem.
pub fn db_path_hash(db_path: &Path) -> String {
    let canonical = db_path
        .canonicalize()
        .unwrap_or_else(|_| db_path.to_path_buf());
    let hash = blake3::hash(canonical.to_string_lossy().as_bytes());
    hash.to_hex().to_string()[..12].to_string()
}

/// Returns the singleton lock file path for a given (job_type, namespace, db_hash).
///
/// Layout: `job-singleton-{tag}-{namespace_slug}-{db_hash}.lock` in the same
/// cache dir as the CLI slots. The namespace is sanitised to a filesystem-safe
/// slug (lowercase, hyphens, alphanumeric) and defaults to `default` when
/// empty. The `db_hash` is the BLAKE3 prefix returned by [`db_path_hash`].
///
/// G30 (v1.0.69): the previous implementation ignored the database path
/// entirely, so two concurrent `enrich` invocations against different
/// `graphrag.sqlite` files (production vs. test) collided on the same
/// cache-dir lock. The db_hash scope makes the singleton per-database while
/// still sharing the same cache dir.
pub fn job_singleton_path(
    job_type: JobType,
    namespace: &str,
    db_hash: &str,
) -> Result<PathBuf, AppError> {
    let cache = cache_dir()?;
    std::fs::create_dir_all(&cache)?;
    let slug = if namespace.is_empty() {
        "default".to_string()
    } else {
        namespace
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
    };
    let safe_hash: String = db_hash
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(16)
        .collect();
    Ok(cache.join(format!(
        "job-singleton-{}-{slug}-{safe_hash}.lock",
        job_type.tag()
    )))
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
///
/// Returns the maximum number of parallel CLI instances the host can sustain
/// without thrashing. The formula:
///
///   safe = min(cpus, available_mb / per_worker_mb) * 1.0
///
/// replaces the previous `... * 0.5` halving factor. The `* 0.5` was the
/// root cause of G18: even on a 64 GB host the result was always
/// clamped to 4 because of the division-by-2.
///
/// The per-worker cost is `LLM_WORKER_RSS_MB` (350): since v1.0.79 every
/// build is LLM-only (the `embedding-legacy` feature and the ONNX path
/// were removed), so the higher fastembed worker cost no longer applies.
///
/// Returns 1 as a defensive floor when system stats are unavailable.
pub fn calculate_safe_concurrency() -> usize {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();
    let available_mb = sys.available_memory() / 1_048_576;
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);

    let per_worker_mb = LLM_WORKER_RSS_MB;

    let memory_bound = if available_mb == 0 {
        cpus
    } else {
        (available_mb / per_worker_mb.max(1)) as usize
    };
    let raw = cpus.min(memory_bound).max(1);
    raw.min(MAX_CONCURRENT_CLI_INSTANCES)
}

/// v1.0.75 — Returns the worker cost in MiB used by `calculate_safe_concurrency`.
/// Exposed for telemetry and `--info` output.
pub fn worker_cost_mb() -> u64 {
    LLM_WORKER_RSS_MB
}

///   `AppError::AllSlotsFull { max, waited_secs: n }` if no slot opens.
///
/// The returned `File` MUST be kept alive until the process exits; dropping it
/// releases the slot automatically via the implicit `flock` on close.
pub fn acquire_cli_slot(
    max_concurrency: usize,
    wait_seconds: Option<u64>,
) -> Result<(File, usize), AppError> {
    // G18: use env override or 2*cpus as ceiling instead of hardcoded 4
    let ncpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let ceiling = std::env::var("SQLITE_GRAPHRAG_MAX_CLI_INSTANCES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or_else(|| (2 * ncpus).max(MAX_CONCURRENT_CLI_INSTANCES));
    let max = max_concurrency.clamp(1, ceiling);
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

    // Polling loop with progressive backoff until the deadline.
    let deadline = Instant::now() + Duration::from_secs(wait_secs);
    let mut polls: u64 = 0;
    loop {
        let poll_delay = CLI_LOCK_POLL_INTERVAL_MS
            .saturating_mul(1 + polls / 4)
            .min(CLI_LOCK_POLL_INTERVAL_MS * 4);
        thread::sleep(Duration::from_millis(poll_delay));
        polls += 1;
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

/// Acquires a process-wide singleton lock for a heavy job type and namespace.
///
/// G28-B (v1.0.68): ensures at most one `enrich`, `ingest --mode
/// claude-code`, or `ingest --mode codex` runs at a time per namespace.
/// A second invocation in the same namespace either:
///
/// - Returns immediately with `AppError::JobSingletonLocked { job_type,
///   namespace }` when `wait_seconds` is `None` or `Some(0)`.
/// - Polls every [`JOB_SINGLETON_POLL_INTERVAL_MS`] ms until the lock
///   drops or the deadline expires, returning the same error on timeout.
///
/// The returned `File` MUST be kept alive until the process exits;
/// dropping it releases the singleton for the next invocation.
pub fn acquire_job_singleton(
    job_type: JobType,
    namespace: &str,
    db_path: &Path,
    wait_seconds: Option<u64>,
    force: bool,
) -> Result<File, AppError> {
    let db_hash = db_path_hash(db_path);
    let path = job_singleton_path(job_type, namespace, &db_hash)?;

    // G30+G09: when --force is set, attempt to break a stale lock by
    // detecting and removing a pre-existing lock file. This is a last
    // resort: only enabled by an explicit operator flag. A real orphan
    // lock from a previous crash leaves a 0-byte file behind, which the
    // next non-forced caller would still try to lock.
    if force && path.exists() {
        tracing::warn!(target: "lock",
            path = %path.display(),
            "force=true; removing pre-existing singleton lock file"
        );
        let _ = std::fs::remove_file(&path);
    }

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    if let Err(e) = file.try_lock_exclusive() {
        if !is_lock_contended(&e) {
            return Err(AppError::Io(e));
        }
        // Already held by another instance.
        let wait_secs = wait_seconds.unwrap_or(0);
        if wait_secs == 0 {
            return Err(AppError::JobSingletonLocked {
                job_type: job_type.tag().to_string(),
                namespace: namespace.to_string(),
            });
        }
        let deadline = Instant::now() + Duration::from_secs(wait_secs);
        // Drop the failed handle before polling; flock is per-process so we
        // re-open each attempt to refresh contention state.
        drop(file);
        loop {
            thread::sleep(Duration::from_millis(JOB_SINGLETON_POLL_INTERVAL_MS));
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)?;
            if file.try_lock_exclusive().is_ok() {
                return Ok(file);
            }
            if Instant::now() >= deadline {
                return Err(AppError::JobSingletonLocked {
                    job_type: job_type.tag().to_string(),
                    namespace: namespace.to_string(),
                });
            }
        }
    }
    Ok(file)
}

/// G45: returns the lock file path for the embedding singleton
/// of a `(namespace, db_hash)` pair. Layout:
/// `embed-singleton-{namespace_slug}-{db_hash}.lock` in the same
/// cache directory as the other singletons. The namespace is sanitised
/// to a filesystem-safe slug the same way as [`job_singleton_path`].
fn embedding_singleton_path(namespace: &str, db_hash: &str) -> Result<PathBuf, AppError> {
    let cache = cache_dir()?;
    std::fs::create_dir_all(&cache)?;
    let slug = if namespace.is_empty() {
        "default".to_string()
    } else {
        namespace
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
    };
    let safe_hash: String = db_hash
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(16)
        .collect();
    Ok(cache.join(format!("embed-singleton-{slug}-{safe_hash}.lock")))
}

/// G45: acquires a cross-process singleton lock for LLM embedding
/// operations against a given `(namespace, db)` pair.
///
/// The lock is opened and held with `flock` (same mechanism as
/// [`acquire_job_singleton`]). Two CLI invocations writing to the same
/// database while both are calling the LLM on entity names will now
/// serialise: the second one receives [`AppError::EmbeddingSingletonLocked`]
/// (exit 75) instead of double-spawning `claude -p` / `codex exec`
/// subprocesses.
///
/// Behaviour:
/// - `wait_seconds = Some(0)` or `None` → fail immediately if held.
/// - `wait_seconds = Some(n) > 0` → poll every
///   [`JOB_SINGLETON_POLL_INTERVAL_MS`] ms until the lock drops or the
///   deadline expires.
/// - `force = true` → remove a stale lock file before acquiring
///   (operator escape hatch, same contract as `acquire_job_singleton`).
///
/// The returned [`File`] MUST be kept alive for the duration of the
/// embedding work; dropping it releases the singleton for the next
/// process.
pub fn acquire_embedding_singleton(
    namespace: &str,
    db_path: &Path,
    wait_seconds: Option<u64>,
    force: bool,
) -> Result<File, AppError> {
    let db_hash = db_path_hash(db_path);
    let path = embedding_singleton_path(namespace, &db_hash)?;

    if force && path.exists() {
        tracing::warn!(target: "lock.g45",
            path = %path.display(),
            "force=true; removing pre-existing embedding singleton lock file"
        );
        let _ = std::fs::remove_file(&path);
    }

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    if let Err(e) = file.try_lock_exclusive() {
        if !is_lock_contended(&e) {
            return Err(AppError::Io(e));
        }
        let wait_secs = wait_seconds.unwrap_or(0);
        if wait_secs == 0 {
            return Err(AppError::EmbeddingSingletonLocked {
                namespace: namespace.to_string(),
            });
        }
        let deadline = Instant::now() + Duration::from_secs(wait_secs);
        drop(file);
        loop {
            thread::sleep(Duration::from_millis(JOB_SINGLETON_POLL_INTERVAL_MS));
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)?;
            if file.try_lock_exclusive().is_ok() {
                return Ok(file);
            }
            if Instant::now() >= deadline {
                return Err(AppError::EmbeddingSingletonLocked {
                    namespace: namespace.to_string(),
                });
            }
        }
    }
    Ok(file)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);

    fn unique_ns() -> String {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        format!("test-{pid}-{n}")
    }

    #[test]
    fn job_singleton_path_sanitises_namespace() {
        let p = job_singleton_path(JobType::Enrich, "Foo Bar/Baz", "abc123def456")
            .expect("path should resolve");
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.contains("enrich"), "got {name}");
        assert!(name.contains("foo-bar-baz"), "got {name}");
        assert!(
            name.contains("abc123def456"),
            "must embed db_hash: got {name}"
        );
    }

    #[test]
    fn job_singleton_blocks_second_invocation_same_namespace() {
        let ns = unique_ns();
        let db = std::env::temp_dir().join(format!("test-{}.sqlite", unique_ns()));
        let first = acquire_job_singleton(JobType::Enrich, &ns, &db, Some(0), false)
            .expect("first acquire should succeed");
        let second = acquire_job_singleton(JobType::Enrich, &ns, &db, Some(0), false);
        assert!(
            matches!(second, Err(AppError::JobSingletonLocked { .. })),
            "expected JobSingletonLocked, got {second:?}"
        );
        drop(first);
    }

    #[test]
    fn job_singleton_allows_different_namespaces() {
        let ns_a = unique_ns();
        let ns_b = unique_ns();
        let db_a = std::env::temp_dir().join(format!("test-a-{}.sqlite", unique_ns()));
        let db_b = std::env::temp_dir().join(format!("test-b-{}.sqlite", unique_ns()));
        let first = acquire_job_singleton(JobType::IngestClaudeCode, &ns_a, &db_a, Some(0), false)
            .expect("ns_a should acquire");
        let second = acquire_job_singleton(JobType::IngestClaudeCode, &ns_b, &db_b, Some(0), false)
            .expect("ns_b should acquire in parallel");
        drop(first);
        drop(second);
    }

    #[test]
    fn job_singleton_scoped_by_db_hash() {
        // G30: two databases, same namespace, different content. Both locks
        // should succeed because the db_hash differs.
        let ns = unique_ns();
        let db_a = std::env::temp_dir().join(format!("test-x-{}.sqlite", unique_ns()));
        let db_b = std::env::temp_dir().join(format!("test-y-{}.sqlite", unique_ns()));
        let first = acquire_job_singleton(JobType::Enrich, &ns, &db_a, Some(0), false)
            .expect("db_a should acquire");
        let second = acquire_job_singleton(JobType::Enrich, &ns, &db_b, Some(0), false)
            .expect("db_b should acquire independently (G30 fix)");
        drop(first);
        drop(second);
    }

    #[test]
    fn db_path_hash_is_stable_for_same_path() {
        let p = std::env::temp_dir().join("hashing-test.sqlite");
        let h1 = db_path_hash(&p);
        let h2 = db_path_hash(&p);
        assert_eq!(h1, h2, "same path must produce same hash");
        assert_eq!(h1.len(), 12, "BLAKE3 prefix must be 12 hex chars");
    }

    #[test]
    fn db_path_hash_differs_for_different_paths() {
        let a = std::env::temp_dir().join("hash-a.sqlite");
        let b = std::env::temp_dir().join("hash-b.sqlite");
        assert_ne!(db_path_hash(&a), db_path_hash(&b));
    }

    // G45: embedding singleton — cross-process coordination
    #[test]
    fn g45_embedding_singleton_blocks_second_invocation_same_db() {
        let ns = unique_ns();
        let db = std::env::temp_dir().join(format!("g45-{}.sqlite", unique_ns()));
        let first = acquire_embedding_singleton(&ns, &db, Some(0), false)
            .expect("first acquire should succeed");
        let second = acquire_embedding_singleton(&ns, &db, Some(0), false);
        assert!(
            matches!(second, Err(AppError::EmbeddingSingletonLocked { .. })),
            "expected EmbeddingSingletonLocked, got {second:?}"
        );
        drop(first);
    }

    #[test]
    fn g45_embedding_singleton_allows_different_namespaces() {
        let ns_a = unique_ns();
        let ns_b = unique_ns();
        let db = std::env::temp_dir().join(format!("g45-multi-{}.sqlite", unique_ns()));
        let first =
            acquire_embedding_singleton(&ns_a, &db, Some(0), false).expect("ns_a should acquire");
        let second = acquire_embedding_singleton(&ns_b, &db, Some(0), false)
            .expect("ns_b should acquire in parallel (different namespace)");
        drop(first);
        drop(second);
    }

    #[test]
    fn g45_embedding_singleton_scoped_by_db_hash() {
        // Same namespace, different databases → independent locks.
        let ns = unique_ns();
        let db_a = std::env::temp_dir().join(format!("g45-x-{}.sqlite", unique_ns()));
        let db_b = std::env::temp_dir().join(format!("g45-y-{}.sqlite", unique_ns()));
        let first =
            acquire_embedding_singleton(&ns, &db_a, Some(0), false).expect("db_a should acquire");
        let second = acquire_embedding_singleton(&ns, &db_b, Some(0), false)
            .expect("db_b should acquire independently (G45 db_hash scope)");
        drop(first);
        drop(second);
    }
}
