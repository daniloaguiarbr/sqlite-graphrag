use crate::cli::MemoryType;
use crate::errors::AppError;
use crate::graph::traverse_from_memories;
use crate::output::{self, OutputFormat, RecallItem, RecallResponse};
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use crate::storage::entities;
use crate::storage::memories;

#[derive(clap::Args)]
pub struct RecallArgs {
    pub query: String,
    #[arg(short = 'k', long, default_value = "10")]
    pub k: usize,
    #[arg(long, value_enum)]
    pub r#type: Option<MemoryType>,
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long)]
    pub no_graph: bool,
    #[arg(long)]
    pub precise: bool,
    #[arg(long, default_value = "2")]
    pub max_hops: u32,
    #[arg(long, default_value = "0.3")]
    pub min_weight: f64,
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
    #[arg(long, env = "NEUROGRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

pub fn run(args: RecallArgs) -> Result<(), AppError> {
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    output::emit_progress("Computing query embedding...");
    let embedder = crate::embedder::get_embedder(&paths.models)?;
    let embedding = crate::embedder::embed_query(embedder, &args.query)?;

    let conn = open_ro(&paths.db)?;

    let memory_type_str = args.r#type.map(|t| t.as_str());
    let knn_results = memories::knn_search(&conn, &embedding, &namespace, memory_type_str, args.k)?;

    let mut direct_matches = Vec::new();
    let mut memory_ids: Vec<i64> = Vec::new();
    for (memory_id, distance) in knn_results {
        let row = {
            let mut stmt = conn.prepare_cached(
                "SELECT id, namespace, name, type, description, body, body_hash,
                        session_id, source, metadata, created_at, updated_at
                 FROM memories WHERE id=?1 AND deleted_at IS NULL",
            )?;
            stmt.query_row(rusqlite::params![memory_id], |r| {
                Ok(memories::MemoryRow {
                    id: r.get(0)?,
                    namespace: r.get(1)?,
                    name: r.get(2)?,
                    memory_type: r.get(3)?,
                    description: r.get(4)?,
                    body: r.get(5)?,
                    body_hash: r.get(6)?,
                    session_id: r.get(7)?,
                    source: r.get(8)?,
                    metadata: r.get(9)?,
                    created_at: r.get(10)?,
                    updated_at: r.get(11)?,
                })
            })
            .ok()
        };
        if let Some(row) = row {
            let snippet: String = row.body.chars().take(300).collect();
            direct_matches.push(RecallItem {
                memory_id: row.id,
                name: row.name,
                namespace: row.namespace,
                memory_type: row.memory_type,
                description: row.description,
                snippet,
                distance,
                source: "direct".to_string(),
            });
            memory_ids.push(memory_id);
        }
    }

    let mut graph_matches = Vec::new();
    if !args.no_graph {
        let entity_knn = entities::knn_search(&conn, &embedding, &namespace, 5)?;
        let entity_ids: Vec<i64> = entity_knn.iter().map(|(id, _)| *id).collect();

        let all_seed_ids: Vec<i64> = memory_ids
            .iter()
            .chain(entity_ids.iter())
            .copied()
            .collect();

        if !all_seed_ids.is_empty() {
            let graph_memory_ids = traverse_from_memories(
                &conn,
                &all_seed_ids,
                &namespace,
                args.min_weight,
                args.max_hops,
            )?;

            for graph_mem_id in graph_memory_ids {
                let row = {
                    let mut stmt = conn.prepare_cached(
                        "SELECT id, namespace, name, type, description, body, body_hash,
                                session_id, source, metadata, created_at, updated_at
                         FROM memories WHERE id=?1 AND deleted_at IS NULL",
                    )?;
                    stmt.query_row(rusqlite::params![graph_mem_id], |r| {
                        Ok(memories::MemoryRow {
                            id: r.get(0)?,
                            namespace: r.get(1)?,
                            name: r.get(2)?,
                            memory_type: r.get(3)?,
                            description: r.get(4)?,
                            body: r.get(5)?,
                            body_hash: r.get(6)?,
                            session_id: r.get(7)?,
                            source: r.get(8)?,
                            metadata: r.get(9)?,
                            created_at: r.get(10)?,
                            updated_at: r.get(11)?,
                        })
                    })
                    .ok()
                };
                if let Some(row) = row {
                    let snippet: String = row.body.chars().take(300).collect();
                    graph_matches.push(RecallItem {
                        memory_id: row.id,
                        name: row.name,
                        namespace: row.namespace,
                        memory_type: row.memory_type,
                        description: row.description,
                        snippet,
                        distance: 0.0,
                        source: "graph".to_string(),
                    });
                }
            }
        }
    }

    output::emit_json(&RecallResponse {
        query: args.query,
        direct_matches,
        graph_matches,
    })?;

    Ok(())
}
