//! XDG/cwd path resolution and traversal-safe overrides.
//!
//! Resolves data directories via [`directories::ProjectDirs`] and validates
//! that user-supplied paths cannot escape the project root.

use crate::errors::AppError;
use crate::i18n::validation;
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
        } else if let Some(home_dir) = home_env_dir()? {
            home_dir.join("graphrag.sqlite")
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
        return Err(AppError::Validation(validation::path_traversal(p)));
    }
    Ok(())
}

/// Resolves `SQLITE_GRAPHRAG_HOME` as the root directory for the default database.
///
/// Returns `Ok(Some(dir))` when the env var is set and valid,
/// `Ok(None)` when absent or empty (falls back to `current_dir`),
/// and `Err(...)` when the value contains traversal components.
fn home_env_dir() -> Result<Option<PathBuf>, AppError> {
    let raw = match std::env::var("SQLITE_GRAPHRAG_HOME") {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if raw.is_empty() {
        return Ok(None);
    }
    validate_path(&raw)?;
    Ok(Some(PathBuf::from(raw)))
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
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    /// Clears all variables that affect `AppPaths::resolve` to isolate the
    /// test from the developer/CI environment.
    fn limpar_env_paths() {
        // SAFETY: testes marcados com #[serial] garantem ausência de concorrência.
        unsafe {
            std::env::remove_var("SQLITE_GRAPHRAG_HOME");
            std::env::remove_var("SQLITE_GRAPHRAG_DB_PATH");
            std::env::remove_var("SQLITE_GRAPHRAG_CACHE_DIR");
        }
    }

    #[test]
    #[serial]
    fn home_env_resolve_db_em_subdir() {
        limpar_env_paths();
        let tmp = TempDir::new().expect("tempdir");
        // SAFETY: serial.
        unsafe {
            std::env::set_var("SQLITE_GRAPHRAG_HOME", tmp.path());
        }

        let paths = AppPaths::resolve(None).expect("resolve com HOME valido");
        assert_eq!(paths.db, tmp.path().join("graphrag.sqlite"));

        limpar_env_paths();
    }

    #[test]
    #[serial]
    fn home_env_traversal_rejeitado() {
        limpar_env_paths();
        // SAFETY: serial.
        unsafe {
            std::env::set_var("SQLITE_GRAPHRAG_HOME", "/tmp/../etc");
        }

        let resultado = AppPaths::resolve(None);
        assert!(
            matches!(resultado, Err(AppError::Validation(_))),
            "traversal em SQLITE_GRAPHRAG_HOME deve falhar como Validation, obteve {resultado:?}"
        );

        limpar_env_paths();
    }

    #[test]
    #[serial]
    fn db_path_vence_home() {
        limpar_env_paths();
        let tmp_home = TempDir::new().expect("tempdir home");
        let tmp_db = TempDir::new().expect("tempdir db");
        let db_explicito = tmp_db.path().join("explicito.sqlite");
        // SAFETY: serial.
        unsafe {
            std::env::set_var("SQLITE_GRAPHRAG_HOME", tmp_home.path());
            std::env::set_var("SQLITE_GRAPHRAG_DB_PATH", &db_explicito);
        }

        let paths = AppPaths::resolve(None).expect("resolve com DB_PATH e HOME");
        assert_eq!(paths.db, db_explicito);

        limpar_env_paths();
    }

    #[test]
    #[serial]
    fn flag_vence_home() {
        limpar_env_paths();
        let tmp_home = TempDir::new().expect("tempdir home");
        let tmp_flag = TempDir::new().expect("tempdir flag");
        let db_flag = tmp_flag.path().join("via-flag.sqlite");
        // SAFETY: serial.
        unsafe {
            std::env::set_var("SQLITE_GRAPHRAG_HOME", tmp_home.path());
        }

        let paths = AppPaths::resolve(Some(db_flag.to_str().expect("utf8")))
            .expect("resolve com flag e HOME");
        assert_eq!(paths.db, db_flag);

        limpar_env_paths();
    }

    #[test]
    #[serial]
    fn home_env_vazio_cai_para_cwd() {
        limpar_env_paths();
        // SAFETY: serial.
        unsafe {
            std::env::set_var("SQLITE_GRAPHRAG_HOME", "");
        }

        let paths = AppPaths::resolve(None).expect("resolve com HOME vazio");
        let esperado = std::env::current_dir()
            .expect("cwd")
            .join("graphrag.sqlite");
        assert_eq!(paths.db, esperado);

        limpar_env_paths();
    }

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
