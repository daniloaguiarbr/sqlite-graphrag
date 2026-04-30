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
/// Propaga [`AppError::Database`] (exit 10) em falhas de consulta SQLite.
///
/// # Examples
///
/// ```
/// use rusqlite::Connection;
/// use sqlite_graphrag::graph::traverse_from_memories;
///
/// // Lista de sementes vazia retorna imediatamente sem consultar o banco.
/// let conn = Connection::open_in_memory().unwrap();
/// let ids = traverse_from_memories(&conn, &[], "global", 0.5, 3).unwrap();
/// assert!(ids.is_empty());
/// ```
///
/// ```
/// use rusqlite::Connection;
/// use sqlite_graphrag::graph::traverse_from_memories;
///
/// // max_hops == 0 retorna imediatamente sem traversal.
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
    let mut seed_entities: Vec<i64> = Vec::new();
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
    let mut visited: HashSet<i64> = seed_entities.iter().cloned().collect();
    let mut frontier = seed_entities.clone();

    for _ in 0..max_hops {
        if frontier.is_empty() {
            break;
        }
        let mut next_frontier = Vec::new();

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
    let seed_set: HashSet<i64> = seed_memory_ids.iter().cloned().collect();
    let graph_only_entities: Vec<i64> = visited
        .into_iter()
        .filter(|id| !seed_entities.contains(id))
        .collect();

    let mut result_ids: Vec<i64> = Vec::new();
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
/// # Errors
///
/// Propaga [`AppError::Database`] (exit 10) em falhas de consulta SQLite.
pub fn traverse_from_memories_with_hops(
    conn: &Connection,
    seed_memory_ids: &[i64],
    namespace: &str,
    min_weight: f64,
    max_hops: u32,
) -> Result<Vec<(i64, u32)>, AppError> {
    if seed_memory_ids.is_empty() || max_hops == 0 {
        return Ok(vec![]);
    }

    // Collect seed entity IDs from seed memories
    let mut seed_entities: Vec<i64> = Vec::new();
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
    let mut frontier = seed_entities.clone();

    for hop in 1..=max_hops {
        if frontier.is_empty() {
            break;
        }
        let mut next_frontier = Vec::new();

        for &entity_id in &frontier {
            let mut stmt = conn.prepare_cached(
                "SELECT target_id FROM relationships
                 WHERE source_id = ?1 AND weight >= ?2 AND namespace = ?3",
            )?;
            let neighbors: Vec<i64> = stmt
                .query_map(params![entity_id, min_weight, namespace], |r| r.get(0))?
                .filter_map(|r| r.ok())
                .filter(|id| !entity_depth.contains_key(id))
                .collect();

            for id in neighbors {
                entity_depth.insert(id, hop);
                next_frontier.push(id);
            }
        }
        frontier = next_frontier;
    }

    // Find memories connected to traversed entities (excluding seeds), preserving hop depth
    let seed_set: std::collections::HashSet<i64> = seed_memory_ids.iter().cloned().collect();
    let seed_entity_set: std::collections::HashSet<i64> = seed_entities.iter().cloned().collect();

    let mut result: Vec<(i64, u32)> = Vec::new();
    let mut seen_memories: std::collections::HashSet<i64> = std::collections::HashSet::new();

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
        let resultado = traverse_from_memories(&conn, &[], "ns", 0.5, 3).unwrap();
        assert!(resultado.is_empty());
    }

    #[test]
    fn returns_empty_when_max_hops_zero() {
        let conn = setup_db();
        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);
        let resultado = traverse_from_memories(&conn, &[1], "ns", 0.5, 0).unwrap();
        assert!(resultado.is_empty());
    }

    #[test]
    fn returns_empty_when_seed_has_no_entities() {
        let conn = setup_db();
        insert_memory(&conn, 1, "ns", false);
        // memoria existe mas não tem entidades associadas
        let resultado = traverse_from_memories(&conn, &[1], "ns", 0.5, 3).unwrap();
        assert!(resultado.is_empty());
    }

    #[test]
    fn returns_empty_when_no_relationships() {
        let conn = setup_db();
        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);
        // entidade 10 não tem relacionamentos
        let resultado = traverse_from_memories(&conn, &[1], "ns", 0.5, 3).unwrap();
        assert!(resultado.is_empty());
    }

    // --- happy path básico ---

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

        let resultado = traverse_from_memories(&conn, &[1], "ns", 0.5, 1).unwrap();
        assert_eq!(resultado, vec![2]);
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

        let mut resultado = traverse_from_memories(&conn, &[1], "ns", 0.5, 2).unwrap();
        resultado.sort_unstable();
        assert_eq!(resultado, vec![2, 3]);
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

        // com apenas 1 hop, memory 3 não deve aparecer
        let resultado = traverse_from_memories(&conn, &[1], "ns", 0.5, 1).unwrap();
        assert_eq!(resultado, vec![2]);
        assert!(!resultado.contains(&3));
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

        let resultado = traverse_from_memories(&conn, &[1], "ns", 0.5, 3).unwrap();
        assert!(resultado.is_empty());
    }

    #[test]
    fn relationship_with_weight_exactly_at_min_included() {
        let conn = setup_db();

        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);

        insert_memory(&conn, 2, "ns", false);
        link_memory_entity(&conn, 2, 20);

        insert_relationship(&conn, 10, 20, 0.5, "ns");

        let resultado = traverse_from_memories(&conn, &[1], "ns", 0.5, 1).unwrap();
        assert_eq!(resultado, vec![2]);
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

        let resultado = traverse_from_memories(&conn, &[1], "ns_a", 0.5, 3).unwrap();
        assert!(resultado.is_empty());
    }

    // --- excluir seeds do resultado ---

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

        let resultado = traverse_from_memories(&conn, &[1], "ns", 0.5, 3).unwrap();
        // memory 1 não deve aparecer mesmo com ciclo
        assert!(!resultado.contains(&1));
        assert_eq!(resultado, vec![2]);
    }

    // --- memórias soft-deleted excluídas ---

    #[test]
    fn deleted_memories_not_included() {
        let conn = setup_db();

        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);

        // memory 2 foi deletada
        insert_memory(&conn, 2, "ns", true);
        link_memory_entity(&conn, 2, 20);

        insert_relationship(&conn, 10, 20, 1.0, "ns");

        let resultado = traverse_from_memories(&conn, &[1], "ns", 0.5, 3).unwrap();
        assert!(resultado.is_empty());
    }

    // --- múltiplos seeds ---

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

        let mut resultado = traverse_from_memories(&conn, &[1, 2], "ns", 0.5, 1).unwrap();
        resultado.sort_unstable();
        assert_eq!(resultado, vec![3, 4]);
    }

    // --- deduplicação de resultado ---

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

        let resultado = traverse_from_memories(&conn, &[1], "ns", 0.5, 1).unwrap();
        // memory 2 deve aparecer apenas uma vez
        assert_eq!(resultado.len(), 1);
        assert_eq!(resultado, vec![2]);
    }

    // --- nó único (single node) ---

    #[test]
    fn single_node_without_neighbors_returns_empty() {
        let conn = setup_db();

        insert_memory(&conn, 1, "ns", false);
        link_memory_entity(&conn, 1, 10);
        // entity 10 não tem relacionamentos de saída

        let resultado = traverse_from_memories(&conn, &[1], "ns", 0.5, 5).unwrap();
        assert!(resultado.is_empty());
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

        // triângulo 10 -> 20 -> 30 -> 10
        insert_relationship(&conn, 10, 20, 1.0, "ns");
        insert_relationship(&conn, 20, 30, 1.0, "ns");
        insert_relationship(&conn, 30, 10, 1.0, "ns");

        let mut resultado = traverse_from_memories(&conn, &[1], "ns", 0.5, 10).unwrap();
        resultado.sort_unstable();
        // deve retornar 2 e 3 sem loop infinito
        assert_eq!(resultado, vec![2, 3]);
    }
}
