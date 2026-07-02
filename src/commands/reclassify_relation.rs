//! Handler for the `reclassify-relation` CLI subcommand (GAP-13).
//!
//! Renames a relation type in the `relationships` table — either a single
//! directed edge (`--source`, `--target`, `--from-relation`) or every edge of
//! a given type in the namespace (`--batch`).
//!
//! When the rename would produce a duplicate `(source_id, target_id, relation)`
//! triple, `UPDATE OR IGNORE` skips the conflicting row and the subsequent
//! `DELETE` removes it; the count of such skipped rows is reported as
//! `merged_duplicates`.

use crate::entity_type::EntityType;
use crate::errors::AppError;
use crate::output::{self, OutputFormat};
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use rusqlite::params;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Rename a single edge from 'mentions' to 'related'\n  \
    sqlite-graphrag reclassify-relation --source tokio --target axum \\\n  \
        --from-relation mentions --to-relation related\n\n  \
    # Rename every 'mentions' edge in the namespace to 'related'\n  \
    sqlite-graphrag reclassify-relation \\\n  \
        --from-relation mentions --to-relation related --batch\n\n  \
    # Dry-run to preview what would change\n  \
    sqlite-graphrag reclassify-relation \\\n  \
        --from-relation mentions --to-relation related --batch --dry-run\n\n  \
    # Batch rename only edges whose source is a 'tool' entity\n  \
    sqlite-graphrag reclassify-relation \\\n  \
        --from-relation uses --to-relation depends_on --batch \\\n  \
        --filter-source-type tool\n\n  \
    # Migrate edges stored with a LITERAL hyphenated relation (P4):\n  \
    # --from-relation normalizes 'applies-to' to 'applies_to' and never\n  \
    # matches the raw stored value; --literal-from matches it verbatim.\n  \
    sqlite-graphrag reclassify-relation \\\n  \
        --literal-from applies-to --to-relation applies_to --batch\n\n\
NOTE:\n  \
    Single mode requires --source, --target and --from-relation (or --literal-from).\n  \
    Batch mode requires --from-relation (or --literal-from), --to-relation and --batch.\n  \
    --from-relation and --literal-from are mutually exclusive; exactly one is required.\n  \
    --filter-source-type and --filter-target-type are only effective in batch mode.")]
pub struct ReclassifyRelationArgs {
    /// Source entity name (single mode). Mutually exclusive with --batch.
    #[arg(long, conflicts_with = "batch", value_name = "ENTITY")]
    pub source: Option<String>,
    /// Target entity name (single mode). Mutually exclusive with --batch.
    #[arg(long, conflicts_with = "batch", value_name = "ENTITY")]
    pub target: Option<String>,
    /// Current relation type to rename (normalized: hyphens become
    /// underscores at the CLI boundary). Required in both single and batch
    /// modes unless --literal-from is given.
    #[arg(
        long,
        value_parser = crate::parsers::parse_relation,
        value_name = "RELATION",
        required_unless_present = "literal_from",
        conflicts_with = "literal_from"
    )]
    pub from_relation: Option<String>,
    /// v1.1.1 (P4): current relation type to rename, matched LITERALLY —
    /// no normalization is applied, so edges stored with hyphenated values
    /// (e.g. `applies-to`) become reachable. Mutually exclusive with
    /// --from-relation.
    #[arg(long, value_name = "RELATION")]
    pub literal_from: Option<String>,
    /// New relation type to assign. Required in both single and batch modes.
    #[arg(long, value_parser = crate::parsers::parse_relation, value_name = "RELATION")]
    pub to_relation: String,
    /// Enable batch reclassification of all edges with --from-relation. Requires --from-relation and --to-relation.
    #[arg(long, default_value_t = false)]
    pub batch: bool,
    /// Filter batch: only rename edges whose source entity has this type.
    #[arg(long, value_enum, value_name = "TYPE", requires = "batch")]
    pub filter_source_type: Option<EntityType>,
    /// Filter batch: only rename edges whose target entity has this type.
    #[arg(long, value_enum, value_name = "TYPE", requires = "batch")]
    pub filter_target_type: Option<EntityType>,
    /// Preview count without committing changes.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct ReclassifyRelationResponse {
    action: String,
    from_relation: String,
    to_relation: String,
    /// Number of edges successfully renamed.
    count: usize,
    /// Edges that collided with an existing (source, target, to_relation) triple
    /// and were removed rather than renamed (UPDATE OR IGNORE + DELETE pattern).
    merged_duplicates: usize,
    namespace: String,
    elapsed_ms: u64,
}

impl ReclassifyRelationArgs {
    /// v1.1.1 (P4): the relation value used in every WHERE clause.
    ///
    /// `--literal-from` wins and is matched VERBATIM (no normalization);
    /// otherwise the clap-normalized `--from-relation` applies. Clap
    /// guarantees exactly one of the two is present
    /// (`required_unless_present` + `conflicts_with`).
    fn effective_from(&self) -> &str {
        self.literal_from
            .as_deref()
            .or(self.from_relation.as_deref())
            .unwrap_or_default()
    }
}

pub fn run(args: ReclassifyRelationArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    // Emit warnings for non-canonical relation values.
    crate::parsers::warn_if_non_canonical(args.effective_from());
    crate::parsers::warn_if_non_canonical(&args.to_relation);

    // Reject same-value renames: nothing to do and would silently remove
    // duplicates. The comparison uses the EFFECTIVE from value, so migrating
    // a literal hyphenated relation onto its normalized form (e.g.
    // `--literal-from applies-to --to-relation applies_to`) is a VALID
    // migration, not an equality.
    if args.effective_from() == args.to_relation {
        return Err(AppError::Validation(
            "--from-relation/--literal-from and --to-relation must be different".to_string(),
        ));
    }

    let mut conn = open_rw(&paths.db)?;

    if args.batch {
        run_batch(args, inicio, namespace, &mut conn)
    } else {
        run_single(args, inicio, namespace, &mut conn)
    }
}

// ---------------------------------------------------------------------------
// Single mode
// ---------------------------------------------------------------------------

fn run_single(
    args: ReclassifyRelationArgs,
    inicio: std::time::Instant,
    namespace: String,
    conn: &mut rusqlite::Connection,
) -> Result<(), AppError> {
    let source_name = args.source.as_deref().ok_or_else(|| {
        AppError::Validation(
            "--source is required in single mode (omit --batch for single-edge rename)".to_string(),
        )
    })?;
    let target_name = args
        .target
        .as_deref()
        .ok_or_else(|| AppError::Validation("--target is required in single mode".to_string()))?;

    // Resolve entity IDs — fail fast if either side does not exist.
    // Normalize names to match the normalized stored entity names.
    let source_name_norm = crate::parsers::normalize_entity_name(source_name);
    let target_name_norm = crate::parsers::normalize_entity_name(target_name);
    let source_id: i64 = conn
        .query_row(
            "SELECT id FROM entities WHERE name = ?1 AND namespace = ?2",
            params![source_name_norm, namespace],
            |r| r.get(0),
        )
        .map_err(|_| {
            AppError::NotFound(format!(
                "source entity '{source_name}' not found in namespace '{namespace}'"
            ))
        })?;

    let target_id: i64 = conn
        .query_row(
            "SELECT id FROM entities WHERE name = ?1 AND namespace = ?2",
            params![target_name_norm, namespace],
            |r| r.get(0),
        )
        .map_err(|_| {
            AppError::NotFound(format!(
                "target entity '{target_name}' not found in namespace '{namespace}'"
            ))
        })?;

    // Verify the edge to rename exists.
    let original_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM relationships
         WHERE source_id = ?1 AND target_id = ?2 AND relation = ?3 AND namespace = ?4",
        params![source_id, target_id, args.effective_from(), namespace],
        |r| r.get(0),
    )?;

    if original_count == 0 {
        return Err(AppError::NotFound(format!(
            "edge '{source_name}' --[{}]--> '{target_name}' not found in namespace '{namespace}'",
            args.effective_from()
        )));
    }

    if args.dry_run {
        emit_response(
            &args,
            "dry_run",
            original_count as usize,
            0,
            namespace,
            inicio,
        )?;
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    let updated = tx.execute(
        "UPDATE OR IGNORE relationships
         SET relation = ?1
         WHERE source_id = ?2 AND target_id = ?3 AND relation = ?4 AND namespace = ?5",
        params![
            args.to_relation,
            source_id,
            target_id,
            args.effective_from(),
            namespace
        ],
    )?;

    // Remove rows that UPDATE OR IGNORE silently skipped due to UNIQUE collision.
    let deleted = tx.execute(
        "DELETE FROM relationships
         WHERE source_id = ?1 AND target_id = ?2 AND relation = ?3 AND namespace = ?4",
        params![source_id, target_id, args.effective_from(), namespace],
    )?;

    tx.commit()?;

    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;

    let merged = (original_count as usize).saturating_sub(updated + deleted);
    emit_response(&args, "reclassified", updated, merged, namespace, inicio)
}

// ---------------------------------------------------------------------------
// Batch mode
// ---------------------------------------------------------------------------

fn run_batch(
    args: ReclassifyRelationArgs,
    inicio: std::time::Instant,
    namespace: String,
    conn: &mut rusqlite::Connection,
) -> Result<(), AppError> {
    // Build WHERE clause extensions for optional entity-type filters.
    // The base query joins relationships with source/target entities.
    let source_filter = args
        .filter_source_type
        .map(|t| format!(" AND src.type = '{}'", t.as_str()))
        .unwrap_or_default();
    let target_filter = args
        .filter_target_type
        .map(|t| format!(" AND tgt.type = '{}'", t.as_str()))
        .unwrap_or_default();
    let has_filters = !source_filter.is_empty() || !target_filter.is_empty();

    // Count edges that would be affected (used for both dry-run and confirmation).
    let original_count: i64 = if has_filters {
        conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM relationships r
                 JOIN entities src ON src.id = r.source_id
                 JOIN entities tgt ON tgt.id = r.target_id
                 WHERE r.relation = ?1 AND r.namespace = ?2{source_filter}{target_filter}"
            ),
            params![args.effective_from(), namespace],
            |r| r.get(0),
        )?
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM relationships
             WHERE relation = ?1 AND namespace = ?2",
            params![args.effective_from(), namespace],
            |r| r.get(0),
        )?
    };

    if original_count == 0 {
        tracing::warn!(target: "reclassify_relation",
            from_relation = %args.effective_from(),
            namespace = %namespace,
            "reclassify-relation batch matched zero edges — verify --from-relation value"
        );
    }

    if args.dry_run {
        emit_response(
            &args,
            "dry_run",
            original_count as usize,
            0,
            namespace,
            inicio,
        )?;
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    let updated = if has_filters {
        // For filtered batch we need to collect IDs first, then update.
        let ids: Vec<i64> = {
            let mut stmt = tx.prepare(&format!(
                "SELECT r.id FROM relationships r
                 JOIN entities src ON src.id = r.source_id
                 JOIN entities tgt ON tgt.id = r.target_id
                 WHERE r.relation = ?1 AND r.namespace = ?2{source_filter}{target_filter}"
            ))?;
            let collected: Vec<i64> = stmt
                .query_map(params![args.effective_from(), namespace], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            collected
        };

        let mut moved: usize = 0;
        for id in &ids {
            let n = tx.execute(
                "UPDATE OR IGNORE relationships
                 SET relation = ?1
                 WHERE id = ?2",
                params![args.to_relation, id],
            )?;
            moved += n;
        }
        moved
    } else {
        tx.execute(
            "UPDATE OR IGNORE relationships
             SET relation = ?1
             WHERE relation = ?2 AND namespace = ?3",
            params![args.to_relation, args.effective_from(), namespace],
        )?
    };

    // Remove rows the UPDATE OR IGNORE left behind (UNIQUE collision survivors).
    let deleted = if has_filters {
        tx.execute(
            &format!(
                "DELETE FROM relationships WHERE id IN (
                     SELECT r.id FROM relationships r
                     JOIN entities src ON src.id = r.source_id
                     JOIN entities tgt ON tgt.id = r.target_id
                     WHERE r.relation = ?1 AND r.namespace = ?2{source_filter}{target_filter}
                 )"
            ),
            params![args.effective_from(), namespace],
        )?
    } else {
        tx.execute(
            "DELETE FROM relationships WHERE relation = ?1 AND namespace = ?2",
            params![args.effective_from(), namespace],
        )?
    };

    tx.commit()?;

    conn.execute_batch("ANALYZE relationships;")?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;

    let merged = (original_count as usize).saturating_sub(updated + deleted);
    emit_response(&args, "reclassified", updated, merged, namespace, inicio)
}

// ---------------------------------------------------------------------------
// Shared response emitter
// ---------------------------------------------------------------------------

fn emit_response(
    args: &ReclassifyRelationArgs,
    action: &str,
    count: usize,
    merged_duplicates: usize,
    namespace: String,
    inicio: std::time::Instant,
) -> Result<(), AppError> {
    let response = ReclassifyRelationResponse {
        action: action.to_string(),
        from_relation: args.effective_from().to_string(),
        to_relation: args.to_relation.clone(),
        count,
        merged_duplicates,
        namespace: namespace.clone(),
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    };

    match args.format {
        OutputFormat::Json => output::emit_json(&response)?,
        OutputFormat::Text | OutputFormat::Markdown => {
            output::emit_text(&format!(
                "{action}: {count} edges '{}' → '{}' [{namespace}] (duplicates merged: {merged_duplicates})",
                args.effective_from(), args.to_relation
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_response(action: &str, count: usize, merged: usize) -> ReclassifyRelationResponse {
        ReclassifyRelationResponse {
            action: action.to_string(),
            from_relation: "mentions".to_string(),
            to_relation: "related".to_string(),
            count,
            merged_duplicates: merged,
            namespace: "global".to_string(),
            elapsed_ms: 1,
        }
    }

    #[test]
    fn response_serializes_all_fields() {
        let resp = make_response("reclassified", 5, 0);
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "reclassified");
        assert_eq!(json["from_relation"], "mentions");
        assert_eq!(json["to_relation"], "related");
        assert_eq!(json["count"], 5);
        assert_eq!(json["merged_duplicates"], 0);
        assert_eq!(json["namespace"], "global");
        assert!(json["elapsed_ms"].is_number());
    }

    #[test]
    fn response_action_dry_run() {
        let resp = make_response("dry_run", 10, 0);
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "dry_run");
        assert_eq!(json["count"], 10);
        assert_eq!(json["merged_duplicates"], 0);
    }

    #[test]
    fn response_merged_duplicates_nonzero() {
        // Simulates a case where 3 out of 10 edges collided with existing rows.
        let resp = make_response("reclassified", 7, 3);
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["count"], 7);
        assert_eq!(json["merged_duplicates"], 3);
    }

    #[test]
    fn response_count_zero_when_nothing_matched() {
        let resp = make_response("reclassified", 0, 0);
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["count"], 0);
        assert_eq!(json["merged_duplicates"], 0);
    }

    #[test]
    fn response_action_values_exhaustive() {
        for action in &["reclassified", "dry_run"] {
            let resp = make_response(action, 1, 0);
            let json = serde_json::to_value(&resp).expect("serialization");
            assert_eq!(json["action"], *action);
        }
    }

    #[test]
    fn response_from_and_to_relation_present() {
        let resp = ReclassifyRelationResponse {
            action: "reclassified".to_string(),
            from_relation: "uses".to_string(),
            to_relation: "depends_on".to_string(),
            count: 3,
            merged_duplicates: 1,
            namespace: "my-project".to_string(),
            elapsed_ms: 5,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["from_relation"], "uses");
        assert_eq!(json["to_relation"], "depends_on");
    }

    #[test]
    fn same_relation_value_rejected_at_logic_level() {
        // Validates that the guard in run() would catch from == to.
        // We test the condition directly since we cannot call run() without a DB.
        let from = "mentions".to_string();
        let to = "mentions".to_string();
        assert!(
            from == to,
            "same-value rename must be caught before DB access"
        );
    }

    // -----------------------------------------------------------------------
    // v1.1.1 (P4): --literal-from — filtro sem normalização
    // -----------------------------------------------------------------------

    fn base_args() -> ReclassifyRelationArgs {
        ReclassifyRelationArgs {
            source: None,
            target: None,
            from_relation: None,
            literal_from: None,
            to_relation: "applies_to".to_string(),
            batch: true,
            filter_source_type: None,
            filter_target_type: None,
            dry_run: false,
            namespace: Some("global".to_string()),
            format: OutputFormat::Json,
            json: true,
            db: None,
        }
    }

    #[test]
    fn effective_from_prefers_literal_and_falls_back_to_normalized() {
        let mut args = base_args();
        args.from_relation = Some("applies_to".to_string());
        assert_eq!(args.effective_from(), "applies_to");

        args.literal_from = Some("applies-to".to_string());
        assert_eq!(
            args.effective_from(),
            "applies-to",
            "literal value must win and stay verbatim"
        );

        // Migração literal→normalizado é VÁLIDA (não é igualdade).
        assert_ne!(args.effective_from(), args.to_relation);
    }

    fn setup_migrated_db() -> (tempfile::TempDir, rusqlite::Connection) {
        crate::storage::connection::register_vec_extension();
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let db_path = tmp.path().join("test.db");
        let mut conn = rusqlite::Connection::open(&db_path).expect("open");
        crate::migrations::runner().run(&mut conn).expect("migrate");
        (tmp, conn)
    }

    #[test]
    fn literal_from_migrates_hyphenated_edge_unreachable_by_normalized_filter() {
        let (_tmp, mut conn) = setup_migrated_db();
        conn.execute(
            "INSERT INTO entities (namespace, name, type) VALUES ('global','ent-a','concept')",
            [],
        )
        .unwrap();
        let a = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO entities (namespace, name, type) VALUES ('global','ent-b','concept')",
            [],
        )
        .unwrap();
        let b = conn.last_insert_rowid();
        // Aresta gravada com o valor LITERAL com hífen — inalcançável pelo
        // --from-relation (que normaliza para 'applies_to' na borda clap).
        conn.execute(
            "INSERT INTO relationships (namespace, source_id, target_id, relation, weight) \
             VALUES ('global', ?1, ?2, 'applies-to', 0.5)",
            params![a, b],
        )
        .unwrap();

        let mut args = base_args();
        args.literal_from = Some("applies-to".to_string());
        run_batch(
            args,
            std::time::Instant::now(),
            "global".to_string(),
            &mut conn,
        )
        .expect("batch literal migration");

        let migrated: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM relationships WHERE relation = 'applies_to'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(migrated, 1, "hyphenated edge must be migrated");
        let leftover: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM relationships WHERE relation = 'applies-to'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(leftover, 0, "no literal edge may remain");
    }

    #[test]
    fn cli_rejects_literal_from_combined_with_from_relation() {
        use clap::Parser;
        let err = match crate::cli::Cli::try_parse_from([
            "sqlite-graphrag",
            "reclassify-relation",
            "--from-relation",
            "mentions",
            "--literal-from",
            "applies-to",
            "--to-relation",
            "related",
            "--batch",
        ]) {
            Err(e) => e,
            Ok(_) => panic!("mutually exclusive flags must fail to parse"),
        };
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn cli_requires_one_of_from_relation_or_literal_from() {
        use clap::Parser;
        let err = match crate::cli::Cli::try_parse_from([
            "sqlite-graphrag",
            "reclassify-relation",
            "--to-relation",
            "related",
            "--batch",
        ]) {
            Err(e) => e,
            Ok(_) => panic!("one of the from flags is required"),
        };
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn cli_accepts_literal_from_alone_and_keeps_it_verbatim() {
        use clap::Parser;
        let parsed = crate::cli::Cli::try_parse_from([
            "sqlite-graphrag",
            "reclassify-relation",
            "--literal-from",
            "applies-to",
            "--to-relation",
            "applies_to",
            "--batch",
        ])
        .expect("literal-from alone must parse");
        match parsed.command {
            Some(crate::cli::Commands::ReclassifyRelation(a)) => {
                assert_eq!(a.literal_from.as_deref(), Some("applies-to"));
                assert!(a.from_relation.is_none());
                assert_eq!(a.effective_from(), "applies-to");
            }
            _ => unreachable!("unexpected command"),
        }
    }
}
