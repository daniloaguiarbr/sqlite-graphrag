//! Handler for the `list` CLI subcommand.

use crate::cli::MemoryType;
use crate::errors::AppError;
use crate::output::{self, OutputFormat};
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use crate::storage::memories;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # List up to 50 memories from the global namespace (default)\n  \
    sqlite-graphrag list\n\n  \
    # Filter by memory type and namespace\n  \
    sqlite-graphrag list --type project --namespace my-project\n\n  \
    # Paginate with limit and offset\n  \
    sqlite-graphrag list --limit 20 --offset 40\n\n  \
    # Include soft-deleted memories\n  \
    sqlite-graphrag list --include-deleted")]
pub struct ListArgs {
    /// Namespace to list memories from. Defaults to "global".
    #[arg(
        long,
        default_value = "global",
        help = "Namespace to list memories from"
    )]
    pub namespace: Option<String>,
    /// Filter by memory.type. Note: distinct from graph entity_type
    /// (project/tool/person/file/concept/incident/decision/memory/dashboard/issue_tracker/organization/location/date)
    /// used in --entities-file.
    #[arg(long, value_enum)]
    pub r#type: Option<MemoryType>,
    /// Maximum number of memories to return (default: 50).
    #[arg(
        long,
        default_value = "50",
        help = "Maximum number of memories to return"
    )]
    pub limit: usize,
    /// Number of memories to skip before returning results.
    #[arg(long, default_value = "0", help = "Number of memories to skip")]
    pub offset: usize,
    /// Output format: json (default), text, or markdown.
    #[arg(long, value_enum, default_value = "json", help = "Output format")]
    pub format: OutputFormat,
    /// Include soft-deleted memories in the listing (deleted_at IS NOT NULL).
    #[arg(long, default_value_t = false, help = "Include soft-deleted memories")]
    pub include_deleted: bool,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    /// Path to graphrag.sqlite (overrides SQLITE_GRAPHRAG_DB_PATH and default CWD).
    #[arg(
        long,
        env = "SQLITE_GRAPHRAG_DB_PATH",
        help = "Path to graphrag.sqlite"
    )]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct ListItem {
    id: i64,
    /// Semantic alias of `id` for the contract documented in SKILL.md and AGENT_PROTOCOL.md.
    memory_id: i64,
    name: String,
    namespace: String,
    #[serde(rename = "type")]
    memory_type: String,
    description: String,
    snippet: String,
    updated_at: i64,
    /// Timestamp RFC 3339 UTC paralelo a `updated_at`.
    updated_at_iso: String,
}

#[derive(Serialize)]
struct ListResponse {
    items: Vec<ListItem>,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: ListArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    // v1.0.22 P1: standardizes exit code 4 with a friendly message when the DB does not exist.
    crate::storage::connection::ensure_db_ready(&paths)?;
    let conn = open_ro(&paths.db)?;

    let memory_type_str = args.r#type.map(|t| t.as_str());
    let rows = memories::list(
        &conn,
        &namespace,
        memory_type_str,
        args.limit,
        args.offset,
        args.include_deleted,
    )?;

    let items: Vec<ListItem> = rows
        .into_iter()
        .map(|r| {
            let snippet: String = r.body.chars().take(200).collect();
            let updated_at_iso = crate::tz::epoch_to_iso(r.updated_at);
            ListItem {
                id: r.id,
                memory_id: r.id,
                name: r.name,
                namespace: r.namespace,
                memory_type: r.memory_type,
                description: r.description,
                snippet,
                updated_at: r.updated_at,
                updated_at_iso,
            }
        })
        .collect();

    match args.format {
        OutputFormat::Json => output::emit_json(&ListResponse {
            items,
            elapsed_ms: inicio.elapsed().as_millis() as u64,
        })?,
        OutputFormat::Text | OutputFormat::Markdown => {
            for item in &items {
                output::emit_text(&format!("{}: {}", item.name, item.snippet));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_response_serializes_items_and_elapsed_ms() {
        let resp = ListResponse {
            items: vec![ListItem {
                id: 1,
                memory_id: 1,
                name: "test-memory".to_string(),
                namespace: "global".to_string(),
                memory_type: "note".to_string(),
                description: "descricao de teste".to_string(),
                snippet: "corpo resumido".to_string(),
                updated_at: 1_745_000_000,
                updated_at_iso: "2025-04-19T00:00:00Z".to_string(),
            }],
            elapsed_ms: 7,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json["items"].is_array());
        assert_eq!(json["items"].as_array().unwrap().len(), 1);
        assert_eq!(json["items"][0]["name"], "test-memory");
        assert_eq!(json["items"][0]["memory_id"], 1);
        assert_eq!(json["elapsed_ms"], 7);
    }

    #[test]
    fn list_response_items_empty_serializes_empty_array() {
        let resp = ListResponse {
            items: vec![],
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json["items"].is_array());
        assert_eq!(json["items"].as_array().unwrap().len(), 0);
        assert_eq!(json["elapsed_ms"], 0);
    }

    #[test]
    fn list_item_memory_id_equals_id() {
        let item = ListItem {
            id: 42,
            memory_id: 42,
            name: "memory-alias".to_string(),
            namespace: "projeto".to_string(),
            memory_type: "fact".to_string(),
            description: "desc".to_string(),
            snippet: "snip".to_string(),
            updated_at: 0,
            updated_at_iso: "1970-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(
            json["id"], json["memory_id"],
            "id e memory_id devem ser iguais"
        );
    }

    #[test]
    fn snippet_truncated_to_200_chars() {
        let body_longo: String = "a".repeat(300);
        let snippet: String = body_longo.chars().take(200).collect();
        assert_eq!(snippet.len(), 200, "snippet deve ter exatamente 200 chars");
    }

    #[test]
    fn updated_at_iso_epoch_zero_yields_valid_utc() {
        let iso = crate::tz::epoch_to_iso(0);
        assert!(
            iso.starts_with("1970-01-01T00:00:00"),
            "epoch 0 deve mapear para 1970-01-01, obtido: {iso}"
        );
        assert!(
            iso.contains('+') || iso.contains('-'),
            "must contain offset sign, got: {iso}"
        );
    }
}
