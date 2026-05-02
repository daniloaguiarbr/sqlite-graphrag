//! Handler for the `hybrid-search` CLI subcommand.

use crate::cli::MemoryType;
use crate::errors::AppError;
use crate::output::{self, JsonOutputFormat, RecallItem};
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use crate::storage::memories;

use std::collections::HashMap;

/// Arguments for the `hybrid-search` subcommand.
///
/// When `--namespace` is omitted the search runs against the `global` namespace,
/// which is the default namespace used by `remember` when no `--namespace` flag
/// is provided. Pass an explicit `--namespace` value to search a different
/// isolated namespace.
#[derive(clap::Args)]
pub struct HybridSearchArgs {
    #[arg(help = "Hybrid search query (vector KNN + FTS5 BM25 fused via RRF)")]
    pub query: String,
    /// Maximum number of fused results to return after RRF combines vector + FTS5 candidates.
    ///
    /// Validated to the inclusive range `1..=4096` (the upper bound matches `sqlite-vec`'s knn
    /// limit). Each underlying search fetches `k * 2` candidates before fusion.
    #[arg(short = 'k', long, alias = "limit", default_value = "10", value_parser = crate::parsers::parse_k_range)]
    pub k: usize,
    #[arg(long, default_value = "60")]
    pub rrf_k: u32,
    #[arg(long, default_value = "1.0")]
    pub weight_vec: f32,
    #[arg(long, default_value = "1.0")]
    pub weight_fts: f32,
    /// Filter by memory.type. Note: distinct from graph entity_type
    /// (project/tool/person/file/concept/incident/decision/memory/dashboard/issue_tracker/organization/location/date)
    /// used in --entities-file.
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
    #[arg(long, value_enum, default_value_t = JsonOutputFormat::Json)]
    pub format: JsonOutputFormat,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// Accept `--json` as a no-op because output is already JSON by default.
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[command(flatten)]
    pub daemon: crate::cli::DaemonOpts,
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
    /// Alias de `combined_score` para contrato documentado em SKILL.md.
    pub score: f64,
    /// Fonte do match: sempre "hybrid" (RRF de vec + fts). Adicionado em v2.0.1.
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vec_rank: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fts_rank: Option<usize>,
    /// Combined RRF score — explicit alias of `combined_score` for integration contracts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rrf_score: Option<f64>,
}

/// RRF weights used in hybrid search: vec (vector) and fts (text).
#[derive(serde::Serialize)]
pub struct Weights {
    pub vec: f32,
    pub fts: f32,
}

#[derive(serde::Serialize)]
pub struct HybridSearchResponse {
    pub query: String,
    pub k: usize,
    /// RRF k parameter used in the combined ranking.
    pub rrf_k: u32,
    /// Pesos aplicados às fontes vec e fts no RRF.
    pub weights: Weights,
    pub results: Vec<HybridSearchItem>,
    pub graph_matches: Vec<RecallItem>,
    /// Total execution time in milliseconds from handler start to serialisation.
    pub elapsed_ms: u64,
}

pub fn run(args: HybridSearchArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let _ = args.format;

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    crate::storage::connection::ensure_db_ready(&paths)?;

    output::emit_progress_i18n(
        "Computing query embedding...",
        "Calculando embedding da consulta...",
    );
    let embedding = crate::daemon::embed_query_or_local(
        &paths.models,
        &args.query,
        args.daemon.autostart_daemon,
    )?;

    let conn = open_ro(&paths.db)?;

    let memory_type_str = args.r#type.map(|t| t.as_str());

    let vec_results = memories::knn_search(
        &conn,
        &embedding,
        &[namespace.clone()],
        memory_type_str,
        args.k * 2,
    )?;

    // Map vector ranking position by memory_id (1-indexed per schema)
    let vec_rank_map: HashMap<i64, usize> = vec_results
        .iter()
        .enumerate()
        .map(|(pos, (id, _))| (*id, pos + 1))
        .collect();

    let fts_results =
        memories::fts_search(&conn, &args.query, &namespace, memory_type_str, args.k * 2)?;

    // Map FTS ranking position by memory_id (1-indexed per schema)
    let fts_rank_map: HashMap<i64, usize> = fts_results
        .iter()
        .enumerate()
        .map(|(pos, row)| (row.id, pos + 1))
        .collect();

    let rrf_k = args.rrf_k as f64;

    // Accumulate combined RRF scores
    let mut combined_scores: HashMap<i64, f64> = HashMap::new();

    for (rank, (memory_id, _)) in vec_results.iter().enumerate() {
        let score = args.weight_vec as f64 * (1.0 / (rrf_k + rank as f64 + 1.0));
        *combined_scores.entry(*memory_id).or_insert(0.0) += score;
    }

    for (rank, row) in fts_results.iter().enumerate() {
        let score = args.weight_fts as f64 * (1.0 / (rrf_k + rank as f64 + 1.0));
        *combined_scores.entry(row.id).or_insert(0.0) += score;
    }

    // Sort by score descending and take the top-k
    let mut ranked: Vec<(i64, f64)> = combined_scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(args.k);

    // Collect all IDs for batch fetch (avoiding N+1)
    let top_ids: Vec<i64> = ranked.iter().map(|(id, _)| *id).collect();

    // Fetch full data for the top memories
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
                score: combined_score,
                source: "hybrid".to_string(),
                vec_rank: vec_rank_map.get(&memory_id).copied(),
                fts_rank: fts_rank_map.get(&memory_id).copied(),
                rrf_score: Some(combined_score),
            })
        })
        .collect();

    output::emit_json(&HybridSearchResponse {
        query: args.query,
        k: args.k,
        rrf_k: args.rrf_k,
        weights: Weights {
            vec: args.weight_vec,
            fts: args.weight_fts,
        },
        results,
        graph_matches: vec![],
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_response(
        k: usize,
        rrf_k: u32,
        weight_vec: f32,
        weight_fts: f32,
    ) -> HybridSearchResponse {
        HybridSearchResponse {
            query: "busca teste".to_string(),
            k,
            rrf_k,
            weights: Weights {
                vec: weight_vec,
                fts: weight_fts,
            },
            results: vec![],
            graph_matches: vec![],
            elapsed_ms: 0,
        }
    }

    #[test]
    fn hybrid_search_response_empty_serializes_correct_fields() {
        let resp = empty_response(10, 60, 1.0, 1.0);
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"results\""), "must contain results field");
        assert!(json.contains("\"query\""), "must contain query field");
        assert!(json.contains("\"k\""), "must contain k field");
        assert!(
            json.contains("\"graph_matches\""),
            "must contain graph_matches field"
        );
        assert!(
            !json.contains("\"combined_rank\""),
            "must not contain combined_rank"
        );
        assert!(
            !json.contains("\"vec_rank_list\""),
            "must not contain vec_rank_list"
        );
        assert!(
            !json.contains("\"fts_rank_list\""),
            "must not contain fts_rank_list"
        );
    }

    #[test]
    fn hybrid_search_response_serializes_rrf_k_and_weights() {
        let resp = empty_response(5, 60, 0.7, 0.3);
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"rrf_k\""), "must contain rrf_k field");
        assert!(json.contains("\"weights\""), "must contain weights field");
        assert!(json.contains("\"vec\""), "must contain weights.vec field");
        assert!(json.contains("\"fts\""), "must contain weights.fts field");
    }

    #[test]
    fn hybrid_search_response_serializes_elapsed_ms() {
        let mut resp = empty_response(5, 60, 1.0, 1.0);
        resp.elapsed_ms = 123;
        let json = serde_json::to_string(&resp).unwrap();
        assert!(
            json.contains("\"elapsed_ms\""),
            "must contain elapsed_ms field"
        );
        assert!(json.contains("123"), "deve serializar valor de elapsed_ms");
    }

    #[test]
    fn weights_struct_serializes_correctly() {
        let w = Weights { vec: 0.6, fts: 0.4 };
        let json = serde_json::to_string(&w).unwrap();
        assert!(json.contains("\"vec\""));
        assert!(json.contains("\"fts\""));
    }

    #[test]
    fn hybrid_search_item_omits_fts_rank_when_none() {
        let item = HybridSearchItem {
            memory_id: 1,
            name: "mem".to_string(),
            namespace: "default".to_string(),
            memory_type: "user".to_string(),
            description: "desc".to_string(),
            body: "content".to_string(),
            combined_score: 0.0328,
            score: 0.0328,
            source: "hybrid".to_string(),
            vec_rank: Some(1),
            fts_rank: None,
            rrf_score: Some(0.0328),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(
            json.contains("\"vec_rank\""),
            "must contain vec_rank when Some"
        );
        assert!(
            !json.contains("\"fts_rank\""),
            "must not contain fts_rank when None"
        );
    }

    #[test]
    fn hybrid_search_item_omits_vec_rank_when_none() {
        let item = HybridSearchItem {
            memory_id: 2,
            name: "mem2".to_string(),
            namespace: "default".to_string(),
            memory_type: "fact".to_string(),
            description: "desc2".to_string(),
            body: "corpo2".to_string(),
            combined_score: 0.016,
            score: 0.016,
            source: "hybrid".to_string(),
            vec_rank: None,
            fts_rank: Some(2),
            rrf_score: Some(0.016),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(
            !json.contains("\"vec_rank\""),
            "must not contain vec_rank when None"
        );
        assert!(
            json.contains("\"fts_rank\""),
            "must contain fts_rank when Some"
        );
    }

    #[test]
    fn hybrid_search_item_serializes_both_ranks_when_some() {
        let item = HybridSearchItem {
            memory_id: 3,
            name: "mem3".to_string(),
            namespace: "ns".to_string(),
            memory_type: "entity".to_string(),
            description: "desc3".to_string(),
            body: "corpo3".to_string(),
            combined_score: 0.05,
            score: 0.05,
            source: "hybrid".to_string(),
            vec_rank: Some(3),
            fts_rank: Some(1),
            rrf_score: Some(0.05),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"vec_rank\""), "must contain vec_rank");
        assert!(json.contains("\"fts_rank\""), "must contain fts_rank");
        assert!(json.contains("\"type\""), "deve serializar type renomeado");
        assert!(!json.contains("memory_type"), "must not expose memory_type");
    }

    #[test]
    fn hybrid_search_response_serializes_k_correctly() {
        let resp = empty_response(5, 60, 1.0, 1.0);
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"k\":5"), "deve serializar k=5");
    }
}
