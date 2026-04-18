use crate::cli::RelationKind;
use crate::errors::AppError;
use crate::output::{self, OutputFormat};
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::entities;
use serde::Serialize;

#[derive(clap::Args)]
pub struct UnlinkArgs {
    #[arg(long)]
    pub from: String,
    #[arg(long)]
    pub to: String,
    #[arg(long, value_enum)]
    pub relation: RelationKind,
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
    #[arg(long, env = "NEUROGRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct UnlinkResponse {
    action: String,
    relationship_id: i64,
    from_name: String,
    to_name: String,
    relation: String,
    namespace: String,
}

pub fn run(args: UnlinkArgs) -> Result<(), AppError> {
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if !paths.db.exists() {
        return Err(AppError::NotFound(format!(
            "database not found at {}. Run 'neurographrag init' first.",
            paths.db.display()
        )));
    }

    let relation_str = args.relation.as_str();

    let mut conn = open_rw(&paths.db)?;

    let source_id = entities::find_entity_id(&conn, &namespace, &args.from)?.ok_or_else(|| {
        AppError::NotFound(format!(
            "entity \"{}\" does not exist in namespace \"{}\"",
            args.from, namespace
        ))
    })?;
    let target_id = entities::find_entity_id(&conn, &namespace, &args.to)?.ok_or_else(|| {
        AppError::NotFound(format!(
            "entity \"{}\" does not exist in namespace \"{}\"",
            args.to, namespace
        ))
    })?;

    let rel = entities::find_relationship(&conn, source_id, target_id, relation_str)?.ok_or_else(
        || {
            AppError::NotFound(format!(
                "relationship \"{}\" --[{}]--> \"{}\" does not exist in namespace \"{}\"",
                args.from, relation_str, args.to, namespace
            ))
        },
    )?;

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    entities::delete_relationship_by_id(&tx, rel.id)?;
    entities::recalculate_degree(&tx, source_id)?;
    entities::recalculate_degree(&tx, target_id)?;
    tx.commit()?;

    let response = UnlinkResponse {
        action: "deleted".to_string(),
        relationship_id: rel.id,
        from_name: args.from.clone(),
        to_name: args.to.clone(),
        relation: relation_str.to_string(),
        namespace: namespace.clone(),
    };

    match args.format {
        OutputFormat::Json => output::emit_json(&response)?,
        OutputFormat::Text | OutputFormat::Markdown => {
            output::emit_text(&format!(
                "deleted: {} --[{}]--> {} [{}]",
                response.from_name, response.relation, response.to_name, response.namespace
            ));
        }
    }

    Ok(())
}
