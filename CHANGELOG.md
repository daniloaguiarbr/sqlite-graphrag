# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
