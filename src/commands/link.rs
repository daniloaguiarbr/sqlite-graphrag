//! Handler for the `link` CLI subcommand.

use crate::cli::RelationKind;
use crate::constants::DEFAULT_RELATION_WEIGHT;
use crate::errors::AppError;
use crate::i18n::{errors_msg, validation};
use crate::output::{self, OutputFormat};
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::entities;
use serde::Serialize;

#[derive(clap::Args)]
pub struct LinkArgs {
    /// Source entity.
    #[arg(long)]
    pub from: String,
    /// Target entity.
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
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
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
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: LinkArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if args.from == args.to {
        return Err(AppError::Validation(validation::self_referential_link()));
    }

    let weight = args.weight.unwrap_or(DEFAULT_RELATION_WEIGHT);
    if !(0.0..=1.0).contains(&weight) {
        return Err(AppError::Validation(validation::invalid_link_weight(
            weight,
        )));
    }

    if !paths.db.exists() {
        return Err(AppError::NotFound(errors_msg::database_not_found(
            &paths.db.display().to_string(),
        )));
    }

    let relation_str = args.relation.as_str();

    let mut conn = open_rw(&paths.db)?;

    let source_id = entities::find_entity_id(&conn, &namespace, &args.from)?
        .ok_or_else(|| AppError::NotFound(errors_msg::entity_not_found(&args.from, &namespace)))?;
    let target_id = entities::find_entity_id(&conn, &namespace, &args.to)?
        .ok_or_else(|| AppError::NotFound(errors_msg::entity_not_found(&args.to, &namespace)))?;

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
        elapsed_ms: inicio.elapsed().as_millis() as u64,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_response_sem_aliases_redundantes() {
        // P1-O: campos source/target foram removidos do JSON de resposta.
        let resp = LinkResponse {
            action: "created".to_string(),
            from: "entidade-a".to_string(),
            to: "entidade-b".to_string(),
            relation: "uses".to_string(),
            weight: 1.0,
            namespace: "default".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialização deve funcionar");
        assert_eq!(json["from"], "entidade-a");
        assert_eq!(json["to"], "entidade-b");
        assert!(
            json.get("source").is_none(),
            "campo 'source' foi removido em P1-O"
        );
        assert!(
            json.get("target").is_none(),
            "campo 'target' foi removido em P1-O"
        );
    }

    #[test]
    fn link_response_serializa_todos_campos() {
        let resp = LinkResponse {
            action: "already_exists".to_string(),
            from: "origem".to_string(),
            to: "destino".to_string(),
            relation: "mentions".to_string(),
            weight: 0.8,
            namespace: "teste".to_string(),
            elapsed_ms: 5,
        };
        let json = serde_json::to_value(&resp).expect("serialização deve funcionar");
        assert!(json.get("action").is_some());
        assert!(json.get("from").is_some());
        assert!(json.get("to").is_some());
        assert!(json.get("relation").is_some());
        assert!(json.get("weight").is_some());
        assert!(json.get("namespace").is_some());
        assert!(json.get("elapsed_ms").is_some());
    }
}
