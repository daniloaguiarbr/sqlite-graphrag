//! XDG/cwd path resolution and traversal-safe overrides.
//!
//! Resolves data directories via [`directories::ProjectDirs`] and validates
//! that user-supplied paths cannot escape the project root.

use crate::errors::AppError;
use crate::i18n::validation;
use directories::ProjectDirs;
use std::path::{Component, Path, PathBuf};

/// Resolved filesystem paths used by the CLI at runtime.
///
/// Constructed via [`AppPaths::resolve`], which applies the three-layer precedence:
/// CLI flag → `SQLITE_GRAPHRAG_DB_PATH` env var → `SQLITE_GRAPHRAG_HOME` env var → cwd.
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// Absolute path to the SQLite database file.
    pub db: PathBuf,
    /// Directory where embedding model files are cached.
    pub models: PathBuf,
}

impl AppPaths {
    pub fn resolve(db_override: Option<&str>) -> Result<Self, AppError> {
        let proj = ProjectDirs::from("", "", "sqlite-graphrag").ok_or_else(|| {
            AppError::Io(std::io::Error::other("could not determine home directory"))
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
    if Path::new(p).components().any(|c| c == Component::ParentDir) {
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
            "path '{}' has no valid parent component",
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
    fn clean_env_paths() {
        // SAFETY: tests are annotated with #[serial], guaranteeing single-threaded execution.
        unsafe {
            std::env::remove_var("SQLITE_GRAPHRAG_HOME");
            std::env::remove_var("SQLITE_GRAPHRAG_DB_PATH");
            std::env::remove_var("SQLITE_GRAPHRAG_CACHE_DIR");
        }
    }

    #[test]
    #[serial]
    fn home_env_resolves_db_in_subdir() {
        clean_env_paths();
        let tmp = TempDir::new().expect("tempdir");
        // SAFETY: tests are annotated with #[serial], guaranteeing single-threaded execution.
        unsafe {
            std::env::set_var("SQLITE_GRAPHRAG_HOME", tmp.path());
        }

        let paths = AppPaths::resolve(None).expect("resolve with valid HOME");
        assert_eq!(paths.db, tmp.path().join("graphrag.sqlite"));

        clean_env_paths();
    }

    #[test]
    #[serial]
    fn home_env_traversal_rejected() {
        clean_env_paths();
        // SAFETY: tests are annotated with #[serial], guaranteeing single-threaded execution.
        unsafe {
            std::env::set_var("SQLITE_GRAPHRAG_HOME", "/tmp/../etc");
        }

        let result = AppPaths::resolve(None);
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "traversal in SQLITE_GRAPHRAG_HOME must fail as Validation, got {result:?}"
        );

        clean_env_paths();
    }

    #[test]
    #[serial]
    fn db_path_overrides_home() {
        clean_env_paths();
        let tmp_home = TempDir::new().expect("tempdir home");
        let tmp_db = TempDir::new().expect("tempdir db");
        let explicit_db = tmp_db.path().join("explicit.sqlite");
        // SAFETY: tests are annotated with #[serial], guaranteeing single-threaded execution.
        unsafe {
            std::env::set_var("SQLITE_GRAPHRAG_HOME", tmp_home.path());
            std::env::set_var("SQLITE_GRAPHRAG_DB_PATH", &explicit_db);
        }

        let paths = AppPaths::resolve(None).expect("resolve with DB_PATH and HOME");
        assert_eq!(paths.db, explicit_db);

        clean_env_paths();
    }

    #[test]
    #[serial]
    fn flag_overrides_home() {
        clean_env_paths();
        let tmp_home = TempDir::new().expect("tempdir home");
        let tmp_flag = TempDir::new().expect("tempdir flag");
        let db_flag = tmp_flag.path().join("via-flag.sqlite");
        // SAFETY: tests are annotated with #[serial], guaranteeing single-threaded execution.
        unsafe {
            std::env::set_var("SQLITE_GRAPHRAG_HOME", tmp_home.path());
        }

        let paths = AppPaths::resolve(Some(db_flag.to_str().expect("utf8")))
            .expect("resolve with flag and HOME");
        assert_eq!(paths.db, db_flag);

        clean_env_paths();
    }

    #[test]
    #[serial]
    fn home_env_empty_falls_back_to_cwd() {
        clean_env_paths();
        // SAFETY: tests are annotated with #[serial], guaranteeing single-threaded execution.
        unsafe {
            std::env::set_var("SQLITE_GRAPHRAG_HOME", "");
        }

        let paths = AppPaths::resolve(None).expect("resolve with empty HOME");
        let expected = std::env::current_dir()
            .expect("cwd")
            .join("graphrag.sqlite");
        assert_eq!(paths.db, expected);

        clean_env_paths();
    }

    #[test]
    fn parent_or_err_accepts_normal_path() {
        let p = PathBuf::from("/home/user/db.sqlite");
        let parent = parent_or_err(&p).expect("valid parent");
        assert_eq!(parent, Path::new("/home/user"));
    }

    #[test]
    fn parent_or_err_accepts_relative_path() {
        let p = PathBuf::from("subdir/file.sqlite");
        let parent = parent_or_err(&p).expect("relative parent");
        assert_eq!(parent, Path::new("subdir"));
    }

    #[test]
    fn parent_or_err_rejects_unix_root() {
        let p = PathBuf::from("/");
        let result = parent_or_err(&p);
        assert!(matches!(result, Err(AppError::Validation(_))));
    }

    #[test]
    fn parent_or_err_rejects_empty_path() {
        let p = PathBuf::from("");
        let result = parent_or_err(&p);
        assert!(matches!(result, Err(AppError::Validation(_))));
    }
}
