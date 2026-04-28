//! Handler for the `read` CLI subcommand.

use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use crate::storage::memories;
use serde::Serialize;

#[derive(clap::Args)]
pub struct ReadArgs {
    /// Memory name as a positional argument. Alternative to `--name`.
    #[arg(value_name = "NAME", conflicts_with = "name")]
    pub name_positional: Option<String>,
    /// Memory name to read. Returns NotFound (exit 4) if missing or soft-deleted.
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long, default_value = "global")]
    pub namespace: Option<String>,
    #[arg(long, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct ReadResponse {
    /// Canonical storage field. Preserved for compatibility with v2.0.0 clients.
    id: i64,
    /// Semantic alias of `id` for the contract documented in SKILL.md and AGENT_PROTOCOL.md.
    memory_id: i64,
    namespace: String,
    name: String,
    /// Semantic alias of `memory_type` for the documented contract.
    #[serde(rename = "type")]
    type_alias: String,
    memory_type: String,
    description: String,
    body: String,
    body_hash: String,
    session_id: Option<String>,
    source: String,
    metadata: serde_json::Value,
    /// Most recent memory version, useful for optimistic control via `--expected-updated-at`.
    version: i64,
    created_at: i64,
    /// Timestamp RFC 3339 UTC paralelo a `created_at` para parsers ISO 8601.
    created_at_iso: String,
    updated_at: i64,
    /// Timestamp RFC 3339 UTC paralelo a `updated_at` para parsers ISO 8601.
    updated_at_iso: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

fn epoch_to_iso(epoch: i64) -> String {
    crate::tz::epoch_to_iso(epoch)
}

pub fn run(args: ReadArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    // Resolve name from positional or --name flag; both are optional, at least one is required.
    let name = args.name_positional.or(args.name).ok_or_else(|| {
        AppError::Validation("name required: pass as positional argument or via --name".to_string())
    })?;
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    if !paths.db.exists() {
        return Err(AppError::NotFound(
            crate::i18n::errors_msg::database_not_found(&paths.db.display().to_string()),
        ));
    }
    let conn = open_ro(&paths.db)?;

    match memories::read_by_name(&conn, &namespace, &name)? {
        Some(row) => {
            // Resolver versão atual via tabela memory_versions (maior version para este memory_id).
            let version: i64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(version), 1) FROM memory_versions WHERE memory_id=?1",
                    rusqlite::params![row.id],
                    |r| r.get(0),
                )
                .unwrap_or(1);

            let response = ReadResponse {
                id: row.id,
                memory_id: row.id,
                namespace: row.namespace,
                name: row.name,
                type_alias: row.memory_type.clone(),
                memory_type: row.memory_type,
                description: row.description,
                body: row.body,
                body_hash: row.body_hash,
                session_id: row.session_id,
                source: row.source,
                metadata: serde_json::from_str::<serde_json::Value>(&row.metadata)
                    .unwrap_or(serde_json::Value::Null),
                version,
                created_at: row.created_at,
                created_at_iso: epoch_to_iso(row.created_at),
                updated_at: row.updated_at,
                updated_at_iso: epoch_to_iso(row.updated_at),
                elapsed_ms: inicio.elapsed().as_millis() as u64,
            };
            output::emit_json(&response)?;
        }
        None => {
            return Err(AppError::NotFound(errors_msg::memory_not_found(
                &name, &namespace,
            )))
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_to_iso_converte_zero_para_epoch_unix() {
        let resultado = epoch_to_iso(0);
        assert!(
            resultado.starts_with("1970-01-01T00:00:00"),
            "epoch 0 deve mapear para 1970-01-01T00:00:00, obtido: {resultado}"
        );
    }

    #[test]
    fn epoch_to_iso_converte_timestamp_conhecido() {
        let resultado = epoch_to_iso(1_705_320_000);
        assert!(
            resultado.starts_with("2024-01-15"),
            "timestamp 1705320000 deve mapear para 2024-01-15, obtido: {resultado}"
        );
    }

    #[test]
    fn epoch_to_iso_retorna_fallback_para_epoch_negativo_invalido() {
        let resultado = epoch_to_iso(i64::MIN);
        assert!(
            !resultado.is_empty(),
            "deve retornar string não vazia mesmo para epoch inválido"
        );
    }

    #[test]
    fn read_response_serializa_aliases_id_e_memory_id() {
        let resp = ReadResponse {
            id: 42,
            memory_id: 42,
            namespace: "global".to_string(),
            name: "minha-mem".to_string(),
            type_alias: "fact".to_string(),
            memory_type: "fact".to_string(),
            description: "desc".to_string(),
            body: "corpo".to_string(),
            body_hash: "abc123".to_string(),
            session_id: None,
            source: "agent".to_string(),
            metadata: serde_json::json!({}),
            version: 1,
            created_at: 1_705_320_000,
            created_at_iso: "2024-01-15T12:00:00Z".to_string(),
            updated_at: 1_705_320_000,
            updated_at_iso: "2024-01-15T12:00:00Z".to_string(),
            elapsed_ms: 5,
        };

        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["id"], 42);
        assert_eq!(json["memory_id"], 42);
        assert_eq!(json["type"], "fact");
        assert_eq!(json["memory_type"], "fact");
        assert_eq!(json["elapsed_ms"], 5u64);
        assert!(
            json["session_id"].is_null(),
            "session_id None deve serializar como null"
        );
        // metadata deve serializar como objeto JSON, não como string escapada
        assert!(
            json["metadata"].is_object(),
            "metadata deve ser um objeto JSON"
        );
    }

    #[test]
    fn read_response_session_id_some_serializa_string() {
        let resp = ReadResponse {
            id: 1,
            memory_id: 1,
            namespace: "global".to_string(),
            name: "mem".to_string(),
            type_alias: "skill".to_string(),
            memory_type: "skill".to_string(),
            description: "d".to_string(),
            body: "b".to_string(),
            body_hash: "h".to_string(),
            session_id: Some("sess-123".to_string()),
            source: "agent".to_string(),
            metadata: serde_json::json!({}),
            version: 2,
            created_at: 0,
            created_at_iso: "1970-01-01T00:00:00Z".to_string(),
            updated_at: 0,
            updated_at_iso: "1970-01-01T00:00:00Z".to_string(),
            elapsed_ms: 0,
        };

        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["session_id"], "sess-123");
    }

    #[test]
    fn read_response_elapsed_ms_esta_presente() {
        let resp = ReadResponse {
            id: 7,
            memory_id: 7,
            namespace: "ns".to_string(),
            name: "n".to_string(),
            type_alias: "procedure".to_string(),
            memory_type: "procedure".to_string(),
            description: "d".to_string(),
            body: "b".to_string(),
            body_hash: "h".to_string(),
            session_id: None,
            source: "agent".to_string(),
            metadata: serde_json::json!({}),
            version: 3,
            created_at: 1000,
            created_at_iso: "1970-01-01T00:16:40Z".to_string(),
            updated_at: 2000,
            updated_at_iso: "1970-01-01T00:33:20Z".to_string(),
            elapsed_ms: 123,
        };

        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["elapsed_ms"], 123u64);
        assert!(json["created_at_iso"].is_string());
        assert!(json["updated_at_iso"].is_string());
    }

    #[test]
    fn read_response_metadata_object_nao_string_escapada() {
        // P2-A: metadata deve serializar como objeto JSON, não como string escapada.
        let resp = ReadResponse {
            id: 3,
            memory_id: 3,
            namespace: "ns".to_string(),
            name: "meta-test".to_string(),
            type_alias: "fact".to_string(),
            memory_type: "fact".to_string(),
            description: "d".to_string(),
            body: "b".to_string(),
            body_hash: "h".to_string(),
            session_id: None,
            source: "agent".to_string(),
            metadata: serde_json::json!({"chave": "valor", "numero": 42}),
            version: 1,
            created_at: 0,
            created_at_iso: "1970-01-01T00:00:00Z".to_string(),
            updated_at: 0,
            updated_at_iso: "1970-01-01T00:00:00Z".to_string(),
            elapsed_ms: 1,
        };

        let json = serde_json::to_value(&resp).expect("serialização falhou");
        // Must be object, not a JSON string containing escaped JSON.
        assert!(json["metadata"].is_object());
        assert_eq!(json["metadata"]["chave"], "valor");
        assert_eq!(json["metadata"]["numero"], 42);
    }

    #[test]
    fn read_response_metadata_fallback_para_null_em_json_invalido() {
        // P2-A: fallback quando metadata é string inválida.
        let raw = "json-invalido{{{";
        let parsed =
            serde_json::from_str::<serde_json::Value>(raw).unwrap_or(serde_json::Value::Null);
        assert!(parsed.is_null());
    }
}
