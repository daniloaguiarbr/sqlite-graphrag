//! Centralized retry infrastructure with exponential backoff and half-jitter.
//!
//! Provides [`RetryConfig`](crate::retry::RetryConfig) with named constructors for each failure domain
//! (SQLite BUSY, LLM rate-limit, cold-start) and a [`compute_delay`](crate::retry::compute_delay) function
//! that applies the configured jitter strategy.

use std::time::{Duration, Instant};

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

// ---------------------------------------------------------------------------
// Circuit Breaker (G28-D, v1.0.68)
// ---------------------------------------------------------------------------

/// Outcome of a single retry attempt, used to feed a [`CircuitBreaker`].
///
/// We keep this intentionally narrow: rate-limit / timeout errors are
/// TRANSIENT and should NOT count toward the breaker; everything else
/// counts as a HARD failure that contributes to opening the breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// Transient error: counts as a successful iteration, does NOT trip the breaker.
    /// Examples: `AppError::RateLimited`, `AppError::Timeout`, `AppError::DbBusy`.
    Transient,
    /// Hard failure: counts toward the breaker's failure threshold.
    /// Examples: `AppError::Validation`, `AppError::Conflict`,
    /// `AppError::Embedding`, `AppError::Internal`.
    HardFailure,
    /// Successful iteration: resets the consecutive-failure counter.
    Success,
}

/// Counts consecutive hard failures and trips open after a threshold.
///
/// G28-D (v1.0.68): caps `enrich --retry-failed` and `ingest --retry-failed`
/// loops so persistent failures (e.g., LLM provider returning the same
/// 4xx for hours) cannot run unbounded.  After `threshold` consecutive
/// [`AttemptOutcome::HardFailure`] outcomes, `record` returns `true` and
/// the caller is expected to abort with `AppError::CircuitBreakerOpen`.
///
/// Rate-limited / transient errors are explicitly NOT counted, so a
/// provider that throttles but eventually recovers will not trip the
/// breaker.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    threshold: u32,
    cooldown: Duration,
    consecutive_failures: u32,
    open_until: Option<Instant>,
}

impl CircuitBreaker {
    /// Creates a breaker that opens after `threshold` consecutive hard
    /// failures and stays open for `cooldown` after the last failure.
    pub fn new(threshold: u32, cooldown: Duration) -> Self {
        Self {
            threshold,
            cooldown,
            consecutive_failures: 0,
            open_until: None,
        }
    }

    /// Records one attempt outcome.
    ///
    /// Returns `true` when the breaker is now open and the caller must
    /// abort the job.  Returns `false` when the attempt should continue.
    pub fn record(&mut self, outcome: AttemptOutcome) -> bool {
        match outcome {
            AttemptOutcome::Success | AttemptOutcome::Transient => {
                self.consecutive_failures = 0;
                false
            }
            AttemptOutcome::HardFailure => {
                self.consecutive_failures = self.consecutive_failures.saturating_add(1);
                if self.consecutive_failures >= self.threshold.max(1) {
                    self.open_until = Some(Instant::now() + self.cooldown);
                    tracing::error!(
                        target: "circuit_breaker",
                        consecutive_failures = self.consecutive_failures,
                        threshold = self.threshold,
                        cooldown_secs = self.cooldown.as_secs(),
                        "circuit breaker opened — aborting job"
                    );
                    true
                } else {
                    false
                }
            }
        }
    }

    /// `true` when the breaker is currently open (and not yet cooled down).
    pub fn is_open(&self) -> bool {
        self.open_until
            .map(|deadline| Instant::now() < deadline)
            .unwrap_or(false)
    }

    /// Resets the breaker to closed state.
    pub fn reset(&mut self) {
        self.consecutive_failures = 0;
        self.open_until = None;
    }

    /// Returns the number of consecutive HardFailure outcomes observed
    /// since the last success or reset. Public so callers can include
    /// the value in their abort log line.
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }
}

#[cfg(test)]
mod circuit_breaker_tests {
    use super::*;

    #[test]
    fn opens_after_threshold_consecutive_hard_failures() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(60));
        assert!(!cb.record(AttemptOutcome::HardFailure));
        assert!(!cb.record(AttemptOutcome::HardFailure));
        assert!(cb.record(AttemptOutcome::HardFailure));
        assert!(cb.is_open());
    }

    #[test]
    fn ignores_transient_errors() {
        let mut cb = CircuitBreaker::new(2, Duration::from_secs(60));
        // 10 transients in a row should never open the breaker.
        for _ in 0..10 {
            assert!(!cb.record(AttemptOutcome::Transient));
        }
        assert!(!cb.is_open());
    }

    #[test]
    fn success_resets_consecutive_failures() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(60));
        cb.record(AttemptOutcome::HardFailure);
        cb.record(AttemptOutcome::HardFailure);
        cb.record(AttemptOutcome::Success);
        assert!(!cb.record(AttemptOutcome::HardFailure));
        assert!(!cb.is_open());
    }
}
