use crate::cli::GraphExportFormat;
use crate::errors::AppError;
use crate::i18n::erros;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use crate::storage::entities;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

/// Sub-subcomandos opcionais. Quando ausente, o comportamento padrão exporta
/// o snapshot completo de entidades (compatível com versões anteriores).
#[derive(clap::Subcommand)]
pub enum GraphSubcommand {
    /// Traverse relationships from a starting entity using BFS
    Traverse(GraphTraverseArgs),
    /// Show graph statistics (node/edge counts, degree distribution)
    Stats(GraphStatsArgs),
}

#[derive(clap::Args)]
pub struct GraphArgs {
    /// Subcomando opcional; sem subcomando exporta snapshot de entidades.
    #[command(subcommand)]
    pub subcommand: Option<GraphSubcommand>,
    /// Filtra por namespace (padrão: todos).
    #[arg(long)]
    pub namespace: Option<String>,
    /// Formato de saída do snapshot.
    #[arg(long, value_enum, default_value = "json")]
    pub format: GraphExportFormat,
    /// Caminho de arquivo para gravar a saída (em vez de stdout).
    #[arg(long)]
    pub output: Option<PathBuf>,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "NEUROGRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(clap::Args)]
pub struct GraphTraverseArgs {
    /// Nome da entidade de origem para a travessia
    #[arg(long)]
    pub from: String,
    /// Profundidade máxima de travessia (default: 2)
    #[arg(long, default_value_t = 2u32)]
    pub depth: u32,
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long, value_enum, default_value = "json")]
    pub format: GraphExportFormat,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "NEUROGRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(clap::Args)]
pub struct GraphStatsArgs {
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "NEUROGRAPHRAG_DB_PATH")]
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

pub fn run(args: GraphArgs) -> Result<(), AppError> {
    match args.subcommand {
        None => run_entities_snapshot(
            args.db.as_deref(),
            args.namespace.as_deref(),
            args.format,
            args.output.as_deref(),
        ),
        Some(GraphSubcommand::Traverse(a)) => run_traverse(a),
        Some(GraphSubcommand::Stats(a)) => run_stats(a),
    }
}

fn run_entities_snapshot(
    db: Option<&str>,
    namespace: Option<&str>,
    format: GraphExportFormat,
    output_path: Option<&std::path::Path>,
) -> Result<(), AppError> {
    let inicio = Instant::now();
    let paths = AppPaths::resolve(db)?;

    if !paths.db.exists() {
        return Err(AppError::NotFound(erros::banco_nao_encontrado(
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

    let rendered = match format {
        GraphExportFormat::Json => render_json(&GraphSnapshot {
            nodes,
            edges,
            elapsed_ms: inicio.elapsed().as_millis() as u64,
        })?,
        GraphExportFormat::Dot => render_dot(&nodes, &edges),
        GraphExportFormat::Mermaid => render_mermaid(&nodes, &edges),
    };

    if let Some(path) = output_path {
        fs::write(path, &rendered)?;
        output::emit_progress(&format!("wrote {}", path.display()));
    } else {
        output::emit_text(&rendered);
    }

    Ok(())
}

fn run_traverse(args: GraphTraverseArgs) -> Result<(), AppError> {
    let inicio = Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if !paths.db.exists() {
        return Err(AppError::NotFound(erros::banco_nao_encontrado(
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
        return Err(AppError::NotFound(erros::banco_nao_encontrado(
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

    output::emit_json(&GraphStatsResponse {
        namespace: args.namespace,
        node_count,
        edge_count,
        avg_degree,
        max_degree,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
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
    out.push_str("digraph neurographrag {\n");
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
mod testes {
    use super::*;

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
}
