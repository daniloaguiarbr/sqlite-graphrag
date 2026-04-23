use crate::cli::MemoryType;
use crate::errors::AppError;
use crate::graph::traverse_from_memories;
use crate::i18n::erros;
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
    /// Filtrar resultados por distance máxima. Se todos os matches tiverem distance > min_distance,
    /// comando sai com exit 4 (not found) conforme contrato documentado em AGENT_PROTOCOL.md.
    /// Default 1.0 (desativado, mantém comportamento v2.0.0 de sempre retornar top-k).
    #[arg(long, default_value = "1.0")]
    pub min_distance: f32,
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// Aceita --json como no-op: output já é JSON por default.
    #[arg(long, hide = true)]
    pub json: bool,
}

pub fn run(args: RecallArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    output::emit_progress_i18n(
        "Computing query embedding...",
        "Calculando embedding da consulta...",
    );
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

    // Filtrar por min_distance se < 1.0 (ativado). Se nenhum hit dentro do threshold, exit 4.
    if args.min_distance < 1.0 {
        let has_relevant = direct_matches
            .iter()
            .any(|item| item.distance <= args.min_distance);
        if !has_relevant {
            return Err(AppError::NotFound(erros::sem_resultados_recall(
                args.min_distance,
                &args.query,
                &namespace,
            )));
        }
    }

    let results: Vec<RecallItem> = direct_matches
        .iter()
        .cloned()
        .chain(graph_matches.iter().cloned())
        .collect();

    output::emit_json(&RecallResponse {
        query: args.query,
        k: args.k,
        direct_matches,
        graph_matches,
        results,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod testes {
    use crate::output::{RecallItem, RecallResponse};

    fn make_item(name: &str, distance: f32, source: &str) -> RecallItem {
        RecallItem {
            memory_id: 1,
            name: name.to_string(),
            namespace: "global".to_string(),
            memory_type: "fact".to_string(),
            description: "desc".to_string(),
            snippet: "snippet".to_string(),
            distance,
            source: source.to_string(),
        }
    }

    #[test]
    fn recall_response_serializa_campos_obrigatorios() {
        let resp = RecallResponse {
            query: "rust memory".to_string(),
            k: 5,
            direct_matches: vec![make_item("mem-a", 0.12, "direct")],
            graph_matches: vec![],
            results: vec![make_item("mem-a", 0.12, "direct")],
            elapsed_ms: 42,
        };

        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["query"], "rust memory");
        assert_eq!(json["k"], 5);
        assert_eq!(json["elapsed_ms"], 42u64);
        assert!(json["direct_matches"].is_array());
        assert!(json["graph_matches"].is_array());
        assert!(json["results"].is_array());
    }

    #[test]
    fn recall_item_serializa_type_renomeado() {
        let item = make_item("mem-teste", 0.25, "direct");
        let json = serde_json::to_value(&item).expect("serialização falhou");

        // O campo memory_type é renomeado para "type" no JSON
        assert_eq!(json["type"], "fact");
        assert_eq!(json["distance"], 0.25f32);
        assert_eq!(json["source"], "direct");
    }

    #[test]
    fn recall_response_results_contem_direct_e_graph() {
        let direct = make_item("d-mem", 0.10, "direct");
        let graph = make_item("g-mem", 0.0, "graph");

        let resp = RecallResponse {
            query: "query".to_string(),
            k: 10,
            direct_matches: vec![direct.clone()],
            graph_matches: vec![graph.clone()],
            results: vec![direct, graph],
            elapsed_ms: 10,
        };

        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["direct_matches"].as_array().unwrap().len(), 1);
        assert_eq!(json["graph_matches"].as_array().unwrap().len(), 1);
        assert_eq!(json["results"].as_array().unwrap().len(), 2);
        assert_eq!(json["results"][0]["source"], "direct");
        assert_eq!(json["results"][1]["source"], "graph");
    }

    #[test]
    fn recall_response_vazio_serializa_arrays_vazios() {
        let resp = RecallResponse {
            query: "nada".to_string(),
            k: 3,
            direct_matches: vec![],
            graph_matches: vec![],
            results: vec![],
            elapsed_ms: 1,
        };

        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["direct_matches"].as_array().unwrap().len(), 0);
        assert_eq!(json["results"].as_array().unwrap().len(), 0);
    }
}
