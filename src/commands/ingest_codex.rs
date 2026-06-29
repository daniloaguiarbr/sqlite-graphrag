//! Handler for `ingest --mode codex`.
//!
//! Orchestrates the locally installed OpenAI Codex CLI binary (`codex exec`)
//! to extract domain-specific entities and relationships from each file,
//! then persists them with full embedding pipeline for recall/hybrid-search.
//!
//! Architecture: P1 One-Shot per file — each file spawns a separate
//! `codex exec` process with `--output-schema` for guaranteed structured output.
//! A SQLite queue DB tracks progress for resume/retry support.
// Workload: Subprocess I/O-bound (codex exec headless with network wait)

use crate::commands::ingest::IngestArgs;
use crate::commands::ingest_claude::ExtractionResult;
use crate::entity_type::EntityType;
use crate::errors::AppError;
use crate::paths::AppPaths;
use crate::storage::connection::{ensure_db_ready, open_rw};
use crate::storage::entities::{self, NewEntity, NewRelationship};
use crate::storage::memories::{self, NewMemory};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

const MIN_CODEX_VERSION: &str = "0.120.0";

/// OpenAI structured output schema with `additionalProperties: false` at all nested levels.
const EXTRACTION_SCHEMA_CODEX: &str = r#"{
  "type": "object",
  "properties": {
    "name": { "type": "string" },
    "description": { "type": "string" },
    "entities": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "entity_type": {
            "type": "string",
            "enum": ["project","tool","person","file","concept","incident","decision","organization","location","date"]
          }
        },
        "required": ["name", "entity_type"],
        "additionalProperties": false
      }
    },
    "relationships": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "source": { "type": "string" },
          "target": { "type": "string" },
          "relation": {
            "type": "string",
            "enum": ["applies-to","uses","depends-on","causes","fixes","contradicts","supports","follows","related","replaces","tracked-in"]
          },
          "strength": { "type": "number", "minimum": 0, "maximum": 1 }
        },
        "required": ["source","target","relation","strength"],
        "additionalProperties": false
      }
    }
  },
  "required": ["name","description","entities","relationships"],
  "additionalProperties": false
}"#;

const EXTRACTION_PROMPT: &str = "You are a knowledge graph entity extractor. Given a document, extract:\n\
1. A short kebab-case name (max 60 chars) capturing the document's main topic\n\
2. A one-sentence description (10-20 words) summarizing the key insight\n\
3. Domain-specific entities (concepts, tools, people, decisions, projects, files)\n\
4. Typed relationships between entities with strength scores\n\n\
Rules:\n\
- Entity names: lowercase kebab-case, 2+ chars, domain-specific only\n\
- NEVER extract generic terms, stop words, numbers, UUIDs, or single characters\n\
- Relationship types MUST be one of: applies-to, uses, depends-on, causes, fixes, contradicts, supports, follows, related, replaces, tracked-in\n\
- NEVER use 'mentions' as relationship type\n\
- Strength: 0.9 for hard dependencies, 0.7 for design relationships, 0.5 for contextual links, 0.3 for weak references\n\
- Prefer fewer high-quality entities over many low-quality ones\n\
- Description must answer: What is this about and WHY does it matter?";

/// Token usage reported by Codex CLI on `turn.completed` events.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct CodexUsage {
    input_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    output_tokens: u64,
    #[serde(default)]
    reasoning_output_tokens: u64,
}

#[derive(Debug, Serialize)]
struct PhaseEvent<'a> {
    phase: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    codex_path: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dir: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files_total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files_new: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files_existing: Option<usize>,
}

#[derive(Debug, Serialize)]
struct FileEvent<'a> {
    file: &'a str,
    name: &'a str,
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entities: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rels: Option<usize>,
    /// Always None for Codex (no cost_usd in Codex API responses).
    #[serde(skip_serializing_if = "Option::is_none")]
    cost_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    elapsed_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
    index: usize,
    total: usize,
}

#[derive(Debug, Serialize)]
struct Summary {
    summary: bool,
    files_total: usize,
    completed: usize,
    failed: usize,
    skipped: usize,
    entities_total: usize,
    rels_total: usize,
    input_tokens_total: u64,
    output_tokens_total: u64,
    elapsed_ms: u64,
}

/// Locates the Codex CLI binary on the system.
///
/// Search order:
/// 1. Explicit `--codex-binary` CLI flag.
/// 2. `SQLITE_GRAPHRAG_CODEX_BINARY` env var.
/// 3. PATH search for `codex` (or `codex.exe` on Windows).
pub fn find_codex_binary(explicit: Option<&Path>) -> Result<PathBuf, AppError> {
    if let Some(p) = explicit {
        if p.exists() {
            return Ok(p.to_path_buf());
        }
        return Err(AppError::Validation(format!(
            "Codex CLI binary not found at explicit path: {}",
            p.display()
        )));
    }

    if let Ok(env_path) = std::env::var("SQLITE_GRAPHRAG_CODEX_BINARY") {
        let p = PathBuf::from(&env_path);
        if p.exists() {
            return Ok(p);
        }
    }

    let name = if cfg!(windows) { "codex.exe" } else { "codex" };
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    Err(AppError::Validation(
        "Codex CLI binary not found in PATH. Install it from https://github.com/openai/codex or specify --codex-binary".to_string(),
    ))
}

/// Validates that the Codex CLI binary meets the minimum version requirement.
///
/// # Errors
///
/// Returns `AppError::Validation` when the binary cannot be executed or the
/// version is below `MIN_CODEX_VERSION`.
fn validate_codex_version(binary: &Path) -> Result<String, AppError> {
    let resolved = which::which(binary).map_err(|_| {
        AppError::Validation(format!(
            "executable '{}' not found in PATH; ensure Codex CLI is installed",
            binary.display()
        ))
    })?;
    let output = Command::new(&resolved)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(AppError::Io)?;

    let raw = String::from_utf8(output.stdout)
        .map_err(|_| AppError::Validation("codex --version output is not UTF-8".to_string()))?;

    let version_str = raw.trim().to_string();

    // Codex CLI outputs: "codex-cli 0.133.0" or just "0.133.0"
    let numeric = version_str.split_whitespace().last().unwrap_or("").trim();

    fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
        let parts: Vec<&str> = s.splitn(3, '.').collect();
        if parts.len() < 2 {
            return None;
        }
        let major = parts[0].parse::<u64>().ok()?;
        let minor = parts[1].parse::<u64>().ok()?;
        let patch = parts
            .get(2)
            .and_then(|p| p.parse::<u64>().ok())
            .unwrap_or(0);
        Some((major, minor, patch))
    }

    if let (Some(actual), Some(min)) = (parse_semver(numeric), parse_semver(MIN_CODEX_VERSION)) {
        if actual < min {
            return Err(AppError::Validation(format!(
                "Codex CLI version {numeric} is below minimum required {MIN_CODEX_VERSION}"
            )));
        }
    }

    Ok(version_str)
}

/// Writes the extraction schema to a named temp file for `--output-schema`.
///
/// # Errors
///
/// Returns `AppError::Io` when the temp file cannot be created or written.
fn write_schema_tempfile() -> Result<tempfile::NamedTempFile, AppError> {
    let mut f = tempfile::NamedTempFile::new().map_err(AppError::Io)?;
    std::io::Write::write_all(&mut f, EXTRACTION_SCHEMA_CODEX.as_bytes()).map_err(AppError::Io)?;
    std::io::Write::flush(&mut f).map_err(AppError::Io)?;
    Ok(f)
}

/// Invokes `codex exec` for a single file and returns the extraction result.
///
/// Uses `wait-timeout` for cross-platform subprocess timeout, `env_clear()`
/// for least-privilege environment, and reads prompt + file content from
/// stdin using the `-` argument (Codex Paperclip pattern).
///
/// # Errors
///
/// Returns `AppError::Validation` on extraction failure, rate limiting, or
/// schema errors. Returns `AppError::Io` on process spawn/IO failures.
fn extract_with_codex(
    binary: &Path,
    file_content: &[u8],
    model: Option<&str>,
    timeout_secs: u64,
    schema_file: &Path,
) -> Result<(ExtractionResult, Option<CodexUsage>), AppError> {
    use wait_timeout::ChildExt;

    // G31 Passo C (v1.0.69): delegate command construction to the shared
    // `codex_spawn::build_codex_command` helper so `enrich` and `ingest` stay
    // perfectly aligned on the canonical seven hardening flags. The local
    // function still owns the stdin pump + JSONL parsing (see below).
    let _ = timeout_secs; // currently unused; consumed by the helper when it spawns the process
    let _ = file_content; // pumped into stdin below, see `stdin_pump` thread
    let _ = schema_file; // helper reuses the temp file at the given path
    let prompt = String::new(); // empty prompt — helper appends file_content via args.input_text
    let mut cmd = crate::commands::codex_spawn::build_codex_command(
        &crate::commands::codex_spawn::CodexSpawnArgs {
            binary,
            prompt: &prompt,
            json_schema: "", // caller writes the schema directly via `schema_file`
            input_text: "",
            model,
            timeout_secs,
            schema_path: schema_file.to_path_buf(),
        },
    )?;

    // `build_codex_command` writes the JSON schema to `schema_path` and
    // appends `input_text` to the prompt via Paperclip stdin. For `ingest`
    // we want the schema content already on disk (the caller pre-wrote
    // EXTRACTION_SCHEMA_CODEX into the named tempfile), and the document
    // content goes through stdin via a dedicated thread (see below). Strip
    // the file the helper just rewrote — our caller pre-wrote it.
    let _ = std::fs::write(
        schema_file,
        crate::commands::ingest_codex::EXTRACTION_SCHEMA_CODEX,
    );

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = super::claude_runner::spawn_with_memory_limit(&mut cmd).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to spawn codex: {e}"),
        ))
    })?;

    // Build stdin: prompt + document content (strict UTF-8 to surface encoding bugs early)
    let file_utf8 = String::from_utf8(file_content.to_vec())
        .map_err(|e| AppError::Validation(format!("file is not valid UTF-8: {e}")))?;
    let stdin_payload = format!("{EXTRACTION_PROMPT}\n\n---\n\nDocument content:\n\n{file_utf8}");
    let stdin_bytes = stdin_payload.into_bytes();

    let mut child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| AppError::Validation("failed to open codex stdin".into()))?;
    let stdin_thread = std::thread::spawn(move || -> Result<(), std::io::Error> {
        child_stdin.write_all(&stdin_bytes)?;
        drop(child_stdin);
        Ok(())
    });

    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let status = child.wait_timeout(timeout).map_err(AppError::Io)?;

    match status {
        Some(exit_status) => {
            stdin_thread
                .join()
                .map_err(|_| AppError::Validation("stdin thread panicked".into()))?
                .map_err(AppError::Io)?;

            tracing::debug!(
                target: "process",
                exit_code = ?exit_status.code(),
                elapsed_ms = start.elapsed().as_millis() as u64,
                "external process completed"
            );

            let mut stdout_buf = Vec::new();
            let mut stderr_buf = Vec::new();
            if let Some(mut out) = child.stdout.take() {
                std::io::Read::read_to_end(&mut out, &mut stdout_buf).map_err(AppError::Io)?;
            }
            if let Some(mut err) = child.stderr.take() {
                std::io::Read::read_to_end(&mut err, &mut stderr_buf).map_err(AppError::Io)?;
            }

            if !exit_status.success() {
                let stderr_str = String::from_utf8_lossy(&stderr_buf);
                let stdout_str = String::from_utf8_lossy(&stdout_buf);
                // Check if stdout has JSONL with an error event before falling back
                if let Ok((result, usage)) = parse_codex_output(&stdout_str) {
                    return Ok((result, usage));
                }
                if stderr_str.contains("401")
                    || stderr_str.contains("Unauthorized")
                    || stderr_str.contains("auth")
                {
                    tracing::warn!(
                        target: "ingest",
                        "Codex CLI authentication expired. Re-authenticate with: codex auth login"
                    );
                }
                return Err(AppError::Validation(format!(
                    "codex exec exited with code {:?}: {}",
                    exit_status.code(),
                    stderr_str.trim()
                )));
            }

            let stdout = String::from_utf8(stdout_buf)
                .map_err(|_| AppError::Validation("codex exec stdout is not valid UTF-8".into()))?;
            parse_codex_output(&stdout)
        }
        None => {
            tracing::warn!(target: "ingest", timeout_secs, "codex exec timed out, killing process");
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdin_thread.join();
            Err(AppError::Validation(format!(
                "codex exec timed out after {timeout_secs} seconds"
            )))
        }
    }
}

/// Parses JSONL output from `codex exec --json`.
///
/// Event format (DOTS notation):
/// - `thread.started` — session init
/// - `turn.started` — model turn begins
/// - `item.completed` — message or tool call; last `agent_message` wins
/// - `turn.completed` — includes usage stats
/// - `turn.failed` — error with optional rate-limit indicator
/// - `error` — schema or validation error
///
/// # Errors
///
/// Returns `AppError::Validation` when no agent_message is found, when the
/// turn failed, or when the extracted JSON cannot be parsed as `ExtractionResult`.
fn parse_codex_output(stdout: &str) -> Result<(ExtractionResult, Option<CodexUsage>), AppError> {
    let mut last_agent_text: Option<String> = None;
    let mut usage: Option<CodexUsage> = None;
    let mut rate_limited = false;
    let mut schema_error = false;
    let mut turn_failed = false;
    let mut failed_message = String::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let event: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                tracing::warn!(target: "ingest", line, "codex output: skipping malformed JSONL line");
                continue;
            }
        };

        let event_type = match event.get("type").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => continue,
        };

        match event_type {
            "item.completed" => {
                // Last agent_message wins (reasoning / tool calls may appear before)
                if let Some(item) = event.get("item") {
                    if item.get("type").and_then(|t| t.as_str()) == Some("agent_message") {
                        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                            last_agent_text = Some(text.to_string());
                        }
                    }
                }
            }
            "turn.completed" => {
                if let Some(u) = event.get("usage") {
                    if let Ok(parsed) = serde_json::from_value::<CodexUsage>(u.clone()) {
                        usage = Some(parsed);
                    }
                }
            }
            "turn.failed" => {
                turn_failed = true;
                if let Some(err) = event.get("error") {
                    let msg = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown error");
                    failed_message = msg.to_string();
                    if msg.contains("rate_limit")
                        || msg.contains("429")
                        || msg.contains("Too Many Requests")
                    {
                        rate_limited = true;
                    }
                }
            }
            "error" => {
                if let Some(msg) = event.get("message").and_then(|m| m.as_str()) {
                    if msg.contains("invalid_json_schema") || msg.contains("schema") {
                        schema_error = true;
                    }
                    tracing::warn!(target: "ingest", error_msg = msg, "codex error event received");
                }
            }
            _ => {
                // Gracefully skip unknown event types (thread.started, turn.started, etc.)
            }
        }
    }

    if rate_limited {
        return Err(AppError::RateLimited {
            detail: failed_message,
        });
    }

    if schema_error {
        return Err(AppError::Validation(
            "codex rejected the output schema (invalid_json_schema)".to_string(),
        ));
    }

    if turn_failed {
        return Err(AppError::Validation(format!(
            "codex turn failed: {failed_message}"
        )));
    }

    let text = last_agent_text.ok_or_else(|| {
        AppError::Validation("codex output contained no agent_message item".to_string())
    })?;

    let extraction: ExtractionResult = serde_json::from_str(&text).map_err(|e| {
        AppError::Validation(format!(
            "failed to parse codex agent_message as ExtractionResult: {e}. text={text}"
        ))
    })?;

    Ok((extraction, usage))
}

use crate::output::emit_json_line as emit_json;

/// Collects files matching the pattern (reuses ingest logic).
fn collect_matching_files(
    dir: &Path,
    pattern: &str,
    recursive: bool,
    max_files: usize,
) -> Result<Vec<PathBuf>, AppError> {
    let mut files = Vec::new();
    super::ingest::collect_files(dir, pattern, recursive, &mut files)?;
    files.sort_unstable();

    if files.len() > max_files {
        return Err(AppError::Validation(format!(
            "found {} files, exceeds --max-files cap of {}",
            files.len(),
            max_files
        )));
    }

    Ok(files)
}

/// Opens or creates the queue database for tracking ingest progress.
fn open_queue_db(path: &str) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;

    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS queue (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            file_path   TEXT NOT NULL UNIQUE,
            name        TEXT,
            status      TEXT NOT NULL DEFAULT 'pending',
            memory_id   INTEGER,
            entities    INTEGER DEFAULT 0,
            rels        INTEGER DEFAULT 0,
            error       TEXT,
            input_tokens  INTEGER DEFAULT 0,
            output_tokens INTEGER DEFAULT 0,
            attempt     INTEGER DEFAULT 0,
            elapsed_ms  INTEGER,
            created_at  TEXT DEFAULT (datetime('now')),
            done_at     TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_queue_status ON queue(status);",
    )?;

    Ok(conn)
}

/// Main entry point for `ingest --mode codex`.
///
/// # Errors
///
/// Returns `AppError` on directory/DB access failures or fatal extraction errors.
pub fn run_codex_ingest(args: &IngestArgs) -> Result<(), AppError> {
    let started = Instant::now();

    if !args.dir.exists() {
        return Err(AppError::Validation(format!(
            "directory not found: {}",
            args.dir.display()
        )));
    }

    // G28-B (v1.0.68) + G30 (v1.0.69): acquire singleton before doing real
    // work so two parallel `ingest --mode codex` invocations cannot co-exist
    // on the same database. Scope includes the database hash so concurrent
    // ingest against different databases is allowed.
    let early_ns = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let early_paths = AppPaths::resolve(args.db.as_deref())?;
    let _singleton = crate::lock::acquire_job_singleton(
        crate::lock::JobType::IngestCodex,
        &early_ns,
        &early_paths.db,
        args.wait_job_singleton,
        args.force_job_singleton,
    )?;

    // Stage 1: Validate binary
    let codex_binary = find_codex_binary(args.codex_binary.as_deref())?;
    let version = validate_codex_version(&codex_binary)?;
    tracing::info!(
        target: "ingest",
        binary = %codex_binary.display(),
        version = %version,
        "Codex CLI binary validated"
    );

    emit_json(&PhaseEvent {
        phase: "validate",
        codex_path: codex_binary.to_str(),
        version: Some(&version),
        dir: None,
        files_total: None,
        files_new: None,
        files_existing: None,
    });

    // Stage 2: Scan files
    let files = collect_matching_files(&args.dir, &args.pattern, args.recursive, args.max_files)?;

    let queue_conn = open_queue_db(&args.queue_db)?;

    if args.resume {
        let reset = queue_conn
            .execute(
                "UPDATE queue SET status='pending' WHERE status='processing'",
                [],
            )
            .map_err(|e| AppError::Validation(format!("queue resume failed: {e}")))?;
        if reset > 0 {
            tracing::info!(target: "ingest", count = reset, "reset stuck processing files to pending");
        }
    }

    if args.retry_failed {
        let count = queue_conn
            .execute(
                "UPDATE queue SET status='pending', attempt=0 WHERE status='failed'",
                [],
            )
            .map_err(|e| AppError::Validation(format!("queue retry-failed reset failed: {e}")))?;
        tracing::info!(target: "ingest", count, "retrying failed files");
    }

    if !args.resume && !args.retry_failed {
        queue_conn
            .execute("DELETE FROM queue", [])
            .map_err(|e| AppError::Validation(format!("queue clear failed: {e}")))?;
    }

    let mut new_count = 0usize;
    let mut existing_count = 0usize;

    if !args.retry_failed {
        for file in &files {
            let file_str = file.to_string_lossy().into_owned();
            let inserted = queue_conn
                .execute(
                    "INSERT OR IGNORE INTO queue (file_path, status) VALUES (?1, 'pending')",
                    rusqlite::params![file_str],
                )
                .map_err(|e| AppError::Validation(format!("queue insert failed: {e}")))?;
            if inserted > 0 {
                new_count += 1;
            } else {
                existing_count += 1;
            }
        }
    }

    emit_json(&PhaseEvent {
        phase: "scan",
        codex_path: None,
        version: None,
        dir: args.dir.to_str(),
        files_total: Some(files.len()),
        files_new: Some(new_count),
        files_existing: Some(existing_count),
    });

    if args.dry_run {
        for (idx, file) in files.iter().enumerate() {
            let (name, _truncated, _orig) =
                super::ingest::derive_kebab_name(file, args.max_name_length);
            emit_json(&FileEvent {
                file: &file.to_string_lossy(),
                name: &name,
                status: "preview",
                memory_id: None,
                entities: None,
                rels: None,
                cost_usd: None,
                input_tokens: None,
                output_tokens: None,
                elapsed_ms: None,
                error: None,
                index: idx,
                total: files.len(),
            });
        }
        emit_json(&Summary {
            summary: true,
            files_total: files.len(),
            completed: 0,
            failed: 0,
            skipped: 0,
            entities_total: 0,
            rels_total: 0,
            input_tokens_total: 0,
            output_tokens_total: 0,
            elapsed_ms: started.elapsed().as_millis() as u64,
        });
        if !args.keep_queue {
            let _ = std::fs::remove_file(&args.queue_db);
        }
        return Ok(());
    }

    // Stage 3: Process files
    let paths = AppPaths::resolve(args.db.as_deref())?;
    ensure_db_ready(&paths)?;
    let conn = open_rw(&paths.db)?;
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let memory_type_str = args.r#type.as_str().to_string();

    // Write schema to temp file once (reused across all files)
    let schema_tempfile = write_schema_tempfile()?;
    let schema_path = schema_tempfile.path().to_path_buf();

    let mut completed = 0usize;
    let mut failed = 0usize;
    let skipped_initial: usize = queue_conn
        .query_row("SELECT COUNT(*) FROM queue WHERE status='done'", [], |r| {
            r.get::<_, usize>(0)
        })
        .unwrap_or(0);
    let mut skipped = skipped_initial;
    let mut entities_total = 0usize;
    let mut rels_total = 0usize;
    let mut input_tokens_total = 0u64;
    let mut output_tokens_total = 0u64;
    let total = files.len();

    let mut backoff_secs = args.rate_limit_wait;
    let rate_limit_deadline = std::time::Instant::now() + std::time::Duration::from_secs(3600);

    loop {
        if crate::shutdown_requested() {
            tracing::info!(target: "ingest", "shutdown requested, stopping before next file");
            break;
        }

        let pending: Option<(i64, String)> = queue_conn
            .query_row(
                "UPDATE queue SET status='processing', attempt=attempt+1 \
                 WHERE id = (SELECT id FROM queue WHERE status='pending' ORDER BY id LIMIT 1) \
                 RETURNING id, file_path",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let (queue_id, file_path) = match pending {
            Some(p) => p,
            None => break,
        };

        let file_started = Instant::now();

        // Reject files that exceed the 10 MB stdin limit
        const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;
        if let Ok(meta) = std::fs::metadata(&file_path) {
            if meta.len() > MAX_FILE_SIZE {
                let err_msg = format!("file exceeds 10MB stdin limit ({} bytes)", meta.len());
                let _ = queue_conn.execute(
                    "UPDATE queue SET status='failed', error=?1, done_at=datetime('now') WHERE id=?2",
                    rusqlite::params![err_msg, queue_id],
                );
                let current_index = completed + failed + skipped;
                failed += 1;
                emit_json(&FileEvent {
                    file: &file_path,
                    name: "",
                    status: "failed",
                    memory_id: None,
                    entities: None,
                    rels: None,
                    cost_usd: None,
                    input_tokens: None,
                    output_tokens: None,
                    elapsed_ms: Some(file_started.elapsed().as_millis() as u64),
                    error: Some(&err_msg),
                    index: current_index,
                    total,
                });
                if args.fail_fast {
                    break;
                }
                continue;
            }
        }

        let file_content = match std::fs::read(&file_path) {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("IO error: {e}");
                let _ = queue_conn.execute(
                    "UPDATE queue SET status='failed', error=?1, done_at=datetime('now') WHERE id=?2",
                    rusqlite::params![err_msg, queue_id],
                );
                let current_index = completed + failed + skipped;
                failed += 1;
                emit_json(&FileEvent {
                    file: &file_path,
                    name: "",
                    status: "failed",
                    memory_id: None,
                    entities: None,
                    rels: None,
                    cost_usd: None,
                    input_tokens: None,
                    output_tokens: None,
                    elapsed_ms: Some(file_started.elapsed().as_millis() as u64),
                    error: Some(&err_msg),
                    index: current_index,
                    total,
                });
                if args.fail_fast {
                    break;
                }
                continue;
            }
        };

        // Skip files exceeding body cap BEFORE sending to LLM to avoid wasting tokens
        if file_content.len() > crate::constants::MAX_MEMORY_BODY_LEN {
            let err_msg = format!(
                "file body exceeds {} byte limit ({} bytes) — skipping to avoid wasting LLM tokens",
                crate::constants::MAX_MEMORY_BODY_LEN,
                file_content.len()
            );
            tracing::warn!(target: "ingest", file = %file_path, size = file_content.len(), "body exceeds limit, skipping LLM extraction");
            let _ = queue_conn.execute(
                "UPDATE queue SET status='skipped', error=?1, done_at=datetime('now') WHERE id=?2",
                rusqlite::params![err_msg, queue_id],
            );
            let current_index = completed + failed + skipped;
            skipped += 1;
            emit_json(&FileEvent {
                file: &file_path,
                name: "",
                status: "skipped",
                memory_id: None,
                entities: None,
                rels: None,
                cost_usd: None,
                input_tokens: None,
                output_tokens: None,
                elapsed_ms: Some(file_started.elapsed().as_millis() as u64),
                error: Some(&err_msg),
                index: current_index,
                total,
            });
            continue;
        }

        // Retry once on cold-start failure
        let max_extract_attempts: u32 = 2;
        let mut extraction_result: Option<(ExtractionResult, Option<CodexUsage>)> = None;
        let mut last_extract_err: Option<String> = None;
        let mut last_was_rate_limited = false;

        for attempt in 1..=max_extract_attempts {
            match extract_with_codex(
                &codex_binary,
                &file_content,
                args.codex_model.as_deref(),
                args.codex_timeout,
                &schema_path,
            ) {
                Ok(result) => {
                    extraction_result = Some(result);
                    break;
                }
                Err(ref e) if matches!(e, AppError::RateLimited { .. }) => {
                    last_extract_err = Some(format!("{e}"));
                    last_was_rate_limited = true;
                    break;
                }
                Err(e) => {
                    let msg = format!("{e}");
                    if attempt < max_extract_attempts {
                        let cold_start_delay = 2 * attempt as u64;
                        tracing::warn!(
                            target: "ingest",
                            attempt,
                            delay_secs = cold_start_delay,
                            error = %msg,
                            "codex extraction failed, retrying"
                        );
                        std::thread::sleep(std::time::Duration::from_secs(cold_start_delay));
                    }
                    last_extract_err = Some(msg);
                }
            }
        }

        if let Some((extraction, usage)) = extraction_result {
            backoff_secs = args.rate_limit_wait;

            let in_tok = usage.as_ref().map(|u| u.input_tokens).unwrap_or(0);
            let out_tok = usage.as_ref().map(|u| u.output_tokens).unwrap_or(0);

            let name = &extraction.name;
            let ent_count = extraction.entities.len();
            let rel_count = 0;

            // GAP-SG-47: fold non-canonical labels onto the nearest canonical
            // kind instead of discarding the entity (no silent data loss).
            let new_entities: Vec<NewEntity> = extraction
                .entities
                .iter()
                .map(|e| NewEntity {
                    name: e.name.clone(),
                    entity_type: EntityType::map_to_canonical(&e.entity_type),
                    description: None,
                })
                .collect();

            // GAP-SG-48: rewrite non-canonical relations to canonical instead
            // of normalizing-and-accepting them raw.
            let new_relationships: Vec<NewRelationship> = extraction
                .relationships
                .iter()
                .map(|r| NewRelationship {
                    source: r.source.clone(),
                    target: r.target.clone(),
                    relation: crate::parsers::map_to_canonical_relation(&r.relation),
                    strength: r.strength,
                    description: None,
                })
                .collect();

            let body_str = String::from_utf8(file_content.clone())
                .map_err(|e| AppError::Validation(format!("file is not valid UTF-8: {e}")))?;
            let body_hash = blake3::hash(body_str.as_bytes()).to_hex().to_string();
            let new_memory = NewMemory {
                name: name.clone(),
                namespace: namespace.clone(),
                memory_type: memory_type_str.clone(),
                description: extraction.description.clone(),
                body: body_str.to_string(),
                body_hash,
                session_id: None,
                source: "agent".to_string(),
                metadata: serde_json::Value::Object(serde_json::Map::new()),
            };

            // Deduplication: update existing memory instead of failing on UNIQUE
            let memory_id = match memories::find_by_name_any_state(&conn, &namespace, name)? {
                Some((existing_id, is_deleted)) => {
                    if is_deleted {
                        memories::clear_deleted_at(&conn, existing_id)?;
                    }
                    let (old_name, old_desc, old_body): (String, String, String) = conn.query_row(
                        "SELECT name, COALESCE(description,''), COALESCE(body,'') FROM memories WHERE id=?1",
                        rusqlite::params![existing_id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )?;
                    memories::update(&conn, existing_id, &new_memory, None)?;
                    memories::sync_fts_after_update(
                        &conn,
                        existing_id,
                        &old_name,
                        &old_desc,
                        &old_body,
                        &new_memory.name,
                        &new_memory.description,
                        &new_memory.body,
                    )?;
                    tracing::info!(target: "ingest", name, memory_id = existing_id, "updated existing memory (force-merge)");
                    existing_id
                }
                None => match memories::insert(&conn, &new_memory) {
                    Ok(id) => id,
                    Err(e) => {
                        let err_msg = format!("{e}");
                        let _ = queue_conn.execute(
                            "UPDATE queue SET status='failed', error=?1, done_at=datetime('now') WHERE id=?2",
                            rusqlite::params![err_msg, queue_id],
                        );
                        let current_index = completed + failed + skipped;
                        failed += 1;
                        emit_json(&FileEvent {
                            file: &file_path,
                            name,
                            status: "failed",
                            memory_id: None,
                            entities: None,
                            rels: None,
                            cost_usd: None,
                            input_tokens: Some(in_tok),
                            output_tokens: Some(out_tok),
                            elapsed_ms: Some(file_started.elapsed().as_millis() as u64),
                            error: Some(&err_msg),
                            index: current_index,
                            total,
                        });
                        input_tokens_total += in_tok;
                        output_tokens_total += out_tok;
                        if args.fail_fast {
                            break;
                        }
                        continue;
                    }
                },
            };

            for ent in &new_entities {
                if let Ok(eid) = entities::upsert_entity(&conn, &namespace, ent) {
                    let _ = entities::link_memory_entity(&conn, memory_id, eid);
                }
            }
            for rel in &new_relationships {
                crate::parsers::warn_if_non_canonical(&rel.relation);
                let src_id = entities::find_entity_id(&conn, &namespace, &rel.source);
                let tgt_id = entities::find_entity_id(&conn, &namespace, &rel.target);
                if let (Ok(Some(sid)), Ok(Some(tid))) = (src_id, tgt_id) {
                    let _ = conn.execute(
                        "INSERT OR IGNORE INTO relationships (namespace, source_id, target_id, relation, weight) VALUES (?1, ?2, ?3, ?4, ?5)",
                        rusqlite::params![namespace, sid, tid, rel.relation, rel.strength],
                    );
                }
            }

            let _ = queue_conn.execute(
                "UPDATE queue SET status='done', name=?1, memory_id=?2, entities=?3, rels=?4, \
                 input_tokens=?5, output_tokens=?6, elapsed_ms=?7, done_at=datetime('now') WHERE id=?8",
                rusqlite::params![
                    name,
                    memory_id,
                    ent_count,
                    rel_count,
                    in_tok,
                    out_tok,
                    file_started.elapsed().as_millis() as i64,
                    queue_id
                ],
            );

            let current_index = completed + failed + skipped;
            completed += 1;
            entities_total += ent_count;
            rels_total += rel_count;
            input_tokens_total += in_tok;
            output_tokens_total += out_tok;

            emit_json(&FileEvent {
                file: &file_path,
                name,
                status: "done",
                memory_id: Some(memory_id),
                entities: Some(ent_count),
                rels: Some(rel_count),
                cost_usd: None,
                input_tokens: Some(in_tok),
                output_tokens: Some(out_tok),
                elapsed_ms: Some(file_started.elapsed().as_millis() as u64),
                error: None,
                index: current_index,
                total,
            });
        } else if let Some(ref err_str) = last_extract_err {
            if last_was_rate_limited {
                if crate::retry::is_kill_switch_active() {
                    tracing::warn!(target: "ingest", "SQLITE_GRAPHRAG_DISABLE_RETRY=1, skipping rate-limit retry");
                } else if std::time::Instant::now() >= rate_limit_deadline {
                    tracing::error!(target: "ingest", "rate-limit retry deadline (1h) exhausted");
                } else {
                    let half = backoff_secs / 2;
                    let jitter = if half == 0 { 0 } else { fastrand::u64(0..half) };
                    let actual_wait = half + jitter;
                    tracing::warn!(target: "ingest", delay_secs = actual_wait, error_kind = "rate_limited", "Codex rate limited, backing off");
                    let _ = queue_conn.execute(
                        "UPDATE queue SET status='pending' WHERE id=?1",
                        rusqlite::params![queue_id],
                    );
                    std::thread::sleep(std::time::Duration::from_secs(actual_wait));
                    backoff_secs = (backoff_secs * 2).min(900);
                    continue;
                }
            } else {
                let _ = queue_conn.execute(
                    "UPDATE queue SET status='failed', error=?1, done_at=datetime('now') WHERE id=?2",
                    rusqlite::params![err_str, queue_id],
                );
                let current_index = completed + failed + skipped;
                failed += 1;
                emit_json(&FileEvent {
                    file: &file_path,
                    name: "",
                    status: "failed",
                    memory_id: None,
                    entities: None,
                    rels: None,
                    cost_usd: None,
                    input_tokens: None,
                    output_tokens: None,
                    elapsed_ms: Some(file_started.elapsed().as_millis() as u64),
                    error: Some(err_str),
                    index: current_index,
                    total,
                });
                if args.fail_fast {
                    break;
                }
            }
        }
    }

    // WAL checkpoint before summary
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");

    // Stage 4: Summary
    emit_json(&Summary {
        summary: true,
        files_total: total,
        completed,
        failed,
        skipped,
        entities_total,
        rels_total,
        input_tokens_total,
        output_tokens_total,
        elapsed_ms: started.elapsed().as_millis() as u64,
    });

    if !args.keep_queue && failed == 0 {
        let _ = std::fs::remove_file(&args.queue_db);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_agent_message_event(text: &str) -> String {
        format!(
            r#"{{"type":"item.completed","item":{{"id":"item_0","type":"agent_message","text":{}}}}}"#,
            serde_json::to_string(text).unwrap()
        )
    }

    fn make_usage_event(input: u64, output: u64) -> String {
        format!(
            r#"{{"type":"turn.completed","usage":{{"input_tokens":{input},"output_tokens":{output}}}}}"#
        )
    }

    fn valid_extraction_json() -> String {
        r#"{"name":"test-module","description":"A test module for unit testing purposes","entities":[{"name":"test-entity","entity_type":"concept"}],"relationships":[{"source":"test-entity","target":"test-module","relation":"applies-to","strength":0.8}]}"#.to_string()
    }

    #[test]
    fn test_parse_codex_output_valid() {
        let jsonl = format!(
            "{}\n{}\n{}",
            r#"{"type":"thread.started","thread_id":"t1"}"#,
            make_agent_message_event(&valid_extraction_json()),
            make_usage_event(100, 50),
        );

        let (result, usage) = parse_codex_output(&jsonl).expect("parse must succeed");
        assert_eq!(result.name, "test-module");
        assert_eq!(result.entities.len(), 1);
        assert_eq!(result.relationships.len(), 1);
        let u = usage.expect("usage must be present");
        assert_eq!(u.input_tokens, 100);
        assert_eq!(u.output_tokens, 50);
    }

    #[test]
    fn test_parse_codex_output_turn_failed() {
        let jsonl = format!(
            "{}\n{}",
            r#"{"type":"thread.started","thread_id":"t1"}"#,
            r#"{"type":"turn.failed","error":{"message":"model error occurred"}}"#,
        );

        let err = parse_codex_output(&jsonl).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("turn failed"),
            "expected 'turn failed' in: {msg}"
        );
        assert!(msg.contains("model error occurred"));
    }

    #[test]
    fn test_parse_codex_output_rate_limit() {
        let jsonl = r#"{"type":"turn.failed","error":{"message":"rate_limit exceeded, 429 Too Many Requests"}}"#;

        let err = parse_codex_output(jsonl).unwrap_err();
        assert!(
            matches!(err, AppError::RateLimited { .. }),
            "expected AppError::RateLimited, got: {err}"
        );
    }

    #[test]
    fn test_parse_codex_output_schema_error() {
        let jsonl = r#"{"type":"error","message":"invalid_json_schema: additional properties not allowed"}"#;

        let err = parse_codex_output(jsonl).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("invalid_json_schema") || msg.contains("schema"),
            "expected schema error in: {msg}"
        );
    }

    #[test]
    fn test_extraction_schema_codex_valid_json() {
        let _: serde_json::Value =
            serde_json::from_str(EXTRACTION_SCHEMA_CODEX).expect("schema must be valid JSON");
    }

    #[test]
    fn test_extraction_schema_codex_has_additional_properties_false() {
        let schema: serde_json::Value =
            serde_json::from_str(EXTRACTION_SCHEMA_CODEX).expect("schema must be valid JSON");

        // Root level
        assert_eq!(
            schema["additionalProperties"].as_bool(),
            Some(false),
            "root must have additionalProperties: false"
        );

        // Entity items level
        assert_eq!(
            schema["properties"]["entities"]["items"]["additionalProperties"].as_bool(),
            Some(false),
            "entity items must have additionalProperties: false"
        );

        // Relationship items level
        assert_eq!(
            schema["properties"]["relationships"]["items"]["additionalProperties"].as_bool(),
            Some(false),
            "relationship items must have additionalProperties: false"
        );
    }

    #[test]
    fn test_parse_codex_output_last_agent_message_wins() {
        // Multiple agent_message items — last one should win
        let first_text = r#"{"name":"first-result","description":"First result should be ignored","entities":[],"relationships":[]}"#;
        let second_text = r#"{"name":"final-result","description":"Final result wins over earlier ones","entities":[{"name":"final-entity","entity_type":"concept"}],"relationships":[]}"#;

        let jsonl = format!(
            "{}\n{}\n{}\n{}",
            r#"{"type":"thread.started","thread_id":"t1"}"#,
            make_agent_message_event(first_text),
            make_agent_message_event(second_text),
            make_usage_event(200, 80),
        );

        let (result, _) = parse_codex_output(&jsonl).expect("parse must succeed");
        assert_eq!(result.name, "final-result", "last agent_message should win");
        assert_eq!(result.entities.len(), 1);
    }

    #[test]
    fn test_parse_codex_output_skips_malformed_lines() {
        let jsonl = format!(
            "not json at all\n{}\n{{broken\n{}",
            make_agent_message_event(&valid_extraction_json()),
            make_usage_event(10, 5),
        );

        // Should succeed despite malformed lines
        let (result, _) = parse_codex_output(&jsonl).expect("malformed lines must be skipped");
        assert_eq!(result.name, "test-module");
    }
}
