//! Centralized retry infrastructure with exponential backoff and half-jitter.
//!
//! Provides [`RetryConfig`](crate::retry::RetryConfig) with named constructors for each failure domain
//! (SQLite BUSY, LLM rate-limit, cold-start) and a [`compute_delay`](crate::retry::compute_delay) function
//! that applies the configured jitter strategy.

use std::time::Duration;

/// Configures retry behavior for a specific failure domain.
///
/// Use the named constructors ([`Self::sqlite_busy`], [`Self::llm_rate_limit`],
/// [`Self::cold_start`]) for pre-tuned policies. All timing values are in
/// milliseconds except `max_elapsed_secs` which is in seconds.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Base delay for the first retry attempt (ms).
    pub initial_delay_ms: u64,
    /// Upper bound on any single delay (ms).
    pub max_delay_ms: u64,
    /// Multiplicative factor applied per attempt.
    pub multiplier: u64,
    /// Hard cap on total attempts (0 = unlimited, use deadline).
    pub max_attempts: u32,
    /// Total elapsed wall-clock time before giving up (seconds).
    pub max_elapsed_secs: u64,
    /// Jitter strategy applied to computed delays.
    pub jitter: JitterKind,
}

/// Jitter strategy for randomizing retry delays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitterKind {
    /// No randomization — deterministic delay.
    None,
    /// Half-jitter: delay in [base/2, base). Guarantees minimum wait.
    Half,
    /// Full-jitter: delay in [0, base). Maximum spread.
    Full,
}

impl RetryConfig {
    /// SQLite BUSY retry: 5 attempts, 300ms base, half-jitter, 30s deadline.
    pub fn sqlite_busy() -> Self {
        Self {
            initial_delay_ms: 300,
            max_delay_ms: 4800,
            multiplier: 2,
            max_attempts: 5,
            max_elapsed_secs: 30,
            jitter: JitterKind::Half,
        }
    }

    /// LLM rate-limit retry: 60s base, 900s cap, half-jitter, 1h deadline.
    pub fn llm_rate_limit() -> Self {
        Self {
            initial_delay_ms: 60_000,
            max_delay_ms: 900_000,
            multiplier: 2,
            max_attempts: 20,
            max_elapsed_secs: 3600,
            jitter: JitterKind::Half,
        }
    }

    /// Cold-start retry: 2s base, 2 attempts, no jitter, 30s deadline.
    pub fn cold_start() -> Self {
        Self {
            initial_delay_ms: 2000,
            max_delay_ms: 4000,
            multiplier: 2,
            max_attempts: 2,
            max_elapsed_secs: 30,
            jitter: JitterKind::None,
        }
    }
}

/// Computes the delay for a given attempt using the config's jitter strategy.
///
/// # Formula
///
/// ```text
/// base = min(initial_delay_ms * multiplier^attempt, max_delay_ms)
/// delay = apply_jitter(base, jitter_kind)
/// ```
pub fn compute_delay(config: &RetryConfig, attempt: u32) -> Duration {
    let base = config
        .initial_delay_ms
        .saturating_mul(config.multiplier.saturating_pow(attempt))
        .min(config.max_delay_ms);

    let delay_ms = match config.jitter {
        JitterKind::None => base,
        JitterKind::Half => {
            let half = base / 2;
            if half == 0 {
                base
            } else {
                half + fastrand::u64(0..half)
            }
        }
        JitterKind::Full => {
            if base == 0 {
                0
            } else {
                fastrand::u64(0..base)
            }
        }
    };

    Duration::from_millis(delay_ms)
}

/// Returns `true` if the env var `SQLITE_GRAPHRAG_DISABLE_RETRY` is set to `1`.
///
/// When active, all retry loops should propagate the error immediately without
/// sleeping. Use during incidents to prevent retry storms.
pub fn is_kill_switch_active() -> bool {
    std::env::var("SQLITE_GRAPHRAG_DISABLE_RETRY").is_ok_and(|v| v == "1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_delay_half_jitter_in_bounds() {
        let cfg = RetryConfig::llm_rate_limit();
        for attempt in 0..5 {
            for _ in 0..100 {
                let d = compute_delay(&cfg, attempt);
                let base = cfg
                    .initial_delay_ms
                    .saturating_mul(cfg.multiplier.saturating_pow(attempt))
                    .min(cfg.max_delay_ms);
                let half = base / 2;
                assert!(d.as_millis() >= half as u128);
                assert!(d.as_millis() < base as u128);
            }
        }
    }

    #[test]
    fn compute_delay_no_jitter_is_deterministic() {
        let cfg = RetryConfig::cold_start();
        let d1 = compute_delay(&cfg, 0);
        let d2 = compute_delay(&cfg, 0);
        assert_eq!(d1, d2);
        assert_eq!(d1, Duration::from_millis(2000));
    }

    #[test]
    fn kill_switch_inactive_by_default() {
        std::env::remove_var("SQLITE_GRAPHRAG_DISABLE_RETRY");
        assert!(!is_kill_switch_active());
    }

    #[test]
    fn sqlite_busy_config_matches_constants() {
        let cfg = RetryConfig::sqlite_busy();
        assert_eq!(cfg.initial_delay_ms, 300);
        assert_eq!(cfg.max_attempts, 5);
        assert_eq!(cfg.max_elapsed_secs, 30);
    }

    #[test]
    fn llm_rate_limit_has_deadline() {
        let cfg = RetryConfig::llm_rate_limit();
        assert_eq!(cfg.max_elapsed_secs, 3600);
        assert_eq!(cfg.max_delay_ms, 900_000);
    }

    #[test]
    fn full_jitter_stays_below_base() {
        let cfg = RetryConfig {
            initial_delay_ms: 1000,
            max_delay_ms: 10_000,
            multiplier: 2,
            max_attempts: 5,
            max_elapsed_secs: 60,
            jitter: JitterKind::Full,
        };
        for attempt in 0..4 {
            for _ in 0..100 {
                let d = compute_delay(&cfg, attempt);
                let base = cfg
                    .initial_delay_ms
                    .saturating_mul(cfg.multiplier.saturating_pow(attempt))
                    .min(cfg.max_delay_ms);
                assert!(d.as_millis() < base as u128);
            }
        }
    }
}
