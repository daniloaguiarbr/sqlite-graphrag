# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.37] - 2026-04-30

### Fixed
- **B1+B2 (BLOCKER, docs)**: Synced `CHANGELOG.pt-BR.md` with the v1.0.36 entry (was missing in the PT mirror) and added two missing callouts to `README.pt-BR.md:108-109` mirroring `README.md` ("**Run `init` first**" and "**`graphrag.sqlite` is created in the current working directory by default**"). Audit on flowaiper docs corpus revealed PT-BR users could not discover the implicit cwd behavior.
- **H7+M9 (HIGH, behavior)**: `list --include-deleted --json` now emits `deleted_at` (Unix epoch) and `deleted_at_iso` (RFC 3339) for soft-deleted memories. Active memories continue to omit both fields via `#[serde(skip_serializing_if = "Option::is_none")]` for backward compatibility. `MemoryRow` in `src/storage/memories.rs` gained a `deleted_at: Option<i64>` field; all four SQL SELECTs (read_by_name, read_full, list-with-type, list-without-type, fts_search-with-type, fts_search-without-type) updated to include the column. `docs/schemas/list.schema.json` updated to document both optional fields. Previously LLM agents calling `list --include-deleted` could not distinguish active from soft-deleted rows without a second SQL query.
- **H8 (HIGH, behavior)**: `src/i18n.rs::Language::from_env_or_locale` now respects POSIX locale precedence `LC_ALL > LC_MESSAGES > LANG`. The previous loop iterated all three vars and returned PT on the first "pt" prefix, violating POSIX semantics where `LC_ALL` overrides `LANG` regardless of value (`LC_ALL=en_US LANG=pt_BR` returned PT instead of EN). The fix stops iteration at the first set var, recognizes both "pt" and "en" prefixes, and falls through to English default only when no locale var is set. Three new regression tests cover the precedence rule.

### Added
- **H9 (CI hardening)**: New `cargo-audit` job in `.github/workflows/ci.yml` runs `cargo audit --deny warnings`. Complements `cargo deny check`, which previously did not flag `RUSTSEC-2025-0119` (number_prefix unmaintained, transitive via fastembed/hf-hub/indicatif) or `RUSTSEC-2024-0436` (paste unmaintained, transitive via tokenizers/text-splitter). Any new advisory now blocks merge until acknowledged or pinned.
- **B6 (multi-platform)**: Added `x86_64-unknown-linux-musl` target to `.github/workflows/release.yml` matrix (uses the existing `Install musl tools` step gated on `matrix.musl == true`). Enables Alpine Linux and distroless container deployments without forcing users to compile from source.
- **B3 (docs)**: Created `docs_rules/rules_rust.md` as the canonical Regra Zero index referenced by the project's `CLAUDE.md`. Lists all eight specific rule files under `docs_rules/` with one-line summaries and inviolable principles.
- **B4 (docs)**: Renamed `docs_rules/rules_rusts_paralelismo_e_multiprocessamento.md` to `rules_rust_paralelismo_e_multiprocessamento.md` (typo fix: extra `s`). The file is gitignored and excluded from the published tarball, so the rename is not visible to crates.io consumers.

### Improved
- **H1 (HIGH, extraction)**: Expanded `ALL_CAPS_STOPWORDS` in `src/extraction.rs:58-173` with 23 additional PT-BR technical/generic words found leaking through into `entities` during a 50-file flowaiper corpus audit: `ACID`, `AINDA`, `APENAS`, `CEO`, `CRIE`, `DDL`, `DEFINIR`, `DEPARTMENT`, `DESC`, `DSL`, `DTO`, `EPERM` (POSIX errno), `ESCREVA`, `ESRCH` (POSIX errno), `ESTADO`, `FATO`, `FIFO` (data structure), `FLUXO`, `FONTES`, `FUNCIONA`, `MESMO`, `METADADOS`, `PONTEIROS`. List grew from 108 to 131 entries; previously these words were captured by `regex_all_caps()` as spurious `concept` entities, polluting the graph with non-entities (~27% of 402 entities in 50-doc corpus were noise). Stopword filter is alphabetically ordered for review readability and uses linear scan via `.contains()`.

### Notes
- Findings discovered during the v1.0.36 audit cycle on the `flowaiper/docs_flowaiper` real-world corpus (495 PT-BR markdown files). Audit phases A/B/C/D completed (D=200/200), phase E (495/495) was running at the time of these fixes.
- Remaining v1.0.38+ backlog: case-insensitive entity dedupe (CLAUDE/Claude, GEMINI/Gemini, GITHUB/GitHub leaking as separate entities), hyphen vs underscore relation alignment (CLI accepts `depends-on`, schema CHECK uses `depends_on`), ADR for daemon vs `rules_rust_cli_stdin_stdout` ("PROIBIDO daemons persistentes") policy, and remaining multi-platform targets (`x86_64-apple-darwin`, `wasm32-wasip2`, universal2 macOS).
- All eight CLAUDE.md validation gates pass: fmt, clippy `-D warnings`, test (431/434, 3 ignored), doc with `RUSTDOCFLAGS="-D warnings"`, audit with documented ignores for two transitive unmaintained advisories pending upstream, deny check, publish dry-run, package list (138 files, zero sensitive).

## [1.0.36] - 2026-04-30

### Fixed (Linguistic policy)
- **C1 (CRITICAL)**: Synced `--type` enum in `skill/sqlite-graphrag-en/SKILL.md:46` and `-pt/SKILL.md:46` from 4 listed values to the full set of 9 (`user, feedback, project, reference, decision, incident, skill, document, note`). Agents using SKILL.md as a contract had been silently losing five memory types since v1.0.30. Source of truth: `src/cli.rs:364-374` (`MemoryType` enum) and `src/commands/remember.rs:26` long-help.
- **H1+H2+H3 (HIGH)**: Translated three Portuguese-without-accent strings in `tracing::warn!` macros that escaped the audit gate `rg '[áéíóúâêôãõç]' src/` documented in v1.0.33: `src/extraction.rs:1204` (`"NER falhou..."` → `"NER failed..."`), `src/extraction.rs:964` (`"batch NER falhou (chunk de N janelas)..."` → `"batch NER failed (chunk of N windows)..."`), `src/commands/remember.rs:345` (`"auto-extraction falhou..."` → `"auto-extraction failed..."`). Bonus: also translated `src/storage/urls.rs:37` (`"falha ao persistir url..."` → `"failed to persist url..."`) and the production error in `src/commands/remember.rs:367` (`"limite de N namespaces ativos excedido..."` → `"active namespace limit of N reached..."`).
- **M1 (MEDIUM)**: Added a complementary CI gate in `.github/workflows/ci.yml language-check` job that scans `tracing::*!`, `#[error(...)]`, doc comments, and `panic!`/`assert!`/`expect`/`bail!`/`ensure!` macros for Portuguese words without diacritical marks (`falhou`, `janelas`, `usando apenas`, `nao foi`, `ja existe`, `obrigatorio`, `memoria`, etc.). Plain string literals are intentionally not scanned because they hold legitimate PT test fixtures for multilingual extraction.
- **M3 (MEDIUM)**: Renamed 33 Portuguese test function names to English across `tests/integration.rs`, `tests/exit_codes_integration.rs`, `tests/concurrency_limit_integration.rs`, `tests/recall_integration.rs`, `tests/prd_compliance.rs`, `tests/loom_lock_slots.rs`, `tests/vacuum_integration.rs`, `src/commands/optimize.rs`, `list.rs`, `health.rs`, `debug_schema.rs`, `unlink.rs`. Examples: `test_link_idempotente_retorna_already_exists` → `test_link_idempotent_returns_already_exists`; `prd_optimize_executa_e_retorna_status_ok` → `prd_optimize_runs_and_returns_status_ok`; `optimize_response_serializa_campos_obrigatorios` → `optimize_response_serializes_required_fields`. Plus ~80 `.expect("X falhou")` test helpers translated to `.expect("X failed")`, doc comments and assert messages cleaned in `src/graph.rs`, `src/memory_guard.rs`, `src/cli.rs`, `src/storage/entities.rs`, and several `tests/*.rs` files. Test fixture STRINGS that exercise PT-BR ingestion (e.g. multilingual NER inputs) remain intentionally in PT-BR.

### Fixed (Code logic)
- **H5 (HIGH)**: Extended `regex_section_marker()` in `src/extraction.rs:210-218` to include `Camada` alongside `Etapa`, `Fase`, `Passo`, `Seção`, `Capítulo`. Audit on a 50-file PT-BR corpus showed `Camada 1` through `Camada 5` leaking through to `entities` with degree 3 each, polluting the graph. The filter now strips them at both the regex prefilter and the BERT NER post-merge stages.
- **M7 (MEDIUM)**: Expanded `ALL_CAPS_STOPWORDS` in `src/extraction.rs:60-165` with `ADICIONADA`, `ADICIONADAS`, `ADICIONADO`, `ADICIONADOS`, `CLARO`, `CONFIRMARAM`, `CONFIRMEI`, `CONFIRMOU` (alphabetically merged into the list). The earlier audit found these PT-BR adjective/verb forms being captured as `concept` entities by `regex_all_caps()` in `apply_regex_prefilter`.
- **L2 (LOW)**: Daemon spawn backoff in `src/daemon.rs:record_spawn_failure` now applies half jitter (`base/2 + rand([0, base/2))`) instead of pure exponential. Avoids retry herd if multiple CLI instances detect daemon failure simultaneously. Uses `SystemTime::now().subsec_nanos()` as a dependency-free entropy source — sufficient for low-frequency spawn coordination.
- **L5+L6 (LOW)**: `src/i18n.rs::Language::from_env_or_locale` now treats empty `SQLITE_GRAPHRAG_LANG=""` as unset (no `tracing::warn!` emitted), matching POSIX convention. `src/i18n.rs::init` short-circuits when the OnceLock is already populated, preventing the env-resolver from running a second time and emitting the warning twice.

### Improved
- **M2 (MEDIUM)**: Added a "JSON Schemas" section to `README.md`, `README.pt-BR.md`, `docs/AGENT_PROTOCOL.md`, and `docs/AGENT_PROTOCOL.pt-BR.md` linking to the 30 canonical JSON Schema files in `docs/schemas/`. These contracts existed since v1.0.33 but were undiscoverable from the public docs.
- **M4 (MEDIUM)**: `src/i18n.rs::tr` no longer leaks one allocation per call. The signature now requires `&'static str` inputs (which all in-tree callers already pass — they are string literals) and returns one of them directly. The previous `Box::leak(en.to_string().into_boxed_str())` pattern accumulated allocations in long-running pipelines.
- **L3 (LOW)**: Added an MSRV (Rust 1.88) callout to `README.md` and `README.pt-BR.md` Installation sections. Previously documented only as a footnote in the Mac Intel notes.

### Notes
- **M6 was reclassified as a documentation/test artefact**: `related --json` was reported to return `graph_depth: null`, but the field is named `hop_distance` (`src/commands/related.rs:77` and serialised key). The audit query used `.graph_depth` which did not exist. The field has always been populated correctly. No code change required.
- **L1 (sys_locale) was deferred**: the manual `LC_ALL`/`LANG` parsing in `src/i18n.rs:34-57` works correctly across the targets used in CI. Adding `sys_locale` would introduce a dependency for marginal benefit (macOS CFLocale APIs and Windows GetUserDefaultLocaleName) without a confirmed reproducer.
- **L4 (BERT NER misclassifications) is out of scope**: `Tokio=location`, `Borda=person`, `Campos=location`, and `AdapterRun=organization` are limitations of `Davlan/bert-base-multilingual-cased-ner-hrl`. Filtering would require either a different model or a curated whitelist; both deferred until they cause concrete user impact.
- All 427 lib tests pass with the new test names and translated assertions. `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo doc`, `cargo audit`, and `cargo deny check advisories licenses bans sources` are clean.
- The new `language-check` gate in CI now blocks any PR re-introducing PT in tracing/error/doc/assert surfaces.

## [1.0.35] - 2026-04-30

### Fixed
- **WAL-AUTO-INIT (HIGH)**: Auto-init path (`remember`, `ingest`, `recall`, `list`, ... — every command that goes through `ensure_db_ready()`) now activates `journal_mode=wal` consistently. Before v1.0.35 only the explicit `init` command flipped journal mode to WAL; databases created on-demand by other commands stayed in `journal_mode=delete`, breaking `sync-safe-copy` checkpoint semantics, the documented concurrency guarantees, and the troubleshooting advice that referenced WAL. Fix moves `PRAGMA journal_mode = WAL` into `apply_connection_pragmas` (called by every `open_rw`) and adds a defensive re-assertion (`ensure_wal_mode`) after migrations to neutralise refinery's internal handle reuse. Regression coverage: `tests/wal_auto_init_regression.rs`.
- **JSON-SCHEMA-VERSION (MEDIUM-HIGH)**: `init --json`, `stats --json` and `migrate --json` now emit `schema_version` as a JSON **number** instead of a string, aligning with `health --json` (which already used number). Fixes parsing inconsistency for clients that consumed both shapes. JSON Schemas (`docs/schemas/stats.schema.json`, `docs/schemas/migrate.schema.json`, `docs/schemas/debug-schema.schema.json`) updated to reflect the canonical type. **Breaking** for clients that explicitly compared as string; clients using numeric comparisons are unaffected.
- **DAEMON-SOCKET-FALLBACK (LOW)**: Unix socket fallback path in `to_local_socket_name()` now respects `XDG_RUNTIME_DIR` then `SQLITE_GRAPHRAG_HOME` before falling back to `/tmp`. Reduces collision risk on multi-tenant hosts. Path is only used when abstract namespace sockets fail to bind (rare).

### Added
- **CLI-LIMIT-ALIAS (UX)**: `recall` and `hybrid-search` now accept `--limit` as alias of `-k/--k`. Aligns with `list`/`related` which already used `--limit`. Non-breaking, additive.
- **CLI-RENAME-FROM-TO (UX)**: `rename` now accepts `--from`/`--to` as aliases of `--name`/`--new-name`. Non-breaking, additive.
- **JSON-RELATED-INPUT-ECHO (UX)**: `related --json` response now includes `name` and `max_hops` echo fields for input transparency. Non-breaking, additive.

### Changed
- **GRAPH-NODE-KIND-DEPRECATED**: `graph --format json` still emits both `kind` and `type` fields per node, but `kind` is now formally documented as **deprecated** (kept for pre-v1.0.35 backward compat). New consumers MUST read `type`. The duplicate field will be removed in a future major release.

### Documentation
- **PRAGMA-USER-VERSION-49**: Added doc comment in `src/constants.rs` explaining why `SCHEMA_USER_VERSION = 49` (project signature for external diagnostic tools) versus `CURRENT_SCHEMA_VERSION = 9` (application-level migration count). They are intentionally different and serve distinct purposes.
- **README**: Expanded the Memory content lifecycle table with `--body-file`/`--body-stdin`/`--entities-file`/`--relationships-file`/`--graph-stdin` flags for `remember`, the new aliases for `recall`/`rename`, and a callout about kebab-case ASCII memory name validation. Added explicit rows for `ingest` and `cache clear-models`.

### Notes
- Audit findings #4 (structured truncation flags in JSON output) and #6 (progress/ETA in ingest summary) are deferred to v1.0.36 — they require schema design beyond a patch release. Truncation is currently surfaced via `tracing::warn!` only; pipeline consumers should monitor stderr.
- All 427 lib tests pass. Regression test `wal_auto_init_regression.rs` added (uses `assert_cmd` + `tempfile`, same pattern as existing integration tests).

## [1.0.34] - 2026-04-30

### Added
- **JS7 (LOW)**: `vacuum --json` response now includes `reclaimed_bytes: u64` derived field, computed as `size_before_bytes.saturating_sub(size_after_bytes)`. Callers no longer need to compute the delta themselves. Schema in `src/commands/vacuum.rs:32-41`. Existing fields `size_before_bytes` and `size_after_bytes` preserved unchanged.

### Documentation
- **PRD-sync (LOW)**: Updated `docs_rules/prd.md` (excluded from published crate via `Cargo.toml exclude`) to reflect schema reality after V008 (v1.0.25) and V009 (v1.0.30) migrations:
  - MemoryType enum: 7 → 9 (added `document`, `note` per V009 CHECK constraint and `MemoryType` enum in `src/cli.rs`).
  - EntityType enum: 10 → 13 (added `organization`, `location`, `date` per V008 CHECK constraint and BERT NER types).

### Notes
- Audit dimension `unwrap`/`expect` reaffirmed clean by `audit-team-v1033/diagnostician`: ZERO production unwraps; 12 production expects all carry English-language documented invariants (regex literal compilation, BERT NER no-NaN logits, OnceLock just-set get, const compile-time invariants) — all fall under CLAUDE.md's "casos impossíveis" exception.
- Unsafe blocks audit reaffirmed clean: all ~14 `unsafe { }` blocks across `main.rs` (4×), `embedder.rs` (1×), `storage/connection.rs` (1×), `commands/optimize.rs` (2×), and `paths.rs` (6× tests) carry SAFETY comments. The earlier finding flagging missing SAFETY comments was a false positive (the comments precede the `unsafe` keyword, outside `-B3` grep context).
- Bumped patch (1.0.33 → 1.0.34) because the new `reclaimed_bytes` field is purely additive (`#[derive(Serialize)]` adds the key) and PRD changes are doc-only (file is in `Cargo.toml exclude`). No API removed; no behavior changed.

## [1.0.33] - 2026-04-30

### Fixed (Linguistic Policy)
- **C3-residual (HIGH)**: Translated remaining Portuguese string in `src/daemon.rs:183` (Drop impl `tracing::debug!` for spawn lock removal). v1.0.32 A1 covered lines 113/131/154/307/419 but missed line 183 inside `impl Drop for DaemonSpawnGuard`. Audit gate `rg '[áéíóúâêôãõç]' src/ -g '!i18n.rs'` now returns ZERO matches.
- **PT-V007 (HIGH)**: Translated 5-line Portuguese SQL header comment in `migrations/V007__memory_urls.sql` to English. The file is part of the published crate (not in `Cargo.toml exclude`), so docs.rs and crates.io tarball previously shipped Portuguese SQL comments.
- **AS-PT (MEDIUM)**: Translated 20 Portuguese `assert!` messages to English across `src/commands/hybrid_search.rs` (19 occurrences) and `src/commands/list.rs` (1 occurrence). All `mem-* deveria existir` assertion messages in `src/storage/memories.rs` (9 occurrences) translated to `mem-* should exist`. Per CLAUDE.md "NUNCA `assert!` com mensagem em português" — even test code is EN-only.

### Fixed (Documentation)
- **D3 (MEDIUM)**: Synchronized `--type` doc-comment in `src/commands/recall.rs:33`, `src/commands/list.rs:30`, `src/commands/hybrid_search.rs:35` to list all 13 graph entity types (`project/tool/person/file/concept/incident/decision/memory/dashboard/issue_tracker/organization/location/date`). Previously listed only 10, omitting `organization/location/date` added by `migrations/V008__expand_entity_types.sql` (BERT NER types). Aligns CLI help with PRD `docs_rules/prd.md` and the V008 CHECK constraint.

### Notes
- Validated against real-world ingest of 50 representative `.md` files (~6.6 MB corpus): 50/50 indexed in 56.9s with `--skip-extraction`; 5/5 indexed with full BERT NER extraction in 57.3s. All 12 functional CLI scenarios (init, ingest, recall, hybrid-search, list, related, graph, health, stats, lifecycle, vacuum, sync-safe-copy) returned exit 0 with valid JSON. Auto-create of `graphrag.sqlite` in CWD (without prior `init`) confirmed working with mode 0600.
- Backwards-compatible duplicate fields in `stats --json` (`memories`/`memories_total`, `entities`/`entities_total`, `relationships`/`relationships_total`, `db_size_bytes`/`db_bytes`, `edges`/`relationships`) and `list --json` (`id`/`memory_id`) are intentional per existing test assertions in `src/commands/stats.rs:244-248` and `src/commands/list.rs:190`. They are deliberately preserved for backwards compatibility with existing JSON parsers.
- `schema_version` type asymmetry between `stats --json` (`String`) and `health --json` (`u32`) is documented as a known issue. Normalization to `u32` everywhere would be a breaking change deferred to v2.0.
- `kill_on_drop(true)` for the daemon child process remains N/A (the orphan detach is deliberate, documented in `src/daemon.rs:491-499` and v1.0.32 M4 / C2). The CLI must return immediately while the daemon stays warm.

## [1.0.32] - 2026-04-30

### Fixed (Critical — Audit findings from v1.0.31)
- **C1 (CRITICAL)**: Auto-init unified across all CRUD handlers via new `ensure_db_ready` helper in `src/storage/connection.rs`. Previously `remember` silently auto-created the DB while `recall`, `list`, etc. returned `NotFound`, breaking the implicit "if it works for one, it works for all" contract. Now every CRUD subcommand creates the database on first use with a single `tracing::info!("creating database (auto-init) at <path> schema_version=9")` log entry. Resolves the 23 inconsistent `paths.db.exists()` checks across `forget`, `related`, `optimize`, `edit`, `health`, `hybrid_search`, `cleanup_orphans`, `rename`, `recall`, `read`, `vacuum`, `graph_export` (×4), `purge`, `list`, `history`, `unlink`, `link`, `stats`, `sync_safe_copy`, `debug_schema`.
- **C2 (CRITICAL)**: Documented the deliberate orphan-daemon detach in `src/daemon.rs:487`. The `Child` handle is now intentionally dropped with a `// SAFETY:` comment explaining lifecycle ownership via spawn lock + ready file + idle-timeout shutdown, plus a `tracing::debug!` log capturing the daemon PID. `Stdio::null()` already covered the I/O detach.
- **C3 (CRITICAL)**: New integration test `tests/readme_examples_executable.rs` parses every `bash` fenced block from `README.md` and `README.pt-BR.md` at compile time and executes each `sqlite-graphrag` invocation against a real binary in an isolated `TempDir`. Blocks containing pipes/redirects or marked `<!-- skip-test -->` are skipped. 22 commands per README are now CI-validated, eliminating the drift uncovered in v1.0.31 (8+ broken examples: `--query` vs positional `<QUERY>`, `--top-k` vs `-k`, `--dir` vs positional `<DIR>`, etc.).

### Fixed (High)
- **A1 (HIGH)**: Translated 8 Portuguese runtime strings to English in `src/lock.rs:36`, `src/daemon.rs:113,131,154,307,419` (including the `daemon.rs:307` IPC payload that leaked PT into JSON `message` fields). Added `Message::EmptyQueryValidation` and `Message::EmptyBodyValidation` (as `validation::empty_query()` / `validation::empty_body()`) in `src/i18n.rs` so user-visible validation messages remain bilingual; internal errors are EN-only. Audit gate `rg '[áéíóúâêôãõç]' src/ -g '!i18n.rs'` now returns ZERO matches.
- **A2 (HIGH)**: Refactored `src/commands/ingest.rs` from per-file fork-spawn (`Command::new(current_exe).args(["remember", ...]).output()`) to in-process pipeline. Loads the embedder once and reuses it across all files via `crate::daemon::embed_passage_or_local`. Measured speedup: 50 files in **21 seconds** vs ~14 minutes previously (≈40× faster, well under the 60s target). Per-file NDJSON event schema unchanged (`{file, name, status, memory_id, action}`).
- **A3 (HIGH)**: Replaced `.expect("OnceLock populated by set() above")` in `src/embedder.rs:56` with `.ok_or_else(|| AppError::Embedding(...))?` propagating a real error variant. Eliminates the only remaining production `.expect()` outside documented invariants.
- **A4 (HIGH)**: Added `#[command(after_long_help = "EXAMPLES: ...")]` with 2-4 realistic invocations to 21 subcommands previously missing it (`init`, `daemon`, `read`, `list`, `forget`, `purge`, `rename`, `edit`, `history`, `restore`, `health`, `migrate`, `namespace-detect`, `optimize`, `stats`, `sync-safe-copy`, `vacuum`, `related`, `cleanup-orphans`, `cache`, `__debug_schema`, plus enrichment of `hybrid-search`/`ingest`).
- **A5 (HIGH)**: Auto-migrate transparency. `ensure_db_ready` now compares `PRAGMA user_version` against `SCHEMA_USER_VERSION` and runs the remaining migrations automatically when an older DB (e.g. v1.0.27 schema 7) is opened by a newer binary. Logs `tracing::warn!(from, to, path, "auto-migrating database schema")` so operators are not surprised. Eliminates the silent failure mode where stale DBs caused indeterminate runtime errors.
- **A6 (HIGH)**: Renamed 23 Portuguese identifiers to English across `tests/property_based.rs`, `tests/i18n_bilingual_integration.rs`, `tests/integration.rs`, `tests/vacuum_integration.rs`, `tests/exit_codes_integration.rs`, `tests/regression_v2_0_4.rs`, `tests/schema_contract_strict.rs`, `src/errors.rs`, `src/commands/health.rs`. Plus residual PT comments and assert messages in `src/storage/entities.rs`, `src/commands/remember.rs`, `src/chunking.rs`, `src/graph.rs`, `src/embedder.rs`, `src/output.rs`, `src/tz.rs`, `src/memory_guard.rs`, `src/daemon.rs`, `src/lock.rs` translated to English.

### Fixed (Medium)
- **M1 (MEDIUM)**: `recall -k` and `hybrid-search -k` now use `value_parser = parse_k_range` validating the inclusive range `1..=4096` (matches `sqlite-vec`'s knn limit) at parse time. Out-of-range values surface a clean Clap error instead of leaking the engine's `"k value in knn query too large"` message. Added unit tests in `src/parsers/mod.rs`.
- **M2 (MEDIUM)**: `purge` UX clarified. Added alias `--max-age-days` for the existing `--retention-days`. When `purged_count == 0`, the JSON response now includes a `message` field (`"no soft-deleted memories older than {N} day(s); use --retention-days 0 to purge all soft-deleted memories regardless of age"`). Help text on `--yes` rewritten to clarify it confirms intent but does NOT override `--retention-days`.
- **M3 (MEDIUM)**: Added `#[arg(help = "...")]` to 9 positional arguments previously bare in `--help` output: `recall <QUERY>`, `hybrid-search <QUERY>`, `ingest <DIR>`, `read <NAME>`, `forget <NAME>`, `rename <NAME>`, `edit <NAME>`, `history <NAME>`, `related <NAME>`.
- **M4 (MEDIUM)**: Verified `daemon --stop` already exists (dispatches to `crate::daemon::try_shutdown`) and that the autostart spawn path uses `std::process::Command` with intentional orphan detach (documented under C2). `tokio::process::Command` `kill_on_drop(true)` was N/A — code path uses std spawn — so no change needed; the C2 safety comment now explains the design rationale.
- **M5 (MEDIUM)**: Audit finding "duplicate v1.0.29 entries with date 2026-04-29" was a false positive (v1.0.29 and v1.0.30 are distinct entries that legitimately share `2026-04-29` as their release date). No CHANGELOG change required.

### Fixed (Low)
- **B_1 (LOW)**: README structure (split `README.md` + `README.pt-BR.md`) preserved; the bilingual policy is documented elsewhere. ADR not required since the split is a deliberate product decision predating the audit.
- **B_2 (LOW)**: Added GitHub Actions CI badge (`[![CI](...)](...)`) to both `README.md` and `README.pt-BR.md`. Final badge order: crates.io → docs.rs → CI → license → Contributor Covenant.
- **B_3 (LOW)**: Added bash example blocks for 16 subcommands previously without one in either README: `daemon`, `ingest`, `rename`, `edit`, `restore`, `migrate`, `namespace-detect`, `optimize`, `vacuum`, `link`, `unlink`, `related`, `graph` (with `stats`/`traverse`/`entities` subcommands), `cleanup-orphans`, `cache`, `history`. All 16 examples are validated by the new `tests/readme_examples_executable.rs`.
- **B_4 (LOW)**: `remember` JSON output now includes `name_was_normalized: bool` and `original_name: Option<String>` (the latter elided via `#[serde(skip_serializing_if = "Option::is_none")]` when normalization was a no-op). Closes the UX gap where users passing `--name "Hello World"` saw only `"name": "hello-world"` with no indication that normalization had happened.

### Added
- `tests/readme_examples_executable.rs` — 442-line integration test (8 unit + 2 integration tests) validating every README bash example.
- `parse_k_range` value parser in `src/parsers/mod.rs` with full unit-test coverage of edge cases (zero, above-limit, non-integer, negative).
- `validation::empty_query()` and `validation::empty_body()` bilingual messages in `src/i18n.rs`.
- `ensure_db_ready(&AppPaths)` helper in `src/storage/connection.rs` (also makes `register_vec_extension` idempotent via `OnceLock`).
- `insert_default_schema_meta` helper extracted to ensure auto-init populates `schema_version`, `model`, `dim`, `created_at`, `sqlite-graphrag_version` consistently with explicit `init`.

### Changed
- `src/commands/ingest.rs` grew from 565 to ~959 lines as the in-process pipeline replicates `remember::run`'s validation + chunking + embedding + persistence transaction. The previous version offloaded that work to a child process per file.
- `register_vec_extension` is now idempotent (guarded by `OnceLock`); safe to invoke from both `main.rs` and library helpers (unblocks unit tests touching CRUD handlers).
- Optimize test `optimize_returns_not_found_when_db_missing` renamed to `optimize_auto_inits_when_db_missing` and inverted to assert success (the new auto-init contract).
- CI-aligned `clippy::uninlined_format_args` cleanup on the new `ensure_db_ready` log line.

### Notes
- Validation pipeline summary: `cargo fmt --check` ✓, `cargo clippy -- -D warnings` ✓, `cargo test --lib` 427/427 ✓, `cargo doc --no-deps` ✓ zero warnings, `cargo audit` ✓ (2 pre-allowed advisories per `deny.toml`), `cargo deny check advisories licenses bans sources` ✓.
- Language gate audit: `rg '[áéíóúâêôãõç]' src/ -g '!i18n.rs'` returns ZERO matches.
- Performance baseline: 50 files ingest in 21s wall-clock (≈40× faster than v1.0.31).

## [1.0.31] - 2026-04-30

### Fixed
- **A2 (P1-CRITICAL)**: `ingest` subcommand now emits proper NDJSON (one JSON object per line). Previously emitted pretty-printed multiline JSON, breaking line-by-line consumers. Switched 5 calls in `src/commands/ingest.rs` from `output::emit_json` to `output::emit_json_compact`.
- **A3 (P1-MEDIUM)**: `stats --json` now reports correct `schema_version` value (e.g., "9") read from `refinery_schema_history` table. Previously returned "unknown" because empty `schema_meta` table was queried.
- **A4 (P1-MEDIUM)**: `forget` command now populates `action` and `deleted_at` fields in JSON output. Three explicit states: `soft_deleted`, `already_deleted`, `not_found`. Race-safe via re-SELECT after soft-delete.
- **A1 (P0-CRITICAL)**: Extraction pipeline no longer hangs on documents larger than ~50 KB. Added `EXTRACTION_MAX_TOKENS=5000` cap (env override `SQLITE_GRAPHRAG_EXTRACTION_MAX_TOKENS`). Body exceeding cap is truncated for NER but full body still goes through regex. Empirical impact: 68 KB document went from >5 minutes to ~37 seconds (88% reduction) while preserving `extraction_method=bert+regex-batch`.
- **A9 (P2-MEDIUM)**: Relationship fan-out reduced — entities co-occurring in same sentence/paragraph now generate edges; previously generated C(N,2) "mentions" between all entities in memory.
- **A10 (P2-MEDIUM)**: Name truncation at 60 chars now logs `tracing::warn` and handles collisions with numeric suffix (-1, -2, ...).

### Added
- **A6**: New integration test suite `tests/ingest_integration.rs` covering NDJSON contract, fail-fast, max-files, name truncation, --skip-extraction, --pattern variants, recursive walk.
- **A7**: V009 end-to-end migration tests in `tests/schema_migration_integration.rs`: `v009_document_type_lifecycle_e2e`, `v009_note_type_lifecycle_e2e`, `v009_invalid_type_rejected`.
- **A11**: PT-BR uppercase stoplist for NER false-positive filter (ADAPTER, PROJETO, PASSIVA, SOMENTE, LEITURA, etc.). Improves entity extraction quality for Portuguese-language corpora.

### Improved
- **A5 (P1-MEDIUM)**: Renamed 210 test functions in `src/*` across 35 files from Portuguese to English identifiers (also covered helper functions like `nova_memoria` → `new_memory`, `cria_node` → `make_node`, `resposta_vazia` → `empty_response`). Brings codebase into full compliance with project's English-exclusive language policy for identifiers.
- **A8 (P1-MEDIUM)**: Refined production-only `.unwrap()`/`.expect()` calls. Original audit count of 167 was inflated — most matches were inside `#[cfg(test)] mod tests` blocks (acceptable per CLAUDE.md). The actual production-path inventory was 13 occurrences. Improvements: 1 `.expect()` in `src/embedder.rs` got a more precise invariant message; 10 `Regex::new(LITERAL).unwrap()` in `src/extraction.rs` static `OnceLock` initializers replaced with `.expect("compile-time validated <kind> regex literal")`; 2 `.max_by(...).unwrap()` over BERT NER logits replaced with `.expect("BERT NER logits invariant: no NaN in classifier output")`; 1 `.expect()` in `src/chunking.rs` translated from PT to EN. The 4 `.unwrap()` calls in `src/graph.rs`, 3 in `src/namespace.rs`, and 2 in `src/output.rs` are inside `///` doctests (idiomatic per Rust API Guidelines C-EXAMPLE).
- **A12+A13**: Translated ~38 PT comments in `tests/signal_handling_integration.rs`, `tests/lock_integration.rs`, and `deny.toml`. Removed 2 obsolete `[advisories.ignore]` entries (RUSTSEC-2024-0436, RUSTSEC-2025-0119) — `cargo deny check` now reports zero advisory-not-detected warnings.
- **A14**: Translated ~150 additional PT comments in `tests/prd_compliance.rs`, `tests/integration.rs`, `tests/concurrency_hardened.rs`, `tests/security_hardening.rs`, and other test files.

### Audit Methodology
- 13 gaps identified empirically via plan-mode audit on installed v1.0.30 binary against real-file corpus (20 markdown PT-BR docs).
- All fixes validated via PDCA + Agent Teams orchestration: 11 tasks, 9 teammates spawned in parallel, each with Rule Zero compliance and per-task validation.
- Validation passed: cargo fmt, cargo clippy --all-targets -- -D warnings, cargo audit, cargo deny check, cargo doc -D warnings, cargo nextest run.

## [1.0.30] - 2026-04-29

### Added (New Subcommand — Bulk Ingestion)
- `sqlite-graphrag ingest <DIR> --type <TYPE>` subcommand for bulk-indexing every file in a directory as a separate memory. Supports `--pattern` (default `*.md`), `--recursive`, `--skip-extraction`, `--fail-fast`, `--max-files` (safety cap default 10000), `--namespace`, `--db`. Output is line-delimited JSON: one event per file (`{file, name, status, memory_id, action}`) followed by a final summary (`{summary: true, files_total, files_succeeded, files_failed, files_skipped, elapsed_ms}`). Names are derived from file basenames in kebab-case. Each file is processed by spawning a child `remember --body-file` invocation, so concurrency slots, lock semantics, and error semantics match standalone `remember`. Resolves the long-standing UX gap where users had to shell-script over `for f in *.md; do remember ...; done` to ingest a corpus.

### Changed (Help Text Clarity — `link` / `unlink`)
- `link --help` and `unlink --help` now make explicit that `--from` and `--to` accept ENTITY names (graph nodes auto-extracted by BERT NER, or created implicitly by prior `link` calls), NOT memory names. Includes an `EXAMPLES:` block and a `NOTES:` block in `after_long_help`. Previously the bare doc-comment "Source entity" was easily misread as "memory name" by new users; the resulting `Erro: entidade '<name>' não existe` was confusing because the user thought they were passing a valid memory name. Field doc comments now mention `graph --format json | jaq '.nodes[].name'` as the canonical way to list eligible entity names.

### Changed (Dependencies — rusqlite/refinery upgrade)
- `rusqlite` bumped from `0.32` to `0.37` and `refinery` bumped from `0.8` to `0.9`. Cargo.lock now resolves `rusqlite v0.37.0`, `refinery v0.9.1`, `refinery-core v0.9.1`, `refinery-macros v0.9.1`, and `libsqlite3-sys v0.35.0`. Zero source code changes were required — both crates kept the public APIs we use stable across these versions. Reach for rusqlite 0.39 was blocked by `refinery-core 0.9.0` capping `rusqlite = ">=0.23, <=0.37"`; revisit when refinery raises that ceiling.

### Fixed (Critical — Schema/CLI Contract Mismatch)
- `migrations/V009__expand_memory_types.sql` — new migration that recreates the `memories` table (and its FK children: `memory_versions`, `memory_chunks`, `memory_entities`, `memory_relationships`, `memory_urls`) to expand the `type` CHECK constraint from 7 to 9 values, adding `'document'` and `'note'`. Without this migration, `--type document` and `--type note` (added to the CLI enum in v1.0.29) were always rejected at runtime with `exit 10` — `CHECK constraint failed: type IN ('user','feedback','project','reference','decision','incident','skill')`. The CLI Clap layer accepted nine values while the database enforced seven, breaking every README example that used `--type document`.
- `tests/schema_migration_integration.rs` updated to assert exactly 9 migrations applied (previously expected 6) and `schema_version = "9"`.

### Fixed (Critical — Language Policy Violations Missed by v1.0.28 Audit)
The v1.0.28 audit used a single-line regex (`rg "tracing::(info|warn|error|debug)!.*[áéíóúâêôãõç]"`) and reported zero violations. Multi-line macro invocations and identifiers without diacritics escaped detection. Fixed in this release:

- `src/extraction.rs:749` — Portuguese `tracing::warn!("relacionamentos truncados em {max_rels} (com {n} entidades, máx teórico era ~{}× combinações)", ...)` translated to `"relationships truncated to {max_rels} (with {n} entities, theoretical max was ~{}x combinations)"`.
- `src/extraction.rs:1025` — Portuguese `tracing::warn!("extração truncada em {MAX_ENTS} entidades (entrada tinha {total_input} candidatos antes da deduplicação)")` translated to `"extraction truncated at {MAX_ENTS} entities (input had {total_input} candidates before deduplication)"`.
- `src/extraction.rs` — Eight `.context("...")`, `.with_context(|| format!("..."))` and `anyhow::anyhow!("...")` calls translated from Portuguese to English: `"forward pass do BertModel"` → `"BertModel forward pass"`, `"forward pass do classificador"` → `"classifier forward pass"`, `"removendo dimensão batch"` → `"removing batch dimension"`, `"criando tensor de ids para batch"` → `"creating id tensor for batch"`, `"padding tensor de ids"` → `"padding id tensor"`, `"criando tensor de máscara para batch"` → `"creating mask tensor for batch"`, `"criando token_type_ids batch"` → `"creating token_type_ids tensor for batch"`, `"forward pass batch BertModel"` → `"BertModel batch forward pass"`, `"criando diretório do modelo"` → `"creating model directory"`, `"carregando tokenizer NER"` → `"loading NER tokenizer"`, `"encoding NER"` → `"encoding NER input"`.
- `src/daemon.rs` — Two `tracing::*!` strings translated: `"falha ao remover lock file de spawn ao encerrar daemon"` → `"failed to remove spawn lock file while shutting down daemon"`; `"daemon encerrado graciosamente; socket será limpo pelo OS ou pelo próximo daemon via try_overwrite"` → `"daemon shut down gracefully; socket will be cleaned up by OS or by the next daemon via try_overwrite"`.
- `src/commands/restore.rs` — `tracing::info!("restore --version omitido; usando última versão não-restore: {}", v)` translated to `"restore --version omitted; using latest non-restore version: {}"`.

### Fixed (Test Identifiers — English-only Policy)
~80 test identifiers (function names, helper names, `mod` names, type aliases) renamed from Portuguese to English. Phase 1 audit only flagged the diacritic subset (`*ção`, `*á`); identifiers without accents (`*_aceita_`, `*_rejeita`, `*_funciona`, `*_retorna`, etc.) were missed. Touched files:

- `src/cli.rs` — `mod testes_concorrencia_pesada` → `mod heavy_concurrency_tests`; `mod testes_formato_json_only` → `mod json_only_format_tests`; 3 inner test fns renamed.
- `src/paths.rs` — `limpar_env_paths` helper + 5 test fns renamed (`home_env_resolve_db_em_subdir`, `home_env_traversal_rejeitado`, `db_path_vence_home`, `flag_vence_home`, `home_env_vazio_cai_para_cwd`, `parent_or_err_aceita/rejeita_*`).
- `src/errors.rs` — 11 test fns renamed (the `_em_portugues` suffix family + 3 others).
- `src/commands/init.rs` — 5 test fns renamed (`init_response_serializa_*`, `latest_schema_version_retorna_*`, `init_response_dim/namespace_alinhado_*`).
- `src/commands/migrate.rs` — 5 test fns + 2 helper fns renamed.
- `src/extraction.rs` — 11 internal test fns renamed (the `iob_mapeia_*`, `regex_*_aceita_*`, `build_relationships_sem_duplicatas`, etc.).
- `src/output.rs`, `src/memory_guard.rs`, `src/commands/{sync_safe_copy, cleanup_orphans, list, vacuum}.rs` — 7 test fns renamed.
- `src/storage/{urls, memories, entities}.rs` — `type Resultado` → `type TestResult` (3 modules, ~70 occurrences).
- `tests/security_hardening.rs` — 16 test fns renamed (`test_path_traversal_rejeitado_*`, `test_chmod_*_apos_init_*`, `test_blake3_*_diferente_*`, `test_sql_injection_em_*`, etc.).
- `tests/integration.rs` — ~28 test fns renamed (the `test_remember_cria/rejeita/aceita_*`, `test_link_cria_relacao_*`, `test_graph_stdin_aceita_*`, etc.).
- `tests/prd_compliance.rs` — ~15 test fns renamed.
- `tests/concurrency_*.rs`, `tests/i18n_bilingual_integration.rs`, `tests/signal_handling_integration.rs`, `tests/v2_breaking_integration.rs`, `tests/lock_integration.rs`, `tests/property_based.rs`, `tests/loom_lock_slots.rs`, `tests/regression_positional_args.rs`, `tests/recall_integration.rs`, `tests/daemon_integration.rs`, `tests/schema_migration_integration.rs` — remaining test fns and helpers translated.

### Notes
- `errors::to_string_pt()` and `main::emit_progress_i18n(en, pt)` continue to hold legitimate Portuguese strings — these are the i18n branch invoked when `--lang pt` (or detected locale) is active. They are not violations.
- Default behaviour `./graphrag.sqlite` in CWD (resolved via `paths.rs:35-41`) confirmed empirically against the v1.0.29 audit corpus (29 of 30 flowaiper Markdown documents indexed end-to-end; recall p50 ~50ms, hybrid-search p50 ~52ms; one stress-test failure was an external 60s timeout, not a tool defect).
- Empirical evidence: the bug was reproducible with one CLI invocation: `sqlite-graphrag remember --type document --name x --description y --body z` returned exit 10 with the schema CHECK error message in v1.0.29.

## [1.0.29] - 2026-04-29

### Fixed (Critical — Language Policy Violations in Production Code)
- `src/paths.rs:21` — Portuguese error message `"não foi possível determinar o diretório home"` in `AppError::Io` translated to `"could not determine home directory"`. Was emitted in `tracing::error!` and CLI stderr regardless of `--lang` flag.
- `src/paths.rs:85-89` — Portuguese error message `"caminho '{}' não possui componente pai válido"` in `AppError::Validation` translated to `"path '{}' has no valid parent component"`.
- `src/main.rs:227` — Portuguese `tracing::warn!("recebido sinal de shutdown...")` translated to `"shutdown signal received; waiting for current command to finish gracefully"`. Tracing logs are required to be English regardless of locale.
- `src/commands/purge.rs:21` — Portuguese doc comment `"[DEPRECATED em v2.0.0]"` translated to `"[DEPRECATED in v2.0.0]"`.
- `src/commands/purge.rs:70-71` — Portuguese warning string `"--older-than-seconds está deprecado..."` (emitted in JSON `warnings` field) translated to `"--older-than-seconds is deprecated; use --retention-days in v2.0.0+"`. JSON output must be language-neutral.
- `src/commands/purge.rs:123` — Portuguese `anyhow!("erro de relógio do sistema: {err}")` translated to `"system clock error: {err}"`.
- `src/commands/purge.rs:192-193` — Portuguese warning `"falha ao limpar vec_chunks..."` (in JSON `warnings`) translated to `"failed to clean vec_chunks for memory_id {memory_id}: {err}"`.
- `src/commands/purge.rs:198-201` — Portuguese warning `"falha ao limpar vec_memories..."` (in JSON `warnings`) translated to `"failed to clean vec_memories for memory_id {memory_id}: {err}"`.
- `src/main.rs:265` — Removed duplicate `tracing::error!(error = %e)` that emitted localized error string into structured logs (line 266 `emit_error(&e.localized_message())` already handles user-visible output). Eliminates the i18n→tracing leakage where Portuguese error payloads were polluting EN-only log channels.

### Fixed (Security — Path Traversal & Unsafe Audit)
- `src/paths.rs:60` — `validate_path` now uses `Path::components().any(|c| c == Component::ParentDir)` instead of substring `.contains("..")`, preventing both false positives on filenames containing `..` (e.g., `..config`) and potential bypass via non-standard path encodings.
- `src/extraction.rs:271` — Added comprehensive `SAFETY:` comment to `unsafe { VarBuilder::from_mmaped_safetensors(...) }` documenting the three soundness invariants (file not concurrently modified, mmaped region lifetime tracking, safetensors format validation).
- `src/storage/connection.rs:14-21` — Added `SAFETY:` comment to `unsafe { rusqlite::ffi::sqlite3_auto_extension(...) }` documenting FFI ABI compatibility, transmute layout invariants, and single-call invocation guarantee.
- `src/paths.rs` (6 SAFETY comments in tests) — Translated from Portuguese (`"SAFETY: testes marcados com #[serial] garantem ausência de concorrência."`) to English (`"SAFETY: tests are annotated with #[serial], guaranteeing single-threaded execution."`).

### Added (UX Improvements)
- `list --include-deleted` flag to surface soft-deleted memories. Without this flag, `forget` followed by `list` would create a workflow dead-end where soft-deleted entries became invisible.
- `history --no-body` flag to omit version body content from the JSON response. Useful for memories with large body content where only metadata/version sequence is needed.
- `MemoryType::Document` and `MemoryType::Note` variants added to the `--type` enum (`remember`, `list`, `recall`). Documentation-style content no longer needs to abuse the `Reference` type.
- `help =` text added to ~10 previously bare flags (`--namespace`, `--limit`, `--offset`, `--format`, `--db`, `--include-deleted`, `--no-body`) across `list`, `history`, and other subcommands.
- README Quick Start now explicitly documents that `sqlite-graphrag init` is the first required command and that `graphrag.sqlite` is created in the current working directory by default.

### Changed (Schema & UX)
- `--json` flag is now hidden in 21 subcommands via `#[arg(long, hide = true)]`. The flag was a no-op (JSON is the default output format) but appeared in `--help` causing confusion. The flag remains accepted for backward compatibility with tools that pass it explicitly.
- `history` JSON response: `metadata` field type changed from `String` (raw JSON-encoded) to `serde_json::Value` (parsed object), aligning with `read` which already exposed it as `Value`. Consumers parsing `metadata` as a JSON string must now read it as an object directly. Empty/invalid metadata defaults to `{}`.
- `history` JSON response: `body` field is now `Option<String>` (omitted when `--no-body` is set). When the field is present (default), the existing schema is unchanged.
- `Cargo.toml` `exclude` list: `/CLAUDE.md`, `/AGENTS.md`, `/MEMORY.md` rewritten without leading `/` for idiomatic relative-path semantics matching cargo conventions.

### Notes
- This is a **patch release** focused on policy compliance and UX fixes detected in the v1.0.28 audit (`/tmp/sqlite-graphrag-audit/reports/audit-v1.0.28.md`).
- One JSON schema change: `history.metadata` from string to object. Consumers that parsed `metadata` as a string must now read it as an object. All other JSON contracts (commands, fields, exit codes) remain unchanged.
- Empirically validated against real Markdown documents from a 495-file corpus during the v1.0.28 audit. CRUD cycle (init → remember → recall → read → edit → forget → purge) verified end-to-end.

## [1.0.28] - 2026-04-28

### Changed
- Enforces the English-only Language Policy across the entire codebase. All `///` and `//!` doc comments, all `tracing::*!` log strings, and all identifiers (functions, statics, modules, enum variants, test names) outside `src/i18n.rs` translation tables are now in English. PT-BR strings remain only in `Language::Portuguese` branches inside `i18n::errors_msg`, `i18n::validation`, and `errors::to_string_pt()`.
- `Language::Portugues` enum variant renamed to `Language::Portuguese` (CLI aliases `pt`, `pt-br`, `pt-BR`, `portugues`, `portuguese` preserved for backward compatibility).
- `IDIOMA_GLOBAL` static renamed to `GLOBAL_LANGUAGE` (`src/i18n.rs`).
- `FUSO_GLOBAL` static renamed to `GLOBAL_TZ` (`src/tz.rs`).
- ~30 PT-named functions renamed to English equivalents in `src/i18n.rs` and `src/tz.rs` (e.g., `formatar_iso` → `format_iso`, `epoch_para_iso` → `epoch_to_iso`, `memoria_nao_encontrada` → `memory_not_found`, `nome_kebab` → `name_kebab`, `validacao` module → `validation`, `erros` module → `errors_msg`).
- 32 internal `mod testes` test modules renamed to `mod tests` for consistency with Rust convention.
- All call-sites in `src/commands/*.rs` and tests propagated to use the renamed identifiers.

### Added
- `//!` crate-level documentation in 37 modules that previously lacked it: `src/cli.rs`, `src/main.rs`, `src/extraction.rs`, `src/embedder.rs`, `src/daemon.rs`, `src/output.rs`, `src/paths.rs`, `src/chunking.rs`, `src/graph.rs`, `src/namespace.rs`, `src/parsers/mod.rs`, `src/tokenizer.rs`, `src/storage/{connection,urls,chunks,versions,mod}.rs`, `src/pragmas.rs`, and 22 handlers in `src/commands/`.
- `language-check` CI job in `.github/workflows/ci.yml` that fails the build when Portuguese diacritics are detected in `///`, `//!`, `tracing::*!` calls, or `#[error(...)]` attributes — automated guardrail against regression.

### Documentation
- Two broken intra-doc links (`[Cli]`, `[TextEmbedding]`) fixed in `src/lib.rs` and `src/embedder.rs` (surfaced when `cargo doc -D warnings` was first run with the new doc coverage).

### Notes
- This is a **non-breaking** change for the CLI and JSON contracts: subcommand names, flags, env vars, exit codes, and JSON field names remain unchanged. Internal Rust identifiers were renamed but the crate is a binary, not a library consumed via `pub use`.
- 65 files changed, +872/-715 lines. All 9 cargo gates pass (fmt, clippy, test, doc, audit, deny, publish dry-run, package list, llvm-cov).

## [1.0.27] - 2026-04-28

### Added
- `CURRENT_SCHEMA_VERSION: u32 = 8` constant in `src/constants.rs` with unit test that asserts equality with the count of `V*.sql` migration files.
- `output::emit_error` and `output::emit_error_i18n` functions centralizing stderr error output (Pattern 5: ÚNICO ponto de I/O em `output.rs`).
- `nextest` test-groups configuration in `.config/nextest.toml` to serialize cross-binary tests sharing the daemon socket and model cache. Eliminates `contract_15_link` flake observed since v1.0.24.

### Changed
- README EN+PT (`Graph Schema` section) now lists `entity_type` as exactly 13 values (was 10) — adds `organization`, `location`, `date` introduced in V008 schema migration of v1.0.25.
- `init --help` docstring documents path resolution precedence (`--db` > `SQLITE_GRAPHRAG_DB_PATH` > `SQLITE_GRAPHRAG_HOME` > cwd).
- `src/commands/recall.rs` graph-distance comment clarified: it remains a hop-count proxy (`1.0 - 1.0/(hop+1)`), real cosine distance is reserved for v1.0.28 (forward-dated reference fixed).
- All 6 `eprintln!` calls in `src/main.rs` migrated to `output::emit_error*` to enforce Pattern 5.

### Documentation
- `SQLITE_GRAPHRAG_LOG_FORMAT` now documented in the env-var table of README EN+PT (was implemented since v1.0.x but undocumented).
- README `unlink` row corrected from the non-existent `--relationship-id` flag to the actual `--from --to --relation` flags. The previous documentation could mislead agents into rejecting valid invocations.
- `docs/MIGRATION.md` and `docs/MIGRATION.pt-BR.md` version reference updated from v1.0.17 to v1.0.27 (3 occurrences each).
- `docs/HOW_TO_USE.md` and `docs/HOW_TO_USE.pt-BR.md` `link` recipe examples corrected to use `--from`/`--to` instead of the non-existent `--source`/`--target` flags.

### Fixed
- Formatting drift in `tests/doc_contract_integration.rs:669` resolved via `cargo fmt --all` (multi-line array → single-line as expected by rustfmt).

### Notes
- Investigation of the audit P1 finding `tokenizer.rs:101-103 std::fs::read in async path` concluded **false positive**: `get_tokenizer` and `get_model_max_length` are called only from `src/commands/remember.rs:389-391` inside `pub fn run()` which is synchronous. No `spawn_blocking` wrap is required. The blocking I/O is appropriate for the synchronous CLI command path.
- Two `advisory-not-detected` warnings from `cargo deny` for ignored advisories `RUSTSEC-2024-0436` (paste) and `RUSTSEC-2025-0119` (number_prefix) were observed but kept in `deny.toml` — they protect against re-introduction via fastembed's transitive deps if upstream regresses. A scheduled cleanup is deferred to v1.0.28 after explicit verification of `cargo tree` confirming the deps are no longer present.

## [1.0.26] - 2026-04-28

### Added
- `SQLITE_GRAPHRAG_HOME` env var for setting the base directory for `graphrag.sqlite` (precedence: `--db` > `SQLITE_GRAPHRAG_DB_PATH` > `SQLITE_GRAPHRAG_HOME` > cwd).
- README sample JSON output for `remember` showing `extracted_entities`, `extracted_relationships`, and `urls_persisted` fields.
- Expanded exit-code table with sub-causes for exit 1 (Validation error or runtime failure).

### Changed
- README clarifies that GraphRAG entity extraction runs by default in `remember` (use `--skip-extraction` to disable per call).
- Renamed reference to "automatic ingestion" in README to disambiguate "daemon autostart" from "automatic entity extraction".

### Fixed
- Daemon `handled_embed_requests` counter now correctly reports the cumulative count after `init` autospawn (was returning 0 since v1.0.24 due to a per-connection local counter shadowing the shared accumulator).
- Test `contract_15_link` aligned with the actual `link --json` output keys (`action`, `from`, `to`, `relation`, `weight`, `namespace`); the obsolete expectations of `source`/`target` numeric IDs were stale since v1.0.24.

## [1.0.25] - 2026-04-28

### Added
- `recall --all-namespaces` flag searches across all namespaces in a single query (P0-1).
- BERT NER now emits `organization` (B-ORG), `location` (B-LOC), and `date` (B-DATE)
  entity types aligned with V008 schema migration. Previous releases mapped ORG→`project`,
  LOC→`concept`, and discarded DATE entirely (P0-2 + V008 alignment).
- Schema migration V008: `entities.type` CHECK constraint expanded to include `organization`,
  `location`, `date`. Additive migration; existing rows are preserved unchanged.
- BRAND_NAME_REGEX captures CamelCase organization names such as "OpenAI", "PostgreSQL",
  "ChatGPT" that BERT NER frequently misclassifies (P0-2).
- Portuguese monosyllabic verb false-positive filter ("Lê", "Vê", "Cá", etc.) for BERT
  outputs below confidence threshold 0.85 (P0-2).
- SECTION_MARKER_REGEX filters text fragments like "Etapa 3", "Fase 1", "Passo 2",
  "Seção 4", "Capítulo 1" from entity extraction (P0-4).
- 12 new ALL_CAPS_STOPWORDS: `API`, `CAPÍTULO`, `CLI`, `ETAPA`, `FASE`, `HTTP`, `HTTPS`,
  `JWT`, `LLM`, `PASSO`, `REST`, `UI`, `URL` (P0-4).
- README documents `graph traverse|stats|entities` subcommands with flags table (P1-A).

### Changed
- `recall.graph_matches[].distance` now reflects graph hop count via proxy
  `1.0 - 1.0 / (hop + 1)`. Previous releases used `0.0` placeholder. Real cosine
  distance is reserved for v1.0.26 (P1-M).
- `merge_and_deduplicate` longest-wins logic rewritten with composite key
  `entity_type + name_lc` and bidirectional substring containment. Resolves
  "Sonne"/"Sonnet" duplication and "Open"/"Paper" truncation issues (P0-3).
- `Cargo.toml` version bumped from `1.0.24` to `1.0.25`.

### Fixed
- `is_valid_entity_type` now accepts new V008 types `organization`, `location`, `date` (P0-A) — without this fix, `remember` would reject any entity emitted by the V008-aligned IOB mapping with exit 1.
- `augment_versioned_model_names` regex no longer captures Portuguese section markers like "Etapa 3" or "Fase 1" (P0-B) — defense-in-depth filter applied after augmentation and inside `iob_to_entities.flush()`.
- `remember --name` longer than 80 bytes now returns exit code 6 (LimitExceeded)
  instead of exit 1 (Validation). Restores the exit code contract used by
  orchestrating agents (P1-J).

### Notes
- `recall.graph_matches[].distance` is approximate; semantic cosine distance reserved for v1.0.26.
- Entity and relationship caps (30 and 50 respectively) remain silent in v1.0.25;
  explicit `--limit-entities` / `--limit-relations` flags planned for v1.0.26.

## [1.0.24] - 2026-04-27

### Added
- BERT NER batch inference via `predict_batch` reduces per-document latency on multi-doc workloads (Phase 3 perf).
- SQLITE_BUSY and SQLITE_LOCKED retry with exponential backoff in `with_busy_retry`; avoids spurious exit 10 on WAL-mode contention (Phase 3).
- `spawn_blocking` warm-up for daemon BERT model init prevents blocking the async executor during startup (Phase 3).
- Schema migration V007: `memory_urls` table with indexes; URLs extracted from BERT NER are now persisted separately instead of leaking into the entity graph (Phase 2).
- `src/storage/urls.rs` CRUD module providing `upsert_urls`, `get_urls_for_memory` and `delete_urls_for_memory` (Phase 2).
- `RememberResponse.urls_persisted: usize` field reporting how many URL entries landed in `memory_urls` (Phase 2).
- `RememberResponse.relationships_truncated: bool` field indicating whether the relationships payload was capped at `max_relationships_per_memory` (Phase 4).
- `namespace_initial` persisted in `schema_meta` on `init`; `purge` resolves contextually via `SQLITE_GRAPHRAG_NAMESPACE` (Phase 4 P1-A/P1-C).
- Positional and flag arguments in `read`, `forget`, `history`, `edit`, `rename`; e.g. `sqlite-graphrag read my-note` is equivalent to `sqlite-graphrag read --name my-note` (Phase 4 P1-B).
- Stopwords list expanded with 17 new entries: `ACEITE`, `ACK`, `ACL`, `BORDA`, `CHECKLIST`, `COMPLETED`, `CONFIRME`, `DEVEMOS`, `DONE`, `FIXED`, `NEGUE`, `PENDING`, `PLAN`, `PODEMOS`, `RECUSE`, `TOKEN`, `VAMOS` (Phase 2 P0-3).
- NFKC unicode normalization in `merge_and_deduplicate` prevents near-duplicate entities caused by composed vs decomposed Unicode forms (Phase 2 P1-E).
- Regression tests for `graph` traverse exit 4 when the database is absent (Phase 1 P0-7).
- Regression tests for positional-plus-flag argument equivalence in `read`, `forget`, `history`, `edit`, `rename` (Phase 4 P1-B).

### Changed
- `ReadResponse.metadata` is now `serde_json::Value` instead of `String`; agents receive a structured object directly without a second `JSON.parse` call (Phase 5 P2-A).
- `LinkResponse` simplified: redundant `source` and `target` fields removed; `LinkArgs` no longer accepts `--source`/`--target` flag aliases (Phase 4 P1-O).
- `purge` no longer defaults namespace to `"global"`; resolves via `SQLITE_GRAPHRAG_NAMESPACE` or explicit `--namespace` (Phase 4 P1-C).
- `recall --precise` behavior is now documented and internally uses `effective_k = 100000` for exhaustive KNN (Phase 1 P0-6).
- `init --model` now uses the typed `EmbeddingModelChoice` enum validated at parse time (Phase 1 P0-8).
- `main.rs` RAM measurement uses `Result` propagation instead of `expect` (Phase 1 P1-G).
- Daemon warm-up model load moved into `spawn_blocking` to avoid blocking the Tokio executor (Phase 3 P1-I).
- `augment_versioned_model_names` regex extended to recognize `GPT-4o`, `Claude 4 Sonnet`, `Llama 3 Pro`, `Mixtral 8x7B` patterns (Phase 5 P2-D).
- `extend_with_numeric_suffix` now accepts alphanumeric suffixes (e.g. `v2`, `3b`, `7B`) in addition to purely numeric ones (Phase 5 P2-E).
- Graph entity serialization uses `Vec::new()` instead of `Option<Vec>` so the `entities` field is always an array, never `null` (Phase 5 P2-C).
- `--type` argument docstrings clarified to distinguish memory `type` from `entity_type` (Phase 5 P2-J).
- `Cargo.toml` version bumped from `1.0.23` to `1.0.24`.

### Fixed
- `remember` rejects names that normalize to an empty string after kebab-case canonicalization; returns exit 1 with a clear validation message (Phase 4 P0-4).
- URLs no longer leak into the entity graph; all URL-shaped tokens from BERT NER are now routed to `memory_urls` via V007 (Phase 2 P0-2).
- `HybridSearchResponse.weights` serialization confirmed correct; field was a no-op phantom flag with no behavioral effect (Phase 4 P1-N).

### Security
- Added `// SAFETY:` comments to every `unsafe { std::env::set_var(...) }` block in `main.rs` (Phase 1 P1-H).
- `deny.toml`: `unmaintained` set to `"workspace"` to scope unmaintained-crate checks to workspace members only; reduces false-positive CI failures on transitively unmaintained crates (Phase 5 P2-K).
- `SQLITE_GRAPHRAG_LANG` invalid value now emits a `tracing::warn!` log instead of silently falling back to English (Phase 1 P1-M).

### Internal
- 412+ tests passing across all phases.
- Bundle release: Phases 1, 2, 3, 4 and 5 land in a single commit.

## [1.0.23] - 2026-04-27

### Fixed
- BERT NER subword merge now prefers the longest candidate when multiple sources extract overlapping names. Previously "OpenAI" from regex could lose to "Open" from a BERT subword leak because both deduplicated to the lowercase key `open`. The new logic in `merge_and_deduplicate` retains the strictly longest entry, biasing toward the most specific brand visible in the corpus (P1 fix in `src/extraction.rs`).
- Versioned model names with a space separator ("Claude 4", "Llama 3", "Python 3") are now extracted as `concept` entities through the new `augment_versioned_model_names` pass. BERT NER frequently classifies these tokens as common nouns and skips them, so the version suffix used to vanish. Hyphenated variants like "GPT-5" remain handled by the existing NER+suffix pipeline (P1 fix in `src/extraction.rs`).
- `recall` now exposes `graph_depth: Option<u32>` on every `RecallItem`. Direct vector matches set it to `None` (rely on `distance`); graph traversal results set it to `Some(0)` as a sentinel for "reachable via graph, depth not yet tracked precisely". The legacy `distance: 0.0` placeholder remains for backward compatibility but should be treated as deprecated for graph rows (P1 fix in `src/commands/recall.rs` and `src/output.rs`).
- `remember` now reports `chunks_persisted: usize` alongside `chunks_created: usize` so callers know exactly how many rows landed in `memory_chunks`. Single-chunk bodies report `chunks_persisted: 0` (the memory row itself acts as the chunk) while multi-chunk bodies report `chunks_persisted == chunks_created`. Resolves the v1.0.22 audit finding where short bodies showed `chunks_created: 1` with zero rows persisted (P1 fix in `src/output.rs` and `src/commands/remember.rs`).

### Added
- `recall --max-graph-results <N>` caps `graph_matches` at most N entries. Defaults to unbounded so v1.0.22 callers see the same shape, but lets dense graph neighbourhoods be capped explicitly. The `-k` docstring now states clearly that it controls only `direct_matches` (P1 UX fix in `src/commands/recall.rs`).
- README EN now lists the `pt-BR` and `portuguese` aliases for `SQLITE_GRAPHRAG_LANG`. Previously only the PT-BR README mentioned them, leaving English readers unaware (P1 docs sync fix).
- README EN+PT now document the five pre-built binary targets explicitly and call out that Mac Intel (`x86_64-apple-darwin`) requires building locally because GitHub retired the macos-13 runner in December 2025 and Apple discontinued x86_64 support. Recommended migration is to Apple Silicon (P1 distribution clarity fix).
- `docs/COOKBOOK.md` and `docs/COOKBOOK.pt-BR.md` taglines now state the correct recipe count of 23 (was incorrectly claiming 15 since the v1.0.22 additions). Counted by `rg -c '^## How To'` in both files (P1 docs accuracy fix).

### Changed
- `Cargo.toml` version bumped from `1.0.22` to `1.0.23`.
- `RememberResponse` JSON gains the `chunks_persisted` field (always present); `RecallItem` JSON gains `graph_depth` (omitted when `None` via `skip_serializing_if`). Both additions are forward-compatible for any client that uses lenient JSON parsers.

## [1.0.22] - 2026-04-27

### Fixed
- `forget` + `restore` workflow no longer dead-ends. `history --name <X>` now returns versions for soft-deleted memories (was filtering `deleted_at IS NULL`); response includes a new boolean `deleted` field. `restore --version` is now optional: when omitted, the latest non-`restore` version is used automatically. Together these fixes make the round-trip `forget` → `restore` work without requiring the user to read SQL (P0 fix in `src/commands/history.rs` and `src/commands/restore.rs`).
- `list`, `forget`, `edit`, `read`, `rename`, `history`, `hybrid-search` now check for missing `graphrag.sqlite` upfront and return `AppError::NotFound` (exit 4) with the friendly "Execute 'sqlite-graphrag init' primeiro" message, matching `stats`/`recall`/`health`. Previously `list` leaked the raw rusqlite error and returned exit 10 (P1 inconsistency fix).
- `remember` now rejects empty or whitespace-only `body` (with no external graph) via `AppError::Validation` (exit 1). Prevents persisting memories with empty embeddings that broke recall semantics (P1 fix in `src/commands/remember.rs`).
- BERT NER post-processing extended to filter additional ALL CAPS PT-BR/EN stopwords observed in stress test of 495 FlowAiper documents (verbs, adjectives, common nouns) and HTTP method names (`GET`, `POST`, `DELETE`, etc.). Single-token NER outputs are now also filtered, not only regex prefilter matches (P1 fix in `src/extraction.rs`).
- BERT NER URL prefilter now strips trailing markdown punctuation (backticks, parens, brackets, dots, semicolons) before persisting URLs as entities. Previously `https://example.com/`` was stored verbatim (P1 fix in `src/extraction.rs`).
- BERT NER entities with hyphenated or space-separated numeric suffixes (e.g. `GPT-5`, `Claude 4`, `Python 3.10`) are now extended in post-processing instead of being truncated. Suffix lookup is conservative: only extends when ≤6 chars and purely numeric (P1 fix in `src/extraction.rs::extend_with_numeric_suffix`).
- README EN and pt-BR `entity_type` enumeration corrected from "9 values" to "10 values" with `issue_tracker` listed (P1 docs fix).

### Added
- `SQLITE_GRAPHRAG_MAX_RELATIONS_PER_MEMORY` environment variable to configure the relationships-per-memory cap (default 50, range [1, 10000]). Auditing identified that documents with rich entity graphs hit the cap silently; users with technical corpora can now tune (P1 fix via `src/constants.rs::max_relationships_per_memory()`).
- `HistoryResponse.deleted: bool` field exposing whether the memory is currently soft-deleted, enabling clients to detect forgotten state without inspecting `memory_versions` directly.
- 18 previously-undocumented CLI flags now have `///` docstrings visible in `--help`: `init --model`, `init --force`, `remember --name/--description/--body/--body-stdin/--metadata/--session-id`, `read --name`, `forget --name`, `edit --name/--body/--body-file/--body-stdin/--description`, `history --name`, `daemon --idle-shutdown-secs/--ping/--stop` (P1 UX fix).

### Changed
- `Cargo.toml` version bumped from `1.0.21` to `1.0.22`.
- `MAX_RELS=50` const in `src/extraction.rs` consolidated into `crate::constants::max_relationships_per_memory()` removing the duplicate definition.
- `restore --version` arg type changed from `i64` to `Option<i64>` (backward-compatible: explicit version still works as before).

## [1.0.21] - 2026-04-26

### Fixed
- BERT NER `iob_to_entities` no longer leaks WordPiece subword fragments like `##AI` or `##hropic` as standalone entities. When BERT emits a `B-*` label on a token starting with `##` (model confused state), the subword is appended to the active entity if any, otherwise discarded (P0 fix in `src/extraction.rs:381-394`). Empirically validated: stress audit of 138 FlowAiper documents produced ZERO `##` fragments in the entity table.
- `recall` rejects empty queries with `AppError::Validation` and a clear message instead of leaking raw rusqlite error `Invalid column type Null at index: 1, name: distance` (P1 fix in `src/commands/recall.rs`).
- `restore` now re-embeds the restored memory body and upserts into `vec_memories` so vector recall works on restored memories. v1.0.20 left `vec_memories` count behind `memories` count after `forget` + `restore` (P1 fix in `src/commands/restore.rs`).
- `stats` reports `chunks_total` accurately by querying `memory_chunks` and treating only "no such table" errors as legacy DB state worth defaulting to zero; other SQLite errors are now logged via `tracing::warn!` for visibility (P1 fix in `src/commands/stats.rs`).
- Six panics in production paths converted to idiomatic `unreachable!()` inside `#[cfg(test)]` blocks (P1 fix in `graph_export.rs`, `memory_guard.rs`, `optimize.rs`, `tz.rs`, `namespace_detect.rs`).
- README EN and pt-BR exit code tables now list `73` (memory guard rejected low RAM condition), matching `llms.txt` and source semantics (P1 docs fix).

### Added
- `RememberResponse.extraction_method: Option<String>` field exposing whether auto-extraction used `bert+regex` or fell back to `regex-only`. Field is omitted from JSON when `--skip-extraction` is set (telemetry P1 in `src/output.rs` and `src/commands/remember.rs`).
- `ExtractionResult.extraction_method` field populated by `extract_graph_auto` and `RegexExtractor`, exposing the actual extraction path taken (P1 fix in `src/extraction.rs`).
- 2 new unit tests covering the IOB merge fix: `iob_strip_subword_b_prefix` and `iob_subword_orphan_descarta`.

### Changed
- `Cargo.toml` version bumped from `1.0.20` to `1.0.21`.

## [1.0.20] - 2026-04-26

### Fixed
- BERT NER model loading now downloads `tokenizer.json` from the `onnx/` subfolder of the `Davlan/bert-base-multilingual-cased-ner-hrl` HuggingFace repository, where it is actually published. v1.0.19 attempted to download from the repository root and got 404 on every ingestion, falling silently into regex-only graceful degradation (P0 primary fix in `src/extraction.rs::ensure_model_files`).
- BERT NER classifier head weights are now loaded from the safetensors file via `VarBuilder::pp("classifier").get(...)` for both `weight` and `bias`. v1.0.19 initialized them with `Tensor::zeros`, which produced a constant argmax across all tokens and would have made every prediction degenerate even after fixing the tokenizer 404. This second P0 was masked downstream by the first and discovered during emergency planning (P0 secondary fix in `src/extraction.rs::BertNerModel::load`).
- Regex prefilter for ALL_CAPS identifiers now filters Portuguese rule keywords (`NUNCA`, `SEMPRE`, `PROIBIDO`, `OBRIGATÓRIO`, `DEVE`, `JAMAIS`, etc.) and English equivalents (`NEVER`, `ALWAYS`, `MUST`, `TODO`, `FIXME`, etc.), preserving identifiers with underscores like `MAX_RETRY` and acronyms like `OPENAI`. In v1.0.19 against technical Portuguese corpora 70% of top entities were rule-keyword noise (P1 fix).
- Email entity type changed from `person` to `concept` because regex alone cannot distinguish individuals from role/list addresses (P2 fix).
- `merge_and_deduplicate` now emits `tracing::warn!` when entity count is truncated to `MAX_ENTS=30`, exposing the previously silent cap (P2 fix).
- `build_relationships` now emits `tracing::warn!` when the relationship cap `MAX_RELS=50` is hit, complementing the entity warning (P2 fix).
- `remember` now treats whitespace-only bodies (`\n\t  `) as empty for auto-extraction skipping, since `.is_empty()` alone passed pure whitespace through (P3 fix in `src/commands/remember.rs`).
- `remember` and `rename` kebab-case normalization now applies `trim_matches('-')` to strip leading and trailing hyphens, fixing rejection of inputs like `my-name-` truncated by filename length limits (P3 fix in `src/commands/remember.rs` and `src/commands/rename.rs`).

### Added
- 4 new unit tests in `src/extraction.rs` covering the stopword filter (`regex_all_caps_filtra_palavra_regra_pt`), constant identifier acceptance (`regex_all_caps_aceita_constante_com_underscore`), domain acronym acceptance (`regex_all_caps_aceita_acronimo_dominio`), and the email→concept reclassification (`regex_email_captura_endereco`).

### Changed
- `Cargo.toml` version bumped from `1.0.19` to `1.0.20`.

## [1.0.19] - 2026-04-26

### Added
- Hierarchical-recursive markdown chunking via `text-splitter = "0.30.1"` (`src/chunking.rs::split_into_chunks_hierarchical`) preserves H1/H2 boundaries and paragraph soft-boundaries for documents starting with markdown markers.
- Automatic hybrid entity extraction (`src/extraction.rs::extract_graph_auto`) combining a regex prefilter (emails, URLs, UUIDs, ALL_CAPS identifiers) with a CPU `candle` BERT NER pass (`Davlan/bert-base-multilingual-cased-ner-hrl`, ~676 MB safetensors, AFL-3.0). NER runs sliding-window with `MAX_SEQ_LEN=512` and `STRIDE=256`, capped at `MAX_ENTS=30`/`MAX_RELS=50`. The model downloads lazily on first use and falls back to regex-only on failure (graceful degradation via `tracing::warn!`).
- `remember` now invokes `extract_graph_auto` automatically when `--skip-extraction` is absent, no `--entities-file`/`--relationships-file`/`--graph-stdin` is provided, and the body is non-empty, materializing entities and `mentions` relationships before persistence.
- 15 unit tests in `src/extraction.rs` covering regex prefilter (email/URL/UUID/ALL_CAPS), IOB decoding (PER/ORG/LOC mapping, DATE discard, ORG-with-`sdk`-suffix → `tool`), `MAX_RELS` enforcement, dedup by lowercase name, and graceful fallback when the NER model is absent.
- 6 new chunking tests in `src/chunking.rs` validating `# H1` and `## H2` boundaries, 60 KB markdown documents with overlap 50, plain-text fallback, and `\n\n` paragraph soft-boundaries.

### Changed
- `Cargo.toml` adds `text-splitter = "0.30.1"` (features `markdown`, `tokenizers`) and `candle-core`/`candle-nn`/`candle-transformers = "0.10.2"` (default-features off) plus `huggingface-hub` (`hf-hub` renamed) for model downloads.
- `Cargo.toml` bumps `sqlite-vec` from `0.1.6` to `0.1.9` (DELETE fix and KNN constraint improvements) and removes six orphan dependencies (`notify`, `slug`, `toml`, `uuid`, `zerocopy`, `tracing-appender`).
- `Cargo.toml` reduces `tokio` from `features = ["full"]` to the minimal set `["rt-multi-thread", "sync", "time", "io-util", "macros"]`.
- Daemon thread footprint reduced from ~65 to ≤4 sustained threads via `RAYON_NUM_THREADS=2`, `ORT_INTRA_OP_NUM_THREADS=1`, and `ORT_INTER_OP_NUM_THREADS=1` set in `src/main.rs` before any runtime initialization.
- `--skip-extraction` flag now ships a help string documenting that it disables automatic entity/relationship extraction; the previously dormant field is reused as the user-facing toggle.

### Fixed
- `recall` now reports `DB inexistente` consistently with other subcommands via the shared `erros::banco_nao_encontrado` helper (P1-A).
- `recall --min-distance` is renamed to `--max-distance` with the legacy `min-distance` retained as alias for backward compatibility (P2-K).
- `related ''` rejects empty strings with a clear validation error rather than producing zero results silently (P2-L).
- 15+ user-facing strings in `embedder.rs`, `daemon.rs`, `paths.rs`, `tokenizer.rs`, and `commands/remember.rs` now ship Portuguese translations alongside the English originals (P2-I).
- `--name` is auto-normalized to kebab-case with a `tracing::warn!` when snake_case or CapsName inputs are detected (P2-H).
- Hidden flags `--body-file`, `--entities-file`, `--relationships-file`, `--graph-stdin`, `--metadata-file` now expose `#[arg(help = ...)]` so they appear in `--help` output (P2-G).
- `stats.memories`, `list.items`, and `health.counts.memories` are unified under the `memories_total` key across all JSON outputs (P3-E).
- `HybridSearchItem.rrf_score: Option<f64>` is now populated with the actual reciprocal-rank-fusion score instead of always returning `null` (P3-F).
- `--tz` rejection now suggests valid IANA timezones in the error message (P3-A).

## [1.0.18] - 2026-04-26

### Added
- New `parent_or_err` helper in `src/paths.rs` and four unit tests guard against malformed paths from `--db /` or empty `SQLITE_GRAPHRAG_DB_PATH`.
- New `DaemonSpawnGuard` in `src/daemon.rs` removes the `daemon-spawn.lock` file on graceful shutdown and emits a structured `tracing::info!` line when the daemon exits.
- Default environment variable `ORT_DISABLE_CPU_MEM_ARENA=1` is now set by `main.rs` before fastembed initializes, complementing the existing `with_arena_allocator(false)` mitigation against runaway RSS growth on variable-shape payloads.
- README and `README.pt-BR.md` now expose four additional `SQLITE_GRAPHRAG_*` environment variables in the runtime configuration table: `DISPLAY_TZ`, `DAEMON_FORCE_AUTOSTART`, `DAEMON_DISABLE_AUTOSTART`, `DAEMON_CHILD`.
- README and `README.pt-BR.md` now ship the four-badge cluster mandated by project rules: crates.io, docs.rs, license, Contributor Covenant.

### Changed
- `path.parent().unwrap()` removed from `src/paths.rs`, `src/daemon.rs::try_acquire_spawn_lock`, and `src/daemon.rs::save_spawn_state`; all three call sites now propagate validation errors via `parent_or_err`.
- README tagline rewritten from a 36-word paragraph to a 12-word blockquote in compliance with the documentation rule on tagline length; the duplicate paragraph above the blockquote was removed.
- README installation snippets no longer hard-code `--version 1.0.17` in eight locations across `README.md` and `README.pt-BR.md`; they now recommend `cargo install sqlite-graphrag --locked` and link to `CHANGELOG.md` for version history.

### Fixed
- CI now pins `cargo-nextest` to `0.9.114`, the newest release compatible with MSRV Rust 1.88.
- Loom tests now use the project-local `sqlite_graphrag_loom` cfg gate so Tokio dependencies are not compiled under upstream `cfg(loom)`.
- Graph relationship JSON now accepts `from`/`to` aliases and dashed relation labels, normalizing them before storage.
- macOS clippy and Windows concurrency tests now handle platform-specific errno and file-lock contention correctly.
- Graph and `related` documentation now matches the shipped CLI surface and no longer claims body-only automatic entity extraction.

## [1.0.17] - 2026-04-26

### Changed
- `remember` now accepts body payloads up to `512000` bytes and up to `512` chunks, with serial multi-chunk embeddings to keep memory bounded on real documentation corpora
- `remember --graph-stdin` now accepts one strict graph object with optional `body`, `entities`, and `relationships`, allowing a single stdin payload to store text plus explicit graph data

### Fixed
- Schema migration `V006__memory_body_limit` raises the SQLite `memories.body` CHECK constraint for existing databases, keeping the Rust limit and database constraint aligned
- `scripts/audit-remember-safely.sh` now wraps daemon cleanup, init, health, and audited `remember` calls with `/usr/bin/timeout -k 30 "${AUDIT_TIMEOUT_SECS:-1800}"`
- Testing docs now recommend timeout-wrapped long commands to reduce the risk of local hangs during slow, loom, heavy, and audit runs

## [1.0.16] - 2026-04-26

### Fixed
- `remember` now creates and migrates the default `./graphrag.sqlite` database before writing, preventing empty SQLite files and `no such table` failures in fresh directories
- `remember --graph-stdin --skip-extraction` now persists explicit graph payloads instead of silently discarding entities and relationships
- Graph payload failures now validate before writes and persist memory, chunks, entities and relationships atomically, so invalid graph input no longer leaves partial memories behind
- Graph input parsing now rejects unknown fields and validates `entity_type`, `relation` and `strength` before touching SQLite
- Agent-facing docs, LLM context files, schemas and `--help` output now align with the strict stdin/stdout JSON contract
- `scripts/test-loom.sh` now wraps long loom runs with a configurable timeout

## [1.0.15] - 2026-04-26

### Fixed
- `remember --graph-stdin` now rejects invalid JSON instead of persisting malformed payloads as memory bodies
- `remember` and `edit` now reject ambiguous body sources such as explicit `--body` together with `--body-stdin`
- Graph CRUD via `--graph-stdin` now preserves declared `entity_type` values when relationships reference existing input entities
- `graph --json` now dominates text formats such as `--format dot`, `--format mermaid`, and stats text output
- `daemon` now accepts the shared `--db` and `--json` flags so agent invocations can use the same deterministic flag surface

## [1.0.14] - 2026-04-25

### Fixed
- The official release matrix now excludes `x86_64-apple-darwin` and `x86_64-unknown-linux-musl`, which the current `ort` dependency chain does not sustain through prebuilt ONNX Runtime binaries in this project configuration
- The release workflow no longer tries to assemble a macOS universal binary from an unsupported Intel artifact
- Release and cross-platform docs now describe only the targets the project can ship consistently without a custom ONNX Runtime build

## [1.0.13] - 2026-04-25

### Fixed
- `x86_64-apple-darwin` now builds on an explicit Intel macOS runner instead of failing on an Apple Silicon host that lacks a compatible prebuilt ORT path for this target
- `x86_64-unknown-linux-musl` now builds through `cross`, providing the musl C++ toolchain required by `esaxx-rs`
- The ARM64 GNU dynamic ONNX Runtime contract and the Windows ARM64 runner requirement are now captured in the release candidate that will validate the full matrix

## [1.0.12] - 2026-04-25

### Fixed
- `aarch64-unknown-linux-gnu` now builds through a target-specific `load-dynamic` ONNX Runtime strategy instead of failing at link time on prebuilt ORT archives
- The ARM64 GNU runtime contract for `libonnxruntime.so` is now documented explicitly across release and agent-facing docs
- The release workflow now targets the official GitHub-hosted Windows ARM64 runner for `aarch64-pc-windows-msvc` instead of an incompatible x64 runner

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


## [Legacy NeuroGraphRAG]
<!-- This block predates the rename to sqlite-graphrag and is preserved for traceability -->

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
