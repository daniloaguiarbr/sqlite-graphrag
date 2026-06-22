//! G28: Reaper for orphan external processes.
//!
//! When the CLI crashes or is killed (SIGKILL, OOM, machine reset), child
//! processes spawned by `claude -p` or `codex exec` may be left running.
//! Without cleanup they accumulate as zombies that consume CPU, RAM, and
//! MCP-spawned subprocess trees (the 2026-06-03 incident: 1.877 processes
//! total, load average 276 on a 10-CPU host).
//!
//! [`scan_and_kill_orphans`] walks the process table at startup and
//! terminates any `claude` or `codex` invocation whose `PPID` is `1`
//! (reparented to `init`/`launchd` after the parent died) and that is
//! older than the `ORPHAN_MIN_AGE_SECS` constant. The scan is conservative: it only
//! kills processes that (a) match a known LLM CLI name, AND (b) are
//! orphaned, AND (c) are older than the threshold. A short-lived CLI
//! that is just starting up is left alone.

// v1.0.74: gate the orphan-reaper internals behind `cfg(unix)` so the
// constants and the `Duration` import are not flagged as dead code on
// Windows. The tests that reference them also need the same gate so the
// Windows test compilation does not break (the tests assert the values
// match the contract documented in CHANGELOG G28).
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
const ORPHAN_MIN_AGE_SECS: u64 = 60;

#[cfg(unix)]
const ORPHAN_SCAN_TARGETS: &[&str] = &["claude", "codex", "sqlite-graphrag"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReaperReport {
    /// Number of orphan processes detected.
    pub found: usize,
    /// Number of orphan processes successfully terminated.
    pub killed: usize,
    /// Number that we could not terminate (permission, ESRCH, etc).
    pub failed: usize,
    /// Elapsed wall time of the scan.
    pub elapsed_ms: u64,
}

/// Walks the process table and kills orphan LLM invocations.
///
/// The scan is best-effort and never panics: on any unexpected error it
/// logs the failure and returns a report with `killed = 0`.
pub fn scan_and_kill_orphans() -> ReaperReport {
    let start = std::time::Instant::now();
    let mut report = ReaperReport {
        found: 0,
        killed: 0,
        failed: 0,
        elapsed_ms: 0,
    };

    #[cfg(unix)]
    {
        if let Err(e) = scan_unix(&mut report) {
            tracing::warn!(target: "reaper", error = %e, "orphan scan failed");
        }
        // G42/S4 (v1.0.79): also remove stale `codex-home-{pid}`
        // isolation directories left behind by crashed invocations.
        clean_stale_codex_homes();
    }

    let max = crate::llm_slots::default_max_concurrency();
    let stale = crate::llm_slots::find_stale_slots(max);
    for slot_id in &stale {
        let _ = crate::llm_slots::force_release(*slot_id);
        tracing::info!(target: "reaper", slot_id, "released stale LLM slot (PID dead)");
    }

    #[cfg(not(unix))]
    {
        tracing::debug!(target: "reaper", "orphan scan is a no-op on non-Unix platforms");
    }

    report.elapsed_ms = start.elapsed().as_millis() as u64;
    if report.killed > 0 {
        tracing::warn!(
            target: "reaper",
            found = report.found,
            killed = report.killed,
            failed = report.failed,
            "reaped orphan LLM subprocesses"
        );
    } else {
        tracing::info!(target: "reaper", found = report.found, "no orphan LLM subprocesses detected");
    }
    report
}

#[cfg(unix)]
fn scan_unix(report: &mut ReaperReport) -> std::io::Result<()> {
    use std::fs;
    use std::path::Path;

    let proc = Path::new("/proc");
    let entries = fs::read_dir(proc)?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if !name_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let pid: i32 = match name_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if pid == std::process::id() as i32 {
            continue;
        }

        let stat_path = entry.path().join("stat");
        let stat = match fs::read_to_string(&stat_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // /proc/[pid]/stat has the form: `pid (comm) state ppid ...`
        // The comm field can contain spaces and parens; the last `)`
        // separates the comm from the rest.
        let Some(close_paren) = stat.rfind(')') else {
            continue;
        };
        let after = &stat[close_paren + 1..];
        let mut parts = after.split_whitespace();
        // parts[0] = state (e.g. "R"), parts[1] = ppid, parts[2] = pgrp, ...
        let state = parts.next().unwrap_or("");
        let ppid: i32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(-1);

        // Only target processes orphaned to init (PPID 1 on Linux/Unix
        // when the parent is gone) or whose parent is also dead.
        if ppid != 1 {
            continue;
        }

        // Skip zombies (state Z) — they need no kill.
        if state.starts_with('Z') {
            continue;
        }

        // Resolve the comm field. proc/[pid]/comm is the short program
        // name (no path); we use it instead of parsing the bracketed
        // comm from stat to avoid encoding edge cases.
        let comm_path = entry.path().join("comm");
        let comm = match fs::read_to_string(&comm_path) {
            Ok(s) => s.trim().to_string(),
            Err(_) => continue,
        };

        if !ORPHAN_SCAN_TARGETS.iter().any(|t| comm == *t) {
            continue;
        }

        // Age check: skip processes that just spawned (under 60s old) so
        // we never race with a concurrent CLI invocation.
        let age_ok = check_process_age(pid, ORPHAN_MIN_AGE_SECS);
        if !age_ok {
            continue;
        }

        report.found += 1;
        match terminate_pid(pid) {
            Ok(()) => {
                report.killed += 1;
                tracing::info!(target: "reaper", pid, comm = %comm, "killed orphan LLM subprocess");
            }
            Err(e) => {
                report.failed += 1;
                tracing::warn!(target: "reaper", pid, comm = %comm, error = %e, "failed to kill orphan");
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn check_process_age(pid: i32, min_age_secs: u64) -> bool {
    use std::fs;
    // /proc/[pid]/stat field 22 is start_time in clock ticks since boot.
    // We instead use the simpler heuristic: stat file mtime.
    let stat_path = std::path::Path::new("/proc")
        .join(pid.to_string())
        .join("stat");
    let Ok(meta) = fs::metadata(&stat_path) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    let Ok(elapsed) = std::time::SystemTime::now().duration_since(modified) else {
        return false;
    };
    elapsed >= Duration::from_secs(min_age_secs)
}

/// G42/S4 (v1.0.79): removes `~/.local/share/sqlite-graphrag/codex-home-{pid}`
/// directories whose owning PID is no longer alive.
///
/// `prepare_isolated_codex_home` creates one directory per process and
/// never deletes it (deleting on exit would race a concurrent invocation
/// re-using the same PID number). The reaper is the right owner for the
/// cleanup: at startup it removes every stale dir in one sweep.
///
/// Best-effort and conservative: a dir is removed only when (a) the name
/// parses as `codex-home-<pid>`, (b) `kill(pid, 0)` reports the process
/// gone (ESRCH), and (c) the pid is not our own.
#[cfg(unix)]
fn clean_stale_codex_homes() {
    let Ok(home) = std::env::var("HOME") else {
        return;
    };
    let base = std::path::Path::new(&home).join(".local/share/sqlite-graphrag");
    let Ok(entries) = std::fs::read_dir(&base) else {
        return;
    };
    let mut removed = 0usize;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        let Some(pid_str) = name_str.strip_prefix("codex-home-") else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<i32>() else {
            continue;
        };
        if pid == std::process::id() as i32 {
            continue;
        }
        // kill(pid, 0): signal 0 performs the permission/existence check
        // without delivering a signal. ESRCH means the process is gone.
        let alive = unsafe { libc::kill(pid, 0) } == 0
            || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH);
        if alive {
            continue;
        }
        if std::fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        tracing::info!(target: "reaper", removed, "removed stale codex-home isolation dirs");
    }
}

#[cfg(unix)]
fn terminate_pid(pid: i32) -> std::io::Result<()> {
    // SIGTERM first; if the process ignores it for >2s, the caller can
    // escalate to SIGKILL. For the reaper we send TERM and return; a
    // follow-up sweep can send KILL if needed.
    let rc = unsafe { libc::kill(pid, libc::SIGTERM) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaper_report_starts_zeroed() {
        let r = ReaperReport {
            found: 0,
            killed: 0,
            failed: 0,
            elapsed_ms: 0,
        };
        assert_eq!(r.found, 0);
        assert_eq!(r.killed, 0);
        assert_eq!(r.failed, 0);
    }

    #[cfg(unix)]
    #[test]
    fn orphan_min_age_is_one_minute() {
        // G28: the threshold of 60s is the safety margin that prevents
        // a CLI invocation from killing a concurrent peer that just
        // started 5s ago.
        assert_eq!(ORPHAN_MIN_AGE_SECS, 60);
    }

    #[cfg(unix)]
    #[test]
    fn orphan_targets_include_claude_and_codex() {
        assert!(ORPHAN_SCAN_TARGETS.contains(&"claude"));
        assert!(ORPHAN_SCAN_TARGETS.contains(&"codex"));
    }

    #[cfg(unix)]
    #[test]
    fn orphan_targets_include_sqlite_graphrag() {
        assert!(ORPHAN_SCAN_TARGETS.contains(&"sqlite-graphrag"));
    }

    #[test]
    fn scan_completes_without_panic_on_linux() {
        // Just ensure the function returns a ReaperReport on the test
        // host. On Linux CI we may be PID 1 in containers; the report
        // will simply have found=0.
        let r = scan_and_kill_orphans();
        assert!(r.elapsed_ms < 30_000, "scan must finish in <30s");
    }
}
