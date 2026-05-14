//! Entity and URL extraction pipeline (NER + regex prefilter).
//!
//! Runs named-entity recognition and regex heuristics to extract structured
//! entities and hyperlinks from raw memory bodies before embedding.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use ort::session::{builder::GraphOptimizationLevel, Session};
use regex::Regex;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::entity_type::EntityType;
use crate::paths::AppPaths;
use crate::storage::entities::{NewEntity, NewRelationship};

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
// v1.0.25 P0-2: captures CamelCase brand names that NER model often misses (e.g. "OpenAI", "PostgreSQL").
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
    "ACID",
    "ACK",
    "ACL",
    "ACRESCENTADO",
    "ADAPTER",
    "ADICIONADA",
    "ADICIONADAS",
    "ADICIONADO",
    "ADICIONADOS",
    "ADICIONAR",
    "AGENTS",
    "AINDA",
    "ALL",
    "ALTA",
    "ALWAYS",
    "APENAS",
    "API",
    "ARTEFATOS",
    "ATIVA",
    "ATIVO",
    "BAIXA",
    "BANCO",
    "BLOQUEAR",
    "BORDA",
    "BUG",
    "CAPÍTULO",
    "CASO",
    "CEO",
    "CHECKLIST",
    "CLARO",
    "CLAUDE_STREAM_IDLE_TIMEOUT_MS",
    "CLI",
    "COMPLETED",
    "CONFIRMADO",
    "CONFIRMARAM",
    "CONFIRME",
    "CONFIRMEI",
    "CONFIRMOU",
    "CONTRATO",
    "CRIE",
    "CRÍTICO",
    "CRITICAL",
    "CSV",
    "DDL",
    "DEFAULT",
    "DEFINIR",
    "DEPARTMENT",
    "DESC",
    "DEVE",
    "DEVEMOS",
    "DISCO",
    "DONE",
    "DSL",
    "DTO",
    "EFEITO",
    "ENTRADA",
    "EOF",
    "EPERM",
    "ERROR",
    "ESCREVA",
    "ESCRITA",
    "ESRCH",
    "ESSA",
    "ESSE",
    "ESSENCIAL",
    "ESTA",
    "ESTADO",
    "ESTE",
    "ETAPA",
    "EVITAR",
    "EXEMPLO",
    "EXPANDIR",
    "EXPOR",
    "FALHA",
    "FASE",
    "FATO",
    "FIFO",
    "FIXED",
    "FIXME",
    "FLUXO",
    "FONTES",
    "FORBIDDEN",
    "FUNCIONA",
    "GNU",
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
    "MCP",
    "MESMO",
    "METADADOS",
    "MUST",
    "NDJSON",
    "NEGUE",
    "NEVER",
    "NOTE",
    "NUNCA",
    "OBRIGATORIA",
    "OBRIGATÓRIO",
    "OBSERVEI",
    "PADRÃO",
    "PASSIVA",
    "PASSO",
    "PENDING",
    "PGID",
    "PID",
    "PLAN",
    "PODEMOS",
    "PONTEIROS",
    "PREFERIR",
    "PROIBIDO",
    "PROJETO",
    "RECUSE",
    "REGRA",
    "REGRAS",
    "REMOVIDAS",
    "REQUIRED",
    "REQUISITO",
    "REST",
    "SEÇÃO",
    "SEMPRE",
    "SHALL",
    "SHOULD",
    "SIGTERM",
    "SOMENTE",
    "SOUL",
    "TODAS",
    "TODO",
    "TODOS",
    "TOKEN",
    "TOOLS",
    "TSV",
    "TUI",
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
// Filtered in apply_regex_prefilter (regex_all_caps path).
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
    // SAFETY: regex literal validated at compile-time via test::regex_literals_compile
    REGEX_EMAIL.get_or_init(|| {
        Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}")
            .expect("compile-time validated email regex literal")
    })
}

fn regex_url() -> &'static Regex {
    // SAFETY: regex literal validated at compile-time via test::regex_literals_compile
    REGEX_URL.get_or_init(|| {
        Regex::new(r#"https?://[^\s\)\]\}"'<>]+"#)
            .expect("compile-time validated URL regex literal")
    })
}

fn regex_uuid() -> &'static Regex {
    // SAFETY: regex literal validated at compile-time via test::regex_literals_compile
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
        // Matches PT-BR document-structure labels followed by a number: "Etapa 3", "Fase 1",
        // "Camada 5", "Passo 2", etc. v1.0.36 (H5): added "Camada" after audit found
        // "Camada 1".."Camada 5" leaking through into entity extraction with degree>=3.
        // Accented characters expressed as escapes to keep this source file ASCII-only
        // per the project language policy. Pattern is equivalent to:
        //   \b(?:Etapa|Fase|Passo|Camada|Se\xe7\xe3o|Cap\xedtulo)\s+\d+\b
        Regex::new("\\b(?:Etapa|Fase|Passo|Camada|Se\u{00e7}\u{00e3}o|Cap\u{00ed}tulo)\\s+\\d+\\b")
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
    pub entity_type: EntityType,
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
    /// Extraction method used: `"gliner-<variant>+regex"` or `"regex-only"`.
    /// Useful for auditing, metrics and user reports.
    pub extraction_method: String,
    /// URLs extracted from the body — stored separately from graph entities.
    pub urls: Vec<ExtractedUrl>,
}

pub trait Extractor: Send + Sync {
    fn extract(&self, body: &str) -> Result<ExtractionResult>;
}

/// GLiNER ONNX model quantization variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GlinerVariant {
    Fp32,
    Fp16,
    Int8,
    Q4,
    Q4f16,
}

impl GlinerVariant {
    /// ONNX filename for this variant in the HuggingFace repository.
    pub fn as_filename(self) -> &'static str {
        match self {
            Self::Fp32 => "model.onnx",
            Self::Fp16 => "model_fp16.onnx",
            Self::Int8 => "model_quantized.onnx",
            Self::Q4 => "model_q4.onnx",
            Self::Q4f16 => "model_q4f16.onnx",
        }
    }

    /// Approximate model size for user-facing messages.
    pub fn display_size(self) -> &'static str {
        match self {
            Self::Fp32 => "1.1 GB",
            Self::Fp16 => "580 MB",
            Self::Int8 => "349 MB",
            Self::Q4 => "894 MB",
            Self::Q4f16 => "472 MB",
        }
    }
}

impl std::fmt::Display for GlinerVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fp32 => f.write_str("fp32"),
            Self::Fp16 => f.write_str("fp16"),
            Self::Int8 => f.write_str("int8"),
            Self::Q4 => f.write_str("q4"),
            Self::Q4f16 => f.write_str("q4f16"),
        }
    }
}

impl std::str::FromStr for GlinerVariant {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "fp32" => Ok(Self::Fp32),
            "fp16" => Ok(Self::Fp16),
            "int8" => Ok(Self::Int8),
            "q4" => Ok(Self::Q4),
            "q4f16" => Ok(Self::Q4f16),
            other => {
                anyhow::bail!("unknown GLiNER variant: {other}. Valid: fp32, fp16, int8, q4, q4f16")
            }
        }
    }
}

const GLINER_MAX_WIDTH: usize = 12;
const GLINER_MAX_SEQ_LEN: usize = 384;
const GLINER_ENT_TOKEN: &str = "<<ENT>>";
const GLINER_SEP_TOKEN: &str = "<<SEP>>";

const GLINER_ENTITY_LABELS: &[(&str, EntityType)] = &[
    ("person", EntityType::Person),
    ("organization", EntityType::Organization),
    ("location", EntityType::Location),
    ("date", EntityType::Date),
    ("project", EntityType::Project),
    ("tool", EntityType::Tool),
    ("file", EntityType::File),
    ("concept", EntityType::Concept),
    ("decision", EntityType::Decision),
    ("incident", EntityType::Incident),
    ("dashboard", EntityType::Dashboard),
    ("issue tracker", EntityType::IssueTracker),
    ("memory", EntityType::Memory),
];

struct GlinerModel {
    session: std::sync::Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
    #[allow(dead_code)]
    variant: GlinerVariant,
}

impl GlinerModel {
    fn load(model_dir: &Path, variant: GlinerVariant) -> Result<Self> {
        let model_path = model_dir.join(variant.as_filename());
        let tokenizer_path = model_dir.join("tokenizer.json");

        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("creating GLiNER session builder: {e}"))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!("setting optimization level: {e}"))?
            .commit_from_file(&model_path)
            .map_err(|e| anyhow::anyhow!("loading GLiNER ONNX model from {model_path:?}: {e}"))?;

        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("loading GLiNER tokenizer: {e}"))?;

        Ok(Self {
            session: std::sync::Mutex::new(session),
            tokenizer,
            variant,
        })
    }

    fn predict(
        &self,
        body: &str,
        entity_labels: &[(&str, EntityType)],
        threshold: f32,
    ) -> Result<Vec<ExtractedEntity>> {
        let label_names: Vec<&str> = entity_labels.iter().map(|(name, _)| *name).collect();
        let words: Vec<&str> = body.split_whitespace().collect();
        if words.is_empty() {
            return Ok(Vec::new());
        }

        // Cap words to fit within model sequence length (accounting for label tokens)
        let label_token_count = label_names.len() * 2 + 1;
        let max_words = GLINER_MAX_SEQ_LEN.saturating_sub(label_token_count + 2);
        let words = if words.len() > max_words {
            tracing::warn!(
                original_words = words.len(),
                capped_words = max_words,
                "GLiNER input truncated to fit model sequence length"
            );
            &words[..max_words]
        } else {
            &words[..]
        };
        let num_words = words.len();

        // Build prompt: [<<ENT>>, label1, <<ENT>>, label2, ..., <<SEP>>, word1, word2, ...]
        let mut prompt_tokens: Vec<String> =
            Vec::with_capacity(label_names.len() * 2 + 1 + num_words);
        for label in &label_names {
            prompt_tokens.push(GLINER_ENT_TOKEN.to_string());
            prompt_tokens.push((*label).to_string());
        }
        prompt_tokens.push(GLINER_SEP_TOKEN.to_string());
        for word in words {
            prompt_tokens.push((*word).to_string());
        }

        // Encode each token individually (word-by-word encoding per GLiNER protocol)
        let mut all_ids: Vec<i64> = Vec::new();
        let mut all_attention: Vec<i64> = Vec::new();
        let mut all_word_mask: Vec<i64> = Vec::new();

        // BOS token
        all_ids.push(1);
        all_attention.push(1);
        all_word_mask.push(0);

        let text_offset = label_names.len() * 2 + 1;
        let mut word_id: i64 = 0;

        for (pos, token_str) in prompt_tokens.iter().enumerate() {
            let encoding = self
                .tokenizer
                .encode(token_str.as_str(), false)
                .map_err(|e| anyhow::anyhow!("GLiNER tokenizer encode error: {e}"))?;
            let ids = encoding.get_ids();
            let is_text_token = pos >= text_offset;

            for (sub_idx, &id) in ids.iter().enumerate() {
                all_ids.push(id as i64);
                all_attention.push(1);
                if is_text_token && sub_idx == 0 {
                    word_id += 1;
                    all_word_mask.push(word_id);
                } else {
                    all_word_mask.push(0);
                }
            }
        }

        // EOS token
        all_ids.push(2);
        all_attention.push(1);
        all_word_mask.push(0);

        let seq_len = all_ids.len();

        // Build ORT tensors using Tensor::from_array((shape, data)) API
        let t_input_ids = ort::value::Tensor::<i64>::from_array(([1usize, seq_len], all_ids))
            .map_err(|e| anyhow::anyhow!("building input_ids tensor: {e}"))?;
        let t_attention = ort::value::Tensor::<i64>::from_array(([1usize, seq_len], all_attention))
            .map_err(|e| anyhow::anyhow!("building attention_mask tensor: {e}"))?;
        let t_words_mask =
            ort::value::Tensor::<i64>::from_array(([1usize, seq_len], all_word_mask))
                .map_err(|e| anyhow::anyhow!("building words_mask tensor: {e}"))?;
        let t_text_lengths =
            ort::value::Tensor::<i64>::from_array(([1usize, 1usize], vec![num_words as i64]))
                .map_err(|e| anyhow::anyhow!("building text_lengths tensor: {e}"))?;

        // Build span tensors
        let num_spans = num_words * GLINER_MAX_WIDTH;
        let mut span_idx_data = vec![0i64; num_spans * 2];
        let mut span_mask_data = vec![false; num_spans];

        for start in 0..num_words {
            let remaining = num_words - start;
            let actual_max_width = GLINER_MAX_WIDTH.min(remaining);
            for width in 0..actual_max_width {
                let dim = start * GLINER_MAX_WIDTH + width;
                span_idx_data[dim * 2] = start as i64;
                span_idx_data[dim * 2 + 1] = (start + width) as i64;
                span_mask_data[dim] = true;
            }
        }

        let t_span_idx =
            ort::value::Tensor::<i64>::from_array(([1usize, num_spans, 2usize], span_idx_data))
                .map_err(|e| anyhow::anyhow!("building span_idx tensor: {e}"))?;
        let t_span_mask =
            ort::value::Tensor::<bool>::from_array(([1usize, num_spans], span_mask_data))
                .map_err(|e| anyhow::anyhow!("building span_mask tensor: {e}"))?;

        // Run inference — Session::run requires &mut Session; bind guard first.
        let mut session_guard = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("GLiNER session mutex poisoned"))?;
        let outputs = session_guard
            .run(ort::inputs![
                "input_ids" => t_input_ids,
                "attention_mask" => t_attention,
                "words_mask" => t_words_mask,
                "text_lengths" => t_text_lengths,
                "span_idx" => t_span_idx,
                "span_mask" => t_span_mask
            ])
            .map_err(|e| anyhow::anyhow!("GLiNER inference forward pass: {e}"))?;

        // Extract logits: [1, num_words, max_width, num_classes]
        // try_extract_tensor returns (&Shape, &[f32]); index manually.
        let (logits_shape, logits_data) = outputs["logits"]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow::anyhow!("extracting logits tensor: {e}"))?;

        let num_classes = label_names.len();
        // Expected shape: [1, num_words, GLINER_MAX_WIDTH, num_classes]
        // Shape derefs to &[i64] so we can index directly.
        let max_width = logits_shape
            .get(2)
            .copied()
            .unwrap_or(GLINER_MAX_WIDTH as i64) as usize;
        let nc = logits_shape.get(3).copied().unwrap_or(num_classes as i64) as usize;

        let mut candidates: Vec<(usize, usize, usize, f32)> = Vec::new();

        for start in 0..num_words {
            for width in 0..max_width {
                let end = start + width;
                if end >= num_words {
                    break;
                }
                for class_idx in 0..nc.min(num_classes) {
                    // flat index: batch=0 * (num_words*max_width*nc) + start*(max_width*nc) + width*nc + class_idx
                    let flat = start * (max_width * nc) + width * nc + class_idx;
                    if flat >= logits_data.len() {
                        break;
                    }
                    let raw = logits_data[flat];
                    let score = 1.0 / (1.0 + (-raw).exp());
                    if score >= threshold {
                        candidates.push((start, end, class_idx, score));
                    }
                }
            }
        }

        // Sort by score descending for greedy NMS
        candidates.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));

        // Greedy non-maximum suppression
        let mut used = vec![false; num_words];
        let mut entities: Vec<ExtractedEntity> = Vec::new();

        for (start, end, class_idx, _score) in &candidates {
            let overlap = (*start..=*end).any(|i| used[i]);
            if overlap {
                continue;
            }
            for flag in used.iter_mut().take(*end + 1).skip(*start) {
                *flag = true;
            }
            let text = words[*start..=*end].join(" ");
            if text.len() < MIN_ENTITY_CHARS {
                continue;
            }
            let entity_type = entity_labels[*class_idx].1;
            entities.push(ExtractedEntity {
                name: text,
                entity_type,
            });
            if entities.len() >= MAX_ENTS {
                break;
            }
        }

        Ok(entities)
    }
}

static GLINER_MODEL: OnceLock<Option<GlinerModel>> = OnceLock::new();

fn gliner_model_dir(paths: &AppPaths, variant: GlinerVariant) -> PathBuf {
    paths.models.join(format!("gliner-multi-v2.1/{variant}"))
}

fn ensure_gliner_model_files(paths: &AppPaths, variant: GlinerVariant) -> Result<PathBuf> {
    let dir = gliner_model_dir(paths, variant);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating GLiNER model directory: {dir:?}"))?;

    let model_file = dir.join(variant.as_filename());
    let tokenizer_file = dir.join("tokenizer.json");

    if model_file.exists() && tokenizer_file.exists() {
        return Ok(dir);
    }

    let repo = crate::constants::gliner_model_repo();
    tracing::info!(
        "Downloading GLiNER model ({variant}, ~{})...",
        variant.display_size()
    );
    crate::output::emit_progress_i18n(
        &format!(
            "Downloading GLiNER model ({variant}, ~{})...",
            variant.display_size()
        ),
        &format!(
            "Baixando modelo GLiNER ({variant}, ~{})...",
            variant.display_size()
        ),
    );

    let api = huggingface_hub::api::sync::Api::new().context("creating HF Hub client")?;
    let hf_repo = api.model(repo);

    let remote_model = format!("onnx/{}", variant.as_filename());
    if !model_file.exists() {
        let src = hf_repo
            .get(&remote_model)
            .with_context(|| format!("downloading {remote_model} from HF Hub"))?;
        std::fs::copy(&src, &model_file)
            .with_context(|| format!("copying {} to cache", variant.as_filename()))?;
    }

    if !tokenizer_file.exists() {
        let src = hf_repo
            .get("tokenizer.json")
            .context("downloading tokenizer.json from HF Hub")?;
        std::fs::copy(&src, &tokenizer_file).context("copying tokenizer.json to cache")?;
    }

    Ok(dir)
}

fn load_gliner_model(paths: &AppPaths, variant: GlinerVariant) -> Result<GlinerModel> {
    let dir = ensure_gliner_model_files(paths, variant)?;
    GlinerModel::load(&dir, variant)
}

fn get_or_init_gliner(paths: &AppPaths, variant: GlinerVariant) -> Option<&'static GlinerModel> {
    GLINER_MODEL
        .get_or_init(|| match load_gliner_model(paths, variant) {
            Ok(m) => Some(m),
            Err(e) => {
                tracing::warn!("GLiNER model unavailable (graceful degradation): {e:#}");
                None
            }
        })
        .as_ref()
}

fn apply_regex_prefilter(body: &str) -> Vec<ExtractedEntity> {
    let mut entities = Vec::with_capacity(16);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let add = |entities: &mut Vec<ExtractedEntity>,
               seen: &mut std::collections::HashSet<String>,
               name: &str,
               entity_type: EntityType| {
        let name = name.trim().to_string();
        if name.len() >= MIN_ENTITY_CHARS && seen.insert(name.clone()) {
            entities.push(ExtractedEntity { name, entity_type });
        }
    };

    // v1.0.25 P0-4: strip section-structure markers before any other processing so that
    // "Etapa 3", "Fase 1", "Passo 2" are not fed to downstream regex passes.
    let cleaned = regex_section_marker().replace_all(body, " ");
    let cleaned = cleaned.as_ref();

    for m in regex_email().find_iter(cleaned) {
        // v1.0.20: email is "concept" (regex alone cannot distinguish person from mailing list/role).
        add(&mut entities, &mut seen, m.as_str(), EntityType::Concept);
    }
    for m in regex_uuid().find_iter(cleaned) {
        add(&mut entities, &mut seen, m.as_str(), EntityType::Concept);
    }
    for m in regex_all_caps().find_iter(cleaned) {
        let candidate = m.as_str();
        // v1.0.22: filtro consolidado (stopwords + HTTP methods); preserva identificadores com underscore.
        if !is_filtered_all_caps(candidate) {
            add(&mut entities, &mut seen, candidate, EntityType::Concept);
        }
    }
    // v1.0.25 P0-2: capture CamelCase brand names that NER model often misses.
    // Maps to "organization" (V008 schema) because brand names are typically organisations.
    for m in regex_brand_camel().find_iter(cleaned) {
        let name = m.as_str();
        // Skip if the uppercased form is a known stopword (e.g. "JsonSchema" → "JSONSCHEMA").
        if !ALL_CAPS_STOPWORDS.contains(&name.to_uppercase().as_str()) {
            add(&mut entities, &mut seen, name, EntityType::Organization);
        }
    }

    entities
}

/// Extracts URLs from a memory body, deduplicated by text.
/// URLs are stored in the `memory_urls` table separately from graph entities.
/// v1.0.24: split of the URL block that polluted apply_regex_prefilter with entity_type='concept'.
pub fn extract_urls(body: &str) -> Vec<ExtractedUrl> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(4);
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
    let mut seen: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();

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

            let key = (i.min(j), i.max(j));
            if !seen.insert(key) {
                continue;
            }

            rels.push(NewRelationship {
                // clone needed: NewRelationship requires owned String for source/target
                source: entities[i].name.clone(),
                target: entities[j].name.clone(),
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
    let mut seen: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
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
                let ei = present[i];
                let ej = present[j];
                let key = (ei.min(ej), ei.max(ej));
                if seen.insert(key) {
                    rels.push(NewRelationship {
                        source: entities[ei].name.clone(),
                        target: entities[ej].name.clone(),
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
                            let mut extended = String::with_capacity(ent.name.len() + suffix.len());
                            extended.push_str(&ent.name);
                            extended.push_str(suffix);
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

/// Captures versioned model names that NER model consistently misses.
///
/// NER model often classifies tokens like "Claude" or "Llama" as common nouns,
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
            entity_type: EntityType::Concept,
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
        let key = {
            let et = ent.entity_type.as_str();
            let mut k = String::with_capacity(et.len() + 1 + name_lc.len());
            k.push_str(et);
            k.push('\0');
            k.push_str(&name_lc);
            k
        };

        // Scan stored entries for substring containment within the same type.
        // Two names collide when one is a case-insensitive substring of the other:
        //   "sonne" ⊂ "sonnet"  → collision, keep "sonnet" (longest-wins)
        //   "open"  ⊂ "openai"  → collision, keep "openai" (longest-wins)
        let type_prefix = {
            let et = ent.entity_type.as_str();
            let mut p = String::with_capacity(et.len() + 1);
            p.push_str(et);
            p.push('\0');
            p
        };
        let mut collision_idx: Option<usize> = None;
        for (existing_key, idx) in &by_lc {
            // Fast-path: check type prefix matches before scanning the name.
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
                    let old_key = {
                        let et = result[idx].entity_type.as_str();
                        let mut k = String::with_capacity(et.len() + 1 + old_name_lc.len());
                        k.push_str(et);
                        k.push('\0');
                        k.push_str(&old_name_lc);
                        k
                    };
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

pub fn extract_graph_auto(
    body: &str,
    paths: &AppPaths,
    variant: GlinerVariant,
) -> Result<ExtractionResult> {
    let regex_entities = apply_regex_prefilter(body);
    let threshold = crate::constants::gliner_confidence_threshold();

    let mut gliner_used = false;
    let ner_entities = match get_or_init_gliner(paths, variant) {
        Some(model) => match model.predict(body, GLINER_ENTITY_LABELS, threshold) {
            Ok(ents) => {
                gliner_used = true;
                ents
            }
            Err(e) => {
                tracing::warn!("GLiNER NER failed, falling back to regex-only extraction: {e:#}");
                Vec::new()
            }
        },
        None => Vec::new(),
    };

    let merged = merge_and_deduplicate(regex_entities, ner_entities);
    let extended = extend_with_numeric_suffix(merged, body);
    let with_models = augment_versioned_model_names(extended, body);
    let with_models: Vec<ExtractedEntity> = with_models
        .into_iter()
        .filter(|e| !regex_section_marker().is_match(&e.name))
        .collect();
    let entities = to_new_entities(with_models);
    let (relationships, relationships_truncated) =
        build_relationships_by_sentence_cooccurrence(body, &entities);

    let extraction_method = if gliner_used {
        format!("gliner-{variant}+regex")
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
    use crate::entity_type::EntityType;

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
            .any(|e| e.name == "someone@company.com" && e.entity_type == EntityType::Concept));
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
        assert!(ents.iter().any(|e| e.entity_type == EntityType::Concept));
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
    fn build_relationships_respeitam_max_rels() {
        let entities: Vec<NewEntity> = (0..20)
            .map(|i| NewEntity {
                name: format!("entidade_{i}"),
                entity_type: EntityType::Concept,
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
                entity_type: EntityType::Concept,
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
            entity_type: EntityType::Concept,
        }];
        let b = vec![ExtractedEntity {
            name: "rust".to_string(),
            entity_type: EntityType::Concept,
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
        // Without a downloaded model, must return Ok with regex-only entities.
        let paths = make_paths();
        let body = "contato: teste@exemplo.com com MAX_RETRY=3";
        let result = extract_graph_auto(body, &paths, GlinerVariant::Int8).unwrap();
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
            entity_type: EntityType::Concept,
        }];
        // Build the NFD form: 'e' followed by combining acute accent U+0301
        let nfd_name = "Cafe\u{301}".to_string();
        let nfd = vec![ExtractedEntity {
            name: nfd_name,
            entity_type: EntityType::Concept,
        }];
        let merged = merge_and_deduplicate(nfc, nfd);
        assert_eq!(
            merged.len(),
            1,
            "NFC 'Caf\\u{{e9}}' and NFD 'Cafe\\u{{301}}' must deduplicate to 1 entity after NFKC normalization"
        );
    }

    #[test]
    fn extraction_method_regex_only_unchanged() {
        // RegexExtractor always returns "regex-only" regardless of GLINER_MODEL state.
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
            entity_type: EntityType::Concept,
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
            entity_type: EntityType::Concept,
        }];
        let result = extend_with_numeric_suffix(ents, "using GPT-4o for advanced tasks");
        assert_eq!(result[0].name, "GPT-4o", "suffix '4o' must be accepted");
    }

    #[test]
    fn extend_suffix_alphanumeric_b_suffix() {
        // P2-E: "5b" suffix (digit + 'b') must be captured.
        let ents = vec![ExtractedEntity {
            name: "Llama".to_string(),
            entity_type: EntityType::Concept,
        }];
        let result = extend_with_numeric_suffix(ents, "Llama-5b open-weight model");
        assert_eq!(result[0].name, "Llama-5b", "suffix '5b' must be accepted");
    }

    #[test]
    fn extend_suffix_alphanumeric_x_suffix() {
        // P2-E: "8x" suffix (digit + 'x') must be captured.
        let ents = vec![ExtractedEntity {
            name: "Mistral".to_string(),
            entity_type: EntityType::Concept,
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
            entity_type: EntityType::Concept,
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
        // "OpenAI" is a CamelCase brand that NER model often misses.
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
            EntityType::Organization,
            "brand CamelCase must map to organization (V008)"
        );
    }

    #[test]
    fn brand_postgresql_extracted_as_organization_v1025() {
        let body = "migrating from MySQL to PostgreSQL for better performance.";
        let ents = apply_regex_prefilter(body);
        assert!(
            ents.iter()
                .any(|e| e.name == "PostgreSQL" && e.entity_type == EntityType::Organization),
            "PostgreSQL must be extracted as organization; entities: {:?}",
            ents.iter()
                .map(|e| (&e.name, &e.entity_type))
                .collect::<Vec<_>>()
        );
    }

    // --- P0-3 longest-wins v1.0.25 ---

    fn entity(name: &str, entity_type: EntityType) -> ExtractedEntity {
        ExtractedEntity {
            name: name.to_string(),
            entity_type,
        }
    }

    #[test]
    fn merge_resolves_sonne_vs_sonnet_keeps_longest_v1025() {
        // "Sonne" is a substring of "Sonnet" — longest-wins must keep "Sonnet".
        let regex = vec![entity("Sonne", EntityType::Concept)];
        let ner = vec![entity("Sonnet", EntityType::Concept)];
        let result = merge_and_deduplicate(regex, ner);
        assert_eq!(result.len(), 1, "expected 1 entity, got: {result:?}");
        assert_eq!(result[0].name, "Sonnet");
    }

    #[test]
    fn merge_resolves_open_vs_openai_keeps_longest_v1025() {
        // "Open" is a substring of "OpenAI" — longest-wins must keep "OpenAI".
        let regex = vec![
            entity("Open", EntityType::Organization),
            entity("OpenAI", EntityType::Organization),
        ];
        let result = merge_and_deduplicate(regex, vec![]);
        assert_eq!(result.len(), 1, "expected 1 entity, got: {result:?}");
        assert_eq!(result[0].name, "OpenAI");
    }

    #[test]
    fn merge_keeps_both_when_no_containment_v1025() {
        // "Alice" and "Bob" share no containment — both must be preserved.
        let regex = vec![
            entity("Alice", EntityType::Person),
            entity("Bob", EntityType::Person),
        ];
        let result = merge_and_deduplicate(regex, vec![]);
        assert_eq!(result.len(), 2, "expected 2 entities, got: {result:?}");
    }

    #[test]
    fn merge_respects_entity_type_boundary_v1025() {
        // Same name "Apple" but different types: both must survive independently.
        let regex = vec![
            entity("Apple", EntityType::Organization),
            entity("Apple", EntityType::Concept),
        ];
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
            entity("OpenAI", EntityType::Organization),
            entity("openai", EntityType::Organization),
        ];
        let result = merge_and_deduplicate(regex, vec![]);
        assert_eq!(
            result.len(),
            1,
            "expected 1 entity after case-insensitive dedup, got: {result:?}"
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
        let result = extract_graph_auto(&body, &paths, GlinerVariant::Int8)
            .expect("extraction must not error");
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
                entity_type: EntityType::Person,
                description: None,
            },
            NewEntity {
                name: "Bob".to_string(),
                entity_type: EntityType::Person,
                description: None,
            },
            NewEntity {
                name: "Carol".to_string(),
                entity_type: EntityType::Person,
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
            entity_type: EntityType::Person,
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
                entity_type: EntityType::Person,
                description: None,
            },
            NewEntity {
                name: "Bob".to_string(),
                entity_type: EntityType::Person,
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

    #[test]
    fn gliner_variant_from_str_valid() {
        assert_eq!(
            "fp32".parse::<GlinerVariant>().unwrap(),
            GlinerVariant::Fp32
        );
        assert_eq!(
            "fp16".parse::<GlinerVariant>().unwrap(),
            GlinerVariant::Fp16
        );
        assert_eq!(
            "int8".parse::<GlinerVariant>().unwrap(),
            GlinerVariant::Int8
        );
        assert_eq!("q4".parse::<GlinerVariant>().unwrap(), GlinerVariant::Q4);
        assert_eq!(
            "q4f16".parse::<GlinerVariant>().unwrap(),
            GlinerVariant::Q4f16
        );
        // Case-insensitive
        assert_eq!(
            "FP32".parse::<GlinerVariant>().unwrap(),
            GlinerVariant::Fp32
        );
        assert_eq!(
            "INT8".parse::<GlinerVariant>().unwrap(),
            GlinerVariant::Int8
        );
    }

    #[test]
    fn gliner_variant_from_str_invalid() {
        assert!("invalid".parse::<GlinerVariant>().is_err());
        assert!("fp64".parse::<GlinerVariant>().is_err());
        assert!("".parse::<GlinerVariant>().is_err());
    }

    #[test]
    fn gliner_variant_filename_mapping() {
        assert_eq!(GlinerVariant::Fp32.as_filename(), "model.onnx");
        assert_eq!(GlinerVariant::Fp16.as_filename(), "model_fp16.onnx");
        assert_eq!(GlinerVariant::Int8.as_filename(), "model_quantized.onnx");
        assert_eq!(GlinerVariant::Q4.as_filename(), "model_q4.onnx");
        assert_eq!(GlinerVariant::Q4f16.as_filename(), "model_q4f16.onnx");
    }

    #[test]
    fn gliner_variant_display() {
        assert_eq!(format!("{}", GlinerVariant::Fp32), "fp32");
        assert_eq!(format!("{}", GlinerVariant::Fp16), "fp16");
        assert_eq!(format!("{}", GlinerVariant::Int8), "int8");
        assert_eq!(format!("{}", GlinerVariant::Q4), "q4");
        assert_eq!(format!("{}", GlinerVariant::Q4f16), "q4f16");
    }

    #[test]
    fn gliner_variant_display_size() {
        assert_eq!(GlinerVariant::Fp32.display_size(), "1.1 GB");
        assert_eq!(GlinerVariant::Int8.display_size(), "349 MB");
    }

    #[test]
    fn gliner_entity_labels_covers_all_types() {
        let label_types: Vec<EntityType> = GLINER_ENTITY_LABELS.iter().map(|(_, t)| *t).collect();
        assert!(label_types.contains(&EntityType::Person));
        assert!(label_types.contains(&EntityType::Organization));
        assert!(label_types.contains(&EntityType::Location));
        assert!(label_types.contains(&EntityType::Date));
        assert!(label_types.contains(&EntityType::Project));
        assert!(label_types.contains(&EntityType::Tool));
        assert!(label_types.contains(&EntityType::File));
        assert!(label_types.contains(&EntityType::Concept));
        assert!(label_types.contains(&EntityType::Decision));
        assert!(label_types.contains(&EntityType::Incident));
        assert!(label_types.contains(&EntityType::Dashboard));
        assert!(label_types.contains(&EntityType::IssueTracker));
        assert!(label_types.contains(&EntityType::Memory));
        assert_eq!(GLINER_ENTITY_LABELS.len(), 13);
    }

    #[test]
    fn gliner_entity_labels_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for (label, _) in GLINER_ENTITY_LABELS {
            assert!(seen.insert(*label), "duplicate label: {label}");
        }
    }

    #[test]
    fn extract_graph_auto_regex_only_fallback() {
        // extract_graph_auto must succeed and capture regex entities regardless of whether
        // GLiNER model files exist in the test environment (GLINER_MODEL is a global OnceLock
        // that may already be initialised by a sibling test, so we cannot assert on
        // extraction_method; use RegexExtractor for that invariant).
        let result = extract_graph_auto(
            "Contact someone@test.com about OPENAI project",
            &make_paths(),
            GlinerVariant::Fp32,
        );
        assert!(result.is_ok());
        let res = result.unwrap();
        // Regex prefilter must always capture the email entity
        assert!(res.entities.iter().any(|e| e.name == "someone@test.com"));
        // extraction_method must be one of the two valid values
        assert!(
            res.extraction_method == "regex-only" || res.extraction_method.starts_with("gliner-"),
            "unexpected extraction_method: {}",
            res.extraction_method
        );
    }

    #[test]
    fn gliner_variant_roundtrip() {
        for variant in &[
            GlinerVariant::Fp32,
            GlinerVariant::Fp16,
            GlinerVariant::Int8,
            GlinerVariant::Q4,
            GlinerVariant::Q4f16,
        ] {
            let s = format!("{variant}");
            let parsed: GlinerVariant = s.parse().unwrap();
            assert_eq!(*variant, parsed);
        }
    }
}
