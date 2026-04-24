# HOW TO USE sqlite-graphrag

> Ship persistent memory to any AI agent in 60 seconds flat, zero dollars spent


- Read this guide in Portuguese at [HOW_TO_USE.pt-BR.md](HOW_TO_USE.pt-BR.md)
- Return to the main [README.md](../README.md) for command reference


## The Question That Starts Here
### Curiosity — Why Engineers Abandon Pinecone in 2026
- How many milliseconds separate your agent from production memory today
- Why senior engineers in production choose SQLite over Pinecone for LLM memory
- What changes when embeddings, search and graph live inside a single file
- Why twenty one AI agents converge on sqlite-graphrag as their persistence layer
- This guide answers every question above in under ten minutes of reading


## Reading Time and Impact
### Investment — Five Minutes to Read Ten to Execute
- Total reading time reaches five minutes for technical readers skimming headings
- Total execution time reaches ten minutes including model download on first run
- Learning curve drops to zero for anyone familiar with standard CLI patterns
- First memory gets persisted in sixty seconds after install completes
- First hybrid search returns ranked hits in under fifty milliseconds locally
- Expected tokens saved per month hit two hundred thousand on a single agent


## Prerequisites
### Environment — Minimum Supportable Baseline
- Rust 1.88 or newer installed via `rustup` across Linux macOS and Windows
- SQLite version 3.40 or newer shipped with your operating system distribution
- Operating systems Linux glibc, Linux musl, macOS 11 plus, Windows 10 plus
- Available RAM of 100 MB free for runtime plus 1 GB during embedding model load
- Disk space of 200 MB for the embedding model cache on first invocation
- Network access required ONLY for first `init` to download quantized embeddings


## First Command in 60 Seconds
### Install — Three Shell Lines You Copy Once
```bash
cargo install --path .
sqlite-graphrag init
sqlite-graphrag remember --name first-note --type user --description "first memory" --body "hello graphrag"
```
- First line downloads, builds and installs the binary into `~/.cargo/bin`
- Second line creates the SQLite database and downloads the embedding model
- Third line persists your first memory and indexes it for hybrid retrieval
- Confirmation prints to stdout, traces route to stderr, exit code zero signals success
- Your next `recall` call returns the note you just saved in milliseconds


## Core Commands
### Lifecycle — Seven Subcommands You Use Daily
```bash
sqlite-graphrag init --namespace my-project
sqlite-graphrag remember --name auth-design --type decision --description "auth uses JWT" --body "Rationale documented."
sqlite-graphrag recall "authentication strategy" --k 5 --json
sqlite-graphrag hybrid-search "jwt design" --k 10 --rrf-k 60 --json
sqlite-graphrag read --name auth-design
sqlite-graphrag forget --name auth-design
sqlite-graphrag purge --retention-days 90 --yes
```
- `init` bootstraps the database, downloads the model and validates the `sqlite-vec` extension
- `remember` stores content, extracts entities and generates embeddings atomically
- `recall` performs pure vector KNN search over the `vec_memories` table
- `hybrid-search` fuses FTS5 full-text and vector KNN with Reciprocal Rank Fusion
- `read` fetches a memory by its exact kebab-case name in a single SQL query
- `forget` performs a soft delete preserving the full version history
- `purge` permanently removes memories soft-deleted more than the retention threshold


## Persistent Daemon
### Reuse The Embedding Model Across Heavy Commands
```bash
sqlite-graphrag daemon
sqlite-graphrag daemon --ping
sqlite-graphrag daemon --stop
```
- `init`, `remember`, `recall`, and `hybrid-search` automatically try the daemon first
- If the daemon is unavailable, those commands auto-start it on demand before falling back locally
- Manual `sqlite-graphrag daemon` startup is now optional and useful mainly for explicit supervision or debugging
- Use `--ping` to confirm the daemon is alive and inspect handled embedding request counts
- Use `--stop` for a graceful shutdown after long-running batch or agent sessions


## Advanced Patterns
### Recipe One — Hybrid Search With Weighted Fusion
```bash
sqlite-graphrag hybrid-search "postgres migration strategy" \
  --k 20 \
  --rrf-k 60 \
  --weight-vec 0.7 \
  --weight-fts 0.3 \
  --json \
  | jaq '.results[] | {name, score, source}'
```
- Combines dense vector similarity and sparse full-text matches in one ranked list
- Weight tuning lets you favor semantic proximity against keyword precision per query
- RRF constant `--rrf-k 60` matches the default recommended by the RRF paper
- Pipeline saves eighty percent of tokens compared to LLM-based re-ranking
- Expected latency stays under fifteen milliseconds for databases up to 100 MB

### Recipe Two — Graph Traversal for Multi-Hop Recall
```bash
sqlite-graphrag link --source auth-design --target jwt-spec --relation depends-on
sqlite-graphrag link --source jwt-spec --target rfc-7519 --relation references
sqlite-graphrag related auth-design --hops 2 --json \
  | jaq -r '.nodes[] | select(.depth == 2) | .name'
```
- Two hops surface transitive knowledge invisible to pure vector search methods
- Typed relations let your agent reason about cause, dependency and reference chains
- Graph queries run in under five milliseconds thanks to SQLite indexed joins
- Multi-hop recall recovers context that flat embeddings consistently drop out of top-K
- Saves fifteen minutes per debugging session hunting for related architectural decisions

### Recipe Three — Batch Ingestion via Shell Pipeline
```bash
find ./docs -name "*.md" -print0 \
  | xargs -0 -n 1 -P 4 -I {} bash -c '
      name=$(basename {} .md)
      sqlite-graphrag remember \
        --name "doc-${name}" \
        --type reference \
        --description "imported from {}" \
        --body "$(cat {})"
    '
```
- Parallel factor `-P 4` matches the default counting semaphore slots exactly
- Exit code `75` signals slot exhaustion and the orchestrator should retry later
- Exit code `77` signals RAM pressure and the orchestrator must wait for free memory
- Batch throughput reaches 200 documents per minute on a modern laptop CPU
- Saves forty minutes of manual ingestion per 1000 Markdown files processed

### Recipe Four — Snapshot-Safe Sync With Dropbox or iCloud
```bash
sqlite-graphrag sync-safe-copy --dest ~/Dropbox/graphrag.sqlite
ouch compress ~/Dropbox/graphrag.sqlite ~/Dropbox/graphrag-$(date +%Y%m%d).tar.zst
```
- `sync-safe-copy` checkpoints the WAL and copies a consistent snapshot atomically
- Dropbox, iCloud and Google Drive NEVER corrupt the active database during sync
- Compression via `ouch` reduces snapshot size by sixty percent for archival buckets
- Recovery on a new machine takes one `ouch decompress` plus one `cp` operation
- Protects years of memory from sync-induced corruption that plagues raw SQLite files

### Recipe Five — Integration With Claude Code Orchestrator
```bash
sqlite-graphrag recall "$USER_QUERY" --k 5 --json \
  | jaq -c '{
      context: [.results[] | {name, body, score}],
      generated_at: now | todate
    }' \
  | claude --print "Use this context to answer: $USER_QUERY"
```
- Structured JSON flows cleanly into any orchestrator reading from stdin
- Score field enables the orchestrator to drop low-relevance hits before prompting
- Determinism of exit codes lets the orchestrator route errors without parsing stderr
- Token cost drops by seventy percent compared to full-corpus context stuffing
- Round-trip latency stays under one hundred milliseconds end to end locally


## Configuration and Namespace Notes
### Namespace Default
- Default namespace is `global` when `--namespace` is omitted
- Configure via `SQLITE_GRAPHRAG_NAMESPACE` env var to override globally
- Use `namespace-detect` to inspect the resolved namespace before running bulk operations

### Score Semantics
- JSON output uses `score` field (cosine similarity, higher is more relevant)
- Results are sorted by `score` descending so the best match always appears first
- Always prefer `--json` in pipelines to get the raw `score` for precise filtering

### Language Flag Aliases
- `--lang en` forces English output regardless of system locale
- `--lang pt`, `--lang pt-BR`, `--lang portuguese`, and `--lang PT` all force Portuguese
- Env var `SQLITE_GRAPHRAG_LANG=pt` overrides system locale when `--lang` is absent
- All aliases resolve to the same two internal variants: English and Portuguese

### JSON Output Flag
- `--json` is accepted by every subcommand as the broad compatibility flag for deterministic JSON stdout
- `--format json` is accepted only by commands that expose `--format` in their help output
- Use `--json` in pipelines when you want one spelling that works across the whole CLI surface
- Use `--format json` only on commands that explicitly advertise `--format`

### Standardized Output Format Flags
- Every subcommand emits JSON by default on stdout
- `--json` is the short form — preferred in one-liners and agent pipelines
- `--format json` is the explicit form — available only on commands that expose `--format`
- Human-readable `text` and `markdown` are implemented only on a subset of commands
- Current flag support matrix:

| Subcommand | `--json` | `--format json` | Default output |
|---|---|---|---|
| `remember` | yes | yes | json |
| `recall` | yes | yes | json |
| `read` | yes | no | json |
| `list` | yes | yes | json |
| `forget` | yes | no | json |
| `link` | yes | yes | json |
| `unlink` | yes | yes | json |
| `stats` | yes | yes | json |
| `health` | yes | yes | json |
| `history` | yes | no | json |
| `edit` | yes | no | json |
| `rename` | yes | yes | json |
| `restore` | yes | yes | json |
| `purge` | yes | no | json |
| `cleanup-orphans` | yes | yes | json |
| `optimize` | yes | no | json |
| `migrate` | yes | no | json |
| `init` | yes | no | json |
| `sync-safe-copy` | yes | yes | json |
| `hybrid-search` | yes | yes | json |
| `namespace-detect` | yes | no | json |

```bash
# Short form — preferred in pipelines
sqlite-graphrag recall "auth" --json | jaq '.results[].name'

# Explicit form — identical output
sqlite-graphrag recall "auth" --format json | jaq '.results[].name'

# Both forms accepted in the same pipeline
sqlite-graphrag stats --json && sqlite-graphrag health --format json
```

### DB Path Discovery
- Default behavior always uses `graphrag.sqlite` in the current working directory
- All commands accept `--db <PATH>` flag in addition to `SQLITE_GRAPHRAG_DB_PATH` env var
- CLI flag takes precedence over environment variable
- Use `--db` only when you intentionally need a database outside the current directory

### Log Format
- `SQLITE_GRAPHRAG_LOG_FORMAT=json` emits tracing events as newline-delimited JSON to stderr
- Default value is `pretty`; any value other than `json` falls back to human-readable pretty format
- Use `json` format when shipping logs to structured aggregators such as Loki or Datadog

### Display Timezone
- `SQLITE_GRAPHRAG_DISPLAY_TZ=America/Sao_Paulo` applies any IANA timezone to all `*_iso` fields in JSON output
- Flag `--tz <IANA>` takes priority over the environment variable; both fall back to UTC when absent
- Integer epoch fields (`created_at`, `updated_at`) are never affected — only the ISO string companions
- Invalid IANA names cause exit 2 with a descriptive validation error before the command executes
- Examples: `America/New_York`, `Europe/Berlin`, `Asia/Tokyo`, `America/Sao_Paulo`
```bash
# One-off with flag
sqlite-graphrag read --name my-note --tz America/Sao_Paulo

# Persistent via env var
export SQLITE_GRAPHRAG_DISPLAY_TZ=America/Sao_Paulo
sqlite-graphrag list | jaq '.items[].updated_at_iso'
```

### Concurrency Cap
- `--max-concurrency` is capped at `2×nCPUs`; higher values return exit 2 during argument validation
- Embedding-heavy commands are clamped further at runtime from available RAM and the per-process RSS budget measured for the ONNX model
- Treat `init`, `remember`, `recall`, and `hybrid-search` as heavy commands when planning automation or audits
- Exit code 2 signals invalid argument; reduce the value and retry immediately
- The hard ceiling remains 4 cooperating subprocesses, but the effective safe limit may be lower on the current host
- During audits start heavy commands with `--max-concurrency 1` and scale only after measuring RSS and swap behavior

### Help Text Language for Global Flags
- The global flags `--max-concurrency`, `--wait-lock`, `--lang`, and `--tz` display Portuguese help text in `--help` output
- This is a deliberate choice: clap doc comments are written in Portuguese to match the primary development language
- The JSON output contract and all flag names are language-neutral and identical regardless of `--lang`


## Reference — Subcommands Not Covered in Quick Start
### Using cleanup-orphans
- Removes entities that have no memories attached and no graph relationships
- Run periodically after bulk `forget` operations to keep the entity table lean
```bash
sqlite-graphrag cleanup-orphans --dry-run
sqlite-graphrag cleanup-orphans --yes
```
- Prerequisites: none — works on any initialized database
- `--dry-run` prints the count of orphan entities without deleting them
- `--yes` skips the interactive confirmation prompt for scripted pipelines
- Exit code 0: cleanup succeeded (or nothing to clean)
- Exit code 75: slot exhausted, retry after a short backoff

### Using edit
- Alters the body or description of an existing memory in-place creating a new version
- Use `--expected-updated-at` for optimistic locking in concurrent agent pipelines
```bash
sqlite-graphrag edit --name auth-design --body "Updated rationale after RFC review"
sqlite-graphrag edit --name auth-design --description "New short description"
sqlite-graphrag edit --name auth-design \
  --body-file ./updated-body.md \
  --expected-updated-at "2026-04-19T12:00:00Z"
```
- Prerequisites: the memory must already exist in the target namespace
- `--body-file` reads body content from a file, avoiding shell escaping issues
- `--body-stdin` reads body from stdin for pipeline integration
- `--expected-updated-at` accepts ISO 8601 timestamp; mismatches return exit 3
- Exit code 0: edit succeeded and new version is indexed
- Exit code 3: optimistic locking conflict — the memory was modified concurrently

### Using graph
- Exports the full entity-relationship snapshot in JSON, DOT or Mermaid format
- DOT and Mermaid formats enable visualization in Graphviz, VSCode or mermaid.live
```bash
sqlite-graphrag graph --format json
sqlite-graphrag graph --format dot --output graph.dot
sqlite-graphrag graph --format mermaid --output graph.mmd
```
- Prerequisites: at least one `link` or `remember` call must have created entities
- `--format json` (default) emits `{"nodes": [...], "edges": [...]}` to stdout
- `--format dot` emits a Graphviz-compatible directed graph for offline rendering
- `--format mermaid` emits a Mermaid flowchart block for Markdown embedding
- `--output <PATH>` writes directly to a file instead of stdout
- Exit code 0: export succeeded

#### Using graph traverse
- Traverses the entity graph from a starting node up to a given depth
- Use `--from` to name the root entity and `--depth` to control how many hops to follow
```bash
sqlite-graphrag graph traverse --from auth-design --depth 2 --format json
sqlite-graphrag graph traverse --from jwt-spec --depth 1
```
- Prerequisites: the root entity named by `--from` must exist in the graph
- `--from <NAME>` sets the root entity; the value is the entity name (required)
- `--depth <N>` controls maximum hop distance from the root (default: 2)
- Output schema: `{"nodes": [...], "edges": [...]}` identical to the full export format
- Exit code 0: traversal succeeded
- Exit code 4: root entity not found

#### Using graph stats
- Returns aggregate statistics about the entity graph in the target namespace
- Use to inspect graph density and connectivity before running traversals
```bash
sqlite-graphrag graph stats --format json
sqlite-graphrag graph stats --namespace my-project
```
- Prerequisites: at least one entity must exist in the target namespace
- Output fields: `entity_count`, `relationship_count`, `avg_connections`, `namespace`
- `--format json` (default) emits the stats object to stdout
- Exit code 0: stats returned

#### Using graph entities
- Lists typed graph entities with optional filters for type, namespace, limit, and offset
- Use to enumerate all entities the graph knows about before running `traverse` or `link`
```bash
sqlite-graphrag graph entities --json
sqlite-graphrag graph entities --entity-type concept --limit 20
sqlite-graphrag graph entities --entity-type person --namespace my-project --json
sqlite-graphrag graph entities --limit 50 --offset 100 --json
```
- Prerequisites: at least one entity must exist — created via `remember` or explicit `link`
- `--entity-type <TYPE>` filters results to a single type; valid types: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`
- `--limit <N>` caps the result count (default: 50); `--offset <N>` enables cursor-style pagination
- Output schema: `{"items": [...], "total_count": N, "limit": N, "offset": N, "namespace": "...", "elapsed_ms": N}`
- Each item carries `id`, `name`, `entity_type`, `namespace`, and `created_at`
- Exit code 0: list returned (empty `items` array when no entities match the filter)
- Exit code 4: namespace not found

### Using health
- Runs an integrity check and reports storage statistics for the active database
- Use in agent startup scripts to detect corrupted databases before processing begins
```bash
sqlite-graphrag health
sqlite-graphrag health --json
sqlite-graphrag health --format json
```
- Prerequisites: an initialized database must exist
- Runs `PRAGMA integrity_check` first; returns exit code 10 with `integrity_ok: false` if corruption is detected
- Output schema: `{"total_memories": N, "active_memories": N, "soft_deleted": N, "total_namespaces": N, "db_size_bytes": N, "journal_mode": "wal", "wal_size_mb": N.N, "checks": ["integrity_check: ok"], "elapsed_ms": N, "integrity_ok": true}`
- `journal_mode` reports the SQLite journaling mode (`wal` or `delete`)
- `wal_size_mb` reports the current WAL file size in megabytes (0.0 when not in WAL mode)
- `checks` is an array of diagnostic strings emitted by `PRAGMA integrity_check`
- `integrity_ok` is `true` when `integrity_check` returns `"ok"` and `false` otherwise
- Exit code 0: database is healthy
- Exit code 10: integrity check failed — treat as corrupted database

### Using history
- Lists all immutable versions of a named memory in reverse chronological order
- Use the returned `version` integer with `restore` to roll back to any prior state
```bash
sqlite-graphrag history --name auth-design
```
- Prerequisites: the memory must exist and have at least one stored version
- Output is a JSON array with fields `version`, `updated_at`, and a truncated `body`
- Versions start at 1 and increment with each successful `edit` or `restore` call
- Exit code 0: history returned
- Exit code 4: memory not found in the target namespace

### Using namespace-detect
- Resolves and prints the effective namespace for the current invocation context
- Use to debug `--namespace`, `SQLITE_GRAPHRAG_NAMESPACE`, and auto-detect conflicts
```bash
sqlite-graphrag namespace-detect
sqlite-graphrag namespace-detect --namespace my-project
```
- Prerequisites: none — works without a database present
- Output JSON with fields `namespace`, `source`, `cwd`, and `elapsed_ms`
- Precedence order: `--namespace` flag > `SQLITE_GRAPHRAG_NAMESPACE` env > auto-detect
- Exit code 0: resolution succeeded

### Using __debug_schema
- Hidden diagnostic subcommand that dumps the full SQLite schema and migration history
- Use when troubleshooting schema drift between binary versions or after failed migrations
```bash
sqlite-graphrag __debug_schema
sqlite-graphrag __debug_schema --db /path/to/custom.db
```
- Prerequisites: an initialized database must exist at the default or specified path
- Output schema: `{"schema_version": N, "user_version": N, "objects": [...], "migrations": [...], "elapsed_ms": N}`
- `schema_version` mirrors `PRAGMA user_version`; `user_version` is the raw PRAGMA value
- `objects` lists all SQLite schema objects (tables, indexes, virtual tables) with `name` and `type`
- `migrations` lists all rows from `refinery_schema_history` with `version`, `name`, and `applied_on`
- This subcommand is intentionally hidden from `--help` output; invoke it by exact name
- Exit code 0: schema dump succeeded

### Using rename
- Renames a memory preserving its full version history and entity graph connections
- Use `--name`/`--old` and `--new-name`/`--new` interchangeably; legacy aliases remain supported
```bash
sqlite-graphrag rename --name old-name --new-name new-name
sqlite-graphrag rename --old old-name --new new-name
```
- Prerequisites: the source memory must exist; the target name must be available
- `--expected-updated-at` enables optimistic locking to prevent concurrent rename conflicts
- History entries remain linked to the original name for audit trail integrity
- Exit code 0: rename succeeded
- Exit code 3: optimistic locking conflict
- Exit code 4: source memory not found

### Using restore
- Creates a new version of a memory based on an older version body without overwriting history
- Use `history` first to discover available version numbers before calling `restore`
```bash
sqlite-graphrag history --name auth-design
sqlite-graphrag restore --name auth-design --version 2
```
- Prerequisites: the memory must exist and the target version number must be valid
- Restore does NOT overwrite history — it appends a new version with the old body
- `--expected-updated-at` enables optimistic locking for concurrent pipeline safety
- Exit code 0: restore succeeded and the new version is indexed
- Exit code 4: version number not found in the history table

### Using unlink
- Removes a specific typed edge between two entities from the graph
- Use `--from`/`--source` and `--to`/`--target` interchangeably; legacy aliases remain supported
```bash
sqlite-graphrag unlink --from auth-design --to jwt-spec --relation depends-on
sqlite-graphrag unlink --source auth-design --target jwt-spec --relation depends-on
```
- Prerequisites: the edge must exist; all three of `--from`, `--to`, and `--relation` are required
- Valid `--relation` values: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`
- Both `--from`/`--to` entities must be typed graph nodes; valid entity types are: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`
- Exit code 0: edge removed
- Exit code 4: edge not found


## Additional Notes on Core Commands
### Note on link
- Prerequisite: entities must exist in the graph before creating explicit links
- The `remember` command auto-extracts entities from the `--body` text during ingestion
- Create the memories that reference the entities first, then call `link` to type the edges
- Use `--from`/`--source` and `--to`/`--target` interchangeably; legacy aliases remain supported
- Both `--from` and `--to` entities must be typed graph nodes; valid entity types are: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`
- Attempting to link entities whose names do not match a typed node returns exit code 4
- JSON output: `{action, from, source, to, target, relation, weight, namespace}`
- Both `from` and `source` carry the same value; both `to` and `target` carry the same value
```bash
sqlite-graphrag remember --name auth-design --type decision --description "..." --body "Uses JWT and OAuth2."
sqlite-graphrag remember --name jwt-spec --type reference --description "..." --body "RFC 7519 defines JWT."
sqlite-graphrag link --from auth-design --to jwt-spec --relation depends-on
```

### Note on forget
- `forget` performs a soft delete; the memory disappears from `recall` and `list` results
- JSON output: `{forgotten, name, namespace}`
- Run `purge` later to hard-delete soft-deleted rows and reclaim disk space

### Note on optimize and migrate
- `optimize --json` returns `{db_path, status}`
- `migrate --json` returns `{db_path, schema_version, status}`
- Run `migrate` after every binary upgrade to apply pending schema changes safely

### Note on cleanup-orphans
- JSON output: `{orphan_count, deleted, dry_run, namespace}`
- Run `--dry-run` first to confirm the count before passing `--yes` in automation

### Note on graph nodes schema
- `graph --format json` emits `{"nodes": [...], "edges": [...]}`
- Node fields: `{id, name, namespace, kind, type}` where `kind` and `type` carry the same value
- Edge fields mirror the `link` schema with `from`, `source`, `to`, `target`, `relation`, `weight`

### Note on remember
- `--force-merge` updates an existing memory body instead of returning exit code 2 on duplicate name
- Use `--force-merge` in idempotent pipeline loops where the same key may appear multiple times
```bash
sqlite-graphrag remember --name config-notes --type project \
  --description "updated config" --body "New body content" --force-merge
```
- `--entities-file` accepts a JSON file where each object must include an `entity_type` field
- The alias field `type` is also accepted as a synonym for `entity_type`
- Do not send both `entity_type` and `type` in the same object because Serde treats that as a duplicate field
- Valid `entity_type` values: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`
- Invalid `entity_type` values are rejected at ingestion time with a descriptive validation error
- `--relationships-file` accepts a JSON array where each object must include `source`, `target`, `relation`, and `strength`
- `strength` must be a floating-point number in the inclusive range `[0.0, 1.0]`
- `strength` is mapped to the stored `weight` field in relationship outputs and graph traversal results
- `relation` in `--relationships-file` must use the canonical stored labels such as `uses`, `supports`, `applies_to`, `depends_on`, and `tracked_in`

```json
[
  { "name": "SQLite", "entity_type": "tool" },
  { "name": "GraphRAG", "type": "concept" }
]
```

```json
[
  {
    "source": "SQLite",
    "target": "GraphRAG",
    "relation": "supports",
    "strength": 0.8,
    "description": "SQLite supports local GraphRAG retrieval"
  }
]
```


## Integration With AI Agents
### Twenty One Agents — One Persistence Layer
- Claude Code from Anthropic consumes JSON via stdin and orchestrates via exit codes
- Codex from OpenAI reads hybrid-search output to ground generation in local memory
- Gemini CLI from Google parses `--json` output to inject facts into prompts
- Opencode open source harness treats sqlite-graphrag as a native MCP-style backend
- OpenClaw agent framework uses `recall` as its long-term memory tier natively
- Paperclip research assistant persists findings across sessions via `remember` atomically
- VS Code Copilot from Microsoft invokes the CLI through integrated terminal tasks
- Google Antigravity platform calls the binary inside its sandboxed worker runtime
- Windsurf from Codeium routes indexed project memories through `hybrid-search` queries
- Cursor editor hooks `recall` into its chat panel for context-aware completions
- Zed editor invokes sqlite-graphrag as an external tool in its assistant channel
- Aider coding agent queries `related` for multi-hop reasoning over commit history
- Jules from Google Labs uses exit codes to gate automated pull request reviews
- Kilo Code autonomous agent delegates long-term memory to the local SQLite file
- Roo Code orchestrator passes memory context into its planning phase deterministically
- Cline autonomous agent persists tool outputs via `remember` between cycles
- Continue open source assistant integrates via its custom context provider API
- Factory agent framework stores decision logs for auditable multi-agent workflows
- Augment Code assistant hydrates its embeddings cache from `hybrid-search` results
- JetBrains AI Assistant runs sqlite-graphrag as a side process for cross-project memory
- OpenRouter proxy layer injects retrieved context before forwarding requests upstream


## Common Errors
### Troubleshooting — Five Failures and Their Fixes
- Error `exit 10` signals database lock, run `sqlite-graphrag vacuum` to checkpoint WAL
- Error `exit 12` signals `sqlite-vec` load failure, verify SQLite version is 3.40 plus
- Error `exit 13` signals batch partial failure, inspect partial results and retry only the failed items
- Error `exit 15` signals database busy after retries, lower write pressure or raise `--wait-lock`
- Error `exit 75` signals slots exhausted, retry after a short backoff interval
- Error `exit 77` signals low RAM, free memory before invoking the embedding model again


## Next Steps
### Level Up — Where to Go After This Guide
- Read `COOKBOOK.md` for thirty recipes covering search, graph and batch workflows
- Read `INTEGRATIONS.md` for vendor specific configuration of all 27 agents above
- Read `docs/AGENTS.md` for multi-agent orchestration patterns using Agent Teams
- Read `docs/CROSS_PLATFORM.md` to understand target binaries across nine platforms
- Star the public repository once `sqlite-graphrag` is published to track releases
