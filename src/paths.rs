use crate::errors::AppError;
use crate::i18n::validacao;
use directories::ProjectDirs;
use std::path::PathBuf;

#[derive(Debug)]
pub struct AppPaths {
    pub db: PathBuf,
    pub models: PathBuf,
    pub logs: PathBuf,
    pub config: PathBuf,
}

impl AppPaths {
    pub fn resolve(db_override: Option<&str>) -> Result<Self, AppError> {
        let proj = ProjectDirs::from("", "", "neurographrag").ok_or_else(|| {
            AppError::Io(std::io::Error::other("cannot determine home directory"))
        })?;

        let data_dir = proj.data_dir().to_path_buf();
        let cache_dir = proj.cache_dir().to_path_buf();
        let config_dir = proj.config_dir().to_path_buf();
        let state_dir = if cfg!(target_os = "linux") {
            proj.state_dir()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| data_dir.join("logs"))
        } else {
            data_dir.join("logs")
        };

        let db = if let Some(p) = db_override {
            validate_path(p)?;
            PathBuf::from(p)
        } else if let Ok(env_path) = std::env::var("NEUROGRAPHRAG_DB_PATH") {
            validate_path(&env_path)?;
            PathBuf::from(env_path)
        } else {
            data_dir.join("graph.sqlite")
        };

        Ok(Self {
            db,
            models: cache_dir.join("models"),
            logs: state_dir.join("logs"),
            config: config_dir.join("config.toml"),
        })
    }

    pub fn ensure_dirs(&self) -> Result<(), AppError> {
        for dir in [
            self.db.parent().unwrap(),
            &self.models,
            &self.logs,
            self.config.parent().unwrap(),
        ] {
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
