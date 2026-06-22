//! Handler for the `deep-research` CLI subcommand.
//!
//! Orchestrates parallel multi-hop GraphRAG search via query decomposition.
//! The workload is I/O-bound (SQLite WAL reads), so tokio is used instead of
//! rayon. Each sub-query opens its own read-only connection.

use crate::errors::AppError;
use crate::graph::{
    bfs_with_predecessors, traverse_from_memories_with_hops_capped, PredecessorMap,
};
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use crate::storage::fusion::{rrf_fuse, rrf_max_possible};
use crate::storage::{entities, memories};

use serde::Serialize;
use std::collections::HashSet;
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
        sqlite-graphrag deep-research \"auth\" --with-bodies\n\n  \
        # Tune RRF and graph scoring\n  \
        sqlite-graphrag deep-research \"auth and deployment\" --rrf-k 60 --graph-decay 0.7"
)]
pub struct DeepResearchArgs {
    /// Research query to decompose and search.
    #[arg(
        value_name = "QUERY",
        allow_hyphen_values = true,
        help = "Research query to decompose and search"
    )]
    pub query: String,
    /// Results per sub-query (Recall@20 captures 95%+ relevant hits).
    #[arg(
        long,
        short,
        aliases = ["limit", "top-k"],
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
    /// RRF k parameter controlling score smoothing (higher = less weight on top ranks).
    #[arg(
        long,
        default_value_t = 60.0,
        help = "RRF k parameter (higher = less weight on top ranks)"
    )]
    pub rrf_k: f64,
    /// Decay factor applied to graph scores per hop (score = seed_score * decay^hop).
    #[arg(
        long,
        default_value_t = 0.7,
        help = "Graph score decay factor per hop (0.0-1.0)"
    )]
    pub graph_decay: f64,
    /// Minimum score threshold for graph-expanded results (filters noise).
    #[arg(
        long,
        default_value_t = 0.05,
        help = "Minimum score threshold for graph-expanded results"
    )]
    pub graph_min_score: f64,
    /// Limit top-k neighbours followed per entity per hop (None = unlimited).
    #[arg(
        long,
        help = "Limit neighbours per entity per hop for graph traversal (default: unlimited)"
    )]
    pub max_neighbors_per_hop: Option<usize>,
    /// Namespace (env: SQLITE_GRAPHRAG_NAMESPACE, default: global).
    #[arg(
        long,
        help = "Namespace (env: SQLITE_GRAPHRAG_NAMESPACE, default: global)"
    )]
    pub namespace: Option<String>,
    /// Research mode: `none` (local heuristic, default), `claude-code`, `codex` (v1.1.0).
    #[arg(long, default_value = "none", value_parser = ["none"], hide = true)]
    pub mode: String,
    /// Maximum LLM cost in USD (effective with --mode claude-code/codex, reserved for v1.1.0).
    #[arg(
        long,
        value_name = "USD",
        help = "Max LLM cost in USD (effective with --mode claude-code/codex)"
    )]
    pub max_cost_usd: Option<f64>,
    /// JSON output (always on, kept for consistency).
    #[arg(long, hide = true)]
    pub json: bool,
    /// Database path.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
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

/// A node in a reconstructed evidence path.
#[derive(Serialize, Clone)]
struct EvidenceNode {
    entity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    relation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    weight: Option<f64>,
}

/// A directed evidence chain reconstructed from BFS predecessors.
///
/// Fields:
/// - `from`: name of the seed (source) entity.
/// - `to`: name of the terminal (target) entity.
/// - `path`: ordered list of intermediate nodes from `from` to `to`.
/// - `total_weight`: product of edge weights along the path.
/// - `sub_query_ids`: which sub-queries produced this chain.
#[derive(Serialize)]
struct EvidenceChain {
    from: String,
    to: String,
    path: Vec<EvidenceNode>,
    total_weight: f64,
    depth: usize,
    sub_query_ids: Vec<usize>,
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
    vec_degraded: bool,
}

#[derive(Serialize)]
struct GraphContextEntity {
    name: String,
    entity_type: String,
    degree: u32,
}

#[derive(Serialize)]
struct GraphContextRel {
    from: String,
    to: String,
    relation: String,
    weight: f64,
}

#[derive(Serialize)]
struct GraphContext {
    entities: Vec<GraphContextEntity>,
    relationships: Vec<GraphContextRel>,
}

#[derive(Serialize)]
struct DeepResearchResponse {
    query: String,
    sub_queries: Vec<SubQuery>,
    results: Vec<DeepResult>,
    evidence_chains: Vec<EvidenceChain>,
    #[serde(skip_serializing_if = "Option::is_none")]
    graph_context: Option<GraphContext>,
    stats: ResearchStats,
}

/// Aggregated hit data: (score, source_label, snippet, body, hop_distance, sub_query_ids).
type MergedHit = (f64, String, String, String, Option<usize>, Vec<usize>);

/// Intermediate result from a single sub-query execution.
struct SubQueryResult {
    sub_query_id: usize,
    /// (memory_id, score, source_label, snippet, body, hop_distance)
    hits: Vec<(i64, f64, String, String, String, Option<usize>)>,
    /// Evidence chains reconstructed from BFS.
    chains: Vec<EvidenceChain>,
}

/// Sync entry point — builds a tokio runtime for the async fan-out.
#[tracing::instrument(skip_all, level = "debug", name = "deep_research")]
pub fn run(
    args: DeepResearchArgs,
    llm_backend: crate::cli::LlmBackendChoice,
) -> Result<(), AppError> {
    tracing::debug!(target: "deep_research", query = %args.query, k = args.k, "starting deep research");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to build tokio runtime: {e}")))?;
    rt.block_on(run_async(args, llm_backend))
}

/// Main async logic: decompose, fan-out, assemble, emit JSON.
async fn run_async(
    args: DeepResearchArgs,
    llm_backend: crate::cli::LlmBackendChoice,
) -> Result<(), AppError> {
    let start = std::time::Instant::now();

    if args.query.trim().is_empty() {
        return Err(AppError::Validation(crate::i18n::validation::empty_query()));
    }

    if args.max_cost_usd.is_some() && args.mode == "none" {
        tracing::warn!(target: "deep_research", "--max-cost-usd has no effect without --mode claude-code/codex");
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

    // GAP-DEEPRESEARCH-001 FIX (v1.0.89): use graceful degradation path
    // instead of hard-fail. When LLM is unavailable (OAuth expired, timeout,
    // slots exhausted), fall back to FTS5-only search per sub-query — same
    // contract as `recall` and `hybrid-search`.
    output::emit_progress_i18n(
        "Computing per-sub-query embeddings...",
        "Calculando embeddings por sub-consulta...",
    );
    let mut sub_embeddings: Vec<Option<Arc<Vec<f32>>>> = Vec::with_capacity(sub_query_texts.len());
    let mut vec_degraded = false;
    for sq_text in &sub_query_texts {
        match crate::embedder::try_embed_query_with_deterministic_fallback(
            &paths.models,
            sq_text,
            Some(llm_backend),
        ) {
            Ok((v, _backend)) => sub_embeddings.push(Some(Arc::new(v))),
            Err(reason) => {
                tracing::warn!(target: "deep_research", fallback_reason = %reason, reason_code = %reason.reason_code(), "embedding failed for sub-query; falling back to FTS5");
                sub_embeddings.push(None);
                vec_degraded = true;
            }
        }
    }

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
        // GAP-DEEPRESEARCH-001 FIX: pass Optional embedding (None = FTS5-only).
        let emb = sub_embeddings[idx].clone();
        let ns = namespace.clone();
        let db_path = paths.db.clone();
        let query_text = sq_text.clone();
        let k = args.k;
        let max_hops = args.max_hops;
        let min_weight = args.min_weight;
        let rrf_k = args.rrf_k;
        let graph_decay = args.graph_decay;
        let graph_min_score = args.graph_min_score;
        let max_neighbors_per_hop = args.max_neighbors_per_hop;

        join_set.spawn(async move {
            let _permit = sem
                .acquire_owned()
                .await
                .map_err(|e| (idx, format!("semaphore closed: {e}")))?;

            // Dereference the Arc to obtain a &[f32] slice for the sync function.
            let result = tokio::time::timeout(timeout_dur, async move {
                execute_sub_query(
                    idx,
                    &query_text,
                    emb.as_ref().map(|v| v.as_slice()),
                    &ns,
                    &db_path,
                    k,
                    max_hops,
                    min_weight,
                    rrf_k,
                    graph_decay,
                    graph_min_score,
                    max_neighbors_per_hop,
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
                tracing::warn!(target: "deep_research", sub_query_id = _idx, reason = %reason, "sub-query failed");
            }
            Err(join_err) => {
                failed_count += 1;
                if join_err.is_panic() {
                    tracing::error!(target: "deep_research", error = %join_err, "sub-query task panicked");
                } else {
                    tracing::warn!(target: "deep_research", error = %join_err, "sub-query task cancelled");
                }
            }
        }
    }

    // Phase 3: Evidence assembly — merge, dedup, rank.
    // Aggregate hits: memory_id -> (best_score, source, snippet, body, hop_distance, sub_query_ids)
    let mut merged: crate::hash::AHashMap<i64, MergedHit> =
        crate::hash::AHashMap::with_capacity_and_hasher(
            sub_query_results.len() * args.k,
            Default::default(),
        );

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

    // GAP-09/10 FIX: Collect evidence chains from reconstructed BFS paths.
    // The old code appended flat node pairs from a global SELECT; now each
    // sub-query returns directed EvidenceChain structs (from, to, path).
    let completed_count = sub_query_results.len();
    let mut evidence_chains: Vec<EvidenceChain> = Vec::with_capacity(completed_count * 2);
    let mut seen_chain_keys: HashSet<String> = HashSet::with_capacity(completed_count * 2);

    for sqr in sub_query_results {
        for chain in sqr.chains {
            // Deduplicate chains by (from, to) pair.
            let key = format!("{}->{}", chain.from, chain.to);
            if seen_chain_keys.insert(key) {
                evidence_chains.push(chain);
            }
        }
    }

    // Sort evidence chains by total_weight descending, discard single-hop trivial chains.
    evidence_chains.retain(|c| c.depth >= 2);
    evidence_chains.sort_by(|a, b| {
        b.total_weight
            .partial_cmp(&a.total_weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let unique_memories = results.len();
    let evidence_count = evidence_chains.len();

    // MEDIUM-01b: Build graph_context with entities and relationships from result memories.
    let graph_context = if !results.is_empty() {
        let result_names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        let mut ctx_entities: Vec<GraphContextEntity> = Vec::with_capacity(results.len());
        let mut ctx_rels: Vec<GraphContextRel> = Vec::with_capacity(results.len() * 2);
        let mut seen_entity_ids: crate::hash::AHashSet<i64> =
            crate::hash::AHashSet::with_capacity_and_hasher(results.len(), Default::default());

        for name in &result_names {
            if let Ok(Some(eid)) = entities::find_entity_id(&conn, &namespace, name) {
                if seen_entity_ids.insert(eid) {
                    let etype: String = conn
                        .query_row(
                            "SELECT COALESCE(type,'concept') FROM entities WHERE id = ?1",
                            rusqlite::params![eid],
                            |r| r.get(0),
                        )
                        .unwrap_or_else(|_| "concept".to_string());
                    let degree: u32 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM relationships WHERE source_id = ?1 OR target_id = ?1",
                            rusqlite::params![eid],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    ctx_entities.push(GraphContextEntity {
                        name: name.to_string(),
                        entity_type: etype,
                        degree,
                    });
                }
            }
        }

        let entity_ids: Vec<i64> = seen_entity_ids.iter().copied().collect();
        if entity_ids.len() >= 2 {
            let placeholders: String = entity_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT s.name, t.name, r.relation, r.weight \
                 FROM relationships r \
                 JOIN entities s ON s.id = r.source_id \
                 JOIN entities t ON t.id = r.target_id \
                 WHERE r.source_id IN ({placeholders}) AND r.target_id IN ({placeholders}) \
                 LIMIT 50"
            );
            if let Ok(mut stmt) = conn.prepare(&sql) {
                let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
                    Vec::with_capacity(entity_ids.len() * 2);
                for id in &entity_ids {
                    params.push(Box::new(*id));
                }
                for id in &entity_ids {
                    params.push(Box::new(*id));
                }
                let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();
                if let Ok(rows) = stmt.query_map(param_refs.as_slice(), |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, f64>(3)?,
                    ))
                }) {
                    for row in rows.flatten() {
                        ctx_rels.push(GraphContextRel {
                            from: row.0,
                            to: row.1,
                            relation: row.2,
                            weight: row.3,
                        });
                    }
                }
            }
        }

        if ctx_entities.is_empty() {
            None
        } else {
            Some(GraphContext {
                entities: ctx_entities,
                relationships: ctx_rels,
            })
        }
    } else {
        None
    };

    tracing::debug!(target: "deep_research",
        total_results = results.len(),
        total_chains = evidence_chains.len(),
        "assembly complete"
    );

    // Phase 4: JSON output.
    output::emit_json(&DeepResearchResponse {
        query: args.query,
        sub_queries,
        results,
        evidence_chains,
        graph_context,
        stats: ResearchStats {
            sub_queries_total: sub_query_texts.len(),
            sub_queries_completed: completed_count,
            sub_queries_failed: failed_count,
            sub_queries_timed_out: timed_out_count,
            unique_memories_found: unique_memories,
            evidence_chains_found: evidence_count,
            elapsed_ms: start.elapsed().as_millis() as u64,
            vec_degraded,
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

    let mut parts: Vec<String> = Vec::with_capacity(max);

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

    // If still no split, try word-pair decomposition for multi-word queries.
    if parts.is_empty() {
        let words: Vec<&str> = query.split_whitespace().filter(|w| w.len() > 2).collect();
        if words.len() >= 3 {
            parts.push(query.to_string());
            parts.push(format!("{} {}", words[0], words[1]));
            parts.push(format!(
                "{} {}",
                words[words.len() - 2],
                words[words.len() - 1]
            ));
        }
    }

    if parts.is_empty() {
        return vec![query.to_string()];
    }

    // Cap at max.
    parts.truncate(max);
    parts
}

/// Reconstruct a directed path from `target_entity_id` back to a seed using the
/// predecessor map built by BFS.  Returns the path nodes from root to target
/// plus the accumulated edge weights.
fn reconstruct_path(
    target_id: i64,
    seed_entity_ids: &HashSet<i64>,
    predecessor: &PredecessorMap,
    entity_names: &crate::hash::AHashMap<i64, String>,
) -> Option<(Vec<EvidenceNode>, f64)> {
    let mut path_ids: Vec<(i64, Option<String>, Option<f64>)> = Vec::with_capacity(8);
    let mut total_weight = 1.0_f64;
    let mut current = target_id;

    loop {
        if seed_entity_ids.contains(&current) {
            break;
        }
        let (parent, relation, weight) = predecessor.get(&current)?;
        total_weight *= weight;
        path_ids.push((current, Some(relation.clone()), Some(*weight)));
        current = *parent;
    }
    // Push the seed entity (root).
    path_ids.push((current, None, None));

    // Reverse so path goes from seed → target.
    path_ids.reverse();

    let nodes: Vec<EvidenceNode> = path_ids
        .into_iter()
        .map(|(id, relation, weight)| EvidenceNode {
            entity: entity_names
                .get(&id)
                .cloned()
                .unwrap_or_else(|| format!("entity-{id}")),
            relation,
            weight,
        })
        .collect();

    Some((nodes, total_weight))
}

/// Execute a single sub-query: hybrid search (KNN + FTS fused via RRF) + graph traversal.
///
/// GAP-07 fix: receives the embedding for THIS sub-query (not the shared original).
/// GAP-08/11 fix: uses rrf_fuse() for proper score fusion instead of hardcoded 0.5.
/// GAP-09/10 fix: builds directed evidence chains filtered to discovered entities.
/// GAP-17: respects max_neighbors_per_hop cap in BFS.
///
/// Runs synchronously on a blocking thread (called from a tokio spawn context).
/// Each call opens its own read-only SQLite connection to leverage WAL concurrency.
#[allow(clippy::too_many_arguments)]
fn execute_sub_query(
    sub_query_id: usize,
    query_text: &str,
    embedding: Option<&[f32]>,
    namespace: &str,
    db_path: &std::path::Path,
    k: usize,
    max_hops: usize,
    min_weight: f64,
    rrf_k: f64,
    graph_decay: f64,
    graph_min_score: f64,
    max_neighbors_per_hop: Option<usize>,
) -> Result<SubQueryResult, String> {
    let conn = open_ro(db_path).map_err(|e| format!("failed to open db: {e}"))?;

    let mut hits: Vec<(i64, f64, String, String, String, Option<usize>)> =
        Vec::with_capacity(k * 2);
    let mut seen_ids: crate::hash::AHashSet<i64> =
        crate::hash::AHashSet::with_capacity_and_hasher(k * 2, Default::default());

    // --- GAP-08/11 FIX: Use RRF fusion for KNN + FTS instead of hardcoded 0.5 ---

    // 1. KNN vector search — collect ranked IDs (skipped when embedding unavailable).
    let (knn_ids, knn_distance_map) = if let Some(emb) = embedding {
        let knn_results = memories::knn_search(&conn, emb, &[namespace.to_string()], None, k)
            .map_err(|e| format!("knn_search failed: {e}"))?;
        let ids: Vec<i64> = knn_results.iter().map(|(id, _)| *id).collect();
        tracing::debug!(target: "deep_research", sub_query_id, knn_count = ids.len(), "KNN complete");
        let dist_map: crate::hash::AHashMap<i64, f64> = knn_results
            .iter()
            .map(|(id, dist)| (*id, *dist as f64))
            .collect();
        (ids, dist_map)
    } else {
        tracing::debug!(target: "deep_research", sub_query_id, "KNN skipped (no embedding); FTS5-only");
        (vec![], crate::hash::AHashMap::default())
    };

    // 2. FTS5 search — collect ranked IDs.
    let fts_results = match memories::fts_search(&conn, query_text, namespace, None, k) {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(target: "deep_research",
                sub_query_id,
                "FTS5 search failed, continuing with KNN only: {e}"
            );
            vec![]
        }
    };
    let fts_ids: Vec<i64> = fts_results.iter().map(|r| r.id).collect();
    tracing::debug!(target: "deep_research", sub_query_id, fts_count = fts_ids.len(), "FTS complete");

    // 3. Fuse via RRF.
    let rrf_scores = rrf_fuse(&[(1.0, &knn_ids), (1.0, &fts_ids)], rrf_k);
    let max_possible = rrf_max_possible(&[1.0, 1.0], rrf_k);

    // 4. Sort fused results and build hits.
    let mut fused: Vec<(i64, f64)> = rrf_scores.into_iter().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    fused.truncate(k * 2);
    tracing::debug!(target: "deep_research",
        sub_query_id,
        fused_count = fused.len(),
        "RRF fusion complete"
    );

    if fused.is_empty() && !knn_ids.is_empty() {
        tracing::warn!(target: "deep_research", sub_query_id, knn_count = knn_ids.len(), fts_count = fts_ids.len(),
            "RRF fusion returned 0 results despite KNN/FTS hits; consider lowering --graph-min-score");
    }

    for (memory_id, combined_score) in &fused {
        if seen_ids.insert(*memory_id) {
            let normalized = if max_possible > 0.0 {
                combined_score / max_possible
            } else {
                0.0
            };
            let score = normalized.clamp(0.0, 1.0);
            let in_knn = knn_distance_map.contains_key(memory_id);
            let in_fts = fts_ids.contains(memory_id);
            let source = match (in_knn, in_fts) {
                (true, true) => "hybrid",
                (true, false) => "knn",
                (false, true) => "fts",
                (false, false) => "graph",
            };
            if let Ok(Some(row)) = memories::read_full(&conn, *memory_id) {
                let snippet: String = row.body.chars().take(300).collect();
                hits.push((
                    *memory_id,
                    score,
                    source.to_string(),
                    snippet,
                    row.body,
                    None,
                ));
            }
        }
    }

    // 5. Graph traversal from discovered memories.
    // GAP-09/10 FIX: entity KNN also uses this sub-query's embedding.
    let memory_ids: Vec<i64> = hits.iter().map(|(id, ..)| *id).collect();
    let mut chains: Vec<EvidenceChain> = Vec::with_capacity(memory_ids.len());

    if !memory_ids.is_empty() && max_hops > 0 {
        // Seed entities from KNN on entity vectors (skipped when embedding unavailable).
        let entity_ids: Vec<i64> = if let Some(emb) = embedding {
            entities::knn_search(&conn, emb, namespace, 5)
                .inspect_err(|e| tracing::warn!(target: "deep_research", error = %e, "entity KNN search failed, skipping graph seed"))
                .unwrap_or_default()
                .iter()
                .map(|(id, _)| *id)
                .collect()
        } else {
            vec![]
        };

        // HIGH-01 FIX: limit seeds to top-5 memories by score to prevent
        // BFS from starting at every node when k >= total memories.
        let top_seed_count = 5.min(memory_ids.len());
        let top_memory_ids = &memory_ids[..top_seed_count];
        let mut seed_entity_ids: Vec<i64> = entity_ids.clone();
        for &mem_id in top_memory_ids {
            let mut stmt = conn
                .prepare_cached("SELECT entity_id FROM memory_entities WHERE memory_id = ?1")
                .map_err(|e| format!("prepare failed: {e}"))?;
            let ids: Vec<i64> = stmt
                .query_map(rusqlite::params![mem_id], |r| r.get(0))
                .map_err(|e| format!("query failed: {e}"))?
                .filter_map(|r| r.ok())
                .collect();
            seed_entity_ids.extend(ids);
        }
        seed_entity_ids.sort_unstable();
        seed_entity_ids.dedup();
        tracing::debug!(target: "deep_research",
            sub_query_id,
            seed_count = seed_entity_ids.len(),
            "seed entities collected"
        );

        let all_seed_ids: Vec<i64> = memory_ids
            .iter()
            .chain(entity_ids.iter())
            .copied()
            .collect();

        // Graph traversal with hop scores.
        if let Ok(graph_results) = traverse_from_memories_with_hops_capped(
            &conn,
            &all_seed_ids,
            namespace,
            min_weight,
            max_hops as u32,
            max_neighbors_per_hop,
        ) {
            // Build seed score map from RRF-fused scores for graph decay computation.
            let seed_score_map: crate::hash::AHashMap<i64, f64> = fused
                .iter()
                .map(|(id, s)| {
                    let normalized = if max_possible > 0.0 {
                        s / max_possible
                    } else {
                        0.0
                    };
                    (*id, normalized.clamp(0.0, 1.0))
                })
                .collect();

            for (graph_mem_id, hop) in graph_results {
                if seen_ids.insert(graph_mem_id) {
                    // GAP-08/11 FIX: graph score = seed_score * decay^hop * edge_weight.
                    // For the seed score, use the best score among the seed memories that
                    // transitively reached this graph memory (approximate with the average
                    // seed score since we don't track the exact path yet).
                    let avg_seed_score: f64 = if seed_score_map.is_empty() {
                        0.5
                    } else {
                        let sum: f64 = seed_score_map.values().sum();
                        sum / seed_score_map.len() as f64
                    };
                    let graph_score =
                        (avg_seed_score * graph_decay.powi(hop as i32)).clamp(0.0, 1.0);

                    if graph_score < graph_min_score {
                        continue;
                    }

                    if let Ok(Some(row)) = memories::read_full(&conn, graph_mem_id) {
                        let snippet: String = row.body.chars().take(300).collect();
                        hits.push((
                            graph_mem_id,
                            graph_score,
                            "graph".to_string(),
                            snippet,
                            row.body,
                            Some(hop as usize),
                        ));
                    }
                }
            }
        }

        // GAP-09/10 FIX: Build directed evidence chains using BFS with predecessor map,
        // filtered to entities discovered in this sub-query.
        if !seed_entity_ids.is_empty() {
            let (entity_depth, predecessor) = bfs_with_predecessors(
                &conn,
                &seed_entity_ids,
                namespace,
                min_weight,
                max_hops as u32,
                max_neighbors_per_hop,
            )
            .unwrap_or_default();

            tracing::debug!(target: "deep_research",
                sub_query_id,
                bfs_nodes = entity_depth.len(),
                predecessors = predecessor.len(),
                "BFS complete"
            );

            let seed_entity_set: HashSet<i64> = seed_entity_ids.iter().copied().collect();

            // Collect entity IDs we need names for.
            let all_entity_ids: Vec<i64> = entity_depth.keys().copied().collect();
            let mut entity_names: crate::hash::AHashMap<i64, String> =
                crate::hash::AHashMap::with_capacity_and_hasher(
                    all_entity_ids.len(),
                    ahash::RandomState::default(),
                );
            for &eid in &all_entity_ids {
                let name_res: rusqlite::Result<String> = conn.query_row(
                    "SELECT name FROM entities WHERE id = ?1",
                    rusqlite::params![eid],
                    |r| r.get(0),
                );
                if let Ok(name) = name_res {
                    entity_names.insert(eid, name);
                }
            }

            // Reconstruct a path for each non-seed entity that has a predecessor.
            for (&target_id, &_hop) in &entity_depth {
                if seed_entity_set.contains(&target_id) {
                    continue;
                }
                if !predecessor.contains_key(&target_id) {
                    continue;
                }
                if let Some((path_nodes, total_weight)) =
                    reconstruct_path(target_id, &seed_entity_set, &predecessor, &entity_names)
                {
                    if path_nodes.len() < 2 {
                        continue;
                    }
                    let from = path_nodes
                        .first()
                        .map(|n| n.entity.clone())
                        .unwrap_or_default();
                    let to = path_nodes
                        .last()
                        .map(|n| n.entity.clone())
                        .unwrap_or_default();
                    let depth = path_nodes.len();
                    chains.push(EvidenceChain {
                        from,
                        to,
                        path: path_nodes,
                        total_weight,
                        depth,
                        sub_query_ids: vec![sub_query_id],
                    });
                }
            }

            // Sort chains by total_weight descending and cap to avoid huge output.
            chains.sort_by(|a, b| {
                b.total_weight
                    .partial_cmp(&a.total_weight)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            chains.truncate(20);
            tracing::debug!(target: "deep_research",
                sub_query_id,
                chains_count = chains.len(),
                "evidence chains built"
            );
        }
    }

    Ok(SubQueryResult {
        sub_query_id,
        hits,
        chains,
    })
}

// ────────────────────────────────────────────────────────────────────────────
// Re-export sub_query_results field initialisation for the stats counter.
// The field is moved out of run_async after the join loop; we need to shadow it.
// ────────────────────────────────────────────────────────────────────────────

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
            vec_degraded: false,
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
            graph_context: None,
            stats: ResearchStats {
                sub_queries_total: 1,
                sub_queries_completed: 1,
                sub_queries_failed: 0,
                sub_queries_timed_out: 0,
                unique_memories_found: 0,
                evidence_chains_found: 0,
                elapsed_ms: 42,
                vec_degraded: false,
            },
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["query"], "test query");
        assert!(json["sub_queries"].is_array());
        assert!(json["results"].is_array());
        assert!(json["evidence_chains"].is_array());
        assert_eq!(json["stats"]["elapsed_ms"], 42);
    }

    // ---- GAP-07 regression: different sub-queries produce distinct embeddings ----
    // We test decompose_query returns texts that *would* produce distinct embeddings
    // (different text inputs → different embedding inputs → different search results).
    #[test]
    fn test_distinct_sub_queries_produce_distinct_texts() {
        let queries = [
            "authentication design decisions",
            "deployment configuration and infrastructure",
        ];
        // These two texts must be different strings (prerequisite for distinct embeddings).
        assert_ne!(queries[0], queries[1]);

        // decompose_query with semicolons must preserve distinct texts.
        let decomposed = decompose_query(
            "authentication design decisions; deployment configuration and infrastructure",
            7,
        );
        assert_eq!(decomposed.len(), 2);
        assert_ne!(decomposed[0], decomposed[1]);
    }

    // ---- GAP-08/11 regression: rrf_fuse integration via fusion module ----
    #[test]
    fn test_rrf_fuse_via_fusion_module() {
        use crate::storage::fusion::rrf_fuse;

        let knn_ids: Vec<i64> = vec![1, 2, 3];
        let fts_ids: Vec<i64> = vec![2, 1, 4];
        let scores = rrf_fuse(&[(1.0, &knn_ids), (1.0, &fts_ids)], 60.0);

        // Items appearing in both lists must score higher than items in only one list.
        let score_1 = scores[&1];
        let score_2 = scores[&2];
        let score_3 = scores[&3]; // knn only, rank 3
        let score_4 = scores[&4]; // fts only, rank 3

        assert!(
            score_1 > score_3,
            "id 1 (both lists) must beat id 3 (knn-only rank 3)"
        );
        assert!(
            score_2 > score_4,
            "id 2 (both lists) must beat id 4 (fts-only rank 3)"
        );
    }

    // ---- GAP-09/10 regression: evidence chains must be directed paths ----
    #[test]
    fn test_evidence_chain_has_from_to_and_path() {
        let chain = EvidenceChain {
            from: "auth-module".to_string(),
            to: "jwt-service".to_string(),
            path: vec![
                EvidenceNode {
                    entity: "auth-module".to_string(),
                    relation: None,
                    weight: None,
                },
                EvidenceNode {
                    entity: "token-validator".to_string(),
                    relation: Some("depends-on".to_string()),
                    weight: Some(0.9),
                },
                EvidenceNode {
                    entity: "jwt-service".to_string(),
                    relation: Some("uses".to_string()),
                    weight: Some(0.8),
                },
            ],
            total_weight: 0.72,
            depth: 3,
            sub_query_ids: vec![0],
        };

        let json = serde_json::to_value(&chain).expect("serialization failed");
        assert!(
            json["from"].is_string(),
            "evidence chain must have 'from' field"
        );
        assert!(
            json["to"].is_string(),
            "evidence chain must have 'to' field"
        );
        assert!(
            json["path"].is_array(),
            "evidence chain must have 'path' array"
        );
        assert_eq!(json["path"].as_array().unwrap().len(), 3);
        assert!(json["total_weight"].is_number(), "must have total_weight");
        assert_eq!(json["depth"], 3);
    }

    // ---- GAP-10 regression: reconstruct_path returns correct node order ----
    #[test]
    fn test_reconstruct_path_root_to_target_order() {
        // Build a simple chain: entity 10 (seed) -> entity 20 -> entity 30 (target)
        let seed_set: HashSet<i64> = [10i64].into_iter().collect();
        let mut predecessor: PredecessorMap = std::collections::HashMap::new();
        predecessor.insert(20, (10, "depends-on".to_string(), 0.9));
        predecessor.insert(30, (20, "uses".to_string(), 0.8));
        let mut entity_names: crate::hash::AHashMap<i64, String> = crate::hash::AHashMap::default();
        entity_names.insert(10, "seed-entity".to_string());
        entity_names.insert(20, "middle-entity".to_string());
        entity_names.insert(30, "target-entity".to_string());

        let result = reconstruct_path(30, &seed_set, &predecessor, &entity_names);
        assert!(result.is_some(), "path must be reconstructed");
        let (nodes, weight) = result.unwrap();
        // Path must be [seed, middle, target]
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].entity, "seed-entity");
        assert_eq!(nodes[1].entity, "middle-entity");
        assert_eq!(nodes[2].entity, "target-entity");
        // total_weight = 0.9 * 0.8
        assert!((weight - 0.72).abs() < 1e-6);
    }

    // ---- GAP-09 regression: evidence chains must NOT be present for 1-hop trivial pairs ----
    #[test]
    fn test_evidence_chains_single_hop_filtered_out() {
        // A chain of depth 1 (only root node) should be discarded.
        let chain = EvidenceChain {
            from: "a".to_string(),
            to: "a".to_string(),
            path: vec![EvidenceNode {
                entity: "a".to_string(),
                relation: None,
                weight: None,
            }],
            total_weight: 1.0,
            depth: 1,
            sub_query_ids: vec![0],
        };
        // Simulate the filter: retain chains with depth >= 2.
        let chains = vec![chain];
        let retained: Vec<_> = chains.into_iter().filter(|c| c.depth >= 2).collect();
        assert!(retained.is_empty(), "depth-1 chains must be filtered out");
    }

    // ---- GAP-17 regression: bfs_with_predecessors honours max_neighbors_per_hop ----
    #[test]
    fn test_bfs_with_predecessors_respects_neighbor_cap() {
        use crate::graph::bfs_with_predecessors;
        use rusqlite::Connection;

        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE relationships (
                source_id INTEGER NOT NULL,
                target_id INTEGER NOT NULL,
                weight REAL NOT NULL,
                namespace TEXT NOT NULL,
                relation TEXT NOT NULL DEFAULT 'related'
             );",
        )
        .unwrap();

        // Seed entity 1 has 5 neighbours.
        for target in 2i64..=6 {
            conn.execute(
                "INSERT INTO relationships (source_id, target_id, weight, namespace) VALUES (?1, ?2, ?3, 'ns')",
                rusqlite::params![1i64, target, 1.0f64],
            )
            .unwrap();
        }

        // Without cap: all 5 neighbours reached.
        let (depth_uncapped, _) = bfs_with_predecessors(&conn, &[1], "ns", 0.0, 1, None).unwrap();
        assert_eq!(
            depth_uncapped.len() - 1,
            5,
            "uncapped must discover all 5 neighbours (plus seed)"
        );

        // With cap=2: only top-2 neighbours (by weight; all equal here so first 2 returned).
        let (depth_capped, _) = bfs_with_predecessors(&conn, &[1], "ns", 0.0, 1, Some(2)).unwrap();
        // seed + 2 neighbours = 3 entries.
        assert_eq!(
            depth_capped.len(),
            3,
            "capped to 2 must yield seed + 2 neighbours"
        );
    }
}
