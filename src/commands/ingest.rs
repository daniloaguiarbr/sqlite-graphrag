//! Handler for the `ingest` CLI subcommand.
//!
//! Bulk-ingests every file under a directory that matches a glob pattern.
//! Each matched file is persisted as a separate memory by invoking the same
//! `sqlite-graphrag remember --body-file` pipeline as a child process. Memory
//! names are derived from file basenames (kebab-case, lowercase, ASCII
//! alphanumerics + hyphens). Running each ingestion as a child process keeps
//! `remember` untouched and naturally honours the same concurrency slot
//! semantics as standalone `remember` invocations.
//!
//! Output is line-delimited JSON: one object per processed file (success or
//! error), followed by a final summary object. Designed for streaming
//! consumption by agents.

use crate::cli::MemoryType;
use crate::errors::AppError;
use crate::output::{self, JsonOutputFormat};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Ingest every Markdown file under ./docs as `document` memories\n  \
    sqlite-graphrag ingest ./docs --type document\n\n  \
    # Ingest .txt files recursively under ./notes\n  \
    sqlite-graphrag ingest ./notes --type note --pattern '*.txt' --recursive\n\n  \
    # Skip BERT NER auto-extraction for faster bulk import\n  \
    sqlite-graphrag ingest ./big-corpus --type reference --skip-extraction\n\n  \
NOTES:\n  \
    Each file becomes a separate memory. Names derive from file basenames\n  \
    (kebab-case, lowercase, ASCII). Output is NDJSON: one JSON object per file,\n  \
    followed by a final summary line with counts. Per-file errors are reported\n  \
    inline and processing continues unless --fail-fast is set.")]
pub struct IngestArgs {
    /// Directory containing files to ingest.
    #[arg(value_name = "DIR")]
    pub dir: PathBuf,

    /// Memory type stored in `memories.type` for every ingested file.
    #[arg(long, value_enum)]
    pub r#type: MemoryType,

    /// Glob pattern matched against file basenames (default: `*.md`). Supports
    /// `*.<ext>`, `<prefix>*`, and exact filename match.
    #[arg(long, default_value = "*.md")]
    pub pattern: String,

    /// Recurse into subdirectories.
    #[arg(long, default_value_t = false)]
    pub recursive: bool,

    /// Disable automatic BERT NER entity/relationship extraction (faster bulk import).
    #[arg(long, default_value_t = false)]
    pub skip_extraction: bool,

    /// Stop on first per-file error instead of continuing with the next file.
    #[arg(long, default_value_t = false)]
    pub fail_fast: bool,

    /// Maximum number of files to ingest (safety cap to prevent runaway ingestion).
    #[arg(long, default_value_t = 10_000)]
    pub max_files: usize,

    /// Namespace for the ingested memories.
    #[arg(long)]
    pub namespace: Option<String>,

    /// Database path. Falls back to `SQLITE_GRAPHRAG_DB_PATH`, then `./graphrag.sqlite`.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,

    #[arg(long, value_enum, default_value_t = JsonOutputFormat::Json)]
    pub format: JsonOutputFormat,

    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
}

#[derive(Serialize)]
struct IngestFileEvent<'a> {
    file: &'a str,
    name: &'a str,
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<String>,
}

#[derive(Serialize)]
struct IngestSummary {
    summary: bool,
    dir: String,
    pattern: String,
    recursive: bool,
    files_total: usize,
    files_succeeded: usize,
    files_failed: usize,
    files_skipped: usize,
    elapsed_ms: u64,
}

pub fn run(args: IngestArgs) -> Result<(), AppError> {
    let started = std::time::Instant::now();

    if !args.dir.exists() {
        return Err(AppError::NotFound(format!(
            "directory not found: {}",
            args.dir.display()
        )));
    }
    if !args.dir.is_dir() {
        return Err(AppError::Validation(format!(
            "path is not a directory: {}",
            args.dir.display()
        )));
    }

    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(&args.dir, &args.pattern, args.recursive, &mut files)?;
    files.sort();

    if files.len() > args.max_files {
        return Err(AppError::Validation(format!(
            "found {} files matching pattern, exceeds --max-files cap of {} (raise the cap or narrow the pattern)",
            files.len(),
            args.max_files
        )));
    }

    let mut succeeded: usize = 0;
    let mut failed: usize = 0;
    let mut skipped: usize = 0;
    let total = files.len();

    let exe = std::env::current_exe().map_err(|e| {
        AppError::Internal(anyhow::anyhow!("could not resolve current executable: {e}"))
    })?;
    let type_str = args.r#type.as_str();

    for path in &files {
        let file_str = path.to_string_lossy().into_owned();
        let derived_name = derive_kebab_name(path);

        if derived_name.is_empty() {
            output::emit_json(&IngestFileEvent {
                file: &file_str,
                name: "",
                status: "skipped",
                error: Some(
                    "could not derive a non-empty kebab-case name from filename".to_string(),
                ),
                memory_id: None,
                action: None,
            })?;
            skipped += 1;
            continue;
        }

        let description = format!("ingested from {}", path.display());

        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("remember")
            .arg("--name")
            .arg(&derived_name)
            .arg("--type")
            .arg(type_str)
            .arg("--description")
            .arg(&description)
            .arg("--body-file")
            .arg(path);
        if args.skip_extraction {
            cmd.arg("--skip-extraction");
        }
        if let Some(ns) = &args.namespace {
            cmd.arg("--namespace").arg(ns);
        }
        if let Some(db) = &args.db {
            cmd.arg("--db").arg(db);
        }
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output_res = cmd.output().map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "failed to spawn child remember process: {e}"
            ))
        })?;

        if output_res.status.success() {
            let memory_id = parse_memory_id(&output_res.stdout);
            let action = parse_action(&output_res.stdout);
            output::emit_json(&IngestFileEvent {
                file: &file_str,
                name: &derived_name,
                status: "indexed",
                error: None,
                memory_id,
                action,
            })?;
            succeeded += 1;
        } else {
            let err_msg = first_error_line(&output_res.stderr);
            output::emit_json(&IngestFileEvent {
                file: &file_str,
                name: &derived_name,
                status: "failed",
                error: Some(err_msg.clone()),
                memory_id: None,
                action: None,
            })?;
            failed += 1;
            if args.fail_fast {
                output::emit_json(&IngestSummary {
                    summary: true,
                    dir: args.dir.display().to_string(),
                    pattern: args.pattern.clone(),
                    recursive: args.recursive,
                    files_total: total,
                    files_succeeded: succeeded,
                    files_failed: failed,
                    files_skipped: skipped,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                })?;
                return Err(AppError::Validation(format!(
                    "ingest aborted on first failure: {err_msg}"
                )));
            }
        }
    }

    output::emit_json(&IngestSummary {
        summary: true,
        dir: args.dir.display().to_string(),
        pattern: args.pattern.clone(),
        recursive: args.recursive,
        files_total: total,
        files_succeeded: succeeded,
        files_failed: failed,
        files_skipped: skipped,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

fn collect_files(
    dir: &Path,
    pattern: &str,
    recursive: bool,
    out: &mut Vec<PathBuf>,
) -> Result<(), AppError> {
    let entries = std::fs::read_dir(dir).map_err(AppError::Io)?;
    for entry in entries {
        let entry = entry.map_err(AppError::Io)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(AppError::Io)?;
        if file_type.is_file() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if matches_pattern(&name_str, pattern) {
                out.push(path);
            }
        } else if file_type.is_dir() && recursive {
            collect_files(&path, pattern, recursive, out)?;
        }
    }
    Ok(())
}

fn matches_pattern(name: &str, pattern: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix('*') {
        name.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        name == pattern
    }
}

fn derive_kebab_name(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let lowered: String = stem
        .chars()
        .map(|c| {
            if c == '_' || c.is_whitespace() {
                '-'
            } else {
                c
            }
        })
        .map(|c| c.to_ascii_lowercase())
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
        .collect();
    let collapsed = collapse_dashes(&lowered);
    let trimmed = collapsed.trim_matches('-').to_string();
    let max_len = 60;
    if trimmed.len() > max_len {
        trimmed[..max_len].trim_matches('-').to_string()
    } else {
        trimmed
    }
}

fn collapse_dashes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for c in s.chars() {
        if c == '-' {
            if !prev_dash {
                out.push('-');
            }
            prev_dash = true;
        } else {
            out.push(c);
            prev_dash = false;
        }
    }
    out
}

fn parse_memory_id(stdout: &[u8]) -> Option<i64> {
    let text = std::str::from_utf8(stdout).ok()?;
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    value.get("memory_id")?.as_i64()
}

fn parse_action(stdout: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(stdout).ok()?;
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    value.get("action")?.as_str().map(String::from)
}

fn first_error_line(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    text.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("(no stderr captured)")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn matches_pattern_suffix() {
        assert!(matches_pattern("foo.md", "*.md"));
        assert!(!matches_pattern("foo.txt", "*.md"));
        assert!(matches_pattern("foo.md", "*"));
    }

    #[test]
    fn matches_pattern_prefix() {
        assert!(matches_pattern("README.md", "README*"));
        assert!(!matches_pattern("CHANGELOG.md", "README*"));
    }

    #[test]
    fn matches_pattern_exact() {
        assert!(matches_pattern("README.md", "README.md"));
        assert!(!matches_pattern("readme.md", "README.md"));
    }

    #[test]
    fn derive_kebab_underscore_to_dash() {
        let p = PathBuf::from("/tmp/claude_code_headless.md");
        assert_eq!(derive_kebab_name(&p), "claude-code-headless");
    }

    #[test]
    fn derive_kebab_uppercase_lowered() {
        let p = PathBuf::from("/tmp/README.md");
        assert_eq!(derive_kebab_name(&p), "readme");
    }

    #[test]
    fn derive_kebab_strips_non_kebab_chars() {
        let p = PathBuf::from("/tmp/some@weird#name!.md");
        assert_eq!(derive_kebab_name(&p), "someweirdname");
    }

    #[test]
    fn derive_kebab_collapses_consecutive_dashes() {
        let p = PathBuf::from("/tmp/a__b___c.md");
        assert_eq!(derive_kebab_name(&p), "a-b-c");
    }

    #[test]
    fn derive_kebab_truncates_to_60_chars() {
        let p = PathBuf::from(format!("/tmp/{}.md", "a".repeat(80)));
        let name = derive_kebab_name(&p);
        assert!(name.len() <= 60, "got len {}", name.len());
    }

    #[test]
    fn collect_files_finds_md_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("a.md"), "x").unwrap();
        std::fs::write(tmp.path().join("b.md"), "y").unwrap();
        std::fs::write(tmp.path().join("c.txt"), "z").unwrap();
        let mut out = Vec::new();
        collect_files(tmp.path(), "*.md", false, &mut out).expect("collect");
        assert_eq!(out.len(), 2, "should find 2 .md files, got {out:?}");
    }

    #[test]
    fn collect_files_recursive_descends_subdirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(tmp.path().join("a.md"), "x").unwrap();
        std::fs::write(sub.join("b.md"), "y").unwrap();
        let mut out = Vec::new();
        collect_files(tmp.path(), "*.md", true, &mut out).expect("collect");
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn collect_files_non_recursive_skips_subdirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(tmp.path().join("a.md"), "x").unwrap();
        std::fs::write(sub.join("b.md"), "y").unwrap();
        let mut out = Vec::new();
        collect_files(tmp.path(), "*.md", false, &mut out).expect("collect");
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn parse_memory_id_extracts_field() {
        let stdout = br#"{"memory_id": 42, "name": "x"}"#;
        assert_eq!(parse_memory_id(stdout), Some(42));
    }

    #[test]
    fn parse_memory_id_returns_none_for_invalid_json() {
        let stdout = b"not json";
        assert_eq!(parse_memory_id(stdout), None);
    }
}
