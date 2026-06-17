## New in v1.0.83
### REQUIRED — Custom Provider Credential Preservation (ADR-0041)
- Six custom-provider env vars are now preserved when spawning `claude -p` or `codex exec` subprocesses. The preserved vars are `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, and `OTEL_EXPORTER_OTLP_ENDPOINT`. Enables Anthropic-compatible providers (Minimax/api.minimax.io, OpenRouter, AWS Bedrock, corporate gateways) without altering the OAuth-only mandate that continues to reject `ANTHROPIC_API_KEY`/`OPENAI_API_KEY`
- The OAuth-only guard at `claude_runner.rs:273`, `codex_spawn.rs:259`, `ingest_claude.rs:282`, and `extract/llm_embedding.rs:237-253` is preserved; the abort error message now references `ANTHROPIC_AUTH_TOKEN` and `~/.codex/auth.json` as legitimate resolutions when an operator mistakenly sets `ANTHROPIC_API_KEY`
- New shared helper `src/spawn/env_whitelist.rs` exposes `apply_env_whitelist(cmd, strict)` and `is_strict_env_clear()`. The three spawners (`claude_runner`, `codex_spawn`, `ingest_claude`) delegate instead of inlining the array, eliminating the drift between duplicated whitelists
- New global flag `--strict-env-clear` / `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` enables strict mode that preserves only `PATH`. Use in PCI-DSS, SOC2, HIPAA environments where credential forwarding via env vars is forbidden by policy. Default remains permissive (forwards the six custom-provider vars)
- NO new telemetry: the fix is silent. No `tracing::info!` macro logs which provider is in use. The no-leak audit test `audit_no_token_leak_in_subprocess_stderr` in `tests/claude_runner_env.rs` enforces that the literal token value NEVER appears in stdout or stderr even with `RUST_LOG=trace`
- 6 new regression tests in `tests/claude_runner_env.rs` cover: custom-provider propagation, OAuth-only abort preservation, codex base-URL inheritance, strict-mode credential dropping, no-leak audit, and one documented scenario left intentionally empty (claude env test) with the equivalent integration path covered for codex. All carry `#[serial_test::serial(env)]`
- Semantic distinction the fix resolves: `ANTHROPIC_API_KEY` (paid API key, PROHIBITED by ADR-0011), `ANTHROPIC_AUTH_TOKEN` (OAuth token for custom provider, PRESERVED), `OPENAI_API_KEY` (PROHIBITED), `OPENAI_BASE_URL` (PRESERVED), `ANTHROPIC_BASE_URL` (PRESERVED). The v1.0.69 mandate was correct; the v1.0.69 env-clear whitelist was overly broad
- See `docs/decisions/adr-0041-preserve-custom-provider-env.md` for the full architectural rationale and `docs/MIGRATION.md#migrating-to-v1083` for operator upgrade steps
- G58 partial resolution: custom-provider env vars route around OAuth quota contention, providing a deterministic fallback for `recall`/`hybrid-search` under official OAuth fatigue
# sqlite-graphrag for AI Agents (v1.0.79)

> Persistent memory for 27 AI agents in a single 6 MB Rust binary.
> v1.0.79 is **LLM-only and one-shot**: every `remember` / `ingest`
> spawns a headless claude code or codex CLI subprocess (OAuth, no
> MCP, no hooks). There is no daemon, no ONNX runtime, no local
> embedding model.

## v1.0.79 Architecture (LLM-Only)

The CLI is a thin orchestrator. Every embedding call spawns a
claude code or codex subprocess that returns an f32 vector of
the ACTIVE dimensionality in JSON — default 64 since v1.0.79
(G42/S1), configurable via `SQLITE_GRAPHRAG_EMBEDDING_DIM`
(range [8, 4096]); pre-existing databases keep their recorded
`schema_meta.dim` (e.g. 384). Since v1.0.79 calls are BATCHED
(`{items:[{i,v}]}`, chunks at 8, entity names at 25 at dim 64, dim-adaptive — G44) and run
under a bounded `Semaphore` fan-out (`--llm-parallelism`).
Every entity extraction call does the same with a different
output schema. The CLI never holds an embedding model in
memory; the LLM subprocess is the model.

The daemon infrastructure was removed in v1.0.76 and the last
remaining daemon code was deleted in v1.0.79.
The CLI is 100% one-shot: each embedding call spawns and
discards the LLM subprocess. There is no persistent process,
no socket, no model cache to manage.

For the full architectural rationale, see ADR-0019. For the
removal history, see ADR-0021.

## LLM Hardening (inherited from v1.0.69)

When spawning `claude code` headless, the CLI always passes:

```
--strict-mcp-config
--mcp-config '{}'
--settings '{"hooks":{}}'
--dangerously-skip-permissions
--output-schema <JSON schema for the response>
--model <claude-sonnet-4-6 | gpt-5.4>
-p <prompt>
```

For `codex`:

```
--json
--output-schema <JSON schema>
--ephemeral
--skip-git-repo-check
--sandbox read-only
--ignore-user-config
--ignore-rules
-c mcp_servers='{}'
--ask-for-approval never
--model <claude-sonnet-4-6 | gpt-5.4>
```

These flags are the canonical hardening set. They are tested
in `src/commands/claude_runner.rs::tests` and
`src/commands/codex_spawn.rs::tests` and must not be removed
without an ADR.

## OAuth Enforcement

The CLI ABORTS with `AppError::Validation` if `ANTHROPIC_API_KEY`
or `OPENAI_API_KEY` is set in the environment. The agent must
use the OAuth flow:

```bash
# First time
claude login   # or: codex login

# After login, verify
claude --version
codex --version
```

The two API-key env vars are also excluded from the env-clear
whitelist, so they cannot be passed through a parent process.
Agents that try to set them will see a clear validation error.


## CLI Flag Aliases (since v1.0.35)
- `recall` and `hybrid-search` accept `--limit` as an alias of `-k`/`--k`. Existing snippets below use `--k` and remain valid.
- `rename` accepts `--from`/`--to` as aliases of `--name`/`--new-name`.
- `rename` accepts positional arguments: `rename <old> <new>` (since v1.0.44)
- `related` accepts a positional name argument: `related <name>` (since v1.0.44)
- `graph entities` JSON response uses `entities` as the top-level array key (renamed from `items` in v1.0.44)
- `schema_version` JSON fields (`init`, `stats`, `migrate`, `health`) are emitted as JSON numbers since v1.0.35.


## New Commands in v1.0.56
### FTS5 Index Maintenance
- `fts rebuild --json` — rebuilds the FTS5 full-text search index from scratch; use after bulk imports or suspected index corruption
- `fts check --json` — runs FTS5 integrity-check and reports any inconsistencies; safe to run on live databases
- `fts stats --json` — returns FTS5 index statistics including row count, token count, and segment count
### Backup
- `backup --output <path> --json` — creates a consistent SQLite online backup using the SQLite Backup API; safe to run while the database is in use; existing destination is atomically replaced via tempfile-rename
### Entity Operations
- `delete-entity --name <entity> --json` — deletes an entity node; use `--cascade` to also remove all edges connected to the entity; without `--cascade` fails with exit 4 if edges exist
- `reclassify --name <entity> --new-type <type> --json` — changes the `entity_type` of an existing entity in place without touching its edges or memory links
- `merge-entities --names "a,b,c" --into <target> --json` — merges two or more source entities into a target entity; all edges from source nodes are redirected to the target; source nodes are deleted after merge
- `memory-entities --name <memory> --json` — lists all entity nodes linked to a given memory; returns the same schema as `graph entities` items
- `prune-ner --entity <name> --json` — removes all NER-derived bindings for a given entity name without deleting the entity node itself; useful for cleaning up low-quality auto-extracted entities

## New in v1.0.79
### REQUIRED — G42: Fast, Parallel, Batched LLM Embedding Pipeline
- The default embedding dimensionality dropped from 384 to 64 (MRL, arXiv 2205.13147). Precedence: `SQLITE_GRAPHRAG_EMBEDDING_DIM` env (range [8, 4096]) > `schema_meta.dim` of the opened database > 64. Pre-existing databases keep their recorded dimensionality unchanged — ZERO schema change.
- Embedding calls are BATCHED (G42/S2): N numbered texts per LLM call with the `{items:[{i,v}]}` schema; chunks batch at 8, entity names at 25 (calibration bases at dim 64; since G44 the batch adapts as clamp(base×64/dim, 1, base) — 384-dim databases use 1/4) — 39 subprocess spawns collapse into 4-5.
- Real bounded parallelism (G42/S3): `Arc<Semaphore>` + `JoinSet` fan-out; new `--llm-parallelism <N>` flag on `remember` (default 4), `ingest` (default 2) and `edit` (default 4), clamp [1, 32]; permits = min(flag, cpus, free RAM × 0.5 / 350 MB per worker, 32).
- `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL` selects the claude embedding model (G42/S5, symmetric to the codex var); `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` (default 300) bounds each LLM call, with `kill_on_drop(true)` on every subprocess.
- The embedding path uses an EMPTY `CLAUDE_CONFIG_DIR` by default (G42/S6): honours `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR`, else a managed `~/.local/state/sqlite-graphrag/claude-empty-config`; the MCP-isolation flags are silently ignored upstream (anthropics/claude-code#10787) and a populated `~/.claude` cost ~223k cache tokens per call (~40-50s → ~10-15s).
- `enrich --operation re-embed --limit N --resume` is the canonical one-shot re-embed path; `edit --force-reembed` regenerates one embedding without changing the body (G42/S9).
- No silent dimension normalisation (G42/C5): divergent vectors fail with an explicit error instead of being truncated or zero-padded.
- Panic-free signal handler (G42/S8): second signal exits 130 with ZERO I/O — eliminates the SIGABRT on orphaned processes.
### REQUIRED — G43: Dimensionality Adoption on Every Connection
- `open_rw` AND `open_ro` adopt `schema_meta.dim` on every database open, so `remember` / `edit` / `recall` / `hybrid-search` operate at the database dimensionality (pre-G43 they silently used the compiled default against pre-v1.0.79 384-dim databases, writing mixed-dim embeddings invisible to cosine).
- `init` no longer stamps `dim=384`; `rename-entity` records the real vector length via the canonical `upsert_entity_vec` writer.
### REQUIRED — Removals and Deprecations (v1.0.79)
- The `embedding-legacy` and `ner-legacy` features were REMOVED (ahead of the v1.1.0 schedule); every build is LLM-only.
- The remaining `daemon` code was DELETED; the CLI is 100% one-shot.
- GLiNER-era flags are formal no-ops with explicit `tracing::warn!`: `--gliner-variant` (on `remember` and `ingest`) and `ingest --mode gliner`; `--enable-ner` performs URL-regex extraction only.
- The CI matrix runs 2 features since v1.0.79: `default` and `llm-only`.
- The CI matrix runs 2 features since v1.0.79: `default` and `llm-only`.

## New in v1.0.80
### REQUIRED — Library API Stability (ADR-0032, G53, v1.0.80)
- The **CLI is the stable public contract**. The `--json` envelopes documented in `docs/schemas/*.schema.json` and the environment variables listed in `llms.txt` and `llms-full.txt` are stable across all v1.x.y releases
- The **library API is unstable** within v1.x.y. Re-exports, public struct fields and function signatures may change in any v1.x.y release without a major version bump
- Patch bumps (1.0.79 -> 1.0.80) are strictly additive at the lib surface: 6 new symbols are exposed in 1.0.80 and NONE were removed, renamed, or had their signature changed (see CHANGELOG.md "Library API Changes" section)
- Library consumers (cargo crate users) must pin to the EXACT version: `sqlite-graphrag = "=1.0.80"`. The `^1.0` shorthand keeps consumers on the CLI-stability track. The `^1.0.80` shorthand permits 1.0.80..<1.1.0 and can include a future 1.0.81 with lib-breaking changes
- For agent use, this split is invisible: the agent calls the CLI and parses `--json` envelopes, which are not affected by lib instability
### REQUIRED — G45 Cross-Process Embedding Singleton (v1.0.80)
- `acquire_embedding_singleton(namespace, db_path, wait_seconds, force)` serialises LLM embedding calls per `(namespace, db)` pair across concurrent CLI invocations
- A second CLI trying to embed against the same database receives `AppError::EmbeddingSingletonLocked { namespace }` (exit 75, retryable)
- Pass `--wait-embed-singleton <SECONDS>` to poll until the lock drops; distinct databases or namespaces acquire independent locks
- Operationally prevents the "two remember invocations, two LLM subprocesses, two parallel batches" pathology from the G45 incident
### REQUIRED — G55 S2: Structural `MemoryNotFound` (v1.0.80)
- The legacy `NotFound(String)` path that masked which lookup target failed is replaced by `AppError::MemoryNotFound { name, namespace }` and `AppError::MemoryNotFoundById { id }` inside `read` and `hybrid-search`
- The identifier is now part of the variant, eliminating the "not found: unknown" class of bugs
- pt-BR messages carry the name and namespace explicitly
### REQUIRED — G56: Entity-Embed In-Process Cache (v1.0.80)
- `embed_entity_texts_cached` routes entity-name batches through a `blake3(model || \0 || text)`-keyed cache
- High hit rate in `ingest` (canonical entities re-embedded across many memories), modest in `remember` and `remember-batch`
- `remember.rs`, `ingest.rs` and `remember_batch.rs` all use the cache; chunk embeds continue through the raw path
### REQUIRED — G58: FTS5 Fallback for `recall` and `hybrid-search` (v1.0.80)
- `recall --fallback-fts-only` and `hybrid-search --fallback-fts-only` route the query through FTS5 BM25 when the LLM subprocess fails (rate limit, OAuth contention, divergent dim)
- New envelope fields `vec_degraded` (bool), `vec_error` (string) and `warning` (string) are populated symmetrically across both commands
- The `recall` and `hybrid-search` tests gained coverage for the FTS5-only path
### REQUIRED — G53-WINDOWS-INFRA (ADR-0033, v1.0.80)
- The windows-2025 matrix jobs gained 2 new steps each, gated on `if: matrix.os == 'windows-2025'`: a pre-warm that downloads rustup into the runner cache, and a verify step that re-checks `rustup show active-toolchain`
- The 2 historical infra failure modes (rustup download with transient network errors and `E0463 can't find crate for core` when the target stdlib is missing) are now recoverable on the first re-run instead of accumulating as red CI
- The explicit `windows-2025` runner label (replacing `windows-latest` since v1.0.73) remains the right call until the VS2026 redirect cutover (2026-06-15)
### REQUIRED — SHUTDOWN Resilience (ADR-0034, v1.0.80)
- `src/signals.rs` is wrapped in a panic-catching boundary; even when the parent's stderr is a closed pipe (orphaned-process scenario), the handler returns cleanly instead of `SIGABRT`-ing on `BrokenPipe`
- The third consecutive Ctrl-C exits with code 130 and ZERO I/O
- The 3-layer SHUTDOWN bypass recipe (`nohup` → `setsid` → `disown`) is the canonical reference for the agent harness when running long embedding jobs in background
- Documented in `docs/HEADLESS_INVOCATION.md` and `docs/COOKBOOK.md`




## New in v1.0.76
### REQUIRED — LLM-Only One-Shot Architecture (G21 + G22 + G23 + G24 + G25)
- The default build of v1.0.76 is LLM-Only and one-shot. No daemon, no ONNX runtime, no `multilingual-e5-small` model download. Embedding generation and NER delegate to a headless `claude code` or `codex` subprocess (OAuth, no MCP, no hooks). Release binary is approximately 6 MB.
- The `embedding-legacy` feature was REMOVED in v1.0.79 (ahead of the v1.1.0 schedule). The v1.0.74 fastembed + ort + tokenizers pipeline no longer exists in any build.
- See ADR-0019 (LLM-Only One-Shot), ADR-0020 (Pure-Rust Cosine), ADR-0021 (Daemon Deprecation), ADR-0022 (BLOB-Backed Embeddings), ADR-0023 (Tokenizer Removal), ADR-0024 (FTS5 Coarse Filter + Cosine Refinement), ADR-0025 (OAuth-Only LLM Credential Flow), ADR-0026 (V002 `vec_tables` Migration Drift).
### REQUIRED — `migrate` Subcommand Family
- USE `migrate --rehash --json` to rewrite recorded migration checksums via `SipHasher13(name|version|sql)`. The algorithm matches `refinery-core 0.9.1` (same SipHasher13 crate, same hashing order). Required for v1.0.74 → v1.0.76 upgrades where V002 was intentionally emptied to a no-op.
- USE `migrate --to-llm-only --drop-vec-tables --json` as the one-shot upgrade for v1.0.74 / v1.0.75 databases. Combines `--rehash` with the V013 vec-table drop and reports vec-table state. The `--drop-vec-tables` flag is REQUIRED as a safety guard. After the migration, embeddings are recomputed lazily on the next `remember` / `edit` / `ingest`.
- Both `migrate --rehash` and `migrate --to-llm-only` have dedicated JSON schemas (`migrate-rehash.schema.json` and `migrate-to-llm-only.schema.json`) in `docs/schemas/`.
### REQUIRED — Schema Version and BLOB-Backed Embeddings
- The current schema version is 13. Migration V013 drops the `vec_memories`, `vec_entities`, `vec_chunks` virtual tables and replaces them with regular BLOB-backed `memory_embeddings`, `entity_embeddings`, `chunk_embeddings` tables. Cosine similarity is computed in pure Rust on demand in `src/similarity.rs`.
- Hybrid-search uses FTS5 for coarse filtering and refines the candidate set with pure-Rust cosine over the BLOB embeddings. FTS5 stays healthy because the rebuild is gated by `optimize --fts-skip-when-functional` (G36 from v1.0.69).
- The `daemon` subcommand was fully removed (infrastructure in v1.0.76; remaining `daemon.rs` code deleted in v1.0.79, ahead of the v1.1.0 schedule).
### REQUIRED — OAuth-Only Reaffirmed
- The OAuth-only mandate from v1.0.69 is REAFFIRMED. The spawn ABORTS with `AppError::Validation` if `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` is set in the environment. Both variables are excluded from the env-clear whitelist as defence in depth.
- New global flag `--extraction-backend llm|embedding|none|both` (default `llm`) selects the extraction backend. `llm` is the LLM-backed path; `embedding` is a permanent stub since v1.0.79 (legacy pipeline removed) that returns a migration error; `none` is a no-op; `both` runs them in parallel and merges.
- The `ExtractionBackend` trait lives in `src/extract/` with four concrete implementations: `LlmBackend` (default), `EmbeddingBackend` (permanent stub since v1.0.79; legacy pipeline removed), `NoneBackend` (no-op), and `CompositeBackend` (merges multiple backends in parallel).
- The `VersionAdapter` trait lives in `src/spawn/` and abstracts executor spawn invocations. `CodexAdapter` detects `codex 0.130.0` through `0.138+` and adapts flags — `codex 0.137.0` removed `--ask-for-approval` in favour of `-a never`. `ClaudeAdapter` covers claude code 2.1.0+. `OpencodeAdapter` covers opencode headless.
## New in v1.0.77
### REQUIRED — G40 Fix: `applied_on = NULL` Blocks All Migrations
- The `run_rehash` INSERT in v1.0.76 omitted the `applied_on` field, leaving it NULL. The refinery-core 0.9.1 rusqlite driver reads `applied_on` as `String` (NOT NULL), crashing with `InvalidColumnType(Null at index: 2)`. All subsequent migrations were blocked (exit 20).
- v1.0.77 adds a `sanitize_null_applied_on` helper that runs an UPDATE on rows with `applied_on IS NULL` before any migration runner call. The INSERT was also fixed to always include `applied_on` with an RFC3339 timestamp.
- v1.0.77 adds `remove_vec_virtual_tables_without_module` that cleans up vec0 virtual tables via `PRAGMA writable_schema` when the `vec0` module is absent (LLM-only build).
- `debug-schema` no longer crashes on databases with `applied_on = NULL` — the field was changed from `String` to `Option<String>`.
- JSON response for `migrate --rehash` now includes `null_rows_fixed` (u64). Response for `migrate --to-llm-only` includes `null_rows_fixed` (u64) and `vec_tables_removed_via_writable_schema` (usize).
- 4 new unit tests and 2 new integration tests cover the fix.
- See ADR-0027 for the full rationale.
## New in v1.0.78
### REQUIRED — G41 Fix: `run_rehash` Registered V013 Without Executing SQL
- The `else` branch in `run_rehash` (migrate.rs:272-281) that inserted phantom rows for unapplied migrations has been removed
- New `ensure_v013_tables_exist` helper detects databases where V013 is in `refinery_schema_history` but the BLOB-backed embedding tables (`memory_embeddings`, `entity_embeddings`, `chunk_embeddings`) were never created, and executes V013 SQL directly
- Auto-repair integrated in `ensure_db_ready` (connection.rs) — any CRUD command heals G41-corrupted databases unconditionally, even when `user_version=50` would skip the migration block
- JSON response for `migrate --rehash` and `migrate --to-llm-only` now includes `v013_tables_created` (boolean)
- 3 new unit tests and 1 updated unit test cover the fix
- See ADR-0028 for the full rationale

### FORBIDDEN — v1.0.76 Anti-patterns
- NEVER install v1.0.76 with `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` in the environment; the spawn aborts.
- NEVER depend on the daemon in new code; the daemon was fully removed (code deleted in v1.0.79).
- NEVER mix `vec_memories` / `vec_entities` / `vec_chunks` queries (removed in v1.0.76); use `memory_embeddings` / `entity_embeddings` / `chunk_embeddings` instead.
- NEVER use `migrate --to-llm-only` without `--drop-vec-tables`; the safety guard refuses the operation otherwise.


## New in v1.0.68
### Process Proliferation Fixes (G28)
- `enrich`, `ingest --mode claude-code`, and `ingest --mode codex` now acquire a per-namespace singleton via `lock::acquire_job_singleton(job_type, namespace, wait_seconds)`.  A second concurrent invocation against the same database fails fast with `AppError::JobSingletonLocked { job_type, namespace }` (exit code 75, classified as retryable).  This prevents the 2026-06-03 276-load-average incident where 4 parallel `enrich` invocations × 2 workers × 10 MCP servers spawned ~192 processes.
- `claude_runner::build_claude_command` now respects the `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` env var.  When set to an existing empty directory, the subprocess is spawned with `CLAUDE_CONFIG_DIR=<that dir>`, suppressing user-scoped MCP servers and their 8-10-process fan-out.  Deliberately avoids `--strict-mcp-config` and `--mcp-config '{}'` because [anthropics/claude-code#10787] documents that Claude Code CLI ignores both flags.
- `retry::CircuitBreaker` struct added with `AttemptOutcome::{Success, Transient, HardFailure}`.  Rate-limited and timeout errors are explicitly excluded from the failure count, so a provider that recovers is not penalised.  Use it in custom retry loops to cap persistent-failure iterations.
- `enrich` emits a `tracing::warn!` (visible with `-v`) when `--llm-parallelism > 4`, recommending to combine with `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` to keep subprocess fan-out manageable.
### Windows Build Fix (G29)
- `cargo install sqlite-graphrag` on Windows now succeeds.  v1.0.66 and v1.0.67 broke with `error[E0308]: mismatched types` in `src/terminal.rs:29` because `HANDLE` in `windows-sys >= 0.59` is `*mut c_void` (was `isize` in 0.48/0.52).  Replaced the unsafe idiom with `!handle.is_null() && handle != INVALID_HANDLE_VALUE`.  `windows-sys` is pinned to `=0.59.0` exact, and CI now runs `cargo check --target x86_64-pc-windows-msvc` on every push.
### Test Fixes
- 3 pre-existing test failures in `src/commands/{history,list,read}.rs` were leaking the `SQLITE_GRAPHRAG_DISPLAY_TZ` env var between parallel tests; fixed by parsing RFC3339 output and comparing `timestamp()` against `DateTime::UNIX_EPOCH` instead of asserting hardcoded `1970-01-01T00:00:00` strings.

## New in v1.0.67
### New Commands
- `remember-batch` — Batch-create memories from NDJSON stdin in a single invocation. Eliminates N-process contention from parallel `remember` calls. Supports `--transaction` (all-or-nothing), `--force-merge` (idempotent updates), `--fail-fast`.

## New in v1.0.65
### New Commands
- `reclassify-relation --from-relation <old> --to-relation <new> --batch --json` — renames relationship types in bulk across the graph; single-edge mode via `--source A --target B`; optional `--filter-source-type` and `--filter-target-type` for targeted batch; handles UNIQUE collisions via `UPDATE OR IGNORE` + `DELETE` merge; `--dry-run` previews count
- `normalize-entities --yes --json` — normalizes all entity names to lowercase kebab-case ASCII, auto-merging collisions (e.g., `Claude Code` + `claude-code` become one node); `--dry-run` previews
- `enrich --operation <op> --mode claude-code --json` — LLM-augmented graph quality pipeline; 3 operations: `memory-bindings` (extract entities from orphan memories), `entity-descriptions` (generate descriptions), `body-enrich` (expand short bodies); queue DB for resume/retry; `--dry-run` previews without spawning LLM; `--llm-parallelism <N>` spawns N parallel LLM worker threads (default 1, max 32) to reduce wall-clock time; output is NDJSON
### Deep Research Improvements
- `deep-research` now computes a separate embedding per sub-query — decomposition was cosmetic in v1.0.64
- `deep-research` fuses KNN + FTS5 + graph pools via RRF instead of hardcoded 0.5 for FTS results
- Evidence chains are now directed seed-to-target paths instead of a flat dump of top-20 global relationships
- New flags: `--rrf-k` (default 60), `--graph-decay` (default 0.7), `--graph-min-score` (default 0.05), `--max-neighbors-per-hop`
### Entity Normalization
- Entity names are now normalized to lowercase kebab-case on every write path (remember, ingest, link, rename-entity)
- `--max-entity-degree N` warning flag on `link` and `remember` — emits `tracing::warn!` when an entity exceeds N edges
### Health Command Additions
- `health` now reports `top_relation`, `top_relation_ratio`, `applies_to_ratio`, and `relation_concentration_warning` when any single relation type exceeds 40% of all edges

## New in v1.0.58
### Bug Fixes
- `remember --force-merge` now synchronizes the FTS5 index after update — previously every force-merge silently corrupted the full-text search index (CRITICAL fix)
- `merge-entities` uses `UPDATE OR IGNORE` for `memory_entities` table — fixes UNIQUE constraint failures when source and target entities share memory bindings
### New Commands and Features
- `rename-entity --name <old> --new-name <new> --json` — renames an entity preserving all relationships and memory bindings; re-embeds the vector with the new name
- `memory-entities --entity <name> --json` — reverse lookup: lists all memories bound to a given entity (complementing the existing memory→entities direction)
- `reclassify --name <entity> --description "text" --json` — updates entity description in single mode (previously only type could be changed)
### Enhancements
- `purge` response now includes `action` field (`"purged"` or `"dry_run"`) for consistency with all other commands
- Entity name validation rejects names with newlines, shorter than 2 characters, or short ALL_CAPS abbreviations (NER noise prevention)
- `fts --help` shows EXAMPLES section for subcommands
- `health` command emits `tracing::info!` at key checkpoints for `-vv` debugging
- `reclassify --help` lists all valid entity types
- `history --diff` JSON field is named `changes` (containing `added_chars` and `removed_chars`), not `diff`


## The Question No Agent Framework Answers
### Open Loop — Why 27 AI Agents Choose This As Their Memory Layer
- Why do 27 AI agents choose sqlite-graphrag as their persistent memory layer?
- Three technical reasons: durable local memory, zero cloud dependencies, deterministic JSON
- Each agent gains persistent memory without spending a single additional token
- Versus heavy MCPs, sqlite-graphrag delivers a deterministic stdin/stdout contract
- The secret the frameworks never document sits inside a single portable SQLite file


## Why Agents Love This CLI
### Five Differentiators — Engineered for Autonomous Loops
- Deterministic JSON output removes every parser hack from your orchestrator code
- Exit codes follow `sysexits.h` so your retry logic works without string matching
- No Python or Node runtime ships alongside the Rust CLI binary
- Stdin accepts structured payloads so your agents never escape shell arguments
- LLM-only one-shot architecture means zero persistent processes to manage
- Cross-platform behavior stays identical on Linux, macOS and Windows out of the box
- Default behavior always creates or opens `graphrag.sqlite` in the current working directory


## Economy That Converts
### Numbers That Sell The Switch
- Remove recurring cloud vector database dependencies from local agent workflows
- Keep retrieval local to the workstation or CI runner instead of a remote RAG stack
- Reduce the operational surface to one SQLite file and one CLI binary
- One-shot architecture eliminates daemon management overhead entirely
- Preserve orchestration determinism through stable JSON and stable exit codes


## Sovereignty as Competitive Advantage
### Why Local Memory Wins In 2026
- Your proprietary data NEVER leaves the developer workstation or the CI runner
- Your compliance surface shrinks to one SQLite file under your own encryption
- Your vendor lock-in vanishes since the schema is documented and portable
- Your audit trail lives in the `memory_versions` table with immutable history
- Your regulated industry gets offline-first RAG without cloud dependency clauses


## Compatible Agents and Orchestrators
### Catalog — 27 Supported Integrations
| Agent | Vendor | Minimum Version | Integration Type | Example |
| --- | --- | --- | --- | --- |
| Claude Code | Anthropic | 1.0+ | Subprocess | `sqlite-graphrag recall "query" --json` |
| Codex CLI | OpenAI | 0.5+ | AGENTS.md + subprocess | `sqlite-graphrag remember --name X --type user --description "..." --body "..."` |
| Gemini CLI | Google | any recent | Subprocess | `sqlite-graphrag hybrid-search "query" --json --k 5` |
| Opencode | open source | any recent | Subprocess | `sqlite-graphrag recall "auth flow" --json --k 3` |
| OpenClaw | community | any recent | Subprocess | `sqlite-graphrag recall "auth flow" --json --k 3` |
| Paperclip | community | any recent | Subprocess | `sqlite-graphrag read --name onboarding-note --json` |
| VS Code Copilot | Microsoft | 1.90+ | tasks.json | `{"command": "sqlite-graphrag", "args": ["recall", "$selection", "--json"]}` |
| Google Antigravity | Google | any recent | Runner | `sqlite-graphrag hybrid-search "prompt" --k 10 --json` |
| Windsurf | Codeium | any recent | Terminal | `sqlite-graphrag recall "refactor plan" --json` |
| Cursor | Cursor | 0.40+ | Terminal | `sqlite-graphrag remember --name cursor-ctx --type project --description "..." --body "..."` |
| Zed | Zed Industries | any recent | Assistant Panel | `sqlite-graphrag recall "open tabs" --json --k 5` |
| Aider | open source | 0.60+ | Shell | `sqlite-graphrag recall "refactor target" --k 5 --json` |
| Jules | Google Labs | preview | CI automation | `sqlite-graphrag stats --json` |
| Kilo Code | community | any recent | Subprocess | `sqlite-graphrag recall "recent tasks" --json` |
| Roo Code | community | any recent | Subprocess | `sqlite-graphrag hybrid-search "repo context" --json` |
| Cline | community | VS Code ext | Terminal | `sqlite-graphrag list --limit 20 --json` |
| Continue | open source | VS Code or JetBrains ext | Terminal | `sqlite-graphrag recall "docstring" --json` |
| Factory | Factory | any recent | API or subprocess | `sqlite-graphrag recall "pr context" --json` |
| Augment Code | Augment | any recent | IDE | `sqlite-graphrag hybrid-search "code review" --json` |
| JetBrains AI Assistant | JetBrains | 2024.2+ | IDE | `sqlite-graphrag recall "stacktrace" --json` |
| OpenRouter | OpenRouter | any | Router for multi-LLM | `sqlite-graphrag recall "routing rule" --json` |
| Minimax | Minimax | any recent | Subprocess | `sqlite-graphrag recall "user preferences" --json --k 5` |
| Z.ai | Z.ai | any recent | Subprocess | `sqlite-graphrag hybrid-search "task context" --json --k 10` |
| Ollama | Ollama | 0.1+ | Subprocess | `sqlite-graphrag remember --name ollama-ctx --type project --description "..." --body "..."` |
| Hermes Agent | community | any recent | Subprocess | `sqlite-graphrag recall "tool call history" --json` |
| LangChain | LangChain | 0.3+ | Subprocess via tool | `sqlite-graphrag hybrid-search "chain context" --json --k 5` |
| LangGraph | LangChain | 0.2+ | Subprocess via node | `sqlite-graphrag recall "graph state" --json --k 3` |


## Agent Integration Details
### Minimax
- Open-source multimodal agent with video, audio, and text reasoning capabilities
- Invoke sqlite-graphrag as subprocess from within a Minimax tool definition:
```bash
sqlite-graphrag recall "user session context" --json --k 5
```
- Output: JSON with `results` entries carrying `name`, `snippet`, `distance`, and `source`

### Z.ai
- Hosted agent platform with multi-step task planning and tool orchestration
- Invoke sqlite-graphrag to persist inter-session memory across planning cycles:
```bash
sqlite-graphrag remember --name "task-plan-$(date +%s)" --type project --description "Z.ai task plan" --body "$PLAN"
sqlite-graphrag recall "previous task plan" --json --k 3
```
- Output: deterministic JSON with `results`, `direct_matches`, and `graph_matches`

### Ollama
- Local LLM server running open models on consumer hardware without cloud calls
- Invoke sqlite-graphrag as a tool to give Ollama agents persistent knowledge:
```bash
sqlite-graphrag recall "conversation history" --json --k 5
sqlite-graphrag remember --name "ollama-session" --type project --description "Ollama conversation" --body "$CONTEXT"
```
- Output: deterministic recall JSON with `elapsed_ms` and stable result fields

### Hermes Agent
- Community agent framework designed for ReAct-style tool-calling loops
- Invoke sqlite-graphrag at the start of each ReAct cycle to load prior context:
```bash
sqlite-graphrag hybrid-search "tool call results" --json --k 5
```
- Output: hybrid-search JSON combining BM25 full-text and cosine vector ranking

### LangChain
- Python orchestration framework for LLM chains with tool and retriever abstractions
- Invoke sqlite-graphrag as a custom retriever tool via subprocess from LangChain Python:
```bash
sqlite-graphrag hybrid-search "chain input query" --json --k 10 --lang en
```
- Output: JSON `results` array consumable by `json.loads` in the LangChain tool wrapper

### LangGraph
- Graph-based state machine framework for multi-agent workflows built on LangChain
- Invoke sqlite-graphrag inside each graph node to persist and recall inter-node state:
```bash
sqlite-graphrag recall "graph node output" --json --k 3
sqlite-graphrag remember --name "node-result-$(date +%s)" --type project --description "LangGraph node output" --body "$OUTPUT"
```
- Output: structured JSON enabling stateful graph traversal across LangGraph runs


## Rust Crate Integrations
### Agent and LLM Crates — Call sqlite-graphrag as a Subprocess
- Every Rust crate that spawns an LLM agent can call sqlite-graphrag via `std::process::Command`
- Deterministic subprocess recall lets Rust crates reuse a stable memory contract
- Zero additional tokens: memory lives in SQLite, not inside the context window
- Each crate gains persistent memory without importing any sqlite-graphrag dependency

### rig-core
- Modular framework for building LLM pipelines, RAG systems, and autonomous agents
- Cargo.toml:
```toml
[dependencies]
rig-core = "0.35.0"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "project context", "--json"])
    .output()?;
```
- Case: persist agent tool results across rig pipeline invocations without tokens

### swarms-rs
- Multi-agent orchestration framework with native MCP support and swarm topologies
- Cargo.toml:
```toml
[dependencies]
swarms-rs = "0.2.1"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "swarm task result", "--json", "--k", "5"])
    .output()?;
```
- Case: share persistent context across swarm agents without a central vector DB

### autoagents
- Multi-agent runtime with Ractor actors, ReAct loops, and WASM sandbox isolation
- Cargo.toml:
```toml
[dependencies]
autoagents = "0.3.7"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["remember", "--name", "react-step", "--type", "project", "--description", "autoagents step", "--body", "step output"])
    .output()?;
```
- Case: checkpoint ReAct intermediate steps for replay and audit in autoagents loops

### agentai
- Thin agent layer over genai with a simple ToolBox abstraction for tool registration
- Cargo.toml:
```toml
[dependencies]
agentai = "0.1.5"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "tool call context", "--json", "--k", "3"])
    .output()?;
```
- Case: inject prior tool call history into agentai ToolBox before each agent run

### llm-agent-runtime
- Full agent runtime with episodic memory, checkpointing, and tool orchestration
- Cargo.toml:
```toml
[dependencies]
llm-agent-runtime = "1.74.0"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "episode context", "--json"])
    .output()?;
```
- Case: extend llm-agent-runtime episodic memory with durable SQLite persistence

### anda
- Agent framework for trusted execution environments and ICP blockchain integrations
- Cargo.toml:
```toml
[dependencies]
anda = "0.4.10"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["read", "--name", "anda-agent-state", "--json"])
    .output()?;
```
- Case: persist verifiable agent state outside the TEE for cross-session continuity

### adk-rust
- Modular agent development kit inspired by LangChain and Autogen patterns
- Cargo.toml:
```toml
[dependencies]
adk-rust = "0.6.0"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "agent memory query", "--json", "--k", "10"])
    .output()?;
```
- Case: replace adk-rust in-memory context store with persistent graph-native recall

### genai
- Unified API client for OpenAI, Anthropic, Gemini, xAI, and Ollama in one crate
- Cargo.toml:
```toml
[dependencies]
genai = "0.6.0-beta.17"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "llm response cache", "--json"])
    .output()?;
```
- Case: cache expensive genai LLM responses in sqlite-graphrag for cross-run reuse

### liter-llm
- Universal LLM client supporting 143 plus providers with OpenTelemetry tracing built in
- Cargo.toml:
```toml
[dependencies]
liter-llm = "1.2.1"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["remember", "--name", "litellm-trace", "--type", "project", "--description", "liter-llm trace", "--body", "trace payload"])
    .output()?;
```
- Case: store OpenTelemetry trace snapshots in sqlite-graphrag for agent replay

### llm-cascade
- LLM cascade client with automatic failover and circuit breaker across providers
- Cargo.toml:
```toml
[dependencies]
llm-cascade = "0.1.0"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "fallback provider result", "--json"])
    .output()?;
```
- Case: persist cascade decisions so the circuit breaker learns from prior failures

### async-openai
- Rust-native async client for the full OpenAI REST API with type-safe models
- Cargo.toml:
```toml
[dependencies]
async-openai = "0.34.0"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "openai assistant output", "--json", "--k", "5"])
    .output()?;
```
- Case: store assistant thread messages in sqlite-graphrag for durable cross-session recall

### anthropic-sdk
- Direct Rust client for the Anthropic API including tool use and streaming responses
- Cargo.toml:
```toml
[dependencies]
anthropic-sdk = "0.1.5"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "claude conversation context", "--json"])
    .output()?;
```
- Case: inject prior Claude conversation turns from sqlite-graphrag before each API call

### ollama-rs
- Idiomatic Rust client for the Ollama local inference server API
- Cargo.toml:
```toml
[dependencies]
ollama-rs = "0.3.4"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["remember", "--name", "ollama-output", "--type", "project", "--description", "ollama-rs output", "--body", "generated text"])
    .output()?;
```
- Case: persist ollama-rs generation outputs for retrieval in subsequent inference calls

### llama-cpp-rs
- Rust bindings for llama.cpp enabling on-device inference with quantized models
- Cargo.toml:
```toml
[dependencies]
llama-cpp-rs = "0.3.0"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "on-device inference context", "--json", "--k", "5"])
    .output()?;
```
- Case: load persistent context into llama-cpp-rs prompt before each local inference

### mistralrs
- High-performance local inference engine for Mistral models with quantization support
- Cargo.toml:
```toml
[dependencies]
mistralrs = "0.8.1"
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "mistral inference context", "--json", "--k", "5"])
    .output()?;
```
- Case: inject sqlite-graphrag persistent context into mistralrs prompts before local inference

### graphbit
- Graph-based workflow engine for deterministic LLM pipeline orchestration in Rust
- Cargo.toml:
```toml
[dependencies]
graphbit = { git = "https://github.com/graphbit-rs/graphbit" }
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["recall", "workflow node state", "--json", "--k", "3"])
    .output()?;
```
- Case: persist graphbit workflow node outputs for stateful cross-run graph traversal

### rs-graph-llm
- Typed interactive graph workflows for LLM pipelines with compile-time safety
- Cargo.toml:
```toml
[dependencies]
rs-graph-llm = { git = "https://github.com/rs-graph-llm/rs-graph-llm" }
```
- Integration with sqlite-graphrag:
```rust
use std::process::Command;
let output = Command::new("sqlite-graphrag")
    .args(["hybrid-search", "graph node output", "--json", "--k", "5"])
    .output()?;
```
- Case: store rs-graph-llm typed pipeline results for persistent memory across executions


## Fundamental Principles
### REQUIRED — Usage Philosophy
- TREAT sqlite-graphrag as a local persistent memory layer
- INVOKE always as a subprocess via `std::process::Command`
- READ stdout for structured data in JSON or NDJSON
- READ stderr for tracing logs and human-readable messages
- CHECK exit code before parsing stdout
- PRESERVE context between sessions via a single SQLite file
- DELEGATE long-term memory to the binary without reimplementing
### FORBIDDEN — Anti-patterns
- NEVER expose the binary as an MCP server or HTTP service
- NEVER depend on cloud vector DBs such as Pinecone or Weaviate
- NEVER write directly to SQLite in parallel with the binary
- NEVER edit the `.sqlite` file with another tool
- NEVER assume output without validating the exit code first
- NEVER confuse `distance` with `combined_score` in ranking
- NEVER mix structured stdout with human-readable logs
- NEVER use `fd | xargs remember` when `ingest` covers the case


## Initialization and Health Check
### REQUIRED — Database Bootstrap
- RUN `sqlite-graphrag init --namespace <project>` on first use
- WAIT for the first LLM round-trip to verify OAuth connectivity
- VALIDATE with `sqlite-graphrag health --json` before operating
- TREAT exit code 10 as a database error or corrupted database
- TREAT exit code 15 as a pending lock; widen `--wait-lock`
- ABORT pipeline when `integrity_ok` returns `false`
- RUN `migrate --json` after each binary upgrade
### REQUIRED — Continuous Monitoring
- INSPECT `wal_size_mb` in `health` to detect fragmentation
- CHECK `journal_mode` equals `wal` in production
- RUN `optimize --json` to refresh planner statistics
- DETECT schema drift via `debug-schema` for troubleshooting
- CHECK `mentions_ratio` (float) and `mentions_warning` (string) in `health --json` output when `mentions` relationships dominate the graph above 50%
- CHECK `top_relation` (string), `top_relation_ratio` (float), `applies_to_ratio` (float), and `relation_concentration_warning` (string) when any single relation type exceeds 40% of edges (v1.0.65)
- CHECK `super_hub_count` (int) and `top_hub_entity` (string) reported when any entity exceeds 50 connections — indicates graph topology that may degrade traversal quality
### Correct Pattern — Bootstrap Sequence
- `sqlite-graphrag init --namespace my-project`
- `sqlite-graphrag health --json | jaq '.integrity_ok'`
- `sqlite-graphrag migrate --json`
- `sqlite-graphrag stats --json | jaq '.memories'`


## Global Configuration
### REQUIRED — Database Path
- USE `--db <PATH>` when the database is not in the current directory
- SET `SQLITE_GRAPHRAG_DB_PATH` for persistent configuration
- NOTE that `--db` takes precedence over the environment variable
- DEFAULT is `graphrag.sqlite` in the current invocation directory
### REQUIRED — Namespace
- SET namespace via `--namespace` or `SQLITE_GRAPHRAG_NAMESPACE`
- VALIDATE resolution with `namespace-detect --json`
- USE `global` as the default namespace when absent
- SINCE v1.0.51 ALL commands respect `SQLITE_GRAPHRAG_NAMESPACE`; previously `list`, `read`, `edit`, `forget`, `history`, `rename`, `restore`, and `remember` ignored it
- ISOLATE projects via namespace per repository
- ADOPT `swarm-<agent_id>` for multi-agent swarms
### REQUIRED — Output Language
- USE `--lang en` or `--lang pt` to force output language
- SET `SQLITE_GRAPHRAG_LANG=en` for session override
- NOTE that `--lang` affects only human-readable stderr
- STDOUT JSON remains deterministic regardless of language
### REQUIRED — Display Timezone
- APPLY `--tz America/New_York` to localized output
- USE `SQLITE_GRAPHRAG_DISPLAY_TZ=<IANA>` to persist
- AFFECTS only `*_iso` fields in the JSON
- INTEGER epoch fields remain in UTC
- ABORT when an invalid IANA name returns exit 2 (Clap argument parsing)
### REQUIRED — Log Format
- ENABLE `SQLITE_GRAPHRAG_LOG_FORMAT=json` for log aggregators
- DEFAULT `pretty` is intended for humans in the terminal only
- RAISE detail via `SQLITE_GRAPHRAG_LOG_LEVEL=debug` for diagnostics
- USE `-v`, `-vv`, `-vvv` for info, debug, and trace in subcommands
### REQUIRED — Global RAM Control
- ENABLE `SQLITE_GRAPHRAG_LOW_MEMORY=1` in constrained containers
- APPLY on hosts with less than 4 GB of available RAM
- HONORS cgroup constraints automatically when set
- TRADE-OFF is 3 to 4 times more wall-clock time
- COMBINE with the `--low-memory` flag in a specific `ingest`
### NOTE — ONNX Runtime No Longer Required (v1.0.76)
- The ONNX runtime and fastembed model were removed in v1.0.76
- All embedding is now done via the LLM subprocess (claude or codex)
- No `libonnxruntime.so` or `ORT_DYLIB_PATH` needed


## CRUD — Create with remember
### REQUIRED — Writing Individual Memories
- USE a unique kebab-case name per memory
- DECLARE `--type` from `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- PREFER `--body-stdin` for long bodies
- USE `--body-file <PATH>` to avoid shell escaping in Markdown
- PASS `--force-merge` in idempotent loops; also restores soft-deleted memories and updates them in one step (since v1.0.51); `--type` and `--description` are optional with `--force-merge` — existing values are inherited when omitted
- USE `--dry-run` to validate the payload (body size, entity/relationship schema, name uniqueness) without persisting anything; exits 0 on success, non-zero on validation failure
- USE `--clear-body` with `--force-merge` to explicitly set the body to empty string instead of inheriting the existing body
- NER is disabled by default; pass `--enable-ner` or set `SQLITE_GRAPHRAG_ENABLE_NER=1` to activate automatic extraction — URL-regex ONLY since v1.0.79 (the GLiNER pipeline was removed)
- `--skip-extraction` is deprecated since v1.0.45 and has no effect; `--gliner-variant` is a no-op since v1.0.79 and emits a `tracing::warn!` when set to a non-default value
- Response field `extraction_method` reports the method used: `url-regex` (URL extraction ran) or `none:extraction-failed`; the `gliner-<variant>+regex` and `regex-only` values are HISTORICAL (≤ v1.0.75)
- RESPECT the limit of 512000 bytes and 512 chunks per body
- USE `--max-rss-mb <MiB>` to cap process RSS during embedding (default: 8192 MiB); aborts with exit 77 if exceeded
### REQUIRED — Attaching Graph in remember
- USE `--entities-file` with a typed JSON array
- USE `--relationships-file` for typed edges
- INCLUDE the `entity_type` field in each entity object
- ACCEPT `type` as a synonym; never both at once
- USE `strength` between `0.0` and `1.0` in relationships
- MAP `from`/`to` as aliases of `source`/`target`
- USE `--graph-stdin` for a single JSON with `body`, `entities`, and `relationships`
### FORBIDDEN — Write Errors
- NEVER send `entity_type` and `type` in the same JSON object
- NEVER use `strength` outside the range `[0.0, 1.0]`
- NEVER duplicate a name without explicit `--force-merge`
- NEVER mix `--body`, `--body-file`, `--body-stdin`, `--graph-stdin`
- NEVER rely on `--enable-ner` for semantic entity extraction; it is URL-regex only since v1.0.79 — use `--graph-stdin` with LLM-curated entities or `ingest --mode claude-code|codex`
- NEVER exceed the relations cap per memory without adjusting env
- NEVER use `remember` in a loop when `ingest` covers the case
### Correct Pattern — remember Examples
- `sqlite-graphrag remember --name design-auth --type decision --description "auth JWT" --body-stdin < doc.md`
- `sqlite-graphrag remember --name doc-readme --type document --description "import" --body-file README.md --force-merge`
- `sqlite-graphrag remember --name spec-x --type reference --description "spec" --body "..." --entities-file ents.json --relationships-file rels.json`
### Valid --type Values
- `user`, `feedback`, `project`, `reference`
- `decision`, `incident`, `skill`, `document`, `note`


## CRUD — Bulk Ingest with ingest
### REQUIRED — When to Use ingest
- USE `ingest <DIR>` to import entire directories as memories
- PREFER over the `fd | xargs remember` loop in any case
- USE `ingest --dry-run` to preview the file-to-name mapping without spawning any LLM subprocess or persisting anything
- `--dry-run` output is NDJSON with `status: "preview"` for each file; use it to detect name truncations and collisions before committing
- EACH file matching the pattern becomes an individual memory
- MEMORY name derives from the file basename without extension in kebab-case
- NAMES longer than 60 characters are TRUNCATED automatically
- NDJSON includes `truncated: true` and `original_name` when truncated
- AGENT must use `original_name` or `name` from NDJSON to access the memory
- OUTPUT is NDJSON, one JSON line per file plus a final summary line
- CONSUME line by line in streaming via `jaq -c` or `while read`
### REQUIRED — File Pattern with --pattern
- DEFAULT is `*.md` only; change as needed
- ACCEPT `*.<ext>` for a generic extension
- ACCEPT `<prefix>*` for a basename prefix
- ACCEPT exact filename without glob characters
- FULL POSIX glob is not supported by ingest
### REQUIRED — Recursion and Limits
- ENABLE `--recursive` to descend into subdirectories
- WITHOUT `--recursive` only top-level is processed
- RESPECT `--max-files 10000` as the default safety cap
- `--max-files` REJECTS the entire operation with exit 1 if count exceeds the cap
- `--max-files` does NOT limit to the first N; it is all-or-nothing validation
- INCREASE the cap only after auditing actual volume
- USE `--fail-fast` to stop at the first per-file failure
- WITHOUT `--fail-fast` the loop continues and reports each error in the NDJSON
### REQUIRED — Bulk Memory Type
- DECLARE `--type` applied to ALL files in the invocation
- DEFAULT is `document` when omitted
- VALID values: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- INVOKE `ingest` separately per type when mixing
- GROUP files by directory according to the desired type
### REQUIRED — RAM Control
- USE `--low-memory` in containers with less than 4 GB
- SET `SQLITE_GRAPHRAG_LOW_MEMORY=1` as a persistent override
- `--low-memory` forces `--ingest-parallelism 1` internally
- TRADE-OFF is 3 to 4 times more execution time
- CHOOSE when RSS is a greater constraint than latency
- USE `--max-rss-mb <MiB>` to abort if process RSS exceeds threshold during embedding (default: 8192 MiB)
### REQUIRED — Two Parallelism Axes
- `--max-concurrency <N>` controls simultaneous CLI invocations
- `--ingest-parallelism <N>` controls extract plus embed in parallel
- DEFAULT for `--max-concurrency` is 4
- DEFAULT for `--ingest-parallelism` is `min(4, max(1, cpus/2))`
- DISTINGUISH the two axes clearly before adjusting
- WIDEN `--wait-lock <SECONDS>` to wait for a slot before exit 75
### REQUIRED — Performance and Extraction
- NER is disabled by default; pass `--enable-ner` to activate automatic extraction — URL-regex ONLY since v1.0.79 (the GLiNER ONNX pipeline, its 1.1 GB model download and `--gliner-variant` selection were removed)
- `--skip-extraction` is deprecated since v1.0.45 and has no effect; `--gliner-variant` is a no-op since v1.0.79 and emits a `tracing::warn!` when set
- Response field `extraction_method` reports `url-regex` or `none:extraction-failed`; `gliner-<variant>+regex` and `regex-only` are HISTORICAL values (≤ v1.0.75)
- USE `--enable-ner` only when URL entity extraction is valuable
- PREFER `--mode claude-code` / `--mode codex` or `--graph-stdin` with LLM-curated entities for semantic extraction quality
### FORBIDDEN — ingest Anti-patterns
- NEVER use `fd | xargs sqlite-graphrag remember` when `ingest` exists
- NEVER omit `--recursive` expecting automatic descent
- NEVER pass a complex unsupported glob pattern
- NEVER ignore exit 75 for exhausted slots in automated loops
- NEVER mix different types in the same invocation
- NEVER raise `--max-files` without measuring RAM and disk first
- NEVER use `--force-merge` in ingest (flag exclusive to `remember`)
### Correct Pattern — ingest Examples
- `sqlite-graphrag ingest ./docs --recursive --pattern "*.md" --json`
- `sqlite-graphrag ingest ./decisions --type decision --json`
- `sqlite-graphrag ingest ./large-corpus --low-memory --max-files 50000 --json`
- `sqlite-graphrag ingest ./skills --type skill --recursive --fail-fast --json`
- `sqlite-graphrag ingest ./notes --type note --pattern "memo-*" --recursive --json`
### Correct Pattern — NDJSON Consumption
- `sqlite-graphrag ingest ./docs --recursive --json | jaq -c 'select(.status == "indexed")'`
- `sqlite-graphrag ingest ./docs --recursive --json | tee results.ndjson`
- NDJSON contains `files_total + 1` lines: one per file plus a final summary line
- FILTER by `select(.status)` to ignore the summary line that has no `status` field
- `jaq -sc '[.[] | select(.status)] | group_by(.status) | map({status: .[0].status, count: length})' < results.ndjson`
### REQUIRED — NDJSON Schema by Line Type
- Per-file line: `file`, `name`, `status` (`"indexed"` `"skipped"` `"failed"`), `truncated`, `original_name?`, `original_filename?`, `memory_id?`, `action?`, `error?`
- `original_filename` preserves the file basename before kebab-case normalization; present when the basename differs from the derived name (e.g., spaces, accents, special characters)
- Final summary line: `summary` (true), `dir`, `pattern`, `recursive`, `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- NER extraction events go to stderr, NOT stdout
### REQUIRED — Ingest Modes (v1.0.62)
- USE `--mode none` (default) for body-only ingestion without extraction
- `--mode gliner` is DEPRECATED since v1.0.79 (URL-regex only; emits a `tracing::warn!`); use `--mode claude-code` or `--mode codex` for semantic extraction
- USE `--mode claude-code` for LLM-curated extraction via locally installed Claude Code CLI
- Claude Code mode requires `claude` binary >= 2.1.0 in PATH with active Pro/Max subscription
- USE `--resume` to continue a previously interrupted claude-code ingest from the queue DB
- USE `--retry-failed` to retry only failed files from a previous run
- USE `--max-cost-usd <N>` to set a budget cap — ingestion stops when cumulative cost exceeds the limit
- USE `--claude-binary <PATH>` to specify an explicit path to the Claude Code binary
- USE `--claude-model <MODEL>` to override the model (e.g. `claude-sonnet-4-6`)
- USE --claude-timeout <S> to set per-file subprocess timeout (default 300s); kills hung claude -p processes
- NDJSON per-file events in claude-code mode include `entities`, `rels`, `cost_usd` fields
- Queue DB `.ingest-queue.sqlite` tracks per-file progress; use `--keep-queue` to retain after completion
- Rate limit handling: automatic exponential backoff (60s → 120s → 300s → 900s)
- `--mode codex` spawns `codex exec --json` per file for LLM-curated extraction via OpenAI Codex CLI
- Requires Codex CLI installed; uses `--output-schema` for structured JSON output
- Codex flags: `--codex-binary`, `--codex-model`, `--codex-timeout` (default 300s)
- Environment variable `SQLITE_GRAPHRAG_CODEX_BINARY` overrides PATH lookup
- Full embedding pipeline applied for recall and hybrid-search
### Correct Pattern — Claude Code Ingest Examples
- `sqlite-graphrag ingest ./docs --mode claude-code --recursive --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --resume --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --max-cost-usd 5.00 --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --claude-model claude-sonnet-4-6 --json`
- `sqlite-graphrag ingest ./docs --mode claude-code --claude-timeout 600 --max-cost-usd 10.00 --json`
### Correct Pattern — Codex Ingest Examples
- `sqlite-graphrag ingest ./docs --mode codex --recursive --json`
- `sqlite-graphrag ingest ./docs --mode codex --codex-model o4-mini --json`
- `sqlite-graphrag ingest ./docs --mode codex --codex-timeout 600 --json`
- `sqlite-graphrag ingest ./docs --mode codex --codex-binary /usr/local/bin/codex --json`
### Authentication Note
> **Authentication:** OAuth is the ONLY accepted credential flow. API keys are PROHIBITED.
> `--mode claude-code` reads OAuth from `~/.claude/.credentials.json` (Claude Pro/Max/Team).
> `--mode codex` reads device auth from `codex auth login` (OpenAI ChatGPT).
> Defining `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` in the environment ABORTS the spawn with `AppError::Validation` and exit code 1. The `--bare` flag (which would also demand an API key) is REMOVED from all executable code paths.


## New in v1.0.69
### OAuth-Only Enforcement (BEHAVIOUR CHANGE — REQUIRED)
- The spawn of `claude -p` and `codex exec` now ABORTS with `AppError::Validation` if `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` are present in the environment. The OAuth flow (Claude Pro/Max subscription, ChatGPT Pro device auth) is the ONLY accepted credential mechanism. See `docs/decisions/adr-0011-oauth-only-enforcement.md` for the full rationale.
- The `--bare` flag (which demands `ANTHROPIC_API_KEY` and disables OAuth) is REMOVED from every executable path. It appears only in documentation explaining why it is forbidden.
- `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` are INTENTIONALLY ABSENT from the `env_clear` whitelists in `claude_runner.rs`, `codex_spawn.rs`, and `ingest_claude.rs`. Defence-in-depth: even if a future refactor moves the OAuth-only guard, the variable never reaches the child.
- Four new tests (`#[serial_test::serial(env)]`) validate the canonical flag set and the abort behaviour in `src/commands/claude_runner.rs:574-666` and `src/commands/codex_spawn.rs:684-758`.
- Migration: any operator currently relying on `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` must migrate to OAuth. The error message is actionable and points at the OAuth login flow.
### Process Proliferation Fixes (G28)
- `claude_runner::build_claude_command` ALWAYS passes `--strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions` (overriding the v1.0.68 behaviour that cited issue #10787 and avoided these flags). The v1.0.68 comment was wrong: the flags are present in code. The v1.0.69 comment aligns with reality and adds defence-in-depth via the OAuth-only guard.
- `run_claude` sends `SIGTERM` on timeout before the `Child` is dropped, so MCP children do not survive the parent.
- New `src/reaper.rs` walks `/proc` at startup, kills any `claude`/`codex` orphan with `PPID=1` and age greater than 60s. Invoked from `main` BEFORE any work.
- New `src/system_load.rs` provides `load_average_one`, `ncpus`, and `is_system_saturated`; `enrich` aborts the spawn when `load_average_one() > 2 * ncpus` and the new `--max-load-check` flag is set (default true).
- `retry::CircuitBreaker` is integrated into the worker loop with `breaker.record(AttemptOutcome::HardFailure)`; the loop aborts after `--circuit-breaker-threshold` consecutive failures (default 5, set to 0 to disable).
### Singleton Scoped by `db_hash` (G30)
- `lock::acquire_job_singleton(job_type, namespace, db_path, wait_seconds, force)`. The lock file path is `job-singleton-{tag}-{namespace_slug}-{db_hash}.lock` where `db_hash` is the first 12 hex characters of `blake3(canonicalize(db_path))`. Two concurrent `enrich` invocations against DIFFERENT databases no longer collide.
- New CLI flags `--wait-job-singleton <SECONDS>` and `--force-job-singleton` on `enrich` and `ingest`. The error message that previously referenced a non-existent `--wait-job-singleton` flag is now actionable.
### Codex Spawn Helper Unified (G31+G32+G33)
- New `src/commands/codex_spawn.rs` (~700 lines, 11 tests) owns the canonical spawn pipeline, the JSONL parser, and the ChatGPT Pro OAuth model validation. Both `enrich --mode codex` and `ingest --mode codex` consume the same helper, eliminating the drift that motivated the `~/.local/bin/codex-clean` external wrapper.
- ChatGPT Pro OAuth model whitelist: `codex-auto-review`, `gpt-5.3-codex-spark`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.5`. Validation runs BEFORE the subprocess is spawned; an invalid model returns `AppError::Validation` listing the accepted values and the closest fuzzy match.
- New top-level subcommand `codex-models --json` exposes the model list, count, and default. `codex-models --suggest <substring>` returns the closest match via substring or Levenshtein.
- Schema JSON moved from `/tmp` to `paths::AppPaths::cache_dir().join("schemas")` so it survives reboots and lives in a trusted directory.
- The new canonical command includes the OAuth-only hardening flag `-c mcp_servers='{}'` and `--ask-for-approval never`.
### Preservation Gate Jaccard (G29)
- New flag `--preserve-threshold <FLOAT>` on `enrich` (default 0.7). The new `src/preservation.rs` module (10 tests) computes Jaccard trigram similarity between original and enriched bodies. If `score < threshold`, the enriched body is rejected with `EnrichItemResult::PreservationFailed` and is NOT persisted.
- Idempotency via `blake3::hash`: when `old_hash == new_hash`, the body is skipped with reason `"enriched body hash matches original (blake3:{hash}); idempotency skip"`. Reprocessing the same memory is safe.
### MemorySource Enum (G29)
- New `src/memory_source.rs` (~180 lines, 8 tests) defines a type-safe enum of the five CHECK-constraint values: `Agent`, `User`, `System`, `Import`, `Sync`. `TryFrom<&str>` returns `AppError::Validation` listing the accepted values.
- Runtime guard `pub fn validate_source(raw: &str) -> Result<&'static str, AppError>` is called from `memories::insert` and `memories::update`. Existing call-sites still use `String` for binary compatibility; the enum is the foundation for the v1.0.70 migration.
### FTS5 Hardening Flags (G36)
- `optimize` pre-checks FTS5 health via `check_fts_functional` BEFORE rebuilding. A healthy index is no longer rebuilt (saves ~10 minutes on a 4.3 GB database).
- New flags: `--fts-dry-run` (exit 1 if rebuild recommended), `--fts-progress <N>` (background poll of `fts_memories` row count every N seconds, default 30, 0 disables), `--yes` (reserved for forward compatibility).
- `OptimizeResponse` exposes `fts_rebuilt`, `fts_skipped_functional`, `fts_unhealthy`, and `fts_rows_indexed` for observability.
### vec Orphan Handling (G39)
- New subcommand family `vec orphan-list`, `vec purge-orphan --yes --dry-run`, `vec stats --json`. `vec purge-orphan` purges THREE tables: `vec_memories`, `vec_entities`, and `vec_chunks` in a single transaction.
- New hook in `src/commands/forget.rs:88-99` calls `memories::delete_vec` BEFORE the soft-delete, preventing new orphans in the steady state.
### Backup Hardening (G38)
- Defaults changed from `run_to_completion(100, 50ms)` to `run_to_completion(1000, 5ms)` (25x speedup on 4.3 GB).
- New flags: `--backup-step-size <PAGES>`, `--backup-step-sleep-ms <MS>`, `--backup-progress <PAGES>`, `--backup-no-sleep`.
### Selective Enrichment (G37)
- New flags `--names <NAME>` (comma-delimited) and `--names-file <PATH>` (one name per line, `#` comments accepted) on `enrich`. Operators can now reprocess a single memory without scanning the full set.
### Preflight and Fallback (G35)
- New flags `--preflight-check`, `--fallback-mode <codex|claude-code>`, and `--rate-limit-buffer <SECONDS>` on `enrich`. The preflight probe issues a 1-turn ping before scanning N candidates; on a Claude rate limit it aborts with a clear error (or switches to `--fallback-mode`).
### Worker Warning by Mode (G34)
- The `llm_parallelism > 4` warning is conditional to the mode: Claude warns at 5, Codex warns at 17, Codex 5..16 is silent (validated at 1161 items, 0 failures in production).


## CRUD — Read with read and list
### REQUIRED — Direct Read by Name (read)
- USE `read --name <kebab-case>` for O(1) fetch by name
- USE `read --id <N>` for direct lookup by integer `memory_id` — useful when agent pipelines pass IDs from `list` or `remember` responses
- PARSE fields `body`, `description`, `created_at_iso`, `updated_at_iso`
- TREAT exit code 4 as memory not found in the namespace
- APPLY `--tz` to localize timestamps in the output
### REQUIRED — Enumeration with Filters (list)
- USE `list --type <kind>` to filter by memory type
- DEFAULT limit is ALL memories when `--json` is active; default is 50 for text output
- ADJUST `--limit <N>` to cap results when JSON default (all) is too broad
- PAGINATE via `--offset <N>` for large datasets
- INCLUDE soft-deleted memories via `--include-deleted`
- EXPORT full dump with `--limit 10000 --json` before backup
- RESPONSE includes `total_count` (total matching rows ignoring limit), `truncated` (true when limit was applied), and `body_length` per item (byte length of the stored body)
### REQUIRED — Streaming Export (export)
- USE `export` to stream all memories as NDJSON for portable backup or migration
- SUPPORTS `--namespace`, `--type`, `--include-deleted`, `--limit`, and `--offset`
- OUTPUT is NDJSON: one JSON line per memory plus a final summary line
- REDIRECT to a file for offline backup: `sqlite-graphrag export --limit 1000 > backup.ndjson`
- FILTER by type and namespace: `sqlite-graphrag export --type decision --namespace my-project > decisions.ndjson`
### Correct Pattern — Read Examples
- `sqlite-graphrag read --name design-auth --json`
- `sqlite-graphrag list --type decision --limit 100 --json`
- `sqlite-graphrag list --include-deleted --json | jaq '.items[] | select(.deleted)'`


## CRUD — Update with edit, rename, and restore
### REQUIRED — Body and Description Editing (edit)
- USE `edit --name <name> --body <text>` for short bodies
- PREFER `--body-file` or `--body-stdin` for long bodies
- CHANGE description via `--description <text>`
- EACH edit creates a new immutable version preserving history
- EDIT re-generates vector embedding when body changes — `recall` and `hybrid-search` return accurate scores after edit (since v1.0.63; description-only edits skip re-embedding)
- VALIDATE exit code 3 as an optimistic locking conflict
- JSON response: `memory_id`, `name`, `action` ("updated"), `version`, `elapsed_ms`
### REQUIRED — History-Preserving Rename (rename)
- USE `rename --name <old> --new-name <new>`
- ACCEPT `--old`/`--new` and `--from`/`--to` as aliases since v1.0.35
- PRESERVE all versions and graph connections
- TREAT exit code 4 as missing source memory
- JSON response: `memory_id`, `name` (new), `action` ("renamed"), `version`, `elapsed_ms`
### REQUIRED — Old Version Restore (restore)
- INSPECT versions via `history --name <name>` first
- USE `restore --name <name> --version <N>` for a specific version
- OMIT `--version` to select the last non-restore version automatically
- RESTORE creates a new version without overwriting prior history
- RESTORE preserves the current memory name — if a memory was renamed after the target version was created, the name stays as-is (fixed in v1.0.63; previously reverted to the version's original name)
- RE-EMBED occurs automatically so vector recall can find it again
- JSON response includes `action: "restored"` field, consistent with other CRUD commands
### REQUIRED — Optimistic Locking
- PASS `--expected-updated-at <epoch_or_RFC3339>` in concurrent pipelines
- TREAT exit code 3 as detected concurrency
- RELOAD `read --json` to get the new `updated_at` before retrying
- APPLY locking in `edit`, `rename`, and `restore`
### Correct Pattern — Update Flows
- `sqlite-graphrag edit --name design-auth --body-file ./revised.md --expected-updated-at "2026-04-19T12:00:00Z"`
- `sqlite-graphrag rename --from old-name --to new-name`
- `sqlite-graphrag history --name design-auth --json && sqlite-graphrag restore --name design-auth --version 2`


## CRUD — Delete with forget, purge, unlink, and cleanup-orphans
### REQUIRED — Soft Delete (forget)
- USE `forget --name <name>` for reversible soft-delete
- MEMORY disappears from `recall` and `list` by default
- VERSION history remains intact in the database
- REVERSIBLE via `restore` while no purge has occurred
- JSON response: `action` (`"soft_deleted"` `"already_deleted"`), `forgotten`, `name`, `namespace`, `deleted_at?`, `deleted_at_iso?`, `elapsed_ms`
- Since v1.0.52: when the memory is not found, `forget` no longer emits JSON to stdout; only a stderr error message and exit code 4 are produced
### REQUIRED — Hard Delete (purge)
- USE `purge --retention-days <N> --yes` in automation
- DEFAULT retention is 90 days for soft-deleted memories
- RUN `--dry-run` first to audit the count
- PERMANENTLY deletes rows and reclaims disk space
### REQUIRED — Edge Removal (unlink)
- USE `unlink --from <a> --to <b> --relation <type>`
- ACCEPT `--source`/`--target` as aliases of `--from`/`--to`
- TREAT exit code 4 as nonexistent edge
- `--relation` is now OPTIONAL; omitting it removes ALL relationships between the pair regardless of type
- NEW MODE: `unlink --entity <name> --all` removes all edges (both directions) of a given entity in one call
### REQUIRED — Orphan Entity Cleanup (cleanup-orphans)
- RUN `cleanup-orphans --dry-run` to audit
- APPLY `--yes` in automated pipelines
- REMOVES entities with no linked memories or edges
- RUN periodically after bulk `forget` operations
### REQUIRED — Bulk Relation Pruning (prune-relations)
- USE `prune-relations --relation <type> --yes` to bulk-delete all relationships of a given type
- USE `--dry-run` to preview the count before committing
- USE `--show-entities` during `--dry-run` to include `affected_entity_names` in the response
- RUN `cleanup-orphans` after to remove entities left without relationships
### Correct Pattern — Forget and Restore Round-Trip
- `sqlite-graphrag forget --name decision-x`
- `sqlite-graphrag history --name decision-x --json | jaq '.deleted'`
- `sqlite-graphrag restore --name decision-x`
- `sqlite-graphrag recall "decision" --json`


## Immutable Version History
### REQUIRED — Inspection with history
- USE `history --name <name> --json` to list versions
- VERSIONS start at 1 and increment with each `edit` or `restore`
- CHRONOLOGICAL reverse order by default
- INCLUDES soft-deleted memories with flag `deleted: true`
### REQUIRED — Version Semantics
- EACH `edit` creates a new immutable version preserving prior ones
- EACH `restore` creates a new version with the body of an old version
- COMPLETE audit trail of who changed what and when
- RETENTION POLICY controls when to purge permanently
### Correct Pattern — Change Audit
- `sqlite-graphrag history --name design-auth --json | jaq '.versions[].created_at_iso'`


## GraphRAG Search
### REQUIRED — Five Search Commands
- USE `recall` for KNN vector search with automatic graph expansion
- USE `hybrid-search` for FTS5 and vector fusion via RRF
- USE `related` for multi-hop traversal from a known memory
- USE `graph traverse` for traversal from a typed entity
- USE `deep-research` for parallel multi-hop research with query decomposition
- COMBINE all five in the canonical three-layer pattern or use `deep-research` as a single-command alternative
### Deep Research (v1.0.64)
- `sqlite-graphrag deep-research "<query>" --k 20 --json` for parallel multi-hop research
- Decomposes query into up to 7 sub-queries via heuristic split (conjunctions, relational prepositions, explicit entities)
- Runs all sub-queries in parallel with bounded concurrency (JoinSet + Semaphore, max 8 permits)
- Returns `sub_queries[]`, `results[]` (deduplicated), `evidence_chains[]` (entity→relation→entity paths), and `stats`
- Use instead of manual 3-layer pipeline (hybrid-search → read → related) for comprehensive research in a single invocation
### REQUIRED — Canonical Three-Layer Pattern
- LAYER 1 — `hybrid-search` to find seed memories by name
- LAYER 2 — `read --name` to expand the full memory body
- LAYER 3 — `related` or `graph traverse` for a multi-hop subgraph
- APPLY layers in order, stopping when context suffices
- INJECT consolidated results into the LLM prompt
### REQUIRED — Layer 1 with hybrid-search
- USE `hybrid-search <query> --k 10 --rrf-k 60 --json`
- COMBINES FTS5 textual and KNN vector via Reciprocal Rank Fusion
- ADJUST `--weight-vec` and `--weight-fts` only with numerical evidence
- DEFAULT for both weights is `1.0` with balanced fusion
- EXTRACT only `name` via `jaq -r '.results[].name'` for the next stage
### REQUIRED — hybrid-search with Graph Expansion
- ENABLE graph traversal via `--with-graph` to discover connected memories
- ADJUST depth with `--max-hops <N>` (default 2)
- FILTER weak edges with `--min-weight <F>` (default 0.3)
- GRAPH results are in `graph_matches[]`, SEPARATE from `results[]`
- `graph_matches[]` uses RecallItem schema: `name`, `distance`, `source` ("graph"), `graph_depth`
- READ BOTH `results[]` and `graph_matches[]` when `--with-graph` is active
- EXTRACT via `jaq -r '(.results[] , .graph_matches[]) | .name'`
### REQUIRED — Alternative Layer 1 with recall
- USE `recall <query> --k 5 --json` for pure semantic queries
- ACCEPT `--limit` as an alias of `--k` since v1.0.35
- RECALL expands automatically via graph by default
- DISABLE automatic graph expansion via `--no-graph`
- INTERPRET `distance` increasing as similarity decreasing
- INTERPRET `score` as `1.0 - distance`, clamped to `[0.0, 1.0]`
- FIELD `source` indicates origin: `"direct"` (KNN) or `"graph"` (traversal)
- FIELD `graph_depth` present only in results with `source: "graph"`
- RecallResponse separates `direct_matches[]`, `graph_matches[]`, and `results[]` (aggregate)
- USE when the query does not mix exact tokens with natural language
### REQUIRED — Layer 2 with read --name
- USE `read --name <name>` to get the full body of the seed memory
- EXPAND context beyond the snippet returned by layer 1
- LOOP over the top-k names to build a context bundle
- PARSE fields `body`, `description`, `created_at_iso`
### REQUIRED — Layer 3 with related
- USE `related <name> --hops <N>` for multi-hop traversal
- TWO hops reveal transitive knowledge invisible to vector search
- HOP distance delivers an explicit signal to the orchestrator
- USE when the query requires chained multi-step reasoning
### REQUIRED — Alternative Layer 3 with graph traverse
- USE `graph traverse --from <root> --depth <N>` for a focused subgraph
- DEFAULT depth is 2 when omitted
- TREAT exit code 4 as nonexistent root entity
- HOPS return `entity`, `relation`, `direction`, `weight`, `depth`
- START from a typed entity, not a memory name
### REQUIRED — Score and Distance Semantics
- `recall` returns `distance` (lower is more similar) and `score` (1.0 - distance)
- `recall` returns `source` (`"direct"` or `"graph"`) and `graph_depth` (when graph)
- `hybrid-search` returns `combined_score`; higher is better ranking
- `hybrid-search` exposes `vec_rank` and `fts_rank` to audit fusion
- `hybrid-search` with `--with-graph` adds `graph_matches[]` in a separate field
- `related` returns `hop_distance`, explicit depth in the graph
- `graph traverse` returns `depth` per visited hop
- DISCARD weak hits before spending tokens in the prompt
### REQUIRED — Command Choice by Query Type
- BROAD conceptual query, `recall` with `--k 5`
- MIXED token and natural-language query, `hybrid-search` with `--rrf-k 60`
- MIXED query with graph context, `hybrid-search --with-graph --max-hops 2`
- EXPLORATORY query starting from memory, `related --hops 2`
- EXPLORATORY query starting from entity, `graph traverse --depth 2`
- GRAPH audit query, `graph entities` or `graph stats`
### FORBIDDEN — Search Anti-patterns
- NEVER use native SQLite text search in parallel with the binary
- NEVER confuse `distance` with `combined_score` in ranking
- NEVER increase `--hops` without inspecting `graph stats` first
- NEVER inject results without filtering by relevance threshold
- NEVER parallelize heavy searches without measuring host RSS
- NEVER skip layer 2 when the snippet is insufficient
- NEVER read only `.results[]` when `--with-graph` is active (you will miss `graph_matches[]`)
### Correct Pattern — Canonical Three-Layer Pipeline
- `sqlite-graphrag hybrid-search "auth jwt design" --k 10 --json | jaq -r '.results[].name' > seeds.txt`
- `while read -r name; do sqlite-graphrag read --name "$name" --json; done < seeds.txt > bodies.ndjson`
- `sqlite-graphrag related "$(head -n1 seeds.txt)" --hops 2 --json > graph.json`
- `paste -d '\n' bodies.ndjson <(cat graph.json) | claude --print`
### Correct Pattern — Pipeline with Graph Expansion
- `sqlite-graphrag hybrid-search "auth" --k 5 --with-graph --json | jaq -r '(.results[], .graph_matches[]) | .name' | sort -u > seeds.txt`
### Correct Pattern — Fine-Tuning hybrid-search Weights
- `--weight-vec 1.0 --weight-fts 1.0` equal weight, recommended default
- `--weight-vec 1.0 --weight-fts 0.0` reproduces pure recall baseline
- `--weight-vec 0.0 --weight-fts 1.0` reproduces pure FTS5
- `--weight-vec 0.7 --weight-fts 0.3` favors semantics over tokens
- `--weight-vec 0.3 --weight-fts 0.7` favors tokens over semantics
### Measured Gains of the Three-Layer Pattern
- REDUCTION of context tokens by up to 72x vs markdown dump
- INCREASE of accuracy by up to 18% over pure vector retrieval
- INCREASE of multi-hop accuracy from 30% to 50% according to Microsoft
- APPROXIMATE latency of 1-3 seconds on modern hardware (LLM one-shot)


## Graph — Construction and Inspection
### REQUIRED — Edge Creation (link)
- USE `link --from <a> --to <b> --relation <type>`
- ENTITIES must exist as typed nodes before linking, except with `--create-missing`
- USE `--create-missing` to auto-create nonexistent entities during link
- USE `--entity-type <type>` to set the type of auto-created entities (default `concept`)
- JSON response includes `created_entities: ["a", "b"]` when entities were created
- ACCEPT `--source`/`--target` as aliases of `--from`/`--to`
- SET `--weight` optional for relation weight (default 0.5)
- TREAT exit code 4 as nonexistent entity (without `--create-missing`)
### REQUIRED — Export with graph
- EXPORT snapshot via `graph --format json`
- USE `--format dot` for offline Graphviz
- USE `--format mermaid` to embed in Markdown
- WRITE directly to a file via `--output <PATH>`
- INSPECT `nodes` and `edges` in the exported JSON
### REQUIRED — Entity Enumeration (graph entities)
- USE `graph entities --json` to list all entities
- ACCESS via `jaq -r '.entities[].name'` (field is `entities`, NOT `items`)
- FILTER by `--entity-type <type>` when needed
- PAGINATE with `--limit` and `--offset`
- SORT with `--sort-by degree|name|created_at` (default `name`) and `--order asc|desc` (default `asc`)
- RESPONSE includes `degree` per entity (total number of edges, both directions)
- USE before planning traversals or batch links
### REQUIRED — Statistics (graph stats)
- USE `graph stats --json` before expensive traversals
- INSPECT `node_count`, `edge_count`, `avg_degree`, `max_degree`
- CHOOSE traversal depth based on actual density
- DETECT subgraph isolation before planning searches
### Canonical Relation Vocabulary
- `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`
- `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- Custom relation types (e.g., `implements`, `tested-by`, `blocks`) are accepted since v1.0.49; non-canonical values emit a `tracing::warn!`
### Valid Entity Types
- `project`, `tool`, `person`, `file`, `concept`, `incident`
- `decision`, `memory`, `dashboard`, `issue_tracker`
- `organization`, `location`, `date`


## Architecture Note — No Local Model Cache (v1.0.76)
- The `cache` subcommand was removed in v1.0.76 along with the ONNX model pipeline
- All embedding is handled by the LLM subprocess (claude or codex via OAuth)
- There is no local model to cache, list, or clear


## JSON Contract and Pipelines
### REQUIRED — Deterministic Output
- USE `--json` in all subcommands before piping
- PREFER `--json` over `--format json` in one-liners
- FILTER fields via `jaq` instead of regex on stdout
- READ only fields actually returned by the subcommand
- TREAT JSON as a SemVer-versioned API
### REQUIRED — --json vs --format json Matrix
- `--json` is accepted by ALL subcommands
- `--format json` accepted only in a subset with `--format`
- WHEN both are present, `--json` wins in conflict
- USE `--json` by default in portable pipelines
### REQUIRED — JSON vs NDJSON Distinction
- INDIVIDUAL commands emit a single JSON envelope on stdout
- `ingest` emits NDJSON, one JSON line per file plus summary on stdout
- CONSUME NDJSON via `jaq -c` or `while read -r line`
- AGGREGATE NDJSON into an array via `jaq -s` when needed
### REQUIRED — Critical Fields by Command
- `recall` returns `results[].name`, `snippet`, `distance`, `score`, `source` (`"direct"`/`"graph"`), `graph_depth?`
- `recall` response-level: `query`, `k`, `direct_matches[]`, `graph_matches[]`, `results[]`, `elapsed_ms`
- `hybrid-search` returns `results[].name`, `combined_score`, `score`, `vec_rank`, `fts_rank`, `source`, `body`, `normalized_score`, `vec_distance`, `fts_bm25`
- `hybrid-search` response-level: `query`, `k`, `rrf_k`, `weights`, `results[]`, `graph_matches[]`, `elapsed_ms`, `fts_degraded`, `fts_error`, `fts_auto_rebuilt`
- `hybrid-search` `graph_matches[]` uses RecallItem: `name`, `distance`, `source` ("graph"), `graph_depth`
- `related` returns `results[].name`, `hop_distance`, `relation`, `source_entity`, `target_entity`, `weight`
- `graph traverse` returns `hops[].entity`, `relation`, `direction`, `weight`, `depth`
- `read` returns `name`, `body`, `description`, `created_at_iso`, `updated_at_iso`
- `edit` returns `memory_id`, `name`, `action` ("updated"), `version`, `elapsed_ms`
- `rename` returns `memory_id`, `name` (new), `action` ("renamed"), `version`, `elapsed_ms`
- `forget` returns `action` (`"soft_deleted"`/`"already_deleted"`), `forgotten`, `name`, `namespace`, `elapsed_ms`
- `list` response-level: `items[]` (also available as `memories[]` alias since v1.0.66), `total_count`, `truncated`, `elapsed_ms`; each item includes `body_length` (byte length of stored body) in addition to existing fields
- `link` response includes `warnings` (array of strings) for non-canonical relation types or other advisory notices; weight reflects the actual DB value (v1.0.66 fix: previously echoed the requested weight even when edge already existed)
- `graph entities` items include `degree` (total edge count for the entity, both directions) and `description` (nullable, v1.0.66)
- `graph --format json` response includes `entities[]` alias for `nodes[]` (v1.0.66) for LLM agent compatibility
- `edit` accepts `--type` to change memory type without re-creating (v1.0.66)
- `deep-research` response includes optional `graph_context` with entities and relationships from result memories (v1.0.66)
- `health` response includes `vec_memories_missing` and `vec_memories_orphaned` for vector index diagnostics (v1.0.66)
- `health` returns `integrity_ok`, `schema_ok`, `vec_memories_ok`, `vec_entities_ok`, `vec_chunks_ok`, `fts_ok`, `fts_query_ok`, `model_ok`, `counts`, `wal_size_mb`, `journal_mode`, `db_path`, `db_size_bytes`, `sqlite_version`, `checks[]`; also emits `mentions_ratio` (float) and `mentions_warning` (string) when `mentions` edges exceed 50% of all relationships; since v1.0.65 also emits `top_relation` (string?), `top_relation_ratio` (float?), `applies_to_ratio` (float?), and `relation_concentration_warning` (string?) when any single relation exceeds 40%
- `health.counts` contains: `memories`, `entities`, `relationships`, `vec_memories`
- `stats` returns GLOBAL data (no namespace filter): `memories`, `entities`, `relationships`, `chunks_total`, `avg_body_len`, `namespaces[]`, `db_size_bytes`, `schema_version`, `elapsed_ms`; also includes legacy aliases `db_bytes`, `edges`, `memories_total`, `entities_total`, `relationships_total`
- `ingest` per file: `file`, `name`, `status` (`"indexed"`/`"skipped"`/`"failed"`/`"preview"`), `truncated`, `original_name?`, `original_filename?`, `memory_id?`, `action?`, `error?`, `body_length?` (byte length of indexed body, present on `"indexed"` lines)
- `ingest` summary: `summary` (true), `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- `export` per memory: one JSON line per memory (NDJSON); final summary line includes `exported`, `namespace`, `elapsed_ms`; supports `--namespace`, `--type`, `--include-deleted`, `--limit`, `--offset`
- `restore` returns `memory_id`, `name`, `action` ("restored"), `version`, `elapsed_ms`
- `prune-relations` returns `action` (`"pruned"`/`"dry_run"`), `relation`, `count`, `entities_affected`, `affected_entity_names?`, `namespace`, `elapsed_ms`
- `cache` subcommand was removed in v1.0.76 (no local model cache)


## JSON Error Envelope
### REQUIRED — Machine-Readable Error Format
- ALL errors emit a JSON object on stdout when `--json` is active: `{"error": true, "code": N, "message": "..."}`
- `code` matches the process exit code (see Exit Codes table)
- `message` is a stable English string suitable for logging and routing
- Stderr continues to carry human-readable tracing output regardless of `--json`
- Parse stdout for the `error` boolean BEFORE accessing other fields when the exit code is non-zero
- Example: `{"error": true, "code": 4, "message": "memory not found: design-auth"}`


## Exit Codes and Retry Strategy
### REQUIRED — Complete Exit Code Handling
- `0` equals success; parse stdout
- `1` equals validation (invalid weight, self-link, max-files exceeded)
- `2` equals Clap argument parsing error (invalid flags, bad timezone value, missing required args)
- `9` equals duplicate (memory already exists without `--force-merge`); since v1.0.51 also returned when the memory is soft-deleted — use `--force-merge` to restore and update, or `restore` to revive
- `3` equals optimistic locking conflict; reload and retry
- `4` equals entity, memory, or version not found
- `5` equals namespace error (invalid name or conflict)
- `6` equals payload above the size limit
- `10` equals database error; run `vacuum` and `health`
- `11` equals embedding failure (LLM subprocess error or model load failure)
- `12` equals failure loading `sqlite-vec`; check SQLite ≥ 3.40
- `13` equals partial batch failure; reprocess only failed
- `14` equals I/O error (inaccessible file, permission, disk full)
- `15` equals database busy; widen `--wait-lock`
- `20` equals internal error or JSON serialization failure
- `75` equals exhausted slots in ingest or other heavy command
- `77` equals RAM pressure; wait for free memory
### FORBIDDEN — Error Anti-patterns
- NEVER ignore a non-zero exit code as success
- NEVER reprocess the entire batch after exit 13
- NEVER increase concurrency after receiving 75 or 77
- NEVER attempt `restore` without inspecting `history` first
- NEVER assume ambiguity without reading stderr first
- NEVER confuse exit 1 (validation) with exit 9 (duplicate)


## Concurrency and Resources
### REQUIRED — Load Control
- START heavy commands with `--max-concurrency 1`
- INCREASE only after measuring host RSS and swap
- RESPECT the hard ceiling of `2×nCPUs` for heavy commands
- TREAT `init`, `remember`, `ingest`, `recall`, `hybrid-search` as heavy
- WIDEN `--wait-lock <ms>` when contention is expected
- LIMIT parallel ingestion in CI to avoid overwhelming the LLM subprocess
### REQUIRED — Two Parallelism Axes in ingest
- `--max-concurrency` governs simultaneous CLI invocations
- `--ingest-parallelism` governs extract plus embed in parallel
- ADJUST both independently according to RAM and CPU
- USE `--low-memory` to force unitary parallelism
- HONOR `SQLITE_GRAPHRAG_LOW_MEMORY=1` on constrained hosts


## Maintenance and Backup
### REQUIRED — Periodic Hygiene
- SCHEDULE `purge --retention-days 30 --yes` weekly
- RUN `vacuum` after large purges
- RUN `optimize` to refresh planner statistics
- CLEAN orphans via `cleanup-orphans --yes` after bulk forget
### REQUIRED — Safe Backup
- USE `sync-safe-copy --dest <path>` before syncing Dropbox or iCloud
- COMPRESS snapshots via `ouch compress` for remote upload
- EXPORT memories via `list --limit 10000 --json` to NDJSON
- VERSION the database with Git LFS when feasible
### REQUIRED — Schema Diagnostics
- USE `debug-schema --json` for troubleshooting
- INSPECT `schema_version`, `objects`, `migrations`
- COMMAND is hidden from `--help`; invoke by exact name
### Correct Pattern — Weekly Cron
- `sqlite-graphrag purge --retention-days 30 --yes`
- `sqlite-graphrag cleanup-orphans --yes`
- `sqlite-graphrag vacuum --json`
- `sqlite-graphrag optimize --json`
- `sqlite-graphrag sync-safe-copy --dest ~/Dropbox/graphrag.sqlite`


## Contract: Stdin and Stdout
### Input — Structured Arguments Only
- CLI flags accept typed arguments validated by `clap` with strict parsing
- Stdin accepts a raw body when `--body-stdin` is active on `remember` or `edit`
- Stdin accepts a graph JSON object with optional `body`, `entities`, and `relationships` when `--graph-stdin` is active on `remember`; invalid JSON fails instead of becoming memory body
- Body sources such as `--body`, `--body-file`, `--body-stdin`, and `--graph-stdin` are rejected when combined ambiguously
- `remember` accepts body payloads up to `512000` bytes and up to `512` chunks; larger payloads return exit code `6`
- Environment variables override defaults without mutating the file database
- The default database path is always `./graphrag.sqlite` in the invocation directory
- Language is controlled by `--lang en` or `--lang pt` for deterministic output


### Output — Deterministic JSON Documents
- Every subcommand emits exactly one JSON document when `--json` is set
- Keys are stable across releases inside the current major version line
- Timestamps follow RFC 3339 with UTC offset notation always present
- Optional fields may be omitted or serialized as `null`; agents must handle both forms
- Arrays preserve deterministic order sorted by `score` or `updated_at` descending


## Exit Codes Table
### Contract — Map Every Status To A Routing Decision
| Code | Meaning | Recommended Action |
| --- | --- | --- |
| `0` | Success | Continue the agent loop |
| `1` | Validation or runtime failure | Log and surface to operator |
| `2` | CLI argument parsing error (Clap) | Fix arguments then retry |
| `9` | Duplicate memory (includes soft-deleted) | Use `--force-merge` to restore and update |
| `3` | Optimistic update conflict | Re-read `updated_at` and retry |
| `4` | Memory or entity not found | Handle missing resource gracefully |
| `5` | Namespace limit or unresolved | Pass `--namespace` explicitly |
| `6` | Payload exceeded allowed limits | Split body into smaller chunks |
| `10` | SQLite database error | Run `health` to inspect integrity |
| `11` | Embedding generation failed | Check model files and retry |
| `12` | `sqlite-vec` extension failed | Reinstall binary with bundled extension |
| `13` | Batch operation partially failed | Inspect partial results and retry failed items |
| `14` | I/O error (file, permission, disk full) | Check file access and available disk space |
| `15` | Database busy after retries | Wait and retry the operation |
| `20` | Internal error or serialization failure | Report bug with full stderr output |
| `75` | Advisory lock held or all slots full | Wait and retry, or lower pressure on heavy commands instead of raising concurrency blindly |
| `77` | Low memory threshold tripped | Free RAM before retry |


## JSON Output Format
### Recall — Vector-Only KNN
```json
{
  "query": "graphrag retrieval",
  "k": 3,
  "direct_matches": [
    { "memory_id": 1, "name": "graphrag-intro", "namespace": "global", "type": "user", "description": "intro doc", "snippet": "GraphRAG combines...", "distance": 0.09, "source": "vec" }
  ],
  "graph_matches": [],
  "results": [
    { "memory_id": 1, "name": "graphrag-intro", "namespace": "global", "type": "user", "description": "intro doc", "snippet": "GraphRAG combines...", "distance": 0.09, "source": "vec" }
  ],
  "elapsed_ms": 12
}
```


### Hybrid Search — FTS5 Plus Vector RRF
```json
{
  "query": "postgres migration",
  "k": 5,
  "rrf_k": 60,
  "weights": { "vec": 1.0, "fts": 1.0 },
  "results": [
    { "memory_id": 1, "name": "postgres-migration-plan", "namespace": "global", "type": "project", "description": "migration plan", "body": "Step 1...", "combined_score": 0.96, "score": 0.96, "source": "hybrid", "vec_rank": 1, "fts_rank": 1 },
    { "memory_id": 2, "name": "db-migration-checklist", "namespace": "global", "type": "reference", "description": "checklist", "body": "Check indexes...", "combined_score": 0.88, "score": 0.88, "source": "hybrid", "vec_rank": 2, "fts_rank": 3 }
  ],
  "graph_matches": [],
  "elapsed_ms": 18
}
```


## Idempotency and Side Effects
### Read-Only Commands — Zero Mutations Guaranteed
- `recall` reads the vector and metadata tables without touching disk state
- `read` fetches a single row by name and emits JSON without side effects
- `list` paginates memories sorted deterministically with stable cursors
- `health` runs SQLite `PRAGMA integrity_check` and reports without writing
- `stats` counts rows in read-only transactions safe for concurrent agents


### Write Commands — Optimistic Locking Protects Concurrency
- `remember` uses `ON CONFLICT(name)` so duplicate calls return exit code `9`
- `rename` requires `--expected-updated-at` to detect stale writes via exit `3`
- `edit` creates a new row in `memory_versions` preserving immutable history
- `restore` rewinds content while appending a new version instead of overwriting
- `forget` is soft-delete so re-running it is safe and idempotent by design


## Payload Limits
### Ceilings — Enforced By The Binary
- `EMBEDDING_MAX_TOKENS` equals 512 tokens measured by the model tokenizer
- `TEXT_BODY_PREVIEW_LEN` equals 200 characters in list and recall snippets
- `MAX_CONCURRENT_CLI_INSTANCES` equals the hard ceiling of 4 across cooperating subprocess agents, but heavy commands may clamp lower dynamically from available RAM
- `CLI_LOCK_DEFAULT_WAIT_SECS` equals 300 seconds before exit code `75`
- `PURGE_RETENTION_DAYS_DEFAULT` equals 90 days before hard delete becomes allowed


## Language Control
### Bilingual Output — One Flag Switches Locale
- Flag `--lang en` forces English messages regardless of system locale
- Flag `--lang pt` or `--lang pt-BR` or `--lang portuguese` or `--lang PT` forces Portuguese
- Short codes `en` and `pt` are the canonical forms; the longer aliases are accepted without error
- Env `SQLITE_GRAPHRAG_LANG=pt` overrides system locale when `--lang` is absent
- Missing flag and env falls back to `sys_locale::get_locale()` detection
- Unknown locales default to English without emitting any warning to stderr
- Env `SQLITE_GRAPHRAG_DISPLAY_TZ=America/Sao_Paulo` sets the IANA timezone applied to all `*_iso` fields in JSON output
- Flag `--tz <IANA>` takes priority over `SQLITE_GRAPHRAG_DISPLAY_TZ`; both fall back to UTC when absent
- Invalid IANA names cause exit 2 with a `Validation` error message before any command runs
- Only `*_iso` string fields are affected; integer epoch fields (`created_at`, `updated_at`) remain unchanged
- Env `SQLITE_GRAPHRAG_LOG_FORMAT=json` switches tracing output to newline-delimited JSON; default is `pretty`


## ARM64 GNU Runtime Note (v1.0.76)
### ONNX Runtime No Longer Required
- Since v1.0.76, all embedding is handled by the LLM subprocess
- No `libonnxruntime.so`, no `ORT_DYLIB_PATH`, no local ONNX model needed
- The binary is self-contained on all platforms including `aarch64-unknown-linux-gnu`


## JSON Output Flag
### Format — `--json` Is Universal and `--format json` Is Command-Specific
- Every subcommand accepts `--json` for deterministic JSON stdout
- Only commands that expose `--format` in their help accept `--format json`
- `--json` is the short form — preferred in one-liners and agent pipelines
- If `--json` appears with a non-JSON `--format`, `--json` wins and stdout remains JSON
- `--format json` is the explicit form — command-specific, preferred where alternate output modes also exist


## Graph Input Payloads
### Contract — `remember` Graph Files
- `--entities-file` accepts a JSON array of entity objects
- Each entity object MUST include `name` and `entity_type`
- The alias field `type` is accepted as a synonym for `entity_type`
- Agents MUST NOT send both `entity_type` and `type` in the same entity object
- Valid `entity_type` values are `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, and `date`
- `--relationships-file` accepts a JSON array of relationship objects
- Each relationship object MUST include `source`/`from`, `target`/`to`, `relation`, and `strength`
- `strength` MUST be a floating-point number in the inclusive range `[0.0, 1.0]`
- Stored graph outputs expose this value as `weight`
- File payloads MAY use canonical stored relation names with underscores such as `applies_to`, `depends_on`, and `tracked_in`; dashed aliases are normalized before storage
- CLI flags for `link` and `unlink` use dashed labels such as `applies-to`, `depends-on`, and `tracked-in`
- `--graph-stdin` accepts a single object with optional `body` plus the same `entities` and `relationships` arrays
- `link --create-missing` auto-creates entities that do not exist during linking, defaulting to type `concept`; use `--entity-type` to override (added in v1.0.44)
- `hybrid-search --with-graph` enables graph traversal seeded from top RRF results; graph matches appear in the `graph_matches` array alongside the `results` array (fixed in v1.0.44 — was previously a no-op)
- `graph entities` JSON response uses top-level key `entities` (renamed from `items` in v1.0.44); update existing `jaq` scripts from `.items[]` to `.entities[]`


## Machine-Readable Schemas
### JSON Schema Draft 2020-12 Files For Every Subcommand
- Directory `docs/schemas/` ships one `.schema.json` file per subcommand
- Every schema declares `"additionalProperties": false` — unknown keys are contract violations
- Schemas use `$defs` for shared subtypes (e.g. `RecallItem`, `HealthCheck`)
- Optional fields are absent from the `required` array and typed with `["T", "null"]` where nullable
- Validate a live response with a real JSON Schema validator: `jsonschema --instance <(sqlite-graphrag stats) docs/schemas/stats.schema.json`
- File `docs/schemas/debug-schema.schema.json` covers the hidden `debug-schema` diagnostic subcommand
- Schemas are updated on every breaking change and follow the CLI SemVer major version


## Superpowers Summary
### Five Reasons Your Orchestrator Will Stay
- Deterministic output eliminates fragile regex parsing in your agent glue code
- Exit codes route decisions without scraping stderr for human-readable messages
- Single binary deploys identically in Docker, GitHub Actions and developer laptops
- SQLite durability survives kernel panics and container kills without corruption
- Graph-native retrieval surfaces multi-hop context that flat vector search misses


## Get Started In 30 Seconds
### Install — One Command Installs The Full Stack
```bash
cargo install --path . && sqlite-graphrag init
```
- Flag `--locked` reuses the shipped `Cargo.lock` to protect MSRV from transitive drift
- Command `init` creates `graphrag.sqlite` in the current working directory and downloads the embedding model locally
- First invocation requires a working OAuth session (Claude Pro/Max or OpenAI ChatGPT Pro)
- Each embedding call spawns and discards an LLM subprocess; there is no persistent model or daemon
- Uninstall with `cargo uninstall sqlite-graphrag` leaving the database file in place


## New in v1.0.82 — Five Gaps Closed
### REQUIRED — pending (Three-Stage remember Checkpoint Queue, ADR-0036)
- USE `sqlite-graphrag pending list --filter-status queued --json` to inspect the queue
- USE `sqlite-graphrag pending show <id> --json` to inspect one row
- USE `sqlite-graphrag pending cleanup --yes --json` to remove terminal-state rows
- SCHEMA: `docs/schemas/pending-list.schema.json`
- EXIT code 4 when `show <id>` references a missing id; exit 1 for invalid `--filter-status`
### REQUIRED — pending-embeddings (Retry Queue, ADR-0040)
- USE `sqlite-graphrag pending-embeddings list --json` to inspect the queue
- USE `sqlite-graphrag pending-embeddings process --json` to reprocess with the next backend in `--llm-backend`
- SCHEMA: `docs/schemas/embedding-list.schema.json`
- COMBINE with `--llm-backend codex,claude` for automatic backend rotation
### REQUIRED — slots (Cross-Process LLM Semaphore, ADR-0039)
- USE `sqlite-graphrag slots status --json` to inspect host-wide slot usage
- USE `sqlite-graphrag slots release --slot-id <N> --yes --json` to reap orphan slots
- FIELDS: `max_concurrency`, `acquired`, `waiting`, `held_by_pid[]`, `p50_wait_ms`, `p99_wait_ms`
- LOCK crate is `fs4 = "0.9"` with `sync` (NOT `fs2`); native backend is `fcntl(F_SETLK)` on Unix and `LockFileEx` on Windows
- COMBINE with `--llm-max-host-concurrency N` to override the default ceiling
### REQUIRED — embedding (Pending Queue Health, ADR-0040)
- USE `sqlite-graphrag embedding status --json` for aggregate counts per status
- USE `sqlite-graphrag embedding list --json` for per-entry inspection
- SCHEMAS: `docs/schemas/embedding-status.schema.json` and `embedding-list.schema.json`
### REQUIRED — --llm-backend Global Flag (ADR-0038)
- USE `--llm-backend codex,claude` to fall back from codex to claude on error
- USE `--llm-backend codex,claude,none` with `--skip-embedding-on-failure` to allow null embedding
- DEFAULT is `codex`; explicit chain only when operators want fallback behavior
### REQUIRED — Exit Code 19 Shutdown JSON Envelope (ADR-0037)
- TREAT exit code 19 as `SHUTDOWN_EXIT_CODE`; partial work was discarded
- ENVELOPE on stdout when SIGTERM/SIGINT/SIGHUP arrives during LLM subprocess
- FIELDS: `error: true`, `code: 19`, `signal`, `graceful: bool`, `message`
- SCHEMA: `docs/schemas/shutdown-envelope.schema.json`
- COMBINE with `--graceful-shutdown-secs <N>` to reserve cleanup time before kill
### REQUIRED — codex OAuth 401 Incident (2026-06-14)
- OPERATOR ACTION after upgrade: `codex login` to refresh the OAuth refresh token
- The stderr-capture fallback chain in ADR-0040 detects `refresh_token_reused` and routes to the next backend in `--llm-backend`
- There is NO definitive upstream fix; mitigation depends on operator-driven `codex login`
