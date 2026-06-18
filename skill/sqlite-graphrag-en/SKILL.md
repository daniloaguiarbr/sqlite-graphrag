---
name: sqlite-graphrag
description:For persistent memory, GraphRAG, or long-term context in Claude Code, Codex, Cursor, Windsurf, AI agents. On: remember this, save conversation, retrieve context, hybrid search, entity graph, SQLite memory, local RAG, LLM-only embedding, OAuth flow, BLOB-backed embedding, migrate to-llm-only, migrate rehash, vec tables drop, embedding-dim, llm-parallelism, batched embedding, re-embed, force-reembed, G28-G58 gaps, OAuth-only enforcement, ANTHROPIC_API_KEY/OPENAI_API_KEY abort, Claude/Codex hardening flags, Mock LLM CLI in CI, daemon removal, A1/A2 audits, ADR-0032/0033/0034, G45/G53/G55/G56/G58, v1.0.82 five-gap (GAP-001..005), pending-embeddings, slots/pending/embedding subcommands, V014/V015 migrations, llm-max-host-concurrency, llm-slot-wait-secs, graceful-shutdown-secs, SHUTDOWN_EXIT_CODE, codex login, ADR-0041 v1.0.83, ANTHROPIC_AUTH_TOKEN, ANTHROPIC_BASE_URL, Minimax, OpenRouter, AWS Bedrock, --dry-run-backend, backend_invoked, vec_degraded_reason, embed_via_claude_local, LlmEmbeddingBuilder, GAP-003 slot circuit breaker, G58 deterministic OAuth fallback, G45-CR5 anthropic-ratelimit headers, G55 bilingual NotFound, v1.0.84, v1.0.85. KW: memory RAG GraphRAG SQLite one-shot OAuth offline persistent graph entity v1.0.82 v1.0.83 v1.0.84 v1.0.85.
---


## Fundamental Principles
- INVOKE always as subprocess via `std::process::Command`
- READ stdout for JSON or NDJSON structured data
- READ stderr for tracing logs and human messages
- CHECK exit code BEFORE parsing stdout
- TRUST JSON contracts as SemVer-versioned API
- BUILD is LLM-only and one-shot; binary is ~6 MB
- BUILD has NO daemon, NO ONNX runtime, NO model cache
- OAUTH-ONLY: spawn ABORTS exit 1 if `ANTHROPIC_API_KEY` is set
- OAUTH-ONLY: spawn ABORTS exit 1 if `OPENAI_API_KEY` is set
- NAMESPACE per project via `--namespace <ns>` or env
- NAMESPACE default is `global` when omitted
- NEVER expose the binary as MCP server or HTTP service
- NEVER write `.sqlite` file in parallel to the binary
- NEVER edit the `.sqlite` file from another tool


## Initialization, Health and Global Config
- RUN `sqlite-graphrag init --namespace <ns>` on first use
- RUN `health --json` to verify `integrity_ok` and `schema_ok`
- VERIFY `schema_version >= 15` after `init` or `migrate`
- RUN `migrate --json` after each binary upgrade
- USE `migrate --to-llm-only --drop-vec-tables --json` for v1.0.74 or v1.0.75 databases
- USE `migrate --rehash --json` to repair V002 SipHasher13 checksum drift
- TREAT exit code 10 as database error; run `vacuum` and `health`
- TREAT exit code 15 as busy; widen `--wait-lock`
- ABORT pipeline when `integrity_ok` returns `false`
- RUN `optimize --json` to refresh planner stats; response includes `fts_rebuilt`
- USE `optimize --skip-fts --json` when FTS5 was recently rebuilt
- RUN `fts rebuild --json` when `health.fts_degraded` is true
- INSPECT `wal_size_mb` in `health` for fragmentation
- VERIFY `journal_mode` equals `wal` in production
- USE `debug-schema --json` for troubleshooting schema drift
- PASS `--db <PATH>` to override database location
- SET `SQLITE_GRAPHRAG_DB_PATH` env for persistent config
- PASS `--namespace <ns>` to isolate project data
- SET `SQLITE_GRAPHRAG_NAMESPACE` env for persistent namespace
- PASS `--lang en` or `--lang pt` to force stderr language
- PASS `--tz America/Sao_Paulo` to localize timestamps
- SET `SQLITE_GRAPHRAG_DISPLAY_TZ` env for persistent timezone
- SET `SQLITE_GRAPHRAG_LOG_FORMAT=json` for log aggregators
- USE `-v` for info, `-vv` for debug, `-vvv` for trace
- ENABLE `SQLITE_GRAPHRAG_LOW_MEMORY=1` in constrained containers
- SET `SQLITE_GRAPHRAG_EMBEDDING_DIM` env in range `[8, 4096]`
- SET `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` for compliance mode (ADR-0041)
- SET `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1` ONLY for CI test harnesses
- VALID `--type` values: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- GLOBAL flags: `--db`, `--namespace`, `--lang`, `--tz`, `--json`, `--low-memory`, `--max-concurrency N`, `--wait-lock SECS`, `--llm-parallelism N`, `--llm-backend claude|codex|none|auto[,fallback...]`, `--dry-run-backend`, `--llm-fallback-mode <claude|codex>`, `--graceful-shutdown-secs N`


## Architecture Contract (OAuth/LLM/One-Shot)
- BUILD is LLM-only; default build has NO `fastembed`, `ort`, `ndarray`, `tokenizers`, `huggingface-hub`, `sqlite-vec`, `GLiNER`
- BUILD removed `daemon` subcommand entirely (ADR-0021)
- COSINE similarity is pure Rust in `src/similarity.rs`
- COSINE runs over BLOB-backed `memory_embeddings`, `entity_embeddings`, `chunk_embeddings`
- SCHEMA v15 after `init` or `migrate` on fresh database
- MIGRATION V013 drops `vec_memories`, `vec_entities`, `vec_chunks` virtual tables
- MIGRATION V014 creates `pending_memories` checkpoint table
- MIGRATION V015 creates `pending_embeddings` retry table
- OAUTH-ONLY: `ANTHROPIC_API_KEY` ABORTS spawn with `AppError::Validation` (ADR-0011)
- OAUTH-ONLY: `OPENAI_API_KEY` ABORTS spawn with `AppError::Validation` (ADR-0011)
- OAUTH-ONLY: both API keys EXCLUDED from env-clear whitelist
- OAUTH-ONLY: `--bare` flag REMOVED from all executable paths
- OAUTH-ONLY: 7 hardening flags ALWAYS passed to `claude -p`
- HARDENING flags for claude: `--strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions --output-schema`
- HARDENING flags for codex: `--json --output-schema --ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules -c mcp_servers='{}' --ask-for-approval never`
- ADR-0041 v1.0.83: `ANTHROPIC_AUTH_TOKEN` PRESERVED for Anthropic-compatible providers
- ADR-0041 v1.0.83: `ANTHROPIC_BASE_URL` PRESERVED for custom endpoints
- ADR-0041 v1.0.83: `OPENAI_BASE_URL` PRESERVED for OpenAI-compatible endpoints
- ADR-0041 v1.0.83: `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, `OTEL_EXPORTER_OTLP_ENDPOINT` PRESERVED
- ADR-0041 v1.0.83: supported providers include OpenRouter, AWS Bedrock, corporate gateways
- EMBEDDING DIM precedence: `SQLITE_GRAPHRAG_EMBEDDING_DIM` env then `schema_meta.dim` then default 64 MRL
- EMBEDDING DIM adapts batch size: base 8 chunks / 25 entity names at dim 64
- MOCK LLM CLI for CI: prepend `tests/mock-llm` to PATH
- SHUTDOWN bypass recipe: `PATH=tests/mock-llm:$PATH SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1 setsid -w timeout 120 sqlite-graphrag …`
- NEVER install with `--features embedding-legacy` or `--features ner-legacy`
- NEVER depend on daemon or `--bare` flag (REMOVED in v1.0.76 and v1.0.79)
- NEVER mix `vec_memories` queries (REMOVED in v1.0.76)
- NEVER call `migrate --to-llm-only` without `--drop-vec-tables` safety guard


## CRUD — Write Path (remember, remember-batch, ingest)
- INVOKE `remember --name <kebab> --type <kind> --description <text> --body-stdin` for long bodies
- INVOKE `remember --name <kebab> --body-file <path>` to avoid shell escaping
- INVOKE `remember --name <kebab> --body <text>` for short bodies
- PASS `--force-merge` for idempotent updates and soft-deleted restoration
- PASS `--clear-body` to wipe body during `--force-merge` update
- PASS `--dry-run` to validate inputs without persisting
- PASS `--max-rss-mb <MiB>` to abort when RSS exceeds threshold (default 8192)
- RESPECT 512000 bytes and 512 chunks limit per body
- INVOKE `remember --graph-stdin` to attach `{body, entities, relationships}` in single JSON
- PASS entities as `[{name, entity_type}]` with kebab-case ASCII
- PASS relationships as `[{source, target, relation, strength}]` where `strength ∈ [0.0, 1.0]`
- USE `--enable-ner` for URL-regex entity extraction (URL-regex ONLY since v1.0.79)
- NEVER send both `entity_type` and `type` in same JSON object
- NEVER use `--gliner-variant` (no-op since v1.0.79)
- INVOKE `remember-batch` for 10+ memories via NDJSON stdin
- EXPECT per-item event: `name`, `status ∈ {created, updated, skipped, failed}`, `memory_id?`, `error?`, `elapsed_ms`
- EXPECT summary line: `total`, `created`, `updated`, `skipped`, `failed`, `elapsed_ms`
- INVOKE `ingest <DIR> --recursive --pattern "*.md"` to import directory
- PASS `--type <kind>` to apply same type to all ingested files
- RESPECT `--max-files 10000` cap as all-or-nothing validation
- USE `--fail-fast` to stop at first per-file failure
- USE `--max-name-length N` to override 60-char name truncation
- EXPECT NDJSON per-file line: `file`, `name`, `status`, `truncated`, `original_name?`, `memory_id?`, `action?`, `error?`
- EXPECT summary line: `files_total`, `files_succeeded`, `files_failed`, `files_skipped`, `elapsed_ms`
- USE `--llm-parallelism N` on `ingest` (default 2, clamp [1, 32])
- DISTINGUISH `--max-concurrency N` (CLI fan-out) from `--ingest-parallelism N` (per-file extract+embed)
- INVOKE `ingest --mode claude-code` for LLM-curated entity extraction
- INVOKE `ingest --mode codex` for OpenAI Codex-curated extraction
- EXPECT claude-code events: `entities` count, `rels` count, `cost_usd` (Omit cost for OAuth)
- USE `--resume` to continue from queue DB after interruption
- USE `--retry-failed` to retry only failed files
- NEVER use `fd | xargs remember`; use `ingest` instead
- NEVER mix `--body`, `--body-file`, `--body-stdin`, `--graph-stdin` in single invocation
- NEVER pass empty body with no entities via `--graph-stdin` (exit 1 since v1.0.54)
- NEVER use `--force-merge` in `ingest` (exclusive to `remember`)
- NEVER mix different memory types in same `ingest` invocation


## CRUD — Read, History, Update
- INVOKE `read --name <kebab>` for O(1) fetch by name
- INVOKE `read --id <N>` for direct lookup by memory_id
- INVOKE `read --with-graph` to include linked entities and relationships
- PARSE fields `body`, `description`, `created_at_iso`, `updated_at_iso`
- TREAT exit code 4 as memory not found in namespace
- EXPECT v1.0.85 G55 bilingual message: `--lang en` emits `Memory not found`, `--lang pt` emits `Memória não encontrada`
- INVOKE `list --type <kind> --limit N` to filter by memory type
- USE `--offset N` to paginate large datasets
- USE `--include-deleted` to include soft-deleted memories
- EXPECT `list` response: `items[]`, `total_count`, `truncated`, `body_length`, `elapsed_ms`
- INVOKE `history --name <n>` to list versions in reverse chronological order
- USE `--diff` to include character diff stats between versions
- EXPECT `versions[]`: `version`, `created_at_iso`, `body_length`, `deleted?`, `changes?`
- INVOKE `edit --name <n> --body-file <path>` to update body from file
- USE `--description <text>` to update description only
- USE `--type <kind>` to change memory type without recreating (v1.0.66)
- USE `--force-reembed` to regenerate embedding without body change (v1.0.79)
- USE `--llm-parallelism N` on `edit` (default 4, clamp [1, 32])
- USE `--expected-updated-at <ts>` for optimistic locking
- TREAT exit code 3 as optimistic lock conflict; reload `read --json` and retry
- INVOKE `rename --from <old> --to <new>` to rename preserving history
- TREAT exit 1 when new name equals old name (v1.0.64)
- INVOKE `restore --name <n> --version <N>` to restore old version
- OMIT `--version` to select last non-restore version automatically
- EXPECT each `edit` or `restore` to create new immutable version
- EXPECT FTS5 desync fix applied (v1.0.56) so edited memories are immediately findable
- NEVER skip optimistic locking in concurrent pipelines


## CRUD — Delete (forget, purge, unlink, prune, cleanup)
- INVOKE `forget --name <n>` for reversible soft-delete
- EXPECT `forget` to disappear from `recall` and `list` outputs
- TREAT exit 4 as memory absent (since v1.0.52)
- INVOKE `restore` to reverse soft-delete before any purge
- INVOKE `purge --retention-days <N> --yes` for hard delete
- USE `--dry-run` first to audit count
- EXPECT default retention 90 days for soft-deleted memories
- INVOKE `unlink --from <a> --to <b> --relation <type>` for targeted edge removal
- OMIT `--relation` to remove all edges between `--from` and `--to`
- USE `--entity <name> --all` to bulk-remove all relationships for entity
- TREAT exit code 4 as nonexistent edge
- INVOKE `prune-relations --relation <type> --yes` for bulk relationship deletion
- USE `--show-entities` with `--dry-run` to list affected entity names
- INVOKE `cleanup-orphans --dry-run` to audit orphaned entities
- APPLY `--yes` in automated pipelines for `cleanup-orphans`
- INVOKE `prune-ner --entity <n>` to remove NER bindings for specific entity
- INVOKE `prune-ner --all --yes` to remove all NER bindings in namespace
- USE standard pipeline: bulk `forget` then `cleanup-orphans --yes` then `vacuum --json`
- NEVER delete manually via `sqlite3` shell; use binary commands only


## Entity Graph (link, graph, memory-entities, rename, delete, merge, reclassify, normalize)
- INVOKE `link --from <a> --to <b> --relation <type>` to create edge
- PASS `--create-missing` to auto-create nonexistent entities during link
- PASS `--entity-type <kind>` for auto-created entities (default `concept`)
- PASS `--weight <float>` for edge weight (default 0.5)
- USE `--strict-relations` to fail on non-canonical relation types
- USE `--max-entity-degree N` to warn when entity exceeds N connections
- INVOKE `graph entities --json` to list all entities
- ACCESS via `.entities[]` (field is `entities` NOT `items`)
- FILTER via `--entity-type <kind>`
- SORT via `--sort-by degree|name|created_at` (default `name`)
- SET direction via `--order asc|desc` (default `asc`)
- PAGINATE via `--limit N --offset N`
- INVOKE `graph stats --json` to inspect `node_count`, `edge_count`, `avg_degree`, `max_degree`
- INVOKE `graph traverse --from <root> --depth <N>` for subgraph traversal
- EXPECT `hops[]`: `entity`, `relation`, `direction`, `weight`, `depth`
- TREAT exit 4 as nonexistent root entity
- USE `--format json|dot|mermaid` with `--output <path>` to export graph
- INVOKE `memory-entities --name <memory>` for forward entity lookup
- INVOKE `memory-entities --entity <name>` for reverse memory lookup
- INVOKE `rename-entity --name <old> --new-name <new>` to rename entity
- TREAT exit 4 as entity not found
- TREAT exit 1 if new name fails validation
- INVOKE `delete-entity --name <n> --cascade` to remove entity and all bindings
- PASS `--cascade` is REQUIRED when entity has relationships (else exit 1)
- INVOKE `merge-entities --names "a,b,c" --into <target>` to merge entities
- INVOKE `reclassify --name <n> --new-type <kind>` for single entity reclassification
- INVOKE `reclassify --from-type <old> --to-type <new> --batch` for bulk reclassification
- INVOKE `reclassify-relation --from-relation <old> --to-relation <new> --batch`
- INVOKE `normalize-entities --yes` to normalize all names to kebab-case ASCII
- VALIDATE names: minimum 2 chars, no newlines, no short ALL_CAPS
- NORMALIZE names via NFKD then ASCII then lowercase then hyphens
- CANONICAL relations: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- NON-CANONICAL mapping: `adds|creates → causes`, `implements → supports`, `blocks → contradicts`, `tested-by → related`, `part-of → applies-to`
- CANONICAL entity types: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- NEVER use `mentions` as default relation (adds noise)
- NEVER persist ephemeral state in entities


## GraphRAG Search (recall, hybrid-search, related, deep-research, enrich)
- USE canonical three-layer pattern: `hybrid-search` then `read --name` then `related|graph traverse`
- INVOKE `recall <query> --k N` for pure semantic KNN search
- PASS `--no-graph` to disable automatic graph expansion
- INTERPRET `distance` increasing as similarity decreasing
- INTERPRET `score` as `1.0 - distance` clamped to `[0.0, 1.0]`
- EXPECT `source ∈ {direct, graph}` and `graph_depth` for graph results
- EXPECT response: `direct_matches[]`, `graph_matches[]`, `results[]`, `elapsed_ms`
- INVOKE `hybrid-search <query> --k N` for FTS5 and KNN fusion via RRF
- PASS `--rrf-k 60` for standard RRF fusion constant
- PASS `--weight-vec 1.0` and `--weight-fts 1.0` for balanced fusion
- USE `--with-graph --max-hops 2 --min-weight 0.3` for graph expansion
- EXPECT `hybrid-search` response (v1.0.84+): `results[]`, `graph_matches[]`, `fts_degraded`, `vec_degraded_reason?`, `backend_invoked`, `elapsed_ms`
- READ BOTH `results[]` AND `graph_matches[]` when `--with-graph` active
- INVOKE `related <name> --hops N` for multi-hop traversal from memory
- PASS `--relation <type>` to filter traversal by relation
- EXPECT `hop_distance` explicit per hop
- INVOKE `deep-research "<query>" --k 20` for parallel multi-hop research
- PASS `--max-sub-queries 7` to cap query decomposition
- PASS `--max-hops 3 --min-weight 0.3 --max-results 50` for graph traversal
- PASS `--with-bodies` to include full memory bodies in results
- EXPECT response: `sub_queries[]`, `results[]`, `evidence_chains[]`, `graph_context?`, `stats`
- INVOKE `enrich --operation <op> --mode claude-code` for LLM graph quality
- OPERATIONS: `memory-bindings`, `entity-descriptions`, `body-enrich` (Jaccard >=0.7), `re-embed --limit N --resume`
- PASS `--llm-parallelism N` to control concurrent LLM subprocesses
- PASS `--max-cost-usd N` to cap cumulative spend (ignored for OAuth users)
- PASS `--resume` and `--retry-failed` for crash resilience
- USE `--dry-run` to preview without spawning LLM
- USE BROAD query for `recall --k 5`
- USE MIXED token query for `hybrid-search --k 10`
- USE MIXED with graph for `hybrid-search --with-graph --max-hops 2`
- USE EXPLORATORY from memory for `related --hops 2`
- USE EXPLORATORY from entity for `graph traverse --depth 2`
- NEVER confuse `distance` with `combined_score` in ranking
- NEVER increase `--hops` without inspecting `graph stats` first
- NEVER skip layer 2 when snippet is insufficient
- NEVER read only `.results[]` when `--with-graph` is active


## JSON Contracts (Top-5 Fields per Command)
- `recall` top fields: `results[].name`, `snippet`, `distance`, `score`, `source`
- `hybrid-search` top fields: `results[].name`, `combined_score`, `vec_rank`, `fts_rank`, `source`
- `health` top fields: `integrity_ok`, `schema_ok`, `counts`, `wal_size_mb`, `schema_version`
- `list` top fields: `items[].name`, `type`, `description`, `updated_at_iso`, `deleted_at_iso?`
- `edit` top fields: `memory_id`, `name`, `action`, `version`, `elapsed_ms`
- `read` top fields: `name`, `body`, `description`, `created_at_iso`, `updated_at_iso`
- `forget` top fields: `action`, `forgotten`, `name`, `namespace`, `elapsed_ms`
- `link` top fields: `action`, `from`, `to`, `relation`, `weight`
- `graph entities` top fields: `entities[].id`, `name`, `entity_type`, `degree`, `description?`
- `deep-research` top fields: `sub_queries[]`, `results[]`, `evidence_chains[]`, `graph_context`, `stats`
- `enrich` NDJSON events: `phase`, `name`, `status`, `entities?`, `rels?`, `cost_usd?`, `elapsed_ms?`
- `pending list` top fields: `id`, `name`, `status`, `created_at`, `namespace`
- `slots status` top fields: `max_concurrency`, `acquired`, `waiting`, `held_by_pid[]`
- `embedding status` top fields: `pending`, `processing`, `done`, `failed`, `skipped`
- `recall` top fields (v1.0.84+): add `backend_invoked`, `vec_degraded_reason?`
- `hybrid-search` top fields (v1.0.84+): add `backend_invoked`, `vec_degraded_reason?`
- `remember`/`edit`/`ingest`/`enrich`/`read` envelopes (v1.0.84+): include `backend_invoked`
- ALL schemas use `"additionalProperties": false` (SemVer-versioned JSON API)
- FULL schemas in `docs/schemas/*.schema.json` (never inline full schema in skill)


## Exit Codes and Retry
- EXIT 0 means success; parse stdout
- EXIT 1 means validation error (invalid weight, self-link, max-files exceeded)
- EXIT 2 means Clap argument parsing error
- EXIT 3 means optimistic lock conflict; reload `read --json` and retry
- EXIT 4 means entity, memory, or version not found
- EXIT 5 means namespace error
- EXIT 6 means payload above size limit
- EXIT 9 means duplicate memory (use `--force-merge` to update or restore)
- EXIT 10 means database error; run `vacuum` and `health`
- EXIT 11 means embedding failure (LLM subprocess error)
- EXIT 13 means partial batch failure; reprocess only failed
- EXIT 14 means I/O error (permission, disk full)
- EXIT 15 means database busy; widen `--wait-lock`
- EXIT 19 means SHUTDOWN_EXIT_CODE (ADR-0037); partial work discarded; RETRY MANDATORY
- EXIT 19 envelope: `{error:true, code:19, signal, graceful, message}`
- EXIT 20 means internal error or JSON serialization failure
- EXIT 75 means slots exhausted OR `JobSingletonLocked`
- EXIT 75 from `enrich`/`ingest --mode claude-code|codex`: parse `job '(\w+)'.*namespace '(\w+)'`
- EXIT 75 v1.0.85 GAP-003 circuit breaker: respect per-namespace cooldown window; do NOT retry immediately
- EXIT 77 means RAM pressure; wait for free memory
- NEVER ignore non-zero exit code as success
- NEVER reprocess entire batch after exit 13
- NEVER increase concurrency after exit 75 or 77
- NEVER confuse exit 1 (validation) with exit 9 (duplicate)


## Concurrency, RAM, Parallelism, Slots
- RESPECT hard ceiling `2 × nCPUs` for heavy commands
- TREAT as heavy: `init`, `remember`, `ingest`, `recall`, `hybrid-search`
- DISTINGUISH `--max-concurrency` (CLI fan-out) from `--ingest-parallelism` (per-file)
- SET `--llm-parallelism N` default 4 on `remember`/`edit`, default 2 on `ingest`
- CLAMP `--llm-parallelism` in range `[1, 32]`
- USE `--llm-max-host-concurrency N` to cap cross-process LLM subprocesses
- USE `--llm-slot-wait-secs N` to wait for slot or `--llm-slot-no-wait` to abort
- WIDEN `--wait-lock SECS` when contention is expected
- ENABLE `SQLITE_GRAPHRAG_LOW_MEMORY=1` for unitary parallelism (3-4x slower)
- USE `--strict-env-clear` (ADR-0041) to preserve only `PATH` for compliance
- SHUTDOWN bypass recipe: prepend `tests/mock-llm` to PATH then set `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1` then wrap with `setsid -w timeout`
- JOB SINGLETON: `enrich`, `ingest --mode claude-code`, `ingest --mode codex` acquire per-namespace singleton
- USE `--wait-job-singleton SECS` to wait for lock or `--force-job-singleton` to break stale lock
- LIMIT parallel ingestion in CI to avoid LLM rate limits
- NEVER run `enrich` in parallel against same database


## v1.0.82+ Surface (pending, slots, embedding, llm-backend, shutdown, v1.0.84/85 fields)
- INVOKE `pending list --filter-status queued` to inspect three-stage remember checkpoint queue
- INVOKE `pending show <id>` to inspect single checkpoint row
- INVOKE `pending cleanup --yes` to remove terminal-state rows
- BACKED by `pending_memories` table created by migration V014 (ADR-0036)
- INVOKE `pending-embeddings list` to inspect retry queue for failed embeddings
- INVOKE `pending-embeddings process` to reprocess with next backend
- BACKED by `pending_embeddings` table created by migration V015 (ADR-0040)
- INVOKE `slots status` to inspect host-wide slot semaphore
- INVOKE `slots release --slot-id <N> --yes` to reap orphan slots
- LOCK via `fs4 = "0.9"` with `fcntl(F_SETLK)` on Unix and `LockFileEx` on Windows (ADR-0039)
- INVOKE `embedding status` for aggregate per-status counts
- INVOKE `embedding list` for per-entry inspection
- PASS `--llm-backend codex,claude` for codex-first with claude fallback (ADR-0038)
- PASS `--llm-backend codex,claude,none` for null embedding fallback
- DEFAULT `--llm-backend` is `codex`
- PASS `--llm-fallback-mode <claude|codex>` to swap backend mid-job on rate-limit
- EXPECT v1.0.85 G58 deterministic fallback when alt backend listed in `--llm-backend codex,claude`
- PASS `--graceful-shutdown-secs N` to reserve cleanup budget before SIGKILL
- PASS `--skip-embedding-on-failure` only when `--llm-backend …,none`
- PASS ADR-0041 `--strict-env-clear` to drop custom-provider credentials in subprocess
- RUN `codex login` after upgrade to refresh OAuth refresh token (2026-06-14 incident)
- OPERATOR action for stale OAuth: `codex login` then retry
- v1.0.84: PASS `--dry-run-backend` to plan backend operation without executing it (idempotent preview)
- v1.0.84: PARSE `backend_invoked` field in recall, hybrid-search, remember, edit, ingest, enrich, read envelopes to confirm effective backend
- v1.0.84: READ `vec_degraded_reason` in recall/hybrid-search envelopes when vec path is degraded
- v1.0.84: KNOW claude backend splits into local embedder via `embed_via_claude_local` (zero-token, OAuth-compatible)
- v1.0.84: USE `LlmEmbeddingBuilder` to compose embedding pipeline: `with_backend(Codex).or_fallback(Claude).or_skip()`
- v1.0.85 GAP-003: RESPECT slot exhaustion circuit breaker; on exit 75, backoff per-namespace cooldown before retry
- v1.0.85 G58: EXPECT deterministic OAuth quota fallback when alt backend declared in `--llm-backend` list
- v1.0.85 G45-CR5: CAPTURE `anthropic-ratelimit-requests-remaining`, `anthropic-ratelimit-tokens-remaining`, `anthropic-ratelimit-input-tokens-reset`, `anthropic-ratelimit-output-tokens-reset` from response headers in envelope
- v1.0.85 G55: EXPECT bilingual NotFound from `read --name <missing>` based on `--lang`: EN emits `Memory not found`, PT emits `Memória não encontrada`
- v1.0.85 G56: DEFAULT embedding dim is 64 (MRL) when `SQLITE_GRAPHRAG_EMBEDDING_DIM` unset and `schema_meta.dim` absent
- v1.0.85.1: KNOW `recall --llm-backend none` and `hybrid-search --llm-backend none` return exit 0 with `vec_degraded_reason: "dim_zero"` (GAP-004 hotfix)
- v1.0.85.2: USE `--dry-run-backend` standalone without a subcommand (BUG-001); `setup_mock_path()` test mock emits proper JSON for claude and JSONL for codex (BUG-002); the `backend_invoked` field in 7 envelopes reflects the RESOLVED backend (BUG-003)


## Maintenance (fts, backup, vacuum, optimize, migrate, export, debug-schema, vec, completions)
- INVOKE `fts rebuild --json` to fully rebuild FTS5 full-text index
- INVOKE `fts check --json` to run FTS5 integrity check
- INVOKE `fts stats --json` to inspect FTS5 health (`total_rows`, `fts_functional`)
- INVOKE `optimize --fts-dry-run` to preview FTS5 rebuild
- INVOKE `optimize --fts-progress N` to print progress every N seconds
- PASS `--no-fts-skip-when-functional` to force FTS5 rebuild even when healthy
- INVOKE `backup --output <path> --json` for safe online backup via SQLite API
- INVOKE `sync-safe-copy --dest <path>` for atomic snapshot before critical operations
- INVOKE `export --namespace <ns> --type <kind> --json` to export memories as NDJSON
- INVOKE `vacuum --json` after large purge to reclaim space
- INVOKE `migrate --rehash --json` to repair V002 checksum drift
- INVOKE `migrate --to-llm-only --drop-vec-tables --json` for v1.0.74/75 upgrades
- INVOKE `debug-schema --json` (hidden from `--help`) to inspect schema state
- INVOKE `completions <bash|zsh|fish|elvish|powershell>` to generate shell completions
- INVOKE `vec orphan-list --json` to list orphaned memory vectors
- INVOKE `vec purge-orphan --yes --dry-run` to PREVIEW purge
- INVOKE `vec purge-orphan --yes` to PERMANENTLY purge orphans
- INVOKE `vec stats --json` to inspect vec table health
- SCHEDULE weekly: `purge --retention-days 30 --yes` then `cleanup-orphans --yes` then `prune-relations --relation mentions --yes` then `vacuum --json` then `optimize --json` then `sync-safe-copy --dest ~/backups/`
- SINCE v1.0.53 every write runs `PRAGMA wal_checkpoint(TRUNCATE)` after commit
- IF corruption occurs despite checkpoint: `sqlite3 broken.sqlite ".recover" | sqlite3 repaired.sqlite`


## Active Rules and Anti-patterns Summary
- NEVER pass `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` (OAuth-only, exit 1)
- NEVER depend on daemon or use `--bare` flag (REMOVED v1.0.76 and v1.0.79)
- NEVER install with `--features embedding-legacy` or `--features ner-legacy` (REMOVED)
- NEVER use `fastembed`, `tokenizers`, `sqlite-vec`, or `GLiNER` crates
- NEVER expect sqlite-vec KNN; cosine is pure Rust in `src/similarity.rs`
- NEVER run `enrich` in parallel against same database (job singleton via `lock::acquire_job_singleton`)
- NEVER write to `.sqlite` file outside the binary
- NEVER ignore exit 19 (SHUTDOWN_EXIT_CODE envelope); partial work discarded, RETRY MANDATORY
- NEVER duplicate content already in `CHANGELOG.md`
- NEVER use `mentions` as default graph relation
- NEVER pass empty body via `--graph-stdin` (exit 1 since v1.0.54)
- NEVER use `--gliner-variant` (no-op since v1.0.79)
- NEVER call `migrate --to-llm-only` without `--drop-vec-tables` safety guard
- NEVER ignore `--wait-lock` flag when contention is expected
- NEVER assume exit 1 equals exit 9 (validation vs duplicate)
