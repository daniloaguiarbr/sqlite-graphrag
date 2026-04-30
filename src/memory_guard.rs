//! Memory guard: checks RAM availability before loading the ONNX model.
//!
//! Loading the model via `fastembed` consumes approximately
//! [`crate::constants::EMBEDDING_LOAD_EXPECTED_RSS_MB`] MiB of resident memory.
//! Without this guard, multiple parallel invocations can exhaust RAM and trigger
//! OOM (Out-Of-Memory), stalling the system.
//!
//! This guard queries the OS via `sysinfo` before any heavy initialisation,
//! aborting with [`crate::errors::AppError::LowMemory`] (exit 77) when the
//! configured floor is not met.

use sysinfo::{
    get_current_pid, MemoryRefreshKind, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System,
    UpdateKind,
};

use crate::errors::AppError;

/// Returns the current available memory in MiB.
pub fn available_memory_mb() -> u64 {
    let sys =
        System::new_with_specifics(RefreshKind::new().with_memory(MemoryRefreshKind::everything()));
    let available_bytes = sys.available_memory();
    available_bytes / (1024 * 1024)
}

/// Returns the current process RSS in MiB when available.
pub fn current_process_memory_mb() -> Option<u64> {
    let pid = get_current_pid().ok()?;
    let mut sys =
        System::new_with_specifics(RefreshKind::new().with_memory(MemoryRefreshKind::everything()));
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::new()
            .with_memory()
            .with_exe(UpdateKind::OnlyIfNotSet),
    );
    sys.process(pid).map(|p| p.memory() / (1024 * 1024))
}

/// Calculates the safe concurrency ceiling for heavy embedding workloads.
///
/// Canonical formula:
/// `permits = min(cpus, available_memory_mb / ram_per_task_mb) * 0.5`
///
/// The result is clamped between `1` and `max_concurrency`.
pub fn calculate_safe_concurrency(
    available_mb: u64,
    cpu_count: usize,
    ram_per_task_mb: u64,
    max_concurrency: usize,
) -> usize {
    let cpu_count = cpu_count.max(1);
    let max_concurrency = max_concurrency.max(1);
    let ram_per_task_mb = ram_per_task_mb.max(1);

    let memory_bound = (available_mb / ram_per_task_mb) as usize;
    let resource_bound = cpu_count.min(memory_bound).max(1);
    let safe_with_margin = (resource_bound / 2).max(1);

    safe_with_margin.min(max_concurrency)
}

/// Checks whether sufficient memory is available to start loading the model.
///
/// # Parameters
/// - `min_mb`: minimum floor in MiB of available memory (typically
///   [`crate::constants::MIN_AVAILABLE_MEMORY_MB`]).
///
/// # Errors
/// Returns [`AppError::LowMemory`] when `available_mb < min_mb`.
///
/// # Returns
/// Returns `Ok(available_mb)` with the actual available memory in MiB.
pub fn check_available_memory(min_mb: u64) -> Result<u64, AppError> {
    let available_mb = available_memory_mb();

    if available_mb < min_mb {
        return Err(AppError::LowMemory {
            available_mb,
            required_mb: min_mb,
        });
    }

    Ok(available_mb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_available_memory_with_zero_always_passes() {
        let result = check_available_memory(0);
        assert!(result.is_ok(), "min_mb=0 must always pass, got: {result:?}");
        let mb = result.unwrap();
        assert!(mb > 0, "system must report positive memory");
    }

    #[test]
    fn check_available_memory_with_huge_value_fails() {
        let result = check_available_memory(u64::MAX);
        assert!(
            matches!(result, Err(AppError::LowMemory { .. })),
            "u64::MAX MiB must fail with LowMemory, got: {result:?}"
        );
    }

    #[test]
    fn low_memory_error_contains_correct_values() {
        match check_available_memory(u64::MAX) {
            Err(AppError::LowMemory {
                available_mb,
                required_mb,
            }) => {
                assert_eq!(required_mb, u64::MAX);
                assert!(available_mb < u64::MAX);
            }
            outro => unreachable!("esperado LowMemory, got: {outro:?}"),
        }
    }

    #[test]
    fn calculate_safe_concurrency_respects_half_margin() {
        let permits = calculate_safe_concurrency(8_000, 8, 1_000, 4);
        assert_eq!(permits, 4);
    }

    #[test]
    fn calculate_safe_concurrency_never_returns_zero() {
        let permits = calculate_safe_concurrency(100, 1, 10_000, 4);
        assert_eq!(permits, 1);
    }

    #[test]
    fn calculate_safe_concurrency_respects_max_ceiling() {
        let permits = calculate_safe_concurrency(128_000, 64, 500, 4);
        assert_eq!(permits, 4);
    }

    #[test]
    fn current_process_memory_mb_returns_some_value() {
        let rss = current_process_memory_mb();
        assert!(rss.is_some());
    }
}
