//! Handler for `ingest --mode claude-code`.
//!
//! Orchestrates the locally installed Claude Code CLI binary (`claude -p`)
//! to extract domain-specific entities and relationships from each file,
//! then persists them via the same pipeline as `remember --graph-stdin`.
//!
//! Architecture: P1 One-Shot per file — each file spawns a separate
//! `claude -p` process with `--json-schema` for guaranteed structured output.
//! A SQLite queue DB tracks progress for resume/retry support.

use crate::commands::ingest::IngestArgs;
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

#[allow(dead_code)]
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
        "required": ["name", "entity_type"]
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
        "required": ["source","target","relation","strength"]
      }
    }
  },
  "required": ["name","description","entities","relationships"]
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
    #[allow(dead_code)]
    subtype: Option<String>,
    #[serde(default)]
    is_error: bool,
    structured_output: Option<ExtractionResult>,
    total_cost_usd: Option<f64>,
    error: Option<String>,
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

    Ok(version)
}

/// Invokes `claude -p` for a single file and returns the extraction result.
fn extract_with_claude(
    binary: &Path,
    file_content: &[u8],
    model: Option<&str>,
) -> Result<(ExtractionResult, f64), AppError> {
    let mut cmd = Command::new(binary);
    cmd.arg("-p")
        .arg(EXTRACTION_PROMPT)
        .arg("--output-format")
        .arg("json")
        .arg("--json-schema")
        .arg(EXTRACTION_SCHEMA)
        .arg("--max-turns")
        .arg("1")
        .arg("--no-session-persistence")
        .arg("--bare")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }

    let mut child = cmd.spawn().map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to spawn claude: {e}"),
        ))
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(file_content).map_err(AppError::Io)?;
    }

    let output = child.wait_with_output().map_err(AppError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Validation(format!(
            "claude -p exited with code {:?}: {}",
            output.status.code(),
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| AppError::Validation("claude -p stdout is not valid UTF-8".to_string()))?;

    parse_claude_output(&stdout)
}

/// Parses the JSON array output from `claude -p --output-format json`.
fn parse_claude_output(stdout: &str) -> Result<(ExtractionResult, f64), AppError> {
    let elements: Vec<ClaudeOutputElement> = serde_json::from_str(stdout).map_err(|e| {
        AppError::Validation(format!("failed to parse claude output as JSON array: {e}"))
    })?;

    let result_elem = elements
        .iter()
        .find(|e| e.r#type.as_deref() == Some("result"))
        .ok_or_else(|| {
            AppError::Validation("claude output missing 'result' element".to_string())
        })?;

    if result_elem.is_error {
        let err_msg = result_elem.error.as_deref().unwrap_or("unknown error");
        if err_msg.contains("rate_limit") || err_msg.contains("overloaded") {
            return Err(AppError::Validation(format!("RATE_LIMITED: {err_msg}")));
        }
        return Err(AppError::Validation(format!(
            "claude extraction failed: {err_msg}"
        )));
    }

    let extraction = result_elem.structured_output.clone().ok_or_else(|| {
        AppError::Validation("claude result missing structured_output".to_string())
    })?;

    let cost = result_elem.total_cost_usd.unwrap_or(0.0);

    Ok((extraction, cost))
}

fn emit_json<T: Serialize>(value: &T) {
    if let Ok(json) = serde_json::to_string(value) {
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        let _ = writeln!(lock, "{json}");
        let _ = lock.flush();
    }
}

/// Collects files matching the pattern (reuses ingest logic).
fn collect_matching_files(
    dir: &Path,
    pattern: &str,
    recursive: bool,
    max_files: usize,
) -> Result<Vec<PathBuf>, AppError> {
    let mut files = Vec::new();
    super::ingest::collect_files(dir, pattern, recursive, &mut files)?;
    files.sort();

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

    let mut new_count = 0usize;
    let mut existing_count = 0usize;

    for file in &files {
        let file_str = file.to_string_lossy().to_string();
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

    emit_json(&PhaseEvent {
        phase: "scan",
        claude_path: None,
        version: None,
        dir: args.dir.to_str(),
        files_total: Some(files.len()),
        files_new: Some(new_count),
        files_existing: Some(existing_count),
    });

    // Stage 3: Process
    let paths = AppPaths::resolve(args.db.as_deref())?;
    ensure_db_ready(&paths)?;
    let conn = open_rw(&paths.db)?;
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let memory_type_str = args.r#type.as_str().to_string();

    let mut completed = 0usize;
    let mut failed = 0usize;
    let skipped = 0usize;
    let mut entities_total = 0usize;
    let mut rels_total = 0usize;
    let mut cost_total = 0.0f64;
    let total = files.len();

    let mut backoff_secs = args.rate_limit_wait;

    loop {
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
        let file_content = match std::fs::read(&file_path) {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("IO error: {e}");
                let _ = queue_conn.execute(
                    "UPDATE queue SET status='failed', error=?1, done_at=datetime('now') WHERE id=?2",
                    rusqlite::params![err_msg, queue_id],
                );
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
                    index: completed + failed + skipped,
                    total,
                });
                if args.fail_fast {
                    break;
                }
                continue;
            }
        };

        match extract_with_claude(&claude_binary, &file_content, args.claude_model.as_deref()) {
            Ok((extraction, cost)) => {
                backoff_secs = args.rate_limit_wait;

                let name = &extraction.name;
                let ent_count = extraction.entities.len();
                let rel_count = extraction.relationships.len();

                let new_entities: Vec<NewEntity> = extraction
                    .entities
                    .iter()
                    .filter_map(|e| {
                        e.entity_type
                            .parse::<EntityType>()
                            .ok()
                            .map(|et| NewEntity {
                                name: e.name.clone(),
                                entity_type: et,
                                description: None,
                            })
                    })
                    .collect();

                let new_relationships: Vec<NewRelationship> = extraction
                    .relationships
                    .iter()
                    .map(|r| NewRelationship {
                        source: r.source.clone(),
                        target: r.target.clone(),
                        relation: r.relation.clone(),
                        strength: r.strength,
                        description: None,
                    })
                    .collect();

                let body_str = String::from_utf8_lossy(&file_content);
                let body_hash = blake3::hash(body_str.as_bytes()).to_hex().to_string();
                let new_memory = NewMemory {
                    name: name.clone(),
                    namespace: namespace.clone(),
                    memory_type: memory_type_str.clone(),
                    description: extraction.description.clone(),
                    body: body_str.to_string(),
                    body_hash,
                    session_id: None,
                    source: "claude-code".to_string(),
                    metadata: serde_json::Value::Object(serde_json::Map::new()),
                };

                let memory_id = match memories::insert(&conn, &new_memory) {
                    Ok(id) => id,
                    Err(e) => {
                        let err_msg = format!("{e}");
                        let _ = queue_conn.execute(
                            "UPDATE queue SET status='failed', error=?1, done_at=datetime('now') WHERE id=?2",
                            rusqlite::params![err_msg, queue_id],
                        );
                        failed += 1;
                        emit_json(&FileEvent {
                            file: &file_path,
                            name,
                            status: "failed",
                            memory_id: None,
                            entities: None,
                            rels: None,
                            cost_usd: Some(cost),
                            elapsed_ms: Some(file_started.elapsed().as_millis() as u64),
                            error: Some(&err_msg),
                            index: completed + failed + skipped,
                            total,
                        });
                        cost_total += cost;
                        if args.fail_fast {
                            break;
                        }
                        continue;
                    }
                };

                for ent in &new_entities {
                    if let Ok(eid) = entities::upsert_entity(&conn, &namespace, ent) {
                        let _ = entities::link_memory_entity(&conn, memory_id, eid);
                    }
                }
                for rel in &new_relationships {
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

                completed += 1;
                entities_total += ent_count;
                rels_total += rel_count;
                cost_total += cost;

                emit_json(&FileEvent {
                    file: &file_path,
                    name,
                    status: "done",
                    memory_id: Some(memory_id),
                    entities: Some(ent_count),
                    rels: Some(rel_count),
                    cost_usd: Some(cost),
                    elapsed_ms: Some(file_started.elapsed().as_millis() as u64),
                    error: None,
                    index: completed + failed + skipped - 1,
                    total,
                });
            }
            Err(ref e) if format!("{e}").contains("RATE_LIMITED") => {
                tracing::warn!(
                    target: "ingest",
                    wait_seconds = backoff_secs,
                    "rate limited, waiting before retry"
                );
                let _ = queue_conn.execute(
                    "UPDATE queue SET status='pending' WHERE id=?1",
                    rusqlite::params![queue_id],
                );
                std::thread::sleep(std::time::Duration::from_secs(backoff_secs));
                backoff_secs = (backoff_secs * 2).min(900);
                continue;
            }
            Err(e) => {
                let err_msg = format!("{e}");
                let _ = queue_conn.execute(
                    "UPDATE queue SET status='failed', error=?1, done_at=datetime('now') WHERE id=?2",
                    rusqlite::params![err_msg, queue_id],
                );
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
                    index: completed + failed + skipped,
                    total,
                });
                if args.fail_fast {
                    break;
                }
            }
        }

        if let Some(budget) = args.max_cost_usd {
            if cost_total >= budget {
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
        let (result, cost) = parse_claude_output(output).expect("parse must succeed");
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
        assert!(format!("{err}").contains("RATE_LIMITED"));
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
}
