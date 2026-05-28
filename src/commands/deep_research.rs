//! Handler for the `deep-research` CLI subcommand.
//!
//! Orchestrates parallel multi-hop GraphRAG search via query decomposition.
//! The workload is I/O-bound (SQLite WAL reads), so tokio is used instead of
//! rayon. Each sub-query opens its own read-only connection.

use crate::errors::AppError;
use crate::graph::traverse_from_memories_with_hops;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use crate::storage::{entities, memories};

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// Arguments for the `deep-research` subcommand.
#[derive(clap::Args)]
#[command(
    about = "Deep parallel multi-hop GraphRAG research via query decomposition",
    after_long_help = "EXAMPLES:\n  \
        # Basic deep research\n  \
        sqlite-graphrag deep-research \"auth architecture decisions\"\n\n  \
        # With custom parameters\n  \
        sqlite-graphrag deep-research \"auth\" --k 20 --max-hops 3 --max-sub-queries 7\n\n  \
        # Include full memory bodies in output\n  \
        sqlite-graphrag deep-research \"auth\" --with-bodies"
)]
pub struct DeepResearchArgs {
    /// Research query to decompose and search.
    #[arg(value_name = "QUERY", help = "Research query to decompose and search")]
    pub query: String,
    /// Results per sub-query (Recall@20 captures 95%+ relevant hits).
    #[arg(
        long,
        short,
        default_value_t = 20,
        help = "Results per sub-query (Recall@20 captures 95%+ relevant hits)"
    )]
    pub k: usize,
    /// Maximum sub-queries from decomposition (covers complex multi-hop queries).
    #[arg(
        long,
        default_value_t = 7,
        help = "Maximum sub-queries (covers complex multi-hop queries)"
    )]
    pub max_sub_queries: usize,
    /// Multi-hop graph traversal depth (sweet spot: 2-3 hops).
    #[arg(
        long,
        default_value_t = 3,
        help = "Multi-hop graph traversal depth (sweet spot: 2-3 hops)"
    )]
    pub max_hops: usize,
    /// Minimum edge weight for graph traversal.
    #[arg(
        long,
        default_value_t = 0.3,
        help = "Minimum edge weight for graph traversal"
    )]
    pub min_weight: f64,
    /// Maximum concurrent sub-queries (default: min(cpus, 8)).
    #[arg(long, help = "Maximum concurrent sub-queries (default: min(cpus, 8))")]
    pub max_concurrency: Option<usize>,
    /// Timeout per sub-query in seconds.
    #[arg(long, default_value_t = 30, help = "Timeout per sub-query in seconds")]
    pub timeout: u64,
    /// Include full memory bodies in results.
    #[arg(
        long,
        default_value_t = false,
        help = "Include full memory bodies in results"
    )]
    pub with_bodies: bool,
    /// Maximum results after deduplication.
    #[arg(
        long,
        default_value_t = 50,
        help = "Maximum results after deduplication"
    )]
    pub max_results: usize,
    /// Namespace (env: SQLITE_GRAPHRAG_NAMESPACE, default: global).
    #[arg(
        long,
        help = "Namespace (env: SQLITE_GRAPHRAG_NAMESPACE, default: global)"
    )]
    pub namespace: Option<String>,
    /// JSON output (always on, kept for consistency).
    #[arg(long, hide = true)]
    pub json: bool,
    /// Database path.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    #[command(flatten)]
    pub daemon: crate::cli::DaemonOpts,
}

#[derive(Serialize)]
struct SubQuery {
    id: usize,
    text: String,
    source: &'static str,
}

#[derive(Serialize)]
struct DeepResult {
    name: String,
    score: f64,
    source: String,
    sub_query_ids: Vec<usize>,
    snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    hop_distance: Option<usize>,
}

#[derive(Serialize)]
struct EvidenceChain {
    path: Vec<EvidenceNode>,
    depth: usize,
    sub_query_ids: Vec<usize>,
}

#[derive(Serialize)]
struct EvidenceNode {
    entity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    relation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    weight: Option<f64>,
}

#[derive(Serialize)]
struct ResearchStats {
    sub_queries_total: usize,
    sub_queries_completed: usize,
    sub_queries_failed: usize,
    sub_queries_timed_out: usize,
    unique_memories_found: usize,
    evidence_chains_found: usize,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct DeepResearchResponse {
    query: String,
    sub_queries: Vec<SubQuery>,
    results: Vec<DeepResult>,
    evidence_chains: Vec<EvidenceChain>,
    stats: ResearchStats,
}

/// Aggregated hit data: (score, source_label, snippet, body, hop_distance, sub_query_ids).
type MergedHit = (f64, String, String, String, Option<usize>, Vec<usize>);

/// Intermediate result from a single sub-query execution.
struct SubQueryResult {
    sub_query_id: usize,
    /// (memory_id, score, source_label, snippet, body, hop_distance)
    hits: Vec<(i64, f64, String, String, String, Option<usize>)>,
    /// Evidence chain nodes discovered via graph traversal.
    evidence: Vec<EvidenceNode>,
}

/// Sync entry point — builds a tokio runtime for the async fan-out.
pub fn run(args: DeepResearchArgs) -> Result<(), AppError> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to build tokio runtime: {e}")))?;
    rt.block_on(run_async(args))
}

/// Main async logic: decompose, fan-out, assemble, emit JSON.
async fn run_async(args: DeepResearchArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();

    if args.query.trim().is_empty() {
        return Err(AppError::Validation(crate::i18n::validation::empty_query()));
    }

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    crate::storage::connection::ensure_db_ready(&paths)?;

    // Phase 1: Query decomposition (sync, pure logic).
    let sub_query_texts = decompose_query(&args.query, args.max_sub_queries);
    let sub_queries: Vec<SubQuery> = sub_query_texts
        .iter()
        .enumerate()
        .map(|(i, text)| SubQuery {
            id: i,
            text: text.clone(),
            source: if sub_query_texts.len() == 1 {
                "original"
            } else {
                "decomposed"
            },
        })
        .collect();

    // Compute embedding ONCE, share via Arc.
    output::emit_progress_i18n(
        "Computing query embedding...",
        "Calculando embedding da consulta...",
    );
    let embedding = Arc::new(crate::daemon::embed_query_or_local(
        &paths.models,
        &args.query,
        args.daemon.autostart_daemon,
    )?);

    // Phase 2: Fan-out — parallel sub-query execution.
    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let permits = args
        .max_concurrency
        .unwrap_or_else(|| cpu_count.min(8))
        .min(sub_queries.len())
        .max(1);
    let semaphore = Arc::new(Semaphore::new(permits));
    let timeout_dur = std::time::Duration::from_secs(args.timeout);

    let mut join_set: JoinSet<Result<SubQueryResult, (usize, String)>> = JoinSet::new();

    for (idx, sq_text) in sub_query_texts.iter().enumerate() {
        let sem = Arc::clone(&semaphore);
        let emb = Arc::clone(&embedding);
        let ns = namespace.clone();
        let db_path = paths.db.clone();
        let query_text = sq_text.clone();
        let k = args.k;
        let max_hops = args.max_hops;
        let min_weight = args.min_weight;

        join_set.spawn(async move {
            let _permit = sem
                .acquire_owned()
                .await
                .map_err(|e| (idx, format!("semaphore closed: {e}")))?;

            let result = tokio::time::timeout(timeout_dur, async {
                execute_sub_query(
                    idx,
                    &query_text,
                    &emb,
                    &ns,
                    &db_path,
                    k,
                    max_hops,
                    min_weight,
                )
            })
            .await;

            match result {
                Ok(inner) => inner.map_err(|e| (idx, e)),
                Err(_) => Err((idx, "timeout".to_string())),
            }
        });
    }

    // Collect results incrementally.
    let mut sub_query_results: Vec<SubQueryResult> = Vec::with_capacity(sub_queries.len());
    let mut failed_count = 0usize;
    let mut timed_out_count = 0usize;

    while let Some(join_result) = join_set.join_next().await {
        match join_result {
            Ok(Ok(sqr)) => sub_query_results.push(sqr),
            Ok(Err((_idx, reason))) => {
                if reason == "timeout" {
                    timed_out_count += 1;
                } else {
                    failed_count += 1;
                }
                tracing::warn!(sub_query_id = _idx, reason = %reason, "sub-query failed");
            }
            Err(join_err) => {
                failed_count += 1;
                if join_err.is_panic() {
                    tracing::error!("sub-query task panicked: {join_err}");
                } else {
                    tracing::warn!("sub-query task cancelled: {join_err}");
                }
            }
        }
    }

    // Phase 3: Evidence assembly — merge, dedup, rank.
    // Aggregate hits: memory_id -> (best_score, source, snippet, body, hop_distance, sub_query_ids)
    let mut merged: HashMap<i64, MergedHit> = HashMap::new();

    for sqr in &sub_query_results {
        for (mem_id, score, source, snippet, body, hop) in &sqr.hits {
            let entry = merged.entry(*mem_id).or_insert_with(|| {
                (
                    *score,
                    source.clone(),
                    snippet.clone(),
                    body.clone(),
                    *hop,
                    Vec::new(),
                )
            });
            // Keep best score.
            if *score > entry.0 {
                entry.0 = *score;
                entry.1 = source.clone();
                entry.2 = snippet.clone();
                entry.3 = body.clone();
                entry.4 = *hop;
            }
            if !entry.5.contains(&sqr.sub_query_id) {
                entry.5.push(sqr.sub_query_id);
            }
        }
    }

    // Resolve memory names for merged results.
    let conn = open_ro(&paths.db)?;
    let mut results: Vec<DeepResult> = Vec::with_capacity(merged.len().min(args.max_results));

    // Sort by score descending.
    let mut ranked: Vec<(i64, MergedHit)> = merged.into_iter().collect();
    ranked.sort_by(|a, b| {
        b.1 .0
            .partial_cmp(&a.1 .0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ranked.truncate(args.max_results);

    for (mem_id, (score, source, snippet, body, hop, sq_ids)) in ranked {
        let name = match memories::read_full(&conn, mem_id)? {
            Some(row) => row.name,
            None => continue,
        };
        results.push(DeepResult {
            name,
            score,
            source,
            sub_query_ids: sq_ids,
            snippet,
            body: if args.with_bodies { Some(body) } else { None },
            hop_distance: hop,
        });
    }

    // Build evidence chains from graph traversal data.
    let mut evidence_chains: Vec<EvidenceChain> = Vec::new();
    let mut seen_chain_keys: HashSet<String> = HashSet::new();

    for sqr in &sub_query_results {
        if sqr.evidence.is_empty() {
            continue;
        }
        // Deduplicate chains by concatenating entity names.
        let key: String = sqr
            .evidence
            .iter()
            .map(|n| n.entity.as_str())
            .collect::<Vec<_>>()
            .join("->");
        if seen_chain_keys.insert(key) {
            evidence_chains.push(EvidenceChain {
                depth: sqr.evidence.len(),
                path: sqr
                    .evidence
                    .iter()
                    .map(|n| EvidenceNode {
                        entity: n.entity.clone(),
                        relation: n.relation.clone(),
                        weight: n.weight,
                    })
                    .collect(),
                sub_query_ids: vec![sqr.sub_query_id],
            });
        }
    }

    let unique_memories = results.len();
    let evidence_count = evidence_chains.len();

    // Phase 4: JSON output.
    output::emit_json(&DeepResearchResponse {
        query: args.query,
        sub_queries,
        results,
        evidence_chains,
        stats: ResearchStats {
            sub_queries_total: sub_query_texts.len(),
            sub_queries_completed: sub_query_results.len(),
            sub_queries_failed: failed_count,
            sub_queries_timed_out: timed_out_count,
            unique_memories_found: unique_memories,
            evidence_chains_found: evidence_count,
            elapsed_ms: start.elapsed().as_millis() as u64,
        },
    })?;

    Ok(())
}

/// Heuristic query decomposition: splits by conjunctions, commas, semicolons,
/// relational phrases, and extracts explicit entities (kebab-case or quoted).
fn decompose_query(query: &str, max: usize) -> Vec<String> {
    if query.is_empty() {
        return vec![query.to_string()];
    }

    let mut parts: Vec<String> = Vec::new();

    // Split by relational phrases first (most specific).
    let relational = [
        " that caused ",
        " depending on ",
        " related to ",
        " connected to ",
        " linked to ",
        " caused by ",
        " followed by ",
    ];
    let mut text = query.to_string();
    let mut did_relational_split = false;
    for phrase in &relational {
        if text.to_lowercase().contains(phrase) {
            let lower = text.to_lowercase();
            if let Some(pos) = lower.find(phrase) {
                let left = text[..pos].trim().to_string();
                let right = text[pos + phrase.len()..].trim().to_string();
                if !left.is_empty() {
                    parts.push(left);
                }
                if !right.is_empty() {
                    text = right;
                }
                did_relational_split = true;
            }
        }
    }
    if did_relational_split && !text.is_empty() {
        parts.push(text.clone());
    }

    // If no relational split, try conjunctions and delimiters.
    if parts.is_empty() {
        // Split by semicolons first.
        let semi_parts: Vec<&str> = query.split(';').collect();
        if semi_parts.len() > 1 {
            for p in &semi_parts {
                let trimmed = p.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
        } else {
            // Split by commas and conjunctions.
            // Replace " and " and " e " (Portuguese) with comma, then split.
            let normalized = query
                .replace(" and ", ", ")
                .replace(" AND ", ", ")
                .replace(" e ", ", ")
                .replace(" E ", ", ");
            let comma_parts: Vec<&str> = normalized.split(',').collect();
            if comma_parts.len() > 1 {
                for p in &comma_parts {
                    let trimmed = p.trim();
                    if !trimmed.is_empty() {
                        parts.push(trimmed.to_string());
                    }
                }
            }
        }
    }

    // If still no split, return original as single sub-query.
    if parts.is_empty() {
        return vec![query.to_string()];
    }

    // Cap at max.
    parts.truncate(max);
    parts
}

/// Execute a single sub-query: hybrid search + KNN + graph traversal.
///
/// Runs synchronously on a blocking thread (called from a tokio spawn context).
/// Each call opens its own read-only SQLite connection to leverage WAL concurrency.
#[allow(clippy::too_many_arguments)]
fn execute_sub_query(
    sub_query_id: usize,
    query_text: &str,
    embedding: &[f32],
    namespace: &str,
    db_path: &std::path::Path,
    k: usize,
    max_hops: usize,
    min_weight: f64,
) -> Result<SubQueryResult, String> {
    let conn = open_ro(db_path).map_err(|e| format!("failed to open db: {e}"))?;

    let mut hits: Vec<(i64, f64, String, String, String, Option<usize>)> =
        Vec::with_capacity(k * 2);
    let mut seen_ids: HashSet<i64> = HashSet::new();

    // 1. KNN vector search.
    let knn_results = memories::knn_search(&conn, embedding, &[namespace.to_string()], None, k)
        .map_err(|e| format!("knn_search failed: {e}"))?;

    for (memory_id, distance) in &knn_results {
        if seen_ids.insert(*memory_id) {
            let score = 1.0 - (*distance as f64);
            let score = score.clamp(0.0, 1.0);
            if let Ok(Some(row)) = memories::read_full(&conn, *memory_id) {
                let snippet: String = row.body.chars().take(300).collect();
                hits.push((
                    *memory_id,
                    score,
                    "knn".to_string(),
                    snippet,
                    row.body,
                    None,
                ));
            }
        }
    }

    // 2. FTS5 search (best-effort; if it fails, continue with KNN results only).
    match memories::fts_search(&conn, query_text, namespace, None, k) {
        Ok(fts_rows) => {
            for row in fts_rows {
                if seen_ids.insert(row.id) {
                    // FTS results lack a distance metric; assign a moderate score.
                    let snippet: String = row.body.chars().take(300).collect();
                    hits.push((row.id, 0.5, "fts".to_string(), snippet, row.body, None));
                }
            }
        }
        Err(e) => {
            tracing::warn!(
                sub_query_id,
                "FTS5 search failed, continuing with KNN only: {e}"
            );
        }
    }

    // 3. Graph traversal from discovered memories.
    let mut evidence: Vec<EvidenceNode> = Vec::new();
    let memory_ids: Vec<i64> = hits.iter().map(|(id, ..)| *id).collect();

    if !memory_ids.is_empty() && max_hops > 0 {
        // Also search entities via KNN for graph seeds.
        let entity_knn = entities::knn_search(&conn, embedding, namespace, 5).unwrap_or_default();
        let entity_ids: Vec<i64> = entity_knn.iter().map(|(id, _)| *id).collect();

        let all_seed_ids: Vec<i64> = memory_ids
            .iter()
            .chain(entity_ids.iter())
            .copied()
            .collect();

        if let Ok(graph_results) = traverse_from_memories_with_hops(
            &conn,
            &all_seed_ids,
            namespace,
            min_weight,
            max_hops as u32,
        ) {
            for (graph_mem_id, hop) in graph_results {
                if seen_ids.insert(graph_mem_id) {
                    let graph_distance = 1.0 - 1.0 / (hop as f64 + 1.0);
                    let score = 1.0 - graph_distance;
                    if let Ok(Some(row)) = memories::read_full(&conn, graph_mem_id) {
                        let snippet: String = row.body.chars().take(300).collect();
                        hits.push((
                            graph_mem_id,
                            score,
                            "graph".to_string(),
                            snippet,
                            row.body,
                            Some(hop as usize),
                        ));
                    }
                }
            }
        }

        // Build evidence chain from entity relationships.
        let entity_sql = "\
            SELECT se.name, te.name, r.relation, r.weight
            FROM relationships r
            JOIN entities se ON se.id = r.source_id
            JOIN entities te ON te.id = r.target_id
            WHERE r.namespace = ?1 AND r.weight >= ?2
            ORDER BY r.weight DESC
            LIMIT 20";
        if let Ok(mut stmt) = conn.prepare(entity_sql) {
            if let Ok(rows) = stmt.query_map(rusqlite::params![namespace, min_weight], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, f64>(3)?,
                ))
            }) {
                for row in rows.flatten() {
                    evidence.push(EvidenceNode {
                        entity: row.0,
                        relation: Some(row.2),
                        weight: Some(row.3),
                    });
                    // Add the target entity too.
                    evidence.push(EvidenceNode {
                        entity: row.1,
                        relation: None,
                        weight: None,
                    });
                }
            }
        }
    }

    Ok(SubQueryResult {
        sub_query_id,
        hits,
        evidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decompose_and_conjunction() {
        let result = decompose_query("A and B", 7);
        assert_eq!(result, vec!["A", "B"]);
    }

    #[test]
    fn test_decompose_no_split() {
        let result = decompose_query("simple query", 7);
        assert_eq!(result, vec!["simple query"]);
    }

    #[test]
    fn test_decompose_three_parts() {
        let result = decompose_query("A, B and C", 7);
        assert_eq!(result, vec!["A", "B", "C"]);
    }

    #[test]
    fn test_decompose_portuguese_conjunctions() {
        let result = decompose_query("A e B", 7);
        assert_eq!(result, vec!["A", "B"]);
    }

    #[test]
    fn test_decompose_max_cap() {
        let parts: Vec<String> = (0..10).map(|i| format!("part{i}")).collect();
        let query = parts.join(", ");
        let result = decompose_query(&query, 7);
        assert!(
            result.len() <= 7,
            "expected at most 7 sub-queries, got {}",
            result.len()
        );
    }

    #[test]
    fn test_decompose_empty_preserves_original() {
        let result = decompose_query("", 7);
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn test_decompose_semicolons() {
        let result = decompose_query("auth design; deployment config; logging", 7);
        assert_eq!(result, vec!["auth design", "deployment config", "logging"]);
    }

    #[test]
    fn test_decompose_relational_phrase() {
        let result = decompose_query("auth that caused deployment failure", 7);
        assert_eq!(result, vec!["auth", "deployment failure"]);
    }

    #[test]
    fn test_sub_query_serialization() {
        let sq = SubQuery {
            id: 0,
            text: "test query".to_string(),
            source: "original",
        };
        let json = serde_json::to_value(&sq).expect("serialization failed");
        assert_eq!(json["id"], 0);
        assert_eq!(json["text"], "test query");
        assert_eq!(json["source"], "original");
    }

    #[test]
    fn test_deep_result_omits_body_when_none() {
        let result = DeepResult {
            name: "test".to_string(),
            score: 0.9,
            source: "knn".to_string(),
            sub_query_ids: vec![0],
            snippet: "snippet".to_string(),
            body: None,
            hop_distance: None,
        };
        let json = serde_json::to_string(&result).expect("serialization failed");
        assert!(!json.contains("\"body\""), "body must be omitted when None");
    }

    #[test]
    fn test_deep_result_includes_body_when_some() {
        let result = DeepResult {
            name: "test".to_string(),
            score: 0.9,
            source: "knn".to_string(),
            sub_query_ids: vec![0, 1],
            snippet: "snippet".to_string(),
            body: Some("full body content".to_string()),
            hop_distance: Some(2),
        };
        let json = serde_json::to_string(&result).expect("serialization failed");
        assert!(json.contains("\"body\""), "body must be present when Some");
        assert!(json.contains("full body content"));
    }

    #[test]
    fn test_evidence_node_omits_none_fields() {
        let node = EvidenceNode {
            entity: "auth-module".to_string(),
            relation: None,
            weight: None,
        };
        let json = serde_json::to_string(&node).expect("serialization failed");
        assert!(
            !json.contains("\"relation\""),
            "relation must be omitted when None"
        );
        assert!(
            !json.contains("\"weight\""),
            "weight must be omitted when None"
        );
    }

    #[test]
    fn test_research_stats_serialization() {
        let stats = ResearchStats {
            sub_queries_total: 3,
            sub_queries_completed: 2,
            sub_queries_failed: 1,
            sub_queries_timed_out: 0,
            unique_memories_found: 10,
            evidence_chains_found: 2,
            elapsed_ms: 1234,
        };
        let json = serde_json::to_value(&stats).expect("serialization failed");
        assert_eq!(json["sub_queries_total"], 3);
        assert_eq!(json["sub_queries_completed"], 2);
        assert_eq!(json["sub_queries_failed"], 1);
        assert_eq!(json["elapsed_ms"], 1234);
    }

    #[test]
    fn test_deep_research_response_serialization() {
        let resp = DeepResearchResponse {
            query: "test query".to_string(),
            sub_queries: vec![SubQuery {
                id: 0,
                text: "test query".to_string(),
                source: "original",
            }],
            results: vec![],
            evidence_chains: vec![],
            stats: ResearchStats {
                sub_queries_total: 1,
                sub_queries_completed: 1,
                sub_queries_failed: 0,
                sub_queries_timed_out: 0,
                unique_memories_found: 0,
                evidence_chains_found: 0,
                elapsed_ms: 42,
            },
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["query"], "test query");
        assert!(json["sub_queries"].is_array());
        assert!(json["results"].is_array());
        assert!(json["evidence_chains"].is_array());
        assert_eq!(json["stats"]["elapsed_ms"], 42);
    }
}
