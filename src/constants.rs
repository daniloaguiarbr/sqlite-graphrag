//! Compile-time constants shared across the crate.
//!
//! Grouped into embedding configuration, length and size limits, SQLite
//! pragmas and retrieval tuning knobs. Values are taken from the PRD and
//! must stay in sync with the migrations under `migrations/`.
//!
//! ## Cálculo dinâmico de permits de concorrência
//!
//! O número máximo de instâncias simultâneas pode ser ajustado em runtime
//! usando a fórmula:
//!
//! ```text
//! permits = min(cpus, available_memory_mb / EMBEDDING_LOAD_EXPECTED_RSS_MB) * 0.5
//! ```
//!
//! onde `available_memory_mb` é obtido via `sysinfo::System::available_memory()`
//! convertido para MiB. O resultado é limitado superiormente por
//! `MAX_CONCURRENT_CLI_INSTANCES` e inferiorizado em 1.

/// Embedding vector dimensionality produced by `multilingual-e5-small`.
pub const EMBEDDING_DIM: usize = 384;

/// Default `fastembed` model identifier used by `remember` and `recall`.
pub const FASTEMBED_MODEL_DEFAULT: &str = "multilingual-e5-small";

/// Batch size for `fastembed` encoding calls.
pub const FASTEMBED_BATCH_SIZE: usize = 32;

/// Maximum byte length for a memory `name` field in kebab-case.
pub const MAX_MEMORY_NAME_LEN: usize = 80;

/// Maximum character length for a memory `description` field.
pub const MAX_MEMORY_DESCRIPTION_LEN: usize = 500;

/// Hard upper bound on memory `body` length in characters.
pub const MAX_MEMORY_BODY_LEN: usize = 20_000;

/// Body character count above which the body is split into chunks.
pub const MAX_BODY_CHARS_BEFORE_CHUNK: usize = 8_000;

/// Maximum attempts when a statement returns `SQLITE_BUSY`.
pub const MAX_SQLITE_BUSY_RETRIES: u32 = 5;

/// Query timeout applied to statements in milliseconds.
pub const QUERY_TIMEOUT_MILLIS: u64 = 5_000;

/// Jaccard threshold above which two memories are considered fuzzy duplicates.
pub const DEDUP_FUZZY_THRESHOLD: f64 = 0.8;

/// Cosine distance threshold below which two memories are semantic duplicates.
pub const DEDUP_SEMANTIC_THRESHOLD: f32 = 0.1;

/// Maximum number of hops allowed in graph traversals.
pub const MAX_GRAPH_HOPS: u32 = 2;

/// Minimum relationship weight required for traversal inclusion.
pub const MIN_RELATION_WEIGHT: f64 = 0.3;

/// Default traversal depth for `related` when `--hops` is omitted.
pub const DEFAULT_MAX_HOPS: u32 = 2;

/// Default minimum weight filter applied during graph traversal.
pub const DEFAULT_MIN_WEIGHT: f64 = 0.3;

/// Default weight assigned to newly created relationships.
pub const DEFAULT_RELATION_WEIGHT: f64 = 0.5;

/// Default `k` used by `recall` when the caller omits `--k`.
pub const DEFAULT_K_RECALL: usize = 10;

/// Default `k` for memory KNN searches when the caller omits `--k`.
pub const K_MEMORIES_DEFAULT: usize = 10;

/// Default `k` for entity KNN searches during graph expansion.
pub const K_ENTITIES_SEARCH: usize = 5;

/// Upper bound on distinct entities persisted per memory.
pub const MAX_ENTITIES_PER_MEMORY: usize = 30;

/// Upper bound on distinct relationships persisted per memory.
pub const MAX_RELATIONSHIPS_PER_MEMORY: usize = 50;

/// Character length of the description preview shown in `list` output.
pub const TEXT_DESCRIPTION_PREVIEW_LEN: usize = 100;

/// `PRAGMA busy_timeout` value applied on every connection.
pub const BUSY_TIMEOUT_MILLIS: i32 = 5_000;

/// `PRAGMA cache_size` value in kibibytes (negative means KiB).
pub const CACHE_SIZE_KB: i32 = -64_000;

/// `PRAGMA mmap_size` value in bytes applied to each connection.
pub const MMAP_SIZE_BYTES: i64 = 268_435_456;

/// `PRAGMA wal_autocheckpoint` threshold in pages.
pub const WAL_AUTOCHECKPOINT_PAGES: i32 = 1_000;

/// Default `k` constant used by Reciprocal Rank Fusion in `hybrid-search`.
pub const RRF_K_DEFAULT: u32 = 60;

/// Chunk size expressed in tokens for body splitting.
pub const CHUNK_SIZE_TOKENS: usize = 400;

/// Token overlap between consecutive chunks.
pub const CHUNK_OVERLAP_TOKENS: usize = 50;

/// Timeout in milliseconds for a single ping probe against the daemon socket.
pub const DAEMON_PING_TIMEOUT_MS: u64 = 10;

/// Idle duration in seconds before the daemon shuts itself down.
pub const DAEMON_IDLE_SHUTDOWN_SECS: u64 = 600;

/// Prefix prepended to bodies before embedding as required by E5 models.
pub const PASSAGE_PREFIX: &str = "passage: ";

/// Prefix prepended to queries before embedding as required by E5 models.
pub const QUERY_PREFIX: &str = "query: ";

/// Crate version string sourced from `CARGO_PKG_VERSION` at build time.
pub const NEUROGRAPHRAG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Regular expression that validates kebab-case identifiers.
pub const SLUG_REGEX: &str = r"^[a-z0-9]+(?:-[a-z0-9]+)*$";

/// Filename used for the advisory exclusive lock that prevents parallel invocations.
pub const CLI_LOCK_FILE: &str = "cli.lock";

/// Polling interval em milliseconds usado por `--wait-lock` entre tentativas de `try_lock_exclusive`.
pub const CLI_LOCK_POLL_INTERVAL_MS: u64 = 500;

/// Process exit code returned when the lock is busy and no wait was requested (EX_TEMPFAIL).
pub const CLI_LOCK_EXIT_CODE: i32 = 75;

/// Número máximo de instâncias CLI em execução simultânea.
///
/// Alinhado com `DAEMON_MAX_CONCURRENT_CLIENTS` do PRD. Limita o semáforo de
/// contagem em [`crate::lock`] para evitar sobrecarga de memória quando múltiplas
/// invocações paralelas tentam carregar o modelo ONNX simultaneamente.
pub const MAX_CONCURRENT_CLI_INSTANCES: usize = 4;

/// Memória disponível mínima em MiB exigida antes de iniciar o carregamento do modelo.
///
/// Se `sysinfo::System::available_memory() / 1_048_576` estiver abaixo deste
/// valor, a invocação é abortada com [`crate::errors::AppError::LowMemory`]
/// (exit code [`LOW_MEMORY_EXIT_CODE`]).
pub const MIN_AVAILABLE_MEMORY_MB: u64 = 2_048;

/// Tempo máximo em segundos que uma instância aguarda para adquirir um slot de concorrência.
///
/// Passado como default de `--max-wait-secs` na CLI. Após esgotar este limite,
/// a invocação retorna [`crate::errors::AppError::AllSlotsFull`] com exit code
/// [`CLI_LOCK_EXIT_CODE`] (75).
pub const CLI_LOCK_DEFAULT_WAIT_SECS: u64 = 300;

/// RSS esperado em MiB de uma única instância com o modelo ONNX carregado via fastembed.
///
/// Usado na fórmula `min(cpus, available_memory_mb / EMBEDDING_LOAD_EXPECTED_RSS_MB) * 0.5`
/// para calcular o número dinâmico de permits. Valor calibrado para
/// `multilingual-e5-small` com runtime ONNX.
pub const EMBEDDING_LOAD_EXPECTED_RSS_MB: u64 = 750;

/// Process exit code retornado quando memória disponível está abaixo de [`MIN_AVAILABLE_MEMORY_MB`].
///
/// Valor `77` é `EX_NOPERM` na glibc sysexits, reutilizado aqui para indicar
/// "recurso de sistema insuficiente para prosseguir".
pub const LOW_MEMORY_EXIT_CODE: i32 = 77;
