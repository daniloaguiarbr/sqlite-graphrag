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
    // SIGINT: ctrlc crate (cross-platform, the only signal that works on
    // both Unix and Windows without a tokio runtime).
    if let Err(e) = ctrlc::set_handler(move || {
        handle_first_signal("SIGINT", 2);
    }) {
        tracing::warn!(target: "signals", error = %e, "SIGINT handler registration failed");
    }

    // SIGTERM + SIGHUP: signal-hook (Unix only; Windows uses TerminateProcess
    // for SIGTERM equivalents and has no SIGHUP).
    #[cfg(unix)]
    {
        use std::sync::mpsc;
        let (tx, rx) = mpsc::channel::<i32>();

        let mut signals = match signal_hook::iterator::Signals::new([
            signal_hook::consts::SIGTERM,
            signal_hook::consts::SIGHUP,
        ]) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(target: "signals", error = %e, "SIGTERM/SIGHUP handler registration failed");
                return;
            }
        };

        // Detached thread: lives until process exit. The kernel kills it
        // automatically on process termination. We do NOT join it because
        // that would require the CLI to wait for an indeterminate signal.
        std::thread::Builder::new()
            .name("sqlite-graphrag-sigterm".into())
            .spawn(move || {
                for sig in signals.forever() {
                    if tx.send(sig).is_err() {
                        break;
                    }
                }
            })
            .inspect_err(|e| tracing::warn!(target: "signals", error = %e, "SIGTERM/SIGHUP handler thread spawn failed"))
            .ok();

        // Drain thread: blocks on the channel and calls the same handler
        // used by the SIGINT path. Synchronous main() can't await this,
        // but the channel is bounded so a 100ms wait is fine.
        std::thread::Builder::new()
            .name("sqlite-graphrag-sigterm-drain".into())
            .spawn(move || {
                while let Ok(sig) = rx.recv() {
                    let (name, number) = match sig {
                        libc::SIGTERM => ("SIGTERM", 15u8),
                        libc::SIGHUP => ("SIGHUP", 1u8),
                        _ => continue,
                    };
                    handle_first_signal(name, number);
                }
            })
            .inspect_err(|e| tracing::warn!(target: "signals", error = %e, "SIGTERM drain thread spawn failed"))
            .ok();
    }
}

/// First-signal handler shared by both SIGINT (via  crate) and
/// SIGTERM/SIGHUP (via signal-hook).
///
/// Idempotent: only the first invocation does work. The Ctrl+C handler is
/// synchronous (no tokio runtime is built in the LLM-only main path).
/// The SIGTERM/SIGHUP task is async but the underlying work is atomic via
/// the  fetch_add pattern.
fn handle_first_signal(signal_name: &'static str, signal_number: u8) {
    let prev = crate::SIGNAL_COUNT.fetch_add(1, Ordering::AcqRel);
    if prev != 0 {
        // Second signal: forced shutdown, NO I/O (G42/S8).
        std::process::exit(130);
    }
    crate::SHUTDOWN.store(true, Ordering::Release);
    crate::SIGNAL_NUMBER.store(signal_number, Ordering::Release);
    crate::cancel_token().cancel();

    // Best-effort stderr notice: closed pipe must NEVER abort (G42/S8).
    use std::io::Write;
    let _ = writeln!(
        std::io::stderr(),
        "shutdown signal received ({signal_name}); finishing current operation gracefully"
    );

    // GAP-002 (v1.0.82): emit JSON envelope to stdout before exit so that
    // piped consumers receive a parseable error with `code: 19`
    // (SHUTDOWN_EXIT_CODE) instead of an empty stdout that triggers
    // a parse error. Best-effort: if stdout is closed, writeln fails
    // silently.
    let envelope = format!(
        "{{\"error\":true,\"code\":19,\"message\":\"shutdown signal received; operation cancelled by {signal_name}\",\"signal\":\"{signal_name}\",\"graceful\":true}}"
    );
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{envelope}");
    let _ = stdout.flush();
}

#[cfg(test)]
mod tests {
    /// G42/S8 regression guard: the SHARED `handle_first_signal` function
    /// (called by both the SIGINT ctrlc closure and the SIGTERM/SIGHUP
    /// signal-hook drain) must not contain `eprintln!` or `tracing::warn!`
    /// — both can panic (and abort under `panic = "abort"`) when stderr
    /// is a closed pipe in an orphaned process.
    #[test]
    fn handler_source_has_no_panicking_io() {
        let source = include_str!("signals.rs");
        // The shared first-signal body starts at `fn handle_first_signal`
        // and ends at the closing brace of the function. We locate the
        // start of the next free-standing function or the test module
        // as the boundary.
        let body_start = source
            .find("fn handle_first_signal(")
            .expect("handle_first_signal must exist");
        let after_body = source[body_start..]
            .find("\nfn ")
            .or_else(|| source[body_start..].find("\n#[cfg(test)]"))
            .expect("body boundary not found");
        let body = &source[body_start..body_start + after_body];
        assert!(
            !body.contains("eprintln!"),
            "handle_first_signal must not use eprintln! (BrokenPipe panic, G42/C2)"
        );
        assert!(
            !body.contains("tracing::"),
            "handle_first_signal must not use tracing (stderr I/O can panic, G42/C2)"
        );
        assert!(
            body.contains("let _ = writeln!"),
            "first-signal notice must be a best-effort write"
        );
        assert!(
            body.contains("std::process::exit(130)"),
            "forced-exit path must remain in the shared handler"
        );
    }

    /// GAP-002 (v1.0.82) regression guard: the JSON envelope must use
    /// the deterministic SHUTDOWN_EXIT_CODE (19) so LLM agents can
    /// branch on a single code regardless of the triggering signal.
    #[test]
    fn envelope_uses_shutdown_exit_code() {
        let source = include_str!("signals.rs");
        // The envelope format string contains "code":19.
        assert!(
            source.contains("\\\"code\\\":19"),
            "shutdown envelope must embed SHUTDOWN_EXIT_CODE = 19"
        );
    }

    /// GAP-002 (v1.0.82) regression guard: `AppError::Shutdown` is the
    /// canonical error variant for shutdown. Constants and i18n are
    /// wired in lock-step — if SHUTDOWN_EXIT_CODE drifts away from 19,
    /// this test fails.
    #[test]
    fn shutdown_exit_code_is_19() {
        use crate::constants::SHUTDOWN_EXIT_CODE;
        assert_eq!(SHUTDOWN_EXIT_CODE, 19);
    }
}
