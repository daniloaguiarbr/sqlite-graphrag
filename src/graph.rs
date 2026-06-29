//! Entity graph traversal (BFS over memory_entities + relations).
//!
//! Queries the SQLite entity and relation tables to expand neighbourhood
//! sets used by the `related` and `recall` commands.

// src/graph.rs

use crate::errors::AppError;
use rusqlite::{params, Connection};

/// Traverses the entity graph by BFS from seed memories.
///
/// Returns `memory_id`s reachable through entity and relationship edges,
/// excluding the seeds themselves. The algorithm:
/// 1. Collects entities associated with seeds via `memory_entities`.
/// 2. Runs BFS over `relationships` filtered by `weight >= min_weight` and `namespace`.
/// 3. Returns memories linked to discovered entities (excluding soft-deleted).
///
/// # Errors
///
/// Propagates [`AppError::Database`] (exit 10) on SQLite query failures.
///
/// # Examples
///
/// ```
/// use rusqlite::Connection;
/// use sqlite_graphrag::graph::traverse_from_memories;
///
/// // Empty seed list returns immediately without querying the database.
/// let conn = Connection::open_in_memory().unwrap();
/// let ids = traverse_from_memories(&conn, &[], "global", 0.5, 3).unwrap();
/// assert!(ids.is_empty());
/// ```
///
/// ```
/// use rusqlite::Connection;
/// use sqlite_graphrag::graph::traverse_from_memories;
///
/// // max_hops == 0 returns immediately without traversal.
/// let conn = Connection::open_in_memory().unwrap();
/// let ids = traverse_from_memories(&conn, &[1, 2], "global", 0.5, 0).unwrap();
/// assert!(ids.is_empty());
/// ```
pub fn traverse_from_memories(
    conn: &Connection,
    seed_memory_ids: &[i64],
    namespace: &str,
    min_weight: f64,
    max_hops: u32,
) -> Result<Vec<i64>, AppError> {
    if seed_memory_ids.is_empty() || max_hops == 0 {
        return Ok(vec![]);
    }

    // Step 1: collect seed entity IDs from seed memories
    let mut seed_entities: Vec<i64> = Vec::with_capacity(seed_memory_ids.len());
    for &mem_id in seed_memory_ids {
        let mut stmt =
            conn.prepare_cached("SELECT entity_id FROM memory_entities WHERE memory_id = ?1")?;
        let ids: Vec<i64> = stmt
            .query_map(params![mem_id], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        seed_entities.extend(ids);
    }
    seed_entities.sort_unstable();
    seed_entities.dedup();

    if seed_entities.is_empty() {
        return Ok(vec![]);
    }

    // Step 2: BFS over relationships
    use std::collections::HashSet;
    let mut visited: HashSet<i64> = seed_entities.iter().copied().collect();
    let mut frontier: Vec<i64> = seed_entities.to_vec();

    for _ in 0..max_hops {
        if frontier.is_empty() {
            break;
        }
        let mut next_frontier = Vec::with_capacity(frontier.len() * 2);

        for &entity_id in &frontier {
            let mut stmt = conn.prepare_cached(
                "SELECT target_id FROM relationships
                 WHERE source_id = ?1 AND weight >= ?2 AND namespace = ?3",
            )?;
            let neighbors: Vec<i64> = stmt
                .query_map(params![entity_id, min_weight, namespace], |r| r.get(0))?
                .filter_map(|r| r.ok())
                .filter(|id| !visited.contains(id))
                .collect();

            for id in neighbors {
                visited.insert(id);
                next_frontier.push(id);
            }
        }
        frontier = next_frontier;
    }

    // Step 3: find memories connected to traversed entities (excluding seeds)
    let seed_set: HashSet<i64> = seed_memory_ids.iter().copied().collect();
    let graph_only_entities: Vec<i64> = visited
        .into_iter()
        .filter(|id| !seed_entities.contains(id))
        .collect();

    let mut result_ids: Vec<i64> = Vec::with_capacity(graph_only_entities.len());
    for &entity_id in &graph_only_entities {
        let mut stmt = conn.prepare_cached(
            "SELECT DISTINCT me.memory_id
             FROM memory_entities me
             JOIN memories m ON m.id = me.memory_id
             WHERE me.entity_id = ?1 AND m.deleted_at IS NULL",
        )?;
        let mem_ids: Vec<i64> = stmt
            .query_map(params![entity_id], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .filter(|id| !seed_set.contains(id))
            .collect();
        result_ids.extend(mem_ids);
    }

    result_ids.sort_unstable();
    result_ids.dedup();
    Ok(result_ids)
}

/// BFS graph traversal that also returns the hop distance for each reached memory.
///
/// Identical to [`traverse_from_memories`] but returns `(memory_id, hop_count)` tuples
/// instead of bare IDs. `hop_count` is the BFS depth at which the entity was first
/// discovered, starting from 1 for direct neighbours of the seed entities.
///
/// When `max_neighbors_per_hop` is `Some(k)`, only the top-`k` neighbours by
/// `weight DESC` are followed at each entity expansion.  Pass `None` to retain
/// the original behaviour (all neighbours above `min_weight` are followed).
///
/// # Errors
///
/// Propagates [`AppError::Database`] (exit 10) on SQLite query failures.
pub fn traverse_from_memories_with_hops(
    conn: &Connection,
    seed_memory_ids: &[i64],
    namespace: &str,
    min_weight: f64,
    max_hops: u32,
) -> Result<Vec<(i64, u32)>, AppError> {
    traverse_from_memories_with_hops_inner(
        conn,
        seed_memory_ids,
        namespace,
        min_weight,
        max_hops,
        None,
    )
}

/// Extended variant that accepts an optional neighbour cap per hop.
///
/// Pass `max_neighbors_per_hop = Some(k)` to prune each entity's expansion to
/// its top-`k` neighbours by edge weight, limiting combinatorial blow-up in
/// dense graphs.  `None` is equivalent to the public
/// [`traverse_from_memories_with_hops`] function.
///
/// # Errors
///
/// Propagates [`AppError::Database`] (exit 10) on SQLite query failures.
pub fn traverse_from_memories_with_hops_capped(
    conn: &Connection,
    seed_memory_ids: &[i64],
    namespace: &str,
    min_weight: f64,
    max_hops: u32,
    max_neighbors_per_hop: Option<usize>,
) -> Result<Vec<(i64, u32)>, AppError> {
    traverse_from_memories_with_hops_inner(
        conn,
        seed_memory_ids,
        namespace,
        min_weight,
        max_hops,
        max_neighbors_per_hop,
    )
}

fn traverse_from_memories_with_hops_inner(
    conn: &Connection,
    seed_memory_ids: &[i64],
    namespace: &str,
    min_weight: f64,
    max_hops: u32,
    max_neighbors_per_hop: Option<usize>,
) -> Result<Vec<(i64, u32)>, AppError> {
    if seed_memory_ids.is_empty() || max_hops == 0 {
        return Ok(vec![]);
    }

    // Collect seed entity IDs from seed memories
    let mut seed_entities: Vec<i64> = Vec::with_capacity(seed_memory_ids.len());
    for &mem_id in seed_memory_ids {
        let mut stmt =
            conn.prepare_cached("SELECT entity_id FROM memory_entities WHERE memory_id = ?1")?;
        let ids: Vec<i64> = stmt
            .query_map(params![mem_id], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        seed_entities.extend(ids);
    }
    seed_entities.sort_unstable();
    seed_entities.dedup();

    if seed_entities.is_empty() {
        return Ok(vec![]);
    }

    // BFS over relationships, tracking depth per entity
    use std::collections::HashMap;
    let mut entity_depth: HashMap<i64, u32> = seed_entities.iter().map(|&id| (id, 0)).collect();
    let mut frontier: Vec<i64> = seed_entities.to_vec();

    for hop in 1..=max_hops {
        if frontier.is_empty() {
            break;
        }
        let mut next_frontier = Vec::with_capacity(frontier.len() * 2);

        for &entity_id in &frontier {
            // Fetch neighbours ordered by weight DESC to support capping.
            let mut stmt = conn.prepare_cached(
                "SELECT target_id, weight FROM relationships
                 WHERE source_id = ?1 AND weight >= ?2 AND namespace = ?3
                 ORDER BY weight DESC",
            )?;
            let mut neighbors: Vec<i64> = stmt
                .query_map(params![entity_id, min_weight, namespace], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
                })?
                .filter_map(|r| r.ok())
                .filter(|(id, _)| !entity_depth.contains_key(id))
                .map(|(id, _)| id)
                .collect();

            // Apply optional per-hop neighbour cap.
            if let Some(cap) = max_neighbors_per_hop {
                neighbors.truncate(cap);
            }

            for id in neighbors {
                entity_depth.insert(id, hop);
                next_frontier.push(id);
            }
        }
        frontier = next_frontier;
    }

    // Find memories connected to traversed entities (excluding seeds), preserving hop depth
    let seed_set: std::collections::HashSet<i64> = seed_memory_ids.iter().copied().collect();
    let seed_entity_set: std::collections::HashSet<i64> = seed_entities.iter().copied().collect();

    let mut result: Vec<(i64, u32)> = Vec::with_capacity(entity_depth.len());
    let mut seen_memories: std::collections::HashSet<i64> =
        std::collections::HashSet::with_capacity(entity_depth.len());

    for (&entity_id, &hop) in &entity_depth {
        if seed_entity_set.contains(&entity_id) {
            continue;
        }
        let mut stmt = conn.prepare_cached(
            "SELECT DISTINCT me.memory_id
             FROM memory_entities me
             JOIN memories m ON m.id = me.memory_id
             WHERE me.entity_id = ?1 AND m.deleted_at IS NULL",
        )?;
        let mem_ids: Vec<i64> = stmt
            .query_map(params![entity_id], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .filter(|id| !seed_set.contains(id) && !seen_memories.contains(id))
            .collect();

        for mem_id in mem_ids {
            seen_memories.insert(mem_id);
            result.push((mem_id, hop));
        }
    }

    result.sort_unstable_by_key(|&(id, _)| id);
    Ok(result)
}

/// Depth map from BFS: entity_id → hop distance from seeds.
pub type EntityDepthMap = std::collections::HashMap<i64, u32>;

/// Predecessor map from BFS: entity_id → (parent_entity_id, relation_type, edge_weight).
///
/// Enables path reconstruction from any discovered entity back to a seed.
pub type PredecessorMap = std::collections::HashMap<i64, (i64, String, f64)>;

/// BFS that also returns a predecessor map for path reconstruction.
///
/// Used by `deep-research` to reconstruct directed evidence chains from
/// discovered entities back to their seeds.
///
/// Returns `(entity_depth, predecessor)` where:
/// - `entity_depth`: depth of each reached entity (0 = seed).
/// - `predecessor`: the BFS tree edge that first reached each non-seed entity.
///
/// # Errors
///
/// Propagates [`AppError::Database`] (exit 10) on SQLite query failures.
pub fn bfs_with_predecessors(
    conn: &Connection,
    seed_entity_ids: &[i64],
    namespace: &str,
    min_weight: f64,
    max_hops: u32,
    max_neighbors_per_hop: Option<usize>,
) -> Result<(EntityDepthMap, PredecessorMap), AppError> {
    use std::collections::HashMap;

    let mut entity_depth: HashMap<i64, u32> = seed_entity_ids.iter().map(|&id| (id, 0)).collect();
    let mut predecessor: HashMap<i64, (i64, String, f64)> =
        HashMap::with_capacity(max_hops as usize * 10);
    let mut frontier: Vec<i64> = seed_entity_ids.to_vec();

    for hop in 1..=max_hops {
        if frontier.is_empty() {
            break;
        }
        let mut next_frontier = Vec::with_capacity(frontier.len() * 2);

        for &entity_id in &frontier {
            let mut stmt = conn.prepare_cached(
                "SELECT target_id, relation, weight FROM relationships
                 WHERE source_id = ?1 AND weight >= ?2 AND namespace = ?3
                 ORDER BY weight DESC",
            )?;
            let mut neighbors: Vec<(i64, String, f64)> = stmt
                .query_map(params![entity_id, min_weight, namespace], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, f64>(2)?,
                    ))
                })?
                .filter_map(|r| r.ok())
                .filter(|(id, _, _)| !entity_depth.contains_key(id))
                .collect();

            if let Some(cap) = max_neighbors_per_hop {
                neighbors.truncate(cap);
            }

            for (id, relation, weight) in neighbors {
                entity_depth.insert(id, hop);
                predecessor.insert(id, (entity_id, relation, weight));
                next_frontier.push(id);
            }
        }
        frontier = next_frontier;
    }

    Ok((entity_depth, predecessor))
}

/// Enforces the per-entity degree cap by pruning the lowest-weight incident
/// edges until the entity's degree is at most `cap` (GAP-SG-49).
///
/// The degree cap used to be purely advisory — callers only emitted a `WARN`
/// while hubs grew to 152+ connections. This makes it ACTIONABLE: after an
/// edge is inserted, call this to bring the entity back under `cap`.
///
/// Degree is the undirected count of relationships where the entity is the
/// source or the target. While over the cap, the lowest-weight incident edge
/// is deleted; ties break by the smallest `id` (oldest edge). If the
/// just-inserted edge is itself the weakest, it is the one removed — i.e. the
/// new edge is effectively refused. The degree of every affected endpoint is
/// recalculated after each deletion.
///
/// A `cap <= 0` disables enforcement and returns `Ok(0)`. Returns the number
/// of edges pruned. Accepts `&Connection`; a `&Transaction` coerces via
/// `Deref`, so callers may run it inside an open transaction.
///
/// # Errors
///
/// Propagates [`AppError::Database`] (exit 10) on SQLite query failures.
pub fn enforce_degree_cap(conn: &Connection, entity_id: i64, cap: i64) -> Result<usize, AppError> {
    if cap <= 0 {
        return Ok(0);
    }
    let mut pruned = 0usize;
    loop {
        let degree: i64 = conn.query_row(
            "SELECT COUNT(*) FROM relationships WHERE source_id = ?1 OR target_id = ?1",
            params![entity_id],
            |r| r.get(0),
        )?;
        if degree <= cap {
            break;
        }
        // Weakest incident edge; oldest (smallest id) breaks weight ties.
        let (rel_id, src, tgt): (i64, i64, i64) = conn.query_row(
            "SELECT id, source_id, target_id FROM relationships
             WHERE source_id = ?1 OR target_id = ?1
             ORDER BY weight ASC, id ASC
             LIMIT 1",
            params![entity_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        conn.execute("DELETE FROM relationships WHERE id = ?1", params![rel_id])?;
        crate::storage::entities::recalculate_degree(conn, src)?;
        if tgt != src {
            crate::storage::entities::recalculate_degree(conn, tgt)?;
        }
        pruned += 1;
    }
    Ok(pruned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE memories (
                id INTEGER PRIMARY KEY,
                namespace TEXT NOT NULL,
                deleted_at TEXT
            );
            CREATE TABLE memory_entities (
                memory_id INTEGER NOT NULL,
                entity_id INTEGER NOT NULL
            );
            CREATE TABLE relationships (
                source_id INTEGER NOT NULL,
                target_id INTEGER NOT NULL,
                weight REAL NOT NULL,
                namespace TEXT NOT NULL
            );",
        )
        .unwrap();
        conn
    }

    fn insert_memory(conn: &Connection, id: i64, namespace: &str, deleted: bool) {
        conn.execute(
            "INSERT INTO memories (id, namespace, deleted_at) VALUES (?1, ?2, ?3)",
            params![
                id,
                namespace,
                if deleted { Some("2024-01-01") } else { None }
            ],
        )
        .unwrap();
    }

    fn link_memory_entity(conn: &Connection, memory_id: i64, entity_id: i64) {
        conn.execute(
            "INSERT INTO memory_entities (memory_id, entity_id) VALUES (?1, ?2)",
            params![memory_id, entity_id],
        )
        .unwrap();
    }

    fn insert_relationship(conn: &Connection, src: i64, tgt: i64, weight: f64, ns: &str) {
        conn.execute(
            "INSERT INTO relationships (source_id, target_id, weight, namespace) VALUES (?1, ?2, ?3, ?4)",
            params![src, tgt, weight, ns],
        )
        .unwrap();
    }

    // --- edge cases retornando vazio ---

    #[test]
    fn returns_empty_when_seeds_empty() {
        let conn = setup_db();
        let result = traverse_from_memories(&conn, &[], "ns", 0.5, 3).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn returns_empty_when_max_hops_zero() {
        let conn = setup_db();
        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);
        let result = traverse_from_memories(&conn, &[1], "ns", 0.5, 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn returns_empty_when_seed_has_no_entities() {
        let conn = setup_db();
        insert_memory(&conn, 1, "ns", false);
        // memory exists but has no associated entities
        let result = traverse_from_memories(&conn, &[1], "ns", 0.5, 3).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn returns_empty_when_no_relationships() {
        let conn = setup_db();
        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);
        // entity 10 has no relationships
        let result = traverse_from_memories(&conn, &[1], "ns", 0.5, 3).unwrap();
        assert!(result.is_empty());
    }

    // --- basic happy path ---

    #[test]
    fn traversal_basic_one_hop() {
        let conn = setup_db();

        // seed: memory 1 com entity 10
        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);

        // vizinha: entity 20 ligada a memory 2
        insert_memory(&conn, 2, "ns", false);
        link_memory_entity(&conn, 2, 20);

        // relacionamento 10 -> 20
        insert_relationship(&conn, 10, 20, 1.0, "ns");

        let result = traverse_from_memories(&conn, &[1], "ns", 0.5, 1).unwrap();
        assert_eq!(result, vec![2]);
    }

    #[test]
    fn traversal_two_hops() {
        let conn = setup_db();

        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);

        insert_memory(&conn, 2, "ns", false);
        link_memory_entity(&conn, 2, 20);

        insert_memory(&conn, 3, "ns", false);
        link_memory_entity(&conn, 3, 30);

        // cadeia 10 -> 20 -> 30
        insert_relationship(&conn, 10, 20, 1.0, "ns");
        insert_relationship(&conn, 20, 30, 1.0, "ns");

        let mut result = traverse_from_memories(&conn, &[1], "ns", 0.5, 2).unwrap();
        result.sort_unstable();
        assert_eq!(result, vec![2, 3]);
    }

    #[test]
    fn max_hops_limits_depth() {
        let conn = setup_db();

        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);

        insert_memory(&conn, 2, "ns", false);
        link_memory_entity(&conn, 2, 20);

        insert_memory(&conn, 3, "ns", false);
        link_memory_entity(&conn, 3, 30);

        insert_relationship(&conn, 10, 20, 1.0, "ns");
        insert_relationship(&conn, 20, 30, 1.0, "ns");

        // with only 1 hop, memory 3 must not appear
        let result = traverse_from_memories(&conn, &[1], "ns", 0.5, 1).unwrap();
        assert_eq!(result, vec![2]);
        assert!(!result.contains(&3));
    }

    // --- filtro de peso ---

    #[test]
    fn relationship_with_weight_below_min_ignored() {
        let conn = setup_db();

        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);

        insert_memory(&conn, 2, "ns", false);
        link_memory_entity(&conn, 2, 20);

        // peso 0.3 < min_weight 0.5
        insert_relationship(&conn, 10, 20, 0.3, "ns");

        let result = traverse_from_memories(&conn, &[1], "ns", 0.5, 3).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn relationship_with_weight_exactly_at_min_included() {
        let conn = setup_db();

        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);

        insert_memory(&conn, 2, "ns", false);
        link_memory_entity(&conn, 2, 20);

        insert_relationship(&conn, 10, 20, 0.5, "ns");

        let result = traverse_from_memories(&conn, &[1], "ns", 0.5, 1).unwrap();
        assert_eq!(result, vec![2]);
    }

    // --- isolamento de namespace ---

    #[test]
    fn relationship_from_different_namespace_ignored() {
        let conn = setup_db();

        insert_memory(&conn, 1, "ns_a", false);
        link_memory_entity(&conn, 1, 10);

        insert_memory(&conn, 2, "ns_a", false);
        link_memory_entity(&conn, 2, 20);

        // relacionamento no namespace errado
        insert_relationship(&conn, 10, 20, 1.0, "ns_b");

        let result = traverse_from_memories(&conn, &[1], "ns_a", 0.5, 3).unwrap();
        assert!(result.is_empty());
    }

    // --- exclude seeds from result ---

    #[test]
    fn seeds_do_not_appear_in_result() {
        let conn = setup_db();

        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);

        insert_memory(&conn, 2, "ns", false);
        link_memory_entity(&conn, 2, 20);

        // relacionamento de 20 de volta para 10 (ciclo)
        insert_relationship(&conn, 10, 20, 1.0, "ns");
        insert_relationship(&conn, 20, 10, 1.0, "ns");

        let result = traverse_from_memories(&conn, &[1], "ns", 0.5, 3).unwrap();
        // memory 1 must not appear even with a cycle
        assert!(!result.contains(&1));
        assert_eq!(result, vec![2]);
    }

    // --- soft-deleted memories excluded ---

    #[test]
    fn deleted_memories_not_included() {
        let conn = setup_db();

        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);

        // memory 2 foi deletada
        insert_memory(&conn, 2, "ns", true);
        link_memory_entity(&conn, 2, 20);

        insert_relationship(&conn, 10, 20, 1.0, "ns");

        let result = traverse_from_memories(&conn, &[1], "ns", 0.5, 3).unwrap();
        assert!(result.is_empty());
    }

    // --- multiple seeds ---

    #[test]
    fn multiple_seeds_merged_in_result() {
        let conn = setup_db();

        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);

        insert_memory(&conn, 2, "ns", false);
        link_memory_entity(&conn, 2, 20);

        insert_memory(&conn, 3, "ns", false);
        link_memory_entity(&conn, 3, 30);

        insert_memory(&conn, 4, "ns", false);
        link_memory_entity(&conn, 4, 40);

        insert_relationship(&conn, 10, 30, 1.0, "ns");
        insert_relationship(&conn, 20, 40, 1.0, "ns");

        let mut result = traverse_from_memories(&conn, &[1, 2], "ns", 0.5, 1).unwrap();
        result.sort_unstable();
        assert_eq!(result, vec![3, 4]);
    }

    // --- result deduplication ---

    #[test]
    fn result_without_duplicates() {
        let conn = setup_db();

        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);
        link_memory_entity(&conn, 1, 11); // dois seeds na mesma memory

        insert_memory(&conn, 2, "ns", false);
        link_memory_entity(&conn, 2, 20);

        // ambos os seeds apontam para a mesma entity 20
        insert_relationship(&conn, 10, 20, 1.0, "ns");
        insert_relationship(&conn, 11, 20, 1.0, "ns");

        let result = traverse_from_memories(&conn, &[1], "ns", 0.5, 1).unwrap();
        // memory 2 deve aparecer apenas uma vez
        assert_eq!(result.len(), 1);
        assert_eq!(result, vec![2]);
    }

    // --- single node ---

    #[test]
    fn single_node_without_neighbors_returns_empty() {
        let conn = setup_db();

        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);
        // entity 10 has no outgoing relationships

        let result = traverse_from_memories(&conn, &[1], "ns", 0.5, 5).unwrap();
        assert!(result.is_empty());
    }

    // --- ciclos no grafo ---

    #[test]
    fn cycle_does_not_cause_infinite_loop() {
        let conn = setup_db();

        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);

        insert_memory(&conn, 2, "ns", false);
        link_memory_entity(&conn, 2, 20);

        insert_memory(&conn, 3, "ns", false);
        link_memory_entity(&conn, 3, 30);

        // triangle 10 -> 20 -> 30 -> 10
        insert_relationship(&conn, 10, 20, 1.0, "ns");
        insert_relationship(&conn, 20, 30, 1.0, "ns");
        insert_relationship(&conn, 30, 10, 1.0, "ns");

        let mut result = traverse_from_memories(&conn, &[1], "ns", 0.5, 10).unwrap();
        result.sort_unstable();
        // deve retornar 2 e 3 sem loop infinito
        assert_eq!(result, vec![2, 3]);
    }

    // --- GAP-SG-49: degree cap enforcement ---

    fn setup_cap_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE entities (
                id INTEGER PRIMARY KEY,
                degree INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE relationships (
                id INTEGER PRIMARY KEY,
                source_id INTEGER NOT NULL,
                target_id INTEGER NOT NULL,
                weight REAL NOT NULL,
                namespace TEXT NOT NULL DEFAULT 'ns'
            );",
        )
        .unwrap();
        conn
    }

    fn insert_entity(conn: &Connection, id: i64) {
        conn.execute("INSERT INTO entities (id) VALUES (?1)", params![id])
            .unwrap();
    }

    fn insert_edge(conn: &Connection, id: i64, src: i64, tgt: i64, weight: f64) {
        conn.execute(
            "INSERT INTO relationships (id, source_id, target_id, weight) VALUES (?1, ?2, ?3, ?4)",
            params![id, src, tgt, weight],
        )
        .unwrap();
    }

    fn degree_of(conn: &Connection, entity_id: i64) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM relationships WHERE source_id = ?1 OR target_id = ?1",
            params![entity_id],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn edge_exists(conn: &Connection, id: i64) -> bool {
        conn.query_row(
            "SELECT COUNT(*) FROM relationships WHERE id = ?1",
            params![id],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            > 0
    }

    #[test]
    fn cap_zero_or_negative_is_noop() {
        let conn = setup_cap_db();
        insert_entity(&conn, 1);
        insert_edge(&conn, 100, 1, 2, 0.9);
        assert_eq!(enforce_degree_cap(&conn, 1, 0).unwrap(), 0);
        assert_eq!(enforce_degree_cap(&conn, 1, -5).unwrap(), 0);
        assert!(edge_exists(&conn, 100));
    }

    #[test]
    fn cap_under_limit_prunes_nothing() {
        let conn = setup_cap_db();
        insert_entity(&conn, 1);
        insert_edge(&conn, 100, 1, 2, 0.9);
        insert_edge(&conn, 101, 1, 3, 0.8);
        assert_eq!(enforce_degree_cap(&conn, 1, 5).unwrap(), 0);
        assert_eq!(degree_of(&conn, 1), 2);
    }

    #[test]
    fn cap_exceeded_removes_lowest_weight_edge() {
        let conn = setup_cap_db();
        insert_entity(&conn, 1);
        // Three incident edges; cap of 2 must drop exactly the weakest (0.2).
        insert_edge(&conn, 100, 1, 2, 0.9);
        insert_edge(&conn, 101, 1, 3, 0.2); // weakest -> victim
        insert_edge(&conn, 102, 1, 4, 0.7);

        let pruned = enforce_degree_cap(&conn, 1, 2).unwrap();

        assert_eq!(pruned, 1);
        assert_eq!(degree_of(&conn, 1), 2);
        assert!(!edge_exists(&conn, 101), "weakest edge must be removed");
        assert!(edge_exists(&conn, 100));
        assert!(edge_exists(&conn, 102));
    }

    #[test]
    fn cap_refuses_just_inserted_weakest_edge() {
        let conn = setup_cap_db();
        insert_entity(&conn, 1);
        insert_edge(&conn, 100, 1, 2, 0.9);
        insert_edge(&conn, 101, 1, 3, 0.8);
        // Newly inserted edge is the weakest -> it is the one refused.
        insert_edge(&conn, 102, 1, 4, 0.1);

        let pruned = enforce_degree_cap(&conn, 1, 2).unwrap();

        assert_eq!(pruned, 1);
        assert_eq!(degree_of(&conn, 1), 2);
        assert!(!edge_exists(&conn, 102), "newest weakest edge is refused");
    }

    #[test]
    fn cap_prunes_multiple_until_under_limit_and_recalculates_degree() {
        let conn = setup_cap_db();
        for id in 1..=6 {
            insert_entity(&conn, id);
        }
        // Hub entity 1 with five incident edges of ascending weight.
        insert_edge(&conn, 100, 1, 2, 0.1);
        insert_edge(&conn, 101, 1, 3, 0.2);
        insert_edge(&conn, 102, 1, 4, 0.3);
        insert_edge(&conn, 103, 1, 5, 0.4);
        insert_edge(&conn, 104, 1, 6, 0.5);
        // Seed stale degree values to prove recalculation runs.
        conn.execute("UPDATE entities SET degree = 99", params![])
            .unwrap();

        let pruned = enforce_degree_cap(&conn, 1, 2).unwrap();

        assert_eq!(pruned, 3);
        assert_eq!(degree_of(&conn, 1), 2);
        // The two strongest edges survive.
        assert!(edge_exists(&conn, 103));
        assert!(edge_exists(&conn, 104));
        // Affected endpoint degree was recalculated, not left stale.
        let endpoint_degree: i64 = conn
            .query_row(
                "SELECT degree FROM entities WHERE id = ?1",
                params![2],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(endpoint_degree, 0, "endpoint 2 lost its only edge");
    }
}
