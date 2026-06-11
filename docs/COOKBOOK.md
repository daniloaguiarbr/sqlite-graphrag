# sqlite-graphrag Cookbook


> 34 production-grade recipes that save your team hours every single week

- Read the Portuguese version at [COOKBOOK.pt-BR.md](COOKBOOK.pt-BR.md)


## CLI Flag Aliases (since v1.0.35)
- `recall` and `hybrid-search` accept `--limit` as an alias of `-k`/`--k`. Recipes below use `--k`; either works.
- `rename` accepts `--from`/`--to` as aliases of `--name`/`--new-name`.
- `schema_version` JSON fields (`init`, `stats`, `migrate`, `health`) are emitted as JSON numbers since v1.0.35.
- `rename` accepts positional arguments: `rename <old> <new>` (since v1.0.44)
- `related` accepts a positional name argument: `related <name>` (since v1.0.44)
- `graph entities` JSON response uses `entities` as the top-level array key (renamed from `items` in v1.0.44)
- `link --create-missing` auto-creates nonexistent entities during link (since v1.0.44)
- `hybrid-search --with-graph` enables graph traversal seeded from top RRF results (since v1.0.44)


## Latency Note — v1.0.76 LLM-Only
- The CLI is 100% one-shot. Every `remember` / `ingest` / `recall` / `hybrid-search` spawns a headless `claude -p` or `codex exec` subprocess (OAuth) for embedding generation
- There is no daemon, no IPC, no background process
- Subprocess spawn cost is approximately 1-3 seconds per call
- Batch pipelines should batch chunks on the LLM side (one prompt with N passages) via `embed_passages_controlled` to amortize spawn cost.
- Operators with very large corpora should rely on FTS5 (`hybrid-search --k 50`) for coarse filtering before reaching the cosine refinement (see ADR-0024).


## Default Values Reference
- `recall --k` default is 10 (not 5) — adjust for precision-recall tradeoff
- `list --limit` default is 50 — use `--limit 10000` for full exports before backup
- `hybrid-search --weight-vec` and `--weight-fts` both default to 1.0
- `purge --retention-days` default is 90 — lower for aggressive cleanup policies
- `ingest --max-files` default is 10000 — all-or-nothing safety cap, not a sliding window
- `ingest --ingest-parallelism` default is `min(4, max(1, cpus/2))`
- `ingest --type` default is `document` when omitted
- `link --weight` default is 0.5
- `graph traverse --depth` default is 2
- `hybrid-search --min-weight` default is 0.3 when `--with-graph` is active


## How To Bootstrap Memory Database In 60 Seconds
### Problem
- Your new laptop has no memory database and your agent keeps losing context
- Every onboarding session burns 30 minutes on fragile setup scripts and README hunts


### Solution
```bash
cargo install --path .
sqlite-graphrag init --namespace default
sqlite-graphrag health --json
```


### Explanation
- Command `init` creates the SQLite file and validates that an LLM CLI (`claude` or `codex`) is reachable on `PATH`. No model download; the LLM subprocess is the model.
- Flag `--namespace default` is a user-chosen name; the built-in fallback namespace is `global`
- Command `health` validates integrity with `PRAGMA integrity_check` and returns JSON
- Exit code `0` signals the database is ready for writes and reads from any agent
- Saves 30 minutes per laptop versus a Pinecone plus Docker plus Python bootstrap


### Variants
- Set `SQLITE_GRAPHRAG_DB_PATH=/data/team.sqlite` to share a networked file between dev pods
- Call `sqlite-graphrag migrate --json` after bumping versions to apply schema upgrades


### See Also
- Recipe "How to integrate sqlite-graphrag with Claude Code subprocess loop"
- Recipe "How to schedule purge and vacuum in cron or GitHub Actions"


## How To Start And Monitor The Daemon For Lower Latency — REMOVED (v1.0.76+; code deleted in v1.0.79)
### Problem (v1.0.74 and earlier)
- Every `recall` and `remember` call paid a 1-second cold start to load the ONNX embedding model
- Interactive agent sessions felt sluggish because the model loaded and unloaded on every invocation

### Status (v1.0.76+)
- The `daemon` subcommand and ALL daemon infrastructure were REMOVED from the codebase
- The CLI is 100% one-shot: every `remember` / `ingest` / `recall` / `hybrid-search` spawns a headless `claude -p` or `codex exec` subprocess for embedding generation
- There is no IPC, no Unix socket, no background process
- See ADR-0021 for the deprecation rationale and ADR-0019 for the LLM-only architecture


### See Also
- Recipe "How to bootstrap memory database in 60 seconds"
- Recipe "How to benchmark hybrid-search against pure vec search"


## How To Upgrade From v1.0.74 Or v1.0.75 To v1.0.76 (LLM-Only)
### Problem
- You have a v1.0.74 or v1.0.75 database with vec0 virtual tables (`vec_memories`, `vec_entities`, `vec_chunks`)
- The v1.0.76 binary removed `sqlite-vec`, `fastembed`, `GLiNER`, and the `tokenizers` crate
- A plain `migrate` may fail with "applied migration V2 is different than filesystem one V2" (refinery checksum mismatch) because V002 was intentionally emptied to a no-op for v1.0.76
- A mislabeled binary can still report `1.0.76` while embedding the old v1.0.54 V002 file
- You want the upgrade to be one-shot, not a manual SQL session

### Solution
```bash
# 1. Install the new binary
cargo install sqlite-graphrag --locked --force

# 2. One-shot upgrade on your existing database
sqlite-graphrag migrate --to-llm-only --drop-vec-tables --db /path/to/graphrag.sqlite
```

### Explanation
- `--to-llm-only` runs three things in one transaction:
  - Detects whether `vec_memories` / `vec_entities` / `vec_chunks` virtual tables exist in `sqlite_master` and reports `vec_tables_were_present: true|false`
  - Rewrites recorded migration checksums to match the current file content (covers the V002 mismatch; see `--rehash` below for the standalone flag)
  - Applies the V013 migration which drops the three vec tables and creates the BLOB-backed `memory_embeddings` / `entity_embeddings` / `chunk_embeddings` tables
- `--drop-vec-tables` is the explicit safety guard; without it, `--to-llm-only` refuses to run
- The CLI is `~6 MB` (down from 39 MB); no ONNX model download; no local fastembed install
- New `remember` / `edit` / `ingest` calls re-embed the affected memory via `claude code` or `codex` headless subprocess (OAuth)
- See `docs/MIGRATION.md` for the full v1.0.74 → v1.0.76 → v1.1.0 path and `docs/decisions/adr-0019-llm-only-one-shot.md` for the architectural rationale

### Variants
- If you only need the checksum rewrite without applying migrations, use `sqlite-graphrag migrate --rehash`
- If the mismatch persists even though `sqlite-graphrag --version` says `1.0.76`, rebuild from the local source checkout and replace the installed binary before touching `refinery_schema_history`:

```bash
cargo build --release
cp target/release/sqlite-graphrag ~/.cargo/bin/sqlite-graphrag
```

- See `docs/decisions/adr-0026-v002-vec-tables-migration-drift.md` for the full root cause and validation trail
- The `embedding-legacy` feature was removed in v1.0.79; to keep the v1.0.74 fastembed pipeline you must pin `--version 1.0.78` or older (unsupported)

### v1.0.78 Fix: `run_rehash` Registered V013 Without Executing SQL (G41)
- v1.0.76 and v1.0.77 had a bug (G41) where `migrate --rehash` inserted phantom rows for unapplied migrations into `refinery_schema_history`
- This caused the refinery runner to skip V013 entirely — the BLOB-backed embedding tables were never created
- Symptoms: `no such table: memory_embeddings` (exit 10) on `recall`, `hybrid-search`, `remember`
- v1.0.78 removes the phantom insertion and adds auto-repair in every CRUD command

```bash
cargo install sqlite-graphrag --version 1.0.78 --force
# Any command auto-repairs — no explicit migrate needed:
sqlite-graphrag recall "test query" --json
```

- See ADR-0028 and `docs/MIGRATION.md` for the full details

### v1.0.77 Fix: `migrate --rehash` Inserted `applied_on = NULL`
- v1.0.76 had a bug (G40) where `migrate --rehash` inserted rows into `refinery_schema_history` without the `applied_on` field, leaving it NULL
- The refinery-core 0.9.1 rusqlite driver reads `applied_on` as `String` (NOT NULL), crashing with `InvalidColumnType(Null at index: 2)` on the next migration
- v1.0.77 automatically detects and fixes these NULL rows before running the migration runner
- If you hit this bug on v1.0.76, upgrade to v1.0.77 — no manual SQL intervention is needed:

```bash
cargo install sqlite-graphrag --version 1.0.77 --force
sqlite-graphrag migrate
```

- See ADR-0027 and `docs/MIGRATION.md` for the full details


## How To Bulk-Import A Knowledge Base Directory
### Problem
- Your 2000 Markdown files sit idle because no loader speaks the sqlite-graphrag schema
- Manual entry burns one entire afternoon per hundred files on simple onboarding


### Solution
```bash
sqlite-graphrag ingest ./docs --recursive --pattern "*.md" --json \
  | jaq -c 'select(.status == "indexed") | .name'
```


### Explanation
- `ingest` replaces the `fd | xargs remember` loop with a single atomic command handling recursion and naming
- `--recursive` descends into subdirectories; without it only top-level files are processed
- `--pattern "*.md"` filters by extension; default is `*.md` so this flag is shown for clarity
- Output is NDJSON: one JSON line per file with `status` field, plus a final summary line with `summary: true`
- Names derive from file basenames in kebab-case; names over 60 chars are truncated with `truncated: true` in the NDJSON
- Saves 4 hours per thousand files versus hand-crafted import scripts or `fd | xargs` loops


### Variants
- Automatic extraction is disabled by default; use `--enable-ner` or `SQLITE_GRAPHRAG_ENABLE_NER=1` to activate it — URL-regex ONLY since v1.0.79 (the GLiNER pipeline was removed)
- `--skip-extraction` is deprecated since v1.0.45 and has no effect; `--gliner-variant` is a no-op since v1.0.79 and emits a `tracing::warn!` when set
- Response field `extraction_method` reports `url-regex` or `none:extraction-failed`; the `gliner-<variant>+regex` and `regex-only` values are HISTORICAL (≤ v1.0.75)
- Duplicate files return `status: "skipped"` with `action: "duplicate"` instead of `status: "failed"`
- Use `--fail-fast` to abort on the first per-file error instead of continuing with inline error reporting
- Use `--max-rss-mb <MiB>` to abort embedding when process RSS exceeds the limit (default 8192 MiB); useful in memory-constrained CI or containers


### See Also
- Recipe "How to ingest large corpora on memory-constrained hosts"
- Recipe "How to export memories to NDJSON for backup"


## How To Ingest A Typed Directory With Streaming Progress
### Problem
- Your CI pipeline ingests 2000 decision documents but has no progress visibility during the run
- The summary-only approach hides per-file failures until the entire batch completes


### Solution
```bash
sqlite-graphrag ingest ./decisions --type decision --recursive --json \
  | while IFS= read -r line; do
      status=$(echo "$line" | jaq -r '.status // empty')
      if [ "$status" = "failed" ]; then
        echo "FAIL: $(echo "$line" | jaq -r '.file')" >&2
      elif [ "$status" = "skipped" ]; then
        echo "SKIP: $(echo "$line" | jaq -r '.file') (duplicate or invalid name)"
      fi
    done
```


### Explanation
- `--type decision` tags every ingested file as a `decision` memory; default type is `document`
- NDJSON output streams one line per file followed by a summary line with `summary: true`
- The `while read` loop processes each line as it arrives instead of waiting for the full batch
- Filter by `select(.status)` to skip the summary line which has no `status` field
- Valid `--type` values: `user`, `feedback`, `project`, `reference`, `decision`, `incident`, `skill`, `document`, `note`
- Invoke `ingest` separately per type when a directory contains mixed content


### Variants
- Aggregate final stats: `| jaq -sc '[.[] | select(.status)] | group_by(.status) | map({status: .[0].status, count: length})'`
- Use `--pattern "memo-*"` to filter by basename prefix instead of extension


### See Also
- Recipe "How to bulk-import a knowledge base directory"
- Recipe "How to export memories to NDJSON for backup"


## How To Combine Vector And FTS Search With Tunable Weights
### Problem
- Pure vector recall misses exact token matches like `TODO-1234` inside code comments
- Pure FTS search misses paraphrases your users typed in synonyms and abbreviations


### Solution
```bash
sqlite-graphrag hybrid-search "postgres migration deadlock" \
  --k 10 --rrf-k 60 --json
```


### Explanation
- `--rrf-k 60` is the Reciprocal Rank Fusion smoothing constant recommended by RRF literature
- Default `--weight-vec 1.0` and `--weight-fts 1.0` treat both signals as equally important
- Override for advanced tuning: `--weight-vec 0.7 --weight-fts 0.3` biases toward semantics
- JSON emits `vec_rank` and `fts_rank` per result so downstream agents can audit fusion
- Saves 50 percent tokens versus asking an LLM to re-rank after pure vector recall


### Variants
- Pass `--weight-vec 1.0 --weight-fts 0.0` to reproduce a pure `recall` baseline for A/B tests
- Raise `--k` to 50 before a re-ranker agent prunes down to the final 5 hits
- Pass `--with-graph --max-hops 2` to seed graph traversal from the top RRF results; read both `results[]` and `graph_matches[]` in the output (since v1.0.44)


### See Also
- Recipe "How to debug slow queries with health and stats"
- Recipe "How to expand hybrid search with graph context"


## How To Expand Hybrid Search With Graph Context
### Problem
- Your hybrid search finds the right seed memories but misses related concepts connected through the entity graph
- Running a separate `related` command after every hybrid search adds pipeline complexity and latency


### Solution
```bash
sqlite-graphrag hybrid-search "authentication architecture" \
  --k 10 --with-graph --max-hops 2 --min-weight 0.3 --json \
  | jaq -r '(.results[], .graph_matches[]) | .name' | sort -u
```


### Explanation
- `--with-graph` enables entity graph traversal seeded from the top RRF results (fixed in v1.0.44)
- Graph matches appear in `graph_matches[]`, a SEPARATE array from `results[]`; read BOTH arrays
- `graph_matches[]` uses RecallItem schema: `name`, `distance`, `source` ("graph"), `graph_depth`
- `--min-weight 0.3` filters weak graph edges to reduce noise from low-confidence relationships
- `--max-hops 2` controls traversal depth; increase only after checking density via `graph stats`
- Eliminates the need for a separate `related` call, reducing pipeline steps from three to two


### Variants
- Set `--min-weight 0.0` to include all edges regardless of weight for maximum recall at higher noise
- Extract names from both arrays: `jaq -r '(.results[], .graph_matches[]) | .name' | sort -u > seeds.txt`


### See Also
- Recipe "How to combine vector and FTS search with tunable weights"
- Recipe "How to explore the entity graph with stats, entities, and traverse"


## How To Run Deep Research For Comprehensive Multi-Hop Analysis
### Problem
- You need to research a complex topic that spans multiple memories and graph connections
- Running the manual 3-layer pipeline (hybrid-search, read, related) for each sub-topic is tedious and slow


### Solution
```bash
sqlite-graphrag deep-research "authentication architecture decisions and security incidents" --k 20 --max-hops 3 --json
```


### Explanation
- Decomposes "authentication architecture decisions and security incidents" into sub-queries ("authentication architecture decisions", "security incidents", "authentication security") and runs them in parallel with graph traversal
- Returns deduplicated results across all sub-queries plus evidence chains showing entity-relation-entity paths
- Single invocation replaces the manual 3-layer pipeline of hybrid-search, read, and related


### Variants
- Add `--with-bodies` for full memory content in the response
- Add `--max-concurrency 4` to limit parallelism on constrained hosts


### See Also
- Recipe "How to combine vector and FTS search with tunable weights"
- Recipe "How to expand hybrid search with graph context"
- Recipe "How to traverse entity graph for multi-hop recall"


## How To Traverse Entity Graph For Multi-Hop Recall
### Problem
- Your query hits one memory but misses connected notes sharing the same entity graph
- Pure vector RAG scores similar tokens and ignores typed relationships that matter


### Solution
```bash
sqlite-graphrag related authentication-flow --hops 2 --json
```


### Explanation
- `related` takes a MEMORY name (kebab-case slug), not an entity name
- The positional argument must match a name stored via `remember` in the same namespace
- `related` walks typed graph relationships between entities with user-controlled hop count
- `--hops 2` includes friends-of-friends memories linked through shared entities
- JSON output reports the traversal path so the LLM can reason about relation chains
- Positional name argument supported since v1.0.44: `related <name>` is equivalent to `related --name <name>`
- Saves re-embedding cost since graph expansion runs as SQLite graph walk not KNN
- Surfaces context that vector-only RAG misses by design with 80 percent fewer tokens


### Variants
- Use `graph --json` to dump the full snapshot when a human auditor wants offline analysis
- Chain `related` into `hybrid-search` by filtering candidates to the traversed set


### See Also
- Recipe "How to combine vector and FTS search with tunable weights"
- Recipe "How to orchestrate parallel recall across namespaces"


## How To Chain Three-Layer Deep Retrieval
### Problem
- Your agent sends one recall query and misses both the full body and the transitive graph neighbors
- Dumping all memories as markdown burns 72x more context tokens than a focused retrieval chain


### Solution
```bash
# Layer 1: hybrid-search finds seed memories via FTS5 + vector RRF
SEED=$(sqlite-graphrag hybrid-search "authentication architecture" --k 3 --json \
  | jaq -r '.results[0].name')

# Layer 2: read expands the full body of the top seed
sqlite-graphrag read "$SEED" --json | jaq -r '.body'

# Layer 3: related discovers transitive knowledge via the entity graph
sqlite-graphrag related "$SEED" --hops 2 --json \
  | jaq -r '.results[].name'
```


### Explanation
- Layer 1 (hybrid-search) finds the best-matching memories using combined text and vector ranking
- Layer 2 (read) retrieves the complete body of the top match (hybrid-search returns truncated snippets)
- Layer 3 (related) traverses the entity graph to discover connected memories invisible to vector search
- This pattern reduces context tokens by up to 72x versus dumping all memories as markdown
- Chain into an LLM prompt by collecting Layer 2 body plus Layer 3 names for a focused context window


### Variants
- Replace `--k 3` with `--k 1` when your queries are highly specific and you trust the top hit
- Increase `--hops` to 3 when your entity graph has sparse connectivity between topics


### See Also
- Recipe "How to combine vector and FTS search with tunable weights"
- Recipe "How to traverse entity graph for multi-hop recall"


## How To Link Entities With Auto-Creation
### Problem
- Creating graph edges requires entities to exist first, forcing a tedious two-step entity-creation workflow
- Your automation script fails with exit code 4 every time it tries to link entities it has not pre-registered


### Solution
```bash
sqlite-graphrag link \
  --from auth-service --to postgres-db \
  --relation depends-on --weight 0.8 \
  --create-missing --entity-type tool
```


### Explanation
- `--create-missing` auto-creates nonexistent entities defaulting to type `concept` (since v1.0.44)
- `--entity-type tool` overrides the default type for all auto-created entities in this invocation
- JSON response includes `created_entities: ["auth-service", "postgres-db"]` when entities were created
- `--weight` is optional with default 0.5; values must fall within the range `[0.0, 1.0]`
- 12 canonical relation types: `applies-to`, `uses`, `depends-on`, `causes`, `fixes`, `contradicts`, `supports`, `follows`, `related`, `mentions`, `replaces`, `tracked-in`. Any custom kebab-case or snake_case string is also accepted since v1.0.49 (e.g., `implements`, `tested-by`, `blocks`).
- Valid entity types: `project`, `tool`, `person`, `file`, `concept`, `incident`, `decision`, `memory`, `dashboard`, `issue_tracker`, `organization`, `location`, `date`


### Variants
- Omit `--create-missing` when entities must pre-exist; exit code 4 signals a missing entity
- Accept `--source`/`--target` as aliases of `--from`/`--to` for scripts that use source/target terminology


### See Also
- Recipe "How to remove a graph edge with unlink"
- Recipe "How to explore the entity graph with stats, entities, and traverse"


## How To Remove A Graph Edge With Unlink
### Problem
- An incorrect `depends-on` edge between two entities pollutes graph traversals with irrelevant paths
- The only removal option your team knows is deleting the entire memory, which destroys the body and version history


### Solution
```bash
sqlite-graphrag unlink --from auth-service --to legacy-db --relation depends-on
```


### Explanation
- `--from` and `--to` are required; `--relation` is optional — omit it to remove ALL relationships between the pair
- `--source`/`--target` are accepted as aliases of `--from`/`--to` for consistency with `link`
- The operation removes only the relationship edge; entities and memories remain untouched
- Exit code 4 signals that the specified edge does not exist in the current namespace
- Run `cleanup-orphans` afterward if the unlinked entities have no remaining connections


### Variants
- Chain `graph entities --json | jaq '.entities[].name'` to discover entity names before unlinking
- Use `graph stats` before and after to verify the edge count decreased as expected


### See Also
- Recipe "How to link entities with auto-creation"
- Recipe "How to clean orphan entities after bulk deletion"


## How To Clean Orphan Entities After Bulk Deletion
### Problem
- After forgetting 500 memories, the entity graph still contains hundreds of orphan nodes with no edges
- Graph traversal wastes cycles visiting dead-end entities that reference nothing


### Solution
```bash
sqlite-graphrag cleanup-orphans --dry-run --json
sqlite-graphrag cleanup-orphans --yes --json
```


### Explanation
- `--dry-run` audits orphan count without modifying the database; always run this first
- `--yes` bypasses the interactive confirmation prompt for use in automated pipelines
- Removes entities that have zero linked memories AND zero edges in the graph
- Schedule periodically after bulk `forget` operations or mass `unlink` runs
- Does not touch memories or version history; only graph entities are affected


### Variants
- Chain with `purge --retention-days 30 --yes` and `vacuum` in a weekly cron for full hygiene
- Inspect candidates first with `graph entities --json | jaq '.entities[] | select(.degree == 0)'` if available


### See Also
- Recipe "How to schedule purge and vacuum in cron or GitHub Actions"
- Recipe "How to remove a graph edge with unlink"


## How To Explore The Entity Graph With Stats, Entities, And Traverse
### Problem
- Your graph has grown to thousands of entities and you have no visibility into its density or connectivity
- Planning traversal depth without knowing `avg_degree` wastes time on empty subgraphs or overloaded fan-outs


### Solution
```bash
sqlite-graphrag graph stats --json | jaq '{node_count, edge_count, avg_degree}'
sqlite-graphrag graph entities --entity-type person --json | jaq '.entities[].name'
sqlite-graphrag graph traverse --from acme-corp --depth 3 --json
sqlite-graphrag graph --format mermaid --output graph.md
```


### Explanation
- `graph stats` reports `node_count`, `edge_count`, `avg_degree`, and `max_degree` to inform traversal planning
- `graph entities` lists all entities; field is `.entities[]` NOT `.items[]` since v1.0.44
- `graph traverse` starts from a typed entity (not a memory name) and walks up to `--depth` hops
- Hops return `entity`, `relation`, `direction`, `weight`, and `depth` per visited edge
- Export formats include `json`, `dot` (Graphviz), and `mermaid`; write to file via `--output <PATH>`
- Exit code 4 from `graph traverse` signals a nonexistent root entity


### Variants
- Filter entities by type: `--entity-type tool` shows only tool nodes
- Paginate large entity lists: `--limit 100 --offset 200` for datasets with thousands of entities


### See Also
- Recipe "How to expand hybrid search with graph context"
- Recipe "How to debug slow queries with health and stats"


## How To Integrate sqlite-graphrag With Claude Code Subprocess Loop
### Problem
- Claude Code restarts every session and forgets the decisions made five minutes ago
- Your orchestrator lacks a deterministic memory it can trust between agent iterations


### Solution
```bash
# .claude/hooks/pre-task.sh
CONTEXT=$(sqlite-graphrag recall "$USER_PROMPT" --k 5 --json)
printf 'Relevant memories:\n%s\n' "$CONTEXT"

# .claude/hooks/post-task.sh
sqlite-graphrag remember \
  --name "session-$(date +%s)" \
  --type project \
  --description "decision log" \
  --body "$ASSISTANT_RESPONSE"
```


### Explanation
- Pre-task hook injects relevant memories into the agent prompt before generation
- Post-task hook persists agent output into the vector store for future sessions
- Hook scripts run as subprocess respecting exit code routing and slot limits
- Exit code `13` or `75` triggers retry inside the hook without killing the agent
- Saves 40 percent context tokens and keeps decisions across Claude Code restarts


### Variants
- Replace `recall` with `hybrid-search` when your prompts mix keywords and concepts
- Add `--namespace $CLAUDE_PROJECT` to isolate per-project memory in multi-repo hosts


### See Also
- Recipe "How to integrate with Codex CLI via AGENTS.md"
- Recipe "How to setup Windsurf or Zed assistant panel with sqlite-graphrag"


## How To Integrate With Codex CLI Via AGENTS.md
### Problem
- Codex reads `AGENTS.md` but skips any capability not listed with exact invocation syntax
- Your ops team loses 10 minutes per session teaching Codex the same CLI from memory


### Solution
```md
<!-- AGENTS.md at repo root -->
## Memory Layer
- Use `sqlite-graphrag recall "<query>" --k 5 --json` to fetch prior decisions
- Use `sqlite-graphrag remember --name "<kebab-name>" --type project --body "<text>"` to persist output
- Prefer `hybrid-search` when the query mixes keywords and natural language
- Respect exit code 75 as retry-later rather than error
```


### Explanation
- AGENTS.md surfaces the CLI contract as part of Codex system context automatically
- Codex invokes subprocess commands listed in AGENTS.md without further operator prompting
- Deterministic exit codes allow Codex to retry on `75` without operator intervention
- JSON output integrates with Codex parsing layer without regex or custom plugin code
- Saves 10 minutes per session and survives Codex upgrades without breaking the contract


### Variants
- Add `SQLITE_GRAPHRAG_NAMESPACE=$REPO_NAME` to `.envrc` so Codex isolates per-project memory
- Include a one-liner example under each command to anchor Codex on real usage


### See Also
- Recipe "How to integrate sqlite-graphrag with Claude Code subprocess loop"
- Recipe "How to integrate with Cursor terminal for in-editor memory"


## How To Integrate With Cursor Terminal For In-Editor Memory
### Problem
- Cursor loses context every time you close the editor or switch between branches locally
- Your paired LLM session restarts cold and re-asks the same questions every morning


### Solution
```jsonc
// Cursor settings.json snippet
{
  "terminal.integrated.env.osx": { "SQLITE_GRAPHRAG_NAMESPACE": "${workspaceFolderBasename}" },
  "cursor.ai.rules": "Before answering, run `sqlite-graphrag recall \"${selection}\" --k 5 --json` and use hits as context"
}
```


### Explanation
- Per-workspace env var isolates memory by project folder name without manual config
- Cursor AI rules instruct the embedded model to call the CLI before answering prompts
- The CLI reads only the selected code so latency stays below 50 ms for small queries
- Exit code `0` with empty hits keeps Cursor silent instead of hallucinating context
- Saves 15 minutes per day of re-asking repeated questions inside Cursor sessions


### Variants
- Swap `recall` for `hybrid-search` when your codebase mixes English docstrings and Portuguese comments
- Add a `post-save` hook that calls `remember` with the diff as body for session-wide memory


### See Also
- Recipe "How to setup Windsurf or Zed assistant panel with sqlite-graphrag"
- Recipe "How to integrate with Codex CLI via AGENTS.md"


## How To Setup Windsurf Or Zed Assistant Panel With sqlite-graphrag
### Problem
- Windsurf and Zed assistant panels ship without pluggable memory backends by default
- Your multi-IDE workflow fragments memory between Cursor Windsurf and Zed silos


### Solution
```bash
# Shared terminal command both IDEs can run
sqlite-graphrag hybrid-search "$EDITOR_CONTEXT" --k 10 --json > /tmp/ng.json
```


### Explanation
- Both Windsurf and Zed call terminal tasks from the assistant panel natively
- `/tmp/ng.json` acts as a lingua franca consumed by both assistant panels for prompts
- Single CLI binary replaces three bespoke plugins avoiding per-IDE maintenance burden
- Exit code `0` with empty hits is benign so the assistant panel degrades gracefully
- Saves hours per week by unifying memory across all editors with no plugin rebuild


### Variants
- Map the command to a shortcut such as `Cmd+Shift+M` for one-key recall invocation
- Pipe output through `jaq` to transform the payload into the exact schema each IDE prefers


### See Also
- Recipe "How to integrate with Cursor terminal for in-editor memory"
- Recipe "How to orchestrate parallel recall across namespaces"


## How To Prevent Dropbox Or iCloud Corruption With sync-safe-copy
### Problem
- Your SQLite file sits in Dropbox and syncs mid-write corrupting the WAL journal
- Classic `cp` snapshots during a write produce invalid files that refuse to open later


### Solution
```bash
sqlite-graphrag sync-safe-copy --dest ~/Dropbox/sqlite-graphrag/snapshot.sqlite
```


### Explanation
- Command forces a WAL checkpoint before the copy so the snapshot is transactionally consistent
- Output file receives `chmod 600` on Unix to prevent other users from reading sensitive memories
- Copy runs atomically via `SQLite Online Backup API` eliminating partial-write risk entirely
- Exit code `0` guarantees the snapshot opens cleanly on any other machine with the same binary
- Saves weekends of recovery work when Dropbox would have otherwise corrupted the live file


### Variants
- Schedule hourly via `launchd` on macOS or `systemd --user` on Linux for continuous backup
- Compress with `ouch compress snapshot.sqlite snapshot.tar.zst` for faster cloud upload


### See Also
- Recipe "How to schedule purge and vacuum in cron or GitHub Actions"
- Recipe "How to version control the SQLite database with Git LFS"


## How To Schedule Purge And Vacuum In Cron Or GitHub Actions
### Problem
- Soft-deleted memories pile up and inflate disk usage over months of heavy agent use
- Your SQLite file balloons past 10 GB because `VACUUM` never runs in automation


### Solution
```yaml
# .github/workflows/ng-maintenance.yml
name: sqlite-graphrag maintenance
on:
  schedule: [{ cron: "0 3 * * 0" }]
jobs:
  maintenance:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo install --path .
      - run: sqlite-graphrag purge --retention-days 30 --yes
      - run: sqlite-graphrag vacuum --json
      - run: sqlite-graphrag optimize --json
```


### Explanation
- `purge --retention-days 30` hard-deletes soft-deleted rows older than the retention window
- `vacuum` reclaims freelist pages and checkpoints the WAL journal to the main file
- `optimize` refreshes query planner statistics for faster recall on the next run
- Weekly cron at 03:00 Sunday avoids contention with business-hour agent activity
- Saves 70 percent disk usage over 6 months versus zero-maintenance deployments


### Variants
- Run on `cron 0 3 * * *` nightly when your team writes thousands of memories per day
- Replace GitHub Actions with `systemd.timer` for air-gapped environments without internet


### See Also
- Recipe "How to prevent Dropbox or iCloud corruption with sync-safe-copy"
- Recipe "How to debug slow queries with health and stats"


## How To Export Memories To NDJSON For Backup
### Problem
- SQLite backups are opaque and require the binary installed for any restore audit
- Compliance asks for plain-text exports to diff between monthly snapshots


### Solution
```bash
sqlite-graphrag export > memories-$(date +%Y%m%d).ndjson
```


### Explanation
- `export` streams all memories as NDJSON with one JSON line per memory plus a summary
- Filter by type with `--type decision` or by namespace with `--namespace my-project`
- Include soft-deleted memories with `--include-deleted` for full audit trail
- Diff two snapshots with `difft` to audit what changed between monthly backups cleanly
- Since v1.0.53 every write checkpoints the WAL so cloud sync tools always see a consistent file


### Variants
- Pipe through `ouch compress` to a `zst` archive before uploading to S3 or GCS buckets
- Loop in shell to page through namespaces if the instance hosts multi-tenant memory


### See Also
- Recipe "How to version control the SQLite database with Git LFS"
- Recipe "How to schedule purge and vacuum in cron or GitHub Actions"


## How To Version Control The SQLite Database With Git LFS
### Problem
- Your 500 MB SQLite file breaks GitHub push limits and bloats every single clone
- Branch rebases corrupt binary blobs when Git tries to merge with textual diff logic


### Solution
```bash
git lfs install
git lfs track "*.sqlite"
echo "*.sqlite filter=lfs diff=lfs merge=lfs -text" >> .gitattributes
git add .gitattributes graphrag.sqlite
git commit -m "chore: track sqlite-graphrag db via LFS"
```


### Explanation
- Git LFS stores SQLite files in a remote cache so the Git repo stays below 100 MB
- Attribute `-text` prevents Git from attempting line-based merges on binary contents
- `sync-safe-copy` before commit guarantees the file is transactionally consistent to push
- Teammates clone with `git lfs pull` fetching the DB only when they actually need it
- Saves 90 percent clone time for teammates who do not need the memory database locally


### Variants
- Tag snapshots with `git tag db-2026-04-18` to pin memory state for release reproducibility
- Skip LFS and store sync-safe-copy outputs in object storage with signed URL references


### See Also
- Recipe "How to export memories to NDJSON for backup"
- Recipe "How to prevent Dropbox or iCloud corruption with sync-safe-copy"


## How To Orchestrate Namespace Recall Safely
### Problem
- Your multi-project agent needs one recall per namespace on the same host
- Blind parallel fan-out can oversubscribe RAM because each `recall` subprocess spawns an LLM subprocess


### Solution
```bash
for ns in project-a project-b project-c project-d; do
  SQLITE_GRAPHRAG_NAMESPACE="$ns" \
    sqlite-graphrag --max-concurrency 1 recall "error rate" --k 5 --json
done
```


### Explanation
- The loop stays intentionally serial because `recall` is an embedding-heavy command
- `--max-concurrency 1` prevents local oversubscription during audits, CI, and desktop use
- Env var `SQLITE_GRAPHRAG_NAMESPACE` scopes each subprocess to its own project cleanly
- One JSON document per namespace still lands in stdout for a downstream aggregator agent to fuse
- This pattern favors host safety and deterministic progress over aggressive wall-clock reduction


### Variants
- Keep parallel fan-out for light commands such as `stats` or `list`, not for `recall`
- Raise concurrency for heavy commands only after measuring RSS, observing swap, and confirming the host remains stable


### See Also
- Recipe "How to combine vector and FTS search with tunable weights"
- Recipe "How to benchmark hybrid-search against pure vec search"


## How To Handle Exit Codes In Automated Pipelines
### Problem
- Your CI pipeline treats every non-zero exit as fatal, killing retryable operations like exit 75 (slot exhaustion)
- Debugging pipeline failures takes 30 minutes because your wrapper does not distinguish validation from locking conflicts


### Solution
```bash
sqlite-graphrag remember --name "$NAME" --type project \
  --description "$DESC" --body-stdin < "$FILE"
rc=$?
case $rc in
  0)  echo "Success" ;;
  2)  echo "Bad CLI argument (invalid flag, bad timezone): fix usage" ;;
  9)  echo "Duplicate or soft-deleted: use --force-merge to restore and update" ;;
  3)  echo "Conflict: re-read and retry" ;;
  6)  echo "Payload too large: split body" ;;
  15) echo "Busy: widen --wait-lock" ;;
  75) echo "Slots full: wait, do NOT raise concurrency" ;;
  77) echo "RAM pressure: free memory first" ;;
  *)  echo "Fatal: rc=$rc" >&2; exit 1 ;;
esac
```


### Explanation
- 17 exit codes from 0 to 77 following sysexits.h conventions for machine-parseable error routing
- Exit 3 means optimistic locking conflict: reload the memory with `read --json` and retry
- Exit 13 means partial batch failure: reprocess only the failed items, NOT the entire batch
- Exit 75 and 77 signal resource pressure: NEVER increase concurrency after receiving these codes
- Exit 15 means database busy: widen `--wait-lock <ms>` to wait longer before failing
- Full code table: 0=success, 1=validation, 2=Clap-argument-error, 9=duplicate-or-soft-deleted, 3=conflict, 4=not-found, 5=namespace, 6=payload, 10=database, 11=embedding, 12=sqlite-vec, 13=partial, 14=I/O, 15=busy, 20=internal, 75=slots, 77=RAM


### Variants
- Wrap the case statement in a retry loop with exponential backoff for codes 3, 15, 75, and 77
- Log `stderr` separately: `2>error.log` captures human-readable messages while `stdout` captures JSON


### See Also
- Recipe "How to orchestrate namespace recall safely"
- Recipe "How to edit a memory with optimistic locking"


## How To Debug Slow Queries With Health And Stats
### Problem
- Your recall used to return in 8 ms and now takes 400 ms after months of writes
- You lack visibility into which table ballooned or which index went stale


### Solution
```bash
sqlite-graphrag health --json | jaq '{integrity, wal_size_mb, journal_mode}'
sqlite-graphrag stats --json | jaq '{memories, memories_total, entities, entities_total, relationships, relationships_total, edges, chunks_total, avg_body_len, db_size_bytes, db_bytes}'
SQLITE_GRAPHRAG_LOG_LEVEL=debug sqlite-graphrag recall "slow query" --k 5 --json
sqlite-graphrag optimize --json
sqlite-graphrag debug-schema --json | jaq '{schema_version, objects: (.objects | length)}'
```


### Explanation
- `health` reports `integrity`, WAL size and journal mode to spot fragmentation fast
- `stats` counts rows to reveal which table grew disproportionately since last audit
- `SQLITE_GRAPHRAG_LOG_LEVEL=debug` emits timings per SQLite stage to stderr for tracing
- Comparing current `avg_body_len` to baseline shows if bodies have grown past defaults
- `optimize` refreshes query planner statistics so the next recall or hybrid-search uses updated indexes
- `debug-schema` is a hidden command that dumps schema version, object count, and migration history for troubleshooting drift
- Saves hours of blind tuning by exposing the exact slow path in three commands total


### Variants
- Schedule a dashboard that scrapes `stats --json` every hour and alerts on growth spikes
- Run `optimize` followed by `vacuum` when WAL exceeds 100 MB to reclaim disk performance


### See Also
- Recipe "How to schedule purge and vacuum in cron or GitHub Actions"
- Recipe "How to benchmark hybrid-search against pure vec search"


## How To Verify Embedding Health (v1.0.76)
### Problem
- You need to confirm that the LLM embedding pipeline is working after a binary upgrade
- You want to check database integrity before running a large batch ingest


### Solution
```bash
sqlite-graphrag health --json | jaq '{integrity_ok, fts_ok, fts_query_ok, vec_memories_ok}'
sqlite-graphrag stats --json | jaq '{memories, entities, relationships}'
```


### Explanation
- The `cache` subcommand was removed in v1.0.76; all embedding is handled by the LLM subprocess
- Use `health --json` to verify database integrity and FTS5 index status
- `stats` provides global counts for capacity monitoring
- No local model files to manage; the LLM subprocess handles model loading internally


### See Also
- Recipe "How to debug slow queries with health and stats"
- Recipe "How to schedule purge and vacuum in cron or GitHub Actions"


## How To Benchmark hybrid-search Against Pure vec search
### Problem
- You lack data to justify enabling hybrid search in production versus pure vector recall
- Your stakeholders want numeric evidence before approving the index storage overhead


### Solution
```bash
hyperfine --warmup 3 \
  'sqlite-graphrag recall "postgres migration" --k 10 --json > /dev/null' \
  'sqlite-graphrag hybrid-search "postgres migration" --k 10 --json > /dev/null'
```


### Explanation
- `hyperfine` measures both commands with warmup runs removing cold-cache noise from results
- Output reports mean latency standard deviation and relative speedup in a clean table
- Results let you compare recall quality versus latency on real production workloads
- Numeric evidence empowers tradeoff conversations with product and finance stakeholders
- Saves weeks of debate by grounding the decision in data rather than intuition alone


### Variants
- Replace the single query with 100 sampled queries to compute p50 p95 p99 latency buckets
- Integrate `hyperfine --export-json` into CI to detect regressions across pull requests


### See Also
- Recipe "How to combine vector and FTS search with tunable weights"
- Recipe "How to orchestrate parallel recall across namespaces"


## How To Integrate With rig-core For Agent Memory
### Problem
- Your `rig-core` agent loses context between invocations without persistent storage
- Rebuilding embeddings every run wastes 50 minutes of compute and API budget weekly

### Solution
```rust
use std::process::Command;
use serde_json::Value;

fn remember_agent_context(namespace: &str, content: &str) -> anyhow::Result<()> {
    let name = format!(
        "rig-context-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let status = Command::new("sqlite-graphrag")
        .args([
            "remember",
            "--namespace", namespace,
            "--name", &name,
            "--type", "project",
            "--description", "rig-core agent context",
            "--body", content,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "sqlite-graphrag remember failed");
    Ok(())
}

fn recall_agent_context(namespace: &str, query: &str, k: u8) -> anyhow::Result<Vec<String>> {
    let output = Command::new("sqlite-graphrag")
        .args(["recall", "--namespace", namespace, "--k", &k.to_string(), "--json", query])
        .output()?;
    anyhow::ensure!(output.status.success(), "sqlite-graphrag recall failed");
    let parsed: Value = serde_json::from_slice(&output.stdout)?;
    let items = parsed["results"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v["snippet"].as_str().map(str::to_owned))
        .collect();
    Ok(items)
}
```

### Explanation
- `Command::new("sqlite-graphrag")` shells out to the 25 MB stateless binary with zero FFI cost
- `--namespace` scopes memory to the specific rig agent preventing cross-agent contamination
- `--json` returns structured output that `serde_json` parses without fragile regex parsing
- `anyhow::ensure!` converts exit-code failures into typed errors your agent can handle
- Reduces 50 minutes of per-run context rebuilding to a single 5-millisecond CLI call

### Variants
- Replace `Command` with `tokio::process::Command` for non-blocking async agent pipelines
- Wrap both functions in a `RigMemoryAdapter` struct that implements a `MemoryStore` trait

### See Also
- Recipe "How to bootstrap memory database in 60 seconds"
- Recipe "How to run ollama offline with ollama-rs and persistent memory"


## How To Integrate With swarms-rs For Multi-Agent Memory
### Problem
- Your swarm of agents overwrites each other's memories when sharing one namespace
- Debugging which agent wrote what takes hours of grep through unstructured log files

### Solution
```rust
use std::process::Command;

fn swarm_remember(agent_id: &str, content: &str) -> anyhow::Result<()> {
    let namespace = format!("swarm-{agent_id}");
    let name = format!(
        "swarm-note-{agent_id}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let status = Command::new("sqlite-graphrag")
        .args([
            "remember",
            "--namespace", &namespace,
            "--name", &name,
            "--type", "project",
            "--description", "swarm agent note",
            "--body", content,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "swarm remember failed for agent {agent_id}");
    Ok(())
}

fn swarm_recall_all(agent_ids: &[&str], query: &str) -> anyhow::Result<Vec<(String, String)>> {
    let mut results = Vec::new();
    for agent_id in agent_ids {
        let namespace = format!("swarm-{agent_id}");
        let output = Command::new("sqlite-graphrag")
            .args(["recall", "--namespace", &namespace, "--k", "5", "--json", query])
            .output()?;
        if output.status.success() {
            let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            if let Some(items) = parsed["results"].as_array() {
                for item in items {
                    if let Some(snippet) = item["snippet"].as_str() {
                        results.push((agent_id.to_string(), snippet.to_owned()));
                    }
                }
            }
        }
    }
    Ok(results)
}
```

### Explanation
- Per-agent namespace `swarm-{agent_id}` isolates memories with zero schema changes required
- A single SQLite file hosts all namespaces eliminating the need for multiple database files
- Iterating namespaces in the coordinator collects ranked results from every swarm member
- Structured JSON output with `serde_json` makes attribution trivial versus plain text logs
- Cuts multi-agent debugging time from hours to minutes by making authorship explicit

### Variants
- Use `tokio::task::JoinSet` to recall all agent namespaces concurrently in async swarms
- Add a `coordinator` namespace where the orchestrator writes synthesized swarm decisions

### See Also
- Recipe "How to orchestrate parallel recall across namespaces"
- Recipe "How to integrate with rig-core for agent memory"


## How To Use genai With sqlite-graphrag For Universal LLM Memory
### Problem
- Switching LLM providers via `genai` resets your agent memory because embeddings differ per vendor
- Your team wastes 40 minutes per provider migration rebuilding semantic search indexes

### Solution
```rust
use std::process::Command;

async fn store_llm_turn(
    namespace: &str,
    role: &str,
    content: &str,
) -> anyhow::Result<()> {
    let entry = format!("[{role}] {content}");
    let name = format!(
        "llm-turn-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let status = Command::new("sqlite-graphrag")
        .args([
            "remember",
            "--namespace", namespace,
            "--name", &name,
            "--type", "project",
            "--description", "LLM conversation turn",
            "--body", &entry,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "failed to persist LLM turn");
    Ok(())
}

async fn retrieve_relevant_context(
    namespace: &str,
    user_query: &str,
    k: u8,
) -> anyhow::Result<String> {
    let output = Command::new("sqlite-graphrag")
        .args([
            "hybrid-search",
            "--namespace", namespace,
            "--k", &k.to_string(),
            "--json",
            user_query,
        ])
        .output()?;
    anyhow::ensure!(output.status.success(), "hybrid-search failed");
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let context = parsed["results"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v["body"].as_str())
        .collect::<Vec<_>>()
        .join("\n---\n");
    Ok(context)
}
```

### Explanation
- sqlite-graphrag stores embeddings via the LLM subprocess (claude or codex) independently of any other LLM provider
- Switching from OpenAI to Mistral via `genai` does not invalidate existing memory entries
- `hybrid-search` combines vector similarity and FTS giving richer context than vector alone
- Formatting turns as `[role] content` preserves conversation structure in the memory body
- Eliminates 40 minutes of index rebuilding per provider migration with a provider-agnostic layer

### Variants
- Prepend retrieved context as a system message before every `genai::chat` request automatically
- Store model name and temperature alongside the turn body to audit which model produced each answer

### See Also
- Recipe "How to combine vector and FTS search with tunable weights"
- Recipe "How to cascade with llm-cascade and memory fallback"


## How To Cascade With llm-cascade And Memory Fallback
### Problem
- Your cascading LLM pipeline forgets previous attempts when a provider fails and retries
- Replaying failed calls without context causes your fallback model to repeat costly mistakes

### Solution
```rust
use std::process::Command;

fn persist_cascade_attempt(
    namespace: &str,
    provider: &str,
    prompt: &str,
    result: &str,
    success: bool,
) -> anyhow::Result<()> {
    let status_label = if success { "SUCCESS" } else { "FAILURE" };
    let entry = format!("[CASCADE:{status_label}:{provider}] prompt={prompt} result={result}");
    let name = format!(
        "cascade-attempt-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let status = Command::new("sqlite-graphrag")
        .args([
            "remember",
            "--namespace", namespace,
            "--name", &name,
            "--type", "project",
            "--description", "llm-cascade attempt log",
            "--body", &entry,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "failed to persist cascade attempt");
    Ok(())
}

fn load_cascade_history(namespace: &str, prompt: &str) -> anyhow::Result<String> {
    let output = Command::new("sqlite-graphrag")
        .args([
            "recall",
            "--namespace", namespace,
            "--k", "10",
            "--json",
            prompt,
        ])
        .output()?;
    anyhow::ensure!(output.status.success(), "recall failed for cascade history");
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let history = parsed["results"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v["snippet"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(history)
}
```

### Explanation
- Labeling entries with `CASCADE:SUCCESS:provider` lets the fallback skip already-failed providers
- Recalling history before each attempt surfaces which models already attempted the same prompt
- A single namespace per pipeline run ensures isolation without managing multiple database files
- Structured labels parse with simple `str::contains` checks avoiding JSON overhead at query time
- Saves costly repeat failures by giving fallback providers full awareness of prior cascade state

### Variants
- Write a `CascadeMemory` struct that automatically calls `persist` and `load` around each try
- Filter `FAILURE` entries in the fallback selection to skip proven-failing providers automatically

### See Also
- Recipe "How to use genai with sqlite-graphrag for universal LLM memory"
- Recipe "How to integrate with rig-core for agent memory"


## How To Run Ollama Offline With ollama-rs And Persistent Memory
### Problem
- Your offline `ollama-rs` agent loses all conversation context when the process restarts
- Air-gapped environments cannot use cloud vector stores so every session starts from scratch

### Solution
```rust
use std::process::Command;

fn offline_remember(content: &str) -> anyhow::Result<()> {
    let name = format!(
        "ollama-turn-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let status = Command::new("sqlite-graphrag")
        .args([
            "remember",
            "--namespace", "ollama-local",
            "--name", &name,
            "--type", "project",
            "--description", "offline ollama context",
            "--body", content,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "offline remember failed: exit code nonzero");
    Ok(())
}

fn offline_recall(query: &str, k: u8) -> anyhow::Result<Vec<String>> {
    let output = Command::new("sqlite-graphrag")
        .args([
            "recall",
            "--namespace", "ollama-local",
            "--k", &k.to_string(),
            "--json",
            query,
        ])
        .output()?;
    anyhow::ensure!(output.status.success(), "offline recall failed");
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let items = parsed["results"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| v["snippet"].as_str().map(str::to_owned))
        .collect();
    Ok(items)
}

fn build_context_prompt(query: &str, memories: &[String]) -> String {
    let context = memories.join("\n---\n");
    format!("Relevant context from memory:\n{context}\n\nUser query: {query}")
}
```

### Explanation
- sqlite-graphrag delegates embedding to the LLM subprocess; the CLI binary is ~6 MB with no bundled model
- The binary writes to a local SQLite file that survives across process restarts
- `--namespace ollama-local` keeps offline memories isolated from any networked agent namespaces
- `build_context_prompt` injects recalled memories into the Ollama prompt before each inference
- Delivers persistent vector memory in fully air-gapped environments with no cloud dependencies

### Variants
- Chain `offline_recall` with `sqlite-graphrag link` to build a knowledge graph from Ollama outputs
- Periodically call `sqlite-graphrag vacuum` to reclaim SQLite space as the offline database grows

### See Also
- Recipe "How to bootstrap memory database in 60 seconds"
- Recipe "How to integrate with rig-core for agent memory"


## How To Display Timestamps in a Local Timezone
### Problem
- JSON output from all subcommands includes `*_iso` fields in UTC by default
- Agents running in a specific region want localized timestamps for logging and display
- Pipelines parsing `created_at_iso` need offset-aware strings for correct sorting

### Solution
```bash
# One-off flag: display timestamps in São Paulo timezone
sqlite-graphrag read --name my-note --tz America/Sao_Paulo

# Persistent env var: all commands in this shell session use the given timezone
export SQLITE_GRAPHRAG_DISPLAY_TZ=America/Sao_Paulo
sqlite-graphrag list --json | jaq '.items[].updated_at_iso'   # or .memories[] (v1.0.66 alias)

# CI pipeline: force UTC explicitly to avoid system timezone surprises
SQLITE_GRAPHRAG_DISPLAY_TZ=UTC sqlite-graphrag recall "deploy notes" --json

# Extract only the offset portion to verify the timezone is applied
sqlite-graphrag read --name deploy-plan --tz Europe/Berlin --json \
  | jaq -r '.created_at_iso' \
  | rg '\+\d{2}:\d{2}$'
```

### Explanation
- Flag `--tz <IANA>` overrides all other settings and applies the given IANA timezone
- Env var `SQLITE_GRAPHRAG_DISPLAY_TZ` persists the setting across invocations without the flag
- Both fall back to UTC when absent, ensuring backward-compatible deterministic output
- Only string fields ending in `_iso` are affected; integer fields remain Unix epoch seconds
- Invalid IANA names cause exit 2 with a `Validation` error message printed to stderr
- Format produced: `2026-04-19T07:00:00-03:00` (offset explicit, no `Z` suffix)

### Variants
- Use `America/New_York` for Eastern Time (UTC-5/UTC-4 depending on DST)
- Use `Asia/Tokyo` for Japan Standard Time (UTC+9, no DST)
- Use `Europe/Berlin` for Central European Time (UTC+1/UTC+2 depending on DST)
- Use `UTC` to reset to the default explicitly in environments with a conflicting env var
- Use `--lang pt` to force human-readable stderr messages in Portuguese; stdout JSON remains language-independent

### See Also
- Recipe "How to bootstrap memory database in 60 seconds"
- Recipe "How to configure language output with --lang flag"


## How To Round-Trip Forget And Restore A Memory
### Problem
- You ran `forget --name important-decision` and now `recall` returns nothing
- Reading SQL from `memory_versions` to recover the row is not part of your job
- v1.0.21 left `history` rejecting forgotten memories and `restore` requiring `--version`


### Solution
```bash
sqlite-graphrag forget --name important-decision
sqlite-graphrag history --name important-decision --json | jaq '.deleted'
sqlite-graphrag restore --name important-decision
sqlite-graphrag recall "decision" --json
```


### Explanation
- `history` in v1.0.22 returns versions for soft-deleted memories with `deleted: true` flag
- `restore` without `--version` automatically picks the latest non-`restore` version
- Together they make `forget` reversible end-to-end without inspecting SQL
- `vec_memories` is re-embedded on restore so vector recall finds the memory again
- Round-trip is idempotent: forgetting an already-forgotten memory is a no-op


### Variants
- Pass `--version N` explicitly when you need to roll back to a specific edit
- Combine with `list --include-deleted --json | jaq '.items[] | select(.deleted)'` to audit all forgotten memories
- Pipe `history --json` into `recall` to detect forgotten state programmatically before restoring


### See Also
- Recipe "How to schedule purge and vacuum in cron or GitHub Actions"
- Recipe "How to export memories to NDJSON for backup"


## How To Edit A Memory With Optimistic Locking
### Problem
- Two agents editing the same memory simultaneously causes silent last-write-wins corruption
- Without conflict detection, your pipeline overwrites a colleague's changes without notice


### Solution
```bash
UPDATED=$(sqlite-graphrag read --name design-auth --json | jaq -r '.updated_at')
sqlite-graphrag edit --name design-auth \
  --body-file ./revised.md \
  --expected-updated-at "$UPDATED"
```


### Explanation
- Each `edit` creates a new immutable version preserving the full history of prior edits
- `--expected-updated-at` enables optimistic locking; exit code 3 signals a concurrent modification
- On exit code 3, re-read the memory with `read --json` to get the new `updated_at`, then retry
- `--body-file` reads the new body from a file; alternatives are `--body` (inline) and `--body-stdin` (pipe)
- Change only the description without touching the body: `edit --name <name> --description "new desc"`
- Change the memory type without recreating: `edit --name <name> --type decision` (skips re-embedding when body is unchanged)
- JSON response includes `memory_id`, `name`, `action` ("updated"), `version`, and `elapsed_ms`


### Variants
- Use `--body-stdin` to pipe the body from another command: `cat revised.md | sqlite-graphrag edit --name design-auth --body-stdin`
- Omit `--expected-updated-at` when concurrent writes are impossible (single-agent pipelines)


### See Also
- Recipe "How to round-trip forget and restore a memory"
- Recipe "How to rename a memory preserving full history"


## How To Rename A Memory Preserving Full History
### Problem
- Your team renamed the project from `auth-v1` to `authentication-flow` but all graph links still point to the old name
- Manual delete-and-recreate loses version history and breaks compliance audits


### Solution
```bash
sqlite-graphrag rename auth-v1 authentication-flow
sqlite-graphrag history --name authentication-flow --json | jaq '.versions | length'
```


### Explanation
- Positional arguments `rename <old> <new>` are supported since v1.0.44
- All versions and graph connections transfer to the new name automatically
- `--from`/`--to` and `--name`/`--new-name` are accepted as flag aliases since v1.0.35
- Exit code 4 signals that the source memory does not exist in the current namespace
- JSON response includes `memory_id`, `name` (new), `action` ("renamed"), `version`, and `elapsed_ms`


### Variants
- Apply optimistic locking: `--expected-updated-at` prevents renaming a memory that changed since your last read
- Verify history preservation: `history --name <new> --json | jaq '.versions[].created_at_iso'`


### See Also
- Recipe "How to edit a memory with optimistic locking"
- Recipe "How to round-trip forget and restore a memory"


## How To Ingest Large Corpora On Memory-Constrained Hosts
### Problem
- Your 5000-file ingestion pipeline strains a memory-constrained host because each LLM embedding worker holds ~350 MB RSS
- HISTORICAL (≤ v1.0.75): GLiNER NER ran on every body and its ONNX model (1.1 GB fp32, 349 MB int8) blew CI memory budgets; the pipeline was removed in v1.0.79


### Solution
```bash
sqlite-graphrag ingest ./big-corpus --recursive \
  --low-memory --max-files 50000 --json \
  | jaq -c 'select(.summary) | {files_total, files_succeeded, elapsed_ms}'
```


### Explanation
- Automatic extraction (`--enable-ner`) is URL-regex only since v1.0.79 and adds negligible cost; the GLiNER model download no longer exists
- Use `--llm-parallelism 1` to cap embedding workers at one subprocess (~350 MB RSS) on memory-constrained hosts
- `--low-memory` forces `--ingest-parallelism 1`, reducing RSS by approximately 40 percent for constrained hosts
- `--max-files 50000` raises the safety cap from the default 10000; the operation is rejected entirely if file count exceeds the cap
- Two parallelism axes exist: `--max-concurrency` controls CLI invocations, `--ingest-parallelism` controls extract+embed threads
- Trade-off is 3 to 4 times more wall-clock time for significantly lower memory footprint
- NDJSON summary line reports `files_total`, `files_succeeded`, `files_failed`, and `elapsed_ms` for pipeline auditing
- Use `--max-rss-mb 2048` (or similar) to abort embedding when process RSS exceeds the limit; provides fine-grained control beyond `--low-memory` in containers with hard memory caps


### Variants
- Set `SQLITE_GRAPHRAG_LOW_MEMORY=1` as a persistent env var instead of passing `--low-memory` per invocation
- Combine with separate `remember --entities-file` calls for curated graphs on critical documents


### See Also
- Recipe "How to bulk-import a knowledge base directory"
- Recipe "How to handle exit codes in automated pipelines"


## How To Bulk-Delete Relationships By Type
### Recipe — Clean up noisy NER-generated edges
- Use `prune-relations` to remove low-signal relationship types in bulk
- Preview with `--dry-run` before committing
- Clean orphaned entities afterward

#### Step 1 — Audit relationship distribution
```bash
sqlite-graphrag graph stats --json | jaq '{nodes: .node_count, edges: .edge_count}'
```

#### Step 2 — Dry-run the prune
```bash
sqlite-graphrag prune-relations --relation mentions --dry-run --json
```

#### Step 3 — Execute the prune
```bash
sqlite-graphrag prune-relations --relation mentions --yes --json
```

#### Step 4 — Clean orphaned entities
```bash
sqlite-graphrag cleanup-orphans --dry-run --json
sqlite-graphrag cleanup-orphans --yes --json
```

#### Step 5 — Verify graph health
```bash
sqlite-graphrag graph stats --json | jaq '{nodes: .node_count, edges: .edge_count}'
sqlite-graphrag health --json | jaq '.integrity_ok'
```


## How To Repair A Corrupted FTS5 Index
### Recipe: Repair Corrupted FTS5 Index
- Problem: `hybrid-search` returns exit 10 or empty results despite memories existing
- Solution: `sqlite-graphrag fts rebuild --json`
- Explanation: Rebuilds the FTS5 B-tree from the memories table without data loss
- Verify: `sqlite-graphrag fts check --json | jaq '.integrity_ok'`

#### Step 1 — Check FTS5 health
```bash
sqlite-graphrag health --json | jaq '.fts_query_ok'
```

#### Step 2 — Rebuild the FTS5 index
```bash
sqlite-graphrag fts rebuild --json
```

#### Step 3 — Confirm integrity
```bash
sqlite-graphrag fts check --json | jaq '.integrity_ok'
```

#### Step 4 — Inspect stats
```bash
sqlite-graphrag fts stats --json
```


## How To Create A Safe Database Backup
### Recipe: Safe Database Backup with WAL
- Problem: Need a consistent backup while the database is in active use
- Solution: `sqlite-graphrag backup --output /path/to/backup.sqlite --json`
- Explanation: Uses SQLite Online Backup API, safe with WAL mode and concurrent readers

#### Step 1 — Run the backup
```bash
sqlite-graphrag backup --output backup.sqlite --json
```

#### Step 2 — Verify the backup
```bash
sqlite-graphrag health --db backup.sqlite --json | jaq '.integrity_ok'
```


## How To Validate Remember Input Before Persisting
### Recipe: Validate Remember Input Before Persisting
- Problem: Want to preview what `remember` will do without writing to the database
- Solution: Pass `--dry-run` to inspect parsing and graph extraction without committing

#### Step 1 — Dry-run a remember call
```bash
sqlite-graphrag remember --name test --type note --description "desc" --body "content" --dry-run --json
```

#### Step 2 — Inspect the preview output
```bash
# Confirm entities and relationships parsed correctly before persisting
sqlite-graphrag remember --name test --type note --description "desc" --body-file ./content.md --dry-run --json | jaq '{name, type, entity_count: (.entities | length)}'
```


## How To Clean Up Noisy NER Entities
### Recipe: Clean Up Noisy NER Entities
- Problem: the GLiNER-era auto-extraction (≤ v1.0.75) created too many low-quality or spurious entities
- Solution: `sqlite-graphrag prune-ner --entity noisy-entity --json` or `--all --yes --json`
- Follow up: `sqlite-graphrag cleanup-orphans --yes --json`

#### Step 1 — Audit entities created by NER
```bash
sqlite-graphrag graph entities --json | jaq -r '.entities[] | select(.source == "ner") | .name'
```

#### Step 2 — Remove a specific noisy entity's NER bindings
```bash
sqlite-graphrag prune-ner --entity noisy-entity --json
```

#### Step 3 — Remove all NER bindings in bulk (with confirmation)
```bash
sqlite-graphrag prune-ner --all --yes --json
```

#### Step 4 — Clean orphaned entities left behind
```bash
sqlite-graphrag cleanup-orphans --dry-run --json
sqlite-graphrag cleanup-orphans --yes --json
```

#### Step 5 — Verify graph health
```bash
sqlite-graphrag health --json | jaq '.integrity_ok'
sqlite-graphrag graph stats --json | jaq '{nodes: .node_count, edges: .edge_count}'
```


## How To Rename An Entity (v1.0.58)
### Problem
- An entity has a misspelled or non-canonical name (e.g., "auth" should be "authentication")
- Manually renaming requires creating a new entity, migrating all edges, and deleting the old one
### Solution
```bash
sqlite-graphrag rename-entity --name auth --new-name authentication --json
```
### Explanation
- Renames the entity row in place preserving all relationships and memory bindings (they use integer FK)
- Re-embeds the vector with the new name for semantic search accuracy
- Returns `{action: "renamed", old_name, new_name, entity_id}` on success
- Fails with exit 4 if entity not found, exit 1 if new name already exists

## How To List Memories Bound To An Entity (v1.0.58)
### Problem
- Before renaming or deleting an entity, you need to know which memories reference it
- The existing `memory-entities` command only works memory→entities, not the reverse
### Solution
```bash
sqlite-graphrag memory-entities --entity authentication --json
```
### Explanation
- `--entity` flag performs a reverse lookup: entity→memories via the `memory_entities` junction table
- Returns `{entity_name, memories: [{name, description, memory_type}], count}` on success
- Only returns non-deleted memories (soft-deleted are excluded)

## How To Update An Entity Description (v1.0.58)
### Problem
- 88% of entities have NULL or empty descriptions (from auto-extraction)
- No command existed to update entity descriptions
### Solution
```bash
sqlite-graphrag reclassify --name rust-lang --description "The Rust programming language" --json
```
### Explanation
- `--description` flag on `reclassify` updates the entity description in single mode
- Can be combined with `--new-type` to change both type and description in one operation
- Batch mode (`--batch`) ignores `--description`

## How To Delete An Entity And Its Relationships (v1.0.56)
### Problem
- An entity was created by mistake or is obsolete
- Removing it manually requires deleting relationships, memory bindings and the entity row
### Solution
```bash
sqlite-graphrag delete-entity --name obsolete-concept --cascade --json
```
### Explanation
- `--cascade` removes the entity, all its relationships, and all memory_entities bindings in one atomic operation
- Without `--cascade` the command refuses to delete an entity that still has relationships (exit 1)
- Run `cleanup-orphans --dry-run --json` afterwards to audit any newly orphaned entities

## How To Merge Duplicate Entities (v1.0.56)
### Problem
- The same concept exists under multiple names (e.g. `jwt-auth` and `jwt-authentication`)
- Relationships are split across duplicates, weakening graph traversal
### Solution
```bash
sqlite-graphrag merge-entities --names "jwt-authentication,jwt-tokens" --into jwt-auth --json
```
### Explanation
- All relationships from source entities are redirected to the target entity
- Duplicate relationships after redirection are removed automatically (UPDATE OR IGNORE)
- Source entities are deleted after the merge
- The target entity must already exist (exit 4 if not found)
- Use `memory-entities --entity jwt-auth --json` afterwards to verify the consolidated bindings

## How To Ingest Documents With LLM-Curated Entities (v1.0.62)
### Problem
- Default `ingest` creates body-only memories with zero entities and zero relationships
- The GLiNER-era NER (≤ v1.0.75) produced noisy entities (ALL_CAPS generics, stop words) and low-quality `mentions` relationships; clean-up of that legacy is still relevant on old databases
- Manual `remember --graph-stdin` per file is time-consuming for large document sets
### Solution
```bash
sqlite-graphrag ingest ./docs --mode claude-code --recursive --json
```
### Explanation
- `--mode claude-code` spawns `claude -p` headless for each file with `--json-schema` for guaranteed structured output
- Extracts domain-specific entities (concepts, tools, decisions) and typed relationships with strength scores
- Entity types and relationship types are constrained to canonical enums — impossible to generate noise
- Requires Claude Code >= 2.1.0 installed locally with active Pro/Max subscription
- Resume interrupted ingestion with `--resume`; retry failures with `--retry-failed`
- Set `--max-cost-usd 5.00` to limit spend; track cost per file in NDJSON output
- --claude-timeout <S> sets per-file subprocess timeout (default 300s) to prevent hung processes

### Recipe: Preview Claude Code Ingest with Dry Run
### Problem
- You want to see which files will be processed and what names will be derived before spending LLM tokens
### Solution
```bash
sqlite-graphrag ingest ./docs --mode claude-code --dry-run --json
```
### Explanation
- `--dry-run` with `--mode claude-code` emits preview events without spawning any Claude processes
- Each file shows its derived kebab-case name in the NDJSON output with `status: "preview"`
- Zero tokens consumed, zero API calls made
- Verify file-to-name mapping before committing to a full extraction run

### Recipe: Resume Interrupted Claude Code Ingest
### Problem
- A large ingest was interrupted (network failure, budget exceeded, manual abort) and you want to continue from where it stopped
### Solution
```bash
sqlite-graphrag ingest ./docs --mode claude-code --resume --keep-queue --json
```
### Explanation
- `--resume` resets files stuck in `processing` status back to `pending` for re-extraction
- Files already marked `done` in the queue DB are skipped — zero duplicate work
- `--keep-queue` retains the queue DB for inspection or further retries
- Combine with `--max-cost-usd` to set incremental budget caps across multiple resume runs

### Recipe: Retry Only Failed Files from Claude Code Ingest
### Problem
- Some files failed during extraction (rate limits, malformed content, timeouts) and you want to retry only those failures
### Solution
```bash
sqlite-graphrag ingest ./docs --mode claude-code --retry-failed --claude-timeout 600 --json
```
### Explanation
- `--retry-failed` resets only `failed` files to `pending`, skipping all `done` files
- `--claude-timeout 600` increases per-file timeout to 10 minutes for large documents that timed out
- Previously successful files are untouched — no duplicate token spend
- Combine with `--keep-queue` to preserve queue for further inspection

## How To Ingest Documents With OpenAI Codex CLI (v1.0.62)

### Problem
- You want LLM-curated entity and relationship extraction during bulk ingestion
- You have an OpenAI API key and Codex CLI installed instead of Claude Code
- You want vendor choice between Anthropic and OpenAI for extraction workloads

### Solution
- Use `ingest --mode codex` to spawn `codex exec --json` per file with structured output
- Codex CLI extracts domain-specific entities and typed relationships from each document
- Full embedding pipeline ensures memories are searchable via `recall` and `hybrid-search`

### Recipe

<!-- skip-test -->
```bash
sqlite-graphrag ingest ./docs --mode codex --recursive --json
```

### Variations
- Use `--codex-model o4-mini` for lower cost extraction
- Use `--codex-binary /usr/local/bin/codex` to specify binary path
- Use `--codex-timeout 600` to increase per-file timeout from default 300s
- Use `--dry-run` to preview file-to-name mapping without spawning Codex
- Use `--resume` to continue interrupted ingest from queue DB
- Use `--max-cost-usd 5.00` to cap cumulative extraction cost (token-based estimation)
- Set `SQLITE_GRAPHRAG_CODEX_BINARY` env var to override PATH lookup permanently

### Notes
- Requires Codex CLI >= 0.120.0 installed locally with active OpenAI API key
- Codex reports token usage (input_tokens, output_tokens) instead of cost_usd
- Uses same NDJSON output format as `--mode claude-code` (PhaseEvent, FileEvent, Summary)
- Queue DB `.ingest-queue.sqlite` enables resume/retry across sessions
- Subprocess runs with `env_clear()` + selective injection for security hardening

> **Authentication:** OAuth is the ONLY accepted credential flow. API keys are PROHIBITED.
> `--mode claude-code` reads OAuth from `~/.claude/.credentials.json` (Claude Pro/Max/Team).
> `--mode codex` reads device auth from `codex login` (OpenAI ChatGPT).
> Defining `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` in the environment ABORTS the spawn with `AppError::Validation` and exit code 1. The `--bare` flag (which would also demand an API key) is REMOVED from all executable code paths.
> See `docs/decisions/adr-0011-oauth-only-enforcement.md` for the full rationale.


## How To Reclassify Relationship Types In Bulk (v1.0.65)

### Problem
- Your graph has hundreds of `mentions` or `applies_to` relationships that should be more precisely typed
- Manually unlinking and relinking each edge is impractical at scale

### Solution
```bash
sqlite-graphrag reclassify-relation --from-relation mentions --to-relation related --batch --dry-run --json
sqlite-graphrag reclassify-relation --from-relation mentions --to-relation related --batch --json
```

### Explanation
- `--dry-run` previews how many edges would change without modifying the database
- `--batch` reclassifies all edges of the specified type in the namespace
- Handles UNIQUE collisions automatically: if (source, target, new_relation) already exists, the old edge is merged and deleted
- Use `--filter-source-type incident --filter-target-type tool` to narrow scope to specific entity type pairs

### Variants
- Single edge: `sqlite-graphrag reclassify-relation --source entity-a --target entity-b --from-relation applies-to --to-relation uses --json`
- Targeted batch: `sqlite-graphrag reclassify-relation --from-relation applies-to --to-relation depends-on --filter-source-type concept --batch --json`


## How To Normalize Entity Names To Kebab-Case (v1.0.65)

### Problem
- Your graph has duplicate entities like `Claude Code` and `claude-code` or `CANONICAL_RELATIONS` and `canonical-relations`
- Graph traversal treats them as separate nodes, splitting relationships and reducing recall

### Solution
```bash
sqlite-graphrag normalize-entities --dry-run --json
sqlite-graphrag normalize-entities --yes --json
```

### Explanation
- `--dry-run` shows which entities would be renamed or merged without touching the database
- Normalization pipeline: NFKD decomposition, ASCII filtering, lowercase, spaces and underscores to hyphens, collapse consecutive hyphens
- When normalization creates a collision (both `Claude Code` and `claude-code` exist), automatically merges all relationships into the target and deletes the source entity
- After normalization, all entity names are guaranteed idempotent lowercase kebab-case

### Notes
- Entity names are also normalized on every write path since v1.0.65 (remember, ingest, link, rename-entity)
- Run `cleanup-orphans --json` after normalization to remove any newly orphaned entities


## How To Enrich Orphan Memories With LLM-Extracted Entities (v1.0.65)

### Problem
- Most memories in your database have zero entity bindings (orphan memories invisible to graph traversal)
- Manual entity extraction via `remember --graph-stdin` is tedious for hundreds of memories

### Solution
```bash
sqlite-graphrag enrich --operation memory-bindings --mode claude-code --limit 50 --dry-run --json
sqlite-graphrag enrich --operation memory-bindings --mode claude-code --limit 50 --json
```

### Explanation
- Scans for memories with zero entity bindings, sends each body to the LLM for structured extraction, and persists extracted entities and relationships via `--force-merge`
- `--dry-run` previews which memories would be enriched without spawning the LLM (zero tokens)
- `--limit 50` caps the batch size for cost control
- `--max-cost-usd 5.00` caps cumulative API spend
- Output is NDJSON: phase events, per-item events, and summary line

### Variants
- Generate entity descriptions: `sqlite-graphrag enrich --operation entity-descriptions --mode claude-code --limit 100 --json`
- Expand short memory bodies: `sqlite-graphrag enrich --operation body-enrich --mode claude-code --limit 20 --json`
- Resume after interruption: `sqlite-graphrag enrich --operation memory-bindings --mode claude-code --resume --json`
- Use Codex instead of Claude: `sqlite-graphrag enrich --operation memory-bindings --mode codex --limit 50 --json`
- Run with parallel LLM workers: `sqlite-graphrag enrich --operation entity-descriptions --mode claude-code --llm-parallelism 4 --json`


## How To Batch-Create Memories From NDJSON (v1.0.67)

### Problem
- You have a list of memories to create and calling `remember` in a loop is slow and non-atomic
- A partial write leaves the database in an inconsistent state when the loop is interrupted

### Solution
```bash
# Batch-create 3 memories from NDJSON
printf '{"name":"note-a","type":"note","description":"first","body":"content a"}\n{"name":"note-b","type":"note","description":"second","body":"content b"}\n{"name":"note-c","type":"note","description":"third","body":"content c"}' | sqlite-graphrag remember-batch --json

# Atomic batch with transaction
cat memories.ndjson | sqlite-graphrag remember-batch --transaction --force-merge --json
```

### Explanation
- `remember-batch` reads one JSON object per line from stdin and inserts each as a memory
- `--transaction` wraps all inserts in a single SQLite transaction: all succeed or all roll back
- `--force-merge` upserts existing memories instead of failing with exit 9 on duplicates
- Each input line supports the same fields as `remember`: `name`, `type`, `description`, `body`
- Output is NDJSON: one result line per input memory plus a summary line with `summary: true`
- Faster than a shell loop because all memories share one CLI invocation and DB connection

### Variants
- Pipe `jaq` output directly: `jaq -c '.[]' memories.json | sqlite-graphrag remember-batch --json`
- Omit `--transaction` when partial success is acceptable and you want per-item error details

### See Also
- Recipe "How to bulk-import a knowledge base directory"
- Recipe "How to handle exit codes in automated pipelines"


## How To Look Up A Memory By ID (v1.0.67)

### Problem
- You have a `memory_id` integer from a `list` or `remember` response and want a direct O(1) fetch
- Looking up by name requires knowing the exact kebab-case slug, which is not always available

### Solution
```bash
# Direct lookup by memory_id (from list or remember response)
sqlite-graphrag read --id 42 --json
```

### Explanation
- `--id` accepts the integer `memory_id` returned by `list`, `remember`, and `remember-batch`
- The lookup is O(1) by primary key — faster than `--name` when you already have the ID
- `--id` and `--name` are mutually exclusive; pass exactly one per invocation
- All other `read` flags (`--tz`, `--json`, `--lang`) work normally with `--id`

### Variants
- Extract the ID from a `remember` response: `sqlite-graphrag remember ... --json | jaq '.memory_id'`
- Combine with `list`: `sqlite-graphrag list --json | jaq '.items[0].memory_id'`

### See Also
- Recipe "How to chain three-layer deep retrieval"
- Recipe "How to edit a memory with optimistic locking"


## How To Install Shell Completions

### Problem
- You type `sqlite-graphrag` subcommands from memory and miss flags that could save you time
- Tab-completion is not available after installing the binary

### Solution
```bash
# Bash completions
sqlite-graphrag completions bash > ~/.bash_completion.d/sqlite-graphrag
# Zsh completions
sqlite-graphrag completions zsh > ~/.zfunc/_sqlite-graphrag
# Fish completions
sqlite-graphrag completions fish > ~/.config/fish/completions/sqlite-graphrag.fish
```

### Explanation
- The `completions` subcommand generates shell completion scripts for bash, zsh, and fish
- Bash: source the file or place it in a directory that bash sources automatically on startup
- Zsh: add `fpath=(~/.zfunc $fpath)` and `autoload -U compinit && compinit` to `~/.zshrc`
- Fish: the completions file is loaded automatically from the fish completions directory
- Completions cover all subcommands, flags, and enum values (e.g. `--type`, `--entity-type`, `--format`)

### Variants
- PowerShell: `sqlite-graphrag completions powershell > sqlite-graphrag.ps1` then dot-source it
- Elvish: `sqlite-graphrag completions elvish > ~/.config/elvish/lib/sqlite-graphrag.elv`

### See Also
- Recipe "How to bootstrap memory database in 60 seconds"
- Recipe "How to integrate sqlite-graphrag with Claude Code subprocess loop"


## How To Cap Process Proliferation On Claude Code Enrichment (G28, v1.0.68)
### Problem
- Running `sqlite-graphrag enrich --mode claude-code --llm-parallelism 4` on a workstation with 10+ MCP servers configured typically spawns 4 × 4 workers × 10 MCPs = 160+ child processes, which on a 10-CPU host can saturate load average to 27× the CPU count (real incident: 2026-06-03).
- Two parallel `enrich` invocations on the same database compound the problem and may also deadlock on the SQLite single-writer contention.

### Solution
```bash
# Step 1: create an empty config directory once (idempotent)
mkdir -p /tmp/claude-empty-config

# Step 2: point the CLI at it (env var, opt-in)
export SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR=/tmp/claude-empty-config

# Step 3: run enrichment with conservative parallelism
sqlite-graphrag enrich --operation body-enrich --mode claude-code \
  --llm-parallelism 4 --limit 50 --json

# Step 4: a second concurrent invocation against the same DB will fail fast:
sqlite-graphrag enrich --operation memory-bindings --mode claude-code --json
# → exit 75, error: "job enrich for namespace 'global' is already running"
```

### Explanation
- `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` is honored by `claude_runner::build_claude_command` in v1.0.68.  The subprocess is spawned with `CLAUDE_CONFIG_DIR=<that dir>`, which is the only mechanism upstream Claude Code actually honours (see [anthropics/claude-code#10787]).  We deliberately do NOT pass `--strict-mcp-config` or `mcp-config` because both are ignored.
- `enrich` (and `ingest --mode claude-code|codex`) acquire a per-namespace singleton in v1.0.68.  The second concurrent invocation receives `AppError::JobSingletonLocked` (exit 75) instead of stacking.
- A `tracing::warn!` is emitted when `--llm-parallelism > 4` recommending the combination with the env-var override to keep the host responsive.

### Variants
- For low-RAM containers (≤ 4 GB): add `SQLITE_GRAPHRAG_LOW_MEMORY=1` and `--llm-parallelism 1`.
- For CI runners: set the env var via the workflow YAML and pass `--max-rss-mb 2048` to `ingest --mode claude-code` to abort early on memory pressure.

### See Also
- Recipe "How to integrate sqlite-graphrag with Claude Code subprocess loop"
- docs/HOW_TO_USE.md → "Capping process proliferation on Claude Code runs (G28, v1.0.68)"
