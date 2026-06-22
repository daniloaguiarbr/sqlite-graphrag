//! OpenCode-curated ingest pipeline (v1.0.90, GAP-OPENCODE-002).
//!
//! Spawns `opencode run` per file to extract entities and relationships
//! via LLM, then persists them alongside the memory body via `remember
//! --graph-stdin --force-merge`.

use crate::commands::ingest::IngestArgs;
use crate::commands::opencode_runner;
use crate::errors::AppError;
use crate::parsers::normalize_entity_name;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

const EXTRACTION_SCHEMA: &str = r#"Return ONLY a valid JSON object with this exact structure (no markdown, no explanation):
{
  "entities": [
    {"name": "entity-name-in-kebab-case", "entity_type": "concept|project|tool|person|file|incident|decision|organization|location|date"}
  ],
  "relationships": [
    {"source": "entity-a", "target": "entity-b", "relation": "applies-to|uses|depends-on|causes|fixes|contradicts|supports|follows|related|replaces|tracked-in", "strength": 0.7}
  ]
}"#;

#[derive(Debug, Deserialize, Serialize)]
pub struct ExtractionResult {
    #[serde(default)]
    pub entities: Vec<ExtractedEntity>,
    #[serde(default)]
    pub relationships: Vec<ExtractedRelationship>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ExtractedRelationship {
    pub source: String,
    pub target: String,
    pub relation: String,
    #[serde(default = "default_strength")]
    pub strength: f64,
}

fn default_strength() -> f64 {
    0.5
}

pub async fn extract_with_opencode(
    binary: &Path,
    model: &str,
    body: &str,
    memory_name: &str,
    timeout_secs: u64,
) -> Result<(ExtractionResult, f64, u64), AppError> {
    let prompt = format!(
        "Analyze the following document and extract domain-specific entities and their relationships.\n\
         Memory name: {memory_name}\n\n\
         {EXTRACTION_SCHEMA}\n\n\
         Document content:\n{body}"
    );

    opencode_runner::call_opencode::<ExtractionResult>(binary, model, &prompt, timeout_secs).await
}

fn emit_json(value: &serde_json::Value) {
    let _ = writeln!(
        std::io::stdout(),
        "{}",
        serde_json::to_string(value).unwrap_or_default()
    );
    let _ = std::io::stdout().flush();
}

pub fn run_opencode_ingest(args: &IngestArgs) -> Result<(), AppError> {
    let started = std::time::Instant::now();

    if !args.dir.exists() {
        return Err(AppError::Validation(format!(
            "directory not found: {}",
            args.dir.display()
        )));
    }

    let binary =
        opencode_runner::find_opencode_binary_with_override(args.opencode_binary.as_deref())?;
    let version = opencode_runner::validate_opencode_version(&binary)?;
    let model = opencode_runner::resolve_opencode_model(args.opencode_model.as_deref());
    let timeout = opencode_runner::resolve_opencode_timeout(if args.opencode_timeout != 300 {
        Some(args.opencode_timeout)
    } else {
        None
    });

    emit_json(&serde_json::json!({
        "phase": "validate",
        "opencode_path": binary.display().to_string(),
        "version": format!("{}.{}.{}", version.0, version.1, version.2),
        "model": &model,
    }));

    let mut files: Vec<PathBuf> = Vec::new();
    super::ingest::collect_files(&args.dir, &args.pattern, args.recursive, &mut files)?;

    if files.len() > args.max_files {
        return Err(AppError::Validation(format!(
            "found {} files exceeding --max-files cap of {}; aborting (all-or-nothing)",
            files.len(),
            args.max_files
        )));
    }

    files.sort();

    emit_json(&serde_json::json!({
        "phase": "scan",
        "dir": args.dir.display().to_string(),
        "files_total": files.len(),
        "files_new": files.len(),
        "files_existing": 0,
    }));

    if args.dry_run {
        for (idx, file) in files.iter().enumerate() {
            let (name, truncated, orig) =
                super::ingest::derive_kebab_name(file, args.max_name_length);
            emit_json(&serde_json::json!({
                "file": file.display().to_string(),
                "name": name,
                "status": "preview",
                "index": idx + 1,
                "total": files.len(),
                "truncated": truncated,
                "original_name": orig,
            }));
        }
        emit_json(&serde_json::json!({
            "summary": true,
            "files_total": files.len(),
            "completed": 0,
            "failed": 0,
            "skipped": 0,
            "entities_total": 0,
            "rels_total": 0,
            "cost_usd": 0.0,
            "elapsed_ms": started.elapsed().as_millis() as u64,
        }));
        return Ok(());
    }

    let rt = crate::embedder::shared_runtime()?;

    let ns = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let app_paths = crate::paths::AppPaths::resolve(args.db.as_deref())?;

    let mut completed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut entities_total = 0usize;
    let mut rels_total = 0usize;
    let mut cost_total: f64 = 0.0;

    for (idx, file) in files.iter().enumerate() {
        let (name, truncated, orig) = super::ingest::derive_kebab_name(file, args.max_name_length);

        let body = match std::fs::read_to_string(file) {
            Ok(b) => b,
            Err(e) => {
                emit_json(&serde_json::json!({
                    "file": file.display().to_string(),
                    "name": name,
                    "status": "failed",
                    "error": format!("read error: {e}"),
                    "index": idx + 1,
                    "total": files.len(),
                }));
                failed += 1;
                if args.fail_fast {
                    break;
                }
                continue;
            }
        };

        if body.len() > 512_000 {
            emit_json(&serde_json::json!({
                "file": file.display().to_string(),
                "name": name,
                "status": "skipped",
                "error": format!("file exceeds 512KB limit ({} bytes)", body.len()),
                "index": idx + 1,
                "total": files.len(),
            }));
            skipped += 1;
            continue;
        }

        let file_started = std::time::Instant::now();

        let extraction = rt.block_on(extract_with_opencode(
            &binary, &model, &body, &name, timeout,
        ));

        match extraction {
            Ok((result, cost, _tokens)) => {
                let ent_count = result.entities.len();
                let rel_count = result.relationships.len();

                let graph_payload = serde_json::json!({
                    "body": body,
                    "entities": result.entities.iter().map(|e| {
                        serde_json::json!({"name": e.name, "entity_type": e.entity_type})
                    }).collect::<Vec<_>>(),
                    "relationships": result.relationships.iter().map(|r| {
                        serde_json::json!({
                            "source": r.source,
                            "target": r.target,
                            "relation": r.relation,
                            "strength": r.strength
                        })
                    }).collect::<Vec<_>>(),
                });

                let remember_result = persist_memory_with_graph(
                    &app_paths.db,
                    &ns,
                    &name,
                    &format!("{:?}", args.r#type).to_lowercase(),
                    &format!("ingested from {} via opencode", file.display()),
                    &graph_payload,
                );

                match remember_result {
                    Ok(memory_id) => {
                        entities_total += ent_count;
                        rels_total += rel_count;
                        cost_total += cost;
                        completed += 1;

                        emit_json(&serde_json::json!({
                            "file": file.display().to_string(),
                            "name": name,
                            "status": "done",
                            "memory_id": memory_id,
                            "entities": ent_count,
                            "rels": rel_count,
                            "cost_usd": cost,
                            "elapsed_ms": file_started.elapsed().as_millis() as u64,
                            "index": idx + 1,
                            "total": files.len(),
                            "truncated": truncated,
                            "original_name": orig,
                        }));
                    }
                    Err(e) => {
                        failed += 1;
                        emit_json(&serde_json::json!({
                            "file": file.display().to_string(),
                            "name": name,
                            "status": "failed",
                            "error": format!("persist error: {e}"),
                            "elapsed_ms": file_started.elapsed().as_millis() as u64,
                            "index": idx + 1,
                            "total": files.len(),
                        }));
                        if args.fail_fast {
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                failed += 1;
                emit_json(&serde_json::json!({
                    "file": file.display().to_string(),
                    "name": name,
                    "status": "failed",
                    "error": format!("extraction error: {e}"),
                    "elapsed_ms": file_started.elapsed().as_millis() as u64,
                    "index": idx + 1,
                    "total": files.len(),
                }));
                if args.fail_fast {
                    break;
                }
            }
        }
    }

    emit_json(&serde_json::json!({
        "summary": true,
        "files_total": files.len(),
        "completed": completed,
        "failed": failed,
        "skipped": skipped,
        "entities_total": entities_total,
        "rels_total": rels_total,
        "cost_usd": cost_total,
        "elapsed_ms": started.elapsed().as_millis() as u64,
    }));

    Ok(())
}

fn persist_memory_with_graph(
    db_path: &Path,
    namespace: &str,
    name: &str,
    memory_type: &str,
    description: &str,
    graph_payload: &serde_json::Value,
) -> Result<i64, AppError> {
    let conn = crate::storage::connection::open_rw(db_path)?;

    let existing = conn
        .query_row(
            "SELECT id FROM memories WHERE name = ?1 AND namespace = ?2",
            rusqlite::params![name, namespace],
            |row| row.get::<_, i64>(0),
        )
        .ok();

    let body = graph_payload
        .get("body")
        .and_then(|b| b.as_str())
        .unwrap_or("");
    let body_hash = blake3::hash(body.as_bytes()).to_hex().to_string();

    let memory_id = if let Some(id) = existing {
        conn.execute(
            "UPDATE memories SET body = ?1, description = ?2, type = ?3, body_hash = ?4, updated_at = strftime('%s','now') WHERE id = ?5",
            rusqlite::params![body, description, memory_type, body_hash, id],
        )
        .map_err(AppError::Database)?;
        id
    } else {
        conn.execute(
            "INSERT INTO memories (name, namespace, type, description, body, body_hash, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, strftime('%s','now'), strftime('%s','now'))",
            rusqlite::params![name, namespace, memory_type, description, body, body_hash],
        )
        .map_err(AppError::Database)?;
        conn.last_insert_rowid()
    };

    if let Some(entities) = graph_payload.get("entities").and_then(|e| e.as_array()) {
        for ent in entities {
            let ent_name = ent.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let ent_type = ent
                .get("entity_type")
                .and_then(|t| t.as_str())
                .unwrap_or("concept");
            if ent_name.len() < 2 {
                continue;
            }
            let normalized = normalize_entity_name(ent_name);
            conn.execute(
                "INSERT OR IGNORE INTO entities (name, type, namespace) VALUES (?1, ?2, ?3)",
                rusqlite::params![normalized, ent_type, namespace],
            )
            .map_err(AppError::Database)?;

            let entity_id: i64 = conn
                .query_row(
                    "SELECT id FROM entities WHERE name = ?1 AND namespace = ?2",
                    rusqlite::params![normalized, namespace],
                    |row| row.get(0),
                )
                .map_err(AppError::Database)?;

            conn.execute(
                "INSERT OR IGNORE INTO memory_entities (memory_id, entity_id) VALUES (?1, ?2)",
                rusqlite::params![memory_id, entity_id],
            )
            .map_err(AppError::Database)?;
        }
    }

    if let Some(rels) = graph_payload
        .get("relationships")
        .and_then(|r| r.as_array())
    {
        for rel in rels {
            let source = rel.get("source").and_then(|s| s.as_str()).unwrap_or("");
            let target = rel.get("target").and_then(|t| t.as_str()).unwrap_or("");
            let relation = rel
                .get("relation")
                .and_then(|r| r.as_str())
                .unwrap_or("related");
            let strength = rel.get("strength").and_then(|s| s.as_f64()).unwrap_or(0.5);

            if source.len() < 2 || target.len() < 2 {
                continue;
            }

            let src_norm = normalize_entity_name(source);
            let tgt_norm = normalize_entity_name(target);

            for name_val in [&src_norm, &tgt_norm] {
                conn.execute(
                    "INSERT OR IGNORE INTO entities (name, type, namespace) VALUES (?1, 'concept', ?2)",
                    rusqlite::params![name_val, namespace],
                )
                .map_err(AppError::Database)?;
            }

            let src_id: i64 = conn
                .query_row(
                    "SELECT id FROM entities WHERE name = ?1 AND namespace = ?2",
                    rusqlite::params![src_norm, namespace],
                    |row| row.get(0),
                )
                .map_err(AppError::Database)?;

            let tgt_id: i64 = conn
                .query_row(
                    "SELECT id FROM entities WHERE name = ?1 AND namespace = ?2",
                    rusqlite::params![tgt_norm, namespace],
                    |row| row.get(0),
                )
                .map_err(AppError::Database)?;

            let rel_normalized = relation.replace('-', "_");
            conn.execute(
                "INSERT OR IGNORE INTO relationships (source_id, target_id, relation, weight, namespace) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![src_id, tgt_id, rel_normalized, strength, namespace],
            )
            .map_err(AppError::Database)?;
        }
    }

    Ok(memory_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_result_deserializes_empty() {
        let json = r#"{"entities":[],"relationships":[]}"#;
        let result: ExtractionResult = serde_json::from_str(json).unwrap();
        assert!(result.entities.is_empty());
        assert!(result.relationships.is_empty());
    }

    #[test]
    fn extraction_result_deserializes_with_data() {
        let json = r#"{
            "entities": [
                {"name": "sqlite-graphrag", "entity_type": "project"},
                {"name": "opencode", "entity_type": "tool"}
            ],
            "relationships": [
                {"source": "sqlite-graphrag", "target": "opencode", "relation": "uses", "strength": 0.8}
            ]
        }"#;
        let result: ExtractionResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.entities.len(), 2);
        assert_eq!(result.relationships.len(), 1);
        assert_eq!(result.relationships[0].strength, 0.8);
    }

    #[test]
    fn extraction_result_default_strength() {
        let json = r#"{
            "entities": [],
            "relationships": [
                {"source": "a", "target": "b", "relation": "related"}
            ]
        }"#;
        let result: ExtractionResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.relationships[0].strength, 0.5);
    }
}
