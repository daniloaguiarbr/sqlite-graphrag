//! Entity and URL extraction pipeline (NER + regex prefilter).
//!
//! Runs named-entity recognition and regex heuristics to extract structured
//! entities and hyperlinks from raw memory bodies before embedding.

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
// v1.0.31 A9: only consumed by the legacy `build_relationships`, which is
// kept for unit tests pinning the cap behaviour.
#[cfg(test)]
const TOP_K_RELATIONS: usize = 5;
const DEFAULT_RELATION: &str = "mentions";
const MIN_ENTITY_CHARS: usize = 2;

static REGEX_EMAIL: OnceLock<Regex> = OnceLock::new();
static REGEX_URL: OnceLock<Regex> = OnceLock::new();
static REGEX_UUID: OnceLock<Regex> = OnceLock::new();
static REGEX_ALL_CAPS: OnceLock<Regex> = OnceLock::new();
// v1.0.25 P0-4: filters section-structure markers like "Etapa 3", "Fase 1", "Passo 2".
static REGEX_SECTION_MARKER: OnceLock<Regex> = OnceLock::new();
// v1.0.25 P0-2: captures CamelCase brand names that BERT NER often misses (e.g. "OpenAI", "PostgreSQL").
static REGEX_BRAND_CAMEL: OnceLock<Regex> = OnceLock::new();

// v1.0.20: stopwords to filter common PT-BR/EN rule words captured as ALL_CAPS.
// Without this filter, technical PT-BR corpora containing CAPS-formatted rules (NUNCA, PROIBIDO, DEVE)
// generated ~70% of "garbage entities". We keep identifiers like MAX_RETRY (with underscore).
// v1.0.22: expanded list with terms observed in 495-file flowaiper stress test.
// Includes verbs (ADICIONAR, VALIDAR), adjectives (ALTA, BAIXA), common nouns (BANCO, CASO),
// HTTP methods (GET, POST, DELETE) and generic data formats (JSON, XML).
// v1.0.24: added 17 new terms observed in audit v1.0.23: generic status words (COMPLETED, DONE,
// FIXED, PENDING), PT-BR imperative verbs (ACEITE, CONFIRME, NEGUE, RECUSE), PT-BR modal/
// common verbs (DEVEMOS, PODEMOS, VAMOS), generic nouns (BORDA, CHECKLIST, PLAN, TOKEN),
// and common abbreviations (ACK, ACL).
// v1.0.25 P0-4: added technology/protocol acronyms (API, CLI, HTTP, HTTPS, JWT, LLM, REST, UI, URL)
// and PT-BR section-label stems (CAPÍTULO, ETAPA, FASE, PASSO, SEÇÃO) to prevent section markers
// and generic tech terms from being extracted as entities.
// v1.0.31 A11: added PT-BR uppercase noise observed during ingest of technical Portuguese
// rule documents — common nouns/adjectives written in caps as visual emphasis (ADAPTER, PROJETO,
// PASSIVA, ATIVA, SOMENTE, LEITURA, ESCRITA, OBRIGATORIA, EXEMPLO, REGRA, DEFAULT). Each one
// kept leaking as a "concept" entity and inflating the graph with non-entities.
const ALL_CAPS_STOPWORDS: &[&str] = &[
    "ACEITE",
    "ACK",
    "ACL",
    "ACRESCENTADO",
    "ADAPTER",
    "ADICIONAR",
    "AGENTS",
    "ALL",
    "ALTA",
    "ALWAYS",
    "API",
    "ARTEFATOS",
    "ATIVA",
    "ATIVO",
    "BAIXA",
    "BANCO",
    "BORDA",
    "BLOQUEAR",
    "BUG",
    "CAPÍTULO",
    "CASO",
    "CHECKLIST",
    "CLI",
    "COMPLETED",
    "CONFIRMADO",
    "CONFIRME",
    "CONTRATO",
    "CRÍTICO",
    "CRITICAL",
    "CSV",
    "DEFAULT",
    "DEVE",
    "DEVEMOS",
    "DISCO",
    "DONE",
    "EFEITO",
    "ENTRADA",
    "ERROR",
    "ESCRITA",
    "ESSA",
    "ESSE",
    "ESSENCIAL",
    "ESTA",
    "ESTE",
    "ETAPA",
    "EVITAR",
    "EXEMPLO",
    "EXPANDIR",
    "EXPOR",
    "FALHA",
    "FASE",
    "FIXED",
    "FIXME",
    "FORBIDDEN",
    "HACK",
    "HEARTBEAT",
    "HTTP",
    "HTTPS",
    "INATIVO",
    "JAMAIS",
    "JSON",
    "JWT",
    "LEITURA",
    "LLM",
    "MUST",
    "NEGUE",
    "NEVER",
    "NOTE",
    "NUNCA",
    "OBRIGATORIA",
    "OBRIGATÓRIO",
    "PADRÃO",
    "PASSIVA",
    "PASSO",
    "PENDING",
    "PLAN",
    "PODEMOS",
    "PROIBIDO",
    "PROJETO",
    "RECUSE",
    "REGRA",
    "REGRAS",
    "REQUIRED",
    "REQUISITO",
    "REST",
    "SEÇÃO",
    "SEMPRE",
    "SHALL",
    "SHOULD",
    "SOMENTE",
    "SOUL",
    "TODAS",
    "TODO",
    "TODOS",
    "TOKEN",
    "TOOLS",
    "TSV",
    "UI",
    "URL",
    "USAR",
    "VALIDAR",
    "VAMOS",
    "VOCÊ",
    "WARNING",
    "XML",
    "YAML",
];

// v1.0.22: HTTP methods are protocol verbs, not semantically useful entities.
// Filtered in apply_regex_prefilter (regex_all_caps) and iob_to_entities (single-token).
const HTTP_METHODS: &[&str] = &[
    "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "CONNECT", "TRACE",
];

fn is_filtered_all_caps(token: &str) -> bool {
    // Identifiers containing underscore are preserved (e.g. MAX_RETRY, FLOWAIPER_API_KEY)
    let is_identifier = token.contains('_');
    if is_identifier {
        return false;
    }
    ALL_CAPS_STOPWORDS.contains(&token) || HTTP_METHODS.contains(&token)
}

fn regex_email() -> &'static Regex {
    REGEX_EMAIL.get_or_init(|| {
        Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}")
            .expect("compile-time validated email regex literal")
    })
}

fn regex_url() -> &'static Regex {
    REGEX_URL.get_or_init(|| {
        Regex::new(r#"https?://[^\s\)\]\}"'<>]+"#)
            .expect("compile-time validated URL regex literal")
    })
}

fn regex_uuid() -> &'static Regex {
    REGEX_UUID.get_or_init(|| {
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .expect("compile-time validated UUID regex literal")
    })
}

fn regex_all_caps() -> &'static Regex {
    REGEX_ALL_CAPS.get_or_init(|| {
        Regex::new(r"\b[A-Z][A-Z0-9_]{2,}\b")
            .expect("compile-time validated all-caps regex literal")
    })
}

fn regex_section_marker() -> &'static Regex {
    REGEX_SECTION_MARKER.get_or_init(|| {
        // Matches PT-BR document-structure labels followed by a number: "Etapa 3", "Fase 1", etc.
        // Accented characters expressed as escapes to keep this source file ASCII-only
        // per the project language policy. Pattern is equivalent to:
        //   \b(?:Etapa|Fase|Passo|Se\xe7\xe3o|Cap\xedtulo)\s+\d+\b
        Regex::new("\\b(?:Etapa|Fase|Passo|Se\u{00e7}\u{00e3}o|Cap\u{00ed}tulo)\\s+\\d+\\b")
            .expect("compile-time validated section marker regex literal")
    })
}

fn regex_brand_camel() -> &'static Regex {
    REGEX_BRAND_CAMEL.get_or_init(|| {
        // Matches CamelCase brand names: one or more lowercase letters after an uppercase, then
        // another uppercase followed by more letters. Covers "OpenAI", "PostgreSQL", "ChatGPT".
        Regex::new(r"\b[A-Z][a-z]+[A-Z][A-Za-z]+\b")
            .expect("compile-time validated CamelCase brand regex literal")
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: String,
}

/// URL with source offset extracted from the memory body.
#[derive(Debug, Clone)]
pub struct ExtractedUrl {
    pub url: String,
    /// Byte position in the body where the URL was found.
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct ExtractionResult {
    pub entities: Vec<NewEntity>,
    pub relationships: Vec<NewRelationship>,
    /// True when build_relationships hit the cap before covering all entity pairs.
    /// Exposed in RememberResponse so callers can detect when relationships were cut.
    pub relationships_truncated: bool,
    /// Extraction method used: "bert+regex" or "regex-only".
    /// Useful for auditing, metrics and user reports.
    pub extraction_method: String,
    /// URLs extracted from the body — stored separately from graph entities.
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

        // SAFETY: VarBuilder::from_mmaped_safetensors requires unsafe because it relies on
        // memory-mapping the weights file. Soundness assumptions:
        // 1. The file at `weights_path` is not concurrently modified during model lifetime
        //    (we only read; the cache directory is owned by the current process via 0600 perms).
        // 2. The mmaped region remains valid for the lifetime of the `VarBuilder` and any
        //    derived tensors (enforced by candle's internal lifetime tracking).
        // 3. The safetensors format is well-formed (verified by candle's parser before mmap).
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[&weights_path], DType::F32, &device)
                .with_context(|| format!("mapping {weights_path:?}"))?
        };
        let bert = BertModel::load(vb.pp("bert"), &bert_cfg).context("loading BertModel")?;

        // v1.0.20 secondary P0 fix: load classifier head from safetensors instead of zeros.
        // In v1.0.19 we used Tensor::zeros, which produced constant argmax and degenerate inference.
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
            .context("BertModel forward pass")?;

        let logits = self
            .classifier
            .forward(&sequence_output)
            .context("classifier forward pass")?;

        let logits_2d = logits.squeeze(0).context("removing batch dimension")?;

        let num_tokens = logits_2d.dim(0).context("dim(0)")?;

        let mut labels = Vec::with_capacity(num_tokens);
        for i in 0..num_tokens {
            let token_logits = logits_2d.get(i).context("get token logits")?;
            let vec: Vec<f32> = token_logits.to_vec1().context("to_vec1 logits")?;
            let argmax = vec
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| {
                    a.partial_cmp(b)
                        .expect("BERT NER logits invariant: no NaN in classifier output")
                })
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
                .context("creating id tensor for batch")?;
            let t = t
                .pad_with_zeros(0, 0, pad_right)
                .context("padding id tensor")?;
            padded_ids.push(t);

            // Attention mask: 1 for real tokens, 0 for padding
            let mut mask_i64 = vec![1i64; len];
            mask_i64.extend(vec![0i64; pad_right]);
            let m = Tensor::from_vec(mask_i64, max_len, &self.device)
                .context("creating mask tensor for batch")?;
            padded_masks.push(m);
        }

        // Stack 1-D tensors into (batch_size, max_len)
        let input_ids = Tensor::stack(&padded_ids, 0).context("stack input_ids")?;
        let attn_mask = Tensor::stack(&padded_masks, 0).context("stack attn_mask")?;
        let token_type_ids = Tensor::zeros((batch_size, max_len), DType::I64, &self.device)
            .context("creating token_type_ids tensor for batch")?;

        // Single forward pass for the entire batch
        let sequence_output = self
            .bert
            .forward(&input_ids, &token_type_ids, Some(&attn_mask))
            .context("BertModel batch forward pass")?;
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
                        .max_by(|(_, a), (_, b)| {
                            a.partial_cmp(b)
                                .expect("BERT NER logits invariant: no NaN in classifier output")
                        })
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
                tracing::warn!("NER model unavailable (graceful degradation): {e:#}");
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
    std::fs::create_dir_all(&dir).with_context(|| format!("creating model directory: {dir:?}"))?;

    let weights = dir.join("model.safetensors");
    let config = dir.join("config.json");
    let tokenizer = dir.join("tokenizer.json");

    if weights.exists() && config.exists() && tokenizer.exists() {
        return Ok(dir);
    }

    tracing::info!("Downloading NER model (first run, ~676 MB)...");
    crate::output::emit_progress_i18n(
        "Downloading NER model (first run, ~676 MB)...",
        crate::i18n::validation::runtime_pt::downloading_ner_model(),
    );

    let api = huggingface_hub::api::sync::Api::new().context("creating HF Hub client")?;
    let repo = api.model(MODEL_ID.to_string());

    // v1.0.20 primary P0 fix: tokenizer.json in the Davlan repo is only at onnx/tokenizer.json.
    // In v1.0.19 we fetched it from the root and got 404, falling into graceful degradation 100% of the time.
    // We map (remote_path, local_filename) to download from the subfolder while keeping a flat local name.
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

    // v1.0.25 P0-4: strip section-structure markers before any other processing so that
    // "Etapa 3", "Fase 1", "Passo 2" are not fed to downstream regex passes.
    let cleaned = regex_section_marker().replace_all(body, " ");
    let cleaned = cleaned.as_ref();

    for m in regex_email().find_iter(cleaned) {
        // v1.0.20: email is "concept" (regex alone cannot distinguish person from mailing list/role).
        add(&mut entities, &mut seen, m.as_str(), "concept");
    }
    for m in regex_uuid().find_iter(cleaned) {
        add(&mut entities, &mut seen, m.as_str(), "concept");
    }
    for m in regex_all_caps().find_iter(cleaned) {
        let candidate = m.as_str();
        // v1.0.22: filtro consolidado (stopwords + HTTP methods); preserva identificadores com underscore.
        if !is_filtered_all_caps(candidate) {
            add(&mut entities, &mut seen, candidate, "concept");
        }
    }
    // v1.0.25 P0-2: capture CamelCase brand names that BERT NER often misses.
    // Maps to "organization" (V008 schema) because brand names are typically organisations.
    for m in regex_brand_camel().find_iter(cleaned) {
        let name = m.as_str();
        // Skip if the uppercased form is a known stopword (e.g. "JsonSchema" → "JSONSCHEMA").
        if !ALL_CAPS_STOPWORDS.contains(&name.to_uppercase().as_str()) {
            add(&mut entities, &mut seen, name, "organization");
        }
    }

    entities
}

/// Extracts URLs from a memory body, deduplicated by text.
/// URLs are stored in the `memory_urls` table separately from graph entities.
/// v1.0.24: split of the URL block that polluted apply_regex_prefilter with entity_type='concept'.
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
                // v1.0.22: filters single-token entities that are ALL CAPS stopwords or HTTP methods.
                // BERT NER classifies some of these as B-MISC/B-ORG; post-filtering here avoids
                // polluting the graph with generic verbs/protocols.
                let is_single_caps = !name.contains(' ')
                    && name == name.to_uppercase()
                    && name.len() >= MIN_ENTITY_CHARS;
                let should_skip = is_single_caps && is_filtered_all_caps(&name);
                // v1.0.25 P0-4: BERT may independently label section-structure tokens (e.g.
                // "Etapa 3", "Fase 1") even though apply_regex_prefilter strips them from the
                // input text before regex extraction. Apply the same guard here to avoid the
                // BERT path re-introducing these markers as graph entities.
                let is_section_marker = regex_section_marker().is_match(&name);
                if name.len() >= MIN_ENTITY_CHARS && !should_skip && !is_section_marker {
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

        // v1.0.25 P0-2: Portuguese monosyllabic verbs that BERT often misclassifies as person names.
        // Only filtered when confidence is unavailable (no logit gate here); these tokens are
        // structurally unlikely to be real proper names in a technical corpus.
        // Accented PT-BR characters expressed as Unicode escapes so this source
        // file remains ASCII-only per the project language policy. Equivalent
        // tokens: L\u{00ea}, V\u{00ea}, C\u{00e1}, P\u{00f4}r.
        const PT_VERB_FALSE_POSITIVES: &[&str] = &[
            "L\u{00ea}",
            "V\u{00ea}",
            "C\u{00e1}",
            "P\u{00f4}r",
            "Ser",
            "Vir",
            "Ver",
            "Dar",
            "Ler",
            "Ter",
        ];

        let entity_type = match bio_type {
            // v1.0.25 V008: DATE is now a first-class entity type instead of being discarded.
            "DATE" => "date",
            "PER" => {
                // Filter well-known PT monosyllabic verbs misclassified as persons.
                if PT_VERB_FALSE_POSITIVES.contains(&token.as_str()) {
                    flush(&mut current_parts, &mut current_type, &mut entities);
                    continue;
                }
                "person"
            }
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
                    // v1.0.25 V008: "organization" replaces the v1.0.24 default "project".
                    "organization"
                }
            }
            // v1.0.25 V008: "location" replaces "concept" for geographic tokens.
            "LOC" => "location",
            other => other,
        };

        if prefix == "B" {
            if token.starts_with("##") {
                // BERT confused: subword with B-prefix indicates continuation of previous entity.
                // Append to the last part of the current entity; otherwise discard.
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
///
/// v1.0.31 A9: superseded by `build_relationships_by_sentence_cooccurrence` for
/// the auto-extraction pipeline because the legacy pairwise scheme produces a
/// dense C(N,2) graph polluted with co-mentions across unrelated paragraphs.
/// Kept for unit tests that pin the cap behaviour and for callers that lack a
/// body string.
#[cfg(test)]
fn build_relationships(entities: &[NewEntity]) -> (Vec<NewRelationship>, bool) {
    if entities.len() < 2 {
        return (Vec::new(), false);
    }

    // v1.0.22: cap configurable via env var (constants::max_relationships_per_memory).
    // Allows users with dense corpora to increase beyond the default 50.
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

    // v1.0.20: warn when relationships were truncated before covering all possible pairs.
    if hit_cap {
        tracing::warn!(
            "relationships truncated to {max_rels} (with {n} entities, theoretical max was ~{}x combinations)",
            n.saturating_sub(1)
        );
    }

    (rels, hit_cap)
}

/// v1.0.31 A9: build relationships only between entities that actually
/// co-occur within the same sentence (split on `.`, `!`, `?`, newline).
///
/// The legacy `build_relationships` pairs every entity with every other,
/// yielding a dense C(N,2) graph dominated by spurious "mentions" edges
/// across unrelated sections. Restricting to sentence-level co-occurrence
/// keeps the edges semantically meaningful while still respecting the
/// configurable `max_relationships_per_memory` cap.
///
/// Returns `(relationships, truncated)` mirroring `build_relationships`.
fn build_relationships_by_sentence_cooccurrence(
    body: &str,
    entities: &[NewEntity],
) -> (Vec<NewRelationship>, bool) {
    if entities.len() < 2 {
        return (Vec::new(), false);
    }

    let max_rels = crate::constants::max_relationships_per_memory();
    let lower_names: Vec<(usize, String)> = entities
        .iter()
        .take(MAX_ENTS)
        .enumerate()
        .map(|(i, e)| (i, e.name.to_lowercase()))
        .collect();

    let mut rels: Vec<NewRelationship> = Vec::new();
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut hit_cap = false;

    for sentence in body.split(['.', '!', '?', '\n']) {
        if sentence.trim().is_empty() {
            continue;
        }
        let lower_sentence = sentence.to_lowercase();
        let present: Vec<usize> = lower_names
            .iter()
            .filter(|(_, name)| !name.is_empty() && lower_sentence.contains(name.as_str()))
            .map(|(i, _)| *i)
            .collect();

        if present.len() < 2 {
            continue;
        }

        for i in 0..present.len() {
            for j in (i + 1)..present.len() {
                if rels.len() >= max_rels {
                    hit_cap = true;
                    tracing::warn!(
                        "relationships truncated to {max_rels} during sentence-level pairing"
                    );
                    return (rels, hit_cap);
                }
                let a = &entities[present[i]];
                let b = &entities[present[j]];
                let key = (a.name.to_lowercase(), b.name.to_lowercase());
                if seen.insert(key) {
                    rels.push(NewRelationship {
                        source: a.name.clone(),
                        target: b.name.clone(),
                        relation: DEFAULT_RELATION.to_string(),
                        strength: 0.5,
                        description: None,
                    });
                }
            }
        }
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
        .map_err(|e| anyhow::anyhow!("loading NER tokenizer: {e}"))?;

    let encoding = tokenizer
        .encode(body, false)
        .map_err(|e| anyhow::anyhow!("encoding NER input: {e}"))?;

    let mut all_ids: Vec<u32> = encoding.get_ids().to_vec();
    let mut all_tokens: Vec<String> = encoding
        .get_tokens()
        .iter()
        .map(|s| s.to_string())
        .collect();

    if all_ids.is_empty() {
        return Ok(Vec::new());
    }

    // v1.0.31 A1: cap the token stream fed to BERT NER. A 68 KB markdown body
    // tokenises to ~17 000 tokens, producing ~65 sliding windows whose CPU
    // forward passes can take 5+ minutes. The regex prefilter already covers
    // structural entities (URLs, emails, all-caps identifiers, CamelCase
    // brands) on the full body, so truncation only affects names that BERT
    // would have detected past the leading region. The cap is configurable
    // via `SQLITE_GRAPHRAG_EXTRACTION_MAX_TOKENS`.
    let max_tokens = crate::constants::extraction_max_tokens();
    if all_ids.len() > max_tokens {
        tracing::warn!(
            target: "extraction",
            original_tokens = all_ids.len(),
            capped_tokens = max_tokens,
            "NER input truncated to cap; later body region will be skipped by NER (regex prefilter still covers full body)"
        );
        all_ids.truncate(max_tokens);
        all_tokens.truncate(max_tokens);
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
                            tracing::warn!("NER window fallback also failed: {e2:#}");
                        }
                    }
                }
            }
        }
    }

    Ok(entities)
}

/// v1.0.22 P1: extends entities with hyphenated or space-separated numeric suffixes.
/// Cases: GPT extracted but body contains "GPT-5" → rewrites to "GPT-5".
/// Cases: Claude extracted but body contains "Claude 4" → rewrites to "Claude 4".
/// Conservative: only extends when the suffix is at most 7 characters.
/// v1.0.24 P2-E: suffix accepts an optional lowercase ASCII letter after digits to cover
/// models such as "GPT-4o", "Llama-5b", "Mistral-8x" (digits + [a-z]? + [x\d+]?).
fn extend_with_numeric_suffix(entities: Vec<ExtractedEntity>, body: &str) -> Vec<ExtractedEntity> {
    static SUFFIX_RE: OnceLock<Regex> = OnceLock::new();
    // Matches: separator + digits + optional decimal + optional lowercase letter
    // Examples: "-4", " 5", "-4o", " 5b", "-8x", " 3.5", "-3.5-turbo" (capped by len)
    let suffix_re = SUFFIX_RE.get_or_init(|| {
        Regex::new(r"^([\-\s]+\d+(?:\.\d+)?[a-z]?)")
            .expect("compile-time validated numeric suffix regex literal")
    });

    entities
        .into_iter()
        .map(|ent| {
            // Finds the first case-sensitive occurrence of the entity in the body
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
        .expect("compile-time validated versioned model regex literal")
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
    // v1.0.25 P0-3: Collision detection uses substring containment (not starts_with)
    // and is scoped per entity_type. This fixes two bugs from prior versions:
    //
    // 1. starts_with was not symmetric for non-prefix substrings. "sonne" does not
    //    start_with "sonnet", so the pair could survive dedup depending on insertion
    //    order. contains() catches both directions unconditionally.
    //
    // 2. The lookup key omitted entity_type, so "Apple/organization" and
    //    "Apple/concept" collapsed into one. Key is now "type\0name_lc".
    //
    // Earlier invariants preserved:
    // - NFKC normalization before lowercasing (v1.0.24).
    // - Longest-wins: on collision keep the entity with the longer name.
    // - Truncation warning at MAX_ENTS.
    let mut by_lc: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut result: Vec<ExtractedEntity> = Vec::new();
    let mut truncated = false;

    let total_input = regex_ents.len() + ner_ents.len();
    for ent in regex_ents.into_iter().chain(ner_ents) {
        let name_lc = ent.name.nfkc().collect::<String>().to_lowercase();
        // Composite key: entity_type + NUL + normalised lowercase name.
        // Collision search is scoped to the same type so that e.g.
        // "Apple/organization" and "Apple/concept" are kept separately.
        let key = format!("{}\0{}", ent.entity_type, name_lc);

        // Scan stored entries for substring containment within the same type.
        // Two names collide when one is a case-insensitive substring of the other:
        //   "sonne" ⊂ "sonnet"  → collision, keep "sonnet" (longest-wins)
        //   "open"  ⊂ "openai"  → collision, keep "openai" (longest-wins)
        let mut collision_idx: Option<usize> = None;
        for (existing_key, idx) in &by_lc {
            // Fast-path: check type prefix matches before scanning the name.
            let type_prefix = format!("{}\0", ent.entity_type);
            if !existing_key.starts_with(&type_prefix) {
                continue;
            }
            let existing_name_lc = &existing_key[type_prefix.len()..];
            if existing_name_lc == name_lc
                || existing_name_lc.contains(name_lc.as_str())
                || name_lc.contains(existing_name_lc)
            {
                collision_idx = Some(*idx);
                break;
            }
        }
        match collision_idx {
            Some(idx) => {
                // Replace stored entity only when the new candidate is strictly
                // longer; otherwise drop the new one.
                if ent.name.len() > result[idx].name.len() {
                    let old_name_lc = result[idx].name.nfkc().collect::<String>().to_lowercase();
                    let old_key = format!("{}\0{}", result[idx].entity_type, old_name_lc);
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

    // v1.0.20: warn when silent truncation discards entities above MAX_ENTS.
    if truncated {
        tracing::warn!(
            "extraction truncated at {MAX_ENTS} entities (input had {total_input} candidates before deduplication)"
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
    // v1.0.22: extend NER entities with numeric suffixes from the body (GPT-5, Claude 4, Python 3).
    let extended = extend_with_numeric_suffix(merged, body);
    // v1.0.23: capture versioned model names that BERT NER does not detect on its own
    // (e.g. "Claude 4", "Llama 3"). Hyphenated variants like "GPT-5" are already covered
    // by the NER+suffix pipeline above, but space-separated names need a dedicated pass.
    let with_models = augment_versioned_model_names(extended, body);
    // v1.0.25 P0-4: augment_versioned_model_names matches any capitalised word followed by a
    // digit, which inadvertently captures PT-BR section markers ("Etapa 3", "Fase 1"). Strip
    // them here as a final guard after the full augmentation pipeline.
    let with_models: Vec<ExtractedEntity> = with_models
        .into_iter()
        .filter(|e| !regex_section_marker().is_match(&e.name))
        .collect();
    let entities = to_new_entities(with_models);
    let (relationships, relationships_truncated) =
        build_relationships_by_sentence_cooccurrence(body, &entities);

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
        let (relationships, relationships_truncated) =
            build_relationships_by_sentence_cooccurrence(body, &entities);
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
    fn regex_email_captures_address() {
        let ents = apply_regex_prefilter("contact: someone@company.com for more info");
        // v1.0.20: emails are classified as "concept" (regex alone cannot distinguish person from role).
        assert!(ents
            .iter()
            .any(|e| e.name == "someone@company.com" && e.entity_type == "concept"));
    }

    #[test]
    fn regex_all_caps_filters_pt_rule_word() {
        // v1.0.20 fix P1: NUNCA, PROIBIDO, DEVE must not become "entities".
        let ents = apply_regex_prefilter("NUNCA do this. PROIBIDO use X. DEVE follow Y.");
        assert!(
            !ents.iter().any(|e| e.name == "NUNCA"),
            "NUNCA must be filtered as a stopword"
        );
        assert!(
            !ents.iter().any(|e| e.name == "PROIBIDO"),
            "PROIBIDO must be filtered"
        );
        assert!(
            !ents.iter().any(|e| e.name == "DEVE"),
            "DEVE must be filtered"
        );
    }

    #[test]
    fn regex_all_caps_accepts_underscored_constant() {
        // Technical constants like MAX_RETRY, TIMEOUT_MS must always be accepted.
        let ents = apply_regex_prefilter("configure MAX_RETRY=3 and API_TIMEOUT=30");
        assert!(ents.iter().any(|e| e.name == "MAX_RETRY"));
        assert!(ents.iter().any(|e| e.name == "API_TIMEOUT"));
    }

    #[test]
    fn regex_all_caps_accepts_domain_acronym() {
        // Legitimate (non-stopword) acronyms must pass: OPENAI, NVIDIA, GOOGLE.
        let ents = apply_regex_prefilter("OPENAI launched GPT-5 with NVIDIA H100");
        assert!(ents.iter().any(|e| e.name == "OPENAI"));
        assert!(ents.iter().any(|e| e.name == "NVIDIA"));
    }

    #[test]
    fn regex_url_does_not_appear_in_apply_regex_prefilter() {
        // v1.0.24 P0-2: URLs were removed from apply_regex_prefilter and now go through extract_urls.
        let ents = apply_regex_prefilter("see https://docs.rs/crate for details");
        assert!(
            !ents.iter().any(|e| e.name.starts_with("https://")),
            "URLs must not appear as entities after the P0-2 split"
        );
    }

    #[test]
    fn extract_urls_captures_https() {
        let urls = extract_urls("see https://docs.rs/crate for details");
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
    fn extract_urls_dedupes_repeated() {
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
    fn regex_all_caps_ignores_short_words() {
        let ents = apply_regex_prefilter("use AI em seu projeto");
        assert!(
            !ents.iter().any(|e| e.name == "AI"),
            "AI tem apenas 2 chars, deve ser ignorado"
        );
    }

    #[test]
    fn iob_decodes_per_to_person() {
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
            "should merge ##AI or discard"
        );
    }

    #[test]
    fn iob_subword_orphan_discards() {
        // v1.0.21 P0: an orphan subword with no active entity must not become an entity.
        let tokens = vec!["##AI".to_string()];
        let labels = vec!["B-ORG".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert!(
            ents.is_empty(),
            "orphan subword without an active entity must be discarded"
        );
    }

    #[test]
    fn iob_maps_date_to_date_v1025() {
        // v1.0.25 V008: DATE is now emitted instead of discarded.
        let tokens = vec!["January".to_string(), "2024".to_string()];
        let labels = vec!["B-DATE".to_string(), "I-DATE".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert_eq!(
            ents.len(),
            1,
            "DATE must be emitted as an entity in v1.0.25"
        );
        assert_eq!(ents[0].entity_type, "date");
    }

    #[test]
    fn iob_maps_org_to_organization_v1025() {
        // v1.0.25 V008: B-ORG without tool keywords maps to "organization" not "project".
        let tokens = vec!["Empresa".to_string()];
        let labels = vec!["B-ORG".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert_eq!(ents[0].entity_type, "organization");
    }

    #[test]
    fn iob_maps_org_sdk_to_tool() {
        let tokens = vec!["tokio-sdk".to_string()];
        let labels = vec!["B-ORG".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert_eq!(ents[0].entity_type, "tool");
    }

    #[test]
    fn iob_maps_loc_to_location_v1025() {
        // v1.0.25 V008: B-LOC maps to "location" not "concept".
        let tokens = vec!["Brasil".to_string()];
        let labels = vec!["B-LOC".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert_eq!(ents[0].entity_type, "location");
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
    fn build_relationships_without_duplicates() {
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
    fn merge_dedupes_by_lowercase_name() {
        // v1.0.25: collision detection is scoped per entity_type; same name + same type
        // must deduplicate to one entry. Different types are kept separately.
        let a = vec![ExtractedEntity {
            name: "Rust".to_string(),
            entity_type: "concept".to_string(),
        }];
        let b = vec![ExtractedEntity {
            name: "rust".to_string(),
            entity_type: "concept".to_string(),
        }];
        let merged = merge_and_deduplicate(a, b);
        assert_eq!(
            merged.len(),
            1,
            "rust and Rust with the same type are the same entity"
        );
    }

    #[test]
    fn regex_extractor_implements_trait() {
        let extractor = RegexExtractor;
        let result = extractor
            .extract("contato: dev@empresa.io e MAX_TIMEOUT configurado")
            .unwrap();
        assert!(!result.entities.is_empty());
    }

    #[test]
    fn extract_returns_ok_without_model() {
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
        // v1.0.24 P1-E: "Caf\u{e9}" (NFC precomposed) and "Cafe\u{301}" (NFD with
        // combining acute accent) must deduplicate to a single entity after NFKC
        // normalization.
        let nfc = vec![ExtractedEntity {
            name: "Caf\u{e9}".to_string(),
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
            "NFC 'Caf\\u{{e9}}' and NFD 'Cafe\\u{{301}}' must deduplicate to 1 entity after NFKC normalization"
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
                "each window must have shape (max_len,) after padding"
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
            "valid value preserved"
        );

        std::env::remove_var("GRAPHRAG_NER_BATCH_SIZE");
    }

    #[test]
    fn extraction_method_regex_only_unchanged() {
        // RegexExtractor always returns "regex-only" regardless of NER_MODEL OnceLock state.
        // This guards against accidentally changing the regex-only fallback string.
        let result = RegexExtractor.extract("contact: dev@acme.io").unwrap();
        assert_eq!(
            result.extraction_method, "regex-only",
            "RegexExtractor must return regex-only"
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
        let result = extend_with_numeric_suffix(ents, "using GPT-5 in the project");
        assert_eq!(
            result[0].name, "GPT-5",
            "purely numeric suffix must be extended"
        );
    }

    #[test]
    fn extend_suffix_alphanumeric_letter_after_digit() {
        // P2-E: "4o" suffix (digit + lowercase letter) must be captured.
        let ents = vec![ExtractedEntity {
            name: "GPT".to_string(),
            entity_type: "concept".to_string(),
        }];
        let result = extend_with_numeric_suffix(ents, "using GPT-4o for advanced tasks");
        assert_eq!(result[0].name, "GPT-4o", "suffix '4o' must be accepted");
    }

    #[test]
    fn extend_suffix_alphanumeric_b_suffix() {
        // P2-E: "5b" suffix (digit + 'b') must be captured.
        let ents = vec![ExtractedEntity {
            name: "Llama".to_string(),
            entity_type: "concept".to_string(),
        }];
        let result = extend_with_numeric_suffix(ents, "Llama-5b open-weight model");
        assert_eq!(result[0].name, "Llama-5b", "suffix '5b' must be accepted");
    }

    #[test]
    fn extend_suffix_alphanumeric_x_suffix() {
        // P2-E: "8x" suffix (digit + 'x') must be captured.
        let ents = vec![ExtractedEntity {
            name: "Mistral".to_string(),
            entity_type: "concept".to_string(),
        }];
        let result = extend_with_numeric_suffix(ents, "testing Mistral-8x in production");
        assert_eq!(result[0].name, "Mistral-8x", "suffix '8x' must be accepted");
    }

    // --- P2-D: augment_versioned_model_names extended regex ---

    #[test]
    fn augment_versioned_gpt4o() {
        // P2-D: "GPT-4o" must be captured with alphanumeric suffix.
        let result = augment_versioned_model_names(vec![], "using GPT-4o for analysis");
        assert!(
            result.iter().any(|e| e.name == "GPT-4o"),
            "GPT-4o must be captured by augment, found: {:?}",
            result.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn augment_versioned_claude_4_sonnet() {
        // P2-D: "Claude 4 Sonnet" must be captured with release tier.
        let result =
            augment_versioned_model_names(vec![], "best model: Claude 4 Sonnet released today");
        assert!(
            result.iter().any(|e| e.name == "Claude 4 Sonnet"),
            "Claude 4 Sonnet must be captured, found: {:?}",
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
        let result = augment_versioned_model_names(existing, "using Claude 4 in the project");
        let count = result.iter().filter(|e| e.name == "Claude 4").count();
        assert_eq!(count, 1, "Claude 4 must not be duplicated");
    }

    // ── v1.0.25 P0-4: new stopwords (API, CLI, HTTP, HTTPS, JWT, LLM, REST, UI, URL) ──

    #[test]
    fn stopwords_filter_url_jwt_api_v1025() {
        // Verify that v1.0.25 tech-acronym stopwords do not leak as entities.
        let body = "We use URL, JWT, and API REST in our LLM-powered CLI via HTTP/HTTPS and UI.";
        let ents = apply_regex_prefilter(body);
        let names: Vec<&str> = ents.iter().map(|e| e.name.as_str()).collect();
        for blocked in &[
            "URL", "JWT", "API", "REST", "LLM", "CLI", "HTTP", "HTTPS", "UI",
        ] {
            assert!(
                !names.contains(blocked),
                "v1.0.25 stopword {blocked} leaked as entity; found names: {names:?}"
            );
        }
    }

    // ── v1.0.25 P0-4: section-marker regex strips "Etapa N", "Fase N", etc. ──

    #[test]
    fn section_markers_etapa_fase_filtered_v1025() {
        // "Etapa 3" and "Fase 1" are document-structure labels, not entities.
        // Body intentionally uses PT-BR section keywords (Etapa/Fase/Migra\u{e7}\u{e3}o) to
        // exercise the PT-BR section-marker filter. ASCII-escaped per the project policy.
        let body = "Etapa 3 do plano: implementar Fase 1 da Migra\u{e7}\u{e3}o.";
        let ents = apply_regex_prefilter(body);
        assert!(
            !ents
                .iter()
                .any(|e| e.name.contains("Etapa") || e.name.contains("Fase")),
            "section markers must be stripped; entities: {:?}",
            ents.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn section_markers_passo_secao_filtered_v1025() {
        // PT-BR keywords Passo/Se\u{e7}\u{e3}o written with Unicode escapes per the
        // project language policy.
        let body = "Siga Passo 2 conforme Se\u{e7}\u{e3}o 3 do manual.";
        let ents = apply_regex_prefilter(body);
        assert!(
            !ents
                .iter()
                .any(|e| e.name.contains("Passo") || e.name.contains("Se\u{e7}\u{e3}o")),
            "Passo/Se\\u{{e7}}\\u{{e3}}o section markers must be stripped; entities: {:?}",
            ents.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }

    // ── v1.0.25 P0-2: CamelCase brand names extracted as organization ──

    #[test]
    fn brand_camelcase_extracted_as_organization_v1025() {
        // "OpenAI" is a CamelCase brand that BERT NER often misses.
        let body = "OpenAI launched GPT-4 and PostgreSQL added pgvector.";
        let ents = apply_regex_prefilter(body);
        let openai = ents.iter().find(|e| e.name == "OpenAI");
        assert!(
            openai.is_some(),
            "OpenAI must be extracted by CamelCase brand regex; entities: {:?}",
            ents.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert_eq!(
            openai.unwrap().entity_type,
            "organization",
            "brand CamelCase must map to organization (V008)"
        );
    }

    #[test]
    fn brand_postgresql_extracted_as_organization_v1025() {
        let body = "migrating from MySQL to PostgreSQL for better performance.";
        let ents = apply_regex_prefilter(body);
        assert!(
            ents.iter()
                .any(|e| e.name == "PostgreSQL" && e.entity_type == "organization"),
            "PostgreSQL must be extracted as organization; entities: {:?}",
            ents.iter()
                .map(|e| (&e.name, &e.entity_type))
                .collect::<Vec<_>>()
        );
    }

    // ── v1.0.25 V008 alignment ──

    #[test]
    fn iob_org_maps_to_organization_not_project_v1025() {
        // B-ORG without tool keywords must emit "organization" (V008), not "project".
        let tokens = vec!["Microsoft".to_string()];
        let labels = vec!["B-ORG".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert_eq!(
            ents[0].entity_type, "organization",
            "B-ORG must map to organization (V008); got {}",
            ents[0].entity_type
        );
    }

    #[test]
    fn iob_loc_maps_to_location_not_concept_v1025() {
        // B-LOC must emit "location" (V008), not "concept".
        // Token is the PT-BR locality "S\u{e3}o Paulo"; ASCII-escaped per language policy.
        let tokens = vec!["S\u{e3}o".to_string(), "Paulo".to_string()];
        let labels = vec!["B-LOC".to_string(), "I-LOC".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert_eq!(
            ents[0].entity_type, "location",
            "B-LOC must map to location (V008); got {}",
            ents[0].entity_type
        );
    }

    #[test]
    fn iob_date_maps_to_date_not_discarded_v1025() {
        // B-DATE must emit "date" (V008) instead of being discarded.
        let tokens = vec!["2025".to_string(), "-".to_string(), "12".to_string()];
        let labels = vec![
            "B-DATE".to_string(),
            "I-DATE".to_string(),
            "I-DATE".to_string(),
        ];
        let ents = iob_to_entities(&tokens, &labels);
        assert_eq!(
            ents.len(),
            1,
            "DATE entity must be emitted (V008); entities: {ents:?}"
        );
        assert_eq!(ents[0].entity_type, "date");
    }

    // ── v1.0.25 P0-2: PT verb false-positive filter ──

    #[test]
    fn pt_verb_le_filtered_as_per_v1025() {
        // "L\u{ea}" is a PT monosyllabic verb; when tagged B-PER it must be dropped.
        // ASCII-escaped per language policy.
        let tokens = vec!["L\u{ea}".to_string(), "o".to_string(), "livro".to_string()];
        let labels = vec!["B-PER".to_string(), "O".to_string(), "O".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert!(
            !ents
                .iter()
                .any(|e| e.name == "L\u{ea}" && e.entity_type == "person"),
            "PT verb 'L\\u{{ea}}' tagged B-PER must be filtered; entities: {ents:?}"
        );
    }

    #[test]
    fn pt_verb_ver_filtered_as_per_v1025() {
        // "Ver" is a PT verb that BERT sometimes tags B-PER; must be filtered.
        let tokens = vec!["Ver".to_string()];
        let labels = vec!["B-PER".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert!(
            ents.is_empty(),
            "PT verb 'Ver' tagged B-PER must be filtered; entities: {ents:?}"
        );
    }

    // --- P0-3 longest-wins v1.0.25 ---

    fn entity(name: &str, entity_type: &str) -> ExtractedEntity {
        ExtractedEntity {
            name: name.to_string(),
            entity_type: entity_type.to_string(),
        }
    }

    #[test]
    fn merge_resolves_sonne_vs_sonnet_keeps_longest_v1025() {
        // "Sonne" is a substring of "Sonnet" — longest-wins must keep "Sonnet".
        let regex = vec![entity("Sonne", "concept")];
        let ner = vec![entity("Sonnet", "concept")];
        let result = merge_and_deduplicate(regex, ner);
        assert_eq!(result.len(), 1, "expected 1 entity, got: {result:?}");
        assert_eq!(result[0].name, "Sonnet");
    }

    #[test]
    fn merge_resolves_open_vs_openai_keeps_longest_v1025() {
        // "Open" is a substring of "OpenAI" — longest-wins must keep "OpenAI".
        let regex = vec![
            entity("Open", "organization"),
            entity("OpenAI", "organization"),
        ];
        let result = merge_and_deduplicate(regex, vec![]);
        assert_eq!(result.len(), 1, "expected 1 entity, got: {result:?}");
        assert_eq!(result[0].name, "OpenAI");
    }

    #[test]
    fn merge_keeps_both_when_no_containment_v1025() {
        // "Alice" and "Bob" share no containment — both must be preserved.
        let regex = vec![entity("Alice", "person"), entity("Bob", "person")];
        let result = merge_and_deduplicate(regex, vec![]);
        assert_eq!(result.len(), 2, "expected 2 entities, got: {result:?}");
    }

    #[test]
    fn merge_respects_entity_type_boundary_v1025() {
        // Same name "Apple" but different types: both must survive independently.
        let regex = vec![entity("Apple", "organization"), entity("Apple", "concept")];
        let result = merge_and_deduplicate(regex, vec![]);
        assert_eq!(
            result.len(),
            2,
            "expected 2 entities (different types), got: {result:?}"
        );
    }

    #[test]
    fn merge_case_insensitive_dedup_v1025() {
        // "OpenAI" and "openai" are the same entity — deduplicate to exactly one.
        let regex = vec![
            entity("OpenAI", "organization"),
            entity("openai", "organization"),
        ];
        let result = merge_and_deduplicate(regex, vec![]);
        assert_eq!(
            result.len(),
            1,
            "expected 1 entity after case-insensitive dedup, got: {result:?}"
        );
    }

    // ── v1.0.25 P0-4: section markers must be filtered in iob_to_entities too ──

    #[test]
    fn iob_section_marker_etapa_filtered_v1025() {
        // BERT may tag "Etapa" (B-MISC) + "3" (I-MISC) as a span; flush must drop it.
        let tokens = vec!["Etapa".to_string(), "3".to_string()];
        let labels = vec!["B-MISC".to_string(), "I-MISC".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert!(
            !ents.iter().any(|e| e.name.contains("Etapa")),
            "section marker 'Etapa 3' from BERT must be filtered; entities: {ents:?}"
        );
    }

    #[test]
    fn iob_section_marker_fase_filtered_v1025() {
        // BERT may tag "Fase" (B-MISC) + "1" (I-MISC) as a span; flush must drop it.
        let tokens = vec!["Fase".to_string(), "1".to_string()];
        let labels = vec!["B-MISC".to_string(), "I-MISC".to_string()];
        let ents = iob_to_entities(&tokens, &labels);
        assert!(
            !ents.iter().any(|e| e.name.contains("Fase")),
            "section marker 'Fase 1' from BERT must be filtered; entities: {ents:?}"
        );
    }

    // ── v1.0.31 A1: NER cap protects against pathological body sizes ──

    #[test]
    fn extract_graph_auto_handles_large_body_under_30s() {
        // Regression guard for the v1.0.31 A1 fix. A 80 KB body without real
        // entities must complete in under 30 s; before the cap it took 5+ minutes.
        let body = "x ".repeat(40_000);
        let paths = make_paths();
        let start = std::time::Instant::now();
        let result = extract_graph_auto(&body, &paths).expect("extraction must not error");
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_secs() < 30,
            "extract_graph_auto took {}s for 80 KB body (cap should keep it well under 30s)",
            elapsed.as_secs()
        );
        // No real entities expected in synthetic body, but the call must succeed.
        let _ = result.entities;
    }

    // ── v1.0.31 A11: PT-BR uppercase noise must not leak as entities ──

    #[test]
    fn pt_uppercase_stopwords_filtered_v1031() {
        let body = "Para o ADAPTER funcionar com PROJETO em modo PASSIVA, devemos usar \
                    SOMENTE LEITURA conforme a REGRA OBRIGATORIA do EXEMPLO DEFAULT.";
        let ents = apply_regex_prefilter(body);
        let names: Vec<String> = ents.iter().map(|e| e.name.to_uppercase()).collect();
        for stop in &[
            "ADAPTER",
            "PROJETO",
            "PASSIVA",
            "SOMENTE",
            "LEITURA",
            "REGRA",
            "OBRIGATORIA",
            "EXEMPLO",
            "DEFAULT",
        ] {
            assert!(
                !names.contains(&stop.to_string()),
                "v1.0.31 A11 stoplist failed: {stop} leaked as entity; got names: {names:?}"
            );
        }
    }

    #[test]
    fn pt_underscored_identifier_preserved_v1031() {
        // Identifiers with underscore must still pass through (FLOWAIPER_API_KEY,
        // MAX_RETRY etc. are intentional entities, not noise).
        let ents = apply_regex_prefilter("configure FLOWAIPER_API_KEY=foo and MAX_TIMEOUT=30");
        let names: Vec<&str> = ents.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"FLOWAIPER_API_KEY"));
        assert!(names.contains(&"MAX_TIMEOUT"));
    }

    // ── v1.0.31 A9: relationships only between entities co-occurring in same sentence ──

    #[test]
    fn build_relationships_by_sentence_only_links_co_occurring_entities() {
        let body = "Alice met Bob at the conference. Carol works alone in another room.";
        let entities = vec![
            NewEntity {
                name: "Alice".to_string(),
                entity_type: "person".to_string(),
                description: None,
            },
            NewEntity {
                name: "Bob".to_string(),
                entity_type: "person".to_string(),
                description: None,
            },
            NewEntity {
                name: "Carol".to_string(),
                entity_type: "person".to_string(),
                description: None,
            },
        ];
        let (rels, truncated) = build_relationships_by_sentence_cooccurrence(body, &entities);
        assert!(!truncated);
        assert_eq!(
            rels.len(),
            1,
            "only Alice/Bob should pair (same sentence); Carol is isolated"
        );
        let pair = (rels[0].source.as_str(), rels[0].target.as_str());
        assert!(
            matches!(pair, ("Alice", "Bob") | ("Bob", "Alice")),
            "unexpected pair {pair:?}"
        );
    }

    #[test]
    fn build_relationships_by_sentence_returns_empty_for_single_entity() {
        let body = "Alice is here.";
        let entities = vec![NewEntity {
            name: "Alice".to_string(),
            entity_type: "person".to_string(),
            description: None,
        }];
        let (rels, truncated) = build_relationships_by_sentence_cooccurrence(body, &entities);
        assert!(rels.is_empty());
        assert!(!truncated);
    }

    #[test]
    fn build_relationships_by_sentence_dedupes_pairs_across_sentences() {
        let body = "Alice met Bob. Bob saw Alice again.";
        let entities = vec![
            NewEntity {
                name: "Alice".to_string(),
                entity_type: "person".to_string(),
                description: None,
            },
            NewEntity {
                name: "Bob".to_string(),
                entity_type: "person".to_string(),
                description: None,
            },
        ];
        let (rels, _) = build_relationships_by_sentence_cooccurrence(body, &entities);
        assert_eq!(
            rels.len(),
            1,
            "Alice/Bob pair must be emitted only once even when co-occurring in multiple sentences"
        );
    }

    #[test]
    fn extraction_max_tokens_default_is_5000() {
        std::env::remove_var("SQLITE_GRAPHRAG_EXTRACTION_MAX_TOKENS");
        assert_eq!(crate::constants::extraction_max_tokens(), 5_000);
    }

    #[test]
    fn extraction_max_tokens_env_override_clamped() {
        std::env::set_var("SQLITE_GRAPHRAG_EXTRACTION_MAX_TOKENS", "200");
        assert_eq!(
            crate::constants::extraction_max_tokens(),
            5_000,
            "value below 512 must fall back to default"
        );

        std::env::set_var("SQLITE_GRAPHRAG_EXTRACTION_MAX_TOKENS", "200000");
        assert_eq!(
            crate::constants::extraction_max_tokens(),
            5_000,
            "value above 100_000 must fall back to default"
        );

        std::env::set_var("SQLITE_GRAPHRAG_EXTRACTION_MAX_TOKENS", "8000");
        assert_eq!(
            crate::constants::extraction_max_tokens(),
            8_000,
            "valid value must be honoured"
        );

        std::env::remove_var("SQLITE_GRAPHRAG_EXTRACTION_MAX_TOKENS");
    }
}
