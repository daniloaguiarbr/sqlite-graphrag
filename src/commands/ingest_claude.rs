//! Handler for `ingest --mode claude-code`.
//!
//! Orchestrates the locally installed Claude Code CLI binary (`claude -p`)
//! to extract domain-specific entities and relationships from each file,
//! then persists them via the same pipeline as `remember --graph-stdin`.
//!
//! Architecture: P1 One-Shot per file — each file spawns a separate
//! `claude -p` process with `--json-schema` for guaranteed structured output.
//! A SQLite queue DB tracks progress for resume/retry support.
// Workload: Subprocess I/O-bound (claude -p headless with network wait)

use crate::commands::ingest::IngestArgs;
use crate::entity_type::EntityType;
use crate::errors::AppError;
use crate::paths::AppPaths;
use crate::spawn::env_whitelist::apply_env_whitelist;
use crate::storage::connection::{ensure_db_ready, open_rw};
use crate::storage::entities::{self, NewEntity, NewRelationship};
use crate::storage::memories::{self, NewMemory};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

const MIN_CLAUDE_VERSION: &str = "2.1.0";

const EXTRACTION_SCHEMA: &str = r#"{
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

#[derive(Debug, Deserialize)]
struct ClaudeOutputElement {
    r#type: Option<String>,
    subtype: Option<String>,
    #[serde(default)]
    is_error: bool,
    structured_output: Option<ExtractionResult>,
    result: Option<String>,
    total_cost_usd: Option<f64>,
    error: Option<String>,
    terminal_reason: Option<String>,
    #[serde(rename = "apiKeySource")]
    api_key_source: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtractionResult {
    pub name: String,
    pub description: String,
    pub entities: Vec<ExtractedEntity>,
    pub relationships: Vec<ExtractedRelationship>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtractedRelationship {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub strength: f64,
}

#[derive(Debug, Serialize)]
struct PhaseEvent<'a> {
    phase: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    claude_path: Option<&'a str>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    cost_usd: Option<f64>,
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
    cost_usd: f64,
    elapsed_ms: u64,
}

/// Locates the Claude Code binary on the system.
pub fn find_claude_binary(explicit: Option<&Path>) -> Result<PathBuf, AppError> {
    if let Some(p) = explicit {
        if p.exists() {
            return Ok(p.to_path_buf());
        }
        return Err(AppError::Validation(format!(
            "Claude Code binary not found at explicit path: {}",
            p.display()
        )));
    }

    if let Ok(env_path) = std::env::var("SQLITE_GRAPHRAG_CLAUDE_BINARY") {
        let p = PathBuf::from(&env_path);
        if p.exists() {
            return Ok(p);
        }
    }

    let name = if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    };
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    Err(AppError::Validation(
        "Claude Code binary not found in PATH. Install it from https://docs.anthropic.com/claude-code or specify --claude-binary".to_string(),
    ))
}

/// Validates that the Claude Code binary meets the minimum version.
fn validate_claude_version(binary: &Path) -> Result<String, AppError> {
    let output = Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(AppError::Io)?;

    if !output.status.success() {
        return Err(AppError::Validation(
            "failed to run 'claude --version'".to_string(),
        ));
    }

    let version_str = String::from_utf8(output.stdout)
        .map_err(|_| AppError::Validation("claude --version output is not UTF-8".to_string()))?;
    let version = version_str.trim().to_string();

    // Extract the numeric version part before first space or paren, e.g. "2.1.149 (Claude Code)" -> "2.1.149"
    let numeric = version.split([' ', '(']).next().unwrap_or("").trim();

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

    if let (Some(actual), Some(min)) = (parse_semver(numeric), parse_semver(MIN_CLAUDE_VERSION)) {
        if actual < min {
            return Err(AppError::Validation(format!(
                "Claude Code version {numeric} is below minimum required {MIN_CLAUDE_VERSION}"
            )));
        }
    }

    Ok(version)
}

/// Invokes `claude -p` for a single file and returns the extraction result.
///
/// OAuth-only enforcement (gaps.md:41-49, v1.0.69 mandate):
///
/// - `wait-timeout` for cross-platform subprocess timeout.
/// - `env_clear()` for least-privilege environment.
/// - OAuth-only flow: NO `--bare` (PROHIBITED, gaps.md:49), no API-key path.
/// - Mandatory hardening: `--strict-mcp-config --mcp-config '{}'` to zero
///   MCP servers, and `--settings '{"hooks":{}}'` to disable hooks.
/// - If `ANTHROPIC_API_KEY` is set in the environment we ABORT the spawn
///   (return a `false` command with a violation marker) — API-key path is
///   PROHIBITED in this project.
fn extract_with_claude(
    binary: &Path,
    file_content: &[u8],
    model: Option<&str>,
    timeout_secs: u64,
) -> Result<(ExtractionResult, f64, bool), AppError> {
    use wait_timeout::ChildExt;

    // OAuth-only guard (gaps.md:47). If `ANTHROPIC_API_KEY` is set in the
    // environment we MUST abort — that is the API-key path which is
    // explicitly PROHIBITED. Use the OAuth flow exclusively.
    if let Ok(_key) = std::env::var("ANTHROPIC_API_KEY") {
        let mut cmd = Command::new("false");
        cmd.env_clear();
        cmd.env("PATH", "/nonexistent");
        cmd.arg("--oauth-only-violation-anthropic-api-key-set");
        return Err(AppError::Validation(
            "ANTHROPIC_API_KEY is set in the environment; \
             sqlite-graphrag operates exclusively with OAuth (Pro/Max) and \
             the API-key path is PROHIBITED (gaps.md:47). Unset the variable \
             and re-run with `claude login` already completed in this session."
                .to_string(),
        ));
    }

    let mut cmd = Command::new(binary);

    // v1.0.83 (ADR-0041): env whitelist delegated to the shared helper.
    // `ANTHROPIC_API_KEY` is INTENTIONALLY ABSENT (defence-in-depth).
    apply_env_whitelist(&mut cmd, crate::spawn::env_whitelist::is_strict_env_clear());

    // Canonical OAuth-only command line (gaps.md:201-208 + 211-213).
    // `--bare` is PROHIBITED (gaps.md:49) — never emitted.
    //
    // GAP-META-005 (v1.0.87, ADR-0045): `--mcp-config '{}'` inline is
    // rejected by Claude Code 2.1.177. Substitute the literal for a
    // tempfile path containing `{"mcpServers":{}}`.
    let mcp_config_path = crate::spawn::preflight::write_empty_mcp_config_tempfile()?;

    cmd.arg("-p")
        .arg(EXTRACTION_PROMPT)
        .arg("--strict-mcp-config")
        .arg("--mcp-config")
        .arg(mcp_config_path.as_os_str())
        .arg("--dangerously-skip-permissions")
        .arg("--settings")
        .arg(r#"{"hooks":{}}"#)
        .arg("--output-format")
        .arg("json")
        .arg("--json-schema")
        .arg(EXTRACTION_SCHEMA)
        .arg("--max-turns")
        .arg("7")
        .arg("--no-session-persistence");

    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // GAP-META-005 (v1.0.87, ADR-0045): pre-flight gate runs after argv
    // is fully built. Validates binary, argv-size, walk-up of `.mcp.json`,
    // and `CLAUDE_CONFIG_DIR` cleanliness.
    let argv_refs: Vec<std::ffi::OsString> = cmd.get_args().map(|s| s.to_os_string()).collect();
    let preflight_args = crate::spawn::preflight::PreFlightArgs {
        binary_path: binary,
        argv: &argv_refs,
        workspace_root: std::path::Path::new("."),
        mcp_config_inline_json: None,
        expected_output_bytes: 65_536,
        spawner_name: "ingest_claude",
    };
    if let Err(e) = crate::spawn::preflight::preflight_check(&preflight_args) {
        // v1.0.88 (BUG-6 fix, ADR-0046): propagate the structured
        // `PreFlightError` via the `From` impl in `errors.rs` so callers
        // receive `AppError::PreFlightFailed` (exit 16) instead of a
        // bare `std::process::exit(16)` that discards the variant name,
        // tracing context, and PT-BR i18n.
        return Err(crate::errors::AppError::from(e));
    }

    let mut child = super::claude_runner::spawn_with_memory_limit(&mut cmd).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to spawn claude: {e}"),
        ))
    })?;

    let stdin_data = file_content.to_vec();
    let mut child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| AppError::Validation("failed to open claude stdin".into()))?;
    let stdin_thread = std::thread::spawn(move || -> Result<(), std::io::Error> {
        child_stdin.write_all(&stdin_data)?;
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
                let stdout_str = String::from_utf8_lossy(&stdout_buf);
                if let Ok(elements) = serde_json::from_str::<Vec<ClaudeOutputElement>>(&stdout_str)
                {
                    if let Some(re) = elements
                        .iter()
                        .find(|e| e.r#type.as_deref() == Some("result"))
                    {
                        if re.terminal_reason.as_deref() == Some("max_turns") {
                            tracing::warn!(
                                target: "ingest",
                                "extraction hit max_turns limit — hooks may have consumed turns"
                            );
                            return Err(AppError::Validation(
                                "claude -p hit max_turns: hooks may be consuming turns".into(),
                            ));
                        }
                        if re.is_error {
                            let err_msg = re
                                .error
                                .as_deref()
                                .or(re.result.as_deref())
                                .unwrap_or("unknown error");
                            if err_msg.contains("rate_limit") || err_msg.contains("overloaded") {
                                return Err(AppError::RateLimited {
                                    detail: err_msg.to_string(),
                                });
                            }
                            if err_msg.contains("Not logged in")
                                || err_msg.contains("authentication")
                            {
                                tracing::warn!(
                                    target: "ingest",
                                    "Claude Code authentication failed. Re-authenticate interactively with: claude"
                                );
                            }
                            return Err(AppError::Validation(format!(
                                "claude -p failed: {err_msg}"
                            )));
                        }
                    }
                }
                let stderr_str = String::from_utf8_lossy(&stderr_buf);
                if stderr_str.contains("auth") || stderr_str.contains("login") {
                    tracing::warn!(
                        target: "ingest",
                        "Claude Code authentication may have failed. Re-authenticate with: claude"
                    );
                }
                return Err(AppError::Validation(format!(
                    "claude -p exited with code {:?}: {}",
                    exit_status.code(),
                    stderr_str.trim()
                )));
            }

            let stdout = String::from_utf8(stdout_buf)
                .map_err(|_| AppError::Validation("claude -p stdout is not valid UTF-8".into()))?;
            parse_claude_output(&stdout)
        }
        None => {
            tracing::warn!(target: "ingest", timeout_secs, "claude -p timed out, killing process");
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdin_thread.join();
            Err(AppError::Validation(format!(
                "claude -p timed out after {timeout_secs} seconds"
            )))
        }
    }
}

/// Parses the JSON array output from `claude -p --output-format json`.
///
/// Returns `(extraction, cost_usd, is_oauth)` where `is_oauth` is true when
/// the init element reports `apiKeySource: "none"` (OAuth subscription).
fn parse_claude_output(stdout: &str) -> Result<(ExtractionResult, f64, bool), AppError> {
    let elements: Vec<ClaudeOutputElement> = serde_json::from_str(stdout).map_err(|e| {
        AppError::Validation(format!("failed to parse claude output as JSON array: {e}"))
    })?;

    let is_oauth = elements
        .iter()
        .find(|e| e.r#type.as_deref() == Some("system") && e.subtype.as_deref() == Some("init"))
        .and_then(|e| e.api_key_source.as_deref())
        .map(|s| s == "none")
        .unwrap_or(false);

    let result_elem = elements
        .iter()
        .find(|e| e.r#type.as_deref() == Some("result"))
        .ok_or_else(|| {
            AppError::Validation("claude output missing 'result' element".to_string())
        })?;

    if result_elem.is_error {
        let err_msg = result_elem
            .error
            .as_deref()
            .or(result_elem.result.as_deref())
            .unwrap_or("unknown error");
        if err_msg.contains("rate_limit") || err_msg.contains("overloaded") {
            return Err(AppError::RateLimited {
                detail: err_msg.to_string(),
            });
        }
        return Err(AppError::Validation(format!(
            "claude extraction failed: {err_msg}"
        )));
    }

    let extraction = result_elem
        .structured_output
        .clone()
        .or_else(|| {
            result_elem
                .result
                .as_ref()
                .and_then(|text| serde_json::from_str::<ExtractionResult>(text).ok())
        })
        .ok_or_else(|| {
            AppError::Validation("claude result missing structured_output and result field".into())
        })?;

    let cost = result_elem.total_cost_usd.unwrap_or(0.0);

    Ok((extraction, cost, is_oauth))
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

    conn.pragma_update(None, "journal_mode", "wal")?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS queue (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            file_path   TEXT NOT NULL UNIQUE,
            name        TEXT,
            status      TEXT NOT NULL DEFAULT 'pending',
            memory_id   INTEGER,
            entities    INTEGER DEFAULT 0,
            rels        INTEGER DEFAULT 0,
            error       TEXT,
            cost_usd    REAL DEFAULT 0.0,
            attempt     INTEGER DEFAULT 0,
            elapsed_ms  INTEGER,
            created_at  TEXT DEFAULT (datetime('now')),
            done_at     TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_queue_status ON queue(status);",
    )?;

    Ok(conn)
}

/// Main entry point for `ingest --mode claude-code`.
pub fn run_claude_ingest(args: &IngestArgs) -> Result<(), AppError> {
    let started = Instant::now();

    if !args.dir.exists() {
        return Err(AppError::Validation(format!(
            "directory not found: {}",
            args.dir.display()
        )));
    }

    // G28-B (v1.0.68) + G30 (v1.0.69): acquire singleton before doing real
    // work so two parallel `ingest --mode claude-code` invocations cannot
    // co-exist on the same database. Scope includes the database hash so
    // concurrent ingest against different databases is allowed.
    let early_ns = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let early_paths = AppPaths::resolve(args.db.as_deref())?;
    let _singleton = crate::lock::acquire_job_singleton(
        crate::lock::JobType::IngestClaudeCode,
        &early_ns,
        &early_paths.db,
        args.wait_job_singleton,
        args.force_job_singleton,
    )?;

    // Stage 1: Validate
    let claude_binary = find_claude_binary(args.claude_binary.as_deref())?;
    let version = validate_claude_version(&claude_binary)?;
    tracing::info!(
        target: "ingest",
        binary = %claude_binary.display(),
        version = %version,
        "Claude Code binary validated"
    );

    emit_json(&PhaseEvent {
        phase: "validate",
        claude_path: claude_binary.to_str(),
        version: Some(&version),
        dir: None,
        files_total: None,
        files_new: None,
        files_existing: None,
    });

    // Stage 2: Scan
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
        claude_path: None,
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
            cost_usd: 0.0,
            elapsed_ms: started.elapsed().as_millis() as u64,
        });
        if !args.keep_queue {
            let _ = std::fs::remove_file(&args.queue_db);
        }
        return Ok(());
    }

    // Stage 3: Process
    let paths = AppPaths::resolve(args.db.as_deref())?;
    ensure_db_ready(&paths)?;
    let conn = open_rw(&paths.db)?;

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let memory_type_str = args.r#type.as_str().to_string();

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
    let mut cost_total = 0.0f64;
    let mut oauth_detected = false;
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

        // G05: reject files that exceed the 10 MB stdin limit
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

        // B08: skip files exceeding body cap BEFORE sending to LLM to avoid wasting tokens
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
                elapsed_ms: Some(file_started.elapsed().as_millis() as u64),
                error: Some(&err_msg),
                index: current_index,
                total,
            });
            continue;
        }

        // B07: retry once on cold-start failure (Claude Code Issue #23265)
        let max_extract_attempts: u32 = 2;
        let mut extraction_result: Option<(ExtractionResult, f64, bool)> = None;
        let mut last_extract_err: Option<String> = None;
        let mut last_was_rate_limited = false;

        for attempt in 1..=max_extract_attempts {
            match extract_with_claude(
                &claude_binary,
                &file_content,
                args.claude_model.as_deref(),
                args.claude_timeout,
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
                        tracing::warn!(target: "ingest", attempt, delay_secs = cold_start_delay, error = %msg, "extraction failed, retrying (cold-start workaround)");
                        std::thread::sleep(std::time::Duration::from_secs(cold_start_delay));
                    }
                    last_extract_err = Some(msg);
                }
            }
        }

        if let Some((extraction, cost, is_oauth)) = extraction_result {
            if is_oauth && !oauth_detected {
                oauth_detected = true;
                tracing::info!(target: "ingest", "OAuth subscription detected — cost_usd omitted from output");
            }
            backoff_secs = args.rate_limit_wait;

            let (normalized_name, _truncated, _orig) = crate::commands::ingest::derive_kebab_name(
                std::path::Path::new(&extraction.name),
                args.max_name_length,
            );
            let name = &normalized_name;
            let ent_count = extraction.entities.len();
            let rel_count = 0;

            let new_entities: Vec<NewEntity> = extraction
                .entities
                .iter()
                .filter_map(|e| match e.entity_type.parse::<EntityType>() {
                    Ok(et) => Some(NewEntity {
                        name: e.name.clone(),
                        entity_type: et,
                        description: None,
                    }),
                    Err(_) => {
                        tracing::warn!(
                            target: "ingest",
                            entity = %e.name,
                            entity_type = %e.entity_type,
                            "entity type not recognized, skipping"
                        );
                        None
                    }
                })
                .collect();

            let new_relationships: Vec<NewRelationship> = extraction
                .relationships
                .iter()
                .map(|r| NewRelationship {
                    source: r.source.clone(),
                    target: r.target.clone(),
                    relation: crate::parsers::normalize_relation(&r.relation),
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

            // B06: deduplication — update existing memory instead of failing on UNIQUE
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
                            cost_usd: if is_oauth { None } else { Some(cost) },
                            elapsed_ms: Some(file_started.elapsed().as_millis() as u64),
                            error: Some(&err_msg),
                            index: current_index,
                            total,
                        });
                        if !is_oauth {
                            cost_total += cost;
                        }
                        if args.fail_fast {
                            break;
                        }
                        continue;
                    }
                },
            };

            for ent in &new_entities {
                match entities::upsert_entity(&conn, &namespace, ent) {
                    Ok(eid) => {
                        let _ = entities::link_memory_entity(&conn, memory_id, eid);
                    }
                    Err(e) => {
                        tracing::warn!(
                            target: "ingest",
                            entity = %ent.name,
                            error = %e,
                            "entity skipped due to validation error"
                        );
                    }
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

            // G01: embedding pipeline — enables recall to find memories created via --mode claude-code
            let body_text = String::from_utf8(file_content.clone())
                .map_err(|e| AppError::Validation(format!("file is not valid UTF-8: {e}")))?;
            let snippet: String = body_text.chars().take(200).collect();
            let chunks_info = crate::chunking::split_into_chunks_hierarchical(&body_text);

            // v1.0.89 (GAP-EMBED-PROPAGATION): honour --llm-backend via embed_passage_with_choice.
            let embedding_result = if chunks_info.len() <= 1 {
                crate::embedder::embed_passage_with_choice(&paths.models, &body_text, None).map(|(v, _)| v)
            } else {
                let mut chunk_embeddings: Vec<Vec<f32>> = Vec::with_capacity(chunks_info.len());
                let mut multi_ok = true;
                for chunk in &chunks_info {
                    let chunk_text = crate::chunking::chunk_text(&body_text, chunk);
                    match crate::embedder::embed_passage_with_choice(&paths.models, chunk_text, None).map(|(v, _)| v) {
                        Ok(emb) => chunk_embeddings.push(emb),
                        Err(e) => {
                            tracing::warn!(
                                target: "ingest",
                                file = %file_path,
                                error = %e,
                                "chunk embedding failed, skipping vector index for this file"
                            );
                            multi_ok = false;
                            break;
                        }
                    }
                }
                if multi_ok {
                    let aggregated = crate::chunking::aggregate_embeddings(&chunk_embeddings);
                    // persist per-chunk vectors
                    if let Err(e) = crate::storage::chunks::insert_chunk_slices(
                        &conn,
                        memory_id,
                        &body_text,
                        &chunks_info,
                    ) {
                        tracing::warn!(
                            target: "ingest",
                            file = %file_path,
                            error = %e,
                            "chunk slice insert failed"
                        );
                    } else {
                        for (i, emb) in chunk_embeddings.iter().enumerate() {
                            if let Err(e) = crate::storage::chunks::upsert_chunk_vec(
                                &conn, i as i64, memory_id, i as i32, emb,
                            ) {
                                tracing::warn!(
                                    target: "ingest",
                                    file = %file_path,
                                    chunk = i,
                                    error = %e,
                                    "chunk vec upsert failed"
                                );
                            }
                        }
                    }
                    Ok(aggregated)
                } else {
                    // fallback: embed whole body for the memory-level vector
                    crate::embedder::embed_passage_with_choice(&paths.models, &body_text, None).map(|(v, _)| v)
                }
            };

            match embedding_result {
                Ok(embedding) => {
                    if let Err(e) = memories::upsert_vec(
                        &conn,
                        memory_id,
                        &namespace,
                        &memory_type_str,
                        &embedding,
                        name,
                        &snippet,
                    ) {
                        tracing::warn!(
                            target: "ingest",
                            file = %file_path,
                            error = %e,
                            "memory vec upsert failed; recall may not find this memory"
                        );
                    }
                    // embed each entity that was successfully upserted
                    for ent in &new_entities {
                        if let Ok(Some(eid)) =
                            entities::find_entity_id(&conn, &namespace, &ent.name)
                        {
                            let entity_text = ent.name.clone();
                            match crate::embedder::embed_passage_with_choice(&paths.models, &entity_text, None).map(|(v, _)| v)
                            {
                                Ok(emb) => {
                                    if let Err(e) = entities::upsert_entity_vec(
                                        &conn,
                                        eid,
                                        &namespace,
                                        ent.entity_type,
                                        &emb,
                                        &ent.name,
                                    ) {
                                        tracing::warn!(
                                            target: "ingest",
                                            entity = %ent.name,
                                            error = %e,
                                            "entity vec upsert failed"
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        target: "ingest",
                                        entity = %ent.name,
                                        error = %e,
                                        "entity embedding failed"
                                    );
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        target: "ingest",
                        file = %file_path,
                        error = %e,
                        "memory embedding failed; recall will not find this memory"
                    );
                }
            }

            let _ = queue_conn.execute(
                "UPDATE queue SET status='done', name=?1, memory_id=?2, entities=?3, rels=?4, cost_usd=?5, elapsed_ms=?6, done_at=datetime('now') WHERE id=?7",
                rusqlite::params![
                    name,
                    memory_id,
                    ent_count,
                    rel_count,
                    cost,
                    file_started.elapsed().as_millis() as i64,
                    queue_id
                ],
            );

            let current_index = completed + failed + skipped;
            completed += 1;
            entities_total += ent_count;
            rels_total += rel_count;
            if !is_oauth {
                cost_total += cost;
            }

            emit_json(&FileEvent {
                file: &file_path,
                name,
                status: "done",
                memory_id: Some(memory_id),
                entities: Some(ent_count),
                rels: Some(rel_count),
                cost_usd: if is_oauth { None } else { Some(cost) },
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
                    tracing::warn!(target: "ingest", delay_secs = actual_wait, error_kind = "rate_limited", "rate limited, backing off");
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

        if let Some(budget) = args.max_cost_usd {
            if oauth_detected {
                tracing::debug!(target: "ingest", "--max-cost-usd ignored: OAuth subscription detected");
            } else if cost_total >= budget {
                tracing::warn!(
                    target: "ingest",
                    spent = cost_total,
                    budget = budget,
                    "budget exceeded, stopping"
                );
                break;
            }
        }
    }

    // Stage 4: Summary
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    let _ = queue_conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");

    emit_json(&Summary {
        summary: true,
        files_total: total,
        completed,
        failed,
        skipped,
        entities_total,
        rels_total,
        cost_usd: cost_total,
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

    #[test]
    fn test_extraction_schema_valid_json() {
        let _: serde_json::Value =
            serde_json::from_str(EXTRACTION_SCHEMA).expect("schema must be valid JSON");
    }

    #[test]
    fn test_parse_claude_output_valid() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"assistant"},
            {"type":"result","is_error":false,"total_cost_usd":0.02,"structured_output":{"name":"test-doc","description":"A test document","entities":[{"name":"test-entity","entity_type":"concept"}],"relationships":[{"source":"test-entity","target":"test-doc","relation":"applies-to","strength":0.8}]}}
        ]"#;
        let (result, cost, _is_oauth) = parse_claude_output(output).expect("parse must succeed");
        assert_eq!(result.name, "test-doc");
        assert_eq!(result.entities.len(), 1);
        assert_eq!(result.relationships.len(), 1);
        assert!((cost - 0.02).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_claude_output_error() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"result","is_error":true,"error":"authentication failed"}
        ]"#;
        let err = parse_claude_output(output).unwrap_err();
        assert!(format!("{err}").contains("authentication failed"));
    }

    #[test]
    fn test_parse_claude_output_rate_limit() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"result","is_error":true,"error":"rate_limit exceeded"}
        ]"#;
        let err = parse_claude_output(output).unwrap_err();
        assert!(matches!(err, AppError::RateLimited { .. }));
    }

    #[test]
    fn test_parse_claude_output_malformed() {
        let output = "not json at all";
        assert!(parse_claude_output(output).is_err());
    }

    #[test]
    fn test_find_claude_binary_not_found() {
        let original_path = std::env::var_os("PATH");
        std::env::set_var("PATH", "/nonexistent");
        std::env::remove_var("SQLITE_GRAPHRAG_CLAUDE_BINARY");
        let result = find_claude_binary(None);
        if let Some(p) = original_path {
            std::env::set_var("PATH", p);
        }
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_claude_output_result_fallback() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"result","is_error":false,"total_cost_usd":0.01,"structured_output":null,"result":"{\"name\":\"test-fallback\",\"description\":\"A fallback test\",\"entities\":[{\"name\":\"fb-entity\",\"entity_type\":\"concept\"}],\"relationships\":[]}"}
        ]"#;
        let (result, cost, _is_oauth) =
            parse_claude_output(output).expect("result fallback must work");
        assert_eq!(result.name, "test-fallback");
        assert_eq!(result.entities.len(), 1);
        assert!(result.relationships.is_empty());
        assert!((cost - 0.01).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_claude_output_error_with_result_field() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"result","is_error":true,"result":"Not logged in · Please run /login"}
        ]"#;
        let err = parse_claude_output(output).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("Not logged in"),
            "expected 'Not logged in' in: {msg}"
        );
    }

    #[test]
    fn test_terminal_reason_max_turns_detected() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"result","is_error":false,"terminal_reason":"max_turns","structured_output":{"name":"t","description":"d","entities":[],"relationships":[]}}
        ]"#;
        let err_or_ok = parse_claude_output(output);
        assert!(
            err_or_ok.is_ok(),
            "max_turns in result without is_error should still parse"
        );
    }

    #[test]
    fn test_detect_oauth_from_init_json() {
        let output = r#"[
            {"type":"system","subtype":"init","apiKeySource":"none"},
            {"type":"result","is_error":false,"total_cost_usd":0.50,"structured_output":{"name":"test-oauth","description":"oauth test","entities":[],"relationships":[]}}
        ]"#;
        let (_result, cost, is_oauth) = parse_claude_output(output).expect("parse must succeed");
        assert!(is_oauth, "apiKeySource=none must be detected as OAuth");
        assert!((cost - 0.50).abs() < f64::EPSILON);
    }

    #[test]
    fn test_api_key_source_not_oauth() {
        let output = r#"[
            {"type":"system","subtype":"init","apiKeySource":"env"},
            {"type":"result","is_error":false,"total_cost_usd":0.10,"structured_output":{"name":"test-api","description":"api test","entities":[],"relationships":[]}}
        ]"#;
        let (_result, _cost, is_oauth) = parse_claude_output(output).expect("parse must succeed");
        assert!(!is_oauth, "apiKeySource=env must NOT be detected as OAuth");
    }

    #[test]
    fn test_missing_api_key_source_defaults_not_oauth() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"result","is_error":false,"total_cost_usd":0.05,"structured_output":{"name":"test-missing","description":"missing test","entities":[],"relationships":[]}}
        ]"#;
        let (_result, _cost, is_oauth) = parse_claude_output(output).expect("parse must succeed");
        assert!(!is_oauth, "missing apiKeySource must default to not OAuth");
    }

    #[test]
    fn test_extraction_schema_entity_types_match_enum() {
        let schema: serde_json::Value = serde_json::from_str(EXTRACTION_SCHEMA).unwrap();
        let types = schema["properties"]["entities"]["items"]["properties"]["entity_type"]["enum"]
            .as_array()
            .expect("schema must have entity_type enum");
        for t in types {
            let s = t.as_str().unwrap();
            assert!(
                s.parse::<EntityType>().is_ok(),
                "schema entity_type '{s}' not in EntityType enum"
            );
        }
    }
}
