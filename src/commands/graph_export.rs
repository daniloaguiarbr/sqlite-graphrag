//! Handler for the `graph-export` CLI subcommand.

use crate::cli::GraphExportFormat;
use crate::entity_type::EntityType;
use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use crate::storage::entities;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

/// Optional nested subcommands. When absent, the default behavior exports
/// the full entity snapshot for backward compatibility.
#[derive(clap::Subcommand)]
pub enum GraphSubcommand {
    /// Traverse relationships from a starting entity using BFS
    Traverse(GraphTraverseArgs),
    /// Show graph statistics (node/edge counts, degree distribution)
    Stats(GraphStatsArgs),
    /// List entities stored in the graph with optional filters
    Entities(GraphEntitiesArgs),
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphTraverseFormat {
    Json,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphStatsFormat {
    Json,
    Text,
}

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Export full entity snapshot as JSON (default)\n  \
    sqlite-graphrag graph\n\n  \
    # Traverse relationships from a starting entity\n  \
    sqlite-graphrag graph traverse --from acme-corp --depth 2\n\n  \
    # Show graph statistics as structured JSON\n  \
    sqlite-graphrag graph stats --format json\n\n  \
    # List entities filtered by type\n  \
    sqlite-graphrag graph entities --entity-type person\n\n  \
    # Export full snapshot in DOT format for Graphviz\n  \
    sqlite-graphrag graph --format dot --output graph.dot\n\n  \
NOTES:\n  \
    Without a subcommand, exports the full entity+edge snapshot.\n  \
    Use `traverse`, `stats`, or `entities` for targeted queries.")]
pub struct GraphArgs {
    /// Optional subcommand; without one, export the full entity snapshot.
    #[command(subcommand)]
    pub subcommand: Option<GraphSubcommand>,
    /// Filter by namespace. Defaults to all namespaces.
    #[arg(long)]
    pub namespace: Option<String>,
    /// Snapshot output format.
    #[arg(long, value_enum, default_value = "json")]
    pub format: GraphExportFormat,
    /// File path to write output instead of stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Traverse relationships from an entity with default depth (2)\n  \
    sqlite-graphrag graph traverse --from acme-corp\n\n  \
    # Increase traversal depth to 3 hops\n  \
    sqlite-graphrag graph traverse --from acme-corp --depth 3\n\n  \
    # Traverse within a specific namespace\n  \
    sqlite-graphrag graph traverse --from acme-corp --namespace project-x\n\n  \
NOTES:\n  \
    Output is always JSON. The `hops` array contains each reachable entity\n  \
    with its relation, direction (inbound/outbound), weight, and depth level.")]
pub struct GraphTraverseArgs {
    /// Root entity name for the traversal.
    #[arg(long)]
    pub from: String,
    /// Maximum traversal depth.
    #[arg(long, default_value_t = 2u32)]
    pub depth: u32,
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long, value_enum, default_value = "json")]
    pub format: GraphTraverseFormat,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Show stats for all namespaces (human-readable text)\n  \
    sqlite-graphrag graph stats --format text\n\n  \
    # Show stats as structured JSON\n  \
    sqlite-graphrag graph stats --format json\n\n  \
    # Show stats for a specific namespace\n  \
    sqlite-graphrag graph stats --namespace project-x --format text\n\n  \
NOTES:\n  \
    Reports node_count, edge_count, avg_degree, and max_degree.\n  \
    Default format is JSON. Use `--format text` for a compact single-line summary.")]
pub struct GraphStatsArgs {
    #[arg(long)]
    pub namespace: Option<String>,
    /// Output format for the stats response.
    #[arg(long, value_enum, default_value = "json")]
    pub format: GraphStatsFormat,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

/// Field to sort entities by in `graph entities`.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum EntitySortField {
    /// Sort alphabetically by entity name.
    Name,
    /// Sort by degree (total number of relationships, descending by default).
    Degree,
    /// Sort by entity creation timestamp.
    CreatedAt,
}

/// Sort direction for `graph entities`.
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # List all entities (default limit applies)\n  \
    sqlite-graphrag graph entities\n\n  \
    # Filter by entity type\n  \
    sqlite-graphrag graph entities --entity-type person\n\n  \
    # Filter by namespace and type\n  \
    sqlite-graphrag graph entities --namespace project-x --entity-type concept\n\n  \
    # Paginate results (skip first 20, return next 10)\n  \
    sqlite-graphrag graph entities --offset 20 --limit 10\n\n  \
    # Sort by degree descending (most connected first)\n  \
    sqlite-graphrag graph entities --sort-by degree --order desc\n\n  \
    # Sort by creation date ascending\n  \
    sqlite-graphrag graph entities --sort-by created-at --order asc\n\n  \
NOTES:\n  \
    Output is always JSON with `entities`, `total_count`, `limit`, and `offset` fields.\n  \
    Entity types are strings extracted by GLiNER NER (e.g. `person`, `organization`, `location`).")]
pub struct GraphEntitiesArgs {
    #[arg(long)]
    pub namespace: Option<String>,
    /// Filter by entity type (one of the 13 canonical types).
    #[arg(long, value_enum)]
    pub entity_type: Option<EntityType>,
    /// Maximum number of results to return.
    #[arg(long, default_value_t = crate::constants::K_GRAPH_ENTITIES_DEFAULT_LIMIT)]
    pub limit: usize,
    /// Number of results to skip for pagination.
    #[arg(long, default_value_t = 0usize)]
    pub offset: usize,
    /// Sort entities by this field. When omitted, the default order is by name ascending.
    #[arg(long, value_enum, help = "Sort entities by field")]
    pub sort_by: Option<EntitySortField>,
    /// Sort direction: `asc` (default) or `desc`.
    #[arg(long, value_enum, default_value_t = SortOrder::Asc, help = "Sort order")]
    pub order: SortOrder,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize, Clone)]
struct NodeOut {
    id: i64,
    name: String,
    namespace: String,
    /// Deprecated alias of `type` kept for backward-compat with pre-v1.0.35 clients.
    /// New consumers MUST read `type` instead. Will be removed in a future major release.
    kind: String,
    /// Canonical entity classification (organization, concept, person, etc.).
    /// Mirrors `kind` while the deprecation window is active.
    #[serde(rename = "type")]
    r#type: String,
}

#[derive(Serialize)]
struct EdgeOut {
    from: String,
    to: String,
    relation: String,
    weight: f64,
}

#[derive(Serialize)]
struct GraphSnapshot {
    nodes: Vec<NodeOut>,
    entities: Vec<NodeOut>,
    edges: Vec<EdgeOut>,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct TraverseHop {
    entity: String,
    relation: String,
    direction: String,
    weight: f64,
    depth: u32,
}

#[derive(Serialize)]
struct GraphTraverseResponse {
    from: String,
    namespace: String,
    depth: u32,
    hops: Vec<TraverseHop>,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct GraphStatsResponse {
    namespace: Option<String>,
    node_count: i64,
    edge_count: i64,
    avg_degree: f64,
    max_degree: i64,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct EntityItem {
    id: i64,
    name: String,
    entity_type: String,
    namespace: String,
    created_at: String,
    /// Total number of relationships (inbound + outbound) for this entity.
    degree: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

#[derive(Serialize)]
struct GraphEntitiesResponse {
    entities: Vec<EntityItem>,
    total_count: i64,
    limit: usize,
    offset: usize,
    namespace: Option<String>,
    elapsed_ms: u64,
}

pub fn run(args: GraphArgs) -> Result<(), AppError> {
    match args.subcommand {
        None => run_entities_snapshot(
            args.db.as_deref(),
            args.namespace.as_deref(),
            args.format,
            args.json,
            args.output.as_deref(),
        ),
        Some(GraphSubcommand::Traverse(a)) => run_traverse(a),
        Some(GraphSubcommand::Stats(a)) => run_stats(a),
        Some(GraphSubcommand::Entities(a)) => run_entities(a),
    }
}

fn run_entities_snapshot(
    db: Option<&str>,
    namespace: Option<&str>,
    format: GraphExportFormat,
    json: bool,
    output_path: Option<&std::path::Path>,
) -> Result<(), AppError> {
    let inicio = Instant::now();
    let paths = AppPaths::resolve(db)?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let conn = open_ro(&paths.db)?;

    let nodes_raw = entities::list_entities(&conn, namespace)?;
    let edges_raw = entities::list_relationships_by_namespace(&conn, namespace)?;

    let id_to_name: HashMap<i64, String> =
        nodes_raw.iter().map(|n| (n.id, n.name.clone())).collect();

    let nodes: Vec<NodeOut> = nodes_raw
        .into_iter()
        .map(|n| NodeOut {
            id: n.id,
            name: n.name,
            namespace: n.namespace,
            r#type: n.kind.clone(),
            kind: n.kind,
        })
        .collect();

    let mut edges: Vec<EdgeOut> = Vec::with_capacity(edges_raw.len());
    let mut orphan_edges: usize = 0;
    for r in edges_raw {
        let from = match id_to_name.get(&r.source_id) {
            Some(n) => n.clone(),
            None => {
                orphan_edges += 1;
                tracing::warn!(target: "graph_export", source_id = r.source_id, relation = %r.relation, "edge skipped: source entity not found in id_to_name map");
                continue;
            }
        };
        let to = match id_to_name.get(&r.target_id) {
            Some(n) => n.clone(),
            None => {
                orphan_edges += 1;
                tracing::warn!(target: "graph_export", target_id = r.target_id, relation = %r.relation, "edge skipped: target entity not found in id_to_name map");
                continue;
            }
        };
        edges.push(EdgeOut {
            from,
            to,
            relation: r.relation,
            weight: r.weight,
        });
    }
    if orphan_edges > 0 {
        tracing::warn!(target: "graph_export",
            count = orphan_edges,
            "edges skipped due to orphaned entity references"
        );
    }

    let effective_format = if json {
        GraphExportFormat::Json
    } else {
        format
    };

    if effective_format == GraphExportFormat::Ndjson {
        let elapsed_ms = inicio.elapsed().as_millis() as u64;
        render_ndjson_streaming(&nodes, &edges, elapsed_ms, output_path)?;
        return Ok(());
    }

    let rendered = match effective_format {
        GraphExportFormat::Json => {
            let entities = nodes.clone();
            render_json(&GraphSnapshot {
                nodes,
                entities,
                edges,
                elapsed_ms: inicio.elapsed().as_millis() as u64,
            })?
        }
        GraphExportFormat::Dot => render_dot(&nodes, &edges),
        GraphExportFormat::Mermaid => render_mermaid(&nodes, &edges),
        GraphExportFormat::Ndjson => unreachable!("ndjson handled above"),
    };

    if let Some(path) = output_path.filter(|_| !json) {
        fs::write(path, &rendered)?;
        output::emit_progress(&format!("wrote {}", path.display()));
    } else {
        output::emit_text(&rendered);
    }

    Ok(())
}

fn run_traverse(args: GraphTraverseArgs) -> Result<(), AppError> {
    let inicio = Instant::now();
    let _ = args.format;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let conn = open_ro(&paths.db)?;
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;

    let from_id = entities::find_entity_id(&conn, &namespace, &args.from)?
        .ok_or_else(|| AppError::NotFound(format!("entity '{}' not found", args.from)))?;

    let all_rels = entities::list_relationships_by_namespace(&conn, Some(&namespace))?;
    let all_entities = entities::list_entities(&conn, Some(&namespace))?;
    let id_to_name: HashMap<i64, String> = all_entities
        .iter()
        .map(|e| (e.id, e.name.clone()))
        .collect();

    let mut hops: Vec<TraverseHop> = Vec::with_capacity(16);
    let mut visited: std::collections::HashSet<i64> =
        std::collections::HashSet::with_capacity(args.depth as usize * 10);
    let mut frontier: Vec<(i64, u32)> = vec![(from_id, 0)];

    while let Some((current_id, current_depth)) = frontier.pop() {
        if current_depth >= args.depth || visited.contains(&current_id) {
            continue;
        }
        visited.insert(current_id);

        for rel in &all_rels {
            if rel.source_id == current_id {
                if let Some(target_name) = id_to_name.get(&rel.target_id) {
                    hops.push(TraverseHop {
                        entity: target_name.clone(),
                        relation: rel.relation.clone(),
                        direction: "outbound".to_string(),
                        weight: rel.weight,
                        depth: current_depth + 1,
                    });
                    frontier.push((rel.target_id, current_depth + 1));
                }
            } else if rel.target_id == current_id {
                if let Some(source_name) = id_to_name.get(&rel.source_id) {
                    hops.push(TraverseHop {
                        entity: source_name.clone(),
                        relation: rel.relation.clone(),
                        direction: "inbound".to_string(),
                        weight: rel.weight,
                        depth: current_depth + 1,
                    });
                    frontier.push((rel.source_id, current_depth + 1));
                }
            }
        }
    }

    output::emit_json(&GraphTraverseResponse {
        from: args.from,
        namespace,
        depth: args.depth,
        hops,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

fn run_stats(args: GraphStatsArgs) -> Result<(), AppError> {
    let inicio = Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let conn = open_ro(&paths.db)?;
    let ns = args.namespace.as_deref();

    let node_count: i64 = if let Some(n) = ns {
        conn.query_row(
            "SELECT COUNT(*) FROM entities WHERE namespace = ?1",
            rusqlite::params![n],
            |r| r.get(0),
        )?
    } else {
        conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))?
    };

    let edge_count: i64 = if let Some(n) = ns {
        conn.query_row(
            "SELECT COUNT(*) FROM relationships r
             JOIN entities s ON s.id = r.source_id
             WHERE s.namespace = ?1",
            rusqlite::params![n],
            |r| r.get(0),
        )?
    } else {
        conn.query_row("SELECT COUNT(*) FROM relationships", [], |r| r.get(0))?
    };

    let max_degree: i64 = if let Some(n) = ns {
        conn.query_row(
            "SELECT COALESCE(MAX(degree), 0) FROM entities WHERE namespace = ?1",
            rusqlite::params![n],
            |r| r.get(0),
        )?
    } else {
        conn.query_row("SELECT COALESCE(MAX(degree), 0) FROM entities", [], |r| {
            r.get(0)
        })?
    };

    // avg_degree = 2 * edge_count / node_count (each edge contributes 2 to total degree sum).
    let avg_degree = if node_count > 0 {
        2.0 * (edge_count as f64) / (node_count as f64)
    } else {
        0.0
    };

    let resp = GraphStatsResponse {
        namespace: args.namespace,
        node_count,
        edge_count,
        avg_degree,
        max_degree,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    };

    let effective_format = if args.json {
        GraphStatsFormat::Json
    } else {
        args.format
    };

    match effective_format {
        GraphStatsFormat::Json => output::emit_json(&resp)?,
        GraphStatsFormat::Text => {
            output::emit_text(&format!(
                "nodes={} edges={} avg_degree={:.2} max_degree={} namespace={}",
                resp.node_count,
                resp.edge_count,
                resp.avg_degree,
                resp.max_degree,
                resp.namespace.as_deref().unwrap_or("all"),
            ));
        }
    }

    Ok(())
}

/// Builds the `ORDER BY` clause fragment from sort options.
///
/// Returns a static SQL fragment such as `ORDER BY e.name ASC`.
fn build_order_by(sort_by: Option<EntitySortField>, order: SortOrder) -> &'static str {
    // The combinations are enumerated as static strings to avoid
    // format!() allocations in the hot path and satisfy the borrow checker
    // when the string is used inside conn.prepare().
    match (sort_by, order) {
        (None, SortOrder::Asc) | (Some(EntitySortField::Name), SortOrder::Asc) => {
            "ORDER BY e.name ASC"
        }
        (Some(EntitySortField::Name), SortOrder::Desc) => "ORDER BY e.name DESC",
        (Some(EntitySortField::Degree), SortOrder::Asc) => "ORDER BY degree ASC",
        (Some(EntitySortField::Degree), SortOrder::Desc) => "ORDER BY degree DESC",
        (Some(EntitySortField::CreatedAt), SortOrder::Asc) => "ORDER BY e.created_at ASC",
        (Some(EntitySortField::CreatedAt), SortOrder::Desc) => "ORDER BY e.created_at DESC",
        // Fallback: None/Desc → sort by name desc (consistent with dir variable).
        (None, SortOrder::Desc) => "ORDER BY e.name DESC",
    }
}

fn run_entities(args: GraphEntitiesArgs) -> Result<(), AppError> {
    let inicio = Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let conn = open_ro(&paths.db)?;

    let row_to_item = |r: &rusqlite::Row<'_>| -> rusqlite::Result<EntityItem> {
        let ts: i64 = r.get(4)?;
        let created_at = chrono::DateTime::from_timestamp(ts, 0)
            .unwrap_or_default()
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        Ok(EntityItem {
            id: r.get(0)?,
            name: r.get(1)?,
            entity_type: r.get(2)?,
            namespace: r.get(3)?,
            created_at,
            degree: r.get(5)?,
            description: r.get(6)?,
        })
    };

    let limit_i = args.limit as i64;
    let offset_i = args.offset as i64;
    let order_clause = build_order_by(args.sort_by, args.order);

    let base_select = "SELECT e.id, e.name, COALESCE(e.type, ''), e.namespace, e.created_at,
                        (SELECT COUNT(*) FROM relationships r
                         WHERE r.source_id = e.id OR r.target_id = e.id) AS degree,
                        e.description
                 FROM entities e";

    let (total_count, items) = match (
        args.namespace.as_deref(),
        args.entity_type.map(|et| et.as_str()),
    ) {
        (Some(ns), Some(et)) => {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM entities WHERE namespace = ?1 AND type = ?2",
                rusqlite::params![ns, et],
                |r| r.get(0),
            )?;
            let sql = format!(
                "{base_select} WHERE e.namespace = ?1 AND e.type = ?2 {order_clause} LIMIT ?3 OFFSET ?4"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(rusqlite::params![ns, et, limit_i, offset_i], row_to_item)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            (count, rows)
        }
        (Some(ns), None) => {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM entities WHERE namespace = ?1",
                rusqlite::params![ns],
                |r| r.get(0),
            )?;
            let sql =
                format!("{base_select} WHERE e.namespace = ?1 {order_clause} LIMIT ?2 OFFSET ?3");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(rusqlite::params![ns, limit_i, offset_i], row_to_item)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            (count, rows)
        }
        (None, Some(et)) => {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM entities WHERE type = ?1",
                rusqlite::params![et],
                |r| r.get(0),
            )?;
            let sql = format!("{base_select} WHERE e.type = ?1 {order_clause} LIMIT ?2 OFFSET ?3");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(rusqlite::params![et, limit_i, offset_i], row_to_item)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            (count, rows)
        }
        (None, None) => {
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))?;
            let sql = format!("{base_select} {order_clause} LIMIT ?1 OFFSET ?2");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(rusqlite::params![limit_i, offset_i], row_to_item)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            (count, rows)
        }
    };

    output::emit_json(&GraphEntitiesResponse {
        entities: items,
        total_count,
        limit: args.limit,
        offset: args.offset,
        namespace: args.namespace,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })
}

fn render_json(snapshot: &GraphSnapshot) -> Result<String, AppError> {
    Ok(serde_json::to_string_pretty(snapshot)?)
}

/// Streams the graph as NDJSON: one object per node, one per edge, then a summary.
///
/// Each line is flushed immediately so consumers can process incrementally.
/// When `output_path` is `Some`, lines are written to the file; otherwise to stdout.
fn render_ndjson_streaming(
    nodes: &[NodeOut],
    edges: &[EdgeOut],
    elapsed_ms: u64,
    output_path: Option<&std::path::Path>,
) -> Result<(), AppError> {
    #[derive(serde::Serialize)]
    struct NdjsonNode<'a> {
        kind: &'static str,
        id: i64,
        name: &'a str,
        namespace: &'a str,
        #[serde(rename = "type")]
        r#type: &'a str,
    }
    #[derive(serde::Serialize)]
    struct NdjsonEdge<'a> {
        kind: &'static str,
        from: &'a str,
        to: &'a str,
        relation: &'a str,
        weight: f64,
    }
    #[derive(serde::Serialize)]
    struct NdjsonSummary {
        kind: &'static str,
        nodes: usize,
        edges: usize,
        elapsed_ms: u64,
    }

    use std::io::Write as IoWrite;

    let mut buf: Vec<u8> = Vec::with_capacity(4096);

    let emit_line =
        |buf: &mut Vec<u8>, line: &str, path: Option<&std::path::Path>| -> Result<(), AppError> {
            buf.clear();
            buf.extend_from_slice(line.as_bytes());
            buf.push(b'\n');
            if let Some(p) = path {
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(p)
                    .map_err(AppError::Io)?;
                f.write_all(buf).map_err(AppError::Io)?;
            } else {
                output::emit_text(line);
            }
            Ok(())
        };

    // Truncate the output file once before starting (avoids re-opening with append for every line).
    if let Some(p) = output_path {
        fs::write(p, b"")?;
    }

    for node in nodes {
        let obj = NdjsonNode {
            kind: "node",
            id: node.id,
            name: &node.name,
            namespace: &node.namespace,
            r#type: &node.r#type,
        };
        let line = serde_json::to_string(&obj)?;
        emit_line(&mut buf, &line, output_path)?;
    }

    for edge in edges {
        let obj = NdjsonEdge {
            kind: "edge",
            from: &edge.from,
            to: &edge.to,
            relation: &edge.relation,
            weight: edge.weight,
        };
        let line = serde_json::to_string(&obj)?;
        emit_line(&mut buf, &line, output_path)?;
    }

    let summary = NdjsonSummary {
        kind: "summary",
        nodes: nodes.len(),
        edges: edges.len(),
        elapsed_ms,
    };
    let line = serde_json::to_string(&summary)?;
    emit_line(&mut buf, &line, output_path)?;

    Ok(())
}

fn sanitize_dot_id(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn render_dot(nodes: &[NodeOut], edges: &[EdgeOut]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(nodes.len() * 80 + edges.len() * 60 + 300);
    out.push_str("digraph sqlite_graphrag {\n");
    out.push_str("  graph [bgcolor=\"white\", fontname=\"Helvetica Neue\", fontsize=12, rankdir=LR, nodesep=0.8, ranksep=1.2];\n");
    out.push_str("  node [shape=box, style=\"filled,rounded\", fillcolor=\"#F2F2F7\", fontname=\"Helvetica Neue\", fontsize=11, color=\"#C7C7CC\"];\n");
    out.push_str("  edge [fontname=\"Helvetica Neue\", fontsize=9, color=\"#8E8E93\"];\n");
    for node in nodes {
        let node_id = sanitize_dot_id(&node.name);
        let escaped = node.name.replace('"', "\\\"");
        let _ = writeln!(out, "  {node_id} [label=\"{escaped}\"];");
    }
    for edge in edges {
        let from = sanitize_dot_id(&edge.from);
        let to = sanitize_dot_id(&edge.to);
        let label = edge.relation.replace('"', "\\\"");
        let _ = writeln!(out, "  {from} -> {to} [label=\"{label}\"];");
    }
    out.push_str("}\n");
    out
}

fn sanitize_mermaid_id(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn render_mermaid(nodes: &[NodeOut], edges: &[EdgeOut]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(nodes.len() * 50 + edges.len() * 40 + 200);
    out.push_str("%%{init: {'theme': 'neutral', 'themeVariables': {'primaryColor': '#F2F2F7', 'primaryTextColor': '#1C1C1E', 'primaryBorderColor': '#C7C7CC', 'lineColor': '#8E8E93'}}}%%\n");
    out.push_str("graph LR\n");
    for node in nodes {
        let id = sanitize_mermaid_id(&node.name);
        let escaped = node.name.replace('"', "\\\"");
        let _ = writeln!(out, "  {id}[\"{escaped}\"]");
    }
    for edge in edges {
        let from = sanitize_mermaid_id(&edge.from);
        let to = sanitize_mermaid_id(&edge.to);
        let label = edge.relation.replace('|', "\\|");
        let _ = writeln!(out, "  {from} -->|{label}| {to}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Commands};
    use clap::Parser;

    fn make_node(kind: &str) -> NodeOut {
        NodeOut {
            id: 1,
            name: "test-entity".to_string(),
            namespace: "default".to_string(),
            kind: kind.to_string(),
            r#type: kind.to_string(),
        }
    }

    #[test]
    fn node_out_type_duplicates_kind() {
        let node = make_node("agent");
        let json = serde_json::to_value(&node).expect("serialization must work");
        assert_eq!(json["kind"], json["type"]);
        assert_eq!(json["kind"], "agent");
        assert_eq!(json["type"], "agent");
    }

    #[test]
    fn node_out_serializes_all_fields() {
        let node = make_node("document");
        let json = serde_json::to_value(&node).expect("serialization must work");
        assert!(json.get("id").is_some());
        assert!(json.get("name").is_some());
        assert!(json.get("namespace").is_some());
        assert!(json.get("kind").is_some());
        assert!(json.get("type").is_some());
    }

    #[test]
    fn graph_snapshot_serializes_nodes_with_type() {
        let node = make_node("concept");
        let entities = vec![make_node("concept")];
        let snapshot = GraphSnapshot {
            nodes: vec![node],
            entities,
            edges: vec![],
            elapsed_ms: 0,
        };
        let json_str = render_json(&snapshot).expect("rendering must work");
        let json: serde_json::Value = serde_json::from_str(&json_str).expect("valid json");
        let first_node = &json["nodes"][0];
        assert_eq!(first_node["kind"], first_node["type"]);
        assert_eq!(first_node["type"], "concept");
    }

    #[test]
    fn graph_traverse_response_serializes_correctly() {
        let resp = GraphTraverseResponse {
            from: "entity-a".to_string(),
            namespace: "global".to_string(),
            depth: 2,
            hops: vec![TraverseHop {
                entity: "entity-b".to_string(),
                relation: "uses".to_string(),
                direction: "outbound".to_string(),
                weight: 1.0,
                depth: 1,
            }],
            elapsed_ms: 5,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["from"], "entity-a");
        assert_eq!(json["depth"], 2);
        assert!(json["hops"].is_array());
        assert_eq!(json["hops"][0]["direction"], "outbound");
    }

    #[test]
    fn graph_stats_response_serializes_correctly() {
        let resp = GraphStatsResponse {
            namespace: Some("global".to_string()),
            node_count: 10,
            edge_count: 15,
            avg_degree: 3.0,
            max_degree: 7,
            elapsed_ms: 2,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["node_count"], 10);
        assert_eq!(json["edge_count"], 15);
        assert_eq!(json["avg_degree"], 3.0);
        assert_eq!(json["max_degree"], 7);
    }

    fn compute_avg_degree(node_count: i64, edge_count: i64) -> f64 {
        if node_count > 0 {
            2.0 * (edge_count as f64) / (node_count as f64)
        } else {
            0.0
        }
    }

    #[test]
    fn avg_degree_is_zero_when_no_nodes() {
        assert_eq!(compute_avg_degree(0, 0), 0.0);
    }

    #[test]
    fn avg_degree_is_zero_when_nodes_but_no_edges() {
        // Reproduces L1 bug: previously returned 1.0 instead of 0.0.
        assert_eq!(compute_avg_degree(2, 0), 0.0);
    }

    #[test]
    fn avg_degree_is_two_when_triangle() {
        // 3 nodes, 3 edges: 2 * 3 / 3 = 2.0
        assert_eq!(compute_avg_degree(3, 3), 2.0);
    }

    #[test]
    fn graph_entities_response_serializes_required_fields() {
        let resp = GraphEntitiesResponse {
            entities: vec![EntityItem {
                id: 1,
                name: "claude-code".to_string(),
                entity_type: "agent".to_string(),
                namespace: "global".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                degree: 0,
                description: None,
            }],
            total_count: 1,
            limit: 50,
            offset: 0,
            namespace: Some("global".to_string()),
            elapsed_ms: 3,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json["entities"].is_array());
        assert_eq!(json["entities"][0]["name"], "claude-code");
        assert_eq!(json["entities"][0]["entity_type"], "agent");
        assert_eq!(json["total_count"], 1);
        assert_eq!(json["limit"], 50);
        assert_eq!(json["offset"], 0);
        assert_eq!(json["namespace"], "global");
    }

    #[test]
    fn entity_item_serializes_all_fields() {
        let item = EntityItem {
            id: 42,
            name: "test-entity".to_string(),
            entity_type: "concept".to_string(),
            namespace: "project-a".to_string(),
            created_at: "2026-04-19T12:00:00Z".to_string(),
            degree: 3,
            description: Some("test description".to_string()),
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["id"], 42);
        assert_eq!(json["name"], "test-entity");
        assert_eq!(json["entity_type"], "concept");
        assert_eq!(json["namespace"], "project-a");
        assert_eq!(json["created_at"], "2026-04-19T12:00:00Z");
    }

    #[test]
    fn entity_item_entity_type_is_never_null() {
        // P2-C: entity_type must never be null, even when DB column is empty.
        let item = EntityItem {
            id: 1,
            name: "sem-tipo".to_string(),
            entity_type: String::new(),
            namespace: "ns".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            degree: 0,
            description: None,
        };
        let json = serde_json::to_value(&item).unwrap();
        assert!(
            !json["entity_type"].is_null(),
            "entity_type must not be null"
        );
        assert!(json["entity_type"].is_string());
    }

    #[test]
    fn graph_traverse_cli_rejects_format_dot() {
        let parsed = Cli::try_parse_from([
            "sqlite-graphrag",
            "graph",
            "traverse",
            "--from",
            "AuthDecision",
            "--format",
            "dot",
        ]);
        assert!(parsed.is_err(), "graph traverse must reject format=dot");
    }

    #[test]
    fn graph_stats_cli_accepts_format_text() {
        let parsed = Cli::try_parse_from(["sqlite-graphrag", "graph", "stats", "--format", "text"])
            .expect("graph stats --format text must be accepted");

        match parsed.command {
            Some(Commands::Graph(args)) => match args.subcommand {
                Some(GraphSubcommand::Stats(stats)) => {
                    assert_eq!(stats.format, GraphStatsFormat::Text);
                }
                _ => unreachable!("unexpected subcommand"),
            },
            _ => unreachable!("unexpected command"),
        }
    }

    #[test]
    fn graph_stats_cli_rejects_format_mermaid() {
        let parsed =
            Cli::try_parse_from(["sqlite-graphrag", "graph", "stats", "--format", "mermaid"]);
        assert!(parsed.is_err(), "graph stats must reject format=mermaid");
    }

    #[test]
    fn graph_entities_response_has_no_items_key() {
        let resp = GraphEntitiesResponse {
            entities: vec![],
            total_count: 0,
            limit: 50,
            offset: 0,
            namespace: None,
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(
            json.get("items").is_none(),
            "legacy 'items' key must not appear"
        );
        assert!(
            json.get("entities").is_some(),
            "'entities' key must be present"
        );
    }

    #[test]
    fn build_order_by_defaults_to_name_asc() {
        let clause = build_order_by(None, SortOrder::Asc);
        assert_eq!(clause, "ORDER BY e.name ASC");
    }

    #[test]
    fn build_order_by_name_desc() {
        let clause = build_order_by(Some(EntitySortField::Name), SortOrder::Desc);
        assert_eq!(clause, "ORDER BY e.name DESC");
    }

    #[test]
    fn build_order_by_degree_desc() {
        let clause = build_order_by(Some(EntitySortField::Degree), SortOrder::Desc);
        assert_eq!(clause, "ORDER BY degree DESC");
    }

    #[test]
    fn build_order_by_degree_asc() {
        let clause = build_order_by(Some(EntitySortField::Degree), SortOrder::Asc);
        assert_eq!(clause, "ORDER BY degree ASC");
    }

    #[test]
    fn build_order_by_created_at_asc() {
        let clause = build_order_by(Some(EntitySortField::CreatedAt), SortOrder::Asc);
        assert_eq!(clause, "ORDER BY e.created_at ASC");
    }

    #[test]
    fn build_order_by_created_at_desc() {
        let clause = build_order_by(Some(EntitySortField::CreatedAt), SortOrder::Desc);
        assert_eq!(clause, "ORDER BY e.created_at DESC");
    }

    #[test]
    fn graph_entities_cli_accepts_sort_by_degree_desc() {
        let parsed = Cli::try_parse_from([
            "sqlite-graphrag",
            "graph",
            "entities",
            "--sort-by",
            "degree",
            "--order",
            "desc",
        ])
        .expect("graph entities --sort-by degree --order desc must parse");
        match parsed.command {
            Some(Commands::Graph(args)) => match args.subcommand {
                Some(GraphSubcommand::Entities(e)) => {
                    assert!(matches!(e.sort_by, Some(EntitySortField::Degree)));
                    assert!(matches!(e.order, SortOrder::Desc));
                }
                _ => unreachable!("unexpected subcommand"),
            },
            _ => unreachable!("unexpected command"),
        }
    }

    #[test]
    fn graph_entities_cli_accepts_sort_by_created_at_asc() {
        let parsed = Cli::try_parse_from([
            "sqlite-graphrag",
            "graph",
            "entities",
            "--sort-by",
            "created-at",
        ])
        .expect("graph entities --sort-by created-at must parse");
        match parsed.command {
            Some(Commands::Graph(args)) => match args.subcommand {
                Some(GraphSubcommand::Entities(e)) => {
                    assert!(matches!(e.sort_by, Some(EntitySortField::CreatedAt)));
                    assert!(matches!(e.order, SortOrder::Asc));
                }
                _ => unreachable!("unexpected subcommand"),
            },
            _ => unreachable!("unexpected command"),
        }
    }

    #[test]
    fn graph_entities_cli_defaults_to_no_sort_by() {
        let parsed = Cli::try_parse_from(["sqlite-graphrag", "graph", "entities"])
            .expect("graph entities must parse without sort flags");
        match parsed.command {
            Some(Commands::Graph(args)) => match args.subcommand {
                Some(GraphSubcommand::Entities(e)) => {
                    assert!(e.sort_by.is_none(), "sort_by must default to None");
                    assert!(
                        matches!(e.order, SortOrder::Asc),
                        "order must default to Asc"
                    );
                }
                _ => unreachable!("unexpected subcommand"),
            },
            _ => unreachable!("unexpected command"),
        }
    }
}
