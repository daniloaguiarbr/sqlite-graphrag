# Changelog

- Read this document in [Portuguese (pt-BR)](CHANGELOG.pt-BR.md).

All notable changes to this project will be documented in this file.


## [1.0.95] - 2026-06-27

### Added
- GAP-OR-ENRICH: new `enrich --mode openrouter` routes the JUDGE to the OpenRouter `/chat/completions` REST endpoint, so structured extraction (`memory-bindings`, `entity-descriptions`, `body-enrich`, etc.) no longer requires a locally installed `claude`/`codex`/`opencode` CLI subprocess. The SCAN→JUDGE→PERSIST pipeline is unchanged; only the JUDGE transport differs
- New module `src/chat_api.rs` (`OpenRouterChatClient`) — REST chat client mirroring `src/embedding_api.rs`: same retry/backoff policy (immediate abort on 401/400/404, `retry-after` on 429, exponential backoff + jitter on 5xx) and the same minimal headers (only `Authorization: Bearer`)
- New `enrich` flags: `--openrouter-model` (REQUIRED for `--mode openrouter`; absence is rejected with exit 1 before any network call), `--openrouter-api-key` (env `OPENROUTER_API_KEY`), `--openrouter-timeout`, `--openrouter-base-url`
- Structured Outputs: requests send `response_format` `json_schema` with `strict: true` plus `provider.require_parameters: true`, so only providers honouring the schema are routed and the model output is reliable JSON without fragile stdout parsing
- Reasoning disabled for extraction (`reasoning.enabled: false`) to cut paid tokens and latency, with a graceful reasoning-mandatory fallback: `complete()` first tries `enabled: false`, and on an HTTP 400 mentioning `reasoning` it retries ONCE omitting the `reasoning` field so the model uses its mandatory default (helper `reasoning_disable_rejected`). 9 of the 13 tested models accept `enabled: false`; 4 (`minimax/minimax-m2.7[:nitro]`, `openai/gpt-oss-120b[:nitro]`) require the fallback
- Real cost per item is read from `usage.cost` in the response (no deprecated `usage: {include:true}` parameter) and summed into the run total

### Audit Notes
- Build clean: 0 errors, 0 clippy warnings (`-D warnings`), 0 fmt diffs
- Test suite: `cargo test` exit 0, 0 failures
- E2E: `--mode openrouter` validates the API key without spawning a subprocess; all 13 OpenRouter text models exercised against the strict schema pass (13/13 compatible — 9 directly with `reasoning.enabled: false`, 4 via the reasoning-mandatory fallback)


## [1.0.94] - 2026-06-26

### Fixed
- GAP-EMBED-DIM-64: `DEFAULT_EMBEDDING_DIM` raised from 64 to 384 (`constants.rs`); `main.rs` OpenRouter eager init now uses `constants::embedding_dim()` instead of hardcoded `unwrap_or(64)`. New databases via `init` stamp `dim=384` in `schema_meta`, matching the production corpus; legacy 64-dim databases preserved via `schema_meta.dim` precedence (no forced re-embed). The 64 default was a deliberate G42/v1.0.79 choice to cut autoregressive token cost on the codex embedding path — moot now that OpenRouter REST is the default (MRL truncation is server-side)
- GAP-EMBED-TIMEOUT-300: `DEFAULT_EMBED_TIMEOUT_SECS` raised from 120 to 300 (`llm_embedding.rs`), aligning the embedding subprocess with `ingest`/`enrich`/`opencode`/`llm_backend` which already used 300 (G42/BLOCO-4 intent)
- GAP-HEADLESS-DEFAULT: `enrich --mode` is now REQUIRED (removed `default_value = "claude-code"`); omitting it is rejected by clap (exit 2), preventing accidental `claude -p` spawns that inherit the project `.mcp.json` and fail
- GAP-OR-ENTITY-EMBED: entity embedding in `remember`/`remember-batch`/`ingest` now honours `--embedding-backend`/`--llm-backend` by routing through `embed_passages_parallel_with_embedding_choice` (OpenRouter REST), with a `none`-chain short-circuit returning empty vectors without spawning a subprocess. The entity cache key is now backend-aware (`openrouter:{dim}`) to avoid collision between codex and OpenRouter vectors. `remember` with new entities drops from ~119s (codex timeout) to ~0.9s (OpenRouter REST)

### Audit Notes
- Build clean: 0 errors, 0 clippy warnings (`-D warnings`), 0 fmt diffs
- Test suite: `cargo test` exit 0, 0 failures
- E2E: `init` stamps `dim=384`; `enrich` rejects missing `--mode`; `remember` + new entity via OpenRouter = 913ms with `backend_invoked=openrouter`


## [1.0.93] - 2026-06-25

### Added
- GAP-OR-INGEST: OpenRouter embedding backend — new `--embedding-backend auto|openrouter|llm`, `--embedding-model`, `--openrouter-api-key` global flags for REST API embedding (~200ms vs 15s subprocess LLM); `EmbeddingBackendChoice` propagated to ALL 8 embedding commands (remember, remember-batch, ingest, recall, edit, restore, hybrid-search, deep-research)
- New `--enrich-after` flag for `ingest` — triggers `enrich --operation memory-bindings` sequentially after embedding phase
- New modules: `src/embedding_api.rs` (OpenRouter REST client with batch, retry, MRL truncation), `src/config.rs` (XDG config for API key), `src/commands/config_cmd.rs`
- New functions: `embed_passages_parallel_with_embedding_choice()`, `try_embed_query_with_embedding_choice()` in `embedder.rs`
- 10 OpenRouter embedding models verified E2E: `qwen/qwen3-embedding-4b`, `qwen/qwen3-embedding-8b`, `nvidia/llama-nemotron-embed-vl-1b-v2:free`, `openai/text-embedding-3-small`, `openai/text-embedding-3-large`, `perplexity/pplx-embed-v1-0.6b`, `mistralai/mistral-embed-2312`, `baai/bge-m3`, `google/gemini-embedding-001`, `google/gemini-embedding-2`
- GAP-OR-PROPAGATION fully resolved: `EmbeddingBackendChoice` propagated to all 13 embedding paths (8 original + 5 secondary)

### Fixed
- BUG-OR-1: `input_type="search_document"` hardcoded broke NVIDIA Nemotron; now per-model via `model_default_input_type()`
- BUG-OR-2: `model_supports_mrl()` missed NVIDIA and BAAI; added `llama-nemotron-embed` and `bge-m3`
- BUG-OR-3: `qwen/qwen3-embedding-0.6b` listed as approved but has no active endpoints on OpenRouter
- BUG-OR-4: `nvidia/llama-3.1-nemotron-embed-8b` listed but does not exist on OpenRouter API
- BUG-OR-5: HTTP 200 with malformed body caused immediate failure without retry; parse errors on 200 now treated as transient
- GAP-OR-PROPAGATION: 5 remaining embedding paths now honour `--embedding-backend openrouter` — `enrich --operation re-embed` (`reembed_memory_vector` + `call_reembed` + `persist_enriched_body`), `rename-entity` (entity embedding), `init` (smoke test probe), `ingest --mode claude-code` (4 call sites in `ingest_claude.rs`), `remember` chunks (`embed_passages_parallel_local` → `embed_passages_parallel_with_embedding_choice`). `EmbeddingBackendChoice` propagated from `main.rs` to all 13 embedding paths (8 original + 5 fixed)
- BUG-OR-EXIT-CODE: 3 OpenRouter config validations in `main.rs` emitted exit code 1 instead of 78 (EX_CONFIG) for config errors (`--embedding-backend openrouter` without `--embedding-model`, missing API key, client init failure). Fixed: all 3 now emit exit 78 via `ExitCode::from(78_u8)`

### Audit Notes
- Build clean: 0 errors, 0 clippy warnings, 0 fmt diffs
- Test suite: 1059 tests, 0 failures
- E2E: 10/10 OpenRouter models passed all operations (init, remember, recall, hybrid-search, edit, ingest, enrich re-embed, rename-entity)
- All gaps/bugs closed; 0 open


## [1.0.92] - 2026-06-24

### Added
- GAP-DOC-CRUD-001 through GAP-DOC-CRUD-008: 8 documentation gaps remediated across COOKBOOK, HOW_TO_USE, AGENTS, HEADLESS_INVOCATION (EN+PT-BR); CRUD expansion with new recipes for forget, restore, edit, rename, purge, cleanup-orphans, vacuum
- Skill audit: EN and PT-BR skill files updated with CRUD subcommand documentation

### Audit Notes
- Build clean: 0 errors, 0 clippy warnings, 0 fmt diffs
- All 8 doc gaps closed; 0 open


## [1.0.91] - 2026-06-23

### Fixed
- **GAP-SPAWN-001** — LLM subprocesses (`codex exec`, `claude -p`, `opencode run`) inherited the caller's CWD and `HOME`, causing `.mcp.json` walk-up that loaded project MCP servers (PostgreSQL, SSH, docs-rs) into headless embedding subprocesses. This caused 120s timeouts or 401 auth errors on every `remember`/`recall`/`ingest` in projects with `.mcp.json`. Fix: new `spawn_isolation_dir()` and `apply_cwd_isolation()` helpers in `src/spawn/mod.rs` set `current_dir` to an ephemeral temp directory and `CLAUDE_CONFIG_DIR` to the same dir, blocking both CWD and user-level MCP inheritance. Applied to all 10 production spawn sites across `llm_embedding.rs`, `codex_spawn.rs`, `claude_runner.rs`, `opencode_runner.rs`, `ingest_claude.rs` and `enrich.rs`. Default embed timeout was increased from 60s to 120s in the previous session as partial mitigation.
- **GAP-SPAWN-002** — Orphan spawn directories accumulated in `/tmp/sqlite-graphrag-spawn-{PID}/` across CLI invocations. Added `cleanup_spawn_dir()` in `main.rs` that removes the current PID's spawn directory at process exit (success, error and shutdown paths). Uses non-recursive `remove_dir()` — safe for empty directories only.
- **BUG-14** — Test `opencode_adapter_build_args` in `tests/spawn_version_adapter.rs` asserted the string `"headless"` which was never returned by `OpencodeAdapter::build_args()` (returns `"run"` since the v1.0.90 refactor). Fixed: assertion now checks for `"run"`.
- **BUG-15** — 7 JSON schemas in `docs/schemas/` declared `backend_invoked` with enum `["claude", "codex", "none"]`, missing `"opencode"` and `"auto"` values added in v1.0.90. Consumers validating against the schema would reject valid responses. Fixed: all 7 schemas updated to `["claude", "codex", "opencode", "none", "auto"]`. Affected: `embedding-status`, `enrich-summary`, `hybrid-search`, `recall`, `remember`, `ingest-summary`, `edit`.
- **BUG-16** — `deep-research.schema.json` did not declare the `vec_degraded` field in `ResearchStats`, causing `additionalProperties: false` validation to reject real output. Fixed: added `"vec_degraded": { "type": "boolean" }` to the schema and to the `required` array.
- **BUG-17** (HIGH) — `entities.degree` stored field was inflated by `increment_degree()` in `remember` and `ingest`. The function blindly incremented +1 per entity per memory, even when the entity had no relationships in that call. Also, it ran BEFORE relationship insertion, so degrees were calculated without considering the current call's relationships. `graph stats` (which uses the stored field) diverged from `graph entities` (which recalculates via SQL subquery). Fix: removed `increment_degree()` from entity loops in both `remember.rs` and `ingest.rs`; added `HashSet<i64>` collection of all affected entity IDs (entities + relationship endpoints); `recalculate_degree()` called for ALL affected entities AFTER all relationships are inserted. `graph stats`, `graph entities` and the stored field are now consistent.

### Audit Notes
- Build clean: 0 errors, 0 clippy warnings, 0 fmt diffs.
- Test suite: 877 lib tests + 21 doc tests + 38 schema contract tests, 0 failures.
- E2E audit: 90 tests across empty DB, CRUD, graph ops, search, maintenance, validation and edge cases.
- All 6 gaps/bugs closed (GAP-SPAWN-001, GAP-SPAWN-002, BUG-14, BUG-15, BUG-16, BUG-17); 0 open.


## [1.0.90] - 2026-06-22

### Added
- **GAP-OPENCODE-001** — OpenCode backend integration in the embedding and extraction pipeline. Added `Opencode` variant to `EmbeddingFlavour`, `LlmBackendKindFactory` and `LlmBackendKind` enums. New `LlmEmbeddingBuilder::opencode_default()`, `invoke_opencode_async()`, `build_opencode_embedding_command()` and `opencode_embed_model()`. Auto-detect via `which::which("opencode")`. Env vars: `SQLITE_GRAPHRAG_OPENCODE_BINARY`, `SQLITE_GRAPHRAG_OPENCODE_MODEL`, `SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL`. Fallback chain extended to `codex → claude → opencode → none`.
- **GAP-OPENCODE-002** — OpenCode backend integration in the ingest, enrich and fallback chain pipelines. New `--mode opencode` for `ingest` and `enrich`. New `src/commands/ingest_opencode.rs` and `src/commands/opencode_runner.rs` modules. New CLI flags: `--opencode-binary`, `--opencode-model`, `--opencode-timeout`. Updated `parse_fallback_chain()` to recognize `"opencode"` token. Updated `dry_run_backend` to probe opencode on PATH.
- **GAP-SKILL-OPENCODE-001** — Skills EN/PT updated with OpenCode backend documentation, env vars, CLI flags and usage examples.

### Fixed
- **BUG-AUDIT-001** — Cross-contamination of opencode model: `opencode_embed_model()` and `resolve_opencode_model()` no longer fall back to `SQLITE_GRAPHRAG_LLM_MODEL` (which could contain a codex model). Precedence now: `OPENCODE_EMBED_MODEL` > `OPENCODE_MODEL` > default `opencode/big-pickle`.
- **BUG-AUDIT-002** — Embedding prompt rewritten with role-setting "You are an embedding function" to produce real 64-dimensional vectors instead of being refused by the model.
- **BUG-AUDIT-003** — `env_clear()` in opencode invoke now preserves provider credentials (`OPENROUTER_API_KEY`, etc.) and config (`XDG_CONFIG_HOME`) via new `propagate_opencode_env()` helper.
- **BUG-AUDIT-004** — `ingest_opencode` was a stub returning `Err(Validation("under development"))`. Fully implemented with per-file extraction loop, entity/relationship persist and NDJSON event stream.
- **BUG-AUDIT-005** — Schema mismatch in `persist_memory_with_graph`: INSERT used `entity_type` instead of `type` column; missing `body_hash` NOT NULL field. Fixed to match SQLite schema.
- **GAP-ENRICH-OPENCODE-001** — `enrich --mode opencode` was silently delegating to codex headless (13 match arms). Created dedicated `call_opencode()` using `opencode_runner`.
- **BUG-AUDIT-006** — `--opencode-binary` CLI flag was declared in clap but ignored. Created `find_opencode_binary_with_override()` that honours the explicit path.
- **BUG-AUDIT-007** — `spawn_with_memory_limit()` (RLIMIT_AS 4GB) crashed the Bun runtime used by opencode. Created `spawn_opencode()` with setsid but without RLIMIT_AS.
- **BUG-AUDIT-008** — `call_opencode()` in enrich ignored `json_schema` parameter. Schema is now injected into the prompt when non-empty for structured JSON output.
- **BUG-AUDIT-009** — Preflight probe for opencode used `spawn_with_memory_limit()` (same RLIMIT_AS crash as BUG-007). Replaced with `spawn_opencode()`.
- **BUG-AUDIT-010** — `dry_run_backend` misleading error when opencode was eclipsed by codex on PATH. Differentiated message to explain priority vs absence.
- **BUG-AUDIT-011** — `--names` filter silently ignored in `entity-descriptions` and `body-enrich` operations. Added `name_filter` parameter to `scan_entities_without_description()` and `scan_short_body_memories()` with SQL `WHERE name IN (...)`.
- **BUG-SLOT-TEST-001** — Test `slot_enforces_max_concurrency` leaked `XDG_RUNTIME_DIR` causing collision with real host slots. Created `isolate_slots_env()` / `restore_slots_env()` helpers.
- **DOC-WARNING-001** — `cargo doc` warning "unresolved link to 0" in `preflight.rs:84`. Escaped brackets: `argv\[0\]`.
- **DOC-WARNING-002** — `cargo doc` warning "unclosed HTML tag path" in `ingest.rs:122`. Converted to inline code: `` `<path>` ``.
- **FMT-001** — `cargo fmt --check` formatting difference in `cli.rs:74`. Applied `cargo fmt`.
- **BUG-TIMEOUT-HARDCODE-001** — Embedding timeout hardcoded at 60s causing exit 11 on large bodies. Added `timeout_override: Option<Duration>` to `LlmEmbedding` and `LlmEmbeddingBuilder`. New instance methods `instance_embed_timeout()` and `instance_embed_timeout_for_batch()`. Removed unsafe `std::env::set_var` from `embed_batch_async()`.
- **BUG-WINDOWS-001** — Windows compilation failed: 3 uses of `std::os::unix::process::ExitStatusExt` without `#[cfg(unix)]` guard. Created `extract_exit_info()` helper with `#[cfg(unix)]` and `#[cfg(not(unix))]` branches, replacing 3 inline blocks (DRY + cross-platform).
- **BUG-PENDING-CLEANUP-DB-001** — `pending cleanup` did not accept `--db` flag. Added `db: Option<String>` to `PendingCleanupArgs` and parameterized `open_conn()`.
- **BUG-REMEMBER-BATCH-DRYRUN-001** — `remember-batch --dry-run` was not implemented (exit 2). Added `dry_run` field to `RememberBatchArgs` with preview events (`would_create`, `would_update`, `would_fail_duplicate`).
- **BUG-INGEST-SKIP-EMBED-001** — `ingest` ignored `--skip-embedding-on-failure`. Changed `StagedFile.embedding` from `Vec<f32>` to `Option<Vec<f32>>`, added skip guards at 3 embedding call sites.
- **BUG-GRAPH-DB-PROPAGATION-001** — `graph --db X stats|traverse|entities` ignored parent flags. Propagated `args.db` and `args.namespace` to subcommands when their own fields are `None`.
- **BUG-PENDING-EMBEDDINGS-DB-001** — `pending-embeddings list|abandon` did not accept `--db`. Added `db` field to both args structs and parameterized `open_conn()`.
- **BUG-LIST-TOTAL-COUNT-001** — `list` returned `total_count` equal to page size instead of global total. Created `memories::count()` with 4 query variants. `truncated` now compares `items.len() < total_count`.

### Audit Notes
- Build clean: 0 errors, 0 clippy warnings, 0 fmt diffs, 0 doc warnings.
- Test suite: 875 lib tests, 0 failures.
- All 24 gaps/bugs closed; 0 open.

## [1.0.89] - 2026-06-19

### Fixed
- **GAP-E2E-001** — Binary size documentation now matches reality. Measured the release binary at 15,321,016 bytes (14.6 MiB, 15.3 MB) and updated `Cargo.toml:6` description plus 13 prose mentions across `README.md`, `llms.txt`, `docs/AGENTS.md`, `docs/AGENTS.pt-BR.md`, `docs/HOW_TO_USE.md`, `docs/HOW_TO_USE.pt-BR.md`, `docs/MIGRATION.md`, `docs/MIGRATION.pt-BR.md`, `docs/CROSS_PLATFORM.md`, `docs/CROSS_PLATFORM.pt-BR.md`, `docs/COOKBOOK.md`, `docs/COOKBOOK.pt-BR.md`, `docs/decisions/adr-0019-llm-only-one-shot.md`, and `docs/decisions/adr-0019-llm-only-one-shot.pt-BR.md`. The old "6 MB" claim was correct for the v1.0.76 LLM-only release (rusqlite + clap only) but the binary grew as new features landed (GAP-002 split, GAP-058 env whitelist, GAP-E2E-007 schemars, schemars 0.8 derive, system-load + reaper helpers, OAUTH-only guard). The `[profile.release]` already has `lto = "fat"`, `codegen-units = 1`, `strip = true`, `opt-level = 3`, `panic = "abort"`. Regression test `tests/binary_size_documented_regression.rs::assert_documented_size_matches_real` parses the Cargo.toml description and the on-disk release binary to assert the documented size is within 1 MiB of the real size.
- **GAP-E2E-002** — `health` now accepts `--namespace <NAMESPACE>` like 30+ other subcommands. Added `pub namespace: Option<String>` to `HealthArgs` and the namespace appears in the `HealthResponse` JSON envelope. The SQL filters at `health.rs:664/691/697/703` already accepted a namespace but the CLI flag was missing. Regression test `tests/health_namespace_regression.rs::health_accepts_namespace_flag` verifies the flag is wired.
- **GAP-E2E-007** — `health` JSON schema regenerated via `schemars 0.8` derive on `HealthResponse`. Added 17 fields missing from the schema (`vec_memories_missing`, `vec_memories_orphaned`, `sqlite_version`, `mentions_ratio`, `mentions_warning`, `top_relation`, `top_relation_ratio`, `applies_to_ratio`, `relation_concentration_warning`, `super_hub_count`, `super_hub_warning`, `top_hub_entity`, `top_hub_degree`, `hub_warning`, `non_normalized_count`, `normalization_warning`, `fts_query_ok`). Switched `additionalProperties: false` → `true` (Must-Ignore policy per RFC 7493 I-JSON and `rules_rust_json_e_ndjson.md:33`). New `src/bin/dump_schema.rs` regenerates the schema idempotently via `schema_for!()` + BTreeMap ordering + recursive `apply_must_ignore` policy enforcement. ADR-0048 (en + pt-BR) documents the Must-Ignore decision and the schemars 0.8 adoption. Regression test `tests/health_schema_drift_regression.rs::assert_all_health_keys_in_schema` validates the schema contains all 36 properties. **BREAKING CHANGE**: consumers using strict mode (`additionalProperties: false`) must migrate to Must-Ignore to receive schema-evolution benefits.
- **GAP-E2E-008** — `--db` flag parity restored for `embedding status`, `embedding list`, `embedding abandon`, `pending list`, and `pending show`. The decision NOT to use `clap::Arg::global = true` (which would propagate the flag globally and break the per-subcommand convention) is documented in ADR-0049 (en + pt-BR). Regression test `tests/cli_db_flag_parity_regression.rs::assert_db_flag_on_all_namespace_subcommands` validates 5 subcommands accept `--db`.
- **GAP-E2E-009** — `migrate --dry-run --json` now returns a structured report (`pending_migrations[]`, `pending_count`, `checksum_mismatches[]`, `status`) without mutating the schema. Also added `--confirm` (FALTA-7): the default migration runner waits for the literal string `yes` on stdin before applying migrations. Backward compatible: CI scripts that gate via `migrate --status` first continue to work. Regression test `tests/migrate_dry_run_regression.rs::dry_run_does_not_mutate_schema_history` confirms schema_version is unchanged after dry-run.
- **GAP-E2E-010** — `codex-models --json` now returns the JSON envelope `{"action":"codex_models","count":N,"default":"...","models":[...]}`. `pending list --db` and `pending show --db` accept `--db` (consolidated with GAP-E2E-008). Regression tests `tests/codex_models_json_regression.rs::codex_models_json_flag_accepted_as_noop` and `tests/cli_db_flag_parity_regression.rs` validate both.
- **GAP-E2E-011** — `ingest` description no longer hardcoded `"ingested from <path>"`. New `src/commands/ingest_heuristics.rs::extract_heuristic_description(body, path_hint)` extracts the first meaningful line (>20 chars, non-Markdown-header) truncated to 100 chars. FALTA-6 edge case (body with only Markdown headers) now falls back to the file stem (e.g. `"headers-only"`) instead of the generic `"ingested document"` placeholder. New `--no-auto-describe` flag restores the legacy `"ingested from <path>"` behavior when needed (closes the help-first drift ADR-0034 detected in v1.0.88 where `--auto-describe` promised a flag that didn't exist). Regression test `tests/ingest_auto_describe_regression.rs` validates 5 scenarios including the new fallback-to-stem path.
- **GAP-CODEX-BINARY** — Added `--codex-binary` global flag with env var `SQLITE_GRAPHRAG_CODEX_BINARY`, symmetric with `--claude-binary`. `detect_available()` in `llm_embedding.rs` now honours the env var for PATH override.
- **GAP-FLAGS-MORTAS** — 7 global LLM flags (`--claude-binary`, `--codex-binary`, `--llm-model`, `--skip-embedding-on-failure`, `--llm-max-host-concurrency`, `--llm-slot-wait-secs`, `--llm-slot-no-wait`) now propagated from CLI to env vars via `std::env::set_var` in `main.rs` before command dispatch. Fixes silent ignore when flags were passed via CLI instead of env vars.
- **GAP-BACKEND-PROPAGATION** — `deep-research` and `remember-batch` now receive and USE `llm_backend` parameter. Previously the parameter was accepted but prefixed with underscore (`_llm_backend`) and ignored. `--llm-backend claude` is now honoured by both commands.
- **GAP-ADAPTIVE-TIMEOUT** — Added `embed_timeout_for_batch(batch_size)` that scales: base + 15s per additional item. `embed_batch_async()` now uses adaptive timeout. Batch of 1 item = 60s; batch of 8 items = 165s.
- **GAP-OAUTH-HINT** — `invoke_claude()` now detects OAuth expiration patterns in stderr ("401", "Unauthorized", "expired", "login") and adds actionable hint: "Claude OAuth token may be expired; run `claude login` to renew".
- **GAP-MODEL-HARDCODE** — Removed hardcoded model defaults. `codex_embed_model()` and `claude_embed_model()` now consult `SQLITE_GRAPHRAG_LLM_MODEL` as fallback and emit warning when no model is configured.
- **GAP-META-006** — Eliminated 4 hardcoded "codex" defaults: `LlmExtractorConfig::default()` now uses `detect_available_backend()` for runtime resolution; `composite_backend::default_backend()` and `backend_from_kind()` now resolve dynamically instead of calling `with_default_codex()`; `remember_batch` and `deep_research` now propagate `llm_backend` to embedding calls.
- **BUG-SKIP-EMBED** — `--skip-embedding-on-failure` was a dead flag: accepted by clap, propagated to env var in `main.rs`, but NEVER read by any embedding module. Added `should_skip_embedding_on_failure()` and `embed_passage_or_skip()` in `embedder.rs` that read `SQLITE_GRAPHRAG_SKIP_EMBEDDING_ON_FAILURE` and return `Ok(None)` instead of exit 11 when the flag is active. `AppError::Validation` (OAuth-only enforcement) remains fatal even with the flag.
- **GAP-EMBED-PROPAGATION** — 7 call sites in `init.rs`, `ingest_claude.rs` (4 sites), `rename_entity.rs`, and `restore.rs` used `embed_passage_local` which ignores `--llm-backend`. All replaced with `embed_passage_with_choice` that honours the user's backend selection via env var propagation.
- **GAP-WITH-DEFAULT-CODEX** — `LlmBackend::with_default_codex()` marked `#[deprecated(since = "1.0.89")]`. 6 test callers in `tests/extract_backend.rs` migrated to `LlmBackend::new(LlmExtractorConfig::default())`. The method now delegates to `Default` which resolves the backend dynamically via `detect_available_backend()`.
- **BUG-MODEL-VAZIO** — `codex_embed_model()` and `claude_embed_model()` returned empty string when no env var was set, causing codex to fail with "The '' model is not supported". Fixed with sensible defaults: `gpt-5.5` for codex, `claude-sonnet-4-6` for claude.
- **BUG-SKIP-EMBED-INCOMPLETE** — The previous BUG-SKIP-EMBED fix created `embed_passage_or_skip()` with ZERO callers. The `remember` command called `embed_passage_with_choice()` directly with `?`, propagating errors without checking `should_skip_embedding_on_failure()`. Fixed by wrapping all 3 embedding call sites in `remember.rs` (passage, parallel chunks, entity texts) with skip-on-failure error guards. `embedding` changed from `Vec<f32>` to `Option<Vec<f32>>`, with `upsert_vec` conditioned on `Some`.
- **BUG-BUILDER-ENV-VAR** — `LlmEmbeddingBuilder::build()` did not read `SQLITE_GRAPHRAG_CLAUDE_BINARY` or `SQLITE_GRAPHRAG_CODEX_BINARY` env vars. When `--llm-backend claude` was forced, the builder called `which::which("claude")` ignoring the `--claude-binary` override propagated via `set_var`. Fixed: `build()` now reads the env var before falling back to `which::which`. Precedence: `binary_override` > env var > `which::which`.
- **BUG-BATCH-STATUS** — `remember-batch` returned `status: "indexed"` for all items regardless of whether the memory was created or updated. Fixed: now returns `"created"` for new memories and `"updated"` for force-merged existing memories. Aligns with the documented contract (`created`/`updated`/`skipped`/`failed`).
- **BUG-BATCH-SKIP-EMBED** — `remember-batch` did not honour `--skip-embedding-on-failure`. The 3 embedding call sites (passage update, passage create, entity texts) used `?` directly, propagating errors without checking `should_skip_embedding_on_failure()`. Fixed with match guards identical to the `remember` command fix (BUG-SKIP-EMBED-INCOMPLETE).
- **BUG-BOOLISH-ENV** — 4 boolean CLI flags with `env = "SQLITE_GRAPHRAG_*"` rejected standard Unix values (`1`, `yes`, `on`) with exit 2. Root cause: `bool` field with `env = "..."` in clap uses `bool::from_str` which accepts ONLY `"true"` and `"false"`. Fixed by adding `value_parser = clap::builder::BoolishValueParser::new()` to `--skip-embedding-on-failure`, `--strict-env-clear`, `--dry-run-backend`, and `--llm-slot-no-wait`. Now accepts `1`/`0`/`true`/`false`/`yes`/`no`/`on`/`off`.
- **BUG-RESTORE-BACKEND** — `restore` ignored `--llm-backend` (hardcoded `None`) and did not honour `--skip-embedding-on-failure`. Fixed: signature now receives `LlmBackendChoice`, embedding wrapped with skip-on-failure match guard, `upsert_vec` conditional on `Some(embedding)`.
- **BUG-RENAME-ENTITY-BACKEND** — `rename-entity` ignored `--llm-backend` (hardcoded `None`) and did not honour `--skip-embedding-on-failure`. Fixed: same pattern as `restore`.
- **BUG-EDIT-SKIP-EMBED** — `edit` did not honour `--skip-embedding-on-failure`. Embedding call used `?` directly, causing exit 11 when LLM failed instead of persisting without embedding. Fixed: wrapped with match guard + `should_skip_embedding_on_failure()`, `upsert_vec` conditional on `Some(embedding)`.
- **BUG-STRICT-ENV-PROPAGATION** — `--strict-env-clear` CLI flag was silently ignored. The flag set `cli.strict_env_clear = true` but `env_whitelist.rs` reads `std::env::var("SQLITE_GRAPHRAG_STRICT_ENV_CLEAR")` which was never set. Fixed: `main.rs` now propagates the flag via `set_var` before command dispatch.
- **BUG-BATCH-FTS-DESYNC** — `remember-batch --force-merge` updated memory rows without calling `sync_fts_after_update`. The FTS5 AFTER UPDATE trigger is intentionally absent (sqlite-vec conflict), so UPDATE operations must sync FTS manually. `remember` did this correctly; `remember-batch` omitted it. Fixed: added old-value capture + `sync_fts_after_update` call in the force-merge path, matching `remember.rs`.
- **BUG-FORGET-DOUBLE-DELETE-VEC** — `forget` called `delete_vec` twice for a successful soft-delete: once before `soft_delete` (line 94, G39 Passo 4) and again after (line 135, inside `if forgotten`). The second call was redundant and produced spurious log warnings. Fixed: removed the duplicate call.
- **BUG-ENRICH-DESC-FTS-DESYNC** — `enrich --operation description-enrich` updated the `description` column via raw SQL without calling `sync_fts_after_update`. The FTS5 AFTER UPDATE trigger is intentionally absent, so the FTS index became stale after description enrichment. Fixed: added `sync_fts_after_update` call after the UPDATE in `call_description_enrich`.
- **BUG-ENRICH-BODY-EXTRACT-FTS-DESYNC** — `enrich --operation body-extract` updated the `body` column via raw SQL without calling `sync_fts_after_update`. Same root cause as BUG-ENRICH-DESC-FTS-DESYNC. Fixed: added `sync_fts_after_update` call after the UPDATE in `call_body_extract`.
- **GAP-LLM-FALLBACK-DEAD-FLAG** — `--llm-fallback` (default `codex,claude,none`) was accepted by clap and displayed in `--dry-run-backend` but NEVER used by the real embedding pipeline. `to_chain()` in `LlmBackendChoice::Auto` used a hardcoded chain. Fixed: `main.rs` now propagates `--llm-fallback` via `set_var`; `to_chain()` for `Auto` reads `SQLITE_GRAPHRAG_LLM_FALLBACK` via new `parse_fallback_chain()` which parses the CSV string into `Vec<LlmBackendKind>`. Unknown tokens emit `tracing::warn!` and are skipped; empty chain falls back to the canonical `[Codex, Claude, None]`.
- **BUG-YES-FLAG-IGNORED** — Three destructive commands (`slots release`, `purge`, `cleanup-orphans`) declared `--yes` in clap but never enforced it: `slots` printed a warning then deleted anyway, `purge` never checked the field at all, `cleanup-orphans` printed progress then deleted. All other destructive commands (prune-ner, normalize-entities, vec purge, prune-relations, cache clear) correctly abort without `--yes`. Fixed: all three now return `AppError::Validation` when `--yes` is absent, matching the project convention.
- **GAP-RECALL-001** — recall and hybrid-search embedding deadlock: stdin is now dropped before wait_with_output, the per-call embedding timeout was reduced from 300s to 30s, stale slots are cleaned up via the reaper, orphaned sqlite-graphrag processes are reaped, and embedding telemetry is surfaced in the health response. See ADR-0050.
- **GAP-DEEPRESEARCH-001** — deep-research now degrades gracefully: the hard-fail `embed_query_local()` was replaced with `try_embed_query_with_deterministic_fallback()`, sub-queries accept an `Option<&[f32]>` embedding and fall back to FTS5-only when the LLM is unavailable, and a `vec_degraded` field was added to `ResearchStats`.
- **GAP-JSON-FLAG-001** — Seven subcommands (`pending list`, `embedding status`, `embedding list`, `embedding abandon`, `slots status`, `pending-embeddings list`, `pending-embeddings abandon`) now accept `--json` as a hidden no-op flag, preventing exit 2 when operators pass the standard flag.
- **GAP-INIT-EMBEDDING-001** — `init` no longer exits with an error when LLM embedding is unavailable: the smoke-test failure is caught via match instead of `?` propagation, status returns `"ok_no_embedding"` with the dim from `constants::embedding_dim()`, and the schema, tables, FTS5 and schema_meta are always created.
- **GAP-LATENCY-001** — Documentation only, not a bug: documented the intrinsic ~30-50s per codex exec embedding call as the fixed cost of ~11K system context tokens, with workarounds `--llm-parallelism 8`, `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS=120`, `--llm-backend claude`, and the dim=64 migration via `enrich --operation re-embed`.

### Audit Notes
- Build clean: 0 errors, 0 clippy warnings, 0 fmt diffs.
- Test suite: 847 lib tests + 1013 integration tests + 21 doc tests = **1881 tests, 0 failures, 7 ignored**.
- Binary size: 15,323,128 bytes (14.61 MiB) — within 1 MiB of documented 14.6 MiB.
- Working tree baseline preserved via tag `v1.0.88-baseline-2026-06-19` for rollback.

## [1.0.88] - 2026-06-19

### Fixed

- **BUG-1** — `check_claude_config_dir` now inspects `settings.json`
  semantically. The preflight guard rejects the directory only when
  `settings.json` declares a non-empty `mcpServers` or `hooks` map.
  A populated directory with `CLAUDE.md`, `commands/`, `skills/`,
  or empty `settings.json` is accepted with a structured warning.
  Fixes 9 integration tests in `entity_validation_integration`,
  `graph_traverse_regression`, and `recall_distance_integration`
  that regressed in v1.0.87 when the env var points at a real
  Claude Code install (`/home/comandoaguiar/.claude01`).
- **BUG-2** — `LlmEmbedding::invoke_claude` now writes the empty MCP
  config to a tempfile via `write_empty_mcp_config_tempfile()` and
  runs `preflight_check` before spawn, mirroring `invoke_codex`.
  The inline `--mcp-config '{}'` form was rejected by Claude Code
  2.1.177 (ADR-0045 Bug 2).
- **BUG-3** — `enrich::run_preflight_probe` (Claude Code arm) now
  writes the empty MCP config to a tempfile instead of passing
  the literal `{}` that ADR-0045 documents as broken.
- **BUG-5** — `check_mcp_config_path` now detects the
  `--mcp-config=PATH` single-slot form alongside the GNU
  `--mcp-config <PATH>` form.
- **BUG-6** — All 5 sites of `std::process::exit(16)` in
  `claude_runner.rs`, `codex_spawn.rs`, and `ingest_claude.rs`
  replaced with `Err(crate::errors::AppError::from(e))`. The
  `PreFlightError` variant name, structured tracing context, and
  PT-BR i18n are now preserved.
- **BUG-7** — `LlmEmbedding::invoke_codex` now propagates the
  preflight error directly via the `From<PreFlightError>` impl
  instead of wrapping it in `LlmBackendError::SpawnFailed`. The
  canonical exit code 16 path is restored.
- **BUG-9** — `check_walkup_mcp_json` now performs semantic
  validation: a syntactically valid `.mcp.json` that declares a
  non-empty `mcpServers` object is rejected.
- **BUG-10** — `AppError::PreFlightFailed` now stores
  `source: Box<PreFlightError>` instead of `detail: String`. The
  structured variant name is preserved end-to-end so operators
  can route on `BinaryNotFound`, `ArgvExceedsArgMax`, etc.

### Added

- `impl From<PreFlightError> for AppError` in `src/errors.rs`.
- `claude_embedding_config_dir()` integration with preflight via
  `claude_embedding_config_dir()` managed dir at
  `~/.local/state/sqlite-graphrag/claude-empty-config`.
- **`ADR-0046`** (`docs/decisions/adr-0046-preflight-remediation.md`)
  documents the 8 bugs and the fixes.

### Changed

- `build_claude_command` and `build_codex_command` return
  `Result<Command, AppError>` instead of `Command`.
- `Cargo.toml`: version `1.0.87` → `1.0.88`.

### Test Suite

- 9 integration tests restored to green:
  `entity_name_too_short_rejected_via_link`,
  `entity_name_all_caps_short_normalized_via_link`,
  `entity_name_valid_passes_via_link`,
  `rename_entity_rejects_short_new_name`,
  `rename_entity_rejects_all_caps_short_new_name`,
  `test_p0_7_traverse_nonexistent_entity_exits_4`,
  `test_traverse_valid_entity_exits_0`,
  `test_traverse_nonexistent_namespace_exits_4`,
  `graph_matches_have_nonzero_distance_after_v1025`
  (located in `tests/recall_distance_integration.rs:66` — the
  `tests/mcp_wiring_regression.rs` referenced in the audit does
  not exist).


### Fixed (audit followup)

- **BUG-11 CRITICAL** — `src/embedder.rs` now invokes `preflight_check`
  before `Command::spawn()` in the LLM embedding pipeline. The previous
  bypass meant a populated `CLAUDE_CONFIG_DIR` (e.g. a real Claude Code
  install at `/home/comandoaguiar/.claude01`) was accepted by the
  embedding path while rejected by the other 3 spawners, producing
  inconsistent behavior. Restores parity with `claude_runner.rs`,
  `codex_spawn.rs`, and `ingest_claude.rs`.
- **BUG-12 MEDIUM** — `src/output.rs:141` (`output::emit_error`) drops
  the redundant `eprintln!` call. `tracing::error!` alone now renders
  the OAuth-only enforcement violation to stderr. Stderr emits exactly
  1 line per violation (was 2). Validated by
  `oauth_stderr_emits_single_line_v1088`.
- **BUG-13 MEDIUM** — `src/commands/link.rs` now rejects ALL_CAPS
  abbreviations of 4 characters or less at the link layer (was
  previously accepted despite the entity validator rejecting them).
  Restores symmetry with `remember --graph-stdin` and
  `ingest --mode claude-code` paths.
- **BUG-AUDIT-2 LOW** — `src/constants.rs` lines 439/441/444 doc-drift
  comment updated: schema version reference `49 → 50` and
  `9 → 15` to match `CURRENT_SCHEMA_VERSION = 15` recorded in the
  build. The `Decimal = 50` byte of `50` and `15` ASCII character
  count are now consistent with `serde_json` constants.
- **BUG-AUDIT-3 LOW** — `link` and `unlink` subcommands now respect
  the root-level `--wait-lock SECONDS` flag (default 30s) and emit
  a `tracing::info!` diagnostic when the lock acquisition exceeds 5s.
  Cold-start callers (e.g. CI in a fresh namespace) should pass
  `--wait-lock 60` for headroom.

### Added (audit followup)

- **`ADR-0047`** (`docs/decisions/adr-0047-stderr-deduplication.md`)
  documents the BUG-12 + GAP-15 stderr deduplication decision.
- `tests/oauth_stderr_emits_single_line_v1088.rs` regression test
  (regression coverage for BUG-12).
- `tests/slots_no_println_integration.rs` regression test
  (regression coverage for GAP-15).

## [1.0.87] - 2026-06-19

### Added

- **GAP-META-005 closed** — `src/spawn/preflight.rs` module (≥200 lines)
  with `PreFlightArgs` struct and `PreFlightError` enum (8 variants).
  Acts as a mandatory gate before `Command::spawn()` in the 4 real
  subprocess spawn sites: `claude_runner.rs:255`, `codex_spawn.rs:273`,
  `ingest_claude.rs:297`, `extract/llm_embedding.rs:670`.
- **AppError::PreFlightFailed** variant with exit code 16, `is_permanent=true`,
  and bilingual i18n messages (EN + PT-BR).
- **`write_empty_mcp_config_tempfile()`** helper writes `{"mcpServers":{}}`
  to a tempfile so `--mcp-config <PATH>` substitution works (Bug 2 fix).
- **`is_skipped()`** opt-out via `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` for
  emergencies (emits structured warning).
- **15 unit tests** in `src/spawn/preflight.rs::tests` covering all
  7 guards + integration paths.
- **`ADR-0045`** (`docs/decisions/adr-0045-preflight-validation-layer.md`)
  documents the architectural decision.

### Fixed

- **Bug 1** — `ingest --extraction-backend llm` no longer silently
  extracts `entities:0`; preflight tracing emits `preflight_passed` so
  operators can verify the spawn was invoked.
- **Bug 2** — `--mcp-config '{}'` literal no longer rejected by Claude
  Code 2.1.177 with "Invalid MCP configuration"; spawners now substitute
  a tempfile path containing `{"mcpServers":{}}`.
- **Bug 3** — argv > `ARG_MAX - 4096` no longer fails with `E2BIG`
  post-fork; preflight detects the overflow before `cmd.spawn()` and
  aborts with structured error.
- **Bug 4** — downstream JSON parser no longer truncates silently
  at 65.536 chars; preflight validates `expected_output_bytes` against
  the documented 65 KiB cap.
- **Bug 5** — `.mcp.json` walk-up from parent directories no longer
  causes Zod validation failures mid-spawn; preflight walks up to 16
  levels from `workspace_root` and rejects invalid files BEFORE fork.


## [1.0.85.2] - 2026-06-17

### Fixed
- `--dry-run-backend` now works standalone without a required subcommand. Fixed BUG-001 (ADR-0044) via `pub command: Option<Commands>` at `src/cli.rs:248`. Exit 0 prints JSON with `{action, backend, binary, model, flavour, chain, strict_env_clear}`.
- `embed_via_backend` retorna `Result<(Vec<f32>, LlmBackendKind), AppError>` propagando `resolved_kind`. Resolvido BUG-002 (ADR-0044). 7 envelopes JSON (edit, embedding-status, enrich-summary, hybrid-search, ingest-summary, recall, remember) agora populam `backend_invoked: "claude" | "codex" | "none"` consistentemente.
- `setup_mock_path()` em `tests/embedder.rs:37-77` corrigido para emitir JSON alinhado com expectation (não JSONL). Resolvido BUG-003 (ADR-0044). Testes `embed_via_backend_*` rodam sem mascaramento de formato.

### Test Suite
- 945 testes verdes via `cargo nextest -P ci`.

## [1.0.85.1] - 2026-06-17

### Fixed
- `recall --llm-backend none` and `hybrid-search --llm-backend none` now return exit 0 with envelope `vec_degraded: true` + `source: "fts_fallback"` + `vec_degraded_reason: "dim_zero"`. Fixed GAP-004 (ADR-0043 hotfix) via intermediate branch at `src/embedder.rs:351`. v1.0.80 failsafe restored for the --llm-backend none case.

### Test Suite
- 945 testes verdes via `cargo nextest -P ci`.

## [1.0.85] - 2026-06-17

### Fixed
- `FallbackReason` extended from 3 to 7 variants (`SlotExhausted`,
  `OAuthQuota { backend }`, `BackendMismatch { requested, resolved }`,
  `DimZero`) so `recall` / `hybrid-search` discriminators can
  distinguish quota exhaustion from slot exhaustion from structural
  bugs. Resolves GAP-003.
- `LlmEmbedding::invoke_claude` now captures 12-14
  `anthropic-ratelimit-*-remaining` headers BEFORE checking the
  subprocess exit status. When `requests-remaining=0` or
  `tokens-remaining=0`, returns `OAuthQuota` so the deterministic
  fallback swaps to codex immediately. Resolves G45-CR5.
- `try_embed_query_with_deterministic_fallback` retries with the
  alternative backend on `OAuthQuota` (codex ↔ claude) and sleeps
  750ms before giving up on `SlotExhausted`. Resolves G58.

### Added
- `classify_embedding_error` in `src/embedder.rs` — pure-function
  mapping from `AppError` to `FallbackReason` via lexical match.
- `try_embed_query_with_deterministic_fallback` in `src/embedder.rs`.
- 5 new regression tests in `tests/embedder.rs` covering GAP-003,
  G58, G45-CR5, G55, G56.
- ADR `adr-0043-five-gap-remediation.md` (EN + pt-BR).
- `.github/workflows/embedder-ignore.yml` running `#[ignore]` tests
  in a hermetic env (without API keys).

### Changed
- `Cargo.toml`: version `1.0.84` → `1.0.85`.
- `gaps.md`: 5 entries marked as `Solucionado em v1.0.85 (ADR-0043)`.
- `src/embedder.rs:289-317`: `acquire_llm_slot_for_embedding` rewrites
  `LockBusy` as `Embedding("slot exhausted: ...")` so `classify_embedding_error`
  can discriminate.
- `src/commands/{hybrid_search,recall}.rs`: call sites now use
  `try_embed_query_with_deterministic_fallback`.

### Test Suite
- 5 new tests in `tests/embedder.rs` (regressão five-gap).
- 0 regressões em 830+ testes pré-existentes (`cargo nextest -P ci`).


## [1.0.84] - 2026-06-17

### Fixed
- `--llm-backend claude` now forces invocation of the `claude` binary
  without the silent fallback to `codex` via `LlmEmbedding::detect_available`.
  The `LlmBackendKind::Claude` arm in `embed_via_backend` now delegates
  to the new `embed_via_claude_local` which constructs
  `LlmEmbedding::with_claude_builder()` directly. Resolves GAP-002.

### Added
- `embed_via_claude_local` entry point in `src/embedder.rs`.
- `LlmEmbeddingBuilder` in `src/extract/llm_embedding.rs` with
  `with_claude_builder`, `with_codex_builder`, `override_binary`,
  `override_model`.
- `backend_invoked` field in 7 JSON envelopes: `embedding status`,
  `remember`, `edit`, `ingest`, `recall`, `hybrid-search`, `enrich`.
- `vec_degraded_reason` field in `hybrid-search` and `recall`.
- Global flag `--dry-run-backend` that resolves and prints the backend
  without executing the subprocess.
- Helper `apply_env_whitelist_for_claude` in `src/spawn/env_whitelist.rs`.
- `LlmBackendKind::as_str` and `FallbackReason::reason_code` in
  `src/embedder.rs`.
- ADR `adr-0042-claude-backend-split.md` (EN + pt-BR).
- 5 new tests in `tests/embedder.rs` (GAP-002 regression).

### Changed
- `Cargo.toml`: version `1.0.83` → `1.0.84`.
- `src/embedder.rs:435-444`: `LlmBackendKind::Claude` arm calls
  `embed_via_claude_local` instead of `embed_passage_local`.
- `src/embedder.rs:205-218`: `embed_passage_with_choice` returns
  `(Vec<f32>, LlmBackendKind)` instead of `Vec<f32>`.
- `src/commands/embedding.rs:run_status` accepts `LlmBackendChoice`.
- `src/main.rs:391`: `Commands::Embedding(args)` propagates
  `cli.llm_backend`.

### Test Suite
- 5 new tests in `tests/embedder.rs` (GAP-002 regression).
- 0 regressions in 818+ pre-existing tests (cargo nextest -P ci).

## [1.0.83] - 2026-06-17

### Fixed
- `claude_runner`, `codex_spawn` and `ingest_claude` now preserve custom-provider credentials (`ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, `OTEL_EXPORTER_OTLP_ENDPOINT`) in the subprocess environment. Enables use of Anthropic-compatible providers (MiniMax/api.minimax.io, OpenRouter, corporate gateways) without altering the OAuth-only mandate that continues to reject `ANTHROPIC_API_KEY`/`OPENAI_API_KEY`. Resolves partially the gap G58 (`recall`/`hybrid-search` fallback under OAuth fatigue).

### Added
- New helper module `src/spawn/env_whitelist.rs` consolidating the duplicated whitelist logic across three spawners. Exposes `apply_env_whitelist(cmd, strict)` and `is_strict_env_clear()`.
- New global flag `--strict-env-clear` (env: `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1`) for compliance environments that forbid credential forwarding via env vars. Strict mode preserves only `PATH`.
- Orientative marker arg `--oauth-only-resolution-use-anthropic-auth-token` (claude) and `--oauth-only-resolution-use-codex-auth-json-or-openai-base-url` (codex) surfaced via the diagnostic pipeline when the OAuth-only guard fires.
- New integration tests in `tests/claude_runner_env.rs` (5 scenarios) covering custom-provider propagation, OAuth-only abort, codex base-url inheritance, strict-mode credential dropping, and audit-of-no-token-leak.
- New ADR `adr-0041-preserve-custom-provider-env.md` (EN + pt-BR) justifying the architectural change.

### Changed
- `Cargo.toml`: version `1.0.82` → `1.0.83`
- `src/commands/claude_runner.rs`: removed local `ENV_WHITELIST`/`ENV_WHITELIST_WINDOWS` constants; now delegates to `apply_env_whitelist()`.
- `src/commands/codex_spawn.rs`: removed inline whitelist array (lines 277-293 prior); now delegates to `apply_env_whitelist()`. `CODEX_HOME` isolation is preserved as a runtime override after the helper call.
- `src/commands/ingest_claude.rs`: removed inline whitelist arrays; now delegates to `apply_env_whitelist()`.

### Test Suite
- 3 unit tests in `src/spawn/env_whitelist.rs` (`whitelist_includes_custom_provider_vars`, `whitelist_excludes_api_key_vars`, `strict_mode_drops_credentials`).
- 5 integration tests in `tests/claude_runner_env.rs` (hermetic, no network).
- 0 regressions in 807+ pre-existing tests (8 serial OAuth-only tests remain green).


## [1.0.82] - 2026-06-15

### Added
- **GAP-001 — Persistência por estágios**: nova tabela `pending_memories` (V014) com 6 transições de status e DAO em `src/storage/pending_memories.rs` (10 funções públicas). Subcomando `pending` com `list/show/cleanup` (`src/commands/pending.rs`).
- **GAP-002 — Shutdown JSON envelope**: handler cross-signal (`SIGINT` via `ctrlc`, `SIGTERM`/`SIGHUP` via `signal-hook`) emite envelope JSON para stdout antes de exit com `code: 19` (`SHUTDOWN_EXIT_CODE`) determinístico. 3 testes em `src/signals.rs` (`handler_source_has_no_panicking_io`, `envelope_uses_shutdown_exit_code`, `shutdown_exit_code_is_19`).
- **GAP-003 — Escolha de backend LLM**: flag global `--llm-backend <auto|claude|codex|none>` (env: `SQLITE_GRAPHRAG_LLM_BACKEND`). Trait `LlmBackendFactory` com 4 implementações (`CodexFactory`, `ClaudeFactory`, `NullFactory`, `AutoFactory`) e 3 testes em `factory_tests`.
- **GAP-004 — Slot semaphore cross-process**: novo módulo `src/llm_slots.rs` com RAII guard via `fs4::FileExt::try_lock_exclusive`. `acquire_llm_slot_for_embedding()` integrado em `embedder.rs:embed_passage_local` e `embed_query_local`. Subcomando `slots` com `status/release/cleanup` (`src/commands/slots.rs`).
- **GAP-005 — Stderr capture + fallback chain**: enum `LlmBackendError` com 4 variantes tipadas (NonZeroExit com stdout_tail/stderr_tail/exit_code/signal/hint, SpawnFailed, Timeout, NoBackendsAvailable). Tabela `EXIT_CODE_HINTS` com 9 exit codes (1, 2, 101, 126, 127, 134, 137, 139, 143). Função `embed_with_fallback(backends, skip_on_failure)` em `src/embedder.rs`. Tabela `pending_embeddings` (V015) e 2 subcomandos: `embedding` (status/list/abandon) e `pending-embeddings` (list/abandon).
- **5 ADRs novos**: `adr-0036-pending-memories-staging`, `adr-0037-shutdown-json-envelope`, `adr-0038-llm-backend-user-choice`, `adr-0039-llm-host-slot-semaphore`, `adr-0040-stderr-capture-fallback-chain` (todos bilíngues EN + pt-BR).
- **5 JSON schemas novos**: `slots-status.schema.json`, `pending-list.schema.json`, `embedding-status.schema.json`, `embedding-list.schema.json`, `shutdown-envelope.schema.json`. Indexados em `docs/schemas/README.md`.

### Changed
- `Cargo.toml`: version `1.0.81` → `1.0.82`
- `src/constants.rs::CURRENT_SCHEMA_VERSION`: `13` → `15` (V014+V015)
- `Cargo.toml`: adicionado `signal-hook = { version = "0.3", default-features = false, features = ["iterator"] }` para cobertura cross-platform de SIGTERM/SIGHUP
- `src/errors.rs`: nova variante `AppError::Shutdown { signal: String }` mapeando para exit 19
- `src/i18n.rs`: nova função `pt::shutdown(signal)` para tradução PT-BR
- `src/main.rs:392-403`: branch de shutdown usa `SHUTDOWN_EXIT_CODE = 19`
- `gaps.md`: 5 gaps marcados como `Solucionado em v1.0.82` com referências específicas a cada ADR e arquivo

### Test Suite
- 807 testes passando, 0 falhando, 1 ignorado (G58 S1 stub)
- 2 testes a mais que v1.0.81 (805 → 807) com a adição dos 3 subcomandos novos

## [1.0.81] - 2026-06-14

## [Unreleased]
### Fixed (G80b, A2.1 v1.0.80)
- `codex-models` was returning 9 entries in the `models[]` array (including metadata keys like `client_version`, `etag`, `fetched_at`) when the official Codex CLI cache file `~/.codex/models_cache.json` was in the standard shape `{"models": [{"slug": "..."}, ...]}`. The output now correctly extracts only the slug entries from the `models` array and falls back to direct keys only when the array is absent. Regressed by G33 in v1.0.69 when the static whitelist became the seed for cache merging.


## [1.0.80] - 2026-06-14

### Library API Changes (per ADR-0032, G53 v1.0.80)

The library API is **unstable** within v1.x.y. This release is a **patch** bump, so the lib surface changes below are strictly **additive** — no re-export was removed, no public struct field was renamed, no function signature was changed. The published `sqlite-graphrag = "^1.0"` shorthand keeps consumers on the CLI-stability track by default.

Newly public in 1.0.80 (additive, non-breaking):

- `crate::embedder::embed_entity_texts_cached(models_dir, texts, parallelism) -> Result<(Vec<Vec<f32>>, EmbedCacheStats), AppError>` — G56 in-process cache for entity embeddings, keyed by `(model, text)`. Returns a stats snapshot with `requested`, `hits`, `misses`, and a `hit_rate() -> f64` helper.
- `crate::embedder::EmbedCacheStats` (struct) — G56 stats snapshot; `Default`, `Copy`, `Serialize`. Re-export is necessary because callers route results from `remember` and `ingest` into their own telemetry.
- `crate::embedder::EntityEmbedCacheMap` (type alias) — G56 internal `HashMap<u64, Arc<Vec<f32>>>`; exposed for advanced consumers who want to inspect the cache from a custom embedder backend.
- `crate::lock::acquire_embedding_singleton(namespace, db_path, wait_seconds, force) -> Result<File, AppError>` — G45 cross-process singleton for LLM embedding against a `(namespace, db)` pair. Reuses `fs4` flock with the same polling/force contract as `acquire_job_singleton`.
- `crate::errors::AppError::EmbeddingSingletonLocked { namespace }` — G45 new structural variant; `is_retryable() == true`, exit code 75, pt-BR localized message via `i18n::validation::app_error_pt::embedding_singleton_locked`.
- `crate::extract::llm_embedding::LlmEmbedding::model_label(&self) -> String` — G56 stable label combining flavour (`"claude" | "codex"`) and the active embed model; used as part of the entity-embed cache key.

No public symbols were removed, renamed, or had their signature changed in 1.0.80. The library consumer workflow is unchanged: pin to `=1.0.80` if you depend on the lib API.

### Added — G45: cross-process embedding coordination

- `acquire_embedding_singleton` serialises LLM embedding calls per `(namespace, db)` pair across concurrent CLI invocations. A second CLI trying to embed against the same database while a first is still in flight receives `EmbeddingSingletonLocked { namespace }` (exit 75) and can pass `--wait-embed-singleton <SECONDS>` to poll until the lock drops. Distinct databases (or distinct namespaces) acquire independent locks; `fs4` flock is the underlying primitive so the lock survives process crashes and is released automatically on drop.
- Operationally the singleton prevents the "two remember invocations on the same database, two LLM subprocesses, two parallel batches" pathology that v1.0.79's in-process cache could not address.

### Added — G53: stability policy and CI gate

- New CI job `semver-checks` (informational in v1.0.80, promoted to blocking in v1.0.81 once the 9 outstanding MAJOR violations are resolved). Runs `cargo semver-checks check-baseline --baseline-version 1.0.79` and surfaces a structured view of lib-API drift. The duplicate `--manifest-path` bug in the v1.0.79-initial commit is fixed.
- README.md and README.pt-BR.md now carry a `Stability Policy / Política de Estabilidade` section recording the CLI-stable / lib-unstable split per ADR-0032.

### Added — G55 S2: structural `MemoryNotFound`

- `AppError::MemoryNotFound { name, namespace }` and `AppError::MemoryNotFoundById { id }` replace the legacy `NotFound(String)` path inside `read` and `hybrid-search`. The required identifier is now part of the variant, eliminating the `not found: unknown` class of bugs that masked which lookup target failed. pt-BR messages carry the name and namespace explicitly.

### Added — G56: entity-embed in-process cache

- `embed_entity_texts_cached` sits in front of `embed_passages_parallel_local` for entity-name batches. Cache key is `blake3(model || "\0" || text)`. Hit rate is high in `ingest` (canonical entities re-embedded across many memories) and modest in `remember` and `remember-batch`. `remember.rs`, `ingest.rs` and `remember_batch.rs` all route entity embeds through the cache; chunk embeds still go through the raw path because chunk uniqueness makes the hit rate negligible. Stats are emitted via `tracing::debug!` (G56 hit/miss/request counts).

### Added — G58: recall and hybrid-search fallback to FTS5

- `recall --fallback-fts-only` and `hybrid-search --fallback-fts-only` route the query through FTS5 BM25 when the LLM subprocess fails (rate limit, OAuth contention, divergent dim). The new envelope fields `vec_degraded` (bool), `vec_error` (string) and `warning` (string) are populated symmetrically across both commands. The `recall` and `hybrid-search` tests gained coverage for the FTS5-only path; 1 test is `#[ignore]` because the G58 S1 stub requires PATH without `codex` or `claude` to exercise `EmbeddingFailed`.

### Added — G53-WINDOWS-INFRA: pre-warm and verify steps on windows-2025 (ADR-0033)

- The `clippy` and `test` jobs of the windows-2025 matrix gained 2 new steps each (gated `if: matrix.os == 'windows-2025'`, no-op on ubuntu/macos): a pre-warm that downloads the rustup toolchain into the runner cache before the build, and a verify step that re-checks `rustup show active-toolchain` after install. The 2 historical infra failure modes (rustup download with transient network errors and `E0463 can't find crate for core` when the target stdlib is missing) are now recoverable on the first re-run instead of accumulating as red CI.
- Local cross-compile validation: `cargo check --target x86_64-pc-windows-msvc --lib --all-features` reproduces and `E0463` is fixed by `rustup target add x86_64-pc-windows-msvc --toolchain 1.88`; the build then reaches the `cc-rs: failed to find tool "lib.exe"` frontier, which is the expected host-Linux cross-compile limit. ADR-0033 documents the rationale and the boundary.

### Added — SHUTDOWN resilience: panic-free third-signal exit (ADR-0034)

- `src/signals.rs` now wraps the first-signal handler in a panic-catching boundary: even when the parent's stderr is a closed pipe (the orphaned-process scenario that the G42/C2 audit identified), the handler returns cleanly instead of `SIGABRT`-ing on `BrokenPipe`. The third consecutive Ctrl-C exits with code 130 and ZERO I/O, matching the contract documented in ADR-0034 and the recipe in `docs/HEADLESS_INVOCATION.md`.
- The 3-layer SHUTDOWN bypass recipe (`nohup` → `setsid` → `disown`) is now the canonical reference for the agent harness when running long embedding jobs in background; HEADLESS_INVOCATION.md and COOKBOOK.md carry the snippet.

## [1.0.79] - 2026-06-11

### Removed

- **Daemon infrastructure fully removed**: `src/daemon.rs` (1120 lines), `src/commands/daemon.rs` (79 lines), `tests/daemon_integration.rs` (316 lines) deleted. `DaemonOpts` struct and `--autostart-daemon` flag removed from all command args. All `crate::daemon::embed_*_or_local` calls replaced with direct `crate::embedder::embed_*_local` wrappers. CLI is now 100% one-shot with zero IPC. 8 daemon constants removed from `src/constants.rs`. Net removal: ~764 lines.
- **Legacy local-model features fully removed (ahead of the v1.1.0 schedule)**: the `embedding-legacy`, `ner-legacy` and `full` Cargo features are gone, together with the optional `fastembed`, `ort`, `ndarray`, `tokenizers` and `hf-hub` dependencies and `src/extraction_gliner.rs`. `EmbeddingBackend` is now a permanent stub returning a clear migration error; `extract_graph_auto` lost its GLiNER delegation path; `calculate_safe_concurrency` budgets heavy commands with `LLM_WORKER_RSS_MB` (350) instead of the obsolete 1100 MB ONNX constant (`EMBEDDING_LOAD_EXPECTED_RSS_MB` deleted). The CI matrix shrinks to `default` + `llm-only`. Every build is LLM-only; there is no local-model path.

### Deprecated

- **GLiNER-era flags are formal no-ops with explicit warnings**: `--gliner-variant` (on `remember` and `ingest`) and `ingest --mode gliner` now emit a `tracing::warn!` deprecation notice when used; `--enable-ner` performs URL-regex extraction only. All help strings rewritten to stop promising the removed GLiNER pipeline (model variants, sizes, thresholds); `SQLITE_GRAPHRAG_GLINER_VARIANT`/`_MODEL`/`_THRESHOLD` remain accepted for compatibility but have no effect.

### Fixed — G42: slow, serialized, fragile LLM embedding pipeline

- **S1 — configurable embedding dimensionality (default 64)**: single source of truth in `constants.rs` (`DEFAULT_EMBEDDING_DIM` + `embedding_dim()`); precedence `--embedding-dim` flag > `SQLITE_GRAPHRAG_EMBEDDING_DIM` env > `schema_meta.dim` of the opened database > 64. Existing 384-dim databases keep working unchanged. ZERO schema change (the `dim` key and columns already existed). Basis: MRL, arXiv 2205.13147 — output per vector drops from ~3072 to ~512 tokens (~6x)
- **S2 — batched LLM calls**: `embed_batch_async` embeds N numbered texts per call with the `{items:[{i,v}]}` schema; chunks batch at 8, entity names at 25 (calibration bases at dim 64; dim-adaptive since G44) — 39 subprocess spawns collapse into 4-5
- **S3 — real parallelism**: `Arc<Semaphore>` + `acquire_owned` + `JoinSet` + `join_next`/`is_panic` bounded fan-out in `embedder.rs`; the global Mutex now guards ONLY the config clone (the old `flush_group` held it across 30-60s of network I/O, forcing effective parallelism 1); results stream through a BOUNDED mpsc channel (backpressure + incremental delivery); permits = min(`--llm-parallelism`, cpus, ram*0.5/350MB, 32); new `--llm-parallelism` flag on `remember` (default 4), `ingest` (default 2, multiplies with `--ingest-parallelism`) and `edit`
- **S4 — schema tempfile RAII**: codex `--output-schema` files are `NamedTempFile`s with randomised names created once per process (no per-call write+delete, no PID-path races); the orphan reaper now also removes stale `codex-home-{pid}` dirs whose PID is gone
- **S5 — claude model env override**: `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL` (symmetric to the codex var); zero hardcoded models without override
- **S6 — empty `CLAUDE_CONFIG_DIR` by default** on the embedding path: honours `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR`, else uses a managed `~/.local/state/sqlite-graphrag/claude-empty-config` (mode 0700, copies `.credentials.json` when present); the MCP-isolation flags are silently ignored upstream (anthropics/claude-code#10787) and a full `~/.claude` cost ~223k tokens per call (~40-50s → ~10-15s)
- **S7 — actionable codex headless error**: `request_user_input` failures now explain the cause and remediation instead of an opaque exit 11
- **S8 — panic-free signal handler**: first signal uses best-effort `writeln!` (BrokenPipe ignored); second signal exits 130 with ZERO I/O — eliminates the SIGABRT on orphaned processes (`panic = "abort"` + closed stderr pipe)
- **S9 — canonical one-shot re-embed**: `enrich --operation re-embed --limit N --resume` documented as the official path; new `edit --force-reembed` regenerates an embedding without changing the body; removed the BROKEN pre-warm recipe (`edit --description "<same>"` never re-embedded) from MIGRATION/HOW_TO_USE docs
- **C5 — no silent dimension normalisation**: `normalise_dim` (truncate/zero-pad) replaced by `validate_dim`, which errors on divergent vectors; the batch parser validates index coverage and per-item dimensionality
- Every LLM subprocess now uses `kill_on_drop(true)` plus an explicit `tokio::time::timeout` (`SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS`, default 300s); a process-wide multi-thread runtime replaces the per-call current-thread runtime
- New concurrency tests: peak never exceeds permits (AtomicUsize), panicking task returns its permit via RAII and surfaces `is_panic`, cancellation terminates the fan-out quickly, divergent dim fails the fan-out

### Fixed — G43: dimensionality adoption did not cover the main commands

- **Dim adoption on every connection open**: the G42/S1 sync (`schema_meta.dim` → active dim) only ran inside `ensure_db_ready`, which `remember` / `edit` / `recall` / `hybrid-search` never call — those commands silently used the compiled default (64) against pre-v1.0.79 384-dim databases, writing mixed-dim embeddings that cosine-score 0.0 against each other (vector recall went blind to the old corpus). `open_rw` AND `open_ro` now adopt the recorded database dim (best-effort, env override still wins); 4 regression tests cover rw/ro adoption, env precedence and virgin databases
- **`init` no longer stamps `dim=384`**: the hardcoded `INSERT OR REPLACE ... ('dim', '384')` stamped NEW databases with a dim that contradicts the active default; replaced by `INSERT OR IGNORE` with the active dim (preserves the recorded dim on re-init of an existing database)
- **`rename-entity` no longer records `dim=384` and a removed model name**: the duplicated INSERT (hardcoded `384` + `multilingual-e5-small`) was replaced by the canonical `upsert_entity_vec` writer (real vector length, CLI version as `model`)
- **Test mocks speak both embedding shapes**: `tests/mock-llm/{claude,codex}` returned a fixed 384-dim single-shape vector, so the ENTIRE `slow-tests` integration suite failed since G42/S1+S2 (the gate never runs on CI, hiding it); the mocks now return 64-dim vectors and answer the `{items:[{i,v}]}` batch schema; the 2 obsolete daemon tests became regression guards for the daemon removal; `.config/nextest.toml` no longer filters on the deleted `daemon_integration` binary — `--features slow-tests` integration suite back to green (69/69 on the `integration` binary)

### Fixed — G44: embedding batch size did not scale with the dimensionality

- **Dim-adaptive batch size**: the G42/S2 batches were FIXED (8 chunks / 25 entity names per LLM call), calibrated for the dim-64 default (~512 / ~1600 floats per response); on legacy 384-dim databases the same chunk batch asked for ~3072 floats — measured in production: claude returned 3 of 8 items (caught by the G42/C5 coverage check) and codex timed out at 300s, failing `remember` twice. The batch size now adapts as `clamp(base×64/dim, 1, base)` (`embedder.rs::adaptive_batch_for_dim`): dim 64 keeps 8/25, dim 384 uses 1/4 — constant float budget per call, no `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` workaround needed; 6 regression tests cover the formula and the env-dim wrappers

## [1.0.78] - 2026-06-09

### Fixed

- **G41**: `run_rehash` no longer inserts phantom rows for unapplied migrations — the `else` branch that caused V013 to be registered without executing its SQL has been removed
- **G41 repair**: new helper `ensure_v013_tables_exist` detects and repairs databases where V013 was registered in `refinery_schema_history` but the BLOB-backed embedding tables (`memory_embeddings`, `entity_embeddings`, `chunk_embeddings`) were never created
- Auto-repair integrated in `ensure_db_ready` — any CRUD command now heals G41-corrupted databases unconditionally

### Added

- Field `v013_tables_created` (boolean) in `RehashReport` and `ToLlmOnlyReport` JSON responses
- 3 new unit tests for `ensure_v013_tables_exist` (noop, phantom repair, no history)
- 1 updated unit test: `rehash_does_not_insert_missing_migrations` (replaces the test that validated buggy behavior)
- ADR-0028 documenting the G41 fix and repair strategy

### Migration

- Upgrade: `cargo install sqlite-graphrag --version 1.0.78 --force`
- Auto-repair is unconditional: any command (`remember`, `recall`, etc.) heals G41-corrupted databases
- Explicit repair: `sqlite-graphrag migrate --rehash` or `migrate --to-llm-only --drop-vec-tables`
- No manual SQL intervention needed

## [1.0.77] - 2026-06-09

### Fixed

- `run_rehash` INSERT now includes `applied_on` with RFC3339 timestamp via `chrono::Utc`
- Helper `sanitize_null_applied_on` fixes existing NULL rows before refinery runs
- Helper `remove_vec_virtual_tables_without_module` cleans vec0 shadow tables via `PRAGMA writable_schema`
- `debug-schema` no longer crashes on databases with `applied_on = NULL`
- Field `applied_on` changed from `String` to `Option<String>` in debug-schema output

### Added

- Field `null_rows_fixed` in `RehashReport` and `ToLlmOnlyReport` JSON responses
- Field `vec_tables_removed_via_writable_schema` in `ToLlmOnlyReport` JSON response
- 4 new unit tests covering sanitization, INSERT fix, and vec table removal
- 2 new integration tests for the NULL `applied_on` fix flow
- ADR-0027 documenting the G40 fix decision

### Migration

- Upgrade is automatic: `cargo install sqlite-graphrag --version 1.0.77 --force && sqlite-graphrag migrate`
- No manual SQL intervention needed
- v1.0.77 detects and fixes NULL `applied_on` rows automatically
- See `docs/MIGRATION.md` for details

## [1.0.76] - 2026-06-07

> **Breaking architectural change.** The default build is now **LLM-only and one-shot**.
> There is no daemon, no ONNX runtime, and no local model cache in the default build.
> All embedding generation, NER, and vector search are delegated to `claude -p` or `codex exec` headless (OAuth, no MCP, no hooks). The CI matrix now runs 3 feature flags in parallel: `default`, `llm-only`, and `embedding-legacy`.

### Removed

- **`fastembed` 5.13.4** — embedding generation now goes through `LlmEmbedding` in `src/extract/llm_embedding.rs`, which spawns `claude -p` or `codex exec` with `--output-schema` enforcing a 384-dim `f32` array.
- **`ort` 2.0.0-rc.12** — no more ONNX runtime in the default build; the LLM does inference.
- **`ndarray` 0.16** — no longer needed; vectors live in BLOB.
- **`tokenizers` 0.22** — replaced with whitespace token heuristic in `src/tokenizer.rs`. `CHARS_PER_TOKEN` is the same calibration the rest of the crate uses.
- **`huggingface-hub` 0.4** — no more model downloads.
- **`GLiNER NER`** in `extraction_gliner.rs` — moved behind the `ner-legacy` feature. Default build uses URL regex only; full NER comes from the LLM `ExtractionBackend` in `src/extract/`.
- **`sqlite-vec` 0.1.9** — REMOVED. The `vec_memories`, `vec_entities`, `vec_chunks` virtual tables are dropped by migration `V013` and replaced with regular BLOB-backed `memory_embeddings`, `entity_embeddings`, `chunk_embeddings` tables. Cosine similarity is computed in pure Rust on demand in `src/similarity.rs`.
- **Daemon as a performance optimization** — the `daemon` subcommand is still present for source compatibility but every `EmbedPassage`/`EmbedQuery` request now goes through the LLM one-shot, defeating the original purpose of the daemon. The daemon will be removed in v1.1.0.

### Added

- **`ExtractionBackend` trait (G21 solution)** — new `src/extract/` module exposes a trait with four implementations: `LlmBackend` (default, invokes `claude -p` or `codex exec` headless), `EmbeddingBackend` (legacy fastembed pipeline, stub when LLM-only), `NoneBackend` (no-op for explicit skip), and `CompositeBackend` (merges multiple backends in parallel). Global flag `--extraction-backend llm|embedding|none|both` selects the backend at runtime; the LLM backend is the new default.
- **`VersionAdapter` trait (G22 solution)** — new `src/spawn/` module abstracts executor spawn invocations behind a trait. Three concrete adapters ship: `CodexAdapter` (detects `codex 0.130.0` through `0.138+` and adapts flags — `codex 0.137.0` removed `--ask-for-approval` in favour of `-a never`, and the adapter emits the new flag automatically), `ClaudeAdapter` (claude code 2.1.0+), and `OpencodeAdapter` (opencode headless). The trait also exposes `ExecutorVersion` (built on `semver::Version`), `CompatMode` (`strict` | `lenient` | `auto`), `ExecutorCapabilities`, `VersionCache`, and an `ErrorPropagator` that propagates subprocess stderr to the user instead of swallowing it (root cause of G22 P16).
- **Adaptive concurrency (G18 solution)** — `MAX_CONCURRENT_CLI_INSTANCES` raised from 4 to 16 (legacy fallback). New `crate::lock::calculate_safe_concurrency()` function reads `sysinfo::System::available_memory()` and computes a dynamic permit count via `min(cpus, available_mb / worker_cost_mb)`. New `LLM_WORKER_RSS_MB = 350` constant for LLM-only workers (vs `EMBEDDING_LOAD_EXPECTED_RSS_MB = 1100` for the legacy fastembed path). The `* 0.5` halving factor that caused the 4-slot ceiling has been removed.
- **Feature flag `llm-only` (G23 foundation)** — opt-in feature that opts the build out of the fastembed + ort pipeline. Already the default behaviour; the feature is now the explicit opt-in marker for the v1.1.0 flip. `embedding-legacy` is recognised by `cfg!()` checks in `src/lock.rs` so the adaptive concurrency formula can pick the right `worker_cost_mb` in feature-gated builds.
- **`tracing` respects `RUST_LOG`** — removed the static `release_max_level_info` feature from `tracing`, so operators can override the log level at runtime via the `RUST_LOG` environment variable (helps G22 P17).
- **`migrate --rehash`** — rewrites recorded migration checksums to match current file content via `SipHasher13(name|version|sql)`. Algorithm matches `refinery-core 0.9.1` (the version the binary embeds); same `SipHasher13` crate, same hashing order. Required for v1.0.74 databases upgrading to v1.0.76 because `V002` was intentionally emptied to a no-op.
- **`migrate --to-llm-only`** — one-shot upgrade for v1.0.74 / v1.0.75 databases: rehash + apply `V013` + report vec-table state. Requires `--drop-vec-tables` as an explicit safety guard.
- **BLOB-backed embedding tables** — `memory_embeddings`, `entity_embeddings`, `chunk_embeddings` replace the old sqlite-vec virtual tables. Pure-Rust cosine similarity in `src/similarity.rs` (ADR-0020, ADR-0022).
- **OAuth-only LLM credential flow (ADR-0025)** — the LLM spawn ABORTS with `AppError::Validation` if `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` is set in the environment. Both variables are excluded from the env-clear whitelist as defence in depth.

### Changed

- **CLI is one-shot by default** — the `remember` / `ingest` / `edit` / `recall` / `hybrid-search` commands no longer autostart the daemon for embeddings. Each embedding is a fresh `claude -p` or `codex exec` subprocess (one OAuth turn per call).
- **Operator workflow shift** — to keep embedding latency under control, operators should run `claude` or `codex` outside `sqlite-graphrag` (e.g., as a systemd unit or a watchexec loop) and let the binary call them when needed.

### Migration

- **Migration `V013` drops the vec tables.** Existing v1.0.74 databases will lose their old embeddings; they are recomputed lazily on the next `remember` / `ingest` / `edit`.
- **Operators who want to preserve old vectors** can dump the vec tables before running `init --force`.
- **Recommended upgrade path** — see `docs/MIGRATION.md` for the step-by-step v1.0.74 → v1.0.76 procedure, including `migrate --to-llm-only --drop-vec-tables`.
- **Rollback procedure** — `cargo install sqlite-graphrag --version 1.0.75 --force` restores the legacy build, then re-`init --force` recreates the vec tables (embeddings are lost unless dumped beforehand).

### Dependencies

- `async-trait = "0.1"` — required for the `ExtractionBackend` and `VersionAdapter` traits to be dyn-compatible.
- `semver = "1"` with `serde` feature — required for `ExecutorVersion` parsing in `src/spawn/`.
- `siphasher = "1.x"` (pinned) — required to compute migration checksums deterministically. Already in the build graph transitively from `refinery-core 0.9.1`; this entry makes the link explicit.
- **REMOVED:** `fastembed 5.13.4`, `ort 2.0.0-rc.12`, `ndarray 0.16`, `tokenizers 0.22`, `huggingface-hub 0.4`, `sqlite-vec 0.1.9`.

### Tests

- 745 lib tests preserved from v1.0.74 baseline.
- Mock LLM CLI wired into 26 test files for the LLM-only build path.
- 107/115 previously-slow tests fixed in commit `bd0a3f5` (mock LLM unblocks CI from real OAuth turns).
- CI matrix 3 features: `default`, `llm-only`, `embedding-legacy` run clippy and tests in parallel.
- 12 new tests in `tests/extract_backend.rs` (LLM, Embedding, None, Composite, factory, dispatch, hints, health).
- 13 new tests in `tests/spawn_version_adapter.rs` (Codex, Claude, Opencode, version matrix, parse, JSONL).
- 6 new tests in `tests/concurrency_adaptive.rs` (legacy formula no longer halves, LLM worker budget, max ceiling).
- 4 new tests in `tests/migrate_rehash_integration.rs` (healthy DB no-op, corrupted checksum fix, to-llm-only success, safety guard refusal).
- 11 new unit tests in `src/commands/migrate.rs` (checksum determinism, no-op history, corrupted checksum rewrite, idempotency, vec-table detection).
- 4 tests in `tests/signal_handling_integration.rs` verified green (4/4) — 3 pre-existing failures fixed by the v1.0.75 daemon-fallback fix.
- 7 tests in `tests/v2_breaking_integration.rs` verified green (7/7) — 2 pre-existing failures fixed.

### Validation

- `cargo check --all-targets --no-default-features --features llm-only`: 0 errors.
- `cargo check --all-targets --no-default-features --features embedding-legacy`: 0 errors.
- `cargo check --all-targets` (default): 0 errors.
- `cargo clippy --all-targets --all-features -- -D warnings`: 0 warnings.
- `cargo fmt --all --check`: 0 differences.
- `cargo build --bin sqlite-graphrag --release` (default, LLM-only): builds in ~25s, binary 6 MB (historical — binary grew to 14.6 MiB in v1.0.89).
- `cargo build --bin sqlite-graphrag --release --no-default-features --features embedding-legacy`: builds in ~1m 11s, binary 39 MB.
- `cargo test --lib`: 745 passed.
- `cargo test --all-features`: green across all 3 feature flags.
- Release binary (default build) reports `sqlite-graphrag 1.0.76`, no ONNX runtime, no `libonnxruntime.so` required.

### Documentation

- New: `docs/HOW_TO_USE.md` (221 lines) — rewritten for v1.0.76 LLM-Only.
- New: `docs/MIGRATION.md` (147 lines) — v1.0.74 → v1.0.76 step-by-step.
- New: `docs/AGENTS.md` (1428 lines) — updated header, LLM-Only architecture, OAuth enforcement, hardening flags.
- Updated: `docs/COOKBOOK.md` — added "How To Upgrade From v1.0.74 Or v1.0.75 To v1.0.76" recipe; updated daemon recipe with DEPRECATED notice; updated Latency Note.
- New ADR: `adr-0019-llm-only-one-shot.md` (PT-BR: `adr-0019-llm-only-one-shot.pt-BR.md`).
- New ADR: `adr-0020-pure-rust-cosine.md` (PT-BR).
- New ADR: `adr-0021-deprecate-daemon.md` (PT-BR).
- New ADR: `adr-0022-blob-embeddings.md` (PT-BR).
- New ADR: `adr-0023-remove-tokenizers.md` (PT-BR).
- New ADR: `adr-0024-fts5-coarse-cosine-refine.md` (PT-BR).
- New ADR: `adr-0025-oauth-only-embedding.md` (PT-BR).
- New ADR: `adr-0026-v002-vec-tables-migration-drift.md` (PT-BR).
- New schema: `migrate-rehash.schema.json` (response of `migrate --rehash --json`).
- New schema: `migrate-to-llm-only.schema.json` (response of `migrate --to-llm-only --json`).
- New doc: `docs/HEADLESS_INVOCATION.md` (promoted from gaps.md) — how to invoke Claude/Codex/OpenCode headless without MCP, OAuth-safe.

## [1.0.75] - 2026-06-07

### Added

- **ExtractionBackend trait (G21 solution)**: new `src/extract/` module exposes a `ExtractionBackend` trait with four implementations: `LlmBackend` (default, invokes claude code / codex CLI headless), `EmbeddingBackend` (legacy fastembed pipeline, stub when LLM-only), `NoneBackend` (no-op for explicit skip), and `CompositeBackend` (merges multiple backends in parallel). The global flag `--extraction-backend llm|embedding|none|both` selects the backend at runtime; the LLM backend is the new default. The trait uses `async-trait` for `dyn` dispatch, returns structured `ExtractionOutput { entities, relationships, embedding, backend, elapsed_ms }`, and is the foundation for the v1.1.0 LLM-only migration.

- **VersionAdapter trait (G22 solution)**: new `src/spawn/` module abstracts executor spawn invocations behind a `VersionAdapter` trait. Three concrete adapters ship: `CodexAdapter` (detects codex 0.130.0 through 0.138+ and adapts flags — `codex 0.137.0` removed `--ask-for-approval` in favour of `-a never`, and the adapter emits the new flag automatically), `ClaudeAdapter` (claude code 2.1.0+), and `OpencodeAdapter` (opencode headless). The trait also exposes `ExecutorVersion` (built on `semver::Version`), `CompatMode` (`strict` | `lenient` | `auto`), `ExecutorCapabilities`, `VersionCache`, and an `ErrorPropagator` that propagates subprocess stderr to the user instead of swallowing it (was the root cause of G22 P16).

- **Adaptive concurrency (G18 solution)**: `MAX_CONCURRENT_CLI_INSTANCES` raised from 4 to 16 (legacy fallback). New `crate::lock::calculate_safe_concurrency()` function reads `sysinfo::System::available_memory()` and computes a dynamic permit count via `min(cpus, available_mb / worker_cost_mb)`. New `LLM_WORKER_RSS_MB = 350` constant for LLM-only workers (vs `EMBEDDING_LOAD_EXPECTED_RSS_MB = 1100` for the legacy fastembed path). The `* 0.5` halving factor that caused the 4-slot ceiling has been removed.

- **Feature flag `llm-only` (G23 foundation)**: opt-in feature that opts the build out of the fastembed + ort pipeline. Currently a no-op alias in the default build; the foundation for the v1.1.0 default flip. `embedding-legacy` is recognised by `cfg!()` checks in `src/lock.rs` so the adaptive concurrency formula can pick the right `worker_cost_mb` in feature-gated builds.

- **tracing respects `RUST_LOG`**: removed the static `release_max_level_info` feature from `tracing`, so operators can override the log level at runtime via the `RUST_LOG` environment variable (helps G22 P17).

### Fixed

- **Daemon client graceful fallback on mid-request disconnect**: `request_or_autostart` now calls `wait_for_daemon_ready` after a version-mismatch auto-restart and after a fresh `ensure_daemon_running` spawn, so the client never races a newly-created socket that the kernel is still wiring up. `request_if_available` and the read/write paths in the daemon request loop treat `ConnectionReset`, `BrokenPipe`, and `ConnectionAborted` as "daemon not available" (`Ok(None)`) and fall through to the local embedder. New `is_daemon_gone()` helper centralises the matching. This was a pre-existing latent bug exposed by the v1.0.75 version bump that introduced a forced daemon restart in the test process; it caused 3 + 2 pre-existing failures in `signal_handling_integration` and `v2_breaking_integration` respectively. Both suites now pass green (4/4 and 7/7).

- **clippy `useless_vec` and `cast` lints** in `src/embedder.rs` and `src/extract/`. The `vec![single]` allocations were replaced with `[single; 1]` arrays (fastembed 5.13.4 wants `AsRef<[S]>`), and an `as usize` cast on a `usize` field was removed. `cargo clippy --all-targets -- -D warnings` is now clean.

- **`doc list item without indentation`** in the `acquire_slot` doc comment of `src/lock.rs:170` — the previous version had a stray line that broke the Markdown list rendering and triggered the lint.

### Dependencies

- `async-trait = "0.1"` — required for the `ExtractionBackend` and `VersionAdapter` traits to be dyn-compatible.
- `semver = "1"` with `serde` feature — required for `ExecutorVersion` parsing in `src/spawn/`.

### Tests

- 745 lib tests preserved (v1.0.74 baseline)
- 12 new tests in `tests/extract_backend.rs` (LLM, Embedding, None, Composite, factory, dispatch, hints, health)
- 13 new tests in `tests/spawn_version_adapter.rs` (Codex, Claude, Opencode, version matrix, parse, JSONL)
- 6 new tests in `tests/concurrency_adaptive.rs` (legacy formula no longer halves, LLM worker budget, max ceiling)
- 3 tests in `tests/v1063_features.rs` verified green (3/3)
- 4 tests in `tests/v1044_features.rs` verified green (8/8) — the previously-failing `related_entity_seed_via_link_succeeds` now passes
- 4 tests in `tests/signal_handling_integration.rs` verified green (4/4) — the 3 pre-existing failures fixed
- 7 tests in `tests/v2_breaking_integration.rs` verified green (7/7) — the 2 pre-existing failures fixed
- 4 tests in `tests/concurrency_limit_integration.rs` verified green (4/4) with `slow-tests` feature
- 9 tests in `tests/cli_integration.rs` verified green (9/9) with `slow-tests` feature
- 25 tests in `tests/exit_codes_integration.rs` verified green (25/25) with `slow-tests` feature
- 5 tests in `tests/entity_validation_integration.rs` verified green (5/5) with `slow-tests` feature
- Total: 776 + 56 = 832 tests verified green across 11 suites

### Validation

- `cargo check --all-targets`: 0 errors, 0 warnings
- `cargo clippy --all-targets -- -D warnings`: 0 warnings
- `cargo fmt --all --check`: 0 differences
- `cargo build --bin sqlite-graphrag --release`: 0 errors in 1m 11s
- `cargo test --lib`: 745 passed
- `cargo test --test extract_backend --test spawn_version_adapter --test concurrency_adaptive`: 31 passed
- Release binary: 39M, reports `sqlite-graphrag 1.0.75`

## [1.0.74] - 2026-06-05

### Fixed

- **`--skip-extraction` no-op compatibility restored (v1.0.45 promise honored)**: v1.0.67 (commit 9ddb17b) promoted the `--skip-extraction` deprecation from a `tracing::warn!` to a hard `AppError::Validation` in both `src/commands/remember.rs:415-417` and `src/commands/ingest.rs:1057-1059`. This broke the CHANGELOG v1.0.45 promise of "kept as a hidden no-op for backwards compatibility" and started failing 5 CI jobs (Slow Contract Suites, Tests ubuntu/macos, Coverage threshold, cargo-careful sanity) whose E2E tests use the flag to skip the GLiNER-ONNX model download. Reverted to `tracing::warn!` with a message that mirrors the v1.0.45 wording plus a hint to remove the flag.

- **`Windows MSVC cross-compile (G29)` failed with `error[E0463]: can't find crate for 'core'`**: the `dtolnay/rust-toolchain@stable` action internally runs `rustup toolchain install stable --target x86_64-pc-windows-msvc --profile minimal`, but `--profile minimal` ignores `--target`, so the cross stdlib was never downloaded. The build then failed at `cfg-if` and `libc` (the first crates compiled for the foreign target). Added an explicit `rustup target add x86_64-pc-windows-msvc --toolchain stable` step after the toolchain action so the cross stdlib is reliably installed.

- **`Miri Unsafe Validation` failed with `can't call foreign function 'mi_malloc_aligned' on OS 'linux'`**: `mimalloc` (the global allocator set in `src/main.rs:3-4`) calls `mi_malloc_aligned` which Miri cannot model. Added `RUSTFLAGS="--cfg sqlite_graphrag_miri"` to the Miri job and gated the `#[global_allocator]` with `#[cfg(not(sqlite_graphrag_miri))]`. The Miri step now uses the default Linux allocator while production binaries still get the mimalloc speedup. Registered the new cfg in `[lints.rust].unexpected_cfgs.check-cfg`.

- **Three `-D warnings` errors in `Tests (windows-2025)` and `Clippy (windows-2025)`**: `RUSTFLAGS=-D warnings` turned the dead-code warnings on `src/reaper.rs:17` (`unused import: std::time::Duration`), `:19` (`ORPHAN_MIN_AGE_SECS is never used`), and `:20` (`ORPHAN_SCAN_TARGETS is never used`) into hard errors on Windows, where the reaper internals are `#[cfg(unix)]`. Gated the three items with `#[cfg(unix)]` and the two tests that reference them with `#[cfg(unix)] #[test]`. The Windows build no longer dead-code-flags items it cannot use.

### Validation

- `cargo check --all-targets`: 0 errors
- `cargo clippy --all-targets --all-features -- -D warnings`: 0 warnings
- `cargo fmt --all --check`: 0 differences
- YAML schema: `python3 -c "import yaml; yaml.safe_load(...)"` valid for `ci.yml` (20 jobs), `release.yml` (4 jobs), `action.yml`
- TOML schema: `python3 tomllib.load(Cross.toml, Cargo.toml)` valid

## [1.0.73] - 2026-06-05

### Fixed

- **`linker 'clang' not found` in `Build aarch64-unknown-linux-gnu` (cross + Docker)**: the `cross` action creates an isolated container from `ghcr.io/cross-rs/aarch64-unknown-linux-gnu` and runs `cargo build` inside it. The container base image does NOT ship `clang` or `mold`. The host's `install-mold-linker` composite action only installs these on the GitHub Actions runner, not inside the cross container. The `pre-build` block in `Cross.toml` previously only installed `libssl-dev` + `pkg-config`, leaving rustc unable to find `clang` for the build scripts of `proc-macro2`, `quote`, and `libc`. Exit code 101. Added `clang`, `mold`, and `lld` to the `pre-build` apt install for `[target.aarch64-unknown-linux-gnu]`, and created `ln -sf` symlinks in `/usr/local/bin` so the cross container picks them up via `$PATH` regardless of the base image tag.

- **Node.js 20 deprecation warnings in 4 `actions/upload-artifact@v5` callsites**: `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24: "true"` forced the v5 action (which declares Node 20 in its manifest) to run on Node 24, producing 4 identical deprecation notices (`actions/upload-artifact@v5. For more information see: https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/`). Bumped all 3 callsites to `actions/upload-artifact@v6` (1 in `release.yml`, 2 in `ci.yml`). v6 declares Node 24 as the default runtime and removes the warning. The artifact names (`coverage-lcov`, `bench-baseline`, `sqlite-graphrag-${{ matrix.target }}`) are unique across the workflow, so the v6 breaking change that disallows same-name multi-upload in one run does not apply.

- **Homebrew tap-trust warnings on `Build aarch64-apple-darwin`**: the macOS step in `install-mold-linker/action.yml` ran `brew update` against an environment with `aws/tap`, `azure/bicep`, and `hashicorp/tap` registered but not explicitly trusted. Homebrew 5.2.0/6.0.0 will make `HOMEBREW_REQUIRE_TAP_TRUST=1` the default, and the warning text was becoming noisy (`brew install mold` triggers `brew doctor`-style notices for the untrusted taps even though none of them are used). Set `HOMEBREW_NO_REQUIRE_TAP_TRUST=1` in the env block of the macOS step. None of the trusted/untapped taps are needed for `brew install mold`.

### Informational

- **`windows-2025` redirect to `windows-2025-vs2026` by June 15, 2026**: a one-line notice from the `windows-2025` runner during `Build x86_64-pc-windows-msvc` announcing the upcoming automatic redirect. The build itself succeeds; the notice is logged for forward planning. No code change required for v1.0.73; a follow-up release will switch the runner label after the cutover date.

### Validation

- YAML schema: `python3 -c "import yaml; yaml.safe_load(...)"` valid for `ci.yml` (20 jobs), `release.yml` (4 jobs), `action.yml`
- TOML schema: `python3 tomllib.load(Cross.toml)` valid; pre-build array has 6 entries
- `actions/upload-artifact@v6` migration: 3/3 callsites updated, no `name:` collisions across the workflow
- `Cross.toml` pre-build: 3 new apt packages (`clang`, `mold`, `lld`) + 3 symlinks; container image will be re-cached by cross-rs on first run
- Composite action macOS step: env block extended with `HOMEBREW_NO_REQUIRE_TAP_TRUST: "1"`


The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.72] - 2026-06-05

### Fixed

- **mold linker missing on `ubuntu-latest` runners**: `.cargo/config.toml` (added in v1.0.69) forces `linker = "clang"` and `rustflags = ["-C", "link-arg=-fuse-ld=mold"]` for the `x86_64-unknown-linux-gnu` target. On the developer's local Fedora machine mold is installed via DNF, and on the macOS dev machine the `x86_64-unknown-linux-gnu` block is silently ignored (target is `aarch64-apple-darwin`), so local `cargo check`/`cargo test`/`cargo clippy` pass without the linker binary present. On the GitHub Actions `ubuntu-latest` runner, however, mold is NOT installed by default, and rustc propagated `-fuse-ld=mold` to clang which then emitted `error: invalid linker name in argument '-fuse-ld=mold'` and exit 1. The build script compilation (proc-macro2, quote, libc, all `build_script_build` binaries) failed first, cascading into 12+ job failures: `Tests (ubuntu/macos/windows)`, `Clippy (ubuntu/windows)`, `Coverage`, `Coverage threshold`, `Documentation`, `MSRV (1.88)`, `Slow Contract Suites`, `Windows MSVC cross-compile (G29)`, `cargo-careful sanity`, and `Benchmark Regression`. The `Annotations` step then aggregated 15 errors + 1 warning + 3 notices.

- **Resolution: composite action installs the mold linker on every compiling job**: added `.github/actions/install-mold-linker/action.yml` (35 lines) that detects the runner OS and installs `mold`+`clang`+`lld` via `apt-get` on Linux and via `brew` on macOS; on Windows the step is a no-op because the MSVC linker path does not honor `-fuse-ld=mold`. Wired the composite action into 15 jobs in `ci.yml` (14 `Swatinem/rust-cache` callsites + the `coverage-threshold` job that does not use `rust-cache`) and 3 jobs in `release.yml` (`validate`, `build-matrix`, `publish-crates-io`). Documented the mold dependency in `.cargo/config.toml` with a 6-line comment block.

### Validation

- 745 lib tests pass, 0 failed, 3 ignored (unchanged from v1.0.71)
- `cargo check --all-targets`: 0 errors (local, 4.88s)
- `cargo clippy --all-targets --all-features -- -D warnings`: 0 warnings
- `cargo nextest run --profile ci --all-features`: 800+ tests pass (full suite requires 10+ min on macOS; CI ubuntu-latest has 5+ min budget)
- `RUSTDOCFLAGS=-D warnings cargo doc --no-deps --all-features`: 0 warnings
- `cargo audit --ignore RUSTSEC-2025-0119 --ignore RUSTSEC-2024-0436 --deny warnings`: 0 vulnerabilities
- `cargo deny check advisories licenses bans sources`: all ok (2 `advisory-not-detected` warnings are intentional for the 2 upstream-unmaintained crates)
- `cargo publish --dry-run --allow-dirty`: package builds + uploads succeeds, dry-run aborts before registry
- `cargo package --list --allow-dirty`: 268 files, no `.env`/`.pem`/`.key`/`credentials`/`docs_rules`/`.claude`/`.serena`/`CLAUDE.md`/`AGENTS.md`
- `tokei . -e target -e docs`: 133 Rust files, 56126 total lines, 47906 code, 2791 comments, 5429 blanks
- YAML schema: `python3 -c "import yaml; yaml.safe_load(...)"` valid for `ci.yml` (20 jobs), `release.yml` (4 jobs), `action.yml`
- TOML schema: `python3 tomllib.load(.cargo/config.toml)` valid, target block unchanged
- **Coverage gate (10/10) deferred**: `cargo llvm-cov --all-features` requires >25 min on macOS dev machine; operator authorized skip per `feedback-never-publish-without-explicit-request` because `git diff --stat src/` is empty (no coverage-relevant change since v1.0.71 which passed the 75% gate in CI). The CI `coverage-threshold` job will re-validate the threshold on the published commit.

## [1.0.71] - 2026-06-05

### Fixed

- **GitHub Actions rust-cache pin resolved**: `Swatinem/rust-cache@v2.8` pinned in 17 call-sites across `ci.yml` and `release.yml` was a non-existent Git ref (only `v2.0.0`-`v2.9.1` exist on the upstream repo). Repinned all 17 call-sites to `Swatinem/rust-cache@v2.9.1` (latest stable, released 2026-03-12, "Fix regression in hash calculation"). Resolved the 22 `Unable to resolve action 'Swatinem/rust-cache@v2.8', unable to find version 'v2.8'` errors that were blocking every job.

- **Language policy residual in doc comments**: 2 doc comments referenced "Correção A" (Portuguese) at `src/commands/claude_runner.rs:231` and `src/commands/codex_spawn.rs:209`. Translated to "Fix A" (English idiom) so the `language-check` job (which scans for `[áéíóúâêôãõç]` outside `i18n.rs`) exits 0.

- **taiki-e/install-action missing `with:` block**: `ci.yml:409` invoked `taiki-e/install-action@v2` without specifying `tool`, producing `install-action: no tool specified; this could be caused by a dependabot bug where @<tool_name> tags on this action are replaced by @<version> tags` and exit 101 in the `coverage-threshold` job. Added the required `with: { tool: cargo-llvm-cov }` block.

- **cargo-careful timeout extended**: `ci.yml:379` had `timeout 600 cargo +nightly careful test -- --test-threads=2` which timed out (exit 124) on full `cargo-careful` runs with 745 tests under nightly. Doubled the budget to `timeout 1200` (20 min) so the sanity job completes on the 2-core `ubuntu-latest` runner even with Nightly's longer compile-then-test cycle.

- **windows-latest redirect notice**: GitHub Blog 2026-05-14 announced `windows-latest` and `windows-2025` will be migrated to `windows-2025-vs2026` (Visual Studio 2026) over the week of 2026-06-08 to 2026-06-15. Replaced the 3 `windows-latest` references (ci.yml clippy matrix x2, release.yml build-matrix for `x86_64-pc-windows-msvc`) with explicit `windows-2025` to opt out of the VS2026 redirect for now and avoid the 2 NOTICEs that the operator flagged in the v1.0.70 release run.

### Validation

- 745 lib tests pass, 0 failed, 3 ignored (unchanged)
- `cargo check --all-targets`: 0 errors (4.88s local)
- `cargo clippy --all-targets --all-features -- -D warnings`: 0 warnings
- `RUSTDOCFLAGS=-D warnings cargo doc --no-deps --all-features`: 0 warnings
- `cargo audit`: 0 vulnerabilities (2 allowed: RUSTSEC-2024-0436 paste unmaintained, RUSTSEC-2025-0119 tokenizers unmaintained)
- `cargo deny check advisories licenses bans sources`: all ok
- `cargo publish --dry-run --allow-dirty`: 268 files, 0 sensitive
- `cargo package --list --allow-dirty`: no `.env`/`.pem`/`.key`/`credentials`/`docs_rules`/`.claude`/`.serena`/`CLAUDE.md`/`AGENTS.md`
- YAML schema: 20 jobs ci.yml + 4 jobs release.yml, 17 rust-cache call-sites validated, 0 unresolved actions
- Language policy: 0 Portuguese chars in `///` or `//!` doc comments outside `i18n.rs`

## [1.0.70] - 2026-06-05

### Fixed

- **i18n POSIX locale precedence**: `Language::from_env_or_locale()` in `src/i18n.rs:34` now implements manual POSIX precedence `LC_ALL > LC_MESSAGES > LANG` via `std::env::var()` instead of calling `sys_locale::get_locale()` directly. The previous implementation ignored env vars set at runtime because `CFLocaleCopyCurrent()` (macOS) and `GetUserDefaultLocaleName` (Windows) cache the system locale. Three i18n tests now pass: `fallback_english_when_env_absent`, `posix_precedence_lc_all_overrides_lang`, `posix_precedence_lc_all_unrecognized_stops_iteration`.

- **GitHub Actions Node 24 migration**: All JavaScript actions in `.github/workflows/ci.yml` and `.github/workflows/release.yml` upgraded ahead of the 2026-06-16 default Node 24 migration and 2026-09-16 Node 20 removal. `actions/checkout@v4` → `@v5`, `actions/cache@v4` → `@v5`, `actions/upload-artifact@v4` → `@v5`, `actions/download-artifact@v4` → `@v5`, `taiki-e/install-action` → `@v2`, `Swatinem/rust-cache` pinned to `@v2.8` (no v3 GA). Added `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24: "true"` in env global of both workflows as defense-in-depth.

- **Duplicate job key in ci.yml**: Renamed the second `coverage:` job at `ci.yml:396` to `coverage-threshold:`. GitHub Actions schema strict validation was rejecting the workflow with `'coverage' is already defined` at line 396 col 3, blocking all 21 jobs from running.

- **dead_code warning in claude_runner.rs**: Added `#[cfg(target_os = "linux")]` to the `DEFAULT_SUBPROCESS_MEMORY_LIMIT_MB` constant (value 4096) at `src/commands/claude_runner.rs:51`. The constant was only referenced from the Linux-only `spawn_with_memory_limit` function and produced `dead_code` warnings on macOS and Windows builds. Resolved without using `#[allow(dead_code)]` (forbidden by `docs_rules`).

### Validation

- 745 lib tests pass (was 742 pass + 3 fail), 0 failed, 3 ignored
- `cargo clippy --all-targets --all-features -- -D warnings`: 0 warnings
- `RUSTDOCFLAGS=-D warnings cargo doc --no-deps --all-features`: 0 warnings
- `cargo audit`: 0 vulnerabilities (2 allowed: RUSTSEC-2024-0436 paste unmaintained, RUSTSEC-2025-0119 tokenizers unmaintained)
- `cargo deny check advisories licenses bans sources`: all ok
- `cargo publish --dry-run --allow-dirty`: 268 files, 0 sensitive
- `cargo package --list --allow-dirty`: no `.env`/`.pem`/`.key`/`credentials`/`docs_rules`/`.claude`/`.serena`/`CLAUDE.md`/`AGENTS.md`

## [1.0.69] - 2026-06-05

### Fixed
- **G28 (CRÍTICA)** Process proliferation at CLI startup. Three reinforcing changes close the root cause: (a) `claude_runner::build_claude_command` now ALWAYS passes `--strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions` so the Claude subprocess never inherits user-scoped MCP servers; the env var `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` remains available for full isolation. (b) `run_claude` sends `SIGTERM` on timeout before the Child is dropped, so MCP children don't survive the parent. (c) New `src/reaper.rs` walks `/proc` at startup, kills any `claude`/`codex` orphan with `PPID=1` and age > 60s, and the reaper is invoked from `main` BEFORE any work. The 4-test reaper suite (`orphan_min_age_is_one_minute`, `orphan_targets_include_claude_and_codex`, `reaper_report_starts_zeroed`, `scan_completes_without_panic_on_linux`) runs in <30s on the test host.
- **G29** `enrich --operation body-enrich` aborted 100% of invocations with `CHECK constraint failed: source IN ('agent','user','system','import','sync')`. The bug was a literal `source: "enrich".to_string()` in `src/commands/enrich.rs:902` that violated the SQLite CHECK constraint. Replaced with `source: "agent".to_string()` plus structured metadata `{operation, orig_chars, new_chars}` (G29 hotfix).
- **G29 audit** `persist_enriched_body` was bypassing the immutable version history. Every body-enrich now inserts a new `memory_versions` row with `change_reason='edit'` BEFORE the update, so `history --name <X>` lists both the original and enriched bodies and `restore --version N` can roll back to the pre-enrich state.
- **G31** `enrich --mode codex` was missing five critical hardening flags compared to `ingest --mode codex` (`--ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules`). Extracted the spawn pipeline into `src/commands/codex_spawn.rs` so BOTH call-sites consume the same canonical command.
- **G32** `enrich --mode codex` was calling `serde_json::from_str` on the raw stdout, but `codex exec --json` emits JSONL. The new `parse_codex_jsonl` helper iterates line by line, picks the last `item.completed` of type `agent_message`, and extracts usage from the last populated `turn.completed` event. Single source of truth, shared by `enrich` and `ingest --mode codex`.
- **G33** `enrich --mode codex --codex-model <name>` was rejected silently AFTER spending an OAuth turn. The new `validate_codex_model` helper checks `--codex-model` against the ChatGPT Pro OAuth whitelist (`codex-auto-review`, `gpt-5.3-codex-spark`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.5`) BEFORE the subprocess is spawned.
- **G34** The `llm_parallelism > 4` warning was emitted in `mode=codex` (which does not spawn MCP children) with the same severity as `mode=claude-code`. The warning is now conditional to the mode: Claude warns at 5, Codex warns at 17, Codex 5..16 is silent (validated at 1161 items, 0 failures in production).
- **G36** `optimize` rebuilt the FTS5 index unconditionally even when `fts check` reported the index was already healthy. The default behaviour is now to skip the rebuild when the index passes integrity-check. Operators can still force a rebuild with `--no-fts-skip-when-functional`. The response now exposes `fts_rebuilt`, `fts_skipped_functional`, `fts_unhealthy` for observability.
- **G38** `backup` defaulted to `run_to_completion(100, Duration::from_millis(50), None)` which on a 4.3 GB database took ~9 minutes purely on sleep. The new defaults are `run_to_completion(1000, Duration::from_millis(5), None)` (~25x speedup) and the response now reports `pages_copied` and `step_size`. Operators can tune with `--backup-step-size`, `--backup-step-sleep-ms`, and `--backup-no-sleep`.
- **G39** `vec_memories_orphaned` was reported by `health` with no remediation path. The new `vec orphan-list` / `vec purge-orphan --yes` / `vec stats --json` commands close the loop. `vec purge-orphan` requires `--yes` to prevent accidental loss; `--dry-run` is supported.

### Added
- **G30** Singleton lock is now scoped per `(job_type, namespace, db_hash)`. Two concurrent `enrich` invocations against DIFFERENT databases no longer collide; the same database still serialises. The `db_hash` is the first 12 hex chars of `blake3(canonicalize(db_path))`.
- **G30+G09** New CLI flags `--wait-job-singleton <SECONDS>` (poll for the lock) and `--force-job-singleton` (break a stale lock from a previously crashed invocation) on `enrich` and `ingest`. The error message that previously referenced a non-existent `--wait-job-singleton` flag is now actionable.
- **G35** New flags `--preflight-check`, `--fallback-mode <codex|claude-code>`, and `--rate-limit-buffer <SECONDS>` on `enrich`. The preflight probe issues a 1-turn ping before scanning N candidates; on a Claude rate limit it aborts with a clear error (or switches to `--fallback-mode`). Default off to keep `--dry-run` and CI flows zero-cost.
- **G37** New flags `--names <NAME>` and `--names-file <PATH>` on `enrich` to select a specific subset of memory names. `--names-file` accepts `#` comments and blank lines. Combined with `--names` as a union when both are set.
- **G14 (refactor)** Extracted `codex_spawn` module: spawn pipeline, JSONL parser, and ChatGPT Pro OAuth model validation live in one place (`src/commands/codex_spawn.rs`) with 8 unit tests covering parser edge cases, rate-limit detection, and command-flag presence.
- **G14 (refactor)** Extracted `vec` subcommand family: `vec orphan-list`, `vec purge-orphan --yes --dry-run`, `vec stats --json`.
- `src/memory_source.rs` — type-safe enum of the five `memories.source` CHECK-constraint values. `TryFrom<&str>` returns `AppError::Validation` listing the accepted values. 8 unit tests cover valid/invalid/empty/display/serialisation paths. The existing call-sites still use `String` for compatibility; the enum is the foundation for the v1.0.70 migration.

### Changed
- `lock::acquire_job_singleton` signature gains `db_path: &Path` and `force: bool` parameters. The lock file name is now `job-singleton-{tag}-{namespace_slug}-{db_hash}.lock` so the OS cache dir can be shared across databases.
- `backup::BackupResponse` adds `pages_copied` and `step_size` fields. Backward-compatible: existing consumers that ignore unknown fields keep working.
- `optimize::OptimizeResponse` adds `fts_skipped_functional` and `fts_unhealthy` fields.
- `lock::db_path_hash` is `pub` so callers can compute the hash without acquiring the lock.
- `claude_runner` spawn env now includes the same whitelisted env vars as the codex spawn (path consistency for users with strict custom configs).

## [1.0.68] - 2026-06-03

### Fixed
- `cargo install sqlite-graphrag` broke on Windows with `error[E0308]: mismatched types` in `src/terminal.rs:29` because `HANDLE` in `windows-sys >= 0.59` is `*mut c_void` (was `isize` in 0.48/0.52).  Replaced `handle != 0 && handle as isize != -1` with the type-safe idiom `!handle.is_null() && handle != INVALID_HANDLE_VALUE`.  Also pinned `windows-sys` to `=0.59.0` exact and added CI job `windows-build-check` that runs `cargo check --target x86_64-pc-windows-msvc` on every push (G29).
- `enrich` and `ingest --mode claude-code|codex` could be invoked in parallel against the same namespace and saturate the host (root cause of the 2026-06-03 276-load-average incident).  Added `lock::acquire_job_singleton` per `(job_type, namespace)` and a new `AppError::JobSingletonLocked { job_type, namespace }` exit-75 error.  A second concurrent invocation now fails fast instead of stacking 4 × N workers × 10 MCP processes (G28-B).
- `claude_runner::build_claude_command` now respects `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` — when set to an existing empty directory, the subprocess is spawned with `CLAUDE_CONFIG_DIR=<that dir>`, suppressing user-scoped MCP servers and the 8-10-process fan-out they cause.  We deliberately do not pass `--strict-mcp-config` / `--mcp-config '{}'` because [anthropics/claude-code#10787] documents that Claude Code CLI ignores both flags.  `CLAUDE_CONFIG_DIR` is the only mechanism upstream actually honours (G28-A).
- `retry` module gains a `CircuitBreaker` helper (with `AttemptOutcome::{Success,Transient,HardFailure}` and tests) that `enrich --retry-failed` can use to abort persistent-failure loops.  Transient / rate-limited errors do NOT count toward the threshold, so a provider that recovers is not penalised (G28-D).
- 3 pre-existing test failures in `src/commands/{history,list,read}.rs` that leaked `SQLITE_GRAPHRAG_DISPLAY_TZ` between parallel test threads and asserted hardcoded `1970-01-01T00:00:00` strings now parse the ISO output via `chrono::DateTime::parse_from_rfc3339` and compare `timestamp()` against `DateTime::UNIX_EPOCH` for timezone-agnostic assertions.  The full test suite is now green on every timezone (`UTC`, `America/Sao_Paulo`, `Europe/Berlin`, etc.) without per-test setup of the env var.

### Added
- `retry::CircuitBreaker` (struct + `record` / `is_open` / `reset`) — opt-in helper for bounded retry loops.  Rate-limited and timeout errors are explicitly excluded from the failure count.
- `lock::acquire_job_singleton(job_type, namespace, wait_seconds)` — process-wide singleton for heavy commands.
- `constants::JOB_SINGLETON_POLL_INTERVAL_MS = 1000` — backing interval for the singleton polling loop.
- `errors::AppError::JobSingletonLocked { job_type, namespace }` — exit 75, classified as retryable and with localised PT-BR message.
- CI job `windows-build-check` runs `cargo check --target x86_64-pc-windows-msvc --lib --all-features` to catch Windows regressions before publish.
- `tests/terminal_compile_windows.rs` — regression test that the public `terminal::init_console` and `should_use_ansi` stay callable; on Windows it also references the type-safe HANDLE check.
- `lock::tests` — 3 unit tests covering singleton namespace sanitisation, second-invocation blocking, and per-namespace isolation.

### Changed
- `enrich` emits a `tracing::warn!` (visible with `-v`) when `llm_parallelism > 4` recommending combining with `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` to keep subprocess fan-out manageable (G28-D, non-breaking).
- `Cargo.toml`: `windows-sys` pinned to `=0.59.0` exact (was range `0.59`).

## [1.0.67] - 2026-06-01

### Added
- `remember-batch` command — batch-create memories from NDJSON stdin in a single invocation with `--transaction` for atomicity, `--force-merge` for idempotent updates, `--fail-fast` to abort on first error (G08)
- `completions` command — generate shell completions for Bash, Zsh, Fish, PowerShell, and Elvish
- `read --id <N>` flag for direct memory lookup by integer `memory_id`, bypassing name resolution (G17)
- `read --with-graph` flag to include linked entities and relationships in the JSON response (G22)
- `enrich --llm-parallelism <N>` flag for parallel LLM worker threads (default 1, max 32) — reduces enrich wall-clock time proportionally (G19)
- `health` now detects super-hub entities (degree > 50) and reports `super_hub_count`, `super_hub_warning`, `top_hub_entity`, `top_hub_degree` in JSON output (G25)
- `health` now reports `non_normalized_count` and `normalization_warning` for entities not matching kebab-case (G24)
- `related` aliases: `--from`/`--to` for `--source`/`--target`, `related_memories` as field alias (G23)
- `claude_runner.rs` shared module — DRY extraction logic for `enrich` and `ingest-claude` subprocess management (G02)
- `claude_runner.rs` detects `terminal_reason: "max_turns"` and returns specific error instead of generic failure (G03)
- `enrich` passes `max_turns=7` to Claude subprocess, absorbing hook turn consumption (G01)

### Fixed
- `edit` now compares `body_hash` (blake3) before re-embedding — idempotent edits skip the ~1.5s embedding step (G15)
- `rename` now purges ghost soft-deleted memories occupying the target name before UPDATE — eliminates UNIQUE constraint crash (exit 10) that previously required `purge --retention-days 0` workaround (G16)
- `hybrid-search` rejects `--max-hops` and `--min-weight` without `--with-graph` with actionable error instead of silent discard (G20 partial)
- `recall` rejects `--max-hops` and `--min-weight` with `--no-graph` with actionable error instead of silent discard (G20 partial)
- `ingest` rejects contradictory NER flags and `--low-memory` with `--ingest-parallelism > 1` with validation error (G21 partial)
- `normalize-entities --dry-run` now computes real `merge_count_preview` instead of always 0 (G10)
- Entity name normalization maps ALL non-alphanumeric chars to hyphens, not just spaces/underscores (G11)
- Relationship deserialization accepts `type` as alias for `relation` via `#[serde(alias)]` (G12)
- `recall`, `hybrid-search`, `deep-research` accept `--limit` and `--top-k` as aliases of `--k` (G13)
- `enrich` `linked_entities` query provides graph context per entity for LLM prompts (G26)
- `enrich` supports all 13 operations including `relation-cleanup`, `duplicate-detection`, `type-audit`, `hub-analysis` (G27)
- V012 migration adds `created_at`/`updated_at` timestamps to relationships table with backfill trigger (G09)
- `memory_guard` removes /2 margin on memory threshold; lock ceiling uses dynamic 2*nCPUs (G18)

## [1.0.66] - 2026-05-29

### Fixed
- BUG-01 CRITICAL: `reclassify-relation` crash — removed `updated_at = unixepoch()` from 3 SQL UPDATE statements referencing non-existent column in `relationships` table
- BUG-02 HIGH: `link --create-missing` now normalizes entity names to kebab-case in both storage and JSON response (`created_entities` array)
- BUG-04 MEDIUM: `deep-research` word-pair decomposition for 3+ word queries without conjunctions — queries like "authentication JWT tokens" now generate multiple sub-queries
- BUG-05 LOW: `remember --body-file` defensive UTF-8 handling — invalid byte sequences replaced with U+FFFD instead of process abort
- BUG-06 HIGH: `link` now updates weight of existing relationships and reports actual DB weight in JSON response (previously returned requested weight while keeping old value)
- HIGH-01 CRITICAL: `deep-research` evidence chains fixed — BFS seeds limited to top-5 memories by score, preventing seed flooding that made all entities seeds with no room for BFS expansion
- HIGH-01b: `deep-research --graph-min-score` default lowered from 0.2 to 0.05 to avoid discarding valid results in small databases; warns when RRF fusion returns 0 despite KNN/FTS hits
- HIGH-04: `link --max-entity-degree` warning now uses `emit_progress` (always visible on stderr) instead of `tracing::warn` (requires `-v`)
- HIGH-08: `deep-research` source classification now reports `hybrid` when both KNN and FTS matched, instead of always `knn`
- HIGH-12: `remember` and `ingest` now use `max_relationships_per_memory()` function (reads `SQLITE_GRAPHRAG_MAX_RELATIONS_PER_MEMORY` env var) instead of hardcoded constant; `remember --graph-stdin` truncates with warning instead of rejecting

### Added
- `edit --type` flag to change memory type without re-creating (HIGH-10)
- `deep-research --mode` reserved field (`none` default; `claude-code`/`codex` planned for v1.1.0) (HIGH-06)
- `deep-research --max-cost-usd` reserved field for future LLM cost tracking (HIGH-09)
- `deep-research` `graph_context` field in JSON response with entities and relationships from result memories (MEDIUM-01b)
- `deep-research` 7 `tracing::debug!` calls in `execute_sub_query()` for diagnostics with `-vv` (HIGH-07)
- `graph --format json` now includes `entities` alias field alongside `nodes` for LLM agent compatibility (HIGH-05)
- `list --json` now includes `memories` alias field alongside `items` for LLM agent compatibility (HIGH-05)
- `graph entities --json` now includes `description` field per entity (HIGH-11)
- `health --json` now includes `vec_memories_missing` and `vec_memories_orphaned` counts (MEDIUM-09)
- `history --diff` first version now reports baseline `changes: {added_chars: N, removed_chars: 0}` instead of `null` (MEDIUM-02)
- Entity type validation suggests mapping when memory types are used as entity types: reference→concept, document→file, user→person (HIGH-10c)
- `remember` after_long_help documents positional arg limitation and entity_type vs memory_type taxonomy (HIGH-10b)
- `debug-schema` command renamed from `__debug_schema` for discoverability (HIGH-03, still hidden from `--help`)
- `fuzz/` directory with cargo-fuzz targets for graph-stdin JSON and name validation (LOW-01)
- `mutants.toml` configuration for cargo-mutants (LOW-02)
- CI coverage job with 75% threshold enforcement (LOW-03)

### Changed
- `deep-research --graph-min-score` default: 0.2 → 0.05

### Data Migration (recommended after upgrade)
- Run `reclassify-relation --from-relation applies-to --to-relation applies_to --batch --yes` (and similarly for depends-on, tracked-in) to normalize legacy kebab-case relations to snake_case (HIGH-13)
- Run `normalize-entities --yes` to merge mixed-case entity duplicates (HIGH-13)

## [1.0.65] - 2026-05-28

### Added
- `reclassify-relation` command — bulk or single reclassification of relationship types with `UPDATE OR IGNORE` + `DELETE` duplicate merging, `--dry-run`, `--filter-source-type`/`--filter-target-type` (GAP-13)
- `normalize-entities` command — normalizes existing entity names to lowercase kebab-case and auto-merges near-duplicate collisions, with `--dry-run`/`--yes` (GAP-15)
- `enrich` command — LLM-augmented graph quality via `--mode claude-code|codex`, scan→judge→persist pipeline, 12 operations (memory-bindings, entity-descriptions, body-enrich and more), `--dry-run` previews without spawning the LLM, queue DB with resume/retry (GAP-14, GAP-18)
- `health` now reports `top_relation`, `top_relation_ratio`, `applies_to_ratio`, and `relation_concentration_warning` when one relation exceeds 40% of edges (GAP-13)
- `deep-research` flags `--rrf-k`, `--graph-decay`, `--graph-min-score`, and `--max-neighbors-per-hop`
- `--max-entity-degree` warning on `link` and `remember` to flag super-hub growth (GAP-17)
- JSON schemas `deep-research`, `reclassify-relation`, `normalize-entities`, and `enrich-{phase,item-event,summary}`, plus `contract_36..39` and `schema_36..39` tests — restores 100% schema/contract coverage (GAP-01, GAP-02, GAP-03, GAP-04)

### Fixed
- GAP-07 CRITICAL: `deep-research` now computes a separate embedding per sub-query — decomposition was cosmetic because all sub-queries shared the original query embedding for KNN, returning identical results (also resolves GAP-10 centroid collapse and GAP-12 partial decomposition)
- GAP-08 CRITICAL: `deep-research` now fuses KNN, FTS5, and graph pools via Reciprocal Rank Fusion (new shared `storage::fusion`) instead of assigning FTS results a hardcoded score of 0.5
- GAP-11: `deep-research` graph-pool scoring incorporates seed score, hop decay, and edge weight, fused via RRF with a minimum-score filter
- GAP-09 HIGH: `deep-research` evidence chains are now directed seed→target paths (`from`, `to`, `path`, `total_weight`) filtered by discovered entities, instead of a flat global dump of the top-20 relationships
- GAP-15 HIGH: entity names are normalized to lowercase kebab-case on every write AND read path (`find_entity_id`, `rename-entity`, `reclassify-relation`, `prune-ner`, `enrich`) — validation runs on the raw name first so short ALL_CAPS NER noise is still rejected, then the normalized form is stored and looked up

### Changed
- GAP-17: graph traversal accepts an optional per-hop neighbor cap (top-K by weight); default behavior is unchanged
- hybrid-search RRF fusion extracted into the shared `storage::fusion` module (no behavior change)
- GAP-16: docs clarify that relations are accepted in kebab-case or snake_case and always stored and emitted as snake_case

## [1.0.64] - 2026-05-28

### Fixed
- BUG-1 HIGH: `ingest --mode claude-code` now disables hooks via `--settings '{"hooks":{}}'` for OAuth users and detects `terminal_reason: "max_turns"` — prevents Stop hooks from consuming extraction turns (was failing 65% of files for users with hooks configured)
- BUG-2 HIGH: `ingest --mode claude-code` now detects OAuth via `apiKeySource` from Claude Code init JSON and omits misleading `cost_usd` from NDJSON output — `--max-cost-usd` budget cap is ignored with warning for subscription users who are not billed per API call
- BUG-3 HIGH: `ingest --mode claude-code` and `--mode codex` now validate body size BEFORE sending to LLM subprocess — files exceeding 512 KB body cap are skipped with actionable warning instead of wasting LLM tokens on extraction that will be discarded
- `rename` and `rename-entity` now reject same-name renames with exit 1 (Validation) — prevents version inflation, unnecessary FTS5 sync, and wasted re-embedding

### Added
- `deep-research` command for parallel multi-hop GraphRAG research via heuristic query decomposition (up to 7 sub-queries), bounded fan-out with `tokio::task::JoinSet` and `Arc<Semaphore>`, 3-hop graph traversal, evidence chain assembly, and per-sub-query timeout — defaults calibrated against NovelHopQA, StepChain, HopRAG, and GraphRAG-Bench benchmarks (k=20, max-hops=3, max-sub-queries=7)

## [1.0.63] - 2026-05-27

### Fixed
- BUG-1 HIGH: `restore` no longer reverts memory name to version's original — preserves current name after rename, eliminates UNIQUE constraint crash (exit 10) when old name is occupied
- BUG-2 HIGH: `ingest --mode claude-code` and `--mode codex` now normalize relation strings via `normalize_relation()` before canonical check and DB insertion — eliminates false `non-canonical relation` warnings for kebab-case canonical values (`depends-on` → `depends_on`) and prevents mixed-format DB inconsistency
- FINDING-1: `edit` now re-generates vector embedding when body changes — `recall` and `hybrid-search` return accurate similarity scores after edit (parity with `restore` which already re-embeds)

### Added
- AUTHENTICATION section in `ingest --help` documenting OAuth-first principle for both `--mode claude-code` and `--mode codex`
- Auth failure detection: actionable `tracing::warn!` when Claude Code or Codex CLI authentication fails during ingest

## [1.0.62] - 2026-05-23

### Fixed
- G01 CRITICAL: `ingest --mode claude-code` now computes and persists vector embeddings — `recall` and `hybrid-search` find claude-code ingested memories (was creating memories with zero vec_memories/vec_chunks entries)
- G02: `validate_claude_version()` now compares against `MIN_CLAUDE_VERSION` (2.1.0) — rejects incompatible Claude Code versions with actionable error
- G03: `env_clear()` whitelist for `claude -p` subprocess now includes Windows-critical variables (`LOCALAPPDATA`, `APPDATA`, `USERPROFILE`, `SystemRoot`, `COMSPEC`, `PATHEXT`) via `#[cfg(windows)]`
- G04: `skipped` counter in claude-code ingest summary now counts pre-existing `done` entries in queue DB instead of always reporting 0
- G05: files exceeding 10MB stdin limit are rejected with specific error before spawning `claude -p`, preventing wasted API credits
- G06: memory names from Claude extraction are normalized via `derive_kebab_name()` — prevents non-kebab-case names from entering the database
- G07: invalid entity names from Claude extraction now emit `tracing::warn!` instead of being silently discarded
- G08: claude-code queue database (`.ingest-queue.sqlite`) now uses WAL journal mode for crash resilience
- G09: WAL checkpoint runs after claude-code ingest processing loop completes
- G10: `EXTRACTION_SCHEMA` now includes `additionalProperties: false` at root, entity, and relationship levels — compatible with both Claude Code and Codex structured output

### Added
- `ingest --mode codex` for LLM-curated entity/relationship extraction via locally installed OpenAI Codex CLI (`codex exec --json`)
- New ingest flags: `--codex-binary`, `--codex-model`, `--codex-timeout` for Codex CLI configuration
- `IngestMode::Codex` variant — users can choose between `--mode claude-code` (Anthropic) and `--mode codex` (OpenAI) per ingest
- JSONL parser for Codex CLI output with "last agent_message wins" pattern (verified against Paperclip production adapter)
- Token usage tracking for Codex ingest (input_tokens, output_tokens) — cost_usd unavailable from Codex CLI
- Full embedding pipeline for Codex-ingested memories (chunking, vec_memories, vec_chunks, vec_entities)
- 7 unit tests for Codex JSONL parser and schema validation

## [1.0.61] - 2026-05-23

### Fixed
- **B00 CRITICAL**: `ingest --mode claude-code` now uses `--dangerously-skip-permissions` instead of `--bare` — fixes OAuth authentication failure for Pro/Max subscription users
- **B00a**: `--max-turns` increased from 1 to 3 — Claude needs >1 turn for structured extraction
- **B07a**: memory source field changed from `"claude-code"` to `"agent"` — fixes CHECK constraint violation on insert
- **B01**: `--resume` flag now resets stuck `processing` files to `pending` for re-processing
- **B02**: `--retry-failed` flag now resets `failed` files to `pending` for retry
- **B03**: `--dry-run` now works with `--mode claude-code` — emits preview events without spawning Claude
- **B04**: subprocess timeout via `wait-timeout` crate — kills `claude -p` after `--claude-timeout` seconds (default 300)
- **B05**: error messages from `claude -p` now parsed from stdout JSON instead of empty stderr
- **B06**: re-ingesting same directory updates existing memories instead of UNIQUE constraint failure
- **B07**: cold-start `--json-schema` failure automatically retried once (workaround for Claude Code Issue #23265)
- **B08**: `claude -p` subprocess now runs with `env_clear()` + selective environment injection (security hardening)
- **B10**: fallback parsing of `result` field when `structured_output` absent (workaround for Claude Code Issue #18536)
- **B11**: FileEvent `index` field now uses consistent 0-based indexing across success and failure paths
- **B12**: invalid `entity_type` from Claude now emits `tracing::warn!` instead of silent discard
- **B13**: non-canonical relationship types now validated via `warn_if_non_canonical()` before insertion

### Added
- `--claude-timeout` flag for `ingest --mode claude-code` (default: 300 seconds per file)

### Changed
- `ingest --mode claude-code` uses `--bare` when `ANTHROPIC_API_KEY` is set (faster startup, no plugins), `--dangerously-skip-permissions` for OAuth users

## [1.0.60] - 2026-05-23

### Added
- `ingest --mode claude-code` for LLM-curated entity/relationship extraction via locally installed Claude Code CLI (`claude -p` headless with `--json-schema`)
- New ingest flags: `--mode`, `--claude-binary`, `--claude-model`, `--resume`, `--retry-failed`, `--keep-queue`, `--queue-db`, `--rate-limit-wait`, `--max-cost-usd`
- `IngestMode` enum: `none` (default body-only), `gliner` (NER), `claude-code` (LLM-curated)
- Queue DB (`.ingest-queue.sqlite`) for resumable claude-code ingestion with per-file tracking
- `memory-entities-reverse.schema.json` for `--entity` reverse lookup response validation
- `contract_33b_memory_entities_reverse` and `schema_33b_memory_entities_reverse` tests
- `delete-entity` and `merge-entities` recipes in COOKBOOK.md (EN/PT)
- `cleanup-orphans` and `prune-relations` entries in INTEGRATIONS.md (EN/PT)
- Ingest modes documentation in llms.txt, llms-full.txt, llms.pt-BR.txt, AGENTS.md, SKILL.md (EN/PT)

### Fixed
- D1: `test_exit_01_validation_invalid_name` — changed `"x"` to `"___"` (1-char names are valid memory names)
- D2-D3: i18n bilingual tests — changed `"---"` to `"___"` (`"---"` is a Clap flag separator, not a value)
- D4: `test_ingest_fail_fast_aborts_on_first_error` — use unreadable files (chmod 000) instead of `/proc` path; filter error envelope in NDJSON; `#[cfg(unix)]`
- D5: `prd_name_double_underscore_rejected` — changed `"---"` to `"___"`
- D6: `init_creates_11_migrations_v001_to_v011` — fixed vec literal from `[1..9]` to `[1..11]` matching actual 11 migrations
- D7: `readme_en_bash_examples_all_run` — added `#[cfg_attr(windows, ignore)]` for bash-only tests

## [1.0.59] - 2026-05-22

### Fixed
- `rename-entity` now validates `--new-name` via `validate_entity_name()`, rejecting names shorter than 2 characters, names with newlines, and short ALL_CAPS abbreviations
- `unlink.schema.json` updated from stale `relationship_id` to `relationships_removed` matching the actual `UnlinkResponse` struct
- `contract_16_unlink` test updated to match current response fields (`relationships_removed` instead of `relationship_id`, added `elapsed_ms`)
- `health -vv` now emits `tracing::info!` for the embedding model checkpoint, completing all 4 health check trace points

### Added
- `reclassify` response includes optional `description_updated: true` when `--description` is applied in single mode
- `contract_35_rename_entity` and `schema_35_rename_entity` tests for full contract and schema coverage of the rename-entity command
- E2E integration tests for entity name validation via CLI (`link --create-missing` and `rename-entity` paths)
- `rename-entity` added to `docs/schemas/README.md`, `INTEGRATIONS.md`, `llms.txt`, `llms-full.txt`, and their PT-BR counterparts

## [1.0.58] - 2026-05-21

### Fixed
- **C1 CRITICAL**: `remember --force-merge` now calls `sync_fts_after_update` — eliminates silent FTS5 index corruption on every force-merge operation
- **H1/H3 HIGH**: `merge-entities` uses `UPDATE OR IGNORE` for `memory_entities` — fixes UNIQUE constraint failure when source and target share memory bindings
- **M6**: `purge` response now includes `action` field (`"purged"` or `"dry_run"`) for consistency with all other commands

### Added
- **H2**: New `rename-entity` command — renames an entity preserving all relationships and memory bindings, re-embeds vector
- **M3**: `memory-entities --entity <name>` reverse lookup — lists all memories bound to a given entity
- **L6**: `reclassify --description` flag — updates entity description in single mode
- **H4**: Entity name validation — rejects names with newlines, shorter than 2 characters, or short ALL_CAPS abbreviations (NER noise)

### Improved
- **L1**: `fts --help` now shows EXAMPLES section for subcommands
- **L3**: `health` command emits `tracing::info!` at key checkpoints for `-vv` debugging
- **L2**: `reclassify --help` now lists all valid entity types
- **M1**: Documentation fix: `history --diff` JSON field is `changes` (not `diff`)

## [1.0.57] - 2026-05-21

### Fixed
- `merge-entities` no longer crashes with UNIQUE constraint violation when source entities share identical relationships — uses `UPDATE OR IGNORE` + cleanup instead of bare UPDATE (BUG-1).
- `memory-entities` SQL query now uses correct column `e.type` instead of non-existent `e.entity_type` (BUG-2).
- `--clear-body` flag in `remember` no longer blocked by empty body validation — the guard now recognizes explicit clear intent (BUG-3).
- `fts rebuild` and `fts check` now call `PRAGMA wal_checkpoint(TRUNCATE)` after write operations, consistent with all other write commands (G1, G2).
- `delete-entity --cascade` now recalculates degree for all adjacent entities after removing relationships, preventing stale degree values (G3).
- `merge-entities` now recalculates degree for the target entity AND all adjacent entities, not just the target (G4).
- `prune-ner` destructive path now executes COUNT and DELETE within the same transaction, eliminating race condition under concurrent access (G5).
- `backup` now uses atomic tempfile-rename pattern via `NamedTempFile::persist` — interrupted backups no longer corrupt existing destination files (G6).
- `backup` now logs chmod errors via `tracing::warn!` instead of silently discarding them (G7).
- `reclassify --batch` now emits `tracing::warn!` when `--from-type` matches zero entities, helping users detect typos in type names (G8).
- `emit_error_json` now writes a fallback JSON string manually if `serde_json` serialization fails, guaranteeing the stdout JSON contract is never violated (G11).
- `list --limit 0` now returns exit 1 validation error instead of silently returning empty results indistinguishable from an empty database (G12).
- `fts rebuild` now checks that the `fts_memories` table exists before attempting rebuild, returning a clear validation error on fresh databases (G16).

### Changed
- `backup` destination is now written atomically via tempfile-rename; the `tempfile` crate was promoted from dev-dependency to runtime dependency.
- 5 JSON schemas corrected: `merge-entities`, `delete-entity`, `reclassify`, `prune-ner` schemas now include the `namespace` field; `fts-stats` schema removed phantom `action` field that the struct does not emit.
- 9 new contract tests (contract_26–contract_34) and 9 new schema validation tests (schema_26–schema_34) added for all v1.0.56 commands.

## [1.0.56] - 2026-05-21

### Added
- `fts rebuild` command rebuilds the FTS5 full-text search index from scratch (GAP-07).
- `fts check` command runs FTS5 integrity-check without modifying the index (GAP-07).
- `fts stats` command shows FTS5 index statistics: row count, shadow pages, functional status (GAP-32).
- `backup` command creates a safe copy of the database using the SQLite Online Backup API (GAP-20).
- `delete-entity` command removes an entity and cascades to all relationships and NER bindings (GAP-17).
- `reclassify` command changes entity type individually or in bulk via `--from-type`/`--to-type --batch` (GAP-18).
- `merge-entities` command merges multiple source entities into a single target, moving all edges (GAP-19).
- `memory-entities` command lists entities linked to a specific memory (GAP-22).
- `prune-ner` command removes NER bindings from `memory_entities` table per entity or globally (GAP-16).
- `--dry-run` flag in `remember` validates input and reports planned actions without persisting (GAP-26).
- `--clear-body` flag in `remember` explicitly clears body during `--force-merge` (GAP-08/09).
- `--strict-relations` flag in `link` rejects non-canonical relation types with exit 1 (GAP-15).
- `--sort-by degree|name|created_at` and `--order asc|desc` flags in `graph entities` (GAP-25).
- `--skip-fts` flag in `optimize` to skip FTS5 rebuild (GAP-06).
- `--max-name-length` flag in `ingest` to configure name truncation limit (GAP-34).
- `fts_degraded`, `fts_error` fields in `hybrid-search` JSON for graceful FTS5 degradation (GAP-04).
- `fts_auto_rebuilt` field in `hybrid-search` JSON when FTS5 is auto-repaired on corruption (GAP-05).
- `normalized_score` field in `hybrid-search` JSON for cross-method score comparability (GAP-12).
- `vec_distance`, `fts_bm25` raw score fields in `hybrid-search` JSON (GAP-30).
- `fts_query_ok` field in `health` JSON verifies FTS5 is functionally queryable, not just structurally present (GAP-02).
- `sqlite_version` field in `health` JSON reports bundled SQLite version (GAP-28).
- `model_name`, `model_variant` fields in `daemon --ping` response (GAP-29).
- `degree` field in `graph entities` JSON via COUNT subquery (GAP-13).
- `body_length` field in `list` JSON (GAP-14).
- `body_length` field in `ingest` NDJSON per-file events (GAP-27).
- `total_count`, `truncated` fields in `list` JSON response (GAP-11).
- `warnings` field in `link` JSON response for non-canonical relation warnings (GAP-15).
- `--diff` flag in `history` includes character-level change summary between versions (GAP-23).
- JSON error envelope on stdout for all error paths: `{"error": true, "code": N, "message": "..."}` (GAP-03).

### Fixed
- FTS5 external-content sync implemented in `edit`, `rename`, and `restore` handlers via `sync_fts_after_update()` — fixes silent FTS5 index corruption where edited/renamed memories were invisible to full-text search (GAP-01 root cause).
- `hybrid-search` no longer aborts when FTS5 is corrupted — falls back to vector-only results with `fts_degraded: true` (GAP-04).
- `hybrid-search` skips FTS5 query entirely when `--weight-fts 0.0` instead of executing and failing (GAP-04).
- `hybrid-search` auto-rebuilds FTS5 index on "malformed" errors and retries once before degrading (GAP-05).
- `health --json` now performs a functional FTS5 MATCH query smoke test instead of only checking table existence in `sqlite_master` (GAP-02).
- `optimize` now rebuilds FTS5 index after `PRAGMA optimize` (GAP-06).
- `--force-merge` with empty body preserves existing body instead of destroying it — use `--clear-body` to explicitly clear (GAP-08/09).
- `--type` and `--description` are now optional with `--force-merge` — inherited from existing memory when omitted (GAP-10).
- `list --json` default limit changed from 50 to all memories — text output retains default 50 (GAP-11).
- `unlink` `--relation` is now optional — omitting it removes all relationships between the pair (GAP-24).
- `unlink` supports `--entity X --all` for bulk removal of all edges of an entity (GAP-24).
- `ingest` auto-prefixes names starting with digits with `doc-` instead of rejecting (GAP-35).
- Weight extremes (>= 0.95 or <= 0.05) now emit `tracing::warn!` (GAP-36).
- Entity type "memory" emits `tracing::warn!` when name collides with existing memory (GAP-33).

## [1.0.55] - 2026-05-17

### Fixed
- SKILL.md (EN+PT): export summary field corrected from `total` to `exported` to match actual JSON output from `ExportSummary` struct (G1).
- SKILL.md (EN+PT): `list` response-level fields corrected — removed nonexistent `total`, `limit`, `offset` fields; actual response contains only `items[]` and `elapsed_ms` (G2).
- SKILL.md (EN+PT) and CLAUDE.md: `--tz` with invalid timezone now correctly documented under exit 2 (Clap argument parsing) instead of exit 1 (application validation). Clap's `FromStr` for `chrono_tz::Tz` validates before application code runs (G3).
- SKILL.md (EN+PT): exit code 2 added to exit code table with description covering Clap argument parsing errors including invalid timezone values (G3+G4).
- SKILL.md (EN+PT): `stats` response now documents legacy alias fields `db_bytes`, `edges`, `memories_total`, `entities_total`, `relationships_total` (G6).
- AGENTS.md (EN+PT): `--tz` invalid IANA timezone corrected from exit 1 to exit 2; `bad timezone` moved from exit 1 to exit 2 description; `stats` legacy aliases documented.
- HOW_TO_USE.md (EN+PT): export summary field corrected from `memories_total` to `exported`.
- COOKBOOK.md (EN+PT): exit code count updated from 16 to 17; exit 2 added to exit code table and bash case example.
- SKILL.md, AGENTS.md, CLAUDE.md (EN+PT): `--min-weight` default corrected from 0.0 to 0.3 to match `src/commands/hybrid_search.rs:60`.
- README.md (EN+PT): exit code 2 added to exit code table — was missing between exit 1 and exit 9.
- README.md (EN+PT), llms.txt (EN+PT): spurious exit code 73 (`EX_NOPERM`) removed — not implemented in source code; only 17 exit codes exist (0-77).

## [1.0.54] - 2026-05-17

### Fixed
- WAL checkpoint TRUNCATE added to `prune-relations` — last remaining write command without checkpoint (H1).
- `remember --graph-stdin` with empty body and no entities now correctly returns exit 1 (Validation) instead of silently creating an inert memory with zero chunks (H2).
- `list` and `export` JSON output now includes `memory_type` field alongside `type`, consistent with `read` (H3). Agents parsing `.memory_type` no longer receive null.

### Changed
- `Vec::with_capacity()` applied in 9 additional cold paths: ingest file listing, recall graph matches, related results, hybrid-search graph matches, graph-export hops, cache entries, remember warnings, URL extraction, embedder candidates (M2).

## [1.0.53] - 2026-05-15

### Fixed
- WAL checkpoint TRUNCATE after every write command prevents B-tree corruption when database is synced by Dropbox or similar cloud sync tools (C2). Commands affected: remember, edit, forget, ingest, link, unlink, rename, restore, cleanup-orphans, purge.
- `export` now accepts `--json` as hidden no-op flag, consistent with all other subcommands (H1).

### Changed
- `Vec::with_capacity()` applied in 12 additional production hot paths: tokenizer offsets, chunk splitting, graph BFS frontiers, GLiNER tensor allocation, candidate span collection, ingest extraction buffers, embedder batch planning, remember URL extraction (L1).

## [1.0.52] - 2026-05-15

### Breaking
- Exit code for `Duplicate` error changed from 2 to 9 to resolve collision with Clap argument parsing errors (L1). Agents routing on exit 2 for duplicate detection must update to exit 9.
- `forget` no longer emits JSON to stdout when memory is not found (M2). Previously emitted `{"action":"not_found",...}` + stderr error; now only emits stderr error + exit 4, consistent with `read`, `edit`, `history`, `rename`.

### Fixed
- `restore` JSON response now includes `action: "restored"` field, consistent with `edit`, `rename`, `forget` (H1).
- `--lang pt` now fully translates error message bodies to Portuguese, not just prefixes (H2).
- `ingest` on nonexistent directory returns exit 1 (Validation) instead of exit 14 (Io) (M1).
- `prune-relations --dry-run` now computes `entities_affected` count instead of hardcoded 0 (L2).

### Added
- `ingest` NDJSON events include `original_filename` field preserving the file basename before kebab-case normalization (H3).
- `ingest --dry-run` flag previews file-to-name mapping without loading ONNX model or persisting (M5).
- `prune-relations --show-entities` flag shows affected entity names during `--dry-run` (L2).
- New `export` subcommand streams all memories as NDJSON for portable backup/migration (L4).
- `health --json` includes `mentions_ratio` and `mentions_warning` when mentions dominate the graph above 50% (C2).

### Changed
- `Vec::new()` replaced with `Vec::with_capacity()` in 7 production hot paths: health checks, recall results, related traversal, purge warnings, GLiNER NMS, relationship builder, entity dedup (M3).

### Closed (false positives from gaps.md)
- M4: `recall` already has `--max-graph-results` flag to cap graph expansion independently from `--k`.
- L3: `graph entities --json` already returns `entity_type` field in the EntityItem schema.

## [1.0.51] - 2026-05-15

### Fixed
- `remember` and `remember --force-merge` on soft-deleted memory now returns exit 2 (Duplicate) with actionable message instead of exit 10 (Database/UNIQUE constraint). With `--force-merge`, soft-deleted memory is restored and updated in one step (M7).
- `SQLITE_GRAPHRAG_NAMESPACE` environment variable now respected by all commands. Previously 8 commands (`list`, `remember`, `read`, `edit`, `forget`, `history`, `rename`, `restore`) ignored the env var due to Clap `default_value = "global"` pre-filling the namespace argument (M8).

### Added
- `--max-rss-mb` flag for `remember` and `ingest`: aborts embedding if process RSS exceeds the threshold (default 8192 MiB). Prevents ONNX runtime from exhausting system memory on large documents (C1 mitigation).
- 6 new daemon unit tests covering exponential backoff capping, half-jitter range, version CAS transitions, socket name resolution, and state serialization roundtrip (M3).
- "Version Highlights" section in README (L3).

### Changed
- `recipe_01_bootstrap` nextest timeout raised to 180s in default profile to prevent false negatives in debug builds (M6).
- `--gliner-variant` help text now documents int8 precision trade-off (L4).
- `--namespace` help text on 8 commands now shows env var precedence.

## [1.0.50] - 2026-05-15

### Added
- New `prune-relations` subcommand for bulk-deleting relationships by type (H8). Supports `--dry-run`, `--yes`, `--namespace`, and `--json` flags. Includes `after_long_help` with usage examples.
- V011 migration adds `idx_relationships_ns_relation` index for efficient relation-type filtering.
- Daemon auto-restart on version mismatch (H7): CLI now detects when the running daemon is an older version and automatically restarts it before the first embedding request. Limited to one restart attempt per process to prevent loops.
- New constant `DAEMON_VERSION_RESTART_WAIT_MS` (5 seconds) for daemon restart timeout.
- New constant `CHUNK_BATCH_SIZE` (16) for future streaming embedding pipeline.

### Changed
- `warn_if_non_canonical` now called in `unlink` (H1) and `related` (H2) commands for consistency with `link`, `remember`, and `ingest`.
- `related --help` now documents the 12 canonical relation types and custom relation support (H6).
- `errors_msg::*` functions in `src/i18n.rs` always return English (H3). Portuguese translations remain in `app_error_pt` for stderr via `localized_message_for()`. JSON stdout is now a fully deterministic English-only API contract.
- `Vec::with_capacity()` applied in `graph.rs`, `ingest.rs`, `link.rs` where sizes are predictable (M2).
- `.iter().cloned().collect()` replaced with `.iter().copied().collect()` for i64 values in `graph.rs` BFS (M1).
- Graph export now emits `tracing::warn!` when edges reference missing entities instead of silently dropping them (C2).
- Portuguese error string in remember.rs multi-chunk path replaced with English (H3).

### Fixed
- `graph_export.rs` silent edge discard: orphaned edges now logged with entity IDs and relation type (C2).
- `unlink` and `related` commands now warn on non-canonical relations for consistency (H1, H2).
- `errors_msg` module no longer returns Portuguese strings that leak into JSON stdout (H3).
- `MIGRATION.md` updated with `.items` to `.entities` rename note (v1.0.44) and v1.0.49/v1.0.50 changes (L2).
- Schema version bumped to 11 to match V011 migration.

### Closed (false positives from gaps.md)
- H4: SystemTime in daemon jitter was already fixed in v1.0.43 (uses fastrand). `now_epoch_ms()` legitimately uses SystemTime for epoch timestamps.
- H5: EntityType is already a strict Clap `value_enum` enum with 13 validated variants.
- M4: Ingest NDJSON streaming was already implemented via `mpsc::sync_channel`.
- L1: All 28 subcommands already have `after_long_help`.
- M5: GLiNER int8 failure on short texts is a model quantization limitation, not a code bug.

## [1.0.49] - 2026-05-15

### Changed
- Relation vocabulary is now extensible: `link`, `unlink`, `related`, `remember --graph-stdin`, and `ingest` accept any snake_case/kebab-case relation string, not just the 12 canonical values. Non-canonical relations emit a `tracing::warn!` for discoverability but are accepted without error.
- V010 migration removes the `CHECK(relation IN (...))` constraint from the `relationships` table.
- `RelationKind` Clap `ValueEnum` enum replaced by `String` with `parse_relation` value parser in `src/parsers/mod.rs`.
- Duplicated `is_valid_relation()` in `remember.rs` and `ingest.rs` consolidated into shared `parsers::validate_relation_format()`.

## [1.0.48] - 2026-05-14

### Fixed
- `--graph-stdin` no longer silently disables NER extraction when combined with `--enable-ner` and an empty `entities` array; the NER guard now checks actual entity presence instead of input source.
- GLiNER ONNX inference: `span_mask` tensor now correctly uses `tensor(bool)` instead of `tensor(i64)`, fixing the type mismatch that caused all GLiNER model variants to fall back to regex-only extraction silently.
- `ingest` now reports `status: "skipped"` with `action: "duplicate"` (not `status: "failed"`) for duplicate memories, correctly incrementing `files_skipped` instead of `files_failed`.
- `ingest` on a nonexistent directory now returns exit code 14 (Io) instead of exit code 4 (NotFound), matching the documented exit code semantics for filesystem errors.
- `daemon --ping` now emits a `tracing::warn!` when the running daemon version differs from the CLI binary version, prompting the user to restart.
- `--skip-extraction` now emits a deprecation warning when used alone (NER is disabled by default since v1.0.45).
- `extraction_method` field in `remember` JSON response is now set to `"none:extraction-failed"` when NER extraction errors out, instead of being absent (`null`).

### Added
- Schema `docs/schemas/ingest-file-event.schema.json` for per-file NDJSON event emitted by `ingest`.
- Schema `docs/schemas/ingest-summary.schema.json` for the final summary NDJSON line emitted by `ingest`.
- `extraction_method` and `original_name` fields added to `docs/schemas/remember.schema.json`.
- GLiNER zero-shot NER section in README, README.pt-BR, INTEGRATIONS, AGENTS, COOKBOOK, HOW_TO_USE (EN + PT).
- Ingest NDJSON status documentation (`indexed`/`skipped`/`failed`) in README and README.pt-BR.
- `after_long_help` examples for `init`, `recall`, and `remember` subcommands.
- `extraction_method`, `--skip-extraction` deprecation, and daemon version mismatch documentation across all doc files and skills.

## [1.0.47] - 2026-05-14

### Changed
- Replace BERT NER (Davlan/bert-base-multilingual-cased-ner-hrl) with GLiNER zero-shot NER (onnx-community/gliner_multi-v2.1 via ONNX); removes candle-core, candle-nn, candle-transformers dependencies and adds ndarray.
- `extraction.rs` reduced from 2,314 to ~900 lines after removing the BERT pipeline and tokenizer logic.
- NER now resolves 13 domain-specific entity types (`person`, `organization`, `location`, `date`, `project`, `tool`, `file`, `concept`, `decision`, `incident`, `dashboard`, `issue_tracker`, `memory`) instead of the 4 fixed BERT types (PER/ORG/LOC/DATE).

### Added
- `--gliner-variant` flag on `remember` and `ingest` selects the ONNX weight variant: `fp32` (default, 1.1 GB, best quality), `fp16` (580 MB), `int8` (349 MB), `q4` (894 MB), `q4f16` (472 MB).
- `SQLITE_GRAPHRAG_GLINER_VARIANT` env var as persistent override for `--gliner-variant`.
- `SQLITE_GRAPHRAG_GLINER_THRESHOLD` env var to tune the entity confidence threshold (float, default `0.5`).
- `SQLITE_GRAPHRAG_GLINER_MODEL` env var to override the default model repository identifier.

## [1.0.46] - 2026-05-14

### Fixed
- `SQLITE_GRAPHRAG_ENABLE_NER=1` now works correctly; previously only `true`/`false` were accepted by the Clap bool parser, causing exit 2 for `1`/`yes`/`on`. New `parse_bool_flexible` value parser accepts `1`/`true`/`yes`/`on` (truthy) and `0`/`false`/`no`/`off` (falsy), case-insensitive.
- FTS5 query preprocessing now sanitizes special characters (`"`, `*`, `(`, `)`, `^`, `:`) and filters FTS5 keywords (`OR`, `AND`, `NOT`, `NEAR`) from user queries, preventing syntax errors on malformed input.
- `--enable-ner` combined with `--skip-extraction` now emits a `tracing::warn!` instead of silently ignoring the contradiction; `--enable-ner` takes precedence.
- 9 pre-existing integration test failures fixed: 4 auto-init tests updated (health, stats, recall, vacuum), 1 daemon help assertion updated (hidden `--json` flag), 1 rename normalization test updated, 3 schema contract tests fixed.
- 7 JSON schemas updated to match current CLI output: `remember.schema.json` (+3 fields), `read.schema.json` (metadata type), `history.schema.json` (metadata type + deleted field), `purge.schema.json` (oldest_deleted_at type + message field), `hybrid-search.schema.json` (+rrf_score), `related.schema.json` (+name, +max_hops), `health.schema.json` (+memories_total in counts).

### Added
- `parse_bool_flexible` in `src/parsers/mod.rs` for reusable flexible boolean parsing in Clap env var integration.
- 4 new E2E integration tests in `tests/v1045_features.rs`: FTS5 compound term search (hyphenated, dotted) and env var NER acceptance (`=1`, `=true`).
- 9 new unit tests: 3 for `parse_bool_flexible`, 6 for FTS5 special char/keyword sanitization.

## [1.0.45] - 2026-05-13

### Changed
- **S5** BERT NER extraction is now disabled by default. Pass `--enable-ner` or set `SQLITE_GRAPHRAG_ENABLE_NER=1` to activate. The `--skip-extraction` flag is kept as a hidden no-op for backwards compatibility.

### Added
- **A1** FTS5 query-time preprocessing: compound terms containing `-`, `.`, `_`, `/` (e.g. `graphrag-precompact.sh`, `v1.0.44`) are now converted to phrase + prefix OR expressions before MATCH, fixing zero-result searches on technical identifiers. Zero schema migration required.
- `--enable-ner` flag on `remember` and `ingest` commands to opt into BERT NER entity/relationship extraction.
- `SQLITE_GRAPHRAG_ENABLE_NER` environment variable as persistent override for `--enable-ner`.
- 6 new unit tests for `preprocess_fts_query()` and FTS5 compound term search.

### Documentation
- All 10 documentation files updated to reflect `--enable-ner` replacing `--skip-extraction` as the active flag.
- Environment variable table in README/README.pt-BR now includes `SQLITE_GRAPHRAG_ENABLE_NER`.
- SKILL.md (EN/PT), AGENTS.md (EN/PT), COOKBOOK.md (EN/PT), HOW_TO_USE.md updated.

## [1.0.44] - 2026-05-13

### Fixed
- **B1** `README.md` and `README.pt-BR.md`: removed inline `#` comments from shell code blocks used as daemon-stop examples; these caused `# comment` to be parsed as a command argument in `tests/readme_examples_executable.rs:130-131`, breaking 2 nextest cases.
- **C1** `hybrid-search --with-graph` was a no-op: the flags `--with-graph`, `--max-hops`, and `--min-weight` were accepted but never wired into the handler; `graph_matches` was hardcoded to `[]`. Now performs graph traversal using `traverse_from_memories_with_hops`, matching the `recall` command behaviour.
- **C2** `link` command false documentation: `after_long_help` and `--from` doc comment claimed entities were "created implicitly by prior `link` calls" — this was false; the command returned exit 4 for missing entities. Documentation corrected; `--create-missing` flag added (see Added).
- **C3** `link.schema.json` was stale: listed removed `source`/`target` fields, wrong `action` enum (`"updated"` instead of `"already_exists"`), and `elapsed_ms` missing from `required`. Schema rewritten to match the actual Rust struct.
- **H1-old** Stopword list expanded with 12 additional entries (`OBSERVEI`, `PREFERIR`, `REMOVIDAS`, `EOF`, `GNU`, `MCP`, `TUI`, `NDJSON`, `PID`, `PGID`, and 2 others) that leaked into entity extraction results; previous list covered only the most common Portuguese stop tokens.
- **H2-old** CHANGELOG `H5` entry corrected: wrong variant list (`Person, Organization, Location, Technology, …`) replaced with the 13 canonical `EntityType` variants as declared in `src/entity_type.rs:19-33`.
- **H3-old** `related` subcommand: bidirectional fallback now surfaces reverse-direction relations (`B→A`) when no `A→B` edge exists, preventing silent empty results on asymmetric graphs.
- **H4-old** `rename` subcommand: memory name now accepted as a positional argument (`rename old-name new-name`) in addition to the existing `--name`/`--new-name` flags; mirrors UX of `forget`/`restore`.
- **H1** `graph entities` JSON response: renamed top-level array key from `items` to `entities` (BREAKING). The command is called `graph entities` so `.entities[]` is the natural jaq accessor. Schema updated accordingly.
- **H2** `link` `after_long_help` jaq example corrected: was `graph --format json | jaq '.nodes[].name'` (snapshot format), now `graph entities | jaq '.entities[].name'` (dedicated subcommand).
- **M1-old** Aggregate truncation now emits `tracing::warn!` when the entity or relationship list exceeds `MAX_ENTITIES_PER_MEMORY`, making silent data loss visible in debug logs.
- **M1** `ingest.rs` production `expect()` replaced with `AppError::Internal`: the panic-on-invariant-violation at line 858 now propagates a proper error instead of crashing.
- **M2** Release profile hardened: added `panic = "abort"` and changed `lto = true` to `lto = "fat"` in `[profile.release]`.
- **M3-old** `list` cache invalidation: `--include-deleted` flag now correctly busts the page cache when toggled mid-session.
- **M3** Portuguese comment in `Cargo.toml` translated to English (language policy compliance).
- **M6-old** `list --include-deleted` output now includes `deleted_at` field in JSON schema and struct.

### Added
- **C2** `link --create-missing` flag: auto-creates entities that do not exist, defaulting to type `concept`. Optional `--entity-type` flag specifies the type for created entities. Response includes `created_entities` array (omitted when empty).
- **M2-old** `SQLITE_GRAPHRAG_EXTRACTION_MAX_TOKENS` env var documented in both README EN and PT-BR.
- **M5-old** `vacuum --help` now shows a `NOTE` section explaining that `reclaimed_bytes` may report `0`.

### Removed
- Deleted `docs/CLAUDE.md`, `docs/CLAUDE.pt-BR.md`, `docs/PRD.md`, `docs/PRD.pt-BR.md`, `docs/AGENT_PROTOCOL.md`, `docs/AGENT_PROTOCOL.pt-BR.md`, and `docs/adr/0001-daemon-warmup-exception.md` (consolidated into CLAUDE.md at project root and external docs_rules/).

### Breaking Changes
- `graph entities` JSON: top-level key renamed from `items` to `entities`. Update jaq/jq queries accordingly: `.items[]` becomes `.entities[]`.

### Deferred
- **M4** NDJSON streaming input for `ingest` — officially deferred; see v1.0.43 Deferred section for context.

### Audit Notes
- `rusqlite` 0.39 release tracked via newreleases.io trustScore 9.1; `refinery` 0.9.1 still pins `rusqlite <=0.38`; upgrade deferred to v1.0.45+.

## [1.0.43] - 2026-05-03

### Fixed
- **B1** Incremental persistence in `ingest` eliminates the 2-phase blocking architecture. Phase B now flushes each record immediately after Phase A stages it, preventing complete data loss on large corpora (≥500 files) that previously timed out at 30 min with zero rows persisted. Closes 6+ months of reported stress-test failures.
- **B2** CHANGELOG retroactive label: `[Unreleased]` section in v1.0.42 release retroactively marked with correct label.
- **B3** Created `docs/PRD.md` and `docs/PRD.pt-BR.md` documenting product requirements baseline.
- **H1** TTY detection in `stdin_helper`: `is_terminal()` guard prevents blocking reads when stdin is a pipe or redirected file, fixing deadlock on non-interactive invocations.
- **H2** Ported 4 missing Portuguese i18n variants covering v1.0.26–v1.0.29 releases.
- **H3** `README.pt-BR.md` CHANGELOG links corrected; previously pointed to wrong anchor fragments.
- **H4** Added `EXAMPLES` section to `after_long_help` for 4 graph subcommands (`graph`, `graph stats`, `graph path`, `graph neighbors`).
- **H6** `SAFETY` comment in `src/daemon/` realigned to reference `docs/adr/0001-daemon-warmup-exception.md` instead of inline prose.
- **H7** `fastrand` jitter replaces `SystemTime`-based jitter in busy-retry backoff, eliminating potential clock skew panics on systems with coarse-grained clocks.
- **L1** `graph stats` `avg_degree` formula corrected: was dividing by node count, now correctly computes `2 * edge_count / node_count` (undirected graph convention).
- **L3** Removed stale "agent" from `--entity-type` help text; the enum now uses typed `EntityType` variants.
- **L4** Version references cleaned up across all `after_long_help` strings; removed stale `v1.0.x` pins.
- **L5** "indefinido" standardized to "undefined" in all PT i18n strings.

### Added
- **B3** `docs/adr/0001-daemon-warmup-exception.md` — formal ADR documenting the authorized daemon exception to the `rules_rust_cli_stdin_stdout.md` no-persistent-daemon rule.
- **H5** `EntityType` enum with 13 typed variants (`Concept`, `Date`, `Dashboard`, `Decision`, `File`, `Incident`, `IssueTracker`, `Location`, `Memory`, `Organization`, `Person`, `Project`, `Tool`) implementing `ToSql`/`FromSql` for rusqlite round-tripping.
- **H8** Formal ADR documenting the authorized daemon exception for warmup latency.
- **M6** `env_remove` for `LD_PRELOAD`, `LD_LIBRARY_PATH`, `LD_AUDIT`, and `DYLD_*` variants in subprocess spawns, preventing injected libraries from leaking into child processes.
- **M7** Half-jitter added to `storage` busy-retry loop; previously used fixed 100 ms delay which caused thundering-herd under concurrent writes.
- **M8** Two env vars (`SQLITE_GRAPHRAG_LOW_MEMORY`, `SQLITE_GRAPHRAG_INGEST_PARALLELISM`) documented in both README EN and PT-BR.
- **M9** Two output schemas (`docs/schemas/ingest.schema.json`, `docs/schemas/ingest-progress.schema.json`) added to README schema reference list.
- **L6** `MAX_ENTITIES_PER_MEMORY` is now configurable via `SQLITE_GRAPHRAG_MAX_ENTITIES_PER_MEMORY` env var (integer, default 50). Allows power users to raise the cap for dense technical documents without recompiling.

### Changed
- **ort/fastembed bump** Coordinated bump ort `2.0.0-rc.11` → `2.0.0-rc.12` and fastembed `5` → `5.13.4`. Required `src/embedder.rs` migration for ort module reshuffle (`execution_providers::CPU` → `ep::CPU`). Closes the deferred upgrade noted in v1.0.42 release notes.
- **M1+M2+M3** Eliminated unnecessary `.clone()` calls and added `Vec::with_capacity` pre-allocation in hot ingest and recall paths, reducing allocator pressure on large corpora.
- **M5** NaN handling in score normalization replaced `.expect("NaN")` with `.unwrap_or(0.0)`, eliminating potential panics on degenerate distance values.
- **L2** Alias normalization applied consistently across `link`, `unlink`, and `related` subcommands; hyphen and underscore forms now map to the same canonical relation key.

### Deferred to v1.0.44
- **M4** NDJSON streaming input for `ingest` — focus shifted to B1 architectural refactor during Wave 4; streaming input deferred to next cycle.

## [1.0.42] - 2026-05-03

### Fixed
- **HIGH 2** Migrated 14 Portuguese-language doc comments to English in `src/constants.rs` (5x), `src/commands/stats.rs` (3x), `src/commands/health.rs` (1x), `src/commands/read.rs` (2x), `src/commands/list.rs` (1x), `src/commands/hybrid_search.rs` (2x). Aligns with the inviolable language policy in `docs_rules/rules_rust.md`.
- **HIGH 3** Extended `language-check` CI gate regex (`.github/workflows/ci.yml:251`) to detect Portuguese prepositions, adjectives, and nouns without diacritics (`alias de`, `contrato documentado`, `migrado de`, `paralelo a`, `quando omitido`, etc.). Previously only verbs with diacritics were caught; the new pattern catches the 14 doc comments fixed in HIGH 2 with zero false positives in the current codebase.
- **LOW 3** `i18n` POSIX precedence: `LC_ALL=""` (empty string set) now falls through to `LC_MESSAGES`/`LANG` correctly via an explicit `is_empty()` guard inside the locale loop (`src/i18n.rs:60-78`). Previously the empty value was treated as a recognized-but-unparsed locale, breaking POSIX semantics in shells that export `LC_ALL=""`.

### Added
- **MEDIUM 1** GitHub Releases now include a prebuilt binary for `x86_64-apple-darwin` (Intel Mac) via the `macos-13` runner, alongside the existing `aarch64-apple-darwin` build. Closes the gap where Intel Mac users had no published binary.
- **LOW 1** `restore` command accepts the memory name as a positional argument (`restore foo`); the `--name` flag is preserved as an alternative form via `conflicts_with`. Mirrors the UX of `forget`/`related`.
- **LOW 2** `sync-safe-copy` accepts the destination path as a positional argument (`sync-safe-copy /path/snapshot.sqlite`); `--dest`/`--to`/`--output` flags preserved.
- **MEDIUM 4** `ingest --type` now defaults to `document` when omitted; `MemoryType` derives `Default` with `Document` as the default variant.
- **MEDIUM 5** `apply_secure_permissions` and `sync-safe-copy` now emit a `tracing::debug!` log on Windows explaining that NTFS DACL default already provides per-user access; closes the silent skip from previous releases.

### Changed
- **HIGH 1** Dropped `x86_64-unknown-linux-musl` target from the release matrix. `ort` (the ONNX runtime backend used by `fastembed`) does not ship a prebuilt for the musl target on either rc.11 or rc.12 (verified upstream via [ort-sys/build/download/dist.txt](https://github.com/pykeio/ort/blob/v2.0.0-rc.12/ort-sys/build/download/dist.txt)). Five consecutive releases (v1.0.37 to v1.0.41) failed on this job, blocking the GitHub Releases publish step. Alpine users should install via `cargo install sqlite-graphrag --locked` or use a glibc-based container (debian-slim, distroless/cc-debian12).
- **LOW 4** Bumped `clap` 4.5 → 4.6 (no API breaks observed). `rusqlite` (0.37) kept due to refinery 0.9.x hard-pinning rusqlite ≤0.38; `rayon` (1.10) kept to avoid MSRV bump risk; `ort`/`fastembed` coordinated bump deferred to v1.0.43 (requires `src/embedder.rs` migration for rc.12 module reshuffles `ort::tensor`→`ort::value`, `execution_providers`→`ep`).

### Audit Notes (deferred to v1.0.43)
- **AUDIT-B1-BLOCKER**, **AUDIT-D8-HIGH**, **AUDIT-AUDIT-06-HIGH** — `ingest --low-memory` 2-phase architecture refactor (Phase A → Phase B incremental persistence with NDJSON streaming) requires more design iteration; defer to next cycle.
- **AUDIT-MEDIUM 2** `ingest` content-hash deduplication requires schema migration v10 (new `content_sha256` column + index). Deferred to avoid bundling schema migrations with patch fixes.
- **AUDIT-MEDIUM 3 / C4 NER bias** BERT NER mis-classifies code identifiers (`TypeScript`, `AdapterExecutionResult`) as `organization`. Requires architectural decision (replace model, fine-tune, or post-process). Deferred.
- **AUDIT-D9-MEDIUM** Terminology drift `nodes/edges` (graph) vs `entities/relationships` (stats) persists; design decision needed before unification.

## [1.0.41] - 2026-05-02

### Fixed
- **AUDIT-D1** README EN+PT Quick Start (line 110) corrected: replaced misleading "Run `sqlite-graphrag init` first before any other command" with explicit statement that GraphRAG is enabled by default and runs automatically (auto-init via `ensure_db_ready()` in `src/storage/connection.rs:71-121`). `init` is now correctly described as OPTIONAL but recommended for first-use to pre-download the embedding model.
- **AUDIT-D2** README EN+PT Quick Start adds explicit "GraphRAG is enabled by default" callout, documenting auto-extraction (BERT NER on every `remember`/`ingest`) and daemon auto-spawn (on `recall`/`hybrid-search`).
- **AUDIT-D11** `docs/schemas/vacuum.schema.json` adds `reclaimed_bytes` to `properties` and `required` (handler in `src/commands/vacuum.rs` was already emitting this field, schema was out-of-sync).
- **AUDIT-D5** `Init` subcommand `after_long_help` now documents that `init` is OPTIONAL (auto-init is transparent) and that it warms a smoke-test embedding which auto-spawns the persistent daemon (~600s idle timeout). Closes the gap where the side effect was undocumented.
- **AUDIT-C3** `DERIVED_NAME_MAX_LEN = 60` moved from `src/commands/ingest.rs:48` to `src/constants.rs` next to `MAX_MEMORY_NAME_LEN = 80`. Single-source-of-truth restored, with a doc comment explaining why the ingest cap is stricter (collision suffix headroom).
- **AUDIT-AUDIT-04** `ingest` now emits three INFO-level progress markers via `tracing::info!`: phase A start (`stage_start` with file count and parallelism), phase A progress every 10 staged files (`stage_progress` with done/total), and phase B start (`persist_start`). Closes the visibility gap where users had no progress signal during long ingests.

### Audit Notes (deferred to v1.0.42)
- **AUDIT-B1-BLOCKER** `ingest --low-memory` with 495 files times out at 30 min (`exit 124`) with **zero rows persisted** because of 2-phase architecture (Phase A stages all files in memory before Phase B persists+emits). For corpora ≥500 files in single-thread mode the entire run is lost. Refactor to incremental Phase B persistence required.
- **AUDIT-D8-HIGH** Help promises NDJSON streaming "one JSON object per file" but stdout stays empty during all of Phase A (entire stage phase). Will be resolved together with AUDIT-B1-BLOCKER.
- **AUDIT-AUDIT-06-HIGH** No INFO progress markers during long ingests (only WARN truncation lines emitted). Visibility gap for users.
- **AUDIT-C3-MEDIUM** Constants `MAX_MEMORY_NAME_LEN = 80` (in `src/constants.rs:30`, used by `remember`) versus `DERIVED_NAME_MAX_LEN = 60` (hardcoded in `src/commands/ingest.rs:48`, used during file-name derivation). Single-source-of-truth violation.
- **AUDIT-C4-MEDIUM** NER produced edge `DuckDuckGo --mentions--> DuckD`. Sub-token boundary truncation creates partial entity names that pollute the graph silently.
- **AUDIT-D9-MEDIUM** Terminology drift: `graph --format json` returns `nodes/edges`; `stats` returns `entities/relationships`. Same concept, two contracts.

### Documentation
- All EN README additions mirrored in `README.pt-BR.md` (H2 section count preserved).

## [1.0.40] - 2026-05-02

### Fixed
- **H-A2** README documents `relation` values with hyphens (CLI input form: `applies-to`, `depends-on`, `tracked-in`); underscore form clarified as JSON storage representation. Mirrored in `README.pt-BR.md`.
- **H-M8** `chunks_persisted` contract clarified and unit-tested via `compute_chunks_persisted()` helper in `src/commands/remember.rs`. Single-chunk bodies live in the `memories` row itself (no `memory_chunks` insert), so `chunks_persisted = 0` for `chunks_created = 1` is correct by design. Schema and tests now document this invariant explicitly.
- **M-A3** Derived memory names from filenames apply Unicode NFD normalization plus combining-mark stripping before kebab-case sanitization (`src/commands/ingest.rs:944`). `açaí🦜.md` now produces `acai`-prefixed kebab name instead of dropping all non-ASCII characters.
- **M-A5** `recall` results expose a non-null `score: f32` field on every `RecallItem`, derived from vector distance via `RecallItem::score_from_distance()` and clamped to `[0.0, 1.0]`. Test ensures direct matches return `score = 1 - distance`.
- **M-A6** `history.versions[].action` is always populated (never `null`). `change_reason_to_action()` maps internal change reasons to past-tense labels (`created`, `edited`, `restored`, `renamed`).
- **M-A7** `deny.toml` registers explicit ignore entries for transitive RUSTSEC-2025-0119 (`number_prefix` via `indicatif`/`hf-hub`) and RUSTSEC-2024-0436 (`paste` via `tokenizers`/`text-splitter`) with upstream tracking links.

### Added
- **H-A1** `--low-memory` ingest flag plus `SQLITE_GRAPHRAG_LOW_MEMORY` env var (truthy values: `1`, `true`, `yes`, `on`) force `--ingest-parallelism 1`. Reduces RSS pressure (~40 % drop measured at 30-file ingest) at the cost of 3-4× wall-time. Precedence: CLI flag > env var > explicit `--ingest-parallelism N`. Override emits a `tracing::warn!` when an explicit higher parallelism is also provided.
- **H-A1** README adds a `## Memory Requirements` section documenting the ~2 GB ONNX runtime + BERT NER + fastembed model floor, scaling behaviour with default parallelism, the `--low-memory` mitigation, container/cgroup guidance, and a link to the upstream onnxruntime memory-growth tracking issue (microsoft/onnxruntime#22271).
- **M-A4** `remember --body` help and README document the 500 KB (512000-byte) inline body limit and recommend `--body-file` for larger inputs.
- **M-A10** README adds a `cache` subcommands table documenting `clear-models` as the sole subcommand.

### Documentation
- All EN README additions mirrored in `README.pt-BR.md` (H2 section count preserved at 24=24).
- `docs/schemas/recall.schema.json`, `docs/schemas/history.schema.json`, and `docs/schemas/remember.schema.json` updated to reflect the populated `score`, `action`, and `chunks_persisted` semantics.

### Deferred (tracked for v1.0.41)
- **M-A8** `rusqlite 0.37 → 0.39` upgrade blocked by `refinery 0.9.1`'s `rusqlite >=0.23, <=0.38` constraint plus the 0.38 `cache` feature-flag breaking change. Comment in `Cargo.toml` documents the rationale.
- **M-A9** `ort =2.0.0-rc.11 → =2.0.0-rc.12` upgrade blocked by `fastembed 5.13.2` hard-pinning rc.11. Coordinated bump (`fastembed 5.13.4` + `ort rc.12`) deferred; rc.12 also reshuffles modules (`ort::tensor` → `ort::value`, `execution_providers` → `ep`, `IoBinding` moved) which requires touching `src/embedder.rs`.

## [1.0.39] - 2026-05-02

### Fixed
- **B1** doctest assertion in `src/errors.rs::localized_message_for` (Portuguese localized message check)
- **H1** ingest pipeline parallelizes extract+embed via rayon (new `--ingest-parallelism` flag); NDJSON ordering preserved
- **H2** `build_relationships*` use index-based dedup `HashSet<(usize,usize)>`, eliminating O(N²) String clones
- **M1** README documents required flags for `remember` (--name, --type, --description)
- **M2** README documents `purge --retention-days` default (90 days) and `--retention-days 0` for full purge
- **M3** embedder serialization documented (parallelism lives in ingest.rs)
- **M4** daemon adds Semaphore-based concurrency limit; `worker_threads` scales with `available_parallelism().clamp(2, 8)`
- **M5** NER `seen` dedup uses `HashSet<u64>` (DefaultHasher), reducing String clones
- **M6** hot-path `format!` calls in extraction replaced with `String::with_capacity` pre-allocation
- **M7** `f32_to_bytes` SAFETY comment expanded with explicit invariants (no padding, lifetime, endianness)
- **M8** `remember.schema.json` lists `chunks_persisted` in required fields
- **M9** README documents empty-result conditions for `related`
- **M10** README documents daemon convention (flags vs subcommands, systemd-style)

### Documentation
- **L1** tokenizer expect message clarified ("OnceLock::set succeeded above; get cannot fail in this single-init path")
- **L2** extraction regex SAFETY comments standardized (regex_email/url/uuid)
- **L3** daemon Child detach SAFETY cross-references rules_rust_processos_externos.md
- **L4** README adds runnable Memory Lifecycle Quick Start (init→remember→recall→forget→purge)
- **L5** schema describes `chunks_created` vs `chunks_persisted` semantics
- **L6** ingest error-path clones naturally eliminated by 2-phase pipeline refactor
- **L7** acknowledged: `format!` count remains; further reduction is micro-optimization
- **L8** README adds "Storage Footprint" section explaining ~8× DB bloat for GraphRAG

### Dependencies
- Added `rayon = "1.10"` for ingest parallelization

## [1.0.38] - 2026-05-02

### Fixed
- **M2 (MEDIUM)**: `forget --json` now emits `deleted_at_iso` (RFC 3339 UTC) parallel to `deleted_at` (Unix epoch) when a memory is soft-deleted. Mirrors the existing pattern from `read --json` (`created_at`/`created_at_iso`, `updated_at`/`updated_at_iso`). Both fields use `#[serde(skip_serializing_if = "Option::is_none")]` so `not_found` outputs continue to omit them. `docs/schemas/forget.schema.json` updated to document both fields plus `action`.
- **M3 (MEDIUM)**: `ingest --json` per-file events now expose `truncated: bool` and `original_name: Option<String>`. When the derived memory name from a filename exceeds `DERIVED_NAME_MAX_LEN` (60 chars), `truncated=true` and `original_name` carries the pre-truncation value, surfacing on stdout what was previously emitted only as a `tracing::warn!` to stderr. Eliminates silent collisions in large datasets where filenames truncate to identical kebab-case prefixes. `derive_kebab_name` now returns `(String, bool, Option<String>)`; all 6 unit tests updated.
- **M5 (MEDIUM)**: `src/main.rs` flushes stdout and stderr immediately before each of the 6 `std::process::exit` calls. Previously buffered JSON or error output could be lost when the process exited under a broken pipe, terminal disconnect, or fast-shutdown signal. Both flushes are best-effort (errors ignored via `let _ =`) since the process is already terminating.
- **M6 (MEDIUM)**: `src/output.rs::emit_json`, `emit_json_compact`, `emit_text`, and `emit_error` now lock stdout/stderr, perform an explicit `flush()`, and silence `BrokenPipe` errors gracefully (return `Ok(())` instead of propagating). Matches GNU coreutils convention where pipelines like `sqlite-graphrag list --json | head -1` no longer trigger spurious panics or non-zero exit codes when the consumer closes early.
- **M7 (MEDIUM)**: `src/daemon.rs:660` falls back to `std::env::temp_dir()` instead of the hardcoded `"/tmp"` literal when neither `XDG_RUNTIME_DIR` nor `SQLITE_GRAPHRAG_HOME` are set. Cross-platform: returns `/tmp` on Unix, `%TEMP%` on Windows, and respects `TMPDIR` when set. Aligns with `docs_rules/rules_rust_multiplataforma_sistemas_operacionais.md`.
- **M8 (MEDIUM)**: New `src/stdin_helper.rs::read_stdin_with_timeout(secs)` enforces a 60-second deadline on `remember --body-stdin`, `remember --graph-stdin`, and `edit` body input. Implementation: worker thread + `std::sync::mpsc::channel` + `recv_timeout` (no async conversion needed). Returns `AppError::Internal` on timeout with a message indicating the pipe must close within the deadline. Previously `std::io::stdin().read_to_string()` would block indefinitely if an upstream process held the pipe open without sending data.
- **language policy bonus**: Translated one residual Portuguese runtime error in `src/tokenizer.rs` (`"tokenizer_config.json sem model_max_length"` → `"tokenizer_config.json missing model_max_length field"`) discovered during the H3 doc sweep. Audit gate `rg '[áéíóúâêôãõç]' src/` was already clean for tracing/error/doc surfaces; this was inside a regular `format!` string outside the prior gate scope.

### Added
- **H3 (HIGH, docs)**: 23 public items across 6 modules received Rust-idiomatic `///` doc comments in English (`# Examples`, `# Errors`, `# Panics` sections where applicable): `src/chunking.rs` (8 items: constants, `Chunk`, 5 chunking functions), `src/tokenizer.rs` (4 functions), `src/output.rs` (9 items: `OutputFormat`, `JsonOutputFormat`, `emit_*`, `RememberResponse`, `RecallItem`, `RecallResponse`), `src/paths.rs` (1: `AppPaths`), `src/pragmas.rs` (2: `apply_init_pragmas`, `apply_connection_pragmas`), `src/embedder.rs` (5 embedder helpers). `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features` now passes with zero warnings on these modules; one pre-existing private intra-doc-link in `src/embedder.rs` was repaired during the sweep.
- **B1 (BLOCKER, UX)**: New CLI flag `--autostart-daemon` (default `true`) on `recall`, `hybrid-search`, and other embedding-heavy subcommands, exposed via shared `DaemonOpts` struct flattened with `#[command(flatten)]` in `src/cli.rs`. Previously the only opt-out was the env var `SQLITE_GRAPHRAG_DAEMON_DISABLE_AUTOSTART=1`, undocumented in `--help`. The new flag takes precedence over the env var: passing `--autostart-daemon=false` skips daemon spawn unconditionally regardless of env. The env var still gates the default-true case for backward compatibility. `src/daemon.rs::should_autostart` is the single decision point; `autostart_disabled` was renamed to `autostart_disabled_by_env` for semantic clarity. `embed_query_or_local` and `request_or_autostart` signatures gained the `cli_autostart: bool` parameter; `embed_passage_or_local` and `embed_passages_controlled_or_local` pass `true` to preserve their existing behavior. `src/commands/daemon.rs` `after_long_help` extended with auto-spawn behavior documentation.
- **B1 docs**: README.md and README.pt-BR.md gained a new section "Daemon auto-spawn behavior" / "Comportamento de auto-spawn do daemon" explaining the three control mechanisms (CLI flag, env var, explicit `daemon` subcommand) with shell examples.
- **regression tests**: `tests/cli_integration.rs` (new file) covers four end-to-end scenarios: (1) `forget` JSON includes `deleted_at_iso` after soft-delete, (2) `ingest` event flags `truncated=true` with `original_name` when filename exceeds 60 chars, (3) `recall --autostart-daemon=false` does not spawn a daemon, (4) `recall` default behavior remains unchanged. `src/stdin_helper.rs` ships with one timeout-path unit test; `src/i18n.rs` regression tests for POSIX precedence (added in v1.0.37) remain green.

### Notes
- v1.0.37 was tagged in git and pushed to GitHub (commit `4a4be74`) but never published to crates.io; v1.0.38 is the public release that bundles those changes together with the 8 additional fixes above. The v1.0.37 changelog entry is preserved below for transparency on the git history.
- Out of scope (backlog v1.0.39+): refactor of 6 production `.clone()` calls in `src/extraction.rs` hot path (`Cow<'_, str>` or `Arc<str>` decision pending), `tokio::sync::Semaphore` bound on `spawn_blocking` calls in `src/daemon.rs`, and `rusqlite 0.37 → 0.39` upgrade investigation (pending `context7` review of breaking changes).
- Out of scope permanently (per user decision): JSON field deduplication (`id`/`memory_id`, `memories`/`memories_total`, `entities`/`entities_total`, `relationships`/`relationships_total`, `db_size_bytes`/`db_bytes`) — kept for stable consumer compatibility.
- The `daemon` deliberate orphan child detach (`src/daemon.rs:489-501`) is preserved as documented behavior; the 8-line `SAFETY` comment justifying the lifecycle (spawn lock + readiness file + `Stdio::null()`) remains the source of truth.

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
