# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.0] - 2026-04-18

### Breaking

- **Exit code `DbBusy` moved from 13 → 15** to free exit 13 for `BatchPartialFailure` per PRD. Shell scripts that detected `EX_UNAVAILABLE` (13) as DB busy must now check for 15.
- **`hybrid-search` response JSON shape reshaped** from `{query, combined_rank[], vec_rank[], fts_rank[]}` to `{query, k, results: [{memory_id, name, namespace, type, description, body, combined_score, vec_rank?, fts_rank?}], graph_matches: []}` per PRD lines 771-787. Consumers parsing `combined_rank` must migrate to `results`.
- **`purge --older-than-seconds` deprecated in favor of `--retention-days`**. The old flag remains as a hidden alias but emits a warning. Will be removed in v3.0.0.
- **`NAME_SLUG_REGEX` stricter than v1.x `SLUG_REGEX`**: multichar names must start with a letter (PRD requirement). Single-char `[a-z0-9]` still allowed. Existing memories with leading-digit names pass unchanged, but `rename` into legacy-style names will now fail.

### Added

- `AppError::BatchPartialFailure { total, failed }` mapping to exit 13 — reserved for `import`, `reindex` and batch stdin (entering in Tier 3/4).
- Constants in `src/constants.rs`: `PURGE_RETENTION_DAYS_DEFAULT=90`, `MAX_NAMESPACES_ACTIVE=100`, `EMBEDDING_MAX_TOKENS=512`, `K_GRAPH_MATCHES_LIMIT=20`, `K_LIST_DEFAULT_LIMIT=100`, `K_GRAPH_ENTITIES_DEFAULT_LIMIT=50`, `K_RELATED_DEFAULT_LIMIT=10`, `K_HISTORY_DEFAULT_LIMIT=20`, `WEIGHT_VEC_DEFAULT=1.0`, `WEIGHT_FTS_DEFAULT=1.0`, `TEXT_BODY_PREVIEW_LEN=200`, `ORT_NUM_THREADS_DEFAULT="1"`, `ORT_INTRA_OP_NUM_THREADS_DEFAULT="1"`, `OMP_NUM_THREADS_DEFAULT="1"`, `BATCH_PARTIAL_FAILURE_EXIT_CODE=13`, `DB_BUSY_EXIT_CODE=15`.
- Flag `--dry-run` and `--retention-days` in `purge`.
- Fields `namespace` and `merged_into_memory_id: Option<i64>` in `RememberResponse`.
- Field `k: usize` in `RecallResponse`.
- Fields `bytes_freed: i64`, `oldest_deleted_at: Option<i64>`, `retention_days_used: u32`, `dry_run: bool` in `PurgeResponse`.
- Flag `--format` in `hybrid-search` (JSON only; text/markdown reserved for Tier 2).
- Flag `--expected-updated-at` (optimistic locking) in `rename` and `restore`.
- Active namespace limit guard (`MAX_NAMESPACES_ACTIVE=100`) in `remember` — returns exit 5 when exceeded.

### Changed

- `SLUG_REGEX` renamed to `NAME_SLUG_REGEX` with PRD-conformant value `r"^[a-z][a-z0-9-]{0,78}[a-z0-9]$|^[a-z0-9]$"`. Multichar names must start with a letter.

### Fixed

- Prefix `__` explicitly rejected in `rename` (previously only enforced in `remember` via regex side-effect).
- Constants fantasma na fórmula RRF (`WEIGHT_VEC_DEFAULT`, `WEIGHT_FTS_DEFAULT`) agora declaradas em `constants.rs` — referências do PRD agora mapeiam símbolos reais.


## [1.2.1] - 2026-04-18

### Fixed

- Installation failure on `rustc` versions in the range `1.88..1.95` caused by transitive dependency `constant_time_eq 0.4.3` (pulled via `blake3`) bumping its MSRV to 1.95.0 in a patch release
- `cargo install neurographrag` without `--locked` now succeeds because the direct pin `constant_time_eq = "=0.4.2"` forces a resolved version compatible with our declared `rust-version = "1.88"`

### Changed

- `Cargo.toml` now declares an explicit preventive pin `constant_time_eq = "=0.4.2"` with an inline comment documenting the MSRV drift reason; the pin will be revisited when we raise `rust-version` to 1.95
- `README.md` (EN and PT) install instructions updated from `cargo install neurographrag` to `cargo install --locked neurographrag`, including a bullet explaining the rationale

### Added

- `docs_rules/prd.md` section "Dependency MSRV Drift Protection" documenting the canonical mitigation pattern — direct pinning of problematic transitive dependencies in the top-level `Cargo.toml`


## [1.2.0] - 2026-04-18

### Added

- Counting semaphore cross-process com até 4 slots simultâneos via `src/lock.rs` (`acquire_cli_slot`)
- Memory guard abortando com exit 77 quando RAM livre está abaixo de 2 GB via `sysinfo` (`src/memory_guard.rs`)
- Signal handler graceful para SIGINT, SIGTERM e SIGHUP via `ctrlc` com feature `termination`
- Flag `--max-concurrency <N>` para controlar limite de invocações paralelas em runtime
- Flag oculta `--skip-memory-guard` para testes automatizados onde a alocação real não ocorre
- Constantes `MAX_CONCURRENT_CLI_INSTANCES`, `MIN_AVAILABLE_MEMORY_MB`, `CLI_LOCK_DEFAULT_WAIT_SECS`, `EMBEDDING_LOAD_EXPECTED_RSS_MB` e `LOW_MEMORY_EXIT_CODE` em `src/constants.rs`
- Variantes `AppError::AllSlotsFull` e `AppError::LowMemory` com mensagens em português brasileiro
- Global `SHUTDOWN: AtomicBool` e função `shutdown_requested()` em `src/lib.rs`

### Changed

- Flag `--wait-lock` default aumentado para 300 segundos (5 minutos) via `CLI_LOCK_DEFAULT_WAIT_SECS`
- Lock file migrado de `cli.lock` único para `cli-slot-{N}.lock` (counting semaphore N=1..4)

### Removed

- BREAKING — flag `--allow-parallel` removida — causou OOM crítico em produção (incidente 2026-04-18)

### Fixed

- Bug crítico onde múltiplas invocações CLI simultâneas esgotavam a RAM do sistema após 58 invocações paralelas travarem o computador por 38 minutos (incidente 2026-04-18)


## [Unreleased]

### Added

- Global flags `--allow-parallel` and `--wait-lock SECONDS` for controlled concurrency
- Module `src/lock.rs` implementing file-based single-instance lock via `fs4`
- New `AppError::LockBusy` variant mapping to exit code 75 (`EX_TEMPFAIL`)
- Environment variables `ORT_NUM_THREADS`, `OMP_NUM_THREADS` and `ORT_INTRA_OP_NUM_THREADS` pre-set to 1 when not already defined by the user
- Singleton `OnceLock<Mutex<TextEmbedding>>` for intra-process model reuse
- Integration tests under `tests/lock_integration.rs` covering lock acquisition and release

### Changed

- Default behavior is now single-instance — a second concurrent invocation exits with code 75 unless `--allow-parallel` is passed
- Embedder module refactored from struct-with-state to free functions operating on a singleton

### Fixed

- Prevents OOM livelock when the CLI is invoked in massively parallel fashion by LLM orchestrators (incident 2026-04-18)


## [0.1.0] - 2026-04-17

### Added

- Phase 1 — Foundation: SQLite schema with vec0 (sqlite-vec), FTS5, entity graph
- Phase 2 — Essential subcommands: init, remember, recall, read, list, forget, rename, edit, history, restore, health, stats, optimize, purge, vacuum, migrate, hybrid-search, namespace-detect, sync-safe-copy

### Fixed

- FTS5 external-content corruption bug in forget+purge cycle (removed manual DELETE in forget.rs)

### Changed

- Raised MSRV from 1.80 to 1.88 (required by transitive dependencies base64ct 1.8.3, ort-sys, time)

[Unreleased]: https://github.com/daniloaguiarbr/neurographrag/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/daniloaguiarbr/neurographrag/releases/tag/v0.1.0
