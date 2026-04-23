use crate::errors::AppError;
use crate::i18n::erros;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use serde::Serialize;
use std::time::Instant;

#[derive(clap::Args)]
pub struct DebugSchemaArgs {
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct SchemaObject {
    name: String,
    #[serde(rename = "type")]
    object_type: String,
}

#[derive(Serialize)]
struct MigrationRecord {
    version: i64,
    name: String,
    applied_on: String,
}

#[derive(Serialize)]
struct DebugSchemaResponse {
    /// Contador interno do SQLite incrementado a cada DDL (PRAGMA schema_version).
    /// Distinto de `user_version`: este é gerenciado automaticamente pelo SQLite.
    schema_version: i64,
    /// Valor canônico SCHEMA_USER_VERSION definido explicitamente pelas migrações
    /// (PRAGMA user_version). Distinto de `schema_version` (DDL counter SQLite)
    /// e de `health.schema_version` (MAX version em refinery_schema_history).
    user_version: i64,
    objects: Vec<SchemaObject>,
    migrations: Vec<MigrationRecord>,
    elapsed_ms: u64,
}

pub fn run(args: DebugSchemaArgs) -> Result<(), AppError> {
    let inicio = Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if !paths.db.exists() {
        return Err(AppError::NotFound(erros::banco_nao_encontrado(
            &paths.db.display().to_string(),
        )));
    }

    let conn = open_ro(&paths.db)?;

    let schema_version: i64 = conn
        .query_row("PRAGMA schema_version", [], |r| r.get(0))
        .unwrap_or(0);

    // PRAGMA user_version é setado explicitamente após migrações (valor canônico SCHEMA_USER_VERSION).
    let user_version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap_or(0);

    let mut stmt = conn.prepare(
        "SELECT name, type FROM sqlite_master \
         WHERE type IN ('table','view','trigger','index') \
         ORDER BY type, name",
    )?;
    let objects: Vec<SchemaObject> = stmt
        .query_map([], |r| {
            Ok(SchemaObject {
                name: r.get(0)?,
                object_type: r.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let existe_hist: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='refinery_schema_history'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let migrations: Vec<MigrationRecord> = if existe_hist > 0 {
        let mut stmt_mig = conn.prepare(
            "SELECT version, name, applied_on \
             FROM refinery_schema_history \
             ORDER BY version",
        )?;
        let rows: Vec<MigrationRecord> = stmt_mig
            .query_map([], |r| {
                Ok(MigrationRecord {
                    version: r.get(0)?,
                    name: r.get(1)?,
                    applied_on: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    } else {
        Vec::new()
    };

    let elapsed_ms = inicio.elapsed().as_millis() as u64;

    output::emit_json(&DebugSchemaResponse {
        schema_version,
        user_version,
        objects,
        migrations,
        elapsed_ms,
    })?;

    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::Value;

    #[test]
    fn debug_schema_response_serializa_campos_obrigatorios() {
        let resp = DebugSchemaResponse {
            schema_version: 42,
            user_version: 49,
            objects: vec![SchemaObject {
                name: "memories".to_string(),
                object_type: "table".to_string(),
            }],
            migrations: vec![MigrationRecord {
                version: 1,
                name: "V001__init".to_string(),
                applied_on: "2026-01-01T00:00:00Z".to_string(),
            }],
            elapsed_ms: 7,
        };
        let json: Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["schema_version"], 42);
        assert_eq!(json["user_version"], 49);
        assert!(json["objects"].is_array());
        assert_eq!(json["objects"][0]["name"], "memories");
        assert_eq!(json["objects"][0]["type"], "table");
        assert!(json["migrations"].is_array());
        assert_eq!(json["migrations"][0]["version"], 1);
        assert_eq!(json["elapsed_ms"], 7);
    }

    #[test]
    fn schema_object_renomeia_campo_type() {
        let obj = SchemaObject {
            name: "entities".to_string(),
            object_type: "table".to_string(),
        };
        let json: Value = serde_json::to_value(&obj).unwrap();
        assert!(json.get("object_type").is_none());
        assert_eq!(json["type"], "table");
    }

    #[test]
    fn migration_record_serializa_todos_campos() {
        let rec = MigrationRecord {
            version: 3,
            name: "V003__indexes".to_string(),
            applied_on: "2026-04-19T12:00:00Z".to_string(),
        };
        let json: Value = serde_json::to_value(&rec).unwrap();
        assert_eq!(json["version"], 3);
        assert_eq!(json["name"], "V003__indexes");
        assert_eq!(json["applied_on"], "2026-04-19T12:00:00Z");
    }
}
