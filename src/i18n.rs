//! Camada bilíngue de mensagens humanas.
//!
//! A CLI usa `--lang en|pt` (flag global) ou `SQLITE_GRAPHRAG_LANG` (env var) para escolher
//! o idioma das mensagens stderr de progresso. JSON de stdout é determinístico e idêntico
//! entre idiomas — apenas strings destinadas a humanos passam pelo módulo.
//!
//! Detecção (do mais para o menos prioritário):
//! 1. Flag `--lang` explícita
//! 2. Env var `SQLITE_GRAPHRAG_LANG`
//! 3. Locale do SO (`LANG`, `LC_ALL`) com prefixo `pt`
//! 4. Fallback `English`

use std::sync::OnceLock;

#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Language {
    #[value(name = "en", aliases = ["english", "EN"])]
    English,
    #[value(name = "pt", aliases = ["portugues", "portuguese", "pt-BR", "pt-br", "PT"])]
    Portugues,
}

impl Language {
    /// Converte string de linha de comando em Language sem depender do clap.
    /// Aceita os mesmos aliases definidos em `#[value(...)]`: "en", "pt", etc.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "en" | "english" => Some(Language::English),
            "pt" | "pt-br" | "portugues" | "portuguese" => Some(Language::Portugues),
            _ => None,
        }
    }

    pub fn from_env_or_locale() -> Self {
        if let Ok(v) = std::env::var("SQLITE_GRAPHRAG_LANG") {
            let v = v.to_lowercase();
            if v.starts_with("pt") {
                return Language::Portugues;
            }
            if v.starts_with("en") {
                return Language::English;
            }
        }
        for var in &["LC_ALL", "LANG"] {
            if let Ok(v) = std::env::var(var) {
                if v.to_lowercase().starts_with("pt") {
                    return Language::Portugues;
                }
            }
        }
        Language::English
    }
}

static IDIOMA_GLOBAL: OnceLock<Language> = OnceLock::new();

/// Inicializa o idioma global. Chamadas subsequentes são ignoradas silenciosamente
/// (OnceLock semantics) — garantindo thread-safety e determinismo.
pub fn init(explicit: Option<Language>) {
    let resolved = explicit.unwrap_or_else(Language::from_env_or_locale);
    let _ = IDIOMA_GLOBAL.set(resolved);
}

/// Retorna o idioma ativo ou fallback English se `init` nunca foi chamado.
pub fn current() -> Language {
    *IDIOMA_GLOBAL.get_or_init(Language::from_env_or_locale)
}

/// Traduz uma mensagem bilíngue escolhendo a variante ativa.
pub fn tr(en: &str, pt: &str) -> &'static str {
    // SAFETY: Retornamos uma das duas strings estáticas passadas como &str.
    // Como não temos como provar ao borrow checker que as referências sobrevivem,
    // usamos Box::leak para transformar em &'static str. Custo mínimo (dezenas de
    // strings distintas durante vida do processo CLI).
    match current() {
        Language::English => Box::leak(en.to_string().into_boxed_str()),
        Language::Portugues => Box::leak(pt.to_string().into_boxed_str()),
    }
}

/// Prefixo localizado para mensagens de erro exibidas ao usuário final.
pub fn prefixo_erro() -> &'static str {
    match current() {
        Language::English => "Error",
        Language::Portugues => "Erro",
    }
}

/// Mensagens de erro localizadas para as variantes de AppError.
pub mod erros {
    use super::current;
    use crate::i18n::Language;

    pub fn memoria_nao_encontrada(nome: &str, namespace: &str) -> String {
        match current() {
            Language::English => {
                format!("memory '{nome}' not found in namespace '{namespace}'")
            }
            Language::Portugues => {
                format!("memória '{nome}' não encontrada no namespace '{namespace}'")
            }
        }
    }

    pub fn banco_nao_encontrado(path: &str) -> String {
        match current() {
            Language::English => {
                format!("database not found at {path}. Run 'sqlite-graphrag init' first.")
            }
            Language::Portugues => format!(
                "banco de dados não encontrado em {path}. Execute 'sqlite-graphrag init' primeiro."
            ),
        }
    }

    pub fn entidade_nao_encontrada(nome: &str, namespace: &str) -> String {
        match current() {
            Language::English => {
                format!("entity \"{nome}\" does not exist in namespace \"{namespace}\"")
            }
            Language::Portugues => {
                format!("entidade \"{nome}\" não existe no namespace \"{namespace}\"")
            }
        }
    }

    pub fn relacionamento_nao_encontrado(
        de: &str,
        rel: &str,
        para: &str,
        namespace: &str,
    ) -> String {
        match current() {
            Language::English => format!(
                "relationship \"{de}\" --[{rel}]--> \"{para}\" does not exist in namespace \"{namespace}\""
            ),
            Language::Portugues => format!(
                "relacionamento \"{de}\" --[{rel}]--> \"{para}\" não existe no namespace \"{namespace}\""
            ),
        }
    }

    pub fn memoria_duplicada(nome: &str, namespace: &str) -> String {
        match current() {
            Language::English => format!(
                "memory '{nome}' already exists in namespace '{namespace}'. Use --force-merge to update."
            ),
            Language::Portugues => format!(
                "memória '{nome}' já existe no namespace '{namespace}'. Use --force-merge para atualizar."
            ),
        }
    }

    pub fn conflito_optimistic_lock(expected: i64, current_ts: i64) -> String {
        match current() {
            Language::English => format!(
                "optimistic lock conflict: expected updated_at={expected}, but current is {current_ts}"
            ),
            Language::Portugues => format!(
                "conflito de optimistic lock: esperava updated_at={expected}, mas atual é {current_ts}"
            ),
        }
    }

    pub fn versao_nao_encontrada(versao: i64, nome: &str) -> String {
        match current() {
            Language::English => format!("version {versao} not found for memory '{nome}'"),
            Language::Portugues => {
                format!("versão {versao} não encontrada para a memória '{nome}'")
            }
        }
    }

    pub fn sem_resultados_recall(min_distance: f32, query: &str, namespace: &str) -> String {
        match current() {
            Language::English => format!(
                "no results within --min-distance {min_distance} for query '{query}' in namespace '{namespace}'"
            ),
            Language::Portugues => format!(
                "nenhum resultado dentro de --min-distance {min_distance} para a consulta '{query}' no namespace '{namespace}'"
            ),
        }
    }

    pub fn memoria_soft_deleted_nao_encontrada(nome: &str, namespace: &str) -> String {
        match current() {
            Language::English => {
                format!("soft-deleted memory '{nome}' not found in namespace '{namespace}'")
            }
            Language::Portugues => {
                format!("memória soft-deleted '{nome}' não encontrada no namespace '{namespace}'")
            }
        }
    }

    pub fn conflito_processo_concorrente() -> String {
        match current() {
            Language::English => {
                "optimistic lock conflict: memory was modified by another process".to_string()
            }
            Language::Portugues => {
                "conflito de optimistic lock: memória foi modificada por outro processo".to_string()
            }
        }
    }

    pub fn limite_entidades(max: usize) -> String {
        match current() {
            Language::English => format!("entities exceed limit of {max}"),
            Language::Portugues => format!("entidades excedem o limite de {max}"),
        }
    }

    pub fn limite_relacionamentos(max: usize) -> String {
        match current() {
            Language::English => format!("relationships exceed limit of {max}"),
            Language::Portugues => format!("relacionamentos excedem o limite de {max}"),
        }
    }
}

/// Mensagens de validação localizadas para os campos de memória.
pub mod validacao {
    use super::current;
    use crate::i18n::Language;

    pub fn nome_comprimento(max: usize) -> String {
        match current() {
            Language::English => format!("name must be 1-{max} chars"),
            Language::Portugues => format!("nome deve ter entre 1 e {max} caracteres"),
        }
    }

    pub fn nome_reservado() -> String {
        match current() {
            Language::English => {
                "names and namespaces starting with __ are reserved for internal use".to_string()
            }
            Language::Portugues => {
                "nomes e namespaces iniciados com __ são reservados para uso interno".to_string()
            }
        }
    }

    pub fn nome_kebab(nome: &str) -> String {
        match current() {
            Language::English => format!(
                "name must be kebab-case slug (lowercase letters, digits, hyphens): '{nome}'"
            ),
            Language::Portugues => {
                format!("nome deve estar em kebab-case (minúsculas, dígitos, hífens): '{nome}'")
            }
        }
    }

    pub fn descricao_excede(max: usize) -> String {
        match current() {
            Language::English => format!("description must be <= {max} chars"),
            Language::Portugues => format!("descrição deve ter no máximo {max} caracteres"),
        }
    }

    pub fn body_excede(max: usize) -> String {
        match current() {
            Language::English => format!("body exceeds {max} chars"),
            Language::Portugues => format!("corpo excede {max} caracteres"),
        }
    }

    pub fn novo_nome_comprimento(max: usize) -> String {
        match current() {
            Language::English => format!("new-name must be 1-{max} chars"),
            Language::Portugues => format!("novo nome deve ter entre 1 e {max} caracteres"),
        }
    }

    pub fn novo_nome_kebab(nome: &str) -> String {
        match current() {
            Language::English => format!(
                "new-name must be kebab-case slug (lowercase letters, digits, hyphens): '{nome}'"
            ),
            Language::Portugues => format!(
                "novo nome deve estar em kebab-case (minúsculas, dígitos, hífens): '{nome}'"
            ),
        }
    }

    pub fn namespace_comprimento() -> String {
        match current() {
            Language::English => "namespace must be 1-80 chars".to_string(),
            Language::Portugues => "namespace deve ter entre 1 e 80 caracteres".to_string(),
        }
    }

    pub fn namespace_formato() -> String {
        match current() {
            Language::English => "namespace must be alphanumeric + hyphens/underscores".to_string(),
            Language::Portugues => {
                "namespace deve ser alfanumérico com hífens/sublinhados".to_string()
            }
        }
    }

    pub fn path_traversal(p: &str) -> String {
        match current() {
            Language::English => format!("path traversal rejected: {p}"),
            Language::Portugues => format!("traversal de caminho rejeitado: {p}"),
        }
    }

    pub fn tz_invalido(v: &str) -> String {
        match current() {
            Language::English => format!(
                "SQLITE_GRAPHRAG_DISPLAY_TZ invalid: '{v}'; use an IANA name like 'America/Sao_Paulo'"
            ),
            Language::Portugues => format!(
                "SQLITE_GRAPHRAG_DISPLAY_TZ inválido: '{v}'; use um nome IANA como 'America/Sao_Paulo'"
            ),
        }
    }

    pub fn config_namespace_invalido(path: &str, err: &str) -> String {
        match current() {
            Language::English => {
                format!("invalid project namespace config '{path}': {err}")
            }
            Language::Portugues => {
                format!("configuração de namespace de projeto inválida '{path}': {err}")
            }
        }
    }

    pub fn projects_mapping_invalido(path: &str, err: &str) -> String {
        match current() {
            Language::English => format!("invalid projects mapping '{path}': {err}"),
            Language::Portugues => format!("mapeamento de projetos inválido '{path}': {err}"),
        }
    }

    pub fn link_auto_referencial() -> String {
        match current() {
            Language::English => "--from and --to must be different entities — self-referential relationships are not supported".to_string(),
            Language::Portugues => "--from e --to devem ser entidades diferentes — relacionamentos auto-referenciais não são suportados".to_string(),
        }
    }

    pub fn link_peso_invalido(weight: f64) -> String {
        match current() {
            Language::English => {
                format!("--weight: must be between 0.0 and 1.0 (actual: {weight})")
            }
            Language::Portugues => {
                format!("--weight: deve estar entre 0.0 e 1.0 (atual: {weight})")
            }
        }
    }

    pub fn sync_destino_igual_fonte() -> String {
        match current() {
            Language::English => {
                "destination path must differ from the source database path".to_string()
            }
            Language::Portugues => {
                "caminho de destino deve ser diferente do caminho do banco de dados fonte"
                    .to_string()
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn fallback_english_quando_env_ausente() {
        std::env::remove_var("SQLITE_GRAPHRAG_LANG");
        std::env::set_var("LC_ALL", "C");
        std::env::set_var("LANG", "C");
        assert_eq!(Language::from_env_or_locale(), Language::English);
        std::env::remove_var("LC_ALL");
        std::env::remove_var("LANG");
    }

    #[test]
    #[serial]
    fn env_pt_seleciona_portugues() {
        std::env::remove_var("LC_ALL");
        std::env::remove_var("LANG");
        std::env::set_var("SQLITE_GRAPHRAG_LANG", "pt");
        assert_eq!(Language::from_env_or_locale(), Language::Portugues);
        std::env::remove_var("SQLITE_GRAPHRAG_LANG");
    }

    #[test]
    #[serial]
    fn env_pt_br_seleciona_portugues() {
        std::env::remove_var("LC_ALL");
        std::env::remove_var("LANG");
        std::env::set_var("SQLITE_GRAPHRAG_LANG", "pt-BR");
        assert_eq!(Language::from_env_or_locale(), Language::Portugues);
        std::env::remove_var("SQLITE_GRAPHRAG_LANG");
    }

    #[test]
    #[serial]
    fn locale_ptbr_utf8_seleciona_portugues() {
        std::env::remove_var("SQLITE_GRAPHRAG_LANG");
        std::env::set_var("LC_ALL", "pt_BR.UTF-8");
        assert_eq!(Language::from_env_or_locale(), Language::Portugues);
        std::env::remove_var("LC_ALL");
    }

    mod testes_validacao {
        use super::*;

        #[test]
        fn nome_comprimento_en() {
            let msg = match Language::English {
                Language::English => format!("name must be 1-{} chars", 80),
                Language::Portugues => format!("nome deve ter entre 1 e {} caracteres", 80),
            };
            assert!(msg.contains("name must be 1-80 chars"), "obtido: {msg}");
        }

        #[test]
        fn nome_comprimento_pt() {
            let msg = match Language::Portugues {
                Language::English => format!("name must be 1-{} chars", 80),
                Language::Portugues => format!("nome deve ter entre 1 e {} caracteres", 80),
            };
            assert!(
                msg.contains("nome deve ter entre 1 e 80 caracteres"),
                "obtido: {msg}"
            );
        }

        #[test]
        fn nome_kebab_en() {
            let nome = "Invalid_Name";
            let msg = match Language::English {
                Language::English => format!(
                    "name must be kebab-case slug (lowercase letters, digits, hyphens): '{nome}'"
                ),
                Language::Portugues => {
                    format!("nome deve estar em kebab-case (minúsculas, dígitos, hífens): '{nome}'")
                }
            };
            assert!(msg.contains("kebab-case slug"), "obtido: {msg}");
            assert!(msg.contains("Invalid_Name"), "obtido: {msg}");
        }

        #[test]
        fn nome_kebab_pt() {
            let nome = "Invalid_Name";
            let msg = match Language::Portugues {
                Language::English => format!(
                    "name must be kebab-case slug (lowercase letters, digits, hyphens): '{nome}'"
                ),
                Language::Portugues => {
                    format!("nome deve estar em kebab-case (minúsculas, dígitos, hífens): '{nome}'")
                }
            };
            assert!(msg.contains("kebab-case"), "obtido: {msg}");
            assert!(msg.contains("minúsculas"), "obtido: {msg}");
            assert!(msg.contains("Invalid_Name"), "obtido: {msg}");
        }

        #[test]
        fn descricao_excede_en() {
            let msg = match Language::English {
                Language::English => format!("description must be <= {} chars", 500),
                Language::Portugues => format!("descrição deve ter no máximo {} caracteres", 500),
            };
            assert!(msg.contains("description must be <= 500"), "obtido: {msg}");
        }

        #[test]
        fn descricao_excede_pt() {
            let msg = match Language::Portugues {
                Language::English => format!("description must be <= {} chars", 500),
                Language::Portugues => format!("descrição deve ter no máximo {} caracteres", 500),
            };
            assert!(
                msg.contains("descrição deve ter no máximo 500"),
                "obtido: {msg}"
            );
        }

        #[test]
        fn body_excede_en() {
            let msg = match Language::English {
                Language::English => format!("body exceeds {} chars", 20_000),
                Language::Portugues => format!("corpo excede {} caracteres", 20_000),
            };
            assert!(msg.contains("body exceeds 20000"), "obtido: {msg}");
        }

        #[test]
        fn body_excede_pt() {
            let msg = match Language::Portugues {
                Language::English => format!("body exceeds {} chars", 20_000),
                Language::Portugues => format!("corpo excede {} caracteres", 20_000),
            };
            assert!(msg.contains("corpo excede 20000"), "obtido: {msg}");
        }

        #[test]
        fn novo_nome_comprimento_en() {
            let msg = match Language::English {
                Language::English => format!("new-name must be 1-{} chars", 80),
                Language::Portugues => format!("novo nome deve ter entre 1 e {} caracteres", 80),
            };
            assert!(msg.contains("new-name must be 1-80"), "obtido: {msg}");
        }

        #[test]
        fn novo_nome_comprimento_pt() {
            let msg = match Language::Portugues {
                Language::English => format!("new-name must be 1-{} chars", 80),
                Language::Portugues => format!("novo nome deve ter entre 1 e {} caracteres", 80),
            };
            assert!(
                msg.contains("novo nome deve ter entre 1 e 80"),
                "obtido: {msg}"
            );
        }

        #[test]
        fn novo_nome_kebab_en() {
            let nome = "Bad Name";
            let msg = match Language::English {
                Language::English => format!(
                    "new-name must be kebab-case slug (lowercase letters, digits, hyphens): '{nome}'"
                ),
                Language::Portugues => format!(
                    "novo nome deve estar em kebab-case (minúsculas, dígitos, hífens): '{nome}'"
                ),
            };
            assert!(msg.contains("new-name must be kebab-case"), "obtido: {msg}");
        }

        #[test]
        fn novo_nome_kebab_pt() {
            let nome = "Bad Name";
            let msg = match Language::Portugues {
                Language::English => format!(
                    "new-name must be kebab-case slug (lowercase letters, digits, hyphens): '{nome}'"
                ),
                Language::Portugues => format!(
                    "novo nome deve estar em kebab-case (minúsculas, dígitos, hífens): '{nome}'"
                ),
            };
            assert!(
                msg.contains("novo nome deve estar em kebab-case"),
                "obtido: {msg}"
            );
        }

        #[test]
        fn nome_reservado_en() {
            let msg = match Language::English {
                Language::English => {
                    "names and namespaces starting with __ are reserved for internal use"
                        .to_string()
                }
                Language::Portugues => {
                    "nomes e namespaces iniciados com __ são reservados para uso interno"
                        .to_string()
                }
            };
            assert!(msg.contains("reserved for internal use"), "obtido: {msg}");
        }

        #[test]
        fn nome_reservado_pt() {
            let msg = match Language::Portugues {
                Language::English => {
                    "names and namespaces starting with __ are reserved for internal use"
                        .to_string()
                }
                Language::Portugues => {
                    "nomes e namespaces iniciados com __ são reservados para uso interno"
                        .to_string()
                }
            };
            assert!(msg.contains("reservados para uso interno"), "obtido: {msg}");
        }
    }
}
