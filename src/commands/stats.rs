use crate::errors::AppError;
use crate::i18n::erros;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use serde::Serialize;

#[derive(clap::Args)]
pub struct StatsArgs {
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// Explicit JSON flag. Accepted as a no-op because output is already JSON by default.
    #[arg(long, default_value_t = false)]
    pub json: bool,
    /// Output format: `json` or `text`. JSON is always emitted on stdout regardless of the value.
    #[arg(long, value_parser = ["json", "text"], hide = true)]
    pub format: Option<String>,
}

#[derive(Serialize)]
struct StatsResponse {
    memories: i64,
    /// Alias de `memories` para contrato documentado em SKILL.md e AGENT_PROTOCOL.md.
    memories_total: i64,
    entities: i64,
    /// Alias de `entities` para contrato documentado.
    entities_total: i64,
    relationships: i64,
    /// Alias de `relationships` para contrato documentado.
    relationships_total: i64,
    /// Alias semântico de `relationships` conforme contrato em AGENT_PROTOCOL.md.
    edges: i64,
    /// Total de chunks indexados (linha por chunk em `memory_chunks`).
    chunks_total: i64,
    /// Comprimento médio do campo body nas memórias ativas (não deletadas).
    avg_body_len: f64,
    namespaces: Vec<String>,
    db_size_bytes: u64,
    /// Alias semântico de `db_size_bytes` para contrato documentado.
    db_bytes: u64,
    schema_version: String,
    /// Tempo total de execução em milissegundos desde início do handler até serialização.
    elapsed_ms: u64,
}

pub fn run(args: StatsArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let _ = args.json; // --json é no-op pois output já é JSON por default
    let _ = args.format; // --format é no-op; JSON sempre emitido no stdout
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if !paths.db.exists() {
        return Err(AppError::NotFound(erros::banco_nao_encontrado(
            &paths.db.display().to_string(),
        )));
    }

    let conn = open_ro(&paths.db)?;

    let memories: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    let entities: i64 = conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))?;
    let relationships: i64 =
        conn.query_row("SELECT COUNT(*) FROM relationships", [], |r| r.get(0))?;

    let mut stmt = conn.prepare(
        "SELECT DISTINCT namespace FROM memories WHERE deleted_at IS NULL ORDER BY namespace",
    )?;
    let namespaces: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let schema_version: String = conn
        .query_row(
            "SELECT value FROM schema_meta WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "unknown".to_string());

    let db_size_bytes = std::fs::metadata(&paths.db).map(|m| m.len()).unwrap_or(0);

    let chunks_total: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_chunks", [], |r| r.get(0))
        .unwrap_or(0);

    let avg_body_len: f64 = conn
        .query_row(
            "SELECT COALESCE(AVG(LENGTH(body)), 0.0) FROM memories WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    output::emit_json(&StatsResponse {
        memories,
        memories_total: memories,
        entities,
        entities_total: entities,
        relationships,
        relationships_total: relationships,
        edges: relationships,
        chunks_total,
        avg_body_len,
        namespaces,
        db_size_bytes,
        db_bytes: db_size_bytes,
        schema_version,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn stats_response_serializa_todos_campos() {
        let resp = StatsResponse {
            memories: 10,
            memories_total: 10,
            entities: 5,
            entities_total: 5,
            relationships: 3,
            relationships_total: 3,
            edges: 3,
            chunks_total: 20,
            avg_body_len: 42.5,
            namespaces: vec!["global".to_string(), "projeto".to_string()],
            db_size_bytes: 8192,
            db_bytes: 8192,
            schema_version: "6".to_string(),
            elapsed_ms: 7,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["memories"], 10);
        assert_eq!(json["memories_total"], 10);
        assert_eq!(json["entities"], 5);
        assert_eq!(json["entities_total"], 5);
        assert_eq!(json["relationships"], 3);
        assert_eq!(json["relationships_total"], 3);
        assert_eq!(json["edges"], 3);
        assert_eq!(json["chunks_total"], 20);
        assert_eq!(json["db_size_bytes"], 8192u64);
        assert_eq!(json["db_bytes"], 8192u64);
        assert_eq!(json["schema_version"], "6");
        assert_eq!(json["elapsed_ms"], 7u64);
    }

    #[test]
    fn stats_response_namespaces_eh_array_de_strings() {
        let resp = StatsResponse {
            memories: 0,
            memories_total: 0,
            entities: 0,
            entities_total: 0,
            relationships: 0,
            relationships_total: 0,
            edges: 0,
            chunks_total: 0,
            avg_body_len: 0.0,
            namespaces: vec!["ns1".to_string(), "ns2".to_string(), "ns3".to_string()],
            db_size_bytes: 0,
            db_bytes: 0,
            schema_version: "unknown".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        let arr = json["namespaces"]
            .as_array()
            .expect("namespaces deve ser array");
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0], "ns1");
        assert_eq!(arr[1], "ns2");
        assert_eq!(arr[2], "ns3");
    }

    #[test]
    fn stats_response_namespaces_vazio_serializa_array_vazio() {
        let resp = StatsResponse {
            memories: 0,
            memories_total: 0,
            entities: 0,
            entities_total: 0,
            relationships: 0,
            relationships_total: 0,
            edges: 0,
            chunks_total: 0,
            avg_body_len: 0.0,
            namespaces: vec![],
            db_size_bytes: 0,
            db_bytes: 0,
            schema_version: "unknown".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        let arr = json["namespaces"]
            .as_array()
            .expect("namespaces deve ser array");
        assert!(arr.is_empty(), "namespaces vazio deve serializar como []");
    }

    #[test]
    fn stats_response_aliases_memories_total_e_memories_iguais() {
        let resp = StatsResponse {
            memories: 42,
            memories_total: 42,
            entities: 7,
            entities_total: 7,
            relationships: 2,
            relationships_total: 2,
            edges: 2,
            chunks_total: 0,
            avg_body_len: 0.0,
            namespaces: vec![],
            db_size_bytes: 0,
            db_bytes: 0,
            schema_version: "6".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["memories"], json["memories_total"]);
        assert_eq!(json["entities"], json["entities_total"]);
        assert_eq!(json["relationships"], json["relationships_total"]);
        assert_eq!(json["relationships"], json["edges"]);
        assert_eq!(json["db_size_bytes"], json["db_bytes"]);
    }
}
