---
name: sqlite-graphrag
description: This skill MUST activate for sqlite-graphrag CLI operations including persistent memory, GraphRAG, entity graph, hybrid search, recall, remember, ingest, enrich, deep-research, LLM embedding, backend selection (codex claude opencode), OAuth-only enforcement, preflight validation, FTS5, BLOB-backed cosine similarity, CWD isolation, namespace management, migration, maintenance. Activates on keywords memory RAG GraphRAG SQLite one-shot OAuth offline persistent graph entity embedding codex claude opencode remember recall hybrid-search ingest enrich forget purge link
---


## When This Skill Activates
- ACTIVATE when the user asks to remember, save, recall, retrieve, search, or persist anything across sessions
- ACTIVATE for long-term context, knowledge graph, GraphRAG, RAG, entity linking, memory management
- ACTIVATE when sqlite, sqlite-graphrag, embedding, FTS5, hybrid-search, or LLM memory is mentioned
- NEVER ACTIVATE for one-off ephemeral data, simple file I/O, or tasks unrelated to persistent context


## LLM Prompt Instruction Rules
- WHEN user says "remember this", "save this", "persist this" → EXECUTE `remember --force-merge` with `--graph-stdin` including curated entities and canonical relations
- WHEN user asks "what do you know about X", "recall X", "search X" → EXECUTE `hybrid-search "X" --k 10 --json` FIRST, then EXPAND top results with `read --name <name> --json`
- WHEN user asks "how is X related to Y" → EXECUTE `graph traverse --from X --depth 2 --json` or `related X --hops 2 --json`
- WHEN user asks "deep research on X" → EXECUTE `deep-research "X" --k 20 --max-hops 3 --json`
- BEFORE creating ANY memory → EXECUTE `hybrid-search "<name>" --k 5 --json` to CHECK duplicates; if found, USE `--force-merge` to UPDATE
- AFTER creating or updating memory → VERIFY with `read --name <name> --json | jaq '{name, description, body_length}'`
- AFTER EVERY turn with new findings → EVALUATE whether to persist via `remember --force-merge`; if nothing new, DECLARE "No new findings to persist"
- WHEN exit code is non-zero → READ JSON error envelope from stdout via `jaq '{code, message, error_class}'`, REPORT remediation steps
- WHEN exit code 9 (duplicate) → RETRY with `--force-merge`
- WHEN exit code 19 (SHUTDOWN) → RETRY MANDATORY; partial work discarded
- WHEN exit code 75 (singleton locked) → WAIT and retry; NEVER increase concurrency
- WHEN exit code 16 (preflight) → FIX MCP config; NEVER bypass with `SKIP_PREFLIGHT`
- ALWAYS parse JSON output with `jaq` (NEVER `jq`)
- ALWAYS pass `--json` flag on every `sqlite-graphrag` invocation
- ALWAYS use `--llm-backend codex --llm-model gpt-5.4-mini` OR `--llm-backend claude --llm-model claude-sonnet-4-6` OR `--llm-backend opencode --llm-model opencode/big-pickle` for embedding commands
- ALWAYS use canonical relations ONLY: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- ALWAYS map non-canonical relations BEFORE persisting: `adds|creates → causes`, `implements → supports`, `blocks → contradicts`, `tested-by → related`, `part-of → applies-to`
- ALWAYS normalize entity names to kebab-case ASCII lowercase BEFORE passing to CLI
- NEVER use MCP Serena or `.md` memory files for persistence
- NEVER write MEMORY.md or any file-based memory
- NEVER start or reference daemon (REMOVED)
- NEVER pass `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` in environment
- PREFER `remember --force-merge` over `edit` for updates to ensure re-indexation
- PREFER `--graph-stdin` with curated entities over `--enable-ner` for extraction quality
- LIMIT graph entities to domain-specific concepts: projects, tools, people, decisions, files, incidents
- REJECT generic words, pronouns, UUIDs, hashes, timestamps as entity names


## Architecture and Principles
- INVOKE always as subprocess; READ stdout for JSON/NDJSON; READ stderr for logs; CHECK exit code BEFORE parsing
- KNOW BUILD is LLM-only one-shot; binary has NO daemon, NO ONNX runtime, NO model cache
- KNOW COSINE similarity is pure Rust over BLOB-backed `memory_embeddings`, `entity_embeddings`, `chunk_embeddings`
- KNOW SCHEMA is v15 after `init` or `migrate` on fresh database
- ENFORCE OAUTH-ONLY: spawn ABORTS exit 1 if `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` is set
- KNOW `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL` are PRESERVED for custom providers (OpenRouter, Bedrock)
- KNOW hardening flags are ALWAYS passed to `claude -p` and `codex exec` subprocesses
- KNOW subprocess CWD is ISOLATED to temp dir via `apply_cwd_isolation`; `CLAUDE_CONFIG_DIR` set to isolation dir
- KNOW orphan spawn directories are cleaned via `cleanup_isolation_dirs`
- KNOW 7 preflight guards run BEFORE every LLM subprocess fork: `check_argv_size`, `check_binary_exists`, `check_mcp_config_inline`, `check_mcp_config_path`, `check_walkup_mcp_json`, `check_output_buffer`, `check_claude_config_dir`
- KNOW exit code 16 (`EX_CONFIG`) is the universal preflight failure code; READ error envelope for variant-specific remediation
- SET `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` ONLY in emergencies
- ISOLATE NAMESPACE per project via `--namespace <ns>` or env; default is `global`
- NEVER expose the binary as MCP server or HTTP service
- NEVER write `.sqlite` file in parallel to the binary or from another tool
- USE MOCK LLM CLI for CI: prepend `tests/mock-llm` to PATH


## Backend LLM Selection
- PASS `--llm-backend codex` to spawn Codex CLI headless (DEFAULT backend)
- PASS `--llm-backend claude` to spawn Claude Code headless via `embed_via_claude_local` (zero-token, OAuth-compatible)
- PASS `--llm-backend opencode` to spawn OpenCode CLI headless (own auth system, NOT OAuth)
- PASS `--llm-backend codex,claude` for codex-first with claude fallback
- PASS `--llm-backend codex,claude,opencode,none` for full fallback chain with null embedding last resort
- PASS `--llm-model <MODEL>` to select embedding model for the active backend
- KNOW DEFAULT models: codex=`gpt-5.5`, claude=`claude-sonnet-4-6`, opencode=`opencode/big-pickle`
- PASS `--llm-fallback-mode <claude|codex|opencode>` to swap backend mid-job on rate-limit
- PASS `--skip-embedding-on-failure` ONLY when `--llm-backend …,none` is active
- PASS `--dry-run-backend` to plan backend operation without executing (idempotent preview)
- PARSE `backend_invoked` field in every embedding envelope to CONFIRM which backend ran
- PASS `--codex-binary <PATH>`, `--claude-binary <PATH>`, `--opencode-binary <PATH>` to override binary locations
- PASS `--opencode-model <MODEL>` and `--opencode-timeout <SECONDS>` for opencode-specific tuning
- PASS `--mode codex|claude-code|opencode` for ingest and enrich extraction pipelines
- KNOW opencode NDJSON output has 3 event types: `step_start`, `text`, `step_finish`
- KNOW opencode free models: `opencode/big-pickle`, `opencode/deepseek-v4-flash-free`, `opencode/mimo-v2.5-free`, `opencode/nemotron-3-ultra-free`, `opencode/north-mini-code-free`
- RUN `codex login` to refresh codex OAuth; refresh claude OAuth when stale
- NEVER pass API keys with any backend; spawn ABORTS exit 1


## Global Flags Reference
- `--db <PATH>` — override database location (NOT global; each subcommand accepts independently)
- `--namespace <ns>` — scope operations to a namespace
- `--lang en|pt` — force stderr language
- `--tz <TIMEZONE>` — localize timestamps
- `--json` — structured JSON output (ALWAYS pass)
- `--low-memory` — unitary parallelism for constrained containers
- `--max-concurrency N` — cap concurrent heavy CLI invocations
- `--wait-lock SECS` — widen lock acquisition window
- `--llm-parallelism N` — cap embedding subprocess fan-out (default 4, clamp [1, 32])
- `--llm-backend <chain>` — backend selection with comma-separated fallback
- `--llm-model <MODEL>` — embedding model for active backend
- `--dry-run-backend` — plan backend operation without executing
- `--llm-fallback-mode <backend>` — swap backend mid-job on rate-limit
- `--llm-fallback <chain>` — comma-separated fallback chain tried when primary fails (default `codex,claude,none`)
- `--llm-slot-no-wait` — fail immediately exit 75 when no LLM slot free (instead of waiting)
- `--embedding-dim N` — embedding dimensionality override [8, 4096] (default 64 MRL)
- `--graceful-shutdown-secs N` — cleanup budget before SIGKILL
- `--skip-embedding-on-failure` — exit 0 on embedding failure (ONLY with fallback ending in `none`)
- `--strict-env-clear` — preserve only `PATH` in subprocess for compliance
- `--codex-binary`, `--claude-binary`, `--opencode-binary` — override binary paths
- `--opencode-model`, `--opencode-timeout` — opencode-specific overrides
- `-v`/`-vv`/`-vvv` — info/debug/trace logging on stderr


## CRUD Write Path (remember, remember-batch, ingest)
- INVOKE `remember --name <kebab> --type <kind> --description <text>` with `--body <text>` or `--body-file <path>` or `--body-stdin`
- INVOKE `remember --graph-stdin` to attach `{body, entities, relationships}` in single JSON
- PASS entities as `[{name, entity_type}]` in kebab-case ASCII
- PASS relationships as `[{source, target, relation, strength}]` where `strength in [0.0, 1.0]`
- PASS `--force-merge` for idempotent updates and soft-deleted restoration
- PASS `--clear-body` to wipe body during `--force-merge` update
- PASS `--dry-run` to validate inputs without persisting
- PASS `--max-rss-mb <MiB>` to abort when RSS exceeds threshold (default 8192)
- RESPECT 512000 bytes and 512 chunks limit per body
- VALID `--type` values: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- USE `--enable-ner` for URL-regex entity extraction (URL-regex ONLY since NER removal)
- INVOKE `remember-batch` for 10+ memories via NDJSON stdin; EXPECT per-item status and summary line
- INVOKE `ingest <DIR> --recursive --pattern "*.md"` to import directory
- PASS `--type <kind>` to apply same type to all ingested files
- PASS `--mode codex|claude-code|opencode` for LLM-curated entity extraction
- USE `--auto-describe` (default true) to extract description from first body line; opt out via `--no-auto-describe`
- USE `--resume` to continue from queue after interruption; `--retry-failed` for failed files only
- USE `--fail-fast` to stop at first per-file failure
- USE `--max-name-length N` to override default name truncation at 60 chars
- USE `--llm-parallelism N` on `ingest` (default 2); `--ingest-parallelism N` for per-file parallelism
- PASS `--claude-model <MODEL>` and `--claude-timeout <secs>` (default 300) for `--mode claude-code`
- PASS `--codex-model <MODEL>` and `--codex-timeout <secs>` (default 300) for `--mode codex`
- PASS `--rate-limit-wait <secs>` (default 60) for initial wait on rate-limit with `--mode claude-code`
- PASS `--queue-db <path>` for custom queue DB; `--keep-queue` to preserve it after completion
- PASS `--low-memory` on `ingest` for single-threaded mode (3-4x slower, <4 GB RAM)
- PASS `--dry-run` on `ingest` to preview file-to-name mapping without persisting
- RESPECT `--max-files 10000` cap as all-or-nothing validation
- NEVER mix `--body`, `--body-file`, `--body-stdin`, `--graph-stdin` in single invocation
- NEVER pass empty body with no entities via `--graph-stdin`
- NEVER use `fd | xargs remember`; INVOKE `ingest` instead
- NEVER use `--force-merge` in `ingest` (exclusive to `remember`)


## CRUD Read, Update, Delete
- INVOKE `read --name <kebab> --json` for O(1) fetch; `read --id <N>` for lookup by memory_id
- PASS `--with-graph` to include linked entities and relationships
- INVOKE `list --type <kind> --limit N --offset N --json` to filter and paginate
- PASS `--include-deleted` to include soft-deleted memories
- INVOKE `history --name <n> --diff --json` for version history with character diff stats
- INVOKE `edit --name <n> --body-file <path>` to update body (re-embeds automatically)
- USE `--description <text>` to update description only (no re-embed)
- USE `--type <kind>` to change memory type without recreating
- USE `--force-reembed` to regenerate embedding without body change
- USE `--expected-updated-at <ts>` for optimistic locking; TREAT exit 3 as conflict
- INVOKE `rename --from <old> --to <new>` to rename preserving history
- INVOKE `restore --name <n> --version <N>` to restore old version
- INVOKE `forget --name <n>` for reversible soft-delete; TREAT exit 4 as absent
- INVOKE `purge --retention-days <N> --yes` for hard delete; USE `--dry-run` first
- INVOKE `unlink --from <a> --to <b> --relation <type>` for edge removal; `--entity <name> --all` for bulk
- INVOKE `prune-relations --relation <type> --yes` for bulk deletion; `--show-entities --dry-run` to preview
- INVOKE `cleanup-orphans --yes` after bulk forget; then `vacuum --json`
- NEVER skip optimistic locking in concurrent pipelines
- NEVER delete manually via `sqlite3` shell


## Entity Graph Operations
- INVOKE `link --from <a> --to <b> --relation <type> --create-missing --weight <float>` to create edge
- PASS `--entity-type <kind>` for auto-created entities (default `concept`)
- PASS `--max-entity-degree N` to warn when entity exceeds N connections
- USE `--strict-relations` to fail on non-canonical relation types
- INVOKE `graph entities --json` to list entities; ACCESS via `.entities[]` (NOT `.items[]`)
- SORT via `--sort-by degree|name|created_at`; PAGINATE via `--limit N --offset N`
- INVOKE `graph stats --json` to inspect `node_count`, `edge_count`, `avg_degree`, `max_degree`
- KNOW entity degree is calculated via accurate COUNT query (`recalculate_degree`)
- INVOKE `graph traverse --from <root> --depth <N> --json` for subgraph traversal
- USE `--format json|dot|mermaid` with `--output <path>` to export graph
- INVOKE `memory-entities --name <memory>` for forward lookup; `--entity <name>` for reverse
- INVOKE `rename-entity`, `delete-entity --cascade`, `merge-entities --names "a,b,c" --into <target>`
- INVOKE `reclassify --name <n> --new-type <kind>` or `--from-type <old> --to-type <new> --batch`
- INVOKE `reclassify-relation --from-relation <old> --to-relation <new> --batch` for bulk relation type migration
- INVOKE `normalize-entities --yes` to normalize all names to kebab-case ASCII
- INVOKE `prune-ner --entity <n>` to remove NER bindings; `prune-ner --all --yes` for all in namespace
- VALIDATE entity names: minimum 2 chars, no newlines, no short ALL_CAPS (4 chars or less REJECTED)
- CANONICAL relations: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- CANONICAL entity types: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- NEVER use `mentions` as default relation


## GraphRAG Search (recall, hybrid-search, related, deep-research, enrich)
- USE canonical three-layer pattern: `hybrid-search` then `read --name` then `related|graph traverse`
- INVOKE `recall <query> --k N` for pure semantic KNN; PASS `--no-graph` to disable graph expansion
- INTERPRET `distance` increasing as similarity decreasing; `score` = `1.0 - distance` clamped [0.0, 1.0]
- INVOKE `hybrid-search <query> --k N` for FTS5+KNN fusion via RRF
- PASS `--rrf-k 60` for standard fusion; `--weight-vec 1.0 --weight-fts 1.0` for balanced
- PASS `--type <kind>` to filter results by memory type
- PASS `--fallback-fts-only` to skip live embedding and serve FTS5 BM25 only (offline mode)
- USE `--with-graph --max-hops 2 --min-weight 0.3` for graph expansion; READ BOTH `results[]` AND `graph_matches[]`
- INVOKE `related <name> --hops N` for multi-hop traversal from memory
- INVOKE `deep-research "<query>" --k 20 --max-hops 3 --max-sub-queries 7 --max-results 50` for parallel multi-hop research
- PASS `--graph-decay <float>` (default 0.7) for score decay per hop; `--graph-min-score <float>` (default 0.05) for minimum threshold
- PASS `--max-neighbors-per-hop N` to limit neighbours per entity per hop
- PASS `--timeout <secs>` (default 30) for per sub-query timeout
- PASS `--with-bodies` to include full memory bodies in results
- INVOKE `enrich --operation <op>` for LLM graph quality: `memory-bindings`, `entity-descriptions`, `body-enrich`, `re-embed --limit N --resume`
- PASS `--llm-parallelism N` to control concurrent LLM subprocesses
- PASS `--max-cost-usd N` to cap accumulated LLM cost (ignored for OAuth users)
- USE `--dry-run` to preview without spawning LLM
- PARSE top fields: `recall` returns `results[].{name, snippet, distance, score, source}`; `hybrid-search` returns `results[].{name, combined_score, vec_rank, fts_rank}`
- PARSE `deep-research` returns `sub_queries[]`, `results[]`, `evidence_chains[]`, `graph_context`, `stats`
- NEVER confuse `distance` with `combined_score` in ranking
- NEVER increase `--hops` without inspecting `graph stats` first


## Exit Codes and Retry Strategy
- EXIT 0: success; EXIT 1: validation error; EXIT 2: argument parsing; EXIT 3: optimistic lock conflict (reload and retry)
- EXIT 4: not found; EXIT 5: namespace error; EXIT 6: payload too large; EXIT 9: duplicate (use `--force-merge`)
- EXIT 10: database error (run `vacuum` + `health`); EXIT 11: embedding failure (check backend + OAuth)
- EXIT 13: partial batch failure (reprocess failed only); EXIT 14: I/O error; EXIT 15: database busy (widen `--wait-lock`)
- EXIT 16: preflight failure (fix MCP config, NEVER treat as transient)
- EXIT 19: SHUTDOWN (RETRY MANDATORY, partial work discarded); PARSE envelope `{error, code, signal, graceful, message}`
- EXIT 20: internal error; EXIT 75: slots exhausted or job singleton locked (respect cooldown, NEVER retry immediately)
- EXIT 77: RAM pressure (wait for free memory)
- NEVER ignore non-zero exit; NEVER reprocess full batch after exit 13; NEVER confuse exit 1 with exit 9


## Concurrency and Parallelism
- RESPECT hard ceiling `2 x nCPUs` for heavy commands: `init`, `remember`, `ingest`, `recall`, `hybrid-search`
- SET `--llm-parallelism N` default 4 on `remember`/`edit`, default 2 on `ingest` (clamp [1, 32])
- USE `--llm-max-host-concurrency N` to cap cross-process LLM subprocesses
- USE `--llm-slot-wait-secs N` to wait for slot or `--llm-slot-no-wait` to abort
- KNOW JOB SINGLETON: `enrich`, `ingest --mode claude-code|codex|opencode` acquire per-namespace singleton
- USE `--wait-job-singleton SECS` or `--force-job-singleton` to break stale lock
- ENABLE `SQLITE_GRAPHRAG_LOW_MEMORY=1` for unitary parallelism (3-4x slower)
- NEVER run `enrich` in parallel against same database


## Maintenance and Diagnostic Subcommands
- RUN `sqlite-graphrag init --namespace <ns>` on first use
- RUN `health --json` to verify `integrity_ok`, `schema_ok`, `schema_version >= 15`
- RUN `migrate --dry-run --json` to preview; then `migrate --json` after binary upgrade
- RUN `optimize --json` to refresh planner stats; includes `fts_rebuilt`
- RUN `fts rebuild --json` when `health.fts_degraded` is true; `fts check --json` for integrity; `fts stats --json` for row counts
- INVOKE `backup --output <path> --json` for online backup; `sync-safe-copy --dest <path>` for atomic snapshot
- INVOKE `export --namespace <ns> --type <kind> --json` to export as NDJSON
- INVOKE `vacuum --json` after large purge; INSPECT `wal_size_mb` in health for fragmentation
- INVOKE `vec orphan-list --json` then `vec purge-orphan --yes` to clean orphaned vectors; `vec stats --json` for health
- INVOKE `debug-schema --json` for schema drift troubleshooting
- INVOKE `completions <bash|zsh|fish|elvish|powershell>` for shell completions
- INVOKE `codex-models --json` to inspect codex model whitelist
- INVOKE `stats --json` to show database statistics (counts, sizes, namespace breakdown)
- INVOKE `namespace-detect --json` to resolve namespace precedence for current invocation
- INVOKE `cache list --json` to list cached model files; `cache clear-models --yes` to force re-download
- INVOKE `pending list --filter-status queued --json` to inspect three-stage checkpoint queue; `pending show <id>` for detail; `pending cleanup --yes` for terminal rows
- INVOKE `pending-embeddings list --json` to inspect failed embedding retry queue; `pending-embeddings process --json` to reprocess with next backend
- INVOKE `slots status --json` to inspect host-wide semaphore; `slots release --slot-id <N> --yes` to reap orphan slots
- INVOKE `embedding status --json` for aggregate counts by status; `embedding list --json` for per-entry inspection
- SCHEDULE weekly: `purge` then `cleanup-orphans` then `prune-relations --relation mentions` then `vacuum` then `optimize` then `sync-safe-copy`
- KNOW every write runs `PRAGMA wal_checkpoint(TRUNCATE)` after commit
- IF corruption: `sqlite3 broken.sqlite ".recover" | sqlite3 repaired.sqlite`


## Environment Variables Reference
- `SQLITE_GRAPHRAG_DB_PATH` — persistent database path override
- `SQLITE_GRAPHRAG_NAMESPACE` — persistent namespace
- `SQLITE_GRAPHRAG_LLM_BACKEND` — persistent backend (codex|claude|opencode|none|auto)
- `SQLITE_GRAPHRAG_LLM_MODEL` — persistent model override
- `SQLITE_GRAPHRAG_CODEX_BINARY` / `SQLITE_GRAPHRAG_CODEX_EMBED_MODEL` — codex binary and embed model
- `SQLITE_GRAPHRAG_CLAUDE_BINARY` — claude binary path override
- `SQLITE_GRAPHRAG_OPENCODE_BINARY` / `SQLITE_GRAPHRAG_OPENCODE_MODEL` / `SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL` / `SQLITE_GRAPHRAG_OPENCODE_TIMEOUT` — opencode overrides
- `SQLITE_GRAPHRAG_EMBEDDING_DIM` — embedding dimension [8, 4096] (default 64 MRL)
- `SQLITE_GRAPHRAG_LOW_MEMORY` — enable unitary parallelism
- `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR` — compliance mode
- `SQLITE_GRAPHRAG_DISPLAY_TZ` — persistent timezone
- `SQLITE_GRAPHRAG_LOG_FORMAT` — `json` for log aggregators
- `SQLITE_GRAPHRAG_SKIP_PREFLIGHT` — bypass preflight (EMERGENCIES ONLY)
- `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN` — CI test harnesses ONLY


## Ready-to-Use CLI Formulas
- INIT namespace: `sqlite-graphrag init --namespace <ns>`
- VERIFY health: `sqlite-graphrag health --namespace <ns> --json | jaq '{integrity_ok, schema_version}'`
- MIGRATE preview: `sqlite-graphrag migrate --dry-run --json`
- MIGRATE apply: `sqlite-graphrag migrate --json`
- REMEMBER codex all flags: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini --codex-binary <path> --llm-parallelism 4 remember --name <n> --type decision --description "desc" --body-file doc.md --force-merge --max-rss-mb 4096 --json`
- REMEMBER claude all flags: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 --claude-binary <path> --llm-parallelism 4 remember --name <n> --type decision --description "desc" --body "content" --force-merge --json`
- REMEMBER opencode all flags: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle --opencode-binary <path> --opencode-timeout 300 --llm-parallelism 4 remember --name <n> --type note --description "desc" --body-stdin --json`
- REMEMBER graph-stdin: `echo '{"body":"text","entities":[{"name":"jwt","entity_type":"concept"}],"relationships":[{"source":"jwt","target":"auth-svc","relation":"uses","strength":0.8}]}' | sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini remember --name <n> --type decision --description "desc" --graph-stdin --force-merge --json`
- REMEMBER-BATCH: pipe NDJSON to `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini remember-batch --json`
- FULL FALLBACK CHAIN: `sqlite-graphrag --llm-backend codex,claude,opencode,none --skip-embedding-on-failure remember --name <n> --type note --description "desc" --body-file note.md --json`
- DRY-RUN backend: `sqlite-graphrag --llm-backend codex --dry-run-backend recall "query" --k 5 --json`
- RECALL codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini recall "query" --k 5 --no-graph --json`
- RECALL claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 recall "query" --k 5 --json`
- RECALL opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle recall "query" --k 5 --json`
- HYBRID-SEARCH codex all flags: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini hybrid-search "query" --k 10 --with-graph --max-hops 2 --min-weight 0.3 --rrf-k 60 --weight-vec 1.0 --weight-fts 1.0 --type decision --json`
- HYBRID-SEARCH claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 hybrid-search "query" --k 10 --json`
- HYBRID-SEARCH opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle hybrid-search "query" --k 10 --with-graph --json`
- HYBRID-SEARCH fts-only: `sqlite-graphrag hybrid-search "query" --k 10 --fallback-fts-only --json`
- DEEP-RESEARCH all flags: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 deep-research "question" --k 20 --max-hops 3 --max-sub-queries 7 --max-results 50 --with-bodies --graph-decay 0.7 --graph-min-score 0.05 --timeout 30 --max-neighbors-per-hop 10 --json`
- RELATED: `sqlite-graphrag related <name> --hops 2 --relation uses --json`
- INGEST codex all flags: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini ingest ./docs --mode codex --recursive --pattern "*.md" --type document --auto-describe --resume --max-files 1000 --max-name-length 80 --llm-parallelism 2 --codex-model gpt-5.4-mini --codex-timeout 300 --fail-fast --low-memory --json`
- INGEST claude all flags: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 ingest ./docs --mode claude-code --recursive --pattern "*.md" --type document --auto-describe --resume --claude-model claude-sonnet-4-6 --claude-timeout 600 --rate-limit-wait 60 --max-cost-usd 5 --queue-db .ingest-queue.sqlite --keep-queue --json`
- INGEST opencode all flags: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle ingest ./docs --mode opencode --recursive --pattern "*.md" --type document --auto-describe --opencode-model opencode/big-pickle --opencode-timeout 600 --json`
- INGEST dry-run: `sqlite-graphrag ingest ./docs --dry-run --pattern "*.md" --recursive --json`
- ENRICH re-embed codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation re-embed --limit 100 --resume --llm-parallelism 4 --json`
- ENRICH memory-bindings claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --max-cost-usd 5 --json`
- ENRICH opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation entity-descriptions --mode opencode --dry-run --json`
- READ with graph: `sqlite-graphrag read --name <n> --with-graph --json`
- READ by id: `sqlite-graphrag read --id 42 --json`
- LIST: `sqlite-graphrag list --type decision --limit 50 --offset 0 --include-deleted --json`
- HISTORY: `sqlite-graphrag history --name <n> --diff --json`
- EDIT body codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini edit --name <n> --body-file new.md --expected-updated-at "2026-01-01T00:00:00Z" --json`
- EDIT description only: `sqlite-graphrag edit --name <n> --description "new desc" --json`
- EDIT force-reembed: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini edit --name <n> --force-reembed --json`
- RENAME: `sqlite-graphrag rename --from <old> --to <new> --json`
- RESTORE: `sqlite-graphrag restore --name <n> --version 2 --json`
- FORGET: `sqlite-graphrag forget --name <n> --json`
- PURGE preview: `sqlite-graphrag purge --retention-days 30 --yes --dry-run --json`
- LINK all flags: `sqlite-graphrag link --from <a> --to <b> --relation uses --weight 0.8 --create-missing --entity-type tool --strict-relations --max-entity-degree 50 --json`
- UNLINK: `sqlite-graphrag unlink --from <a> --to <b> --relation uses --json`
- UNLINK bulk: `sqlite-graphrag unlink --entity <name> --all --json`
- GRAPH stats: `sqlite-graphrag graph stats --json | jaq '{node_count, edge_count, avg_degree}'`
- GRAPH entities: `sqlite-graphrag graph entities --sort-by degree --order desc --limit 20 --json`
- GRAPH traverse: `sqlite-graphrag graph traverse --from <entity> --depth 2 --json`
- GRAPH export: `sqlite-graphrag graph --format <json|dot|mermaid> --output <path>`
- MERGE entities: `sqlite-graphrag merge-entities --names "a,b,c" --into target --json`
- NORMALIZE entities: `sqlite-graphrag normalize-entities --yes --json`
- RECLASSIFY entity: `sqlite-graphrag reclassify --name <n> --new-type concept --json`
- RECLASSIFY batch: `sqlite-graphrag reclassify --from-type tool --to-type concept --batch --json`
- RECLASSIFY-RELATION: `sqlite-graphrag reclassify-relation --from-relation <old> --to-relation <new> --batch --json`
- PRUNE-NER: `sqlite-graphrag prune-ner --entity <n>` or `prune-ner --all --yes`
- PRUNE-RELATIONS preview: `sqlite-graphrag prune-relations --relation mentions --yes --show-entities --dry-run`
- CLEANUP pipeline: INVOKE `forget --name <n>` then `cleanup-orphans --yes --json` then `vacuum --json`
- PENDING list: `sqlite-graphrag pending list --filter-status queued --json`
- PENDING-EMBEDDINGS: `sqlite-graphrag pending-embeddings list --json` then `pending-embeddings process --json`
- SLOTS: `sqlite-graphrag slots status --json` and `slots release --slot-id <N> --yes --json`
- EMBEDDING status: `sqlite-graphrag embedding status --json` and `embedding list --json`
- FTS: `sqlite-graphrag fts rebuild --json` and `fts check --json` and `fts stats --json`
- VEC: `sqlite-graphrag vec stats --json` and `vec orphan-list --json` then `vec purge-orphan --yes --json`
- BACKUP: `sqlite-graphrag backup --output backup.sqlite --json`
- SYNC-SAFE-COPY: `sqlite-graphrag sync-safe-copy --dest snapshot.sqlite`
- EXPORT: `sqlite-graphrag export --namespace <ns> --type decision --json`
- OPTIMIZE: `sqlite-graphrag optimize --json`
- VACUUM: `sqlite-graphrag vacuum --json`
- DEBUG-SCHEMA: `sqlite-graphrag debug-schema --json`
- CODEX-MODELS: `sqlite-graphrag codex-models --json`
- COMPLETIONS: `sqlite-graphrag completions <bash|zsh|fish|elvish|powershell>`
- STATS: `sqlite-graphrag stats --json`
- NAMESPACE-DETECT: `sqlite-graphrag namespace-detect --json`
- CACHE list: `sqlite-graphrag cache list --json`
- CACHE clear: `sqlite-graphrag cache clear-models --yes`
- FALLBACK CHAIN: `sqlite-graphrag --llm-backend codex --llm-fallback codex,claude,opencode,none --skip-embedding-on-failure remember --name <n> --type note --description "desc" --body-file note.md --json`


## Active Rules
- ALWAYS pass `--llm-backend` and `--llm-model` explicitly for embedding commands
- ALWAYS parse `backend_invoked` to confirm which backend ran
- ALWAYS run `codex login` or refresh claude OAuth when backend reports stale OAuth
- NEVER pass API keys (OAuth-only, exit 1); NEVER use daemon, `--bare`, `--gliner-variant` (REMOVED)
- NEVER install with `--features embedding-legacy` or `--features ner-legacy`
- NEVER run `enrich` in parallel against same database; NEVER write `.sqlite` outside the binary
- NEVER ignore exit 19 (RETRY MANDATORY) or exit 16 (fix MCP config)
- NEVER call `migrate --to-llm-only` without `--drop-vec-tables` guard
