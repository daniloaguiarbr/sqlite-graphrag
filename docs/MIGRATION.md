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
