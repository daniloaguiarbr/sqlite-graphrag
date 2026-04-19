//! Library-wide error type.
//!
//! `AppError` is the single error type returned by every public API in the
//! crate. Each variant maps to a deterministic exit code through
//! `AppError::exit_code`, which the binary propagates to the shell on
//! failure. See the README for the full exit code contract.

use thiserror::Error;

/// Unified error type for all CLI and library operations.
///
/// Each variant corresponds to a distinct failure category. The
/// [`AppError::exit_code`] method converts a variant into a stable numeric
/// code so that shell callers and LLM agents can route on it.
#[derive(Error, Debug)]
pub enum AppError {
    /// Input failed schema, length or format validation. Maps to exit code `1`.
    #[error("validation error: {0}")]
    Validation(String),

    /// A memory or entity with the same `(namespace, name)` already exists. Maps to exit code `2`.
    #[error("duplicate detected: {0}")]
    Duplicate(String),

    /// Optimistic update lost the race because `updated_at` changed. Maps to exit code `3`.
    #[error("conflict: {0}")]
    Conflict(String),

    /// The requested record does not exist or was soft-deleted. Maps to exit code `4`.
    #[error("not found: {0}")]
    NotFound(String),

    /// Namespace could not be resolved from flag, environment or markers. Maps to exit code `5`.
    #[error("namespace not resolved: {0}")]
    NamespaceError(String),

    /// Payload exceeded one of the configured body, name or batch limits. Maps to exit code `6`.
    #[error("limit exceeded: {0}")]
    LimitExceeded(String),

    /// Low-level SQLite error propagated from `rusqlite`. Maps to exit code `10`.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// Embedding generation via `fastembed` failed or produced the wrong shape. Maps to exit code `11`.
    #[error("embedding error: {0}")]
    Embedding(String),

    /// The `sqlite-vec` extension could not load or register its virtual table. Maps to exit code `12`.
    #[error("sqlite-vec extension failed: {0}")]
    VecExtension(String),

    /// SQLite returned `SQLITE_BUSY` after exhausting retries. Maps to exit code `15` (antes de v2.0.0 era `13`; movido para liberar `13` para BatchPartialFailure conforme PRD).
    #[error("database busy: {0}")]
    DbBusy(String),

    /// Batch operation failed partially — N of M items failed. Maps to exit code `13` (PRD 1822).
    ///
    /// Reservado para uso em `import`, `reindex` e batch stdin (BLOCO 3/4). Variante presente
    /// desde v2.0.0 mesmo que call-sites ainda não existam — mapeamento estável de exit code.
    #[error("falha parcial em batch: {failed} de {total} itens falharam")]
    BatchPartialFailure { total: usize, failed: usize },

    /// Filesystem I/O error while reading or writing the database or cache. Maps to exit code `14`.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Unexpected internal error surfaced through `anyhow`. Maps to exit code `20`.
    #[error("internal error: {0}")]
    Internal(#[from] anyhow::Error),

    /// JSON serialization or deserialization failure. Maps to exit code `20`.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Another instance is already running and holds the advisory lock. Maps to exit code `75`.
    ///
    /// Use `--allow-parallel` to skip the lock or `--wait-lock SECONDS` to retry.
    #[error("lock busy: {0}")]
    LockBusy(String),

    /// Todos os slots de concorrência estão ocupados após o tempo de espera. Maps to exit code `75`.
    ///
    /// Ocorre quando [`crate::constants::MAX_CONCURRENT_CLI_INSTANCES`] instâncias já estão
    /// ativas e o limite de espera [`crate::constants::CLI_LOCK_DEFAULT_WAIT_SECS`] foi esgotado.
    #[error(
        "todos os {max} slots de concorrência ocupados após aguardar {waited_secs}s (exit 75); \
         use --max-concurrency ou aguarde outras invocações terminarem"
    )]
    AllSlotsFull { max: usize, waited_secs: u64 },

    /// Memória disponível abaixo do mínimo para carregar o modelo. Maps to exit code `77`.
    ///
    /// Retornado quando `sysinfo` reporta memória disponível inferior a
    /// [`crate::constants::MIN_AVAILABLE_MEMORY_MB`] MiB antes de iniciar o carregamento ONNX.
    #[error(
        "memória disponível ({available_mb}MB) abaixo do mínimo requerido ({required_mb}MB) \
         para carregar o modelo; aborte outras cargas ou use --skip-memory-guard (exit 77)"
    )]
    LowMemory { available_mb: u64, required_mb: u64 },
}

impl AppError {
    /// Returns the deterministic process exit code for this error variant.
    ///
    /// The codes follow the contract documented in the README: `1` for
    /// validation, `2` for duplicates, `3` for conflicts, `4` for missing
    /// records, `5` for namespace errors, `6` for limit violations, `10`–`14`
    /// for infrastructure failures, `13` for BatchPartialFailure (PRD 1822),
    /// `15` for DbBusy (migrated from `13` in v2.0.0), `20` for internal errors,
    /// `75` (EX_TEMPFAIL) when the advisory CLI lock is held or all concurrency
    /// slots are exhausted, and `77` when available memory is insufficient to
    /// load the embedding model.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Validation(_) => 1,
            Self::Duplicate(_) => 2,
            Self::Conflict(_) => 3,
            Self::NotFound(_) => 4,
            Self::NamespaceError(_) => 5,
            Self::LimitExceeded(_) => 6,
            Self::Database(_) => 10,
            Self::Embedding(_) => 11,
            Self::VecExtension(_) => 12,
            Self::BatchPartialFailure { .. } => crate::constants::BATCH_PARTIAL_FAILURE_EXIT_CODE,
            Self::DbBusy(_) => crate::constants::DB_BUSY_EXIT_CODE,
            Self::Io(_) => 14,
            Self::Internal(_) => 20,
            Self::Json(_) => 20,
            Self::LockBusy(_) => crate::constants::CLI_LOCK_EXIT_CODE,
            Self::AllSlotsFull { .. } => crate::constants::CLI_LOCK_EXIT_CODE,
            Self::LowMemory { .. } => crate::constants::LOW_MEMORY_EXIT_CODE,
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::io;

    #[test]
    fn exit_code_validation_retorna_1() {
        assert_eq!(AppError::Validation("campo inválido".into()).exit_code(), 1);
    }

    #[test]
    fn exit_code_duplicate_retorna_2() {
        assert_eq!(AppError::Duplicate("namespace/nome".into()).exit_code(), 2);
    }

    #[test]
    fn exit_code_conflict_retorna_3() {
        assert_eq!(AppError::Conflict("updated_at mudou".into()).exit_code(), 3);
    }

    #[test]
    fn exit_code_not_found_retorna_4() {
        assert_eq!(AppError::NotFound("memória ausente".into()).exit_code(), 4);
    }

    #[test]
    fn exit_code_namespace_error_retorna_5() {
        assert_eq!(
            AppError::NamespaceError("não resolvido".into()).exit_code(),
            5
        );
    }

    #[test]
    fn exit_code_limit_exceeded_retorna_6() {
        assert_eq!(
            AppError::LimitExceeded("corpo muito grande".into()).exit_code(),
            6
        );
    }

    #[test]
    fn exit_code_embedding_retorna_11() {
        assert_eq!(
            AppError::Embedding("falha de modelo".into()).exit_code(),
            11
        );
    }

    #[test]
    fn exit_code_vec_extension_retorna_12() {
        assert_eq!(
            AppError::VecExtension("extensão não carregou".into()).exit_code(),
            12
        );
    }

    #[test]
    fn exit_code_db_busy_retorna_15() {
        assert_eq!(AppError::DbBusy("esgotou retries".into()).exit_code(), 15);
    }

    #[test]
    fn exit_code_batch_partial_failure_retorna_13() {
        assert_eq!(
            AppError::BatchPartialFailure {
                total: 10,
                failed: 3
            }
            .exit_code(),
            13
        );
    }

    #[test]
    fn display_batch_partial_failure_inclui_contagens() {
        let err = AppError::BatchPartialFailure {
            total: 50,
            failed: 7,
        };
        let msg = err.to_string();
        assert!(msg.contains("7"));
        assert!(msg.contains("50"));
        assert!(msg.contains("falha parcial"));
    }

    #[test]
    fn exit_code_io_retorna_14() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "arquivo ausente");
        assert_eq!(AppError::Io(io_err).exit_code(), 14);
    }

    #[test]
    fn exit_code_internal_retorna_20() {
        let anyhow_err = anyhow::anyhow!("erro interno inesperado");
        assert_eq!(AppError::Internal(anyhow_err).exit_code(), 20);
    }

    #[test]
    fn exit_code_json_retorna_20() {
        let json_err = serde_json::from_str::<serde_json::Value>("json inválido {{").unwrap_err();
        assert_eq!(AppError::Json(json_err).exit_code(), 20);
    }

    #[test]
    fn exit_code_lock_busy_retorna_75() {
        assert_eq!(
            AppError::LockBusy("outra instância ativa".into()).exit_code(),
            75
        );
    }

    #[test]
    fn display_validation_inclui_mensagem() {
        let err = AppError::Validation("cpf inválido".into());
        assert!(err.to_string().contains("cpf inválido"));
        assert!(err.to_string().contains("validation error"));
    }

    #[test]
    fn display_duplicate_inclui_mensagem() {
        let err = AppError::Duplicate("proj/mem".into());
        assert!(err.to_string().contains("proj/mem"));
        assert!(err.to_string().contains("duplicate detected"));
    }

    #[test]
    fn display_not_found_inclui_mensagem() {
        let err = AppError::NotFound("id 42".into());
        assert!(err.to_string().contains("id 42"));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn display_embedding_inclui_mensagem() {
        let err = AppError::Embedding("dimensão errada".into());
        assert!(err.to_string().contains("dimensão errada"));
        assert!(err.to_string().contains("embedding error"));
    }

    #[test]
    fn display_lock_busy_inclui_mensagem() {
        let err = AppError::LockBusy("pid 1234".into());
        assert!(err.to_string().contains("pid 1234"));
        assert!(err.to_string().contains("lock busy"));
    }

    #[test]
    fn from_io_error_converte_corretamente() {
        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "sem permissão");
        let app_err: AppError = io_err.into();
        assert_eq!(app_err.exit_code(), 14);
        assert!(app_err.to_string().contains("IO error"));
    }

    #[test]
    fn from_anyhow_error_converte_corretamente() {
        let anyhow_err = anyhow::anyhow!("detalhe interno");
        let app_err: AppError = anyhow_err.into();
        assert_eq!(app_err.exit_code(), 20);
        assert!(app_err.to_string().contains("internal error"));
    }

    #[test]
    fn from_serde_json_error_converte_corretamente() {
        let json_err = serde_json::from_str::<serde_json::Value>("{campo_ruim}").unwrap_err();
        let app_err: AppError = json_err.into();
        assert_eq!(app_err.exit_code(), 20);
        assert!(app_err.to_string().contains("json error"));
    }

    #[test]
    fn exit_code_lock_busy_bate_com_constante() {
        assert_eq!(
            AppError::LockBusy("test".into()).exit_code(),
            crate::constants::CLI_LOCK_EXIT_CODE
        );
    }
}
