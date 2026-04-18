use crate::cli::GraphExportFormat;
use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use crate::storage::entities;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(clap::Args)]
pub struct GraphArgs {
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long, value_enum, default_value = "json")]
    pub format: GraphExportFormat,
    #[arg(long)]
    pub output: Option<PathBuf>,
    #[arg(long, env = "NEUROGRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct NodeOut {
    id: i64,
    name: String,
    namespace: String,
    kind: String,
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
}

pub fn run(args: GraphArgs) -> Result<(), AppError> {
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if !paths.db.exists() {
        return Err(AppError::NotFound(format!(
            "database not found at {}. Run 'neurographrag init' first.",
            paths.db.display()
        )));
    }

    let conn = open_ro(&paths.db)?;

    let nodes_raw = entities::list_entities(&conn, args.namespace.as_deref())?;
    let edges_raw = entities::list_relationships_by_namespace(&conn, args.namespace.as_deref())?;

    let id_to_name: HashMap<i64, String> =
        nodes_raw.iter().map(|n| (n.id, n.name.clone())).collect();

    let nodes: Vec<NodeOut> = nodes_raw
        .into_iter()
        .map(|n| NodeOut {
            id: n.id,
            name: n.name,
            namespace: n.namespace,
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

    let rendered = match args.format {
        GraphExportFormat::Json => render_json(&GraphSnapshot { nodes, edges })?,
        GraphExportFormat::Dot => render_dot(&nodes, &edges),
        GraphExportFormat::Mermaid => render_mermaid(&nodes, &edges),
    };

    if let Some(path) = &args.output {
        fs::write(path, &rendered)?;
        output::emit_progress(&format!("wrote {}", path.display()));
    } else {
        // `rendered` already contains the final payload; print via output layer so the
        // JSON format stays byte-identical to other commands (pretty-printed).
        output::emit_text(&rendered);
    }

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
