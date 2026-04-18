use crate::cli::RelationKind;
use crate::constants::DEFAULT_RELATION_WEIGHT;
use crate::errors::AppError;
use crate::output::{self, OutputFormat};
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::entities;
use serde::Serialize;

#[derive(clap::Args)]
pub struct LinkArgs {
    #[arg(long)]
    pub from: String,
    #[arg(long)]
    pub to: String,
    #[arg(long, value_enum)]
    pub relation: RelationKind,
    #[arg(long)]
    pub weight: Option<f64>,
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
    #[arg(long, env = "NEUROGRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct LinkResponse {
    action: String,
    from: String,
    to: String,
    relation: String,
    weight: f64,
    namespace: String,
}

pub fn run(args: LinkArgs) -> Result<(), AppError> {
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if args.from == args.to {
        return Err(AppError::Validation(
            "--from and --to must be different entities — self-referential relationships are not supported".to_string(),
        ));
    }

    let weight = args.weight.unwrap_or(DEFAULT_RELATION_WEIGHT);
    if !(0.0..=1.0).contains(&weight) {
        return Err(AppError::Validation(format!(
            "--weight: must be between 0.0 and 1.0 (actual: {weight})"
        )));
    }

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

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (_rel_id, was_created) = entities::create_or_fetch_relationship(
        &tx,
        &namespace,
        source_id,
        target_id,
        relation_str,
        weight,
        None,
    )?;

    if was_created {
        entities::recalculate_degree(&tx, source_id)?;
        entities::recalculate_degree(&tx, target_id)?;
    }
    tx.commit()?;

    let action = if was_created {
        "created".to_string()
    } else {
        "already_exists".to_string()
    };

    let response = LinkResponse {
        action: action.clone(),
        from: args.from.clone(),
        to: args.to.clone(),
        relation: relation_str.to_string(),
        weight,
        namespace: namespace.clone(),
    };

    match args.format {
        OutputFormat::Json => output::emit_json(&response)?,
        OutputFormat::Text | OutputFormat::Markdown => {
            output::emit_text(&format!(
                "{}: {} --[{}]--> {} [{}]",
                action, response.from, response.relation, response.to, response.namespace
            ));
        }
    }

    Ok(())
}
