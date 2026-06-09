---
name: sqlite-graphrag
description: Use this skill WHENEVER the user asks about adding persistent memory or GraphRAG or long-term context to Claude Code Codex Cursor Windsurf or any AI coding agent. MUST trigger for queries mentioning remember this, save conversation, retrieve previous context, hybrid search, entity graph, SQLite memory, local RAG, LLM-only embedding, OAuth flow, BLOB-backed embedding, memory migration v1.0.76, migrate to-llm-only, migrate rehash, vec tables drop, codex-spawn helper, vec orphan handling, or any G28-G39 gap remediation. Auto-invokes even without explicit mention when user describes agent losing context between sessions or wants an offline-first local memory layer in Rust. MUST also trigger on OAuth-only enforcement, ANTHROPIC_API_KEY or OPENAI_API_KEY abort, 7 hardening flags for Claude and Codex, Mock LLM CLI in CI, Mock LLM CLI in CI, or removal of the daemon subcommand. Keywords memory RAG GraphRAG SQLite LLM-only one-shot OAuth Claude Codex Cursor Windsurf offline local persistent graph entity v1.0.76.
---


## Fundamental Principles

- Read this document in [Portuguese (pt-BR)](../sqlite-graphrag-pt/SKILL.md).
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
- SINCE v1.0.76, `init` validates that an LLM CLI (`claude` or `codex`) is reachable on PATH; there is no local model download
- VALIDATE with `sqlite-graphrag health --json` before operating
- TREAT exit code 10 as a database error or corrupted database
- TREAT exit code 15 as a pending lock; widen `--wait-lock`
- ABORT pipeline when `integrity_ok` returns `false`
- RUN `migrate --json` after each binary upgrade
### REQUIRED — Continuous Monitoring
- INSPECT `wal_size_mb` in `health` to detect fragmentation
- CHECK `journal_mode` equals `wal` in production
- RUN `optimize --json` to refresh planner statistics; response includes `fts_rebuilt` (bool) indicating whether the FTS5 index was also rebuilt
- USE `optimize --skip-fts --json` to skip the FTS5 rebuild step (faster, use when FTS5 was recently rebuilt)
- DETECT schema drift via `debug-schema` for troubleshooting
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
- ISOLATE projects via namespace per repository
- ADOPT `swarm-<agent_id>` for multi-agent swarms
- NOTE that `SQLITE_GRAPHRAG_NAMESPACE` is now respected by all commands (fixed in v1.0.51; previously 8 commands ignored it)
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
- The ONNX runtime (`libonnxruntime.so`) and `ORT_DYLIB_PATH` are NO LONGER needed in the default LLM-only build
- Embeddings are generated via headless `claude -p` or `codex exec` subprocess (OAuth)
- No local model download or ONNX runtime is needed for the default build


## CRUD — Create with remember
### REQUIRED — Writing Individual Memories
- USE a unique kebab-case name per memory
- DECLARE `--type` from `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`; `--type` and `--description` are OPTIONAL when `--force-merge` is used (inherited from existing memory)
- PREFER `--body-stdin` for long bodies
- USE `--body-file <PATH>` to avoid shell escaping in Markdown
- PASS `--force-merge` in idempotent loops; also restores soft-deleted memories and updates them in one step (since v1.0.51)
- USE `--dry-run` to validate inputs without persisting or running embeddings
- USE `--clear-body` to explicitly clear the body of an existing memory when using `--force-merge`; without `--clear-body`, `--force-merge` with an empty body PRESERVES the existing body
- NER is disabled by default; pass `--enable-ner` or set `SQLITE_GRAPHRAG_ENABLE_NER=1` to activate GLiNER extraction
- Response field `extraction_method` reports: `gliner-<variant>+regex`, `regex-only`, or `none:extraction-failed`
- `--skip-extraction` is deprecated since v1.0.45 and has no effect; use `--enable-ner` to activate NER
- RESPECT the limit of 512000 bytes and 512 chunks per body
- USE `--max-rss-mb <MiB>` to abort embedding if process RSS exceeds the threshold (default 8192 MiB); lower this in memory-constrained environments
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
- NEVER rely on GLiNER auto-extraction in RAM-sensitive CI
- NEVER exceed the relations cap per memory without adjusting env
- NEVER use `remember` in a loop when `ingest` covers the case
- NEVER pass empty body with no entities via `--graph-stdin`; since v1.0.54 this returns exit 1 (Validation) instead of silently creating an inert memory with zero chunks
### Correct Pattern — remember Examples
- `sqlite-graphrag remember --name design-auth --type decision --description "auth JWT" --body-stdin < doc.md`
- `sqlite-graphrag remember --name doc-readme --type document --description "import" --body-file README.md --force-merge`
- `sqlite-graphrag remember --name spec-x --type reference --description "spec" --body "..." --entities-file ents.json --relationships-file rels.json`
### Valid --type Values
- `user`, `feedback`, `project`, `reference`
- `decision`, `incident`, `skill`, `document`, `note`


## CRUD — Batch Create with remember-batch (v1.0.67)
### REQUIRED — NDJSON Batch Memory Creation
- USE `remember-batch` for creating multiple memories in a single invocation via NDJSON stdin
- EACH input line is a JSON object with `name`, `type`, `description`, `body` fields
- OUTPUT is NDJSON: one event per item plus a summary line
- USE `--force-merge` to update existing memories in the batch
- USE `--dry-run` to validate the batch without persisting
- PREFER over looping `remember` for 10+ memories — reduces overhead from repeated model loading
- Per-item event: `name`, `status` (`"created"`/`"updated"`/`"skipped"`/`"failed"`), `memory_id?`, `error?`, `elapsed_ms`
- Summary line: `summary` (true), `total`, `created`, `updated`, `skipped`, `failed`, `elapsed_ms`
### Correct Pattern — remember-batch Examples
- `echo '{"name":"a","type":"note","description":"x","body":"hello"}' | sqlite-graphrag remember-batch --json`
- `cat batch.ndjson | sqlite-graphrag remember-batch --force-merge --json`


## New in v1.0.68
### REQUIRED — Process Lifecycle Governance (G28-B)
- KNOW that `enrich`, `ingest --mode claude-code`, and `ingest --mode codex` acquire a per-namespace singleton via `lock::acquire_job_singleton(job_type, namespace, wait_seconds)` before any work
- TREAT `AppError::JobSingletonLocked { job_type, namespace }` (exit 75, retryable) as a signal that another invocation is in progress on the same database
- DO NOT parallelise these commands against the same namespace — use the queue DB with `--resume` or sequence them
- KNOW that the previous design (semaphore shared with all CLI commands) allowed 4 concurrent `enrich` invocations × 2 workers × 10 MCP servers = ~192 processes, which is the root cause of the 2026-06-03 276-load-average incident
### REQUIRED — MCP Isolation via env var (G28-A)
- SET `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR=/path/to/empty/dir` to suppress user-scoped MCP servers in `claude -p` subprocesses
- KNOW that the empty directory MUST exist but contain no files; the CLI sets `CLAUDE_CONFIG_DIR=<that dir>` on the subprocess
- KNOW that the empty dir is the ONLY mechanism upstream Claude Code actually honours — [anthropics/claude-code#10787] documents that `--strict-mcp-config` and `--mcp-config '{}'` are silently ignored
- EXPECT a `tracing::warn!` when `--llm-parallelism > 4`, recommending the combination with `CLAUDE_CONFIG_DIR` override
### REQUIRED — Circuit Breaker Helper (G28-D)
- USE `retry::CircuitBreaker::new(threshold, cooldown)` to cap persistent-failure retry loops in custom code
- KNOW that `AttemptOutcome::Transient` (from `AppError::RateLimited` or `AppError::Timeout`) does NOT count toward the failure threshold
- KNOW that `AttemptOutcome::HardFailure` (from `AppError::Validation` or `AppError::Conflict`) counts; after `threshold` consecutive hits, `record()` returns `true` and the caller should abort
- CALL `cb.reset()` when starting a new job to clear the consecutive-failure counter
### REQUIRED — Windows HANDLE Type Safety (G29)
- KNOW that v1.0.68 is the first release since v1.0.65 that compiles on Windows via `cargo install`
- KNOW that `windows-sys >= 0.59` defines `HANDLE` as `*mut c_void` (was `isize` in 0.48/0.52); `Cargo.toml:111` pins `=0.59.0` exact
- EXPECT a `windows-build-check` CI job to run `cargo check --target x86_64-pc-windows-msvc --lib --all-features` on every push
- IF a user reports a Windows compile failure, redirect them to upgrade to v1.0.68 or apply the manual patch documented in `docs/CROSS_PLATFORM.md`
### REQUIRED — Test Fixes (Timezone Leak)
- KNOW that 3 pre-existing test failures in `src/commands/{history,list,read}.rs` are fixed in v1.0.68
- KNOW that the tests previously leaked `SQLITE_GRAPHRAG_DISPLAY_TZ` between parallel test threads and asserted hardcoded `1970-01-01T00:00:00` strings
- EXPECT the tests to now parse the ISO string via `chrono::DateTime::parse_from_rfc3339` and compare `timestamp()` against `DateTime::UNIX_EPOCH` for timezone-agnostic assertions
- TRUST that `cargo test --lib` is green on all timezones (`UTC`, `America/Sao_Paulo`, `Europe/Berlin`, etc.) since v1.0.68
### FORBIDDEN — Process Lifecycle Anti-patterns (G28)
- NEVER run multiple `enrich` invocations on the same database concurrently — they will saturate the host
- NEVER pass `--strict-mcp-config` or `--mcp-config '{}'` to Claude Code CLI — it ignores both (issue #10787)
- NEVER bypass the singleton via direct file manipulation of `~/.local/share/sqlite-graphrag/job-singleton-*.lock`
- NEVER assume that `enrich` running for 30 minutes means it's stuck — long enrichments are normal


## New in v1.0.69
### REQUIRED — OAuth-Only Enforcement (CRITICAL behaviour change)
- KNOW that v1.0.69 is the first release where OAuth is the ONLY accepted credential flow
- KNOW that `claude_runner::build_claude_command` ALWAYS passes 7 hardening flags: `--strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions --output-schema` plus 2 from `codex_spawn::build_codex_command` (G28-A, G31)
- KNOW that spawn ABORTS with `AppError::Validation` (exit 1) if `ANTHROPIC_API_KEY` is set in the environment
- KNOW that spawn ABORTS with `AppError::Validation` (exit 1) if `OPENAI_API_KEY` is set in the environment
- KNOW that the `--bare` flag (which would demand an API key) is REMOVED from all executable code paths; it appears only in documentation explaining why it is forbidden
- KNOW that `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` are EXCLUDED from the env-clear whitelist (defense in depth)
- KNOW that 4 `#[serial_test::serial(env)]` tests in `claude_runner.rs` and 4 in `codex_spawn.rs` validate the canonical flag set and the abort behaviour
- REFERENCE `docs/decisions/adr-0011-oauth-only-enforcement.md` for the full rationale
- OPERATORS using API keys MUST migrate to OAuth (Claude Pro/Max or OpenAI ChatGPT Pro) before upgrading
### REQUIRED — Orphan Reaper (G28-C)
- KNOW that `src/reaper.rs::scan_and_kill_orphans()` walks `/proc` at startup BEFORE any work
- KNOW that the reaper kills any `claude` or `codex` orphan with `PPID=1` and age > 60 seconds
- KNOW that `ORPHAN_MIN_AGE_SECS=60` and `ORPHAN_SCAN_TARGETS=["claude", "codex"]` are the constants
- TRUST that the 4-test reaper suite runs in <30s on Linux (`orphan_min_age_is_one_minute`, `orphan_targets_include_claude_and_codex`, `reaper_report_starts_zeroed`, `scan_completes_without_panic_on_linux`)
- The reaper is called from `main.rs` startup, BEFORE the CLI dispatches to any subcommand
### REQUIRED — System Load and Circuit Breaker (G28-D)
- KNOW that `src/system_load.rs` exposes `load_average_one()`, `ncpus()`, and `is_system_saturated(threshold)`
- KNOW that `is_system_saturated` defaults to threshold `2.0 × ncpus`
- USE `load_average_one()` to decide whether to enqueue a new enrich or wait — load is Mutex-cached with 1s throttle to avoid hammering `/proc/loadavg`
- KNOW that `retry::CircuitBreaker::new(threshold, cooldown)` caps persistent-failure retry loops
- KNOW that `AttemptOutcome::Transient` (rate limit, timeout) does NOT count toward the failure threshold
- KNOW that `AttemptOutcome::HardFailure` (validation, conflict) counts; after `threshold` consecutive hits, `record()` returns `true` and the caller aborts
- CALL `cb.reset()` when starting a new job to clear the consecutive-failure counter
### REQUIRED — MemorySource enum and Source Validation (G29)
- KNOW that `src/memory_source.rs` defines a type-safe enum with 5 values: `agent`, `user`, `system`, `import`, `sync`
- KNOW that `MemorySource::TryFrom(&str)` returns `AppError::Validation` listing the accepted values
- KNOW that `validate_source()` is the runtime guard called in `storage/memories.rs::insert` and `update`
- KNOW that 8 unit tests cover valid/invalid/empty/display/serialisation paths
- REFERENCE `docs/decisions/adr-0012-memory-source-enum.md` for the migration plan
### REQUIRED — Preservation Gate and Idempotency (G29)
- KNOW that `src/preservation.rs` defines `jaccard_similarity(a: &str, b: &str) -> f64` (trigram-based, UTF-8 safe via `char_indices`)
- KNOW that `PreservationVerdict` enum has `Preserved { score, threshold }`, `Rejected { score, threshold }`, and `Unchanged { byte_len }` variants
- KNOW that the default preservation threshold is `0.7` and is enforced on every `enrich --operation body-enrich`
- KNOW that blake3-based idempotency skip compares the old and new body hashes BEFORE the Jaccard check
- KNOW that 10 unit tests cover Jaccard edge cases (empty, single char, identical, threshold boundary, Unicode)
- REFERENCE `docs/decisions/adr-0015-preservation-gate.md`
### REQUIRED — Scripts Deprecation (G29 Passo 6)
- KNOW that `scripts/legacy/` directory contains the deprecated Python workaround `expand-curtas.py` plus a README.md explaining why it was retired
- KNOW that `scripts/legacy/` is added to `.gitignore` to prevent CI from re-running it
- USE `enrich --operation body-enrich` directly instead of the Python wrapper
### REQUIRED — Singleton Lock Scoped by db_hash (G30)
- KNOW that `lock::acquire_job_singleton` signature gains `db_path: &Path` and `force: bool` parameters
- KNOW that the lock file name is now `job-singleton-{tag}-{namespace_slug}-{db_hash}.lock`
- KNOW that the `db_hash` is the first 12 hex chars of `blake3(canonicalize(db_path))`
- KNOW that `lock::db_path_hash` is `pub` so callers can compute the hash without acquiring the lock
- USE new flags `--wait-job-singleton <SECONDS>` (poll for the lock) and `--force-job-singleton` (break a stale lock)
- Two concurrent `enrich` invocations against DIFFERENT databases no longer collide; the same database still serialises
- The error message that previously referenced a non-existent `--wait-job-singleton` flag is now actionable
- REFERENCE `docs/decisions/adr-0013-singleton-scoped-by-db-hash.md`
### REQUIRED — Unified codex_spawn Helper (G31+G32+G33)
- KNOW that `src/commands/codex_spawn.rs` (~700 lines, 11 tests) unifies the spawn pipeline, JSONL parser, and ChatGPT Pro OAuth model validation
- KNOW that BOTH `enrich --mode codex` and `ingest --mode codex` consume the same canonical command (was divergent, motivated the `~/.local/bin/codex-clean` wrapper)
- KNOW that the 7 hardening flags are: `--json --output-schema --ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules` PLUS `-c mcp_servers='{}' --ask-for-approval never`
- KNOW that `parse_codex_jsonl` iterates `for line in stdout.lines()` and picks the last `item.completed` of type `agent_message`
- KNOW that `validate_codex_model` checks `--codex-model` against the ChatGPT Pro OAuth whitelist BEFORE the subprocess is spawned
- ACCEPT only these 5 models: `codex-auto-review`, `gpt-5.3-codex-spark`, `gpt-5.4`, `gpt-5.4-mini`, `gpt-5.5`
- DEFAULT `--codex-model` is `gpt-5.5`
- REFERENCE `docs/decisions/adr-0014-codex-spawn-helper.md`
### REQUIRED — Conditional LLM Parallelism Warning (G34)
- KNOW that the `llm_parallelism > 4` warning is now conditional to the spawn mode
- Claude mode warns at 5 (high severity)
- Codex 5..16 is silent (Codex does not spawn MCP children)
- Codex warns at 17 (medium severity)
- VALIDATED at 1161 items, 0 failures in production
### REQUIRED — Preflight Check and Fallback Mode (G35)
- USE `--preflight-check` on `enrich` to issue a 1-turn ping before scanning N candidates
- USE `--fallback-mode <codex|claude-code>` to switch mode automatically on rate limit
- USE `--rate-limit-buffer <SECONDS>` to reserve budget for graceful shutdown
- DEFAULT off to keep `--dry-run` and CI flows zero-cost
- On a Claude rate limit the preflight ABORTS with a clear error OR switches to `--fallback-mode`
### REQUIRED — Selective Enrichment (G37)
- USE `--names <NAME>` (repeatable) on `enrich` to select a specific subset of memory names
- USE `--names-file <PATH>` on `enrich` to read names from a file (accepts `#` comments and blank lines)
- COMBINE `--names` and `--names-file` as a union when both are set
- KNOW that `scan_unbound_memories(conn, namespace, limit, name_filter: &[String])` uses `WHERE m.name IN (?2, ?3, ...)` for safe parameterised query
### REQUIRED — FTS5 Hardening Flags (G36)
- USE `optimize --fts-dry-run` to preview what the FTS5 rebuild would do
- USE `optimize --fts-progress <N>` to print progress every N seconds
- USE `optimize --yes` to skip the interactive confirmation
- KNOW that `optimize` now pre-checks `fts check` and SKIPS the rebuild when the index passes integrity-check
- USE `optimize --no-fts-skip-when-functional` to force a rebuild even when FTS5 is healthy
- KNOW that `OptimizeResponse` exposes `fts_rebuilt`, `fts_skipped_functional`, `fts_unhealthy`, `fts_rows_indexed`
- KNOW that the FTS5 progress thread uses `crate::storage::connection::open_ro(&db_path)` in a SEPARATE thread (rusqlite::Connection is not Send)
- REFERENCE `docs/decisions/adr-0016-fts5-hardening-flags.md`
### REQUIRED — Backup 25x Speedup (G38)
- KNOW that the new defaults are `run_to_completion(1000, Duration::from_millis(5), None)` — 25x faster than the previous 100/50ms
- USE `--backup-step-size <N>` to tune the number of pages per step
- USE `--backup-step-sleep-ms <N>` to tune the sleep between steps
- USE `--backup-no-sleep` to disable inter-step sleep entirely (use with caution on SSDs)
- KNOW that `BackupResponse` adds `pages_copied` and `step_size` fields
- KNOW that the loop is MANUAL because `Backup::step()` returns `StepResult` which is `#[non_exhaustive]`
### REQUIRED — vec Subcommand Family (G39)
- USE `vec orphan-list --json` to list all orphaned memory vectors (no corresponding memory row)
- USE `vec purge-orphan --yes --dry-run` to PREVIEW purge without removing
- USE `vec purge-orphan --yes` to PERMANENTLY purge orphans from the 3 vec tables (`vec_memories`, `vec_entities`, `vec_chunks`)
- USE `vec stats --json` to inspect vec table health (row counts per table, orphan ratio, last vacuum timestamp)
- KNOW that `forget` now calls `delete_vec` BEFORE `soft_delete` to prevent creating new vec orphans
- KNOW that the 3-test suite covers orphan-list, purge-orphan, and stats (all use in-memory SQLite for isolation)
- REFERENCE `docs/decisions/adr-0017-vec-orphan-handling.md`
### REQUIRED — 4 New JSON Schemas (v1.0.69)
- KNOW that 4 new schemas were added to `docs/schemas/`:
  - `vec-orphan-list.schema.json` — list of orphaned memory vectors
  - `vec-purge-orphan.schema.json` — purge response
  - `vec-stats.schema.json` — vec table health statistics
  - `codex-models.schema.json` — ChatGPT Pro OAuth model whitelist response
- ALL follow the project convention `"additionalProperties": false`
- INDEXED in `docs/schemas/README.md` (which has its own v1.0.69 entry pointing back to G33 + G39)
### REQUIRED — 8 New ADRs (v1.0.69)
- KNOW that 8 new Architecture Decision Records live in `docs/decisions/`:
  - `adr-0011-oauth-only-enforcement.md` — full rationale for the OAuth-only mandate
  - `adr-0012-memory-source-enum.md` — type-safe enum migration plan
  - `adr-0013-singleton-scoped-by-db-hash.md` — BLAKE3 hashing of the database path
  - `adr-0014-codex-spawn-helper.md` — DRY refactor of codex spawn pipeline
  - `adr-0015-preservation-gate.md` — Jaccard preservation + blake3 idempotency
  - `adr-0016-fts5-hardening-flags.md` — FTS5 dry-run, progress, and thread separation
  - `adr-0017-vec-orphan-handling.md` — vec subcommand family + forget hook
  - `adr-0018-v1-0-69-status.md` — executive status of gap closure
### REQUIRED — Test Suite Growth
- KNOW that v1.0.69 adds 53 tests to the suite (692 → 745)
- KNOW that 0 tests fail and 3 are ignored
- KNOW that 8 ADRs document the architectural decisions behind the 53 new tests
- KNOW that 4 of the new tests are `#[serial_test::serial(env)]` to validate OAuth-only env var enforcement
### FORBIDDEN — v1.0.69 Anti-patterns
- NEVER pass `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` in the environment — the spawn will ABORT
- NEVER use `--bare` flag — it has been REMOVED from all executable code paths
- NEVER pass `gpt-4*`, `o4-mini`, or `gpt-5-codex` as `--codex-model` — these are rejected by ChatGPT Pro OAuth
- NEVER run `enrich` in parallel against the same database even with the new singleton — wait for the singleton or use `--wait-job-singleton`
- NEVER call `reaper::scan_and_kill_orphans()` from a child process — only the main process at startup
- NEVER pass `--llm-parallelism > 4` for Claude mode without combining with `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR`
- NEVER call `optimize` without checking `fts stats` first if you only want to verify health (use `fts check` instead)


## New in v1.0.76
### REQUIRED — LLM-Only and One-Shot Architecture (BREAKING)
- KNOW that v1.0.76 is the first release where the default build no longer bundles any local model
- KNOW that all embedding generation, NER, and vector search now delegate to headless `claude -p` or `codex exec` (OAuth, no MCP, no hooks)
- KNOW that the CLI is one-shot — there is no daemon, no ONNX runtime, no model download
- KNOW that the release binary is ~6 MB (down from 39 MB)
- KNOW that the `fastembed`, `ort`, `ndarray`, `tokenizers`, `huggingface-hub`, `sqlite-vec`, and `GLiNER` crates are REMOVED from the default build
- KNOW that the `daemon` subcommand was fully removed in v1.0.76 (ADR-0021)
- KNOW that migration V013 drops the `vec_memories` / `vec_entities` / `vec_chunks` virtual tables and creates BLOB-backed `memory_embeddings` / `entity_embeddings` / `chunk_embeddings` tables
- KNOW that cosine similarity is now computed in pure Rust on demand in `src/similarity.rs` (ADR-0020, ADR-0022)
- KNOW that the `llm-only` feature is the canonical marker for the v1.1.0 default flip

### REQUIRED — OAuth-Only LLM Embedding Flow
- KNOW that v1.0.76 inherits the OAuth-only mandate from v1.0.69 and applies it to the embedding pipeline
- KNOW that the LLM spawn ABORTS with `AppError::Validation` and exit code 1 if `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` is set in the environment
- KNOW that both API-key env vars are EXCLUDED from the env-clear whitelist in `claude_runner.rs`, `codex_spawn.rs`, and `ingest_claude.rs`
- KNOW that the `--bare` flag (which would also demand an API key) is REMOVED from every executable path
- KNOW that the OAuth flow (Claude Pro/Max or ChatGPT Pro subscription) is the ONLY accepted credential mechanism
- REFERENCE `docs/decisions/adr-0011-oauth-only-enforcement.md` and `docs/decisions/adr-0025-oauth-only-embedding.md`

### REQUIRED — Migrate Subcommands for v1.0.74 / v1.0.75 Databases
- USE `migrate --rehash --json` to rewrite recorded migration checksums via SipHasher13 to match the current file content
- USE `migrate --to-llm-only --drop-vec-tables --json` as the one-shot upgrade for v1.0.74 / v1.0.75 databases (rehash + V013 + drop vec tables)
- KNOW that `--drop-vec-tables` is the explicit safety guard — the CLI refuses to run without it
- KNOW that migration V002 was intentionally emptied to a no-op for v1.0.76, so `--rehash` is REQUIRED for v1.0.74 databases to upgrade cleanly
- REFERENCE `docs/MIGRATION.md` for the full v1.0.74 → v1.0.76 → v1.1.0 path and `docs/decisions/adr-0026-v002-vec-tables-migration-drift.md` for the V002 root cause
- SCHEMA: `migrate-rehash.schema.json` and `migrate-to-llm-only.schema.json` (both in `docs/schemas/`)

### REQUIRED — 3-Feature CI Matrix and Mock LLM CLI
- KNOW that the CI workflow runs `clippy` and `test` jobs with a stub `mock-llm` CLI on `PATH` so embedding round-trip tests run without real OAuth credentials
- KNOW that 26 test files were wired to consume the mock LLM CLI as a drop-in replacement for `claude -p` and `codex exec`
- KNOW that 107 of 115 previously-slow tests were fixed in commit `bd0a3f5` (mock LLM unblocks tests that depended on a real OAuth turn)
- KNOW that 11 new unit tests cover the migrate subcommand and 4 new integration tests cover the CLI subcommands end-to-end

### REQUIRED — 7 New ADRs (v1.0.76)
- KNOW that 7 new Architecture Decision Records were added (all with PT-BR translations):
  - `adr-0019-llm-only-one-shot.md` — rationale for removing fastembed, ort, ndarray, tokenizers, hf-hub, sqlite-vec
  - `adr-0020-pure-rust-cosine.md` — replacement for sqlite-vec KNN with pure-Rust cosine
  - `adr-0021-deprecate-daemon.md` — the daemon is no longer a performance optimization
  - `adr-0022-blob-embeddings.md` — migration V013 drops vec tables; BLOB-backed tables
  - `adr-0023-remove-tokenizers.md` — whitespace token heuristic replaces the tokenizers crate
  - `adr-0024-fts5-coarse-cosine-refine.md` — FTS5 coarse filter + cosine refinement
  - `adr-0025-oauth-only-embedding.md` — OAuth-only flow for the LLM embedding pipeline
- KNOW that `adr-0026-v002-vec-tables-migration-drift.md` documents the V002 mismatch root cause

### REQUIRED — 2 New JSON Schemas (v1.0.76)
- KNOW that `migrate-rehash.schema.json` defines the JSON contract for `migrate --rehash --json` (fields: `action`, `rewritten`, `skipped`, `errors`, `namespace`, `db_path`, `elapsed_ms`)
- KNOW that `migrate-to-llm-only.schema.json` defines the JSON contract for `migrate --to-llm-only --json` (fields: `action`, `rewritten`, `v013_applied`, `schema_version`, `vec_tables_were_present`, `vec_tables_dropped`, `embedding_tables_created`, `namespace`, `db_path`, `elapsed_ms`)
- Both schemas follow the project convention `"additionalProperties": false` and are indexed in `docs/schemas/README.md`

### REQUIRED — New Documentation (v1.0.76)
- KNOW that `docs/HOW_TO_USE.md` and `docs/HOW_TO_USE.pt-BR.md` were rewritten for v1.0.76 LLM-Only
- KNOW that `docs/MIGRATION.md` and `docs/MIGRATION.pt-BR.md` were created covering v1.0.74 → v1.0.76 → v1.1.0
- KNOW that `docs/HEADLESS_INVOCATION.md` and `docs/HEADLESS_INVOCATION.pt-BR.md` were created covering Claude/Codex/OpenCode OAuth-safe headless invocation
- KNOW that `docs/AGENTS.md` gained "v1.0.76 Architecture (LLM-Only)" and "OAuth Enforcement" sections
- KNOW that `docs/TESTING.md` gained "v1.0.76 Test Infrastructure — 3-Feature CI Matrix" section
- KNOW that `docs/COOKBOOK.md` gained "How To Upgrade From v1.0.74 Or v1.0.75 To v1.0.76" recipe

### FORBIDDEN — v1.0.76 Anti-patterns
- NEVER try to install `fastembed`, `tokenizers`, or `sqlite-vec` — these crates are removed from the default build
- NEVER use the v1.0.76 default build on a host without `claude` or `codex` CLI on `PATH` — the embedding pipeline requires it
- NEVER set `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` and expect a successful embedding call — the spawn ABORTS
- NEVER depend on the `daemon` subcommand for new code — it is removed in v1.1.0
- NEVER call `migrate --to-llm-only` without `--drop-vec-tables` — the CLI refuses to run for safety
- NEVER add new code that depends on the v1.0.74 ONNX model cache — the default build is LLM-only
- NEVER assume cosine similarity is computed by sqlite-vec — it is now pure-Rust on demand in `src/similarity.rs`


## CRUD — Bulk Ingest with ingest
### REQUIRED — When to Use ingest
- USE `ingest <DIR>` to import entire directories as memories
- PREFER over the `fd | xargs remember` loop in any case
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
- USE `--max-rss-mb <MiB>` to abort if process RSS exceeds the threshold during embedding (default 8192 MiB)
### REQUIRED — Two Parallelism Axes
- `--max-concurrency <N>` controls simultaneous CLI invocations
- `--ingest-parallelism <N>` controls extract plus embed in parallel
- DEFAULT for `--max-concurrency` is 4
- DEFAULT for `--ingest-parallelism` is `min(4, max(1, cpus/2))`
- DISTINGUISH the two axes clearly before adjusting
- WIDEN `--wait-lock <SECONDS>` to wait for a slot before exit 75
### REQUIRED — Performance and Extraction
- NER is disabled by default; pass `--enable-ner` to activate GLiNER extraction
- GLiNER NER adds approximately 100-200 ms per file with model loaded on modern hardware
- GLiNER NER adds 2 to 30 seconds per file in `--low-memory` or on first load
- GLiNER NER downloads the ONNX model on first run (fp32: 1.1 GB, int8: 349 MB via `--gliner-variant`)
- USE `--gliner-variant int8` for CI/containers to reduce model size from 1.1 GB to 349 MB
- USE `--enable-ner` only when automated entity enrichment is valuable
- Response field `extraction_method` reports: `gliner-<variant>+regex`, `regex-only`, or `none:extraction-failed`
- Ingest duplicates emit `status: "skipped"` with `action: "duplicate"` instead of `status: "failed"`
- PREFER `--graph-stdin` with LLM-curated entities for best quality (NER is off by default; `--skip-extraction` is deprecated since v1.0.45)
- USE `--dry-run` to preview file-to-name mapping without spawning LLM subprocess or persisting
- NDJSON per-file events include `original_filename` field preserving the file basename before kebab-case normalization
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
- Per-file line: `file`, `name`, `status` (`"indexed"` `"skipped"` `"failed"`), `truncated`, `original_name?`, `memory_id?`, `action?`, `error?`, `body_length?`
- Final summary line: `summary` (true), `dir`, `pattern`, `recursive`, `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- NER extraction events go to stderr, NOT stdout
- USE `--max-name-length N` to override the default 60-character truncation threshold for memory names
- NUMERIC basenames (e.g. `123.md`) are automatically prefixed with `doc-` to produce valid kebab-case names (e.g. `doc-123`)
### REQUIRED — Ingest Modes (v1.0.62)
- `--mode none` (default): body-only ingestion without entity/relationship extraction
- `--mode gliner`: GLiNER NER extraction (requires `--enable-ner`, uses local ONNX model)
- `--mode claude-code`: LLM-curated extraction via locally installed Claude Code CLI (`claude -p` headless)
- Claude Code mode spawns `claude -p` per file with `--json-schema` for guaranteed structured output
- Requires Claude Code >= 2.1.0 installed on user's machine with active Pro/Max subscription
- Extracts domain-specific entities and typed relationships constrained to canonical enums
- `--resume` continues interrupted ingest from queue DB; `--retry-failed` retries only failed files
- `--max-cost-usd <N>` stops when cumulative LLM cost exceeds the budget
- `--claude-binary <PATH>` overrides PATH lookup; `--claude-model <MODEL>` selects model
- --claude-timeout <S> sets per-file subprocess timeout (default 300s); kills hung processes
- Queue DB `.ingest-queue.sqlite` tracks per-file progress; `--keep-queue` retains after completion
- Rate limit handling: automatic exponential backoff (60s → 120s → 300s → 900s)
- `--dry-run` with `--mode claude-code` emits `status: "preview"` events without spawning Claude — zero tokens consumed
- Re-ingesting the same directory UPDATES existing memories (force-merge) instead of failing with UNIQUE constraint
- Cold-start `--json-schema` failure automatically retried once after 2s delay (workaround for Claude Code Issue #23265)
- Subprocess runs with `env_clear()` + selective injection for security hardening
- OAuth is the ONLY accepted credential flow for `claude -p` (since v1.0.69)
- ALWAYS passes `--strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions` (7 hardening flags; `--bare` REMOVED from all executable code paths in v1.0.69)
- ABORT spawn with `AppError::Validation` if `ANTHROPIC_API_KEY` is set in the environment (OAuth-only enforcement, v1.0.69)
- `ANTHROPIC_API_KEY` is excluded from the env-clear whitelist as defense in depth (v1.0.69)
- 4 `#[serial_test::serial(env)]` tests validate the canonical flag set and the abort behaviour (v1.0.69)
- NDJSON per-file events include `entities` (count), `rels` (count), `cost_usd` fields; since v1.0.64 `cost_usd` is omitted for OAuth users (subscription, not billed per API call)
- Summary includes `entities_total`, `rels_total`, `cost_usd` totals; `--max-cost-usd` is ignored with warning for OAuth users (since v1.0.64)
- Since v1.0.64: files exceeding 512 KB body cap are skipped BEFORE LLM extraction with `status: "skipped"` to avoid wasting tokens
- Schemas: `ingest-claude-phase.schema.json`, `ingest-claude-file-event.schema.json`, `ingest-claude-summary.schema.json`
- `--mode codex`: LLM-curated extraction via OpenAI Codex CLI (`codex exec --json` headless per file)
- Codex mode requires Codex CLI >= 0.120.0 with active OpenAI API key; uses `--output-schema` for structured JSON
- `--codex-binary <PATH>` overrides PATH lookup; `--codex-model <MODEL>` selects model; `--codex-timeout <S>` (default 300s)
- Environment variable `SQLITE_GRAPHRAG_CODEX_BINARY` overrides PATH lookup
- Full embedding pipeline applied — memories are fully searchable via `recall` and `hybrid-search`
- Since v1.0.63: relation strings from LLM extraction are normalized before DB insertion (`depends-on` → `depends_on`) — consistent with `remember` command
- Codex mode reuses the same NDJSON schema format as claude-code: `ingest-claude-phase.schema.json`, `ingest-claude-file-event.schema.json`, `ingest-claude-summary.schema.json`
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


## CRUD — Read with read and list
### REQUIRED — Direct Read by Name or ID (read)
- USE `read --name <kebab-case>` for O(1) fetch by name
- USE `read --id <N>` for direct lookup by memory_id (v1.0.67) — avoids semantic search when ID is known from prior `list` or `recall` output
- USE `read --with-graph` to include linked entities and relationships in the response (v1.0.67)
- PARSE fields `body`, `description`, `created_at_iso`, `updated_at_iso`
- TREAT exit code 4 as memory not found in the namespace
- APPLY `--tz` to localize timestamps in the output
### REQUIRED — Enumeration with Filters (list)
- USE `list --type <kind>` to filter by memory type
- ADJUST `--limit <N>`; default is ALL records in JSON mode, 50 in text mode
- PAGINATE via `--offset <N>` for large datasets
- INCLUDE soft-deleted memories via `--include-deleted`
- EXPORT full dump with `--limit 10000 --json` before backup
- RESPONSE now includes `total_count` (total matching records), `truncated` (bool), and `body_length` (int) per item
### Correct Pattern — Read Examples
- `sqlite-graphrag read --name design-auth --json`
- `sqlite-graphrag list --type decision --limit 100 --json`
- `sqlite-graphrag list --include-deleted --json | jaq '.items[] | select(.deleted)'`


## CRUD — Update with edit, rename, and restore
### REQUIRED — Body and Description Editing (edit)
- USE `edit --name <name> --body <text>` for short bodies
- PREFER `--body-file` or `--body-stdin` for long bodies
- CHANGE description via `--description <text>`
- CHANGE memory type via `--type <kind>` (e.g., `note` to `decision`) without recreating the memory (v1.0.67); skips re-embedding when body is unchanged
- EACH edit creates a new immutable version preserving history
- EDIT re-generates vector embedding when body changes — `recall` and `hybrid-search` return accurate scores after edit (since v1.0.63; description-only edits skip re-embedding)
- VALIDATE exit code 3 as an optimistic locking conflict
- JSON response: `memory_id`, `name`, `action` ("updated"), `version`, `elapsed_ms`
- v1.0.56: FTS5 desync bug fixed — edited memories are immediately findable via full-text search
### REQUIRED — History-Preserving Rename (rename)
- USE `rename --name <old> --new-name <new>`
- ACCEPT `--old`/`--new` and `--from`/`--to` as aliases since v1.0.35
- PRESERVE all versions and graph connections
- TREAT exit code 4 as missing source memory
- Since v1.0.64: rejects rename to the same name with exit 1 (Validation) — prevents version inflation
- JSON response: `memory_id`, `name` (new), `action` ("renamed"), `version`, `elapsed_ms`, `ghost_purged` (bool?, v1.0.67 — true when a soft-deleted memory occupying the target name was auto-purged)
- v1.0.56: FTS5 desync bug fixed — renamed memories are immediately findable via full-text search
### REQUIRED — Old Version Restore (restore)
- INSPECT versions via `history --name <name>` first
- USE `restore --name <name> --version <N>` for a specific version
- OMIT `--version` to select the last non-restore version automatically
- RESTORE creates a new version without overwriting prior history
- RESTORE preserves the current memory name — if a memory was renamed after the target version was created, the name stays as-is (fixed in v1.0.63; previously reverted to the version's original name)
- RE-EMBED occurs automatically so vector recall can find it again
- JSON response includes `action: "restored"`, `memory_id`, `name`, `version`, `restored_from`, `elapsed_ms`
- v1.0.56: FTS5 desync bug fixed — restored memories are immediately findable via full-text search
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
- Since v1.0.52: forget does NOT emit JSON when memory is not found; returns only stderr error + exit 4
### REQUIRED — Hard Delete (purge)
- USE `purge --retention-days <N> --yes` in automation
- DEFAULT retention is 90 days for soft-deleted memories
- RUN `--dry-run` first to audit the count
- PERMANENTLY deletes rows and reclaims disk space
### REQUIRED — Edge Removal (unlink)
- USE `unlink --from <a> --to <b> --relation <type>` for targeted removal
- `--relation` is now OPTIONAL; omit to remove all edges between `--from` and `--to`
- USE `--entity <name> --all` to bulk-remove ALL relationships for a given entity (any direction)
- ACCEPT `--source`/`--target` as aliases of `--from`/`--to`
- TREAT exit code 4 as nonexistent edge
- `--relation` accepts any kebab-case or snake_case string; non-canonical values emit a `tracing::warn!` since v1.0.50
### REQUIRED — Orphan Entity Cleanup (cleanup-orphans)
- RUN `cleanup-orphans --dry-run` to audit
- APPLY `--yes` in automated pipelines
- REMOVES entities with no linked memories or edges
- RUN periodically after bulk `forget` operations
### REQUIRED — Bulk Relationship Deletion (prune-relations)
- USE `prune-relations --relation <type> --yes` for bulk-deleting all relationships of a given type
- USE `--dry-run` to preview the count before committing
- USE `--show-entities` with `--dry-run` to list affected entity names in the response
- USE `--yes` to skip interactive confirmation in automated pipelines
- ACCEPTS any kebab-case or snake_case relation string
- RUN `cleanup-orphans` afterward to remove entities left without relationships
- JSON response: `action` (`"pruned"` `"dry_run"`), `relation`, `count`, `entities_affected`, `affected_entity_names?`, `namespace`, `elapsed_ms`
### Correct Pattern — Forget and Restore Round-Trip
- `sqlite-graphrag forget --name decision-x`
- `sqlite-graphrag history --name decision-x --json | jaq '.deleted'`
- `sqlite-graphrag restore --name decision-x`
- `sqlite-graphrag recall "decision" --json`


## Entity Management (v1.0.56)
### REQUIRED — Entity Name Validation and Normalization (v1.0.58, improved in v1.0.65)
- ALL entity creation paths (`link --create-missing`, `remember --graph-stdin`, `ingest --enable-ner`, `rename-entity --new-name`) validate names via `validate_entity_name()`
- REJECTS names shorter than 2 characters (exit 1)
- REJECTS names containing newline characters (exit 1)
- REJECTS ALL_CAPS abbreviations of 4 characters or fewer as NER noise (exit 1)
- Since v1.0.65: after validation, names are NORMALIZED to lowercase kebab-case ASCII via `normalize_entity_name()` before storage — `"Claude Code"` becomes `claude-code`, `"CANONICAL_RELATIONS"` becomes `canonical-relations`
### REQUIRED — Delete Entity (delete-entity)
- USE `delete-entity --name <entity> --json` to permanently remove an entity node
- ADD `--cascade` to also remove all relationships and memory bindings attached to the entity
- WITHOUT `--cascade` the command fails with exit 1 if the entity has relationships
- JSON response: `action`, `entity_name`, `relationships_removed`, `bindings_removed`, `elapsed_ms`
- TREAT exit code 4 as entity not found
### REQUIRED — Reclassify Entity Type (reclassify)
- USE `reclassify --name <entity> --entity-type <new> --json` to change a single entity's type
- USE `reclassify --from-type <old> --to-type <new> --batch --json` to bulk-reclassify all entities of one type
- JSON response: `action`, `count`, `description_updated?`, `namespace`, `elapsed_ms`
### REQUIRED — Merge Entities (merge-entities)
- USE `merge-entities --names "a,b,c" --into <target> --json` to merge multiple entities into one
- ALL relationships from source entities are moved to `<target>`
- SOURCE entities are deleted after merge
- JSON response: `action`, `sources`, `target`, `relationships_moved`, `entities_removed`, `elapsed_ms`
- TREAT exit code 4 as any named entity not found
### REQUIRED — List Memory Entities (memory-entities)
- USE `memory-entities --name <memory> --json` to list all entities linked to a specific memory
- USE `memory-entities --entity <entity-name> --json` to list all memories bound to an entity (reverse lookup, v1.0.58)
- Forward JSON response: `memory_name`, `entities: [{entity_id, name, entity_type}]`, `count`, `elapsed_ms`
- Reverse JSON response: `entity_name`, `memories: [{memory_id, name, description, memory_type}]`, `count`, `elapsed_ms`
- TREAT exit code 4 as memory or entity not found; exit 0 with count 0 means it exists but has no bindings
### REQUIRED — Remove NER Bindings (prune-ner)
- USE `prune-ner --entity <name> --json` to remove NER bindings for a specific entity
- USE `prune-ner --all --yes --json` to remove ALL NER bindings in the namespace
- JSON response: `action`, `bindings_removed`, `elapsed_ms`
- NER bindings are the links created automatically by GLiNER extraction; manual graph links are NOT affected


## Immutable Version History
### REQUIRED — Inspection with history
- USE `history --name <name> --json` to list versions
- USE `history --name <name> --diff --json` to include character diff stats between versions
- VERSIONS start at 1 and increment with each `edit` or `restore`
- CHRONOLOGICAL reverse order by default
- INCLUDES soft-deleted memories with flag `deleted: true`
- WITH `--diff`, each version includes `changes: {added_chars, removed_chars}` showing the diff vs the previous version
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
### Deep Research (v1.0.64, improved in v1.0.65)
- `sqlite-graphrag deep-research "<query>" --k 20 --json` — parallel multi-hop research with query decomposition
- Splits query into up to 7 sub-queries, computes a SEPARATE embedding per sub-query (v1.0.65 fix — was sharing one embedding), runs in parallel via bounded JoinSet + Semaphore
- Fuses KNN + FTS5 results via RRF per sub-query (v1.0.65 fix — FTS was hardcoded at 0.5)
- Evidence chains are directed seed-to-target paths (v1.0.65 fix — was flat global dump of top-20 relationships)
- Graph scores incorporate seed score, hop decay, and edge weight (v1.0.65 fix)
- Output: `sub_queries[]`, `results[]`, `evidence_chains[]`, `graph_context?` (entities + relationships from result memories, v1.0.66), `stats`
- Replaces manual 3-layer pipeline for comprehensive research in a single invocation
- `--k 20` results per sub-query (default, Recall@20 captures 95%+ relevant hits)
- `--max-sub-queries 7` caps decomposition (default, calibrated against MuSiQue/StepChain benchmarks)
- `--max-hops 3` graph traversal depth (default, sweet spot per NovelHopQA benchmark)
- `--min-weight 0.3` filters weak edges during traversal (default)
- `--max-results 50` caps deduplicated output (default)
- `--with-bodies` includes full memory bodies in results (opt-in)
- `--max-concurrency N` limits parallel sub-queries (default: min(cpus, 8))
- `--timeout 30` per-sub-query timeout in seconds (default)
- `--rrf-k 60` RRF fusion constant (v1.0.65, same as hybrid-search)
- `--graph-decay 0.7` graph score decay factor per hop (v1.0.65)
- `--graph-min-score 0.05` minimum score threshold for graph-expanded results (v1.0.65)
- `--max-neighbors-per-hop N` caps BFS fan-out per entity per hop (v1.0.65, default unlimited)
### Reclassify Relationship Types (v1.0.65)
- `sqlite-graphrag reclassify-relation --from-relation <old> --to-relation <new> --batch --json` — bulk renames relationship types
- Single mode: `--source A --target B --from-relation old --to-relation new`
- Batch mode: `--from-relation old --to-relation new --batch`
- Optional filters: `--filter-source-type`, `--filter-target-type`
- Handles UNIQUE collisions via `UPDATE OR IGNORE` + `DELETE` merge
- `--dry-run` previews count without modifying the database
- JSON response: `action`, `from_relation`, `to_relation`, `count`, `merged_duplicates`, `namespace`, `elapsed_ms`
### Normalize Entity Names (v1.0.65)
- `sqlite-graphrag normalize-entities --yes --json` — normalizes all entity names to lowercase kebab-case ASCII
- Auto-merges collisions: `Claude Code` + `claude-code` become one node with combined relationships
- `--dry-run` previews which entities would be renamed or merged
- Normalization: NFKD decomposition → ASCII filter → lowercase → spaces/underscores to hyphens → collapse consecutive hyphens
- Entity names are also normalized on every write path since v1.0.65 (remember, ingest, link, rename-entity)
- JSON response: `action`, `normalized_count`, `merged_count`, `namespace`, `elapsed_ms`
### Enrich Graph Quality With LLM (v1.0.65)
- `sqlite-graphrag enrich --operation <op> --mode claude-code --json` — LLM-augmented graph quality pipeline
- 3 operations: `memory-bindings` (extract entities from orphan memories), `entity-descriptions` (generate descriptions for entities with none), `body-enrich` (expand short memory bodies)
- `--dry-run` previews without spawning LLM (zero tokens)
- `--max-cost-usd N` caps cumulative API spend (ignored for OAuth users)
- `--resume` and `--retry-failed` for crash resilience via queue DB
- `--llm-parallelism <N>` controls how many LLM subprocesses run concurrently (v1.0.67, default 1); set to 2-4 to reduce wall-clock time for large enrichment batches
- Output is NDJSON: phase events, per-item events (status: `done`/`failed`/`skipped`/`preview`), summary line
- Schemas: `enrich-phase.schema.json`, `enrich-item-event.schema.json`, `enrich-summary.schema.json`
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
- `--relation` filter accepts any kebab-case or snake_case string; non-canonical values emit a `tracing::warn!` since v1.0.50
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
- `hybrid-search` response now includes `fts_degraded` (bool), `fts_error` (string?), `fts_auto_rebuilt` (bool); when `fts_degraded` is true, only vector results are returned
- `hybrid-search` per-result fields also include `normalized_score` (0-1 normalized combined score), `vec_distance` (float?), `fts_bm25` (float?)
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
- APPROXIMATE latency of 1-3 seconds on modern hardware (LLM one-shot subprocess)


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
- USE `--strict-relations` to fail with exit 1 when a non-canonical relation type is used; response includes `warnings` field listing any non-canonical relations when not strict
- USE `--max-entity-degree N` to emit `tracing::warn!` when creating an edge that would push an entity above N connections (v1.0.65, also available on `remember`)
### REQUIRED — Export with graph
- EXPORT snapshot via `graph --format json`
- USE `--format dot` for offline Graphviz
- USE `--format mermaid` to embed in Markdown
- WRITE directly to a file via `--output <PATH>`
- INSPECT `nodes` and `edges` in the exported JSON
- EDGES referencing missing entities are logged via `tracing::warn!` and skipped since v1.0.50
### REQUIRED — Entity Enumeration (graph entities)
- USE `graph entities --json` to list all entities
- ACCESS via `jaq -r '.entities[].name'` (field is `entities`, NOT `items`)
- FILTER by `--entity-type <type>` when needed
- PAGINATE with `--limit` and `--offset`
- USE before planning traversals or batch links
- SORT via `--sort-by degree|name|created_at` (default `name`)
- SET sort direction via `--order asc|desc` (default `asc`)
- RESPONSE now includes `degree` field per entity (number of connected relationships)
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


## LLM-Driven Graph Quality
### REQUIRED — Relation Mapping Table
- MAP non-canonical relations to canonical equivalents before persisting
- `adds` maps to `causes` (creation implies causation)
- `creates` maps to `causes` (same rationale)
- `implements` maps to `supports` (implementation supports a design)
- `blocks` maps to `contradicts` (blocking contradicts progress)
- `tested-by` maps to `related` (testing is a form of relatedness)
- `part-of` maps to `applies-to` (a part applies to its whole)
- PREFER the canonical value over custom strings to avoid `tracing::warn!` noise
- CUSTOM relations are accepted but canonical ones yield better cross-memory recall
### REQUIRED — Entity Curation
- EXTRACT only domain-specific concepts: real projects, tools, people, decisions, files
- NEVER create entities from stop words, articles, pronouns, or generic verbs
- NEVER create entities from UUIDs, hashes, timestamps, or line numbers
- NEVER create entities from single characters or two-letter abbreviations
- CHOOSE entity_type deliberately: `concept` for abstract ideas, `tool` for software, `decision` for architectural choices, `project` for codebases, `person` for contributors, `file` for source paths
- PREFER fewer high-quality entities over many low-signal ones
- DEDUPLICATE: search `graph entities --json` before creating to avoid near-duplicates like "auth" and "authentication"
### REQUIRED — Relation Curation
- `depends-on`: A cannot function without B (hard dependency)
- `uses`: A leverages B but could substitute it (soft dependency)
- `supports`: A reinforces or enables B (design backing implementation)
- `causes`: A triggers or produces B (causal chain)
- `fixes`: A resolves a problem described in B (bug fix, incident resolution)
- `contradicts`: A conflicts with or invalidates B (competing designs, blockers)
- `applies-to`: A is relevant to or scoped within B (rule applies to module)
- `follows`: A comes after B in sequence or priority (workflow ordering)
- `replaces`: A supersedes B (migration, deprecation)
- `tracked-in`: A is monitored or managed in B (issue in tracker, metric in dashboard)
- `related`: A and B share context but no stronger relation fits (use sparingly, never as default)
- `mentions`: A references B without implying a relationship (use ONLY for citations, never as a catch-all)
- ASSIGN `strength` based on coupling: 0.9 for hard dependencies, 0.7 for design relationships, 0.5 for contextual links, 0.3 for weak references
### REQUIRED — Description Enrichment
- GENERIC descriptions like "ingested from docs/README.md" waste the description field
- UPGRADE via `edit --name <name> --description "concise semantic summary"`
- GOOD description answers: what is this memory ABOUT and WHY does it matter?
- BAD: "ingested from auth.md" → GOOD: "JWT token rotation strategy with 15-min expiry and refresh flow"
- BAD: "user feedback" → GOOD: "user prefers single bundled PR over many small ones for refactors"
- LIMIT to one sentence, 10-20 words, focusing on the unique insight
- RUN `list --type <kind> --json | jaq '.items[] | select(.description | test("ingested|imported|added")) | .name'` to find generic descriptions
- BATCH enrichment: pipe names to a loop calling `edit --description` for each
### REQUIRED — Graph Quality Improvement Workflow
- STEP 1 — Audit: `graph stats --json` to measure node_count, edge_count, avg_degree
- STEP 2 — Identify noise: `list --json | jaq '.items[] | select(.description | test("ingested|imported")) | .name'`
- STEP 3 — Enrich descriptions: `edit --name <name> --description "semantic summary"`
- STEP 4 — Prune low-signal relations: `prune-relations --relation mentions --dry-run --json`
- STEP 5 — Execute prune: `prune-relations --relation mentions --yes --json`
- STEP 6 — Clean orphans: `cleanup-orphans --yes --json`
- STEP 7 — Verify: `health --json | jaq '.integrity_ok'`
- SCHEDULE this workflow after bulk `ingest` operations
### FORBIDDEN — LLM Graph Anti-patterns
- NEVER use `mentions` as a default relation; it adds noise without signal
- NEVER create entities from implementation details (variable names, line numbers, commit hashes)
- NEVER set all strengths to 1.0; differentiate coupling levels
- NEVER leave "ingested from" descriptions without enrichment
- NEVER create redundant edges (if A depends-on B, do not also add A uses B)
- NEVER persist ephemeral state (current branch, WIP progress, temporary workarounds)
- NEVER skip deduplication; search `hybrid-search` or `graph entities` before creating


## Architecture Note — No Daemon (v1.0.76)
### NOTE — Daemon Infrastructure Fully Removed
- The daemon IPC infrastructure (`sqlite-graphrag daemon`, `daemon --ping`, `daemon --stop`) was fully removed in v1.0.76
- The CLI is now 100% one-shot: each embedding operation spawns a headless `claude -p` or `codex exec` subprocess via OAuth
- There is no in-memory model server and no warm cache between invocations
- Latency per embedding call is 1-3 seconds (LLM round-trip)
- No Cargo feature restores the daemon — it is permanently removed


## Architecture Note — No Local Model Cache (v1.0.76)
### NOTE — Cache Commands Removed
- The `cache list` and `cache clear-models` commands were removed in v1.0.76
- There is no local ONNX model cache in the default LLM-only build
- Embedding health is verified via `health --json` (check `integrity_ok`) and `stats --json`


## JSON Contract and Pipelines
### REQUIRED — Deterministic Output
- USE `--json` in all subcommands before piping
- PREFER `--json` over `--format json` in one-liners
- FILTER fields via `jaq` instead of regex on stdout
- READ only fields actually returned by the subcommand
- TREAT JSON as a SemVer-versioned API
### REQUIRED — Error JSON Contract (v1.0.56, updated v1.0.68)
- ALL error paths now emit a JSON object on stdout: `{"error": true, "code": N, "message": "..."}`
- stderr still receives the human-readable error with a descriptive prefix
- CONSUMERS must check `stdout` JSON first (look for `"error": true`), then fall back to the exit code
- This applies to ALL commands when `--json` is passed; without `--json` errors go only to stderr
- Since v1.0.68 the `code: 75` envelope has TWO distinct templates — both map to the same exit code: template A `job <job_type> for namespace '<namespace>' is already running (exit 75); wait for it to finish or pass --wait-job-singleton <SECONDS>` (emitted by `enrich`, `ingest --mode claude-code`, `ingest --mode codex` when another invocation holds the singleton), and template B `all <max> concurrency slots occupied after waiting <waited_secs>s (exit 75); use --max-concurrency or wait for other invocations to finish` (legacy semaphore exhaustion)
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
- `hybrid-search` returns `results[].name`, `combined_score`, `score`, `vec_rank`, `fts_rank`, `source`, `body`
- `hybrid-search` response-level: `query`, `k`, `rrf_k`, `weights`, `results[]`, `graph_matches[]`, `elapsed_ms`
- `hybrid-search` `graph_matches[]` uses RecallItem: `name`, `distance`, `source` ("graph"), `graph_depth`
- `related` returns `results[].name`, `hop_distance`, `relation`, `source_entity`, `target_entity`, `weight`
- `graph traverse` returns `hops[].entity`, `relation`, `direction`, `weight`, `depth`
- `read` returns `name`, `body`, `description`, `created_at_iso`, `updated_at_iso`
- `edit` returns `memory_id`, `name`, `action` ("updated"), `version`, `elapsed_ms`
- `rename` returns `memory_id`, `name` (new), `action` ("renamed"), `version`, `elapsed_ms`
- `forget` returns `action` (`"soft_deleted"`/`"already_deleted"`), `forgotten`, `name`, `namespace`, `elapsed_ms`
- `list` response-level: `items[]`, `elapsed_ms`; each item has `id`, `memory_id`, `name`, `namespace`, `type`, `memory_type`, `description`, `snippet`, `updated_at`, `updated_at_iso`, `deleted_at?`, `deleted_at_iso?`
- `export` per-line: `name`, `type`, `memory_type`, `description`, `body`, `namespace`, `created_at_iso`, `updated_at_iso`, `deleted_at_iso?`; summary line: `summary` (true), `exported`, `namespace`, `elapsed_ms`
- `health` returns `integrity_ok`, `schema_ok`, `vec_memories_ok`, `vec_entities_ok`, `vec_chunks_ok`, `fts_ok`, `model_ok`, `counts`, `wal_size_mb`, `journal_mode`, `db_path`, `db_size_bytes`, `checks[]`
- `health.counts` contains: `memories`, `entities`, `relationships`, `vec_memories`
- `health` optionally returns `mentions_ratio` (float) and `mentions_warning` (string) when mentions exceed 50% of relationships
- `health` now includes `fts_query_ok` (bool) indicating whether a live FTS5 query succeeded (not just schema integrity), and `sqlite_version` (string) showing the SQLite version in use
- `stats` returns GLOBAL data (no namespace filter): `memories`, `entities`, `relationships`, `chunks_total`, `avg_body_len`, `namespaces[]`, `db_size_bytes`, `schema_version`, `elapsed_ms`; also includes legacy aliases `db_bytes`, `edges`, `memories_total`, `entities_total`, `relationships_total`
- `ingest` per file: `file`, `name`, `status` (`"indexed"`/`"skipped"`/`"failed"`), `truncated`, `original_name?`, `original_filename?`, `memory_id?`, `action?`, `error?`
- `ingest` summary: `summary` (true), `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- `ingest --mode claude-code` phase: `phase` (`"validate"`/`"scan"`), `claude_path?`, `version?`, `dir?`, `files_total?`, `files_new?`, `files_existing?`
- `ingest --mode claude-code` per file: `file`, `name`, `status` (`"done"`/`"failed"`/`"preview"`), `memory_id?`, `entities?`, `rels?`, `cost_usd?`, `elapsed_ms?`, `error?`, `index`, `total`
- `ingest --mode claude-code` summary: `summary` (true), `files_total`, `completed`, `failed`, `skipped`, `entities_total`, `rels_total`, `cost_usd`, `elapsed_ms`
- NOTE: `cache list` and `cache clear-models` were removed in v1.0.76 (no local model cache in LLM-only build)
- `prune-relations` returns `action` (`"pruned"`/`"dry_run"`), `relation`, `count`, `entities_affected`, `affected_entity_names?`, `namespace`, `elapsed_ms`
- `fts rebuild` returns `action` ("rebuilt"), `rows_indexed`, `elapsed_ms`
- `fts check` returns `action` ("checked"), `integrity_ok`, `detail?`, `elapsed_ms`
- `fts stats` returns `total_rows`, `shadow_pages?`, `fts_functional`, `elapsed_ms`
- `backup` returns `action` ("backed_up"), `source`, `destination`, `size_bytes`, `elapsed_ms`
- `delete-entity` returns `action` ("deleted"), `entity_name`, `namespace`, `relationships_removed`, `bindings_removed`, `elapsed_ms`
- `reclassify` returns `action` ("reclassified"), `count`, `description_updated?` (bool, present when `--description` applied), `namespace`, `elapsed_ms`
- `merge-entities` returns `action` ("merged"), `sources[]`, `target`, `namespace`, `relationships_moved`, `entities_removed`, `elapsed_ms`
- `memory-entities` forward returns `memory_name`, `entities[].{entity_id, name, entity_type}`, `count`, `elapsed_ms`
- `memory-entities` reverse (`--entity`) returns `entity_name`, `memories[].{memory_id, name, description, memory_type}`, `count`, `elapsed_ms`
- `prune-ner` returns `action` (`"pruned"`/`"dry_run"`/`"aborted"`), `bindings_removed`, `namespace`, `entity?`, `elapsed_ms`
- `link` returns `action` ("linked"), `from`, `to`, `relation`, `weight`, `namespace`, `elapsed_ms`, `created_entities?` (array, when `--create-missing`), `warnings?` (array, when non-canonical relation)
- `unlink` returns `action` ("deleted"), `from_name`, `to_name`, `relation`, `relationships_removed`, `namespace`, `elapsed_ms`
- `rename-entity` returns `action` ("renamed"), `old_name`, `new_name`, `entity_id`, `namespace`, `elapsed_ms`
- `deep-research` returns `query`, `sub_queries[]` (`id`, `text`, `source`), `results[]` (`name`, `score`, `source` enum: knn/fts/hybrid/graph, `sub_query_ids`, `snippet`, `body?`, `hop_distance?`), `evidence_chains[]` (`from`, `to`, `path[]`, `total_weight`, `depth`, `sub_query_ids`), `graph_context?` (`entities[]` with `name`, `entity_type`, `degree`; `relationships[]` with `from`, `to`, `relation`, `weight`), `stats` (`sub_queries_total`, `sub_queries_completed`, `sub_queries_failed`, `sub_queries_timed_out`, `unique_memories_found`, `evidence_chains_found`, `elapsed_ms`)
- `reclassify-relation` returns `action` ("reclassified"/"dry_run"), `from_relation`, `to_relation`, `count`, `merged_duplicates`, `namespace`, `elapsed_ms`
- `normalize-entities` returns `action` ("normalized"/"dry_run"), `normalized_count`, `merged_count`, `namespace`, `elapsed_ms`
- `enrich` emits NDJSON: phase events (`phase`, `operation`), item events (`name`, `status`, `entities?`, `rels?`, `cost_usd?`, `elapsed_ms?`), summary (`operation`, `completed`, `failed`, `skipped`, `cost_usd`, `elapsed_ms`)
- `health` also returns `top_relation` (string?), `top_relation_ratio` (float?), `applies_to_ratio` (float?), `relation_concentration_warning` (string?) when any single relation exceeds 40% of edges (v1.0.65); `vec_memories_missing` (i64) and `vec_memories_orphaned` (i64) for vector desync diagnostics (v1.0.66)
- `health` returns super-hub detection fields (v1.0.67): `super_hub_count` (i64?), `super_hub_warning` (string?), `top_hub_entity` (string?), `top_hub_degree` (i64?), `hub_warning` (string?) when entities exceed degree threshold; also `non_normalized_count` (i64?) and `normalization_warning` (string?) for entity name normalization audit
- `graph --format json` returns `nodes[]` AND `entities[]` (alias, v1.0.66) with `id`, `name`, `namespace`, `kind`, `type`; `edges[]`; `elapsed_ms`
- `list --json` returns `items[]` AND `memories[]` (alias, v1.0.66); each item includes `body_length`
- `graph entities --json` returns `entities[]` with `id`, `name`, `entity_type`, `namespace`, `created_at`, `degree`, `description?` (v1.0.66)
- `edit` accepts `--type` to change memory type without re-creating (v1.0.66); `--body` and `--description` remain unchanged
- `remember-batch` emits per-item NDJSON with `name`, `status`, `memory_id?`, `error?`, `elapsed_ms` plus a summary line (v1.0.67)


## Exit Codes and Retry Strategy
### REQUIRED — Complete Exit Code Handling
- `0` equals success; parse stdout
- `1` equals validation (invalid weight, self-link, max-files exceeded)
- `2` equals Clap argument parsing error (invalid flag, bad timezone value, missing required arg)
- `9` equals duplicate (memory already exists without `--force-merge`); since v1.0.51 also returned when the memory is soft-deleted — use `--force-merge` to restore and update, or `restore` to revive
- `3` equals optimistic locking conflict; reload and retry
- `4` equals entity, memory, or version not found
- `5` equals namespace error (invalid name or conflict)
- `6` equals payload above the size limit
- `10` equals database error; run `vacuum` and `health`
- `11` equals embedding failure (LLM subprocess error or model load failure)
- `12` equals failure loading vector extension (historical; `sqlite-vec` removed in v1.0.76)
- `13` equals partial batch failure; reprocess only failed
- `14` equals I/O error (inaccessible file, permission, disk full)
- `15` equals database busy; widen `--wait-lock`
- `20` equals internal error or JSON serialization failure
- `75` equals exhausted slots in ingest or other heavy command OR `AppError::JobSingletonLocked` from `enrich`, `ingest --mode claude-code`, or `ingest --mode codex` since v1.0.68; the `message` field embeds `job_type` and `namespace` for parsing via `job '(\w+)'.*namespace '(\w+)'` regex
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
- LIMIT parallel ingestion in CI to avoid LLM rate limits
### REQUIRED — Two Parallelism Axes in ingest
- `--max-concurrency` governs simultaneous CLI invocations
- `--ingest-parallelism` governs extract plus embed in parallel
- ADJUST both independently according to RAM and CPU
- USE `--low-memory` to force unitary parallelism
- HONOR `SQLITE_GRAPHRAG_LOW_MEMORY=1` on constrained hosts


## FTS5 Management (v1.0.56)
### REQUIRED — FTS5 Commands
- USE `fts rebuild --json` to fully rebuild the FTS5 full-text index; response: `{action, rows_indexed, elapsed_ms}`
- USE `fts check --json` to run the FTS5 integrity-check; response: `{action, integrity_ok, detail, elapsed_ms}`
- USE `fts stats --json` to inspect FTS5 health; response: `{total_rows, shadow_pages, fts_functional, elapsed_ms}`
- RUN `fts rebuild` when `hybrid-search` returns `fts_degraded: true` or after suspected index corruption
- RUN `fts check` as part of periodic health audits alongside `health --json`
- TREAT `fts_functional: false` in `fts stats` as a signal to run `fts rebuild`


## Safe Backup (v1.0.56)
### REQUIRED — backup Command
- USE `backup --output <path> --json` for a safe, online backup using the SQLite Online Backup API
- BACKUP is consistent even while writes are in progress
- JSON response: `{action, source, destination, size_bytes, elapsed_ms}`
- PREFER `backup` over `sync-safe-copy` for programmatic backups; both are safe but `backup` uses the native SQLite API
- TREAT exit code 14 as an I/O error (destination path not writable, disk full)


## Entity Operations (v1.0.56)
### REQUIRED — delete-entity
- USE `delete-entity --name <entity> --cascade --json` to remove an entity and all its relationships and memory bindings
- FLAG `--cascade` is required as a confirmation gate; without it the command exits with validation error
- JSON response: `{action, entity_name, namespace, relationships_removed, bindings_removed, elapsed_ms}`
- RUN `cleanup-orphans` afterwards to remove any newly orphaned entities
- TREAT exit code 4 as entity not found
### REQUIRED — rename-entity (v1.0.58)
- USE `rename-entity --name <old> --new-name <new> --json` to rename an entity preserving all relationships and memory bindings
- RE-EMBEDS the entity vector with the new name for semantic search accuracy
- JSON response: `{action: "renamed", old_name, new_name, entity_id, namespace, elapsed_ms}`
- TREAT exit code 4 as entity not found; exit 1 if new name already exists or fails validation (shorter than 2 chars, contains newlines, or short ALL_CAPS abbreviation)
- ALL relationships and memory_entities bindings use integer FK and are unaffected by the name change
### REQUIRED — reclassify
- USE `reclassify --name <entity> --new-type <type> --json` for single entity type change
- USE `reclassify --from-type <old> --to-type <new> --batch --json` for bulk reclassification
- USE `reclassify --name <entity> --description "text" --json` to update entity description in single mode (v1.0.58)
- COMBINE `--new-type` with `--description` to change both type and description in one operation
- JSON response: `{action, count, description_updated?, namespace, elapsed_ms}`
- TREAT count 0 in batch mode as indication that --from-type may be a typo
### REQUIRED — merge-entities
- USE `merge-entities --names "a,b" --into <target> --json` to merge source entities into a target
- ALL relationships from source nodes are redirected to the target via UPDATE OR IGNORE
- DUPLICATE relationships are removed automatically after redirection
- JSON response: `{action, sources, target, namespace, relationships_moved, entities_removed, elapsed_ms}`
- TREAT exit code 4 as target entity not found
### REQUIRED — memory-entities
- USE `memory-entities --name <memory> --json` to list all entities linked to a specific memory
- USE `memory-entities --entity <entity-name> --json` to list all memories bound to an entity (reverse lookup, v1.0.58)
- FORWARD response: `{memory_name, entities: [{entity_id, name, entity_type}], count, elapsed_ms}`
- REVERSE response: `{entity_name, memories: [{memory_id, name, description, memory_type}], count, elapsed_ms}`
- TREAT exit code 4 as memory/entity not found; exit 0 with count 0 means it exists but has no linked items
- USE reverse lookup before rename-entity or delete-entity for impact assessment
### REQUIRED — prune-ner
- USE `prune-ner --entity <name> --dry-run --json` to preview NER binding removal
- USE `prune-ner --entity <name> --yes --json` to remove NER bindings for a single entity
- USE `prune-ner --all --yes --json` to remove ALL NER bindings in the namespace
- JSON response: `{action, bindings_removed, namespace, entity, elapsed_ms}`
- RUN `cleanup-orphans` afterwards to remove entity nodes left without any bindings


## Maintenance and Backup
### REQUIRED — Periodic Hygiene
- SCHEDULE `purge --retention-days 30 --yes` weekly
- RUN `vacuum` after large purges
- RUN `optimize` to refresh planner statistics
- CLEAN orphans via `cleanup-orphans --yes` after bulk forget
### REQUIRED — Safe Backup
- SINCE v1.0.53, every write command runs `PRAGMA wal_checkpoint(TRUNCATE)` after committing, ensuring the `.sqlite` file is always self-contained when cloud sync tools (Dropbox, iCloud, OneDrive) read it
- USE `sync-safe-copy --dest <path>` for atomic snapshots before critical operations
- COMPRESS snapshots via `ouch compress` for remote upload
- EXPORT memories via `sqlite-graphrag export` as NDJSON (one JSON line per memory + summary); supports `--namespace`, `--type`, `--include-deleted`, `--limit`
- VERSION the database with Git LFS when feasible
- IF corruption occurs despite checkpoint, recover with `sqlite3 broken.sqlite ".recover" | sqlite3 repaired.sqlite`
### REQUIRED — Schema Diagnostics
- USE `debug-schema --json` for troubleshooting
- INSPECT `schema_version`, `objects`, `migrations`
- CURRENT schema version is 13 (V013 drops vec virtual tables and creates BLOB-backed embedding tables; V012 added relationship timestamps)
- COMMAND is hidden from `--help`; invoke by exact name
### Correct Pattern — Weekly Cron
- `sqlite-graphrag purge --retention-days 30 --yes`
- `sqlite-graphrag cleanup-orphans --yes`
- `sqlite-graphrag prune-relations --relation mentions --yes` (when NER-generated edges need cleanup)
- `sqlite-graphrag vacuum --json`
- `sqlite-graphrag optimize --json`
- `sqlite-graphrag sync-safe-copy --dest ~/Dropbox/graphrag.sqlite`


## New in v1.0.76 — LLM-Only One-Shot (G21 + G22 + G23 + G24 + G25)
### REQUIRED — Default Build Architecture Change
- The default build of v1.0.76 is LLM-Only and one-shot. There is no daemon, no ONNX runtime, no `multilingual-e5-small` model download. Embedding generation and NER delegate to a headless `claude code` or `codex` subprocess (OAuth, no MCP, no hooks). The release binary is approximately 6 MB.
- The default build is LLM-only with no local model dependencies
- See ADR-0019 for the full architectural rationale, ADR-0021 for the daemon deprecation timeline, ADR-0022 for the BLOB-backed embedding tables, ADR-0023 for the tokenizer removal, ADR-0024 for the FTS5 coarse-filter + cosine-refinement search path, and ADR-0025 for the reaffirmed OAuth-only credential flow.
### REQUIRED — Migrate Subcommand Family
- USE `migrate --rehash --json` to rewrite recorded migration checksums via `SipHasher13(name|version|sql)` so the algorithm matches `refinery-core 0.9.1`. The same SipHasher13 crate and the same hashing order are used. Response schema: `migrate-rehash.schema.json`.
- USE `migrate --to-llm-only --drop-vec-tables --json` as the one-shot upgrade for v1.0.74 / v1.0.75 databases. Combines checksum rewrite (--rehash) with the V013 vec-table-drop migration and reports vec-table state. The `--drop-vec-tables` flag is REQUIRED as a safety guard. Response schema: `migrate-to-llm-only.schema.json`.
- After `migrate --to-llm-only`, embeddings are recomputed lazily on the next `remember` / `edit` / `ingest`. Operators who want to pre-warm a large corpus can loop `edit --description "<same>"` over `list --json | jaq -r '.items[].name'`.
- The V002 migration was intentionally emptied to a no-op for v1.0.76; this is the root cause of the `applied migration V2 is different than filesystem one V2` checksum mismatch that `migrate --rehash` repairs. See ADR-0026 for the full drift narrative.
### REQUIRED — Schema Version and BLOB-Backed Embeddings
- The current schema version is 13. Migration V013 drops the `vec_memories`, `vec_entities`, and `vec_chunks` virtual tables and replaces them with regular BLOB-backed `memory_embeddings`, `entity_embeddings`, and `chunk_embeddings` tables. Cosine similarity is computed in pure Rust on demand in `src/similarity.rs` (ADR-0020, ADR-0022).
- Hybrid-search still uses FTS5 for coarse filtering and now refines the candidate set with a pure-Rust cosine over the BLOB embeddings. FTS5 stays healthy because the rebuild is gated by `optimize --fts-skip-when-functional` (G36 from v1.0.69).
- The daemon infrastructure was fully removed in v1.0.76. The LLM subprocess is the new "model loader" — each call spawns a headless process.
### REQUIRED — OAuth-Only Reaffirmed
- The OAuth-only mandate from v1.0.69 is REAFFIRMED. The spawn ABORTS with `AppError::Validation` if `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` is set in the environment. Both variables are excluded from the env-clear whitelist as defence in depth.
- New `--extraction-backend llm|none` global flag (default `llm`) selects the extraction backend. `llm` is the LLM-backed path; `none` is a no-op
### FORBIDDEN — v1.0.76 Anti-patterns
- NEVER install v1.0.76 with `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` in the environment; the spawn aborts.
- NEVER depend on the daemon in new code; the daemon will be REMOVED in v1.1.0.
- NEVER mix `vec_memories` / `vec_entities` / `vec_chunks` queries (removed in v1.0.76); use `memory_embeddings` / `entity_embeddings` / `chunk_embeddings` instead.
- NEVER use `migrate --to-llm-only` without `--drop-vec-tables`; the safety guard refuses the operation otherwise.


## Shell Completions (v1.0.67)
### REQUIRED — completions Command
- USE `completions <shell>` to generate shell completion scripts
- SUPPORTED shells: `bash`, `zsh`, `fish`, `elvish`, `powershell`
- PIPE output to appropriate shell config file
### Correct Pattern — completions Examples
- `sqlite-graphrag completions bash > ~/.local/share/bash-completion/completions/sqlite-graphrag`
- `sqlite-graphrag completions zsh > ~/.zfunc/_sqlite-graphrag`
- `sqlite-graphrag completions fish > ~/.config/fish/completions/sqlite-graphrag.fish`
