use crate::cli::MemoryType;
use crate::errors::AppError;
use crate::output::{self, OutputFormat, RecallItem};
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use crate::storage::memories;

use std::collections::HashMap;

#[derive(clap::Args)]
pub struct HybridSearchArgs {
    pub query: String,
    #[arg(short = 'k', long, default_value = "10")]
    pub k: usize,
    #[arg(long, default_value = "60")]
    pub rrf_k: u32,
    #[arg(long, default_value = "1.0")]
    pub weight_vec: f32,
    #[arg(long, default_value = "1.0")]
    pub weight_fts: f32,
    #[arg(long, value_enum)]
    pub r#type: Option<MemoryType>,
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long)]
    pub with_graph: bool,
    #[arg(long, default_value = "2")]
    pub max_hops: u32,
    #[arg(long, default_value = "0.3")]
    pub min_weight: f64,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
    #[arg(long, env = "NEUROGRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(serde::Serialize)]
pub struct HybridSearchItem {
    pub memory_id: i64,
    pub name: String,
    pub namespace: String,
    #[serde(rename = "type")]
    pub memory_type: String,
    pub description: String,
    pub body: String,
    pub combined_score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vec_rank: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fts_rank: Option<usize>,
}

#[derive(serde::Serialize)]
pub struct HybridSearchResponse {
    pub query: String,
    pub k: usize,
    pub results: Vec<HybridSearchItem>,
    pub graph_matches: Vec<RecallItem>,
}

pub fn run(args: HybridSearchArgs) -> Result<(), AppError> {
    match args.format {
        OutputFormat::Text | OutputFormat::Markdown => {
            return Err(AppError::Validation(
                "formato text/markdown ainda não implementado para hybrid-search — use --format json ou aguarde Tier 2".into(),
            ));
        }
        OutputFormat::Json => {}
    }

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    output::emit_progress("Calculando embedding da consulta...");
    let embedder = crate::embedder::get_embedder(&paths.models)?;
    let embedding = crate::embedder::embed_query(embedder, &args.query)?;

    let conn = open_ro(&paths.db)?;

    let memory_type_str = args.r#type.map(|t| t.as_str());

    let vec_results =
        memories::knn_search(&conn, &embedding, &namespace, memory_type_str, args.k * 2)?;

    // Mapear posição de ranking vetorial por memory_id
    let vec_rank_map: HashMap<i64, usize> = vec_results
        .iter()
        .enumerate()
        .map(|(pos, (id, _))| (*id, pos))
        .collect();

    let fts_results =
        memories::fts_search(&conn, &args.query, &namespace, memory_type_str, args.k * 2)?;

    // Mapear posição de ranking FTS por memory_id
    let fts_rank_map: HashMap<i64, usize> = fts_results
        .iter()
        .enumerate()
        .map(|(pos, row)| (row.id, pos))
        .collect();

    let rrf_k = args.rrf_k as f64;

    // Acumular scores RRF combinados
    let mut combined_scores: HashMap<i64, f64> = HashMap::new();

    for (rank, (memory_id, _)) in vec_results.iter().enumerate() {
        let score = args.weight_vec as f64 * (1.0 / (rrf_k + rank as f64 + 1.0));
        *combined_scores.entry(*memory_id).or_insert(0.0) += score;
    }

    for (rank, row) in fts_results.iter().enumerate() {
        let score = args.weight_fts as f64 * (1.0 / (rrf_k + rank as f64 + 1.0));
        *combined_scores.entry(row.id).or_insert(0.0) += score;
    }

    // Ordenar por score descendente e tomar os top-k
    let mut ranked: Vec<(i64, f64)> = combined_scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(args.k);

    // Coletar todos os IDs para busca em batch (evitar N+1)
    let top_ids: Vec<i64> = ranked.iter().map(|(id, _)| *id).collect();

    // Buscar dados completos das memórias top
    let mut memory_data: HashMap<i64, memories::MemoryRow> = HashMap::new();
    for id in &top_ids {
        if let Some(row) = memories::read_full(&conn, *id)? {
            memory_data.insert(*id, row);
        }
    }

    // Construir resultados finais na ordem de ranking
    let results: Vec<HybridSearchItem> = ranked
        .into_iter()
        .filter_map(|(memory_id, combined_score)| {
            memory_data.remove(&memory_id).map(|row| HybridSearchItem {
                memory_id: row.id,
                name: row.name,
                namespace: row.namespace,
                memory_type: row.memory_type,
                description: row.description,
                body: row.body,
                combined_score,
                vec_rank: vec_rank_map.get(&memory_id).copied(),
                fts_rank: fts_rank_map.get(&memory_id).copied(),
            })
        })
        .collect();

    output::emit_json(&HybridSearchResponse {
        query: args.query,
        k: args.k,
        results,
        graph_matches: vec![],
    })?;

    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn hybrid_search_response_vazia_serializa_campos_corretos() {
        let resp = HybridSearchResponse {
            query: "busca teste".to_string(),
            k: 10,
            results: vec![],
            graph_matches: vec![],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"results\""), "deve conter campo results");
        assert!(json.contains("\"query\""), "deve conter campo query");
        assert!(json.contains("\"k\""), "deve conter campo k");
        assert!(
            json.contains("\"graph_matches\""),
            "deve conter campo graph_matches"
        );
        assert!(
            !json.contains("\"combined_rank\""),
            "NÃO deve conter combined_rank"
        );
        assert!(
            !json.contains("\"vec_rank_list\""),
            "NÃO deve conter vec_rank_list"
        );
        assert!(
            !json.contains("\"fts_rank_list\""),
            "NÃO deve conter fts_rank_list"
        );
    }

    #[test]
    fn hybrid_search_item_omite_fts_rank_quando_none() {
        let item = HybridSearchItem {
            memory_id: 1,
            name: "mem".to_string(),
            namespace: "default".to_string(),
            memory_type: "user".to_string(),
            description: "desc".to_string(),
            body: "conteúdo".to_string(),
            combined_score: 0.0328,
            vec_rank: Some(0),
            fts_rank: None,
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(
            json.contains("\"vec_rank\""),
            "deve conter vec_rank quando Some"
        );
        assert!(
            !json.contains("\"fts_rank\""),
            "NÃO deve conter fts_rank quando None"
        );
    }

    #[test]
    fn hybrid_search_item_omite_vec_rank_quando_none() {
        let item = HybridSearchItem {
            memory_id: 2,
            name: "mem2".to_string(),
            namespace: "default".to_string(),
            memory_type: "fact".to_string(),
            description: "desc2".to_string(),
            body: "corpo2".to_string(),
            combined_score: 0.016,
            vec_rank: None,
            fts_rank: Some(1),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(
            !json.contains("\"vec_rank\""),
            "NÃO deve conter vec_rank quando None"
        );
        assert!(
            json.contains("\"fts_rank\""),
            "deve conter fts_rank quando Some"
        );
    }

    #[test]
    fn hybrid_search_item_serializa_ambos_ranks_quando_some() {
        let item = HybridSearchItem {
            memory_id: 3,
            name: "mem3".to_string(),
            namespace: "ns".to_string(),
            memory_type: "entity".to_string(),
            description: "desc3".to_string(),
            body: "corpo3".to_string(),
            combined_score: 0.05,
            vec_rank: Some(2),
            fts_rank: Some(0),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"vec_rank\""), "deve conter vec_rank");
        assert!(json.contains("\"fts_rank\""), "deve conter fts_rank");
        assert!(json.contains("\"type\""), "deve serializar type renomeado");
        assert!(!json.contains("memory_type"), "NÃO deve expor memory_type");
    }

    #[test]
    fn hybrid_search_response_serializa_k_corretamente() {
        let resp = HybridSearchResponse {
            query: "q".to_string(),
            k: 5,
            results: vec![],
            graph_matches: vec![],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"k\":5"), "deve serializar k=5");
    }
}
