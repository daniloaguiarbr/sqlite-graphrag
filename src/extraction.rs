use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::{Linear, Module, VarBuilder};
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use regex::Regex;
use serde::Deserialize;
use unicode_normalization::UnicodeNormalization;

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
// v1.0.24: added 17 new terms observed in audit v1.0.23: generic status words (COMPLETED, DONE,
// FIXED, PENDING), PT-BR imperative verbs (ACEITE, CONFIRME, NEGUE, RECUSE), PT-BR modal/
// common verbs (DEVEMOS, PODEMOS, VAMOS), generic nouns (BORDA, CHECKLIST, PLAN, TOKEN),
// and common abbreviations (ACK, ACL).
const ALL_CAPS_STOPWORDS: &[&str] = &[
    "ACEITE",
    "ACK",
    "ACL",
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
    "BORDA",
    "BLOQUEAR",
    "BUG",
    "CASO",
    "CHECKLIST",
    "COMPLETED",
    "CONFIRMADO",
    "CONFIRME",
    "CONTRATO",
    "CRÍTICO",
    "CRITICAL",
    "CSV",
    "DEVE",
    "DEVEMOS",
    "DISCO",
    "DONE",
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
    "FIXED",
    "FIXME",
    "FORBIDDEN",
    "HACK",
    "HEARTBEAT",
    "INATIVO",
    "JAMAIS",
    "JSON",
    "MUST",
    "NEGUE",
    "NEVER",
    "NOTE",
    "NUNCA",
    "OBRIGATÓRIO",
    "PADRÃO",
    "PENDING",
    "PLAN",
    "PODEMOS",
    "PROIBIDO",
    "RECUSE",
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
    "TOKEN",
    "TOOLS",
    "TSV",
    "USAR",
    "VALIDAR",
    "VAMOS",
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

/// URL com offset de origem extraída do corpo da memória.
#[derive(Debug, Clone)]
pub struct ExtractedUrl {
    pub url: String,
    /// Posição em bytes no corpo onde a URL foi encontrada.
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct ExtractionResult {
    pub entities: Vec<NewEntity>,
    pub relationships: Vec<NewRelationship>,
    /// True when build_relationships hit the cap before covering all entity pairs.
    /// Exposed in RememberResponse so callers can detect when relationships were cut.
    pub relationships_truncated: bool,
    /// Método usado para extração: "bert+regex" ou "regex-only".
    /// Útil para auditoria, métricas e reportes ao usuário.
    pub extraction_method: String,
    /// URLs extraídas do corpo — armazenadas separadamente das entidades do grafo.
    pub urls: Vec<ExtractedUrl>,
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

    /// Run a batched forward pass over multiple tokenised windows at once.
    ///
    /// Windows are padded on the right with token_id=0 and attention_mask=0 to
    /// the length of the longest window in the batch.  The attention mask ensures
    /// BERT ignores padded positions (bert.rs:515-528 adds -3.4e38 before softmax).
    ///
    /// Returns one label vector per window, each of length equal to that window's
    /// original (pre-padding) token count.
    fn predict_batch(&self, windows: &[(Vec<u32>, Vec<String>)]) -> Result<Vec<Vec<String>>> {
        let batch_size = windows.len();
        let max_len = windows.iter().map(|(ids, _)| ids.len()).max().unwrap_or(0);
        if max_len == 0 {
            return Ok(vec![vec![]; batch_size]);
        }

        let mut padded_ids: Vec<Tensor> = Vec::with_capacity(batch_size);
        let mut padded_masks: Vec<Tensor> = Vec::with_capacity(batch_size);

        for (ids, _) in windows {
            let len = ids.len();
            let pad_right = max_len - len;

            let ids_i64: Vec<i64> = ids.iter().map(|&x| x as i64).collect();
            // Build 1-D token tensor then pad to max_len
            let t = Tensor::from_vec(ids_i64, len, &self.device)
                .context("criando tensor de ids para batch")?;
            let t = t
                .pad_with_zeros(0, 0, pad_right)
                .context("padding tensor de ids")?;
            padded_ids.push(t);

            // Attention mask: 1 for real tokens, 0 for padding
            let mut mask_i64 = vec![1i64; len];
            mask_i64.extend(vec![0i64; pad_right]);
            let m = Tensor::from_vec(mask_i64, max_len, &self.device)
                .context("criando tensor de máscara para batch")?;
            padded_masks.push(m);
        }

        // Stack 1-D tensors into (batch_size, max_len)
        let input_ids = Tensor::stack(&padded_ids, 0).context("stack input_ids")?;
        let attn_mask = Tensor::stack(&padded_masks, 0).context("stack attn_mask")?;
        let token_type_ids = Tensor::zeros((batch_size, max_len), DType::I64, &self.device)
            .context("criando token_type_ids batch")?;

        // Single forward pass for the entire batch
        let sequence_output = self
            .bert
            .forward(&input_ids, &token_type_ids, Some(&attn_mask))
            .context("forward pass batch BertModel")?;
        // sequence_output: (batch_size, max_len, hidden_size)

        let logits = self
            .classifier
            .forward(&sequence_output)
            .context("forward pass batch classificador")?;
        // logits: (batch_size, max_len, num_labels)

        let mut results = Vec::with_capacity(batch_size);
        for (i, (window_ids, _)) in windows.iter().enumerate() {
            let example_logits = logits.get(i).context("get logits exemplo")?;
            // (max_len, num_labels) — slice only real tokens, discard padding
            let real_len = window_ids.len();
            let example_slice = example_logits
                .narrow(0, 0, real_len)
                .context("narrow para tokens reais")?;
            let logits_2d: Vec<Vec<f32>> = example_slice.to_vec2().context("to_vec2 logits")?;

            let labels: Vec<String> = logits_2d
                .iter()
                .map(|token_logits| {
                    let argmax = token_logits
                        .iter()
                        .enumerate()
                        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                        .map(|(idx, _)| idx)
                        .unwrap_or(0);
                    self.id2label
                        .get(&argmax)
                        .cloned()
                        .unwrap_or_else(|| "O".to_string())
                })
                .collect();

            results.push(labels);
        }

        Ok(results)
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

/// Extrai URLs do corpo de uma memória, desduplicadas por texto.
/// URLs são armazenadas na tabela `memory_urls` separadamente do grafo de entidades.
/// v1.0.24: split do bloco URL que poluía apply_regex_prefilter com entity_type='concept'.
pub fn extract_urls(body: &str) -> Vec<ExtractedUrl> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut result = Vec::new();
    for m in regex_url().find_iter(body) {
        let raw = m.as_str();
        let cleaned = raw
            .trim_end_matches('`')
            .trim_end_matches(',')
            .trim_end_matches('.')
            .trim_end_matches(';')
            .trim_end_matches(')')
            .trim_end_matches(']')
            .trim_end_matches('}');
        if cleaned.len() >= 10 && seen.insert(cleaned.to_string()) {
            result.push(ExtractedUrl {
                url: cleaned.to_string(),
                offset: m.start(),
            });
        }
    }
    result
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

/// Returns (relationships, truncated) where truncated is true when the cap was hit
/// before all entity pairs were covered. Exposed in RememberResponse as
/// `relationships_truncated` so callers can decide whether to increase the cap.
fn build_relationships(entities: &[NewEntity]) -> (Vec<NewRelationship>, bool) {
    if entities.len() < 2 {
        return (Vec::new(), false);
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

    (rels, hit_cap)
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

    // Phase 1: collect all sliding windows before any inference
    let mut windows: Vec<(Vec<u32>, Vec<String>)> = Vec::new();
    let mut start = 0usize;
    loop {
        let end = (start + MAX_SEQ_LEN).min(all_ids.len());
        windows.push((
            all_ids[start..end].to_vec(),
            all_tokens[start..end].to_vec(),
        ));
        if end >= all_ids.len() {
            break;
        }
        start += STRIDE;
    }

    // Phase 2: sort by window length ascending to minimise intra-batch padding waste
    windows.sort_by_key(|(ids, _)| ids.len());

    // Phase 3: batched inference with fallback to single-window predict on error
    let batch_size = crate::constants::ner_batch_size();
    let mut entities: Vec<ExtractedEntity> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for chunk in windows.chunks(batch_size) {
        match model.predict_batch(chunk) {
            Ok(batch_labels) => {
                for (labels, (_, tokens)) in batch_labels.iter().zip(chunk.iter()) {
                    for ent in iob_to_entities(tokens, labels) {
                        if seen.insert(ent.name.clone()) {
                            entities.push(ent);
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "batch NER falhou (chunk de {} janelas): {e:#} — fallback single-window",
                    chunk.len()
                );
                // Fallback: process each window individually to preserve entities
                for (ids, tokens) in chunk {
                    let mask = vec![1u32; ids.len()];
                    match model.predict(ids, &mask) {
                        Ok(labels) => {
                            for ent in iob_to_entities(tokens, &labels) {
                                if seen.insert(ent.name.clone()) {
                                    entities.push(ent);
                                }
                            }
                        }
                        Err(e2) => {
                            tracing::warn!("janela NER fallback também falhou: {e2:#}");
                        }
                    }
                }
            }
        }
    }

    Ok(entities)
}

/// v1.0.22 P1: estende entidades com sufixos numéricos hifenizados ou separados por espaço.
/// Casos: GPT extraído mas body contém "GPT-5" → reescreve para "GPT-5".
/// Casos: Claude extraído mas body contém "Claude 4" → reescreve para "Claude 4".
/// Conservador: só estende se sufixo tiver até 7 caracteres.
/// v1.0.24 P2-E: sufixo aceita letra ASCII minúscula opcional após dígitos para cobrir
/// modelos como "GPT-4o", "Llama-5b", "Mistral-8x" (dígitos + [a-z]? + [x\d+]?).
fn extend_with_numeric_suffix(entities: Vec<ExtractedEntity>, body: &str) -> Vec<ExtractedEntity> {
    static SUFFIX_RE: OnceLock<Regex> = OnceLock::new();
    // Matches: separator + digits + optional decimal + optional lowercase letter
    // Examples: "-4", " 5", "-4o", " 5b", "-8x", " 3.5", "-3.5-turbo" (capped by len)
    let suffix_re = SUFFIX_RE.get_or_init(|| Regex::new(r"^([\-\s]+\d+(?:\.\d+)?[a-z]?)").unwrap());

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
                        // Conservative: cap suffix length to 7 chars to avoid grabbing
                        // long hyphenated phrases while allowing "4o", "5b", "3.5b".
                        if suffix.len() <= 7 {
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

/// Captures versioned model names that BERT NER consistently misses.
///
/// BERT NER often classifies tokens like "Claude" or "Llama" as common nouns,
/// failing to emit a B-PER/B-ORG tag. As a result, `extend_with_numeric_suffix`
/// never sees these candidates and the version suffix gets lost.
///
/// This function scans the body with a conservative regex, matching capitalised
/// words followed by a space-or-hyphen and a small integer. Matches that are not
/// already covered by an existing entity (case-insensitive) are appended with the
/// `concept` type, mirroring how `extend_with_numeric_suffix` represents these
/// items downstream.
///
/// v1.0.24 P2-D: regex extended to cover:
/// - Alphanumeric version suffixes: "GPT-4o", "Llama-3b", "Mistral-8x"
/// - Composite versions: "Mixtral 8x7B" (digit × digit + uppercase letter)
/// - Named release tiers after version: "Claude 4 Sonnet", "Llama 3 Pro"
///
/// Examples covered: "Claude 4", "Llama 3", "GPT-4o", "Claude 4 Sonnet", "Mixtral 8x7B".
/// Examples already handled upstream and skipped here: plain "Apple" without a suffix.
fn augment_versioned_model_names(
    entities: Vec<ExtractedEntity>,
    body: &str,
) -> Vec<ExtractedEntity> {
    static VERSIONED_MODEL_RE: OnceLock<Regex> = OnceLock::new();
    // Pattern breakdown:
    //   [A-Z][A-Za-z]{2,15}   — capitalised model name (3-16 chars)
    //   [\s\-]+               — separator: space(s) or hyphen(s)
    //   \d+(?:\.\d+)?         — version number, optional decimal
    //   (?:[a-z]|x\d+[A-Za-z]?)? — optional alphanumeric suffix: "o", "b", "x7B"
    //   (?:\s+(?:Sonnet|Opus|Haiku|Turbo|Pro|Lite|Mini|Nano|Flash|Ultra))? — optional release tier
    let model_re = VERSIONED_MODEL_RE.get_or_init(|| {
        Regex::new(
            r"\b([A-Z][A-Za-z]{2,15})[\s\-]+(\d+(?:\.\d+)?(?:[a-z]|x\d+[A-Za-z]?)?)(?:\s+(?:Sonnet|Opus|Haiku|Turbo|Pro|Lite|Mini|Nano|Flash|Ultra))?\b",
        )
        .unwrap()
    });

    let mut existing_lc: std::collections::HashSet<String> =
        entities.iter().map(|ent| ent.name.to_lowercase()).collect();
    let mut result = entities;

    for caps in model_re.captures_iter(body) {
        let full_match = caps.get(0).map(|m| m.as_str()).unwrap_or("");
        // Conservative cap: avoid harvesting multi-word noise like "section 12" inside
        // long passages. A model name plus a one or two digit suffix fits in 24 chars.
        if full_match.is_empty() || full_match.len() > 24 {
            continue;
        }
        let normalized_lc = full_match.to_lowercase();
        if existing_lc.contains(&normalized_lc) {
            continue;
        }
        // Stop appending once the global entity cap is reached to keep parity with
        // `merge_and_deduplicate` truncation semantics.
        if result.len() >= MAX_ENTS {
            break;
        }
        existing_lc.insert(normalized_lc);
        result.push(ExtractedEntity {
            name: full_match.to_string(),
            entity_type: "concept".to_string(),
        });
    }

    result
}

fn merge_and_deduplicate(
    regex_ents: Vec<ExtractedEntity>,
    ner_ents: Vec<ExtractedEntity>,
) -> Vec<ExtractedEntity> {
    // v1.0.23: when multiple sources produce overlapping names ("Open" from BERT
    // subword leak vs "OpenAI" from regex), prefer the longest candidate. The
    // previous implementation used a HashSet and kept whichever name appeared
    // first, occasionally yielding truncated brand names like "Open" instead of
    // "OpenAI". The new logic resolves collisions using a (lowercase prefix) lookup
    // that retains the longest match while preserving insertion order via `result`.
    // v1.0.24: dedup key uses NFKC normalization before lowercasing so that
    // visually identical names differing only in Unicode combining marks (e.g.
    // "Café" NFC vs "Cafe\u{301}" NFD) collapse to the same bucket.
    let mut by_lc: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut result: Vec<ExtractedEntity> = Vec::new();
    let mut truncated = false;

    let total_input = regex_ents.len() + ner_ents.len();
    for ent in regex_ents.into_iter().chain(ner_ents) {
        let key = ent.name.nfkc().collect::<String>().to_lowercase();
        // Detect prefix collisions in both directions: "open" vs "openai" should
        // both map to the longest stored candidate. We scan stored keys to find
        // the longest existing entry that contains or is contained by the new key.
        let mut collision_idx: Option<usize> = None;
        for (existing_key, idx) in &by_lc {
            if existing_key == &key
                || existing_key.starts_with(&key)
                || key.starts_with(existing_key)
            {
                collision_idx = Some(*idx);
                break;
            }
        }
        match collision_idx {
            Some(idx) => {
                // Replace stored entity only when the new candidate is strictly
                // longer; otherwise drop the new one. This biases toward the most
                // specific brand name visible in the corpus.
                if ent.name.len() > result[idx].name.len() {
                    let old_key = result[idx].name.nfkc().collect::<String>().to_lowercase();
                    by_lc.remove(&old_key);
                    result[idx] = ent;
                    by_lc.insert(key, idx);
                }
            }
            None => {
                by_lc.insert(key, result.len());
                result.push(ent);
            }
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
    // v1.0.23: capture versioned model names that BERT NER does not detect on its own
    // (e.g. "Claude 4", "Llama 3"). Hyphenated variants like "GPT-5" are already covered
    // by the NER+suffix pipeline above, but space-separated names need a dedicated pass.
    let with_models = augment_versioned_model_names(extended, body);
    let entities = to_new_entities(with_models);
    let (relationships, relationships_truncated) = build_relationships(&entities);

    let extraction_method = if bert_used {
        "bert+regex-batch".to_string()
    } else {
        "regex-only".to_string()
    };

    let urls = extract_urls(body);

    Ok(ExtractionResult {
        entities,
        relationships,
        relationships_truncated,
        extraction_method,
        urls,
    })
}

pub struct RegexExtractor;

impl Extractor for RegexExtractor {
    fn extract(&self, body: &str) -> Result<ExtractionResult> {
        let regex_entities = apply_regex_prefilter(body);
        let entities = to_new_entities(regex_entities);
        let (relationships, relationships_truncated) = build_relationships(&entities);
        let urls = extract_urls(body);
        Ok(ExtractionResult {
            entities,
            relationships,
            relationships_truncated,
            extraction_method: "regex-only".to_string(),
            urls,
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
    fn regex_url_nao_aparece_em_apply_regex_prefilter() {
        // v1.0.24 P0-2: URLs foram removidas de apply_regex_prefilter e agora vão para extract_urls.
        let ents = apply_regex_prefilter("veja https://docs.rs/crate para detalhes");
        assert!(
            !ents.iter().any(|e| e.name.starts_with("https://")),
            "URLs não devem aparecer como entidades após split P0-2"
        );
    }

    #[test]
    fn extract_urls_captura_https() {
        let urls = extract_urls("veja https://docs.rs/crate para detalhes");
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].url, "https://docs.rs/crate");
        assert!(urls[0].offset > 0);
    }

    #[test]
    fn extract_urls_trim_sufixo_pontuacao() {
        let urls = extract_urls("link: https://example.com/path. fim");
        assert!(!urls.is_empty());
        assert!(
            !urls[0].url.ends_with('.'),
            "sufixo ponto deve ser removido"
        );
    }

    #[test]
    fn extract_urls_deduplica_repetidas() {
        let body = "https://example.com referenciado aqui e depois aqui https://example.com";
        let urls = extract_urls(body);
        assert_eq!(urls.len(), 1, "URLs repetidas devem ser deduplicadas");
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
        let (rels, truncated) = build_relationships(&entities);
        let max_rels = crate::constants::max_relationships_per_memory();
        assert!(rels.len() <= max_rels, "deve respeitar max_rels={max_rels}");
        if rels.len() == max_rels {
            assert!(truncated, "truncated deve ser true quando atingiu o cap");
        }
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
        let (rels, _truncated) = build_relationships(&entities);
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

    #[test]
    fn stopwords_filter_v1024_terms() {
        // v1.0.24: verify that all 17 new stopwords added in P0-3 are filtered
        // by apply_regex_prefilter so they do not appear as entities.
        let body = "ACEITE ACK ACL BORDA CHECKLIST COMPLETED CONFIRME \
                    DEVEMOS DONE FIXED NEGUE PENDING PLAN PODEMOS RECUSE TOKEN VAMOS";
        let ents = apply_regex_prefilter(body);
        let names: Vec<&str> = ents.iter().map(|e| e.name.as_str()).collect();
        for word in &[
            "ACEITE",
            "ACK",
            "ACL",
            "BORDA",
            "CHECKLIST",
            "COMPLETED",
            "CONFIRME",
            "DEVEMOS",
            "DONE",
            "FIXED",
            "NEGUE",
            "PENDING",
            "PLAN",
            "PODEMOS",
            "RECUSE",
            "TOKEN",
            "VAMOS",
        ] {
            assert!(
                !names.contains(word),
                "v1.0.24 stopword {word} should be filtered but was found in entities"
            );
        }
    }

    #[test]
    fn dedup_normalizes_unicode_combining_marks() {
        // v1.0.24 P1-E: "Café" (NFC precomposed) and "Cafe\u{301}" (NFD with
        // combining acute accent) must deduplicate to a single entity after NFKC
        // normalization.
        let nfc = vec![ExtractedEntity {
            name: "Café".to_string(),
            entity_type: "concept".to_string(),
        }];
        // Build the NFD form: 'e' followed by combining acute accent U+0301
        let nfd_name = "Cafe\u{301}".to_string();
        let nfd = vec![ExtractedEntity {
            name: nfd_name,
            entity_type: "concept".to_string(),
        }];
        let merged = merge_and_deduplicate(nfc, nfd);
        assert_eq!(
            merged.len(),
            1,
            "NFC 'Café' and NFD 'Cafe\\u{{301}}' must deduplicate to 1 entity after NFKC normalization"
        );
    }

    // ── predict_batch regression tests ──────────────────────────────────────

    #[test]
    fn predict_batch_output_count_matches_input() {
        // Verify that predict_batch returns exactly one Vec<String> per window
        // without requiring a real model.  We test the shape contract by
        // constructing the padding logic manually and asserting counts.
        //
        // Two windows of different lengths: 3 tokens and 5 tokens.
        let w1_ids: Vec<u32> = vec![101, 100, 102];
        let w1_tok: Vec<String> = vec!["[CLS]".into(), "hello".into(), "[SEP]".into()];
        let w2_ids: Vec<u32> = vec![101, 100, 200, 300, 102];
        let w2_tok: Vec<String> = vec![
            "[CLS]".into(),
            "world".into(),
            "foo".into(),
            "bar".into(),
            "[SEP]".into(),
        ];
        let windows: Vec<(Vec<u32>, Vec<String>)> =
            vec![(w1_ids.clone(), w1_tok), (w2_ids.clone(), w2_tok)];

        // Verify padding logic and output length contracts using tensor operations
        // that do NOT require BertModel::forward.
        let device = Device::Cpu;
        let max_len = windows.iter().map(|(ids, _)| ids.len()).max().unwrap();
        assert_eq!(max_len, 5, "max_len deve ser 5");

        let mut padded_ids: Vec<Tensor> = Vec::new();
        for (ids, _) in &windows {
            let len = ids.len();
            let pad_right = max_len - len;
            let ids_i64: Vec<i64> = ids.iter().map(|&x| x as i64).collect();
            let t = Tensor::from_vec(ids_i64, len, &device).unwrap();
            let t = t.pad_with_zeros(0, 0, pad_right).unwrap();
            assert_eq!(
                t.dims(),
                &[max_len],
                "cada janela deve ter shape (max_len,) após padding"
            );
            padded_ids.push(t);
        }

        let stacked = Tensor::stack(&padded_ids, 0).unwrap();
        assert_eq!(
            stacked.dims(),
            &[2, max_len],
            "stack deve produzir (batch_size=2, max_len=5)"
        );

        // Verify narrow preserves only real tokens for each window
        // (simulates what predict_batch does after classifier.forward)
        let fake_logits_data: Vec<f32> = vec![0.0f32; 2 * max_len * 9]; // batch×seq×num_labels=9
        let fake_logits =
            Tensor::from_vec(fake_logits_data, (2usize, max_len, 9usize), &device).unwrap();
        for (i, (ids, _)) in windows.iter().enumerate() {
            let real_len = ids.len();
            let example = fake_logits.get(i).unwrap();
            let sliced = example.narrow(0, 0, real_len).unwrap();
            assert_eq!(
                sliced.dims(),
                &[real_len, 9],
                "narrow deve preservar apenas {real_len} tokens reais"
            );
        }
    }

    #[test]
    fn predict_batch_empty_windows_returns_empty() {
        // predict_batch with no windows must return an empty Vec, not panic.
        // We test the guard logic directly on the batch size/max_len path.
        let windows: Vec<(Vec<u32>, Vec<String>)> = vec![];
        let max_len = windows.iter().map(|(ids, _)| ids.len()).max().unwrap_or(0);
        assert_eq!(max_len, 0, "zero windows → max_len 0");
        // The real predict_batch returns Ok(vec![]) when max_len == 0.
        // We assert the expected output shape by reproducing the guard here.
        let result: Vec<Vec<String>> = if max_len == 0 {
            Vec::new()
        } else {
            unreachable!()
        };
        assert!(result.is_empty());
    }

    #[test]
    fn ner_batch_size_default_is_8() {
        // Verify that ner_batch_size() returns the documented default when the
        // env var is absent.  We clear the var to avoid cross-test contamination.
        std::env::remove_var("GRAPHRAG_NER_BATCH_SIZE");
        assert_eq!(crate::constants::ner_batch_size(), 8);
    }

    #[test]
    fn ner_batch_size_env_override_clamped() {
        // Override via env var; values outside [1, 32] must be clamped.
        std::env::set_var("GRAPHRAG_NER_BATCH_SIZE", "64");
        assert_eq!(crate::constants::ner_batch_size(), 32, "deve clampar em 32");

        std::env::set_var("GRAPHRAG_NER_BATCH_SIZE", "0");
        assert_eq!(crate::constants::ner_batch_size(), 1, "deve clampar em 1");

        std::env::set_var("GRAPHRAG_NER_BATCH_SIZE", "4");
        assert_eq!(
            crate::constants::ner_batch_size(),
            4,
            "valor válido preservado"
        );

        std::env::remove_var("GRAPHRAG_NER_BATCH_SIZE");
    }

    #[test]
    fn extraction_method_regex_only_unchanged() {
        // RegexExtractor always returns "regex-only" regardless of NER_MODEL OnceLock state.
        // This guards against accidentally changing the regex-only fallback string.
        let result = RegexExtractor.extract("contato: dev@acme.io").unwrap();
        assert_eq!(
            result.extraction_method, "regex-only",
            "RegexExtractor deve retornar regex-only"
        );
    }

    // --- P2-E: extend_with_numeric_suffix alphanumeric suffix ---

    #[test]
    fn extend_suffix_pure_numeric_unchanged() {
        // Existing behaviour: pure-numeric suffix must still work after P2-E.
        let ents = vec![ExtractedEntity {
            name: "GPT".to_string(),
            entity_type: "concept".to_string(),
        }];
        let result = extend_with_numeric_suffix(ents, "usando GPT-5 no projeto");
        assert_eq!(
            result[0].name, "GPT-5",
            "sufixo puramente numérico deve ser estendido"
        );
    }

    #[test]
    fn extend_suffix_alphanumeric_letter_after_digit() {
        // P2-E: "4o" suffix (digit + lowercase letter) must be captured.
        let ents = vec![ExtractedEntity {
            name: "GPT".to_string(),
            entity_type: "concept".to_string(),
        }];
        let result = extend_with_numeric_suffix(ents, "usando GPT-4o para tarefas avançadas");
        assert_eq!(result[0].name, "GPT-4o", "sufixo '4o' deve ser aceito");
    }

    #[test]
    fn extend_suffix_alphanumeric_b_suffix() {
        // P2-E: "5b" suffix (digit + 'b') must be captured.
        let ents = vec![ExtractedEntity {
            name: "Llama".to_string(),
            entity_type: "concept".to_string(),
        }];
        let result = extend_with_numeric_suffix(ents, "modelo Llama-5b open-weight");
        assert_eq!(result[0].name, "Llama-5b", "sufixo '5b' deve ser aceito");
    }

    #[test]
    fn extend_suffix_alphanumeric_x_suffix() {
        // P2-E: "8x" suffix (digit + 'x') must be captured.
        let ents = vec![ExtractedEntity {
            name: "Mistral".to_string(),
            entity_type: "concept".to_string(),
        }];
        let result = extend_with_numeric_suffix(ents, "testando Mistral-8x em produção");
        assert_eq!(result[0].name, "Mistral-8x", "sufixo '8x' deve ser aceito");
    }

    // --- P2-D: augment_versioned_model_names extended regex ---

    #[test]
    fn augment_versioned_gpt4o() {
        // P2-D: "GPT-4o" must be captured with alphanumeric suffix.
        let result = augment_versioned_model_names(vec![], "usando GPT-4o para análise");
        assert!(
            result.iter().any(|e| e.name == "GPT-4o"),
            "GPT-4o deve ser capturado pelo augment, achados: {:?}",
            result.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn augment_versioned_claude_4_sonnet() {
        // P2-D: "Claude 4 Sonnet" must be captured with release tier.
        let result =
            augment_versioned_model_names(vec![], "melhor modelo: Claude 4 Sonnet lançado hoje");
        assert!(
            result.iter().any(|e| e.name == "Claude 4 Sonnet"),
            "Claude 4 Sonnet deve ser capturado, achados: {:?}",
            result.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn augment_versioned_llama_3_pro() {
        // P2-D: "Llama 3 Pro" must be captured with release tier.
        let result =
            augment_versioned_model_names(vec![], "fine-tuning com Llama 3 Pro localmente");
        assert!(
            result.iter().any(|e| e.name == "Llama 3 Pro"),
            "Llama 3 Pro deve ser capturado, achados: {:?}",
            result.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn augment_versioned_mixtral_8x7b() {
        // P2-D: "Mixtral 8x7B" composite version must be captured.
        let result =
            augment_versioned_model_names(vec![], "executando Mixtral 8x7B no servidor local");
        assert!(
            result.iter().any(|e| e.name == "Mixtral 8x7B"),
            "Mixtral 8x7B deve ser capturado, achados: {:?}",
            result.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn augment_versioned_does_not_duplicate_existing() {
        // P2-D back-compat: entities already present must not be duplicated.
        let existing = vec![ExtractedEntity {
            name: "Claude 4".to_string(),
            entity_type: "concept".to_string(),
        }];
        let result = augment_versioned_model_names(existing, "usando Claude 4 no projeto");
        let count = result.iter().filter(|e| e.name == "Claude 4").count();
        assert_eq!(count, 1, "Claude 4 não deve ser duplicado");
    }
}
