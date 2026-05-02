//! Handler for the `history` CLI subcommand.

use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use rusqlite::params;
use rusqlite::OptionalExtension;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # List all versions of a memory (positional form)\n  \
    sqlite-graphrag history onboarding\n\n  \
    # List versions using the named flag form\n  \
    sqlite-graphrag history --name onboarding\n\n  \
    # Omit body content to reduce response size\n  \
    sqlite-graphrag history onboarding --no-body")]
pub struct HistoryArgs {
    /// Memory name as a positional argument. Alternative to `--name`.
    #[arg(
        value_name = "NAME",
        conflicts_with = "name",
        help = "Memory name whose version history to return; alternative to --name"
    )]
    pub name_positional: Option<String>,
    /// Memory name whose version history will be returned. Includes soft-deleted memories
    /// so that `restore --version <V>` workflow remains discoverable after `forget`.
    #[arg(long)]
    pub name: Option<String>,
    /// Namespace to query history from. Defaults to "global".
    #[arg(long, default_value = "global", help = "Namespace to query")]
    pub namespace: Option<String>,
    /// Omit body content from each version to reduce response size.
    #[arg(
        long,
        default_value_t = false,
        help = "Omit body content from response"
    )]
    pub no_body: bool,
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
struct HistoryVersion {
    version: i64,
    name: String,
    #[serde(rename = "type")]
    memory_type: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    metadata: serde_json::Value,
    /// Past-tense action label derived from `change_reason`; always populated
    /// so consumers do not see `null` for the documented `action` contract
    /// (M-A6 fix in v1.0.40). Known mappings: `create→created`, `edit→edited`,
    /// `rename→renamed`, `restore→restored`, `merge→merged`, `forget→forgotten`.
    /// Unknown verbs are passed through unchanged.
    action: String,
    change_reason: String,
    changed_by: Option<String>,
    created_at: i64,
    created_at_iso: String,
}

/// Maps the raw `change_reason` stored in `memory_versions` to the past-tense
/// `action` exposed in the JSON contract. Centralized so future call sites
/// (e.g. `read --include-history`) reuse the same mapping.
fn change_reason_to_action(reason: &str) -> String {
    match reason {
        "create" => "created",
        "edit" => "edited",
        "update" => "updated",
        "rename" => "renamed",
        "restore" => "restored",
        "merge" => "merged",
        "forget" => "forgotten",
        other => other,
    }
    .to_string()
}

#[derive(Serialize)]
struct HistoryResponse {
    name: String,
    namespace: String,
    /// True when the memory is currently soft-deleted (forgotten).
    /// Allows the user to discover the version for `restore` even after `forget`.
    deleted: bool,
    versions: Vec<HistoryVersion>,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: HistoryArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    // Resolve name from positional or --name flag; both are optional, at least one is required.
    let name = args.name_positional.or(args.name).ok_or_else(|| {
        AppError::Validation("name required: pass as positional argument or via --name".to_string())
    })?;
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    crate::storage::connection::ensure_db_ready(&paths)?;
    let conn = open_ro(&paths.db)?;

    // v1.0.22 P0: direct query WITHOUT deleted_at filter — history MUST return versions
    // of forgotten memories so the user can discover the version to use in `restore`.
    // The old find_by_name filtered deleted_at IS NULL and was a dead-end in the forget+restore workflow.
    let row: Option<(i64, Option<i64>)> = conn
        .query_row(
            "SELECT id, deleted_at FROM memories WHERE namespace = ?1 AND name = ?2",
            params![namespace, name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (memory_id, deleted_at) =
        row.ok_or_else(|| AppError::NotFound(errors_msg::memory_not_found(&name, &namespace)))?;
    let deleted = deleted_at.is_some();

    let mut stmt = conn.prepare(
        "SELECT version, name, type, description, body, metadata,
                change_reason, changed_by, created_at
         FROM memory_versions
         WHERE memory_id = ?1
         ORDER BY version ASC",
    )?;

    let no_body = args.no_body;
    let versions = stmt
        .query_map(params![memory_id], |r| {
            let created_at: i64 = r.get(8)?;
            let created_at_iso = crate::tz::epoch_to_iso(created_at);
            let body_str: String = r.get(4)?;
            let metadata_str: String = r.get(5)?;
            let metadata_value: serde_json::Value = serde_json::from_str(&metadata_str)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            let change_reason: String = r.get(6)?;
            let action = change_reason_to_action(&change_reason);
            Ok(HistoryVersion {
                version: r.get(0)?,
                name: r.get(1)?,
                memory_type: r.get(2)?,
                description: r.get(3)?,
                body: if no_body { None } else { Some(body_str) },
                metadata: metadata_value,
                action,
                change_reason,
                changed_by: r.get(7)?,
                created_at,
                created_at_iso,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    output::emit_json(&HistoryResponse {
        name,
        namespace,
        deleted,
        versions,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::change_reason_to_action;

    // Bug M-A6: action is always populated and maps known reasons to past tense.
    #[test]
    fn change_reason_create_maps_to_created() {
        assert_eq!(change_reason_to_action("create"), "created");
    }

    #[test]
    fn change_reason_edit_maps_to_edited() {
        assert_eq!(change_reason_to_action("edit"), "edited");
    }

    #[test]
    fn change_reason_rename_maps_to_renamed() {
        assert_eq!(change_reason_to_action("rename"), "renamed");
    }

    #[test]
    fn change_reason_restore_maps_to_restored() {
        assert_eq!(change_reason_to_action("restore"), "restored");
    }

    #[test]
    fn change_reason_merge_maps_to_merged() {
        assert_eq!(change_reason_to_action("merge"), "merged");
    }

    #[test]
    fn change_reason_forget_maps_to_forgotten() {
        assert_eq!(change_reason_to_action("forget"), "forgotten");
    }

    #[test]
    fn change_reason_unknown_passes_through() {
        assert_eq!(change_reason_to_action("custom-action"), "custom-action");
    }

    #[test]
    fn epoch_zero_yields_valid_iso() {
        // epoch_to_iso uses chrono-tz with explicit offset (+00:00 for UTC)
        let iso = crate::tz::epoch_to_iso(0);
        assert!(iso.starts_with("1970-01-01T00:00:00"), "got: {iso}");
        assert!(iso.contains("00:00"), "must contain offset, got: {iso}");
    }

    #[test]
    fn typical_epoch_yields_iso_rfc3339() {
        let iso = crate::tz::epoch_to_iso(1_745_000_000);
        assert!(!iso.is_empty(), "created_at_iso must not be empty");
        assert!(iso.contains('T'), "created_at_iso must contain T separator");
        // With UTC the offset is +00:00; verifies general format without relying on the global tz
        assert!(
            iso.contains('+') || iso.contains('-'),
            "must contain offset sign, got: {iso}"
        );
    }

    #[test]
    fn invalid_epoch_returns_fallback() {
        let iso = crate::tz::epoch_to_iso(i64::MIN);
        assert!(
            !iso.is_empty(),
            "invalid epoch must return non-empty fallback"
        );
    }
}
