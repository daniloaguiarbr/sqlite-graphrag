use crate::errors::AppError;
use crate::i18n::validacao;
use directories::ProjectDirs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct AppPaths {
    pub db: PathBuf,
    pub models: PathBuf,
}

impl AppPaths {
    pub fn resolve(db_override: Option<&str>) -> Result<Self, AppError> {
        let proj = ProjectDirs::from("", "", "sqlite-graphrag").ok_or_else(|| {
            AppError::Io(std::io::Error::other(
                "não foi possível determinar o diretório home",
            ))
        })?;

        let cache_root = if let Some(override_dir) = std::env::var_os("SQLITE_GRAPHRAG_CACHE_DIR") {
            PathBuf::from(override_dir)
        } else {
            proj.cache_dir().to_path_buf()
        };

        let db = if let Some(p) = db_override {
            validate_path(p)?;
            PathBuf::from(p)
        } else if let Ok(env_path) = std::env::var("SQLITE_GRAPHRAG_DB_PATH") {
            validate_path(&env_path)?;
            PathBuf::from(env_path)
        } else {
            std::env::current_dir()
                .map_err(AppError::Io)?
                .join("graphrag.sqlite")
        };

        Ok(Self {
            db,
            models: cache_root.join("models"),
        })
    }

    pub fn ensure_dirs(&self) -> Result<(), AppError> {
        for dir in [parent_or_err(&self.db)?, self.models.as_path()] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

fn validate_path(p: &str) -> Result<(), AppError> {
    if p.contains("..") {
        return Err(AppError::Validation(validacao::path_traversal(p)));
    }
    Ok(())
}

pub(crate) fn parent_or_err(path: &Path) -> Result<&Path, AppError> {
    path.parent().ok_or_else(|| {
        AppError::Validation(format!(
            "caminho '{}' não possui componente pai válido",
            path.display()
        ))
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn parent_or_err_aceita_path_normal() {
        let p = PathBuf::from("/home/usuario/db.sqlite");
        let pai = parent_or_err(&p).expect("parent valido");
        assert_eq!(pai, Path::new("/home/usuario"));
    }

    #[test]
    fn parent_or_err_aceita_path_relativo() {
        let p = PathBuf::from("subpasta/arquivo.sqlite");
        let pai = parent_or_err(&p).expect("parent relativo");
        assert_eq!(pai, Path::new("subpasta"));
    }

    #[test]
    fn parent_or_err_rejeita_raiz_unix() {
        let p = PathBuf::from("/");
        let resultado = parent_or_err(&p);
        assert!(matches!(resultado, Err(AppError::Validation(_))));
    }

    #[test]
    fn parent_or_err_rejeita_path_vazio() {
        let p = PathBuf::from("");
        let resultado = parent_or_err(&p);
        assert!(matches!(resultado, Err(AppError::Validation(_))));
    }
}
