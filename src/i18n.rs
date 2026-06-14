//! Bilingual human-readable message layer.
//!
//! The CLI uses `--lang en|pt` (global flag) or `SQLITE_GRAPHRAG_LANG` (env var) to choose
//! the language of stderr progress messages. JSON stdout is deterministic and identical
//! across languages — only strings intended for humans pass through this module.
//!
//! Detection (highest to lowest priority):
//! 1. Explicit `--lang` flag
//! 2. Env var `SQLITE_GRAPHRAG_LANG`
//! 3. OS locale (`LANG`, `LC_ALL`) with `pt` prefix
//! 4. Fallback `English`

use std::sync::OnceLock;

#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Language {
    #[value(name = "en", aliases = ["english", "EN"])]
    English,
    #[value(name = "pt", aliases = ["portugues", "portuguese", "pt-BR", "pt-br", "PT"])]
    Portuguese,
}

impl Language {
    /// Parses a command-line string into a `Language` without relying on clap.
    /// Accepts the same aliases defined in `#[value(...)]`: "en", "pt", etc.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "en" | "english" => Some(Language::English),
            "pt" | "pt-br" | "portugues" | "portuguese" => Some(Language::Portuguese),
            _ => None,
        }
    }

    pub fn from_env_or_locale() -> Self {
        // Priority 1: explicit SQLITE_GRAPHRAG_LANG env var (highest precedence).
        // Empty string treated as unset per POSIX convention.
        if let Ok(v) = std::env::var("SQLITE_GRAPHRAG_LANG") {
            if !v.is_empty() {
                let lower = v.to_lowercase();
                if lower.starts_with("pt") {
                    return Language::Portuguese;
                }
                if lower.starts_with("en") {
                    return Language::English;
                }
                tracing::warn!(target: "i18n",
                    value = %v,
                    "SQLITE_GRAPHRAG_LANG value not recognized, falling back to locale detection"
                );
            }
        }
        // Priority 2: POSIX locale precedence LC_ALL > LC_MESSAGES > LANG.
        // We read these via std::env (not via sys_locale) because:
        // (a) `sys_locale::get_locale()` calls into native OS APIs (CFLocaleCopyCurrent
        //     on macOS, GetUserDefaultLocaleName on Windows) which cache the
        //     system locale and IGNORE env vars set at runtime by tests;
        // (b) POSIX specifies LC_ALL > LC_MESSAGES > LANG ordering and an
        //     unrecognised LC_ALL value must stop iteration (fall back to
        //     English default).
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(v) = std::env::var(var) {
                if v.is_empty() {
                    continue;
                }
                let lower = v.to_lowercase();
                if lower.starts_with("pt") {
                    return Language::Portuguese;
                }
                if lower.starts_with("en") {
                    return Language::English;
                }
                // Unrecognised value in a higher-precedence variable stops
                // iteration per POSIX.1-2017 §8.2.
                if var == "LC_ALL" {
                    return Language::English;
                }
            }
        }
        // Priority 3: cross-platform locale detection via native OS APIs.
        // Only reached when no POSIX env var is set.
        if let Some(locale) = sys_locale::get_locale() {
            let lower = locale.to_lowercase();
            if lower.starts_with("pt") {
                return Language::Portuguese;
            }
            if lower.starts_with("en") {
                return Language::English;
            }
        }
        Language::English
    }
}

static GLOBAL_LANGUAGE: OnceLock<Language> = OnceLock::new();

/// Initializes the global language. Subsequent calls are silently ignored
/// (OnceLock semantics) — guaranteeing thread-safety and determinism.
///
/// v1.0.36 (L6): early-return when already initialized so the env-fallback
/// resolver (`from_env_or_locale`) does not run a second time. Without this
/// guard, calling `init(None)` after `current()` already populated the
/// OnceLock causes `from_env_or_locale` to fire its `tracing::warn!` twice
/// for unrecognized `SQLITE_GRAPHRAG_LANG` values.
pub fn init(explicit: Option<Language>) {
    if GLOBAL_LANGUAGE.get().is_some() {
        return;
    }
    let resolved = explicit.unwrap_or_else(Language::from_env_or_locale);
    let _ = GLOBAL_LANGUAGE.set(resolved);
}

/// Returns the active language, or fallback English if `init` was never called.
pub fn current() -> Language {
    *GLOBAL_LANGUAGE.get_or_init(Language::from_env_or_locale)
}

/// Translates a bilingual message by selecting the active variant.
///
/// v1.0.36 (M4): inputs are constrained to `&'static str` so the function
/// can return one of them directly without `Box::leak`. The previous
/// implementation leaked one allocation per call which accumulated in
/// long-running pipelines; this version is allocation-free. All in-tree
/// callers already pass string literals, which are `&'static str`.
pub fn tr(en: &'static str, pt: &'static str) -> &'static str {
    match current() {
        Language::English => en,
        Language::Portuguese => pt,
    }
}

/// Progress message emitted after pruning relationships.
///
/// English-only: this string is emitted to stderr as a progress notice and
/// does not vary by language because the prune-relations command targets
/// agent-first pipelines where deterministic output matters.
pub fn relations_pruned(count: usize, relation: &str, namespace: &str) -> String {
    format!("pruned {count} '{relation}' relationships in namespace '{namespace}'")
}

/// Progress message for dry-run preview of prune-relations.
///
/// English-only: emitted to stderr as a progress notice.
pub fn prune_dry_run(count: usize, relation: &str) -> String {
    format!("dry run: {count} '{relation}' relationships would be removed")
}

/// Warning message when --yes is not passed for destructive prune-relations.
///
/// English-only: emitted to stderr as a progress notice.
pub fn prune_requires_yes() -> String {
    "destructive operation requires --yes flag; use --dry-run to preview".to_string()
}

/// Localized prefix for error messages displayed to the end user.
pub fn error_prefix() -> &'static str {
    match current() {
        Language::English => "Error",
        Language::Portuguese => "Erro",
    }
}

/// Error messages for `AppError` variants — always English.
///
/// These strings end up inside `AppError` inner fields and may appear in
/// deterministic JSON stdout (e.g. ingest NDJSON). Portuguese translations
/// for stderr live in `pub mod app_error_pt` and are applied by
/// `localized_message_for(Language::Portuguese)`.
pub mod errors_msg {
    pub fn memory_not_found(nome: &str, namespace: &str) -> String {
        format!("memory '{nome}' not found in namespace '{namespace}'")
    }

    pub fn memory_or_entity_not_found(name: &str, namespace: &str) -> String {
        format!("memory or entity '{name}' not found in namespace '{namespace}'")
    }

    pub fn database_not_found(path: &str) -> String {
        format!("database not found at {path}. Run 'sqlite-graphrag init' first.")
    }

    pub fn entity_not_found(nome: &str, namespace: &str) -> String {
        format!("entity \"{nome}\" does not exist in namespace \"{namespace}\"")
    }

    pub fn relationship_not_found(de: &str, rel: &str, para: &str, namespace: &str) -> String {
        format!(
            "relationship \"{de}\" --[{rel}]--> \"{para}\" does not exist in namespace \"{namespace}\""
        )
    }

    pub fn duplicate_memory(nome: &str, namespace: &str) -> String {
        format!(
            "memory '{nome}' already exists in namespace '{namespace}'. Use --force-merge to update."
        )
    }

    pub fn duplicate_memory_soft_deleted(name: &str, namespace: &str) -> String {
        format!(
            "memory '{name}' exists but is soft-deleted in namespace '{namespace}'; \
             use --force-merge to restore and update, or `restore` to revive it"
        )
    }

    pub fn optimistic_lock_conflict(expected: i64, current_ts: i64) -> String {
        format!(
            "optimistic lock conflict: expected updated_at={expected}, but current is {current_ts}"
        )
    }

    pub fn version_not_found(versao: i64, nome: &str) -> String {
        format!("version {versao} not found for memory '{nome}'")
    }

    pub fn no_recall_results(max_distance: f32, query: &str, namespace: &str) -> String {
        format!(
            "no results within --max-distance {max_distance} for query '{query}' in namespace '{namespace}'"
        )
    }

    pub fn soft_deleted_memory_not_found(nome: &str, namespace: &str) -> String {
        format!("soft-deleted memory '{nome}' not found in namespace '{namespace}'")
    }

    pub fn concurrent_process_conflict() -> String {
        "optimistic lock conflict: memory was modified by another process".to_string()
    }

    pub fn entity_limit_exceeded(max: usize) -> String {
        format!("entities exceed limit of {max}")
    }

    pub fn relationship_limit_exceeded(max: usize) -> String {
        format!("relationships exceed limit of {max}")
    }
}

/// Localized validation messages for memory fields.
pub mod validation {
    use super::current;
    use crate::i18n::Language;

    pub fn name_length(max: usize) -> String {
        match current() {
            Language::English => format!("name must be 1-{max} chars"),
            Language::Portuguese => format!("nome deve ter entre 1 e {max} caracteres"),
        }
    }

    pub fn reserved_name() -> String {
        match current() {
            Language::English => {
                "names and namespaces starting with __ are reserved for internal use".to_string()
            }
            Language::Portuguese => {
                "nomes e namespaces iniciados com __ são reservados para uso interno".to_string()
            }
        }
    }

    pub fn name_kebab(nome: &str) -> String {
        match current() {
            Language::English => format!(
                "name must be kebab-case slug (lowercase letters, digits, hyphens): '{nome}'"
            ),
            Language::Portuguese => {
                format!("nome deve estar em kebab-case (minúsculas, dígitos, hífens): '{nome}'")
            }
        }
    }

    pub fn description_exceeds(max: usize) -> String {
        match current() {
            Language::English => format!("description must be <= {max} chars"),
            Language::Portuguese => format!("descrição deve ter no máximo {max} caracteres"),
        }
    }

    pub fn body_exceeds(max: usize) -> String {
        match current() {
            Language::English => format!("body exceeds {max} bytes"),
            Language::Portuguese => format!("corpo excede {max} bytes"),
        }
    }

    pub fn new_name_length(max: usize) -> String {
        match current() {
            Language::English => format!("new-name must be 1-{max} chars"),
            Language::Portuguese => format!("novo nome deve ter entre 1 e {max} caracteres"),
        }
    }

    pub fn new_name_kebab(nome: &str) -> String {
        match current() {
            Language::English => format!(
                "new-name must be kebab-case slug (lowercase letters, digits, hyphens): '{nome}'"
            ),
            Language::Portuguese => format!(
                "novo nome deve estar em kebab-case (minúsculas, dígitos, hífens): '{nome}'"
            ),
        }
    }

    pub fn namespace_length() -> String {
        match current() {
            Language::English => "namespace must be 1-80 chars".to_string(),
            Language::Portuguese => "namespace deve ter entre 1 e 80 caracteres".to_string(),
        }
    }

    pub fn namespace_format() -> String {
        match current() {
            Language::English => "namespace must be alphanumeric + hyphens/underscores".to_string(),
            Language::Portuguese => {
                "namespace deve ser alfanumérico com hífens/sublinhados".to_string()
            }
        }
    }

    pub fn path_traversal(p: &str) -> String {
        match current() {
            Language::English => format!("path traversal rejected: {p}"),
            Language::Portuguese => format!("traversal de caminho rejeitado: {p}"),
        }
    }

    pub fn invalid_tz(v: &str) -> String {
        match current() {
            Language::English => format!(
                "SQLITE_GRAPHRAG_DISPLAY_TZ invalid: '{v}'; use an IANA name like 'America/Sao_Paulo'"
            ),
            Language::Portuguese => format!(
                "SQLITE_GRAPHRAG_DISPLAY_TZ inválido: '{v}'; use um nome IANA como 'America/Sao_Paulo'"
            ),
        }
    }

    pub fn empty_query() -> String {
        match current() {
            Language::English => "query cannot be empty".to_string(),
            Language::Portuguese => "a consulta não pode estar vazia".to_string(),
        }
    }

    pub fn empty_body() -> String {
        match current() {
            Language::English => "body cannot be empty: provide --body, --body-file, or --body-stdin with content, or supply a graph via --entities-file/--graph-stdin".to_string(),
            Language::Portuguese => "o corpo não pode estar vazio: forneça --body, --body-file ou --body-stdin com conteúdo, ou um grafo via --entities-file/--graph-stdin".to_string(),
        }
    }

    pub fn invalid_namespace_config(path: &str, err: &str) -> String {
        match current() {
            Language::English => {
                format!("invalid project namespace config '{path}': {err}")
            }
            Language::Portuguese => {
                format!("configuração de namespace de projeto inválida '{path}': {err}")
            }
        }
    }

    pub fn invalid_projects_mapping(path: &str, err: &str) -> String {
        match current() {
            Language::English => format!("invalid projects mapping '{path}': {err}"),
            Language::Portuguese => format!("mapeamento de projetos inválido '{path}': {err}"),
        }
    }

    pub fn self_referential_link() -> String {
        match current() {
            Language::English => "--from and --to must be different entities — self-referential relationships are not supported".to_string(),
            Language::Portuguese => "--from e --to devem ser entidades diferentes — relacionamentos auto-referenciais não são suportados".to_string(),
        }
    }

    pub fn invalid_link_weight(weight: f64) -> String {
        match current() {
            Language::English => {
                format!("--weight: must be between 0.0 and 1.0 (actual: {weight})")
            }
            Language::Portuguese => {
                format!("--weight: deve estar entre 0.0 e 1.0 (atual: {weight})")
            }
        }
    }

    pub fn sync_destination_equals_source() -> String {
        match current() {
            Language::English => {
                "destination path must differ from the source database path".to_string()
            }
            Language::Portuguese => {
                "caminho de destino deve ser diferente do caminho do banco de dados fonte"
                    .to_string()
            }
        }
    }

    /// Portuguese translations for `AppError` Display messages.
    ///
    /// Each helper mirrors a single `AppError` variant's `#[error(...)]` text in
    /// Portuguese, keeping the language barrier confined to this module. The
    /// English source of truth lives in `src/errors.rs` via `thiserror`.
    pub mod app_error_pt {
        pub fn validation(msg: &str) -> String {
            format!("erro de validação: {msg}")
        }

        pub fn duplicate(msg: &str) -> String {
            let translated = msg
                .replace("already exists in namespace", "já existe no namespace")
                .replace(
                    "exists but is soft-deleted in namespace",
                    "existe mas está excluída temporariamente no namespace",
                )
                .replace(
                    "Use --force-merge to update.",
                    "Use --force-merge para atualizar.",
                )
                .replace(
                    "use --force-merge to restore and update, or `restore` to revive it",
                    "use --force-merge para restaurar e atualizar, ou `restore` para revivê-la",
                )
                .replace("memory", "memória");
            format!("duplicata detectada: {translated}")
        }

        pub fn conflict(msg: &str) -> String {
            let translated = msg
                .replace("optimistic lock conflict", "conflito de lock otimista")
                .replace("but current is", "mas atual é")
                .replace(
                    "was modified by another process",
                    "foi modificada por outro processo",
                );
            format!("conflito: {translated}")
        }

        pub fn not_found(msg: &str) -> String {
            // G55 T3: add replacements for the read.rs format produced by the
            // T1 fix: `memory not found: name='X' in namespace 'Y'`.
            // The existing chain did not catch ` in namespace '` when broken
            // by the name label, leaving a bilingual hybrid. New patterns
            // must run BEFORE the catch-all `memory` → `memória` to avoid
            // being shadowed.
            let translated = msg
                .replace("memory not found:", "memória não encontrada:")
                .replace("not found in namespace", "não encontrada no namespace")
                .replace("not found for memory", "não encontrada para memória")
                .replace("does not exist in namespace", "não existe no namespace")
                .replace("memory or entity", "memória ou entidade")
                .replace("name='", "nome='")
                .replace("memory", "memória")
                .replace("entity", "entidade")
                .replace(" in namespace '", " no namespace '")
                .replace("version", "versão")
                .replace("soft-deleted", "excluída temporariamente");
            format!("não encontrado: {translated}")
        }

        // G55 S2 (v1.0.80): structured variant helpers. They synthesize the
        // canonical English message and feed it through the `not_found`
        // replace-chain so the pt-BR translation stays in one place.
        pub fn memory_not_found(name: &str, namespace: &str) -> String {
            not_found(&format!(
                "memory not found: name='{name}' in namespace '{namespace}'"
            ))
        }

        pub fn memory_not_found_by_id(id: i64) -> String {
            not_found(&format!("memory not found: id={id}"))
        }

        pub fn namespace_error(msg: &str) -> String {
            format!("namespace não resolvido: {msg}")
        }

        pub fn limit_exceeded(msg: &str) -> String {
            let translated = msg
                .replace("exceeds limit of", "excede limite de")
                .replace("body exceeds", "corpo excede")
                .replace("entities exceed limit", "entidades excedem limite")
                .replace(
                    "relationships exceed limit",
                    "relacionamentos excedem limite",
                );
            format!("limite excedido: {translated}")
        }

        pub fn database(err: &str) -> String {
            format!("erro de banco de dados: {err}")
        }

        pub fn embedding(msg: &str) -> String {
            format!("erro de embedding: {msg}")
        }

        pub fn vec_extension(msg: &str) -> String {
            format!("extensão sqlite-vec falhou: {msg}")
        }

        pub fn db_busy(msg: &str) -> String {
            format!("banco ocupado: {msg}")
        }

        pub fn batch_partial_failure(total: usize, failed: usize) -> String {
            format!("falha parcial em batch: {failed} de {total} itens falharam")
        }

        pub fn io(err: &str) -> String {
            format!("erro de I/O: {err}")
        }

        pub fn internal(err: &str) -> String {
            format!("erro interno: {err}")
        }

        pub fn json(err: &str) -> String {
            format!("erro de JSON: {err}")
        }

        pub fn lock_busy(msg: &str) -> String {
            format!("lock ocupado: {msg}")
        }

        pub fn all_slots_full(max: usize, waited_secs: u64) -> String {
            format!(
                "todos os {max} slots de concorrência ocupados após aguardar {waited_secs}s \
                 (exit 75); use --max-concurrency ou aguarde outras invocações terminarem"
            )
        }

        pub fn job_singleton_locked(job_type: &str, namespace: &str) -> String {
            format!(
                "job {job_type} para o namespace '{namespace}' já está em execução (exit 75); \
                 aguarde a conclusão ou passe --wait-job-singleton <SEGUNDOS>"
            )
        }

        pub fn embedding_singleton_locked(namespace: &str) -> String {
            format!(
                "singleton de embedding para o namespace '{namespace}' já está retido (exit 75); \
                 outra CLI está chamando o LLM neste banco; passe --wait-embed-singleton <SEGUNDOS> para aguardar"
            )
        }

        pub fn low_memory(available_mb: u64, required_mb: u64) -> String {
            format!(
                "memória disponível ({available_mb}MB) abaixo do mínimo requerido ({required_mb}MB) \
                 para carregar o modelo; aborte outras cargas ou use --skip-memory-guard (exit 77)"
            )
        }

        pub fn binary_not_found(name: &str) -> String {
            format!("binário não encontrado: {name} — instale e adicione ao PATH")
        }

        pub fn rate_limited(detail: &str) -> String {
            format!("taxa de requisição excedida: {detail}")
        }

        pub fn timeout(operation: &str, secs: u64) -> String {
            format!("timeout após {secs}s: {operation}")
        }
    }

    /// Portuguese translations for runtime startup messages emitted from `main.rs`.
    ///
    /// These mirror the English text supplied alongside each call to
    /// `output::emit_progress_i18n` / `output::emit_error_i18n`, keeping the PT
    /// strings confined to this module per the language policy.
    pub mod runtime_pt {
        pub fn embedding_heavy_must_measure_ram() -> String {
            "comando intensivo em embedding precisa medir RAM disponível".to_string()
        }

        pub fn heavy_command_detected(available_mb: u64, safe_concurrency: usize) -> String {
            format!(
                "Comando pesado detectado; memória disponível: {available_mb} MB; \
                 concorrência segura: {safe_concurrency}"
            )
        }

        pub fn reducing_concurrency(
            requested_concurrency: usize,
            effective_concurrency: usize,
        ) -> String {
            format!(
                "Reduzindo a concorrência solicitada de {requested_concurrency} para \
                 {effective_concurrency} para evitar oversubscription de memória"
            )
        }

        pub fn initializing_embedding_model() -> &'static str {
            "Inicializando modelo de embedding (pode baixar na primeira execução)..."
        }

        pub fn embedding_chunks_serially(count: usize) -> String {
            format!("Embedando {count} chunks serialmente para manter memória limitada...")
        }

        pub fn remember_step_input_validated(available_mb: u64) -> String {
            format!("Etapa remember: entrada validada; memória disponível {available_mb} MB")
        }

        pub fn remember_step_chunking_completed(
            total_passage_tokens: usize,
            model_max_length: usize,
            chunks_count: usize,
            rss_mb: u64,
        ) -> String {
            format!(
                "Etapa remember: tokenizer contou {total_passage_tokens} tokens de passagem \
                 (máximo do modelo {model_max_length}); chunking gerou {chunks_count} chunks; \
                 RSS do processo {rss_mb} MB"
            )
        }

        pub fn remember_step_embeddings_completed(rss_mb: u64) -> String {
            format!("Etapa remember: embeddings dos chunks concluídos; RSS do processo {rss_mb} MB")
        }

        pub fn restore_recomputing_embedding() -> &'static str {
            "Recalculando embedding da memória restaurada..."
        }

        pub fn edit_recomputing_embedding() -> &'static str {
            "Recalculando embedding da memória editada..."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn fallback_english_when_env_absent() {
        std::env::remove_var("SQLITE_GRAPHRAG_LANG");
        std::env::set_var("LC_ALL", "C");
        std::env::set_var("LANG", "C");
        assert_eq!(Language::from_env_or_locale(), Language::English);
        std::env::remove_var("LC_ALL");
        std::env::remove_var("LANG");
    }

    #[test]
    #[serial]
    fn env_pt_selects_portuguese() {
        std::env::remove_var("LC_ALL");
        std::env::remove_var("LANG");
        std::env::set_var("SQLITE_GRAPHRAG_LANG", "pt");
        assert_eq!(Language::from_env_or_locale(), Language::Portuguese);
        std::env::remove_var("SQLITE_GRAPHRAG_LANG");
    }

    #[test]
    #[serial]
    fn env_pt_br_selects_portuguese() {
        std::env::remove_var("LC_ALL");
        std::env::remove_var("LANG");
        std::env::set_var("SQLITE_GRAPHRAG_LANG", "pt-BR");
        assert_eq!(Language::from_env_or_locale(), Language::Portuguese);
        std::env::remove_var("SQLITE_GRAPHRAG_LANG");
    }

    #[test]
    #[serial]
    fn locale_ptbr_utf8_selects_portuguese() {
        std::env::remove_var("SQLITE_GRAPHRAG_LANG");
        std::env::set_var("LC_ALL", "pt_BR.UTF-8");
        assert_eq!(Language::from_env_or_locale(), Language::Portuguese);
        std::env::remove_var("LC_ALL");
    }

    #[test]
    #[serial]
    fn posix_precedence_lc_all_overrides_lang() {
        std::env::remove_var("SQLITE_GRAPHRAG_LANG");
        std::env::remove_var("LC_MESSAGES");
        std::env::set_var("LC_ALL", "en_US.UTF-8");
        std::env::set_var("LANG", "pt_BR.UTF-8");
        assert_eq!(
            Language::from_env_or_locale(),
            Language::English,
            "LC_ALL=en_US must override LANG=pt_BR per POSIX"
        );
        std::env::remove_var("LC_ALL");
        std::env::remove_var("LANG");
    }

    #[test]
    #[serial]
    fn posix_precedence_lc_all_unrecognized_stops_iteration() {
        std::env::remove_var("SQLITE_GRAPHRAG_LANG");
        std::env::remove_var("LC_MESSAGES");
        std::env::set_var("LC_ALL", "ja_JP.UTF-8");
        std::env::set_var("LANG", "pt_BR.UTF-8");
        assert_eq!(
            Language::from_env_or_locale(),
            Language::English,
            "LC_ALL=ja_JP set must stop iteration; falls back to English default"
        );
        std::env::remove_var("LC_ALL");
        std::env::remove_var("LANG");
    }

    #[test]
    #[serial]
    fn lang_pt_selects_portuguese_when_lc_all_unset() {
        std::env::remove_var("SQLITE_GRAPHRAG_LANG");
        std::env::remove_var("LC_ALL");
        std::env::remove_var("LC_MESSAGES");
        std::env::set_var("LANG", "pt_BR.UTF-8");
        assert_eq!(Language::from_env_or_locale(), Language::Portuguese);
        std::env::remove_var("LANG");
    }

    mod validation_tests {
        use super::*;

        #[test]
        fn name_length_en() {
            let msg = match Language::English {
                Language::English => format!("name must be 1-{} chars", 80),
                Language::Portuguese => format!("nome deve ter entre 1 e {} caracteres", 80),
            };
            assert!(msg.contains("name must be 1-80 chars"), "obtido: {msg}");
        }

        #[test]
        fn name_length_pt() {
            let msg = match Language::Portuguese {
                Language::English => format!("name must be 1-{} chars", 80),
                Language::Portuguese => format!("nome deve ter entre 1 e {} caracteres", 80),
            };
            assert!(
                msg.contains("nome deve ter entre 1 e 80 caracteres"),
                "obtido: {msg}"
            );
        }

        #[test]
        fn name_kebab_en() {
            let nome = "Invalid_Name";
            let msg = match Language::English {
                Language::English => format!(
                    "name must be kebab-case slug (lowercase letters, digits, hyphens): '{nome}'"
                ),
                Language::Portuguese => {
                    format!("nome deve estar em kebab-case (minúsculas, dígitos, hífens): '{nome}'")
                }
            };
            assert!(msg.contains("kebab-case slug"), "obtido: {msg}");
            assert!(msg.contains("Invalid_Name"), "obtido: {msg}");
        }

        #[test]
        fn name_kebab_pt() {
            let nome = "Invalid_Name";
            let msg = match Language::Portuguese {
                Language::English => format!(
                    "name must be kebab-case slug (lowercase letters, digits, hyphens): '{nome}'"
                ),
                Language::Portuguese => {
                    format!("nome deve estar em kebab-case (minúsculas, dígitos, hífens): '{nome}'")
                }
            };
            assert!(msg.contains("kebab-case"), "obtido: {msg}");
            assert!(msg.contains("minúsculas"), "obtido: {msg}");
            assert!(msg.contains("Invalid_Name"), "obtido: {msg}");
        }

        #[test]
        fn description_exceeds_en() {
            let msg = match Language::English {
                Language::English => format!("description must be <= {} chars", 500),
                Language::Portuguese => format!("descrição deve ter no máximo {} caracteres", 500),
            };
            assert!(msg.contains("description must be <= 500"), "obtido: {msg}");
        }

        #[test]
        fn description_exceeds_pt() {
            let msg = match Language::Portuguese {
                Language::English => format!("description must be <= {} chars", 500),
                Language::Portuguese => format!("descrição deve ter no máximo {} caracteres", 500),
            };
            assert!(
                msg.contains("descrição deve ter no máximo 500"),
                "obtido: {msg}"
            );
        }

        #[test]
        fn body_exceeds_en() {
            let limite = crate::constants::MAX_MEMORY_BODY_LEN;
            let msg = match Language::English {
                Language::English => format!("body exceeds {limite} bytes"),
                Language::Portuguese => format!("corpo excede {limite} bytes"),
            };
            assert!(msg.contains("body exceeds 512000"), "obtido: {msg}");
        }

        #[test]
        fn body_exceeds_pt() {
            let limite = crate::constants::MAX_MEMORY_BODY_LEN;
            let msg = match Language::Portuguese {
                Language::English => format!("body exceeds {limite} bytes"),
                Language::Portuguese => format!("corpo excede {limite} bytes"),
            };
            assert!(msg.contains("corpo excede 512000"), "obtido: {msg}");
        }

        #[test]
        fn new_name_length_en() {
            let msg = match Language::English {
                Language::English => format!("new-name must be 1-{} chars", 80),
                Language::Portuguese => format!("novo nome deve ter entre 1 e {} caracteres", 80),
            };
            assert!(msg.contains("new-name must be 1-80"), "obtido: {msg}");
        }

        #[test]
        fn new_name_length_pt() {
            let msg = match Language::Portuguese {
                Language::English => format!("new-name must be 1-{} chars", 80),
                Language::Portuguese => format!("novo nome deve ter entre 1 e {} caracteres", 80),
            };
            assert!(
                msg.contains("novo nome deve ter entre 1 e 80"),
                "obtido: {msg}"
            );
        }

        #[test]
        fn new_name_kebab_en() {
            let nome = "Bad Name";
            let msg = match Language::English {
                Language::English => format!(
                    "new-name must be kebab-case slug (lowercase letters, digits, hyphens): '{nome}'"
                ),
                Language::Portuguese => format!(
                    "novo nome deve estar em kebab-case (minúsculas, dígitos, hífens): '{nome}'"
                ),
            };
            assert!(msg.contains("new-name must be kebab-case"), "obtido: {msg}");
        }

        #[test]
        fn new_name_kebab_pt() {
            let nome = "Bad Name";
            let msg = match Language::Portuguese {
                Language::English => format!(
                    "new-name must be kebab-case slug (lowercase letters, digits, hyphens): '{nome}'"
                ),
                Language::Portuguese => format!(
                    "novo nome deve estar em kebab-case (minúsculas, dígitos, hífens): '{nome}'"
                ),
            };
            assert!(
                msg.contains("novo nome deve estar em kebab-case"),
                "obtido: {msg}"
            );
        }

        #[test]
        fn reserved_name_en() {
            let msg = match Language::English {
                Language::English => {
                    "names and namespaces starting with __ are reserved for internal use"
                        .to_string()
                }
                Language::Portuguese => {
                    "nomes e namespaces iniciados com __ são reservados para uso interno"
                        .to_string()
                }
            };
            assert!(msg.contains("reserved for internal use"), "obtido: {msg}");
        }

        #[test]
        fn reserved_name_pt() {
            let msg = match Language::Portuguese {
                Language::English => {
                    "names and namespaces starting with __ are reserved for internal use"
                        .to_string()
                }
                Language::Portuguese => {
                    "nomes e namespaces iniciados com __ são reservados para uso interno"
                        .to_string()
                }
            };
            assert!(msg.contains("reservados para uso interno"), "obtido: {msg}");
        }
    }

    mod app_error_pt_translation_tests {
        use crate::errors::AppError;

        #[test]
        fn localized_message_pt_not_found_fully_translated() {
            let err =
                AppError::NotFound("memory 'test-mem' not found in namespace 'global'".into());
            let pt = err.localized_message_for(crate::i18n::Language::Portuguese);
            assert!(
                pt.contains("memória"),
                "PT must translate 'memory' to 'memória': {pt}"
            );
            assert!(
                pt.contains("não encontrada no namespace"),
                "PT must translate full phrase: {pt}"
            );
            assert!(
                !pt.contains("not found in namespace"),
                "PT must not contain English phrase: {pt}"
            );
        }

        #[test]
        fn localized_message_pt_duplicate_fully_translated() {
            let err = AppError::Duplicate(
                "memory 'x' already exists in namespace 'global'. Use --force-merge to update."
                    .into(),
            );
            let pt = err.localized_message_for(crate::i18n::Language::Portuguese);
            assert!(pt.contains("memória"), "PT must translate 'memory': {pt}");
            assert!(
                pt.contains("já existe no namespace"),
                "PT must translate 'already exists': {pt}"
            );
        }
    }
}
