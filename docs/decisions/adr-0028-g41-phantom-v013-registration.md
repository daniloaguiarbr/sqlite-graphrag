# ADR-0028: G41 Phantom V013 Registration Fix


## Status
- Accepted (2026-06-09)
- Deciders: Danilo Aguiar
- Scope: `src/commands/migrate.rs`, `src/storage/connection.rs`
- Covers the phantom V013 registration bug in v1.0.76/v1.0.77


## Context
### Root Cause
- `run_rehash` in `migrate.rs` iterated ALL 13 embedded migrations
- For any migration NOT present in `refinery_schema_history`, inserted a row with `INSERT OR IGNORE`
- This registered V013 as "already applied" without executing its SQL
- `runner().run()` read the history, saw V013 present, and skipped it entirely
- The BLOB-backed embedding tables were never created
### Incident Impact
- Every embedding operation failed with exit 10: `no such table: memory_embeddings`
- Affected commands: `recall`, `hybrid-search`, `remember`, `edit`, `ingest`
- The database entered a dead-end cycle with no command able to execute V013 SQL
### Aggravating Factors
- `ensure_db_ready` in `connection.rs` only runs migrations when `user_version < SCHEMA_USER_VERSION`
- Databases corrupted by G41 already had `user_version=50`
- The migration block was skipped entirely in CRUD commands
- No existing command could trigger repair


## Decision
### Fix 1 — Remove Phantom Registration
- Remove the `else` branch in `run_rehash` (lines 272-281) that inserted missing migrations
- `run_rehash` now ONLY rewrites checksums of migrations already in the history
- Missing migrations are left for `runner().run()` to apply with their SQL
### Fix 2 — `ensure_v013_tables_exist` Helper
- Detects the phantom registration state
- V013 in history but `memory_embeddings` absent
- Executes the V013 SQL directly when phantom state detected
- V013 uses `CREATE TABLE IF NOT EXISTS` and `INSERT OR REPLACE`
- Operation is idempotent by design
### Fix 3 — Helper Called in 4 Entry Points
- `run()` in migrate.rs
- `run_rehash` in migrate.rs
- `run_to_llm_only` in migrate.rs
- `ensure_db_ready` in connection.rs (unconditionally, outside the version check)


## Consequences
### Positive
- Databases corrupted by G41 in v1.0.76/v1.0.77 are auto-repaired by any command
- `run_rehash` is now safe and never registers unapplied migrations
- The dead-end cycle is broken
- Compatible with 5 database scenarios (fresh, v1.0.74, corrupted, correct, CRUD-only)
### Negative
- None
- V013 SQL is idempotent
- The repair check is two cheap SELECTs on `sqlite_master` and `refinery_schema_history`


## References
- File: `src/commands/migrate.rs` (phantom registration fix)
- File: `src/storage/connection.rs` (ensure_v013_tables_exist call)
- Version: v1.0.78
- Related: ADR-0027 (G40 applied_on NULL fix)
