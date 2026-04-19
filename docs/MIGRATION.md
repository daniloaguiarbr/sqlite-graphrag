# Migration Guide — neurographrag


## Migrating From v1.x to v2.x


### Breaking Changes in v2.0.0
- Flag `--allow-parallel` was removed with no replacement
- Concurrency is now controlled exclusively via `--max-concurrency` (default: 4)
- Any script passing `--allow-parallel` must remove that flag before upgrading
- The `--max-concurrency` ceiling is `2×nCPUs`; values above the ceiling return exit 2

### Breaking Changes in v2.0.1
- Flag `--days` on `purge` was replaced by `--retention-days` as the canonical name
- The alias `--days` remains available in v2.0.1 and later for backwards compatibility
- Scripts using `--days` continue to work but should migrate to `--retention-days`
- Flag `--to` on `sync-safe-copy` was replaced by `--dest` as the canonical name

### New Global Flags in v2.0.1
- `--lang <en|pt>` selects the output language for human-readable stderr messages
- `--tz <IANA>` applies a timezone to all `*_iso` fields in JSON output
- Both flags are global and can be placed before any subcommand

### Schema Version Changes
- v2.0.0 introduced new columns; run `neurographrag migrate` after upgrading the binary
- `migrate` is idempotent and safe to run multiple times on the same database
- Run `neurographrag health --json` to confirm `schema_version` matches the expected value


## Step-by-Step Upgrade From v1.x

### Step 1 — Install the new binary
```bash
cargo install neurographrag --version 2.1.0
```

### Step 2 — Apply schema migrations
```bash
neurographrag migrate
```

### Step 3 — Update any scripts using removed flags
- Replace `--allow-parallel` with `--max-concurrency <N>` (e.g. `--max-concurrency 4`)
- Replace `purge --days N` with `purge --retention-days N`
- Replace `sync-safe-copy --to PATH` with `sync-safe-copy --dest PATH`

### Step 4 — Verify the database
```bash
neurographrag health --json
neurographrag stats --json
```

### Step 5 — Confirm JSON output format
- `list --json` now returns `{"items": [...]}` (not a bare array)
- Update any `jaq` pipelines from `.[]` to `.items[]` for list output
- `recall --json` and `hybrid-search --json` return `{"results": [...]}`
- Update any `jaq` pipelines from `.[]` to `.results[]` for search output


## Exit Code Changes

| Code | Meaning                                    | Since  |
|------|--------------------------------------------|--------|
| 13   | Database busy (was 15 in v1.x)             | v2.0.0 |
| 75   | Counting semaphore exhausted               | v1.2.0 |
| 77   | Low RAM guard triggered                    | v1.2.0 |


## Rollback Instructions

### Rolling back to v1.x
- The v2.x schema is forward-only; no automatic downgrade exists
- To roll back: restore from a pre-migration backup snapshot
- Use `sync-safe-copy` to create a backup BEFORE running `migrate`
```bash
neurographrag sync-safe-copy --dest ~/backup/neurographrag-pre-v2.sqlite
neurographrag migrate
```


## See Also
- `CHANGELOG.md` for the full list of changes per release
- `docs/HOW_TO_USE.md` for current flag reference
- `docs/COOKBOOK.md` for updated pipeline recipes
