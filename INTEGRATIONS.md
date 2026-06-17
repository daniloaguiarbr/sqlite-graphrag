# Integrations

> Read this document in [Portuguese (pt-BR)](INTEGRATIONS.pt-BR.md)


> 21 agents and 20+ platforms in a single CLI contract

- Read the Portuguese version at [INTEGRATIONS.pt-BR.md](INTEGRATIONS.pt-BR.md)
- Every recipe below is ready to copy and costs nothing to run
- **v1.0.79: every build is LLM-only and one-shot.** Embedding generation delegates to a headless `claude code` or `codex` subprocess (OAuth). The daemon, the ONNX runtime and the `embedding-legacy` feature were fully removed; embeddings are batched, parallel (`--llm-parallelism`) and default to 64 dimensions (`--embedding-dim`, range [8, 4096]).


## CLI Flag Aliases (since v1.0.35)
- `recall` and `hybrid-search` accept `--limit` as an alias of `-k`/`--k`. Existing examples below use `--k` and remain valid.
- `rename` accepts `--from`/`--to` as aliases of `--name`/`--new-name` (legacy `--old`/`--new` also remain valid).
- All `schema_version` JSON fields (`init`, `stats`, `migrate`, `health`) are emitted as JSON numbers (was string in `init`/`stats`/`migrate` before v1.0.35).
- Auto-init via `remember`/`ingest`/etc. now activates `journal_mode = wal` correctly (regression fix).

## New Flags (since v1.0.45)
- NER entity extraction is **disabled by default**. Pass `--enable-ner` on `remember` or `ingest` to opt in; set `SQLITE_GRAPHRAG_ENABLE_NER=1` for a persistent session override.
- `--skip-extraction` is deprecated and has no effect since v1.0.45 (NER is off by default); the flag is kept as a hidden no-op for backwards compatibility — remove it from scripts.
- `--graph-stdin` on `remember` reads a single JSON object from stdin containing `body`, `entities`, and `relationships`, making it the preferred way to supply curated graphs from an LLM.

## New Flags (since v1.0.47)
- The GLiNER zero-shot NER pipeline was REMOVED in v1.0.79 with the `ner-legacy` feature; `--enable-ner` now performs URL-regex extraction only.
- `--gliner-variant`, `SQLITE_GRAPHRAG_GLINER_VARIANT` and `SQLITE_GRAPHRAG_GLINER_THRESHOLD` are accepted for compatibility but have NO effect since v1.0.79.
- For LLM-curated entity/relationship extraction use `ingest --mode claude-code` or `ingest --mode codex`.
- Entity types now include `organization`, `location`, `date` alongside `person`, `project`, `tool`, `file`, `concept`, `decision`, `incident`, `dashboard`, `issue_tracker`, `memory`.

## New Commands and Flags (since v1.0.68)
### Process Lifecycle (G28)
- `enrich`, `ingest --mode claude-code`, and `ingest --mode codex` now acquire a per-namespace singleton before doing real work.  A second concurrent invocation against the same database fails fast with `AppError::JobSingletonLocked { job_type, namespace }` (exit 75) instead of stacking up subprocess trees.
- `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` env var (opt-in) — when set to an existing empty directory, the Claude Code subprocess is spawned with `CLAUDE_CONFIG_DIR=<that dir>`, suppressing user-scoped MCP servers and their 8-10-process fan-out.  This is the only mechanism upstream Claude Code actually honours (see [anthropics/claude-code#10787]).  We deliberately do NOT pass `--strict-mcp-config` or `--mcp-config '{}'` because both are ignored.
- `retry::CircuitBreaker` (Rust crate API) — opt-in helper with `AttemptOutcome::{Success, Transient, HardFailure}`.  Rate-limited and timeout errors are explicitly excluded from the failure count.  Use in custom retry loops to cap persistent-failure iterations.
- `enrich` emits a `tracing::warn!` (visible with `-v`) when `--llm-parallelism > 4`, recommending to combine with `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` to keep subprocess fan-out manageable.
### Windows Build (G29)
- `cargo install sqlite-graphrag` on Windows now succeeds.  `HANDLE` type is treated type-safely via `!handle.is_null() && handle != INVALID_HANDLE_VALUE`.  `windows-sys` is pinned to `=0.59.0` exact in `Cargo.toml`.  New CI job `windows-build-check` runs `cargo check --target x86_64-pc-windows-msvc --lib --all-features` on every push and PR.

## New Commands and Flags (since v1.0.69)
### OAuth-Only Enforcement (G28-A, G31, Behaviour Change)
- `claude -p` and `codex exec` spawns now ABORT with `AppError::Validation` if `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` are present in the environment.  OAuth (Claude Pro/Max or ChatGPT Pro) is the ONLY accepted credential flow.  See `docs/decisions/adr-0011-oauth-only-enforcement.md` for the full rationale.
- The `--bare` flag (which demands an API key and disables OAuth) is REMOVED from every executable path.  Both API key env vars are also excluded from the `env_clear` whitelist as defence in depth.
### `enrich` — New Subcommand (G29 + G35 + G37)
- `enrich --operation <op> --mode <claude-code|codex> --json` runs LLM-curated graph quality.  Three operations are fully implemented: `memory-bindings` (extract entities from orphan memories), `entity-descriptions` (fill NULL/empty entity descriptions), and `body-enrich` (expand short memory bodies, now succeeds 100% after the G29 hotfix on the `source` CHECK constraint and the G29 audit trail via `memory_versions`).
- `--preserve-threshold <FLOAT>` (default 0.7) controls the Jaccard trigram preservation gate from `src/preservation.rs` (10 tests).  Scores below the threshold are rejected and emitted as `EnrichItemResult::PreservationFailed`.
- `--preflight-check`, `--fallback-mode <claude-code|codex>`, and `--rate-limit-buffer <SECONDS>` (default 300) prevent batch loss when the Claude OAuth 5-hour window closes mid-run.  The preflight probe issues a 1-turn ping; on a rate limit it aborts with a clear error or switches to `--fallback-mode`.
- `--names <a,b,c>` and `--names-file <PATH>` select a specific subset of memory names.  `--names-file` accepts `#` comments and blank lines.  Both flags combine as a union.
- `--llm-parallelism <N>` warning is conditional to the mode: Claude warns at 5 (OAuth-MCP fan-out), Codex warns at 17 (rate-limit risk), Codex 5..16 is silent (validated at 1161 items, 0 failures in production).
- `--max-load-check` refuses to start when load average > `2 × ncpus`.  `--circuit-breaker-threshold <N>` (default 5) aborts after N consecutive `HardFailure` outcomes.
### `vec` Subcommand Family (G39)
- `vec orphan-list --json` lists orphan memory embedding rows with `vector_hash` (BLAKE3 of the embedding blob).
- `vec purge-orphan --yes --dry-run --json` previews the deletion.  `vec purge-orphan --yes --json` purges the THREE vec tables (`vec_memories`, `vec_entities`, `vec_chunks`) in a single transaction.
- `vec stats --json` exposes `vec_memories_rows`, `vec_entities_rows`, `vec_chunks_rows`, `orphans`, and the last vacuum timestamp.
- `forget` now calls `memories::delete_vec` BEFORE the soft-delete, preventing new orphans in the steady state.
### `codex-models` Subcommand (G33)
- `codex-models --json` lists the ChatGPT Pro OAuth accepted-model whitelist: `codex-auto-review`, `gpt-5.3-codex-spark`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.5`.  Returns `models`, `count`, and `default`.
- `codex-models --suggest <substring> --json` returns the closest match via substring lookup with a Levenshtein fallback.  `enrich --codex-model-validate` (default true) checks the model BEFORE the subprocess is spawned and aborts with a suggestion when invalid.  `--codex-model-fallback <MODEL>` auto-substitutes instead of aborting.
### `optimize` and `backup` Hardening (G36 + G38)
- `optimize` pre-checks FTS5 health via `check_fts_functional` BEFORE rebuilding.  `--fts-dry-run` exits 1 if rebuild is recommended.  `--fts-progress <N>` (default 30) emits progress every N seconds.  `--yes` skips the confirmation prompt.  `--no-fts-skip-when-functional` forces a rebuild.
- `backup` defaults to `run_to_completion(1000, Duration::from_millis(5), None)` — 25x faster than the v1.0.68 defaults.  `--backup-step-size <PAGES>`, `--backup-step-sleep-ms <MS>`, `--backup-no-sleep`, and `--backup-progress <PAGES>` (default 100) provide tunability.
### Singleton Scoped by `db_hash` (G30)
- `lock::acquire_job_singleton(job_type, namespace, db_path, wait_seconds, force)`.  Two concurrent `enrich` invocations against DIFFERENT databases no longer collide.  `db_hash` is the first 12 hex chars of `blake3(canonicalize(db_path))`.
- `--wait-job-singleton <SECONDS>` polls for the lock.  `--force-job-singleton` breaks a stale lock.  Both available on `enrich` and `ingest`.
### Codex Spawn Helper Unified (G31 + G32 + G33)
- `src/commands/codex_spawn.rs` (~700 lines, 11 tests) unifies the spawn pipeline, JSONL parser, and ChatGPT Pro OAuth model validation.  Both `enrich --mode codex` and `ingest --mode codex` consume the same canonical command.  The external `~/.local/bin/codex-clean` wrapper is now obsolete.
- 7 hardening flags: `--json --output-schema --ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules` plus `-c mcp_servers='{}' --ask-for-approval never`.  Schema JSON now lives in `paths::AppPaths::cache_dir().join("schemas")` instead of `/tmp` (trusted dir).
### `MemorySource` Enum and Preservation (G29)
- `src/memory_source.rs` defines a type-safe enum of the five CHECK-constraint values: `Agent`, `User`, `System`, `Import`, `Sync`.  `TryFrom<&str>` returns `AppError::Validation` listing the accepted values.  Runtime guard `validate_source` is called from `memories::insert` and `memories::update`.  The enum is the foundation for the v1.0.70 migration.
- Idempotency via `blake3::hash`: when `old_hash == new_hash`, the body is skipped with reason `"enriched body hash matches original (blake3:{hash}); idempotency skip"`.  Reprocessing the same memory is safe.
### Circuit Breaker and System Load (G28-D)
- `retry::CircuitBreaker` is integrated into the worker loop with `breaker.record(AttemptOutcome::HardFailure)`.  The loop aborts after `--circuit-breaker-threshold` consecutive failures (default 5, set to 0 to disable).
- `src/system_load.rs` provides `load_average_one`, `ncpus`, and `is_system_saturated`.  `enrich` aborts the spawn when `load_average_one() > 2 * ncpus` and `--max-load-check` is set (default true).
### Orphan Reaper (G28-C)
- `src/reaper.rs` walks `/proc` at startup, kills any `claude`/`codex` orphan with `PPID=1` and age greater than 60s.  Invoked from `main` BEFORE any work.  4-test suite: `orphan_min_age_is_one_minute`, `orphan_targets_include_claude_and_codex`, `reaper_report_starts_zeroed`, `scan_completes_without_panic_on_linux`.

## New Commands and Flags (since v1.0.76)
### LLM-Only One-Shot Architecture (G21 + G22 + G23 + G24 + G25)
- The default build of v1.0.76 is LLM-Only and one-shot.  No daemon, no ONNX runtime, no `multilingual-e5-small` model download.  Embedding generation and NER delegate to a headless `claude code` or `codex` subprocess (OAuth, no MCP, no hooks).  Release binary is approximately 6 MB.
- The `embedding-legacy` feature was REMOVED in v1.0.79 (ahead of the v1.1.0 schedule).  The legacy fastembed + ort + tokenizers pipeline no longer exists; every build is LLM-only.
- See ADR-0019, ADR-0020, ADR-0021, ADR-0022, ADR-0023, ADR-0024, ADR-0025, ADR-0026 for the full architectural decisions.
### `migrate` Subcommand Family (v1.0.76)
- `migrate --rehash --json` rewrites recorded migration checksums to match the current file content.  Algorithm matches `refinery-core 0.9.1` (SipHasher13, same hashing order).  Required for v1.0.74 → v1.0.76 upgrades where V002 was intentionally emptied to a no-op.  Response schema: `migrate-rehash.schema.json`.
- `migrate --to-llm-only --drop-vec-tables --json` is the one-shot upgrade for v1.0.74 / v1.0.75 databases: rehash + V013 vec-table drop + vec-table state report.  The `--drop-vec-tables` flag is REQUIRED as a safety guard.  Response schema: `migrate-to-llm-only.schema.json`.
### BLOB-Backed Embedding Tables (G22)
- V013 migration drops the `vec_memories`, `vec_entities`, `vec_chunks` virtual tables and replaces them with regular BLOB-backed `memory_embeddings`, `entity_embeddings`, `chunk_embeddings` tables.  Cosine similarity is computed in pure Rust on demand in `src/similarity.rs` (ADR-0020, ADR-0022).
### Hybrid Search Refinement (G24)
- `hybrid-search` uses FTS5 for coarse filtering and refines the candidate set with a pure-Rust cosine over the BLOB embeddings.  FTS5 stays healthy because the rebuild is gated by `optimize --fts-skip-when-functional` (G36 from v1.0.69).
### Extraction Backend Selector
- New `--extraction-backend llm|embedding|none|both` global flag (default `llm`) selects the extraction backend.  `llm` is the LLM-backed path; `embedding` is a permanent stub since v1.0.79 (legacy pipeline removed) that returns a migration error; `none` is a no-op; `both` runs them in parallel and merges the results.
- `src/extract/` exposes the `ExtractionBackend` trait with the four implementations.  `src/spawn/` exposes the `VersionAdapter` trait with `CodexAdapter` (detects `codex 0.130.0` through `0.138+` and adapts flags — `codex 0.137.0` removed `--ask-for-approval` in favour of `-a never`), `ClaudeAdapter` (claude code 2.1.0+), and `OpencodeAdapter` (opencode headless).
### Daemon Removal (ADR-0021)
- The `daemon` subcommand was DEPRECATED in v1.0.76 and FULLY REMOVED in v1.0.79 (ahead of the v1.1.0 schedule).  The LLM subprocess is the "model loader"; the CLI is 100% one-shot with zero IPC.

## New Commands and Flags (v1.0.79 — G42 embedding pipeline)
- `--embedding-dim <N>` global flag sets the embedding dimensionality (default 64, range [8, 4096]); precedence: flag > `SQLITE_GRAPHRAG_EMBEDDING_DIM` env > the `dim` recorded in `schema_meta` > 64; existing 384-dim databases keep working unchanged
- `--llm-parallelism <N>` is now available on `remember` (default 4), `ingest` (default 2) and `edit` — bounded fan-out via `Semaphore` + `JoinSet`, permits clamp [1, 32]
- `enrich --operation re-embed --limit N --resume` is the canonical one-shot re-embed path (e.g. after changing `--embedding-dim`)
- `edit --force-reembed` regenerates the embedding of one memory without changing its body
- `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL` overrides the claude embedding model (symmetric to the codex variable); `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS` bounds each LLM embedding call (default 300)
- LLM calls are batched (`{items:[{i,v}]}` schema — calibration bases of 8 chunks / 25 entity names at dim 64, dim-adaptive as clamp(base×64/dim, 1, base) since G44) and every subprocess uses `kill_on_drop` plus an explicit timeout

## New Commands and Flags (since v1.0.67)
- `remember-batch` batch-creates memories from NDJSON stdin in a single invocation; `--transaction` for atomicity, `--force-merge` for idempotent updates, `--fail-fast` to stop on first error
- `completions` generates shell completions for Bash, Zsh, Fish, PowerShell, and Elvish
- `read --id <N>` fetches a memory by integer `memory_id` directly (bypasses name resolution)
- `read --with-graph` includes linked entities and relationships in the JSON response
- `enrich --llm-parallelism <N>` spawns N parallel LLM worker threads (default 1, max 32)
- `health` detects super-hub entities (degree > 50) and reports `super_hub_count`, `top_hub_entity`, `top_hub_degree`
- `health` reports `non_normalized_count` and `normalization_warning` for entities not matching kebab-case
- `edit` skips re-embedding when body content is unchanged (body_hash comparison)
- `rename` purges ghost soft-deleted memories occupying the target name before UPDATE
- `hybrid-search` and `recall` reject `--max-hops` and `--min-weight` when graph traversal is disabled
- V012 migration adds `created_at`/`updated_at` timestamps to relationships table

## New Commands and Flags (since v1.0.66)
- `edit --type` changes memory type without re-creating the memory
- `deep-research` `graph_context` field in JSON response with entities and relationships from result memories
- `graph --format json` includes `entities` alias alongside `nodes` for LLM agent compatibility
- `list --json` includes `memories` alias alongside `items` for LLM agent compatibility
- `graph entities --json` includes `description` field per entity
- `health --json` includes `vec_memories_missing` and `vec_memories_orphaned` counts

## New Commands and Flags (since v1.0.65)
- `reclassify-relation --from-relation <old> --to-relation <new> --batch` renames relationship types in bulk; single mode via `--source`/`--target`; handles UNIQUE collisions via `UPDATE OR IGNORE` + `DELETE`; `--dry-run` previews; optional `--filter-source-type`/`--filter-target-type`
- `normalize-entities --yes` normalizes all entity names to lowercase kebab-case ASCII; auto-merges collisions; `--dry-run` previews
- `enrich --operation <op> --mode claude-code` LLM-augmented graph quality; operations: `memory-bindings`, `entity-descriptions`, `body-enrich`; `--dry-run` previews without LLM; `--max-cost-usd`, `--resume`, `--retry-failed`
- `deep-research` new flags: `--rrf-k` (default 60), `--graph-decay` (default 0.7), `--graph-min-score` (default 0.05)), `--max-neighbors-per-hop`
- `--max-entity-degree N` on `link` and `remember` emits `tracing::warn!` when an entity exceeds N connections
- `health` reports `top_relation`, `top_relation_ratio`, `applies_to_ratio`, `relation_concentration_warning` when any relation exceeds 40%
- Entity names are normalized to lowercase kebab-case on every write path (remember, ingest, link, rename-entity)

## Daemon Behavior (HISTORICAL — daemon removed in v1.0.79)
- v1.0.50 through v1.0.78 only: the CLI auto-restarted the daemon on version mismatch.  Since v1.0.79 there is no daemon process at all

## New Commands and Flags (since v1.0.56)
- `fts rebuild` rebuilds the FTS5 full-text search index from scratch
- `fts check` runs FTS5 integrity-check without modifying the index
- `fts stats` shows FTS5 index statistics (row count, shadow pages, functional status)
- `backup --output <path>` creates a safe database copy via SQLite Online Backup API
- `delete-entity --name <entity> --cascade` deletes entity and cascades to all relationships and NER bindings
- `reclassify --name <entity> --entity-type <new>` changes entity type; `--from-type <old> --to-type <new> --batch` for bulk
- `merge-entities --names "a,b,c" --into <target>` merges source entities into target, moving all edges
- `rename-entity --name <old> --new-name <new>` renames a graph entity preserving all FK-based relationships and re-embeds for semantic search
- `memory-entities --name <memory>` lists entities linked to a specific memory
- `prune-ner --entity <name>` or `--all --yes` removes NER bindings from memory_entities table
- `cleanup-orphans --dry-run --json` audits entities with zero memories and zero relationships; `--yes` removes them
- `prune-relations --relation <type> --dry-run --json` previews bulk removal of all relationships of a given type; `--yes` executes
- `remember --dry-run` validates input and reports planned actions without persisting
- `remember --clear-body` explicitly clears body during `--force-merge` (empty body now preserves existing by default)
- `remember --type` and `--description` are now optional with `--force-merge` (inherited from existing memory)
- `list` default limit is all memories with `--json`, 50 for text; response includes `total_count`, `truncated`, `body_length`
- `history --diff` includes character-level change summary between consecutive versions
- `hybrid-search` graceful FTS5 degradation: `fts_degraded`, `fts_error`, `fts_auto_rebuilt` fields; auto-rebuilds on corruption
- `hybrid-search` adds `normalized_score` (0-1), `vec_distance`, `fts_bm25` raw scores
- `health` adds `fts_query_ok` (functional FTS5 MATCH test), `sqlite_version`
- `optimize --skip-fts` skips FTS5 rebuild; `fts_rebuilt` field in response
- `link --strict-relations` rejects non-canonical relation types; `warnings` field in response
- `unlink --relation` is now optional (removes all between pair); `--entity <name> --all` for bulk
- `graph entities --sort-by degree|name|created_at --order asc|desc`; `degree` field in response
- `ingest --max-name-length N` configures name truncation; `body_length` in NDJSON; auto-prefix `doc-` for numeric names
- `daemon --ping` added `model_name`, `model_variant` fields (HISTORICAL — the daemon was removed in v1.0.79)
- ALL error paths now emit JSON on stdout: `{"error": true, "code": N, "message": "..."}`
- FTS5 sync fixed in `edit`, `rename`, `restore` — edited memories are now immediately findable via full-text search


## Summary Table
### Catalog — Every Supported Integration
| Name | Type | Minimum Version | Example | Official Docs |
| --- | --- | --- | --- | --- |
| Claude Code | AI Agent | 1.0+ | `sqlite-graphrag recall "query" --json` | https://docs.anthropic.com/claude-code |
| Codex CLI | AI Agent | 0.5+ | `sqlite-graphrag remember --name X --type user --body "..."` | https://github.com/openai/codex |
| Gemini CLI | AI Agent | any recent | `sqlite-graphrag hybrid-search "query" --k 5 --json` | https://github.com/google-gemini/gemini-cli |
| Opencode | AI Agent | any recent | `sqlite-graphrag recall "auth flow" --json` | https://github.com/opencode-ai/opencode |
| OpenClaw | AI Agent | any recent | `sqlite-graphrag list --type user --json` | community project |
| Paperclip | AI Agent | any recent | `sqlite-graphrag read --name note --json` | community project |
| VS Code Copilot | AI Agent | 1.90+ | tasks.json | https://code.visualstudio.com/docs/copilot |
| Google Antigravity | AI Agent | any recent | `sqlite-graphrag hybrid-search "prompt" --json` | Google Antigravity docs |
| Windsurf | AI Agent | any recent | `sqlite-graphrag recall "refactor plan" --json` | https://windsurf.com/docs |
| Cursor | AI Agent | 0.40+ | `sqlite-graphrag remember --name cursor-ctx --type project --body "..."` | https://cursor.com/docs |
| Zed | AI Agent | any recent | `sqlite-graphrag recall "open tabs" --json` | https://zed.dev/docs |
| Aider | AI Agent | 0.60+ | `sqlite-graphrag recall "refactor" --k 5 --json` | https://aider.chat |
| Jules | AI Agent | preview | `sqlite-graphrag stats --json` | https://jules.google |
| Kilo Code | AI Agent | any recent | `sqlite-graphrag recall "tasks" --json` | community project |
| Roo Code | AI Agent | any recent | `sqlite-graphrag hybrid-search "repo ctx" --json` | community project |
| Cline | AI Agent | VS Code ext | `sqlite-graphrag list --limit 20 --json` | https://cline.bot |
| Continue | AI Agent | VS Code or JetBrains | `sqlite-graphrag recall "docstring" --json` | https://docs.continue.dev |
| Factory | AI Agent | any recent | `sqlite-graphrag recall "pr context" --json` | https://factory.ai |
| Augment Code | AI Agent | any recent | `sqlite-graphrag hybrid-search "review" --json` | https://docs.augmentcode.com |
| JetBrains AI Assistant | AI Agent | 2024.2+ | `sqlite-graphrag recall "stacktrace" --json` | https://www.jetbrains.com/ai |
| OpenRouter | AI Router | any | `sqlite-graphrag recall "rule" --json` | https://openrouter.ai/docs |
| POSIX Shells | Shell | any | `sqlite-graphrag recall "$query" --json` | https://www.gnu.org/software/bash |
| Nushell | Shell | 0.90+ | `^sqlite-graphrag recall "query" --k 5 --json \| from json \| get results` | https://www.nushell.sh/book |
| GitHub Actions | CI/CD | any | workflow YAML | https://docs.github.com/actions |
| GitLab CI | CI/CD | any | `.gitlab-ci.yml` | https://docs.gitlab.com/ee/ci |
| CircleCI | CI/CD | any | `.circleci/config.yml` | https://circleci.com/docs |
| Jenkins | CI/CD | 2.400+ | Jenkinsfile | https://www.jenkins.io/doc |
| Docker and Podman Alpine | Container | any | Dockerfile | https://docs.docker.com |
| Kubernetes | Orchestrator | 1.25+ | Job or CronJob | https://kubernetes.io/docs |
| Scoop and Chocolatey | Package Manager | Windows | `scoop install sqlite-graphrag` (planned) | https://scoop.sh and https://chocolatey.org |
| Nix and Flakes | Package Manager | any | `nix run .#sqlite-graphrag` | https://nixos.org |


## Claude Code
### Anthropic Agent — Subprocess Integration
- Recipe ready to copy into `.claude/hooks/`, zero cloud cost, memory stays on your machine
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to persist context across Claude Code sessions without external memory services
- Use `sqlite-graphrag recall "$USER_PROMPT" --k 5 --json` in a pre-task hook to inject context
- Minimum version requires Claude Code 1.0 or later for stable `.claude/hooks/` directory support
- Official docs live at https://docs.anthropic.com/claude-code describing hook lifecycle events
- Golden tip is to capture exit code `75` as retry-later and keep the agent alive gracefully
- Since v1.0.61, `ingest --mode claude-code` uses the Claude Code binary for LLM-curated entity/relationship extraction during bulk ingestion
- The ingest mode spawns `claude -p` headless per file — requires Claude Code >= 2.1.0 with active Pro/Max subscription
- Use `--claude-timeout <S>` (default 300s) to prevent hung subprocesses in CI/cron pipelines


## Codex CLI
### OpenAI Agent — AGENTS.md Driven Subprocess
- Recipe ready to paste into `AGENTS.md` at repo root, zero cloud cost to activate
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to expose the memory contract through the native `AGENTS.md` convention
- Use `sqlite-graphrag recall "<query>" --k 5 --json` documented inside `AGENTS.md` at repo root
- Minimum version requires Codex CLI 0.5 or later for deterministic AGENTS.md parsing rules
- Official docs live at https://github.com/openai/codex covering AGENTS.md discovery order
- Golden tip is to include a working invocation example under each listed command for Codex
- Since v1.0.62, `ingest --mode codex` uses the Codex CLI binary for LLM-curated entity/relationship extraction during bulk ingestion
- The ingest mode spawns `codex exec --json` headless per file — requires Codex CLI >= 0.120.0 with active OpenAI API key
- Use `--codex-timeout <S>` (default 300s) to prevent hung subprocesses in CI/cron pipelines

> **Authentication:** OAuth is the ONLY accepted credential flow. API keys are PROHIBITED.
> `--mode claude-code` reads OAuth from `~/.claude/.credentials.json` (Claude Pro/Max/Team).
> `--mode codex` reads device auth from `codex login` (OpenAI ChatGPT).
> Defining `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` in the environment ABORTS the spawn with `AppError::Validation` and exit code 1. The `--bare` flag (which would also demand an API key) is REMOVED from all executable code paths.
> See `docs/decisions/adr-0011-oauth-only-enforcement.md` for the full rationale.

## Gemini CLI
### Google Agent — Subprocess With JSON Contract
- Recipe ready to copy into your Gemini CLI config, zero cloud cost, runs fully local
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to inject memory into Gemini 2.5 Pro prompts during long coding sessions
- Use `sqlite-graphrag hybrid-search "query" --k 5 --json` for recall with mixed keyword intent
- Minimum version supports any recent Gemini CLI release with subprocess invocation enabled
- Official docs live at https://github.com/google-gemini/gemini-cli for tool integration patterns
- Golden tip is to set `SQLITE_GRAPHRAG_LANG=pt` when prompting Gemini in Portuguese contexts


## Opencode
### Community Agent — Subprocess Integration
- Recipe ready to copy into the Opencode plugin hook, zero cloud cost, runs as subprocess
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to persist multi-turn context in the open source Opencode orchestration loop
- Use `sqlite-graphrag recall "$query" --json` as part of the Opencode pre-generation pipeline
- Minimum version supports any recent Opencode release exposing a plugin subprocess hook
- Official project lives at https://github.com/opencode-ai/opencode with community issue tracker
- Golden tip is to set the namespace to the repo slug to avoid cross-project memory leakage


## OpenClaw
### Community Agent — Subprocess Driver
- Recipe ready to drop into OpenClaw startup, zero cloud cost, memory is fully local
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to inject persistent memory into OpenClaw agent loops without plugin rebuild
- Use `sqlite-graphrag list --type user --json` to fetch seed context at the start of a run
- Minimum version supports any recent OpenClaw release able to shell out to CLI binaries
- Official docs live inside the OpenClaw GitHub README explaining subprocess integration rules
- Golden tip is to run the binary inside the target project folder and keep the default `graphrag.sqlite`


## Paperclip
### Community Agent — Subprocess Client
- Recipe ready to paste into Paperclip hook config, zero cloud cost, all memory stays local
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to persist cross-session memory in the Paperclip autonomous developer agent
- Use `sqlite-graphrag read --name onboarding-note --json` to seed the session with prior notes
- Minimum version supports any recent Paperclip release that can spawn child subprocess calls
- Official docs live in the Paperclip community repository describing subprocess hook contracts
- Golden tip is to run `health --json` at startup and abort when integrity reports any damage


## VS Code Copilot
### Microsoft Agent — tasks.json Integration
- Recipe ready to paste into tasks.json, zero cloud cost, recall fires from inside the editor
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to surface relevant memory from a selection inside VS Code Copilot chat panels
- Use the example tasks.json entry that calls `sqlite-graphrag recall "$selection" --json`
- Minimum version requires VS Code 1.90 or later for the latest tasks.json variable substitutions
- Official docs live at https://code.visualstudio.com/docs/copilot covering chat tool registration
- Golden tip is to bind the task to `Cmd+Shift+M` for single-keystroke memory recall invocation


## Google Antigravity
### Google Agent — Runner Integration
- Recipe ready to register as an Antigravity runner, zero cloud cost, binary is self-contained
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to run sqlite-graphrag as a first-class runner inside Antigravity pipelines at scale
- Use `sqlite-graphrag hybrid-search "$PROMPT" --json --k 10` as the retrieval step in a runner
- Minimum version supports any recent Antigravity release that accepts arbitrary runner binaries
- Official docs live on the Google Antigravity product page describing runner configuration format
- Golden tip is to run `sync-safe-copy` before each pipeline to guard the shared memory artifact


## Windsurf
### Codeium Agent — Terminal Integration
- Recipe ready to paste into a Windsurf Run task binding, zero cloud cost to activate recall
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to expose memory recall to Windsurf assistant panels via terminal task invocation
- Use `sqlite-graphrag recall "$EDITOR_CONTEXT" --json` mapped to a Windsurf Run task binding
- Minimum version supports any recent Windsurf release with terminal task execution enabled
- Official docs live at https://windsurf.com/docs describing the terminal task binding syntax
- Golden tip is to persist results to `/tmp/ng.json` so Windsurf prompt templates can read them


## Cursor
### Cursor Agent — Terminal Integration
- Recipe ready to drop into `.cursorrules` or a terminal binding, zero cloud cost, memory is local
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to pair Cursor AI with a local memory backend that survives editor restarts
- Use `sqlite-graphrag remember --name cursor-ctx --type project --body "$SELECTION"` from a key binding
- Minimum version requires Cursor 0.40 or later for stable AI rules and terminal env override
- Official docs live at https://cursor.com/docs covering AI rules and terminal integration patterns
- Golden tip is to set `SQLITE_GRAPHRAG_NAMESPACE=${workspaceFolderBasename}` per project workspace


## Zed
### Zed Industries Agent — Assistant Panel Integration
- Recipe ready to add as a Zed task profile, zero cloud cost, runs from the built-in terminal
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to wire memory recall into the Zed assistant panel without custom extensions
- Use `sqlite-graphrag recall "open tabs" --json --k 5` as a terminal command available to Zed
- Minimum version supports any recent Zed release with the assistant panel and terminal tasks
- Official docs live at https://zed.dev/docs describing assistant panel and terminal integration
- Golden tip is to define a Zed task profile sharing memory across multiple open workspaces


## Aider
### Open Source Agent — Shell Integration
- Recipe ready to paste into your shell alias before `aider`, zero cloud cost, zero config server
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to augment Aider pair programming with durable memory across git repositories
- Use `sqlite-graphrag recall "refactor target" --k 5 --json` invoked before each Aider prompt
- Minimum version requires Aider 0.60 or later for stable subprocess and hook invocation
- Official docs live at https://aider.chat describing configuration and custom shell commands
- Golden tip is to scope memory by repository via `SQLITE_GRAPHRAG_NAMESPACE=$(basename $(pwd))`


## Jules
### Google Labs Agent — CI Automation
- Recipe ready to add as a Jules CI step, zero cloud cost, binary installs in seconds via cargo
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to run memory maintenance inside Jules preview automation pipelines automatically
- Use `sqlite-graphrag stats --json` as a CI step to monitor memory growth week over week
- Minimum version is the current Jules preview release available via Google Labs early access
- Official docs live at https://jules.google explaining CI job configuration and authentication
- Golden tip is to fail the pipeline when `stats.memories` exceeds agreed thresholds for a project


## Kilo Code
### Community Agent — Subprocess Integration
- Recipe ready to paste into Kilo Code startup hook, zero cloud cost, memory is a local file
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to expose a persistent memory layer to the Kilo Code autonomous engineering agent
- Use `sqlite-graphrag recall "recent tasks" --json` at the start of every Kilo Code agent run
- Minimum version supports any recent Kilo Code release capable of spawning child processes
- Official docs live in the Kilo Code community repository describing the subprocess contract
- Golden tip is to log exit code `75` as retryable rather than fatal when orchestrator is busy


## Roo Code
### Community Agent — Subprocess Integration
- Recipe ready to wire into Roo Code hook lifecycle, zero cloud cost, all data is local SQLite
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to inject memory into Roo Code agent prompts for deeper repository understanding
- Use `sqlite-graphrag hybrid-search "repo context" --json` for recall across mixed query types
- Minimum version supports any recent Roo Code release with hook capabilities for subprocess
- Official docs live in the Roo Code community repository explaining hook lifecycle conventions
- Golden tip is to chain `related <name> --hops 2` after recall for multi-hop graph expansion


## Cline
### Community VS Code Extension — Terminal Integration
- Recipe ready to register as a Cline terminal tool, zero cloud cost, memory persists locally
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to give Cline persistent memory across VS Code sessions without cloud services
- Use `sqlite-graphrag list --limit 20 --json` as a seed step at Cline conversation startup
- Minimum version supports the current Cline VS Code extension release in the marketplace
- Official docs live at https://cline.bot covering terminal tool registration and usage patterns
- Golden tip is to bind the command to a Cline tool with descriptive name and usage explanation


## Continue
### Open Source Agent — IDE Terminal Integration
- Recipe ready to paste into Continue custom commands config, zero cloud cost, no server needed
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to surface sqlite-graphrag memory inside Continue chat panels in VS Code or JetBrains
- Use `sqlite-graphrag recall "docstring" --json` from a Continue custom command registration
- Minimum version supports any recent Continue extension release in VS Code or JetBrains stores
- Official docs live at https://docs.continue.dev describing custom commands and tool integration
- Golden tip is to document each command in the Continue config so the embedded LLM picks it up


## Factory
### Factory Agent — API Or Subprocess
- Recipe ready to add to the Factory droid tool config, zero cloud cost, binary is self-contained
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to integrate sqlite-graphrag with Factory autonomous development droids in production
- Use `sqlite-graphrag recall "pr context" --json` during the Factory droid plan preparation phase
- Minimum version supports any recent Factory release with subprocess or API tool integration
- Official docs live at https://factory.ai explaining droid tool configuration and plan execution
- Golden tip is to set a long `--wait-lock` for Factory droids running under heavy concurrency


## Augment Code
### Augment Agent — IDE Integration
- Recipe ready to wire into Augment IDE tool registration, zero cloud cost, runs as subprocess
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to feed Augment Code review agents with persistent cross-repository memory state
- Use `sqlite-graphrag hybrid-search "code review" --json` inside Augment IDE review preparation
- Minimum version supports any recent Augment Code release with terminal and subprocess hooks
- Official docs live at https://docs.augmentcode.com describing tool registration and agents
- Golden tip is to enable `--lang en` explicitly for consistent review language across teams


## JetBrains AI Assistant
### JetBrains Agent — IDE Integration
- Recipe ready to register as a JetBrains external tool, zero cloud cost, recall takes milliseconds
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to add sqlite-graphrag memory to JetBrains AI Assistant across IntelliJ PyCharm WebStorm
- Use `sqlite-graphrag recall "$SELECTION" --json` registered as a JetBrains external tool runner
- Minimum version requires JetBrains AI Assistant 2024.2 or later for modern tool registration
- Official docs live at https://www.jetbrains.com/ai explaining tool and external runner registration
- Golden tip is to bind the tool to a keyboard shortcut to invoke recall with one hand on keyboard


## OpenRouter
### Multi-LLM Router — Any Version Supported
- Recipe ready to add as a preamble to any OpenRouter pipeline, zero cloud cost, memory stays local
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to share a common memory backend across every OpenRouter-hosted LLM in a pipeline
- Use `sqlite-graphrag recall "routing rule" --json` as a preamble step before any routed request
- Minimum version supports any OpenRouter API release since memory remains local and independent
- Official docs live at https://openrouter.ai/docs explaining routing rules and API integration
- Golden tip is to reuse the same namespace across all routed models for consistent context


## Minimax (since v1.0.83 — ADR-0041)
### Anthropic-Compatible Provider — MiniMax/api.minimax.io
- Recipe ready to route Claude Code through any Anthropic-compatible endpoint without breaking the OAuth-only mandate
- While the OAuth-only guard still rejects `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` with exit 1 (defence in depth from v1.0.69), the new whitelist preserves `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `CODEX_ACCESS_TOKEN`, `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, and `OTEL_EXPORTER_OTLP_ENDPOINT`
- Purpose is to enable Anthropic-compatible providers (MiniMax/api.minimax.io, OpenRouter, AWS Bedrock custom routes, corporate gateways) without forcing operators to pay the official Anthropic API key path
- Use the env vars below before invoking any `sqlite-graphrag` command that triggers embedding (`remember`, `edit`, `ingest --mode claude-code`)
- Minimum version requires `sqlite-graphrag` 1.0.83 or later; older releases will spawn the subprocess without the custom-provider env vars and the provider will return `401 Invalid authentication credentials`
- Official docs live at https://platform.minimax.io/document and `docs/decisions/adr-0041-preserve-custom-provider-env.md` explains the architectural rationale
- Golden tip is to verify the provider reachability with `curl -fsS "$ANTHROPIC_BASE_URL/v1/models" -H "Authorization: Bearer $ANTHROPIC_AUTH_TOKEN"` before running any `sqlite-graphrag` command

### Configuration Block
```bash
# Configure once per shell session before invoking sqlite-graphrag
export ANTHROPIC_AUTH_TOKEN="sk-cp-your-provider-token"
export ANTHROPIC_BASE_URL="https://api.minimax.io/anthropic"
# Optional: opt out of subprocess telemetry forwarding
export DISABLE_TELEMETRY="1"
# Optional: route OpenTelemetry to a local collector instead of provider default
export OTEL_EXPORTER_OTLP_ENDPOINT="http://localhost:4317"
```

### Smoke Test
```bash
# 1. Verify the provider returns models for the configured token
curl -fsS "$ANTHROPIC_BASE_URL/v1/models" \
  -H "Authorization: Bearer $ANTHROPIC_AUTH_TOKEN" \
  | head -c 200 && echo

# 2. Persist a smoke-test memory through the custom provider
sqlite-graphrag remember \
  --name smoke-test-minimax-v183 \
  --type note \
  --description "validacao do provider customizado via v1.0.83" \
  --body "smoke test executado em $(date -u +%FT%TZ)" \
  --graph-stdin <<'EOF'
{
  "body": "smoke test executado em $(date -u +%FT%TZ)",
  "entities": [
    {"name": "minimax", "entity_type": "tool", "description": "Anthropic-compatible provider"}
  ],
  "relationships": []
}
EOF

# 3. Confirm the embedding landed in memory_embeddings (not NULL)
sqlite-graphrag read --name smoke-test-minimax-v183 --json | jaq '{name, memory_id, has_embedding: (.body | length > 0)}'

# 4. Run a recall to verify the embedding participates in vector search
sqlite-graphrag recall "validacao do provider customizado" --k 3 --json | jaq '.results[] | {name, score}'
```

### Troubleshooting 401 Invalid Authentication Credentials
- **Symptom**: `sqlite-graphrag remember` returns exit 11 with `claude exited with exit status: 1: stderr=` (or `codex` equivalent)
- **Cause**: the `ANTHROPIC_AUTH_TOKEN` or `ANTHROPIC_BASE_URL` env vars did NOT reach the subprocess (older sqlite-graphrag, or strict mode, or shell wrapping that strips env)
- **Resolution paths**:
  - Confirm `sqlite-graphrag --version` reports `1.0.83` or later
  - Confirm the env vars are exported in the SAME shell where the command runs (not a parent shell, not a `.envrc` consumed only by direnv)
  - Run with `env | rg "ANTHROPIC_(AUTH_TOKEN|BASE_URL)"` to confirm presence
  - If the host enforces env-var isolation, drop the strict mode override: `unset SQLITE_GRAPHRAG_STRICT_ENV_CLEAR` or remove `--strict-env-clear`
  - Capture the exact error with `RUST_LOG=trace sqlite-graphrag remember ... 2> trace.log` and grep for `apply_env_whitelist`
- **Defense-in-depth confirmation**: the OAuth-only guard still rejects `ANTHROPIC_API_KEY` if accidentally set; verify with `export ANTHROPIC_API_KEY=sk-ant-test && sqlite-graphrag remember --name test --body x` returning exit 1
## POSIX Shells
### Bash Zsh Fish PowerShell — Any Version
- Recipe ready to paste into any shell alias or script, zero cloud cost, pipes work out of the box
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess by default with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to compose sqlite-graphrag with classic Unix and Windows shell pipelines seamlessly
- Use `sqlite-graphrag recall "$query" --json | jaq '.hits[].name'` in any POSIX-compatible shell
- Minimum version supports any recent Bash Zsh Fish or PowerShell 7 release
- Official docs live at https://www.gnu.org/software/bash and respective shell project homepages
- Golden tip is to quote variables explicitly to avoid word splitting in queries with spaces


## Nushell
### Nushell — Structured Data Pipeline Integration
- Recipe ready to paste into a Nushell script, zero cloud cost, output becomes native Nu table
- While MCPs require a dedicated server, sqlite-graphrag runs as a subprocess via `^` sigil in Nu
- Purpose is to compose sqlite-graphrag output with Nushell structured data pipelines natively
- Use `^sqlite-graphrag recall "query" --k 5 --json | from json | get results` to query memory
- Minimum version supports Nushell 0.90 or later for stable external command and `from json` pipeline
- Official docs live at https://www.nushell.sh/book describing external commands and JSON parsing
- Golden tip is to pipe results into `select name score` to display a ranked memory table in Nu


## GitHub Actions
### CI/CD — Any Recent Runner Image
- Recipe ready to copy into `.github/workflows/`, zero cloud cost, runs on any GitHub runner image
- While MCPs require a dedicated server, sqlite-graphrag installs in seconds via cargo on any runner
- Purpose is to run memory maintenance and backups inside scheduled GitHub Actions workflows
- Use a scheduled cron workflow that runs `sqlite-graphrag purge --days 30 --yes` and `vacuum`
- Minimum version works on any `ubuntu-latest`, `macos-latest` or `windows-latest` GitHub runner
- Official docs live at https://docs.github.com/actions describing scheduled workflows syntax
- Golden tip is to upload the sync-safe-copy output as a build artifact for rollback capability


## GitLab CI
### CI/CD — Any Recent Runner
- Recipe ready to copy into `.gitlab-ci.yml`, zero cloud cost, runs on any GitLab runner image
- While MCPs require a dedicated server, sqlite-graphrag installs in seconds via cargo on any runner
- Purpose is to run sqlite-graphrag maintenance inside GitLab CI scheduled pipelines routinely
- Use a scheduled `.gitlab-ci.yml` stage invoking `cargo install --path .` first
- Minimum version supports any recent GitLab runner image with Rust toolchain available for install
- Official docs live at https://docs.gitlab.com/ee/ci describing scheduled pipelines configuration
- Golden tip is to cache the cargo install directory between runs for faster job startup times


## CircleCI
### CI/CD — Any Recent Executor
- Recipe ready to copy into CircleCI config, zero cloud cost, binary installs via cargo in seconds
- While MCPs require a dedicated server, sqlite-graphrag installs in seconds via cargo on any executor
- Purpose is to run sqlite-graphrag maintenance and backups inside CircleCI scheduled workflows
- Use a scheduled workflow with `cargo install --path .` followed by the job steps
- Minimum version supports any recent CircleCI Linux or macOS executor with Rust toolchain
- Official docs live at https://circleci.com/docs describing scheduled pipelines and workflows
- Golden tip is to persist the DB to workspace storage so downstream jobs can audit the snapshot


## Jenkins
### CI/CD — Jenkins 2.400+
- Recipe ready to paste into a Jenkinsfile stage, zero cloud cost, works in air-gapped environments
- While MCPs require a dedicated server, sqlite-graphrag installs via cargo and runs as a one-shot subprocess with no daemon to manage (the daemon was removed in v1.0.79)
- Purpose is to integrate sqlite-graphrag backups into self-hosted Jenkins pipelines for regulated environments
- Use a Jenkinsfile stage running `cargo install --path .` and the operational commands
- Minimum version requires Jenkins 2.400 or later for stable pipeline and agent management features
- Official docs live at https://www.jenkins.io/doc covering declarative pipeline syntax in depth
- Golden tip is to archive the sync-safe-copy output as a build artifact for long-term retention


## Docker and Podman Alpine
### Container — Any Recent Version
- Recipe ready to copy into a Dockerfile, zero cloud cost, final image fits under 25 MB Alpine
- While MCPs require a dedicated server, sqlite-graphrag is a single static binary with no runtime deps
- Purpose is to package sqlite-graphrag in minimal Alpine images for reproducible production deployments
- Use a multi-stage Dockerfile with a Rust builder stage and an Alpine runtime copying the binary
- Minimum version supports any Docker or Podman release compatible with multi-stage build syntax
- Official docs live at https://docs.docker.com covering multi-stage build and image minimization
- Golden tip is to mount the SQLite file as a named volume to persist memory across container restarts


## Kubernetes Jobs And CronJobs
### Kubernetes — 1.25+
- Recipe ready to copy into a CronJob manifest, zero cloud cost, runs inside your existing cluster
- While MCPs require a dedicated server, sqlite-graphrag runs as a one-shot Job with no sidecar needed
- Purpose is to run sqlite-graphrag maintenance as Kubernetes CronJobs inside managed production clusters
- Use a CronJob manifest referencing the Alpine image and invoking purge plus vacuum on schedule
- Minimum version requires Kubernetes 1.25 or later for stable CronJob and concurrency policy support
- Official docs live at https://kubernetes.io/docs describing Job CronJob and PersistentVolumeClaim
- Golden tip is to mount the DB from a PVC with access mode `ReadWriteOnce` for data safety


## Scoop And Chocolatey
### Package Manager — Windows
- Recipe ready to run once the manifest lands, zero cloud cost, installs the same binary as cargo
- While MCPs require a dedicated server, sqlite-graphrag is a single exe with no runtime dependency
- Purpose is to install sqlite-graphrag on Windows with Scoop or Chocolatey familiar to Windows developers
- Use `scoop install sqlite-graphrag` or `choco install sqlite-graphrag` once official manifests land
- Minimum version supports any Scoop 0.3 or Chocolatey 2.0 release with modern manifest features
- Official docs live at https://scoop.sh and https://chocolatey.org explaining manifest conventions
- Golden tip is to run the binary inside the target project folder so it creates `graphrag.sqlite` there


## Nix And Flakes
### Package Manager — Any Nix Version
- Recipe ready to add as a flake input, zero cloud cost, binary hash is pinned for reproducibility
- While MCPs require a dedicated server, sqlite-graphrag runs as a pure binary in any Nix dev shell
- Purpose is to install sqlite-graphrag in reproducible Nix environments including NixOS and dev shells
- Use `nix run github:daniloaguiarbr/sqlite-graphrag#sqlite-graphrag` to execute without installation
- Minimum version requires Nix 2.4 or later with Flakes feature enabled in user configuration
- Official docs live at https://nixos.org describing Flakes enablement and usage from command line
- Golden tip is to pin the flake input hash so the binary stays reproducible across every rebuild
