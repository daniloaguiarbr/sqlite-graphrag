use crate::cli::MemoryType;
use crate::errors::AppError;
use crate::output::{self, OutputFormat};
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use crate::storage::memories;
use serde::Serialize;

#[derive(clap::Args)]
pub struct ListArgs {
    #[arg(long, default_value = "global")]
    pub namespace: Option<String>,
    /// Filter by memory.type. Note: distinct from graph entity_type
    /// (project/tool/person/file/concept/incident/decision/memory/dashboard/issue_tracker)
    /// used in --entities-file.
    #[arg(long, value_enum)]
    pub r#type: Option<MemoryType>,
    #[arg(long, default_value = "50")]
    pub limit: usize,
    #[arg(long, default_value = "0")]
    pub offset: usize,
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
    #[arg(long, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct ListItem {
    id: i64,
    /// Alias semântico de `id` para contrato documentado em SKILL.md e AGENT_PROTOCOL.md.
    memory_id: i64,
    name: String,
    namespace: String,
    #[serde(rename = "type")]
    memory_type: String,
    description: String,
    snippet: String,
    updated_at: i64,
    /// Timestamp RFC 3339 UTC paralelo a `updated_at`.
    updated_at_iso: String,
}

#[derive(Serialize)]
struct ListResponse {
    items: Vec<ListItem>,
    /// Tempo total de execução em milissegundos desde início do handler até serialização.
    elapsed_ms: u64,
}

pub fn run(args: ListArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    // v1.0.22 P1: padroniza exit code 4 com mensagem amigável quando DB não existe.
    if !paths.db.exists() {
        return Err(AppError::NotFound(
            crate::i18n::erros::banco_nao_encontrado(&paths.db.display().to_string()),
        ));
    }
    let conn = open_ro(&paths.db)?;

    let memory_type_str = args.r#type.map(|t| t.as_str());
    let rows = memories::list(&conn, &namespace, memory_type_str, args.limit, args.offset)?;

    let items: Vec<ListItem> = rows
        .into_iter()
        .map(|r| {
            let snippet: String = r.body.chars().take(200).collect();
            let updated_at_iso = crate::tz::epoch_para_iso(r.updated_at);
            ListItem {
                id: r.id,
                memory_id: r.id,
                name: r.name,
                namespace: r.namespace,
                memory_type: r.memory_type,
                description: r.description,
                snippet,
                updated_at: r.updated_at,
                updated_at_iso,
            }
        })
        .collect();

    match args.format {
        OutputFormat::Json => output::emit_json(&ListResponse {
            items,
            elapsed_ms: inicio.elapsed().as_millis() as u64,
        })?,
        OutputFormat::Text | OutputFormat::Markdown => {
            for item in &items {
                output::emit_text(&format!("{}: {}", item.name, item.snippet));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn list_response_serializa_items_e_elapsed_ms() {
        let resp = ListResponse {
            items: vec![ListItem {
                id: 1,
                memory_id: 1,
                name: "teste-memoria".to_string(),
                namespace: "global".to_string(),
                memory_type: "note".to_string(),
                description: "descricao de teste".to_string(),
                snippet: "corpo resumido".to_string(),
                updated_at: 1_745_000_000,
                updated_at_iso: "2025-04-19T00:00:00Z".to_string(),
            }],
            elapsed_ms: 7,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json["items"].is_array());
        assert_eq!(json["items"].as_array().unwrap().len(), 1);
        assert_eq!(json["items"][0]["name"], "teste-memoria");
        assert_eq!(json["items"][0]["memory_id"], 1);
        assert_eq!(json["elapsed_ms"], 7);
    }

    #[test]
    fn list_response_items_vazio_serializa_array_vazio() {
        let resp = ListResponse {
            items: vec![],
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json["items"].is_array());
        assert_eq!(json["items"].as_array().unwrap().len(), 0);
        assert_eq!(json["elapsed_ms"], 0);
    }

    #[test]
    fn list_item_memory_id_igual_a_id() {
        let item = ListItem {
            id: 42,
            memory_id: 42,
            name: "memoria-alias".to_string(),
            namespace: "projeto".to_string(),
            memory_type: "fact".to_string(),
            description: "desc".to_string(),
            snippet: "snip".to_string(),
            updated_at: 0,
            updated_at_iso: "1970-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(
            json["id"], json["memory_id"],
            "id e memory_id devem ser iguais"
        );
    }

    #[test]
    fn snippet_truncado_em_200_chars() {
        let body_longo: String = "a".repeat(300);
        let snippet: String = body_longo.chars().take(200).collect();
        assert_eq!(snippet.len(), 200, "snippet deve ter exatamente 200 chars");
    }

    #[test]
    fn updated_at_iso_epoch_zero_gera_utc_valido() {
        let iso = crate::tz::epoch_para_iso(0);
        assert!(
            iso.starts_with("1970-01-01T00:00:00"),
            "epoch 0 deve mapear para 1970-01-01, obtido: {iso}"
        );
        assert!(
            iso.contains('+') || iso.contains('-'),
            "deve conter sinal de offset, obtido: {iso}"
        );
    }
}
