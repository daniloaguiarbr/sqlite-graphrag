---
name: sqlite-graphrag
description: This skill MUST activate for sqlite-graphrag CLI operations including persistent memory, GraphRAG, entity graph, hybrid-search, recall, remember, ingest, enrich, deep-research, LLM embedding via openrouter, codex, claude, opencode backends, OAuth enforcement, preflight validation, FTS5, BLOB cosine similarity, CWD isolation, namespace management and maintenance. Activates on keywords SQLite GraphRAG graph entity embedding openrouter codex claude opencode remember recall hybrid-search ingest enrich forget purge link deep-research
---


## When This Skill Activates
- ACTIVATE when user asks to remember, save, recall, retrieve, search, or persist anything across sessions
- ACTIVATE for long-term context, knowledge graph, GraphRAG, RAG, entity linking, memory management
- ACTIVATE when sqlite, sqlite-graphrag, embedding, FTS5, hybrid-search, or LLM memory is mentioned
- NEVER ACTIVATE for one-off ephemeral data, simple file I/O, or tasks unrelated to persistent context


## LLM Prompt Instruction Rules
- WHEN user says "remember this" → EXECUTE `remember --force-merge` with `--graph-stdin` including curated entities and canonical relations
- WHEN user asks "what do you know about X" → EXECUTE `hybrid-search "X" --k 10 --json` FIRST, then EXPAND top results with `read --name <name> --json`
- WHEN user asks "how is X related to Y" → EXECUTE `graph traverse --from X --depth 2 --json` or `related X --hops 2 --json`
- WHEN user asks "deep research on X" → EXECUTE `deep-research "X" --k 20 --max-hops 3 --json`
- BEFORE creating ANY memory → EXECUTE `hybrid-search "<name>" --k 5 --json` to CHECK duplicates; if found USE `--force-merge`
- AFTER creating or updating memory → VERIFY with `read --name <name> --json | jaq '{name, description, body_length}'`
- AFTER EVERY turn with new findings → EVALUATE whether to persist via `remember --force-merge`; if nothing new DECLARE "No new findings to persist"
- WHEN exit code is non-zero → READ JSON error envelope from stdout via `jaq '{code, message, error_class}'`, REPORT remediation
- WHEN exit 9 (duplicate) → RETRY with `--force-merge`
- WHEN exit 19 (SHUTDOWN) → RETRY MANDATORY; partial work discarded
- WHEN exit 75 (singleton locked) → WAIT and retry; NEVER increase concurrency
- WHEN exit 16 (preflight) → FIX MCP config; NEVER bypass with `SKIP_PREFLIGHT`
- ALWAYS parse JSON output with `jaq` (NEVER `jq`)
- ALWAYS pass `--json` flag on every `sqlite-graphrag` invocation
- ALWAYS use canonical relations ONLY: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- ALWAYS map non-canonical relations BEFORE persisting: `adds|creates → causes`, `implements → supports`, `blocks → contradicts`, `tested-by → related`, `part-of → applies-to`
- ALWAYS normalize entity names to kebab-case ASCII lowercase BEFORE passing to CLI
- NEVER use MCP Serena or `.md` memory files for persistence; NEVER write MEMORY.md
- NEVER start or reference daemon (REMOVED); NEVER pass `ANTHROPIC_API_KEY` or `OPENAI_API_KEY`
- PREFER `remember --force-merge` over `edit` for updates; PREFER `--graph-stdin` over `--enable-ner`
- LIMIT entities to domain concepts; REJECT generic words, pronouns, UUIDs, timestamps


## Architecture and Principles
- INVOKE always as subprocess; READ stdout for JSON/NDJSON; READ stderr for logs; CHECK exit code BEFORE parsing
- KNOW binary has NO daemon, NO ONNX runtime, NO model cache
- KNOW cosine similarity is pure Rust over BLOB-backed `memory_embeddings`, `entity_embeddings`, `chunk_embeddings`
- KNOW schema is v15 after `init` or `migrate` on fresh database
- ENFORCE OAUTH-ONLY for LLM subprocess backends: spawn ABORTS exit 1 if `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` is set
- KNOW `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL` are PRESERVED for custom providers
- KNOW subprocess CWD is ISOLATED via `apply_cwd_isolation`; orphan dirs cleaned via `cleanup_isolation_dirs`
- KNOW 7 preflight guards run BEFORE every LLM subprocess fork
- KNOW exit 16 is universal preflight failure; READ envelope for variant-specific remediation
- SET `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` ONLY in emergencies
- ISOLATE NAMESPACE per project via `--namespace <ns>` or env; default is `global`
- NEVER expose binary as MCP server or HTTP service
- NEVER write `.sqlite` file in parallel from another tool
- KNOW `EmbeddingBackendChoice` is SEPARATE from `LlmBackendChoice` — embedding and extraction are independent


## Embedding Backend Selection
- PASS `--embedding-backend auto|openrouter|llm` to select embedding backend (SEPARATE from `--llm-backend`)
- PASS `--embedding-backend openrouter` for direct REST API embedding (~100-500ms vs 20-60s subprocess LLM)
- PASS `--embedding-backend llm` to use the subprocess LLM chain (`--llm-backend` governs which)
- PASS `--llm-backend codex` to spawn Codex CLI headless (DEFAULT LLM backend)
- PASS `--llm-backend claude` to spawn Claude Code headless via OAuth-compatible zero-token path
- PASS `--llm-backend opencode` to spawn OpenCode CLI headless (own auth system, NOT OAuth)
- PASS `--llm-backend codex,claude,opencode,none` for full fallback chain with null embedding last resort
- PASS `--llm-model <MODEL>` to select embedding model for active LLM backend
- KNOW DEFAULT LLM models: codex=`gpt-5.5`, claude=`claude-sonnet-4-6`, opencode=`opencode/big-pickle`
- PASS `--llm-fallback-mode <backend>` to swap backend mid-job on rate-limit
- PASS `--skip-embedding-on-failure` ONLY when fallback chain ends in `none`
- PARSE `backend_invoked` field in every embedding envelope to CONFIRM which backend ran
- KNOW opencode free models: `opencode/big-pickle`, `opencode/deepseek-v4-flash-free`, `opencode/mimo-v2.5-free`, `opencode/nemotron-3-ultra-free`, `opencode/north-mini-code-free`
- RUN `codex login` to refresh codex OAuth; refresh claude OAuth when stale


## OpenRouter Embedding Setup and Model Verification
- PASS `--openrouter-api-key <KEY>` or SET `OPENROUTER_API_KEY` env var for authentication
- PASS `--embedding-model <MODEL>` — REQUIRED when using `--embedding-backend openrouter`; NO default model exists
- KNOW exit 78 (`EX_CONFIG`) for missing API key, missing model, or invalid key
- KNOW `OPENROUTER_API_KEY` is handled via `secrecy::SecretString` with zeroize-on-drop — NEVER logged
- KNOW MRL truncation is applied to configured `--embedding-dim` (default 64)
- KNOW 10 verified OpenRouter embedding models:
- `google/gemini-embedding-001`
- `google/gemini-embedding-2`
- `mistralai/mistral-embed-2312`
- `qwen/qwen3-embedding-8b`
- `qwen/qwen3-embedding-4b`
- `openai/text-embedding-3-small`
- `nvidia/llama-nemotron-embed-vl-1b-v2:free`
- `baai/bge-m3`
- `openai/text-embedding-3-large`
- `perplexity/pplx-embed-v1-0.6b`
- KNOW `--embedding-backend openrouter` propagates to ALL 13 embedding paths: `remember`, `remember-batch`, `ingest`, `recall`, `edit`, `restore`, `hybrid-search`, `deep-research`, `enrich`, `init`, `rename-entity`, ingest claude mode, remember chunk embedding
- PASS `--enrich-after` on `ingest` to trigger `enrich --operation memory-bindings` automatically after all files ingested
- INVOKE `sqlite-graphrag codex-models --json` to inspect embedding model whitelist with compatibility info


## OpenRouter API Key Management
- ADD key via stdin: `echo "sk-or-v1-..." | sqlite-graphrag config add-key --provider openrouter --from-stdin`
- LIST stored keys: `sqlite-graphrag config list-keys --json`
- REMOVE key by fingerprint: `sqlite-graphrag config remove-key <fingerprint> --json`
- RUN doctor check: `sqlite-graphrag config doctor --json`
- INSPECT config path: `sqlite-graphrag config path`
- KNOW keys are stored in XDG config (`~/.config/sqlite-graphrag/config.toml`) with `chmod 600`
- KNOW precedence: env var > config.toml > CLI flag
- NEVER pass API key as CLI argument in production — use stdin or env var to avoid shell history exposure


## Global Flags Reference
- `--db <PATH>` — override database location (NOT global; each subcommand accepts independently)
- `--namespace <ns>` — scope operations to a namespace
- `--lang en|pt` — force stderr language
- `--tz <TIMEZONE>` — localize timestamps in output
- `--json` — structured JSON output (ALWAYS pass)
- `--low-memory` — unitary parallelism for constrained containers
- `--max-concurrency N` — cap concurrent heavy CLI invocations
- `--wait-lock SECS` — widen lock acquisition window
- `--llm-parallelism N` — cap embedding subprocess fan-out (default 4, clamp [1, 32])
- `--llm-backend <chain>` — LLM subprocess backend selection with comma-separated fallback
- `--llm-model <MODEL>` — embedding model for active LLM backend
- `--llm-fallback <chain>` — comma-separated fallback chain tried when primary fails (default `codex,claude,none`)
- `--embedding-backend auto|openrouter|llm` — embedding backend selector
- `--embedding-model <MODEL>` — embedding model for OpenRouter backend
- `--openrouter-api-key <KEY>` — OpenRouter API key
- `--embedding-dim N` — embedding dimensionality [8, 4096] (default 64 MRL)
- `--graceful-shutdown-secs N` — cleanup budget before SIGKILL
- `--strict-env-clear` — preserve only `PATH` in subprocess for compliance
- `--codex-binary`, `--claude-binary`, `--opencode-binary` — override binary paths
- `-v`/`-vv`/`-vvv` — info/debug/trace logging on stderr


## CRUD Write Operations
- INVOKE `remember --name <kebab> --type <kind> --description <text>` with `--body <text>` or `--body-file <path>` or `--body-stdin`
- INVOKE `remember --graph-stdin` to attach `{body, entities, relationships}` in single JSON
- PASS entities as `[{name, entity_type}]` in kebab-case ASCII
- PASS relationships as `[{source, target, relation, strength}]` where `strength in [0.0, 1.0]`
- PASS `--force-merge` for idempotent updates and soft-deleted restoration
- PASS `--dry-run` to validate inputs without persisting
- VALID `--type` values: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- INVOKE `remember-batch` for 10+ memories via NDJSON stdin
- INVOKE `ingest <DIR> --recursive --pattern "*.md"` to import directory
- PASS `--mode codex|claude-code|opencode` for LLM-curated entity extraction
- USE `--resume` to continue from queue after interruption
- RESPECT 512000 bytes and 512 chunks limit per body
- NEVER mix `--body`, `--body-file`, `--body-stdin`, `--graph-stdin` in single invocation
- NEVER use `fd | xargs remember`; INVOKE `ingest` instead
- NEVER use `--force-merge` in `ingest` (exclusive to `remember`)


## CRUD Read Update Delete
- INVOKE `read --name <kebab> --json` for O(1) fetch; PASS `--with-graph` to include linked entities
- INVOKE `list --type <kind> --limit N --offset N --json` to filter and paginate
- INVOKE `history --name <n> --diff --json` for version history with character diff stats
- INVOKE `edit --name <n> --body-file <path>` to update body (re-embeds automatically)
- USE `--force-reembed` to regenerate embedding without body change
- USE `--expected-updated-at <ts>` for optimistic locking; TREAT exit 3 as conflict
- INVOKE `rename --from <old> --to <new>` to rename preserving history
- INVOKE `restore --name <n> --version <N>` to restore old version
- INVOKE `forget --name <n>` for reversible soft-delete
- INVOKE `purge --retention-days <N> --yes --dry-run` for preview then hard delete
- INVOKE `cleanup-orphans --yes` after bulk forget; then `vacuum --json`
- NEVER skip optimistic locking in concurrent pipelines
- NEVER delete manually via `sqlite3` shell


## Entity Graph Operations
- INVOKE `link --from <a> --to <b> --relation <type> --create-missing --weight <float>` to create edge
- INVOKE `graph entities --json` to list entities; ACCESS via `.entities[]` (NOT `.items[]`)
- INVOKE `graph stats --json` to inspect `node_count`, `edge_count`, `avg_degree`, `max_degree`
- INVOKE `graph traverse --from <root> --depth <N> --json` for subgraph traversal
- USE `--format json|dot|mermaid` with `--output <path>` to export graph
- INVOKE `rename-entity`, `delete-entity --cascade`, `merge-entities --names "a,b,c" --into <target>`
- INVOKE `reclassify --name <n> --new-type <kind>` or `--from-type <old> --to-type <new> --batch`
- INVOKE `reclassify-relation --from-relation <old> --to-relation <new> --batch` for bulk relation type migration
- INVOKE `normalize-entities --yes` to normalize all names to kebab-case ASCII
- INVOKE `prune-ner --entity <n>` to remove NER bindings; `prune-ner --all --yes` for all in namespace
- INVOKE `memory-entities --name <memory>` for forward lookup (memory → entities); `--entity <name>` for reverse lookup (entity → memories)
- PASS `--max-entity-degree N` on `link` to warn when entity exceeds N connections
- ORDER graph entities via `--sort-by degree|name|created_at`; PAGINATE via `--limit N --offset N`
- CANONICAL relations: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- CANONICAL entity types: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- VALIDATE entity names: minimum 2 chars, no newlines, no short ALL_CAPS (4 chars or less REJECTED)
- NEVER use `mentions` as default relation


## GraphRAG Search Operations
- USE canonical three-layer pattern: `hybrid-search` then `read --name` then `related|graph traverse`
- INVOKE `recall <query> --k N` for pure semantic KNN search
- INVOKE `hybrid-search <query> --k N` for FTS5+KNN fusion via RRF
- PASS `--rrf-k 60` for standard fusion; `--weight-vec 1.0 --weight-fts 1.0` for balanced
- PASS `--fallback-fts-only` to skip live embedding and serve FTS5 BM25 only (offline mode)
- USE `--with-graph --max-hops 2 --min-weight 0.3` for graph expansion; READ BOTH `results[]` AND `graph_matches[]`
- INVOKE `related <name> --hops N` for multi-hop traversal from memory
- INVOKE `deep-research "<query>" --k 20 --max-hops 3 --max-sub-queries 7 --max-results 50` for parallel multi-hop research
- INVOKE `enrich --operation <op>` for LLM graph quality: `memory-bindings`, `entity-descriptions`, `body-enrich`, `re-embed --limit N --resume`
- PARSE `recall` returns `results[].{name, snippet, distance, score, source}`
- PARSE `hybrid-search` returns `results[].{name, combined_score, vec_rank, fts_rank}`
- PARSE `deep-research` returns `sub_queries[]`, `results[]`, `evidence_chains[]`, `graph_context`, `stats`
- NEVER confuse `distance` with `combined_score` in ranking
- NEVER increase `--hops` without inspecting `graph stats` first


## OpenRouter Embedding Formulas
- INIT via OpenRouter: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY init --namespace <ns>`
- REMEMBER via OpenRouter: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember --name <n> --type decision --description "desc" --body "text" --json`
- REMEMBER-BATCH via OpenRouter: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember-batch --json`
- INGEST via OpenRouter: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY ingest ./docs --recursive --pattern "*.md" --enrich-after --json`
- EDIT via OpenRouter: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY edit --name <n> --body-file new.md --json`
- RESTORE via OpenRouter: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY restore --name <n> --version 2 --json`
- RECALL via OpenRouter: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY recall "query" --k 10 --json`
- HYBRID-SEARCH via OpenRouter: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY hybrid-search "query" --k 10 --with-graph --json`
- DEEP-RESEARCH via OpenRouter: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY deep-research "query" --k 20 --max-hops 3 --json`
- RENAME-ENTITY via OpenRouter: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY rename-entity --from <old> --to <new> --json`
- ENRICH re-embed via OpenRouter: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend codex --llm-model gpt-5.4-mini enrich --operation re-embed --limit 100 --json`


## Enrichment Pipelines — OpenRouter Embed Then LLM Enrich
- REMEMBER then ENRICH via codex: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember --name <n> --type decision --description "desc" --body "text" --json && sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --json`
- REMEMBER then ENRICH via claude: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember --name <n> --type decision --description "desc" --body "text" --json && sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --json`
- REMEMBER then ENRICH via opencode: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember --name <n> --type decision --description "desc" --body "text" --json && sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --json`
- REMEMBER-BATCH then ENRICH via codex: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember-batch --json && sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --json`
- REMEMBER-BATCH then ENRICH via claude: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember-batch --json && sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --json`
- REMEMBER-BATCH then ENRICH via opencode: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY remember-batch --json && sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --json`
- INGEST then ENRICH via codex: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY ingest ./docs --recursive --pattern "*.md" --json && sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --json`
- INGEST then ENRICH via claude: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY ingest ./docs --recursive --pattern "*.md" --json && sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --json`
- INGEST then ENRICH via opencode: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY ingest ./docs --recursive --pattern "*.md" --json && sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --json`
- EDIT then ENRICH via codex: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY edit --name <n> --body-file new.md --json && sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --json`
- EDIT then ENRICH via claude: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY edit --name <n> --body-file new.md --json && sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --json`
- EDIT then ENRICH via opencode: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY edit --name <n> --body-file new.md --json && sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --json`
- RESTORE then ENRICH via codex: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY restore --name <n> --version 2 --json && sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --json`
- RESTORE then ENRICH via claude: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY restore --name <n> --version 2 --json && sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --json`
- RESTORE then ENRICH via opencode: `sqlite-graphrag --embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY restore --name <n> --version 2 --json && sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --json`


## CLI Formulas — LLM Subprocess Backends
- INIT: `sqlite-graphrag init --namespace <ns>`
- HEALTH: `sqlite-graphrag health --namespace <ns> --json | jaq '{integrity_ok, schema_version}'`
- MIGRATE preview: `sqlite-graphrag migrate --dry-run --json`
- REMEMBER codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini remember --name <n> --type decision --description "desc" --body-file doc.md --force-merge --json`
- REMEMBER claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 remember --name <n> --type decision --description "desc" --body "content" --force-merge --json`
- REMEMBER opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle remember --name <n> --type note --description "desc" --body-stdin --json`
- REMEMBER graph-stdin: `echo '{"body":"text","entities":[{"name":"jwt","entity_type":"concept"}],"relationships":[{"source":"jwt","target":"auth-svc","relation":"uses","strength":0.8}]}' | sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini remember --name <n> --type decision --description "desc" --graph-stdin --force-merge --json`
- RECALL codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini recall "query" --k 5 --no-graph --json`
- RECALL claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 recall "query" --k 5 --json`
- RECALL opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle recall "query" --k 5 --json`
- HYBRID-SEARCH codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini hybrid-search "query" --k 10 --with-graph --max-hops 2 --rrf-k 60 --json`
- HYBRID-SEARCH claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 hybrid-search "query" --k 10 --with-graph --json`
- HYBRID-SEARCH opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle hybrid-search "query" --k 10 --json`
- HYBRID-SEARCH fts-only: `sqlite-graphrag hybrid-search "query" --k 10 --fallback-fts-only --json`
- DEEP-RESEARCH claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 deep-research "question" --k 20 --max-hops 3 --max-sub-queries 7 --with-bodies --json`
- DEEP-RESEARCH codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini deep-research "question" --k 20 --max-hops 3 --json`
- DEEP-RESEARCH opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle deep-research "question" --k 20 --max-hops 3 --json`
- INGEST codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini ingest ./docs --mode codex --recursive --pattern "*.md" --type document --resume --json`
- INGEST claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 ingest ./docs --mode claude-code --recursive --pattern "*.md" --type document --resume --json`
- INGEST opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle ingest ./docs --mode opencode --recursive --pattern "*.md" --json`
- RELATED: `sqlite-graphrag related <name> --hops 2 --relation uses --json`
- ENRICH re-embed: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation re-embed --limit 100 --resume --json`
- ENRICH memory-bindings: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --json`
- READ with graph: `sqlite-graphrag read --name <n> --with-graph --json`
- LIST: `sqlite-graphrag list --type decision --limit 50 --offset 0 --json`
- EDIT codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini edit --name <n> --body-file new.md --json`
- EDIT claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 edit --name <n> --body-file new.md --json`
- EDIT opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle edit --name <n> --body-file new.md --json`
- RESTORE codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini restore --name <n> --version 2 --json`
- RESTORE claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 restore --name <n> --version 2 --json`
- RESTORE opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle restore --name <n> --version 2 --json`
- RENAME-ENTITY codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini rename-entity --from <old> --to <new> --json`
- RENAME-ENTITY claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 rename-entity --from <old> --to <new> --json`
- RENAME-ENTITY opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle rename-entity --from <old> --to <new> --json`
- LINK: `sqlite-graphrag link --from <a> --to <b> --relation uses --weight 0.8 --create-missing --json`
- GRAPH stats: `sqlite-graphrag graph stats --json | jaq '{node_count, edge_count, avg_degree}'`
- GRAPH traverse: `sqlite-graphrag graph traverse --from <entity> --depth 2 --json`
- MERGE entities: `sqlite-graphrag merge-entities --names "a,b,c" --into target --json`
- FORGET: `sqlite-graphrag forget --name <n> --json`
- PURGE preview: `sqlite-graphrag purge --retention-days 30 --yes --dry-run --json`
- BACKUP: `sqlite-graphrag backup --output backup.sqlite --json`
- VACUUM: `sqlite-graphrag vacuum --json`
- STATS: `sqlite-graphrag stats --json`
- FULL FALLBACK CHAIN: `sqlite-graphrag --llm-backend codex,claude,opencode,none --skip-embedding-on-failure remember --name <n> --type note --description "desc" --body-file note.md --json`


## Exit Codes and Retry Strategy
- EXIT 0: success; EXIT 1: validation error; EXIT 2: argument parsing; EXIT 3: optimistic lock conflict (reload and retry)
- EXIT 4: not found; EXIT 5: namespace error; EXIT 6: payload too large; EXIT 9: duplicate (use `--force-merge`)
- EXIT 10: database error (run `vacuum` + `health`); EXIT 11: embedding failure (check backend and OAuth)
- EXIT 13: partial batch failure (reprocess failed only); EXIT 14: I/O error; EXIT 15: database busy (widen `--wait-lock`)
- EXIT 16: preflight failure (fix MCP config, NEVER treat as transient)
- EXIT 19: SHUTDOWN (RETRY MANDATORY, partial work discarded)
- EXIT 20: internal error; EXIT 75: slots exhausted or singleton locked (respect cooldown, NEVER retry immediately)
- EXIT 77: RAM pressure (wait for free memory); EXIT 78: config error (OpenRouter key or model missing)
- NEVER ignore non-zero exit; NEVER reprocess full batch after exit 13; NEVER confuse exit 1 with exit 9


## Concurrency and Parallelism
- RESPECT hard ceiling `2 x nCPUs` for heavy commands: `init`, `remember`, `ingest`, `recall`, `hybrid-search`
- SET `--llm-parallelism N` default 4 on `remember`/`edit`, default 2 on `ingest` (clamp [1, 32])
- KNOW JOB SINGLETON: `enrich`, `ingest --mode claude-code|codex|opencode` acquire per-namespace singleton
- USE `--wait-job-singleton SECS` or `--force-job-singleton` to break stale lock
- ENABLE `SQLITE_GRAPHRAG_LOW_MEMORY=1` for unitary parallelism (3-4x slower)
- NEVER run `enrich` in parallel against same database


## Maintenance and Diagnostics
- RUN `sqlite-graphrag init --namespace <ns>` on first use
- RUN `health --json` to verify `integrity_ok`, `schema_ok`, `schema_version >= 15`
- RUN `migrate --dry-run --json` to preview; then `migrate --json` after binary upgrade
- RUN `optimize --json` to refresh planner stats
- RUN `fts rebuild --json` when `health.fts_degraded` is true
- RUN `fts check --json` for FTS5 integrity verification; `fts stats --json` for FTS5 row counts
- INVOKE `vacuum --json` after large purge
- INVOKE `vec orphan-list --json` then `vec purge-orphan --yes` to clean orphaned vectors
- INVOKE `pending-embeddings list --json` then `pending-embeddings process --json` to reprocess failed embeddings
- INVOKE `slots status --json` to inspect host-wide semaphore; `slots release --slot-id <N> --yes` for orphan slots
- INVOKE `embedding status --json` for embedding counts; `embedding list --json` for per-entry inspection
- INVOKE `export --namespace <ns> --type <kind> --json` to export memories as NDJSON
- INVOKE `debug-schema --json` for schema drift troubleshooting
- INVOKE `namespace-detect --json` to resolve namespace precedence for current invocation
- INVOKE `cache list --json` to list cached model files; `cache clear-models --yes` to force re-download
- INVOKE `completions bash|zsh|fish|elvish|powershell` for shell tab completions
- INVOKE `codex-models --json` to inspect embedding model whitelist
- INVOKE `sync-safe-copy --dest <path>` for atomic snapshot without taking lock
- INVOKE `pending list --filter-status queued --json`; `pending show <id>`; `pending cleanup --yes`
- INVOKE `stats --json` for database statistics (counts, sizes, namespace breakdown)
- SCHEDULE weekly: `purge` then `cleanup-orphans` then `prune-relations --relation mentions` then `vacuum` then `optimize` then `sync-safe-copy`
- IF corruption: `sqlite3 broken.sqlite ".recover" | sqlite3 repaired.sqlite`


## Environment Variables
- `SQLITE_GRAPHRAG_DB_PATH` — persistent database path override
- `SQLITE_GRAPHRAG_NAMESPACE` — persistent namespace
- `SQLITE_GRAPHRAG_LLM_BACKEND` — persistent LLM backend (codex|claude|opencode|none|auto)
- `SQLITE_GRAPHRAG_LLM_MODEL` — persistent LLM model override
- `SQLITE_GRAPHRAG_CODEX_BINARY` / `SQLITE_GRAPHRAG_CODEX_EMBED_MODEL` — codex binary and embed model
- `SQLITE_GRAPHRAG_CLAUDE_BINARY` — claude binary path override
- `SQLITE_GRAPHRAG_OPENCODE_BINARY` / `SQLITE_GRAPHRAG_OPENCODE_MODEL` / `SQLITE_GRAPHRAG_OPENCODE_TIMEOUT` — opencode overrides
- `OPENROUTER_API_KEY` — OpenRouter API key for embedding backend
- `SQLITE_GRAPHRAG_EMBEDDING_BACKEND` — persistent embedding backend override (openrouter|llm|auto)
- `SQLITE_GRAPHRAG_EMBEDDING_DIM` — embedding dimension [8, 4096] (default 64 MRL)
- `SQLITE_GRAPHRAG_LOW_MEMORY` — enable unitary parallelism
- `SQLITE_GRAPHRAG_LOG_FORMAT` — `json` for log aggregators
- `SQLITE_GRAPHRAG_SKIP_PREFLIGHT` — bypass preflight (EMERGENCIES ONLY)
- `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN` — CI test harnesses ONLY


## Active Rules
- ALWAYS pass `--json` on every invocation
- ALWAYS pass `--embedding-backend openrouter --embedding-model google/gemini-embedding-001 --openrouter-api-key $OPENROUTER_API_KEY` when using OpenRouter
- ALWAYS pass `--llm-backend` and `--llm-model` explicitly for LLM subprocess embedding commands
- ALWAYS parse `backend_invoked` to confirm which backend ran
- ALWAYS run `codex login` or refresh claude OAuth when stale
- NEVER pass API keys with LLM subprocess backends (OAuth-only, exit 1)
- NEVER run `enrich` in parallel against same database; NEVER write `.sqlite` outside the binary
- NEVER ignore exit 19 (RETRY MANDATORY) or exit 16 (fix MCP config)
- NEVER pass `--embedding-backend openrouter` without `--embedding-model` — exit 78 guaranteed
- NEVER pass `--embedding-backend openrouter` without `--openrouter-api-key` or `OPENROUTER_API_KEY` — exit 78 guaranteed
