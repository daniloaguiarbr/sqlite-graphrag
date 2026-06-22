---
name: sqlite-graphrag
description:For persistent memory, GraphRAG, or long-term context in Claude Code, Codex, OpenCode, Cursor, Windsurf, AI agents. On: remember, save, recall, hybrid search, entity graph, SQLite memory, local RAG, LLM-only embedding, OAuth, BLOB-backed, migrate, embedding-dim, llm-parallelism, re-embed, force-reembed, OAuth-only, codex claude opencode hardening, mock LLM CI, ADR-0041 ADR-0051, OpenRouter Bedrock, --dry-run-backend, backend_invoked, preflight exit 16, --llm-backend codex claude opencode none auto, --llm-model gpt-5.4-mini claude-sonnet-4-6 opencode/big-pickle, headless fallback chain, v1.0.86-v1.0.90. KW: memory RAG GraphRAG SQLite one-shot OAuth offline persistent graph entity opencode codex claude.
---


## Current Version (v1.0.90)
- Current binary version: v1.0.90 (released 2026-06-22)
- Current schema version: v15 (after init or migrate on fresh database)
- This skill documents v1.0.86 through v1.0.90 features
- Earlier versions (v1.0.85.2 and below) are out of scope
- For older versions, CONSULT git history of this skill


## When This Skill Activates
- ACTIVATE when the user asks to remember, save, recall, retrieve, search, or persist anything across sessions
- ACTIVATE for long-term context, knowledge graph, GraphRAG, RAG, entity linking, memory management
- ACTIVATE when sqlite, sqlite-graphrag, embedding, FTS5, hybrid-search, or LLM memory is mentioned
- NEVER ACTIVATE for one-off ephemeral data, simple file I/O, or tasks unrelated to persistent context


## LLM Prompt Instruction Rules
- WHEN user says "remember this", "save this", "persist this" → EXECUTE `remember --force-merge` with `--graph-stdin` including curated entities and canonical relations
- WHEN user asks "what do you know about X", "recall X", "search for X" → EXECUTE `hybrid-search "X" --k 10 --json` FIRST, then EXPAND top results with `read --name <name> --json`
- WHEN user asks "how is X related to Y" → EXECUTE `graph traverse --from X --depth 2 --json` or `related X --hops 2 --json`
- WHEN user asks "deep research on X" → EXECUTE `deep-research "X" --k 20 --max-hops 3 --json`
- BEFORE creating ANY memory → EXECUTE `hybrid-search "<name or description>" --k 5 --json` to CHECK for duplicates; if found, USE `--force-merge` to UPDATE instead of creating new
- AFTER creating or updating a memory → VERIFY with `read --name <name> --json | jaq '{name, description, body_length}'`
- AFTER EVERY turn with new findings → EVALUATE whether to persist via `remember --force-merge`; if nothing new, DECLARE "No new findings to persist this turn"
- WHEN exit code is non-zero → READ JSON error envelope from stdout (`jaq '{code, message, error_class}'`), REPORT to user with remediation steps
- WHEN exit code 9 (duplicate) → RETRY with `--force-merge` flag
- WHEN exit code 19 (SHUTDOWN) → RETRY MANDATORY; partial work was discarded
- WHEN exit code 75 (singleton locked) → WAIT and retry; do NOT increase concurrency
- WHEN exit code 16 (preflight) → FIX the MCP config issue; do NOT bypass with `SKIP_PREFLIGHT`
- ALWAYS parse JSON output with `jaq` (NEVER `jq`)
- ALWAYS pass `--json` flag on every `sqlite-graphrag` invocation
- ALWAYS use `--llm-backend codex --llm-model gpt-5.4-mini` OR `--llm-backend claude --llm-model claude-sonnet-4-6` OR `--llm-backend opencode --llm-model opencode/big-pickle` for embedding commands
- ALWAYS use canonical relations ONLY: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- ALWAYS map non-canonical relations BEFORE persisting: `adds|creates → causes`, `implements → supports`, `blocks → contradicts`, `tested-by → related`, `part-of → applies-to`
- ALWAYS normalize entity names to kebab-case ASCII lowercase BEFORE passing to CLI
- NEVER use MCP Serena or `.md` memory files for persistence
- NEVER write MEMORY.md or any file-based memory
- NEVER start or reference daemon (removed in v1.0.76)
- NEVER pass `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` in environment
- PREFER `remember --force-merge` over `edit` for updates to ensure re-indexation
- PREFER `--graph-stdin` with curated entities over `--enable-ner` for quality extraction
- LIMIT graph entities to domain-specific concepts: projects, tools, people, decisions, files, incidents
- REJECT generic words, pronouns, UUIDs, hashes, timestamps as entity names
```bash
# RULE: ALWAYS check duplicates BEFORE creating memory
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  hybrid-search "auth JWT design" --k 5 --json | jaq '.results[].name'
# If found: UPDATE with --force-merge
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name auth-design --type decision --force-merge \
  --description "JWT rotation strategy updated" --body-file auth.md

# RULE: ALWAYS use --graph-stdin with curated entities and canonical relations
echo '{"body":"JWT with 15-min expiry","entities":[{"name":"jwt","entity_type":"concept"},{"name":"auth-service","entity_type":"tool"}],"relationships":[{"source":"auth-service","target":"jwt","relation":"uses","strength":0.9}]}' \
  | sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
    remember --name auth-design --type decision --description "JWT rotation" --graph-stdin --force-merge

# RULE: ALWAYS verify after write
sqlite-graphrag read --name auth-design --json | jaq '{name, description, body_length}'

# RULE: WHEN exit 9 (duplicate) → retry with --force-merge
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name existing-mem --type note --description "x" --body "y" --force-merge
```


## Fundamental Principles
- INVOKE always as subprocess via `std::process::Command`
- READ stdout for JSON or NDJSON structured data
- READ stderr for tracing logs and human messages
- CHECK exit code BEFORE parsing stdout
- TRUST JSON contracts as SemVer-versioned API
- KNOW BUILD is LLM-only and one-shot; binary is 14.6 MiB stripped ELF (NOT 6 MB as in older docs)
- KNOW BUILD has NO daemon, NO ONNX runtime, NO model cache
- ENFORCE OAUTH-ONLY: spawn ABORTS exit 1 if `ANTHROPIC_API_KEY` is set
- ENFORCE OAUTH-ONLY: spawn ABORTS exit 1 if `OPENAI_API_KEY` is set
- ISOLATE NAMESPACE per project via `--namespace <ns>` or env
- KNOW NAMESPACE default is `global` when omitted
- NEVER expose the binary as MCP server or HTTP service
- NEVER write `.sqlite` file in parallel to the binary
- NEVER edit the `.sqlite` file from another tool

```bash
# INVOKE as subprocess, CHECK exit code, then PARSE stdout
sqlite-graphrag health --json
echo "exit=$?"

# READ structured stdout with jaq, NEVER parse stderr as data
sqlite-graphrag health --json | jaq '.integrity_ok'
```


## Quick Reference Card
- INIT first time: `sqlite-graphrag init --namespace <ns>`
- VERIFY health: `sqlite-graphrag health --json | jaq '.integrity_ok'`
- STORE memory (codex): `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini remember --name <kebab> --type note --description "x" --body "y"`
- STORE memory (claude): `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 remember --name <kebab> --type note --description "x" --body "y"`
- BATCH STORE (codex): `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini remember-batch --json < batch.ndjson`
- BATCH STORE (claude): `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 remember-batch --json < batch.ndjson`
- STORE memory (opencode): `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle remember --name <kebab> --type note --description "x" --body "y"`
- BATCH STORE (opencode): `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle remember-batch --json < batch.ndjson`
- EDIT memory (codex): `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini edit --name <n> --body-file <path>`
- EDIT memory (claude): `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 edit --name <n> --body-file <path>`
- EDIT memory (opencode): `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle edit --name <n> --body-file <path>`
- INGEST folder (codex): `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini ingest ./docs --mode codex --recursive --pattern "*.md" --json`
- INGEST folder (claude): `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 ingest ./docs --mode claude-code --recursive --pattern "*.md" --json`
- INGEST folder (opencode): `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle ingest ./docs --mode opencode --recursive --pattern "*.md" --json`
- SEARCH semantic (codex): `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini recall "query" --k 5 --json`
- SEARCH semantic (claude): `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 recall "query" --k 5 --json`
- SEARCH semantic (opencode): `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle recall "query" --k 5 --json`
- SEARCH hybrid (codex): `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini hybrid-search "query" --k 10 --rrf-k 60 --json`
- SEARCH hybrid (claude): `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 hybrid-search "query" --k 10 --rrf-k 60 --json`
- SEARCH hybrid (opencode): `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle hybrid-search "query" --k 10 --rrf-k 60 --json`
- DEEP RESEARCH (codex): `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini deep-research "question" --k 20 --max-hops 3 --json`
- DEEP RESEARCH (claude): `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 deep-research "question" --k 20 --max-hops 3 --json`
- DEEP RESEARCH (opencode): `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle deep-research "question" --k 20 --max-hops 3 --json`
- ENRICH graph (codex): `sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini enrich --operation re-embed --limit 100 --resume --json`
- ENRICH graph (claude): `sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 enrich --operation memory-bindings --mode claude-code --json`
- ENRICH graph (opencode): `sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle enrich --operation re-embed --limit 100 --resume --json`
- READ memory (no backend): `sqlite-graphrag read --name <n> --json`
- LIST memories (no backend): `sqlite-graphrag list --type decision --limit 50 --json`
- GRAPH traversal (no backend): `sqlite-graphrag graph traverse --from <entity> --depth 2 --json`
- GRAPH stats (no backend): `sqlite-graphrag graph stats --json`
- LINK entities (no backend): `sqlite-graphrag link --from <a> --to <b> --relation uses --create-missing --json`
- HARD DELETE (no backend): `sqlite-graphrag forget --name <n>` then `sqlite-graphrag purge --retention-days 30 --yes`


## Initialization, Health, and Global Config
- RUN `sqlite-graphrag init --namespace <ns>` on first use
- RUN `health --json` to verify `integrity_ok` and `schema_ok`
- VERIFY `schema_version >= 15` after `init` or `migrate`
- RUN `migrate --json` after each binary upgrade
- USE `migrate --to-llm-only --drop-vec-tables --json` for v1.0.74 or v1.0.75 databases
- USE `migrate --rehash --json` to repair V002 SipHasher13 checksum drift
- USE `migrate --dry-run --json` to PREVIEW pending migrations without applying
- TREAT exit code 10 as database error; RUN `vacuum` and `health`
- TREAT exit code 15 as busy; WIDEN `--wait-lock`
- TREAT exit code 16 as preflight failure (v1.0.87+); FIX MCP config or SET `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1`
- ABORT pipeline when `integrity_ok` returns `false`
- RUN `optimize --json` to refresh planner stats; response includes `fts_rebuilt`
- USE `optimize --skip-fts --json` when FTS5 was recently rebuilt
- RUN `fts rebuild --json` when `health.fts_degraded` is true
- INSPECT `wal_size_mb` in `health` for fragmentation
- VERIFY `journal_mode` equals `wal` in production
- USE `debug-schema --json` for troubleshooting schema drift
- PASS `--db <PATH>` to override database location (now accepted on `embedding status/list/abandon`, `pending list/show` since v1.0.89, ADR-0049)
- PASS `--namespace <NS>` on `health` since v1.0.89 to filter counts to one namespace
- SET `SQLITE_GRAPHRAG_DB_PATH` env for persistent config
- SET `SQLITE_GRAPHRAG_NAMESPACE` env for persistent namespace
- PASS `--lang en` or `--lang pt` to force stderr language
- PASS `--tz America/Sao_Paulo` to localize timestamps
- SET `SQLITE_GRAPHRAG_DISPLAY_TZ` env for persistent timezone
- SET `SQLITE_GRAPHRAG_LOG_FORMAT=json` for log aggregators
- USE `-v` for info, `-vv` for debug, `-vvv` for trace
- ENABLE `SQLITE_GRAPHRAG_LOW_MEMORY=1` in constrained containers
- SET `SQLITE_GRAPHRAG_EMBEDDING_DIM` env in range `[8, 4096]` (default 64 MRL)
- SET `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` for compliance mode (ADR-0041)
- SET `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1` ONLY for CI test harnesses
- KNOW VALID `--type` values: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- KNOW GLOBAL flags: `--db`, `--namespace`, `--lang`, `--tz`, `--json`, `--low-memory`, `--max-concurrency N`, `--wait-lock SECS`, `--llm-parallelism N`, `--llm-backend claude|codex|opencode|none|auto[,fallback...]`, `--llm-model <MODEL>`, `--dry-run-backend`, `--llm-fallback-mode <claude|codex|opencode>`, `--graceful-shutdown-secs N`, `--claude-binary <PATH>`, `--codex-binary <PATH>`, `--opencode-binary <PATH>`, `--opencode-model <MODEL>`, `--opencode-timeout <SECS>`, `--skip-embedding-on-failure`

```bash
# INIT a project namespace
sqlite-graphrag init --namespace myproject

# VERIFY integrity and schema version
sqlite-graphrag health --json | jaq '{integrity_ok, schema_ok, schema_version}'

# MIGRATE after binary upgrade, then PREVIEW first
sqlite-graphrag migrate --dry-run --json | jaq '.would_apply[]? | {name, version}'
sqlite-graphrag migrate --json

# OPTIMIZE planner stats and rebuild FTS5 if degraded
sqlite-graphrag optimize --json | jaq '{fts_rebuilt}'
sqlite-graphrag fts rebuild --json

# OVERRIDE database location and namespace per invocation
sqlite-graphrag --db /data/prod.sqlite --namespace prod health --json | jaq '.counts'
```


## Architecture Contract (OAuth/LLM/One-Shot)
- KNOW BUILD is LLM-only; default build has NO `fastembed`, `ort`, `ndarray`, `tokenizers`, `huggingface-hub`, `sqlite-vec`, `GLiNER`
- KNOW BUILD removed `daemon` subcommand entirely (ADR-0021)
- KNOW COSINE similarity is pure Rust in `src/similarity.rs`
- KNOW COSINE runs over BLOB-backed `memory_embeddings`, `entity_embeddings`, `chunk_embeddings`
- KNOW SCHEMA v15 after `init` or `migrate` on fresh database
- KNOW MIGRATION V013 drops `vec_memories`, `vec_entities`, `vec_chunks` virtual tables
- KNOW MIGRATION V014 creates `pending_memories` checkpoint table
- KNOW MIGRATION V015 creates `pending_embeddings` retry table
- ENFORCE OAUTH-ONLY: `ANTHROPIC_API_KEY` ABORTS spawn with `AppError::Validation` (ADR-0011)
- ENFORCE OAUTH-ONLY: `OPENAI_API_KEY` ABORTS spawn with `AppError::Validation` (ADR-0011)
- KNOW OAUTH-ONLY: both API keys EXCLUDED from env-clear whitelist
- KNOW OAUTH-ONLY: `--bare` flag REMOVED from all executable paths
- KNOW OAUTH-ONLY: 7 hardening flags ALWAYS passed to `claude -p`
- KNOW HARDENING flags for claude: `--model claude-sonnet-4-6 --strict-mcp-config --mcp-config '{}' --settings '{"hooks":{}}' --dangerously-skip-permissions --output-schema`
- KNOW HARDENING flags for codex: `--model gpt-5.5 --json --output-schema --ephemeral --skip-git-repo-check --sandbox read-only --ignore-user-config --ignore-rules -c mcp_servers='{}' --ask-for-approval never`
- KNOW ADR-0041 v1.0.83: `ANTHROPIC_AUTH_TOKEN` PRESERVED for Anthropic-compatible providers
- KNOW ADR-0041 v1.0.83: `ANTHROPIC_BASE_URL` PRESERVED for custom endpoints
- KNOW ADR-0041 v1.0.83: `OPENAI_BASE_URL` PRESERVED for OpenAI-compatible endpoints
- KNOW ADR-0041 v1.0.83: `CLAUDE_CODE_ENTRYPOINT`, `DISABLE_TELEMETRY`, `OTEL_EXPORTER_OTLP_ENDPOINT` PRESERVED
- KNOW ADR-0041 v1.0.83: supported providers include OpenRouter, AWS Bedrock, corporate gateways
- KNOW EMBEDDING DIM precedence: `SQLITE_GRAPHRAG_EMBEDDING_DIM` env then `schema_meta.dim` then default 64 MRL
- KNOW EMBEDDING DIM adapts batch size: base 8 chunks / 25 entity names at dim 64
- USE MOCK LLM CLI for CI: prepend `tests/mock-llm` to PATH
- USE SHUTDOWN bypass recipe: `PATH=tests/mock-llm:$PATH SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1 setsid -w timeout 120 sqlite-graphrag …`
- NEVER install with `--features embedding-legacy` or `--features ner-legacy`
- NEVER depend on daemon or `--bare` flag (REMOVED in v1.0.76 and v1.0.79)
- NEVER mix `vec_memories` queries (REMOVED in v1.0.76)
- NEVER call `migrate --to-llm-only` without `--drop-vec-tables` safety guard

```bash
# CONFIRM OAuth-only enforcement: setting an API key ABORTS the spawn
ANTHROPIC_API_KEY=sk-test sqlite-graphrag init 2>&1 || echo "exit=$?"

# CONFIRM the build is LLM-only (no vec tables, BLOB-backed embeddings)
sqlite-graphrag debug-schema --json | jaq '.tables'

# DROP vec tables ONLY with the safety guard present
sqlite-graphrag migrate --to-llm-only --drop-vec-tables --json
```


## Backend LLM Selection — Codex and Claude Headless
- NOTE: for OpenCode backend examples and env vars, SEE the dedicated section "Backend LLM Selection — OpenCode Headless (v1.0.90)" below
- MANDATE selecting the embedding backend explicitly via `--llm-backend` for every embedding command
- PASS `--llm-backend codex` to spawn OpenAI Codex CLI headless for embedding and extraction
- PASS `--llm-backend claude` to spawn Claude Code CLI headless via `embed_via_claude_local` (zero-token, OAuth-compatible)
- PASS `--llm-backend codex,claude` for codex-first with claude fallback (ADR-0038)
- PASS `--llm-backend codex,claude,none` to fall back to null embedding when both backends fail
- PASS `--llm-backend none` ONLY when you DEMAND zero embedding and accept a non-searchable memory
- KNOW DEFAULT `--llm-backend` is `codex`
- PASS `--llm-model <MODEL>` to select the model for the active backend (v1.0.89, ADR-0050)
- KNOW DEFAULT model for codex backend is `gpt-5.5`; for claude backend is `claude-sonnet-4-6`
- USE `--llm-model gpt-5.4-mini` for fast, cheap codex embedding; `--llm-model gpt-5.5` for highest codex quality
- USE `--llm-model claude-sonnet-4-6` for balanced claude embedding
- PASS `--llm-fallback-mode <claude|codex>` to swap backend mid-job on rate-limit
- PASS `--dry-run-backend` to plan backend operation without executing it (idempotent preview)
- PARSE `backend_invoked` field in every embedding envelope to CONFIRM which backend actually ran
- PASS `--codex-binary <PATH>` to override codex binary location (v1.0.89, ADR-0050)
- PASS `--claude-binary <PATH>` to override claude binary location (propagated via set_var since v1.0.89)
- SET env `SQLITE_GRAPHRAG_LLM_BACKEND` for persistent backend selection
- SET env `SQLITE_GRAPHRAG_LLM_MODEL` for persistent model selection
- SET env `SQLITE_GRAPHRAG_CODEX_BINARY` for persistent codex binary path
- SET env `SQLITE_GRAPHRAG_CODEX_EMBED_MODEL` for persistent codex embedding model
- USE `LlmEmbeddingBuilder` to compose embedding pipeline: `with_backend(Codex).or_fallback(Claude).or_skip()`
- RUN `codex login` after upgrade to refresh OAuth refresh token (2026-06-14 incident)
- RUN `claude` OAuth refresh when claude backend reports stale OAuth
- NEVER pass `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` with either backend; spawn ABORTS exit 1

```bash
# CODEX HEADLESS — explicit backend and fast model
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name auth-design --type decision \
  --description "JWT rotation strategy" --body "15-min expiry with refresh"

# CLAUDE CODE HEADLESS — explicit backend and balanced model
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  remember --name auth-design --type decision \
  --description "JWT rotation strategy" --body "15-min expiry with refresh"

# CODEX EXCLUSIVE — fail on error, no fallback
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name auth-design --type decision \
  --description "JWT rotation strategy" --body-file auth.md

# CODEX-FIRST WITH CLAUDE FALLBACK
sqlite-graphrag --llm-backend codex,claude --llm-model gpt-5.5 \
  remember --name auth-design --type decision \
  --description "JWT rotation strategy" --body-file auth.md

# CODEX-FIRST, CLAUDE FALLBACK, THEN NULL EMBEDDING (last resort)
sqlite-graphrag --llm-backend codex,claude,none --skip-embedding-on-failure \
  remember --name auth-design --type decision \
  --description "JWT rotation strategy" --body-file auth.md

# DRY-RUN BACKEND — plan without executing, confirm backend choice
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini --dry-run-backend \
  remember --name preview --type note --description x --body y | jaq '.backend_invoked'

# SWAP BACKEND MID-JOB ON RATE LIMIT
sqlite-graphrag --llm-backend codex --llm-fallback-mode claude \
  enrich --operation re-embed --limit 200 --resume --json

# CONFIRM EFFECTIVE BACKEND from the envelope
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  recall "auth" --k 3 --json | jaq '.backend_invoked'
```

```bash
# PERSISTENT codex backend via env vars
export SQLITE_GRAPHRAG_LLM_BACKEND=codex
export SQLITE_GRAPHRAG_LLM_MODEL=gpt-5.4-mini
export SQLITE_GRAPHRAG_CODEX_EMBED_MODEL=gpt-5.4-mini

# PERSISTENT claude backend via env vars
export SQLITE_GRAPHRAG_LLM_BACKEND=claude
export SQLITE_GRAPHRAG_LLM_MODEL=claude-sonnet-4-6

# OVERRIDE codex binary path persistently
export SQLITE_GRAPHRAG_CODEX_BINARY=/usr/local/bin/codex

# OVERRIDE claude binary path per invocation
sqlite-graphrag --claude-binary /usr/local/bin/claude --llm-backend claude \
  remember --name x --type note --description "x" --body "y"

# OVERRIDE codex binary path per invocation
sqlite-graphrag --codex-binary /usr/local/bin/codex --llm-backend codex \
  remember --name x --type note --description "x" --body "y"
```


## Backend LLM Selection — OpenCode Headless (v1.0.90)
- PASS `--llm-backend opencode` to spawn OpenCode CLI headless for embedding and extraction
- PASS `--llm-backend codex,claude,opencode,none` for full fallback chain
- KNOW opencode is the THIRD priority in auto-detect: codex > claude > opencode > none
- KNOW opencode uses its OWN auth system (NOT OAuth); `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` are NOT required
- KNOW opencode has NO `--output-schema` or `--json-schema` flag; structured output relies on role-setting prompts + JSON parsing
- KNOW opencode NDJSON output has 3 event types: `step_start`, `text` (`.part.text`), `step_finish`
- SET env `SQLITE_GRAPHRAG_OPENCODE_BINARY` for persistent binary path override
- SET env `SQLITE_GRAPHRAG_OPENCODE_MODEL` for persistent model selection (default: `opencode/big-pickle`)
- SET env `SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL` for persistent embedding model
- SET env `SQLITE_GRAPHRAG_OPENCODE_TIMEOUT` for persistent timeout (default: 300s)
- PASS `--opencode-binary <PATH>` to override binary location per invocation
- PASS `--opencode-model <MODEL>` for ingest/enrich model selection
- PASS `--opencode-timeout <SECONDS>` for ingest/enrich timeout
- PASS `--mode opencode` for ingest and enrich pipelines
- KNOW opencode embedding uses role-setting prompt "You are an embedding function" to produce real numeric vectors
- KNOW `SQLITE_GRAPHRAG_OPENCODE_MODEL` does NOT fall back to `SQLITE_GRAPHRAG_LLM_MODEL` (cross-contamination fix v1.0.90 audit)
- KNOW `propagate_opencode_env()` forwards OPENCODE_*, OPENROUTER_*, XDG_*, LANG, TERM, USER, LOGNAME, TMPDIR to subprocess
- KNOW opencode free models: `opencode/big-pickle`, `opencode/deepseek-v4-flash-free`, `opencode/mimo-v2.5-free`, `opencode/nemotron-3-ultra-free`, `opencode/north-mini-code-free`
- KNOW minimum opencode version: 1.17.0

```bash
# OPENCODE HEADLESS — explicit backend and free model
sqlite-graphrag --llm-backend opencode \
  remember --name auth-design --type decision \
  --description "JWT rotation strategy" --body "15-min expiry with refresh"

# OPENCODE WITH SPECIFIC MODEL
sqlite-graphrag --llm-backend opencode --llm-model opencode/deepseek-v4-flash-free \
  remember --name auth-design --type decision \
  --description "JWT rotation strategy" --body-file auth.md

# FULL FALLBACK CHAIN: codex first, then claude, then opencode, then none
sqlite-graphrag --llm-backend codex,claude,opencode,none --skip-embedding-on-failure \
  remember --name auth-design --type decision \
  --description "JWT rotation strategy" --body-file auth.md

# INGEST WITH OPENCODE EXTRACTION
sqlite-graphrag ingest ./docs --mode opencode --recursive --json

# INGEST WITH SPECIFIC OPENCODE MODEL AND TIMEOUT
sqlite-graphrag ingest ./docs --mode opencode --opencode-model opencode/mimo-v2.5-free \
  --opencode-timeout 600 --recursive --json

# ENRICH WITH OPENCODE
sqlite-graphrag enrich --operation memory-bindings --mode opencode --json

# DRY-RUN WITH OPENCODE BACKEND
sqlite-graphrag --llm-backend opencode --dry-run-backend \
  remember --name preview --type note --description x --body y | jaq '.backend_invoked'
```

```bash
# PERSISTENT opencode backend via env vars
export SQLITE_GRAPHRAG_LLM_BACKEND=opencode
export SQLITE_GRAPHRAG_OPENCODE_MODEL=opencode/big-pickle
export SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL=opencode/big-pickle

# OVERRIDE opencode binary path
export SQLITE_GRAPHRAG_OPENCODE_BINARY=~/.opencode/bin/opencode
```


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
- USE `--auto-describe` (default true since v1.0.89) to extract description from first significant body line; OPT OUT via `--no-auto-describe`
- INVOKE `ingest --mode claude-code` for LLM-curated entity extraction
- INVOKE `ingest --mode codex` for OpenAI Codex-curated extraction
- EXPECT claude-code events: `entities` count, `rels` count, `cost_usd` (Omit cost for OAuth)
- USE `--resume` to continue from queue DB after interruption
- USE `--retry-failed` to retry only failed files
- NEVER use `fd | xargs remember`; INVOKE `ingest` instead
- NEVER mix `--body`, `--body-file`, `--body-stdin`, `--graph-stdin` in single invocation
- NEVER pass empty body with no entities via `--graph-stdin` (exit 1 since v1.0.54)
- NEVER use `--force-merge` in `ingest` (exclusive to `remember`)
- NEVER mix different memory types in same `ingest` invocation

```bash
# REMEMBER via stdin (codex backend) — long body
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name design-auth --type decision \
  --description "auth JWT" --body-stdin < doc.md

# REMEMBER via stdin (claude backend) — long body
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  remember --name design-auth --type decision \
  --description "auth JWT" --body-stdin < doc.md

# REMEMBER via stdin (opencode backend) — long body
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  remember --name design-auth --type decision \
  --description "auth JWT" --body-stdin < doc.md

# REMEMBER from file with idempotent merge (codex)
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name doc-readme --type document \
  --description "import" --body-file README.md --force-merge

# REMEMBER from file with idempotent merge (claude)
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  remember --name doc-readme --type document \
  --description "import" --body-file README.md --force-merge

# REMEMBER from file with idempotent merge (opencode)
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  remember --name doc-readme --type document \
  --description "import" --body-file README.md --force-merge

# ATTACH curated graph in a single JSON via --graph-stdin (codex)
echo '{"body":"JWT rotation","entities":[{"name":"jwt","entity_type":"concept"}],"relationships":[{"source":"jwt","target":"auth-service","relation":"uses","strength":0.8}]}' \
  | sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
    remember --name spec-x --type reference --description "spec" --graph-stdin

# DRY-RUN to validate inputs without persisting
sqlite-graphrag remember --name spec-x --type reference --description "spec" --body "x" --dry-run

# REMEMBER-BATCH 10+ memories via NDJSON (codex backend)
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember-batch --force-merge --json < batch.ndjson | jaq -c 'select(.summary != true)'

# REMEMBER-BATCH 10+ memories via NDJSON (claude backend)
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  remember-batch --force-merge --json < batch.ndjson | jaq -c 'select(.summary != true)'

# REMEMBER-BATCH 10+ memories via NDJSON (opencode backend)
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  remember-batch --force-merge --json < batch.ndjson | jaq -c 'select(.summary != true)'

# INGEST a directory with Codex extraction
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  ingest ./docs --mode codex --recursive --pattern "*.md" --json

# INGEST a directory with Claude Code extraction
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  ingest ./docs --mode claude-code --recursive --pattern "*.md" --json

# INGEST a directory with OpenCode extraction
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  ingest ./docs --mode opencode --recursive --pattern "*.md" --json

# INGEST with auto-describe and parallelism, RESUME after interruption (codex)
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  ingest ./corpus --mode codex --recursive --auto-describe \
  --llm-parallelism 2 --max-files 1000 --resume --json | jaq -c 'select(.status == "done")'
```


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
- TREAT exit code 3 as optimistic lock conflict; RELOAD `read --json` and RETRY
- INVOKE `rename --from <old> --to <new>` to rename preserving history
- TREAT exit 1 when new name equals old name (v1.0.64)
- INVOKE `restore --name <n> --version <N>` to restore old version
- OMIT `--version` to select last non-restore version automatically
- EXPECT each `edit` or `restore` to create new immutable version
- EXPECT FTS5 desync fix applied (v1.0.56) so edited memories are immediately findable
- NEVER skip optimistic locking in concurrent pipelines

```bash
# READ memory by name (no backend needed for read)
sqlite-graphrag read --name design-auth --json | jaq '{description, body_length}'

# READ memory by id, with linked graph
sqlite-graphrag read --id 42 --with-graph --json | jaq '{name, entities, relationships}'

# LIST decisions, paginated
sqlite-graphrag list --type decision --limit 50 --offset 0 --json | jaq '.items[].name'

# INSPECT version history with character diffs
sqlite-graphrag history --name design-auth --diff --json | jaq '.versions[] | {version, changes}'

# EDIT body from file (codex backend re-embeds)
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  edit --name design-auth --body-file ./revised.md \
  --expected-updated-at "2026-04-19T12:00:00Z"

# EDIT body from file (claude backend re-embeds)
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  edit --name design-auth --body-file ./revised.md

# EDIT body from file (opencode backend re-embeds)
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  edit --name design-auth --body-file ./revised.md

# FORCE-REEMBED without body change (codex)
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  edit --name design-auth --force-reembed --json | jaq '.backend_invoked'

# FORCE-REEMBED without body change (opencode)
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  edit --name design-auth --force-reembed --json | jaq '.backend_invoked'

# RENAME preserving history (no backend needed)
sqlite-graphrag rename --from old-name --to new-name --json

# RESTORE an old version (no backend needed; auto re-embed runs)
sqlite-graphrag restore --name design-auth --version 2 --json
```


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
- NEVER delete manually via `sqlite3` shell; INVOKE binary commands only

```bash
# FORGET (soft-delete, reversible)
sqlite-graphrag forget --name old-design --json

# PURGE physically after auditing count first
sqlite-graphrag purge --retention-days 30 --yes --dry-run
sqlite-graphrag purge --retention-days 30 --yes

# UNLINK a specific edge
sqlite-graphrag unlink --from jwt --to auth-service --relation uses --json

# UNLINK ALL edges between two entities
sqlite-graphrag unlink --from jwt --to auth-service --json

# BULK-REMOVE all relationships for one entity
sqlite-graphrag unlink --entity jwt --all --json

# PRUNE noisy relations in bulk, preview affected entities first
sqlite-graphrag prune-relations --relation mentions --show-entities --dry-run --json
sqlite-graphrag prune-relations --relation mentions --yes --json

# CLEANUP orphaned entities, then vacuum
sqlite-graphrag cleanup-orphans --yes --json
sqlite-graphrag vacuum --json
```


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
- KNOW `--cascade` is REQUIRED when entity has relationships (else exit 1)
- INVOKE `merge-entities --names "a,b,c" --into <target>` to merge entities
- INVOKE `reclassify --name <n> --new-type <kind>` for single entity reclassification
- INVOKE `reclassify --from-type <old> --to-type <new> --batch` for bulk reclassification
- INVOKE `reclassify-relation --from-relation <old> --to-relation <new> --batch`
- INVOKE `normalize-entities --yes` to normalize all names to kebab-case ASCII
- VALIDATE names: minimum 2 chars, no newlines, no short ALL_CAPS (4 chars or less rejected since v1.0.88 BUG-13 fix)
- NORMALIZE names via NFKD then ASCII then lowercase then hyphens
- KNOW CANONICAL relations: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- KNOW NON-CANONICAL mapping: `adds|creates → causes`, `implements → supports`, `blocks → contradicts`, `tested-by → related`, `part-of → applies-to`
- KNOW CANONICAL entity types: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`
- NEVER use `mentions` as default relation (adds noise)
- NEVER persist ephemeral state in entities

```bash
# LINK two entities, auto-creating them if absent (no backend needed)
sqlite-graphrag link --from jwt --to auth-service --relation uses \
  --weight 0.8 --create-missing --entity-type concept --json

# LIST entities sorted by degree, descending
sqlite-graphrag graph entities --sort-by degree --order desc --limit 20 --json \
  | jaq -r '.entities[] | "\(.name) \(.degree)"'

# INSPECT graph stats before deciding traversal depth
sqlite-graphrag graph stats --json | jaq '{node_count, edge_count, avg_degree, max_degree}'

# TRAVERSE a 2-hop subgraph from a root entity
sqlite-graphrag graph traverse --from jwt --depth 2 --json \
  | jaq -r '.hops[] | "\(.entity) \(.relation) (depth \(.depth))"'

# EXPORT the graph as mermaid to a file
sqlite-graphrag graph --format mermaid --output graph.mmd --json

# FORWARD lookup: which entities a memory links to
sqlite-graphrag memory-entities --name design-auth --json | jaq '.entities[].name'

# REVERSE lookup: which memories link to an entity
sqlite-graphrag memory-entities --entity jwt --json | jaq '.memories[].name'

# RENAME an entity preserving relationships
sqlite-graphrag rename-entity --name auth --new-name authentication --json

# DELETE an entity and cascade its relationships
sqlite-graphrag delete-entity --name stale-entity --cascade --json

# MERGE near-duplicate entities into one
sqlite-graphrag merge-entities --names "auth,authn,authentication" --into authentication --json

# NORMALIZE all entity names to kebab-case ASCII
sqlite-graphrag normalize-entities --yes --json | jaq '{normalized_count, merged_count}'
```


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
- EXPECT `hybrid-search` response: `results[]`, `graph_matches[]`, `fts_degraded`, `vec_degraded_reason?`, `backend_invoked`, `elapsed_ms`
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
- KNOW OPERATIONS: `memory-bindings`, `entity-descriptions`, `body-enrich` (Jaccard >=0.7), `re-embed --limit N --resume`
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

```bash
# RECALL with Codex embedding — pure semantic KNN
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  recall "JWT authentication" --k 5 --json | jaq '.results[] | {name, score}'

# RECALL with Claude embedding — pure semantic KNN
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  recall "JWT authentication" --k 5 --json | jaq '.results[] | {name, score}'

# RECALL with OpenCode embedding — pure semantic KNN
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  recall "JWT authentication" --k 5 --json | jaq '.results[] | {name, score}'

# HYBRID SEARCH with Codex embedding and graph expansion
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  hybrid-search "auth flow" --k 10 --rrf-k 60 --with-graph --max-hops 2 --json \
  | jaq -r '(.results[] | .name), (.graph_matches[] | .name)' | sort -u

# HYBRID SEARCH with Claude embedding and graph expansion
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  hybrid-search "auth flow" --k 10 --with-graph --json \
  | jaq -r '(.results[] | .name), (.graph_matches[] | .name)' | sort -u

# HYBRID SEARCH with OpenCode embedding and graph expansion
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  hybrid-search "auth flow" --k 10 --with-graph --json \
  | jaq -r '(.results[] | .name), (.graph_matches[] | .name)' | sort -u

# RELATED traversal from a known memory (no embedding needed)
sqlite-graphrag related design-auth --hops 2 --json | jaq '.results[] | {name, hop_distance}'

# DEEP RESEARCH with Codex embedding
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  deep-research "How does the binary authenticate to OAuth providers?" \
  --k 20 --max-hops 3 --max-sub-queries 5 --json | jaq '.stats'

# DEEP RESEARCH with Claude embedding
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  deep-research "How does the binary authenticate to OAuth providers?" \
  --k 20 --max-hops 3 --json | jaq '.stats'

# DEEP RESEARCH with OpenCode embedding
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  deep-research "How does the binary authenticate to OAuth providers?" \
  --k 20 --max-hops 3 --json | jaq '.stats'

# ENRICH with Codex backend — rebuild missing embeddings
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  enrich --operation re-embed --limit 100 --resume --json

# ENRICH with Claude backend — extract entity bindings
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  enrich --operation memory-bindings --mode claude-code --json

# ENRICH with OpenCode backend — extract entity bindings
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  enrich --operation memory-bindings --mode opencode --json
```


## v1.0.86+ Surface (pending, slots, embedding, llm-backend, shutdown)
- INVOKE `pending list --filter-status queued` to inspect three-stage remember checkpoint queue
- INVOKE `pending show <id>` to inspect single checkpoint row
- INVOKE `pending cleanup --yes` to remove terminal-state rows
- KNOW BACKED by `pending_memories` table created by migration V014 (ADR-0036)
- PASS `--db <PATH>` on `pending list`/`pending show` (v1.0.89, ADR-0049)
- INVOKE `pending-embeddings list` to inspect retry queue for failed embeddings
- INVOKE `pending-embeddings process` to reprocess with next backend
- KNOW BACKED by `pending_embeddings` table created by migration V015 (ADR-0040)
- INVOKE `slots status` to inspect host-wide slot semaphore
- INVOKE `slots release --slot-id <N> --yes` to reap orphan slots
- KNOW LOCK via `fs4 = "0.9"` with `fcntl(F_SETLK)` on Unix and `LockFileEx` on Windows (ADR-0039)
- INVOKE `embedding status` for aggregate per-status counts
- INVOKE `embedding list` for per-entry inspection
- PASS `--db <PATH>` on `embedding status`/`embedding list`/`embedding abandon` (v1.0.89, ADR-0049)
- PASS `--llm-backend codex,claude` for codex-first with claude fallback (ADR-0038)
- PASS `--llm-backend codex,claude,none` for null embedding fallback
- KNOW DEFAULT `--llm-backend` is `codex`
- PASS `--llm-fallback-mode <claude|codex>` to swap backend mid-job on rate-limit
- PASS `--max-concurrency N` global flag to limit concurrent heavy CLI invocations
- PASS `--wait-lock SECS` global flag to widen lock acquisition window
- PASS `--llm-parallelism N` global flag to cap embedding subprocess fan-out (default 4, clamp [1, 32])
- PASS `--ingest-parallelism N` to control per-file extract+embed parallelism in `ingest`
- PASS `--graceful-shutdown-secs N` to reserve cleanup budget before SIGKILL
- PASS `--skip-embedding-on-failure` only when `--llm-backend …,none`
- PASS ADR-0041 `--strict-env-clear` to drop custom-provider credentials in subprocess
- PASS `--dry-run-backend` to plan backend operation without executing it (idempotent preview)
- PARSE `backend_invoked` field in recall, hybrid-search, remember, edit, ingest, enrich, read envelopes to confirm effective backend
- READ `vec_degraded_reason` in recall/hybrid-search envelopes when vec path is degraded
- KNOW claude backend splits into local embedder via `embed_via_claude_local` (zero-token, OAuth-compatible)
- USE `LlmEmbeddingBuilder` to compose embedding pipeline: `with_backend(Codex).or_fallback(Claude).or_skip()`
- INVOKE `codex-models --json` since v1.0.89 to emit JSON envelope `{"action":"codex_models","count":N,"default":"...","models":[...]}` (no-op alias)
- RUN `codex login` after upgrade to refresh OAuth refresh token (2026-06-14 incident)
- KNOW OPERATOR action for stale OAuth: `codex login` then retry

```bash
# INSPECT the three-stage remember checkpoint queue
sqlite-graphrag pending list --filter-status queued --json | jaq '.[] | {id, name, status}'
sqlite-graphrag pending show 7 --json
sqlite-graphrag pending cleanup --yes --json

# INSPECT and reprocess the failed-embedding retry queue (codex-first, claude fallback)
sqlite-graphrag pending-embeddings list --json | jaq '.[] | {id, status}'
sqlite-graphrag --llm-backend codex,claude pending-embeddings process --json

# INSPECT host-wide slot semaphore and reap an orphan slot
sqlite-graphrag slots status --json | jaq '{max_concurrency, acquired, waiting}'
sqlite-graphrag slots release --slot-id 3 --yes --json

# INSPECT embedding queue status with explicit --db path
sqlite-graphrag --db /data/prod.sqlite embedding status --json | jaq '{pending, done, failed}'

# CONFIRM backend whitelist before a large embed job
sqlite-graphrag codex-models --json | jaq '{count, default, models: .models[:3]}'
```


## v1.0.87+ Pre-Flight Validation Layer (ADR-0045, GAP-META-005)
- KNOW that `src/spawn/preflight.rs` ports every LLM subprocess spawn through 7 guards BEFORE fork
- KNOW exit code 16 (`EX_CONFIG`) is the universal preflight failure exit code (added v1.0.87)
- KNOW 7 guards run in order: `check_argv_size`, `check_binary_exists`, `check_mcp_config_inline`, `check_mcp_config_path`, `check_walkup_mcp_json`, `check_output_buffer`, `check_claude_config_dir`
- KNOW `check_argv_size` rejects argv exceeding `ARG_MAX - 4096` bytes (margin for kernel env vars)
- KNOW `check_binary_exists` aborts when `claude` or `codex` is not in PATH
- KNOW `check_mcp_config_inline` rewrites `--mcp-config '{}'` literal to a tempfile with `{"mcpServers":{}}` (Claude Code 2.1.177 rejects the literal form)
- KNOW `check_mcp_config_path` validates JSON content of `--mcp-config <PATH>` files
- KNOW `check_walkup_mcp_json` rejects invalid `.mcp.json` in CWD ancestor chain (up to 16 levels via `Path::ancestors()`)
- KNOW `check_output_buffer` doubles parser buffer above 64 KB to handle large model outputs
- KNOW `check_claude_config_dir` avoids MCP leak from user-level `~/.claude/`
- SET `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` ONLY in emergencies; bypass reverts to direct `Command::spawn()` and inherits all 5 GAP-META-005 bug classes
- READ `AppError::PreFlightFailed(PreFlightError)` envelope JSON for variant-specific remediation
- KNOW v1.0.88 BUG-11 fix ensures preflight failure propagates via `embed_via_backend_strict`; NEVER expect silent success when preflight fails
- NEVER proceed past exit code 16 without addressing the specific variant reported

```bash
# REPRODUCE a preflight failure (bad MCP config dir) — expect exit 16
CLAUDE_CONFIG_DIR=/tmp/bad-mcp sqlite-graphrag --llm-backend claude \
  remember --name test --type note --description x --body y 2>&1 || echo "exit=$?"

# READ the variant-specific remediation from the error envelope
CLAUDE_CONFIG_DIR=/tmp/bad-mcp sqlite-graphrag --llm-backend claude \
  remember --name test --type note --description x --body y --json 2>/dev/null \
  | jaq '{error, code, message}'

# BYPASS preflight ONLY in emergencies (inherits all bug classes)
SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1 sqlite-graphrag --llm-backend codex \
  remember --name test --type note --description x --body y
```


## v1.0.88+ Hotfixes (BUG-11, BUG-12, BUG-13)
- KNOW BUG-11 (CRITICAL) FIXED: preflight failure in `extract/llm_embedding.rs:563-565` now propagates to `remember` via `embed_via_backend_strict` instead of silent persist with `backend_invoked: "none"` and zero chunks
- REPRODUCE BUG-11 fix: `CLAUDE_CONFIG_DIR=/tmp/bad-config-with-mcp sqlite-graphrag remember --name X --type note --description x --body y` returns exit 11 with JSON error envelope
- KNOW BUG-12 (MEDIUM) FIXED: OAuth-only enforcement emits exactly 1 stderr line (was 2 — duplicate `eprintln!` removed in `src/output.rs`)
- VERIFY BUG-12 fix: `ANTHROPIC_API_KEY=sk-test sqlite-graphrag init` emits 1 stderr line
- KNOW BUG-13 (MEDIUM) FIXED: `link --create-missing` validates entity names BEFORE normalizing (was bypassing validation; ALL_CAPS 3-4 char abbreviations like `API`, `WAL`, `RUST` now correctly rejected via CLI matching the `remember --graph-stdin` path)
- VERIFY BUG-13 fix: `sqlite-graphrag link --from api --to service --create-missing --relation uses` returns exit 1 with validation error
- INVOKE `AppError::PreFlightFailed(PreFlightError)` variant in error handling; exit code 16, `is_permanent() == true`

```bash
# VERIFY BUG-11 fix — preflight failure propagates instead of silent persist
CLAUDE_CONFIG_DIR=/tmp/bad-config-with-mcp sqlite-graphrag --llm-backend claude \
  remember --name X --type note --description x --body y 2>&1 || echo "exit=$?"

# VERIFY BUG-12 fix — OAuth enforcement emits exactly 1 stderr line
ANTHROPIC_API_KEY=sk-test sqlite-graphrag init 2>&1 1>/dev/null | wc -l

# VERIFY BUG-13 fix — ALL_CAPS short abbreviation rejected on link
sqlite-graphrag link --from api --to service --create-missing --relation uses 2>&1 || echo "exit=$?"
```


## v1.0.89+ Embedding Deadlock Remediation (ADR-0050)
- PASS `--llm-model <MODEL>` global flag to select embedding model for ALL backends (v1.0.89, ADR-0050)
- KNOW DEFAULT model for codex backend: `gpt-5.5`; for claude backend: `claude-sonnet-4-6`
- SET env `SQLITE_GRAPHRAG_LLM_MODEL` as persistent override for `--llm-model`
- PASS `--codex-binary <PATH>` to override codex binary location (v1.0.89, ADR-0050)
- SET env `SQLITE_GRAPHRAG_CODEX_BINARY` as persistent override for `--codex-binary`
- PASS `--claude-binary <PATH>` to override claude binary location (propagated via set_var since v1.0.89)
- PASS `--skip-embedding-on-failure` to exit 0 when LLM embedding fails (wired end-to-end since v1.0.89, ADR-0050)
- KNOW 7 dead CLI flags were fixed in v1.0.89 via `set_var` propagation in `main.rs`: `--llm-model`, `--llm-fallback`, `--skip-embedding-on-failure`, `--claude-binary`, `--codex-binary`, `--llm-max-host-concurrency`, `--llm-slot-wait-secs`
- KNOW `deep-research` and `remember-batch` now receive `llm_backend` from main.rs (v1.0.89, ADR-0050)
- KNOW adaptive timeout scales with batch size: `base + 15s × (batch_size - 1)` (v1.0.89, ADR-0050)
- KNOW OAuth expiry errors now include actionable hint: "run codex login" or "refresh claude OAuth" (v1.0.89)
- KNOW `BoolishValueParser` accepts `1/yes/on/true` and `0/no/off/false` for boolean env vars (v1.0.89, ADR-0050)
- KNOW `--yes` flag on `slots release`, `purge`, `cleanup-orphans` was wired end-to-end (v1.0.89, BUG-YES-FLAG-IGNORED)

```bash
# SELECT a specific embedding model per backend
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name x --type note --description x --body y
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  remember --name x --type note --description x --body y

# OVERRIDE binary paths for both backends
sqlite-graphrag --llm-backend codex --codex-binary /usr/local/bin/codex \
  --llm-model gpt-5.5 remember --name x --type note --description x --body y
sqlite-graphrag --llm-backend claude --claude-binary /usr/local/bin/claude \
  --llm-model claude-sonnet-4-6 remember --name x --type note --description x --body y

# EXIT 0 on embedding failure (degraded, non-searchable memory)
sqlite-graphrag --llm-backend codex,claude,none --skip-embedding-on-failure \
  remember --name x --type note --description x --body-file big.md

# DEEP-RESEARCH and REMEMBER-BATCH now honor the global backend
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  deep-research "OAuth flow" --k 20 --json | jaq '.stats'
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember-batch --json < batch.ndjson | jaq -c 'select(.summary == true)'
```


## v1.0.89+ Schema Drift and Flag Parity (ADR-0048, ADR-0049)
- KNOW `health.schema.json` regenerated via `schemars` derive macro (ADR-0048); `additionalProperties: true` per Must-Ignore policy (RFC 7493 I-JSON)
- KNOW 17 new fields added to `health` envelope since v1.0.88: `fts_query_ok`, `vec_memories_missing`, `vec_memories_orphaned`, `sqlite_version`, `mentions_ratio`, `mentions_warning`, `top_relation`, `top_relation_ratio`, `applies_to_ratio`, `relation_concentration_warning`, `super_hub_count`, `super_hub_warning`, `top_hub_entity`, `top_hub_degree`, `hub_warning`, `non_normalized_count`, `normalization_warning`
- REGENERATE schemas via `cargo run --bin dump-schema` (idempotent BTreeMap ordering)
- PASS `--namespace <NS>` on `health` to filter counts to one namespace
- USE `migrate --dry-run --json` to PREVIEW pending migrations without applying; lists names+versions, validates checksums, checks preconditions
- USE `codex-models --json` as no-op alias returning JSON envelope
- USE `--auto-describe` (default true) on `ingest` to extract description from first significant body line; OPT OUT via `--no-auto-describe`
- PASS `--db <PATH>` on `embedding status`/`embedding list`/`embedding abandon`/`pending list`/`pending show` (ADR-0049)
- KNOW `--db <PATH>` is NOT global; each subcommand accepts it independently (clap `Arg::global = true` was REJECTED as invasive)
- TREAT binary size as 14.6 MiB stripped ELF (NOT 6 MB as in older docs); see `Cargo.toml:6` description

```bash
# INSPECT the 17 new health fields
sqlite-graphrag health --json \
  | jaq '{fts_query_ok, sqlite_version, top_relation, super_hub_count, normalization_warning}'

# REGENERATE schemas idempotently and CHECK they are in sync
cargo run --bin dump-schema -- --check
cargo run --bin dump-schema

# HEALTH scoped to a namespace
sqlite-graphrag health --namespace prod --json | jaq '.counts'

# PREVIEW pending migrations without applying
sqlite-graphrag migrate --dry-run --json | jaq '.would_apply[]? | {name, version}'
```


## JSON Contracts (Top-5 Fields per Command)
- KNOW `recall` top fields: `results[].name`, `snippet`, `distance`, `score`, `source`
- KNOW `hybrid-search` top fields: `results[].name`, `combined_score`, `vec_rank`, `fts_rank`, `source`
- KNOW `health` top fields: `integrity_ok`, `schema_ok`, `counts`, `wal_size_mb`, `schema_version`
- KNOW `list` top fields: `items[].name`, `type`, `description`, `updated_at_iso`, `deleted_at_iso?`
- KNOW `edit` top fields: `memory_id`, `name`, `action`, `version`, `elapsed_ms`
- KNOW `read` top fields: `name`, `body`, `description`, `created_at_iso`, `updated_at_iso`
- KNOW `forget` top fields: `action`, `forgotten`, `name`, `namespace`, `elapsed_ms`
- KNOW `link` top fields: `action`, `from`, `to`, `relation`, `weight`
- KNOW `graph entities` top fields: `entities[].id`, `name`, `entity_type`, `degree`, `description?`
- KNOW `deep-research` top fields: `sub_queries[]`, `results[]`, `evidence_chains[]`, `graph_context`, `stats`
- KNOW `enrich` NDJSON events: `phase`, `name`, `status`, `entities?`, `rels?`, `cost_usd?`, `elapsed_ms?`
- KNOW `pending list` top fields: `id`, `name`, `status`, `created_at`, `namespace`
- KNOW `slots status` top fields: `max_concurrency`, `acquired`, `waiting`, `held_by_pid[]`
- KNOW `embedding status` top fields: `pending`, `processing`, `done`, `failed`, `skipped`
- KNOW `remember`/`edit`/`ingest`/`enrich`/`read` envelopes: include `backend_invoked` and `vec_degraded_reason?`
- KNOW `health.schema.json` uses `"additionalProperties": true` per Must-Ignore policy (RFC 7493 I-JSON) since v1.0.89 (ADR-0048); the other 49 schemas in `docs/schemas/` still use `"additionalProperties": false` (Must-Validate) pending regeneration in v1.0.90+
- READ FULL schemas in `docs/schemas/*.schema.json` (never inline full schema in skill)

```bash
# PARSE recall top fields
sqlite-graphrag --llm-backend codex recall "auth" --k 3 --json \
  | jaq '.results[] | {name, snippet, distance, score, source}'

# PARSE hybrid-search top fields plus backend confirmation
sqlite-graphrag --llm-backend claude hybrid-search "auth" --k 5 --json \
  | jaq '{backend_invoked, results: [.results[] | {name, combined_score, vec_rank, fts_rank}]}'

# PARSE deep-research top fields
sqlite-graphrag --llm-backend codex deep-research "OAuth flow" --k 20 --json \
  | jaq '{sub_queries: (.sub_queries | length), evidence_chains: (.evidence_chains | length), stats}'
```


## Exit Codes and Retry
- KNOW EXIT 0 means success; parse stdout
- KNOW EXIT 1 means validation error (invalid weight, self-link, max-files exceeded, link ALL_CAPS bypass)
- KNOW EXIT 2 means Clap argument parsing error
- KNOW EXIT 3 means optimistic lock conflict; reload `read --json` and retry
- KNOW EXIT 4 means entity, memory, or version not found
- KNOW EXIT 5 means namespace error
- KNOW EXIT 6 means payload above size limit
- KNOW EXIT 9 means duplicate memory (use `--force-merge` to update or restore)
- KNOW EXIT 10 means database error; run `vacuum` and `health`
- KNOW EXIT 11 means embedding failure (LLM subprocess error, including preflight fail since BUG-11 fix)
- KNOW EXIT 13 means partial batch failure; reprocess only failed
- KNOW EXIT 14 means I/O error (permission, disk full)
- KNOW EXIT 15 means database busy; widen `--wait-lock`
- KNOW EXIT 16 means preflight validation failure (v1.0.87+, ADR-0045); check JSON envelope for variant
- KNOW EXIT 19 means SHUTDOWN_EXIT_CODE (ADR-0037); partial work discarded; RETRY MANDATORY
- KNOW EXIT 19 envelope: `{error:true, code:19, signal, graceful, message}`
- KNOW EXIT 20 means internal error or JSON serialization failure
- KNOW EXIT 75 means slots exhausted OR `JobSingletonLocked`
- KNOW EXIT 75 from `enrich`/`ingest --mode claude-code|codex|opencode`: parse `job '(\w+)'.*namespace '(\w+)'`
- KNOW EXIT 75 circuit breaker: respect per-namespace cooldown window; do NOT retry immediately
- KNOW EXIT 77 means RAM pressure; wait for free memory
- NEVER ignore non-zero exit code as success
- NEVER reprocess entire batch after exit 13
- NEVER increase concurrency after exit 75 or 77
- NEVER confuse exit 1 (validation) with exit 9 (duplicate)
- NEVER treat exit 16 as transient; fix the underlying preflight issue

```bash
# CHECK exit code first, then branch on it
sqlite-graphrag --llm-backend codex recall "auth" --k 3 --json
case $? in
  0) echo "success" ;;
  11) echo "embedding failure — check backend and OAuth" ;;
  16) echo "preflight failure — fix MCP config" ;;
  19) echo "shutdown — RETRY MANDATORY" ;;
  75) echo "slots exhausted — wait for cooldown, do NOT retry immediately" ;;
  *) echo "other failure: $?" ;;
esac
```


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
- USE SHUTDOWN bypass recipe: prepend `tests/mock-llm` to PATH then set `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1` then wrap with `setsid -w timeout`
- KNOW JOB SINGLETON: `enrich`, `ingest --mode claude-code`, `ingest --mode codex`, `ingest --mode opencode` acquire per-namespace singleton
- USE `--wait-job-singleton SECS` to wait for lock or `--force-job-singleton` to break stale lock
- LIMIT parallel ingestion in CI to avoid LLM rate limits
- NEVER run `enrich` in parallel against same database

```bash
# CAP embedding subprocess fan-out and cross-process concurrency
sqlite-graphrag --llm-backend codex --llm-parallelism 4 --llm-max-host-concurrency 8 \
  ingest ./docs --mode codex --recursive --json

# WAIT for a slot instead of aborting
sqlite-graphrag --llm-backend codex --llm-slot-wait-secs 30 \
  recall "auth" --k 5 --json

# RUN unitary, low-memory mode in a constrained container
SQLITE_GRAPHRAG_LOW_MEMORY=1 sqlite-graphrag --llm-backend codex \
  ingest ./docs --mode codex --recursive --json

# WAIT for the per-namespace job singleton on enrich
sqlite-graphrag --llm-backend codex enrich --operation re-embed --limit 100 \
  --wait-job-singleton 60 --resume --json
```


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
- INVOKE `migrate --dry-run --json` to preview migrations (v1.0.89)
- INVOKE `debug-schema --json` (hidden from `--help`) to inspect schema state
- INVOKE `completions <bash|zsh|fish|elvish|powershell>` to generate shell completions
- INVOKE `vec orphan-list --json` to list orphaned memory vectors
- INVOKE `vec purge-orphan --yes --dry-run` to PREVIEW purge
- INVOKE `vec purge-orphan --yes` to PERMANENTLY purge orphans
- INVOKE `vec stats --json` to inspect vec table health
- REGENERATE schemas via `cargo run --bin dump-schema` (v1.0.89, ADR-0048)
- SCHEDULE weekly: `purge --retention-days 30 --yes` then `cleanup-orphans --yes` then `prune-relations --relation mentions --yes` then `vacuum --json` then `optimize --json` then `sync-safe-copy --dest ~/backups/`
- KNOW SINCE v1.0.53 every write runs `PRAGMA wal_checkpoint(TRUNCATE)` after commit
- KNOW IF corruption occurs despite checkpoint: `sqlite3 broken.sqlite ".recover" | sqlite3 repaired.sqlite`

```bash
# REBUILD and CHECK the FTS5 index
sqlite-graphrag fts rebuild --json
sqlite-graphrag fts check --json | jaq '.integrity_ok'
sqlite-graphrag fts stats --json | jaq '{total_rows, fts_functional}'

# BACKUP online and SNAPSHOT atomically
sqlite-graphrag backup --output ~/backups/graphrag.sqlite --json
sqlite-graphrag sync-safe-copy --dest ~/backups/graphrag-$(date +%Y%m%d).sqlite

# EXPORT a namespace as NDJSON
sqlite-graphrag export --namespace prod --type decision --json > decisions.ndjson

# LIST and PURGE orphaned vectors safely
sqlite-graphrag vec orphan-list --json | jaq 'length'
sqlite-graphrag vec purge-orphan --yes --dry-run --json
sqlite-graphrag vec purge-orphan --yes --json

# GENERATE shell completions
sqlite-graphrag completions bash > ~/.local/share/bash-completion/completions/sqlite-graphrag
```


## Ready-Made Examples

### Example 1 — Bootstrap a project namespace
```bash
sqlite-graphrag init --namespace myproject
sqlite-graphrag health --json | jaq '.integrity_ok'
sqlite-graphrag health --json | jaq '{schema_version, counts}'
```
- EXPECT: exit 0, `integrity_ok: true`, `schema_version >= 15`

### Example 2 — Store and retrieve a memory (codex backend)
```bash
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  remember --name auth-decision --type decision \
  --description "JWT 15-min expiry with refresh flow" \
  --body-stdin <<'EOF'
We chose JWT with 15-minute expiry because:
- Refresh tokens are HTTP-only cookies
- 15min limit reduces blast radius of XSS
- Refresh flow reissues tokens on user activity
EOF

sqlite-graphrag read --name auth-decision --json | jaq '{description, body_length}'
```
- EXPECT: memory persisted, body contains full text, `body_length` > 100

### Example 3 — Store the same memory (claude backend)
```bash
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  remember --name auth-decision --type decision \
  --description "JWT 15-min expiry with refresh flow" \
  --body "15-min expiry, HTTP-only refresh cookies, reissue on activity" --force-merge

sqlite-graphrag read --name auth-decision --json | jaq '.backend_invoked'
```
- EXPECT: idempotent merge; `backend_invoked` confirms the claude path ran

### Example 3b — Store the same memory (opencode backend)
```bash
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  remember --name auth-decision --type decision \
  --description "JWT 15-min expiry with refresh flow" \
  --body "15-min expiry, HTTP-only refresh cookies, reissue on activity" --force-merge

sqlite-graphrag read --name auth-decision --json | jaq '.backend_invoked'
```
- EXPECT: idempotent merge; `backend_invoked` confirms the opencode path ran

### Example 4 — Search with hybrid ranking + graph expansion (both backends)
```bash
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  hybrid-search "JWT authentication" --k 5 --with-graph --max-hops 2 --json \
  | jaq -r '(.results[] | .name), (.graph_matches[] | .name)' | sort -u

sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  hybrid-search "JWT authentication" --k 5 --with-graph --max-hops 2 --json \
  | jaq -r '(.results[] | .name), (.graph_matches[] | .name)' | sort -u
```
- EXPECT: top 5 KNN+FTS5 fused results plus 0-N multi-hop neighbors

### Example 5 — Bulk ingest a docs directory (codex extraction)
```bash
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini \
  ingest ./docs --mode codex --recursive --type document \
  --pattern "*.md" --max-files 1000 --auto-describe --json \
  | jaq -c 'select(.status)' | jaq -s 'group_by(.status) | map({status: .[0].status, count: length})'
```
- EXPECT: NDJSON progress; summary shows `files_total`, `files_succeeded`, `files_failed`

### Example 5b — Bulk ingest a docs directory (opencode extraction)
```bash
sqlite-graphrag --llm-backend opencode --llm-model opencode/big-pickle \
  ingest ./docs --mode opencode --recursive --type document \
  --pattern "*.md" --auto-describe --json \
  | jaq -c 'select(.status)' | jaq -s 'group_by(.status) | map({status: .[0].status, count: length})'
```
- EXPECT: NDJSON progress; opencode extracts entities and relations per file

### Example 6 — Graph traversal from a known entity
```bash
sqlite-graphrag graph entities --json | jaq -r '.entities[].name' | head -10
sqlite-graphrag graph traverse --from jwt --depth 2 --json | jaq -r '.hops[] | "\(.entity) \(.relation)"'
```
- EXPECT: list of entities; traversal shows 2-hop neighborhood via canonical relations

### Example 7 — Deep research question (claude backend)
```bash
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  deep-research "How does the binary authenticate to OAuth providers?" \
  --k 20 --max-hops 3 --max-sub-queries 5 --json \
  | jaq '{stats, evidence_chains: (.evidence_chains | length)}'
```
- EXPECT: decomposed sub-queries, evidence chains linking seed to target, graph_context populated

### Example 8 — LLM-curated entity extraction from existing docs (claude-code mode)
```bash
sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 \
  ingest ./corpus --mode claude-code --recursive --resume --json \
  | jaq -c 'select(.status == "done") | {file, entities, rels}'
```
- EXPECT: per-file NDJSON with `entities` count, `rels` count; `--resume` continues after interruption

### Example 9 — Diagnose a preflight failure (exit 16)
```bash
CLAUDE_CONFIG_DIR=/tmp/bad-mcp sqlite-graphrag --llm-backend claude \
  remember --name test --type note --description x --body y 2>&1
echo "exit=$?"
sqlite-graphrag --llm-backend claude \
  remember --name test --type note --description x --body y 2>&1 || echo "exit=$?"
```
- EXPECT: first invocation returns exit 16 with `AppError::PreFlightFailed` envelope
- EXPECT: second invocation without bad MCP dir returns exit 0

### Example 10 — Recovery from soft-delete
```bash
sqlite-graphrag forget --name auth-decision
sqlite-graphrag history --name auth-decision --json | jaq '.versions[0].deleted'
sqlite-graphrag restore --name auth-decision
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini recall "JWT" --k 3 --json | jaq '.results[].name'
```
- EXPECT: soft-delete hides from recall; restore brings it back; recall shows it again

### Example 11 — Codex-first with claude fallback on a flaky network
```bash
sqlite-graphrag --llm-backend codex,claude --llm-model gpt-5.5 \
  remember --name resilient-note --type note \
  --description "survives codex rate limit" --body-file note.md

sqlite-graphrag read --name resilient-note --json | jaq '.backend_invoked'
```
- EXPECT: codex attempted first; on rate-limit the claude backend completes; `backend_invoked` confirms which ran

### Example 11b — Full fallback chain: codex → claude → opencode → none
```bash
sqlite-graphrag --llm-backend codex,claude,opencode,none --skip-embedding-on-failure \
  remember --name max-resilience --type note \
  --description "survives all backend failures" --body-file note.md

sqlite-graphrag read --name max-resilience --json | jaq '.backend_invoked'
```
- EXPECT: tries codex, then claude, then opencode, then degrades to null embedding; `backend_invoked` confirms which ran

### Example 12 — Health check with namespace filter and vec tables
```bash
sqlite-graphrag health --namespace prod --json | jaq '{integrity_ok, schema_version, counts}'
sqlite-graphrag vec stats --json | jaq '.'
sqlite-graphrag embedding status --json | jaq '{pending, done, failed}'
```
- EXPECT: scoped counts for the `prod` namespace; vec table health; embedding queue status

### Example 13 — Regenerate JSON schemas after type changes
```bash
cargo run --bin dump-schema -- --check
git diff --stat docs/schemas/
cargo run --bin dump-schema  # if --check failed
```
- EXPECT: `--check` exits 0 when schemas are in sync; regeneration produces idempotent output

### Example 14 — Maintenance pipeline (weekly)
```bash
sqlite-graphrag purge --retention-days 30 --yes --dry-run
sqlite-graphrag cleanup-orphans --yes --dry-run
sqlite-graphrag prune-relations --relation mentions --yes --dry-run
sqlite-graphrag vacuum --json
sqlite-graphrag optimize --json
sqlite-graphrag sync-safe-copy --dest ~/backups/graphrag-$(date +%Y%m%d).sqlite
```
- EXPECT: each dry-run reports counts; full pipeline reclaims space and snapshots safely

### Example 15 — Inspect Codex models whitelist (v1.0.89, no-op alias, GAP-E2E-010a)
```bash
sqlite-graphrag codex-models --json | jaq '{count, default, models: .models[:3]}'
sqlite-graphrag codex-models  # text mode for humans
sqlite-graphrag codex-models --json | jaq '.models | length'
```
- EXPECT: JSON envelope `{"action":"codex_models","count":N,"default":"gpt-5.5","models":[...]}`
- EXPECT: text mode emits human-readable list of supported models
- USE when validating that current OAuth scope includes required codex model names

### Example 16 — Health check scoped to one namespace (v1.0.89, GAP-E2E-002)
```bash
sqlite-graphrag health --namespace prod --json | jaq '{integrity_ok, schema_version, counts}'
sqlite-graphrag health --namespace dev --json | jaq '.counts'  # different counts
sqlite-graphrag health --json | jaq '.counts'  # global counts
```
- EXPECT: counts filtered to the specified namespace; integrity and schema_version fields unchanged
- USE in multi-tenant environments to verify per-namespace isolation
- OMISSION RULE: when `--namespace` is omitted, counts aggregate across all namespaces (global view)

### Example 17 — Dry-run migration preview (v1.0.89, GAP-E2E-009)
```bash
sqlite-graphrag migrate --dry-run --json | jaq '.would_apply[]? | {name, version}'
sqlite-graphrag migrate --to-llm-only --drop-vec-tables --dry-run --json | jaq '.'
sqlite-graphrag migrate --dry-run --json  # always PREVIEW before destructive migrations
```
- EXPECT: list of pending migrations with name+version without applying them; database remains unchanged
- EXPECT: `--to-llm-only --dry-run` reports vec table drop plan without executing
- USE in CI pipelines and before any irreversible migration step

### Example 18 — Plan backend operation without executing (dry-run-backend)
```bash
sqlite-graphrag --llm-backend codex --llm-model gpt-5.4-mini --dry-run-backend \
  remember --name preview --type note --description x --body y --json | jaq '.backend_invoked'

sqlite-graphrag --llm-backend claude --llm-model claude-sonnet-4-6 --dry-run-backend \
  recall "auth" --k 5 --json | jaq '{backend_invoked, vec_degraded_reason}'
```
- EXPECT: idempotent preview; `backend_invoked` reports the planned backend; nothing persisted


## References to Extended Documentation

For details beyond this skill's daily-use scope, the following project documents extend coverage:

- READ `docs/HOW_TO_USE.md` — quickstart, installation, common workflows
- READ `docs/COOKBOOK.md` — 50+ recipes for advanced patterns (preflight diagnostics, schema drift recovery, etc.)
- READ `docs/MIGRATION.md` — version-to-version upgrade paths
- READ `docs/CROSS_PLATFORM.md` — behavior across Linux, macOS, Windows ARM64
- READ `docs/AGENTS.pt-BR.md` — extended PT-BR documentation for AI agents
- READ `docs/schemas/*.schema.json` — full JSON Schema contracts (versioned per SemVer)
- READ `docs/decisions/adr-*.md` — Architecture Decision Records (justifications for each design choice)
- READ `llms-full.txt` — complete LLM-context dump with all rules
- READ `gaps.md` — current open and closed gaps
- READ `CHANGELOG.md` — version-by-version release notes
- READ `Cargo.toml` — package metadata and binary size documentation (14.6 MiB)


## Active Rules and Anti-patterns Summary
- NEVER pass `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` (OAuth-only, exit 1)
- NEVER depend on daemon or use `--bare` flag (REMOVED v1.0.76 and v1.0.79)
- NEVER install with `--features embedding-legacy` or `--features ner-legacy` (REMOVED)
- NEVER use `fastembed`, `tokenizers`, `sqlite-vec`, or `GLiNER` crates
- NEVER expect sqlite-vec KNN; cosine is pure Rust in `src/similarity.rs`
- NEVER run `enrich` in parallel against same database (job singleton via `lock::acquire_job_singleton`)
- NEVER write to `.sqlite` file outside the binary
- NEVER ignore exit 19 (SHUTDOWN_EXIT_CODE envelope); partial work discarded, RETRY MANDATORY
- NEVER ignore exit 16 (preflight failure); fix MCP config or `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1`
- NEVER duplicate content already in `CHANGELOG.md`
- NEVER use `mentions` as default graph relation
- NEVER pass empty body via `--graph-stdin` (exit 1 since v1.0.54)
- NEVER use `--gliner-variant` (no-op since v1.0.79)
- NEVER call `migrate --to-llm-only` without `--drop-vec-tables` safety guard
- NEVER ignore `--wait-lock` flag when contention is expected
- NEVER assume exit 1 equals exit 9 (validation vs duplicate)
- NEVER assume binary size is 6 MB; actual is 14.6 MiB stripped ELF
- ALWAYS pass `--llm-backend` and `--llm-model` explicitly for embedding commands
- ALWAYS parse `backend_invoked` to confirm which backend ran
- ALWAYS run `codex login` or refresh claude OAuth when a backend reports stale OAuth
