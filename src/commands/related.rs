//! Handler for the `related` CLI subcommand.

use crate::cli::RelationKind;
use crate::constants::{
    DEFAULT_K_RECALL, DEFAULT_MAX_HOPS, DEFAULT_MIN_WEIGHT, TEXT_DESCRIPTION_PREVIEW_LEN,
};
use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output::{self, OutputFormat};
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

/// Tuple returned by the adjacency fetch: (neighbour_entity_id, source_name,
/// target_name, relation, weight).
type Neighbour = (i64, String, String, String, f64);

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # List memories connected to a memory via the entity graph (default 2 hops)\n  \
    sqlite-graphrag related onboarding\n\n  \
    # Increase hop distance and filter by relation type\n  \
    sqlite-graphrag related onboarding --max-hops 3 --relation related\n\n  \
    # Cap result count and require minimum edge weight\n  \
    sqlite-graphrag related onboarding --limit 5 --min-weight 0.5")]
pub struct RelatedArgs {
    /// Memory name as a positional argument. Alternative to `--name`.
    #[arg(
        value_name = "NAME",
        conflicts_with = "name",
        help = "Memory name whose neighbours to traverse; alternative to --name"
    )]
    pub name_positional: Option<String>,
    /// Memory name as a flag. Required when the positional form is absent.
    #[arg(long)]
    pub name: Option<String>,
    /// Maximum graph hop count. Also accepts the alias `--hops`.
    #[arg(long, alias = "hops", default_value_t = DEFAULT_MAX_HOPS)]
    pub max_hops: u32,
    #[arg(long, value_enum)]
    pub relation: Option<RelationKind>,
    #[arg(long, default_value_t = DEFAULT_MIN_WEIGHT)]
    pub min_weight: f64,
    #[arg(long, default_value_t = DEFAULT_K_RECALL)]
    pub limit: usize,
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct RelatedResponse {
    /// Echo of the seed memory name resolved from `--name` or the positional argument.
    /// Added in v1.0.35 for input transparency in JSON output.
    name: String,
    /// Echo of the resolved `--max-hops` value (default 2). Added in v1.0.35.
    max_hops: u32,
    results: Vec<RelatedMemory>,
    elapsed_ms: u64,
}

#[derive(Serialize, Clone)]
struct RelatedMemory {
    memory_id: i64,
    name: String,
    namespace: String,
    #[serde(rename = "type")]
    memory_type: String,
    description: String,
    hop_distance: u32,
    source_entity: Option<String>,
    target_entity: Option<String>,
    relation: Option<String>,
    weight: Option<f64>,
}

pub fn run(args: RelatedArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let name = args
        .name_positional
        .as_deref()
        .or(args.name.as_deref())
        .ok_or_else(|| {
            AppError::Validation(
                "name required: pass as positional argument or via --name".to_string(),
            )
        })?
        .to_string();

    if name.trim().is_empty() {
        return Err(AppError::Validation("name must not be empty".to_string()));
    }

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let conn = open_ro(&paths.db)?;

    // Locate the seed memory.
    let seed_id: i64 = match conn.query_row(
        "SELECT id FROM memories
         WHERE namespace = ?1 AND name = ?2 AND deleted_at IS NULL",
        params![namespace, name],
        |r| r.get(0),
    ) {
        Ok(id) => id,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(AppError::NotFound(errors_msg::memory_not_found(
                &name, &namespace,
            )));
        }
        Err(e) => return Err(AppError::Database(e)),
    };

    // Collect seed entity IDs from seed memory.
    let seed_entity_ids: Vec<i64> = {
        let mut stmt =
            conn.prepare_cached("SELECT entity_id FROM memory_entities WHERE memory_id = ?1")?;
        let rows: Vec<i64> = stmt
            .query_map(params![seed_id], |r| r.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        rows
    };

    let relation_filter = args.relation.map(|r| r.as_str().to_string());
    let results = traverse_related(
        &conn,
        seed_id,
        &seed_entity_ids,
        &namespace,
        args.max_hops,
        args.min_weight,
        relation_filter.as_deref(),
        args.limit,
    )?;

    match args.format {
        OutputFormat::Json => output::emit_json(&RelatedResponse {
            name: name.clone(),
            max_hops: args.max_hops,
            results,
            elapsed_ms: inicio.elapsed().as_millis() as u64,
        })?,
        OutputFormat::Text => {
            for item in &results {
                if item.description.is_empty() {
                    output::emit_text(&format!(
                        "{}. {} ({})",
                        item.hop_distance, item.name, item.namespace
                    ));
                } else {
                    let preview: String = item
                        .description
                        .chars()
                        .take(TEXT_DESCRIPTION_PREVIEW_LEN)
                        .collect();
                    output::emit_text(&format!(
                        "{}. {} ({}): {}",
                        item.hop_distance, item.name, item.namespace, preview
                    ));
                }
            }
        }
        OutputFormat::Markdown => {
            for item in &results {
                if item.description.is_empty() {
                    output::emit_text(&format!(
                        "- **{}** ({}) — hop {}",
                        item.name, item.namespace, item.hop_distance
                    ));
                } else {
                    let preview: String = item
                        .description
                        .chars()
                        .take(TEXT_DESCRIPTION_PREVIEW_LEN)
                        .collect();
                    output::emit_text(&format!(
                        "- **{}** ({}) — hop {}: {}",
                        item.name, item.namespace, item.hop_distance, preview
                    ));
                }
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn traverse_related(
    conn: &Connection,
    seed_memory_id: i64,
    seed_entity_ids: &[i64],
    namespace: &str,
    max_hops: u32,
    min_weight: f64,
    relation_filter: Option<&str>,
    limit: usize,
) -> Result<Vec<RelatedMemory>, AppError> {
    if seed_entity_ids.is_empty() || max_hops == 0 {
        return Ok(Vec::new());
    }

    // BFS over entities keeping track of hop distance and the (source, target, relation, weight)
    // of the edge that first reached each entity.
    let mut visited: HashSet<i64> = seed_entity_ids.iter().copied().collect();
    let mut entity_hop: HashMap<i64, u32> = HashMap::new();
    for &e in seed_entity_ids {
        entity_hop.insert(e, 0);
    }
    // Per-entity edge info: source_name, target_name, relation, weight (captures the FIRST edge
    // that reached this entity — equivalent to BFS shortest path recall edge).
    let mut entity_edge: HashMap<i64, (String, String, String, f64)> = HashMap::new();

    let mut queue: VecDeque<i64> = seed_entity_ids.iter().copied().collect();

    while let Some(current_entity) = queue.pop_front() {
        let current_hop = *entity_hop.get(&current_entity).unwrap_or(&0);
        if current_hop >= max_hops {
            continue;
        }

        let neighbours =
            fetch_neighbours(conn, current_entity, namespace, min_weight, relation_filter)?;

        for (neighbour_id, source_name, target_name, relation, weight) in neighbours {
            if visited.insert(neighbour_id) {
                entity_hop.insert(neighbour_id, current_hop + 1);
                entity_edge.insert(neighbour_id, (source_name, target_name, relation, weight));
                queue.push_back(neighbour_id);
            }
        }
    }

    // For each discovered entity (hop >= 1) find its memories, skipping the seed memory.
    let mut out: Vec<RelatedMemory> = Vec::new();
    let mut dedup_ids: HashSet<i64> = HashSet::new();
    dedup_ids.insert(seed_memory_id);

    // Sort entities by hop ASC, weight DESC so we emit closer entities first.
    let mut ordered_entities: Vec<(i64, u32)> = entity_hop
        .iter()
        .filter(|(id, _)| !seed_entity_ids.contains(id))
        .map(|(id, hop)| (*id, *hop))
        .collect();
    ordered_entities.sort_by(|a, b| {
        let weight_a = entity_edge.get(&a.0).map(|e| e.3).unwrap_or(0.0);
        let weight_b = entity_edge.get(&b.0).map(|e| e.3).unwrap_or(0.0);
        a.1.cmp(&b.1).then_with(|| {
            weight_b
                .partial_cmp(&weight_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });

    for (entity_id, hop) in ordered_entities {
        let mut stmt = conn.prepare_cached(
            "SELECT m.id, m.name, m.namespace, m.type, m.description
             FROM memory_entities me
             JOIN memories m ON m.id = me.memory_id
             WHERE me.entity_id = ?1 AND m.deleted_at IS NULL",
        )?;
        let rows = stmt
            .query_map(params![entity_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        for (mid, name, ns, mtype, desc) in rows {
            if !dedup_ids.insert(mid) {
                continue;
            }
            let edge = entity_edge.get(&entity_id);
            out.push(RelatedMemory {
                memory_id: mid,
                name,
                namespace: ns,
                memory_type: mtype,
                description: desc,
                hop_distance: hop,
                source_entity: edge.map(|e| e.0.clone()),
                target_entity: edge.map(|e| e.1.clone()),
                relation: edge.map(|e| e.2.clone()),
                weight: edge.map(|e| e.3),
            });
            if out.len() >= limit {
                return Ok(out);
            }
        }
    }

    Ok(out)
}

fn fetch_neighbours(
    conn: &Connection,
    entity_id: i64,
    namespace: &str,
    min_weight: f64,
    relation_filter: Option<&str>,
) -> Result<Vec<Neighbour>, AppError> {
    // Follow edges in both directions: source -> target and target -> source so traversal is
    // undirected, which is how users typically reason about "related" memories.
    let base_sql = "\
        SELECT r.target_id, se.name, te.name, r.relation, r.weight
        FROM relationships r
        JOIN entities se ON se.id = r.source_id
        JOIN entities te ON te.id = r.target_id
        WHERE r.source_id = ?1 AND r.weight >= ?2 AND r.namespace = ?3";

    let reverse_sql = "\
        SELECT r.source_id, se.name, te.name, r.relation, r.weight
        FROM relationships r
        JOIN entities se ON se.id = r.source_id
        JOIN entities te ON te.id = r.target_id
        WHERE r.target_id = ?1 AND r.weight >= ?2 AND r.namespace = ?3";

    let mut results: Vec<Neighbour> = Vec::new();

    let forward_sql = match relation_filter {
        Some(_) => format!("{base_sql} AND r.relation = ?4"),
        None => base_sql.to_string(),
    };
    let rev_sql = match relation_filter {
        Some(_) => format!("{reverse_sql} AND r.relation = ?4"),
        None => reverse_sql.to_string(),
    };

    let mut stmt = conn.prepare_cached(&forward_sql)?;
    let rows: Vec<_> = if let Some(rel) = relation_filter {
        stmt.query_map(params![entity_id, min_weight, namespace, rel], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, f64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map(params![entity_id, min_weight, namespace], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, f64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    results.extend(rows);

    let mut stmt = conn.prepare_cached(&rev_sql)?;
    let rows: Vec<_> = if let Some(rel) = relation_filter {
        stmt.query_map(params![entity_id, min_weight, namespace, rel], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, f64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map(params![entity_id, min_weight, namespace], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, f64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    results.extend(rows);

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_related_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("failed to open in-memory db");
        conn.execute_batch(
            "CREATE TABLE memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                namespace TEXT NOT NULL DEFAULT 'global',
                type TEXT NOT NULL DEFAULT 'fact',
                description TEXT NOT NULL DEFAULT '',
                deleted_at INTEGER
            );
            CREATE TABLE entities (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                namespace TEXT NOT NULL,
                name TEXT NOT NULL
            );
            CREATE TABLE relationships (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                namespace TEXT NOT NULL,
                source_id INTEGER NOT NULL,
                target_id INTEGER NOT NULL,
                relation TEXT NOT NULL DEFAULT 'related_to',
                weight REAL NOT NULL DEFAULT 1.0
            );
            CREATE TABLE memory_entities (
                memory_id INTEGER NOT NULL,
                entity_id INTEGER NOT NULL
            );",
        )
        .expect("failed to create test tables");
        conn
    }

    fn insert_memory(conn: &rusqlite::Connection, name: &str, namespace: &str) -> i64 {
        conn.execute(
            "INSERT INTO memories (name, namespace) VALUES (?1, ?2)",
            rusqlite::params![name, namespace],
        )
        .expect("failed to insert memory");
        conn.last_insert_rowid()
    }

    fn insert_entity(conn: &rusqlite::Connection, name: &str, namespace: &str) -> i64 {
        conn.execute(
            "INSERT INTO entities (name, namespace) VALUES (?1, ?2)",
            rusqlite::params![name, namespace],
        )
        .expect("failed to insert entity");
        conn.last_insert_rowid()
    }

    fn link_memory_entity(conn: &rusqlite::Connection, memory_id: i64, entity_id: i64) {
        conn.execute(
            "INSERT INTO memory_entities (memory_id, entity_id) VALUES (?1, ?2)",
            rusqlite::params![memory_id, entity_id],
        )
        .expect("failed to link memory-entity");
    }

    fn insert_relationship(
        conn: &rusqlite::Connection,
        namespace: &str,
        source_id: i64,
        target_id: i64,
        relation: &str,
        weight: f64,
    ) {
        conn.execute(
            "INSERT INTO relationships (namespace, source_id, target_id, relation, weight)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![namespace, source_id, target_id, relation, weight],
        )
        .expect("failed to insert relationship");
    }

    #[test]
    fn related_response_serializes_results_and_elapsed_ms() {
        let resp = RelatedResponse {
            name: "seed-mem".to_string(),
            max_hops: 2,
            results: vec![RelatedMemory {
                memory_id: 1,
                name: "neighbor-mem".to_string(),
                namespace: "global".to_string(),
                memory_type: "document".to_string(),
                description: "desc".to_string(),
                hop_distance: 1,
                source_entity: Some("entity-a".to_string()),
                target_entity: Some("entity-b".to_string()),
                relation: Some("related_to".to_string()),
                weight: Some(0.9),
            }],
            elapsed_ms: 7,
        };

        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert!(json["results"].is_array());
        assert_eq!(json["results"].as_array().unwrap().len(), 1);
        assert_eq!(json["elapsed_ms"], 7u64);
        assert_eq!(json["results"][0]["type"], "document");
        assert_eq!(json["results"][0]["hop_distance"], 1);
    }

    #[test]
    fn traverse_related_returns_empty_without_seed_entities() {
        let conn = setup_related_db();
        let result = traverse_related(&conn, 1, &[], "global", 2, 0.0, None, 10)
            .expect("traverse_related failed");
        assert!(
            result.is_empty(),
            "no seed entities must yield empty results"
        );
    }

    #[test]
    fn traverse_related_returns_empty_with_max_hops_zero() {
        let conn = setup_related_db();
        let mem_id = insert_memory(&conn, "seed-mem", "global");
        let ent_id = insert_entity(&conn, "ent-a", "global");
        link_memory_entity(&conn, mem_id, ent_id);

        let result = traverse_related(&conn, mem_id, &[ent_id], "global", 0, 0.0, None, 10)
            .expect("traverse_related failed");
        assert!(result.is_empty(), "max_hops=0 must return empty");
    }

    #[test]
    fn traverse_related_discovers_neighbor_memory_via_graph() {
        let conn = setup_related_db();

        let seed_id = insert_memory(&conn, "seed-mem", "global");
        let neighbor_id = insert_memory(&conn, "neighbor-mem", "global");
        let ent_a = insert_entity(&conn, "ent-a", "global");
        let ent_b = insert_entity(&conn, "ent-b", "global");

        link_memory_entity(&conn, seed_id, ent_a);
        link_memory_entity(&conn, neighbor_id, ent_b);
        insert_relationship(&conn, "global", ent_a, ent_b, "related_to", 1.0);

        let result = traverse_related(&conn, seed_id, &[ent_a], "global", 2, 0.0, None, 10)
            .expect("traverse_related failed");

        assert_eq!(result.len(), 1, "must find 1 neighboring memory");
        assert_eq!(result[0].name, "neighbor-mem");
        assert_eq!(result[0].hop_distance, 1);
    }

    #[test]
    fn traverse_related_respects_limit() {
        let conn = setup_related_db();

        let seed_id = insert_memory(&conn, "seed", "global");
        let ent_seed = insert_entity(&conn, "ent-seed", "global");
        link_memory_entity(&conn, seed_id, ent_seed);

        for i in 0..5 {
            let mem_id = insert_memory(&conn, &format!("neighbor-{i}"), "global");
            let ent_id = insert_entity(&conn, &format!("ent-{i}"), "global");
            link_memory_entity(&conn, mem_id, ent_id);
            insert_relationship(&conn, "global", ent_seed, ent_id, "related_to", 1.0);
        }

        let result = traverse_related(&conn, seed_id, &[ent_seed], "global", 1, 0.0, None, 3)
            .expect("traverse_related failed");

        assert!(
            result.len() <= 3,
            "limit=3 must constrain to at most 3 results"
        );
    }

    #[test]
    fn related_memory_optional_null_fields_serialized() {
        let mem = RelatedMemory {
            memory_id: 99,
            name: "no-relation".to_string(),
            namespace: "ns".to_string(),
            memory_type: "concept".to_string(),
            description: "".to_string(),
            hop_distance: 2,
            source_entity: None,
            target_entity: None,
            relation: None,
            weight: None,
        };

        let json = serde_json::to_value(&mem).expect("serialization failed");
        assert!(json["source_entity"].is_null());
        assert!(json["target_entity"].is_null());
        assert!(json["relation"].is_null());
        assert!(json["weight"].is_null());
        assert_eq!(json["hop_distance"], 2);
    }
}
