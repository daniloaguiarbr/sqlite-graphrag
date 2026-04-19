use crate::errors::AppError;
use crate::i18n::validacao;
use directories::ProjectDirs;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NamespaceSource {
    ExplicitFlag,
    Environment,
    ProjectConfig,
    ProjectsMapping,
    Default,
}

#[derive(Debug, Clone, Serialize)]
pub struct NamespaceResolution {
    pub namespace: String,
    pub source: NamespaceSource,
    pub cwd: String,
    pub project_config_path: String,
    pub projects_mapping_path: String,
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
/// use neurographrag::namespace::resolve_namespace;
///
/// // Flag explícita válida é aceita e refletida no resultado.
/// let ns = resolve_namespace(Some("meu-projeto")).unwrap();
/// assert_eq!(ns, "meu-projeto");
/// ```
///
/// ```
/// use neurographrag::namespace::resolve_namespace;
/// use neurographrag::errors::AppError;
///
/// // Namespace com caracteres inválidos causa erro de validação (exit 1).
/// let err = resolve_namespace(Some("ns com espaço")).unwrap_err();
/// assert_eq!(err.exit_code(), 1);
/// ```
pub fn resolve_namespace(explicit: Option<&str>) -> Result<String, AppError> {
    Ok(detect_namespace(explicit)?.namespace)
}

/// Resolve o namespace ativo retornando estrutura com origem e caminhos.
///
/// A precedência é: flag explícita > `NEUROGRAPHRAG_NAMESPACE` > `.neurographrag/config.toml`
/// > `projects.toml` > fallback `"global"`.
///
/// # Errors
///
/// Retorna [`AppError::Validation`] se o namespace resolvido contiver caracteres inválidos.
///
/// # Examples
///
/// ```
/// use neurographrag::namespace::{detect_namespace, NamespaceSource};
///
/// // Com flag explícita, a fonte é `ExplicitFlag`.
/// let res = detect_namespace(Some("producao")).unwrap();
/// assert_eq!(res.namespace, "producao");
/// assert_eq!(res.source, NamespaceSource::ExplicitFlag);
/// ```
///
/// ```
/// use neurographrag::namespace::{detect_namespace, NamespaceSource};
///
/// // Sem nenhuma configuração e fora de diretório mapeado, fallback é "global".
/// // Desativa env var para garantir comportamento determinístico.
/// std::env::remove_var("NEUROGRAPHRAG_NAMESPACE");
/// let res = detect_namespace(None).unwrap();
/// // Fonte pode ser Default ou ProjectsMapping dependendo do diretório de trabalho;
/// // mas o namespace nunca é vazio.
/// assert!(!res.namespace.is_empty());
/// ```
pub fn detect_namespace(explicit: Option<&str>) -> Result<NamespaceResolution, AppError> {
    let cwd = std::env::current_dir().map_err(AppError::Io)?;
    let cwd_display = normalize_path(&cwd);
    let project_config_path = cwd.join(".neurographrag").join("config.toml");
    let projects_mapping_path = project_dirs()
        .map(|dirs| dirs.config_dir().join("projects.toml"))
        .unwrap_or_else(|| PathBuf::from("projects.toml"));

    if let Some(ns) = explicit {
        validate_namespace(ns)?;
        return Ok(NamespaceResolution {
            namespace: ns.to_owned(),
            source: NamespaceSource::ExplicitFlag,
            cwd: cwd_display,
            project_config_path: project_config_path.display().to_string(),
            projects_mapping_path: projects_mapping_path.display().to_string(),
        });
    }

    if let Ok(ns) = std::env::var("NEUROGRAPHRAG_NAMESPACE") {
        if !ns.is_empty() {
            validate_namespace(&ns)?;
            return Ok(NamespaceResolution {
                namespace: ns,
                source: NamespaceSource::Environment,
                cwd: cwd_display,
                project_config_path: project_config_path.display().to_string(),
                projects_mapping_path: projects_mapping_path.display().to_string(),
            });
        }
    }

    if let Some(ns) = read_project_namespace(&project_config_path)? {
        return Ok(NamespaceResolution {
            namespace: ns,
            source: NamespaceSource::ProjectConfig,
            cwd: cwd_display.clone(),
            project_config_path: project_config_path.display().to_string(),
            projects_mapping_path: projects_mapping_path.display().to_string(),
        });
    }

    if let Some(ns) = read_projects_mapping(&projects_mapping_path, &cwd)? {
        return Ok(NamespaceResolution {
            namespace: ns,
            source: NamespaceSource::ProjectsMapping,
            cwd: cwd_display,
            project_config_path: project_config_path.display().to_string(),
            projects_mapping_path: projects_mapping_path.display().to_string(),
        });
    }

    Ok(NamespaceResolution {
        namespace: "global".to_owned(),
        source: NamespaceSource::Default,
        cwd: cwd_display,
        project_config_path: project_config_path.display().to_string(),
        projects_mapping_path: projects_mapping_path.display().to_string(),
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

fn read_project_namespace(path: &Path) -> Result<Option<String>, AppError> {
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(path)?;
    let value = content.parse::<toml::Value>().map_err(|e| {
        AppError::Validation(validacao::config_namespace_invalido(
            &path.display().to_string(),
            &e.to_string(),
        ))
    })?;

    let namespace = value
        .get("namespace")
        .and_then(toml::Value::as_str)
        .or_else(|| {
            value
                .get("project")
                .and_then(|p| p.get("namespace"))
                .and_then(toml::Value::as_str)
        });

    match namespace {
        Some(ns) if !ns.is_empty() => {
            validate_namespace(ns)?;
            Ok(Some(ns.to_owned()))
        }
        _ => Ok(None),
    }
}

fn read_projects_mapping(path: &Path, cwd: &Path) -> Result<Option<String>, AppError> {
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(path)?;
    let value = content.parse::<toml::Value>().map_err(|e| {
        AppError::Validation(validacao::projects_mapping_invalido(
            &path.display().to_string(),
            &e.to_string(),
        ))
    })?;

    let cwd_normalized = normalize_path(cwd);

    if let Some(projects) = value.get("projects").and_then(toml::Value::as_table) {
        for (project_path, namespace_value) in projects {
            let Some(namespace) = namespace_value.as_str() else {
                continue;
            };
            if normalize_path(Path::new(project_path)) == cwd_normalized {
                validate_namespace(namespace)?;
                return Ok(Some(namespace.to_owned()));
            }
        }
    }

    if let Some(entries) = value.get("project").and_then(toml::Value::as_array) {
        for entry in entries {
            let Some(table) = entry.as_table() else {
                continue;
            };
            let Some(project_path) = table.get("path").and_then(toml::Value::as_str) else {
                continue;
            };
            let Some(namespace) = table.get("namespace").and_then(toml::Value::as_str) else {
                continue;
            };
            if normalize_path(Path::new(project_path)) == cwd_normalized {
                validate_namespace(namespace)?;
                return Ok(Some(namespace.to_owned()));
            }
        }
    }

    Ok(None)
}

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "neurographrag")
}

fn normalize_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}
