---
name: sqlite-graphrag
description: Use this skill WHENEVER the user asks about adding persistent memory or GraphRAG or long-term context to Claude Code Codex Cursor Windsurf or any AI coding agent. MUST trigger for queries mentioning remember this, save conversation, retrieve previous context, hybrid search, entity graph, SQLite memory, local RAG, offline embeddings, fastembed, sqlite-vec, multilingual-e5, KNN search, memory-safe copy, FTS5 and vec fusion. Auto-invokes even without explicit mention when user describes agent losing context between sessions or wants an offline vector database in Rust. Keywords memory RAG GraphRAG SQLite vector embeddings Claude Codex Cursor Windsurf offline local persistent graph entity.
---


## Fundamental Principles
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
- WAIT for offline download of the `multilingual-e5-small` model
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
- DETECT schema drift via `__debug_schema` for troubleshooting
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
### REQUIRED — ONNX Runtime on ARM64 GNU
- DISTRIBUTE `libonnxruntime.so` alongside the binary
- SET `ORT_DYLIB_PATH` explicitly in CI and systemd
- AFFECTS heavy embedding commands on `aarch64-unknown-linux-gnu`
- FAILS on the first embedding operation without the runtime accessible


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
- USE `--dry-run` to preview file-to-name mapping without loading ONNX model or persisting
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


## CRUD — Read with read and list
### REQUIRED — Direct Read by Name (read)
- USE `read --name <kebab-case>` for O(1) fetch by name
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
- EACH edit creates a new immutable version preserving history
- VALIDATE exit code 3 as an optimistic locking conflict
- JSON response: `memory_id`, `name`, `action` ("updated"), `version`, `elapsed_ms`
- v1.0.56: FTS5 desync bug fixed — edited memories are immediately findable via full-text search
### REQUIRED — History-Preserving Rename (rename)
- USE `rename --name <old> --new-name <new>`
- ACCEPT `--old`/`--new` and `--from`/`--to` as aliases since v1.0.35
- PRESERVE all versions and graph connections
- TREAT exit code 4 as missing source memory
- JSON response: `memory_id`, `name` (new), `action` ("renamed"), `version`, `elapsed_ms`
- v1.0.56: FTS5 desync bug fixed — renamed memories are immediately findable via full-text search
### REQUIRED — Old Version Restore (restore)
- INSPECT versions via `history --name <name>` first
- USE `restore --name <name> --version <N>` for a specific version
- OMIT `--version` to select the last non-restore version automatically
- RESTORE creates a new version without overwriting prior history
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
### REQUIRED — Entity Name Validation (v1.0.58)
- ALL entity creation paths (`link --create-missing`, `remember --graph-stdin`, `ingest --enable-ner`, `rename-entity --new-name`) validate names via `validate_entity_name()`
- REJECTS names shorter than 2 characters (exit 1)
- REJECTS names containing newline characters (exit 1)
- REJECTS ALL_CAPS abbreviations of 4 characters or fewer as NER noise (exit 1)
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
- JSON response: `memory_name`, `entities: [{entity_id, name, entity_type}]`, `count`, `elapsed_ms`
- TREAT exit code 4 as memory not found
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
### REQUIRED — Four Search Commands
- USE `recall` for KNN vector search with automatic graph expansion
- USE `hybrid-search` for FTS5 and vector fusion via RRF
- USE `related` for multi-hop traversal from a known memory
- USE `graph traverse` for traversal from a typed entity
- COMBINE all four in the canonical three-layer pattern
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
- APPROXIMATE latency of 1 second on modern hardware with daemon


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


## Daemon and Reduced Latency
### REQUIRED — Embedding Model Reuse
- START `sqlite-graphrag daemon` in long agent sessions
- CHECK health via `daemon --ping --json`
- STOP via `daemon --stop` at session end
- LET `init`, `remember`, `ingest`, `recall`, `hybrid-search` reuse automatically
- TREAT daemon as optional for single-shot invocations
- INSPECT the embedding request counter in `--ping`
- `daemon --ping` warns when daemon version differs from CLI version; restart with `daemon --stop` followed by `daemon` after upgrades
- Since v1.0.50, the CLI auto-restarts the daemon on version mismatch before the first embedding request; manual `daemon --stop` after upgrades is no longer required
- `daemon --ping` response now includes `model_name` and `model_variant` fields showing the currently loaded embedding model


## Cache — Model Management
### REQUIRED — Cache Maintenance
- LIST cached models via `cache list --json`
- REMOVE model cache via `cache clear-models --json`
- `clear-models` forces re-download on the next embedding operation
- USE `cache list` to diagnose disk usage by ONNX models


## JSON Contract and Pipelines
### REQUIRED — Deterministic Output
- USE `--json` in all subcommands before piping
- PREFER `--json` over `--format json` in one-liners
- FILTER fields via `jaq` instead of regex on stdout
- READ only fields actually returned by the subcommand
- TREAT JSON as a SemVer-versioned API
### REQUIRED — Error JSON Contract (v1.0.56)
- ALL error paths now emit a JSON object on stdout: `{"error": true, "code": N, "message": "..."}`
- stderr still receives the human-readable error with a descriptive prefix
- CONSUMERS must check `stdout` JSON first (look for `"error": true`), then fall back to the exit code
- This applies to ALL commands when `--json` is passed; without `--json` errors go only to stderr
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
- `cache list` returns models with size in bytes and total disk usage
- `prune-relations` returns `action` (`"pruned"`/`"dry_run"`), `relation`, `count`, `entities_affected`, `affected_entity_names?`, `namespace`, `elapsed_ms`
- `fts rebuild` returns `action` ("rebuilt"), `rows_indexed`, `elapsed_ms`
- `fts check` returns `action` ("checked"), `integrity_ok`, `detail?`, `elapsed_ms`
- `fts stats` returns `total_rows`, `shadow_pages?`, `fts_functional`, `elapsed_ms`
- `backup` returns `action` ("backed_up"), `source`, `destination`, `size_bytes`, `elapsed_ms`
- `delete-entity` returns `action` ("deleted"), `entity_name`, `namespace`, `relationships_removed`, `bindings_removed`, `elapsed_ms`
- `reclassify` returns `action` ("reclassified"), `count`, `description_updated?` (bool, present when `--description` applied), `namespace`, `elapsed_ms`
- `merge-entities` returns `action` ("merged"), `sources[]`, `target`, `namespace`, `relationships_moved`, `entities_removed`, `elapsed_ms`
- `memory-entities` returns `memory_name`, `entities[].{entity_id, name, entity_type}`, `count`, `elapsed_ms`
- `prune-ner` returns `action` (`"pruned"`/`"dry_run"`/`"aborted"`), `bindings_removed`, `namespace`, `entity?`, `elapsed_ms`
- `link` returns `action` ("linked"), `from`, `to`, `relation`, `weight`, `namespace`, `elapsed_ms`, `created_entities?` (array, when `--create-missing`), `warnings?` (array, when non-canonical relation)
- `unlink` returns `action` ("deleted"), `from_name`, `to_name`, `relation`, `relationships_removed`, `namespace`, `elapsed_ms`
- `rename-entity` returns `action` ("renamed"), `old_name`, `new_name`, `entity_id`, `namespace`, `elapsed_ms`


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
- `11` equals embedding failure (corrupted model or missing ORT)
- `12` equals failure loading `sqlite-vec`; check SQLite ≥ 3.40
- `13` equals partial batch failure; reprocess only failed
- `14` equals I/O error (inaccessible file, permission, disk full)
- `15` equals database busy; widen `--wait-lock`
- `20` equals internal error or JSON serialization failure
- `75` equals exhausted slots in ingest or other heavy command
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
- LIMIT parallel ingestion in CI without an active daemon
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
- BACKUP is consistent even while writes are in progress — no need to stop the daemon
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
- USE `__debug_schema --json` for troubleshooting
- INSPECT `schema_version`, `objects`, `migrations`
- CURRENT schema version is 11 (V011 adds `idx_relationships_ns_relation` index)
- COMMAND is hidden from `--help`; invoke by exact name
### Correct Pattern — Weekly Cron
- `sqlite-graphrag purge --retention-days 30 --yes`
- `sqlite-graphrag cleanup-orphans --yes`
- `sqlite-graphrag prune-relations --relation mentions --yes` (when NER-generated edges need cleanup)
- `sqlite-graphrag vacuum --json`
- `sqlite-graphrag optimize --json`
- `sqlite-graphrag sync-safe-copy --dest ~/Dropbox/graphrag.sqlite`
