//! Handler for the `hybrid-search` CLI subcommand.

use crate::cli::MemoryType;
use crate::errors::AppError;
use crate::graph::traverse_from_memories_with_hops;
use crate::output::{self, JsonOutputFormat, RecallItem};
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use crate::storage::entities;
use crate::storage::memories;

use std::collections::HashMap;

/// Arguments for the `hybrid-search` subcommand.
///
/// When `--namespace` is omitted the search runs against the `global` namespace,
/// which is the default namespace used by `remember` when no `--namespace` flag
/// is provided. Pass an explicit `--namespace` value to search a different
/// isolated namespace.
#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Basic hybrid search combining FTS5 + vector via RRF\n  \
    sqlite-graphrag hybrid-search \"postgres migration deadlock\" --k 10\n\n  \
    # Tune RRF weights to favor keyword matches over semantic similarity\n  \
    sqlite-graphrag hybrid-search \"jwt auth\" --weight-fts 1.5 --weight-vec 0.5 --k 5\n\n  \
    # Add graph traversal matches (entities connected to top results)\n  \
    sqlite-graphrag hybrid-search \"frontend architecture\" --with-graph --k 10\n\n  \
    # Graph traversal with custom depth and minimum edge weight\n  \
    sqlite-graphrag hybrid-search \"auth design\" --with-graph --max-hops 3 --min-weight 0.5 --k 10\n\n  \
NOTES:\n  \
    --with-graph enables entity graph traversal seeded by the top RRF results.\n  \
    Graph matches appear in the `graph_matches` array (separate from `results`).\n  \
    Without --with-graph, `graph_matches` is always empty.")]
pub struct HybridSearchArgs {
    #[arg(
        allow_hyphen_values = true,
        help = "Hybrid search query (vector KNN + FTS5 BM25 fused via RRF)"
    )]
    pub query: String,
    /// Maximum number of fused results to return after RRF combines vector + FTS5 candidates.
    ///
    /// Validated to the inclusive range `1..=4096` (the upper bound matches `sqlite-vec`'s knn
    /// limit). Each underlying search fetches `k * 2` candidates before fusion.
    #[arg(short = 'k', long, aliases = ["limit", "top-k"], default_value = "10", value_parser = crate::parsers::parse_k_range)]
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
    /// G58 (v1.0.80): skip the live query embedding and serve FTS5 BM25 only.
    /// Useful in CI/CD with tight OAuth quota and in deterministic tests.
    #[arg(long, help = "Skip live query embedding; serve FTS5 BM25 only")]
    pub fallback_fts_only: bool,
    /// Graph traversal depth (requires --with-graph; default 2 when active).
    #[arg(long)]
    pub max_hops: Option<u32>,
    /// Minimum edge weight for graph traversal (requires --with-graph; default 0.3 when active).
    #[arg(long)]
    pub min_weight: Option<f64>,
    #[arg(long, value_enum, default_value_t = JsonOutputFormat::Json)]
    pub format: JsonOutputFormat,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// Accept `--json` as a no-op because output is already JSON by default.
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
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
    pub snippet: String,
    pub combined_score: f64,
    /// Alias of `combined_score` for the documented contract in SKILL.md.
    pub score: f64,
    /// Source of the match: always "hybrid" (RRF of vec + fts). Added in v2.0.1.
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vec_rank: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fts_rank: Option<usize>,
    /// Combined RRF score — explicit alias of `combined_score` for integration contracts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rrf_score: Option<f64>,
    /// RRF score normalized to [0.0, 1.0] for cross-method comparability.
    pub normalized_score: f64,
    /// Raw KNN distance from the vector index (lower = more similar).
    ///
    /// Present when the result came from the vector search path; `None` when the
    /// result appeared only in the FTS5 results and was not ranked by the KNN index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vec_distance: Option<f64>,
    /// Raw BM25 score from the FTS5 index. Currently always `None`; reserved for
    /// a future release when the FTS5 BM25 score is exposed by the storage layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fts_bm25: Option<f64>,
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
    /// Weights applied to vec and fts sources in the RRF fusion.
    pub weights: Weights,
    pub results: Vec<HybridSearchItem>,
    pub graph_matches: Vec<RecallItem>,
    /// True when FTS5 failed and the response is vec-only.
    ///
    /// Omitted from JSON when `false` to keep the happy-path envelope clean.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub fts_degraded: bool,
    /// Human-readable description of the FTS5 failure when `fts_degraded` is true.
    ///
    /// Omitted from JSON when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fts_error: Option<String>,
    /// True when the FTS5 index was corrupted and successfully auto-rebuilt during this request.
    ///
    /// Omitted from JSON when `false` to keep the happy-path envelope clean.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub fts_auto_rebuilt: bool,
    /// G58 (v1.0.80): symmetric to `fts_degraded`; `true` when the live query
    /// embedding failed and the response degraded to FTS5-only. Absent on the
    /// wire when false.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub vec_degraded: bool,
    /// G58 (v1.0.80): human-readable description of the embedding failure
    /// that triggered the fallback. Absent on the wire when `vec_degraded` is
    /// false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vec_error: Option<String>,
    /// G58 (v1.0.80): advisory warning echoed for callers that branch on
    /// top-level status. Distinguishes a FTS5-only fallback from a clean
    /// hybrid response so downstream pipelines can lower their confidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    /// Total execution time in milliseconds from handler start to serialisation.
    pub elapsed_ms: u64,
}

#[tracing::instrument(skip_all, level = "debug", name = "hybrid_search")]
pub fn run(args: HybridSearchArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let _ = args.format;
    tracing::debug!(target: "hybrid_search", query = %args.query, k = args.k, "fusing results");

    // G20: reject graph-specific flags when --with-graph is not active
    // G48: Option<T> detects an explicitly provided flag even when the value
    // equals the old default (pre-fix, `--max-hops 2` was silently accepted).
    if !args.with_graph {
        if args.max_hops.is_some() {
            return Err(AppError::Validation(
                "--max-hops requires --with-graph to be active".to_string(),
            ));
        }
        if args.min_weight.is_some() {
            return Err(AppError::Validation(
                "--min-weight requires --with-graph to be active".to_string(),
            ));
        }
    }

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    crate::storage::connection::ensure_db_ready(&paths)?;

    output::emit_progress_i18n(
        "Computing query embedding...",
        "Calculando embedding da consulta...",
    );
    let conn = open_ro(&paths.db)?;
    // G58 (v1.0.80): when the live embedding fails (OAuth contention, rate
    // limit, timeout, missing CLI), skip the KNN half of the RRF and serve
    // FTS5-only results. The RRF degenerates to a pure BM25 ranking and the
    // envelope surfaces `vec_degraded` + `vec_error` + `warning`.
    let (embedding, vec_degraded, vec_error) = if args.fallback_fts_only {
        (None, true, Some("fallback_fts_only requested".to_string()))
    } else {
        match crate::embedder::try_embed_query_with_fallback(&paths.models, &args.query) {
            Ok(v) => (Some(v), false, None),
            Err(reason) => {
                let msg = reason.to_string();
                tracing::warn!(target: "hybrid_search", fallback_reason = %msg, "live embedding failed; falling back to FTS5");
                (None, true, Some(msg))
            }
        }
    };

    let memory_type_str = args.r#type.map(|t| t.as_str());

    let vec_results: Vec<(i64, f32)> = if let Some(emb) = embedding.as_ref() {
        memories::knn_search(
            &conn,
            emb,
            std::slice::from_ref(&namespace),
            memory_type_str,
            args.k * 2,
        )?
    } else {
        Vec::new()
    };

    // Map vector ranking position by memory_id (1-indexed per schema)
    let vec_rank_map: HashMap<i64, usize> = vec_results
        .iter()
        .enumerate()
        .map(|(pos, (id, _))| (*id, pos + 1))
        .collect();

    // Map raw KNN distance by memory_id for GAP-30: vec_distance field.
    let vec_distance_map: HashMap<i64, f64> = vec_results
        .iter()
        .map(|(id, dist)| (*id, *dist as f64))
        .collect();

    let (fts_results, fts_degraded, fts_error, fts_auto_rebuilt) = if args.weight_fts == 0.0 {
        (vec![], false, None, false)
    } else {
        match memories::fts_search(&conn, &args.query, &namespace, memory_type_str, args.k * 2) {
            Ok(r) => (r, false, None, false),
            Err(e) => {
                let err_msg = e.to_string();
                let is_malformed = err_msg.contains("malformed") || err_msg.contains("corrupt");
                if is_malformed {
                    tracing::warn!(target: "hybrid_search", "FTS5 index corrupted, attempting auto-rebuild");
                    if conn
                        .execute_batch("INSERT INTO fts_memories(fts_memories) VALUES('rebuild');")
                        .is_ok()
                    {
                        match memories::fts_search(
                            &conn,
                            &args.query,
                            &namespace,
                            memory_type_str,
                            args.k * 2,
                        ) {
                            Ok(r) => (r, false, None, true),
                            Err(e2) => {
                                tracing::error!(target: "hybrid_search", error = %e2, "FTS5 auto-rebuild failed to recover");
                                (vec![], true, Some(e2.to_string()), true)
                            }
                        }
                    } else {
                        (vec![], true, Some(err_msg), false)
                    }
                } else {
                    tracing::warn!(target: "hybrid_search", error = %e, "FTS5 query failed, falling back to vec-only");
                    (vec![], true, Some(err_msg), false)
                }
            }
        }
    };

    // Map FTS ranking position by memory_id (1-indexed per schema)
    let fts_rank_map: HashMap<i64, usize> = fts_results
        .iter()
        .enumerate()
        .map(|(pos, row)| (row.id, pos + 1))
        .collect();

    let rrf_k = args.rrf_k as f64;

    // Accumulate combined RRF scores
    let mut combined_scores: crate::hash::AHashMap<i64, f64> =
        crate::hash::AHashMap::with_capacity_and_hasher(
            vec_results.len() + fts_results.len(),
            Default::default(),
        );

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
    let mut memory_data: crate::hash::AHashMap<i64, memories::MemoryRow> =
        crate::hash::AHashMap::with_capacity_and_hasher(ranked.len(), Default::default());
    for id in &top_ids {
        if let Some(row) = memories::read_full(&conn, *id)? {
            memory_data.insert(*id, row);
        }
    }

    let max_possible = args.weight_vec as f64 * (1.0 / (rrf_k + 1.0))
        + args.weight_fts as f64 * (1.0 / (rrf_k + 1.0));

    // Build final results in ranking order
    let results: Vec<HybridSearchItem> = ranked
        .into_iter()
        .filter_map(|(memory_id, combined_score)| {
            let normalized_score = if max_possible > 0.0 {
                combined_score / max_possible
            } else {
                0.0
            };
            memory_data.remove(&memory_id).map(|row| {
                let snippet: String = row.body.chars().take(300).collect();
                HybridSearchItem {
                    memory_id: row.id,
                    name: row.name,
                    namespace: row.namespace,
                    memory_type: row.memory_type,
                    description: row.description,
                    body: row.body,
                    snippet,
                    combined_score,
                    score: combined_score,
                    source: "hybrid".to_string(),
                    vec_rank: vec_rank_map.get(&memory_id).copied(),
                    fts_rank: fts_rank_map.get(&memory_id).copied(),
                    rrf_score: Some(combined_score),
                    normalized_score,
                    vec_distance: vec_distance_map.get(&memory_id).copied(),
                    fts_bm25: None,
                }
            })
        })
        .collect();

    // --- Graph traversal (activated by --with-graph) ---
    let mut graph_matches: Vec<RecallItem> = Vec::with_capacity(8);
        if let Some(emb) = args.with_graph.then_some(()).filter(|_| !results.is_empty()).and(embedding.as_ref()) {
        let namespace_for_graph = namespace.clone();
        let memory_ids: Vec<i64> = results.iter().map(|r| r.memory_id).collect();

        let entity_knn =
            entities::knn_search(&conn, emb, &namespace_for_graph, 5)?;
        let entity_ids: Vec<i64> = entity_knn.iter().map(|(id, _)| *id).collect();

        let all_seed_ids: Vec<i64> = memory_ids
            .iter()
            .chain(entity_ids.iter())
            .copied()
            .collect();

        if !all_seed_ids.is_empty() {
            let graph_memory_ids = traverse_from_memories_with_hops(
                &conn,
                &all_seed_ids,
                &namespace_for_graph,
                args.min_weight.unwrap_or(0.3),
                args.max_hops.unwrap_or(2),
            )?;

            let already_in_results: std::collections::HashSet<i64> =
                results.iter().map(|r| r.memory_id).collect();

            for (graph_mem_id, hop) in graph_memory_ids {
                if already_in_results.contains(&graph_mem_id) {
                    continue;
                }
                if let Some(row) = memories::read_full(&conn, graph_mem_id)? {
                    let snippet: String = row.body.chars().take(300).collect();
                    let graph_distance = 1.0 - 1.0 / (hop as f32 + 1.0);
                    graph_matches.push(RecallItem {
                        memory_id: row.id,
                        name: row.name,
                        namespace: row.namespace,
                        memory_type: row.memory_type,
                        description: row.description,
                        snippet,
                        distance: graph_distance,
                        score: RecallItem::score_from_distance(graph_distance),
                        source: "graph".to_string(),
                        graph_depth: Some(hop),
                    });
                }
            }
        }
    }

    output::emit_json(&HybridSearchResponse {
        query: args.query,
        k: args.k,
        rrf_k: args.rrf_k,
        weights: Weights {
            vec: args.weight_vec,
            fts: args.weight_fts,
        },
        results,
        graph_matches,
        fts_degraded,
        fts_error,
        fts_auto_rebuilt,
        vec_degraded,
        vec_error,
        warning: if vec_degraded {
            Some(
                "live query embedding unavailable; results are FTS5 BM25 only (semantic relevance reduced)"
                    .to_string(),
            )
        } else {
            None
        },
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(clap::Parser)]
    struct TestCli {
        #[command(flatten)]
        args: HybridSearchArgs,
    }

    #[test]
    fn graph_flags_parse_as_none_when_absent() {
        // G48: with plain u32/f64 defaults, an explicit `--max-hops 2` was
        // indistinguishable from the default and silently bypassed the G20
        // validation. Option<T> restores real flag-presence detection.
        use clap::Parser;
        let cli = TestCli::try_parse_from(["hybrid-search", "q"]).expect("bare query parses");
        assert!(cli.args.max_hops.is_none());
        assert!(cli.args.min_weight.is_none());
        let cli = TestCli::try_parse_from(["hybrid-search", "q", "--max-hops", "2"])
            .expect("explicit flag parses");
        assert_eq!(cli.args.max_hops, Some(2));
    }

    fn empty_response(
        k: usize,
        rrf_k: u32,
        weight_vec: f32,
        weight_fts: f32,
    ) -> HybridSearchResponse {
        HybridSearchResponse {
            query: "test query".to_string(),
            k,
            rrf_k,
            weights: Weights {
                vec: weight_vec,
                fts: weight_fts,
            },
            results: vec![],
            graph_matches: vec![],
            fts_degraded: false,
            fts_error: None,
            fts_auto_rebuilt: false,
            vec_degraded: false,
            vec_error: None,
            warning: None,
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
            snippet: "content".to_string(),
            combined_score: 0.0328,
            score: 0.0328,
            source: "hybrid".to_string(),
            vec_rank: Some(1),
            fts_rank: None,
            rrf_score: Some(0.0328),
            normalized_score: 1.0,
            vec_distance: Some(0.12),
            fts_bm25: None,
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
            snippet: "corpo2".to_string(),
            combined_score: 0.016,
            score: 0.016,
            source: "hybrid".to_string(),
            vec_rank: None,
            fts_rank: Some(2),
            rrf_score: Some(0.016),
            normalized_score: 0.5,
            vec_distance: None,
            fts_bm25: None,
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
            snippet: "corpo3".to_string(),
            combined_score: 0.05,
            score: 0.05,
            source: "hybrid".to_string(),
            vec_rank: Some(3),
            fts_rank: Some(1),
            rrf_score: Some(0.05),
            normalized_score: 0.8,
            vec_distance: Some(0.25),
            fts_bm25: None,
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

    #[test]
    fn hybrid_search_response_with_graph_matches() {
        use crate::output::RecallItem;
        let resp = HybridSearchResponse {
            query: "test".to_string(),
            k: 5,
            rrf_k: 60,
            weights: Weights { vec: 1.0, fts: 1.0 },
            results: vec![],
            graph_matches: vec![RecallItem {
                memory_id: 1,
                name: "graph-hit".to_string(),
                namespace: "global".to_string(),
                memory_type: "document".to_string(),
                description: "found via graph".to_string(),
                snippet: "graph content".to_string(),
                distance: 0.1,
                score: 0.9,
                source: "graph".to_string(),
                graph_depth: Some(1),
            }],
            fts_degraded: false,
            fts_error: None,
            fts_auto_rebuilt: false,
            vec_degraded: false,
            vec_error: None,
            warning: None,
            elapsed_ms: 42,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["graph_matches"].as_array().unwrap().len(), 1);
        assert_eq!(json["graph_matches"][0]["source"], "graph");
        assert_eq!(json["graph_matches"][0]["graph_depth"], 1);
    }

    #[test]
    fn fts_degraded_omitted_on_success_present_on_failure() {
        // Happy path: fts_degraded=false must be absent from JSON (skip_serializing_if).
        let ok_resp = empty_response(5, 60, 1.0, 1.0);
        let ok_json = serde_json::to_string(&ok_resp).unwrap();
        assert!(
            !ok_json.contains("\"fts_degraded\""),
            "fts_degraded must be absent when false"
        );
        assert!(
            !ok_json.contains("\"fts_error\""),
            "fts_error must be absent when None"
        );

        // Degraded path: fts_degraded=true and fts_error=Some must appear in JSON.
        let mut degraded_resp = empty_response(5, 60, 1.0, 1.0);
        degraded_resp.fts_degraded = true;
        degraded_resp.fts_error = Some("FTS5 table corrupted".to_string());
        let degraded_json = serde_json::to_string(&degraded_resp).unwrap();
        assert!(
            degraded_json.contains("\"fts_degraded\":true"),
            "fts_degraded must be present and true when degraded"
        );
        assert!(
            degraded_json.contains("\"fts_error\""),
            "fts_error must be present when Some"
        );
        assert!(
            degraded_json.contains("FTS5 table corrupted"),
            "fts_error must contain the error message"
        );
    }
}
