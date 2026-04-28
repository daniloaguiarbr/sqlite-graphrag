//! Handler for the `graph-export` CLI subcommand.

use crate::cli::GraphExportFormat;
use crate::errors::AppError;
use crate::i18n::errors_msg;
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
    #[arg(long, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(clap::Args)]
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
    #[arg(long, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(clap::Args)]
pub struct GraphStatsArgs {
    #[arg(long)]
    pub namespace: Option<String>,
    /// Output format for the stats response.
    #[arg(long, value_enum, default_value = "json")]
    pub format: GraphStatsFormat,
    #[arg(long, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(clap::Args)]
pub struct GraphEntitiesArgs {
    #[arg(long)]
    pub namespace: Option<String>,
    /// Filter by entity type, for example `person`, `concept`, or `agent`.
    #[arg(long)]
    pub entity_type: Option<String>,
    /// Maximum number of results to return.
    #[arg(long, default_value_t = crate::constants::K_GRAPH_ENTITIES_DEFAULT_LIMIT)]
    pub limit: usize,
    /// Number of results to skip for pagination.
    #[arg(long, default_value_t = 0usize)]
    pub offset: usize,
    #[arg(long, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct NodeOut {
    id: i64,
    name: String,
    namespace: String,
    kind: String,
    /// Duplicata de `kind` para compatibilidade com docs que usam `type`.
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
}

#[derive(Serialize)]
struct GraphEntitiesResponse {
    items: Vec<EntityItem>,
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

    if !paths.db.exists() {
        return Err(AppError::NotFound(errors_msg::database_not_found(
            &paths.db.display().to_string(),
        )));
    }

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
    for r in edges_raw {
        let from = match id_to_name.get(&r.source_id) {
            Some(n) => n.clone(),
            None => continue,
        };
        let to = match id_to_name.get(&r.target_id) {
            Some(n) => n.clone(),
            None => continue,
        };
        edges.push(EdgeOut {
            from,
            to,
            relation: r.relation,
            weight: r.weight,
        });
    }

    let effective_format = if json {
        GraphExportFormat::Json
    } else {
        format
    };

    let rendered = match effective_format {
        GraphExportFormat::Json => render_json(&GraphSnapshot {
            nodes,
            edges,
            elapsed_ms: inicio.elapsed().as_millis() as u64,
        })?,
        GraphExportFormat::Dot => render_dot(&nodes, &edges),
        GraphExportFormat::Mermaid => render_mermaid(&nodes, &edges),
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

    if !paths.db.exists() {
        return Err(AppError::NotFound(errors_msg::database_not_found(
            &paths.db.display().to_string(),
        )));
    }

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

    let mut hops: Vec<TraverseHop> = Vec::new();
    let mut visited: std::collections::HashSet<i64> = std::collections::HashSet::new();
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

    if !paths.db.exists() {
        return Err(AppError::NotFound(errors_msg::database_not_found(
            &paths.db.display().to_string(),
        )));
    }

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

    let (avg_degree, max_degree): (f64, i64) = if let Some(n) = ns {
        conn.query_row(
            "SELECT COALESCE(AVG(degree), 0.0), COALESCE(MAX(degree), 0) FROM entities WHERE namespace = ?1",
            rusqlite::params![n],
            |r| Ok((r.get::<_, f64>(0)?, r.get::<_, i64>(1)?)),
        )?
    } else {
        conn.query_row(
            "SELECT COALESCE(AVG(degree), 0.0), COALESCE(MAX(degree), 0) FROM entities",
            [],
            |r| Ok((r.get::<_, f64>(0)?, r.get::<_, i64>(1)?)),
        )?
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

fn run_entities(args: GraphEntitiesArgs) -> Result<(), AppError> {
    let inicio = Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if !paths.db.exists() {
        return Err(AppError::NotFound(errors_msg::database_not_found(
            &paths.db.display().to_string(),
        )));
    }

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
        })
    };

    let limit_i = args.limit as i64;
    let offset_i = args.offset as i64;

    let (total_count, items) = match (args.namespace.as_deref(), args.entity_type.as_deref()) {
        (Some(ns), Some(et)) => {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM entities WHERE namespace = ?1 AND type = ?2",
                rusqlite::params![ns, et],
                |r| r.get(0),
            )?;
            let mut stmt = conn.prepare(
                "SELECT id, name, COALESCE(type, ''), namespace, created_at FROM entities
                 WHERE namespace = ?1 AND type = ?2
                 ORDER BY name ASC LIMIT ?3 OFFSET ?4",
            )?;
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
            let mut stmt = conn.prepare(
                "SELECT id, name, COALESCE(type, ''), namespace, created_at FROM entities
                 WHERE namespace = ?1
                 ORDER BY name ASC LIMIT ?2 OFFSET ?3",
            )?;
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
            let mut stmt = conn.prepare(
                "SELECT id, name, COALESCE(type, ''), namespace, created_at FROM entities
                 WHERE type = ?1
                 ORDER BY name ASC LIMIT ?2 OFFSET ?3",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![et, limit_i, offset_i], row_to_item)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            (count, rows)
        }
        (None, None) => {
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))?;
            let mut stmt = conn.prepare(
                "SELECT id, name, COALESCE(type, ''), namespace, created_at FROM entities
                 ORDER BY name ASC LIMIT ?1 OFFSET ?2",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![limit_i, offset_i], row_to_item)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            (count, rows)
        }
    };

    output::emit_json(&GraphEntitiesResponse {
        items,
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
    let mut out = String::new();
    out.push_str("digraph sqlite-graphrag {\n");
    for node in nodes {
        let node_id = sanitize_dot_id(&node.name);
        let escaped = node.name.replace('"', "\\\"");
        out.push_str(&format!("  {node_id} [label=\"{escaped}\"];\n"));
    }
    for edge in edges {
        let from = sanitize_dot_id(&edge.from);
        let to = sanitize_dot_id(&edge.to);
        let label = edge.relation.replace('"', "\\\"");
        out.push_str(&format!("  {from} -> {to} [label=\"{label}\"];\n"));
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
    let mut out = String::new();
    out.push_str("graph LR\n");
    for node in nodes {
        let id = sanitize_mermaid_id(&node.name);
        let escaped = node.name.replace('"', "\\\"");
        out.push_str(&format!("  {id}[\"{escaped}\"]\n"));
    }
    for edge in edges {
        let from = sanitize_mermaid_id(&edge.from);
        let to = sanitize_mermaid_id(&edge.to);
        let label = edge.relation.replace('|', "\\|");
        out.push_str(&format!("  {from} -->|{label}| {to}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Commands};
    use clap::Parser;

    fn cria_node(kind: &str) -> NodeOut {
        NodeOut {
            id: 1,
            name: "entidade-teste".to_string(),
            namespace: "default".to_string(),
            kind: kind.to_string(),
            r#type: kind.to_string(),
        }
    }

    #[test]
    fn node_out_type_duplica_kind() {
        let node = cria_node("agent");
        let json = serde_json::to_value(&node).expect("serialização deve funcionar");
        assert_eq!(json["kind"], json["type"]);
        assert_eq!(json["kind"], "agent");
        assert_eq!(json["type"], "agent");
    }

    #[test]
    fn node_out_serializa_todos_campos() {
        let node = cria_node("document");
        let json = serde_json::to_value(&node).expect("serialização deve funcionar");
        assert!(json.get("id").is_some());
        assert!(json.get("name").is_some());
        assert!(json.get("namespace").is_some());
        assert!(json.get("kind").is_some());
        assert!(json.get("type").is_some());
    }

    #[test]
    fn graph_snapshot_serializa_nodes_com_type() {
        let node = cria_node("concept");
        let snapshot = GraphSnapshot {
            nodes: vec![node],
            edges: vec![],
            elapsed_ms: 0,
        };
        let json_str = render_json(&snapshot).expect("renderização deve funcionar");
        let json: serde_json::Value = serde_json::from_str(&json_str).expect("json válido");
        let primeiro_node = &json["nodes"][0];
        assert_eq!(primeiro_node["kind"], primeiro_node["type"]);
        assert_eq!(primeiro_node["type"], "concept");
    }

    #[test]
    fn graph_traverse_response_serializa_corretamente() {
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
    fn graph_stats_response_serializa_corretamente() {
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

    #[test]
    fn graph_entities_response_serializa_campos_obrigatorios() {
        let resp = GraphEntitiesResponse {
            items: vec![EntityItem {
                id: 1,
                name: "claude-code".to_string(),
                entity_type: "agent".to_string(),
                namespace: "global".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
            }],
            total_count: 1,
            limit: 50,
            offset: 0,
            namespace: Some("global".to_string()),
            elapsed_ms: 3,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json["items"].is_array());
        assert_eq!(json["items"][0]["name"], "claude-code");
        assert_eq!(json["items"][0]["entity_type"], "agent");
        assert_eq!(json["total_count"], 1);
        assert_eq!(json["limit"], 50);
        assert_eq!(json["offset"], 0);
        assert_eq!(json["namespace"], "global");
    }

    #[test]
    fn entity_item_serializa_todos_campos() {
        let item = EntityItem {
            id: 42,
            name: "test-entity".to_string(),
            entity_type: "concept".to_string(),
            namespace: "project-a".to_string(),
            created_at: "2026-04-19T12:00:00Z".to_string(),
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["id"], 42);
        assert_eq!(json["name"], "test-entity");
        assert_eq!(json["entity_type"], "concept");
        assert_eq!(json["namespace"], "project-a");
        assert_eq!(json["created_at"], "2026-04-19T12:00:00Z");
    }

    #[test]
    fn entity_item_entity_type_nunca_e_null() {
        // P2-C: entity_type must never be null, even when DB column is empty.
        let item = EntityItem {
            id: 1,
            name: "sem-tipo".to_string(),
            entity_type: String::new(),
            namespace: "ns".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_value(&item).unwrap();
        assert!(
            !json["entity_type"].is_null(),
            "entity_type nao deve ser null"
        );
        assert!(json["entity_type"].is_string());
    }

    #[test]
    fn graph_traverse_cli_rejeita_format_dot() {
        let parsed = Cli::try_parse_from([
            "sqlite-graphrag",
            "graph",
            "traverse",
            "--from",
            "AuthDecision",
            "--format",
            "dot",
        ]);
        assert!(
            parsed.is_err(),
            "graph traverse nao deve aceitar format=dot"
        );
    }

    #[test]
    fn graph_stats_cli_aceita_format_text() {
        let parsed = Cli::try_parse_from(["sqlite-graphrag", "graph", "stats", "--format", "text"])
            .expect("graph stats --format text deve ser aceito");

        match parsed.command {
            Commands::Graph(args) => match args.subcommand {
                Some(GraphSubcommand::Stats(stats)) => {
                    assert_eq!(stats.format, GraphStatsFormat::Text);
                }
                _ => unreachable!("subcomando inesperado"),
            },
            _ => unreachable!("comando inesperado"),
        }
    }

    #[test]
    fn graph_stats_cli_rejeita_format_mermaid() {
        let parsed =
            Cli::try_parse_from(["sqlite-graphrag", "graph", "stats", "--format", "mermaid"]);
        assert!(
            parsed.is_err(),
            "graph stats nao deve aceitar format=mermaid"
        );
    }
}
