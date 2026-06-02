//! Cross-platform signal handling: SIGINT, SIGTERM, SIGHUP.

use std::sync::atomic::Ordering;

/// Registers the global shutdown handler for Ctrl+C / SIGTERM / SIGHUP.
///
/// First signal: sets [`SHUTDOWN`](crate::SHUTDOWN) flag, cancels the global
/// cancellation token, logs graceful shutdown intent.
///
/// Second signal: calls [`std::process::exit(130)`] for immediate termination
/// following Unix convention (128 + SIGINT=2).
pub fn register_shutdown_handler() {
    if let Err(e) = ctrlc::set_handler(move || {
        let prev = crate::SIGNAL_COUNT.fetch_add(1, Ordering::AcqRel);
        if prev == 0 {
            crate::SHUTDOWN.store(true, Ordering::Release);
            crate::SIGNAL_NUMBER.store(2, Ordering::Release);
            crate::cancel_token().cancel();
            tracing::warn!(
                target: "signals",
                "shutdown signal received; finishing current operation gracefully"
            );
        } else {
            eprintln!("\nForced shutdown (second signal received). Exiting immediately.");
            std::process::exit(130);
        }
    }) {
        tracing::warn!(target: "signals", error = %e, "signal handler registration failed");
    }
}
