//! Stdin reader with timeout to prevent indefinite blocking when the
//! upstream pipe is held open without sending data.
//!
//! Used by `remember --body-stdin` and `edit` body input to enforce a
//! deadline (default 60s). When the timeout fires, the spawned reader
//! thread is leaked because `std::io::stdin()` cannot be cancelled
//! from outside; this is acceptable in error scenarios because the
//! process is about to exit anyway.
//!
//! When stdin is attached to a terminal (interactive TTY), the function
//! returns an `AppError::Internal` immediately with an actionable message
//! instead of blocking for up to `secs` seconds waiting for EOF.

use crate::errors::AppError;
use std::io::{IsTerminal, Read};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Reads stdin to a `String` with a hard deadline.
///
/// Returns `AppError::Internal` immediately when stdin is attached to a
/// terminal (TTY) — the caller must redirect data via a pipe or file.
///
/// # Errors
/// Returns `AppError::Internal` when stdin is a TTY, when the read does
/// not finish within `secs` seconds, or `AppError::Io` when the
/// underlying read fails.
pub fn read_stdin_with_timeout(secs: u64) -> Result<String, AppError> {
    if std::io::stdin().is_terminal() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "stdin is attached to a terminal; pipe data via stdin \
             (e.g. `echo ... | sqlite-graphrag ...` or `... < file`) \
             or use --body instead of --body-stdin"
        )));
    }
    let (tx, rx) = mpsc::channel::<std::io::Result<String>>();
    thread::spawn(move || {
        let mut buf = String::new();
        let result = std::io::stdin().read_to_string(&mut buf).map(|_| buf);
        let _ = tx.send(result);
    });
    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(Ok(buf)) => Ok(buf),
        Ok(Err(e)) => Err(AppError::Io(e)),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(AppError::Internal(anyhow::anyhow!(
            "stdin read timed out after {secs}s; pipe must close within timeout window"
        ))),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(AppError::Internal(anyhow::anyhow!(
            "stdin reader thread disconnected unexpectedly"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    // Note: we cannot easily test the success path because tests inherit stdin
    // from the test runner. We only assert the timeout path here.
    #[test]
    fn read_stdin_with_timeout_returns_internal_error_on_timeout() {
        // 1s is enough — stdin in test runner is typically a tty or pipe with no input.
        let start = Instant::now();
        let result = read_stdin_with_timeout(1);
        let elapsed = start.elapsed();
        // We expect either a timeout (most cases), an immediate TTY error, or a
        // successful EOF read (rare in CI environments).
        match result {
            Err(AppError::Internal(e)) => {
                let msg = e.to_string();
                // Accept both the TTY-detected error and the timeout error.
                assert!(
                    msg.contains("timed out") || msg.contains("terminal"),
                    "unexpected internal error: {msg}"
                );
                // TTY path exits immediately; timeout path takes ~1s.
                assert!(elapsed.as_secs_f64() < 2.5);
            }
            Ok(_) | Err(AppError::Io(_)) => {
                // EOF reached before timeout — also acceptable in CI environments.
            }
            Err(other) => panic!("unexpected error variant: {other:?}"),
        }
    }

    // TTY detection cannot be simulated in unit tests because the test runner
    // always provides a non-TTY stdin (pipe). Empirical validation:
    //   cargo run --release -- remember --body-stdin --name h1-test
    // Expected: exits in <2s with "stdin is attached to a terminal" message.
}
