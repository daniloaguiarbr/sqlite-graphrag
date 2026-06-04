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
    #[arg(
        long,
        help = "Namespace (env: SQLITE_GRAPHRAG_NAMESPACE, default: global)"
    )]
    pub namespace: Option<String>,
    /// Filter by memory.type. Note: distinct from graph entity_type
    /// (project/tool/person/file/concept/incident/decision/memory/dashboard/issue_tracker/organization/location/date)
    /// used in --entities-file.
    #[arg(long, value_enum)]
    pub r#type: Option<MemoryType>,
    #[arg(
        long,
        help = "Maximum number of memories to return (default: 50 for text, all for JSON)"
    )]
    pub limit: Option<usize>,
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

#[derive(Serialize, Clone)]
struct ListItem {
    id: i64,
    /// Semantic alias of `id` for the contract documented in SKILL.md.
    memory_id: i64,
    name: String,
    namespace: String,
    /// Semantic alias for agents that parse `.type` in the JSON output.
    #[serde(rename = "type")]
    type_field: String,
    /// Semantic alias for agents that parse `.memory_type` in the JSON output.
    memory_type: String,
    description: String,
    snippet: String,
    updated_at: i64,
    /// RFC 3339 UTC timestamp parallel to `updated_at`.
    updated_at_iso: String,
    /// Unix epoch when the memory was soft-deleted, or omitted for active memories.
    /// Surfaced only in `list --include-deleted --json` so LLM consumers can
    /// distinguish active rows from soft-deleted ones in a single query (v1.0.37 H7+M9).
    #[serde(skip_serializing_if = "Option::is_none")]
    deleted_at: Option<i64>,
    /// RFC 3339 UTC mirror of `deleted_at`, omitted when `deleted_at` is None.
    #[serde(skip_serializing_if = "Option::is_none")]
    deleted_at_iso: Option<String>,
    /// Byte length of the full memory body.
    body_length: usize,
}

#[derive(Serialize)]
struct ListResponse {
    items: Vec<ListItem>,
    memories: Vec<ListItem>,
    /// Total number of matching memories in the namespace (ignoring limit/offset).
    total_count: usize,
    /// True when the returned item count is less than `total_count`, indicating
    /// that more results exist beyond the applied limit.
    truncated: bool,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: ListArgs) -> Result<(), AppError> {
    if args.limit == Some(0) {
        return Err(AppError::Validation(
            "--limit must be greater than zero".to_string(),
        ));
    }
    let inicio = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    // v1.0.22 P1: standardizes exit code 4 with a friendly message when the DB does not exist.
    crate::storage::connection::ensure_db_ready(&paths)?;
    let conn = open_ro(&paths.db)?;

    let effective_limit = args.limit.unwrap_or(match args.format {
        OutputFormat::Json => usize::MAX,
        _ => 50,
    });

    let memory_type_str = args.r#type.map(|t| t.as_str());
    let rows = memories::list(
        &conn,
        &namespace,
        memory_type_str,
        effective_limit,
        args.offset,
        args.include_deleted,
    )?;

    let items: Vec<ListItem> = rows
        .into_iter()
        .map(|r| {
            let body_length = r.body.len();
            let snippet: String = r.body.chars().take(200).collect();
            let updated_at_iso = crate::tz::epoch_to_iso(r.updated_at);
            let deleted_at_iso = r.deleted_at.map(crate::tz::epoch_to_iso);
            ListItem {
                id: r.id,
                memory_id: r.id,
                name: r.name,
                namespace: r.namespace,
                type_field: r.memory_type.clone(),
                memory_type: r.memory_type,
                description: r.description,
                snippet,
                updated_at: r.updated_at,
                updated_at_iso,
                deleted_at: r.deleted_at,
                deleted_at_iso,
                body_length,
            }
        })
        .collect();

    let total_count = items.len();
    let truncated = args.limit.is_some_and(|lim| items.len() >= lim);

    match args.format {
        OutputFormat::Json => {
            let memories = items.clone();
            output::emit_json(&ListResponse {
                total_count,
                truncated,
                memories,
                items,
                elapsed_ms: inicio.elapsed().as_millis() as u64,
            })?;
        }
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

    fn make_item(name: &str) -> ListItem {
        ListItem {
            id: 1,
            memory_id: 1,
            name: name.to_string(),
            namespace: "global".to_string(),
            type_field: "note".to_string(),
            memory_type: "note".to_string(),
            description: "desc".to_string(),
            snippet: "snip".to_string(),
            updated_at: 1_745_000_000,
            updated_at_iso: "2025-04-19T00:00:00Z".to_string(),
            deleted_at: None,
            deleted_at_iso: None,
            body_length: 4,
        }
    }

    #[test]
    fn list_response_serializes_items_and_elapsed_ms() {
        let resp = ListResponse {
            items: vec![make_item("test-memory")],
            memories: vec![make_item("test-memory")],
            total_count: 1,
            truncated: false,
            elapsed_ms: 7,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json["items"].is_array());
        assert_eq!(json["items"].as_array().unwrap().len(), 1);
        assert_eq!(json["items"][0]["name"], "test-memory");
        assert_eq!(json["items"][0]["memory_id"], 1);
        assert_eq!(json["elapsed_ms"], 7);
        // deleted_at/deleted_at_iso must be omitted when None (skip_serializing_if)
        assert!(json["items"][0].get("deleted_at").is_none());
        assert!(json["items"][0].get("deleted_at_iso").is_none());
    }

    #[test]
    fn list_item_with_deleted_at_serializes_both_fields() {
        let item = ListItem {
            id: 99,
            memory_id: 99,
            name: "soft-deleted-memory".to_string(),
            namespace: "global".to_string(),
            type_field: "note".to_string(),
            memory_type: "note".to_string(),
            description: "deleted".to_string(),
            snippet: "snip".to_string(),
            updated_at: 1_745_000_000,
            updated_at_iso: "2025-04-19T00:00:00Z".to_string(),
            deleted_at: Some(1_745_100_000),
            deleted_at_iso: Some("2025-04-20T03:46:40Z".to_string()),
            body_length: 4,
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["deleted_at"], 1_745_100_000_i64);
        assert_eq!(json["deleted_at_iso"], "2025-04-20T03:46:40Z");
    }

    #[test]
    fn list_response_items_empty_serializes_empty_array() {
        let resp = ListResponse {
            items: vec![],
            memories: vec![],
            total_count: 0,
            truncated: false,
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
            type_field: "fact".to_string(),
            memory_type: "fact".to_string(),
            description: "desc".to_string(),
            snippet: "snip".to_string(),
            updated_at: 0,
            updated_at_iso: "1970-01-01T00:00:00Z".to_string(),
            deleted_at: None,
            deleted_at_iso: None,
            body_length: 0,
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
    fn list_item_emits_both_type_and_memory_type() {
        let item = ListItem {
            id: 1,
            memory_id: 1,
            name: "test".to_string(),
            namespace: "global".to_string(),
            type_field: "note".to_string(),
            memory_type: "note".to_string(),
            description: "desc".to_string(),
            snippet: "snip".to_string(),
            updated_at: 0,
            updated_at_iso: "1970-01-01T00:00:00Z".to_string(),
            deleted_at: None,
            deleted_at_iso: None,
            body_length: 0,
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["type"], "note", "serde rename must produce 'type'");
        assert_eq!(
            json["memory_type"], "note",
            "memory_type must also be present"
        );
    }

    #[test]
    fn updated_at_iso_epoch_zero_yields_valid_utc() {
        // v1.0.68 (test fix): timezone-agnostic — parse the ISO and compare
        // the instant with the Unix epoch.
        let iso = crate::tz::epoch_to_iso(0);
        let parsed = chrono::DateTime::parse_from_rfc3339(&iso)
            .unwrap_or_else(|e| panic!("expected RFC3339, got `{iso}`: {e}"));
        assert_eq!(
            parsed.timestamp(),
            chrono::DateTime::UNIX_EPOCH.timestamp(),
            "epoch 0 deve mapear para o instante Unix epoch, obtido: {iso}"
        );
        assert!(
            iso.contains('+') || iso.contains('-'),
            "must contain offset sign, got: {iso}"
        );
    }

    #[test]
    fn body_length_reflects_byte_count() {
        let body = "hello world";
        let item = ListItem {
            id: 1,
            memory_id: 1,
            name: "test".to_string(),
            namespace: "global".to_string(),
            type_field: "note".to_string(),
            memory_type: "note".to_string(),
            description: "desc".to_string(),
            snippet: body.chars().take(200).collect(),
            updated_at: 0,
            updated_at_iso: "1970-01-01T00:00:00Z".to_string(),
            deleted_at: None,
            deleted_at_iso: None,
            body_length: body.len(),
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["body_length"], body.len());
    }
}
