# ADR-0027: G40 Fix — `applied_on = NULL` Blocks Migrations


## Status
- Accepted (2026-06-09)
- Deciders: Danilo Aguiar
- Scope: `src/commands/migrate.rs`, `src/commands/debug_schema.rs`
- Covers the migration flow from v1.0.74 to v1.0.77


## Context
### Root Cause
- `run_rehash` in `migrate.rs:263` inserted rows without `applied_on`
- The field was left NULL in SQLite after the operation
- `refinery-core` 0.9.1 reads `applied_on` as `String` (NOT NULL)
- `row.get::<_, String>(2)` fails with `InvalidColumnType`
- All subsequent migrations are blocked (exit 20)
### Incident Impact
- Real incident: approximately 28 hours of blockage on 2026-06-09
- No migration could execute after rehash with NULL field
### Aggravating Factors
- V013 DROP requires `vec0` module absent in the LLM-only build
- `debug_schema.rs` also crashes when reading `applied_on` NULL
- The operator had no functional diagnostic tool available


## Decision
### Fix 1 — `sanitize_null_applied_on` Helper
- UPDATE on rows with `applied_on` NULL before the runner
- Fills with current RFC3339 timestamp via `chrono`
- Executes before any refinery operation
### Fix 2 — INSERT with Explicit `applied_on`
- Every INSERT now includes `applied_on` with an RFC3339 timestamp
- Uses `chrono::Utc::now().to_rfc3339()` as the default value
- Prevents creation of new rows with NULL field
### Fix 3 — `remove_vec_virtual_tables_without_module`
- Cleans up orphan virtual tables via `writable_schema`
- Removes references to the `vec0` module absent in the LLM-only build
- Unblocks migration V013 without depending on sqlite-vec
### Fix 4 — `debug_schema.rs` NULL-Tolerant
- `applied_on` field changed from `String` to `Option<String>`
- Diagnostics accessible even on databases with NULL field
- No crash when inspecting databases affected by the bug


## Consequences
### Positive
- 4 backward-compatible scenarios covered by the fix
- Fresh install scenario works without intervention
- v1.0.74 scenario migrates correctly to v1.0.77
- v1.0.76 scenario with bug fixed automatically
- v1.0.74 scenario with accumulated bug also resolved
- No manual operator intervention required in any case
- `debug-schema` accessible on databases affected by NULL
- Idempotency guaranteed in all sanitization operations
### Negative
- VACUUM after `writable_schema` can be slow on large databases
- `chrono` RFC3339 emits `+00:00` instead of `Z` as suffix
- `+00:00` format is compatible but differs from the `time` crate
- Sanitization overhead on migrate startup (negligible)


## Verification
- `cargo build`: ZERO compilation errors
- `cargo test --lib`: 723 tests executed (4 new)
- ZERO failures in the complete test suite
- `cargo clippy`: ZERO warnings reported
- 4 new tests cover the backward-compatibility scenarios


## References
- File: `src/commands/migrate.rs` (sanitization helper)
- File: `src/commands/debug_schema.rs` (NULL tolerance)
- Version: v1.0.77
- Incident date: 2026-06-09
