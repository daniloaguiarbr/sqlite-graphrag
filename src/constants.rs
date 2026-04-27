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

/// Hard upper bound on memory `body` length in bytes.
pub const MAX_MEMORY_BODY_LEN: usize = 512_000;

/// Body character count above which the body is split into chunks.
pub const MAX_BODY_CHARS_BEFORE_CHUNK: usize = 8_000;

/// Maximum attempts when a statement returns `SQLITE_BUSY`.
pub const MAX_SQLITE_BUSY_RETRIES: u32 = 5;

/// Base delay in milliseconds for the first SQLITE_BUSY retry.
///
/// Each subsequent attempt doubles the delay (exponential backoff):
/// 300 ms → 600 ms → 1200 ms → 2400 ms → 4800 ms (≈ 9.3 s total).
pub const SQLITE_BUSY_BASE_DELAY_MS: u64 = 300;

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

/// Resolve o cap de relacionamentos por memória, respeitando override por env var.
///
/// v1.0.22: torna o cap (default 50) configurável via `SQLITE_GRAPHRAG_MAX_RELATIONS_PER_MEMORY`.
/// Auditoria identificou que documentos ricos batiam o cap silenciosamente; usuários
/// com corpus técnico denso podem aumentar via env. Valores fora de [1, 10000] caem no default.
pub fn max_relationships_per_memory() -> usize {
    std::env::var("SQLITE_GRAPHRAG_MAX_RELATIONS_PER_MEMORY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| (1..=10_000).contains(&n))
        .unwrap_or(MAX_RELATIONSHIPS_PER_MEMORY)
}

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

/// Guard operacional explícito para documentos multi-chunk no `remember`.
///
/// O caminho multi-chunk usa embeddings seriais para evitar amplificação de memória no ONNX.
/// Este limite preserva um teto operacional claro para agentes e scripts.
pub const REMEMBER_MAX_SAFE_MULTI_CHUNKS: usize = 512;

/// Teto de chunks por micro-batch controlado no `remember`.
///
/// O runtime do `fastembed` usa padding `BatchLongest`, então batches muito grandes amplificam
/// o custo do maior chunk. Este teto mantém batches pequenos mesmo quando os chunks são curtos.
pub const REMEMBER_MAX_CONTROLLED_BATCH_CHUNKS: usize = 4;

/// Orçamento máximo de tokens preenchidos por micro-batch controlado no `remember`.
///
/// O orçamento usa `max_tokens_no_batch * tamanho_do_batch`, aproximando o custo real do
/// padding `BatchLongest`. Valores acima disso voltam para batches menores ou serialização.
pub const REMEMBER_MAX_CONTROLLED_BATCH_PADDED_TOKENS: usize = 512;

/// Timeout in milliseconds for a single ping probe against the daemon socket.
pub const DAEMON_PING_TIMEOUT_MS: u64 = 10;

/// Idle duration in seconds before the daemon shuts itself down.
pub const DAEMON_IDLE_SHUTDOWN_SECS: u64 = 600;

/// Tempo máximo de espera para o daemon ficar saudável após auto-start.
pub const DAEMON_AUTO_START_MAX_WAIT_MS: u64 = 5_000;

/// Intervalo inicial de polling para verificar se o daemon ficou saudável.
pub const DAEMON_AUTO_START_INITIAL_BACKOFF_MS: u64 = 50;

/// Teto do backoff entre tentativas automáticas de spawn do daemon.
pub const DAEMON_AUTO_START_MAX_BACKOFF_MS: u64 = 30_000;

/// Backoff base usado após falhas de spawn/health do daemon.
pub const DAEMON_SPAWN_BACKOFF_BASE_MS: u64 = 500;

/// Tempo máximo de espera para obter o lock de spawn do daemon.
pub const DAEMON_SPAWN_LOCK_WAIT_MS: u64 = 2_000;

/// Prefix prepended to bodies before embedding as required by E5 models.
pub const PASSAGE_PREFIX: &str = "passage: ";

/// Prefix prepended to queries before embedding as required by E5 models.
pub const QUERY_PREFIX: &str = "query: ";

/// Crate version string sourced from `CARGO_PKG_VERSION` at build time.
pub const SQLITE_GRAPHRAG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Batch size for BERT NER forward passes.
///
/// Larger values amortise fixed forward-pass overhead but increase peak RAM.
/// Memory guide (CPU only, max 512-token windows):
///   N=4  → ~54 MiB peak
///   N=8  → ~108 MiB peak  ← default
///   N=16 → ~216 MiB peak
///   N=32 → ~432 MiB peak  (not recommended without 16+ GiB RAM)
///
/// Override via `GRAPHRAG_NER_BATCH_SIZE` env var. Values outside [1, 32] are
/// clamped silently.
pub fn ner_batch_size() -> usize {
    std::env::var("GRAPHRAG_NER_BATCH_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(8)
        .clamp(1, 32)
}

/// PRD-canonical regex que valida nomes e namespaces. Permite 1 char `[a-z0-9]`
/// OU string de 2-80 chars começando com letra e terminando com letra/dígito,
/// contendo apenas `[a-z0-9-]`. Rejeita prefixo `__` (internal reserved).
pub const NAME_SLUG_REGEX: &str = r"^[a-z][a-z0-9-]{0,78}[a-z0-9]$|^[a-z0-9]$";

/// Retenção padrão (dias) usada por `purge` quando `--retention-days` é omitido.
pub const PURGE_RETENTION_DAYS_DEFAULT: u32 = 90;

/// Limite máximo de namespaces ativos (deleted_at IS NULL) simultâneos. Exit 5 ao exceder.
pub const MAX_NAMESPACES_ACTIVE: u32 = 100;

/// Máximo de tokens aceito por embedding input antes de chunking.
pub const EMBEDDING_MAX_TOKENS: usize = 512;

/// Limite máximo de resultados da CTE recursiva de grafo em `recall`.
pub const K_GRAPH_MATCHES_LIMIT: usize = 20;

/// Default `--limit` para `list` quando omitido.
pub const K_LIST_DEFAULT_LIMIT: usize = 100;

/// Default `--limit` para `graph entities` quando omitido.
pub const K_GRAPH_ENTITIES_DEFAULT_LIMIT: usize = 50;

/// Default `--limit` para `related` quando omitido.
pub const K_RELATED_DEFAULT_LIMIT: usize = 10;

/// Default `--limit` para `history` quando omitido.
pub const K_HISTORY_DEFAULT_LIMIT: usize = 20;

/// Peso padrão da contribuição vetorial na fórmula RRF de `hybrid-search`.
pub const WEIGHT_VEC_DEFAULT: f64 = 1.0;

/// Peso padrão da contribuição textual BM25 na fórmula RRF de `hybrid-search`.
pub const WEIGHT_FTS_DEFAULT: f64 = 1.0;

/// Tamanho em caracteres do preview do body emitido em formatos text/markdown.
pub const TEXT_BODY_PREVIEW_LEN: usize = 200;

/// Valor default injetado em ORT_NUM_THREADS quando não definido pelo usuário.
pub const ORT_NUM_THREADS_DEFAULT: &str = "1";

/// Valor default injetado em ORT_INTRA_OP_NUM_THREADS quando não definido.
pub const ORT_INTRA_OP_NUM_THREADS_DEFAULT: &str = "1";

/// Valor default injetado em OMP_NUM_THREADS quando não definido pelo usuário.
pub const OMP_NUM_THREADS_DEFAULT: &str = "1";

/// Exit code para falha parcial de batch (PRD linha 1822). Conflita com DbBusy em v1.x;
/// em v2.0.0 DbBusy migra para 15 e este código assume 13 conforme PRD.
pub const BATCH_PARTIAL_FAILURE_EXIT_CODE: i32 = 13;

/// Exit code para DbBusy em v2.0.0 (migrado de 13 para liberar 13 para batch failure).
pub const DB_BUSY_EXIT_CODE: i32 = 15;

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
/// para calcular o número dinâmico de permits.
///
/// Valor calibrado em 2026-04-23 com `/usr/bin/time -v` sobre `sqlite-graphrag v1.0.3`
/// nos comandos pesados `remember`, `recall` e `hybrid-search`, todos com pico de RSS
/// próximo de 1.03 GiB por processo. O valor abaixo arredonda para cima com margem defensiva.
pub const EMBEDDING_LOAD_EXPECTED_RSS_MB: u64 = 1_100;

/// Process exit code retornado quando memória disponível está abaixo de [`MIN_AVAILABLE_MEMORY_MB`].
///
/// Valor `77` é `EX_NOPERM` na glibc sysexits, reutilizado aqui para indicar
/// "recurso de sistema insuficiente para prosseguir".
pub const LOW_MEMORY_EXIT_CODE: i32 = 77;

/// Valor canônico de `PRAGMA user_version` gravado após migrações.
///
/// Deve permanecer em sincronia com o identificador legível-por-humanos
/// da versão do schema. Refinery usa sua própria tabela de histórico;
/// `user_version` é um campo auxiliar de diagnóstico para ferramentas
/// externas (ex: `sqlite3 db.sqlite "PRAGMA user_version"`).
pub const SCHEMA_USER_VERSION: i64 = 49;
