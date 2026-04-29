//! Handler for the `unlink` CLI subcommand.

use crate::cli::RelationKind;
use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output::{self, OutputFormat};
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::entities;
use serde::Serialize;

#[derive(clap::Args)]
pub struct UnlinkArgs {
    /// Source entity. Also accepts the alias `--source`.
    #[arg(long, alias = "source")]
    pub from: String,
    /// Target entity. Also accepts the alias `--target`.
    #[arg(long, alias = "target")]
    pub to: String,
    #[arg(long, value_enum)]
    pub relation: RelationKind,
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
struct UnlinkResponse {
    action: String,
    relationship_id: i64,
    from_name: String,
    to_name: String,
    relation: String,
    namespace: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: UnlinkArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

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

    let rel = entities::find_relationship(&conn, source_id, target_id, relation_str)?.ok_or_else(
        || {
            AppError::NotFound(errors_msg::relationship_not_found(
                &args.from,
                relation_str,
                &args.to,
                &namespace,
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
        elapsed_ms: inicio.elapsed().as_millis() as u64,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::RelationKind;

    #[test]
    fn unlink_response_serializa_todos_campos() {
        let resp = UnlinkResponse {
            action: "deleted".to_string(),
            relationship_id: 99,
            from_name: "entidade-a".to_string(),
            to_name: "entidade-b".to_string(),
            relation: "uses".to_string(),
            namespace: "global".to_string(),
            elapsed_ms: 5,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["action"], "deleted");
        assert_eq!(json["relationship_id"], 99i64);
        assert_eq!(json["from_name"], "entidade-a");
        assert_eq!(json["to_name"], "entidade-b");
        assert_eq!(json["relation"], "uses");
        assert_eq!(json["namespace"], "global");
        assert_eq!(json["elapsed_ms"], 5u64);
    }

    #[test]
    fn unlink_args_relation_kind_as_str_correto() {
        assert_eq!(RelationKind::Uses.as_str(), "uses");
        assert_eq!(RelationKind::DependsOn.as_str(), "depends_on");
        assert_eq!(RelationKind::AppliesTo.as_str(), "applies_to");
        assert_eq!(RelationKind::Causes.as_str(), "causes");
        assert_eq!(RelationKind::Fixes.as_str(), "fixes");
    }

    #[test]
    fn unlink_response_action_deve_ser_deleted() {
        let resp = UnlinkResponse {
            action: "deleted".to_string(),
            relationship_id: 1,
            from_name: "a".to_string(),
            to_name: "b".to_string(),
            relation: "related".to_string(),
            namespace: "global".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(
            json["action"], "deleted",
            "ação de unlink deve sempre ser 'deleted'"
        );
    }

    #[test]
    fn unlink_response_relationship_id_positivo() {
        let resp = UnlinkResponse {
            action: "deleted".to_string(),
            relationship_id: 42,
            from_name: "origem".to_string(),
            to_name: "destino".to_string(),
            relation: "supports".to_string(),
            namespace: "projeto".to_string(),
            elapsed_ms: 3,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert!(
            json["relationship_id"].as_i64().unwrap() > 0,
            "relationship_id deve ser positivo após unlink"
        );
    }
}
