//! Cross-platform signal handling: SIGINT, SIGTERM, SIGHUP.

use std::sync::atomic::Ordering;

/// Registers the global shutdown handler for Ctrl+C / SIGTERM / SIGHUP.
///
/// First signal: sets [`SHUTDOWN`](crate::SHUTDOWN) flag, cancels the global
/// cancellation token and emits a best-effort notice on stderr.
///
/// Second signal: calls [`std::process::exit(130)`] for immediate termination
/// following Unix convention (128 + SIGINT=2) — with ZERO I/O on that path.
///
/// # G42/S8 — panic-free by contract
///
/// The pre-v1.0.79 handler used `eprintln!` (second signal) and
/// `tracing::warn!` (first signal). When the parent shell dies the CLI is
/// reparented to PID 1 and stderr becomes a CLOSED pipe; `eprintln!` then
/// panics with `BrokenPipe`, which under `panic = "abort"` becomes the
/// SIGABRT observed on the "ctrl-c" thread (G42/C2 crash report). This
/// handler therefore:
/// - writes the first-signal notice with `writeln!` and IGNORES any I/O
///   error (`let _ =`), never panicking;
/// - performs NO I/O at all on the forced-exit path.
///
/// BrokenPipe on stdout/stderr elsewhere is handled by resetting SIGPIPE
/// to its default disposition in `main` (clean exit 141, Unix convention).
pub fn register_shutdown_handler() {
    if let Err(e) = ctrlc::set_handler(move || {
        let prev = crate::SIGNAL_COUNT.fetch_add(1, Ordering::AcqRel);
        if prev == 0 {
            crate::SHUTDOWN.store(true, Ordering::Release);
            crate::SIGNAL_NUMBER.store(2, Ordering::Release);
            crate::cancel_token().cancel();
            // Best-effort notice: a closed stderr pipe must NEVER abort
            // the process (G42/S8). `writeln!` returns the io::Error that
            // the panicking macro would swallow into an abort; we
            // discard it explicitly.
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "shutdown signal received; finishing current operation gracefully"
            );
        } else {
            // Forced shutdown: NO I/O of any kind before exiting (a
            // write here was the exact SIGABRT trigger of G42/C2).
            std::process::exit(130);
        }
    }) {
        tracing::warn!(target: "signals", error = %e, "signal handler registration failed");
    }
}

#[cfg(test)]
mod tests {
    /// G42/S8 regression guard: the handler source must not contain
    /// `eprintln!` or `tracing::warn!` inside the signal closure — both
    /// can panic (and abort under `panic = "abort"`) when stderr is a
    /// closed pipe in an orphaned process.
    #[test]
    fn handler_source_has_no_panicking_io() {
        let source = include_str!("signals.rs");
        let closure_start = source
            .find("ctrlc::set_handler")
            .expect("handler registration must exist");
        // The closure body ends at the forced-exit call (searched FROM
        // the closure start — the doc comment above the fn also mentions
        // exit(130)); the registration-failure log AFTER the closure may
        // use tracing (it runs on the main thread with a live stderr).
        let closure_end = closure_start
            + source[closure_start..]
                .find("std::process::exit(130)")
                .expect("forced-exit path must exist");
        let closure_body = &source[closure_start..closure_end];
        assert!(
            !closure_body.contains("eprintln!"),
            "signal closure must not use eprintln! (BrokenPipe panic, G42/C2)"
        );
        assert!(
            !closure_body.contains("tracing::"),
            "signal closure must not use tracing (stderr I/O can panic, G42/C2)"
        );
        assert!(
            closure_body.contains("let _ = writeln!"),
            "first-signal notice must be a best-effort write"
        );
    }
}
