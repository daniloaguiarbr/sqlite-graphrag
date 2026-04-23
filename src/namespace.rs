use crate::errors::AppError;
use crate::i18n::validacao;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NamespaceSource {
    ExplicitFlag,
    Environment,
    Default,
}

#[derive(Debug, Clone, Serialize)]
pub struct NamespaceResolution {
    pub namespace: String,
    pub source: NamespaceSource,
    pub cwd: String,
}

/// Resolve o namespace ativo retornando apenas o nome final.
///
/// Atalho sobre [`detect_namespace`] quando a origem não importa.
/// Com flag explícita válida, o namespace retornado é exatamente o valor passado.
/// Sem flag, o fallback final é `"global"`.
///
/// # Errors
///
/// Retorna [`AppError::Validation`] se `explicit` contiver caracteres inválidos
/// ou ultrapassar 80 caracteres.
///
/// # Examples
///
/// ```
/// use sqlite_graphrag::namespace::resolve_namespace;
///
/// // Flag explícita válida é aceita e refletida no resultado.
/// let ns = resolve_namespace(Some("meu-projeto")).unwrap();
/// assert_eq!(ns, "meu-projeto");
/// ```
///
/// ```
/// use sqlite_graphrag::namespace::resolve_namespace;
/// use sqlite_graphrag::errors::AppError;
///
/// // Namespace com caracteres inválidos causa erro de validação (exit 1).
/// let err = resolve_namespace(Some("ns com espaço")).unwrap_err();
/// assert_eq!(err.exit_code(), 1);
/// ```
pub fn resolve_namespace(explicit: Option<&str>) -> Result<String, AppError> {
    Ok(detect_namespace(explicit)?.namespace)
}

/// Resolve o namespace ativo retornando estrutura com origem e diretório atual.
///
/// A precedência é: flag explícita > `SQLITE_GRAPHRAG_NAMESPACE` > fallback `"global"`.
///
/// # Errors
///
/// Retorna [`AppError::Validation`] se o namespace resolvido contiver caracteres inválidos.
///
/// # Examples
///
/// ```
/// use sqlite_graphrag::namespace::{detect_namespace, NamespaceSource};
///
/// // Com flag explícita, a fonte é `ExplicitFlag`.
/// let res = detect_namespace(Some("producao")).unwrap();
/// assert_eq!(res.namespace, "producao");
/// assert_eq!(res.source, NamespaceSource::ExplicitFlag);
/// ```
///
/// ```
/// use sqlite_graphrag::namespace::{detect_namespace, NamespaceSource};
///
/// // Sem nenhuma configuração explícita, fallback é "global".
/// // Desativa env var para garantir comportamento determinístico.
/// std::env::remove_var("SQLITE_GRAPHRAG_NAMESPACE");
/// let res = detect_namespace(None).unwrap();
/// assert_eq!(res.namespace, "global");
/// assert_eq!(res.source, NamespaceSource::Default);
/// ```
pub fn detect_namespace(explicit: Option<&str>) -> Result<NamespaceResolution, AppError> {
    let cwd = std::env::current_dir().map_err(AppError::Io)?;
    let cwd_display = normalize_path(&cwd);

    if let Some(ns) = explicit {
        validate_namespace(ns)?;
        return Ok(NamespaceResolution {
            namespace: ns.to_owned(),
            source: NamespaceSource::ExplicitFlag,
            cwd: cwd_display,
        });
    }

    if let Ok(ns) = std::env::var("SQLITE_GRAPHRAG_NAMESPACE") {
        if !ns.is_empty() {
            validate_namespace(&ns)?;
            return Ok(NamespaceResolution {
                namespace: ns,
                source: NamespaceSource::Environment,
                cwd: cwd_display,
            });
        }
    }

    Ok(NamespaceResolution {
        namespace: "global".to_owned(),
        source: NamespaceSource::Default,
        cwd: cwd_display,
    })
}

fn validate_namespace(ns: &str) -> Result<(), AppError> {
    if ns.is_empty() || ns.len() > 80 {
        return Err(AppError::Validation(validacao::namespace_comprimento()));
    }
    if !ns
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::Validation(validacao::namespace_formato()));
    }
    Ok(())
}

fn normalize_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}
