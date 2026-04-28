//! Namespace resolution layer (flag > env > "global" fallback).
//!
//! Validates and resolves the active namespace used to scope all SQLite
//! operations, enforcing safe characters and traversal-free names.

use crate::errors::AppError;
use crate::i18n::validation;
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

/// Resolves the active namespace, returning only the final name.
///
/// Shortcut over [`detect_namespace`] when the source does not matter.
/// With a valid explicit flag, the returned namespace is exactly the passed value.
/// Without a flag, the final fallback is `"global"`.
///
/// # Errors
///
/// Returns [`AppError::Validation`] if `explicit` contains invalid characters
/// or exceeds 80 characters.
///
/// # Examples
///
/// ```
/// use sqlite_graphrag::namespace::resolve_namespace;
///
/// // A valid explicit flag is accepted and reflected in the result.
/// let ns = resolve_namespace(Some("meu-projeto")).unwrap();
/// assert_eq!(ns, "meu-projeto");
/// ```
///
/// ```
/// use sqlite_graphrag::namespace::resolve_namespace;
/// use sqlite_graphrag::errors::AppError;
///
/// // Namespace with invalid characters causes a validation error (exit 1).
/// let err = resolve_namespace(Some("ns with space")).unwrap_err();
/// assert_eq!(err.exit_code(), 1);
/// ```
pub fn resolve_namespace(explicit: Option<&str>) -> Result<String, AppError> {
    Ok(detect_namespace(explicit)?.namespace)
}

/// Resolves the active namespace, returning a struct with the source and current directory.
///
/// Precedence: explicit flag > `SQLITE_GRAPHRAG_NAMESPACE` > fallback `"global"`.
///
/// # Errors
///
/// Returns [`AppError::Validation`] if the resolved namespace contains invalid characters.
///
/// # Examples
///
/// ```
/// use sqlite_graphrag::namespace::{detect_namespace, NamespaceSource};
///
/// // With an explicit flag, the source is `ExplicitFlag`.
/// let res = detect_namespace(Some("producao")).unwrap();
/// assert_eq!(res.namespace, "producao");
/// assert_eq!(res.source, NamespaceSource::ExplicitFlag);
/// ```
///
/// ```
/// use sqlite_graphrag::namespace::{detect_namespace, NamespaceSource};
///
/// // Without any explicit configuration, fallback is "global".
/// // Removes env var to guarantee deterministic behaviour.
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
        return Err(AppError::Validation(validation::namespace_length()));
    }
    if !ns
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::Validation(validation::namespace_format()));
    }
    Ok(())
}

fn normalize_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}
