# Migration Guide — neurographrag to sqlite-graphrag

- This guide covers the rename from legacy `neurographrag` to `sqlite-graphrag v1.0.16`
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
- Install the published release with `cargo install sqlite-graphrag --version 1.0.16 --locked`

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

## Compatibility Notes
- There is no backward-compatibility alias for the old binary name in this repository copy
- Existing JSON contracts, exit codes, and operational semantics remain aligned with the legacy `v2.3.0` behavior
- The current public release under the new name is `sqlite-graphrag v1.0.16`

## Rollback
- Reinstall or restore the legacy `neurographrag` binary if you need to revert immediately
- Restore the old `NEUROGRAPHRAG_*` environment variables if needed
- If you changed paths, point the legacy binary back to the previous database file before retrying

## See Also
- `README.md` for the current installation path and release guidance
- `CHANGELOG.md` for legacy lineage and renamed release notes
- `docs/HOW_TO_USE.md` for current command examples
