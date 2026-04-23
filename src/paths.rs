use crate::errors::AppError;
use crate::i18n::validacao;
use directories::ProjectDirs;
use std::path::PathBuf;

#[derive(Debug)]
pub struct AppPaths {
    pub db: PathBuf,
    pub models: PathBuf,
}

impl AppPaths {
    pub fn resolve(db_override: Option<&str>) -> Result<Self, AppError> {
        let proj = ProjectDirs::from("", "", "sqlite-graphrag").ok_or_else(|| {
            AppError::Io(std::io::Error::other("cannot determine home directory"))
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
        for dir in [self.db.parent().unwrap(), &self.models] {
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
