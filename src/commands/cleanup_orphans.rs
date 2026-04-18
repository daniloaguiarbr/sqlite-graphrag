use crate::errors::AppError;
use crate::output::{self, OutputFormat};
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::entities;
use serde::Serialize;

#[derive(clap::Args)]
pub struct CleanupOrphansArgs {
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub yes: bool,
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
    #[arg(long, env = "NEUROGRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct CleanupResponse {
    orphan_count: usize,
    deleted: usize,
    dry_run: bool,
    namespace: Option<String>,
}

pub fn run(args: CleanupOrphansArgs) -> Result<(), AppError> {
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if !paths.db.exists() {
        return Err(AppError::NotFound(format!(
            "database not found at {}. Run 'neurographrag init' first.",
            paths.db.display()
        )));
    }

    let mut conn = open_rw(&paths.db)?;

    let orphan_ids = entities::find_orphan_entity_ids(&conn, args.namespace.as_deref())?;
    let orphan_count = orphan_ids.len();

    let deleted = if args.dry_run {
        0
    } else {
        if orphan_count > 0 && !args.yes {
            output::emit_progress(&format!(
                "removing {orphan_count} orphan entities (use --yes to skip this notice)"
            ));
        }
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let removed = entities::delete_entities_by_ids(&tx, &orphan_ids)?;
        tx.commit()?;
        removed
    };

    let response = CleanupResponse {
        orphan_count,
        deleted,
        dry_run: args.dry_run,
        namespace: args.namespace.clone(),
    };

    match args.format {
        OutputFormat::Json => output::emit_json(&response)?,
        OutputFormat::Text | OutputFormat::Markdown => {
            let ns = response.namespace.as_deref().unwrap_or("<all>");
            output::emit_text(&format!(
                "orphans: {} found, {} deleted (dry_run={}) [{}]",
                response.orphan_count, response.deleted, response.dry_run, ns
            ));
        }
    }

    Ok(())
}
