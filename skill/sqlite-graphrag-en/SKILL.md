---
name: sqlite-graphrag
description: This skill MUST activate for every sqlite-graphrag CLI operation covering persistent memory, GraphRAG knowledge graph, entity linking, hybrid-search, recall, deep-research, remember, remember-batch, ingest, edit, restore, enrich, forget, purge, link, rename-entity and graph maintenance. This skill teaches the LLM to embed via the OpenRouter REST backend with explicit model and price selection, to run entity extraction and enrichment as a SEPARATE step through codex, claude-code or opencode headless backends with explicit model choice, to add and verify OpenRouter API keys, to honour OAuth-only subprocess rules, preflight isolation, FTS5 plus BLOB cosine fusion, canonical relations, exit-code retry strategy and namespace isolation. This skill activates on keywords sqlite-graphrag GraphRAG memory embedding openrouter codex claude opencode remember recall hybrid-search ingest enrich deep-research forget purge link rename-entity
---


## When This Skill Activates
- ACTIVATE when user asks to remember, save, recall, retrieve, search, or persist anything across sessions
- ACTIVATE for long-term context, knowledge graph, GraphRAG, RAG, entity linking, memory management
- ACTIVATE when sqlite-graphrag, embedding, FTS5, hybrid-search, OpenRouter, or LLM memory is mentioned
- NEVER ACTIVATE for one-off ephemeral data, simple file I/O, or tasks unrelated to persistent context


## Core Mental Model — Three Independent Backends
- KNOW the CLI has THREE separate selectors; NEVER conflate them
- USE `--embedding-backend` to choose HOW vectors are produced: `openrouter` (REST, fast) or `llm` (subprocess) or `auto`
- USE `--llm-backend` to choose WHICH subprocess runs LLM embedding when `--embedding-backend llm`: `codex`, `claude`, `opencode`, `none`
- USE `--extraction-backend` (and `enrich --mode`) to choose WHICH headless CLI extracts entities and relations: `codex`, `claude-code`, `opencode`
- SEPARATE writing from enriching ALWAYS: a write produces embeddings; a SEPARATE `enrich` invocation extracts the graph
- PASS `--llm-backend none` on EVERY write (`remember`, `remember-batch`, `ingest`, `edit`, `restore`) to SKIP inline entity embedding and avoid the codex subprocess timeout
- RUN `enrich` only AFTER a write returns exit 0, as a DISTINCT command, NEVER chained into the write
- PARSE `backend_invoked` in every embedding envelope to CONFIRM which backend actually ran


## LLM Prompt Instruction Rules
- WHEN user says "remember this" → EXECUTE `remember --force-merge` with `--graph-stdin` carrying curated entities and canonical relations, then a SEPARATE `enrich`
- WHEN user asks "what do you know about X" → EXECUTE `hybrid-search "X" --k 10 --json` FIRST, then EXPAND top results with `read --name <name> --json`
- WHEN user asks "how is X related to Y" → EXECUTE `graph traverse --from X --depth 2 --json` or `related X --hops 2 --json`
- WHEN user asks "deep research on X" → EXECUTE `deep-research "X" --k 20 --max-hops 3 --json`
- BEFORE creating ANY memory → EXECUTE `hybrid-search "<name>" --k 5 --json` to CHECK duplicates; if found USE `--force-merge`
- AFTER creating or updating memory → VERIFY with `read --name <name> --json | jaq '{name, description, body_length}'`
- AFTER EVERY turn with new findings → EVALUATE whether to persist; if nothing new DECLARE "No new findings to persist"
- WHEN exit code is non-zero → READ the JSON error envelope from stdout via `jaq '{code, message, error_class}'`, REPORT remediation
- ALWAYS parse JSON output with `jaq` (NEVER `jq`)
- ALWAYS pass `--json` on every `sqlite-graphrag` invocation
- ALWAYS capture stdout to a variable FIRST, then parse; NEVER pipe `sqlite-graphrag ... | jaq` directly because multi-line NDJSON masks failures as silent nulls
- ALWAYS use canonical relations ONLY: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- ALWAYS map non-canonical relations BEFORE persisting: `adds|creates → causes`, `implements → supports`, `blocks → contradicts`, `tested-by → related`, `part-of → applies-to`
- ALWAYS normalize entity names to kebab-case ASCII lowercase BEFORE passing to CLI
- NEVER use MCP Serena or `.md` memory files for persistence; NEVER write MEMORY.md
- NEVER start or reference a daemon; NEVER pass `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` to subprocess backends
- PREFER `remember --force-merge` over `edit` for updates; PREFER `--graph-stdin` over inline entity extraction
- LIMIT entities to domain concepts; REJECT generic words, pronouns, UUIDs, timestamps


## Architecture and Principles
- INVOKE always as subprocess; READ stdout for JSON/NDJSON; READ stderr for logs; CHECK exit code BEFORE parsing
- KNOW the binary has NO daemon, NO ONNX runtime, NO model cache
- KNOW cosine similarity is pure Rust over BLOB-backed `memory_embeddings`, `entity_embeddings`, `chunk_embeddings`
- KNOW schema is v15 after `init` or `migrate` on a fresh database
- ENFORCE OAUTH-ONLY for codex and claude subprocess backends: the spawn ABORTS exit 1 if `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` is set
- KNOW `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL` are PRESERVED for custom providers
- KNOW subprocess CWD is ISOLATED; orphan dirs are cleaned automatically
- KNOW 7 preflight guards run BEFORE every LLM subprocess fork; exit 16 is the universal preflight failure
- KNOW the headless extraction subprocess inherits the current working directory and any `.mcp.json` present, which can break `claude -p`; ISOLATE with an empty config dir when extracting via claude-code
- SET `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` ONLY in emergencies
- ISOLATE NAMESPACE per project via `--namespace <ns>` or env; default is `global`
- NEVER expose the binary as an MCP server or HTTP service
- NEVER write the `.sqlite` file in parallel from another tool


## OpenRouter Embedding Models and Prices
- PASS `--embedding-model <MODEL>` when `--embedding-backend openrouter`; there is NO default model, so omission triggers exit 78
- KNOW prices below are per one million tokens; CHOOSE the model by cost and quality for the task
- USE `nvidia/llama-nemotron-embed-vl-1b-v2:free` for FREE zero-cost embedding (RECOMMENDED default)
- USE `perplexity/pplx-embed-v1-0.6b` for the CHEAPEST paid option at about 0.004 USD
- USE `qwen/qwen3-embedding-8b` at about 0.01 USD
- USE `baai/bge-m3` at about 0.01 USD
- USE `qwen/qwen3-embedding-4b` at about 0.02 USD
- USE `openai/text-embedding-3-small` at about 0.02 USD
- USE `mistralai/mistral-embed-2312` at about 0.10 USD
- USE `google/gemini-embedding-2` at about 0.12 USD
- USE `openai/text-embedding-3-large` at about 0.13 USD
- USE `google/gemini-embedding-001` at about 0.15 USD
- KEEP `--embedding-dim 384` consistent across writes and reads; a mismatched dimension collides with the stored index and fails knn with exit 11
- KNOW MRL truncation is applied server-side to the requested `--embedding-dim`, so a higher dimension stays cheap on the OpenRouter REST path
- VERIFY the embedding model whitelist with `sqlite-graphrag codex-models --json`
- KNOW `--embedding-backend openrouter` propagates to ALL embedding paths: `remember`, `remember-batch`, `ingest`, `recall`, `edit`, `restore`, `hybrid-search`, `deep-research`, `enrich`, `init`, `rename-entity`


## OpenRouter API Key Management
- ADD a key via stdin: `echo "sk-or-v1-..." | sqlite-graphrag config add-key --provider openrouter --from-stdin`
- LIST stored keys: `sqlite-graphrag config list-keys --json`
- REMOVE a key by fingerprint: `sqlite-graphrag config remove-key <fingerprint> --json`
- RUN the diagnostic doctor: `sqlite-graphrag config doctor --json`
- INSPECT the config path: `sqlite-graphrag config path`
- KNOW keys live in XDG config `~/.config/sqlite-graphrag/config.toml` with `chmod 600` and are zeroized on drop, NEVER logged
- KNOW precedence: env var `OPENROUTER_API_KEY` > config.toml > CLI flag `--openrouter-api-key`
- NEVER pass the API key as a CLI argument in production; PREFER stdin or env var to avoid shell-history exposure


## Headless LLM Backends — Codex, Claude, OpenCode
- CHOOSE codex with `--llm-backend codex --llm-model gpt-5.4-mini` for embedding and `--mode codex --codex-model gpt-5.4-mini` for extraction; refresh OAuth with `codex login`
- CHOOSE claude with `--llm-backend claude --llm-model claude-sonnet-4-6` for embedding and `--mode claude-code --claude-model claude-sonnet-4-6` for extraction via the OAuth zero-token path
- CHOOSE opencode with `--llm-backend opencode --llm-model opencode/big-pickle` for embedding and `--mode opencode --opencode-model opencode/big-pickle` for extraction via its own auth (NOT OAuth)
- KNOW DEFAULT models: codex `gpt-5.5`, claude `claude-sonnet-4-6`, opencode `opencode/big-pickle`
- KNOW free opencode models: `opencode/big-pickle`, `opencode/deepseek-v4-flash-free`, `opencode/mimo-v2.5-free`, `opencode/nemotron-3-ultra-free`, `opencode/north-mini-code-free`
- OVERRIDE binary paths with `--codex-binary`, `--claude-binary`, `--opencode-binary` when the CLI is not on PATH
- TUNE per-backend timeouts on `ingest` with `--codex-timeout`, `--claude-timeout`, `--opencode-timeout` (seconds)
- VALIDATE codex models with `--codex-model-validate` and auto-substitute with `--codex-model-fallback <MODEL>`
- SWAP backend mid-job on rate limit with `--fallback-mode codex` on `enrich`, or `--llm-fallback codex,claude,none` globally
- WARN that `claude-code` extraction spawns `claude -p`, which inherits the CWD `.mcp.json` and may fail; PREFER codex extraction or isolate the config dir


## Global Flags Reference
- `--db <PATH>` — override database location (accepted per subcommand)
- `--namespace <ns>` — scope operations to a namespace
- `--json` — structured JSON output (ALWAYS pass)
- `--lang en|pt` — force stderr language
- `--tz <TIMEZONE>` — localize timestamps
- `--embedding-backend auto|openrouter|llm` — vector production selector
- `--embedding-model <MODEL>` — OpenRouter embedding model
- `--embedding-dim N` — embedding dimensionality [8, 4096], default 384 MRL
- `--openrouter-api-key <KEY>` — OpenRouter API key
- `--llm-backend codex|claude|opencode|none|auto` — subprocess embedding backend, comma-separated chain allowed
- `--llm-model <MODEL>` — model for the active LLM backend
- `--llm-fallback <chain>` — comma-separated fallback chain when the primary fails
- `--extraction-backend codex|claude-code|opencode` — entity-extraction subprocess selector
- `--llm-parallelism N` — embedding subprocess fan-out, default 4, clamp [1, 32]
- `--max-concurrency N` — cap concurrent heavy invocations, clamp [1, 2×nCPUs]
- `--llm-max-host-concurrency N` — cap host-wide LLM subprocess slots
- `--llm-slot-wait-secs N` — wait for a free slot before aborting; `--llm-slot-no-wait` to fail fast
- `--wait-lock SECS` — widen the lock acquisition window
- `--low-memory` — unitary parallelism for constrained containers
- `--strict-env-clear` — preserve only PATH in subprocess for compliance
- `--graceful-shutdown-secs N` — cleanup budget before SIGKILL
- `--skip-embedding-on-failure` — store without a vector when the chain ends in `none`
- `--codex-binary`, `--claude-binary`, `--opencode-binary` — override binary paths
- `-v`/`-vv`/`-vvv` — info/debug/trace logging on stderr


## CRUD Write Operations
- INVOKE `remember --name <kebab> --type <kind> --description <text>` with `--body <text>` or `--body-file <path>` or `--body-stdin` or `--graph-stdin`
- INVOKE `remember --graph-stdin` to attach `{body, entities, relationships}` in a single JSON document
- PASS entities as `[{name, entity_type}]` in kebab-case ASCII; PASS relationships as `[{source, target, relation, strength}]` where strength is in [0.0, 1.0]
- PASS `--force-merge` for idempotent updates and soft-deleted restoration
- PASS `--dry-run` to validate inputs without persisting
- VALID `--type` values: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- INVOKE `remember-batch` for 10 or more memories via NDJSON stdin; PASS `--transaction` for all-or-nothing
- INVOKE `ingest <DIR> --recursive --pattern "*.md" --mode none` to import a directory as body-only, then enrich SEPARATELY
- KNOW `ingest --mode` accepts `none` (default body-only), `claude-code`, `codex`; opencode is NOT an ingest mode, so enrich with opencode in a SEPARATE step
- USE `--resume` to continue from the queue after interruption; `--retry-failed` for failed items only; `--auto-describe` to synthesize descriptions
- RESPECT the 512000 bytes and 512 chunks limit per body
- NEVER mix `--body`, `--body-file`, `--body-stdin`, `--graph-stdin` in a single invocation
- NEVER use `fd | xargs remember`; INVOKE `ingest` instead
- NEVER pass `--llm-backend codex` on any write; the entity path would force the codex subprocess and stall on its timeout; ALWAYS pass `--llm-backend none`


## CRUD Read Update Delete
- INVOKE `read --name <kebab> --json` for O(1) fetch; PASS `--with-graph` to include linked entities
- INVOKE `list --type <kind> --limit N --offset N --json` to filter and paginate
- INVOKE `history --name <n> --diff --json` for version history with character diff stats
- INVOKE `edit --name <n> --body-file <path>` to update the body, or `--description <text>` and `--memory-type <kind>` for metadata
- USE `--force-reembed` to regenerate the embedding without a body change
- USE `--expected-updated-at <ts>` for optimistic locking; TREAT exit 3 as a conflict, reload and retry
- INVOKE `rename --name <old> --new-name <new>` to rename a memory preserving history
- INVOKE `restore --name <n> --version <N>` to restore an old version
- INVOKE `forget --name <n>` for a reversible soft-delete
- INVOKE `purge --retention-days <N> --yes --dry-run` to preview, then drop `--dry-run` for the hard delete
- INVOKE `cleanup-orphans --yes` after bulk forget, then `vacuum --json`
- NEVER skip optimistic locking in concurrent pipelines; NEVER delete manually via the `sqlite3` shell


## Entity Graph Operations
- INVOKE `link --from <a> --to <b> --relation <type> --create-missing --weight <float>` to create an edge
- INVOKE `unlink --from <a> --to <b> --relation <type>` to remove one edge, or `--entity <name> --all` to drop all edges of an entity
- INVOKE `graph entities --json` to list entities via `.entities[]` (NOT `.items[]`); ORDER with `--sort-by degree|name|created_at`; PAGINATE with `--limit N --offset N`
- INVOKE `graph stats --json` to inspect `node_count`, `edge_count`, `avg_degree`, `max_degree`
- INVOKE `graph traverse --from <root> --depth <N> --json` for subgraph traversal; EXPORT with `--format json|dot|mermaid --output <path>`
- INVOKE `rename-entity --name <old> --new-name <new>` to rename an entity preserving edges
- INVOKE `delete-entity --name <n> --cascade` to delete an entity and its edges
- INVOKE `merge-entities --names "a,b,c" --into <target>` to merge duplicates
- INVOKE `reclassify --name <n> --new-type <kind>` for one entity, or `--from-type <old> --to-type <new> --batch` for bulk type migration
- INVOKE `reclassify-relation --from-relation <old> --to-relation <new> --batch` for bulk relation-type migration; FILTER with `--filter-source-type` and `--filter-target-type`
- INVOKE `prune-relations --relation mentions --dry-run` to preview low-value edges, then drop `--dry-run` with `--yes`
- INVOKE `normalize-entities --yes` to normalize all names to kebab-case ASCII
- INVOKE `prune-ner --entity <n>` to remove NER bindings; `prune-ner --all --yes` for the whole namespace
- INVOKE `memory-entities --name <memory>` for forward lookup, or `--entity <name>` for reverse lookup
- PASS `--max-entity-degree N` on `link` to warn when an entity exceeds N connections
- CANONICAL entity types: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- VALIDATE entity names: minimum 2 chars, no newlines, no short ALL_CAPS of 4 chars or less
- NEVER use `mentions` as a default relation


## GraphRAG Search Operations
- USE the canonical three-layer pattern: `hybrid-search` then `read --name` then `related|graph traverse`
- INVOKE `recall <query> --k N` for pure semantic KNN; PASS `--no-graph` to disable graph expansion, `--precise` for exact scoring, `--max-distance <f>`, `--max-graph-results N`, `--all-namespaces`
- INVOKE `hybrid-search <query> --k N` for FTS5 plus KNN fusion via RRF
- PASS `--rrf-k 60` for standard fusion; `--weight-vec 1.0 --weight-fts 1.0` for balanced fusion
- PASS `--fallback-fts-only` to skip live embedding and serve FTS5 BM25 only in offline mode
- USE `--with-graph --max-hops 2 --min-weight 0.3` for graph expansion; READ BOTH `results[]` AND `graph_matches[]`
- INVOKE `related <name> --hops N --relation <type>` for multi-hop traversal from a memory
- INVOKE `deep-research "<query>" --k 20 --max-hops 3 --max-sub-queries 7 --max-results 50 --with-bodies` for parallel multi-hop research
- TUNE deep-research with `--graph-decay <f>`, `--graph-min-score <f>`, `--max-neighbors-per-hop N`, `--max-cost-usd <f>`, `--timeout <secs>`
- PARSE `recall` returns `results[].{name, snippet, distance, score, source}`
- PARSE `hybrid-search` returns `results[].{name, combined_score, vec_rank, fts_rank}`
- PARSE `deep-research` returns `sub_queries[]`, `results[]`, `evidence_chains[]`, `graph_context`, `stats`
- NEVER confuse `distance` with `combined_score` in ranking; NEVER raise `--hops` without inspecting `graph stats` first


## Enrich Operations
- INVOKE `enrich --operation <op> --mode <backend>` where BOTH flags are MANDATORY; omitting `--mode` is rejected by the parser with exit 2
- VALID `--operation` values: `memory-bindings`, `entity-descriptions`, `body-enrich`, `re-embed`
- VALID `--mode` values: `codex`, `claude-code`, `opencode`
- PASS `--codex-model`, `--claude-model`, or `--opencode-model` to pick the extraction model matching the chosen mode
- PASS `--limit N --resume` for `re-embed`; `--retry-failed` to reprocess only failed items; `--dry-run` to preview
- PASS `--min-output-chars N` to guard `body-enrich` output length; `--fallback-mode codex` to survive a Claude rate limit
- NEVER run `enrich` in parallel against the same database; it acquires a per-namespace singleton


## Write Then Enrich — Two Separate Steps
- TREAT every write as STEP 1 (embed via OpenRouter, `--llm-backend none`) followed by a DISTINCT STEP 2 (`enrich`); NEVER chain them with `&&`
- CHOOSE the OpenRouter model from the price table; CHOOSE the enrich backend and model independently
- REMEMBER step 1: `echo '{"body":"text","entities":[{"name":"jwt","entity_type":"concept"}],"relationships":[{"source":"jwt","target":"auth-svc","relation":"uses","strength":0.8}]}' | sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend none remember --name <n> --type decision --description "desc" --graph-stdin --force-merge --json`
- REMEMBER step 2 codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini --json`
- REMEMBER step 2 claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --claude-model claude-sonnet-4-6 --json`
- REMEMBER step 2 opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --mode opencode --opencode-model opencode/big-pickle --json`
- REMEMBER-BATCH step 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model qwen/qwen3-embedding-8b --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend none remember-batch --transaction --json`
- REMEMBER-BATCH step 2 codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini --json`
- REMEMBER-BATCH step 2 claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --claude-model claude-sonnet-4-6 --json`
- REMEMBER-BATCH step 2 opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --mode opencode --opencode-model opencode/big-pickle --json`
- INGEST step 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend none ingest ./docs --mode none --recursive --pattern "*.md" --type document --resume --json`
- INGEST step 2 codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini --json`
- INGEST step 2 claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --claude-model claude-sonnet-4-6 --json`
- INGEST step 2 opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --mode opencode --opencode-model opencode/big-pickle --json`
- EDIT step 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model perplexity/pplx-embed-v1-0.6b --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend none edit --name <n> --body-file new.md --json`
- EDIT step 2 codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini --json`
- EDIT step 2 claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --claude-model claude-sonnet-4-6 --json`
- EDIT step 2 opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --mode opencode --opencode-model opencode/big-pickle --json`
- RESTORE step 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend none restore --name <n> --version 2 --json`
- RESTORE step 2 codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini --json`
- RESTORE step 2 claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --claude-model claude-sonnet-4-6 --json`
- RESTORE step 2 opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --mode opencode --opencode-model opencode/big-pickle --json`


## Read-Only OpenRouter Formulas
- INIT: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY init --namespace <ns>`
- RECALL: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY recall "query" --k 10 --json`
- HYBRID-SEARCH: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY hybrid-search "query" --k 10 --with-graph --max-hops 2 --min-weight 0.3 --rrf-k 60 --json`
- DEEP-RESEARCH: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY deep-research "question" --k 20 --max-hops 3 --max-sub-queries 7 --max-results 50 --with-bodies --json`
- RENAME-ENTITY: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY rename-entity --name <old> --new-name <new> --json`
- ENRICH re-embed: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend codex --llm-model gpt-5.4-mini enrich --operation re-embed --limit 100 --resume --mode codex --codex-model gpt-5.4-mini --json`
- HYBRID-SEARCH offline: `sqlite-graphrag hybrid-search "query" --k 10 --fallback-fts-only --json`


## Diagnostics and Maintenance
- INIT: `sqlite-graphrag init --namespace <ns>`; HEALTH: `sqlite-graphrag health --json | jaq '{integrity_ok, schema_version}'`
- MIGRATE: `sqlite-graphrag migrate --dry-run --json` to preview, then `migrate --json` after a binary upgrade
- OPTIMIZE: `sqlite-graphrag optimize --json` to refresh planner stats; VACUUM: `sqlite-graphrag vacuum --json` after a large purge
- FTS: `fts check --json` for integrity, `fts stats --json` for counts, `fts rebuild --json` when `health.fts_degraded` is true
- VEC: `vec orphan-list --json` then `vec purge-orphan --yes`; `vec stats --json` for vector health
- EMBEDDING: `embedding --status --json` for counts; `pending-embeddings --status --json` then `pending-embeddings process --json` to reprocess failures
- SLOTS: `slots status --json` to inspect the host semaphore; `slots release --slot-id <N> --yes` for orphans
- PENDING: `pending list --filter-status queued --json`; `pending show <id>`; `pending cleanup --yes`
- EXPORT: `export --namespace <ns> --type <kind> --json` as NDJSON; STATS: `stats --json` for counts and sizes
- BACKUP: `backup --output backup.sqlite --json`; SNAPSHOT: `sync-safe-copy --dest <path>` without taking a lock
- INSPECT: `namespace-detect --json`, `debug-schema --json`, `cache list --json`, `cache clear-models --yes`
- COMPLETIONS: `completions bash|zsh|fish|elvish|powershell`
- SCHEDULE weekly: `purge` then `cleanup-orphans` then `prune-relations --relation mentions` then `vacuum` then `optimize` then `sync-safe-copy`
- IF corruption: `sqlite3 broken.sqlite ".recover" | sqlite3 repaired.sqlite`


## Exit Codes and Retry Strategy
- EXIT 0 success; EXIT 1 validation error; EXIT 2 argument parsing (missing required flag); EXIT 3 optimistic lock conflict, reload and retry
- EXIT 4 not found; EXIT 5 namespace error; EXIT 6 payload too large; EXIT 9 duplicate, use `--force-merge`
- EXIT 10 database error, run `vacuum` plus `health`; EXIT 11 embedding failure, check backend, dimension and OAuth
- EXIT 13 partial batch failure, reprocess failed only; EXIT 14 I/O error; EXIT 15 database busy, widen `--wait-lock`
- EXIT 16 preflight failure, fix MCP config, NEVER treat as transient
- EXIT 19 SHUTDOWN, retry MANDATORY, partial work discarded
- EXIT 20 internal error; EXIT 75 slots exhausted or singleton locked, respect cooldown, NEVER retry immediately
- EXIT 77 RAM pressure, wait for free memory; EXIT 78 config error, OpenRouter key or model missing
- NEVER ignore a non-zero exit; NEVER reprocess a full batch after exit 13; NEVER confuse exit 1 with exit 9


## Concurrency
- RESPECT the hard ceiling `2 x nCPUs` for heavy commands: `init`, `remember`, `ingest`, `recall`, `hybrid-search`
- SET `--llm-parallelism N` default 4 on `remember` and `edit`, default 2 on `ingest`, clamp [1, 32]
- KNOW JOB SINGLETON: `enrich` and `ingest --mode codex|claude-code` acquire a per-namespace singleton
- USE `--wait-job-singleton SECS` or `--force-job-singleton` to break a stale lock
- ENABLE `SQLITE_GRAPHRAG_LOW_MEMORY=1` for unitary parallelism, 3 to 4 times slower
- NEVER run `enrich` in parallel against the same database


## Environment Variables
- `SQLITE_GRAPHRAG_DB_PATH` — database path override
- `SQLITE_GRAPHRAG_NAMESPACE` — persistent namespace
- `SQLITE_GRAPHRAG_LLM_BACKEND` — persistent LLM backend
- `SQLITE_GRAPHRAG_LLM_MODEL` — persistent LLM model
- `SQLITE_GRAPHRAG_EMBEDDING_BACKEND` — persistent embedding backend
- `SQLITE_GRAPHRAG_EMBEDDING_MODEL` — persistent OpenRouter embedding model
- `SQLITE_GRAPHRAG_EMBEDDING_DIM` — embedding dimension [8, 4096], default 384
- `OPENROUTER_API_KEY` — OpenRouter API key, zeroized on drop
- `SQLITE_GRAPHRAG_CODEX_BINARY`, `SQLITE_GRAPHRAG_CLAUDE_BINARY`, `SQLITE_GRAPHRAG_OPENCODE_BINARY` — binary path overrides
- `SQLITE_GRAPHRAG_OPENCODE_MODEL`, `SQLITE_GRAPHRAG_OPENCODE_TIMEOUT` — opencode overrides
- `SQLITE_GRAPHRAG_LOW_MEMORY` — enable unitary parallelism
- `SQLITE_GRAPHRAG_LOG_FORMAT` — `json` for log aggregators
- `SQLITE_GRAPHRAG_SKIP_PREFLIGHT` — bypass preflight, EMERGENCIES ONLY


## Active Rules
- ALWAYS pass `--json` on every invocation
- ALWAYS pass `--embedding-backend openrouter --embedding-model <MODEL> --embedding-dim 384` on every embedding operation, with the key via env or `--openrouter-api-key`
- ALWAYS pass `--llm-backend none` on writes; ALWAYS run `enrich` as a SEPARATE step with `--mode` and the matching model
- ALWAYS parse `backend_invoked` to confirm which backend ran
- ALWAYS refresh OAuth with `codex login`, or the claude OAuth, when stale
- NEVER pass API keys to codex or claude subprocess backends, OAuth-only, exit 1
- NEVER pass `--llm-backend codex` on `remember`, `remember-batch`, `ingest`, `edit`, `restore`
- NEVER run `enrich` in parallel against the same database; NEVER write the `.sqlite` outside the binary
- NEVER ignore exit 19 (retry mandatory) or exit 16 (fix MCP config)
- NEVER pass `--embedding-backend openrouter` without `--embedding-model` and a key — exit 78 guaranteed
