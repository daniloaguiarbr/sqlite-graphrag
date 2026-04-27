use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::{Linear, Module, VarBuilder};
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use regex::Regex;
use serde::Deserialize;

use crate::paths::AppPaths;
use crate::storage::entities::{NewEntity, NewRelationship};

const MODEL_ID: &str = "Davlan/bert-base-multilingual-cased-ner-hrl";
const MAX_SEQ_LEN: usize = 512;
const STRIDE: usize = 256;
const MAX_ENTS: usize = 30;
const TOP_K_RELATIONS: usize = 5;
const DEFAULT_RELATION: &str = "mentions";
const MIN_ENTITY_CHARS: usize = 2;

static REGEX_EMAIL: OnceLock<Regex> = OnceLock::new();
static REGEX_URL: OnceLock<Regex> = OnceLock::new();
static REGEX_UUID: OnceLock<Regex> = OnceLock::new();
static REGEX_ALL_CAPS: OnceLock<Regex> = OnceLock::new();

// v1.0.20: stopwords para filtrar palavras-regra PT-BR/EN comuns capturadas como ALL_CAPS.
// Sem este filtro, corpus técnico em PT-BR contendo regras formatadas em CAPS (NUNCA, PROIBIDO, DEVE)
// gerava ~70% de "entidades" lixo. Mantemos identificadores tipo MAX_RETRY (com underscore).
// v1.0.22: lista expandida com termos observados em stress test 495 arquivos do flowaiper.
// Inclui verbos (ADICIONAR, VALIDAR), adjetivos (ALTA, BAIXA), substantivos comuns (BANCO, CASO),
// HTTP methods (GET, POST, DELETE) e formatos de dados genéricos (JSON, XML).
const ALL_CAPS_STOPWORDS: &[&str] = &[
    "ACRESCENTADO",
    "ADICIONAR",
    "AGENTS",
    "ALL",
    "ALTA",
    "ALWAYS",
    "ARTEFATOS",
    "ATIVO",
    "BAIXA",
    "BANCO",
    "BLOQUEAR",
    "BUG",
    "CASO",
    "CONFIRMADO",
    "CONTRATO",
    "CRÍTICO",
    "CRITICAL",
    "CSV",
    "DEVE",
    "DISCO",
    "EFEITO",
    "ENTRADA",
    "ERROR",
    "ESSA",
    "ESSE",
    "ESSENCIAL",
    "ESTA",
    "ESTE",
    "EVITAR",
    "EXPANDIR",
    "EXPOR",
    "FALHA",
    "FIXME",
    "FORBIDDEN",
    "HACK",
    "HEARTBEAT",
    "INATIVO",
    "JAMAIS",
    "JSON",
    "MUST",
    "NEVER",
    "NOTE",
    "NUNCA",
    "OBRIGATÓRIO",
    "PADRÃO",
    "PROIBIDO",
    "REGRAS",
    "REQUIRED",
    "REQUISITO",
    "SEMPRE",
    "SHALL",
    "SHOULD",
    "SOUL",
    "TODAS",
    "TODO",
    "TODOS",
    "TOOLS",
    "TSV",
    "USAR",
    "VALIDAR",
    "VOCÊ",
    "WARNING",
    "XML",
    "YAML",
];

// v1.0.22: HTTP methods são verbos de protocolo, não entidades semanticamente úteis.
// Filtrados em apply_regex_prefilter (regex_all_caps) e iob_to_entities (single-token).
const HTTP_METHODS: &[&str] = &[
    "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "CONNECT", "TRACE",
];

fn is_filtered_all_caps(token: &str) -> bool {
    // Identificadores com underscore são preservados (ex: MAX_RETRY, FLOWAIPER_API_KEY)
    let is_identifier = token.contains('_');
    if is_identifier {
        return false;
    }
    ALL_CAPS_STOPWORDS.contains(&token) || HTTP_METHODS.contains(&token)
}

fn regex_email() -> &'static Regex {
    REGEX_EMAIL
        .get_or_init(|| Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}").unwrap())
}

fn regex_url() -> &'static Regex {
    REGEX_URL.get_or_init(|| Regex::new(r#"https?://[^\s\)\]\}"'<>]+"#).unwrap())
}

fn regex_uuid() -> &'static Regex {
    REGEX_UUID.get_or_init(|| {
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .unwrap()
    })
}

fn regex_all_caps() -> &'static Regex {
    REGEX_ALL_CAPS.get_or_init(|| Regex::new(r"\b[A-Z][A-Z0-9_]{2,}\b").unwrap())
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: String,
}

#[derive(Debug, Clone)]
pub struct ExtractionResult {
    pub entities: Vec<NewEntity>,
    pub relationships: Vec<NewRelationship>,
    /// Método usado para extração: "bert+regex" ou "regex-only".
    /// Útil para auditoria, métricas e reportes ao usuário.
    pub extraction_method: String,
}

pub trait Extractor: Send + Sync {
    fn extract(&self, body: &str) -> Result<ExtractionResult>;
}

#[derive(Deserialize)]
struct ModelConfig {
    #[serde(default)]
    id2label: HashMap<String, String>,
    hidden_size: usize,
}

struct BertNerModel {
    bert: BertModel,
    classifier: Linear,
    device: Device,
    id2label: HashMap<usize, String>,
}

impl BertNerModel {
    fn load(model_dir: &Path) -> Result<Self> {
        let config_path = model_dir.join("config.json");
        let weights_path = model_dir.join("model.safetensors");

        let config_str = std::fs::read_to_string(&config_path)
            .with_context(|| format!("lendo config.json em {config_path:?}"))?;
        let model_cfg: ModelConfig =
            serde_json::from_str(&config_str).context("parseando config.json do modelo NER")?;

        let id2label: HashMap<usize, String> = model_cfg
            .id2label
            .into_iter()
            .filter_map(|(k, v)| k.parse::<usize>().ok().map(|n| (n, v)))
            .collect();

        let num_labels = id2label.len().max(9);
        let hidden_size = model_cfg.hidden_size;

        let bert_config_str = std::fs::read_to_string(&config_path)
            .with_context(|| format!("relendo config.json para bert em {config_path:?}"))?;
        let bert_cfg: BertConfig =
            serde_json::from_str(&bert_config_str).context("parseando BertConfig")?;

        let device = Device::Cpu;

        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[&weights_path], DType::F32, &device)
                .with_context(|| format!("mapeando {weights_path:?}"))?
        };
        let bert = BertModel::load(vb.pp("bert"), &bert_cfg).context("carregando BertModel")?;

        // v1.0.20 fix P0 secundário: carregar classifier head do safetensors em vez de zeros.
        // Em v1.0.19 usávamos Tensor::zeros, o que produzia argmax constante e inferência degenerada.
        let cls_vb = vb.pp("classifier");
        let weight = cls_vb
            .get((num_labels, hidden_size), "weight")
            .context("carregando classifier.weight do safetensors")?;
        let bias = cls_vb
            .get(num_labels, "bias")
            .context("carregando classifier.bias do safetensors")?;
        let classifier = Linear::new(weight, Some(bias));

        Ok(Self {
            bert,
            classifier,
            device,
            id2label,
        })
    }

    fn predict(&self, token_ids: &[u32], attention_mask: &[u32]) -> Result<Vec<String>> {
        let len = token_ids.len();
        let ids_i64: Vec<i64> = token_ids.iter().map(|&x| x as i64).collect();
        let mask_i64: Vec<i64> = attention_mask.iter().map(|&x| x as i64).collect();

        let input_ids = Tensor::from_vec(ids_i64, (1, len), &self.device)
            .context("criando tensor input_ids")?;
        let token_type_ids = Tensor::zeros((1, len), DType::I64, &self.device)
            .context("criando tensor token_type_ids")?;
        let attn_mask = Tensor::from_vec(mask_i64, (1, len), &self.device)
            .context("criando tensor attention_mask")?;

        let sequence_output = self
            .bert
            .forward(&input_ids, &token_type_ids, Some(&attn_mask))
            .context("forward pass do BertModel")?;

        let logits = self
            .classifier
            .forward(&sequence_output)
            .context("forward pass do classificador")?;

        let logits_2d = logits.squeeze(0).context("removendo dimensão batch")?;

        let num_tokens = logits_2d.dim(0).context("dim(0)")?;

        let mut labels = Vec::with_capacity(num_tokens);
        for i in 0..num_tokens {
            let token_logits = logits_2d.get(i).context("get token logits")?;
            let vec: Vec<f32> = token_logits.to_vec1().context("to_vec1 logits")?;
            let argmax = vec
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .map(|(idx, _)| idx)
                .unwrap_or(0);
            let label = self
                .id2label
                .get(&argmax)
                .cloned()
                .unwrap_or_else(|| "O".to_string());
            labels.push(label);
        }

        Ok(labels)
    }
}

static NER_MODEL: OnceLock<Option<BertNerModel>> = OnceLock::new();

fn get_or_init_model(paths: &AppPaths) -> Option<&'static BertNerModel> {
    NER_MODEL
        .get_or_init(|| match load_model(paths) {
            Ok(m) => Some(m),
            Err(e) => {
                tracing::warn!("NER model não disponível (graceful degradation): {e:#}");
                None
            }
        })
        .as_ref()
}

fn model_dir(paths: &AppPaths) -> PathBuf {
    paths.models.join("bert-multilingual-ner")
}

fn ensure_model_files(paths: &AppPaths) -> Result<PathBuf> {
    let dir = model_dir(paths);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("criando diretório do modelo: {dir:?}"))?;

    let weights = dir.join("model.safetensors");
    let config = dir.join("config.json");
    let tokenizer = dir.join("tokenizer.json");

    if weights.exists() && config.exists() && tokenizer.exists() {
        return Ok(dir);
    }

    tracing::info!("Baixando modelo NER (primeira execução, ~676 MB)...");
    crate::output::emit_progress_i18n(
        "Downloading NER model (first run, ~676 MB)...",
        "Baixando modelo NER (primeira execução, ~676 MB)...",
    );

    let api = huggingface_hub::api::sync::Api::new().context("criando cliente HF Hub")?;
    let repo = api.model(MODEL_ID.to_string());

    // v1.0.20 fix P0 primário: tokenizer.json no repo Davlan está apenas em onnx/tokenizer.json.
    // Em v1.0.19 buscávamos da raiz e recebíamos 404, caindo em graceful degradation 100% das vezes.
    // Mapeamos (remote_path, local_filename) para baixar do subfolder mantendo nome plano local.
    for (remote, local) in &[
        ("model.safetensors", "model.safetensors"),
        ("config.json", "config.json"),
        ("onnx/tokenizer.json", "tokenizer.json"),
        ("tokenizer_config.json", "tokenizer_config.json"),
    ] {
        let dest = dir.join(local);
        if !dest.exists() {
            let src = repo
                .get(remote)
                .with_context(|| format!("baixando {remote} do HF Hub"))?;
            std::fs::copy(&src, &dest).with_context(|| format!("copiando {local} para cache"))?;
        }
    }

    Ok(dir)
}

fn load_model(paths: &AppPaths) -> Result<BertNerModel> {
    let dir = ensure_model_files(paths)?;
    BertNerModel::load(&dir)
}

fn apply_regex_prefilter(body: &str) -> Vec<ExtractedEntity> {
    let mut entities = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let add = |entities: &mut Vec<ExtractedEntity>,
               seen: &mut std::collections::HashSet<String>,
               name: &str,
               entity_type: &str| {
        let name = name.trim().to_string();
        if name.len() >= MIN_ENTITY_CHARS && seen.insert(name.clone()) {
            entities.push(ExtractedEntity {
                name,
                entity_type: entity_type.to_string(),
            });
        }
    };

    for m in regex_email().find_iter(body) {
        // v1.0.20: email é "concept" (regex sozinho não distingue pessoa de mailing list/role).
        add(&mut entities, &mut seen, m.as_str(), "concept");
    }
    for m in regex_url().find_iter(body) {
        // v1.0.22: URLs strip de sufixo de markdown (backtick fechando, parens, brackets).
        // Mantidas como entity_type "concept" para preservar rastreabilidade de citações.
        let raw = m.as_str();
        let cleaned = raw
            .trim_end_matches('`')
            .trim_end_matches(',')
            .trim_end_matches('.')
            .trim_end_matches(';')
            .trim_end_matches(')')
            .trim_end_matches(']')
            .trim_end_matches('}');
        add(&mut entities, &mut seen, cleaned, "concept");
    }
    for m in regex_uuid().find_iter(body) {
        add(&mut entities, &mut seen, m.as_str(), "concept");
    }
    for m in regex_all_caps().find_iter(body) {
        let candidate = m.as_str();
        // v1.0.22: filtro consolidado (stopwords + HTTP methods); preserva identificadores com underscore.
        if !is_filtered_all_caps(candidate) {
            add(&mut entities, &mut seen, candidate, "concept");
        }
    }

    entities
}

fn iob_to_entities(tokens: &[String], labels: &[String]) -> Vec<ExtractedEntity> {
    let mut entities: Vec<ExtractedEntity> = Vec::new();
    let mut current_parts: Vec<String> = Vec::new();
    let mut current_type: Option<String> = None;

    let flush =
        |parts: &mut Vec<String>, typ: &mut Option<String>, entities: &mut Vec<ExtractedEntity>| {
            if let Some(t) = typ.take() {
                let name = parts.join(" ").trim().to_string();
                // v1.0.22: filtra single-token entities que sejam stopwords ALL CAPS ou HTTP methods.
                // BERT NER classifica algumas dessas como B-MISC/B-ORG; pós-filtro aqui evita
                // poluir o grafo com verbos/protocolos genéricos.
                let is_single_caps = !name.contains(' ')
                    && name == name.to_uppercase()
                    && name.len() >= MIN_ENTITY_CHARS;
                let should_skip = is_single_caps && is_filtered_all_caps(&name);
                if name.len() >= MIN_ENTITY_CHARS && !should_skip {
                    entities.push(ExtractedEntity {
                        name,
                        entity_type: t,
                    });
                }
                parts.clear();
            }
        };

    for (token, label) in tokens.iter().zip(labels.iter()) {
        if label == "O" {
            flush(&mut current_parts, &mut current_type, &mut entities);
            continue;
        }

        let (prefix, bio_type) = if let Some(rest) = label.strip_prefix("B-") {
            ("B", rest)
        } else if let Some(rest) = label.strip_prefix("I-") {
            ("I", rest)
        } else {
            flush(&mut current_parts, &mut current_type, &mut entities);
            continue;
        };

        let entity_type = match bio_type {
            "DATE" => {
                flush(&mut current_parts, &mut current_type, &mut entities);
                continue;
            }
            "PER" => "person",
            "ORG" => {
                let t = token.to_lowercase();
                if t.contains("lib")
                    || t.contains("sdk")
                    || t.contains("cli")
                    || t.contains("crate")
                    || t.contains("npm")
                {
                    "tool"
                } else {
                    "project"
                }
            }
            "LOC" => "concept",
            other => other,
        };

        if prefix == "B" {
            if token.starts_with("##") {
                // BERT confuso: subword com B-prefix indica continuação de entidade anterior.
                // Anexar à última parte da entidade atual; senão descartar.
                let clean = token.strip_prefix("##").unwrap_or(token.as_str());
                if let Some(last) = current_parts.last_mut() {
                    last.push_str(clean);
                }
                continue;
            }
            flush(&mut current_parts, &mut current_type, &mut entities);
            current_parts.push(token.clone());
            current_type = Some(entity_type.to_string());
        } else if prefix == "I" && current_type.is_some() {
            let clean = token.strip_prefix("##").unwrap_or(token.as_str());
            if token.starts_with("##") {
                if let Some(last) = current_parts.last_mut() {
                    last.push_str(clean);
                }
            } else {
                current_parts.push(clean.to_string());
            }
        }
    }

    flush(&mut current_parts, &mut current_type, &mut entities);
    entities
}

fn build_relationships(entities: &[NewEntity]) -> Vec<NewRelationship> {
    if entities.len() < 2 {
        return Vec::new();
    }

    // v1.0.22: cap configurável via env var (constants::max_relationships_per_memory).
    // Permite usuários com corpus denso aumentar além do default 50.
    let max_rels = crate::constants::max_relationships_per_memory();
    let n = entities.len().min(MAX_ENTS);
    let mut rels: Vec<NewRelationship> = Vec::new();
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();

    let mut hit_cap = false;
    'outer: for i in 0..n {
        if rels.len() >= max_rels {
            hit_cap = true;
            break;
        }

        let mut for_entity = 0usize;
        for j in (i + 1)..n {
            if for_entity >= TOP_K_RELATIONS {
                break;
            }
            if rels.len() >= max_rels {
                hit_cap = true;
                break 'outer;
            }

            let src = &entities[i].name;
            let tgt = &entities[j].name;
            let key = (src.clone(), tgt.clone());

            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);

            rels.push(NewRelationship {
                source: src.clone(),
                target: tgt.clone(),
                relation: DEFAULT_RELATION.to_string(),
                strength: 0.5,
                description: None,
            });
            for_entity += 1;
        }
    }

    // v1.0.20: avisar quando relacionamentos foram truncados antes de cobrir todos os pares possíveis.
    if hit_cap {
        tracing::warn!(
            "relacionamentos truncados em {max_rels} (com {n} entidades, máx teórico era ~{}× combinações)",
            n.saturating_sub(1)
        );
    }

    rels
}

fn run_ner_sliding_window(
    model: &BertNerModel,
    body: &str,
    paths: &AppPaths,
) -> Result<Vec<ExtractedEntity>> {
    let tokenizer_path = model_dir(paths).join("tokenizer.json");
    let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| anyhow::anyhow!("carregando tokenizer NER: {e}"))?;

    let encoding = tokenizer
        .encode(body, false)
        .map_err(|e| anyhow::anyhow!("encoding NER: {e}"))?;

    let all_ids: Vec<u32> = encoding.get_ids().to_vec();
    let all_tokens: Vec<String> = encoding
        .get_tokens()
        .iter()
        .map(|s| s.to_string())
        .collect();

    if all_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut entities: Vec<ExtractedEntity> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut start = 0usize;
    loop {
        let end = (start + MAX_SEQ_LEN).min(all_ids.len());
        let window_ids = &all_ids[start..end];
        let window_tokens = &all_tokens[start..end];
        let attention_mask: Vec<u32> = vec![1u32; window_ids.len()];

        match model.predict(window_ids, &attention_mask) {
            Ok(labels) => {
                let window_ents = iob_to_entities(window_tokens, &labels);
                for ent in window_ents {
                    if seen.insert(ent.name.clone()) {
                        entities.push(ent);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("janela NER falhou (start={start}): {e:#}");
            }
        }

        if end >= all_ids.len() {
            break;
        }
        start += STRIDE;
    }

    Ok(entities)
}

/// v1.0.22 P1: estende entidades com sufixos numéricos hifenizados ou separados por espaço.
/// Casos: GPT extraído mas body contém "GPT-5" → reescreve para "GPT-5".
/// Casos: Claude extraído mas body contém "Claude 4" → reescreve para "Claude 4".
/// Conservador: só estende se sufixo tiver até 6 caracteres e for puramente numérico.
fn extend_with_numeric_suffix(entities: Vec<ExtractedEntity>, body: &str) -> Vec<ExtractedEntity> {
    static SUFFIX_RE: OnceLock<Regex> = OnceLock::new();
    let suffix_re = SUFFIX_RE.get_or_init(|| Regex::new(r"^([\-\s]+\d+(?:\.\d+)?)").unwrap());

    entities
        .into_iter()
        .map(|ent| {
            // Encontra a primeira ocorrência case-sensitive da entidade no body
            if let Some(pos) = body.find(&ent.name) {
                let after_pos = pos + ent.name.len();
                if after_pos < body.len() {
                    let after = &body[after_pos..];
                    if let Some(m) = suffix_re.find(after) {
                        let suffix = m.as_str();
                        // Conservador: limita comprimento total do sufixo a 6 chars
                        if suffix.len() <= 6 {
                            let extended = format!("{}{}", ent.name, suffix);
                            return ExtractedEntity {
                                name: extended,
                                entity_type: ent.entity_type,
                            };
                        }
                    }
                }
            }
            ent
        })
        .collect()
}

fn merge_and_deduplicate(
    regex_ents: Vec<ExtractedEntity>,
    ner_ents: Vec<ExtractedEntity>,
) -> Vec<ExtractedEntity> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut result: Vec<ExtractedEntity> = Vec::new();
    let mut truncated = false;

    let total_input = regex_ents.len() + ner_ents.len();
    for ent in regex_ents.into_iter().chain(ner_ents) {
        let key = ent.name.to_lowercase();
        if seen.insert(key) {
            result.push(ent);
        }
        if result.len() >= MAX_ENTS {
            truncated = true;
            break;
        }
    }

    // v1.0.20: avisar quando truncamento silencioso descarta entidades acima do MAX_ENTS.
    if truncated {
        tracing::warn!(
            "extração truncada em {MAX_ENTS} entidades (entrada tinha {total_input} candidatos antes da deduplicação)"
        );
    }

    result
}

fn to_new_entities(extracted: Vec<ExtractedEntity>) -> Vec<NewEntity> {
    extracted
        .into_iter()
        .map(|e| NewEntity {
            name: e.name,
            entity_type: e.entity_type,
            description: None,
        })
        .collect()
}

pub fn extract_graph_auto(body: &str, paths: &AppPaths) -> Result<ExtractionResult> {
    let regex_entities = apply_regex_prefilter(body);

    let mut bert_used = false;
    let ner_entities = match get_or_init_model(paths) {
        Some(model) => match run_ner_sliding_window(model, body, paths) {
            Ok(ents) => {
                bert_used = true;
                ents
            }
            Err(e) => {
                tracing::warn!("NER falhou, usando apenas regex: {e:#}");
                Vec::new()
            }
        },
        None => Vec::new(),
    };

    let merged = merge_and_deduplicate(regex_entities, ner_entities);
    // v1.0.22: estender entidades NER com sufixos numéricos do body (GPT-5, Claude 4, Python 3).
    let extended = extend_with_numeric_suffix(merged, body);
    let entities = to_new_entities(extended);
    let relationships = build_relationships(&entities);

    let extraction_method = if bert_used {
        "bert+regex".to_string()
    } else {
        "regex-only".to_string()
    };

    Ok(ExtractionResult {
        entities,
        relationships,
        extraction_method,
    })
}

pub struct RegexExtractor;

impl Extractor for RegexExtractor {
    fn extract(&self, body: &str) -> Result<ExtractionResult> {
        let regex_entities = apply_regex_prefilter(body);
        let entities = to_new_entities(regex_entities);
        let relationships = build_relationships(&entities);
        Ok(ExtractionResult {
            entities,
            relationships,
            extraction_method: "regex-only".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_paths() -> AppPaths {
        use std::path::PathBuf;
        AppPaths {
            db: PathBuf::from("/tmp/test.sqlite"),
            models: PathBuf::from("/tmp/test_models"),
        }
    }

    #[test]
    fn regex_email_captura_endereco() {
        let ents = apply_regex_prefilter("contato: fulano@empresa.com.br para mais info");
        // v1.0.20: emails são classificados como "concept" (regex sozinho não distingue pessoa de role).
        assert!(ents
            .iter()
            .any(|e| e.name == "fulano@empresa.com.br" && e.entity_type == "concept"));
    }

    #[test]
    fn regex_all_caps_filtra_palavra_regra_pt() {
        // v1.0.20 fix P1: NUNCA, PROIBIDO, DEVE não devem virar "entidades".
        let ents = apply_regex_prefilter("NUNCA fazer isso. PROIBIDO usar X. DEVE seguir Y.");
        assert!(
            !ents.iter().any(|e| e.name == "NUNCA"),
            "NUNCA deveria ser filtrado como stopword"
        );
        assert!(
            !ents.iter().any(|e| e.name == "PROIBIDO"),
            "PROIBIDO deveria ser filtrado"
        );
        assert!(
            !ents.iter().any(|e| e.name == "DEVE"),
            "DEVE deveria ser filtrado"
        );
    }

    #[test]
    fn regex_all_caps_aceita_constante_com_underscore() {
        // Constantes técnicas tipo MAX_RETRY, TIMEOUT_MS sempre devem ser aceitas.
        let ents = apply_regex_prefilter("configure MAX_RETRY=3 e API_TIMEOUT=30");
        assert!(ents.iter().any(|e| e.name == "MAX_RETRY"));
        assert!(ents.iter().any(|e| e.name == "API_TIMEOUT"));
    }

    #[test]
    fn regex_all_caps_aceita_acronimo_dominio() {
        // Acrônimos legítimos (não-stopword) devem passar: OPENAI, NVIDIA, GOOGLE.
        let ents = apply_regex_prefilter("OPENAI lançou GPT-5 com NVIDIA H100");
        assert!(ents.iter().any(|e| e.name == "OPENAI"));
        assert!(ents.iter().any(|e| e.name == "NVIDIA"));
    }

    #[test]
    fn regex_url_captura_link() {
        let ents = apply_regex_prefilter("veja https://docs.rs/crate para detalhes");
        assert!(ents
            .iter()
            .any(|e| e.name.starts_with("https://") && e.entity_type == "concept"));
    }

    #[test]
    fn regex_uuid_captura_identificador() {
        let ents = apply_regex_prefilter("id=550e8400-e29b-41d4-a716-446655440000 no sistema");
        assert!(ents.iter().any(|e| e.entity_type == "concept"));
    }

    #[test]
    fn regex_all_caps_captura_constante() {
        let ents = apply_regex_prefilter("configure MAX_RETRY e TIMEOUT_MS");
        assert!(ents.iter().any(|e| e.name == "MAX_RETRY"));
        assert!(ents.iter().any(|e| e.name == "TIMEOUT_MS"));
    }

    #[test]
    fn regex_all_caps_ignora_palavras_curtas() {
        let ents = apply_regex_prefilter("use AI em seu projeto");
        assert!(
            !ents.iter().any(|e| e.name == "AI"),
            "AI tem apenas 2 chars, deve ser ignorado"
        );
    }

    #[test]
    fn iob_decodifica_per_para_person() {
        let tokens = vec![
            "John".to_string(),
            "Doe".to_string(),
            "trabalhou".to_string(),
        ];
        let labels = vec!["B-PER".to_string(), "I-PER".to_string(), "O".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert_eq!(ents.len(), 1);
        assert_eq!(ents[0].entity_type, "person");
        assert!(ents[0].name.contains("John"));
    }

    #[test]
    fn iob_strip_subword_b_prefix() {
        // v1.0.21 P0: BERT às vezes emite ##AI com B-prefix (subword confuso).
        // Deve mergear na entidade ativa em vez de criar entidade fantasma "##AI".
        let tokens = vec!["Open".to_string(), "##AI".to_string()];
        let labels = vec!["B-ORG".to_string(), "B-ORG".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert!(
            ents.iter().any(|e| e.name == "OpenAI" || e.name == "Open"),
            "deveria mergear ##AI ou descartar"
        );
    }

    #[test]
    fn iob_subword_orphan_descarta() {
        // v1.0.21 P0: subword órfão sem entidade ativa não deve virar entidade.
        let tokens = vec!["##AI".to_string()];
        let labels = vec!["B-ORG".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert!(
            ents.is_empty(),
            "subword órfão sem entidade ativa deve ser descartado"
        );
    }

    #[test]
    fn iob_descarta_date() {
        let tokens = vec!["Janeiro".to_string(), "2024".to_string()];
        let labels = vec!["B-DATE".to_string(), "I-DATE".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert!(ents.is_empty(), "DATE deve ser descartado");
    }

    #[test]
    fn iob_mapeia_org_para_project() {
        let tokens = vec!["Empresa".to_string()];
        let labels = vec!["B-ORG".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert_eq!(ents[0].entity_type, "project");
    }

    #[test]
    fn iob_mapeia_org_sdk_para_tool() {
        let tokens = vec!["tokio-sdk".to_string()];
        let labels = vec!["B-ORG".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert_eq!(ents[0].entity_type, "tool");
    }

    #[test]
    fn iob_mapeia_loc_para_concept() {
        let tokens = vec!["Brasil".to_string()];
        let labels = vec!["B-LOC".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert_eq!(ents[0].entity_type, "concept");
    }

    #[test]
    fn build_relationships_respeitam_max_rels() {
        let entities: Vec<NewEntity> = (0..20)
            .map(|i| NewEntity {
                name: format!("entidade_{i}"),
                entity_type: "concept".to_string(),
                description: None,
            })
            .collect();
        let rels = build_relationships(&entities);
        let max_rels = crate::constants::max_relationships_per_memory();
        assert!(rels.len() <= max_rels, "deve respeitar max_rels={max_rels}");
    }

    #[test]
    fn build_relationships_sem_duplicatas() {
        let entities: Vec<NewEntity> = (0..5)
            .map(|i| NewEntity {
                name: format!("ent_{i}"),
                entity_type: "concept".to_string(),
                description: None,
            })
            .collect();
        let rels = build_relationships(&entities);
        let mut pares: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        for r in &rels {
            let par = (r.source.clone(), r.target.clone());
            assert!(pares.insert(par), "par duplicado encontrado");
        }
    }

    #[test]
    fn merge_deduplica_por_nome_lowercase() {
        let a = vec![ExtractedEntity {
            name: "Rust".to_string(),
            entity_type: "concept".to_string(),
        }];
        let b = vec![ExtractedEntity {
            name: "rust".to_string(),
            entity_type: "tool".to_string(),
        }];
        let merged = merge_and_deduplicate(a, b);
        assert_eq!(merged.len(), 1, "rust e Rust são a mesma entidade");
    }

    #[test]
    fn regex_extractor_implementa_trait() {
        let extractor = RegexExtractor;
        let result = extractor
            .extract("contato: dev@empresa.io e MAX_TIMEOUT configurado")
            .unwrap();
        assert!(!result.entities.is_empty());
    }

    #[test]
    fn extract_retorna_ok_sem_modelo() {
        // Sem modelo baixado, deve retornar Ok com apenas entidades regex
        let paths = make_paths();
        let body = "contato: teste@exemplo.com com MAX_RETRY=3";
        let result = extract_graph_auto(body, &paths).unwrap();
        assert!(result
            .entities
            .iter()
            .any(|e| e.name.contains("teste@exemplo.com")));
    }
}
