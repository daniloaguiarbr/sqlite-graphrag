use crate::cli::RelationKind;
use crate::constants::{
    DEFAULT_K_RECALL, DEFAULT_MAX_HOPS, DEFAULT_MIN_WEIGHT, TEXT_DESCRIPTION_PREVIEW_LEN,
};
use crate::errors::AppError;
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
pub struct RelatedArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long, default_value_t = DEFAULT_MAX_HOPS)]
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
    #[arg(long, env = "NEUROGRAPHRAG_DB_PATH")]
    pub db: Option<String>,
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
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if !paths.db.exists() {
        return Err(AppError::NotFound(format!(
            "database not found at {}. Run 'neurographrag init' first.",
            paths.db.display()
        )));
    }

    let conn = open_ro(&paths.db)?;

    // Locate the seed memory.
    let seed_id: i64 = match conn.query_row(
        "SELECT id FROM memories
         WHERE namespace = ?1 AND name = ?2 AND deleted_at IS NULL",
        params![namespace, args.name],
        |r| r.get(0),
    ) {
        Ok(id) => id,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(AppError::NotFound(format!(
                "memory '{}' not found in namespace '{}'",
                args.name, namespace
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
        OutputFormat::Json => output::emit_json(&results)?,
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
