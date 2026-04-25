# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.11] - 2026-04-25

### Fixed
- Installed-binary smoke coverage now includes the public fallback contract for `./graphrag.sqlite` in the invocation directory, closing a release-audit blind spot
- Contract tests now require the current wrapper shapes for `list` (`items`) and `related` (`results`) instead of silently accepting legacy root arrays
- `graph traverse` and `graph stats` now expose only the formats they actually support, preventing misleading help output and invalid documented invocations
- Less-central subcommand help text is now consistently English-first across the audited public CLI surface
- `COOKBOOK`, `AGENTS`, `INTEGRATIONS`, schema guidance, and graph/health examples are now aligned with the real payloads and valid command forms shipped by the binary

## [1.0.10] - 2026-04-24

### Changed
- CLI `--help` is now consistently English-first for static clap output, while `--lang` remains the control for human-facing runtime messages
- Release documentation now makes upgrade and active-version verification explicit with `cargo install ... --force` and `sqlite-graphrag --version`
- Testing documentation now distinguishes default nextest coverage from the release-critical `slow-tests` contract suites

### Added
- New CI job `slow-contracts` runs `doc_contract_integration` and `prd_compliance` with `--features slow-tests`
- `installed_binary_smoke` now enforces installed-binary version parity with the current workspace by default, with an explicit escape hatch for deliberate legacy audits

## [1.0.9] - 2026-04-24

### Fixed
- `--skip-memory-guard` now disables daemon auto-start by default so test and audit subprocesses do not leak resident embedding daemons unless they explicitly opt back in
- The daemon now shuts itself down when its control directory disappears, preventing tempdir-based test runs from leaving orphan processes behind
- `installed_binary_smoke` now disables daemon auto-start explicitly for the installed binary path
- `audit-remember-safely.sh` now isolates `SQLITE_GRAPHRAG_CACHE_DIR` and stops the daemon on exit, avoiding resident process leaks after audits

### Added
- New daemon regression test proving `--skip-memory-guard` does not auto-start the daemon unless forced
- New daemon regression test proving the daemon exits when the temp cache/control directory disappears

## [1.0.8] - 2026-04-24

### Added
- Automatic daemon auto-start on the first heavy embedding command when the daemon socket is unavailable
- Spawn serialization via a dedicated daemon spawn lock file to prevent process storms
- Persistent daemon spawn backoff state to suppress repeated failed spawn attempts
- New daemon tests covering auto-start and automatic restart after shutdown

### Changed
- Heavy commands now try the daemon, auto-start it on demand, and fall back locally only when backoff or spawn failure requires it
- `sqlite-graphrag daemon` remains available for explicit foreground management, but the common path no longer requires manual startup

### Fixed
- The last major daemon gap from `v1.0.7` is closed: the daemon is no longer purely opt-in

## [1.0.7] - 2026-04-24

### Fixed
- Integration docs no longer claim the project runs "without daemons" now that `sqlite-graphrag daemon` exists
- Agent-facing docs now describe heavy-command reuse of the persistent daemon instead of a purely stateless-only model
- HOW_TO_USE now documents `sqlite-graphrag daemon`, `--ping`, `--stop`, and the automatic fallback path in heavy commands
- TESTING now documents the daemon integration test suite and basic daemon recovery workflow

## [1.0.6] - 2026-04-24

### Added
- New `daemon` subcommand to keep the embedding model loaded in a persistent IPC process
- New local-socket JSON protocol for `ping`, `shutdown`, `embed_passage`, `embed_query`, and controlled batch passage embeddings
- New daemon integration test suite proving `init`, `remember`, `recall`, and `hybrid-search` increment the daemon embed counter when the daemon is available
- New `scripts/audit-remember-safely.sh` helper to audit installed or local binaries under cgroup memory limits

### Changed
- `init`, `remember`, `recall`, and `hybrid-search` now try the persistent daemon first and fall back to the current local path when the daemon is unavailable
- `remember` now uses the real `multilingual-e5-small` tokenizer before embedding, replacing the old char-based chunk approximation on the hot path
- Multi-chunk embedding in `remember` now uses controlled micro-batching based on padded-token budget instead of all-or-nothing serial chunk embedding
- `remember --type` help now makes explicit that it targets `memories.type`, not graph `entity_type`

### Fixed
- The safe remember audit script now uses a unique temporary work directory per run and validates the database with `health` after `init`
- Token-heavy but byte-dense synthetic inputs below the byte guard no longer over-fragment into artificial 7-chunk failures in the local improved build

## [1.0.5] - 2026-04-24

### Fixed
- `chunking::Chunk` no longer stores owned chunk bodies, so multi-chunk `remember` avoids duplicating the full body across every chunk in memory
- Chunk persistence now inserts text slices directly from the stored body instead of allocating another owned chunk collection
- Public docs now correctly describe `1.0.4` as the current published release and `1.0.5` as the next local line
- `remember` now emits stage-by-stage memory instrumentation and rejects documents that exceed the current explicit safe multi-chunk limit before ONNX work begins
- The explicit safe multi-chunk limit was tightened from 8 to 6 after a cgroup-isolated audit showed OOM persisting on moderate 7-chunk inputs under `MemoryMax=4G`
- `remember` now also rejects dense multi-chunk bodies above `4500` bytes before ONNX work starts, based on the observed OOM threshold window from the safe cgroup audit
- The embedder now forces `max_length = 512` explicitly and disables the CPU execution provider arena allocator to reduce retained memory across repeated variable-shape inference calls

### Root Cause
- The previous design still duplicated the body through `Vec<Chunk>` values carrying owned `String` payloads for each chunk
- That duplication amplified allocator pressure exactly in the multi-chunk path already stressed by ONNX inference
- The absence of an explicit operational guard also allowed moderate Markdown inputs to reach the heavy multi-chunk embedding path without an early safety stop
- Follow-up safe auditing showed that even some 7-chunk documents remained unsafe under a `4G` cgroup, justifying a stricter temporary ceiling
- Follow-up safe auditing also showed that some dense documents in the `4540` to `4792` byte range still triggered OOM below the chunk ceiling, justifying an additional temporary size guard
- Official ONNX Runtime guidance confirms that `enable_cpu_mem_arena = true` is the default, that disabling it reduces memory consumption, and that the trade-off is potentially higher latency
- The `ort` API also documents disabling `memory_pattern` when input size varies, which matches the `remember` path with repeated chunk inference and variable effective shapes
- Inspection of `fastembed 5.13.2` showed that the CPU path does not disable the ONNX Runtime CPU memory arena by default and only disables `memory_pattern` automatically in the DirectML path
- Inspection of the `multilingual-e5-small` tokenizer metadata confirmed that the real model ceiling is `512`, so explicitly forcing `max_length = 512` matches the model instead of relying on a generic library default
- The retained CPU arena is therefore treated as a strongly supported and technically coherent cause, but not yet as the single fully proven cause in every pathological case

## [1.0.4] - 2026-04-23

### Fixed
- `remember` now embeds chunked bodies serially and reuses the same per-chunk embeddings for aggregation and vec-chunk persistence, avoiding the hanging batch path seen on real Markdown documents
- `remember` now avoids an extra `Vec<String>` clone for chunk texts and avoids building an intermediate `Vec<storage::chunks::Chunk>` copy before chunk persistence
- `remember` now computes cheap duplicate checks before any embedding work and no longer clones the full body into `NewMemory` unnecessarily
- `namespace-detect` now accepts `--db` as a no-op so the public command contract matches the rest of the CLI surface
- Public docs and release workflow text now reflect the published `1.0.3` line and the explicit graph contract more accurately
- Chunking now uses a more conservative chars-per-token heuristic and guarantees UTF-8-safe forward progress, reducing the risk of pathological chunk sizes on real Markdown inputs

### Root Cause
- Real-world Markdown with paragraph-heavy structure could drive non-monotonic chunk progression under the old overlap logic
- The old `remember` path also duplicated memory pressure by cloning chunk texts into a dedicated `Vec<String>` and by rebuilding chunk payload structs with owned `String` copies before persistence
- The old `remember` path also spent ONNX work before resolving cheap duplicate conditions and cloned the full body into `NewMemory` before insert or update
- The combination increased allocator pressure and made the heavy embedding path more vulnerable to pathological memory growth on problematic inputs

## [1.0.3] - 2026-04-23

### Fixed
- Heavy commands now calculate safe concurrency dynamically from available memory, CPU count, and per-task embedding RSS budget before acquiring CLI slots
- `init`, `remember`, `recall`, and `hybrid-search` now emit defensive progress logs showing detected heavy workload and computed safe concurrency
- The runtime now clamps `--max-concurrency` down to the safe memory budget for embedding-heavy commands instead of allowing the documented heuristic to remain unenforced
- The embedding RSS budget used by the concurrency heuristic is now calibrated from measured peak RSS instead of an older historical estimate

### Added
- Unit coverage for heavy-command classification and safe concurrency calculation

## [1.0.2] - 2026-04-23

### Added
- Formal input schemas for `remember --entities-file` and `remember --relationships-file`
- Stable graph input contract in `AGENT_PROTOCOL`, `AGENTS`, `HOW_TO_USE`, and `llms-full.txt`
- Short graph input contract summary in `llms.txt` and `llms.pt-BR.txt`

### Fixed
- `AGENTS` headings now describe `--json` as universal and `--format json` as command-specific
- `HOW_TO_USE` output matrix now reflects the real default output for `link`, `unlink`, and `cleanup-orphans`
- Public docs no longer present the project as pre-publication

## [1.0.1] - 2026-04-23

### Fixed
- Restrict `--format` to `json` on commands that do not implement `text` or `markdown`, preventing help and parse contracts from promising unsupported output modes
- `hybrid-search` no longer accepts `text` or `markdown` only to fail later at runtime; unsupported formats are now rejected by `clap` during argument parsing
- Docs and agent-facing guides now explain that `--json` is the broad compatibility flag while `--format json` is command-specific

### Added
- `remember` payload docs now explain that `--relationships-file` requires `strength` in `[0.0, 1.0]` and that the field maps to `weight` in graph outputs
- `remember` payload docs now explain that `type` is accepted as an alias of `entity_type`, but both fields together are invalid

## [1.0.0] - 2026-04-19

- First public release under the `sqlite-graphrag` name
- Feature set is derived from legacy `neurographrag v2.3.0`

### Fixed
- graph entities SQL query now uses correct column name (NG-V220-01 CRITICAL)
- stats and health now accept --format json flag (NG-V220-02 HIGH)
- remember --type obligation documented in all examples (NV-005 HIGH)
- rename docs corrected to --name/--new-name (NV-002)
- recall docs clarify positional QUERY argument (NV-004)
- forget docs remove non-existent --yes flag (NV-001)
- list docs reference correct items field (NV-006)
- related docs reference correct results field (NV-010)
- MIGRATION.md now documents the rename transition and the `v1.0.0` release plan

### Added
- unlink --relation required flag documented (NV-003)
- graph traverse --from expects entity name documented (NV-007)
- entity_type restricted value list documented (NV-009)
- sync-safe-copy --format flag added for output control (NG-V220-04)

### Changed
- __debug_schema clarifies user_version versus schema_version semantics (NG-V220-03)
- i18n global flags documented as PT-only (GAP-I18N-02 LOW)

## [2.2.0] - 2026-04-19

### Fixed
- G-017: `sync-safe-copy --to` flag alias restored; `--destination` remains canonical (regression from v2.0.3)
- G-027: `PRAGMA user_version` now set to 49 after refinery migrations to match `refinery_schema_history` row count
- NG-08: `health` subcommand now runs `PRAGMA integrity_check` before memory/entity counts for defense-in-depth; output gains `journal_mode`, `wal_size_mb`, and `checks[]` fields

### Added
- NG-04: `graph entities` subcommand lists graph nodes with optional `--type` filter and `--json` output
- NG-06: `--format` flag added to `graph stats` for parity with `graph traverse`
- NG-05: `__debug_schema` hidden diagnostic subcommand documented; emits `schema_version`, `user_version`, `objects`, and `migrations` fields
- NG-03: Every subcommand now accepts both `--json` (short) and `--format json` (explicit) producing identical output

### Changed
- NG-07: `link` and `unlink` clarified to operate on typed graph entities only; valid entity types documented in `--help`

## [2.1.0] - 2026-04-19

### Fixed
- G-001: `rename` now emits `action: "renamed"` in JSON output (`src/commands/rename.rs`)
- G-002: `hybrid-search` ranks now 1-based matching schema constraint `minimum: 1`
- G-003: `--expected-updated-at` now enforces optimistic lock via WHERE clause + `changes()` check (exit 3 on conflict)
- G-005: i18n prefix `Error:` now translated to `Erro:` in PT via `i18n::prefixo_erro()` in `main.rs`
- G-007: `health` returns exit 10 when `integrity_ok: false` via `AppError::Database` (emits JSON before returning Err)
- G-013: `restore` now finds soft-deleted memories (WHERE includes `deleted_at IS NOT NULL`)
- G-018: `emit_progress()` now uses `tracing::info!` respecting `LOG_FORMAT=json`
- Fixed COOKBOOK recipes 8 and 14 to use `jaq '.items[]'` matching `list --json` output structure
- Fixed HOW_TO_USE pt-BR inverted score semantics (`score` high = more relevant, not distance low)

### Added
- G-004: Documentation of `--entities-file entity_type` valid values (`project|tool|person|file|concept|incident|decision|memory|dashboard|issue_tracker`)
- G-006: `docs/MIGRATION.md` + `docs/MIGRATION.pt-BR.md` for v1.x to v2.x upgrade guidance
- G-016: `graph traverse` subcommand (flags `--from`/`--depth`) with new schema `docs/schemas/graph-traverse.schema.json`
- G-016: `graph stats` subcommand with new schema `docs/schemas/graph-stats.schema.json`
- G-019/G-020: Global `--tz` flag + `tz::init()` in `main.rs` populating `FUSO_GLOBAL` for timezone-aware timestamps
- G-024: `namespace-detect --db` flag for multi-DB override
- G-025: `vacuum --checkpoint` + `--format` flags
- G-026: `migrate --status` subcommand with `applied_migrations` response
- G-027: `PRAGMA user_version = 49` set after refinery migrations complete
- 6 new H3 sections in HOW_TO_USE.pt-BR.md (Language Flag Aliases, JSON Output Flag, DB Path Discovery, Concurrency Cap, Note on forget, Note on optimize and migrate)
- New COOKBOOK pt-BR recipe: "Como Exibir Timestamps no Fuso Horário Local"

### Changed
- `migrate.schema.json` now uses `oneOf` covering run vs `--status` modes with `$defs.MigrationEntry`
- `--json` accepted as no-op in `remember`/`read`/`history`/`forget`/`purge` for consistency
- `docs/schemas/README.md` documents `__debug_schema` binary name vs kebab-case schema file convention

### Deprecated
- `--allow-parallel` removed in v1.2.0 — see `docs/MIGRATION.md` for upgrade path


## [2.0.5] — 2026-04-19

### Fixed
- Exit code 13 documentado como `BatchPartialFailure` e exit code 15 como `DbBusy` em AGENTS.md — separação correta conforme `src/errors.rs` desde v2.0.0
- Exit code 73 substituído por 75 (`LockBusy/AllSlotsFull`) em todas as referências de documentação
- `PURGE_RETENTION_DAYS` corrigido de 30 para 90 em AGENTS.md e HOW_TO_USE.md EN+pt-BR — alinhado à constante `PURGE_RETENTION_DAYS_DEFAULT = 90` em `src/constants.rs`

### Added
- `elapsed_ms: u64` padronizado em todos os comandos que ainda não expunham o campo — uniformidade de contrato JSON
- `schema_version: u32` adicionado ao JSON stdout de `health` — facilita detecção de migração por agentes
- Subcomando oculto `__debug_schema` que imprime schema SQLite + versão de migrations para diagnóstico
- Diretório `docs/schemas/` com JSON Schema Draft 2020-12 público de cada resposta
- 12 suites de testes cobrindo: contrato JSON, exit codes P0, migração de schema, concorrência, property-based, sinais, i18n, segurança, benchmarks, smoke de instalado, receitas do cookbook e regressão v2.0.4
- 4 benchmarks criterion em `benches/cli_benchmarks.rs` validando SLAs de latência
- `proptest = { version = "1", features = ["std"] }` e `criterion = { version = "0.5", features = ["html_reports"] }` em `[dev-dependencies]`
- `[[bench]]` com `name = "cli_benchmarks"` e `harness = false` em `Cargo.toml`


## [2.0.4] — 2026-04-19

### Fixed
- `--expected-updated-at` now accepts both Unix epoch integer and RFC 3339 string via dual parser in src/parsers/mod.rs — applied to edit, rename, restore, remember subcommands (GAP 1 CRITICAL)
- `entities-file` JSON now accepts field `"type"` as alias of `"entity_type"` via `#[serde(alias = "type")]` — removes 422 on valid agent payloads (GAP 12 HIGH)
- Validation inner messages now localized EN/PT via `i18n::validacao` module — 7 functions covering name-length, reserved-name, kebab-case, description-length, body-length (GAP 13 MEDIUM)
- `purge --yes` flag silently accepted as no-op for compatibility with documented examples (GAP 19 MEDIUM)
- `link` JSON response now duplicates `from` as `source` and `to` as `target` — zero breaking change, adds expected aliases (GAP 20 MEDIUM)
- `graph` node objects now duplicate `kind` as `type` via `#[serde(rename = "type")]` in graph_export.rs — zero breaking change (GAP 21 LOW)
- `history` version records now include `created_at_iso` RFC 3339 field parallel to existing `created_at` Unix timestamp (GAP 24 LOW)

### Added
- `health` JSON schema expanded to full PRD spec: +db_size_bytes, +integrity_ok, +schema_ok, +vec_memories_ok, +vec_entities_ok, +vec_chunks_ok, +fts_ok, +model_ok, +checks[] array with 7 entries (GAP 4 HIGH)
- `recall` JSON response now includes `elapsed_ms: u64` measured via Instant (GAP 8 HIGH)
- `hybrid-search` JSON response now includes `elapsed_ms: u64`, `rrf_k: u32`, and `weights: {vec, fts}` fields (GAPs 8+10 HIGH)
- i18n validation module `src/i18n/validacao.rs` — all 7 validation error messages available in EN and PT
- Dual timestamp parser `src/parsers/mod.rs` — accepts Unix epoch i64 and RFC 3339 via `chrono::DateTime::parse_from_rfc3339`

### Changed
- Docs sweep EN (T9): schemas for recall, hybrid-search, list, health, stats aligned to binary output; weights corrected 0.6/0.4 → 1.0/1.0; namespace default documented as `global`; `--json` no-op alias documented; `related` documented to take memory name not ID
- Docs sweep PT (T10): COOKBOOK.pt-BR.md, CROSS_PLATFORM.pt-BR.md, AGENTS.pt-BR.md, README.pt-BR.md, skill/sqlite-graphrag-pt/SKILL.md, llms.pt-BR.txt aligned to mirror T9 EN corrections
- 18 binary source files updated; 1 new file added (src/parsers/mod.rs)
- 283 tests PASS, zero clippy warnings, zero check errors after binary changes


## [2.0.3] - 2026-04-19

### Added
- `purge --days` accepted as alias of `--retention-days` for backwards compat with docs (GAP 3)
- `recall --json` and `hybrid-search --json` accepted as no-op (GAP 6) — JSON output is already default
- `health` JSON now includes `wal_size_mb` and `journal_mode` (GAP 7)
- `stats` JSON now includes `edges` (alias of `relationships`) and `avg_body_len` (GAP 8)
- `AppError` variants now localized via `Idioma` enum / `Mensagem` exhaustive match (GAP 13) — `--lang en/pt` applies to error messages too
- 8 new sections in HOW_TO_USE.md for subcommands previously zero-doc (GAP 12): cleanup-orphans, edit, graph, history, namespace-detect, rename, restore, unlink
- Bilingual HOW_TO_USE.pt-BR.md mirror
- Latency disclaimer in COOKBOOK noting CLI ~1s per invocation vs daemon plans (GAP P1)

### Changed
- All docs: `--type agent` replaced with `--type project` everywhere (GAP 1) — PRD defines 7 valid types (user/feedback/project/reference/decision/incident/skill); `agent` was never valid
- All docs: `purge --days` rewritten as `purge --retention-days` (GAP 3)
- All docs: examples of `remember` now include `--description "..."` (GAP 2)
- README, CLAUDE, AGENT_PROTOCOL: agent count standardized to 27 (GAP 14)
- AGENTS.md schemas: JSON root for `recall` documented as `direct_matches[]/graph_matches[]/results[]` (reality per PRD), `hybrid-search` as `results[]` with `vec_rank/fts_rank` (GAPs 4, 5)
- COOKBOOK defaults corrected: recall --k 10, list --limit 50, hybrid-search weights 1.0/1.0, purge --retention-days 90 (GAPs 28-31)
- Docs note on `distance` (cosine, lower=better) vs `score` (1-distance, higher=better) in JSON vs text/markdown (GAP 17)
- Docs note on default namespace `global` (not `default`) (GAP 16)

### Fixed
- Binary no longer returns exit 2 for `purge --days 30` (GAP 3)
- Binary no longer returns exit 2 for `recall --json "q"` (GAP 6)
- Documentation of `link` now explicitly states entity-prerequisite (GAP 9)
- Documentation of `--force-merge` flag (GAP 18)
- Documentation of `graph --format dot|mermaid` (GAP 22)
- Documentation of `--db <PATH>` flag (GAP 25)
- Documentation of `--max-concurrency` cap at 2×nCPUs (GAP 27)

### Docs
- `27 AI agents` standardized as the official integrated agent count everywhere
- Evidence: test plan from 2026-04-19 catalogued 31 gaps in `/tmp/sqlite-graphrag-testplan-v2.0.2/gaps.md`; v2.0.3 closes all 31
- GAP 11 `elapsed_ms` universal in JSON deferred to v2.1.0 (requires processing_time capture across all commands)
- GAP P1 latency < 50ms requires daemon mode planned for v3.0.0


## [2.0.2] - 2026-04-19

### Fixed

- Flag `--lang` now accepts `en`/`pt` short codes as documented.
- Previously required full identifiers `english`/`portugues`; now aliases added: `en/english/EN`, `pt/portugues/portuguese/pt-BR/pt-br/PT`.


## [2.0.1] - 2026-04-19

### Added

- Flag aliases for backward compatibility with bilingual documentation contracts.
- `rename --old/--new` added as aliases of `--name/--new-name`.
- `link/unlink --source/--target` added as aliases of `--from/--to`.
- `related --hops` added as alias of `--max-hops`.
- `sync-safe-copy --output` added as alias of `--dest`.
- `related` now also accepts the memory name as a positional argument.
- `--json` accepted as no-op on `health`, `stats`, `migrate`, `namespace-detect`.
- Global `--lang en|pt` flag with `SQLITE_GRAPHRAG_LANG` env var fallback.
- `LC_ALL`/`LANG` locale fallback used for stderr progress messages.
- New module `i18n` exposing `Language` enum and `init`/`current`/`tr` helpers.
- Bilingual progress helpers added in `output::emit_progress_i18n`.
- ISO 8601 timestamps: `created_at_iso` added to `RememberResponse`.
- `updated_at_iso` added to `list` items.
- `created_at_iso`/`updated_at_iso` added to `read`, parallel to existing epoch integers.
- `read` response now includes `memory_id` (alias of `id`).
- `read` response now includes `type` (alias of `memory_type`).
- `read` response now includes `version` for optimistic locking.
- `hybrid-search` items now include `score` (alias of `combined_score`).
- `hybrid-search` items now include `source: "hybrid"`.
- `list` items now include `memory_id` (alias of `id`).
- `stats` response now includes `memories_total`, `entities_total`, `relationships_total`.
- `stats` response now includes `chunks_total`, `db_bytes` for contract conformance.
- `health` response now includes top-level `schema_version` per PRD contract.
- `health` response now includes `missing_entities[]` per PRD contract.
- `RememberResponse` includes `operation` (alias of `action`), `created_at`, `created_at_iso`.
- `RecallResponse` includes `results[]` merging `direct_matches` and `graph_matches`.
- `init --namespace` flag added, resolved and echoed back in `InitResponse.namespace`.
- `recall --min-distance <float>` flag added (default 1.0, deactivated by default).
- When `--min-distance` is set below 1.0, returns exit 4 if all hits exceed threshold.

### Fixed

- DB and snapshot files created by `open_rw` now receive chmod 600 on Unix.
- `sync-safe-copy` output files now receive chmod 600 on Unix.
- Prevents credential leakage on shared mounts (Dropbox, NFS, multi-user `/tmp`).
- Progress messages in `remember`, `recall`, `hybrid-search`, `init` now use bilingual helper.
- Language selection now respected consistently (previously mixed EN/PT in same session).

### Documentation

- COOKBOOK, AGENT_PROTOCOL, SKILL, CLAUDE.md updated to match real schemas and flags.
- README, INTEGRATIONS and llms.txt updated to match real exit codes.
- Cross-reviewed against `--help` output of each subcommand.
- `graph` and `cleanup-orphans` subcommands now documented in appropriate guides.
- Honest latency disclaimer added: recall and hybrid-search take ~1s per invocation.
- ~8ms latency requires a daemon (planned for v3.0.0 Tier 4).


## [2.0.0] - 2026-04-18

### Breaking

- EXIT CODE: `DbBusy` moved from 13 to 15 to free exit 13 for `BatchPartialFailure`.
- Shell scripts detecting `EX_UNAVAILABLE` (13) as DB busy must now check for 15.
- HYBRID-SEARCH: response JSON shape reshaped; old shape was `{query, combined_rank[], vec_rank[], fts_rank[]}`.
- New shape is `{query, k, results: [{memory_id, name, namespace, type, description, body, combined_score, vec_rank?, fts_rank?}], graph_matches: []}`.
- Consumers parsing `combined_rank` must migrate to `results` per PRD lines 771-787.
- PURGE: `--older-than-seconds` deprecated in favor of `--retention-days`.
- The old flag remains as a hidden alias but emits a warning; will be removed in v3.0.0.
- NAME SLUG: `NAME_SLUG_REGEX` is stricter than v1.x `SLUG_REGEX`.
- Multichar names must now start with a letter (PRD requirement).
- Single-char `[a-z0-9]` still allowed; existing leading-digit memories pass unchanged.
- `rename` into legacy-style names (leading digit, multichar) will now fail.

### Added

- `AppError::BatchPartialFailure { total, failed }` mapping to exit 13.
- Reserved for `import`, `reindex` and batch stdin (entering in Tier 3/4).
- Constants in `src/constants.rs`: `PURGE_RETENTION_DAYS_DEFAULT=90`, `MAX_NAMESPACES_ACTIVE=100`.
- Constants: `EMBEDDING_MAX_TOKENS=512`, `K_GRAPH_MATCHES_LIMIT=20`, `K_LIST_DEFAULT_LIMIT=100`.
- Constants: `K_GRAPH_ENTITIES_DEFAULT_LIMIT=50`, `K_RELATED_DEFAULT_LIMIT=10`, `K_HISTORY_DEFAULT_LIMIT=20`.
- Constants: `WEIGHT_VEC_DEFAULT=1.0`, `WEIGHT_FTS_DEFAULT=1.0`, `TEXT_BODY_PREVIEW_LEN=200`.
- Constants: `ORT_NUM_THREADS_DEFAULT="1"`, `ORT_INTRA_OP_NUM_THREADS_DEFAULT="1"`, `OMP_NUM_THREADS_DEFAULT="1"`.
- Constants: `BATCH_PARTIAL_FAILURE_EXIT_CODE=13`, `DB_BUSY_EXIT_CODE=15`.
- Flag `--dry-run` and `--retention-days` in `purge`.
- Fields `namespace` and `merged_into_memory_id: Option<i64>` in `RememberResponse`.
- Field `k: usize` in `RecallResponse`.
- Fields `bytes_freed: i64`, `oldest_deleted_at: Option<i64>` in `PurgeResponse`.
- Fields `retention_days_used: u32`, `dry_run: bool` in `PurgeResponse`.
- Flag `--format` in `hybrid-search` (JSON only; text/markdown reserved for Tier 2).
- Flag `--expected-updated-at` (optimistic locking) in `rename` and `restore`.
- Active namespace limit guard (`MAX_NAMESPACES_ACTIVE=100`) in `remember`.
- Returns exit 5 when active namespace limit is exceeded.

### Changed

- `SLUG_REGEX` renamed to `NAME_SLUG_REGEX` with PRD-conformant value.
- New pattern: `r"^[a-z][a-z0-9-]{0,78}[a-z0-9]$|^[a-z0-9]$"`.
- Multichar names must start with a letter.

### Fixed

- Prefix `__` explicitly rejected in `rename` (previously only enforced in `remember`).
- Constants `WEIGHT_VEC_DEFAULT`, `WEIGHT_FTS_DEFAULT` now declared in `constants.rs`.
- PRD references now map to real symbols.


## [1.2.1] - 2026-04-18

### Fixed

- Installation failure on `rustc` versions in the range `1.88..1.95`.
- Caused by transitive dependency `constant_time_eq 0.4.3` (pulled via `blake3`).
- That dependency bumped its MSRV to 1.95.0 in a patch release.
- `cargo install sqlite-graphrag` without `--locked` now succeeds.
- Direct pin `constant_time_eq = "=0.4.2"` forces a version compatible with `rust-version = "1.88"`.

### Changed

- `Cargo.toml` now declares explicit preventive pin `constant_time_eq = "=0.4.2"`.
- Inline comment documents the MSRV drift reason.
- Pin will be revisited when `rust-version` is raised to 1.95.
- `README.md` (EN and PT) install instructions updated to use `cargo install --locked sqlite-graphrag`.
- Bullet added explaining the rationale for `--locked`.

### Added

- `docs_rules/prd.md` section "Dependency MSRV Drift Protection" documents the canonical mitigation pattern.
- Pattern: direct pinning of problematic transitive dependencies in the top-level `Cargo.toml`.


## [1.2.0] - 2026-04-18

### Added

- Counting semaphore cross-process with up to 4 simultaneous slots via `src/lock.rs` (`acquire_cli_slot`).
- Memory guard aborting with exit 77 when free RAM is below 2 GB via `sysinfo` (`src/memory_guard.rs`).
- Signal handler for SIGINT, SIGTERM and SIGHUP via `ctrlc` with `termination` feature.
- Flag `--max-concurrency <N>` to control parallel invocation limit at runtime.
- Hidden flag `--skip-memory-guard` for automated tests where real allocation does not occur.
- Constants `MAX_CONCURRENT_CLI_INSTANCES`, `MIN_AVAILABLE_MEMORY_MB`, `CLI_LOCK_DEFAULT_WAIT_SECS` in `src/constants.rs`.
- Constants `EMBEDDING_LOAD_EXPECTED_RSS_MB` and `LOW_MEMORY_EXIT_CODE` in `src/constants.rs`.
- `AppError::AllSlotsFull` and `AppError::LowMemory` variants with messages in Brazilian Portuguese.
- Global `SHUTDOWN: AtomicBool` and function `shutdown_requested()` in `src/lib.rs`.

### Changed

- Flag `--wait-lock` default increased to 300 seconds (5 minutes) via `CLI_LOCK_DEFAULT_WAIT_SECS`.
- Lock file migrated from single `cli.lock` to `cli-slot-{N}.lock` (counting semaphore N=1..4).

### Removed

- BREAKING: flag `--allow-parallel` removed; caused critical OOM in production (incident 2026-04-18).

### Fixed

- Critical bug where parallel CLI invocations exhausted system RAM.
- 58 simultaneous invocations locked the computer for 38 minutes (incident 2026-04-18).


## [Unreleased]

### Added

- Global flags `--allow-parallel` and `--wait-lock SECONDS` for controlled concurrency.
- Module `src/lock.rs` implementing file-based single-instance lock via `fs4`.
- New `AppError::LockBusy` variant mapping to exit code 75 (`EX_TEMPFAIL`).
- Environment variables `ORT_NUM_THREADS`, `OMP_NUM_THREADS` and `ORT_INTRA_OP_NUM_THREADS` pre-set to 1.
- Singleton `OnceLock<Mutex<TextEmbedding>>` for intra-process model reuse.
- Integration tests under `tests/lock_integration.rs` covering lock acquisition and release.
- `.cargo/config.toml` with conservative `RUST_TEST_THREADS` default and standardized cargo aliases.
- `.config/nextest.toml` with `default`, `ci`, `heavy` profiles and `threads-required` override for loom and stress tests.
- `scripts/test-loom.sh` as canonical invocation for local loom runs with `RUSTFLAGS="--cfg loom"`.
- `docs/TESTING.md` and `docs/TESTING.pt-BR.md` bilingual testing guide.
- `slow-tests` Cargo feature for future opt-in heavy tests.

### Changed

- Default behavior is now single-instance.
- A second concurrent invocation exits with code 75 unless `--allow-parallel` is passed.
- Embedder module refactored from struct-with-state to free functions operating on a singleton.
- Move `loom = "0.7"` to `[target.'cfg(loom)'.dev-dependencies]` — skipped by default cargo test.
- Remove legacy `loom-tests` Cargo feature replaced by official `#[cfg(loom)]` gate.
- CI workflow `ci.yml` migrated to `cargo nextest run --profile ci` with explicit `RUST_TEST_THREADS` per job.
- Loom CI job now exports `LOOM_MAX_PREEMPTIONS=2`, `LOOM_MAX_BRANCHES=500`, `RUST_TEST_THREADS=1`, `--release`.

### Fixed

- Prevents OOM livelock when the CLI is invoked in massively parallel fashion by LLM orchestrators.
- Prevent thermal livelock on loom concurrency tests by aligning `#[cfg(loom)]` gate with upstream pattern.
- Serialize `tests/loom_lock_slots.rs` with `#[serial(loom_model)]` to forbid parallel execution of loom models.


## [0.1.0] - 2026-04-17

### Added

- Phase 1: Foundation: SQLite schema with vec0 (sqlite-vec), FTS5, entity graph.
- Phase 2: Essential subcommands: init, remember, recall, read, list, forget, rename, edit, history.
- Phase 2 continued: restore, health, stats, optimize, purge, vacuum, migrate, hybrid-search.
- Phase 2 continued: namespace-detect, sync-safe-copy.

### Fixed

- FTS5 external-content corruption bug in forget+purge cycle.
- Removed manual DELETE in forget.rs that caused the corruption.

### Changed

- Raised MSRV from 1.80 to 1.88 (required by transitive dependencies base64ct 1.8.3, ort-sys, time).

- Historical release links below still point to the legacy `neurographrag` repository
- The renamed project starts its public version line at `sqlite-graphrag v1.0.0`

[Unreleased]: https://github.com/daniloaguiarbr/neurographrag/compare/v2.3.0...HEAD
[2.1.0]: https://github.com/daniloaguiarbr/neurographrag/releases/tag/v2.1.0
[2.0.2]: https://github.com/daniloaguiarbr/neurographrag/releases/tag/v2.0.2
[2.0.1]: https://github.com/daniloaguiarbr/neurographrag/releases/tag/v2.0.1
[2.0.0]: https://github.com/daniloaguiarbr/neurographrag/releases/tag/v2.0.0
[1.2.1]: https://github.com/daniloaguiarbr/neurographrag/releases/tag/v1.2.1
[1.2.0]: https://github.com/daniloaguiarbr/neurographrag/releases/tag/v1.2.0
[0.1.0]: https://github.com/daniloaguiarbr/neurographrag/releases/tag/v0.1.0
