use crate::errors::AppError;
use serde::Serialize;

#[derive(Debug, Clone, Copy, clap::ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Json,
    Text,
    Markdown,
}

pub fn emit_json<T: Serialize>(value: &T) -> Result<(), AppError> {
    let json = serde_json::to_string_pretty(value)?;
    println!("{json}");
    Ok(())
}

pub fn emit_json_compact<T: Serialize>(value: &T) -> Result<(), AppError> {
    let json = serde_json::to_string(value)?;
    println!("{json}");
    Ok(())
}

pub fn emit_text(msg: &str) {
    println!("{msg}");
}

pub fn emit_progress(msg: &str) {
    eprintln!("{msg}");
}

#[derive(Serialize)]
pub struct RememberResponse {
    pub memory_id: i64,
    pub name: String,
    pub action: String,
    pub version: i64,
    pub entities_persisted: usize,
    pub relationships_persisted: usize,
    pub chunks_created: usize,
    pub warnings: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct RecallItem {
    pub memory_id: i64,
    pub name: String,
    pub namespace: String,
    #[serde(rename = "type")]
    pub memory_type: String,
    pub description: String,
    pub snippet: String,
    pub distance: f32,
    pub source: String,
}

#[derive(Serialize)]
pub struct RecallResponse {
    pub query: String,
    pub direct_matches: Vec<RecallItem>,
    pub graph_matches: Vec<RecallItem>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Dummy {
        val: u32,
    }

    // Tipo não-serializável para forçar erro de serialização JSON
    struct NotSerializable;
    impl Serialize for NotSerializable {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom(
                "falha intencional de serialização",
            ))
        }
    }

    #[test]
    fn emit_json_retorna_ok_para_valor_valido() {
        let v = Dummy { val: 42 };
        assert!(emit_json(&v).is_ok());
    }

    #[test]
    fn emit_json_retorna_erro_para_valor_nao_serializavel() {
        let v = NotSerializable;
        assert!(emit_json(&v).is_err());
    }

    #[test]
    fn emit_json_compact_retorna_ok_para_valor_valido() {
        let v = Dummy { val: 7 };
        assert!(emit_json_compact(&v).is_ok());
    }

    #[test]
    fn emit_json_compact_retorna_erro_para_valor_nao_serializavel() {
        let v = NotSerializable;
        assert!(emit_json_compact(&v).is_err());
    }

    #[test]
    fn emit_text_nao_entra_em_panico() {
        emit_text("mensagem de teste");
    }

    #[test]
    fn emit_progress_nao_entra_em_panico() {
        emit_progress("progresso de teste");
    }

    #[test]
    fn remember_response_serializa_corretamente() {
        let r = RememberResponse {
            memory_id: 1,
            name: "teste".to_string(),
            action: "created".to_string(),
            version: 1,
            entities_persisted: 2,
            relationships_persisted: 3,
            chunks_created: 4,
            warnings: vec!["aviso".to_string()],
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("memory_id"));
        assert!(json.contains("aviso"));
    }

    #[test]
    fn recall_item_serializa_campo_type_renomeado() {
        let item = RecallItem {
            memory_id: 10,
            name: "entidade".to_string(),
            namespace: "ns".to_string(),
            memory_type: "entity".to_string(),
            description: "desc".to_string(),
            snippet: "trecho".to_string(),
            distance: 0.5,
            source: "db".to_string(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"type\""));
        assert!(!json.contains("memory_type"));
    }

    #[test]
    fn recall_response_serializa_com_listas() {
        let resp = RecallResponse {
            query: "busca".to_string(),
            direct_matches: vec![],
            graph_matches: vec![],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("direct_matches"));
        assert!(json.contains("graph_matches"));
    }

    #[test]
    fn output_format_default_eh_json() {
        let fmt = OutputFormat::default();
        assert!(matches!(fmt, OutputFormat::Json));
    }

    #[test]
    fn output_format_variantes_existem() {
        let _text = OutputFormat::Text;
        let _md = OutputFormat::Markdown;
        let _json = OutputFormat::Json;
    }

    #[test]
    fn recall_item_clone_produz_valor_igual() {
        let item = RecallItem {
            memory_id: 99,
            name: "clone".to_string(),
            namespace: "ns".to_string(),
            memory_type: "relation".to_string(),
            description: "d".to_string(),
            snippet: "s".to_string(),
            distance: 0.1,
            source: "src".to_string(),
        };
        let cloned = item.clone();
        assert_eq!(cloned.memory_id, item.memory_id);
        assert_eq!(cloned.name, item.name);
    }
}
