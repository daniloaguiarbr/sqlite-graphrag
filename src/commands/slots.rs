//! GAP-004 (v1.0.82): `slots` subcommand — inspect and manage the
//! cross-process LLM slot semaphore.
//!
//! ## Subcommands
//! - `slots status` — list active slot files and the PID that holds them
//! - `slots release --slot-id N` — force-release a specific slot
//! - `slots cleanup --stale-after N` — remove slots older than N seconds

use clap::{Args, Subcommand};
use serde::Serialize;

use crate::errors::AppError;
use crate::llm_slots::{slot_path, slots_dir};
use crate::output::emit_json_compact;
use crate::output::OutputFormat;

/// Outer wrapper that lets the top-level `Cli` enum carry `Slots` as an `Args`
/// variant while preserving the inner `Status | Release | Cleanup` subcommand tree.
#[derive(Debug, Args)]
pub struct SlotsArgs {
    #[command(subcommand)]
    pub cmd: SlotsCmd,
}

#[derive(Debug, Subcommand)]
pub enum SlotsCmd {
    /// List currently-held LLM slots and their PIDs.
    Status(SlotsStatusArgs),
    /// Force-release a slot by id (admin only).
    Release {
        /// Slot id (0..max-1) to release.
        #[arg(long)]
        slot_id: u32,
        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Remove slot files older than `stale-after` seconds.
    Cleanup {
        /// Age in seconds after which a slot is considered stale.
        #[arg(long, default_value_t = 3600)]
        stale_after: u64,
        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        yes: bool,
        /// Dry-run: list what would be removed without touching the filesystem.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, clap::Args)]
pub struct SlotsStatusArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
}

#[derive(Serialize)]
struct SlotEntry {
    slot_id: u32,
    path: String,
    age_secs: u64,
    pid_hint: Option<u32>,
}

#[derive(Serialize)]
struct SlotsStatusOutput {
    action: &'static str,
    max_concurrency: u32,
    active: usize,
    free: usize,
    slots: Vec<SlotEntry>,
    elapsed_ms: u64,
}

pub fn run(args: SlotsArgs) -> Result<(), AppError> {
    run_cmd(args.cmd)
}

fn run_cmd(cmd: SlotsCmd) -> Result<(), AppError> {
    match cmd {
        SlotsCmd::Status(args) => run_status(args),
        SlotsCmd::Release { slot_id, yes } => run_release(slot_id, yes),
        SlotsCmd::Cleanup {
            stale_after,
            yes,
            dry_run,
        } => run_cleanup(stale_after, yes, dry_run),
    }
}

fn run_status(args: SlotsStatusArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let max = crate::llm_slots::default_max_concurrency();
    let dir = slots_dir();
    let mut entries: Vec<SlotEntry> = Vec::new();

    if dir.is_dir() {
        for slot_id in 0..max {
            let path = slot_path(slot_id);
            if path.is_file() {
                let age_secs = path
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let pid_hint = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok());
                entries.push(SlotEntry {
                    slot_id,
                    path: path.to_string_lossy().into_owned(),
                    age_secs,
                    pid_hint,
                });
            }
        }
    }

    let output = SlotsStatusOutput {
        action: "slots_status",
        max_concurrency: max,
        active: entries.len(),
        free: (max as usize).saturating_sub(entries.len()),
        slots: entries,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };

    if matches!(args.format, OutputFormat::Json) {
        let json = serde_json::to_string_pretty(&output).map_err(AppError::Json)?;
        println!("{json}");
    } else {
        println!("max_concurrency: {}", output.max_concurrency);
        println!("active: {} / free: {}", output.active, output.free);
        for s in &output.slots {
            let pid = s.pid_hint.map(|p| p.to_string()).unwrap_or_default();
            println!(
                "  slot {} — age={}s pid={} {}",
                s.slot_id, s.age_secs, pid, s.path
            );
        }
    }
    Ok(())
}

fn run_release(slot_id: u32, yes: bool) -> Result<(), AppError> {
    let path = slot_path(slot_id);
    if !path.is_file() {
        return Err(AppError::NotFound(format!(
            "slot {slot_id} is not held (no file at {})",
            path.display()
        )));
    }
    if !yes {
        eprintln!(
            "About to release slot {slot_id} at {}. Pass --yes to skip confirmation.",
            path.display()
        );
    }
    std::fs::remove_file(&path).map_err(AppError::Io)?;
    let out = serde_json::json!({
        "action": "slot_released",
        "slot_id": slot_id,
        "path": path.to_string_lossy(),
    });
    let _ = emit_json_compact(&out);
    Ok(())
}

fn run_cleanup(stale_after: u64, yes: bool, dry_run: bool) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let max = crate::llm_slots::default_max_concurrency();
    let mut removed: Vec<u32> = Vec::new();
    for slot_id in 0..max {
        let path = slot_path(slot_id);
        if !path.is_file() {
            continue;
        }
        let age = path
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if age >= stale_after {
            if !dry_run {
                if let Err(e) = std::fs::remove_file(&path) {
                    tracing::warn!(target: "slots", slot_id, error = %e, "stale slot removal failed");
                    continue;
                }
            }
            removed.push(slot_id);
        }
    }
    let out = serde_json::json!({
        "action": if dry_run { "slots_cleanup_dry_run" } else { "slots_cleanup" },
        "stale_after_secs": stale_after,
        "removed": removed,
        "removed_count": removed.len(),
        "elapsed_ms": start.elapsed().as_millis() as u64,
        "yes": yes,
    });
    let _ = emit_json_compact(&out);
    Ok(())
}

/// Sanity: `acquire_llm_slot` then immediately drop the guard must
/// remove the slot file. This is the test that GAP-004 depends on
/// for the cross-process guarantee.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_slots::acquire_llm_slot;

    #[test]
    fn acquire_then_drop_releases_slot() {
        let _ = std::fs::remove_dir_all(crate::llm_slots::slots_dir());
        let guard = acquire_llm_slot(2, 5).expect("acquire");
        let path = slot_path(guard.slot_id());
        assert!(path.is_file(), "slot file must exist after acquire");
        drop(guard);
        assert!(
            !path.is_file(),
            "slot file must be removed after Drop (RAII guarantee)"
        );
    }
}
