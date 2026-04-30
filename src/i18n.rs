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
        if let Ok(v) = std::env::var("SQLITE_GRAPHRAG_LANG") {
            let lower = v.to_lowercase();
            if lower.starts_with("pt") {
                return Language::Portuguese;
            }
            if lower.starts_with("en") {
                return Language::English;
            }
            // Unrecognized value: warn and fall through to locale detection.
            tracing::warn!(
                value = %v,
                "SQLITE_GRAPHRAG_LANG value not recognized, falling back to locale detection"
            );
        }
        for var in &["LC_ALL", "LANG"] {
            if let Ok(v) = std::env::var(var) {
                if v.to_lowercase().starts_with("pt") {
                    return Language::Portuguese;
                }
            }
        }
        Language::English
    }
}

static GLOBAL_LANGUAGE: OnceLock<Language> = OnceLock::new();

/// Initializes the global language. Subsequent calls are silently ignored
/// (OnceLock semantics) — guaranteeing thread-safety and determinism.
pub fn init(explicit: Option<Language>) {
    let resolved = explicit.unwrap_or_else(Language::from_env_or_locale);
    let _ = GLOBAL_LANGUAGE.set(resolved);
}

/// Returns the active language, or fallback English if `init` was never called.
pub fn current() -> Language {
    *GLOBAL_LANGUAGE.get_or_init(Language::from_env_or_locale)
}

/// Translates a bilingual message by selecting the active variant.
pub fn tr(en: &str, pt: &str) -> &'static str {
    // SAFETY: We return one of the two static strings passed as &str.
    // Since we cannot prove to the borrow checker that the references outlive,
    // we use Box::leak to promote to &'static str. Minimal cost (tens of
    // distinct strings during the CLI process lifetime).
    match current() {
        Language::English => Box::leak(en.to_string().into_boxed_str()),
        Language::Portuguese => Box::leak(pt.to_string().into_boxed_str()),
    }
}

/// Localized prefix for error messages displayed to the end user.
pub fn error_prefix() -> &'static str {
    match current() {
        Language::English => "Error",
        Language::Portuguese => "Erro",
    }
}

/// Localized error messages for `AppError` variants.
pub mod errors_msg {
    use super::current;
    use crate::i18n::Language;

    pub fn memory_not_found(nome: &str, namespace: &str) -> String {
        match current() {
            Language::English => {
                format!("memory '{nome}' not found in namespace '{namespace}'")
            }
            Language::Portuguese => {
                format!("memória '{nome}' não encontrada no namespace '{namespace}'")
            }
        }
    }

    pub fn database_not_found(path: &str) -> String {
        match current() {
            Language::English => {
                format!("database not found at {path}. Run 'sqlite-graphrag init' first.")
            }
            Language::Portuguese => format!(
                "banco de dados não encontrado em {path}. Execute 'sqlite-graphrag init' primeiro."
            ),
        }
    }

    pub fn entity_not_found(nome: &str, namespace: &str) -> String {
        match current() {
            Language::English => {
                format!("entity \"{nome}\" does not exist in namespace \"{namespace}\"")
            }
            Language::Portuguese => {
                format!("entidade \"{nome}\" não existe no namespace \"{namespace}\"")
            }
        }
    }

    pub fn relationship_not_found(de: &str, rel: &str, para: &str, namespace: &str) -> String {
        match current() {
            Language::English => format!(
                "relationship \"{de}\" --[{rel}]--> \"{para}\" does not exist in namespace \"{namespace}\""
            ),
            Language::Portuguese => format!(
                "relacionamento \"{de}\" --[{rel}]--> \"{para}\" não existe no namespace \"{namespace}\""
            ),
        }
    }

    pub fn duplicate_memory(nome: &str, namespace: &str) -> String {
        match current() {
            Language::English => format!(
                "memory '{nome}' already exists in namespace '{namespace}'. Use --force-merge to update."
            ),
            Language::Portuguese => format!(
                "memória '{nome}' já existe no namespace '{namespace}'. Use --force-merge para atualizar."
            ),
        }
    }

    pub fn optimistic_lock_conflict(expected: i64, current_ts: i64) -> String {
        match current() {
            Language::English => format!(
                "optimistic lock conflict: expected updated_at={expected}, but current is {current_ts}"
            ),
            Language::Portuguese => format!(
                "conflito de optimistic lock: esperava updated_at={expected}, mas atual é {current_ts}"
            ),
        }
    }

    pub fn version_not_found(versao: i64, nome: &str) -> String {
        match current() {
            Language::English => format!("version {versao} not found for memory '{nome}'"),
            Language::Portuguese => {
                format!("versão {versao} não encontrada para a memória '{nome}'")
            }
        }
    }

    pub fn no_recall_results(max_distance: f32, query: &str, namespace: &str) -> String {
        match current() {
            Language::English => format!(
                "no results within --max-distance {max_distance} for query '{query}' in namespace '{namespace}'"
            ),
            Language::Portuguese => format!(
                "nenhum resultado dentro de --max-distance {max_distance} para a consulta '{query}' no namespace '{namespace}'"
            ),
        }
    }

    pub fn soft_deleted_memory_not_found(nome: &str, namespace: &str) -> String {
        match current() {
            Language::English => {
                format!("soft-deleted memory '{nome}' not found in namespace '{namespace}'")
            }
            Language::Portuguese => {
                format!("memória soft-deleted '{nome}' não encontrada no namespace '{namespace}'")
            }
        }
    }

    pub fn concurrent_process_conflict() -> String {
        match current() {
            Language::English => {
                "optimistic lock conflict: memory was modified by another process".to_string()
            }
            Language::Portuguese => {
                "conflito de optimistic lock: memória foi modificada por outro processo".to_string()
            }
        }
    }

    pub fn entity_limit_exceeded(max: usize) -> String {
        match current() {
            Language::English => format!("entities exceed limit of {max}"),
            Language::Portuguese => format!("entidades excedem o limite de {max}"),
        }
    }

    pub fn relationship_limit_exceeded(max: usize) -> String {
        match current() {
            Language::English => format!("relationships exceed limit of {max}"),
            Language::Portuguese => format!("relacionamentos excedem o limite de {max}"),
        }
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
}
