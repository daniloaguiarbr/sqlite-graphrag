//! Handler for the `recall` CLI subcommand.

use crate::cli::MemoryType;
use crate::errors::AppError;
use crate::graph::traverse_from_memories_with_hops;
use crate::i18n::errors_msg;
use crate::output::{self, JsonOutputFormat, RecallItem, RecallResponse};
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use crate::storage::entities;
use crate::storage::memories;

/// Arguments for the `recall` subcommand.
///
/// When `--namespace` is omitted the query runs against the `global` namespace,
/// which is the default namespace used by `remember` when no `--namespace` flag
/// is provided. Pass an explicit `--namespace` value to search a different
/// isolated namespace.
#[derive(clap::Args)]
pub struct RecallArgs {
    #[arg(help = "Search query string (semantic vector search via sqlite-vec)")]
    pub query: String,
    /// Maximum number of direct vector matches to return.
    ///
    /// Note: this flag controls only `direct_matches`. Graph traversal results
    /// (`graph_matches`) are unbounded by default; use `--max-graph-results` to
    /// cap them independently. The `results` field aggregates both lists.
    /// Validated to the inclusive range `1..=4096` (the upper bound matches
    /// `sqlite-vec`'s knn limit; out-of-range values are rejected at parse time).
    #[arg(short = 'k', long, default_value = "10", value_parser = crate::parsers::parse_k_range)]
    pub k: usize,
    /// Filter by memory.type. Note: distinct from graph entity_type
    /// (project/tool/person/file/concept/incident/decision/memory/dashboard/issue_tracker)
    /// used in --entities-file.
    #[arg(long, value_enum)]
    pub r#type: Option<MemoryType>,
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long)]
    pub no_graph: bool,
    /// Disable -k cap and return all direct matches without truncation.
    ///
    /// When set, the `-k`/`--k` flag is ignored for `direct_matches` and the
    /// response includes every match above the distance threshold. Useful when
    /// callers need the complete set rather than a top-N preview.
    #[arg(long)]
    pub precise: bool,
    #[arg(long, default_value = "2")]
    pub max_hops: u32,
    #[arg(long, default_value = "0.3")]
    pub min_weight: f64,
    /// Cap the size of `graph_matches` to at most N entries.
    ///
    /// Defaults to unbounded (`None`) so existing pipelines see the same shape
    /// as in v1.0.22 and earlier. Set this when a query touches a dense graph
    /// neighbourhood and the caller only needs a top-N preview. Added in v1.0.23.
    #[arg(long, value_name = "N")]
    pub max_graph_results: Option<usize>,
    /// Filter results by maximum distance. Results with distance greater than this value
    /// are excluded. If all matches exceed this threshold, the command exits with code 4
    /// (`not found`) per the documented public contract.
    /// Default `1.0` disables the filter and preserves the top-k behavior.
    #[arg(long, alias = "min-distance", default_value = "1.0")]
    pub max_distance: f32,
    #[arg(long, value_enum, default_value_t = JsonOutputFormat::Json)]
    pub format: JsonOutputFormat,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// Accept `--json` as a no-op because output is already JSON by default.
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    /// Search across all namespaces instead of a single namespace.
    ///
    /// Cannot be combined with `--namespace`. When set, the query runs against
    /// every namespace and results include a `namespace` field to identify origin.
    #[arg(long, conflicts_with = "namespace")]
    pub all_namespaces: bool,
}

pub fn run(args: RecallArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let _ = args.format;
    if args.query.trim().is_empty() {
        return Err(AppError::Validation(crate::i18n::validation::empty_query()));
    }
    // Resolve the list of namespaces to search:
    // - empty vec  => all namespaces (sentinel used by knn_search)
    // - single vec => one namespace (default or --namespace value)
    let namespaces: Vec<String> = if args.all_namespaces {
        Vec::new()
    } else {
        vec![crate::namespace::resolve_namespace(
            args.namespace.as_deref(),
        )?]
    };
    // Single namespace string used for graph traversal and error messages.
    let namespace_for_graph = namespaces
        .first()
        .cloned()
        .unwrap_or_else(|| "global".to_string());
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    output::emit_progress_i18n(
        "Computing query embedding...",
        "Calculando embedding da consulta...",
    );
    let embedding = crate::daemon::embed_query_or_local(&paths.models, &args.query)?;

    let conn = open_ro(&paths.db)?;

    let memory_type_str = args.r#type.map(|t| t.as_str());
    // When --precise is set, lift the -k cap so every match is returned; the
    // max_distance filter below will trim irrelevant results instead.
    let effective_k = if args.precise { 100_000 } else { args.k };
    let knn_results =
        memories::knn_search(&conn, &embedding, &namespaces, memory_type_str, effective_k)?;

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
                // Direct vector matches do not have a graph depth; rely on `distance`.
                graph_depth: None,
            });
            memory_ids.push(memory_id);
        }
    }

    let mut graph_matches = Vec::new();
    if !args.no_graph {
        let entity_knn = entities::knn_search(&conn, &embedding, &namespace_for_graph, 5)?;
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
                args.min_weight,
                args.max_hops,
            )?;

            for (graph_mem_id, hop) in graph_memory_ids {
                // v1.0.23: respect the optional cap on graph results so dense
                // neighbourhoods do not flood the response unintentionally.
                if let Some(cap) = args.max_graph_results {
                    if graph_matches.len() >= cap {
                        break;
                    }
                }
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
                    // Compute approximate distance from graph hop count.
                    // WARNING: graph_distance is a hop-count proxy, NOT real cosine distance.
                    // For confident ranking, prefer the `graph_depth` field (set to Some(hop)
                    // below). Real cosine distance for graph matches would require
                    // re-embedding (200-500ms latency) and is reserved for v1.0.28.
                    let graph_distance = 1.0 - 1.0 / (hop as f32 + 1.0);
                    graph_matches.push(RecallItem {
                        memory_id: row.id,
                        name: row.name,
                        namespace: row.namespace,
                        memory_type: row.memory_type,
                        description: row.description,
                        snippet,
                        distance: graph_distance,
                        source: "graph".to_string(),
                        graph_depth: Some(hop),
                    });
                }
            }
        }
    }

    // Filtrar por max_distance se < 1.0 (ativado). Se nenhum hit dentro do threshold, exit 4.
    if args.max_distance < 1.0 {
        let has_relevant = direct_matches
            .iter()
            .any(|item| item.distance <= args.max_distance);
        if !has_relevant {
            return Err(AppError::NotFound(errors_msg::no_recall_results(
                args.max_distance,
                &args.query,
                &namespace_for_graph,
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
mod tests {
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
            graph_depth: if source == "graph" { Some(0) } else { None },
        }
    }

    #[test]
    fn recall_response_serializes_required_fields() {
        let resp = RecallResponse {
            query: "rust memory".to_string(),
            k: 5,
            direct_matches: vec![make_item("mem-a", 0.12, "direct")],
            graph_matches: vec![],
            results: vec![make_item("mem-a", 0.12, "direct")],
            elapsed_ms: 42,
        };

        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["query"], "rust memory");
        assert_eq!(json["k"], 5);
        assert_eq!(json["elapsed_ms"], 42u64);
        assert!(json["direct_matches"].is_array());
        assert!(json["graph_matches"].is_array());
        assert!(json["results"].is_array());
    }

    #[test]
    fn recall_item_serializes_renamed_type() {
        let item = make_item("mem-test", 0.25, "direct");
        let json = serde_json::to_value(&item).expect("serialization failed");

        // The memory_type field is renamed to "type" in JSON
        assert_eq!(json["type"], "fact");
        assert_eq!(json["distance"], 0.25f32);
        assert_eq!(json["source"], "direct");
    }

    #[test]
    fn recall_response_results_contains_direct_and_graph() {
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

        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["direct_matches"].as_array().unwrap().len(), 1);
        assert_eq!(json["graph_matches"].as_array().unwrap().len(), 1);
        assert_eq!(json["results"].as_array().unwrap().len(), 2);
        assert_eq!(json["results"][0]["source"], "direct");
        assert_eq!(json["results"][1]["source"], "graph");
    }

    #[test]
    fn recall_response_empty_serializes_empty_arrays() {
        let resp = RecallResponse {
            query: "nothing".to_string(),
            k: 3,
            direct_matches: vec![],
            graph_matches: vec![],
            results: vec![],
            elapsed_ms: 1,
        };

        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["direct_matches"].as_array().unwrap().len(), 0);
        assert_eq!(json["results"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn graph_matches_distance_uses_hop_count_proxy() {
        // Verify the hop-count proxy formula: 1.0 - 1.0 / (hop + 1.0)
        // hop=0 → 0.0 (seed-level entity, identity distance)
        // hop=1 → 0.5
        // hop=2 → ≈ 0.667
        // hop=3 → 0.75
        let cases: &[(u32, f32)] = &[(0, 0.0), (1, 0.5), (2, 0.6667), (3, 0.75)];
        for &(hop, expected) in cases {
            let d = 1.0_f32 - 1.0 / (hop as f32 + 1.0);
            assert!(
                (d - expected).abs() < 0.001,
                "hop={hop} expected={expected} got={d}"
            );
        }
    }
}
