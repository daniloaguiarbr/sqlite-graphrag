//! Handler for the `cache` CLI subcommand and its nested operations.
//!
//! Manages cached resources such as the multilingual-e5-small ONNX model and
//! the BERT NER classifier downloaded into the XDG cache directory on first
//! `init`. Used to reclaim disk space or recover from corrupted cache state.

use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use serde::Serialize;

#[derive(clap::Args)]
pub struct CacheArgs {
    #[command(subcommand)]
    pub command: CacheCommands,
}

#[derive(clap::Subcommand)]
pub enum CacheCommands {
    /// Remove cached embedding/NER model files (forces re-download on next `init`).
    ClearModels(ClearModelsArgs),
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
