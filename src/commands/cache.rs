//! Handler for the `cache` CLI subcommand and its nested operations.
//!
//! Manages cached resources such as the multilingual-e5-small ONNX model and
//! the GLiNER NER classifier downloaded into the XDG cache directory on first
//! `init`. Used to reclaim disk space or recover from corrupted cache state.

use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Remove cached embedding/NER model files (forces re-download on next init)\n  \
    sqlite-graphrag cache clear-models\n\n  \
    # Skip the confirmation prompt\n  \
    sqlite-graphrag cache clear-models --yes\n\n  \
    # List cached model files\n  \
    sqlite-graphrag cache list\n\n  \
    # List cached model files as JSON\n  \
    sqlite-graphrag cache list --json")]
pub struct CacheArgs {
    #[command(subcommand)]
    pub command: CacheCommands,
}

#[derive(clap::Subcommand)]
pub enum CacheCommands {
    /// Remove cached embedding/NER model files (forces re-download on next `init`).
    ClearModels(ClearModelsArgs),
    /// List cached embedding/NER model files with sizes and total disk usage.
    List(CacheListArgs),
}

#[derive(clap::Args)]
pub struct CacheListArgs {
    /// Output as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args)]
pub struct ClearModelsArgs {
    /// Skip confirmation prompt and proceed with deletion immediately.
    #[arg(long, default_value_t = false, help = "Skip confirmation prompt")]
    pub yes: bool,
    /// Output format: json (default), text, or markdown.
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
}

#[derive(Serialize)]
struct ClearModelsResponse {
    cache_path: String,
    existed: bool,
    bytes_freed: u64,
    files_removed: usize,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: CacheArgs) -> Result<(), AppError> {
    match args.command {
        CacheCommands::ClearModels(a) => clear_models(a),
        CacheCommands::List(a) => run_list(a),
    }
}

fn clear_models(args: ClearModelsArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    // Resolve the canonical models directory through AppPaths to honour
    // SQLITE_GRAPHRAG_CACHE_DIR overrides used by tests and CI.
    let paths = AppPaths::resolve(None)?;
    let models_dir = paths.models.clone();

    if !args.yes {
        // For machine consumption stay deterministic: refuse without --yes.
        return Err(AppError::Validation(
            "destructive operation: pass --yes to confirm cache deletion".to_string(),
        ));
    }

    let existed = models_dir.exists();
    let mut bytes_freed: u64 = 0;
    let mut files_removed: usize = 0;

    if existed {
        bytes_freed = dir_size(&models_dir).unwrap_or(0);
        files_removed = count_files(&models_dir).unwrap_or(0);
        std::fs::remove_dir_all(&models_dir)?;
    }

    output::emit_json(&ClearModelsResponse {
        cache_path: models_dir.display().to_string(),
        existed,
        bytes_freed,
        files_removed,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[derive(Serialize)]
struct CacheFileEntry {
    name: String,
    path: String,
    size_bytes: u64,
    modified_at: String,
}

#[derive(Serialize)]
struct CacheListResponse {
    schema_version: u32,
    cache_path: String,
    files: Vec<CacheFileEntry>,
    total_bytes: u64,
    total_human: String,
}

fn format_bytes_human(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn collect_cache_files(
    dir: &std::path::Path,
    base: &std::path::Path,
    entries: &mut Vec<CacheFileEntry>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        let path = entry.path();
        if meta.is_dir() {
            collect_cache_files(&path, base, entries)?;
        } else {
            let size_bytes = meta.len();
            let relative = path.strip_prefix(base).unwrap_or(&path);
            let name = relative.to_string_lossy().into_owned();
            let modified_at = meta
                .modified()
                .ok()
                .map(|t| {
                    let secs = t
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    // Format as RFC 3339 (UTC) without chrono dependency.
                    let secs_i64 = secs as i64;
                    let (y, mo, d, h, mi, s) = epoch_to_ymd_hms(secs_i64);
                    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
                })
                .unwrap_or_else(|| "unknown".to_string());
            entries.push(CacheFileEntry {
                name,
                path: path.display().to_string(),
                size_bytes,
                modified_at,
            });
        }
    }
    Ok(())
}

/// Converts Unix epoch seconds to (year, month, day, hour, minute, second) UTC.
fn epoch_to_ymd_hms(secs: i64) -> (i32, u8, u8, u8, u8, u8) {
    let s = (secs % 60) as u8;
    let total_min = secs / 60;
    let mi = (total_min % 60) as u8;
    let total_h = total_min / 60;
    let h = (total_h % 24) as u8;
    let mut days = total_h / 24;
    // Compute year/month/day from days since epoch (1970-01-01).
    let mut y = 1970i32;
    loop {
        let days_in_y = if is_leap(y) { 366 } else { 365 };
        if days < days_in_y {
            break;
        }
        days -= days_in_y;
        y += 1;
    }
    let leap = is_leap(y);
    let months = [
        31u8,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut mo = 1u8;
    for &days_in_m in &months {
        if days < days_in_m as i64 {
            break;
        }
        days -= days_in_m as i64;
        mo += 1;
    }
    let d = (days + 1) as u8;
    (y, mo, d, h, mi, s)
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn run_list(args: CacheListArgs) -> Result<(), AppError> {
    let paths = AppPaths::resolve(None)?;
    let models_dir = &paths.models;

    let mut entries: Vec<CacheFileEntry> = Vec::with_capacity(4);
    if models_dir.exists() {
        collect_cache_files(models_dir, models_dir, &mut entries).map_err(AppError::Io)?;
    }

    entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    let total_bytes: u64 = entries.iter().map(|e| e.size_bytes).sum();
    let total_human = format_bytes_human(total_bytes);
    let n_files = entries.len();

    if args.json {
        output::emit_json(&CacheListResponse {
            schema_version: 1,
            cache_path: models_dir.display().to_string(),
            files: entries,
            total_bytes,
            total_human,
        })?;
    } else if entries.is_empty() {
        output::emit_text("(empty)");
    } else {
        for e in &entries {
            output::emit_text(&format!(
                "{:<40} {:>10}  {}",
                e.name,
                format_bytes_human(e.size_bytes),
                e.modified_at
            ));
        }
        output::emit_text(&format!("\nTOTAL: {n_files} files, {total_human}"));
    }

    Ok(())
}

fn dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_dir() {
            total = total.saturating_add(dir_size(&entry.path()).unwrap_or(0));
        } else {
            total = total.saturating_add(meta.len());
        }
    }
    Ok(total)
}

fn count_files(path: &std::path::Path) -> std::io::Result<usize> {
    let mut count = 0usize;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_dir() {
            count = count.saturating_add(count_files(&entry.path()).unwrap_or(0));
        } else {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_models_response_serializes_all_fields() {
        let resp = ClearModelsResponse {
            cache_path: "/tmp/sqlite-graphrag/models".to_string(),
            existed: true,
            bytes_freed: 465_000_000,
            files_removed: 14,
            elapsed_ms: 12,
        };
        let json = serde_json::to_value(&resp).expect("serialization");
        assert_eq!(json["existed"], true);
        assert_eq!(json["bytes_freed"], 465_000_000u64);
        assert_eq!(json["files_removed"], 14);
        assert_eq!(json["elapsed_ms"], 12);
    }

    #[test]
    fn clear_models_without_yes_returns_validation_error() {
        let args = ClearModelsArgs {
            yes: false,
            json: false,
        };
        let result = clear_models(args);
        assert!(matches!(result, Err(AppError::Validation(_))));
    }
}
