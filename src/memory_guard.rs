//! Memory guard: checks RAM availability before heavy embedding workloads.
//!
//! Each LLM embedding worker spawns a `claude -p` / `codex exec` subprocess
//! costing roughly [`crate::constants::LLM_WORKER_RSS_MB`] MiB of resident
//! memory. Without this guard, multiple parallel invocations can exhaust RAM
//! and trigger OOM (Out-Of-Memory), stalling the system.
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
    // G18: removed unconditional /2 margin — callers should pass lower ram_per_task_mb
    // when daemon is active (model shared) instead of halving the result
    resource_bound.min(max_concurrency)
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

/// Rejects an embedding input that would overflow the model's token window
/// (GAP-SG-02).
///
/// The PRIMARY limit is TOKENS: `qwen/qwen3-embedding-8b` accepts roughly 32K
/// tokens, so an input above [`crate::constants::EMBEDDING_REQUEST_MAX_TOKENS`]
/// is rejected before the HTTP request, using the conservative cl100k_base
/// proxy in [`crate::tokenizer::count_tokens`]. The byte cap
/// [`crate::constants::MAX_MEMORY_BODY_LEN`] is a SECONDARY, coarser guard kept
/// as a cheap short-circuit so a pathological input is rejected even before
/// tokenisation.
///
/// # Errors
/// Returns [`AppError::Validation`] (exit 1, permanent) when either limit is
/// exceeded; the message advises splitting the input into smaller memories.
pub fn check_embedding_input_size(text: &str) -> Result<(), AppError> {
    // Secondary guard: a byte length far above the body cap cannot fit the
    // token window, and the check is O(1) versus tokenising the whole input.
    let bytes = text.len();
    if bytes > crate::constants::MAX_MEMORY_BODY_LEN {
        return Err(AppError::Validation(format!(
            "embedding input is {} bytes, above the {}-byte body cap; \
             split it into smaller memories",
            bytes,
            crate::constants::MAX_MEMORY_BODY_LEN
        )));
    }

    // Primary guard: the model's real ceiling is in tokens.
    let tokens = crate::tokenizer::count_tokens(text);
    if tokens > crate::constants::EMBEDDING_REQUEST_MAX_TOKENS {
        return Err(AppError::Validation(format!(
            "embedding input is {} tokens, above the {}-token model ceiling; \
             split it into smaller memories",
            tokens,
            crate::constants::EMBEDDING_REQUEST_MAX_TOKENS
        )));
    }

    Ok(())
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
            other => unreachable!("expected LowMemory, got: {other:?}"),
        }
    }

    #[test]
    fn calculate_safe_concurrency_no_half_margin() {
        // v1.0.75 (G18): halving margin removed. 8000 MB / 1000 MB = 8, min(8, 8) = 8.
        let permits = calculate_safe_concurrency(8_000, 8, 1_000, 16);
        assert_eq!(permits, 8);
    }

    #[test]
    fn calculate_safe_concurrency_never_returns_zero() {
        let permits = calculate_safe_concurrency(100, 1, 10_000, 16);
        assert_eq!(permits, 1);
    }

    #[test]
    fn calculate_safe_concurrency_respects_max_ceiling() {
        // 128 GB / 500 MB = 256, min(64, 256) = 64, clamped to max 16
        let permits = calculate_safe_concurrency(128_000, 64, 500, 16);
        assert_eq!(permits, 16);
    }

    #[test]
    fn calculate_safe_concurrency_llm_worker_budget() {
        // LLM workers: 64 GB available, 8 CPUs, 350 MB per worker.
        // 64_000 / 350 = 182, min(8, 182) = 8.
        let permits = calculate_safe_concurrency(64_000, 8, 350, 16);
        assert_eq!(permits, 8);
    }

    #[test]
    fn current_process_memory_mb_returns_some_value() {
        let rss = current_process_memory_mb();
        assert!(rss.is_some());
    }

    #[test]
    fn check_embedding_input_size_accepts_small_text() {
        assert!(check_embedding_input_size("a short passage").is_ok());
    }

    #[test]
    fn check_embedding_input_size_rejects_above_token_ceiling() {
        // "word " repeated is ~1 cl100k token per word; well above 30K words
        // exceeds EMBEDDING_REQUEST_MAX_TOKENS while staying under the byte cap.
        let big = "word ".repeat(crate::constants::EMBEDDING_REQUEST_MAX_TOKENS + 5_000);
        assert!(
            big.len() <= crate::constants::MAX_MEMORY_BODY_LEN,
            "token guard, not byte guard, must be exercised"
        );
        match check_embedding_input_size(&big) {
            Err(AppError::Validation(msg)) => assert!(msg.contains("tokens")),
            other => unreachable!("expected Validation(tokens), got: {other:?}"),
        }
    }

    #[test]
    fn check_embedding_input_size_rejects_above_byte_cap() {
        let huge = "x".repeat(crate::constants::MAX_MEMORY_BODY_LEN + 1);
        match check_embedding_input_size(&huge) {
            Err(AppError::Validation(msg)) => assert!(msg.contains("bytes")),
            other => unreachable!("expected Validation(bytes), got: {other:?}"),
        }
    }
}
