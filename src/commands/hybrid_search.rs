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
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
    #[arg(long, env = "NEUROGRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(serde::Serialize)]
pub struct HybridSearchResponse {
    pub query: String,
    pub combined_rank: Vec<RecallItem>,
    pub vec_rank: Vec<RecallItem>,
    pub fts_rank: Vec<RecallItem>,
}

pub fn run(args: HybridSearchArgs) -> Result<(), AppError> {
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    output::emit_progress("Computing query embedding...");
    let embedder = crate::embedder::get_embedder(&paths.models)?;
    let embedding = crate::embedder::embed_query(embedder, &args.query)?;

    let conn = open_ro(&paths.db)?;

    let memory_type_str = args.r#type.map(|t| t.as_str());

    let vec_results =
        memories::knn_search(&conn, &embedding, &namespace, memory_type_str, args.k * 2)?;

    let mut vec_rank = Vec::new();
    for (memory_id, distance) in vec_results.iter() {
        if let Some(row) = memories::read_full(&conn, *memory_id)? {
            let snippet: String = row.body.chars().take(300).collect();
            vec_rank.push(RecallItem {
                memory_id: row.id,
                name: row.name,
                namespace: row.namespace,
                memory_type: row.memory_type,
                description: row.description,
                snippet,
                distance: *distance,
                source: "vector".to_string(),
            });
        }
    }

    let fts_results =
        memories::fts_search(&conn, &args.query, &namespace, memory_type_str, args.k * 2)?;

    let mut fts_rank = Vec::new();
    for row in fts_results.iter() {
        let snippet: String = row.body.chars().take(300).collect();
        fts_rank.push(RecallItem {
            memory_id: row.id,
            name: row.name.clone(),
            namespace: row.namespace.clone(),
            memory_type: row.memory_type.clone(),
            description: row.description.clone(),
            snippet,
            distance: 0.0,
            source: "fts".to_string(),
        });
    }

    let mut combined_scores: HashMap<i64, (f32, RecallItem)> = HashMap::new();
    let rrf_k = args.rrf_k as f32;

    for (rank, item) in vec_rank.iter().enumerate() {
        let score = args.weight_vec * (1.0 / (rrf_k + rank as f32 + 1.0));
        combined_scores
            .entry(item.memory_id)
            .or_insert_with(|| (score, item.clone()))
            .0 += score;
    }

    for (rank, item) in fts_rank.iter().enumerate() {
        let score = args.weight_fts * (1.0 / (rrf_k + rank as f32 + 1.0));
        let entry = combined_scores
            .entry(item.memory_id)
            .or_insert_with(|| (0.0, item.clone()));
        entry.0 += score;
    }

    let mut combined: Vec<_> = combined_scores.into_iter().collect();
    combined.sort_by(|a, b| {
        b.1 .0
            .partial_cmp(&a.1 .0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let combined_rank: Vec<RecallItem> = combined
        .into_iter()
        .take(args.k)
        .map(|(_, (_, item))| item)
        .collect();

    output::emit_json(&HybridSearchResponse {
        query: args.query,
        combined_rank,
        vec_rank,
        fts_rank,
    })?;

    Ok(())
}
