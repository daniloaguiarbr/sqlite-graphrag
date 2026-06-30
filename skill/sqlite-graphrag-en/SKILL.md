---
name: sqlite-graphrag
description: This skill MUST activate for every sqlite-graphrag CLI operation covering persistent memory, GraphRAG knowledge graph, entity linking, hybrid-search, recall, deep-research, remember, remember-batch, ingest, edit, restore, enrich, forget, purge, link, rename-entity and graph maintenance. This skill teaches the LLM to embed via the OpenRouter REST backend with explicit model and price selection, to run entity extraction and enrichment as a SEPARATE step through codex, claude-code, opencode or openrouter backends with explicit model choice, to add and verify OpenRouter API keys, to honour OAuth-only subprocess rules, preflight isolation, FTS5 plus BLOB cosine fusion, canonical relations, exit-code retry strategy and namespace isolation. This skill activates on keywords sqlite-graphrag GraphRAG memory embedding openrouter codex claude opencode remember recall hybrid-search ingest enrich deep-research forget purge link rename-entity
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
- USE `--extraction-backend` (and `enrich --mode`) to choose WHICH backend extracts entities and relations: `codex`, `claude-code`, `opencode` (headless CLIs) or `openrouter` (REST `/chat/completions`, no local CLI)
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
- KNOW `init` or `migrate` brings a fresh database to the current schema version; READ the live number from `health --json` `schema_version`
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
- USE `qwen/qwen3-embedding-8b` at about 0.01 USD (CHEAPEST paid option)
- USE `baai/bge-m3` at about 0.01 USD
- USE `qwen/qwen3-embedding-4b` at about 0.02 USD
- USE `openai/text-embedding-3-small` at about 0.02 USD
- USE `perplexity/pplx-embed-v1-0.6b` at about 0.04 USD
- USE `mistralai/mistral-embed-2312` at about 0.10 USD
- USE `google/gemini-embedding-2` at about 0.12 USD
- USE `openai/text-embedding-3-large` at about 0.13 USD
- USE `google/gemini-embedding-001` at about 0.15 USD
- KEEP `--embedding-dim 384` consistent across writes and reads; a mismatched dimension collides with the stored index and fails knn with exit 11
- KNOW MRL truncation is applied server-side to the requested `--embedding-dim`, so a higher dimension stays cheap on the OpenRouter REST path
- KNOW NO subcommand enumerates OpenRouter embedding models; the curated price table above IS the authoritative menu
- VERIFY the OpenRouter key and config resolution with `sqlite-graphrag config doctor --json`; an invalid model fails fast with exit 78
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
- CHOOSE openrouter for extraction ONLY with `--mode openrouter --openrouter-model <model>` routing the judge to OpenRouter `/chat/completions` REST; the key comes from `OPENROUTER_API_KEY` and `--openrouter-model` is MANDATORY (no default; a missing value exits 1 before any network call)
- KNOW DEFAULT models: codex `gpt-5.5`, claude `claude-sonnet-4-6`, opencode `opencode/big-pickle`
- KNOW the opencode model catalog is EXTERNAL and dynamic, rotating free tiers like Big Pickle, GPT-5 Nano, Nemotron Super and MiniMax Free; the CLI passes `--opencode-model` through UNVALIDATED, so PASS any current OpenCode Zen id (the verified default is `opencode/big-pickle`) and CONSULT `opencode.ai/zen` for the live catalog instead of hardcoding volatile ids
- OVERRIDE binary paths with `--codex-binary`, `--claude-binary`, `--opencode-binary` when the CLI is not on PATH
- TUNE per-backend timeouts on `ingest` with `--codex-timeout`, `--claude-timeout`, `--opencode-timeout` (seconds)
- VALIDATE codex models with `--codex-model-validate` and auto-substitute with `--codex-model-fallback <MODEL>`
- LIST the codex OAuth models with `sqlite-graphrag codex-models --json` to pick `--codex-model` for `--mode codex`; this lists CODEX models, NOT OpenRouter models
- SWAP backend mid-job on rate limit with `--fallback-mode codex` on `enrich`, or `--llm-fallback codex,claude,none` globally
- WARN that `claude-code` extraction spawns `claude -p`, which inherits the CWD `.mcp.json` and may fail; PREFER codex extraction or isolate the config dir
- KNOW `--mode openrouter` does NOT spawn any subprocess — it makes a REST `/chat/completions` call, so it needs NO claude, codex or opencode CLI installed
- WEIGH the trade-off: `openrouter` extraction bills tokens against `OPENROUTER_API_KEY` (read `usage.cost` from the response), whereas codex, claude-code and opencode bill no OpenRouter tokens via their OAuth or own-auth zero-token paths


## OpenRouter Text Models for Enrich
- PASS `--openrouter-model <MODEL>` from this table on `--mode openrouter`; prices are input/output USD per one million tokens
- KNOW these models serve ONLY entity extraction and enrichment, NEVER embedding; the embedding table above is separate
- USE `openai/gpt-oss-120b` at 0.039/0.18 USD, 131k context, 36 tps (CHEAPEST input, RECOMMENDED judge default)
- USE `openai/gpt-oss-120b:nitro` at 0.15/0.60 USD, 131k context, 300 tps (FASTEST throughput)
- USE `xiaomi/mimo-v2.5` at 0.10/0.28 USD, 1M context, 17 tps
- USE `deepseek/deepseek-v4-flash` at 0.09/0.18 USD, 1M context, 20 tps
- USE `deepseek/deepseek-v4-flash:nitro` at 0.14/0.28 USD, 1M context, 109 tps
- USE `minimax/minimax-m2.7` at 0.25/1.00 USD, 205k context, 43 tps
- USE `minimax/minimax-m3` at 0.30/1.20 USD, 1M context, 42 tps
- USE `minimax/minimax-m2.7:nitro` at 0.30/1.20 USD, 205k context, 146 tps
- USE `xiaomi/mimo-v2.5-pro` at 0.43/0.87 USD, 1M context, 29 tps
- USE `google/gemini-3.1-flash-lite` at 0.95/3.00 USD, 1M context, 100 tps
- USE `deepseek/deepseek-v4-pro` at 1.30/2.60 USD, 1M context, 26 tps
- USE `z-ai/glm-5.2` and `z-ai/glm-5.2:nitro` whose price varies by provider; CONFIRM the real cost via `usage.cost` in the response
- KNOW `:nitro` variants route to the fastest provider at a higher price
- VERIFY a model honours strict `json_schema` BEFORE production; a model without Structured Outputs support fails with an explicit OpenRouter error
- READ `usage.cost` from the chat response to account the real token cost per item


## Global Flags Reference
- `--db <PATH>` — override database location; PLACE it AFTER the subcommand (e.g. `remember --db <PATH>`), because the canonical position-independent override is the env var `SQLITE_GRAPHRAG_DB_PATH`
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
- `--extraction-backend codex|claude-code|opencode|openrouter` — entity-extraction backend selector (openrouter is REST, not a subprocess)
- `--openrouter-model <MODEL>` — MANDATORY judge model for `--mode openrouter` (no default; absence exits 1 before any network call)
- `--openrouter-base-url <URL>` — optional OpenRouter endpoint override for chat enrich
- `--openrouter-timeout <SECS>` — chat enrich request timeout, default 600
- `--llm-parallelism N` — embedding fan-out width, default 4, clamp [1, 32]; governs BOTH the subprocess fan-out AND the concurrent OpenRouter REST fan-out (bounded JoinSet), so `--llm-parallelism 8` yields effective concurrency 8 on the REST path; small single-batch inputs stay serial
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
- INVOKE `remember --graph-file <path>` to load the entity graph from a file; COMBINE with `--body-file <path>` to supply the body and the graph from separate files
- PASS entities as `[{name, entity_type}]` in kebab-case ASCII; PASS relationships as `[{source, target, relation, strength}]` where strength is in [0.0, 1.0]
- PASS `--strict-name` to REJECT a non-kebab-case name instead of auto-normalizing it
- PASS `--force-merge` for idempotent updates and soft-deleted restoration
- PASS `--replace-graph` together with `--force-merge` to ZERO the existing entity/relationship bindings before writing the new graph (full replace, not merge)
- PASS `--dry-run` to validate inputs without persisting
- VALID `--type` values: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- INVOKE `remember-batch` for 10 or more memories via NDJSON stdin; PASS `--transaction` for all-or-nothing
- INVOKE `ingest <DIR> --recursive --pattern "*.md" --mode none` to import a directory as body-only, then enrich SEPARATELY
- KNOW `ingest --mode` accepts `none` (default body-only), `claude-code`, `codex`; opencode is NOT an ingest mode, so enrich with opencode in a SEPARATE step
- USE `--resume` to continue from the queue after interruption; `--retry-failed` for failed items only; `--auto-describe` to synthesize descriptions
- PASS `--force-merge` on `ingest` to UPDATE duplicate files instead of skipping them; ingest dedups by `body_hash`, so an unchanged file is skipped even after a rename
- KNOW `ingest` natively auto-splits an oversized body into multiple chunks, so a file above the per-body limit is chunked, NOT rejected
- RESPECT the 512000 bytes and 512 chunks limit per body
- NEVER mix `--body`, `--body-file`, `--body-stdin`, `--graph-stdin` in a single invocation
- NEVER use `fd | xargs remember`; INVOKE `ingest` instead
- NEVER pass `--llm-backend codex` on any write; the entity path would force the codex subprocess and stall on its timeout; ALWAYS pass `--llm-backend none`


## CRUD Read Update Delete
- INVOKE `read --name <kebab> --json` for O(1) fetch; PASS `--with-graph` to include linked entities
- USE `read --name <n> --format raw` to print the pure body text with NO JSON envelope, ideal for piping into another tool
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
- INVOKE `unlink --memory <name> --entity <name>` to remove a single curated memory-to-entity binding without touching entity-to-entity edges
- INVOKE `graph entities --json` to list entities via `.entities[]` (NOT `.items[]`); ORDER with `--sort-by name|degree|created-at` plus `--order asc|desc` (default `asc`; when `--sort-by` is omitted the default is name ascending); USE `--order desc` for most-connected-first; PAGINATE with `--limit N --offset N`
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
- KNOW that graph writes are purely ADDITIVE: there is NO degree cap, so hubs grow unbounded and no write prunes edges; NORMALIZE only via explicit maintenance commands (`prune-relations`, `merge-entities`, `normalize-entities`), NEVER during a write
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
- INVOKE `enrich --operation <op> --mode <backend>` where BOTH flags are MANDATORY for any LLM operation; omitting `--mode` is rejected with exit 2 — EXCEPT the read-only inspectors `--status`, `--list-dead`, `--requeue-dead` and `--prune-dead-orphans`, which do NOT require `--operation` and `--mode`
- VALID `--operation` values: `memory-bindings`, `entity-descriptions`, `body-enrich`, `re-embed`, `augment-bindings`, `body-extract`
- VALID `--mode` values: `codex`, `claude-code`, `opencode`, `openrouter`
- USE `augment-bindings` to add MORE bindings to memories that are ALREADY linked; it REQUIRES `--names <a,b,c>` or `--names-file <path>` to scope the targets
- USE `body-extract --body-extract-graph-only` to extract the graph from a body READ-ONLY, persisting only entities and relationships without rewriting the body
- PASS `--codex-model`, `--claude-model`, `--opencode-model`, or `--openrouter-model` to pick the extraction model matching the chosen mode
- KNOW `--mode openrouter` requires `--openrouter-model` (no default), reads the key from `OPENROUTER_API_KEY`, makes a REST `/chat/completions` call with NO local CLI, sends `response_format` json_schema strict with `provider.require_parameters:true`, and bills tokens via `usage.cost`; the other three modes are OAuth or own-auth zero-token
- PASS `--limit N --resume` for `re-embed`; `--retry-failed` to reprocess only failed items; `--dry-run` to preview
- PASS `--min-output-chars N` to guard `body-enrich` output length; `--fallback-mode codex` to survive a Claude rate limit
- NEVER run `enrich` in parallel against the same database; it acquires a per-namespace singleton
- PASS `--until-empty` to loop scan->drain INTERNALLY until the eligible queue empties or `--max-runtime` expires, REPLACING the external bash drain loop
- PASS `--max-runtime <SECONDS>` to cap the `--until-empty` wall-clock budget; default 3600
- PASS `--max-attempts <N>` to bound Transient retries before an item turns `dead`; default 8, range 1..=20
- PASS `--status` for a read-only JSON report of `unbound_backlog`, `queue_pending/done/failed/dead/skipped`, `eligible_now` and `waiting`; it calls NO LLM and acquires NO singleton (and requires NO `--operation`/`--mode`)
- PASS `--rest-concurrency <N>` to set the REST fan-out for `--mode openrouter`; clamp 1..=16, default 8, DISTINCT from `--llm-parallelism`
- PASS `--list-dead` for a read-only JSON listing of every terminal `dead` item with its `error_class` and `message`; `--requeue-dead` moves those items back to `pending` for another pass; `--ignore-backoff` dequeues eligible items immediately, ignoring the `next_retry_at` cooldown
- PASS `--prune-dead-orphans` to delete ONLY enrich-queue rows where `status='dead'` and `item_type='memory'` whose `item_key` (memory name) is ABSENT from the main DB; entity-keyed dead rows are UNTOUCHED; the main DB is read-only — ONLY the sidecar `.enrich-queue.sqlite` is mutated; the JSON `DeadSummary` includes a `pruned` field with the count of rows removed; NO `--operation`/`--mode`/LLM flags needed — it is a pure SQLite inspector with no singleton acquisition; FORMULA: `sqlite-graphrag enrich --prune-dead-orphans --json`; USE this BEFORE `--requeue-dead` to clear memory-orphan dead rows (memory renamed or purged AFTER enqueue, `error_class=permanent` 'not found') that `--requeue-dead` alone would only re-fail
- KNOW the dead-letter queue HAS `error_class` and `next_retry_at` columns plus a terminal `dead` status: Transient failures (rate-limit, timeout, 5xx) reschedule with exponential backoff, HardFailures (validation, parse) go terminal at once, and dequeue skips `dead` so the live set strictly shrinks toward convergence
- KNOW the enrich queue lives in a sidecar database `.enrich-queue.sqlite` next to the main `.sqlite`
- STATUS formula: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --status --json` (no LLM call, no singleton)
- UNTIL-EMPTY formula: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --until-empty --max-runtime 3600 --max-attempts 8 --rest-concurrency 8 --json`


## Write Then Enrich — Two Separate Steps
- TREAT every write as STEP 1 (embed via OpenRouter, `--llm-backend none`) followed by a DISTINCT STEP 2 (`enrich`); NEVER chain them with `&&`
- CHOOSE the OpenRouter model from the price table; CHOOSE the enrich backend and model independently
- REMEMBER step 1: `echo '{"body":"text","entities":[{"name":"jwt","entity_type":"concept"}],"relationships":[{"source":"jwt","target":"auth-svc","relation":"uses","strength":0.8}]}' | sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend none remember --name <n> --type decision --description "desc" --graph-stdin --force-merge --json`
- REMEMBER step 2 codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini --json`
- REMEMBER step 2 claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --claude-model claude-sonnet-4-6 --json`
- REMEMBER step 2 opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --mode opencode --opencode-model opencode/big-pickle --json`
- REMEMBER step 2 openrouter: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --json` (key from `OPENROUTER_API_KEY`)
- REMEMBER-BATCH step 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model qwen/qwen3-embedding-8b --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend none remember-batch --transaction --json`
- REMEMBER-BATCH step 2 codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini --json`
- REMEMBER-BATCH step 2 claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --claude-model claude-sonnet-4-6 --json`
- REMEMBER-BATCH step 2 opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --mode opencode --opencode-model opencode/big-pickle --json`
- REMEMBER-BATCH step 2 openrouter: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --json` (key from `OPENROUTER_API_KEY`)
- INGEST step 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend none ingest ./docs --mode none --recursive --pattern "*.md" --type document --resume --json`
- INGEST step 2 codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini --json`
- INGEST step 2 claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --claude-model claude-sonnet-4-6 --json`
- INGEST step 2 opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --mode opencode --opencode-model opencode/big-pickle --json`
- INGEST step 2 openrouter: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --json` (key from `OPENROUTER_API_KEY`)
- EDIT step 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model perplexity/pplx-embed-v1-0.6b --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend none edit --name <n> --body-file new.md --json`
- EDIT step 2 codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini --json`
- EDIT step 2 claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --claude-model claude-sonnet-4-6 --json`
- EDIT step 2 opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --mode opencode --opencode-model opencode/big-pickle --json`
- EDIT step 2 openrouter: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --json` (key from `OPENROUTER_API_KEY`)
- RESTORE step 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-backend none restore --name <n> --version 2 --json`
- RESTORE step 2 codex: `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini --json`
- RESTORE step 2 claude: `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --claude-model claude-sonnet-4-6 --json`
- RESTORE step 2 opencode: `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation memory-bindings --mode opencode --opencode-model opencode/big-pickle --json`
- RESTORE step 2 openrouter: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --json` (key from `OPENROUTER_API_KEY`)


## Parallel Embedding and Enrich via OpenRouter — Multiprocessing
- SCALE REST embedding with `--llm-parallelism N`: it splits texts into chunks and dispatches them across a bounded JoinSet of N concurrent OpenRouter requests, preserving input order by chunk index
- SCALE REST enrich with `--rest-concurrency N` plus `--until-empty`: N concurrent `/chat/completions` calls drain the queue while the SQLite write stays serial via WAL plus atomic claim, the real bottleneck
- CLAMP `--llm-parallelism` to 1..32 and `--rest-concurrency` to 1..16; KEEP both in the Cloudflare-safe 4..16 band for paid models; `:free` models cap at 20 req/min, so USE a low N
- REMEMBER that multiple keys do NOT add capacity; the ceiling is the OpenRouter network plus the namespace singleton, NOT the key count
- REMEMBER parallel step 1: `echo '{"body":"...","entities":[...],"relationships":[...]}' | sqlite-graphrag --embedding-backend openrouter --embedding-model qwen/qwen3-embedding-8b --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-parallelism 8 --llm-backend none remember --name <n> --type decision --description "desc" --graph-stdin --force-merge --json`
- REMEMBER parallel step 2: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --rest-concurrency 8 --until-empty --max-runtime 3600 --max-attempts 8 --json`
- REMEMBER-BATCH parallel step 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model qwen/qwen3-embedding-8b --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-parallelism 12 --llm-backend none remember-batch --transaction --json`
- REMEMBER-BATCH parallel step 2: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model deepseek/deepseek-v4-flash:nitro --rest-concurrency 12 --until-empty --max-runtime 3600 --json`
- INGEST parallel step 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model nvidia/llama-nemotron-embed-vl-1b-v2:free --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-parallelism 6 --llm-backend none ingest ./docs --mode none --recursive --pattern "*.md" --type document --resume --json`
- INGEST parallel step 2: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b:nitro --rest-concurrency 12 --until-empty --max-runtime 7200 --max-attempts 8 --json`
- EDIT parallel step 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model qwen/qwen3-embedding-8b --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-parallelism 8 --llm-backend none edit --name <n> --body-file new.md --json`
- EDIT parallel step 2: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --rest-concurrency 8 --until-empty --json`
- RESTORE parallel step 1: `sqlite-graphrag --embedding-backend openrouter --embedding-model qwen/qwen3-embedding-8b --embedding-dim 384 --openrouter-api-key $OPENROUTER_API_KEY --llm-parallelism 8 --llm-backend none restore --name <n> --version 2 --json`
- RESTORE parallel step 2: `sqlite-graphrag enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --rest-concurrency 8 --until-empty --json`
- MONITOR convergence between steps with `enrich --operation memory-bindings --mode openrouter --openrouter-model openai/gpt-oss-120b --status --json`; when `eligible_now` is 0 and `queue_pending` is 0, the queue has converged
- INSPECT terminal items with `--status`: `queue_dead` lists HardFailures that will NEVER be reprocessed; treat them as data debt, not a transient error


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
- EMBEDDING: `embedding --status --json` for counts plus a `coverage` object reporting the real vector counts per table; `pending-embeddings --status --json` then `pending-embeddings process --json` to reprocess failures
- SLOTS: `slots status --json` to inspect the host semaphore; `slots release --slot-id <N> --yes` for orphans
- PENDING: `pending list --filter-status queued --json`; `pending show <id>`; `pending cleanup --yes`
- EXPORT: `export --namespace <ns> --type <kind> --json` as NDJSON; STATS: `stats --json` for counts and sizes, including a top-level `total_memories`
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
