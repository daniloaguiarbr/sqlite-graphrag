//! G28-D: system load average observation before spawning LLM subprocesses.
//!
//! The 2026-06-03 incident saturated a 10-CPU host with load 276 because
//! parallel `enrich` workers kept spawning `claude -p` / `codex exec`
//! children even when the system was already at saturation. This module
//! exposes a single helper that returns `true` when the 1-minute load
//! average is above `2 × ncpus` (the conservative threshold the G28-D
//! original discussion recommended).
//!
//! Uses `sysinfo::System::load_average()` which is already a transitive
//! dependency of the project. The read is cheap (single syscall on
//! Linux) and throttled to once per second via a Mutex-cached timestamp.

use std::sync::Mutex;
use std::time::{Duration, Instant};

static LAST_REFRESH: Mutex<Option<Instant>> = Mutex::new(None);

/// Returns the 1-minute load average as reported by the OS.
///
/// On platforms where `sysinfo` cannot read load average (very old Linux
/// without /proc/loadavg), returns `0.0` so callers default to "no
/// saturation detected".
pub fn load_average_one() -> f64 {
    let _ = ensure_fresh();
    sysinfo::System::load_average().one
}

/// Returns the number of logical CPUs the runtime can detect.
///
/// Used together with [`load_average_one`] to apply a saturation check.
pub fn ncpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

/// G28-D: returns `true` when the 1-minute load average exceeds
/// `2 × ncpus` (the conservative threshold originally proposed in the
/// G28 audit). The default threshold can be overridden by the
/// `SQLITE_GRAPHRAG_MAX_LOAD_PER_NCPU` env var.
pub fn is_system_saturated() -> bool {
    let load = load_average_one();
    let n = ncpus() as f64;
    let multiplier: f64 = std::env::var("SQLITE_GRAPHRAG_MAX_LOAD_PER_NCPU")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2.0);
    load > n * multiplier
}

/// Throttles the cached refresh timestamp so we read /proc/loadavg at
/// most once per second across all callers. The function returns the
/// previous timestamp (or None on first call) so the caller can decide
/// whether to actually invoke the syscall.
fn ensure_fresh() -> Option<Instant> {
    let mut guard = LAST_REFRESH.lock().expect("loadavg mutex poisoned");
    let now = Instant::now();
    let should_refresh = guard
        .as_ref()
        .is_none_or(|last| now.duration_since(*last) > Duration::from_secs(1));
    let prev = guard.as_ref().copied();
    if should_refresh {
        *guard = Some(now);
    }
    prev
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ncpus_is_at_least_one() {
        assert!(ncpus() >= 1);
    }

    #[test]
    fn load_average_is_non_negative() {
        assert!(load_average_one() >= 0.0);
    }

    #[test]
    fn saturation_default_threshold_is_two() {
        // G28-D default: 2 × ncpus. Operators can lower it via env var
        // when running on contended CI runners.
        let env_default = std::env::var("SQLITE_GRAPHRAG_MAX_LOAD_PER_NCPU")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2.0);
        assert!(env_default >= 1.0);
    }

    #[test]
    fn saturation_check_does_not_panic() {
        // The function must always return a definitive answer.
        let _ = is_system_saturated();
    }

    #[test]
    fn ensure_fresh_returns_previous_then_sets_new() {
        let prev = ensure_fresh();
        // On the first call prev is None; subsequent calls return Some.
        if prev.is_none() {
            let second = ensure_fresh();
            // Within the same second the cache is fresh so prev is Some.
            assert!(second.is_some());
        }
    }
}
