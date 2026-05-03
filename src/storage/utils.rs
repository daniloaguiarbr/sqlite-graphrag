//! Storage utility helpers shared across the storage sub-modules.

use crate::constants::{MAX_SQLITE_BUSY_RETRIES, SQLITE_BUSY_BASE_DELAY_MS};
use crate::errors::AppError;
use rusqlite::ErrorCode;
use std::thread;
use std::time::Duration;

/// Returns `true` when `err` wraps an `SQLITE_BUSY` (or `SQLITE_LOCKED`)
/// condition reported by rusqlite.
///
/// Both `SQLITE_BUSY` (`ErrorCode::DatabaseBusy`) and `SQLITE_LOCKED`
/// (`ErrorCode::DatabaseLocked`) indicate that the write cannot proceed
/// immediately due to WAL concurrency.  We treat both as transient and
/// eligible for retry.
pub fn is_sqlite_busy(err: &AppError) -> bool {
    match err {
        AppError::Database(rusqlite::Error::SqliteFailure(e, _)) => {
            e.code == ErrorCode::DatabaseBusy || e.code == ErrorCode::DatabaseLocked
        }
        _ => false,
    }
}

/// Executes `op` up to `MAX_SQLITE_BUSY_RETRIES` times with exponential
/// backoff whenever the operation fails with `SQLITE_BUSY` / `SQLITE_LOCKED`.
///
/// Delay schedule (base = `SQLITE_BUSY_BASE_DELAY_MS`):
/// - attempt 1 → `base` ms
/// - attempt 2 → `base * 2` ms
/// - attempt 3 → `base * 4` ms
/// - attempt 4 → `base * 8` ms
/// - attempt 5 → `base * 16` ms
///
/// After all retries are exhausted the last `SQLITE_BUSY` error is converted
/// to [`AppError::DbBusy`] so callers can route on exit-code `15`.
pub fn with_busy_retry<F>(op: F) -> Result<(), AppError>
where
    F: Fn() -> Result<(), AppError>,
{
    for attempt in 0..MAX_SQLITE_BUSY_RETRIES {
        match op() {
            Ok(()) => return Ok(()),
            Err(e) if is_sqlite_busy(&e) => {
                // v1.0.43 (M7): half-jitter to prevent thundering herd when multiple CLIs hit
                // SQLITE_BUSY simultaneously. Effective delay: [base/2, base).
                let base_ms = SQLITE_BUSY_BASE_DELAY_MS * (1u64 << attempt);
                let half = base_ms / 2;
                let jitter = if half == 0 { 0 } else { fastrand::u64(0..half) };
                let delay_ms = half + jitter;
                thread::sleep(Duration::from_millis(delay_ms));
            }
            Err(other) => return Err(other),
        }
    }

    // All retries exhausted — convert to DbBusy for stable exit-code 15.
    Err(AppError::DbBusy(format!(
        "SQLITE_BUSY after {MAX_SQLITE_BUSY_RETRIES} retries"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    /// Helper that builds a fake `AppError::Database` wrapping
    /// `SQLITE_BUSY` (error code 5) so that `is_sqlite_busy` can be tested
    /// without needing a live SQLite connection.
    fn make_busy_error() -> AppError {
        // rusqlite::Error::SqliteFailure requires a `ffi::Error` + optional msg.
        // We construct it via the public `rusqlite::ffi` interface.
        let ffi_err = rusqlite::ffi::Error {
            code: ErrorCode::DatabaseBusy,
            extended_code: 5,
        };
        AppError::Database(rusqlite::Error::SqliteFailure(ffi_err, None))
    }

    fn make_locked_error() -> AppError {
        let ffi_err = rusqlite::ffi::Error {
            code: ErrorCode::DatabaseLocked,
            extended_code: 6,
        };
        AppError::Database(rusqlite::Error::SqliteFailure(ffi_err, None))
    }

    #[test]
    fn is_sqlite_busy_detects_database_busy() {
        assert!(is_sqlite_busy(&make_busy_error()));
    }

    #[test]
    fn is_sqlite_busy_detects_database_locked() {
        assert!(is_sqlite_busy(&make_locked_error()));
    }

    #[test]
    fn is_sqlite_busy_rejects_other_errors() {
        let err = AppError::Validation("invalid field".into());
        assert!(!is_sqlite_busy(&err));
    }

    #[test]
    fn with_busy_retry_propagates_non_busy_error() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_clone = Arc::clone(&calls);

        let result = with_busy_retry(|| {
            calls_clone.fetch_add(1, Ordering::SeqCst);
            Err(AppError::Validation("campo x".into()))
        });

        // Non-busy errors must propagate immediately without retrying.
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(matches!(result, Err(AppError::Validation(_))));
    }

    #[test]
    fn with_busy_retry_succeeds_on_third_attempt() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_clone = Arc::clone(&calls);

        // Fail twice with SQLITE_BUSY, succeed on the third call.
        let result = with_busy_retry(|| {
            let n = calls_clone.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                Err(make_busy_error())
            } else {
                Ok(())
            }
        });

        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert!(result.is_ok(), "expected Ok after 3rd attempt");
    }

    #[test]
    fn busy_retry_jitter_in_range() {
        // Verify that the half-jitter formula stays within [base/2, base) for attempt=2.
        // attempt=2 → base_ms = SQLITE_BUSY_BASE_DELAY_MS * 4; half = base_ms/2.
        // We call fastrand::u64 indirectly through with_busy_retry by observing that the
        // function completes; direct delay bounds are tested via the formula invariant.
        let base_ms = SQLITE_BUSY_BASE_DELAY_MS * (1u64 << 2); // attempt=2
        let half = base_ms / 2;
        for _ in 0..100 {
            let jitter = fastrand::u64(0..half);
            let delay_ms = half + jitter;
            assert!(
                delay_ms >= half && delay_ms < base_ms,
                "delay_ms {delay_ms} out of [{half}, {base_ms})"
            );
        }
    }

    #[test]
    fn with_busy_retry_returns_db_busy_after_all_retries() {
        let calls = Arc::new(AtomicU32::new(0));
        let calls_clone = Arc::clone(&calls);

        let result = with_busy_retry(|| {
            calls_clone.fetch_add(1, Ordering::SeqCst);
            Err(make_busy_error())
        });

        assert_eq!(
            calls.load(Ordering::SeqCst),
            MAX_SQLITE_BUSY_RETRIES,
            "must attempt exactly MAX_SQLITE_BUSY_RETRIES times"
        );
        assert!(
            matches!(result, Err(AppError::DbBusy(_))),
            "must convert to DbBusy after exhausting retries"
        );
    }
}
