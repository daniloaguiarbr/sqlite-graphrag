# Migration Guide — neurographrag to sqlite-graphrag

- This guide covers the rename from legacy `neurographrag` to `sqlite-graphrag v1.0.27`
- The renamed project keeps the same core feature set as legacy `neurographrag v2.3.0`
- The public crate and repository are live; use the local checkout only when validating unreleased changes

## What Changes
- Binary name changes from `neurographrag` to `sqlite-graphrag`
- Cargo package name changes from `neurographrag` to `sqlite-graphrag`
- Rust crate path changes from `neurographrag` to `sqlite_graphrag`
- Environment variables change from `NEUROGRAPHRAG_*` to `SQLITE_GRAPHRAG_*`
- Default local database file becomes `./graphrag.sqlite` in the invocation directory
- Default XDG application directories change from `neurographrag` to `sqlite-graphrag`
- Database schema stays compatible; the biggest risk is path drift, not schema drift

## Step-by-Step Migration
### Step 1 — Install the renamed binary
```bash
cargo install --path .
```
- Install the published release with `cargo install sqlite-graphrag --version 1.0.27 --locked`

### Step 2 — Update command invocations
```bash
sqlite-graphrag init
sqlite-graphrag health --json
sqlite-graphrag recall "postgres migration" --k 5 --json
```
- Replace every `neurographrag ...` call in scripts, CI jobs, and local aliases

### Step 3 — Update environment variables
| Old | New |
| --- | --- |
| `NEUROGRAPHRAG_DB_PATH` | `SQLITE_GRAPHRAG_DB_PATH` |
| `NEUROGRAPHRAG_CACHE_DIR` | `SQLITE_GRAPHRAG_CACHE_DIR` |
| `NEUROGRAPHRAG_NAMESPACE` | `SQLITE_GRAPHRAG_NAMESPACE` |
| `NEUROGRAPHRAG_LANG` | `SQLITE_GRAPHRAG_LANG` |
| `NEUROGRAPHRAG_LOG_LEVEL` | `SQLITE_GRAPHRAG_LOG_LEVEL` |
| `NEUROGRAPHRAG_LOG_FORMAT` | `SQLITE_GRAPHRAG_LOG_FORMAT` |
| `NEUROGRAPHRAG_DISPLAY_TZ` | `SQLITE_GRAPHRAG_DISPLAY_TZ` |

### Step 4 — Decide how to handle the database path
- To keep using the legacy database file, point `SQLITE_GRAPHRAG_DB_PATH` to the old absolute path explicitly
- To start clean under the renamed defaults, do nothing and let `sqlite-graphrag` create `./graphrag.sqlite`
- Project-local `.neurographrag/config.toml` is no longer part of the default flow

### Step 5 — Verify the migrated setup
```bash
sqlite-graphrag health --json
sqlite-graphrag stats --json
sqlite-graphrag namespace-detect
```
- Confirm `schema_version` is valid and that the resolved namespace and database path are the expected ones

## JSON Schema Changes

### v1.0.44 — `graph entities` output rename
- The JSON array field was renamed from `.items` to `.entities`
- Consumers must update their filters: `.items[]` → `.entities[]`
- Example: `sqlite-graphrag graph entities --json | jaq '.entities[].name'`

### v1.0.49 — Extensible relation vocabulary
- The `--relation` argument now accepts any kebab-case or snake_case string
- 12 canonical relations remain as well-known values
- Non-canonical relations emit a `tracing::warn!` on stderr but are accepted

### v1.0.50 — `prune-relations`, daemon auto-restart, schema v11
- New `prune-relations` subcommand for bulk-deleting relationships by type: `sqlite-graphrag prune-relations --relation mentions --yes --json`
- Daemon auto-restart on version mismatch: CLI detects stale daemon and restarts before the first embedding request (one attempt per process)
- V011 migration adds `idx_relationships_ns_relation` index for relation-type filtering
- Schema version bumped from 10 to 11
- `warn_if_non_canonical` now emits warnings in `unlink` and `related` (previously only in `link`, `remember`, `ingest`)
- `errors_msg::*` functions always return English; JSON stdout is a deterministic English-only API contract
- Graph export logs orphaned edges via `tracing::warn!` instead of silently skipping them

### v1.0.60 — ingest --mode claude-code, CI fixes, reverse schema

#### New feature: ingest --mode claude-code
- `sqlite-graphrag ingest ./docs --mode claude-code --recursive --json` uses the locally installed Claude Code CLI for LLM-curated entity/relationship extraction
- Spawns `claude -p` headless per file with `--json-schema` for guaranteed structured output
- Requires Claude Code >= 2.1.0 with active Pro/Max subscription — zero API keys needed
- Resumable via `--resume`; budget control via `--max-cost-usd <N>`; rate limit with automatic exponential backoff
- Queue DB (`.ingest-queue.sqlite`) tracks per-file progress; NDJSON events include `entities`, `rels`, `cost_usd` per file
- Existing `--mode none` (default) and `--mode gliner` continue working unchanged

#### New schema: memory-entities-reverse.schema.json
- `memory-entities --entity <name> --json` reverse lookup now has a dedicated JSON Schema
- Forward (`--name`) uses `memory-entities.schema.json`; reverse (`--entity`) uses `memory-entities-reverse.schema.json`
- Agents validating reverse responses against the forward schema should update to use the reverse schema

#### CI test fixes
- 8 test failures fixed across exit codes, i18n, ingest fail-fast, migration count, and Windows bash examples
- No runtime behavior changes — all fixes are in test code only

### v1.0.58 — FTS5 force-merge sync fix (CRITICAL), merge-entities UNIQUE fix, rename-entity, entity validation

#### CRITICAL: FTS5 index corruption via remember --force-merge fixed
- Every `remember --force-merge` operation was silently corrupting the FTS5 index since v1.0.56
- `hybrid-search` returned stale results and `fts check` reported `integrity_ok: false` after force-merge
- Fixed: `sync_fts_after_update()` is now called after the UPDATE in the force-merge path
- **Action**: Run `sqlite-graphrag fts rebuild` once after upgrading to rebuild the corrupted FTS5 index

#### merge-entities UNIQUE constraint fixed for memory_entities
- `merge-entities` failed with exit 10 when source and target entities shared bindings to the same memory
- Fixed: uses `UPDATE OR IGNORE` + cleanup for `memory_entities` (same pattern already applied to `relationships` in v1.0.57)
- No action needed: previously failing merges will now succeed

#### New command: rename-entity
- `sqlite-graphrag rename-entity --name <old> --new-name <new> --json` renames an entity preserving all relationships and memory bindings
- Re-embeds the vector with the new name for semantic search accuracy
- Agents that previously used manual unlink/re-link cascades can now use this single command

#### New feature: memory-entities --entity (reverse lookup)
- `sqlite-graphrag memory-entities --entity <name> --json` lists all memories bound to a given entity
- Complements the existing memory→entities direction
- Useful for impact assessment before renaming or deleting an entity

#### New feature: reclassify --description
- `sqlite-graphrag reclassify --name <entity> --description "text" --json` updates entity description in single mode
- Previously only `entity_type` could be changed; now description is also updatable

#### Entity name validation
- Entity names are now validated at creation time: newlines rejected, minimum 2 characters, short ALL_CAPS abbreviations rejected
- Existing entities with invalid names are not affected retroactively
- Agents providing `--graph-stdin` with curated entities should verify names comply

#### purge response includes action field
- `purge` JSON response now includes `"action": "purged"` or `"action": "dry_run"` for consistency with all other commands
- Agents parsing purge response should update to check the new `action` field

### v1.0.57 — 16 fixes: merge-entities relationships UNIQUE, memory-entities column, WAL checkpoint, atomic backup

- `merge-entities` relationships `UPDATE OR IGNORE` fix (same pattern now extended to `memory_entities` in v1.0.58)
- `memory-entities` column name fixed from `type` to `entity_type`
- `--clear-body` validation for `remember --force-merge`
- WAL checkpoint TRUNCATE added to `fts rebuild` and `fts check`
- Degree recalculation for adjacent entities in `delete-entity` and `merge-entities`
- Atomic backup via tempfile-rename pattern
- 18 new contract and schema tests
- No breaking changes; no action needed

### v1.0.56 — FTS5 sync fix, 7 new commands, JSON error envelope, graceful degradation

- FTS5 sync now works in `edit`, `rename`, `restore` — previously edited memories were invisible to full-text search
- `hybrid-search` degrades gracefully when FTS5 is corrupted: falls back to vector-only with `fts_degraded: true`
- ALL error paths emit JSON on stdout: `{"error": true, "code": N, "message": "..."}`
- `--force-merge` with empty body preserves existing body (breaking change: use `--clear-body` to explicitly clear)
- `--type` and `--description` are now optional with `--force-merge` (inherited from existing memory)
- `list --json` default limit changed from 50 to all memories (text output retains default 50)
- `unlink --relation` is now optional (removes all between pair); `--entity X --all` for bulk removal
- 7 new commands: `fts` (rebuild/check/stats), `backup`, `delete-entity`, `reclassify`, `merge-entities`, `memory-entities`, `prune-ner`
- `graph entities` adds `degree` field and `--sort-by degree|name|created_at --order asc|desc`
- `health` adds `fts_query_ok` (functional FTS5 test) and `sqlite_version`
- `optimize` now rebuilds FTS5 index (skip with `--skip-fts`)
- `ingest` auto-prefixes numeric basenames with `doc-` and adds `--max-name-length` flag

### v1.0.55 — Documentation accuracy fixes for SKILL.md, CLAUDE.md, and exit code table

#### Export summary field corrected from `total` to `exported`
- SKILL.md previously documented the export summary field as `total`; the actual JSON field is `exported`
- Agents parsing `.total` from export summary should switch to `.exported`

#### List response fields corrected
- SKILL.md previously documented `total`, `limit`, `offset` as top-level fields in the `list` response
- The actual response contains only `items[]` and `elapsed_ms` at the top level
- Agents parsing `.total`, `.limit`, or `.offset` from list should remove those references

#### Invalid timezone exit code corrected from 1 to 2
- `--tz` with an invalid timezone value returns exit 2 (Clap argument parsing), not exit 1 (application validation)
- Clap validates `chrono_tz::Tz` via `FromStr` before application code runs
- Exit code 2 now explicitly documented in SKILL.md and CLAUDE.md exit code tables

#### Stats legacy alias fields documented
- `stats` response includes undocumented legacy aliases: `db_bytes`, `edges`, `memories_total`, `entities_total`, `relationships_total`
- These are now documented; prefer the canonical field names (`db_size_bytes`, `relationships`, etc.)

### v1.0.54 — WAL checkpoint for prune-relations, empty body validation, memory_type consistency

#### WAL checkpoint TRUNCATE added to prune-relations
- `prune-relations` was the last remaining write command without `PRAGMA wal_checkpoint(TRUNCATE)` after commit
- All 12 write commands now checkpoint consistently; no action needed

#### Empty body validation with --graph-stdin
- `remember --graph-stdin` with empty body and no entities now correctly returns exit 1 (Validation) instead of silently creating an inert memory with zero chunks
- Agents that relied on empty-body `--graph-stdin` creating a memory must provide a non-empty body or at least one entity

#### memory_type field added to list and export JSON
- `list` and `export` JSON output now includes `memory_type` alongside `type`, consistent with `read`
- Agents parsing `.memory_type` from `list` or `export` no longer receive null
- No action needed: the existing `type` field remains unchanged

#### Vec::with_capacity applied in 9 cold paths
- Performance improvement only; no API or behavioral changes

### v1.0.53 — WAL checkpoint after writes, export --json

#### WAL checkpoint TRUNCATE on every write command
- All write commands (remember, edit, forget, ingest, link, unlink, rename, restore, cleanup-orphans, purge) now run `PRAGMA wal_checkpoint(TRUNCATE)` after committing
- This ensures the database file is always self-contained when external tools (Dropbox, iCloud, OneDrive, rsync) read it
- No action needed: the checkpoint is automatic and adds ~1-5ms per write
- If a checkpoint fails due to contention (SQLITE_BUSY after 5s timeout), the command fails with an error exit code
- Exception: `ingest` uses best-effort checkpoint (ignores failure) to avoid losing the NDJSON summary after a large batch

#### export accepts --json flag
- `export --json` is now accepted as a no-op hidden flag for contract uniformity
- Previously returned Clap exit 2; now returns exit 0 with the same NDJSON output
- No action needed unless you were explicitly handling exit 2 from `export --json`

### v1.0.52

#### Breaking: Duplicate exit code changed from 2 to 9
- `AppError::Duplicate` now returns exit code 9 instead of 2
- Exit code 2 is now exclusively used by Clap for argument parsing errors
- Agents routing on exit 2 for duplicate detection must update to exit 9
- Constant `DUPLICATE_EXIT_CODE` added to `src/constants.rs`

#### Breaking: forget no longer emits JSON on not-found
- `forget` with a nonexistent memory name now returns only stderr error + exit 4
- Previously it emitted JSON `{"action":"not_found",...}` to stdout AND stderr error
- This aligns with `read`, `edit`, `history`, `rename` behavior on not-found
- Agents parsing JSON stdout for forget not-found must switch to exit code routing

### v1.0.51

- `SQLITE_GRAPHRAG_NAMESPACE` is now honored by all commands. If you relied on the previous behavior where `list`, `read`, `edit`, `forget`, `history`, `rename`, `restore`, and `remember` always used 'global' regardless of the environment variable, explicitly pass `--namespace global` to preserve the old behavior.
- New `--max-rss-mb` flag for `remember` and `ingest` (default: 8192 MiB). No action needed unless you want to lower the threshold.

## Compatibility Notes
- There is no backward-compatibility alias for the old binary name in this repository copy
- Existing JSON contracts, exit codes, and operational semantics remain aligned with the legacy `v2.3.0` behavior
- The current public release under the new name is `sqlite-graphrag v1.0.27`

## Rollback
- Reinstall or restore the legacy `neurographrag` binary if you need to revert immediately
- Restore the old `NEUROGRAPHRAG_*` environment variables if needed
- If you changed paths, point the legacy binary back to the previous database file before retrying

## See Also
- `README.md` for the current installation path and release guidance
- `CHANGELOG.md` for legacy lineage and renamed release notes
- `docs/HOW_TO_USE.md` for current command examples
